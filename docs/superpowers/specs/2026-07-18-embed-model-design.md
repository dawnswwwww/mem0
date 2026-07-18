# mem0 v1.3 spec: Built-in embedding model (fastembed-rs, opt-in)

**Date:** 2026-07-18
**Status:** Approved (pending spec review)
**Scope:** v1.3 — add an **optional, local, CPU** text-embedding capability on top of
v1.2's vector store. When compiled in, `add`/`vsearch` can produce vectors themselves
instead of requiring the caller to pipe them. The caller-supplied-vector path from v1.2
is preserved unchanged as the low-level interface.

## 1. Goal

Let mem0 embed text locally (macOS / Linux / Windows, CPU) so that `add` and `vsearch`
work end-to-end without an external embedder, while keeping mem0's single-binary,
local-first, synchronous, no-Tokio character. Embedding is an **opt-in build feature**:
the default dev build is byte-for-byte unchanged; a release build with the feature on
gets a self-contained embedder whose model ships beside the binary.

This fills the gap left by v1.2 §1/§2: v1.2 deliberately put no model inside mem0, so
semantic search required the caller to compute vectors. v1.3 makes that optional —
"caller-supplied vector" remains the highest-precedence source, but mem0 can now be its
own caller.

## 2. Non-Goals (v1.3)

- **No GPU requirement.** CPU is the target. (DirectML on Windows is a fastembed feature
  flag that *could* be enabled later; it is off by default and out of scope here.)
- **No change to the store layer.** Vectors produced by the built-in embedder flow
  through the **existing** `vectors::upsert` / `vectors::search`. No new storage code, no
  new migration, no new dimension policy — the v1.2 `meta.embedding_dim` lock is reused
  as-is.
- **No multi-model coexistence.** One model at a time, fixed by the first vector mem0
  sees (v1.2 invariant, unchanged).
- **No model hot-swap command.** Switching models/dimensions stays the v1.2 manual
  procedure (clear `memories_vec` + `meta.embedding_dim`).
- **No async runtime.** fastembed-rs is synchronous; mem0 stays sync.
- **No re-embedding of existing rows.** If `content` changes, re-embedding is the
  caller's job (same as v1.2).
- **Default build unchanged.** `cargo build` without `--features embed` behaves exactly
  like v1.2 in every command.

## 3. Architecture

```
embed/                      ← NEW module, entirely #[cfg(feature = "embed")]
  mod.rs        embed_text(text, role) -> MemResult<Vec<f32>>
                embed_batch(texts, role) -> MemResult<Vec<Vec<f32>>>
                enum Role { Passage, Query }      ← drives the e5 prefix
  model.rs      ModelChoice: name <-> fastembed::EmbeddingModel
                default = MultilingualE5Small (384-dim)
  store.rs      lazily-initialized singleton holding TextEmbedding
                resolve_model_path() — sidecar search path (§6)
cli/
  add.rs        ← extended: auto-embed by default when feature on (§5)
  vsearch.rs    ← extended: auto-embed the query when no stdin vector
  embed.rs      ← NEW subcommand: text -> {"embedding":[...]} (reusable / pipeable)
core/
  error.rs      ← gains EmbedderInitError, EmbedderInferenceError, EmbedFeatureNotEnabled
store/  memories.rs, vectors.rs, db.rs, migrations.rs   ← UNCHANGED
```

**Dependency direction** unchanged (`cli → store → core`; new `cli → embed → core`).
`embed` has no dependency on `store` — it only produces `Vec<f32>`, which the cli layer
hands to the existing `store::vectors`.

**The embedder is "just another vector source."** Its output feeds the v1.2
`vectors::upsert` / `vectors::search`, so the lazy dimension lock, cosine KNN, and
layer/session filtering all apply without modification. If a DB was previously locked to
a different dimension (e.g. a caller piped 768-dim vectors), the built-in 384-dim
embedder hits the existing `EmbeddingDimMismatch` — correct behaviour, no new code.

## 4. CLI Surface

### 4.1 New `embed` subcommand

```text
mem0 embed "some text"                       # -> {"embedding":[...],"dim":384,"model":"multilingual-e5-small"}
echo "some text" | mem0 embed                # text from stdin
mem0 embed "doc" --as-passage --model bge-small-zh-v1.5
```

