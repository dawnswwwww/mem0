pub mod cli;
pub mod core;
pub mod output;
pub mod store;

pub use core::{MemError, MemResult};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
