use domain::{Fill, MarketId, Order, OrderId, Price, Quantity, Side, TraderId};

use crate::CommandId;

/// Application-owned settlement request.
///
/// Keeping this type here preserves the hexagonal boundary: the application
/// says what must settle, while Solana-specific crates decide how to settle it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementBatch {
    command_id: CommandId,
    intents: Vec<SettlementIntent>,
}

impl SettlementBatch {
    pub fn from_order_fills(command_id: CommandId, taker_order: Order, fills: &[Fill]) -> Self {
        Self {
            command_id,
            intents: fills
                .iter()
                .copied()
                .map(|fill| SettlementIntent::from_fill(taker_order, fill))
                .collect(),
        }
    }

    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    pub fn intents(&self) -> &[SettlementIntent] {
        &self.intents
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementIntent {
    market_id: MarketId,
    buyer_trader_id: TraderId,
    seller_trader_id: TraderId,
    maker_order_id: OrderId,
    taker_order_id: OrderId,
    price: Price,
    quantity: Quantity,
}

impl SettlementIntent {
    pub fn from_fill(taker_order: Order, fill: Fill) -> Self {
        let (buyer_trader_id, seller_trader_id) = match taker_order.side() {
            Side::Bid => (fill.taker_trader_id(), fill.maker_trader_id()),
            Side::Ask => (fill.maker_trader_id(), fill.taker_trader_id()),
        };

        Self {
            market_id: taker_order.market_id(),
            buyer_trader_id,
            seller_trader_id,
            maker_order_id: fill.maker_order_id(),
            taker_order_id: fill.taker_order_id(),
            price: fill.price(),
            quantity: fill.quantity(),
        }
    }

    pub const fn market_id(self) -> MarketId {
        self.market_id
    }

    pub const fn buyer_trader_id(self) -> TraderId {
        self.buyer_trader_id
    }

    pub const fn seller_trader_id(self) -> TraderId {
        self.seller_trader_id
    }

    pub const fn maker_order_id(self) -> OrderId {
        self.maker_order_id
    }

    pub const fn taker_order_id(self) -> OrderId {
        self.taker_order_id
    }

    pub const fn price(self) -> Price {
        self.price
    }

    pub const fn quantity(self) -> Quantity {
        self.quantity
    }
}

/// Outbound application port for settlement side effects.
///
/// Adapters implement this at the composition edge. The application does not
/// depend on relayer, Solana clients, RPC, blockhashes, or transaction signing.
pub trait SettlementPort {
    type Error;

    fn submit_settlements(
        &mut self,
        batch: SettlementBatch,
    ) -> std::result::Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{MarketId, OrderSequence};

    fn command(id: u128) -> CommandId {
        CommandId::new(id).unwrap()
    }

    fn trader(id: u64) -> TraderId {
        TraderId::new(id).unwrap()
    }

    fn order(id: u128, trader_id: u64, side: Side) -> Order {
        Order::new(
            OrderId::new(id).unwrap(),
            trader(trader_id),
            MarketId::new(7).unwrap(),
            side,
            Price::new(100).unwrap(),
            Quantity::new(5).unwrap(),
            OrderSequence::new(id.try_into().unwrap()).unwrap(),
        )
    }

    fn fill() -> Fill {
        Fill::from_parts(
            OrderId::new(1).unwrap(),
            OrderId::new(2).unwrap(),
            trader(10),
            trader(20),
            Price::new(99).unwrap(),
            Quantity::new(3).unwrap(),
        )
    }

    #[test]
    fn bid_taker_buys_from_maker() {
        let intent = SettlementIntent::from_fill(order(2, 20, Side::Bid), fill());

        assert_eq!(intent.buyer_trader_id(), trader(20));
        assert_eq!(intent.seller_trader_id(), trader(10));
        assert_eq!(intent.market_id(), MarketId::new(7).unwrap());
    }

    #[test]
    fn ask_taker_sells_to_maker() {
        let intent = SettlementIntent::from_fill(order(2, 20, Side::Ask), fill());

        assert_eq!(intent.buyer_trader_id(), trader(10));
        assert_eq!(intent.seller_trader_id(), trader(20));
    }

    #[test]
    fn batch_keeps_command_and_maps_fills() {
        let batch =
            SettlementBatch::from_order_fills(command(9), order(2, 20, Side::Bid), &[fill()]);

        assert_eq!(batch.command_id(), command(9));
        assert_eq!(batch.intents().len(), 1);
        assert_eq!(
            batch.intents()[0].maker_order_id(),
            OrderId::new(1).unwrap()
        );
        assert_eq!(
            batch.intents()[0].taker_order_id(),
            OrderId::new(2).unwrap()
        );
    }
}
