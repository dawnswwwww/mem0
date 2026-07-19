use mem0::core::Lifecycle;
use mem0::store::memories::{self, normalize_content, store, ListFilter, MemoryDraft, StoreAction};
use mem0::store::{db, sessions};
use rusqlite::Connection;
use tempfile::TempDir;

fn fresh() -> (TempDir, Connection) {
    let tmp = tempfile::tempdir().unwrap();
    let conn = db::open(&tmp.path().join("t.db")).unwrap();
    db::migrate(&conn).unwrap();
    (tmp, conn)
}

fn draft(lc: Lifecycle, content: &str, tags: Vec<String>, sid: Option<uuid::Uuid>) -> MemoryDraft {
    MemoryDraft { lifecycle: lc, content: content.into(), tags, session_id: sid, source: None }
}

#[test]
fn normalize_collapses_whitespace_keeps_case() {
    assert_eq!(normalize_content("  hello   world  "), "hello world");
    assert_eq!(normalize_content("\thello\nworld"), "hello world");
    assert_eq!(normalize_content("JST"), "JST");
    // NOT casefolded — case-sensitive tokens stay distinct.
    assert_ne!(normalize_content("JST"), normalize_content("jst"));
}

#[test]
fn semantic_dedups_globally_ignoring_session() {
    let (_t, c) = fresh();
    let s1 = sessions::new(&c, "s1").unwrap().id;
    let s2 = sessions::new(&c, "s2").unwrap().id;
    // Same content in two different sessions, semantic layer -> still one row.
    let (id1, a1) = store(&c, &draft(Lifecycle::Semantic, "user likes whiskey", vec![], Some(s1)), true).unwrap();
    let (id2, a2) = store(&c, &draft(Lifecycle::Semantic, "user likes whiskey", vec![], Some(s2)), true).unwrap();
    assert_eq!(a1, StoreAction::Inserted);
    assert_eq!(a2, StoreAction::Touched);
    assert_eq!(id1, id2, "semantic dup touches the existing row");
    assert_eq!(memories::list(&c, ListFilter::default_limit(100)).unwrap().len(), 1);
}

#[test]
fn episodic_dedups_within_session_not_across() {
    let (_t, c) = fresh();
    let s1 = sessions::new(&c, "s1").unwrap().id;
    let s2 = sessions::new(&c, "s2").unwrap().id;
    let (_, a1) = store(&c, &draft(Lifecycle::Episodic, "decided X", vec![], Some(s1)), true).unwrap();
    let (_, a2) = store(&c, &draft(Lifecycle::Episodic, "decided X", vec![], Some(s1)), true).unwrap();
    let (_, a3) = store(&c, &draft(Lifecycle::Episodic, "decided X", vec![], Some(s2)), true).unwrap();
    assert_eq!(a1, StoreAction::Inserted);
    assert_eq!(a2, StoreAction::Touched, "same session -> dedup");
    assert_eq!(a3, StoreAction::Inserted, "different session -> distinct event");
    assert_eq!(memories::list(&c, ListFilter::default_limit(100)).unwrap().len(), 2);
}

#[test]
fn no_dedup_inserts_literal_duplicate() {
    let (_t, c) = fresh();
    let (id1, a1) = store(&c, &draft(Lifecycle::Semantic, "dup", vec![], None), false).unwrap();
    let (id2, a2) = store(&c, &draft(Lifecycle::Semantic, "dup", vec![], None), false).unwrap();
    assert_eq!((a1, a2), (StoreAction::Inserted, StoreAction::Inserted));
    assert_ne!(id1, id2, "no_dedup creates a new row");
}

#[test]
fn touch_merges_tags_and_normalizes_whitespace() {
    let (_t, c) = fresh();
    let (id1, _) = store(&c, &draft(Lifecycle::Semantic, "fact", vec!["a".into()], None), true).unwrap();
    let before = memories::get(&c, id1).unwrap();
    // Whitespace-padded variant still dedups against "fact".
    let (id2, a2) = store(&c, &draft(Lifecycle::Semantic, "  fact  ", vec!["b".into()], None), true).unwrap();
    assert_eq!(a2, StoreAction::Touched);
    assert_eq!(id1, id2);
    let after = memories::get(&c, id1).unwrap();
    assert_eq!(after.tags, vec!["a".to_string(), "b".to_string()], "tags unioned, existing first");
    assert!(after.updated_at >= before.updated_at, "updated_at refreshed");
}
