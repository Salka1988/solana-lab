use crate::{
    newtype::{non_zero_newtype, zeroable_u64_newtype},
    Error, MarketId, OrderId, Price, Quantity, TraderId,
};
use core::num::NonZeroU64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Bid,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
}

non_zero_newtype!(OrderSequence, u64, NonZeroU64);
zeroable_u64_newtype!(RemainingQuantity);

impl From<Quantity> for RemainingQuantity {
    fn from(quantity: Quantity) -> Self {
        Self(quantity.get())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order {
    id: OrderId,
    trader_id: TraderId,
    market_id: MarketId,
    side: Side,
    price: Price,
    original_quantity: Quantity,
    remaining_quantity: RemainingQuantity,
    sequence: OrderSequence,
    status: OrderStatus,
}

impl Order {
    pub fn new(
        id: OrderId,
        trader_id: TraderId,
        market_id: MarketId,
        side: Side,
        price: Price,
        quantity: Quantity,
        sequence: OrderSequence,
    ) -> Self {
        Self {
            id,
            trader_id,
            market_id,
            side,
            price,
            original_quantity: quantity,
            remaining_quantity: quantity.into(),
            sequence,
            status: OrderStatus::Open,
        }
    }

    pub const fn id(&self) -> OrderId {
        self.id
    }

    pub const fn trader_id(&self) -> TraderId {
        self.trader_id
    }

    pub const fn market_id(&self) -> MarketId {
        self.market_id
    }

    pub const fn side(&self) -> Side {
        self.side
    }

    pub const fn price(&self) -> Price {
        self.price
    }

    pub const fn original_quantity(&self) -> Quantity {
        self.original_quantity
    }

    pub const fn remaining_quantity(&self) -> RemainingQuantity {
        self.remaining_quantity
    }

    pub const fn sequence(&self) -> OrderSequence {
        self.sequence
    }

    pub const fn status(&self) -> OrderStatus {
        self.status
    }

    pub const fn is_terminal(&self) -> bool {
        matches!(self.status, OrderStatus::Filled | OrderStatus::Cancelled)
    }

    pub fn apply_fill(&mut self, fill_quantity: Quantity) -> Result<(), Error> {
        if self.is_terminal() {
            return Err(Error::OrderAlreadyTerminal);
        }

        let fill = fill_quantity.get();
        let remaining = self.remaining_quantity.get();

        if fill > remaining {
            return Err(Error::FillExceedsRemainingQuantity);
        }

        let updated_remaining = remaining - fill;
        self.remaining_quantity = RemainingQuantity::new(updated_remaining);
        self.status = if updated_remaining == 0 {
            OrderStatus::Filled
        } else {
            OrderStatus::PartiallyFilled
        };

        Ok(())
    }

    pub fn cancel(&mut self) -> Result<(), Error> {
        if self.is_terminal() {
            return Err(Error::OrderAlreadyTerminal);
        }

        self.status = OrderStatus::Cancelled;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(quantity: u64) -> Order {
        Order::new(
            OrderId::new(1).unwrap(),
            TraderId::new(2).unwrap(),
            MarketId::new(3).unwrap(),
            Side::Bid,
            Price::new(100).unwrap(),
            Quantity::new(quantity).unwrap(),
            OrderSequence::new(4).unwrap(),
        )
    }

    #[test]
    fn new_order_starts_open_with_full_remaining_quantity() {
        let order = order(10);

        assert_eq!(order.status(), OrderStatus::Open);
        assert_eq!(order.original_quantity().get(), 10);
        assert_eq!(order.remaining_quantity().get(), 10);
        assert!(!order.is_terminal());
    }

    #[test]
    fn partial_fill_reduces_remaining_quantity() {
        let mut order = order(10);

        order.apply_fill(Quantity::new(4).unwrap()).unwrap();

        assert_eq!(order.status(), OrderStatus::PartiallyFilled);
        assert_eq!(order.remaining_quantity().get(), 6);
        assert!(!order.is_terminal());
    }

    #[test]
    fn full_fill_sets_remaining_quantity_to_zero_and_terminal_status() {
        let mut order = order(10);

        order.apply_fill(Quantity::new(10).unwrap()).unwrap();

        assert_eq!(order.status(), OrderStatus::Filled);
        assert_eq!(order.remaining_quantity(), RemainingQuantity::ZERO);
        assert!(order.is_terminal());
    }

    #[test]
    fn overfill_is_rejected() {
        let mut order = order(10);

        assert_eq!(
            order.apply_fill(Quantity::new(11).unwrap()),
            Err(Error::FillExceedsRemainingQuantity)
        );
        assert_eq!(order.status(), OrderStatus::Open);
        assert_eq!(order.remaining_quantity().get(), 10);
    }

    #[test]
    fn cancel_open_order_sets_terminal_status() {
        let mut order = order(10);

        order.cancel().unwrap();

        assert_eq!(order.status(), OrderStatus::Cancelled);
        assert_eq!(order.remaining_quantity().get(), 10);
        assert!(order.is_terminal());
    }

    #[test]
    fn cancel_partially_filled_order_keeps_remaining_quantity_for_release() {
        let mut order = order(10);

        order.apply_fill(Quantity::new(4).unwrap()).unwrap();
        order.cancel().unwrap();

        assert_eq!(order.status(), OrderStatus::Cancelled);
        assert_eq!(order.remaining_quantity().get(), 6);
        assert!(order.is_terminal());
    }

    #[test]
    fn cancelled_order_rejects_fill() {
        let mut order = order(10);

        order.cancel().unwrap();

        assert_eq!(
            order.apply_fill(Quantity::new(1).unwrap()),
            Err(Error::OrderAlreadyTerminal)
        );
    }

    #[test]
    fn filled_order_rejects_cancel() {
        let mut order = order(10);

        order.apply_fill(Quantity::new(10).unwrap()).unwrap();

        assert_eq!(order.cancel(), Err(Error::OrderAlreadyTerminal));
    }

    #[test]
    fn sequence_rejects_zero_and_orders_by_value() {
        let early = OrderSequence::new(1).unwrap();
        let late = OrderSequence::new(2).unwrap();

        assert_eq!(OrderSequence::new(0), Err(Error::ZeroValue));
        assert!(early < late);
    }
}
