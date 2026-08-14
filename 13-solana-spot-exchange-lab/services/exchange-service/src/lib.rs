#![forbid(unsafe_code)]

use core::fmt;
use std::{
    collections::BTreeMap,
    env,
    net::SocketAddr,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex},
    time::Duration,
};

use anchor_lang::prelude::Pubkey;
use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{Extension, Path, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use domain::{
    AssetId, BalanceAmount, LotSize, MarketId, MarketSpec, Order, OrderId, OrderSequence, Price,
    Quantity, Side, TickSize, TraderId,
};
use persistence::{
    PersistenceError, PostgresEventJournal, SettlementOutboxItem, SettlementOutboxRow,
};
use runtime::{MarketActorHandle, MarketReply, MarketSnapshot};
use serde::{Deserialize, Serialize};
use solana_keypair::Keypair;
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

const MARKET_SOL_USDC: &str = "SOL-USDC";
pub const EXCHANGE_BOOT_MODE_ENV: &str = "EXCHANGE_BOOT_MODE";
pub const EXCHANGE_HTTP_ADDR_ENV: &str = "EXCHANGE_HTTP_ADDR";
pub const EXCHANGE_API_KEY_ENV: &str = "EXCHANGE_API_KEY";
pub const DATABASE_URL_ENV: &str = "DATABASE_URL";
pub const EXCHANGE_RELAYER_ENABLED_ENV: &str = "EXCHANGE_RELAYER_ENABLED";
pub const EXCHANGE_SOLANA_RPC_URL_ENV: &str = "EXCHANGE_SOLANA_RPC_URL";
pub const EXCHANGE_RELAYER_MARKET_ID_ENV: &str = "EXCHANGE_RELAYER_MARKET_ID";
pub const EXCHANGE_RELAYER_SETTLEMENT_AUTHORITY_ENV: &str = "EXCHANGE_RELAYER_SETTLEMENT_AUTHORITY";
pub const EXCHANGE_RELAYER_BASE_MINT_ENV: &str = "EXCHANGE_RELAYER_BASE_MINT";
pub const EXCHANGE_RELAYER_QUOTE_MINT_ENV: &str = "EXCHANGE_RELAYER_QUOTE_MINT";
pub const EXCHANGE_RELAYER_PAYER_ENV: &str = "EXCHANGE_RELAYER_PAYER";
pub const EXCHANGE_RELAYER_SETTLEMENT_AUTHORITY_KEYPAIR_ENV: &str =
    "EXCHANGE_RELAYER_SETTLEMENT_AUTHORITY_KEYPAIR";
pub const EXCHANGE_RELAYER_PAYER_KEYPAIR_ENV: &str = "EXCHANGE_RELAYER_PAYER_KEYPAIR";
pub const EXCHANGE_RELAYER_INTERVAL_MS_ENV: &str = "EXCHANGE_RELAYER_INTERVAL_MS";
pub const X_API_KEY_HEADER: &str = "x-api-key";
pub const X_REQUEST_ID_HEADER: &str = "x-request-id";
const DEFAULT_RELAYER_INTERVAL_MS: u64 = 1_000;
const SETTLEMENT_WORKER_BATCH_LIMIT: i64 = 32;
const SETTLEMENT_OUTBOX_VIEW_LIMIT: i64 = 50;

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
    settlement: SettlementEdgeState,
    settlement_outbox: Option<PostgresEventJournal>,
    metrics: ServiceMetrics,
    api_key: Option<String>,
    settlement_config: Option<SettlementBridgeConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementBridgeConfig {
    pub market_id: MarketId,
    pub settlement_authority: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub payer: Pubkey,
}

#[derive(Debug, Clone, Default)]
pub struct SettlementEdgeState {
    inner: Arc<SettlementEdgeStateInner>,
}

#[derive(Debug, Default)]
struct SettlementEdgeStateInner {
    signed_orders: Mutex<BTreeMap<OrderId, relayer::SettlementSignedOrder>>,
    queued_requests: Mutex<Vec<relayer::SignedSettlementRequest>>,
    next_settlement_id: AtomicU64,
}

impl SettlementEdgeState {
    fn store_signed_order(
        &self,
        order_id: OrderId,
        signed_order: relayer::SettlementSignedOrder,
    ) -> bool {
        self.inner
            .signed_orders
            .lock()
            .expect("signed order store poisoned")
            .insert(order_id, signed_order)
            .is_some()
    }

    pub fn allocate_settlement_ids(&self, count: usize) -> Result<Option<u64>, ApiError> {
        if count == 0 {
            return Ok(None);
        }

        let count = u64::try_from(count)
            .map_err(|_| ApiError::Internal("settlement id allocation overflow"))?;
        let first = self
            .inner
            .next_settlement_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(count)
            })
            .map_err(|_| ApiError::Internal("settlement id allocation overflow"))?;

        Ok(Some(first))
    }

    fn queue_settlement_requests(&self, requests: Vec<relayer::SignedSettlementRequest>) {
        self.inner
            .queued_requests
            .lock()
            .expect("settlement request queue poisoned")
            .extend(requests);
    }

    fn queued_settlement_count(&self) -> usize {
        self.inner
            .queued_requests
            .lock()
            .expect("settlement request queue poisoned")
            .len()
    }

    pub fn drain_queued_settlement_requests(&self) -> Vec<relayer::SignedSettlementRequest> {
        std::mem::take(
            &mut self
                .inner
                .queued_requests
                .lock()
                .expect("settlement request queue poisoned"),
        )
    }
}

impl relayer::SettlementSignedOrderSource for SettlementEdgeState {
    fn signed_order(&self, order_id: OrderId) -> Option<relayer::SettlementSignedOrder> {
        self.inner
            .signed_orders
            .lock()
            .expect("signed order store poisoned")
            .get(&order_id)
            .copied()
    }
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
    settlement_requests_queued_total: AtomicU64,
    settlement_requests_submitted_total: AtomicU64,
    settlement_requests_failed_total: AtomicU64,
    api_errors_total: AtomicU64,
}

