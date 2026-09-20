#![forbid(unsafe_code)]

mod schema;
mod storage;

pub use schema::CURRENT_SCHEMA_VERSION;
pub use storage::*;
