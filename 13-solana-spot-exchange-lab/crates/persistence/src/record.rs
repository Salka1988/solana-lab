use application::{CommandId, Event};
use domain::{
    AssetId, BalanceAmount, Fill, MarketId, Order, OrderId, OrderSequence, Price, Quantity, Side,
    TraderId,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistenceError {
    Application(application::Error),
    Domain(domain::Error),
    Serde(String),
    Sql(String),
    UnknownEventType(String),
}

impl From<application::Error> for PersistenceError {
    fn from(value: application::Error) -> Self {
        Self::Application(value)
    }
}

impl From<domain::Error> for PersistenceError {
    fn from(value: domain::Error) -> Self {
        Self::Domain(value)
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value.to_string())
    }
}

impl From<sqlx::Error> for PersistenceError {
    fn from(value: sqlx::Error) -> Self {
        Self::Sql(value.to_string())
    }
}

impl From<sqlx::migrate::MigrateError> for PersistenceError {
    fn from(value: sqlx::migrate::MigrateError) -> Self {
        Self::Sql(value.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub command_id: CommandId,
    pub event_type: String,
    pub payload: serde_json::Value,
}

impl EventRecord {
    pub fn into_event(self) -> Result<Event, PersistenceError> {
        Event::try_from(self)
    }
}

impl TryFrom<&Event> for EventRecord {
    type Error = PersistenceError;

    fn try_from(event: &Event) -> Result<Self, Self::Error> {
        match event {
            Event::DepositCredited {
                command_id,
                trader_id,
                asset_id,
                amount,
            } => Ok(Self {
                command_id: *command_id,
                event_type: "deposit_credited".to_owned(),
                payload: serde_json::to_value(DepositCreditedPayload {
                    trader_id: trader_id.get(),
                    asset_id: asset_id.get(),
                    amount: amount.get(),
                })?,
            }),
            Event::OrderPlaced {
                command_id,
                order,
                fills,
            } => Ok(Self {
                command_id: *command_id,
                event_type: "order_placed".to_owned(),
                payload: serde_json::to_value(OrderPlacedPayload {
                    order: OrderPayload::from_order(*order),
                    fills: fills.iter().copied().map(FillPayload::from_fill).collect(),
                })?,
            }),
        }
    }
}

impl TryFrom<EventRecord> for Event {
    type Error = PersistenceError;

    fn try_from(record: EventRecord) -> Result<Self, Self::Error> {
        let EventRecord {
            command_id,
            event_type,
            payload,
        } = record;

        match event_type.as_str() {
            "deposit_credited" => {
                let payload: DepositCreditedPayload = serde_json::from_value(payload)?;
                Ok(Self::DepositCredited {
                    command_id,
                    trader_id: TraderId::new(payload.trader_id)?,
                    asset_id: AssetId::new(payload.asset_id)?,
                    amount: BalanceAmount::new(payload.amount),
                })
            }
            "order_placed" => {
                let payload: OrderPlacedPayload = serde_json::from_value(payload)?;
                Ok(Self::OrderPlaced {
                    command_id,
                    order: payload.order.into_order()?,
                    fills: payload
                        .fills
                        .into_iter()
                        .map(FillPayload::into_fill)
                        .collect::<Result<Vec<_>, _>>()?,
                })
            }
            _ => Err(PersistenceError::UnknownEventType(event_type)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct DepositCreditedPayload {
    trader_id: u64,
    asset_id: u32,
    amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OrderPlacedPayload {
    order: OrderPayload,
    fills: Vec<FillPayload>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct OrderPayload {
    order_id: u128,
    trader_id: u64,
    market_id: u32,
    side: SidePayload,
    price: u64,
    original_quantity: u64,
    sequence: u64,
}

impl OrderPayload {
    fn from_order(order: Order) -> Self {
        Self {
            order_id: order.id().get(),
            trader_id: order.trader_id().get(),
            market_id: order.market_id().get(),
            side: SidePayload::from_side(order.side()),
            price: order.price().get(),
            original_quantity: order.original_quantity().get(),
            sequence: order.sequence().get(),
        }
    }

    fn into_order(self) -> Result<Order, domain::Error> {
        Ok(Order::new(
            OrderId::new(self.order_id)?,
            TraderId::new(self.trader_id)?,
            MarketId::new(self.market_id)?,
            self.side.into_side(),
            Price::new(self.price)?,
            Quantity::new(self.original_quantity)?,
            OrderSequence::new(self.sequence)?,
        ))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SidePayload {
    Bid,
    Ask,
}

impl SidePayload {
    const fn from_side(side: Side) -> Self {
        match side {
            Side::Bid => Self::Bid,
            Side::Ask => Self::Ask,
        }
    }

    const fn into_side(self) -> Side {
        match self {
            Self::Bid => Side::Bid,
            Self::Ask => Side::Ask,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct FillPayload {
    maker_order_id: u128,
    taker_order_id: u128,
    maker_trader_id: u64,
    taker_trader_id: u64,
    price: u64,
    quantity: u64,
}

impl FillPayload {
    fn from_fill(fill: Fill) -> Self {
        Self {
            maker_order_id: fill.maker_order_id().get(),
            taker_order_id: fill.taker_order_id().get(),
            maker_trader_id: fill.maker_trader_id().get(),
            taker_trader_id: fill.taker_trader_id().get(),
            price: fill.price().get(),
            quantity: fill.quantity().get(),
        }
    }

    fn into_fill(self) -> Result<Fill, domain::Error> {
        Ok(Fill::from_parts(
            OrderId::new(self.maker_order_id)?,
            OrderId::new(self.taker_order_id)?,
            TraderId::new(self.maker_trader_id)?,
            TraderId::new(self.taker_trader_id)?,
            Price::new(self.price)?,
            Quantity::new(self.quantity)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::OrderSequence;

    fn command(id: u128) -> CommandId {
        CommandId::new(id).unwrap()
    }

    fn order(id: u128, trader_id: u64, side: Side) -> Order {
        Order::new(
            OrderId::new(id).unwrap(),
            TraderId::new(trader_id).unwrap(),
            MarketId::new(1).unwrap(),
            side,
            Price::new(100).unwrap(),
            Quantity::new(7).unwrap(),
            OrderSequence::new(id.try_into().unwrap()).unwrap(),
        )
    }

    #[test]
    fn deposit_event_round_trips_through_record() {
        let event = Event::DepositCredited {
            command_id: command(1),
            trader_id: TraderId::new(2).unwrap(),
            asset_id: AssetId::new(3).unwrap(),
            amount: BalanceAmount::new(4),
        };

        let decoded = EventRecord::try_from(&event).unwrap().into_event().unwrap();

        assert_eq!(decoded, event);
    }

    #[test]
    fn order_event_round_trips_through_record() {
        let maker = order(1, 10, Side::Ask);
        let taker = order(2, 20, Side::Bid);
        let event = Event::OrderPlaced {
            command_id: command(3),
            order: taker,
            fills: vec![Fill::new(
                maker,
                taker,
                Price::new(100).unwrap(),
                Quantity::new(7).unwrap(),
            )],
        };

        let decoded = EventRecord::try_from(&event).unwrap().into_event().unwrap();

        assert_eq!(decoded, event);
    }
}
