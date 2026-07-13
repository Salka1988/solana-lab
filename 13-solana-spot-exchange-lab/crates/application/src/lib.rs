#![forbid(unsafe_code)]

use domain::{
    AssetId, BalanceAmount, BalanceSheet, Error, Fill, MarketSpec, MatchingEngine, Order,
    Reservation, TraderId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeApplication {
    market: MarketSpec,
    balances: BalanceSheet,
    matching: MatchingEngine,
}

impl ExchangeApplication {
    pub fn new(market: MarketSpec) -> Self {
        Self {
            market,
            balances: BalanceSheet::new(),
            matching: MatchingEngine::new(),
        }
    }

    pub const fn market(&self) -> MarketSpec {
        self.market
    }

    pub const fn balances(&self) -> &BalanceSheet {
        &self.balances
    }

    pub const fn matching(&self) -> &MatchingEngine {
        &self.matching
    }

    pub fn credit_deposit(
        &mut self,
        trader_id: TraderId,
        asset_id: AssetId,
        amount: BalanceAmount,
    ) -> Result<(), Error> {
        self.balances.credit_available(trader_id, asset_id, amount)
    }

    pub fn place_order(&mut self, order: Order) -> Result<Vec<Fill>, Error> {
        let reservation = Reservation::for_order(&order, self.market)?;
        self.balances.reserve(reservation)?;
        self.matching.place_order(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{LotSize, MarketId, OrderId, OrderSequence, Price, Quantity, Side, TickSize};

    fn base_asset() -> AssetId {
        AssetId::new(10).unwrap()
    }

    fn quote_asset() -> AssetId {
        AssetId::new(20).unwrap()
    }

    fn trader(id: u64) -> TraderId {
        TraderId::new(id).unwrap()
    }

    fn market() -> MarketSpec {
        MarketSpec::new(
            base_asset(),
            quote_asset(),
            TickSize::new(1).unwrap(),
            LotSize::new(1).unwrap(),
        )
        .unwrap()
    }

    fn order(id: u128, trader_id: TraderId, side: Side, price: u64, quantity: u64) -> Order {
        Order::new(
            OrderId::new(id).unwrap(),
            trader_id,
            MarketId::new(1).unwrap(),
            side,
            Price::new(price).unwrap(),
            Quantity::new(quantity).unwrap(),
            OrderSequence::new(id.try_into().unwrap()).unwrap(),
        )
    }

    #[test]
    fn deposit_credits_available_balance() {
        let mut app = ExchangeApplication::new(market());

        app.credit_deposit(trader(1), base_asset(), BalanceAmount::new(10))
            .unwrap();

        let balance = app.balances().balance(trader(1), base_asset());
        assert_eq!(balance.available(), BalanceAmount::new(10));
        assert_eq!(balance.reserved(), BalanceAmount::ZERO);
    }

    #[test]
    fn funded_bid_reserves_quote_and_rests() {
        let mut app = ExchangeApplication::new(market());
        let bid = order(1, trader(1), Side::Bid, 100, 7);

        app.credit_deposit(trader(1), quote_asset(), BalanceAmount::new(700))
            .unwrap();
        let fills = app.place_order(bid).unwrap();

        assert!(fills.is_empty());
        let quote_balance = app.balances().balance(trader(1), quote_asset());
        assert_eq!(quote_balance.available(), BalanceAmount::ZERO);
        assert_eq!(quote_balance.reserved(), BalanceAmount::new(700));
        assert_eq!(
            app.matching().bids().best_order().unwrap().id(),
            OrderId::new(1).unwrap()
        );
    }

    #[test]
    fn funded_ask_reserves_base_and_rests() {
        let mut app = ExchangeApplication::new(market());
        let ask = order(1, trader(1), Side::Ask, 100, 7);

        app.credit_deposit(trader(1), base_asset(), BalanceAmount::new(7))
            .unwrap();
        let fills = app.place_order(ask).unwrap();

        assert!(fills.is_empty());
        let base_balance = app.balances().balance(trader(1), base_asset());
        assert_eq!(base_balance.available(), BalanceAmount::ZERO);
        assert_eq!(base_balance.reserved(), BalanceAmount::new(7));
        assert_eq!(
            app.matching().asks().best_order().unwrap().id(),
            OrderId::new(1).unwrap()
        );
    }

    #[test]
    fn unfunded_bid_is_rejected_and_not_inserted() {
        let mut app = ExchangeApplication::new(market());
        let bid = order(1, trader(1), Side::Bid, 100, 7);

        assert_eq!(
            app.place_order(bid),
            Err(Error::InsufficientAvailableBalance)
        );
        assert!(app.matching().bids().is_empty());
        assert!(app.matching().asks().is_empty());
    }

    #[test]
    fn unfunded_ask_is_rejected_and_not_inserted() {
        let mut app = ExchangeApplication::new(market());
        let ask = order(1, trader(1), Side::Ask, 100, 7);

        assert_eq!(
            app.place_order(ask),
            Err(Error::InsufficientAvailableBalance)
        );
        assert!(app.matching().bids().is_empty());
        assert!(app.matching().asks().is_empty());
    }

    #[test]
    fn funded_crossing_orders_produce_fill() {
        let mut app = ExchangeApplication::new(market());
        let ask = order(1, trader(1), Side::Ask, 100, 7);
        let bid = order(2, trader(2), Side::Bid, 105, 7);

        app.credit_deposit(trader(1), base_asset(), BalanceAmount::new(7))
            .unwrap();
        app.credit_deposit(trader(2), quote_asset(), BalanceAmount::new(735))
            .unwrap();
        app.place_order(ask).unwrap();
        let fills = app.place_order(bid).unwrap();

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].maker_order_id(), OrderId::new(1).unwrap());
        assert_eq!(fills[0].taker_order_id(), OrderId::new(2).unwrap());
        assert_eq!(fills[0].price(), Price::new(100).unwrap());
        assert_eq!(fills[0].quantity(), Quantity::new(7).unwrap());
        assert!(app.matching().bids().is_empty());
        assert!(app.matching().asks().is_empty());
    }
}
