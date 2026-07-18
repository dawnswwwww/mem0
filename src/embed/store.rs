//! Per-invocation TextEmbedding initialisation + model path resolution.

use crate::core::error::{MemError, MemResult};
use crate::embed::ModelChoice;

/// Initialise the model for this invocation. Path resolution + caching lands in Task 6;
/// for now this uses fastembed's download/cache path.
pub fn init(model: ModelChoice) -> MemResult<fastembed::TextEmbedding> {
    // Task 6 replaces this body with resolve_model_path() -> sidecar | cache | download.
    let opts = fastembed::TextInitOptions::new(model.to_fastembed())
        .with_show_download_progress(true);
    fastembed::TextEmbedding::try_new(opts).map_err(|e| MemError::EmbedderInitError(e.to_string()))
}
