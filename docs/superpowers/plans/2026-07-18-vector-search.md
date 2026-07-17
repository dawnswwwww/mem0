# Vector Search (sqlite-vec) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add opt-in vector (semantic) retrieval to mem0 via `sqlite-vec`, where the caller supplies embeddings and mem0 only stores vectors + runs cosine KNN — alongside the existing FTS5 keyword `search`.

**Architecture:** A new `memories_vec` `vec0` virtual table (cosine distance) keyed by `memories`' hidden `rowid`, created lazily once the caller's first vector fixes the dimension (stored in a new `meta` table). A new `mem0 vsearch` command reads a query vector from stdin and runs KNN; `mem0 add` optionally stores a vector from stdin. `sqlite-vec` is statically linked (single binary, no runtime `.so`) and registered once globally via `sqlite3_auto_extension`, hidden inside `db::open`.

**Tech Stack:** Rust (edition 2024), `rusqlite 0.32` (bundled), `sqlite-vec 0.1.9`, clap, serde_json. Verified by a throwaway spike (see "Spike outcome" below).

**Spec:** `docs/superpowers/specs/2026-07-18-vector-search-design.md`. Where this plan refines the spec (noted inline), the plan is authoritative — it reflects the verified API.

---

## Spike outcome (API verified against `sqlite-vec 0.1.9`)

| Concern | Verified answer |
|---|---|
| crate / version | `sqlite-vec = "0.1.9"` (stable; compiles statically; **do not use `0.1.10-alpha.*` — it fails to build, missing `sqlite-vec-diskann.c`**) |
| registration | `rusqlite::ffi::sqlite3_auto_extension(transmute(sqlite_vec::sqlite3_vec_init))`, **once, before any connection opens** (not a per-connection `load()`, which does not exist) |
| DDL | `CREATE VIRTUAL TABLE memories_vec USING vec0(embedding float[DIM], distance_metric=cosine)` |
| insert | `INSERT INTO memories_vec(rowid, embedding) VALUES(?,?)` with little-endian f32 blob |
| KNN | `SELECT rowid, distance FROM memories_vec WHERE embedding MATCH ? ORDER BY distance LIMIT k` |
| distance | cosine (lower = nearer); column name is literally `distance` |
| encoding | `f32::to_le_bytes()` concatenated |

The whole feature is built on these seven facts. Nothing below is guessed.

---

## File Structure

| File | Responsibility | Status |
|---|---|---|
| `Cargo.toml` | add `sqlite-vec = "0.1.9"` | modify |
| `src/store/db.rs` | `open()` registers sqlite-vec (via `Once`); `migrate()` gains v3 branch | modify |
| `src/store/migrations.rs` | `apply_v3_vector` creates the `meta` table | modify |
| `src/store/vectors.rs` | `f32_to_blob`, `ensure_vec_table`, `upsert`, `search` | **create** |
| `src/store/mod.rs` | declare `vectors` module | modify |
| `src/store/memories.rs` | `row_to_item` becomes `pub(crate)` (reused by `vectors::search`) | modify |
| `src/core/error.rs` | `EmbeddingDimMismatch`, `EmbeddingParseError`, `VectorNotInitialized` | modify |
| `src/cli/mod.rs` | `Command::Vsearch` + dispatch + `exit_code_for` branches | modify |
| `src/cli/vsearch.rs` | read query vector from stdin, run KNN, render | **create** |
| `src/cli/add.rs` | optionally read a vector from stdin and upsert it | modify |
| `src/output/format.rs` | `vsearch_line`, `memory_json_with_distance` | modify |
| `.claude/skills/mem0/SKILL.md` | document `vsearch` workflow | modify |
| `tests/store_vectors.rs` | store-layer vector tests | **create** |
| `tests/cli_vsearch.rs` | CLI end-to-end | **create** |
| `tests/cli_add.rs` | extend with vector add | modify |

`memories::insert` is **not** changed — `add` reads `conn.last_insert_rowid()` instead (avoids touching 28 call sites).

---

## Task 1: Add sqlite-vec dependency and register it on open

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/store/db.rs`
- Test: `tests/store_db.rs` (append)

- [ ] **Step 1: Add the dependency**

In `Cargo.toml`, under `[dependencies]`, add after the `rusqlite` line:

```toml
sqlite-vec = "0.1.9"
```

- [ ] **Step 2: Write the failing test**

Append to `tests/store_db.rs`:

```rust
#[test]
fn sqlite_vec_is_available_after_open() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("mem0.db");
    let conn = mem0::store::db::open(&path).unwrap();
    mem0::store::db::migrate(&conn).unwrap();
    let version: String = conn
        .query_row("SELECT vec_version()", [], |r| r.get(0))
        .expect("vec_version() should exist after open");
    assert!(version.starts_with("v"), "got: {version}");
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --test store_db sqlite_vec_is_available_after_open`
Expected: FAIL — `vec_version()` function not found (sqlite-vec not registered).

- [ ] **Step 4: Implement registration inside `open()`**

In `src/store/db.rs`, add at the top of the file (after the `use` lines):

```rust
use std::sync::Once;

