#![forbid(unsafe_code)]

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
use runtime::{MarketActorHandle, MarketReply, MarketSnapshot};
use serde::{Deserialize, Serialize};

const MARKET_SOL_USDC: &str = "SOL-USDC";

#[derive(Clone)]
pub struct ServiceState {
    market_id: String,
    market: MarketSpec,
    actor: MarketActorHandle,
}

pub fn app() -> Router {
    let market = default_market();
    let actor = MarketActorHandle::spawn(market, 1024);
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/deposits", post(credit_deposit))
        .route("/orders", post(place_order))
        .route("/markets/{market_id}/snapshot", get(snapshot))
        .with_state(ServiceState {
            market_id: MARKET_SOL_USDC.to_owned(),
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

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn ready(State(state): State<ServiceState>) -> Result<Json<ReadyResponse>, ApiError> {
    let snapshot = actor_snapshot(&state.actor).await?;
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
        MarketReply::DepositCredited => Ok(Json(DepositResponse { accepted: true })),
        _ => Err(ApiError::Internal("unexpected deposit reply")),
    }
}

async fn place_order(
    State(state): State<ServiceState>,
    Json(request): Json<OrderRequest>,
) -> Result<Json<OrderResponse>, ApiError> {
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
        MarketReply::OrderPlaced { fills } => Ok(Json(OrderResponse {
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
        })),
        _ => Err(ApiError::Internal("unexpected order reply")),
    }
}

async fn snapshot(
    State(state): State<ServiceState>,
    Path(market_id): Path<String>,
) -> Result<Json<SnapshotResponse>, ApiError> {
    if market_id != state.market_id {
        return Err(ApiError::NotFound);
    }

    let snapshot = actor_snapshot(&state.actor).await?;
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
}
