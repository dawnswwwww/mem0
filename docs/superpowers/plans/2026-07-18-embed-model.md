# Built-in Embedding Model (fastembed-rs) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in, local, CPU text-embedding capability to mem0 (macOS/Linux/Windows) so `add`/`vsearch` can produce vectors themselves; the v1.2 caller-supplied-vector path stays the highest-precedence source and the store layer is untouched.

**Architecture:** A new feature-gated `src/embed/` module wraps `fastembed-rs` (sync ONNX runtime). It only produces `Vec<f32>`; the cli layer feeds those into the existing v1.2 `store::vectors::upsert`/`search`. New `--embed`/`--no-embed`/`--model` flags and an `embed` subcommand are always declared in clap; when the feature is off they return `EmbedFeatureNotEnabled`. When on, `add`/`vsearch` auto-embed by default (overridable by `--no-embed`, a piped stdin vector, or `MEM0_EMBED=off`). The default model `multilingual-e5-small` (384-dim) ships as a sidecar file beside the binary (`<exe_dir>/models` → cache dir → lazy-download fallback).

**Tech Stack:** Rust (edition 2024), `fastembed = "5"` (optional; pulls `ort` + `tokenizers`), `dirs = "5"`, clap, serde_json, existing `rusqlite`/`sqlite-vec`. fastembed-rs API surface verified by Task 1's spike.

**Spec:** `docs/superpowers/specs/2026-07-18-embed-model-design.md`. Where this plan refines the spec (noted inline), the plan is authoritative — it reflects the verified API.

---

## Global Constraints

- **Default build unchanged.** `cargo build` (no `--features embed`) compiles and behaves byte-for-byte like v1.2; `fastembed` is NOT linked. All v1.2 tests stay green.
- **Feature name:** `embed`. Declared in `[features] embed = ["dep:fastembed"]`.
- **No `store/` changes.** The embed module produces `Vec<f32>` only. No new migration, no new dimension logic — the v1.2 `meta.embedding_dim` lock is reused.
- **No async.** fastembed-rs is synchronous; do not introduce Tokio.
- **Cross-platform CPU.** macOS (aarch64 + x86_64), Linux, Windows. CPU execution provider only (no `directml`/`qwen3`/`nomic-v2-moe` features).
- **Default model:** `multilingual-e5-small` → `fastembed::EmbeddingModel::MultilingualE5Small`, 384-dim.
- **e5 prefix is mandatory and centralized:** storage = `passage: `, query = `query: `, applied inside `embed::` only.
- **Exit codes:** new errors all map to exit 2 (see Task 3).
- **Commits:** one logical commit per task; message prefix `feat(embed):` / `test(embed):` / `docs(embed):`.

---

### Task 1: Spike — verify fastembed-rs API

**Goal:** Confirm the exact fastembed-rs API surface before building on it, mirroring how the v1.2 plan verified sqlite-vec. Record outcomes in the "Spike outcome" block below (edit this file).

**Files:**
- Create (throwaway, not committed): `/tmp/mem0-spike/Cargo.toml` and `/tmp/mem0-spike/src/main.rs`

- [x] **Step 1: Create a throwaway crate outside the repo**

```bash
mkdir -p /tmp/mem0-spike/src
```

`/tmp/mem0-spike/Cargo.toml`:
```toml
[package]
name = "mem0-spike"
version = "0.0.0"
edition = "2024"

[dependencies]
fastembed = "5"
```

`/tmp/mem0-spike/src/main.rs`:
```rust
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

fn main() -> anyhow::Result<()> {
    // 1. Confirm the non-deprecated init options type + builder methods.
    let mut model = TextEmbedding::try_new(
        TextInitOptions::new(EmbeddingModel::MultilingualE5Small)
            .with_show_download_progress(true),
    )?;
    // 2. Confirm embed() signature (note: &mut self) and the e5 prefix behaviour.
    let out = model.embed(vec!["query: hello", "passage: world"], None)?;
    println!("n={} dim={}", out.len(), out[0].len());
    assert_eq!(out[0].len(), 384, "e5-small must be 384-dim");
    // 3. List whether a quantized variant exists.
    let names: Vec<&str> = TextEmbedding::list_supported_models()
        .into_iter().map(|m| m.model.as_str()).collect();
    for n in names.iter().filter(|n| n.contains("multilingual-e5")) {
        println!("supported: {n}");
    }
    Ok(())
}
```

- [x] **Step 2: Run it**

Run: `cd /tmp/mem0-spike && cargo run`
Expected: prints `n=2 dim=384` and a list containing `intfloat/multilingual-e5-small` (and notes whether `…-small-Q` / a quantized form appears). First run downloads the model (~120 MB) into the HF cache.

- [x] **Step 3: Verify the cache-dir offline approach + repo ids**

Research finding: fastembed's `UserDefinedEmbeddingModel` takes **raw bytes** (`&[u8]` + a
`TokenizerFiles` struct), not paths — so byte-loading a sidecar is awkward. Instead we ship
a **pre-populated fastembed cache dir** beside the binary and pass it via
`TextInitOptions::with_cache_dir`. Verify this works offline:

