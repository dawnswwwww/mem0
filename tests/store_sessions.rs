use mem0::store::{db, sessions};
use rusqlite::Connection;
use tempfile::TempDir;

fn fresh() -> (TempDir, Connection) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");
    let conn = db::open(&path).unwrap();
    db::migrate(&conn).unwrap();
    (tmp, conn)
}

#[test]
fn new_creates_row_with_unique_name() {
    let (_tmp, conn) = fresh();
    let s = sessions::new(&conn, "standup-0616").unwrap();
    assert_eq!(s.name, "standup-0616");
    assert!(s.closed_at.is_none());
}

#[test]
fn new_rejects_duplicate_name() {
    let (_tmp, conn) = fresh();
    sessions::new(&conn, "alpha").unwrap();
    let err = sessions::new(&conn, "alpha").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("UNIQUE") || msg.contains("unique"), "msg: {msg}");
}

#[test]
fn get_by_name_resolves() {
    let (_tmp, conn) = fresh();
    let s = sessions::new(&conn, "beta").unwrap();
    let found = sessions::get(&conn, &s.name).unwrap();
    assert_eq!(found.id, s.id);
}

#[test]
fn get_by_id_prefix() {
    let (_tmp, conn) = fresh();
    let s = sessions::new(&conn, "gamma").unwrap();
    let prefix = &s.id.to_string()[..8];
    let found = sessions::get(&conn, prefix).unwrap();
    assert_eq!(found.id, s.id);
}

#[test]
fn get_unknown_returns_not_found() {
    let (_tmp, conn) = fresh();
    let err = sessions::get(&conn, "no-such-thing").unwrap_err();
    assert!(matches!(err, mem0::MemError::NotFound(_)));
}

#[test]
fn close_sets_closed_at() {
    let (_tmp, conn) = fresh();
    let s = sessions::new(&conn, "delta").unwrap();
    assert!(s.closed_at.is_none());
    sessions::close(&conn, &s.name).unwrap();
    let fetched = sessions::get(&conn, &s.name).unwrap();
    assert!(fetched.closed_at.is_some());
}

#[test]
fn list_returns_all_in_creation_order() {
    let (_tmp, conn) = fresh();
    sessions::new(&conn, "a").unwrap();
    sessions::new(&conn, "b").unwrap();
    sessions::new(&conn, "c").unwrap();
    let names: Vec<_> = sessions::list(&conn).unwrap().into_iter().map(|s| s.name).collect();
    assert_eq!(names, vec!["a", "b", "c"]);
}
