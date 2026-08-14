#![forbid(unsafe_code)]

pub mod error;
pub mod event;

use std::collections::BTreeSet;

use domain::{
    AssetId, BalanceAmount, BalanceSheet, Fill, MarketSpec, MatchingEngine, Order, Reservation,
    TraderId,
};

pub use error::Error;
pub use event::{CommandId, Event};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeApplication {
    market: MarketSpec,
    balances: BalanceSheet,
    matching: MatchingEngine,
    events: Vec<Event>,
    seen_commands: BTreeSet<CommandId>,
}

impl ExchangeApplication {
    pub fn new(market: MarketSpec) -> Self {
        Self {
            market,
            balances: BalanceSheet::new(),
            matching: MatchingEngine::new(),
            events: Vec::new(),
            seen_commands: BTreeSet::new(),
        }
    }

    pub fn replay(market: MarketSpec, events: impl IntoIterator<Item = Event>) -> Result<Self> {
        let mut app = Self::new(market);

        for event in events {
            app.apply_event(event)?;
        }

        Ok(app)
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

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn credit_deposit(
        &mut self,
        command_id: CommandId,
        trader_id: TraderId,
        asset_id: AssetId,
        amount: BalanceAmount,
    ) -> Result<()> {
        let event = self.credit_deposit_event(command_id, trader_id, asset_id, amount)?;
        self.apply_event(event)
    }

    pub fn credit_deposit_event(
        &self,
        command_id: CommandId,
        trader_id: TraderId,
        asset_id: AssetId,
        amount: BalanceAmount,
    ) -> Result<Event> {
        self.reject_duplicate(command_id)?;
        Ok(Event::DepositCredited {
            command_id,
            trader_id,
            asset_id,
            amount,
        })
    }

    pub fn place_order(&mut self, command_id: CommandId, order: Order) -> Result<Vec<Fill>> {
        let (event, fills) = self.place_order_event(command_id, order)?;
        self.apply_event(event)?;

        Ok(fills)
    }

    pub fn place_order_event(
        &self,
        command_id: CommandId,
        order: Order,
    ) -> Result<(Event, Vec<Fill>)> {
        self.reject_duplicate(command_id)?;

        let mut simulation = self.clone();
        let fills = simulation.place_order_in_domain(order)?;
        let event = Event::OrderPlaced {
            command_id,
            order,
            fills: fills.clone(),
        };

        Ok((event, fills))
    }

    pub fn apply_event(&mut self, event: Event) -> Result<()> {
        self.reject_duplicate(event.command_id())?;

        match event {
            Event::DepositCredited {
                trader_id,
                asset_id,
                amount,
                ..
            } => {
                self.balances
                    .credit_available(trader_id, asset_id, amount)?;
                self.record_applied_event(event)
            }
            Event::OrderPlaced {
                order, ref fills, ..
            } => {
                let replayed_fills = self.place_order_in_domain(order)?;
                if replayed_fills != *fills {
                    return Err(Error::ReplayMismatch);
                }
                self.record_applied_event(event)
            }
        }
    }

    fn place_order_in_domain(&mut self, order: Order) -> Result<Vec<Fill>> {
        if order.is_terminal() {
            return Err(Error::Domain(domain::Error::OrderAlreadyTerminal));
        }

        let reservation = Reservation::for_order(&order, self.market)?;
        let mut balances = self.balances.clone();
        let mut matching = self.matching.clone();

        balances.reserve(reservation)?;
        let fills = matching.place_order(order).map_err(Error::from)?;

        self.balances = balances;
        self.matching = matching;

        Ok(fills)
    }

    fn record_applied_event(&mut self, event: Event) -> Result<()> {
        let command_id = event.command_id();
        if !self.seen_commands.insert(command_id) {
            return Err(Error::DuplicateCommand);
        }
        self.events.push(event);
        Ok(())
    }

    fn reject_duplicate(&self, command_id: CommandId) -> Result<()> {
        if self.seen_commands.contains(&command_id) {
            return Err(Error::DuplicateCommand);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{LotSize, MarketId, OrderId, OrderSequence, Price, Quantity, Side, TickSize};

    fn command(id: u128) -> CommandId {
        CommandId::new(id).unwrap()
    }

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
    fn deposit_records_event_and_credits_available_balance() {
        let mut app = ExchangeApplication::new(market());

        app.credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(10))
            .unwrap();

        let balance = app.balances().balance(trader(1), base_asset());
        assert_eq!(balance.available(), BalanceAmount::new(10));
        assert_eq!(balance.reserved(), BalanceAmount::ZERO);
        assert_eq!(app.events().len(), 1);
    }

    #[test]
    fn funded_bid_records_event_reserves_quote_and_rests() {
        let mut app = ExchangeApplication::new(market());
        let bid = order(1, trader(1), Side::Bid, 100, 7);

        app.credit_deposit(
            command(1),
            trader(1),
            quote_asset(),
            BalanceAmount::new(700),
        )
        .unwrap();
        let fills = app.place_order(command(2), bid).unwrap();

        assert!(fills.is_empty());
        assert_eq!(app.events().len(), 2);
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

        app.credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(7))
            .unwrap();
        let fills = app.place_order(command(2), ask).unwrap();

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
    fn unfunded_bid_is_rejected_and_records_no_event() {
        let mut app = ExchangeApplication::new(market());
        let bid = order(1, trader(1), Side::Bid, 100, 7);

        assert_eq!(
            app.place_order(command(1), bid),
            Err(Error::Domain(domain::Error::InsufficientAvailableBalance))
        );
        assert!(app.events().is_empty());
        assert!(app.matching().bids().is_empty());
        assert!(app.matching().asks().is_empty());
    }

    #[test]
    fn unfunded_ask_is_rejected_and_records_no_event() {
        let mut app = ExchangeApplication::new(market());
        let ask = order(1, trader(1), Side::Ask, 100, 7);

        assert_eq!(
            app.place_order(command(1), ask),
            Err(Error::Domain(domain::Error::InsufficientAvailableBalance))
        );
        assert!(app.events().is_empty());
        assert!(app.matching().bids().is_empty());
        assert!(app.matching().asks().is_empty());
    }

    #[test]
    fn deposit_event_derivation_does_not_mutate_state() {
        let mut app = ExchangeApplication::new(market());

        let event = app
            .credit_deposit_event(command(1), trader(1), base_asset(), BalanceAmount::new(10))
            .unwrap();

        let balance = app.balances().balance(trader(1), base_asset());
        assert_eq!(balance.available(), BalanceAmount::ZERO);
        assert!(app.events().is_empty());

        app.apply_event(event).unwrap();

        let balance = app.balances().balance(trader(1), base_asset());
        assert_eq!(balance.available(), BalanceAmount::new(10));
        assert_eq!(app.events().len(), 1);
    }

    #[test]
    fn order_event_derivation_does_not_mutate_state() {
        let mut app = ExchangeApplication::new(market());
        let ask = order(1, trader(1), Side::Ask, 100, 7);

        app.credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(7))
            .unwrap();
        let (event, fills) = app.place_order_event(command(2), ask).unwrap();

        assert!(fills.is_empty());
        let base_balance = app.balances().balance(trader(1), base_asset());
        assert_eq!(base_balance.available(), BalanceAmount::new(7));
        assert_eq!(base_balance.reserved(), BalanceAmount::ZERO);
        assert!(app.matching().asks().is_empty());
        assert_eq!(app.events().len(), 1);

        app.apply_event(event).unwrap();

        let base_balance = app.balances().balance(trader(1), base_asset());
        assert_eq!(base_balance.available(), BalanceAmount::ZERO);
        assert_eq!(base_balance.reserved(), BalanceAmount::new(7));
        assert_eq!(
            app.matching().asks().best_order().unwrap().id(),
            OrderId::new(1).unwrap()
        );
        assert_eq!(app.events().len(), 2);
    }

    #[test]
    fn terminal_order_is_rejected_before_reserving_balance() {
        let mut app = ExchangeApplication::new(market());
        let mut ask = order(1, trader(1), Side::Ask, 100, 7);

        app.credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(7))
            .unwrap();
        ask.cancel().unwrap();

        assert_eq!(
            app.place_order(command(2), ask),
            Err(Error::Domain(domain::Error::OrderAlreadyTerminal))
        );

        let balance = app.balances().balance(trader(1), base_asset());
        assert_eq!(balance.available(), BalanceAmount::new(7));
        assert_eq!(balance.reserved(), BalanceAmount::ZERO);
        assert_eq!(app.events().len(), 1);
        assert!(app.matching().bids().is_empty());
        assert!(app.matching().asks().is_empty());
    }

