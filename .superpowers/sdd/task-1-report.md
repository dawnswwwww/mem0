# Task 1 Spike Report — fastembed-rs API verification

**Date:** 2026-07-18
**Branch:** `feat/embed-model`
**Throwaway crate:** `/tmp/mem0-spike` (removed after the run)

## Outcome

**Spike GREEN.** `embed()` returns 384-dim vectors for `MultilingualE5Small`,
offline-from-cache loading via `TextInitOptions::with_cache_dir(...)` under
`HF_HUB_OFFLINE=1` works with zero network. Two plan corrections required (repo
id + no Q-variant) — see "Plan corrections" below.

## Resolved crate versions (from `/tmp/mem0-spike/Cargo.lock`)

| crate | resolved |
|---|---|
| `fastembed` | **5.17.3** |
| `ort` | 2.0.0-rc.12 |
| `hf-hub` | 0.5.0 |
| `tokenizers` | 0.22.2 |

## Commands run

```bash
# Step 1: scaffold
mkdir -p /tmp/mem0-spike/src
# (Cargo.toml + src/main.rs written — Cargo.toml includes anyhow="1" per task
#  resolution; main.rs prints the whole ModelInfo struct so the repo-id field
#  is observed, not assumed.)

# Step 2: first run (model download). huggingface.co is firewalled here, so the
# HF mirror was used. The default fastembed cache dir (.fastembed_cache) was
# primed with the model.onnx (470 MB) + tokenizer.json; three small tokenizer
# sidecar files were fetched manually because the mirror's LFS redirect broke
# hf-hub's range download for them ("Header Content-Range is missing"). The
# network issue is environment-specific and does NOT affect the API facts.
HF_ENDPOINT=https://hf-mirror.com FASTEMBED_CACHE_DIR=./.fastembed_cache cargo run
#   -> printed: n=2 dim=384   (embed() works, 384-dim confirmed)

# Step 3: offline-from-cache via with_cache_dir (the exact sidecar strategy)
HF_HUB_OFFLINE=1 cargo run
#   -> printed: offline ok, n=1 dim=384   (no network, cache-dir loading works)
```

## Verified API facts (read directly from fastembed-5.17.3 source + confirmed
at runtime)

### `TextInitOptions` / init (src/init.rs, src/text_embedding/init.rs)

```rust
pub type TextInitOptions = InitOptionsWithLength<EmbeddingModel>;

// InitOptionsWithLength<M> fields (all pub):
//   model_name: M,
//   execution_providers: Vec<ExecutionProviderDispatch>,
//   cache_dir: PathBuf,
//   show_download_progress: bool,
//   max_length: usize,
//   intra_threads: Option<usize>,

impl ... InitOptionsWithLength<M> {
    pub fn new(model_name: M) -> Self;
    pub fn with_cache_dir(self, PathBuf) -> Self;              // CONFIRMED
    pub fn with_show_download_progress(self, bool) -> Self;    // CONFIRMED
    pub fn with_max_length(self, usize) -> Self;
    pub fn with_execution_providers(self, ...) -> Self;
    pub fn with_intra_threads(self, usize) -> Self;
}

// Default cache_dir = get_cache_dir() = env FASTEMBED_CACHE_DIR ?? ".fastembed_cache"
```

- Load entry point: `TextEmbedding::try_new(TextInitOptions) -> Result<TextEmbedding>` — confirmed.
- `UserDefinedEmbeddingModel` takes raw bytes (`onnx_file: Vec<u8>` + `TokenizerFiles`) — the plan's decision to use the pre-populated cache dir (NOT `UserDefinedEmbeddingModel`) is correct.

### `embed()` (src/text_embedding/impl.rs:432)

```rust
pub fn embed<S: AsRef<str> + Send + Sync>(
    &mut self,                                   // CONFIRMED &mut self
    texts: impl AsRef<[S]>,
    batch_size: Option<usize>,
) -> Result<Vec<Embedding>>
```

- `pub type Embedding = Vec<f32>;` (src/common.rs:25) — confirmed.
- Accepts `Vec<&str>` (or `Vec<String>`); the spike passed `vec!["query: hello", "passage: world"]`.
- e5 prefix is NOT added by fastembed — caller must prepend `query: `/`passage: ` (confirmed: the model's ModelInfo has no prefix logic; the plan's centralised `apply_prefix` is required).

### `list_supported_models()` (src/models/text_embedding.rs)

