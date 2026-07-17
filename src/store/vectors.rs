use rusqlite::{params, Connection};

use crate::core::error::{MemError, MemResult};
use crate::store::memories::{row_to_item, ListFilter, MemoryItem};

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
    match conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![DIM_KEY],
        |r| r.get::<_, String>(0),
    ) {
        Ok(s) => Ok(s.parse::<usize>().ok()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
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
/// `meta` and create the vec0 table configured for cosine distance. Subsequent
/// calls must match.
///
/// sqlite-vec 0.1.9's `float[N]` column type defaults to L2 (Euclidean) distance,
/// which is unsuitable for embedding similarity. Cosine is requested via the
/// column-inline `distance_metric=cosine` option (note: inside the column spec,
/// with NO comma separating it as a table option — `vec0(embedding float[N]
/// distance_metric=cosine)`, not `vec0(embedding float[N], distance_metric=cosine)`).
/// The comma form is rejected with "Unknown table option".
pub fn ensure_vec_table(conn: &Connection, dim: usize) -> MemResult<()> {
    match read_dim(conn)? {
        None => {
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec \
                 USING vec0(embedding float[{dim}] distance_metric=cosine)"
            ))?;
            write_dim(conn, dim)?;
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

/// Run cosine KNN for `query`, then filter by layer/session and return up to
/// `filter.limit` hits as `(MemoryItem, distance)`. Lower distance = nearer.
///
/// Strategy (per spec): a pure KNN fetch over an expanded window, then a single
/// `memories` lookup (selecting rowid) that applies layer/session filters. This
/// keeps the KNN query in the exact form sqlite-vec requires and reuses the
/// existing filter columns.
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

    let knn_limit = filter.limit.saturating_mul(5).clamp(100, 1000);

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
