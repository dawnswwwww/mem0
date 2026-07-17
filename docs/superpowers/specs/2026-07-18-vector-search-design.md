# mem0 v1.2 spec: Vector search (sqlite-vec)

**Date:** 2026-07-18
**Status:** Approved (pending spec review)
**Scope:** v1.2 — add opt-in vector (semantic) retrieval alongside the existing FTS5
keyword search. Embeddings are produced by the **caller**; mem0 only stores vectors
and runs KNN.

## 1. Goal

Give mem0 a semantic recall path that complements FTS5 keyword search, without
embedding any model into the CLI. The caller (an AI agent / LLM with access to an
embedding model) computes vectors and passes them in via stdin; mem0 stores them in
the same single `.db` file using the `sqlite-vec` extension and answers nearest-neighbour
queries. mem0 never touches an embedding model, so changing models does not change
mem0's data format — only the stored vectors.

This is the feature explicitly deferred as a v1 Non-Goal ("Embedding-based or hybrid
semantic retrieval") and not covered by any v1.1 spec-1..4.

## 2. Non-Goals (v1.2)

- **No embedding model inside mem0.** mem0 does not call any embedding API, load any
  ONNX/candle model, or touch the network. Vectors are caller-supplied.
- **No hybrid search / re-ranking.** `search` (FTS5) and `vsearch` (vector) are
  independent commands. No RRF, no result merging.
- **No automatic re-embedding.** If `content` changes after a vector was stored, the
  caller is responsible for re-embedding and updating the vector.
- **No multi-dimension coexistence.** A single configurable dimension is recorded in
  `meta`; mixing vectors of different dimensions in one DB is unsupported.
- **No dimension reset command.** Switching embedding models (changing dimension) is a
  manual, documented procedure (see §8), not a CLI command.
- **No change to existing commands.** `search`, `list`, `show`, `add` semantics for the
  no-vector path are unchanged and backward-compatible.

## 3. Architecture

```
cli/
  add.rs        ← extended: optionally read a vector from stdin
  vsearch.rs    ← NEW: vector KNN
store/
  vectors.rs    ← NEW: vector upsert/read + KNN + lazy vec0 init + dim guard
  memories.rs   ← unchanged except insert() must surface the rowid it used
  db.rs         ← migrate() gains a v3 branch; open() loads sqlite-vec per connection
  migrations.rs ← gains apply_v3_vector (meta table only)
core/
  error.rs      ← gains EmbeddingDimMismatch, EmbeddingParseError, VectorNotInitialized
```

**Dependency direction** is unchanged: `cli → store → core`; `store → core`; `core`
has no project-internal deps.

**Key association — rowid reuse, no mapping table.** `memories` is declared
`id TEXT PRIMARY KEY`, so it has an independent hidden `rowid` (a non-`INTEGER` PRIMARY
KEY does not become the rowid alias). The `vec0` virtual table uses this same `rowid`
as its integer primary key, giving a 1:1 correspondence with `memories`. Queries join
with `JOIN memories m ON m.rowid = v.rowid`. No separate id-mapping table is needed.

**Runtime model** is unchanged: stateless CLI. Each invocation opens SQLite, loads
`sqlite-vec`, migrates, performs its operation, closes.

## 4. Schema & Migration (two-phase)

`vec0` bakes the dimension into its DDL (`float[N]`), but the dimension is not known
at migration time — it is decided by the caller the first time a vector is supplied.
The schema work is therefore split: the meta framework is created during migration, and
the `vec0` table is created lazily at runtime once the dimension is known.

### 4.1 Phase one — `apply_v3_vector` (during `migrate()`, `version < 3`, one transaction)

```sql
CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

`meta` is a generic key/value config table. It records `embedding_dim` for this feature
and is available for future reuse by v1.1 spec-3 (`mem0 config`). `apply_v3_vector` does
**not** create `memories_vec` — the dimension is unknown at migration time.

`store::db::migrate` gains:

```text
if version < 3 {
    crate::store::migrations::apply_v3_vector(conn)?;
    conn.pragma_update(None, "user_version", 3_i64)?;
}
```

The existing `BEGIN ... COMMIT` wrapper in `migrate()` covers v3, so a failure leaves the
DB at v2 with no half-applied schema.

### 4.2 Phase two — `vectors::ensure_vec_table(conn, dim)` (runtime, lazy)

Called from the `add`-with-vector and `vsearch` paths before any vector I/O:

1. Read `meta['embedding_dim']`.
2. If absent → write `dim`, then
   `CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec USING vec0(embedding float[DIM])`.
3. If present and equals `dim` → no-op.
4. If present and differs → return `EmbeddingDimMismatch { expected, got: dim }`.

The exact `vec0` DDL (column options, auxiliary columns) is confirmed by the §11 spike.
The design intent is a single `embedding float[DIM]` column with no auxiliary columns;
layer/session filtering is done by joining `memories` after KNN (see §6).

## 5. CLI Surface

```text
# add — optional stdin vector (backward compatible: no stdin ⇒ text-only, as today)
echo '{"embedding":[...]}' | mem0 add "user likes whiskey" --to=semantic --tag=preference

# NEW — vector KNN (query vector from stdin)
echo '{"embedding":[...]}' | mem0 vsearch --layer=semantic --limit=20
```

**stdin vector format:** a single JSON object `{"embedding":[<f32>, ...]}`. An object
(rather than a bare array) leaves room for future metadata (e.g. `model`) without a
format break.

**stdin detection:** the CLI reads a vector from stdin **only when stdin is not a tty**
(`std::io::IsTerminal`). The existing interactive / no-pipe `add` behaviour is fully
preserved — nothing is read, nothing changes for current users and the 78 existing
tests.

**`add` flags:** unchanged (`--to`, `--tag`, `--session`). The vector is orthogonal; a
text-only `add` (no piped stdin) behaves exactly as in v1.1.

**`vsearch` flags:** `--layer=<l>`, `--session=<name>`, `--limit=<n>` (default 20, matching
`search`). `<QUERY>` is **not** a positional text argument — the query comes from stdin as
a vector. (`mem0 vsearch` takes no positional args.)

**Output:** reuses `output::format`. Human mode: one memory per line, each suffixed with
`distance=<f>`. JSON mode: each item gains a `distance` field (f32, lower = nearer).

## 6. Data Flow

### 6.1 `add` with a vector

1. If stdin is piped, read and parse `{"embedding":[...]}` → `Vec<f32>`.
2. `store::memories::insert` inserts the text row (unchanged), then returns the rowid it
   used (`conn.last_insert_rowid()`). `insert`'s signature changes to return
   `(uuid::Uuid, i64)` — the rowid is needed by the vector path. (See §7.)
3. `vectors::ensure_vec_table(conn, vec.len())` — lazy init / dim guard.
4. Serialize the vector as little-endian f32 bytes and
   `INSERT OR REPLACE INTO memories_vec(rowid, embedding) VALUES (?, ?)`.
   A fresh `add` always allocates a new rowid, so there is no collision in practice;
   `OR REPLACE` is defensive idempotency reserved for a future vector-update path.

If stdin is not piped, steps 1/3/4 are skipped; behaviour is identical to v1.1.

### 6.2 `vsearch`

1. Read stdin query vector → `Vec<f32>`.
2. `ensure_vec_table(conn, q.len())`. If `meta` has no `embedding_dim` yet
   (no vector has ever been stored) → return `VectorNotInitialized`.
3. KNN with an expanded window to absorb post-filter loss:
   `SELECT rowid, distance FROM memories_vec WHERE embedding MATCH ? ORDER BY distance LIMIT ?`
   with `LIMIT = min(max(limit*5, 100), 1000)`. (Exact KNN syntax confirmed by §11 spike.)
4. `JOIN memories m ON m.rowid = v.rowid`, apply `--layer` / `--session` filters, then
   `LIMIT limit`.
5. Render hits with `distance`.

**Filtering strategy:** KNN-then-join-filter (not `vec0` auxiliary columns). This keeps
the `vec0` schema minimal and makes `vsearch` filtering symmetric with the existing
`search` filter code. At mem0's single-user scale this is sufficient; a future version
may move filters into auxiliary columns if benchmarks demand it.

## 7. Module-level Changes

| File | Change |
|---|---|
| `src/store/migrations.rs` | Add `pub fn apply_v3_vector(conn) -> MemResult<()>` creating the `meta` table. |
| `src/store/db.rs` | `migrate()` dispatches `apply_v3_vector` when `version < 3`. `open()` calls `sqlite_vec::load(&conn)` after opening (per connection) to register the `vec0` module. |
| `src/store/vectors.rs` | NEW. `ensure_vec_table(conn, dim)`, `upsert(conn, rowid, &Vec<f32>)`, `knn(conn, &Vec<f32>, limit) -> Vec<(rowid, f32)>`. Vector ↔ little-endian f32 blob serialization helpers. |
| `src/store/memories.rs` | `insert()` returns `(uuid::Uuid, i64)` (id + rowid) so the vector path can associate. All callers updated. |
| `src/core/error.rs` | Add `EmbeddingDimMismatch { expected: usize, got: usize }`, `EmbeddingParseError(String)`, `VectorNotInitialized`. Map to exit codes (§8). |
| `src/cli/add.rs` | If stdin is not a tty, read `{"embedding":[...]}`; on success, upsert the vector for the returned rowid. Parse failures ⇒ `EmbeddingParseError`. |
| `src/cli/vsearch.rs` | NEW. Reads query vector from stdin, runs KNN, applies filters, renders. |
| `src/cli/mod.rs` | Register the `vsearch` subcommand. |
| `src/output/format.rs` | `memory_human_line` / `list_json` accept an optional `distance: Option<f64>` and render it when present. |
| `Cargo.toml` | Add `sqlite-vec = "<version from §11 spike>"`. |
| `.claude/skills/mem0/SKILL.md` | Document `vsearch` and the caller-embeds-then-pipes workflow (§12). |

## 8. Error Handling

| Failure | `MemError` variant | Exit |
|---|---|---|
| Vector dimension ≠ `meta.embedding_dim` | `EmbeddingDimMismatch { expected, got }` | 2 |
| stdin JSON invalid or missing `embedding` field | `EmbeddingParseError(detail)` | 2 |
| `vsearch` before any vector stored (no `embedding_dim`) | `VectorNotInitialized` | 3 |
| `sqlite-vec` fails to load (extension/register error) | `Storage` | 4 |

**Changing dimension (switching embedding models):** unsupported via CLI in v1.2. The
documented manual procedure is: stop using mem0 → `DELETE FROM memories_vec` →
`DELETE FROM meta WHERE key='embedding_dim'` → resume with vectors of the new dimension
(the next vector-bearing `add` lazily recreates `memories_vec` at the new dim). This is
deliberately manual; an automated `vec reset` is deferred (YAGNI).

## 9. Dependencies & Build

```toml
[dependencies]
sqlite-vec = "<version from §11 spike>"   # statically linked C; sqlite_vec::load(&conn)
```

The `sqlite-vec` crate embeds its C implementation and registers `vec0` in-process via
`sqlite_vec::load(&conn)` — no runtime `.so/.dylib/.dll`, preserving mem0's single-binary,
`rusqlite(bundled)` philosophy. Whether a `build.rs` adjustment is needed is confirmed by
the §11 spike.

## 10. Testing Strategy

| Test | Verifies |
|---|---|
| `tests/store_migrations.rs::migrate_v2_db_picks_up_v3` | A v2 DB gains the `meta` table after `migrate()`; `user_version` reads 3; existing rows intact. |
| `tests/store_vectors.rs::ensure_vec_table_lazy_init` | First call with dim=N creates `memories_vec` at `float[N]` and writes `meta.embedding_dim=N`. |
| `tests/store_vectors.rs::ensure_vec_table_dim_guard` | Second call with a different dim returns `EmbeddingDimMismatch`. |
| `tests/store_vectors.rs::knn_returns_nearest` | Insert 3 known vectors; KNN for a query returns them in correct distance order. |
| `tests/store_vectors.rs::knn_layer_filter` | KNN results are filtered by `--layer` via the `memories` join. |
| `tests/store_vectors.rs::upsert_replaces` | Calling `vectors::upsert` twice on the same rowid replaces the vector (store-layer idempotency guard). |
| `tests/cli_add.rs::add_with_vector` | `echo '{"embedding":[...]}' \| mem0 add ...` stores the vector; `vsearch` recalls it. |
| `tests/cli_add.rs::add_no_stdin_unchanged` | `add` with no piped stdin behaves exactly as v1.1 (regression guard). |
| `tests/cli_vsearch.rs::vsearch_e2e` | stdin query vector → ranked hits with `distance`, `--json` schema includes `distance`. |
| `tests/cli_vsearch.rs::vsearch_not_initialized` | `vsearch` on a DB with no vectors exits 3. |

All vector tests require `sqlite-vec` to load successfully; they double as the integration
guard for the §11 spike outcome.

## 11. Open Questions / Spike (implementation prerequisite)

A short **technical spike** is the first task of implementation, to confirm the
sqlite-vec Rust integration. It must resolve, with a runnable `cargo` proof:

1. **Crate & version:** exact `sqlite-vec` crate name and version on crates.io, and its
   `sqlite_vec::load(&conn)` API.
2. **Static linking:** that it links into `rusqlite(bundled)` producing a single binary
   with no runtime extension file; whether `build.rs` changes are needed.
3. **DDL:** exact `vec0` CREATE syntax and options for `float[N]`.
4. **KNN syntax:** exact `MATCH ... ORDER BY distance LIMIT k` form (and whether `k` is a
   `LIMIT` or an `AND k = ?` constraint).
5. **Blob encoding:** confirm little-endian f32 byte blobs are accepted for `float[N]`.

The spike's outcome is recorded in the implementation plan; design details above marked
"confirmed by the §11 spike" are finalized there. If the spike shows static linking is
not cleanly achievable, fall back is the loadable-extension path (plan B from
brainstorming), to be re-evaluated with the user before proceeding.

## 12. SKILL Update (deliverable)

`.claude/skills/mem0/SKILL.md` gains a `vsearch` section describing the caller workflow:
compute an embedding with the agent's embedding tool → pipe `{"embedding":[...]}` into
`mem0 add` (store) and `mem0 vsearch` (query). This is what makes the feature actually
usable by an agent, so it ships with the implementation, not as an afterthought.

## 13. Definition of Done (v1.2)

- §11 spike passes: `sqlite-vec` loads in-process, single binary, KNN query demonstrably
  returns correct nearest neighbours.
- `migrate()` takes a v2 DB to v3 in one transaction; `user_version` reads 3.
- `mem0 add` with a piped vector stores it; without a pipe it is byte-for-byte the v1.1
  behaviour (all existing tests green).
- `mem0 vsearch` returns ranked hits with `distance`; `--json` schema stable.
- All §10 tests green; `cargo clippy --all-targets -- -D warnings` clean.
- `SKILL.md` documents the `vsearch` workflow.