- Returns `Vec<ModelInfo<EmbeddingModel>>`. The HF repo id field is **`model_code: String`** (NOT `model`, `model_name`, or `repo()`). The onnx file is `model_file: String`. `ModelInfo` derives `Debug`.
- `ModelInfo` fields: `model`, `dim`, `description`, `model_code`, `model_file`, `additional_files: Vec<String>`, `output_key: Option<OutputKey>`.

### Default features (fastembed-5.17.3/Cargo.toml)

```toml
[features]
default = ["ort-download-binaries-native-tls", "hf-hub-native-tls", "image-models"]
hf-hub = ["dep:hf-hub", "hf-hub?/ureq"]
hf-hub-native-tls = ["hf-hub", "hf-hub?/native-tls"]
online = ["hf-hub-native-tls"]
```

→ **Plain `fastembed = "5"` enables network download** (default features pull in `hf-hub-native-tls`). No extra feature needed for the download path. (CPU execution provider only; no `directml`/`cuda`/`mkl`.)

### `MultilingualE5SmallQ`?

**Does NOT exist in fastembed 5.17.3.** The full `MultilingualE5*` family is exactly three variants:

| variant | dim | model_code (HF repo) | model_file |
|---|---|---|---|
| `MultilingualE5Small` | 384 | **`intfloat/multilingual-e5-small`** | `onnx/model.onnx` |
| `MultilingualE5Base`  | 768 | `intfloat/multilingual-e5-base`  | `onnx/model.onnx` |
| `MultilingualE5Large` | 1024 | `Qdrant/multilingual-e5-large-onnx` | `model.onnx` (+additional `model.onnx_data`) |

Q-variants DO exist for other families (`AllMiniLML6V2Q`, `BGESmallENV15Q`, `NomicEmbedTextV15Q`, `SnowflakeArcticEmbed*Q`, `MxbaiEmbedLargeV1Q`, `GTE*Q`, `EmbeddingGemma300MQ`/`Q4`) but **not** for any `MultilingualE5*`. The plan's "consider shipping `MultilingualE5SmallQ` to halve sidecar size" line is not actionable with this fastembed version — the small model's full-precision 470 MB ONNX is the only built-in option.

### `model_code` / cache subdir for the five plan models

Verified directly from `src/models/text_embedding.rs`:

| `ModelChoice` (plan) | fastembed variant | **verified `model_code`** | plan's old `repo()` | match? | cache subdir |
|---|---|---|---|---|---|
| MultilingualE5Small (default) | `MultilingualE5Small` | **`intfloat/multilingual-e5-small`** | `Qdrant/multilingual-e5-small` | **NO** | `models--intfloat--multilingual-e5-small` |
| AllMiniLML6V2 | `AllMiniLML6V2` | **`Qdrant/all-MiniLM-L6-v2-onnx`** | `Qdrant/all-MiniLM-L6-v2` | **NO** | `models--Qdrant--all-MiniLM-L6-v2-onnx` |
| BGESmallENV15 | `BGESmallENV15` | **`Xenova/bge-small-en-v1.5`** | `Qdrant/bge-small-en-v1.5` | **NO** | `models--Xenova--bge-small-en-v1.5` |
| BGESmallZHV15 | `BGESmallZHV15` | **`Xenova/bge-small-zh-v1.5`** | `Qdrant/bge-small-zh-v1.5` | **NO** | `models--Xenova--bge-small-zh-v1.5` |
| NomicEmbedTextV15 | `NomicEmbedTextV15` | `nomic-ai/nomic-embed-text-v1.5` | `nomic-ai/nomic-embed-text-v1.5` | yes | `models--nomic-ai--nomic-embed-text-v1.5` |

**4 of 5 `repo()` strings in the plan are wrong** and must be corrected in Task 4.

Also note `model_file` differs: AllMiniLML6V2 uses bare `model.onnx`, the others use `onnx/model.onnx`. The cache subdir transform `models--<org>--<name>` is standard and the plan's `hf_cache_subdir()` is correct given the right `repo()` input.

### Send + Sync

`TextEmbedding` is auto `Send + Sync` (holds `tokenizers::Tokenizer` + `ort::session::Session`; `ort::session::SharedSessionInner` is `unsafe impl Send + Sync` in ort 2.0.0-rc.12). The plan's "init-per-call" refinement still stands (avoids `&mut self` in a `OnceLock`), but a singleton would be type-safe if ever needed.

## Cache-dir mechanics (src/common.rs `pull_from_hf`, hf-hub 0.5.0)

fastembed resolves the cache dir as:
```rust
let cache_dir = env::var("HF_HOME").map(PathBuf::from).unwrap_or(default_cache_dir);
```
where `default_cache_dir` is the value passed via `with_cache_dir(...)` (or the
`.fastembed_cache`/`FASTEMBED_CACHE_DIR` default).

