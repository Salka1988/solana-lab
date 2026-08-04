#![forbid(unsafe_code)]

use core::fmt;
use std::env;

use async_trait::async_trait;
use axum::{
    extract::{Path, State},
    http::StatusCode,
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

#[derive(Clone)]
pub struct ServiceState {
    market_id: String,
    market: MarketSpec,
    actor: MarketActorHandle,
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
    app_with_actor(MARKET_SOL_USDC, market, actor)
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

    Ok(app_with_actor(MARKET_SOL_USDC, market, actor))
}

pub fn app_with_actor(
    market_id: impl Into<String>,
    market: MarketSpec,
    actor: MarketActorHandle,
) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/deposits", post(credit_deposit))
        .route("/orders", post(place_order))
        .route("/markets/{market_id}/snapshot", get(snapshot))
        .with_state(ServiceState {
            market_id: market_id.into(),
            market,
            actor,
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

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn ready(State(state): State<ServiceState>) -> Result<Json<ReadyResponse>, ApiError> {
    let snapshot = actor_snapshot(&state.actor).await?;
    info!(
        market_id = %state.market_id,
        event_count = snapshot.event_count,
        "readiness checked"
    );
    Ok(Json(ReadyResponse {
        status: "ready",
        market_id: state.market_id,
        event_count: snapshot.event_count,
    }))
}

async fn credit_deposit(
    State(state): State<ServiceState>,
    Json(request): Json<DepositRequest>,
) -> Result<Json<DepositResponse>, ApiError> {
    info!(
        command_id = request.command_id,
        trader_id = request.trader_id,
        asset_id = request.asset_id,
        amount = request.amount,
        "credit deposit requested"
    );
    let reply = state
        .actor
        .credit_deposit(
            command_id(request.command_id)?,
            trader_id(request.trader_id)?,
            asset_id(request.asset_id)?,
            BalanceAmount::new(request.amount),
        )
        .await?;

    match reply {
        MarketReply::DepositCredited => {
            info!(command_id = request.command_id, "credit deposit accepted");
            Ok(Json(DepositResponse { accepted: true }))
        }
        _ => Err(ApiError::Internal("unexpected deposit reply")),
    }
}

async fn place_order(
    State(state): State<ServiceState>,
    Json(request): Json<OrderRequest>,
) -> Result<Json<OrderResponse>, ApiError> {
    info!(
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
    let order = Order::new(
        order_id(request.order_id)?,
        trader_id(request.trader_id)?,
        MarketId::new(request.market_id)?,
        side(request.side)?,
        Price::new(request.price)?,
        Quantity::new(request.quantity)?,
        OrderSequence::new(request.sequence)?,
    );

    state.market.validate_price(order.price())?;
    state.market.validate_quantity(order.original_quantity())?;

    let reply = state
        .actor
        .place_order(command_id(request.command_id)?, order)
        .await?;

    match reply {
        MarketReply::OrderPlaced { fills } => {
            info!(
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
    Path(market_id): Path<String>,
) -> Result<Json<SnapshotResponse>, ApiError> {
    if market_id != state.market_id {
        warn!(requested_market_id = %market_id, configured_market_id = %state.market_id, "unknown market snapshot requested");
        return Err(ApiError::NotFound);
    }

    let snapshot = actor_snapshot(&state.actor).await?;
    info!(market_id = %market_id, event_count = snapshot.event_count, "snapshot returned");
    Ok(Json(SnapshotResponse {
        market_id,
        event_count: snapshot.event_count,
    }))
}

async fn actor_snapshot(actor: &MarketActorHandle) -> Result<MarketSnapshot, ApiError> {
    match actor.snapshot().await? {
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
    pub event_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SnapshotResponse {
    pub market_id: String,
    pub event_count: usize,
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
                "event_count": 0
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
        let service = app_with_actor(MARKET_SOL_USDC, market, actor);

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
        assert!(body["event_count"].is_number());
    }
}
