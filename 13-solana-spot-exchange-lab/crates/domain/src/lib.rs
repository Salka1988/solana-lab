#![forbid(unsafe_code)]

pub mod error;
pub mod ids;
pub mod market;
pub mod money;
mod newtype;
pub mod order;
pub mod order_book;

pub use error::Error;
pub use ids::{AssetId, MarketId, OrderId, TraderId};
pub use market::MarketSpec;
pub use money::{Amount, LotSize, Price, Quantity, TickSize};
pub use order::{Order, OrderSequence, OrderStatus, RemainingQuantity, Side};
pub use order_book::OrderBookSide;