1. Step 2 already primed the cache. Find where: `ls -d .fastembed_cache/models--* 2>/dev/null
   || ls -d "$HF_HOME"/models--* 2>/dev/null` — record the exact subdir name for
   multilingual-e5-small. **Spike-verified: `models--intfloat--multilingual-e5-small`** (fastembed's `model_code` for `MultilingualE5Small` is `intfloat/multilingual-e5-small`, not `Qdrant/…`).
2. Replace `main.rs`'s body with an offline-from-cache run:

```rust
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
fn main() -> anyhow::Result<()> {
    let cache = std::path::PathBuf::from("./.fastembed_cache"); // the primed dir from step 1
    let mut model = TextEmbedding::try_new(
        TextInitOptions::new(EmbeddingModel::MultilingualE5Small).with_cache_dir(cache),
    )?;
    let out = model.embed(vec!["query: hi"], None)?;
    println!("offline ok, dim={}", out[0].len());
    Ok(())
}
```

Run: `cd /tmp/mem0-spike && HF_HUB_OFFLINE=1 cargo run`
Expected: prints `offline ok, dim=384` with **no network access**. Record the exact cache
subdir name for Task 6 + packaging. If `HF_HUB_OFFLINE=1` still hits the network, record
the working offline incantation (the sidecar depends on it).

- [x] **Step 4: Record outcomes — fill in this block**

Edit this plan file, replacing the Spike outcome table (below) with the verified values. Do NOT proceed to Task 2 until the table is filled with concrete answers (not guesses).

- [x] **Step 5: Clean up**

Run: `rm -rf /tmp/mem0-spike`

### Spike outcome (fastembed-rs — VERIFIED in Task 1, 2026-07-18)

Spike crate `/tmp/mem0-spike` ran green; offline embed confirmed. Resolved
versions: `fastembed 5.17.3`, `ort 2.0.0-rc.12`, `hf-hub 0.5.0`,
`tokenizers 0.22.2`. Full report: `.superpowers/sdd/task-1-report.md`.

| Concern | Verified answer |
|---|---|
| crate / version | `fastembed = "5"` → **resolves to 5.17.3** (`ort 2.0.0-rc.12`, `hf-hub 0.5.0`, `tokenizers 0.22.2`). |
| init type | `TextInitOptions = InitOptionsWithLength<EmbeddingModel>`. `TextInitOptions::new(model).with_show_download_progress(bool).with_cache_dir(PathBuf)` — **both builders confirmed**. Also `.with_max_length(usize)` / `.with_intra_threads(usize)` / `.with_execution_providers(...)` exist (unused here). Default `cache_dir = FASTEMBED_CACHE_DIR ?? ".fastembed_cache"`. |
| load model | `TextEmbedding::try_new(TextInitOptions) -> Result<TextEmbedding>` — confirmed. **Sidecar strategy = ship a pre-populated cache dir, point `with_cache_dir` at it** — NOT `UserDefinedEmbeddingModel` (which takes `onnx_file: Vec<u8>` + `TokenizerFiles`, awkward for a file sidecar). |
| cache-dir offline | **Confirmed.** `HF_HUB_OFFLINE=1` + `with_cache_dir(primed_dir)` loads with **zero network** and embeds successfully (`offline ok, n=1 dim=384`). **Caveat:** `HF_HOME`, if set, *overrides* `with_cache_dir(...)` (fastembed `pull_from_hf` prefers `HF_HOME`). Document this in the packaging note; the CLI sidecar path assumes `HF_HOME` is unset. |
| cache subdir (default model) | **`models--intfloat--multilingual-e5-small`** (NOT `models--Qdrant--…`). Snapshot revision `614241f622f53c4eeff9890bdc4f31cfecc418b3`. Files fastembed fetches: `onnx/model.onnx` (470 MB), `tokenizer.json` (17 MB), `config.json`, `special_tokens_map.json`, `tokenizer_config.json`. |
| embed | `pub fn embed<S: AsRef<str> + Send + Sync>(&mut self, texts: impl AsRef<[S]>, batch_size: Option<usize>) -> Result<Vec<Embedding>>` where `pub type Embedding = Vec<f32>;`. **`&mut self` confirmed.** Accepts `Vec<&str>` or `Vec<String>`. e5 prefix is NOT added by fastembed — caller must prepend. |
| default features | Plain `fastembed = "5"` **enables network download** — `default = ["ort-download-binaries-native-tls", "hf-hub-native-tls", "image-models"]`. No extra feature needed for download; CPU execution provider only (no `directml`/`cuda`/`mkl`). |
| quantized | **`MultilingualE5SmallQ` does NOT exist in fastembed 5.17.3.** The `MultilingualE5*` family is exactly `MultilingualE5Small` (384) / `MultilingualE5Base` (768) / `MultilingualE5Large` (1024) — none have a `Q` variant. The "ship quantized to halve sidecar size" idea is **not actionable** for the default model with this version. (Q-variants DO exist for `AllMiniLML6V2Q`, `BGESmallENV15Q`, `NomicEmbedTextV15Q`, `SnowflakeArcticEmbed*Q`, etc.) |
| Send+Sync / static | `TextEmbedding` is auto `Send + Sync` (`ort::session::SharedSessionInner: Send + Sync`). We still init-per-call per the refinement below — no `OnceLock<&mut>` problem. |

**HF repo (`model_code`) strings — VERIFIED from `src/models/text_embedding.rs`** (the field is `model_code`, not `model`/`repo()`; the onnx filename is `model_file`):

| `ModelChoice` | fastembed variant | `model_code` (HF repo) | `model_file` | cache subdir |
|---|---|---|---|---|
| MultilingualE5Small (default) | `MultilingualE5Small` | **`intfloat/multilingual-e5-small`** | `onnx/model.onnx` | `models--intfloat--multilingual-e5-small` |
| AllMiniLML6V2 | `AllMiniLML6V2` | **`Qdrant/all-MiniLM-L6-v2-onnx`** | `model.onnx` | `models--Qdrant--all-MiniLM-L6-v2-onnx` |
| BGESmallENV15 | `BGESmallENV15` | **`Xenova/bge-small-en-v1.5`** | `onnx/model.onnx` | `models--Xenova--bge-small-en-v1.5` |
| BGESmallZHV15 | `BGESmallZHV15` | **`Xenova/bge-small-zh-v1.5`** | `onnx/model.onnx` | `models--Xenova--bge-small-zh-v1.5` |
| NomicEmbedTextV15 | `NomicEmbedTextV15` | `nomic-ai/nomic-embed-text-v1.5` | `onnx/model.onnx` | `models--nomic-ai--nomic-embed-text-v1.5` |

**⚠ Plan corrections applied downstream** (Tasks 4/6/10): the previous draft's
`repo()` strings guessed `Qdrant/…` for 4 of 5 models; the verified values
above are authoritative (4 differ). Task 4's `repo()` match-arms, Task 6's
`hf_cache_subdir` unit test, and Task 10's packaging doc have been updated to
use `intfloat/multilingual-e5-small` for the default.

**Refinement vs spec §3 (plan-authoritative):** fastembed's `embed()` is `&mut self`, so a `OnceLock`-stored singleton borrows awkwardly. Since the CLI is stateless (one embed per invocation), `embed::embed_text`/`embed_batch` **initialise the model per call/batch** instead of holding a process-wide singleton. For `add`/`vsearch` (one embed each) this is one init per invocation — same cost, no `&mut`-in-static problem. The spec's "singleton" intent (don't re-init within a batch) is still satisfied because `embed_batch` inits once for the whole batch.

---

## File Structure

| File | Responsibility | Status |
|---|---|---|
| `Cargo.toml` | add optional `fastembed = "5"`; `[features] embed = ["dep:fastembed"]` | modify |
| `src/lib.rs` | declare `embed` module behind `#[cfg(feature = "embed")]` | modify |
| `src/core/error.rs` | `EmbedderInitError`, `EmbedderInferenceError`, `EmbedFeatureNotEnabled` | modify |
| `src/cli/mod.rs` | `Command::Embed`; `exit_code_for` branches for new errors | modify |
| `src/output/format.rs` | `error_json` arms for new errors | modify |
| `src/embed/mod.rs` | `Role`, `apply_prefix`, public `embed_text`/`embed_batch` | **create** (feature-gated) |
| `src/embed/model.rs` | `ModelChoice`: name ↔ `EmbeddingModel`, default `MultilingualE5Small` | **create** (feature-gated) |
| `src/embed/store.rs` | `SearchRoots`, `resolve`, `hf_cache_subdir`, `init` (cache-dir resolution + per-call init) | **create** (feature-gated) |
| `src/cli/embed.rs` | `embed` subcommand: text → `{"embedding":…}` (feature-gated body, always declared) | **create** |
| `src/cli/add.rs` | `--embed`/`--no-embed`/`--model` + §5 precedence | modify |
| `src/cli/vsearch.rs` | optional positional `<QUERY>` + `--embed`/`--no-embed`/`--model` + precedence | modify |
| `.claude/skills/mem0/SKILL.md` | document auto-embed default, opt-outs, sidecar location | modify |
| `tests/embed_unit.rs` | prefix + model-mapping + path-resolution unit tests (no network) | **create** |
| `tests/cli_embed_feature_off.rs` | `--embed`/`embed` without feature → exit 2 | **create** |
| `tests/cli_embed.rs` | e2e auto-embed round-trip (`#[ignore]`, network) | **create** |

`src/store/*` is **not modified** by any task.

---

### Task 2: Feature gate + module scaffold

**Goal:** Add the optional dependency and feature, and create the feature-gated `embed` module skeleton so both build configurations compile. No behaviour yet.

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Create: `src/embed/mod.rs`
- Test: `cargo build` (default) and `cargo build --features embed` both succeed.

**Interfaces:**
- Produces: `crate::embed` module (empty stub) visible only under `feature = "embed"`.

- [ ] **Step 1: Add the optional dependency and feature to `Cargo.toml`**

In the `[dependencies]` table, add:
```toml
fastembed = { version = "5", optional = true }
```
At the end of the file, add:
```toml
[features]
embed = ["dep:fastembed"]
```

- [ ] **Step 2: Declare the feature-gated module in `src/lib.rs`**

Change the module declarations (currently lines 1–4) to:
```rust
pub mod cli;
pub mod core;
pub mod output;
pub mod store;
#[cfg(feature = "embed")]
pub mod embed;
```

- [ ] **Step 3: Create the stub module `src/embed/mod.rs`**

```rust
//! Local CPU text embedding (opt-in `embed` feature).
//!
//! Produces `Vec<f32>` only; the cli layer feeds results into the existing
//! v1.2 `store::vectors` path. This module has no dependency on `store`.
```

- [ ] **Step 4: Verify the default build is unchanged**

Run: `cargo build`
Expected: compiles; `fastembed` is NOT pulled in (confirm with `cargo tree | grep -c fastembed` → `0`).

- [ ] **Step 5: Verify the feature build compiles**

Run: `cargo build --features embed`
Expected: compiles (fastembed + ort + tokenizers are fetched/linked).

- [ ] **Step 6: Confirm v1.2 tests still pass on the default build**

Run: `cargo test`
Expected: all existing tests pass (no regressions — nothing functional changed yet).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/embed/mod.rs
git commit -m "feat(embed): scaffold opt-in embed feature + module"
```

---

### Task 3: Error variants, exit codes, error_json

**Goal:** Add the three new error variants and wire them into `exit_code_for` and `error_json`. Feature-agnostic — these exist in both builds (so the always-declared `--embed` flag can return `EmbedFeatureNotEnabled` even when the feature is off).

**Files:**
- Modify: `src/core/error.rs`
- Modify: `src/cli/mod.rs:55-67` (`exit_code_for`)
- Modify: `src/output/format.rs:51-65` (`error_json`)
- Test: `src/core/error.rs` (append unit tests)

**Interfaces:**
- Produces: `MemError::EmbedderInitError(String)`, `MemError::Inference(String)`-style variants, `MemError::EmbedFeatureNotEnabled`, all mapping to exit code 2.

- [ ] **Step 1: Write the failing test**

Append to the `#[cfg(test)] mod tests` block in `src/core/error.rs`:
```rust
    #[test]
    fn embed_errors_display() {
        assert!(MemError::EmbedderInitError("boom".into())
            .to_string().contains("boom"));
        assert!(MemError::EmbedderInferenceError("x".into())
            .to_string().contains("x"));
        assert!(MemError::EmbedFeatureNotEnabled
            .to_string().contains("not compiled in"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib core::error::tests::embed_errors_display`
Expected: FAIL — variants do not exist.

- [ ] **Step 3: Add the variants to `MemError`**

In `src/core/error.rs`, add these three variants inside `enum MemError` (after `VectorNotInitialized`, before the closing brace):
```rust
    #[error("embedder init failed: {0}")]
    EmbedderInitError(String),

    #[error("embedder inference failed: {0}")]
    EmbedderInferenceError(String),

    #[error("embedding support is not compiled in (rebuild with --features embed)")]
    EmbedFeatureNotEnabled,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib core::error::tests::embed_errors_display`
Expected: PASS.

- [ ] **Step 5: Add exit-code branches in `src/cli/mod.rs`**

In `exit_code_for` (currently lines 55–67), add explicit arms. Replace the function body's match so it includes (keep all existing arms, add these three before the `_ => 1` fallback):
```rust
        MemError::EmbedderInitError(_)        => 2,
        MemError::EmbedderInferenceError(_)   => 2,
        MemError::EmbedFeatureNotEnabled      => 2,
```

- [ ] **Step 6: Add error_json arms in `src/output/format.rs`**

In `error_json`'s match (currently lines 52–63), add arms for the new variants (keep existing arms):
```rust
        MemError::EmbedderInitError(_)        => "EmbedderInitError",
        MemError::EmbedderInferenceError(_)   => "EmbedderInferenceError",
        MemError::EmbedFeatureNotEnabled      => "EmbedFeatureNotEnabled",
```

- [ ] **Step 7: Verify everything compiles and passes on the default build**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/core/error.rs src/cli/mod.rs src/output/format.rs
git commit -m "feat(embed): add embedder error variants + exit codes"
```

---

### Task 4: `embed::model` — name ↔ enum mapping

**Goal:** Map `--model` string names to `fastembed::EmbeddingModel` variants, defaulting to `multilingual-e5-small`. Pure data — no model loading, no network — so it is fully unit-testable.

**Files:**
- Create: `src/embed/model.rs`
- Modify: `src/embed/mod.rs` (declare submodule)
- Test: `tests/embed_unit.rs` (create; gated so it only compiles/runs under the feature)

**Interfaces:**
- Produces: `pub enum ModelChoice { MultilingualE5Small, AllMiniLML6V2, BGESmallENV15, BGESmallZHV15, NomicEmbedTextV15 }`, `ModelChoice::DEFAULT`, `ModelChoice::from_name(&str) -> MemResult<ModelChoice>`, `ModelChoice::name(&self) -> &'static str`, `ModelChoice::dim(&self) -> usize`, `ModelChoice::to_fastembed(&self) -> fastembed::EmbeddingModel`.

- [ ] **Step 1: Write the failing test**

Create `tests/embed_unit.rs`:
```rust
#![cfg(feature = "embed")]

use mem0::embed::model::ModelChoice;

#[test]
fn default_is_multilingual_e5_small_384() {
    let m = ModelChoice::DEFAULT;
    assert_eq!(m.name(), "multilingual-e5-small");
    assert_eq!(m.dim(), 384);
}

#[test]
fn name_roundtrip_for_known_models() {
    for name in [
        "multilingual-e5-small",
        "all-MiniLM-L6-v2",
        "bge-small-en-v1.5",
        "bge-small-zh-v1.5",
        "nomic-embed-text-v1.5",
    ] {
        let m = ModelChoice::from_name(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(m.name(), name);
    }
}

#[test]
fn unknown_model_errors() {
    assert!(ModelChoice::from_name("gpt-4").is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features embed --test embed_unit`
Expected: FAIL — `embed::model` does not exist.

- [ ] **Step 3: Create `src/embed/model.rs`**

```rust
//! Maps `--model` names to fastembed variants. Default = multilingual-e5-small (384-dim).

use crate::core::error::{MemError, MemResult};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ModelChoice {
    MultilingualE5Small,
    AllMiniLML6V2,
    BGESmallENV15,
    BGESmallZHV15,
    NomicEmbedTextV15,
}

impl ModelChoice {
    pub const DEFAULT: ModelChoice = ModelChoice::MultilingualE5Small;

    pub fn name(&self) -> &'static str {
        match self {
            ModelChoice::MultilingualE5Small => "multilingual-e5-small",
            ModelChoice::AllMiniLML6V2       => "all-MiniLM-L6-v2",
            ModelChoice::BGESmallENV15       => "bge-small-en-v1.5",
            ModelChoice::BGESmallZHV15       => "bge-small-zh-v1.5",
            ModelChoice::NomicEmbedTextV15   => "nomic-embed-text-v1.5",
        }
    }

    pub fn dim(&self) -> usize {
        match self {
            ModelChoice::MultilingualE5Small => 384,
            ModelChoice::AllMiniLML6V2       => 384,
            ModelChoice::BGESmallENV15       => 384,
            ModelChoice::BGESmallZHV15       => 512,
            ModelChoice::NomicEmbedTextV15   => 768,
        }
    }

    pub fn from_name(s: &str) -> MemResult<ModelChoice> {
        match s {
            "multilingual-e5-small"  => Ok(ModelChoice::MultilingualE5Small),
            "all-MiniLM-L6-v2"       => Ok(ModelChoice::AllMiniLML6V2),
            "bge-small-en-v1.5"      => Ok(ModelChoice::BGESmallENV15),
            "bge-small-zh-v1.5"      => Ok(ModelChoice::BGESmallZHV15),
            "nomic-embed-text-v1.5"  => Ok(ModelChoice::NomicEmbedTextV15),
            other => Err(MemError::InvalidArgument(format!(
                "unknown model '{other}'; supported: multilingual-e5-small, \
                 all-MiniLM-L6-v2, bge-small-en-v1.5, bge-small-zh-v1.5, nomic-embed-text-v1.5"
            ))),
        }
    }

    /// Slug used for the sidecar directory name, e.g. `multilingual-e5-small`.
    pub fn slug(&self) -> &'static str {
        self.name()
    }

    /// HuggingFace repo fastembed downloads from (the fastembed `model_code`
    /// field, verified in Task 1). Used to detect a pre-populated sidecar cache
    /// subdir (`models--<org>--<name>`).
    pub fn repo(&self) -> &'static str {
        match self {
            ModelChoice::MultilingualE5Small => "intfloat/multilingual-e5-small",
            ModelChoice::AllMiniLML6V2       => "Qdrant/all-MiniLM-L6-v2-onnx",
            ModelChoice::BGESmallENV15       => "Xenova/bge-small-en-v1.5",
            ModelChoice::BGESmallZHV15       => "Xenova/bge-small-zh-v1.5",
            ModelChoice::NomicEmbedTextV15   => "nomic-ai/nomic-embed-text-v1.5",
        }
    }

    pub fn to_fastembed(&self) -> fastembed::EmbeddingModel {
        match self {
            ModelChoice::MultilingualE5Small => fastembed::EmbeddingModel::MultilingualE5Small,
            ModelChoice::AllMiniLML6V2       => fastembed::EmbeddingModel::AllMiniLML6V2,
            ModelChoice::BGESmallENV15       => fastembed::EmbeddingModel::BGESmallENV15,
            ModelChoice::BGESmallZHV15       => fastembed::EmbeddingModel::BGESmallZHV15,
            ModelChoice::NomicEmbedTextV15   => fastembed::EmbeddingModel::NomicEmbedTextV15,
        }
    }
}
```

> **Spike-confirmed (Task 1):** all five `fastembed::EmbeddingModel::*` variant names above exist in 5.17.3. The `repo()` strings are the verified fastembed `model_code` values (4 of 5 differ from the original draft — see the Spike outcome table). **No `MultilingualE5SmallQ` variant exists** in this fastembed version, so no quantized arm is added for the default model.

- [ ] **Step 4: Declare the submodule in `src/embed/mod.rs`**

```rust
//! Local CPU text embedding (opt-in `embed` feature).
//!
//! Produces `Vec<f32>` only; the cli layer feeds results into the existing
//! v1.2 `store::vectors` path. This module has no dependency on `store`.

pub mod model;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --features embed --test embed_unit`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add src/embed/model.rs src/embed/mod.rs tests/embed_unit.rs
git commit -m "feat(embed): model name <-> fastembed variant mapping"
```

---

### Task 5: `embed::mod` — Role + prefix + public embed API

**Goal:** Implement the asymmetric e5 prefix (`passage:` / `query:`) and the public `embed_text`/`embed_batch` entry points. Prefix logic is pure and unit-tested without any model.

**Files:**
- Modify: `src/embed/mod.rs`
- Modify: `src/embed/store.rs` (create — minimal `init` used by this task; full path logic lands in Task 6)
- Test: `tests/embed_unit.rs` (append)

**Interfaces:**
- Produces: `pub enum Role { Passage, Query }`, `pub fn embed_text(text: &str, role: Role, model: ModelChoice) -> MemResult<Vec<f32>>`, `pub fn embed_batch(texts: &[&str], role: Role, model: ModelChoice) -> MemResult<Vec<Vec<f32>>>`.
- Consumes: `ModelChoice` (Task 4); `crate::embed::store::init` (this task creates a minimal version).

- [ ] **Step 1: Write the failing test**

Append to `tests/embed_unit.rs`:
```rust
use mem0::embed::{Role, apply_prefix};

#[test]
fn prefix_is_asymmetric() {
    assert_eq!(apply_prefix("hello", Role::Passage), "passage: hello");
    assert_eq!(apply_prefix("hello", Role::Query),   "query: hello");
}

#[test]
fn prefix_trims_only_leading_whitespace_of_input_not_added() {
    // input is taken verbatim after the prefix; prefix is exactly "passage: " / "query: "
    assert_eq!(apply_prefix("  spaced", Role::Query), "query:   spaced");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features embed --test embed_unit`
Expected: FAIL — `Role`/`apply_prefix` do not exist.

- [ ] **Step 3: Implement prefix + Role + public API in `src/embed/mod.rs`**

Replace the file contents with:
```rust
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
```

- [ ] **Step 4: Create minimal `src/embed/store.rs` (full path logic in Task 6)**

```rust
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
```

> **Spike-dependent:** confirm `TextInitOptions::new(...).with_show_download_progress(bool)` and `TextEmbedding::try_new(...)` are the verified names (Task 1). Adjust if the spike found different names.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --features embed --test embed_unit`
Expected: PASS (5 tests; the two prefix tests + 3 from Task 4).

- [ ] **Step 6: Verify default build still compiles (embed module is feature-gated)**

Run: `cargo build`
Expected: compiles.

- [ ] **Step 7: Commit**

```bash
git add src/embed/mod.rs src/embed/store.rs tests/embed_unit.rs
git commit -m "feat(embed): Role/prefix + embed_text/embed_batch API"
```

---

### Task 6: `embed::store` — sidecar model path resolution

**Goal:** Resolve the model from sidecar → cache → lazy-download (spec §6). The path-resolution logic is pure (testable with temp dirs, no network); the actual init is unchanged behaviour.

**Files:**
- Modify: `src/embed/store.rs`
- Test: `tests/embed_unit.rs` (append)

**Interfaces:**
- Produces: `pub struct SearchRoots`, `pub fn resolve(model, &SearchRoots) -> Option<PathBuf>` (first cache root containing the model's HF subdir), `pub fn hf_cache_subdir(repo) -> String`, `pub fn init(model)` (points fastembed at the resolved cache dir via `with_cache_dir`, else falls back to download).

- [ ] **Step 1: Write the failing test**

Append to `tests/embed_unit.rs`:
```rust
use mem0::embed::store::{resolve, SearchRoots, hf_cache_subdir};

#[test]
fn hf_cache_subdir_transform() {
    // Default model repo is intfloat/multilingual-e5-small (spike-verified).
    assert_eq!(hf_cache_subdir("intfloat/multilingual-e5-small"),
               "models--intfloat--multilingual-e5-small");
}

#[test]
fn resolve_returns_first_root_with_the_model_subdir() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let subdir = hf_cache_subdir(ModelChoice::DEFAULT.repo());
    // 'b' contains the model's cache subdir; 'a' does not.
    std::fs::create_dir_all(b.path().join(&subdir)).unwrap();
    let roots = SearchRoots { roots: vec![a.path().to_path_buf(), b.path().to_path_buf()] };
    // resolve returns the ROOT (passed to with_cache_dir), not the model subdir.
    assert_eq!(resolve(ModelChoice::DEFAULT, &roots), Some(b.path().to_path_buf()));
}

#[test]
fn resolve_none_when_no_root_has_the_subdir() {
    let a = tempfile::tempdir().unwrap();
    let roots = SearchRoots { roots: vec![a.path().to_path_buf()] };
    assert_eq!(resolve(ModelChoice::DEFAULT, &roots), None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --features embed --test embed_unit`
Expected: FAIL — `SearchRoots`/`resolve` do not exist.

- [ ] **Step 3: Implement cache-dir resolution in `src/embed/store.rs`**

Replace `src/embed/store.rs` with:
```rust
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
        if let Ok(d) = std::env::var("MEM0_EMBED_MODEL_DIR") {
            if !d.is_empty() {
                roots.push(PathBuf::from(d));
            }
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
/// fastembed at it (offline); otherwise fall back to fastembed's default download.
pub fn init(model: ModelChoice) -> MemResult<fastembed::TextEmbedding> {
    let cache_dir = resolve(model, &SearchRoots::from_env());
    let mut opts = fastembed::TextInitOptions::new(model.to_fastembed())
        .with_show_download_progress(true);
    if let Some(dir) = cache_dir {
        opts = opts.with_cache_dir(dir);
    }
    fastembed::TextEmbedding::try_new(opts).map_err(|e| MemError::EmbedderInitError(e.to_string()))
}
```

> **Spike-confirmed (Task 1):** `TextInitOptions::with_cache_dir(PathBuf)` exists and `HF_HUB_OFFLINE=1` + `with_cache_dir(primed_dir)` loads with **no network** (verified: `offline ok, n=1 dim=384`). The HF cache subdir transform (`models--<org>--<name>`) is standard; the default model's repo is **`intfloat/multilingual-e5-small`** → subdir `models--intfloat--multilingual-e5-small`. **Caveat:** fastembed's `pull_from_hf` prefers `HF_HOME` over the `with_cache_dir` value, so if `HF_HOME` is set the sidecar is bypassed — acceptable (the download fallback still works); document in the packaging note. The download-fallback path (no `with_cache_dir`) is correct regardless and is what the `#[ignore]` e2e tests exercise.

- [ ] **Step 4: Add `tempfile` to dev-dependencies if missing**

Check `Cargo.toml` `[dev-dependencies]` already lists `tempfile = "3"` (it does, per v1.2). No change needed; confirm with `grep tempfile Cargo.toml`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --features embed --test embed_unit`
Expected: PASS (8 tests).

- [ ] **Step 6: Commit**

```bash
git add src/embed/store.rs tests/embed_unit.rs
git commit -m "feat(embed): sidecar cache-dir resolution (env/exe/cache)"
```

---

### Task 7: `embed` subcommand

**Goal:** Add `mem0 embed [TEXT]...` that prints `{"embedding":[...],"dim":N,"model":"..."}`. The subcommand is always declared; when the feature is off it returns `EmbedFeatureNotEnabled`. Default role is Query; `--as-passage` switches to Passage.

**Files:**
- Create: `src/cli/embed.rs`
- Modify: `src/cli/mod.rs` (declare module + `Command::Embed` + dispatch)
- Test: `tests/cli_embed.rs` (create, `#[ignore]` e2e under feature), `tests/cli_embed_feature_off.rs` (create, default build)

**Interfaces:**
- Produces: `crate::cli::embed::{Args, run}`.
- Consumes: `crate::embed::{embed_text, Role, ModelChoice}` (feature-gated).

- [ ] **Step 1: Write the failing test (feature-off guard)**

Create `tests/cli_embed_feature_off.rs` (runs on the default build, no feature):
```rust
// Compiles WITHOUT --features embed; the subcommand exists but errors.
use assert_cmd::Command;

#[test]
fn embed_without_feature_exits_2() {
    let mut cmd = Command::cargo_bin("mem0").unwrap();
    let out = cmd.args(["embed", "hello"]).unwrap_err();
    // assert_cmd treats non-zero exit as Err; the exit code is on the Output.
    let code = out.code().unwrap_or(0);
    assert_eq!(code, 2, "expected exit 2 (EmbedFeatureNotEnabled)");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test cli_embed_feature_off`
Expected: FAIL — `embed` subcommand does not exist yet (clap error, not exit 2).

- [ ] **Step 3: Create `src/cli/embed.rs`**

```rust
use std::io::{IsTerminal, Read};

use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::{MemError, MemResult};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Text to embed. If omitted, reads from stdin.
    pub text: Vec<String>,

    /// Embed as a passage (`passage:` prefix) instead of a query (`query:`).
    #[arg(long)]
    pub as_passage: bool,

    /// Override the default model (multilingual-e5-small).
    #[arg(long)]
    pub model: Option<String>,
}

pub fn run(_conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    // Gather text: positional args, else stdin if piped.
    let text = if !args.text.is_empty() {
        args.text.join(" ")
    } else {
        let mut stdin = std::io::stdin();
        if stdin.is_terminal() {
            return Err(MemError::InvalidArgument(
                "embed needs text: pass args or pipe text on stdin".into(),
            ));
        }
        let mut raw = String::new();
        stdin.read_to_string(&mut raw)?;
        raw.trim_end().to_string()
    };
    if text.is_empty() {
        return Err(MemError::InvalidArgument("embed text is empty".into()));
    }

    // `--json` is accepted for CLI parity; the embed command always emits a JSON
    // object, so the flag is intentionally unused here.
    let _ = json;

    #[cfg(not(feature = "embed"))]
    {
        let _ = (args.as_passage, args.model);
        return Err(MemError::EmbedFeatureNotEnabled);
    }

    #[cfg(feature = "embed")]
    {
        use crate::embed::{embed_text, ModelChoice, Role};
        let model = match args.model.as_deref() {
            Some(name) => ModelChoice::from_name(name)?,
            None => ModelChoice::DEFAULT,
        };
        let role = if args.as_passage { Role::Passage } else { Role::Query };
        let vec = embed_text(&text, role, model)?;
        let payload = serde_json::json!({
            "embedding": vec,
            "dim": vec.len(),
            "model": model.name(),
        });
        println!("{}", serde_json::to_string(&payload)?);
        Ok(())
    }
}
```

- [ ] **Step 4: Register the subcommand in `src/cli/mod.rs`**

- Add to the `pub mod ...` block (after `pub mod compact;`):
  ```rust
  pub mod embed;
  ```
- Add a variant to `Command` (after `Compact`):
  ```rust
      Embed    (crate::cli::embed::Args),
  ```
- Add to the `match cli.command` dispatch in `run` (after the `Compact` arm):
  ```rust
          Command::Embed(a)    => crate::cli::embed::run(&conn, a, cli.json),
  ```

- [ ] **Step 5: Run the feature-off test to verify it passes**

Run: `cargo test --test cli_embed_feature_off`
Expected: PASS (exit 2).

- [ ] **Step 6: Write the e2e test (feature on, network)**

Create `tests/cli_embed.rs`:
```rust
#![cfg(feature = "embed")]
use assert_cmd::Command;

#[test]
#[ignore] // network: first run downloads the model
fn embed_prints_384_dim_object() {
    let output = Command::cargo_bin("mem0")
        .unwrap()
        .args(["embed", "hello world"])
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON on stdout");
    assert_eq!(v["dim"], 384);
    assert_eq!(v["model"], "multilingual-e5-small");
    assert_eq!(v["embedding"].as_array().unwrap().len(), 384);
}
```

- [ ] **Step 7: Verify default build + feature build both compile and pass**

Run: `cargo test` then `cargo test --features embed`
Expected: default passes all; feature build compiles (`cli_embed` ignored unless `--ignored`).

- [ ] **Step 8: Commit**

```bash
git add src/cli/embed.rs src/cli/mod.rs tests/cli_embed_feature_off.rs tests/cli_embed.rs
git commit -m "feat(embed): mem0 embed subcommand (feature-gated)"
```

---

### Task 8: `add` auto-embed precedence

**Goal:** Implement spec §5 in `add`. Add `--embed`/`--no-embed`/`--model` flags (always declared). With the feature on, `add "x"` auto-embeds unless overridden; with the feature off, `--embed` → `EmbedFeatureNotEnabled` and plain `add "x"` is text-only (v1.2). A piped stdin vector always wins.

**Files:**
- Modify: `src/cli/add.rs`
- Test: `tests/cli_add.rs` (append; existing tests must still pass)

**Interfaces:**
- Consumes: `crate::embed::{embed_text, ModelChoice, Role}` (feature-gated).

- [ ] **Step 1: Write the failing tests (precedence + conflicts; no network)**

Append to `tests/cli_add.rs`:
```rust
mod embed_precedence {
    use super::*; // reuse whatever helpers cli_add.rs already has (tmp db, run helper)

    // NOTE: these tests do NOT require the `embed` feature (they assert text-only /
    // error behaviour and flag-conflict parsing). They run on the default build.
    //
    // Adapt the db-path / run helper to match the existing style at the top of
    // tests/cli_add.rs (e.g. `fn cli(db: &str) -> Command`).

    #[test]
    fn embed_and_no_embed_conflict() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let out = cli(db.path().to_str().unwrap())
            .args(["add", "x", "--to=semantic", "--embed", "--no-embed"])
            .unwrap_err();
        assert_eq!(out.code().unwrap_or(0), 2);
    }

    #[test]
    fn embed_without_feature_exits_2() {
        // Only meaningful on the default (no-feature) build; on --features embed this
        // would instead auto-embed. Guard so it asserts exit 2 only when feature off.
        let db = tempfile::NamedTempFile::new().unwrap();
        let r = cli(db.path().to_str().unwrap())
            .args(["add", "x", "--to=semantic", "--embed"])
            .ok();
        if !cfg!(feature = "embed") {
            let out = r.expect_err("should fail without feature");
            assert_eq!(out.code().unwrap_or(0), 2);
        }
    }
}
```

> If `tests/cli_add.rs` has no `cli(db)` helper, inline the `assert_cmd::Command::cargo_bin("mem0").args(["--db", db]).…` pattern it already uses; copy that exact pattern rather than inventing one.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test cli_add embed_precedence`
Expected: FAIL — the flags don't exist (clap rejects them).

- [ ] **Step 3: Add the flags to `add::Args`**

In `src/cli/add.rs`, extend the `Args` struct (currently lines 43–52) — add after the `session` field:
```rust
    /// Force local embedding for this memory (overrides MEM0_EMBED=off).
    #[arg(long)]
    pub embed: bool,

    /// Store text only, do not embed (overrides auto-embed default).
    #[arg(long)]
    pub no_embed: bool,

    /// Override the default embedding model.
    #[arg(long)]
    pub model: Option<String>,
```

- [ ] **Step 4: Implement the precedence in `add::run`**

Replace the vector-resolution part of `run` (currently the `let vec_opt = maybe_read_vector()?;` block at lines 81–95) with a decision tree. Insert this **before** the `conn.execute_batch("BEGIN")?;` line:

```rust
    // --- vector-source precedence (spec §5) ---
    if args.embed && args.no_embed {
        return Err(MemError::InvalidArgument(
            "conflicting --embed and --no-embed".into(),
        ));
    }
    #[cfg(not(feature = "embed"))]
    if args.embed {
        return Err(MemError::EmbedFeatureNotEnabled);
    }

    let vec_opt: Option<Vec<f32>> = {
        // 1. piped stdin vector wins.
        let piped = maybe_read_vector()?;
        if piped.is_some() && args.embed {
            return Err(MemError::InvalidArgument(
                "piped vector and --embed both request a vector source".into(),
            ));
        }
        match piped {
            Some(v) => Some(v),
            None => {
                // 2/3/4/5: decide whether to auto-embed.
                #[cfg(feature = "embed")]
                {
                    if should_embed(args.embed, args.no_embed) {
                        let model = match args.model.as_deref() {
                            Some(n) => crate::embed::ModelChoice::from_name(n)?,
                            None => crate::embed::ModelChoice::DEFAULT,
                        };
                        Some(crate::embed::embed_text(&content, crate::embed::Role::Passage, model)?)
                    } else {
                        None
                    }
                }
                #[cfg(not(feature = "embed"))]
                { None }
            }
        }
    };
```

Then keep the existing transaction + `if let Some(vec) = &vec_opt { vectors::upsert(...) }` block unchanged (it already handles `Option`).

Add the `should_embed` helper at the bottom of `src/cli/add.rs`:
```rust
/// spec §5 rules 2–5 (feature on): embed > no-embed > MEM0_EMBED=off > default-on.
#[cfg(feature = "embed")]
fn should_embed(embed: bool, no_embed: bool) -> bool {
    if embed { return true; }
    if no_embed { return false; }
    match std::env::var("MEM0_EMBED") {
        Ok(v) if v.eq_ignore_ascii_case("off") => false,
        _ => true,
    }
}
```

- [ ] **Step 5: Run the precedence tests to verify they pass**

Run: `cargo test --test cli_add`
Expected: PASS (new + all existing `cli_add` tests green — the no-flag, no-pipe path still stores text-only and is covered by the existing `add_no_stdin_unchanged`-style tests).

- [ ] **Step 6: Write the auto-embed round-trip (feature, network, ignored)**

Append to `tests/cli_add.rs`:
```rust
#[cfg(feature = "embed")]
mod autoembed {
    use super::*;

    #[test]
    #[ignore] // network: downloads model on first run
    fn add_autoembed_then_vsearch_recalls() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let s = db.path().to_str().unwrap();
        cli(s).args(["add", "the user prefers single malt whiskey", "--to=semantic"])
            .assert().success();
        cli(s).args(["add", "unrelated note about the weather", "--to=semantic"])
            .assert().success();
        // vsearch via the embed subcommand piped in (Task 9 adds positional auto-embed too).
        let q = cli("").args(["embed", "what does the user drink"]).output().unwrap();
        let qvec = q.stdout.clone();
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_mem0"))
            .args(["--db", s, "vsearch", "--layer=semantic", "--limit=5"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn().unwrap();
        // feed the query vector on stdin
        use std::io::Write;
        let mut child = out;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&qvec).unwrap();
        }
        let out = child.wait_with_output().unwrap();
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("whiskey"), "top hit should be the whiskey memory: {s}");
    }
}
```

> `CARGO_BIN_EXE_mem0` requires `#[allow(unused)]` or use `assert_cmd::Command::cargo_bin("mem0")` with `.pipe_stdin()`; adapt to the helper style already in `cli_add.rs`. The intent: add auto-embeds, vsearch recalls by nearest distance. Keep it `#[ignore]`.

- [ ] **Step 7: Verify both builds**

Run: `cargo test` then `cargo test --features embed`
Expected: default green; feature build green (round-trip ignored).

- [ ] **Step 8: Commit**

```bash
git add src/cli/add.rs tests/cli_add.rs
git commit -m "feat(embed): add auto-embed precedence (--embed/--no-embed/--model)"
```

---

### Task 9: `vsearch` auto-embed

**Goal:** Give `vsearch` an optional positional `<QUERY>` text that auto-embeds (Role::Query) when no stdin vector is present, plus `--embed`/`--no-embed`/`--model`. Precedence mirrors `add`. Piped stdin vector still wins.

**Files:**
- Modify: `src/cli/vsearch.rs`
- Test: `tests/cli_vsearch.rs` (append; existing stdin-vector tests stay green)

**Interfaces:**
- Consumes: `crate::embed::{embed_text, ModelChoice, Role}` (feature-gated).

- [ ] **Step 1: Write the failing test (text query without feature errors cleanly)**

Append to `tests/cli_vsearch.rs`:
```rust
#[test]
fn text_query_without_feature_or_vector_errors() {
    // On the default build, a positional query with no piped vector and no embedder
    // must fail (exit != 0), not silently search a nonexistent vector.
    let db = tempfile::NamedTempFile::new().unwrap();
    let r = cli(db.path().to_str().unwrap())
        .args(["vsearch", "--layer=semantic", "some query"])
        .ok();
    if !cfg!(feature = "embed") {
        r.expect_err("should fail without a vector source");
    }
}
```

> Adapt `cli(db)` to the helper style already used at the top of `tests/cli_vsearch.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test cli_vsearch text_query_without_feature_or_vector_errors`
Expected: FAIL — `vsearch` takes no positional arg today (clap rejects it).

- [ ] **Step 3: Add positional + flags to `vsearch::Args`**

In `src/cli/vsearch.rs`, extend `Args` (currently lines 12–20):
```rust
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Query text to embed locally (requires the `embed` feature). Mutually exclusive
    /// with a piped stdin vector; the piped vector wins if both are present.
    pub query: Option<String>,

    #[arg(long)]
    pub layer: Option<String>,
    #[arg(long)]
    pub session: Option<String>,
    #[arg(long)]
    pub limit: Option<u32>,

    /// Force local embedding of the query (overrides MEM0_EMBED=off).
    #[arg(long)]
    pub embed: bool,
    /// Do not embed the query text (require a piped vector instead).
    #[arg(long)]
    pub no_embed: bool,
    /// Override the default embedding model.
    #[arg(long)]
    pub model: Option<String>,
}
```

- [ ] **Step 4: Replace `read_query_vector` + `run` with the precedence-aware version**

Replace the body of `src/cli/vsearch.rs` from the `fn read_query_vector` through the end of `run` with:
```rust
/// Resolve the query vector per spec §5 (piped vector > --embed > --no-embed >
/// MEM0_EMBED=off > default auto-embed).
fn resolve_query(args: &Args) -> MemResult<Vec<f32>> {
    if args.embed && args.no_embed {
        return Err(MemError::InvalidArgument(
            "conflicting --embed and --no-embed".into(),
        ));
    }

    // 1. piped stdin vector wins.
    let mut stdin = std::io::stdin();
    let piped = if stdin.is_terminal() { None } else {
        let mut raw = String::new();
        use std::io::Read;
        stdin.read_to_string(&mut raw)?;
        let trimmed = raw.trim();
        if trimmed.is_empty() { None } else {
            let v: serde_json::Value = serde_json::from_str(trimmed)
                .map_err(|e| MemError::EmbeddingParseError(e.to_string()))?;
            let arr = v.get("embedding").and_then(|e| e.as_array())
                .ok_or_else(|| MemError::EmbeddingParseError("missing 'embedding' array".into()))?;
            Some(arr.iter().map(|x| x.as_f64().map(|f| f as f32)
                .ok_or_else(|| MemError::EmbeddingParseError("non-numeric element".into())))
                .collect::<MemResult<Vec<f32>>>()?)
        }
    };

    if piped.is_some() && args.embed {
        return Err(MemError::InvalidArgument(
            "piped vector and --embed both request a vector source".into(),
        ));
    }
    if let Some(v) = piped { return Ok(v); }

    // 2–5: embed the positional query text (if any).
    let text = args.query.as_deref().ok_or_else(|| MemError::EmbeddingParseError(
        "vsearch needs a query: pass text (with the embed feature) or pipe {\"embedding\":[...]}".into()
    ))?;

    #[cfg(not(feature = "embed"))]
    {
        // No embedder compiled in: a text query is unusable. (The --embed flag is
        // accepted for help-stability but cannot do work; the error is the same.)
        let _ = args.embed;
        return Err(MemError::EmbedFeatureNotEnabled);
    }
    #[cfg(feature = "embed")]
    {
        if args.no_embed { return Err(MemError::InvalidArgument(
            "--no-embed given but no piped vector is available".into())); }
        if matches!(std::env::var("MEM0_EMBED"), Ok(v) if v.eq_ignore_ascii_case("off"))
            && !args.embed
        {
            return Err(MemError::InvalidArgument(
                "MEM0_EMBED=off and no piped vector; pass a vector or unset MEM0_EMBED".into()));
        }
        let model = match args.model.as_deref() {
            Some(n) => crate::embed::ModelChoice::from_name(n)?,
            None => crate::embed::ModelChoice::DEFAULT,
        };
        crate::embed::embed_text(text, crate::embed::Role::Query, model)
    }
}

pub fn run(conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    let query = resolve_query(&args)?;
    let layer = args.layer.as_deref().map(str::parse::<Lifecycle>).transpose()?;
    let session = match args.session.as_deref() {
        Some(name) => Some(crate::store::sessions::get(conn, name)?.id),
        None => None,
    };
    let filter = ListFilter {
        layer, session, since_nanos: None,
        limit: args.limit.unwrap_or(20),
    };
    let hits = vectors::search(conn, &query, filter)?;
    if json {
        let refs: Vec<(&crate::store::memories::MemoryItem, f64)> =
            hits.iter().map(|(m, d)| (m, *d)).collect();
        println!("{}", serde_json::to_string_pretty(&format::vsearch_json(&refs))?);
    } else if hits.is_empty() {
        println!("(no matches)");
    } else {
        for (m, d) in &hits {
            println!("{}", format::vsearch_line(m, *d));
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Run the existing vsearch tests to confirm the stdin path still works**

Run: `cargo test --test cli_vsearch`
Expected: PASS — existing stdin-vector tests unchanged (piped vector still wins), new test passes.

- [ ] **Step 6: Verify both builds**

Run: `cargo test` then `cargo test --features embed`
Expected: green on both.

- [ ] **Step 7: Commit**

```bash
git add src/cli/vsearch.rs tests/cli_vsearch.rs
git commit -m "feat(embed): vsearch auto-embeds positional query text"
```

---

### Task 10: SKILL docs + packaging note

**Goal:** Document the new behaviour so agents/users can use it; record the sidecar packaging step.

**Files:**
- Modify: `.claude/skills/mem0/SKILL.md`
- Modify: `README.md` (brief note) — optional, only if README documents commands.

- [ ] **Step 1: Add an "Embedding (built-in)" section to `.claude/skills/mem0/SKILL.md`**

Append a section:
```markdown
## Embedding (built-in, opt-in `embed` feature)

When mem0 is built with the `embed` feature (`cargo build --features embed`),
`add` and `vsearch` embed text locally on CPU — no external embedder needed.

- `mem0 add "user likes whiskey" --to=semantic`   # auto-embeds (passage)
- `mem0 vsearch "drink preferences" --layer=semantic`  # auto-embeds (query)
- `mem0 embed "any text"`                          # prints {"embedding":...,"dim":384,"model":...}

Opt out of auto-embedding:
- `--no-embed` on a single command (text-only `add`).
- `MEM0_EMBED=off` environment variable (disables auto-embed globally for that shell).

Low-level path (still works, highest precedence — use this with an external embedder):
- `echo '{"embedding":[...]}' | mem0 add "x" --to=semantic`
- `echo '{"embedding":[...]}' | mem0 vsearch --layer=semantic`
- `my-embed "q" | mem0 vsearch`  (a piped vector always wins over auto-embed)

Model: default `multilingual-e5-small` (384-dim); override with `--model`
(`all-MiniLM-L6-v2`, `bge-small-en-v1.5`, `bge-small-zh-v1.5`, `nomic-embed-text-v1.5`).

Model location (searched in order): `$MEM0_EMBED_MODEL_DIR` → `<exe_dir>/models/` →
`<cache_dir>/mem0/fastembed/` → lazy download. Release builds ship the model beside
the binary so the common path is offline.
```

- [ ] **Step 2: Add a packaging note (project docs)**

Append to `docs/superpowers/specs/2026-07-18-embed-model-design.md` §6 a concrete pointer, OR add a short `docs/embed-packaging.md`:
```markdown
# Release packaging for the `embed` feature

`cargo build --features embed --release` produces the binary. To make the common path
offline, fetch the default model into `<release_dir>/models/multilingual-e5-small/`:

1. Build: `cargo build --features embed --release`
2. Prime the cache once: run `./target/release/mem0 embed "warmup"` (downloads into the
   fastembed cache as a subdir named **`models--intfloat--multilingual-e5-small`** —
   spike-verified; fastembed's `model_code` for `MultilingualE5Small` is
   `intfloat/multilingual-e5-small`, not `Qdrant/…`). Five files land in
   `blobs/` (+ symlinks under `snapshots/<rev>/`): `onnx/model.onnx` (~470 MB),
   `tokenizer.json`, `config.json`, `special_tokens_map.json`, `tokenizer_config.json`.
3. Ship that cache subdir beside the binary, **preserving its name**:
   `mkdir -p target/release/models && cp -R .fastembed_cache/models--intfloat--multilingual-e5-small target/release/models/`
   (or from wherever `FASTEMBED_CACHE_DIR`/`HF_HOME` pointed). This works because
   `<exe_dir>/models/` is a `SearchRoot` and `resolve()` looks for
   `models--intfloat--multilingual-e5-small`.
4. Archive `target/release/{mem0, models/}` together.

**`HF_HOME` caveat:** fastembed's `pull_from_hf` prefers `HF_HOME` over the
`with_cache_dir(...)` value, so if the end-user has `HF_HOME` set, the shipped
sidecar under `<exe_dir>/models/` is bypassed (fastembed will look in `$HF_HOME`
instead). This is harmless — the binary still works (lazy download fallback) —
but document it. For a guaranteed-offline sidecar, advise users to leave
`HF_HOME` unset, or have the CLI unset `HF_HOME` for the embed path (out of
scope for v1.3). `HF_HUB_OFFLINE=1` + a fully-primed cache loads with no network.

The binary still works without the sidecar (falls back to lazy download).
```

- [ ] **Step 3: Commit**

```bash
git add .claude/skills/mem0/SKILL.md docs/embed-packaging.md
git commit -m "docs(embed): SKILL + release packaging for built-in embedding"
```

---

## Self-Review (run after writing, before handoff)

**Spec coverage** (spec section → task):
- §3 architecture (embed module, no store dep) → Tasks 2,4,5,6
- §4 CLI surface (`embed`, `--embed`/`--no-embed`/`--model`) → Tasks 7,8,9
- §5 precedence → Tasks 8,9 (`should_embed`, `resolve_query`)
- §6 model delivery (sidecar/cache/download) → Task 6
- §7 e5 prefix → Task 5 (`apply_prefix`, Role)
- §8 config (`MEM0_EMBED`, no meta write) → Tasks 8,9 (meta.embed_model intentionally dropped: YAGNI, keeps store unchanged)
- §9 feature gate → Tasks 2,7,8,9 (`#[cfg]` + `EmbedFeatureNotEnabled`)
- §10 errors → Task 3
- §11 module changes table → all tasks
- §12 testing → unit (Task 4,5,6), feature-off (Task 3,7,8), ignored e2e (Task 7,8)
- §13 SKILL → Task 10
- §14 spike → Task 1
- §15 DoD → covered by per-task verification + final `cargo test` / `cargo test --features embed`

**Placeholder scan:** spike-dependent code blocks are marked and reference Task 1; no "TODO"/"TBD" left as instructions. The `cli(db)` test-helper instructions say "copy the existing pattern" — acceptable since the helper already exists in each test file.

**Type consistency:** `ModelChoice` (Task 4) used identically in Tasks 5–9; `Role::Passage`/`Query` (Task 5) used in Tasks 7–9; `apply_prefix` (Task 5) matches its tests; `SearchRoots`/`resolve` (Task 6) names match tests; error variant names (`EmbedderInitError`, `EmbedderInferenceError`, `EmbedFeatureNotEnabled`) identical in Task 3 and all call sites.

---

## Final verification (after Task 10)

- [ ] `cargo build` compiles; `cargo tree | grep -c fastembed` → `0`.
- [ ] `cargo build --features embed` compiles.
- [ ] `cargo test` — all green (default build).
- [ ] `cargo test --features embed` — all green; `cargo test --features embed -- --ignored` runs the network e2e and passes.
- [ ] `cargo clippy --all-targets -- -D warnings` clean; `cargo clippy --features embed --all-targets -- -D warnings` clean.
- [ ] `mem0 add "x" --to=semantic` (feature build) stores a 384-dim vector; `mem0 vsearch "x" --layer=semantic` recalls it; `mem0 --embed`-less default build is byte-for-byte v1.2.
