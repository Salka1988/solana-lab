use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    ZeroValue,
    SameMarketAssets,
    PriceNotTickAligned,
    QuantityNotLotAligned,
    ArithmeticOverflow,
    ArithmeticUnderflow,
    FillExceedsRemainingQuantity,
    OrderAlreadyTerminal,
    WrongOrderSide,
    OrderNotFound,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroValue => f.write_str("value must be non-zero"),
            Self::SameMarketAssets => f.write_str("market base and quote assets must differ"),
            Self::PriceNotTickAligned => f.write_str("price is not aligned to market tick size"),
            Self::QuantityNotLotAligned => {
                f.write_str("quantity is not aligned to market lot size")
            }
            Self::ArithmeticOverflow => f.write_str("arithmetic overflow"),
            Self::ArithmeticUnderflow => f.write_str("arithmetic underflow"),
            Self::FillExceedsRemainingQuantity => {
                f.write_str("fill exceeds remaining order quantity")
            }
            Self::OrderAlreadyTerminal => f.write_str("order is already terminal"),
            Self::WrongOrderSide => f.write_str("order side does not match book side"),
            Self::OrderNotFound => f.write_str("order not found"),
        }
    }
}

impl std::error::Error for Error {}