Default role is **Query** (search-oriented); `--as-passage` switches to the passage
prefix. `--model` overrides the default. Output is the same `{"embedding":[...]}` object
v1.2's `add`/`vsearch` read from stdin, so `mem0 embed "q" | mem0 vsearch` composes.

### 4.2 `add` — auto-embed by default (feature on)

```text
mem0 add "user likes whiskey" --to=semantic          # auto-embeds (new default)
mem0 add "raw note" --no-embed                       # text only (opt out)
echo '{"embedding":...}' | mem0 add "x" --to=semantic  # caller vector (unchanged v1.2 path)
mem0 add "x" --embed                                  # force embed (overrides MEM0_EMBED=off)
```

### 4.3 `vsearch` — auto-embed the query (feature on)

```text
mem0 vsearch "whiskey preferences" --layer=semantic --limit=20   # auto-embeds the query text
echo '{"embedding":...}' | mem0 vsearch --layer=semantic           # caller query vector (unchanged)
```

`vsearch` gains an optional positional `<QUERY>` text arg. A piped stdin vector still
wins (§5).

## 5. Vector-Source Precedence (the core decision rule)

For both `add` and `vsearch`, the vector source is resolved in this order:

```text
1. stdin piped {"embedding":...}     -> use caller vector          (highest; explicit)
2. --embed                           -> force local embed          (overrides config-off)
3. --no-embed                        -> none (text only / error for vsearch)
4. MEM0_EMBED=off                    -> none
5. default (feature compiled in)     -> local auto-embed
   default (feature NOT compiled)    -> none (v1.2 behaviour)
```

**Conflict rules (errors, exit 2):**
- `--embed` and `--no-embed` together → `InvalidArgument("conflicting --embed/--no-embed")`.
- stdin vector **and** `--embed` → `InvalidArgument("piped vector and --embed both request a vector source")`.
- `vsearch` with neither a piped vector nor query text (and no auto-embed) → `VectorNotInitialized`/`InvalidArgument`.

**Why piped wins (by design):** users of an external embedder (OpenAI, a large local
model) can still `my-embed "x" | mem0 add "x"` and mem0 will not override their
high-quality vector with the built-in small model. The built-in embedder is the default
*when nothing better is supplied*, never a usurper of an explicit vector.

## 6. Model Delivery — sidecar resource file

The model ships **beside the binary** in the release package, not inside the executable
and not lazily on first use. Resolution at init time (`embed::store::resolve_model_path`):

```text
1. $MEM0_EMBED_MODEL_DIR            (explicit override; air-gapped / power users)
2. <exe_dir>/models/<model-slug>/   (sidecar — ships in the release tarball/zip/installer)
3. <cache_dir>/mem0/fastembed/<model-slug>/   (prior lazy-download cache)
4. none of the above -> lazy download via InitOptions::with_cache_dir (fallback)
```

- `<cache_dir>` is `dirs::cache_dir()` (macOS `~/Library/Caches`, Linux `~/.cache`,
  Windows `%LOCALAPPDATA%`).
- Paths 1–3 load via fastembed `try_new_from_user_defined(onnx_file, tokenizer_files)`;
  path 4 via `TextEmbedding::try_new(InitOptions::new(model).with_cache_dir(...))`.
- The fallback (4) means a bare binary downloaded without the sidecar still works — it
  just downloads once on first embed. The sidecar makes the common path offline-ready
  and removes the first-`add` download surprise.

**Packaging** (release workflow, documented; likely a small script/CI step, not Rust
code): `cargo build --features embed --release` produces the binary; a packaging step
fetches the model into `models/<slug>/` next to it, then archives the pair. The Rust code
only resolves paths — it does not perform the release packaging.

## 7. The e5 Prefix (asymmetric query/passage)

multilingual-e5 requires an instruction prefix for best retrieval quality, and fastembed
does **not** add it automatically. `embed::embed_text` takes a `Role` and prepends:

| Call site | Role | Prefix |
|---|---|---|
| `add` (store a memory) | `Passage` | `passage: ` |
| `vsearch` (query) | `Query` | `query: ` |
| `embed` subcommand | `Query` default; `--as-passage` → `Passage` | as chosen |

The prefix is applied inside the `embed` module; `add`/`vsearch` callers pass plain text
and are unaware of it. Omitting this is a silent retrieval-quality regression, so it is
centralized in one place and unit-tested.

