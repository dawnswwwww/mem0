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
/// `meta` and create the vec0 table (default cosine distance for `float[N]`).
/// Subsequent calls must match.
///
/// Note: sqlite-vec 0.1.9 rejects the `distance_metric=cosine` table option with
/// "Unknown table option"; `float[N]` already defaults to cosine, so we rely on
/// the default rather than naming it.
pub fn ensure_vec_table(conn: &Connection, dim: usize) -> MemResult<()> {
    match read_dim(conn)? {
        None => {
            write_dim(conn, dim)?;
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec \
                 USING vec0(embedding float[{dim}])"
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
///
/// sqlite-vec 0.1.9's vec0 does not honor `INSERT OR REPLACE`, `INSERT OR IGNORE`,
/// or `ON CONFLICT … DO UPDATE` on its virtual PK (all raise errors), so we
/// implement replace as `DELETE` + `INSERT`. Deleting a missing row is a no-op.
pub fn upsert(conn: &Connection, rowid: i64, vec: &[f32]) -> MemResult<()> {
    ensure_vec_table(conn, vec.len())?;
    conn.execute(
        "DELETE FROM memories_vec WHERE rowid = ?1",
        params![rowid],
    )?;
    conn.execute(
        "INSERT INTO memories_vec(rowid, embedding) VALUES (?1, ?2)",
        params![rowid, f32_to_blob(vec)],
    )?;
    Ok(())
}
