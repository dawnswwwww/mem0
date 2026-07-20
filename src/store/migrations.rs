use rusqlite::Connection;

use crate::core::MemResult;

pub fn apply_v1_initial(conn: &Connection) -> MemResult<()> {
    conn.execute_batch(V1_SCHEMA)?;
    Ok(())
}

/// v1 → v2 migration:
///  1. ADD COLUMN tags_text
///  2. Backfill tags_text from existing tags JSON
///  3. DROP FTS5 + triggers
///  4. CREATE new FTS5 with trigram tokenizer indexing (content, tags_text)
///  5. Reindex from base table
///  6. CREATE new triggers referencing tags_text
///
/// Idempotent: column-existence guard + DROP IF EXISTS make retry safe.
pub fn apply_v2_v1_1(conn: &Connection) -> MemResult<()> {
    // 1. Add tags_text column if not present.
    let has_tags_text: bool = conn
        .query_row(
            "SELECT count(*) > 0 FROM pragma_table_info('memories') WHERE name = 'tags_text'",
            [],
            |r| r.get(0),
        )?;
    if !has_tags_text {
        conn.execute_batch(
            "ALTER TABLE memories ADD COLUMN tags_text TEXT NOT NULL DEFAULT ''",
        )?;
    }

    // 2. Backfill tags_text from existing tags JSON for all rows.
    //    Strip [ ] " , → space, collapse double spaces, lowercase.
    conn.execute_batch(
        "UPDATE memories SET tags_text = lower( \
           replace(replace(replace(replace(replace(tags, '[', ''), ']', ''), '\"', ''), ',', ' '), '  ', ' ') \
         ) WHERE tags_text = '' OR tags_text IS NULL",
    )?;

    // 3. Drop old FTS5 + triggers.
    conn.execute_batch("DROP TABLE IF EXISTS memories_fts")?;
    conn.execute_batch("DROP TRIGGER IF EXISTS memories_ai")?;
    conn.execute_batch("DROP TRIGGER IF EXISTS memories_ad")?;
    conn.execute_batch("DROP TRIGGER IF EXISTS memories_au")?;

    // 4. Create new FTS5 with trigram tokenizer.
    conn.execute_batch(
        "CREATE VIRTUAL TABLE memories_fts \
         USING fts5(content, tags_text, content='memories', content_rowid='rowid', tokenize='trigram')",
    )?;

    // 5. Reindex from base table.
    conn.execute_batch(
        "INSERT INTO memories_fts(rowid, content, tags_text) \
         SELECT rowid, content, tags_text FROM memories",
    )?;

    // 6. Create new triggers referencing tags_text.
    conn.execute_batch(
        "CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN \
           INSERT INTO memories_fts(rowid, content, tags_text) VALUES (new.rowid, new.content, new.tags_text); \
         END",
    )?;
    conn.execute_batch(
        "CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN \
           INSERT INTO memories_fts(memories_fts, rowid, content, tags_text) VALUES('delete', old.rowid, old.content, old.tags_text); \
         END",
    )?;
    conn.execute_batch(
        "CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN \
           INSERT INTO memories_fts(memories_fts, rowid, content, tags_text) VALUES('delete', old.rowid, old.content, old.tags_text); \
           INSERT INTO memories_fts(rowid, content, tags_text) VALUES (new.rowid, new.content, new.tags_text); \
         END",
    )?;

    Ok(())
}

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

/// v3 → v4 migration: add `content_key` (normalized content) for dedup, backfill
/// existing rows, and index it. Idempotent (column guard + IF NOT EXISTS index).
pub fn apply_v4_content_hash(conn: &Connection) -> MemResult<()> {
    let has_content_key: bool = conn.query_row(
        "SELECT count(*) > 0 FROM pragma_table_info('memories') WHERE name = 'content_key'",
        [],
        |r| r.get(0),
    )?;
    if !has_content_key {
        conn.execute_batch("ALTER TABLE memories ADD COLUMN content_key TEXT")?;
    }
    // Backfill any rows missing a content_key.
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT rowid, content FROM memories WHERE content_key IS NULL OR content_key = ''",
        )?;
        let mapped =
            stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (rowid, content) in &rows {
        let key = crate::store::memories::normalize_content(content);
        conn.execute(
            "UPDATE memories SET content_key = ?1 WHERE rowid = ?2",
            rusqlite::params![key, rowid],
        )?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_memories_content_key ON memories(lifecycle, content_key)",
    )?;
    Ok(())
}

pub const V1_SCHEMA: &str = r#"
-- 1. sessions table (parent, referenced by memories.session_id)
CREATE TABLE IF NOT EXISTS sessions (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,
  created_at  INTEGER NOT NULL,
  closed_at   INTEGER
);

-- 2. memories table (child, has session_id FK)
CREATE TABLE IF NOT EXISTS memories (
  id          TEXT PRIMARY KEY,
  lifecycle   TEXT NOT NULL CHECK (lifecycle IN ('working','episodic','semantic')),
  content     TEXT NOT NULL,
  source      TEXT,
  session_id  TEXT REFERENCES sessions(id) ON DELETE SET NULL,
  tags        TEXT NOT NULL DEFAULT '[]',
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  accessed_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_memories_layer_created
  ON memories(lifecycle, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memories_session
  ON memories(session_id) WHERE session_id IS NOT NULL;

CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
  USING fts5(content, tags, content='memories', content_rowid='rowid');

CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;
CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags) VALUES('delete', old.rowid, old.content, old.tags);
END;
CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, content, tags) VALUES('delete', old.rowid, old.content, old.tags);
  INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;
"#;
