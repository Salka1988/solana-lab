use crate::{Error, Order, OrderBookSide, OrderId, Price, Quantity, Side, TraderId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fill {
    maker_order_id: OrderId,
    taker_order_id: OrderId,
    maker_trader_id: TraderId,
    taker_trader_id: TraderId,
    price: Price,
    quantity: Quantity,
}

impl Fill {
    pub fn new(maker: Order, taker: Order, price: Price, quantity: Quantity) -> Self {
        Self {
            maker_order_id: maker.id(),
            taker_order_id: taker.id(),
            maker_trader_id: maker.trader_id(),
            taker_trader_id: taker.trader_id(),
            price,
            quantity,
        }
    }

    pub const fn maker_order_id(self) -> OrderId {
        self.maker_order_id
    }

    pub const fn taker_order_id(self) -> OrderId {
        self.taker_order_id
    }

    pub const fn maker_trader_id(self) -> TraderId {
        self.maker_trader_id
    }

    pub const fn taker_trader_id(self) -> TraderId {
        self.taker_trader_id
    }

    pub const fn price(self) -> Price {
        self.price
    }

    pub const fn quantity(self) -> Quantity {
        self.quantity
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchingEngine {
    bids: OrderBookSide,
    asks: OrderBookSide,
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self {
            bids: OrderBookSide::new(Side::Bid),
            asks: OrderBookSide::new(Side::Ask),
        }
    }

    pub const fn bids(&self) -> &OrderBookSide {
        &self.bids
    }

    pub const fn asks(&self) -> &OrderBookSide {
        &self.asks
    }

    pub fn place_order(&mut self, order: Order) -> Result<Vec<Fill>, Error> {
        if order.is_terminal() {
            return Err(Error::OrderAlreadyTerminal);
        }

        match order.side() {
            Side::Bid => self.match_bid(order),
            Side::Ask => self.match_ask(order),
        }
    }

    fn match_bid(&mut self, mut taker: Order) -> Result<Vec<Fill>, Error> {
        let mut fills = Vec::new();

        while !taker.remaining_quantity().is_zero() && crosses_bid(taker, self.asks.best_order()) {
            let mut maker = self.asks.pop_best_order().ok_or(Error::NoMatchingOrder)?;
            let fill_quantity = min_remaining_quantity(taker, maker)?;
            let fill = Fill::new(maker, taker, maker.price(), fill_quantity);

            maker.apply_fill(fill_quantity)?;
            taker.apply_fill(fill_quantity)?;
            fills.push(fill);

            if !maker.is_terminal() {
                self.asks.reinsert_front(maker)?;
            }
        }

        if !taker.is_terminal() {
            self.bids.insert(taker)?;
        }

        Ok(fills)
    }

    fn match_ask(&mut self, mut taker: Order) -> Result<Vec<Fill>, Error> {
        let mut fills = Vec::new();

        while !taker.remaining_quantity().is_zero() && crosses_ask(taker, self.bids.best_order()) {
            let mut maker = self.bids.pop_best_order().ok_or(Error::NoMatchingOrder)?;
            let fill_quantity = min_remaining_quantity(taker, maker)?;
            let fill = Fill::new(maker, taker, maker.price(), fill_quantity);

            maker.apply_fill(fill_quantity)?;
            taker.apply_fill(fill_quantity)?;
            fills.push(fill);

            if !maker.is_terminal() {
                self.bids.reinsert_front(maker)?;
            }
        }

        if !taker.is_terminal() {
            self.asks.insert(taker)?;
        }

        Ok(fills)
    }
}

impl Default for MatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn crosses_bid(taker: Order, best_ask: Option<&Order>) -> bool {
    best_ask.is_some_and(|ask| taker.price() >= ask.price())
}

fn crosses_ask(taker: Order, best_bid: Option<&Order>) -> bool {
    best_bid.is_some_and(|bid| taker.price() <= bid.price())
}

fn min_remaining_quantity(lhs: Order, rhs: Order) -> Result<Quantity, Error> {
    let quantity = lhs
        .remaining_quantity()
        .get()
        .min(rhs.remaining_quantity().get());
    Quantity::new(quantity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MarketId, OrderSequence};

    fn order(id: u128, trader_id: u64, side: Side, price: u64, quantity: u64) -> Order {
        Order::new(
            OrderId::new(id).unwrap(),
            TraderId::new(trader_id).unwrap(),
            MarketId::new(1).unwrap(),
            side,
            Price::new(price).unwrap(),
            Quantity::new(quantity).unwrap(),
            OrderSequence::new(id.try_into().unwrap()).unwrap(),
        )
    }

    #[test]
    fn bid_below_best_ask_does_not_match_and_rests() {
        let mut engine = MatchingEngine::new();

        engine.place_order(order(1, 1, Side::Ask, 100, 10)).unwrap();
        let fills = engine.place_order(order(2, 2, Side::Bid, 99, 10)).unwrap();

        assert!(fills.is_empty());
        assert_eq!(
            engine.asks().best_order().unwrap().id(),
            OrderId::new(1).unwrap()
        );
        assert_eq!(
            engine.bids().best_order().unwrap().id(),
            OrderId::new(2).unwrap()
        );
    }

    #[test]
    fn crossing_bid_matches_resting_ask_at_maker_price() {
        let mut engine = MatchingEngine::new();

        engine.place_order(order(1, 1, Side::Ask, 100, 10)).unwrap();
        let fills = engine.place_order(order(2, 2, Side::Bid, 105, 10)).unwrap();

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].maker_order_id(), OrderId::new(1).unwrap());
        assert_eq!(fills[0].taker_order_id(), OrderId::new(2).unwrap());
        assert_eq!(fills[0].price(), Price::new(100).unwrap());
        assert_eq!(fills[0].quantity(), Quantity::new(10).unwrap());
        assert!(engine.asks().is_empty());
        assert!(engine.bids().is_empty());
    }

    #[test]
    fn crossing_ask_matches_resting_bid_at_maker_price() {
        let mut engine = MatchingEngine::new();

        engine.place_order(order(1, 1, Side::Bid, 105, 10)).unwrap();
        let fills = engine.place_order(order(2, 2, Side::Ask, 100, 10)).unwrap();

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].maker_order_id(), OrderId::new(1).unwrap());
        assert_eq!(fills[0].price(), Price::new(105).unwrap());
        assert!(engine.asks().is_empty());
        assert!(engine.bids().is_empty());
    }

    #[test]
    fn partial_maker_fill_keeps_maker_at_front() {
        let mut engine = MatchingEngine::new();

        engine.place_order(order(1, 1, Side::Ask, 100, 10)).unwrap();
        engine.place_order(order(2, 2, Side::Ask, 100, 10)).unwrap();
        let fills = engine.place_order(order(3, 3, Side::Bid, 100, 4)).unwrap();

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].maker_order_id(), OrderId::new(1).unwrap());
        let best_ask = engine.asks().best_order().unwrap();
        assert_eq!(best_ask.id(), OrderId::new(1).unwrap());
        assert_eq!(best_ask.remaining_quantity().get(), 6);
    }

    #[test]
    fn partially_filled_taker_rests_with_remaining_quantity() {
        let mut engine = MatchingEngine::new();

        engine.place_order(order(1, 1, Side::Ask, 100, 4)).unwrap();
        let fills = engine.place_order(order(2, 2, Side::Bid, 100, 10)).unwrap();

        assert_eq!(fills.len(), 1);
        assert!(engine.asks().is_empty());
        let resting_bid = engine.bids().best_order().unwrap();
        assert_eq!(resting_bid.id(), OrderId::new(2).unwrap());
        assert_eq!(resting_bid.remaining_quantity().get(), 6);
    }

    #[test]
    fn taker_can_fill_multiple_makers_in_fifo_order() {
        let mut engine = MatchingEngine::new();

        engine.place_order(order(1, 1, Side::Ask, 100, 5)).unwrap();
        engine.place_order(order(2, 2, Side::Ask, 100, 5)).unwrap();
        engine.place_order(order(3, 3, Side::Ask, 101, 5)).unwrap();
        let fills = engine.place_order(order(4, 4, Side::Bid, 101, 12)).unwrap();

        assert_eq!(fills.len(), 3);
        assert_eq!(fills[0].maker_order_id(), OrderId::new(1).unwrap());
        assert_eq!(fills[1].maker_order_id(), OrderId::new(2).unwrap());
        assert_eq!(fills[2].maker_order_id(), OrderId::new(3).unwrap());
        assert_eq!(fills[2].quantity(), Quantity::new(2).unwrap());
        assert_eq!(
            engine
                .asks()
                .best_order()
                .unwrap()
                .remaining_quantity()
                .get(),
            3
        );
    }

    #[test]
    fn same_sequence_of_orders_replays_to_same_fills_and_book_state() {
        fn run() -> (Vec<Fill>, MatchingEngine) {
            let mut engine = MatchingEngine::new();
            let mut fills = Vec::new();

            for order in [
                order(1, 1, Side::Ask, 100, 5),
                order(2, 2, Side::Ask, 101, 5),
                order(3, 3, Side::Bid, 101, 7),
                order(4, 4, Side::Bid, 99, 4),
            ] {
                fills.extend(engine.place_order(order).unwrap());
            }

            (fills, engine)
        }

        let first = run();
        let second = run();

        assert_eq!(first, second);
    }
}
