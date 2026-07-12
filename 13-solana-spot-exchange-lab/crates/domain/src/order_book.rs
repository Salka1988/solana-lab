use crate::{Error, Order, OrderId, Price, Side};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBookSide {
    side: Side,
    levels: BTreeMap<Price, VecDeque<Order>>,
}

impl OrderBookSide {
    pub fn new(side: Side) -> Self {
        Self {
            side,
            levels: BTreeMap::new(),
        }
    }

    pub const fn side(&self) -> Side {
        self.side
    }

    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    pub fn price_level_count(&self) -> usize {
        self.levels.len()
    }

    pub fn insert(&mut self, order: Order) -> Result<(), Error> {
        if order.side() != self.side {
            return Err(Error::WrongOrderSide);
        }

        self.levels
            .entry(order.price())
            .or_default()
            .push_back(order);

        Ok(())
    }

    pub fn best_order(&self) -> Option<&Order> {
        self.best_level().and_then(|(_, orders)| orders.front())
    }

    pub fn pop_best_order(&mut self) -> Option<Order> {
        let price = self.best_price()?;
        let level = self.levels.get_mut(&price)?;
        let order = level.pop_front();

        if level.is_empty() {
            self.levels.remove(&price);
        }

        order
    }

    pub fn cancel(&mut self, order_id: OrderId) -> Result<Order, Error> {
        let mut empty_price = None;

        for (price, orders) in &mut self.levels {
            if let Some(index) = orders.iter().position(|order| order.id() == order_id) {
                let mut order = orders.remove(index).ok_or(Error::OrderNotFound)?;
                order.cancel()?;

                if orders.is_empty() {
                    empty_price = Some(*price);
                }

                if let Some(price) = empty_price {
                    self.levels.remove(&price);
                }

                return Ok(order);
            }
        }

        Err(Error::OrderNotFound)
    }

    fn best_price(&self) -> Option<Price> {
        match self.side {
            Side::Bid => self.levels.last_key_value().map(|(price, _)| *price),
            Side::Ask => self.levels.first_key_value().map(|(price, _)| *price),
        }
    }

    fn best_level(&self) -> Option<(&Price, &VecDeque<Order>)> {
        match self.side {
            Side::Bid => self.levels.last_key_value(),
            Side::Ask => self.levels.first_key_value(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MarketId, OrderSequence, Price, Quantity, TraderId};

    fn order(id: u128, side: Side, price: u64, sequence: u64) -> Order {
        Order::new(
            OrderId::new(id).unwrap(),
            TraderId::new(1).unwrap(),
            MarketId::new(1).unwrap(),
            side,
            Price::new(price).unwrap(),
            Quantity::new(10).unwrap(),
            OrderSequence::new(sequence).unwrap(),
        )
    }

    #[test]
    fn bid_book_best_order_is_highest_price() {
        let mut book = OrderBookSide::new(Side::Bid);

        book.insert(order(1, Side::Bid, 100, 1)).unwrap();
        book.insert(order(2, Side::Bid, 105, 2)).unwrap();
        book.insert(order(3, Side::Bid, 101, 3)).unwrap();

        assert_eq!(book.best_order().unwrap().id(), OrderId::new(2).unwrap());
    }

    #[test]
    fn ask_book_best_order_is_lowest_price() {
        let mut book = OrderBookSide::new(Side::Ask);

        book.insert(order(1, Side::Ask, 100, 1)).unwrap();
        book.insert(order(2, Side::Ask, 95, 2)).unwrap();
        book.insert(order(3, Side::Ask, 101, 3)).unwrap();

        assert_eq!(book.best_order().unwrap().id(), OrderId::new(2).unwrap());
    }

    #[test]
    fn same_price_preserves_fifo() {
        let mut book = OrderBookSide::new(Side::Bid);

        book.insert(order(1, Side::Bid, 100, 1)).unwrap();
        book.insert(order(2, Side::Bid, 100, 2)).unwrap();
        book.insert(order(3, Side::Bid, 100, 3)).unwrap();

        assert_eq!(
            book.pop_best_order().unwrap().id(),
            OrderId::new(1).unwrap()
        );
        assert_eq!(
            book.pop_best_order().unwrap().id(),
            OrderId::new(2).unwrap()
        );
        assert_eq!(
            book.pop_best_order().unwrap().id(),
            OrderId::new(3).unwrap()
        );
    }

    #[test]
    fn wrong_side_insert_is_rejected() {
        let mut book = OrderBookSide::new(Side::Bid);

        assert_eq!(
            book.insert(order(1, Side::Ask, 100, 1)),
            Err(Error::WrongOrderSide)
        );
        assert!(book.is_empty());
    }

    #[test]
    fn cancel_front_middle_and_back_orders() {
        let mut book = OrderBookSide::new(Side::Bid);

        book.insert(order(1, Side::Bid, 100, 1)).unwrap();
        book.insert(order(2, Side::Bid, 100, 2)).unwrap();
        book.insert(order(3, Side::Bid, 100, 3)).unwrap();

        assert_eq!(
            book.cancel(OrderId::new(1).unwrap()).unwrap().id(),
            OrderId::new(1).unwrap()
        );
        assert_eq!(
            book.cancel(OrderId::new(3).unwrap()).unwrap().id(),
            OrderId::new(3).unwrap()
        );
        assert_eq!(
            book.cancel(OrderId::new(2).unwrap()).unwrap().id(),
            OrderId::new(2).unwrap()
        );
        assert!(book.is_empty());
    }

    #[test]
    fn cancelling_missing_order_returns_not_found() {
        let mut book = OrderBookSide::new(Side::Bid);

        book.insert(order(1, Side::Bid, 100, 1)).unwrap();

        assert_eq!(
            book.cancel(OrderId::new(2).unwrap()),
            Err(Error::OrderNotFound)
        );
    }

    #[test]
    fn empty_price_level_is_removed_after_pop() {
        let mut book = OrderBookSide::new(Side::Ask);

        book.insert(order(1, Side::Ask, 100, 1)).unwrap();
        assert_eq!(book.price_level_count(), 1);

        assert_eq!(
            book.pop_best_order().unwrap().id(),
            OrderId::new(1).unwrap()
        );
        assert_eq!(book.price_level_count(), 0);
        assert!(book.is_empty());
    }
}
