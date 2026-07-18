//! Local CPU text embedding (opt-in `embed` feature).
//!
//! Produces `Vec<f32>` only; the cli layer feeds results into the existing
//! v1.2 `store::vectors` path. This module has no dependency on `store`.

pub mod model;
