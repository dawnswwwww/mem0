pub mod cli;
pub mod core;
pub mod output;
pub mod store;
#[cfg(feature = "embed")]
pub mod embed;

pub use core::{MemError, MemResult};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
