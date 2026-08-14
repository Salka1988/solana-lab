#![forbid(unsafe_code)]

use core::fmt;
use std::{
    env,
    sync::atomic::{AtomicU64, Ordering},
    sync::Arc,
};

use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{Extension, Path, State},
    http::{HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use domain::{
    AssetId, BalanceAmount, LotSize, MarketId, MarketSpec, Order, OrderId, OrderSequence, Price,
    Quantity, Side, TickSize, TraderId,
};
use persistence::{PersistenceError, PostgresEventJournal};
use runtime::{MarketActorHandle, MarketReply, MarketSnapshot};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

const MARKET_SOL_USDC: &str = "SOL-USDC";
pub const EXCHANGE_BOOT_MODE_ENV: &str = "EXCHANGE_BOOT_MODE";
pub const DATABASE_URL_ENV: &str = "DATABASE_URL";
pub const X_REQUEST_ID_HEADER: &str = "x-request-id";

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(String);

impl RequestId {
    fn generated() -> Self {
        let counter = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("req-{}-{counter}", std::process::id()))
    }

    fn from_header(value: &HeaderValue) -> Option<Self> {
        value
            .to_str()
            .ok()
            .filter(|value| !value.is_empty())
            .map(|value| Self(value.to_owned()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone)]
pub struct ServiceState {
    market_id: String,
    boot_mode: ReadyBootMode,
    journal_mode: JournalMode,
    market: MarketSpec,
    actor: MarketActorHandle,
    metrics: ServiceMetrics,
}

#[derive(Debug, Clone, Default)]
pub struct ServiceMetrics {
    inner: Arc<ServiceMetricsInner>,
}

#[derive(Debug, Default)]
struct ServiceMetricsInner {
    http_requests_total: AtomicU64,
    ready_checks_total: AtomicU64,
    snapshot_requests_total: AtomicU64,
    deposits_accepted_total: AtomicU64,
    orders_accepted_total: AtomicU64,
    api_errors_total: AtomicU64,
}

impl ServiceMetrics {
    fn snapshot(&self, runtime: runtime::RuntimeMetricsSnapshot) -> MetricsResponse {
        MetricsResponse {
            http_requests_total: self.inner.http_requests_total.load(Ordering::Relaxed),
            ready_checks_total: self.inner.ready_checks_total.load(Ordering::Relaxed),
            snapshot_requests_total: self.inner.snapshot_requests_total.load(Ordering::Relaxed),
            deposits_accepted_total: self.inner.deposits_accepted_total.load(Ordering::Relaxed),
            orders_accepted_total: self.inner.orders_accepted_total.load(Ordering::Relaxed),
            api_errors_total: self.inner.api_errors_total.load(Ordering::Relaxed),
            actor_commands_received_total: runtime.actor_commands_received_total,
            actor_commands_accepted_total: runtime.actor_commands_accepted_total,
            actor_commands_rejected_total: runtime.actor_commands_rejected_total,
            actor_journal_append_failures_total: runtime.actor_journal_append_failures_total,
            actor_apply_after_append_failures_total: runtime
                .actor_apply_after_append_failures_total,
        }
    }

    fn record_http_request(&self) {
        self.inner
            .http_requests_total
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_ready_check(&self) {
        self.inner
            .ready_checks_total
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_snapshot_request(&self) {
        self.inner
            .snapshot_requests_total
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_deposit_accepted(&self) {
        self.inner
            .deposits_accepted_total
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_order_accepted(&self) {
        self.inner
            .orders_accepted_total
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_api_error(&self) {
        self.inner.api_errors_total.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
pub struct PostgresRuntimeJournal {
    journal: PostgresEventJournal,
}

impl PostgresRuntimeJournal {
    pub const fn new(journal: PostgresEventJournal) -> Self {
        Self { journal }
    }
}

#[async_trait]
impl runtime::EventJournal for PostgresRuntimeJournal {
    async fn append(&mut self, event: &application::Event) -> application::Result<()> {
        self.journal
            .append(event)
            .await
            .map_err(|_| application::Error::JournalAppendFailed)
    }
}

pub fn app() -> Router {
    let market = default_market();
    let actor = MarketActorHandle::spawn(market, 1024);
    app_with_actor(
        MARKET_SOL_USDC,
        ReadyBootMode::Local,
        JournalMode::Noop,
        market,
        actor,
    )
}

pub async fn app_from_env() -> Result<Router, StartupError> {
    app_from_config(BootConfig::from_env()?).await
}

pub async fn app_from_config(config: BootConfig) -> Result<Router, StartupError> {
    info!(boot_mode = ?config.mode, "building exchange service");
    match config.mode {
        BootMode::Local => Ok(app()),
        BootMode::Postgres => {
            app_with_postgres(
                config.database_url.as_deref().ok_or_else(|| {
                    StartupError::Config(format!("{DATABASE_URL_ENV} is required"))
                })?,
            )
            .await
        }
    }
}

pub async fn app_with_postgres(database_url: &str) -> Result<Router, StartupError> {
    let market = default_market();
    info!("connecting postgres event journal");
    let journal = PostgresEventJournal::connect(database_url).await?;
    info!("running postgres migrations");
    journal.migrate().await?;

    let events = journal.read_all().await?;
    info!(event_count = events.len(), "replaying exchange events");
    let exchange = application::ExchangeApplication::replay(market, events)?;
    let actor =
        MarketActorHandle::spawn_from_app(exchange, 1024, PostgresRuntimeJournal::new(journal));

    Ok(app_with_actor(
        MARKET_SOL_USDC,
        ReadyBootMode::Postgres,
        JournalMode::Postgres,
        market,
        actor,
    ))
}

pub fn app_with_actor(
    market_id: impl Into<String>,
    boot_mode: ReadyBootMode,
    journal_mode: JournalMode,
    market: MarketSpec,
    actor: MarketActorHandle,
) -> Router {
    let metrics = ServiceMetrics::default();
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics_endpoint))
        .route("/deposits", post(credit_deposit))
        .route("/orders", post(place_order))
        .route("/markets/{market_id}/snapshot", get(snapshot))
        .layer(middleware::from_fn_with_state(
            metrics.clone(),
            metrics_middleware,
        ))
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(ServiceState {
            market_id: market_id.into(),
            boot_mode,
            journal_mode,
            market,
            actor,
            metrics,
        })
}

pub fn default_market() -> MarketSpec {
    MarketSpec::new(
        AssetId::new(1).unwrap(),
        AssetId::new(2).unwrap(),
        TickSize::new(1).unwrap(),
        LotSize::new(1).unwrap(),
    )
    .unwrap()
}

pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_target(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootConfig {
    pub mode: BootMode,
    pub database_url: Option<String>,
}

impl BootConfig {
    pub fn from_env() -> Result<Self, StartupError> {
        Self::from_values(
            env::var(EXCHANGE_BOOT_MODE_ENV).ok().as_deref(),
            env::var(DATABASE_URL_ENV).ok().as_deref(),
        )
    }

    pub fn from_values(
        boot_mode: Option<&str>,
        database_url: Option<&str>,
    ) -> Result<Self, StartupError> {
        let mode = boot_mode.map_or(Ok(BootMode::Local), BootMode::parse)?;
        let database_url = database_url.map(str::to_owned);

        if mode == BootMode::Postgres && database_url.is_none() {
            return Err(StartupError::Config(format!(
                "{DATABASE_URL_ENV} is required"
            )));
        }

        Ok(Self { mode, database_url })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    Local,
    Postgres,
}

impl BootMode {
    fn parse(value: &str) -> Result<Self, StartupError> {
        match value {
            "" | "local" => Ok(Self::Local),
            "postgres" => Ok(Self::Postgres),
            _ => Err(StartupError::Config(format!(
                "unsupported {EXCHANGE_BOOT_MODE_ENV}: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadyBootMode {
    Local,
    Postgres,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalMode {
    Noop,
    Postgres,
}

#[derive(Debug)]
pub enum StartupError {
    Config(String),
    Persistence(PersistenceError),
    Application(application::Error),
}

impl From<PersistenceError> for StartupError {
    fn from(value: PersistenceError) -> Self {
        Self::Persistence(value)
    }
}

impl From<application::Error> for StartupError {
    fn from(value: application::Error) -> Self {
        Self::Application(value)
    }
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(f, "configuration startup failed: {error}"),
            Self::Persistence(error) => write!(f, "persistence startup failed: {error}"),
            Self::Application(error) => write!(f, "application replay failed: {error}"),
        }
    }
}

impl std::error::Error for StartupError {}

async fn request_id_middleware(mut request: Request<Body>, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(X_REQUEST_ID_HEADER)
        .and_then(RequestId::from_header)
        .unwrap_or_else(RequestId::generated);
    request.extensions_mut().insert(request_id.clone());

    let mut response = next.run(request).await;
    if let Ok(header_value) = HeaderValue::from_str(request_id.as_str()) {
        response
            .headers_mut()
            .insert(X_REQUEST_ID_HEADER, header_value);
    }
    response
}

async fn metrics_middleware(
    State(metrics): State<ServiceMetrics>,
    request: Request<Body>,
    next: Next,
) -> Response {
    metrics.record_http_request();
    next.run(request).await
}

async fn health(Extension(request_id): Extension<RequestId>) -> Json<HealthResponse> {
    info!(request_id = %request_id.as_str(), "health checked");
    Json(HealthResponse { status: "ok" })
}

async fn ready(
    State(state): State<ServiceState>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Json<ReadyResponse>, ApiError> {
    state.metrics.record_ready_check();
    let snapshot = actor_snapshot(&state.actor, &request_id)
        .await
        .map_err(|error| record_api_error(error, &state.metrics))?;
    info!(
        request_id = %request_id.as_str(),
        market_id = %state.market_id,
        event_count = snapshot.event_count,
        "readiness checked"
    );
    Ok(Json(ReadyResponse {
        status: "ready",
        market_id: state.market_id,
        boot_mode: state.boot_mode,
        journal_mode: state.journal_mode,
        event_count: snapshot.event_count,
    }))
}

async fn metrics_endpoint(State(state): State<ServiceState>) -> Json<MetricsResponse> {
    Json(state.metrics.snapshot(state.actor.metrics_snapshot()))
}

async fn credit_deposit(
    State(state): State<ServiceState>,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<DepositRequest>,
) -> Result<Json<DepositResponse>, ApiError> {
    info!(
        request_id = %request_id.as_str(),
        command_id = request.command_id,
        trader_id = request.trader_id,
        asset_id = request.asset_id,
        amount = request.amount,
        "credit deposit requested"
    );
    let command_id =
        command_id(request.command_id).map_err(|error| record_api_error(error, &state.metrics))?;
    let trader_id =
        trader_id(request.trader_id).map_err(|error| record_api_error(error, &state.metrics))?;
    let asset_id =
        asset_id(request.asset_id).map_err(|error| record_api_error(error, &state.metrics))?;
    let reply = state
        .actor
        .credit_deposit_with_request_id(
            Some(request_id.as_str().to_owned()),
            command_id,
            trader_id,
            asset_id,
            BalanceAmount::new(request.amount),
        )
        .await
        .map_err(|error| record_api_error(ApiError::from(error), &state.metrics))?;

    match reply {
        MarketReply::DepositCredited => {
            state.metrics.record_deposit_accepted();
            info!(
                request_id = %request_id.as_str(),
                command_id = request.command_id,
                "credit deposit accepted"
            );
            Ok(Json(DepositResponse { accepted: true }))
        }
        _ => Err(ApiError::Internal("unexpected deposit reply")),
    }
}

async fn place_order(
    State(state): State<ServiceState>,
    Extension(request_id): Extension<RequestId>,
    Json(request): Json<OrderRequest>,
) -> Result<Json<OrderResponse>, ApiError> {
    info!(
        request_id = %request_id.as_str(),
        command_id = request.command_id,
        order_id = request.order_id,
        trader_id = request.trader_id,
        market_id = request.market_id,
        side = ?request.side,
        price = request.price,
        quantity = request.quantity,
        sequence = request.sequence,
        "order requested"
    );
    let order_id =
        order_id(request.order_id).map_err(|error| record_api_error(error, &state.metrics))?;
    let trader_id =
        trader_id(request.trader_id).map_err(|error| record_api_error(error, &state.metrics))?;
    let market_id = MarketId::new(request.market_id)
        .map_err(|error| record_api_error(ApiError::from(error), &state.metrics))?;
    let side = side(request.side).map_err(|error| record_api_error(error, &state.metrics))?;
    let price = Price::new(request.price)
        .map_err(|error| record_api_error(ApiError::from(error), &state.metrics))?;
    let quantity = Quantity::new(request.quantity)
        .map_err(|error| record_api_error(ApiError::from(error), &state.metrics))?;
    let sequence = OrderSequence::new(request.sequence)
        .map_err(|error| record_api_error(ApiError::from(error), &state.metrics))?;
    let order = Order::new(
        order_id, trader_id, market_id, side, price, quantity, sequence,
    );

    state
        .market
        .validate_price(order.price())
        .map_err(|error| record_api_error(ApiError::from(error), &state.metrics))?;
    state
        .market
        .validate_quantity(order.original_quantity())
        .map_err(|error| record_api_error(ApiError::from(error), &state.metrics))?;

    let reply = state
        .actor
        .place_order_with_request_id(
            Some(request_id.as_str().to_owned()),
            command_id(request.command_id)
                .map_err(|error| record_api_error(error, &state.metrics))?,
            order,
        )
        .await
        .map_err(|error| record_api_error(ApiError::from(error), &state.metrics))?;

    match reply {
        MarketReply::OrderPlaced { fills } => {
            state.metrics.record_order_accepted();
            info!(
                request_id = %request_id.as_str(),
                command_id = request.command_id,
                order_id = request.order_id,
                fill_count = fills.len(),
                "order accepted"
            );
            Ok(Json(OrderResponse {
                accepted: true,
                fills: fills
                    .into_iter()
                    .map(|fill| FillResponse {
                        maker_order_id: fill.maker_order_id().get(),
                        taker_order_id: fill.taker_order_id().get(),
                        price: fill.price().get(),
                        quantity: fill.quantity().get(),
                    })
                    .collect(),
            }))
        }
        _ => Err(ApiError::Internal("unexpected order reply")),
    }
}

async fn snapshot(
    State(state): State<ServiceState>,
    Extension(request_id): Extension<RequestId>,
    Path(market_id): Path<String>,
) -> Result<Json<SnapshotResponse>, ApiError> {
    state.metrics.record_snapshot_request();
    if market_id != state.market_id {
        warn!(
            request_id = %request_id.as_str(),
            requested_market_id = %market_id,
            configured_market_id = %state.market_id,
            "unknown market snapshot requested"
        );
        state.metrics.record_api_error();
        return Err(ApiError::NotFound);
    }

    let snapshot = actor_snapshot(&state.actor, &request_id)
        .await
        .map_err(|error| record_api_error(error, &state.metrics))?;
    info!(
        request_id = %request_id.as_str(),
        market_id = %market_id,
        event_count = snapshot.event_count,
        "snapshot returned"
    );
    Ok(Json(SnapshotResponse {
        market_id,
        event_count: snapshot.event_count,
    }))
}

fn record_api_error(error: ApiError, metrics: &ServiceMetrics) -> ApiError {
    metrics.record_api_error();
    error
}

async fn actor_snapshot(
    actor: &MarketActorHandle,
    request_id: &RequestId,
) -> Result<MarketSnapshot, ApiError> {
    match actor
        .snapshot_with_request_id(Some(request_id.as_str().to_owned()))
        .await?
    {
        MarketReply::Snapshot(snapshot) => Ok(snapshot),
        _ => Err(ApiError::Internal("unexpected snapshot reply")),
    }
}

fn command_id(value: u128) -> Result<application::CommandId, ApiError> {
    application::CommandId::new(value).map_err(ApiError::from)
}

fn trader_id(value: u64) -> Result<TraderId, ApiError> {
    TraderId::new(value).map_err(ApiError::from)
}

fn asset_id(value: u32) -> Result<AssetId, ApiError> {
    AssetId::new(value).map_err(ApiError::from)
}

fn order_id(value: u128) -> Result<OrderId, ApiError> {
    OrderId::new(value).map_err(ApiError::from)
}

fn side(value: OrderSideDto) -> Result<Side, ApiError> {
    match value {
        OrderSideDto::Bid => Ok(Side::Bid),
        OrderSideDto::Ask => Ok(Side::Ask),
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DepositRequest {
    pub command_id: u128,
    pub trader_id: u64,
    pub asset_id: u32,
    pub amount: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct DepositResponse {
    pub accepted: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct OrderRequest {
    pub command_id: u128,
    pub order_id: u128,
    pub trader_id: u64,
    pub market_id: u32,
    pub side: OrderSideDto,
    pub price: u64,
    pub quantity: u64,
    pub sequence: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderSideDto {
    Bid,
    Ask,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OrderResponse {
    pub accepted: bool,
    pub fills: Vec<FillResponse>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct FillResponse {
    pub maker_order_id: u128,
    pub taker_order_id: u128,
    pub price: u64,
    pub quantity: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReadyResponse {
    pub status: &'static str,
    pub market_id: String,
    pub boot_mode: ReadyBootMode,
    pub journal_mode: JournalMode,
    pub event_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SnapshotResponse {
    pub market_id: String,
    pub event_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct MetricsResponse {
    pub http_requests_total: u64,
    pub ready_checks_total: u64,
    pub snapshot_requests_total: u64,
    pub deposits_accepted_total: u64,
    pub orders_accepted_total: u64,
    pub api_errors_total: u64,
    pub actor_commands_received_total: u64,
    pub actor_commands_accepted_total: u64,
    pub actor_commands_rejected_total: u64,
    pub actor_journal_append_failures_total: u64,
    pub actor_apply_after_append_failures_total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    BadRequest(String),
    Conflict(String),
    Internal(&'static str),
    NotFound,
}

impl From<domain::Error> for ApiError {
    fn from(value: domain::Error) -> Self {
        match value {
            domain::Error::ZeroValue
            | domain::Error::PriceNotTickAligned
            | domain::Error::QuantityNotLotAligned
            | domain::Error::SameMarketAssets
            | domain::Error::AmountConversionOverflow => Self::BadRequest(value.to_string()),
            domain::Error::InsufficientAvailableBalance
            | domain::Error::InsufficientReservedBalance
            | domain::Error::OrderAlreadyTerminal
            | domain::Error::OrderNotFound => Self::Conflict(value.to_string()),
            _ => Self::Internal("domain error"),
        }
    }
}

impl From<application::Error> for ApiError {
    fn from(value: application::Error) -> Self {
        match value {
            application::Error::Domain(error) => error.into(),
            application::Error::DuplicateCommand => Self::Conflict(value.to_string()),
            application::Error::ActorClosed => Self::Internal("actor closed"),
            application::Error::ReplayMismatch => Self::Internal("replay mismatch"),
            application::Error::JournalAppendFailed => Self::Internal("journal append failed"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::Internal(message) => (StatusCode::INTERNAL_SERVER_ERROR, message.to_owned()),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found".to_owned()),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ErrorResponse {
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn request_id_header_is_echoed() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health")
                    .header(X_REQUEST_ID_HEADER, "test-request-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(X_REQUEST_ID_HEADER).unwrap(),
            "test-request-1"
        );
    }

    #[tokio::test]
    async fn request_id_header_is_generated_when_missing() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let request_id = response
            .headers()
            .get(X_REQUEST_ID_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(request_id.starts_with("req-"));
    }

    #[test]
    fn boot_config_defaults_to_local_without_database() {
        assert_eq!(
            BootConfig::from_values(None, None).unwrap(),
            BootConfig {
                mode: BootMode::Local,
                database_url: None
            }
        );
    }

    #[test]
    fn boot_config_allows_explicit_local_without_database() {
        assert_eq!(
            BootConfig::from_values(Some("local"), None).unwrap(),
            BootConfig {
                mode: BootMode::Local,
                database_url: None
            }
        );
    }

    #[test]
    fn boot_config_requires_database_url_for_postgres() {
        assert!(matches!(
            BootConfig::from_values(Some("postgres"), None),
            Err(StartupError::Config(_))
        ));

        assert_eq!(
            BootConfig::from_values(Some("postgres"), Some("postgres://localhost/exchange"))
                .unwrap(),
            BootConfig {
                mode: BootMode::Postgres,
                database_url: Some("postgres://localhost/exchange".to_owned())
            }
        );
    }

    #[test]
    fn boot_config_rejects_unknown_mode() {
        assert!(matches!(
            BootConfig::from_values(Some("memory"), None),
            Err(StartupError::Config(_))
        ));
    }

    #[tokio::test]
    async fn app_from_local_config_reports_ready() {
        let service = app_from_config(BootConfig {
            mode: BootMode::Local,
            database_url: None,
        })
        .await
        .unwrap();

        let ready = service
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(ready.status(), StatusCode::OK);
        assert_eq!(
            response_json(ready).await,
            json!({
                "status": "ready",
                "market_id": "SOL-USDC",
                "boot_mode": "local",
                "journal_mode": "noop",
                "event_count": 0
            })
        );
    }

    #[tokio::test]
    async fn metrics_counts_http_requests_acceptance_and_errors() {
        let service = app();

        let health = service
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);

        let deposit = service
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/deposits")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "command_id": 1,
                            "trader_id": 1,
                            "asset_id": 1,
                            "amount": 7
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(deposit.status(), StatusCode::OK);

        let unfunded_order = service
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/orders")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "command_id": 2,
                            "order_id": 1,
                            "trader_id": 1,
                            "market_id": 1,
                            "side": "bid",
                            "price": 100,
                            "quantity": 7,
                            "sequence": 1
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unfunded_order.status(), StatusCode::CONFLICT);

        let metrics = service
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(metrics.status(), StatusCode::OK);
        assert_eq!(
            response_json(metrics).await,
            json!({
                "http_requests_total": 4,
                "ready_checks_total": 0,
                "snapshot_requests_total": 0,
                "deposits_accepted_total": 1,
                "orders_accepted_total": 0,
                "api_errors_total": 1,
                "actor_commands_received_total": 2,
                "actor_commands_accepted_total": 1,
                "actor_commands_rejected_total": 1,
                "actor_journal_append_failures_total": 0,
                "actor_apply_after_append_failures_total": 0
            })
        );
    }

    #[tokio::test]
    async fn metrics_counts_readiness_and_snapshots() {
        let service = app();

        let ready = service
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ready.status(), StatusCode::OK);

        let snapshot = service
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/markets/SOL-USDC/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(snapshot.status(), StatusCode::OK);

        let metrics = service
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(metrics.status(), StatusCode::OK);
        assert_eq!(
            response_json(metrics).await,
            json!({
                "http_requests_total": 3,
                "ready_checks_total": 1,
                "snapshot_requests_total": 1,
                "deposits_accepted_total": 0,
                "orders_accepted_total": 0,
                "api_errors_total": 0,
                "actor_commands_received_total": 2,
                "actor_commands_accepted_total": 2,
                "actor_commands_rejected_total": 0,
                "actor_journal_append_failures_total": 0,
                "actor_apply_after_append_failures_total": 0
            })
        );
    }

    #[tokio::test]
    async fn funded_crossing_orders_return_fill() {
        let service = app();

        for body in [
            json!({ "command_id": 1, "trader_id": 1, "asset_id": 1, "amount": 7 }),
            json!({ "command_id": 2, "trader_id": 2, "asset_id": 2, "amount": 735 }),
        ] {
            let response = service
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/deposits")
                        .header("content-type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let ask = service
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/orders")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "command_id": 3,
                            "order_id": 1,
                            "trader_id": 1,
                            "market_id": 1,
                            "side": "ask",
                            "price": 100,
                            "quantity": 7,
                            "sequence": 1
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ask.status(), StatusCode::OK);

        let bid = service
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/orders")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "command_id": 4,
                            "order_id": 2,
                            "trader_id": 2,
                            "market_id": 1,
                            "side": "bid",
                            "price": 105,
                            "quantity": 7,
                            "sequence": 2
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(bid.status(), StatusCode::OK);
        assert_eq!(
            response_json(bid).await,
            json!({
                "accepted": true,
                "fills": [{
                    "maker_order_id": 1,
                    "taker_order_id": 2,
                    "price": 100,
                    "quantity": 7
                }]
            })
        );
    }

    #[tokio::test]
    async fn unfunded_order_returns_conflict() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/orders")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "command_id": 1,
                            "order_id": 1,
                            "trader_id": 1,
                            "market_id": 1,
                            "side": "bid",
                            "price": 100,
                            "quantity": 7,
                            "sequence": 1
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn app_with_replayed_actor_restores_event_count_and_rejects_duplicate_command() {
        let market = default_market();
        let mut source = application::ExchangeApplication::new(market);
        source
            .credit_deposit(
                application::CommandId::new(1).unwrap(),
                TraderId::new(1).unwrap(),
                AssetId::new(1).unwrap(),
                BalanceAmount::new(7),
            )
            .unwrap();
        let replayed =
            application::ExchangeApplication::replay(market, source.events().iter().cloned())
                .unwrap();
        let actor = MarketActorHandle::spawn_from_app(replayed, 8, runtime::NoopEventJournal);
        let service = app_with_actor(
            MARKET_SOL_USDC,
            ReadyBootMode::Local,
            JournalMode::Noop,
            market,
            actor,
        );

        let ready = service
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ready.status(), StatusCode::OK);
        assert_eq!(
            response_json(ready).await,
            json!({
                "status": "ready",
                "market_id": "SOL-USDC",
                "boot_mode": "local",
                "journal_mode": "noop",
                "event_count": 1
            })
        );

        let duplicate = service
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/deposits")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "command_id": 1,
                            "trader_id": 2,
                            "asset_id": 1,
                            "amount": 7
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn app_with_postgres_boots_and_reports_ready() {
        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let service = app_with_postgres(&database_url).await.unwrap();

        let ready = service
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(ready.status(), StatusCode::OK);
        let body = response_json(ready).await;
        assert_eq!(body["status"], "ready");
        assert_eq!(body["market_id"], MARKET_SOL_USDC);
        assert_eq!(body["boot_mode"], "postgres");
        assert_eq!(body["journal_mode"], "postgres");
        assert!(body["event_count"].is_number());
    }
}
