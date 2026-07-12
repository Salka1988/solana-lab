use crate::newtype::non_zero_newtype;
use core::num::{NonZeroU128, NonZeroU32, NonZeroU64};

non_zero_newtype!(AssetId, u32, NonZeroU32);
non_zero_newtype!(MarketId, u32, NonZeroU32);
non_zero_newtype!(TraderId, u64, NonZeroU64);
non_zero_newtype!(OrderId, u128, NonZeroU128);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn ids_reject_zero() {
        assert_eq!(AssetId::new(0), Err(Error::ZeroValue));
        assert_eq!(MarketId::new(0), Err(Error::ZeroValue));
        assert_eq!(TraderId::new(0), Err(Error::ZeroValue));
        assert_eq!(OrderId::new(0), Err(Error::ZeroValue));
    }
}
