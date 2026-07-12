use crate::{AssetId, Error, LotSize, Price, Quantity, TickSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarketSpec {
    base_asset: AssetId,
    quote_asset: AssetId,
    tick_size: TickSize,
    lot_size: LotSize,
}

impl MarketSpec {
    pub fn new(
        base_asset: AssetId,
        quote_asset: AssetId,
        tick_size: TickSize,
        lot_size: LotSize,
    ) -> Result<Self, Error> {
        if base_asset == quote_asset {
            return Err(Error::SameMarketAssets);
        }

        Ok(Self {
            base_asset,
            quote_asset,
            tick_size,
            lot_size,
        })
    }

    pub const fn base_asset(self) -> AssetId {
        self.base_asset
    }

    pub const fn quote_asset(self) -> AssetId {
        self.quote_asset
    }

    pub const fn tick_size(self) -> TickSize {
        self.tick_size
    }

    pub const fn lot_size(self) -> LotSize {
        self.lot_size
    }

    pub fn validate_price(self, price: Price) -> Result<(), Error> {
        if !price.get().is_multiple_of(self.tick_size.get()) {
            return Err(Error::PriceNotTickAligned);
        }

        Ok(())
    }

    pub fn validate_quantity(self, quantity: Quantity) -> Result<(), Error> {
        if !quantity.get().is_multiple_of(self.lot_size.get()) {
            return Err(Error::QuantityNotLotAligned);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn market() -> MarketSpec {
        MarketSpec::new(
            AssetId::new(1).unwrap(),
            AssetId::new(2).unwrap(),
            TickSize::new(5).unwrap(),
            LotSize::new(10).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn market_rejects_same_base_and_quote_asset() {
        let asset = AssetId::new(1).unwrap();

        assert_eq!(
            MarketSpec::new(
                asset,
                asset,
                TickSize::new(1).unwrap(),
                LotSize::new(1).unwrap()
            ),
            Err(Error::SameMarketAssets)
        );
    }

    #[test]
    fn price_must_align_to_tick_size() {
        let market = market();

        assert_eq!(market.validate_price(Price::new(15).unwrap()), Ok(()));
        assert_eq!(
            market.validate_price(Price::new(16).unwrap()),
            Err(Error::PriceNotTickAligned)
        );
    }

    #[test]
    fn quantity_must_align_to_lot_size() {
        let market = market();

        assert_eq!(market.validate_quantity(Quantity::new(20).unwrap()), Ok(()));
        assert_eq!(
            market.validate_quantity(Quantity::new(21).unwrap()),
            Err(Error::QuantityNotLotAligned)
        );
    }
}
