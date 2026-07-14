#![forbid(unsafe_code)]

pub mod postgres;
pub mod record;

pub use postgres::PostgresEventJournal;
pub use record::{EventRecord, PersistenceError};
