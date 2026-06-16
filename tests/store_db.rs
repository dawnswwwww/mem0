use mem0::store::db;

#[test]
fn open_creates_file_and_enables_wal() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.db");

    let conn = db::open(&path).unwrap();

    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}
