//! Tests for the v1 → v2 schema migration.
//!
//! These tests build a v1-shaped DB manually (using the original `V1_SCHEMA`
//! with `tags` as a JSON column and FTS5 indexing it directly) and verify
//! that `store::db::migrate()` upgrades it to v2 cleanly.

use rusqlite::Connection;
use tempfile::TempDir;

use mem0::store::db;
use mem0::store::migrations;

/// Build a fresh v1-shaped DB without going through migrate(). Used to
/// simulate a v0.1.0 user upgrading to v1.1.
fn v1_db() -> (TempDir, Connection) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let conn = db::open(&path).unwrap();
    conn.execute_batch(migrations::V1_SCHEMA).unwrap();
    // v0.1.0 did not set user_version, so leave it at 0.
    let _: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    (tmp, conn)
}

#[test]
fn v1_db_helper_has_no_tags_text_column() {
    let (_tmp, conn) = v1_db();
    let cols: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('memories')")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(!cols.contains(&"tags_text".to_string()),
        "v1 helper should not have tags_text; got {cols:?}");
}