    #[test]
    fn funded_crossing_orders_produce_fill_and_record_event() {
        let mut app = ExchangeApplication::new(market());
        let ask = order(1, trader(1), Side::Ask, 100, 7);
        let bid = order(2, trader(2), Side::Bid, 105, 7);

        app.credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(7))
            .unwrap();
        app.credit_deposit(
            command(2),
            trader(2),
            quote_asset(),
            BalanceAmount::new(735),
        )
        .unwrap();
        app.place_order(command(3), ask).unwrap();
        let fills = app.place_order(command(4), bid).unwrap();

        assert_eq!(fills.len(), 1);
        assert_eq!(app.events().len(), 4);
        assert_eq!(fills[0].maker_order_id(), OrderId::new(1).unwrap());
        assert_eq!(fills[0].taker_order_id(), OrderId::new(2).unwrap());
        assert_eq!(fills[0].price(), Price::new(100).unwrap());
        assert_eq!(fills[0].quantity(), Quantity::new(7).unwrap());
        assert!(app.matching().bids().is_empty());
        assert!(app.matching().asks().is_empty());
    }

    #[test]
    fn duplicate_command_is_rejected() {
        let mut app = ExchangeApplication::new(market());

        app.credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(10))
            .unwrap();

        assert_eq!(
            app.credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(10)),
            Err(Error::DuplicateCommand)
        );
        assert_eq!(app.events().len(), 1);
    }

    #[test]
    fn replay_rebuilds_balances_book_and_events() {
        let mut app = ExchangeApplication::new(market());
        let ask = order(1, trader(1), Side::Ask, 100, 7);
        let bid = order(2, trader(2), Side::Bid, 99, 7);

        app.credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(7))
            .unwrap();
        app.credit_deposit(
            command(2),
            trader(2),
            quote_asset(),
            BalanceAmount::new(693),
        )
        .unwrap();
        app.place_order(command(3), ask).unwrap();
        app.place_order(command(4), bid).unwrap();

        let replayed = ExchangeApplication::replay(market(), app.events().iter().cloned()).unwrap();

        assert_eq!(replayed, app);
    }

    #[test]
    fn replay_rejects_duplicate_command_id_in_log() {
        let event = Event::DepositCredited {
            command_id: command(1),
            trader_id: trader(1),
            asset_id: base_asset(),
            amount: BalanceAmount::new(10),
        };

        assert_eq!(
            ExchangeApplication::replay(market(), [event.clone(), event]),
            Err(Error::DuplicateCommand)
        );
    }
}