/// Register the sqlite-vec extension globally. Idempotent via `Once`. Must run
/// before any `Connection::open_*`; `open()` calls it first. `sqlite3_auto_extension`
/// makes every subsequently opened connection auto-load the `vec0` module.
fn install_sqlite_vec() {
    static INSTALL: Once = Once::new();
    INSTALL.run(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}
```

Then, as the **first line inside `pub fn open(path: &Path) -> MemResult<Connection>`** (before `Connection::open_with_flags`), add:

```rust
    install_sqlite_vec();
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --test store_db sqlite_vec_is_available_after_open`
Expected: PASS. (First run compiles `sqlite-vec`'s C — may take ~30s.)

- [ ] **Step 6: Confirm no existing test regressed**

Run: `cargo test --test store_db`
Expected: all store_db tests PASS.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/store/db.rs tests/store_db.rs
git commit -m "feat(store): register sqlite-vec extension in db::open"
```

---

## Task 2: v3 migration — the `meta` table

**Files:**
- Modify: `src/store/migrations.rs`
- Modify: `src/store/db.rs`
- Test: `tests/store_migrations.rs` (append)

- [ ] **Step 1: Write the failing test**

Append to `tests/store_migrations.rs`:

```rust
#[test]
fn migrate_v2_db_picks_up_v3_meta_table() {
    // Build a v2 DB by running the current migrate on a fresh file (already at v3
    // after this task, but the assertion is that `meta` exists and version is 3).
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("mem0.db");
    let conn = mem0::store::db::open(&path).unwrap();
    mem0::store::db::migrate(&conn).unwrap();

    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 3);

    let n: i64 = conn
        .query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='meta'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "meta table must exist after v3 migration");
}

#[test]
fn migrate_v3_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("mem0.db");
    let conn = mem0::store::db::open(&path).unwrap();
    mem0::store::db::migrate(&conn).unwrap();
    mem0::store::db::migrate(&conn).unwrap(); // second call must not error
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 3);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test store_migrations migrate_v3`
Expected: FAIL — `user_version` is still 2.

- [ ] **Step 3: Add `apply_v3_vector` to migrations**

In `src/store/migrations.rs`, append:

```rust
/// v2 → v3 migration: create the generic `meta` key/value table.
/// The `memories_vec` vec0 table is NOT created here — its dimension is unknown
/// at migration time and is fixed lazily on the first caller-supplied vector
/// (see `store::vectors::ensure_vec_table`). Idempotent via `IF NOT EXISTS`.
pub fn apply_v3_vector(conn: &Connection) -> MemResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (\
           key   TEXT PRIMARY KEY,\
           value TEXT NOT NULL\
         )",
    )?;
    Ok(())
}
```

- [ ] **Step 4: Wire v3 into `migrate()`**

In `src/store/db.rs`, inside the `(|| -> MemResult<()> { ... })` closure in `migrate()`, after the `if version < 2 { ... }` block, add:

```rust
        if version < 3 {
            crate::store::migrations::apply_v3_vector(conn)?;
            conn.pragma_update(None, "user_version", 3_i64)?;
        }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --test store_migrations migrate_v3`
Expected: PASS.

- [ ] **Step 6: Run the full migration + store suite to confirm no regression**

Run: `cargo test --test store_migrations --test store_db --test store_memories`
Expected: all PASS.

- [ ] **Step 7: Commit**

```bash
git add src/store/migrations.rs src/store/db.rs tests/store_migrations.rs
git commit -m "feat(store): v3 migration adds meta table"
```

---

## Task 3: Error variants + exit/JSON mappings

**Files:**
- Modify: `src/core/error.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/output/format.rs`

- [ ] **Step 1: Add error variants**

In `src/core/error.rs`, inside the `MemError` enum (before the closing `}`), add:

```rust
    #[error("embedding dimension mismatch: expected {expected}, got {got}")]
    EmbeddingDimMismatch { expected: usize, got: usize },

    #[error("embedding parse error: {0}")]
    EmbeddingParseError(String),

    #[error("vector index not initialized: add a memory with a vector first")]
    VectorNotInitialized,
```

- [ ] **Step 2: Map new variants to exit codes**

In `src/cli/mod.rs`, in `exit_code_for`, replace the `MemError::Storage(_) => 4,` line context by adding the three new arms (place them before the `InvalidTransition` arm):

```rust
        MemError::EmbeddingDimMismatch { .. } => 2,
        MemError::EmbeddingParseError(_)    => 2,
        MemError::VectorNotInitialized      => 3,
```

- [ ] **Step 3: Map new variants in `error_json`**

In `src/output/format.rs`, in `error_json`'s `match e` block, add (before the `Storage` arm):

```rust
        MemError::EmbeddingDimMismatch { .. } => "EmbeddingDimMismatch",
        MemError::EmbeddingParseError(_)      => "EmbeddingParseError",
        MemError::VectorNotInitialized        => "VectorNotInitialized",
```

- [ ] **Step 4: Build to confirm the exhaustive matches compile**

Run: `cargo build`
Expected: succeeds. (Both `match` blocks are exhaustive; adding arms is required.)

- [ ] **Step 5: Commit**

```bash
git add src/core/error.rs src/cli/mod.rs src/output/format.rs
git commit -m "feat(core): add vector-related error variants and mappings"
```

---

## Task 4: `store::vectors` — encoding, lazy table init, upsert

**Files:**
- Create: `src/store/vectors.rs`
- Modify: `src/store/mod.rs`
- Modify: `src/store/memories.rs` (`row_to_item` visibility)
- Test: `tests/store_vectors.rs` (create)

- [ ] **Step 1: Expose `row_to_item` to sibling modules**

In `src/store/memories.rs`, change the `fn row_to_item` declaration from:

```rust
fn row_to_item(row: &Row<'_>) -> rusqlite::Result<MemoryItem> {
```

to:

```rust
pub(crate) fn row_to_item(row: &Row<'_>) -> rusqlite::Result<MemoryItem> {
```

- [ ] **Step 2: Declare the module**

In `src/store/mod.rs`, add (alongside the other `pub mod` lines):

```rust
pub mod vectors;
```

- [ ] **Step 3: Write the failing tests**

Create `tests/store_vectors.rs`:

```rust
use mem0::store::{db, memories::{MemoryDraft, Lifecycle}, vectors};
use rusqlite::Connection;
use tempfile::TempDir;

fn fresh() -> (TempDir, Connection) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("mem0.db");
    let conn = db::open(&path).unwrap();
    db::migrate(&conn).unwrap();
    (tmp, conn)
}

fn dim(conn: &Connection) -> Option<usize> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = 'embedding_dim'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|s| s.parse().ok())
}

#[test]
fn ensure_vec_table_lazy_init_writes_dim_and_creates_table() {
    let (_t, conn) = fresh();
    assert!(dim(&conn).is_none());
    vectors::ensure_vec_table(&conn, 4).unwrap();
    assert_eq!(dim(&conn), Some(4));
    let n: i64 = conn
        .query_row("SELECT count(*) FROM sqlite_master WHERE name='memories_vec'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn ensure_vec_table_dim_guard_rejects_mismatch() {
    let (_t, conn) = fresh();
    vectors::ensure_vec_table(&conn, 4).unwrap();
    let err = vectors::ensure_vec_table(&conn, 8).unwrap_err();
    assert!(matches!(
        err,
        mem0::MemError::EmbeddingDimMismatch { expected: 4, got: 8 }
    ));
}

#[test]
fn ensure_vec_table_idempotent_on_same_dim() {
    let (_t, conn) = fresh();
    vectors::ensure_vec_table(&conn, 4).unwrap();
    vectors::ensure_vec_table(&conn, 4).unwrap(); // no error
}

#[test]
fn upsert_stores_and_replaces_vector() {
    let (_t, conn) = fresh();
    conn.execute(
        "INSERT INTO memories (id, lifecycle, content, tags, tags_text, created_at, updated_at) \
         VALUES ('00000000-0000-7000-0000-000000000001','semantic','x','[]','',1,1)",
        [],
    )
    .unwrap();
    let rowid = conn.last_insert_rowid();
    vectors::upsert(&conn, rowid, &[1.0, 2.0, 3.0, 4.0]).unwrap();
    // replace with a different vector for the same rowid
    vectors::upsert(&conn, rowid, &[9.0, 9.0, 9.0, 9.0]).unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM memories_vec WHERE rowid = ?", [rowid], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "upsert must replace, not duplicate");
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --test store_vectors`
Expected: FAIL — `unresolved module store::vectors` / functions not found.

- [ ] **Step 5: Implement `vectors.rs`**

Create `src/store/vectors.rs`:

```rust
use rusqlite::{params, Connection};

use crate::core::error::{MemError, MemResult};

const DIM_KEY: &str = "embedding_dim";

/// Encode f32 slice as little-endian bytes — the format vec0 expects for float[N].
pub fn f32_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for f in v {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b
}

fn read_dim(conn: &Connection) -> MemResult<Option<usize>> {
    let s: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![DIM_KEY],
            |r| r.get(0),
        )
        .ok();
    Ok(s.and_then(|v| v.parse::<usize>().ok()))
}

fn write_dim(conn: &Connection, dim: usize) -> MemResult<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![DIM_KEY, dim.to_string()],
    )?;
    Ok(())
}

/// Ensure `memories_vec` exists at dimension `dim`. On first call, record `dim` in
/// `meta` and create the vec0 table (cosine distance). Subsequent calls must match.
pub fn ensure_vec_table(conn: &Connection, dim: usize) -> MemResult<()> {
    match read_dim(conn)? {
        None => {
            write_dim(conn, dim)?;
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec \
                 USING vec0(embedding float[{dim}], distance_metric=cosine)"
            ))?;
            Ok(())
        }
        Some(existing) if existing == dim => Ok(()),
        Some(existing) => Err(MemError::EmbeddingDimMismatch {
            expected: existing,
            got: dim,
        }),
    }
}

/// Store (or replace) the vector for a given `memories` rowid. Lazily initializes
/// the vec0 table at the vector's dimension on first use.
pub fn upsert(conn: &Connection, rowid: i64, vec: &[f32]) -> MemResult<()> {
    ensure_vec_table(conn, vec.len())?;
    conn.execute(
        "INSERT OR REPLACE INTO memories_vec(rowid, embedding) VALUES (?1, ?2)",
        params![rowid, f32_to_blob(vec)],
    )?;
    Ok(())
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test store_vectors`
Expected: PASS (4 tests).

- [ ] **Step 7: Commit**

```bash
git add src/store/vectors.rs src/store/mod.rs src/store/memories.rs tests/store_vectors.rs
git commit -m "feat(store): vectors module — encoding, lazy init, upsert"
```

---

## Task 5: `store::vectors::search` — KNN with layer/session filter

**Files:**
- Modify: `src/store/vectors.rs`
- Test: `tests/store_vectors.rs` (append)

- [ ] **Step 1: Write the failing tests**

Append to `tests/store_vectors.rs`:

```rust
use mem0::store::memories::ListFilter;

fn add_mem(conn: &Connection, lc: Lifecycle, content: &str) -> i64 {
    let _ = memories::insert(
        conn,
        &MemoryDraft {
            lifecycle: lc,
            content: content.into(),
            tags: vec![],
            session_id: None,
            source: None,
        },
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[test]
fn search_returns_nearest_first() {
    let (_t, conn) = fresh();
    let r1 = add_mem(&conn, Lifecycle::Semantic, "close");
    let r2 = add_mem(&conn, Lifecycle::Semantic, "nearby");
    let r3 = add_mem(&conn, Lifecycle::Semantic, "far");
    vectors::upsert(&conn, r1, &[1.0, 0.0, 0.0, 0.0]).unwrap();
    vectors::upsert(&conn, r2, &[0.9, 0.1, 0.0, 0.0]).unwrap();
    vectors::upsert(&conn, r3, &[0.0, 0.0, 0.0, 1.0]).unwrap();

    let hits = vectors::search(&conn, &[1.0, 0.0, 0.0, 0.0], ListFilter::default_limit(10)).unwrap();
    let contents: Vec<&str> = hits.iter().map(|(m, _)| m.content.as_str()).collect();
    assert_eq!(contents, vec!["close", "nearby"]); // "far" excluded by LIMIT after cosine ordering
}

#[test]
fn search_layer_filter_excludes_other_layers() {
    let (_t, conn) = fresh();
    let rs = add_mem(&conn, Lifecycle::Semantic, "s");
    let rw = add_mem(&conn, Lifecycle::Working, "w");
    vectors::upsert(&conn, rs, &[1.0, 0.0, 0.0, 0.0]).unwrap();
    vectors::upsert(&conn, rw, &[1.0, 0.0, 0.0, 0.0]).unwrap();

    let mut f = ListFilter::default_limit(10);
    f.layer = Some(Lifecycle::Semantic);
    let hits = vectors::search(&conn, &[1.0, 0.0, 0.0, 0.0], f).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0.lifecycle, Lifecycle::Semantic);
}

#[test]
fn search_before_any_vector_is_vector_not_initialized() {
    let (_t, conn) = fresh();
    let err = vectors::search(&conn, &[1.0, 2.0, 3.0, 4.0], ListFilter::default_limit(5)).unwrap_err();
    assert!(matches!(err, mem0::MemError::VectorNotInitialized));
}

#[test]
fn search_dim_mismatch_errors() {
    let (_t, conn) = fresh();
    vectors::ensure_vec_table(&conn, 4).unwrap();
    let err = vectors::search(&conn, &[1.0, 2.0, 3.0], ListFilter::default_limit(5)).unwrap_err();
    assert!(matches!(
        err,
        mem0::MemError::EmbeddingDimMismatch { expected: 4, got: 3 }
    ));
}
```

- [ ] **Step 2: Add the `default_limit` helper to `ListFilter`**

In `src/store/memories.rs`, inside the `impl` or right after the `ListFilter` struct definition, add a convenience constructor (the struct already derives `Default`, but `Default` gives `limit: 0`):

```rust
impl ListFilter {
    /// A filter with no layer/session/time constraint and the given result cap.
    pub fn default_limit(limit: u32) -> Self {
        ListFilter {
            layer: None,
            session: None,
            since_nanos: None,
            limit,
        }
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --test store_vectors`
Expected: FAIL — `vectors::search` not found.

- [ ] **Step 4: Implement `search`**

Append to `src/store/vectors.rs`:

```rust
use crate::store::memories::{row_to_item, ListFilter, MemoryItem};

/// Run cosine KNN for `query`, then filter by layer/session and return up to
/// `filter.limit` hits as `(MemoryItem, distance)`. Lower distance = nearer.
///
/// Strategy (per spec §6.2): a pure KNN fetch over an expanded window, then a
/// `memories` lookup that applies layer/session filters. This keeps the KNN query
/// in the exact form sqlite-vec requires and reuses the existing filter columns.
pub fn search(
    conn: &Connection,
    query: &[f32],
    filter: ListFilter,
) -> MemResult<Vec<(MemoryItem, f64)>> {
    let dim = read_dim(conn)?.ok_or(MemError::VectorNotInitialized)?;
    if dim != query.len() {
        return Err(MemError::EmbeddingDimMismatch {
            expected: dim,
            got: query.len(),
        });
    }

    let knn_limit = filter
        .limit
        .saturating_mul(5)
        .max(100)
        .min(1000);

    // 1. Pure KNN over the expanded window. Scoped so the statement (which borrows
    //    conn) is dropped before we re-borrow conn for the candidate fetch below.
    let knn: Vec<(i64, f64)> = {
        let mut stmt = conn.prepare(
            "SELECT rowid, distance FROM memories_vec \
             WHERE embedding MATCH ?1 ORDER BY distance LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![f32_to_blob(query), knn_limit], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    if knn.is_empty() {
        return Ok(Vec::new());
    }

    // 2. One filtered fetch of the candidate memories, selecting rowid so distance
    //    can be rejoined in Rust without a second query per row (and without
    //    re-borrowing conn while iterating).
    let placeholders = (0..knn.len()).map(|_| "?").collect::<Vec<_>>().join(",");
    let mut sql = String::from(
        "SELECT rowid, id, lifecycle, content, source, session_id, tags, \
                created_at, updated_at, accessed_at \
         FROM memories WHERE rowid IN (",
    );
    sql.push_str(&placeholders);
    sql.push(')');
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = knn
        .iter()
        .map(|(rowid, _)| Box::new(*rowid) as Box<dyn rusqlite::ToSql>)
        .collect();
    if let Some(layer) = filter.layer {
        sql.push_str(" AND lifecycle = ?");
        binds.push(Box::new(layer.to_string()));
    }
    if let Some(sid) = filter.session {
        sql.push_str(" AND session_id = ?");
        binds.push(Box::new(sid.to_string()));
    }

    let dist: std::collections::HashMap<i64, f64> =
        knn.iter().map(|(r, d)| (*r, *d)).collect();
    let mut stmt2 = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|b| &**b as &dyn rusqlite::ToSql).collect();
    let rows = stmt2.query_map(rusqlite::params_from_iter(params), |row| {
        let rowid: i64 = row.get("rowid")?;
        let item = row_to_item(row)?;
        Ok((rowid, item))
    })?;
    let mut out: Vec<(MemoryItem, f64)> = Vec::new();
    for r in rows {
        let (rowid, item) = r?;
        if let Some(d) = dist.get(&rowid) {
            out.push((item, *d));
        }
    }

    // 3. Preserve KNN distance order (row order from the filtered fetch is not
    //    guaranteed to be by distance), then cap.
    out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let limit = if filter.limit == 0 { 20 } else { filter.limit };
    out.truncate(limit.min(1000) as usize);
    Ok(out)
}
```

> Note: `MemoryItem` has no `rowid` field, so the candidate fetch selects `rowid` explicitly and distance is rejoined in Rust via a hashmap. Both prepared statements borrow `conn`, so the KNN statement is wrapped in a block to drop it before the candidate fetch is prepared.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test store_vectors`
Expected: PASS (all 8 tests).

- [ ] **Step 6: Commit**

```bash
git add src/store/vectors.rs src/store/memories.rs tests/store_vectors.rs
git commit -m "feat(store): vectors::search — cosine KNN with layer/session filter"
```

---

## Task 6: Output helpers for distance

**Files:**
- Modify: `src/output/format.rs`
- Test: `tests/output_format.rs` (create — currently format is tested inline via `#[cfg(test)]`; add a focused unit test inside the module instead)

- [ ] **Step 1: Write the failing test**

In `src/output/format.rs`, inside the existing `#[cfg(test)] mod tests` block, append:

```rust
    #[test]
    fn vsearch_line_includes_distance() {
        let m = sample("11111111-2222-3333-4444-555555555555", Lifecycle::Semantic, "x");
        let line = vsearch_line(&m, 0.123);
        assert!(line.contains("x"));
        assert!(line.contains("0.123"), "missing distance: {line}");
    }

    #[test]
    fn json_with_distance_has_distance_field() {
        let m = sample("11111111-2222-3333-4444-555555555555", Lifecycle::Semantic, "x");
        let v = memory_json_with_distance(&m, 0.5);
        assert_eq!(v["distance"], 0.5);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib format::tests`
Expected: FAIL — `vsearch_line` / `memory_json_with_distance` not found.

- [ ] **Step 3: Implement the helpers**

In `src/output/format.rs`, after the `memory_json` function, add:

```rust
/// One-line human rendering with a trailing distance, for `vsearch`.
pub fn vsearch_line(m: &MemoryItem, distance: f64) -> String {
    format!("{} (distance={})", memory_human_line(m), distance)
}

/// Structured rendering with distance, for `vsearch --json`.
pub fn memory_json_with_distance(m: &MemoryItem, distance: f64) -> Value {
    let mut v = memory_json(m);
    v["distance"] = json!(distance);
    v
}

/// JSON list wrapper for vsearch hits, each carrying its distance.
pub fn vsearch_json(hits: &[(&MemoryItem, f64)]) -> Value {
    let arr: Vec<Value> = hits
        .iter()
        .map(|(m, d)| memory_json_with_distance(m, *d))
        .collect();
    json!({ "items": arr, "count": arr.len() })
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib format::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/output/format.rs
git commit -m "feat(output): vsearch rendering helpers with distance"
```

---

## Task 7: `mem0 vsearch` command

**Files:**
- Create: `src/cli/vsearch.rs`
- Modify: `src/cli/mod.rs`
- Test: `tests/cli_vsearch.rs` (create)

- [ ] **Step 1: Write the failing CLI tests**

Create `tests/cli_vsearch.rs`:

```rust
use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn db_arg(dir: &TempDir) -> String {
    dir.path().join("mem0.db").to_string_lossy().to_string()
}

#[test]
fn vsearch_returns_ranked_hits_with_distance() {
    let dir = TempDir::new().unwrap();
    let db = db_arg(&dir);

    // add two memories with vectors via stdin
    for (content, vec) in [
        ("close one", r#"{"embedding":[1.0,0.0,0.0,0.0]}"#),
        ("far one",   r#"{"embedding":[0.0,0.0,0.0,1.0]}"#),
    ] {
        Command::cargo_bin("mem0").unwrap()
            .args(["--db", &db, "add", content, "--to", "semantic"])
            .pipe_stdin(vec.as_bytes())
            .assert().success();
    }

    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "--json", "vsearch"])
        .pipe_stdin(r#"{"embedding":[1.0,0.0,0.0,0.0]}"#.as_bytes())
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 2);
    let first = &v["items"][0];
    assert!(first["content"].as_str().unwrap().contains("close one"));
    assert!(first["distance"].as_f64().unwrap() < 0.001, "nearest should be ~0 distance");
}

#[test]
fn vsearch_without_vectors_exits_3() {
    let dir = TempDir::new().unwrap();
    let db = db_arg(&dir);
    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "vsearch"])
        .pipe_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#.as_bytes())
        .output().unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn vsearch_dim_mismatch_exits_2() {
    let dir = TempDir::new().unwrap();
    let db = db_arg(&dir);
    // seed dim=4
    Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "x", "--to", "semantic"])
        .pipe_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#.as_bytes())
        .assert().success();
    // query with wrong dim
    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "vsearch"])
        .pipe_stdin(r#"{"embedding":[1.0,2.0,3.0]}"#.as_bytes())
        .output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test cli_vsearch`
Expected: FAIL — `vsearch` is not a recognized subcommand.

- [ ] **Step 3: Implement `cli/vsearch.rs`**

Create `src/cli/vsearch.rs`:

```rust
use std::io::{IsTerminal, Read};

use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::{MemError, MemResult};
use crate::core::memory::Lifecycle;
use crate::output::format;
use crate::store::memories::ListFilter;
use crate::store::vectors;

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[arg(long)]
    pub layer: Option<String>,
    #[arg(long)]
    pub session: Option<String>,
    #[arg(long)]
    pub limit: Option<u32>,
}

/// Read the query vector from stdin (must be piped): `{"embedding":[f32,...]}`.
fn read_query_vector() -> MemResult<Vec<f32>> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err(MemError::EmbeddingParseError(
            "vsearch requires a query vector on stdin, e.g. echo '{\"embedding\":[...]}' | mem0 vsearch".into(),
        ));
    }
    let mut raw = String::new();
    stdin.read_to_string(&mut raw)?;
    let v: serde_json::Value =
        serde_json::from_str(raw.trim()).map_err(|e| MemError::EmbeddingParseError(e.to_string()))?;
    let arr = v
        .get("embedding")
        .and_then(|e| e.as_array())
        .ok_or_else(|| MemError::EmbeddingParseError("missing 'embedding' array".into()))?;
    arr.iter()
        .map(|x| {
            x.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| MemError::EmbeddingParseError("embedding has non-numeric element".into()))
        })
        .collect()
}

pub fn run(conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    let query = read_query_vector()?;
    let layer = args
        .layer
        .as_deref()
        .map(str::parse::<Lifecycle>)
        .transpose()?;
    let session = match args.session.as_deref() {
        Some(name) => Some(crate::store::sessions::get(conn, name)?.id),
        None => None,
    };
    let filter = ListFilter {
        layer,
        session,
        since_nanos: None,
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

- [ ] **Step 4: Register the subcommand**

In `src/cli/mod.rs`:

Add the module declaration (with the other `pub mod` lines):

```rust
pub mod vsearch;
```

Add the variant to the `Command` enum (after `Search`):

```rust
    Vsearch  (crate::cli::vsearch::Args),
```

Add the dispatch arm in `run()` (after the `Command::Search(a)` arm):

```rust
        Command::Vsearch(a) => crate::cli::vsearch::run(&conn, a, cli.json),
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test cli_vsearch`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add src/cli/vsearch.rs src/cli/mod.rs tests/cli_vsearch.rs
git commit -m "feat(cli): add mem0 vsearch command"
```

---

## Task 8: `mem0 add` optionally stores a vector from stdin

**Files:**
- Modify: `src/cli/add.rs`
- Modify: `tests/cli_add.rs` (append)

- [ ] **Step 1: Write the failing tests**

Append to `tests/cli_add.rs`:

```rust
use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn add_with_vector_then_vsearch_recalls_it() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mem0.db").to_string_lossy().to_string();

    Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "user likes whiskey", "--to", "semantic"])
        .pipe_stdin(r#"{"embedding":[1.0,0.0,0.0,0.0]}"#.as_bytes())
        .assert().success();

    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "--json", "vsearch"])
        .pipe_stdin(r#"{"embedding":[0.9,0.1,0.0,0.0]}"#.as_bytes())
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["count"], 1);
    assert!(v["items"][0]["content"].as_str().unwrap().contains("whiskey"));
}

#[test]
fn add_without_stdin_is_unchanged() {
    // No piped stdin ⇒ text-only add, exactly as before. Confirms the regression guard.
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mem0.db").to_string_lossy().to_string();
    let out = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "add", "plain text memory", "--to", "working"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // No vector indexed ⇒ vsearch reports not initialized.
    let vout = Command::cargo_bin("mem0").unwrap()
        .args(["--db", &db, "vsearch"])
        .pipe_stdin(r#"{"embedding":[1.0,2.0,3.0,4.0]}"#.as_bytes())
        .output().unwrap();
    assert_eq!(vout.status.code(), Some(3));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test cli_add add_with_vector`
Expected: FAIL — `vsearch` recall returns 0 / not initialized (add ignored the vector).

- [ ] **Step 3: Implement stdin-vector ingestion in `add`**

In `src/cli/add.rs`:

Change the top imports to:

```rust
use std::io::{IsTerminal, Read};

use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::{MemError, MemResult};
use crate::core::memory::Lifecycle;
use crate::output::format;
use crate::store::memories::{self, MemoryDraft};
use crate::store::vectors;
```

Add a helper to parse an optional stdin vector (returns `Ok(None)` when stdin is a terminal):

```rust
/// If stdin is piped, parse `{"embedding":[...]}`. If stdin is a terminal (no pipe),
/// return `Ok(None)` so text-only `add` is unchanged. A piped-but-invalid payload is
/// an error.
fn maybe_read_vector() -> MemResult<Option<Vec<f32>>> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }
    let mut raw = String::new();
    stdin.read_to_string(&mut raw)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let v: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|e| MemError::EmbeddingParseError(e.to_string()))?;
    let arr = v
        .get("embedding")
        .and_then(|e| e.as_array())
        .ok_or_else(|| MemError::EmbeddingParseError("missing 'embedding' array".into()))?;
    let out: Vec<f32> = arr
        .iter()
        .map(|x| {
            x.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| MemError::EmbeddingParseError("embedding has non-numeric element".into()))
        })
        .collect::<MemResult<_>>()?;
    Ok(Some(out))
}
```

Then in `pub fn run`, replace the block:

```rust
    let id = memories::insert(conn, &draft)?;
    let item = memories::get(conn, id)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&format::memory_json(&item))?);
    } else {
        println!("stored {} as {}", &id.to_string()[..8], item.lifecycle);
    }
    Ok(())
```

with:

```rust
    let id = memories::insert(conn, &draft)?;
    let item = memories::get(conn, id)?;

    // Optional caller-supplied vector: store it for the new row's rowid.
    if let Some(vec) = maybe_read_vector()? {
        let rowid = conn.last_insert_rowid();
        vectors::upsert(conn, rowid, &vec)?;
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&format::memory_json(&item))?);
    } else {
        println!("stored {} as {}", &id.to_string()[..8], item.lifecycle);
    }
    Ok(())
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test cli_add`
Expected: PASS (existing tests + 2 new).

- [ ] **Step 5: Run the full CLI suite for regressions**

Run: `cargo test --test cli_add --test cli_search --test cli_list --test cli_show`
Expected: all PASS (text-only paths unchanged).

- [ ] **Step 6: Commit**

```bash
git add src/cli/add.rs tests/cli_add.rs
git commit -m "feat(cli): add optionally stores a caller-supplied vector from stdin"
```

---

## Task 9: SKILL docs, clippy, full suite

**Files:**
- Modify: `.claude/skills/mem0/SKILL.md`

- [ ] **Step 1: Document `vsearch` in the skill**

Append a new section to `.claude/skills/mem0/SKILL.md` (after the existing search guidance), using the same tone and structure as the surrounding doc:

````markdown
## Vector search (`vsearch`) — semantic recall

`vsearch` does cosine nearest-neighbour over memories that have a stored vector.
mem0 never computes embeddings — **the caller does**. Compute an embedding for the
text you want to find (and for memories when storing them), then pipe the vector as
`{"embedding":[...]}` on stdin.

```bash
# store a memory WITH a vector (vector is optional; omit to store text-only)
echo '{"embedding":[...your embedding...]}' | mem0 add "user prefers dark mode" --to=semantic

# semantic search: pipe the query vector
echo '{"embedding":[...query embedding...]}' | mem0 vsearch --layer=semantic --limit=10
```

Rules:
- The dimension is fixed by the **first** vector mem0 sees; all later vectors (add or
  vsearch) must match it, else exit code 2.
- `search` (FTS5 keywords) and `vsearch` (vector) are independent — use either or both.
- Memories added without a vector never appear in `vsearch` (they still appear in
  `search`/`list`). To make a memory vector-searchable, the embedding must be supplied
  at `add` time.
- Changing embedding model / dimension is manual: clear `memories_vec` and
  `meta.embedding_dim`, then re-add with the new model's vectors.
````

- [ ] **Step 2: Run clippy with warnings as errors**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no warnings. Fix any that arise.

- [ ] **Step 3: Run the entire test suite**

Run: `cargo test`
Expected: all tests PASS (all pre-existing + new store_vectors + cli_vsearch + cli_add additions).

- [ ] **Step 4: Commit**

```bash
git add .claude/skills/mem0/SKILL.md
git commit -m "docs(skill): document mem0 vsearch vector workflow"
```

- [ ] **Step 5: Manual smoke test**

```bash
DB=$(mktemp -d)/mem0.db
printf '%s' '{"embedding":[1.0,0.0,0.0,0.0]}' | cargo run -q -- --db "$DB" add "likes whiskey" --to=semantic
printf '%s' '{"embedding":[0.0,1.0,0.0,0.0]}' | cargo run -q -- --db "$DB" add "likes golf"      --to=semantic
printf '%s' '{"embedding":[0.9,0.1,0.0,0.0]}' | cargo run -q -- --db "$DB" vsearch
```
Expected: `likes whiskey` ranked first (nearest cosine distance).

---

## Self-Review (run before handing off)

**Spec coverage** — every spec section maps to a task:
- §3 architecture → Tasks 1, 4, 5, 7, 8 (module wiring)
- §4 two-phase schema → Task 2 (meta), Task 4 (`ensure_vec_table`)
- §5 CLI surface → Tasks 7, 8
- §6 data flow → Task 4 (upsert), Task 5 (search), Task 8 (add path)
- §7 module changes → all tasks
- §8 error handling → Task 3 (+ consumed in 4/5/7/8)
- §9 deps/build → Task 1
- §10 testing → embedded in every task
- §11 spike → completed; outcome at top of this plan
- §12 SKILL → Task 9
- §13 DoD → Task 9 steps 2–5

**Refinements vs spec (plan is authoritative):**
- Registration is `sqlite3_auto_extension` (global, in `db::open`), not a per-connection `load()` — `load()` doesn't exist in 0.1.9.
- Distance is **cosine** via `distance_metric=cosine` (spec said "cosine" loosely; now concrete).
- `memories::insert` is **unchanged**; `add` uses `conn.last_insert_rowid()` — avoids 28 call-site changes (spec §6.1 proposed a signature change).
- Crate version pinned to `0.1.9` (0.1.10-alpha does not build).

**Type/symbol consistency check:**
- `vectors::ensure_vec_table`, `upsert`, `search`, `f32_to_blob` — defined Task 4/5, used Task 7/8. ✓
- `ListFilter::default_limit` — defined Task 5 Step 2, used Task 5 tests. ✓
- `format::vsearch_line`, `vsearch_json`, `memory_json_with_distance` — defined Task 6, used Task 7. ✓
- `MemError::{EmbeddingDimMismatch, EmbeddingParseError, VectorNotInitialized}` — defined Task 3, mapped Task 3, raised Task 4/5/7/8. ✓
- `row_to_item` visibility change (Task 4 Step 1) precedes its use in `vectors::search` (Task 5 Step 4). ✓

**Execution order:** Tasks are sequenced so each compiles and tests green before the next. Tasks 1–3 are foundation; 4–5 build the store; 6 is independent; 7–8 are the CLI; 9 is closeout.
