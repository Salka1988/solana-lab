#![forbid(unsafe_code)]

pub mod error;
pub mod ids;
pub mod market;
pub mod money;
mod newtype;

pub use error::Error;
pub use ids::{AssetId, MarketId, OrderId, TraderId};
pub use market::MarketSpec;
pub use money::{Amount, LotSize, Price, Quantity, TickSize};
