use mem0::store::{db, vectors};
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
    vectors::ensure_vec_table(&conn, 4).unwrap();
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
    vectors::upsert(&conn, rowid, &[9.0, 9.0, 9.0, 9.0]).unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM memories_vec WHERE rowid = ?", [rowid], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "upsert must replace, not duplicate");
}

#[test]
fn memories_vec_uses_cosine_distance() {
    let (_t, conn) = fresh();
    conn.execute(
        "INSERT INTO memories (id, lifecycle, content, tags, tags_text, created_at, updated_at) \
         VALUES ('00000000-0000-7000-0000-000000000001','semantic','x','[]','',1,1)",
        [],
    )
    .unwrap();
    let rowid = conn.last_insert_rowid();
    vectors::upsert(&conn, rowid, &[1.0, 0.0, 0.0, 0.0]).unwrap();

    // Same direction, different magnitude: cosine distance ~ 0; L2 would be 4.0.
    let d: f64 = conn
        .query_row(
            "SELECT distance FROM memories_vec WHERE embedding MATCH ? \
             ORDER BY distance LIMIT 1",
            [&vectors::f32_to_blob(&[5.0, 0.0, 0.0, 0.0])],
            |r| r.get(0),
        )
        .unwrap();
    assert!(d < 0.001, "expected cosine distance ~0, got {d}");
}
