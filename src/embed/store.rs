//! Per-invocation TextEmbedding initialisation + model cache-dir resolution (spec §6).
//!
//! Sidecar strategy: ship a *pre-populated fastembed cache dir* beside the binary
//! and point fastembed at it via `TextInitOptions::with_cache_dir`. We do NOT use
//! `UserDefinedEmbeddingModel` (it takes raw bytes, awkward for a file sidecar).

use std::path::PathBuf;

use crate::core::error::{MemError, MemResult};
use crate::embed::ModelChoice;

/// Ordered candidate cache dirs to search for a pre-populated model.
pub struct SearchRoots {
    pub roots: Vec<PathBuf>,
}

impl SearchRoots {
    /// 1. `$MEM0_EMBED_MODEL_DIR`
    /// 2. `<exe_dir>/models`
    /// 3. `<cache_dir>/mem0/fastembed`
    pub fn from_env() -> Self {
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Ok(d) = std::env::var("MEM0_EMBED_MODEL_DIR") && !d.is_empty() {
            roots.push(PathBuf::from(d));
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(exe_dir) = exe.parent()
        {
            roots.push(exe_dir.join("models"));
        }
        if let Some(cache) = dirs::cache_dir() {
            roots.push(cache.join("mem0").join("fastembed"));
        }
        SearchRoots { roots }
    }
}

/// HuggingFace cache subdir name for a repo: "Qdrant/x" -> "models--Qdrant--x".
pub fn hf_cache_subdir(repo: &str) -> String {
    format!("models--{}", repo.replace('/', "--"))
}

/// Return the first root whose `<root>/<hf_cache_subdir(repo)>/` exists (the cache
/// dir to hand to `with_cache_dir`), else `None`.
pub fn resolve(model: ModelChoice, roots: &SearchRoots) -> Option<PathBuf> {
    let subdir = hf_cache_subdir(model.repo());
    roots.roots.iter().find(|r| r.join(&subdir).is_dir()).cloned()
}

/// Initialise the model for this invocation. If a sidecar cache dir resolves, point
/// fastembed at it and pin hf-hub to it offline (no network); otherwise fall back to
/// fastembed's default download.
pub fn init(model: ModelChoice) -> MemResult<fastembed::TextEmbedding> {
    let opts = fastembed::TextInitOptions::new(model.to_fastembed())
        .with_show_download_progress(true);

    if let Some(dir) = resolve(model, &SearchRoots::from_env()) {
        // A sidecar resolved: make it authoritative and fully offline. Without this,
        // hf-hub would (a) hit the HF API to resolve the revision and (b) let
        // `$HF_HOME` override `with_cache_dir` — either can hang/fail on a
        // firewalled machine even though the model is right here.
        // SAFETY: mem0 is single-threaded at CLI startup, before any embed work; no
        // concurrent getenv can race these mutations.
        unsafe {
            std::env::set_var("HF_HUB_OFFLINE", "1");
            std::env::remove_var("HF_HOME");
        }
        return fastembed::TextEmbedding::try_new(opts.with_cache_dir(dir))
            .map_err(|e| MemError::EmbedderInitError(e.to_string()));
    }

    fastembed::TextEmbedding::try_new(opts).map_err(|e| MemError::EmbedderInitError(e.to_string()))
}
