use crate::{newtype::zeroable_u64_newtype, AssetId, Error, MarketSpec, Order, Side, TraderId};
use std::collections::BTreeMap;

zeroable_u64_newtype!(BalanceAmount);

impl BalanceAmount {
    pub fn from_u128(value: u128) -> Result<Self, Error> {
        let value = u64::try_from(value).map_err(|_| Error::AmountConversionOverflow)?;
        Ok(Self(value))
    }

    pub fn checked_add(self, rhs: Self) -> Result<Self, Error> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(Error::ArithmeticOverflow)
    }

    pub fn checked_sub(self, rhs: Self) -> Result<Self, Error> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or(Error::ArithmeticUnderflow)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Balance {
    available: BalanceAmount,
    reserved: BalanceAmount,
}

impl Balance {
    pub const fn available(self) -> BalanceAmount {
        self.available
    }

    pub const fn reserved(self) -> BalanceAmount {
        self.reserved
    }

    pub fn credit_available(&mut self, amount: BalanceAmount) -> Result<(), Error> {
        self.available = self.available.checked_add(amount)?;
        Ok(())
    }

    pub fn reserve(&mut self, amount: BalanceAmount) -> Result<(), Error> {
        if self.available < amount {
            return Err(Error::InsufficientAvailableBalance);
        }

        self.available = self.available.checked_sub(amount)?;
        self.reserved = self.reserved.checked_add(amount)?;
        Ok(())
    }

    pub fn release(&mut self, amount: BalanceAmount) -> Result<(), Error> {
        if self.reserved < amount {
            return Err(Error::InsufficientReservedBalance);
        }

        self.reserved = self.reserved.checked_sub(amount)?;
        self.available = self.available.checked_add(amount)?;
        Ok(())
    }

