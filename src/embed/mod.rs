//! Local CPU text embedding (opt-in `embed` feature).
//!
//! Produces `Vec<f32>` only; the cli layer feeds results into the existing
//! v1.2 `store::vectors` path. This module has no dependency on `store`.

pub mod model;
pub mod store;

use crate::core::error::{MemError, MemResult};
pub use model::ModelChoice;

/// Which side of the e5 query/passage asymmetry a text is on. e5 models need an
/// instruction prefix for best retrieval quality; fastembed does NOT add it, so we do.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Role {
    /// Stored content (`add`). Prefix: `passage: `.
    Passage,
    /// A search query (`vsearch`, `embed` default). Prefix: `query: `.
    Query,
}

/// Prepend the e5 instruction prefix. Centralised so callers pass plain text.
pub fn apply_prefix(text: &str, role: Role) -> String {
    let pfx = match role {
        Role::Passage => "passage: ",
        Role::Query   => "query: ",
    };
    format!("{pfx}{text}")
}

/// Embed a single text. Initialises the model once for this call.
pub fn embed_text(text: &str, role: Role, model: ModelChoice) -> MemResult<Vec<f32>> {
    let mut out = embed_batch(&[text], role, model)?;
    Ok(out.pop().expect("embed_batch returns one vec per input"))
}

/// Embed many texts under one role. Initialises the model once for the whole batch.
pub fn embed_batch(texts: &[&str], role: Role, model: ModelChoice) -> MemResult<Vec<Vec<f32>>> {
    let prefixed: Vec<String> = texts.iter().map(|t| apply_prefix(t, role)).collect();
    let mut te = store::init(model)?;
    te.embed(prefixed, None).map_err(|e| MemError::EmbedderInferenceError(e.to_string()))
}