## 8. Configuration

- **`MEM0_EMBED`** env var: `off` disables auto-embed (rule 4 in §5); unset or `auto`
  leaves the default (auto-embed when the feature is compiled). This is the escape hatch
  for users/scripts that want v1.2 text-only behaviour from an `embed`-compiled binary.
- **`meta` table** (v1.2's table, reused): on first embed, write
  `meta.embed_model = "multilingual-e5-small"` for transparency/debuggability. The
  enforced invariant remains `meta.embedding_dim` (v1.2).

## 9. Feature Gate

```toml
[dependencies]
fastembed = { version = "5", optional = true }   # pulls ort (download-binaries) + tokenizers

[features]
embed = ["dep:fastembed"]
```

- `src/embed/` is entirely `#[cfg(feature = "embed")]`.
- The CLI flags (`--embed`, `--no-embed`, `--model`) and the `embed` subcommand are
  **always declared** in clap so `--help` is stable across builds. When the feature is
  off, invoking any of them returns `EmbedFeatureNotEnabled` with the message
  *"Embedding support is not compiled in. Rebuild with `cargo build --features embed`."*
- Two artifacts: default `cargo build` (lean, no ORT) and
  `cargo build --features embed --release` (end-user, `--embed` works out of the box).

**Invariant:** an `embed`-compiled binary with `MEM0_EMBED=off` (or `--no-embed`) behaves
identically to a non-`embed` binary for that command — no model is loaded, no network
touched, no inference run. The feature's only cost when *unused* is binary size.

## 10. Error Handling

New `MemError` variants in `core/error.rs`:

| Failure | `MemError` variant | Exit |
|---|---|---|
| Model init / download / load failure | `EmbedderInitError(String)` | 2 |
| Inference failure (ORT internal) | `EmbedderInferenceError(String)` | 2 |
| `--embed`/`embed` used without the feature compiled | `EmbedFeatureNotEnabled` | 2 |

Existing variants reused unchanged: `EmbeddingDimMismatch` (embedder dim ≠ locked dim),
`EmbeddingParseError`, `VectorNotInitialized`. The dimension-lock error path gets an
enhanced hint when the mismatch is between the built-in model's dim and the DB's locked
dim: *"DB is locked to {expected}-dim; the built-in model is {got}-dim. Use `--no-embed`,
clear `memories_vec`+`embedding_dim`, or embed externally."*

## 11. Module-level Changes

| File | Change |
|---|---|
| `Cargo.toml` | Add optional `fastembed = "5"`; `[features] embed = ["dep:fastembed"]`. |
| `src/embed/mod.rs` | NEW (feature-gated). `embed_text`, `embed_batch`, `Role`, prefix logic. |
| `src/embed/model.rs` | NEW (feature-gated). `ModelChoice` name↔enum map; default `MultilingualE5Small`. |
| `src/embed/store.rs` | NEW (feature-gated). Singleton `TextEmbedding`; `resolve_model_path` (§6). |
| `src/cli/embed.rs` | NEW. `embed` subcommand: text→`{"embedding":...}`. Feature-gated body; declared always. |
| `src/cli/add.rs` | Apply §5 precedence; on auto-embed, call `embed::embed_text(content, Passage)` then existing `vectors::upsert`. |
| `src/cli/vsearch.rs` | Apply §5 precedence; on auto-embed, `embed::embed_text(query, Query)` then existing `vectors::search`. Add optional positional `<QUERY>`. |
| `src/cli/mod.rs` | Register `embed` subcommand; map new errors to exit codes (§10). |
| `src/core/error.rs` | Add `EmbedderInitError`, `EmbedderInferenceError`, `EmbedFeatureNotEnabled`. |
| `src/store/*` | **Unchanged.** |
| `.claude/skills/mem0/SKILL.md` | Document auto-embed default, `--no-embed`, `MEM0_EMBED=off`, `mem0 embed`, sidecar model location (§13). |

## 12. Testing Strategy

| Test | Verifies | Feature | Network |
|---|---|---|---|
| `embed::prefix_passage` / `prefix_query` | Correct `passage:`/`query:` prefix per Role. | `embed` | no |
| `embed::model_name_roundtrip` | `--model` string ↔ `EmbeddingModel` enum maps both ways; unknown name errors. | `embed` | no |
| `embed::resolve_path_search_order` | Sidecar path resolution order (1→4) with temp dirs. | `embed` | no |
| `cli_embed_feature_off` | `--embed`/`embed` without feature → `EmbedFeatureNotEnabled`, exit 2. | default | no |
| `cli_add::no_embed_flag_text_only` | `add --no-embed` stores text, creates no `memories_vec` row. | `embed` | no |
| `cli_add::piped_vector_beats_autoembed` | Piped vector + feature-on uses the piped vector, not the embedder. | `embed` | no |
| `cli_add::embed_conflict_errors` | `--embed`+`--no-embed` and pipe+`--embed` both error. | `embed` | no |
| `cli_add::autoembed_roundtrip` **[ignore]** | `add "x"` auto-embeds → `vsearch "x"` recalls it with low distance. | `embed` | yes |
| `cli_embed::subcommand_e2e` **[ignore]** | `mem0 embed "x"` emits 384-dim JSON; pipeable into `vsearch`. | `embed` | yes |
| `cli_vsearch.rs` (v1.2, stdin path) | **Unchanged** — regression guard that the caller-vector path still works. | both | no |

`#[ignore]`'d tests run via `cargo test --features embed -- --ignored`. CI without network
skips them; the no-network tests still exercise all precedence/config/feature-gate logic.
The v1.2 stdin-vector tests remain the regression baseline for the store layer.

## 13. SKILL Update (deliverable)

`.claude/skills/mem0/SKILL.md` gains an "Embedding (built-in)" section:
- The default: `add`/`vsearch` auto-embed when mem0 is built with `embed`; no external
  embedder needed.
- Opt-outs: `--no-embed`, `MEM0_EMBED=off`.
- Low-level path still available: `mem0 embed "q" | mem0 vsearch`, or pipe a caller
  vector into `add`/`vsearch` (highest precedence).
- Where the model lives: sidecar `<exe_dir>/models/`, cache dir, or `MEM0_EMBED_MODEL_DIR`.

## 14. Open Questions / Spike (implementation prerequisite)

A short **technical spike** is the first implementation task, confirming the fastembed-rs
integration with a runnable `cargo --features embed` proof:

1. **Crate & API:** exact `fastembed` version and the `TextEmbedding::try_new` /
   `InitOptions::with_cache_dir` / `try_new_from_user_defined` /
   `EmbeddingModel::MultilingualE5Small` surface.
2. **Quantized variant:** whether `MultilingualE5SmallQ` exists; if so, prefer it for the
   sidecar (~half the disk size). Falls back to `MultilingualE5Small` otherwise.
3. **Sidecar loading:** that `try_new_from_user_defined(onnx_file, tokenizer_files)`
   loads a model from `<exe_dir>/models/<slug>/` without network.
4. **Cross-platform CPU:** that `ort`'s default `download-binaries` fetches a CPU runtime
   for macOS (aarch64 + x86_64), Linux, and Windows, and links into the single binary.
5. **Binary size delta:** measure `cargo build --features embed --release` vs default.
6. **Model on-disk size:** confirm the sidecar footprint for packaging.

Spike outcomes are recorded in the implementation plan; details above marked "confirmed
by the §14 spike" are finalized there.

## 15. Definition of Done (v1.3)

- §14 spike passes: a `--features embed` build embeds text on all three OSes (or is
  demonstrably cross-compilable to them), CPU-only, single binary.
- Default `cargo build` is byte-for-byte v1.2 (all existing tests green, no new deps
  linked).
- `add "x"` with the feature on auto-embeds and stores a vector; `--no-embed`,
  `MEM0_EMBED=off`, and a piped vector all override it per §5; conflicts error.
- `vsearch "q"` auto-embeds the query and returns ranked hits; the v1.2 stdin-vector
  path is unchanged.
- `mem0 embed` emits `{"embedding":...,"dim":384,"model":...}` and pipes into `vsearch`.
- Model resolves from sidecar → cache → lazy download (§6); works offline once sidecar
  present.
- `--embed`/`embed` without the feature returns `EmbedFeatureNotEnabled`, exit 2.
- All §12 tests green (network tests via `--ignored`); `cargo clippy --all-targets
  -- -D warnings` clean on both feature configurations.
- `SKILL.md` documents the auto-embed default, opt-outs, and model location.
