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
            conn.execute_batch(
                "CREATE TRIGGER IF NOT EXISTS memories_vec_ad AFTER DELETE ON memories BEGIN \
                   DELETE FROM memories_vec WHERE rowid = old.rowid; \
                 END",
            )?;
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

    // 3. Preserve KNN distance order, optionally drop distant noise, optionally
    //    apply gap-based auto-cutoff, then cap.
    out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    if let Some(maxd) = filter.max_distance {
        out.retain(|(_, d)| *d <= maxd);
    }
    if filter.auto_cutoff && out.len() > 1 {
        let dists: Vec<f64> = out.iter().map(|(_, d)| *d).collect();
        let keep = gap_cutoff(&dists);
        out.truncate(keep);
    }
    let limit = if filter.limit == 0 { 20 } else { filter.limit };
    out.truncate(limit.min(1000) as usize);
    Ok(out)
}

/// Gap-based auto-cutoff: given distances sorted ascending, return how many of
/// the leading (nearest) hits to keep. Cuts after the largest distance gap when
/// that gap is a clear outlier (both absolutely noticeable and ≥ 2× the
/// next-largest gap); otherwise keeps all. Counting-metric distances only.
///
/// Rationale: small embedding models compress cosine distances, so irrelevant
/// hits cluster just behind the relevant ones. The relevant hits form a tight
/// leading cluster; the jump to the noise tail is the largest gap. Cutting there
/// adapts per-query instead of a fixed threshold.
fn gap_cutoff(distances: &[f64]) -> usize {
    /// A distance gap below this is not "noticeable" enough to justify cutting
    /// (tuned for cosine distance on small embedding models, ~0.0–0.3 range).
    const MIN_GAP: f64 = 0.03;

    let n = distances.len();
    if n < 3 {
        return n; // too few points to judge cluster structure
    }
    let gaps: Vec<f64> = distances.windows(2).map(|w| w[1] - w[0]).collect();
    // Largest gap, and the next-largest (to test whether the max is an outlier).
    let mut desc: Vec<f64> = gaps.clone();
    desc.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let max_gap = desc[0];
    let second = desc.get(1).copied().unwrap_or(0.0);
    if max_gap < MIN_GAP || max_gap < 2.0 * second {
        return n; // no clear gap — keep everything
    }
    // Cut after the first occurrence of the max gap.
    let idx = gaps
        .iter()
        .position(|g| (*g - max_gap).abs() < 1e-12)
        .unwrap_or(0);
    idx + 1
}

#[cfg(test)]
mod tests {
    use super::gap_cutoff;

    #[test]
    fn cuts_at_clear_gap_after_leading_cluster() {
        // One relevant hit, then a big jump to noise -> keep only the first.
        let d = [0.102, 0.153, 0.156, 0.157, 0.160];
        assert_eq!(gap_cutoff(&d), 1);
    }

    #[test]
    fn keeps_cluster_before_gap() {
        // Three relevant hits clustered, then noise -> keep the three.
        let d = [0.10, 0.11, 0.12, 0.30, 0.31, 0.32];
        assert_eq!(gap_cutoff(&d), 3);
    }

    #[test]
    fn no_cut_when_distances_spread_evenly() {
        let d = [0.10, 0.12, 0.14, 0.16, 0.18];
        assert_eq!(gap_cutoff(&d), d.len(), "even spread -> keep all");
    }

    #[test]
    fn no_cut_when_gaps_below_floor() {
        // Tightly packed (all gaps tiny) -> keep all even if one is the largest.
        let d = [0.100, 0.101, 0.103, 0.104];
        assert_eq!(gap_cutoff(&d), d.len());
    }

    #[test]
    fn too_few_to_judge_keeps_all() {
        assert_eq!(gap_cutoff(&[0.1]), 1);
        assert_eq!(gap_cutoff(&[0.1, 0.5]), 2);
    }
}