    pub fn debit_reserved(&mut self, amount: BalanceAmount) -> Result<(), Error> {
        if self.reserved < amount {
            return Err(Error::InsufficientReservedBalance);
        }

        self.reserved = self.reserved.checked_sub(amount)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reservation {
    trader_id: TraderId,
    asset_id: AssetId,
    amount: BalanceAmount,
}

impl Reservation {
    pub const fn new(trader_id: TraderId, asset_id: AssetId, amount: BalanceAmount) -> Self {
        Self {
            trader_id,
            asset_id,
            amount,
        }
    }

    pub const fn trader_id(self) -> TraderId {
        self.trader_id
    }

    pub const fn asset_id(self) -> AssetId {
        self.asset_id
    }

    pub const fn amount(self) -> BalanceAmount {
        self.amount
    }

    pub fn for_order(order: &Order, market: MarketSpec) -> Result<Self, Error> {
        let (asset_id, amount) = match order.side() {
            Side::Bid => (
                market.quote_asset(),
                BalanceAmount::from_u128(order.price().quote_cost(order.original_quantity())?)?,
            ),
            Side::Ask => (
                market.base_asset(),
                BalanceAmount::new(order.original_quantity().get()),
            ),
        };

        Ok(Self::new(order.trader_id(), asset_id, amount))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BalanceSheet {
    balances: BTreeMap<(TraderId, AssetId), Balance>,
}

impl BalanceSheet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn balance(&self, trader_id: TraderId, asset_id: AssetId) -> Balance {
        self.balances
            .get(&(trader_id, asset_id))
            .copied()
            .unwrap_or_default()
    }

    pub fn credit_available(
        &mut self,
        trader_id: TraderId,
        asset_id: AssetId,
        amount: BalanceAmount,
    ) -> Result<(), Error> {
        self.balance_mut(trader_id, asset_id)
            .credit_available(amount)
    }

    pub fn reserve(&mut self, reservation: Reservation) -> Result<(), Error> {
        self.balance_mut(reservation.trader_id, reservation.asset_id)
            .reserve(reservation.amount)
    }

    pub fn release(&mut self, reservation: Reservation) -> Result<(), Error> {
        self.balance_mut(reservation.trader_id, reservation.asset_id)
            .release(reservation.amount)
    }

    pub fn debit_reserved(&mut self, reservation: Reservation) -> Result<(), Error> {
        self.balance_mut(reservation.trader_id, reservation.asset_id)
            .debit_reserved(reservation.amount)
    }

    fn balance_mut(&mut self, trader_id: TraderId, asset_id: AssetId) -> &mut Balance {
        self.balances.entry((trader_id, asset_id)).or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MarketId, OrderId, OrderSequence, Price, Quantity, TickSize};

    fn trader() -> TraderId {
        TraderId::new(1).unwrap()
    }

    fn base_asset() -> AssetId {
        AssetId::new(10).unwrap()
    }

    fn quote_asset() -> AssetId {
        AssetId::new(20).unwrap()
    }

    fn market() -> MarketSpec {
        MarketSpec::new(
            base_asset(),
            quote_asset(),
            TickSize::new(1).unwrap(),
            crate::LotSize::new(1).unwrap(),
        )
        .unwrap()
    }

    fn order(side: Side, price: u64, quantity: u64) -> Order {
        Order::new(
            OrderId::new(1).unwrap(),
            trader(),
            MarketId::new(1).unwrap(),
            side,
            Price::new(price).unwrap(),
            Quantity::new(quantity).unwrap(),
            OrderSequence::new(1).unwrap(),
        )
    }

    #[test]
    fn reserve_moves_available_to_reserved() {
        let mut sheet = BalanceSheet::new();
        let reservation = Reservation::new(trader(), base_asset(), BalanceAmount::new(7));

        sheet
            .credit_available(trader(), base_asset(), BalanceAmount::new(10))
            .unwrap();
        sheet.reserve(reservation).unwrap();

        let balance = sheet.balance(trader(), base_asset());
        assert_eq!(balance.available(), BalanceAmount::new(3));
        assert_eq!(balance.reserved(), BalanceAmount::new(7));
    }

    #[test]
    fn reserve_insufficient_available_balance_fails_without_mutation() {
        let mut sheet = BalanceSheet::new();
        let reservation = Reservation::new(trader(), base_asset(), BalanceAmount::new(11));

        sheet
            .credit_available(trader(), base_asset(), BalanceAmount::new(10))
            .unwrap();

        assert_eq!(
            sheet.reserve(reservation),
            Err(Error::InsufficientAvailableBalance)
        );
        assert_eq!(
            sheet.balance(trader(), base_asset()).available(),
            BalanceAmount::new(10)
        );
        assert_eq!(
            sheet.balance(trader(), base_asset()).reserved(),
            BalanceAmount::ZERO
        );
    }

    #[test]
    fn release_moves_reserved_to_available() {
        let mut sheet = BalanceSheet::new();
        let reservation = Reservation::new(trader(), base_asset(), BalanceAmount::new(7));

        sheet
            .credit_available(trader(), base_asset(), BalanceAmount::new(10))
            .unwrap();
        sheet.reserve(reservation).unwrap();
        sheet
            .release(Reservation::new(
                trader(),
                base_asset(),
                BalanceAmount::new(4),
            ))
            .unwrap();

        let balance = sheet.balance(trader(), base_asset());
        assert_eq!(balance.available(), BalanceAmount::new(7));
        assert_eq!(balance.reserved(), BalanceAmount::new(3));
    }

    #[test]
    fn release_insufficient_reserved_balance_fails_without_mutation() {
        let mut sheet = BalanceSheet::new();
        let reservation = Reservation::new(trader(), base_asset(), BalanceAmount::new(7));

        sheet
            .credit_available(trader(), base_asset(), BalanceAmount::new(10))
            .unwrap();
        sheet.reserve(reservation).unwrap();

        assert_eq!(
            sheet.release(Reservation::new(
                trader(),
                base_asset(),
                BalanceAmount::new(8)
            )),
            Err(Error::InsufficientReservedBalance)
        );

        let balance = sheet.balance(trader(), base_asset());
        assert_eq!(balance.available(), BalanceAmount::new(3));
        assert_eq!(balance.reserved(), BalanceAmount::new(7));
    }

    #[test]
    fn debit_reserved_consumes_reserved_balance() {
        let mut sheet = BalanceSheet::new();
        let reservation = Reservation::new(trader(), base_asset(), BalanceAmount::new(7));

        sheet
            .credit_available(trader(), base_asset(), BalanceAmount::new(10))
            .unwrap();
        sheet.reserve(reservation).unwrap();
        sheet
            .debit_reserved(Reservation::new(
                trader(),
                base_asset(),
                BalanceAmount::new(4),
            ))
            .unwrap();

        let balance = sheet.balance(trader(), base_asset());
        assert_eq!(balance.available(), BalanceAmount::new(3));
        assert_eq!(balance.reserved(), BalanceAmount::new(3));
    }

    #[test]
    fn bid_reservation_uses_quote_asset_and_quote_cost() {
        let bid = order(Side::Bid, 100, 7);

        let reservation = Reservation::for_order(&bid, market()).unwrap();

        assert_eq!(reservation.trader_id(), trader());
        assert_eq!(reservation.asset_id(), quote_asset());
        assert_eq!(reservation.amount(), BalanceAmount::new(700));
    }

    #[test]
    fn ask_reservation_uses_base_asset_and_quantity() {
        let ask = order(Side::Ask, 100, 7);

        let reservation = Reservation::for_order(&ask, market()).unwrap();

        assert_eq!(reservation.trader_id(), trader());
        assert_eq!(reservation.asset_id(), base_asset());
        assert_eq!(reservation.amount(), BalanceAmount::new(7));
    }

    #[test]
    fn quote_cost_over_u64_is_rejected_for_balance_reservation() {
        let bid = order(Side::Bid, u64::MAX, u64::MAX);

        assert_eq!(
            Reservation::for_order(&bid, market()),
            Err(Error::AmountConversionOverflow)
        );
    }
}
