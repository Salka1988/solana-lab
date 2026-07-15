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
    NoMatchingOrder,
    AmountConversionOverflow,
    InsufficientAvailableBalance,
    InsufficientReservedBalance,
    SignedOrderMarketMismatch,
    SignedOrderWrongSide,
    SignedOrderExpired,
    SignedOrderSelfTrade,
    SignedOrderPricesDoNotCross,
    FillPriceOutsideSignedOrder,
    FillQuantityExceedsSignedOrder,
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
            Self::NoMatchingOrder => f.write_str("no matching order"),
            Self::AmountConversionOverflow => f.write_str("amount conversion overflow"),
            Self::InsufficientAvailableBalance => f.write_str("insufficient available balance"),
            Self::InsufficientReservedBalance => f.write_str("insufficient reserved balance"),
            Self::SignedOrderMarketMismatch => {
                f.write_str("signed orders must belong to the same market")
            }
            Self::SignedOrderWrongSide => {
                f.write_str("signed fill requires a bid and an ask order")
            }
            Self::SignedOrderExpired => f.write_str("signed order is expired"),
            Self::SignedOrderSelfTrade => f.write_str("signed fill cannot settle self trade"),
            Self::SignedOrderPricesDoNotCross => f.write_str("signed order prices do not cross"),
            Self::FillPriceOutsideSignedOrder => {
                f.write_str("fill price is outside signed order limits")
            }
            Self::FillQuantityExceedsSignedOrder => {
                f.write_str("fill quantity exceeds signed order quantity")
            }
        }
    }
}

impl std::error::Error for Error {}