impl ServiceMetrics {
    fn snapshot(
        &self,
        runtime: runtime::RuntimeMetricsSnapshot,
        settlements_pending: usize,
    ) -> MetricsResponse {
        MetricsResponse {
            http_requests_total: self.inner.http_requests_total.load(Ordering::Relaxed),
            ready_checks_total: self.inner.ready_checks_total.load(Ordering::Relaxed),
            snapshot_requests_total: self.inner.snapshot_requests_total.load(Ordering::Relaxed),
            deposits_accepted_total: self.inner.deposits_accepted_total.load(Ordering::Relaxed),
            orders_accepted_total: self.inner.orders_accepted_total.load(Ordering::Relaxed),
            settlement_requests_queued_total: self
                .inner
                .settlement_requests_queued_total
                .load(Ordering::Relaxed),
            settlement_requests_submitted_total: self
                .inner
                .settlement_requests_submitted_total
                .load(Ordering::Relaxed),
            settlement_requests_failed_total: self
                .inner
                .settlement_requests_failed_total
                .load(Ordering::Relaxed),
            settlement_requests_pending: u64::try_from(settlements_pending).unwrap_or(u64::MAX),
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

    fn record_settlement_requests_queued(&self, count: usize) {
        self.inner
            .settlement_requests_queued_total
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    fn record_settlement_requests_submitted(&self, count: usize) {
        self.inner
            .settlement_requests_submitted_total
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    fn record_settlement_requests_failed(&self, count: usize) {
        self.inner
            .settlement_requests_failed_total
            .fetch_add(count as u64, Ordering::Relaxed);
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
    let relayer = config.relayer.clone();
    match config.mode {
        BootMode::Local => {
            let market = default_market();
            let actor = MarketActorHandle::spawn(market, 1024);
            Ok(app_with_actor_api_key_and_relayer(
                MARKET_SOL_USDC,
                ReadyBootMode::Local,
                JournalMode::Noop,
                market,
                actor,
                config.api_key,
                relayer,
            ))
        }
        BootMode::Postgres => {
            app_with_postgres_and_api_key(
                config.database_url.as_deref().ok_or_else(|| {
                    StartupError::Config(format!("{DATABASE_URL_ENV} is required"))
                })?,
                config.api_key,
                relayer,
            )
            .await
        }
    }
}

pub async fn app_with_postgres(database_url: &str) -> Result<Router, StartupError> {
    app_with_postgres_and_api_key(database_url, None, RelayerBootConfig::Disabled).await
}

async fn app_with_postgres_and_api_key(
    database_url: &str,
    api_key: Option<String>,
    relayer: RelayerBootConfig,
) -> Result<Router, StartupError> {
    info!("connecting postgres event journal");
    let journal = PostgresEventJournal::connect(database_url).await?;
    app_with_postgres_journal(journal, api_key, relayer).await
}

async fn app_with_postgres_journal(
    journal: PostgresEventJournal,
    api_key: Option<String>,
    relayer: RelayerBootConfig,
) -> Result<Router, StartupError> {
    let market = default_market();
    info!("running postgres migrations");
    journal.migrate().await?;

    let events = journal.read_all().await?;
    info!(event_count = events.len(), "replaying exchange events");
    let exchange = application::ExchangeApplication::replay(market, events)?;
    let settlement_outbox = Some(journal.clone());
    let actor =
        MarketActorHandle::spawn_from_app(exchange, 1024, PostgresRuntimeJournal::new(journal));

    Ok(app_with_actor_api_key_and_relayer_and_outbox(
        MARKET_SOL_USDC,
        ReadyBootMode::Postgres,
        JournalMode::Postgres,
        market,
        actor,
        api_key,
        relayer,
        settlement_outbox,
    ))
}

pub fn app_with_actor(
    market_id: impl Into<String>,
    boot_mode: ReadyBootMode,
    journal_mode: JournalMode,
    market: MarketSpec,
    actor: MarketActorHandle,
) -> Router {
    app_with_actor_and_api_key(market_id, boot_mode, journal_mode, market, actor, None)
}

pub fn app_with_actor_and_api_key(
    market_id: impl Into<String>,
    boot_mode: ReadyBootMode,
    journal_mode: JournalMode,
    market: MarketSpec,
    actor: MarketActorHandle,
    api_key: Option<String>,
) -> Router {
    app_with_actor_api_key_and_relayer(
        market_id,
        boot_mode,
        journal_mode,
        market,
        actor,
        api_key,
        RelayerBootConfig::Disabled,
    )
}

pub fn app_with_actor_api_key_and_relayer(
    market_id: impl Into<String>,
    boot_mode: ReadyBootMode,
    journal_mode: JournalMode,
    market: MarketSpec,
    actor: MarketActorHandle,
    api_key: Option<String>,
    relayer: RelayerBootConfig,
) -> Router {
    app_with_actor_api_key_and_relayer_and_outbox(
        market_id,
        boot_mode,
        journal_mode,
        market,
        actor,
        api_key,
        relayer,
        None,
    )
}

fn app_with_actor_api_key_and_relayer_and_outbox(
    market_id: impl Into<String>,
    boot_mode: ReadyBootMode,
    journal_mode: JournalMode,
    market: MarketSpec,
    actor: MarketActorHandle,
    api_key: Option<String>,
    relayer: RelayerBootConfig,
    settlement_outbox: Option<PostgresEventJournal>,
) -> Router {
    let (settlement_config, worker_config) = match relayer {
        RelayerBootConfig::Disabled => (None, None),
        RelayerBootConfig::Enabled(config) => (Some(config.bridge), Some(config)),
    };

    app_with_actor_api_key_settlement_and_worker(
        market_id,
        boot_mode,
        journal_mode,
        market,
        actor,
        api_key,
        settlement_config,
        worker_config,
        settlement_outbox,
    )
}

pub fn app_with_actor_api_key_and_settlement(
    market_id: impl Into<String>,
    boot_mode: ReadyBootMode,
    journal_mode: JournalMode,
    market: MarketSpec,
    actor: MarketActorHandle,
    api_key: Option<String>,
    settlement_config: Option<SettlementBridgeConfig>,
) -> Router {
    app_with_actor_api_key_settlement_and_worker(
        market_id,
        boot_mode,
        journal_mode,
        market,
        actor,
        api_key,
        settlement_config,
        None,
        None,
    )
}

fn app_with_actor_api_key_settlement_and_worker(
    market_id: impl Into<String>,
    boot_mode: ReadyBootMode,
    journal_mode: JournalMode,
    market: MarketSpec,
    actor: MarketActorHandle,
    api_key: Option<String>,
    settlement_config: Option<SettlementBridgeConfig>,
    worker_config: Option<RelayerWorkerConfig>,
    settlement_outbox: Option<PostgresEventJournal>,
) -> Router {
    let metrics = ServiceMetrics::default();
    let settlement = SettlementEdgeState::default();
    if let Some(worker_config) = worker_config {
        spawn_settlement_worker(
            settlement.clone(),
            settlement_outbox.clone(),
            metrics.clone(),
            worker_config,
        );
    }

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(prometheus_metrics_endpoint))
        .route("/metrics.json", get(metrics_json_endpoint))
        .route("/deposits", post(credit_deposit))
        .route("/orders", post(place_order))
        .route("/signed-orders", post(register_signed_order))
        .route("/settlements/pending", get(pending_settlements))
        .route("/settlements/outbox", get(list_settlement_outbox))
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
            settlement,
            settlement_outbox,
            metrics,
            api_key,
            settlement_config,
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

fn spawn_settlement_worker(
    settlement: SettlementEdgeState,
    settlement_outbox: Option<PostgresEventJournal>,
    metrics: ServiceMetrics,
    config: RelayerWorkerConfig,
) {
    tokio::spawn(async move {
        let settlement_authority =
            match Keypair::try_from_base58_string(&config.settlement_authority_keypair) {
                Ok(keypair) => keypair,
                Err(error) => {
                    warn!(error = %error, "settlement authority keypair rejected");
                    return;
                }
            };
        let payer = match Keypair::try_from_base58_string(&config.payer_keypair) {
            Ok(keypair) => keypair,
            Err(error) => {
                warn!(error = %error, "settlement payer keypair rejected");
                return;
            }
        };
        let rpc_submitter = relayer::RpcSubmitter::new(
            relayer::RpcBlockhashProvider::new(config.rpc_url.clone()),
            relayer::InMemoryTransactionSigner::new(vec![settlement_authority, payer]),
            relayer::RpcTransactionSender::new(config.rpc_url.clone()),
        );
        let confirming = relayer::ConfirmingSubmitter::new(
            rpc_submitter,
            relayer::PollingConfirmer::new(
                relayer::RpcConfirmationPoller::new(config.rpc_url),
                relayer::PollingPolicy::default(),
            ),
        );
        let retrying = relayer::RetryingSubmitter::new(confirming, relayer::RetryPolicy::default());
        let dead_lettering =
            relayer::DeadLetteringSubmitter::new(retrying, relayer::RecordingDeadLetterSink::new());
        let mut worker = relayer::SettlementRequestWorker::new(dead_lettering);
        let interval = Duration::from_millis(config.interval_ms.max(1));

        loop {
            let work = claim_settlement_work(&settlement, settlement_outbox.as_ref()).await;
            for item in work {
                let report = worker.submit_requests([item.request]).await;
                if let Some(submitted) = report.submitted.first() {
                    metrics.record_settlement_requests_submitted(1);
                    if let Some(outbox) = settlement_outbox.as_ref() {
                        if let Some(outbox_id) = item.outbox_id {
                            if let Err(error) = outbox
                                .mark_settlement_submitted(outbox_id, submitted.signature)
                                .await
                            {
                                warn!(error = %error, outbox_id, "settlement outbox update failed");
                            }
                        }
                    }
                } else if let Some(failure) = report.failed.first() {
                    let retryable =
                        failure.error.is_retryable() && item.attempts < item.max_attempts;
                    if let Some(outbox) = settlement_outbox.as_ref() {
                        if let Some(outbox_id) = item.outbox_id {
                            let error_text = format!("{:?}", failure.error);
                            let update = if retryable {
                                outbox.keep_settlement_pending(outbox_id, &error_text).await
                            } else {
                                outbox.mark_settlement_failed(outbox_id, &error_text).await
                            };
                            if let Err(error) = update {
                                warn!(error = %error, outbox_id, "settlement outbox update failed");
                            }
                        }
                    }
                    if !retryable {
                        metrics.record_settlement_requests_failed(1);
                    }
                    warn!(error = ?failure.error, "settlement worker reported failed submission");
                }
            }
            tokio::time::sleep(interval).await;
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SettlementWorkItem {
    outbox_id: Option<i64>,
    attempts: i32,
    max_attempts: i32,
    request: relayer::SignedSettlementRequest,
}

async fn claim_settlement_work(
    settlement: &SettlementEdgeState,
    settlement_outbox: Option<&PostgresEventJournal>,
) -> Vec<SettlementWorkItem> {
    if let Some(outbox) = settlement_outbox {
        return match outbox
            .claim_pending_settlements(SETTLEMENT_WORKER_BATCH_LIMIT)
            .await
        {
            Ok(items) => items.into_iter().map(SettlementWorkItem::from).collect(),
            Err(error) => {
                warn!(error = %error, "settlement outbox claim failed");
                Vec::new()
            }
        };
    }

    settlement
        .drain_queued_settlement_requests()
        .into_iter()
        .map(|request| SettlementWorkItem {
            outbox_id: None,
            attempts: 1,
            max_attempts: 1,
            request,
        })
        .collect()
}

impl From<SettlementOutboxItem> for SettlementWorkItem {
    fn from(item: SettlementOutboxItem) -> Self {
        Self {
            outbox_id: Some(item.outbox_id),
            attempts: item.attempts,
            max_attempts: item.max_attempts,
            request: item.request,
        }
    }
}

pub fn http_addr_from_env() -> Result<SocketAddr, StartupError> {
    http_addr_from_value(env::var(EXCHANGE_HTTP_ADDR_ENV).ok().as_deref())
}

pub fn http_addr_from_value(value: Option<&str>) -> Result<SocketAddr, StartupError> {
    value
        .unwrap_or("127.0.0.1:3000")
        .parse()
        .map_err(|error| StartupError::Config(format!("invalid {EXCHANGE_HTTP_ADDR_ENV}: {error}")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootConfig {
    pub mode: BootMode,
    pub database_url: Option<String>,
    pub api_key: Option<String>,
    pub relayer: RelayerBootConfig,
}

impl BootConfig {
    pub fn from_env() -> Result<Self, StartupError> {
        let mut config = Self::from_values(
            env::var(EXCHANGE_BOOT_MODE_ENV).ok().as_deref(),
            env::var(DATABASE_URL_ENV).ok().as_deref(),
            env::var(EXCHANGE_API_KEY_ENV).ok().as_deref(),
        )?;
        config.relayer = RelayerBootConfig::from_env()?;
        Ok(config)
    }

    pub fn from_values(
        boot_mode: Option<&str>,
        database_url: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<Self, StartupError> {
        let mode = boot_mode.map_or(Ok(BootMode::Local), BootMode::parse)?;
        let database_url = database_url.map(str::to_owned);
        let api_key = api_key.filter(|value| !value.is_empty()).map(str::to_owned);

        if mode == BootMode::Postgres && database_url.is_none() {
            return Err(StartupError::Config(format!(
                "{DATABASE_URL_ENV} is required"
            )));
        }

        Ok(Self {
            mode,
            database_url,
            api_key,
            relayer: RelayerBootConfig::Disabled,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayerBootConfig {
    Disabled,
    Enabled(RelayerWorkerConfig),
}

impl RelayerBootConfig {
    fn from_env() -> Result<Self, StartupError> {
        if !env_flag(env::var(EXCHANGE_RELAYER_ENABLED_ENV).ok().as_deref())? {
            return Ok(Self::Disabled);
        }

        let rpc_url = required_env(EXCHANGE_SOLANA_RPC_URL_ENV)?;
        let market_id = parse_market_id_env(EXCHANGE_RELAYER_MARKET_ID_ENV)?;
        let settlement_authority = parse_pubkey_env(EXCHANGE_RELAYER_SETTLEMENT_AUTHORITY_ENV)?;
        let base_mint = parse_pubkey_env(EXCHANGE_RELAYER_BASE_MINT_ENV)?;
        let quote_mint = parse_pubkey_env(EXCHANGE_RELAYER_QUOTE_MINT_ENV)?;
        let payer = parse_pubkey_env(EXCHANGE_RELAYER_PAYER_ENV)?;
        let settlement_authority_keypair =
            required_env(EXCHANGE_RELAYER_SETTLEMENT_AUTHORITY_KEYPAIR_ENV)?;
        let payer_keypair = required_env(EXCHANGE_RELAYER_PAYER_KEYPAIR_ENV)?;
        validate_keypair(
            &settlement_authority_keypair,
            EXCHANGE_RELAYER_SETTLEMENT_AUTHORITY_KEYPAIR_ENV,
        )?;
        validate_keypair(&payer_keypair, EXCHANGE_RELAYER_PAYER_KEYPAIR_ENV)?;
        let interval_ms = env::var(EXCHANGE_RELAYER_INTERVAL_MS_ENV)
            .ok()
            .as_deref()
            .map(parse_interval_ms)
            .transpose()?
            .unwrap_or(DEFAULT_RELAYER_INTERVAL_MS);

        Ok(Self::Enabled(RelayerWorkerConfig {
            bridge: SettlementBridgeConfig {
                market_id,
                settlement_authority,
                base_mint,
                quote_mint,
                payer,
            },
            rpc_url,
            settlement_authority_keypair,
            payer_keypair,
            interval_ms,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayerWorkerConfig {
    pub bridge: SettlementBridgeConfig,
    pub rpc_url: String,
    pub settlement_authority_keypair: String,
    pub payer_keypair: String,
    pub interval_ms: u64,
}

fn env_flag(value: Option<&str>) -> Result<bool, StartupError> {
    match value.unwrap_or("") {
        "" | "0" | "false" | "False" | "FALSE" => Ok(false),
        "1" | "true" | "True" | "TRUE" => Ok(true),
        value => Err(StartupError::Config(format!(
            "invalid {EXCHANGE_RELAYER_ENABLED_ENV}: {value}"
        ))),
    }
}

fn required_env(name: &'static str) -> Result<String, StartupError> {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| StartupError::Config(format!("{name} is required")))
}

fn parse_market_id_env(name: &'static str) -> Result<MarketId, StartupError> {
    let value = required_env(name)?;
    let parsed = value
        .parse::<u32>()
        .map_err(|error| StartupError::Config(format!("invalid {name}: {error}")))?;
    MarketId::new(parsed).map_err(|error| StartupError::Config(format!("invalid {name}: {error}")))
}

fn parse_pubkey_env(name: &'static str) -> Result<Pubkey, StartupError> {
    let value = required_env(name)?;
    Pubkey::from_str(&value)
        .map_err(|error| StartupError::Config(format!("invalid {name}: {error}")))
}

fn parse_interval_ms(value: &str) -> Result<u64, StartupError> {
    value.parse::<u64>().map_err(|error| {
        StartupError::Config(format!(
            "invalid {EXCHANGE_RELAYER_INTERVAL_MS_ENV}: {error}"
        ))
    })
}

fn validate_keypair(value: &str, name: &'static str) -> Result<(), StartupError> {
    Keypair::try_from_base58_string(value)
        .map(|_| ())
        .map_err(|error| StartupError::Config(format!("invalid {name}: {error}")))
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

async fn metrics_json_endpoint(State(state): State<ServiceState>) -> Json<MetricsResponse> {
    Json(state.metrics.snapshot(
        state.actor.metrics_snapshot(),
        pending_settlement_count(&state).await,
    ))
}

async fn prometheus_metrics_endpoint(State(state): State<ServiceState>) -> impl IntoResponse {
    let metrics = state.metrics.snapshot(
        state.actor.metrics_snapshot(),
        pending_settlement_count(&state).await,
    );
    (
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        metrics.to_prometheus_text(),
    )
}

async fn credit_deposit(
    State(state): State<ServiceState>,
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    Json(request): Json<DepositRequest>,
) -> Result<Json<DepositResponse>, ApiError> {
    authorize_api_key(&state, &headers).map_err(|error| record_api_error(error, &state.metrics))?;
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
    headers: HeaderMap,
    Json(request): Json<OrderRequest>,
) -> Result<Json<OrderResponse>, ApiError> {
    authorize_api_key(&state, &headers).map_err(|error| record_api_error(error, &state.metrics))?;
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
            queue_settlement_requests(&state, request.command_id, order, &fills, &request_id).await;
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

async fn queue_settlement_requests(
    state: &ServiceState,
    command_id: u128,
    taker_order: Order,
    fills: &[domain::Fill],
    request_id: &RequestId,
) {
    let Some(config) = state.settlement_config else {
        return;
    };
    if fills.is_empty() {
        return;
    }

    let Ok(Some(first_settlement_id)) = state.settlement.allocate_settlement_ids(fills.len())
    else {
        warn!(
            request_id = %request_id.as_str(),
            command_id,
            fill_count = fills.len(),
            "settlement id allocation failed"
        );
        return;
    };
    let Ok(command_id) = application::CommandId::new(command_id) else {
        warn!(
            request_id = %request_id.as_str(),
            "settlement batch command id was invalid"
        );
        return;
    };

    let batch = application::SettlementBatch::from_order_fills(command_id, taker_order, fills);
    let bridge = relayer::ApplicationSettlementBridge::new(
        config.market_id,
        config.settlement_authority,
        config.base_mint,
        config.quote_mint,
        config.payer,
    );

    match bridge.requests_from_batch(&batch, &state.settlement, first_settlement_id) {
        Ok(requests) => {
            let settlement_count = requests.len();
            if let Some(outbox) = state.settlement_outbox.as_ref() {
                if let Err(error) = outbox.enqueue_settlement_requests(&requests).await {
                    warn!(
                        request_id = %request_id.as_str(),
                        error = %error,
                        settlement_count,
                        "settlement outbox enqueue failed"
                    );
                    return;
                }
            } else {
                state.settlement.queue_settlement_requests(requests);
            }
            state
                .metrics
                .record_settlement_requests_queued(settlement_count);
            info!(
                request_id = %request_id.as_str(),
                settlement_count,
                first_settlement_id,
                "settlement requests queued"
            );
        }
        Err(error) => {
            warn!(
                request_id = %request_id.as_str(),
                error = ?error,
                "settlement request bridging skipped"
            );
        }
    }
}

async fn register_signed_order(
    State(state): State<ServiceState>,
    headers: HeaderMap,
    Json(request): Json<SignedOrderRequest>,
) -> Result<Json<SignedOrderResponse>, ApiError> {
    authorize_api_key(&state, &headers).map_err(|error| record_api_error(error, &state.metrics))?;

    let order_id =
        order_id(request.order_id).map_err(|error| record_api_error(error, &state.metrics))?;
    let trader_id =
        trader_id(request.trader_id).map_err(|error| record_api_error(error, &state.metrics))?;
    let order_id_u64 = u64::try_from(order_id.get()).map_err(|_| {
        record_api_error(
            ApiError::BadRequest("order_id exceeds u64".to_owned()),
            &state.metrics,
        )
    })?;
    let order_hash = bytes32(request.order_hash, "order_hash")
        .map_err(|error| record_api_error(error, &state.metrics))?;
    let trader = pubkey_from_bytes(request.trader_pubkey, "trader_pubkey")
        .map_err(|error| record_api_error(error, &state.metrics))?;
    let market_config = pubkey_from_bytes(request.market_config, "market_config")
        .map_err(|error| record_api_error(error, &state.metrics))?;
    let signature = bytes64(request.signature, "signature")
        .map_err(|error| record_api_error(error, &state.metrics))?;

    let replaced = state.settlement.store_signed_order(
        order_id,
        relayer::SettlementSignedOrder {
            trader_id,
            order_hash,
            order: spot_settlement::SignedOrderPayload {
                order_id: order_id_u64,
                market_config,
                trader,
                side: signed_order_side(request.side),
                price: request.price,
                quantity: request.quantity,
                nonce: request.nonce,
                expiry_slot: request.expiry_slot,
            },
            signature,
        },
    );

    Ok(Json(SignedOrderResponse {
        accepted: true,
        replaced,
    }))
}

async fn pending_settlements(
    State(state): State<ServiceState>,
) -> Json<PendingSettlementsResponse> {
    Json(PendingSettlementsResponse {
        queued: pending_settlement_count(&state).await,
    })
}

async fn list_settlement_outbox(
    State(state): State<ServiceState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SettlementOutboxRowResponse>>, ApiError> {
    authorize_api_key(&state, &headers).map_err(|error| record_api_error(error, &state.metrics))?;

    let Some(outbox) = state.settlement_outbox.as_ref() else {
        return Ok(Json(Vec::new()));
    };

    let rows = outbox
        .recent_settlement_outbox(SETTLEMENT_OUTBOX_VIEW_LIMIT)
        .await
        .map_err(|error| {
            warn!(error = %error, "settlement outbox list failed");
            record_api_error(
                ApiError::Internal("settlement outbox list failed"),
                &state.metrics,
            )
        })?;

    Ok(Json(
        rows.into_iter()
            .map(SettlementOutboxRowResponse::from)
            .collect(),
    ))
}

async fn pending_settlement_count(state: &ServiceState) -> usize {
    if let Some(outbox) = state.settlement_outbox.as_ref() {
        return match outbox.settlement_pending_count().await {
            Ok(count) => count,
            Err(error) => {
                warn!(error = %error, "settlement outbox pending count failed");
                0
            }
        };
    }

    state.settlement.queued_settlement_count()
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

fn authorize_api_key(state: &ServiceState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state.api_key.as_deref() else {
        return Ok(());
    };

    match headers
        .get(X_API_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
    {
        Some(actual) if actual == expected => Ok(()),
        _ => Err(ApiError::Unauthorized),
    }
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

fn signed_order_side(value: OrderSideDto) -> spot_settlement::SignedOrderSide {
    match value {
        OrderSideDto::Bid => spot_settlement::SignedOrderSide::Bid,
        OrderSideDto::Ask => spot_settlement::SignedOrderSide::Ask,
    }
}

fn pubkey_from_bytes(value: Vec<u8>, field: &'static str) -> Result<Pubkey, ApiError> {
    bytes32(value, field).map(Pubkey::new_from_array)
}

fn bytes32(value: Vec<u8>, field: &'static str) -> Result<[u8; 32], ApiError> {
    value
        .try_into()
        .map_err(|_| ApiError::BadRequest(format!("{field} must contain 32 bytes")))
}

fn bytes64(value: Vec<u8>, field: &'static str) -> Result<[u8; 64], ApiError> {
    value
        .try_into()
        .map_err(|_| ApiError::BadRequest(format!("{field} must contain 64 bytes")))
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

#[derive(Debug, Clone, Deserialize)]
pub struct SignedOrderRequest {
    pub order_id: u128,
    pub trader_id: u64,
    pub order_hash: Vec<u8>,
    pub trader_pubkey: Vec<u8>,
    pub market_config: Vec<u8>,
    pub side: OrderSideDto,
    pub price: u64,
    pub quantity: u64,
    pub nonce: u64,
    pub expiry_slot: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct SignedOrderResponse {
    pub accepted: bool,
    pub replaced: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct PendingSettlementsResponse {
    pub queued: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementOutboxRowResponse {
    pub outbox_id: i64,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<SettlementOutboxRow> for SettlementOutboxRowResponse {
    fn from(row: SettlementOutboxRow) -> Self {
        Self {
            outbox_id: row.outbox_id,
            status: row.status,
            attempts: row.attempts,
            max_attempts: row.max_attempts,
            last_error: row.last_error,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
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
    pub settlement_requests_queued_total: u64,
    pub settlement_requests_submitted_total: u64,
    pub settlement_requests_failed_total: u64,
    pub settlement_requests_pending: u64,
    pub api_errors_total: u64,
    pub actor_commands_received_total: u64,
    pub actor_commands_accepted_total: u64,
    pub actor_commands_rejected_total: u64,
    pub actor_journal_append_failures_total: u64,
    pub actor_apply_after_append_failures_total: u64,
}

impl MetricsResponse {
    fn to_prometheus_text(self) -> String {
        let mut output = String::new();
        write_counter(
            &mut output,
            "exchange_http_requests_total",
            "Total HTTP requests handled by the exchange service.",
            self.http_requests_total,
        );
        write_counter(
            &mut output,
            "exchange_ready_checks_total",
            "Total readiness checks handled by the exchange service.",
            self.ready_checks_total,
        );
        write_counter(
            &mut output,
            "exchange_snapshot_requests_total",
            "Total market snapshot requests handled by the exchange service.",
            self.snapshot_requests_total,
        );
        write_counter(
            &mut output,
            "exchange_deposits_accepted_total",
            "Total deposit commands accepted by the exchange service.",
            self.deposits_accepted_total,
        );
        write_counter(
            &mut output,
            "exchange_orders_accepted_total",
            "Total order commands accepted by the exchange service.",
            self.orders_accepted_total,
        );
        write_counter(
            &mut output,
            "exchange_settlement_requests_queued_total",
            "Total settlement requests queued by the exchange service.",
            self.settlement_requests_queued_total,
        );
        write_counter(
            &mut output,
            "exchange_settlement_requests_submitted_total",
            "Total settlement requests submitted by the relayer worker.",
            self.settlement_requests_submitted_total,
        );
        write_counter(
            &mut output,
            "exchange_settlement_requests_failed_total",
            "Total settlement requests failed by the relayer worker.",
            self.settlement_requests_failed_total,
        );
        write_gauge(
            &mut output,
            "exchange_settlement_requests_pending",
            "Settlement requests currently waiting in memory.",
            self.settlement_requests_pending,
        );
        write_counter(
            &mut output,
            "exchange_api_errors_total",
            "Total API errors returned by the exchange service.",
            self.api_errors_total,
        );
        write_counter(
            &mut output,
            "exchange_actor_commands_received_total",
            "Total commands received by the market actor.",
            self.actor_commands_received_total,
        );
        write_counter(
            &mut output,
            "exchange_actor_commands_accepted_total",
            "Total commands accepted by the market actor.",
            self.actor_commands_accepted_total,
        );
        write_counter(
            &mut output,
            "exchange_actor_commands_rejected_total",
            "Total commands rejected by the market actor.",
            self.actor_commands_rejected_total,
        );
        write_counter(
            &mut output,
            "exchange_actor_journal_append_failures_total",
            "Total actor journal append failures.",
            self.actor_journal_append_failures_total,
        );
        write_counter(
            &mut output,
            "exchange_actor_apply_after_append_failures_total",
            "Total actor apply-after-append failures.",
            self.actor_apply_after_append_failures_total,
        );
        output
    }
}

fn write_counter(output: &mut String, name: &str, help: &str, value: u64) {
    write_metric(output, name, help, "counter", value);
}

fn write_gauge(output: &mut String, name: &str, help: &str, value: u64) {
    write_metric(output, name, help, "gauge", value);
}

fn write_metric(output: &mut String, name: &str, help: &str, kind: &str, value: u64) {
    output.push_str("# HELP ");
    output.push_str(name);
    output.push(' ');
    output.push_str(help);
    output.push('\n');
    output.push_str("# TYPE ");
    output.push_str(name);
    output.push(' ');
    output.push_str(kind);
    output.push('\n');
    output.push_str(name);
    output.push(' ');
    output.push_str(&value.to_string());
    output.push('\n');
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    Unauthorized,
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
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_owned()),
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

    async fn response_text(response: axum::response::Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn signed_order_body(
        order_id: u128,
        trader_id: u64,
        order_hash: [u8; 32],
        trader_pubkey: [u8; 32],
        market_config: Pubkey,
        side: &str,
        price: u64,
        quantity: u64,
    ) -> Value {
        json!({
            "order_id": order_id,
            "trader_id": trader_id,
            "order_hash": Vec::from(order_hash),
            "trader_pubkey": Vec::from(trader_pubkey),
            "market_config": Vec::from(market_config.to_bytes()),
            "side": side,
            "price": price,
            "quantity": quantity,
            "nonce": order_id,
            "expiry_slot": u64::MAX,
            "signature": vec![trader_id as u8; 64]
        })
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
            BootConfig::from_values(None, None, None).unwrap(),
            BootConfig {
                mode: BootMode::Local,
                database_url: None,
                api_key: None,
                relayer: RelayerBootConfig::Disabled,
            }
        );
    }

    #[test]
    fn boot_config_allows_explicit_local_without_database() {
        assert_eq!(
            BootConfig::from_values(Some("local"), None, Some("secret")).unwrap(),
            BootConfig {
                mode: BootMode::Local,
                database_url: None,
                api_key: Some("secret".to_owned()),
                relayer: RelayerBootConfig::Disabled,
            }
        );
    }

    #[test]
    fn boot_config_requires_database_url_for_postgres() {
        assert!(matches!(
            BootConfig::from_values(Some("postgres"), None, None),
            Err(StartupError::Config(_))
        ));

        assert_eq!(
            BootConfig::from_values(
                Some("postgres"),
                Some("postgres://localhost/exchange"),
                None
            )
            .unwrap(),
            BootConfig {
                mode: BootMode::Postgres,
                database_url: Some("postgres://localhost/exchange".to_owned()),
                api_key: None,
                relayer: RelayerBootConfig::Disabled,
            }
        );
    }

    #[test]
    fn boot_config_rejects_unknown_mode() {
        assert!(matches!(
            BootConfig::from_values(Some("memory"), None, None),
            Err(StartupError::Config(_))
        ));
    }

    #[test]
    fn http_addr_defaults_to_localhost_and_parses_override() {
        assert_eq!(
            http_addr_from_value(None).unwrap(),
            "127.0.0.1:3000".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            http_addr_from_value(Some("0.0.0.0:3000")).unwrap(),
            "0.0.0.0:3000".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn http_addr_rejects_invalid_override() {
        assert!(matches!(
            http_addr_from_value(Some("not-a-socket")),
            Err(StartupError::Config(_))
        ));
    }

    #[tokio::test]
    async fn app_from_local_config_reports_ready() {
        let service = app_from_config(BootConfig {
            mode: BootMode::Local,
            database_url: None,
            api_key: None,
            relayer: RelayerBootConfig::Disabled,
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
    async fn protected_deposit_rejects_missing_api_key() {
        let service = app_from_config(BootConfig {
            mode: BootMode::Local,
            database_url: None,
            api_key: Some("secret".to_owned()),
            relayer: RelayerBootConfig::Disabled,
        })
        .await
        .unwrap();

        let response = service
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

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response_json(response).await,
            json!({"error": "unauthorized"})
        );
    }

    #[tokio::test]
    async fn protected_deposit_accepts_matching_api_key() {
        let service = app_from_config(BootConfig {
            mode: BootMode::Local,
            database_url: None,
            api_key: Some("secret".to_owned()),
            relayer: RelayerBootConfig::Disabled,
        })
        .await
        .unwrap();

        let response = service
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/deposits")
                    .header("content-type", "application/json")
                    .header(X_API_KEY_HEADER, "secret")
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

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response_json(response).await, json!({"accepted": true}));
    }

    #[test]
    fn settlement_edge_allocates_contiguous_ids_and_stores_signed_orders() {
        let settlement = SettlementEdgeState::default();
        let order_id = OrderId::new(1).unwrap();
        let signed_order = relayer::SettlementSignedOrder {
            trader_id: TraderId::new(2).unwrap(),
            order_hash: [3; 32],
            order: spot_settlement::SignedOrderPayload {
                order_id: 1,
                market_config: Pubkey::new_from_array([4; 32]),
                trader: Pubkey::new_from_array([5; 32]),
                side: spot_settlement::SignedOrderSide::Bid,
                price: 100,
                quantity: 7,
                nonce: 9,
                expiry_slot: 10,
            },
            signature: [6; 64],
        };

        assert!(!settlement.store_signed_order(order_id, signed_order));
        assert_eq!(
            relayer::SettlementSignedOrderSource::signed_order(&settlement, order_id),
            Some(signed_order)
        );
        assert_eq!(settlement.allocate_settlement_ids(0).unwrap(), None);
        assert_eq!(settlement.allocate_settlement_ids(2).unwrap(), Some(0));
        assert_eq!(settlement.allocate_settlement_ids(3).unwrap(), Some(2));

        let request = relayer::SignedSettlementRequest {
            settlement_authority: Pubkey::new_from_array([8; 32]),
            base_mint: Pubkey::new_from_array([9; 32]),
            quote_mint: Pubkey::new_from_array([10; 32]),
            buyer: Pubkey::new_from_array([11; 32]),
            seller: Pubkey::new_from_array([12; 32]),
            payer: Pubkey::new_from_array([13; 32]),
            args: spot_settlement::SignedFillArgs {
                settlement_id: 1,
                fill_price: 100,
                fill_quantity: 7,
                buyer_order_hash: [1; 32],
                seller_order_hash: [2; 32],
                buyer_order: signed_order.order,
                buyer_signature: [1; 64],
                seller_order: signed_order.order,
                seller_signature: [2; 64],
            },
        };

        settlement.queue_settlement_requests(vec![request]);
        assert_eq!(settlement.queued_settlement_count(), 1);
        assert_eq!(settlement.drain_queued_settlement_requests(), vec![request]);
        assert_eq!(settlement.queued_settlement_count(), 0);
    }

    #[tokio::test]
    async fn signed_order_registration_accepts_and_replaces_order() {
        let service = app();
        let body = json!({
            "order_id": 1,
            "trader_id": 2,
            "order_hash": vec![3; 32],
            "trader_pubkey": vec![4; 32],
            "market_config": vec![5; 32],
            "side": "bid",
            "price": 100,
            "quantity": 7,
            "nonce": 9,
            "expiry_slot": 10,
            "signature": vec![6; 64]
        });

        let first = service
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/signed-orders")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(
            response_json(first).await,
            json!({"accepted": true, "replaced": false})
        );

        let second = service
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/signed-orders")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::OK);
        assert_eq!(
            response_json(second).await,
            json!({"accepted": true, "replaced": true})
        );
    }

    #[tokio::test]
    async fn signed_order_registration_rejects_wrong_byte_lengths() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/signed-orders")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "order_id": 1,
                            "trader_id": 2,
                            "order_hash": vec![3; 31],
                            "trader_pubkey": vec![4; 32],
                            "market_config": vec![5; 32],
                            "side": "bid",
                            "price": 100,
                            "quantity": 7,
                            "nonce": 9,
                            "expiry_slot": 10,
                            "signature": vec![6; 64]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response_json(response).await,
            json!({"error": "order_hash must contain 32 bytes"})
        );
    }

    #[tokio::test]
    async fn crossing_order_with_signed_orders_queues_settlement() {
        let market = default_market();
        let actor = MarketActorHandle::spawn(market, 1024);
        let base_mint = Pubkey::new_from_array([11; 32]);
        let quote_mint = Pubkey::new_from_array([12; 32]);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let service = app_with_actor_api_key_and_settlement(
            MARKET_SOL_USDC,
            ReadyBootMode::Local,
            JournalMode::Noop,
            market,
            actor,
            None,
            Some(SettlementBridgeConfig {
                market_id: MarketId::new(1).unwrap(),
                settlement_authority: Pubkey::new_from_array([8; 32]),
                base_mint,
                quote_mint,
                payer: Pubkey::new_from_array([9; 32]),
            }),
        );

        for body in [
            json!({ "command_id": 1, "trader_id": 1, "asset_id": 1, "amount": 7 }),
            json!({ "command_id": 2, "trader_id": 2, "asset_id": 2, "amount": 735 }),
            signed_order_body(1, 1, [1; 32], [21; 32], market_config, "ask", 100, 7),
            signed_order_body(2, 2, [2; 32], [22; 32], market_config, "bid", 105, 7),
        ] {
            let uri = if body.get("asset_id").is_some() {
                "/deposits"
            } else {
                "/signed-orders"
            };
            let response = service
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(uri)
                        .header("content-type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        for body in [
            json!({
                "command_id": 3,
                "order_id": 1,
                "trader_id": 1,
                "market_id": 1,
                "side": "ask",
                "price": 100,
                "quantity": 7,
                "sequence": 1
            }),
            json!({
                "command_id": 4,
                "order_id": 2,
                "trader_id": 2,
                "market_id": 1,
                "side": "bid",
                "price": 105,
                "quantity": 7,
                "sequence": 2
            }),
        ] {
            let response = service
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/orders")
                        .header("content-type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let pending = service
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/settlements/pending")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(pending.status(), StatusCode::OK);
        assert_eq!(response_json(pending).await, json!({ "queued": 1 }));
    }

    #[tokio::test]
    async fn settlement_outbox_view_is_protected_and_empty_without_postgres() {
        let market = default_market();
        let actor = MarketActorHandle::spawn(market, 1024);
        let service = app_with_actor_and_api_key(
            MARKET_SOL_USDC,
            ReadyBootMode::Local,
            JournalMode::Noop,
            market,
            actor,
            Some("secret".to_owned()),
        );

        let missing_key = service
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/settlements/outbox")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_key.status(), StatusCode::UNAUTHORIZED);

        let accepted = service
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/settlements/outbox")
                    .header(X_API_KEY_HEADER, "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::OK);
        assert_eq!(response_json(accepted).await, json!([]));
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
                    .uri("/metrics.json")
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
                "settlement_requests_queued_total": 0,
                "settlement_requests_submitted_total": 0,
                "settlement_requests_failed_total": 0,
                "settlement_requests_pending": 0,
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
                    .uri("/metrics.json")
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
                "settlement_requests_queued_total": 0,
                "settlement_requests_submitted_total": 0,
                "settlement_requests_failed_total": 0,
                "settlement_requests_pending": 0,
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
    async fn metrics_endpoint_returns_prometheus_text() {
        let service = app();

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
            metrics.headers().get("content-type").unwrap(),
            "text/plain; version=0.0.4; charset=utf-8"
        );
        let body = response_text(metrics).await;
        assert!(body.contains("# TYPE exchange_http_requests_total counter\n"));
        assert!(body.contains("exchange_http_requests_total 2\n"));
        assert!(body.contains("exchange_deposits_accepted_total 1\n"));
        assert!(body.contains("exchange_actor_commands_received_total 1\n"));
        assert!(body.contains("exchange_actor_commands_accepted_total 1\n"));
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

    #[sqlx::test(migrations = "../../crates/persistence/migrations")]
    #[ignore = "requires DATABASE_URL"]
    async fn app_with_postgres_replays_events_after_restart(pool: sqlx::PgPool) {
        let first_service = app_with_postgres_journal(
            PostgresEventJournal::from_pool(pool.clone()),
            None,
            RelayerBootConfig::Disabled,
        )
        .await
        .unwrap();

        let deposit = first_service
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

        let second_service = app_with_postgres_journal(
            PostgresEventJournal::from_pool(pool.clone()),
            None,
            RelayerBootConfig::Disabled,
        )
        .await
        .unwrap();

        let ready = second_service
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
                "boot_mode": "postgres",
                "journal_mode": "postgres",
                "event_count": 1
            })
        );

        let duplicate = second_service
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
}