**Caveat:** `HF_HOME`, if set, **overrides** `with_cache_dir(...)`. The CLI
sidecar path must therefore either (a) not rely on `HF_HOME`, or (b) document
that `HF_HOME` takes precedence. Worth a one-line note in Task 6 / the packaging
doc.

Endpoint: `env HF_ENDPOINT ?? "https://huggingface.co"`. The mirror worked when
`HF_ENDPOINT=https://hf-mirror.com`.

`HF_HUB_OFFLINE=1`: fastembed does not read this env var directly, but hf-hub's
cache lookup serves fully-primed repos without a network round-trip; setting
`HF_HUB_OFFLINE=1` additionally prevents any residual metadata fetch. Verified
empirically: a fully-primed cache + `HF_HUB_OFFLINE=1` + `with_cache_dir(...)` →
loads and embeds with no network.

## Primed cache layout for `MultilingualE5Small` (verified — exact paths/sizes)

```
.fastembed_cache/
└── models--intfloat--multilingual-e5-small/        ← THE cache subdir
    ├── refs/
    │   └── main            (40 B; revision "614241f622f53c4eeff9890bdc4f31cfecc418b3")
    ├── blobs/
    │   ├── ca45606…8665    (470 268 510 B = onnx/model.onnx)
    │   ├── 0b44a9d…4c39    (17 082 730 B  = tokenizer.json)
    │   ├── 691377…f959     (655 B         = config.json)
    │   ├── d05497…07a7     (167 B         = special_tokens_map.json)
    │   └── a1d6bc…425b     (443 B         = tokenizer_config.json)
    └── snapshots/
        └── 614241f622f53c4eeff9890bdc4f31cfecc418b3/
            ├── config.json            -> ../../blobs/691377…
            ├── special_tokens_map.json-> ../../blobs/d05497…
            ├── tokenizer.json         -> ../../blobs/0b44a9d…
            ├── tokenizer_config.json  -> ../../blobs/a1d6bc…
            └── onnx/
                └── model.onnx         -> ../../../blobs/ca45606…
```

fastembed fetches exactly 5 files for `MultilingualE5Small`:
`onnx/model.onnx`, `tokenizer.json`, `config.json`, `special_tokens_map.json`,
`tokenizer_config.json`. Shipping these 5 files in the standard hf-hub cache
layout (blobs + symlinks under a `models--intfloat--multilingual-e5-small`
subdir) is sufficient for offline loading.

## Plan corrections required (Tasks 4, 6, 10)

1. **Task 4 `ModelChoice::repo()` — fix 4 of 5 strings** to the verified
   `model_code` values above (esp. the default → `intfloat/multilingual-e5-small`).
   This also fixes the `hf_cache_subdir` unit test, which currently asserts
   `"models--Qdrant--multilingual-e5-small"` and must become
   `"models--intfloat--multilingual-e5-small"`.

2. **Task 1 spike outcome table / Task 10 packaging doc — remove the
   `MultilingualE5SmallQ` line.** No such variant exists in fastembed 5.17.3;
   the quantised-sidecar idea is not available for the default model. (If a
   smaller sidecar is wanted later, switch the default to `BGESmallENV15Q`
   which DOES exist and is also 384-dim — but that changes the model and is out
   of scope for v1.3.)

3. **Task 6 / packaging doc — add a note** that `HF_HOME` overrides
   `with_cache_dir(...)`, and that `HF_HUB_OFFLINE=1` + a fully-primed cache
   gives a no-network load. The `resolve()` + `with_cache_dir` strategy is
   sound as-is.

4. **Task 5 / Task 6 `init` body — the verified builder names are correct as
   written** (`TextInitOptions::new(model).with_show_download_progress(bool)`
   and `.with_cache_dir(PathBuf)`, `TextEmbedding::try_new(opts)`). No change
   needed; this just removes the "Spike-dependent: confirm…" caveats.

## Environment note (not a blocker, but worth recording)

`huggingface.co` is firewalled in this environment (connection timeout); the
spike used `HF_ENDPOINT=https://hf-mirror.com` to prime the cache. The mirror's
LFS redirect does not support hf-hub's resumable range-download for some files
("Header Content-Range is missing"), so 3 of the 5 files were fetched with
plain `curl -sSL` and placed into the cache manually (matching hf-hub's
blob+symlink layout). This is purely a cache-priming workaround and does not
affect any of the verified API facts, which come from (a) the fastembed-5.17.3
source and (b) successful offline embed runs.
