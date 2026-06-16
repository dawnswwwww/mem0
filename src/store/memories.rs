use rusqlite::{Connection, OptionalExtension, Row};

use crate::core::error::{MemError, MemResult};
use crate::core::ids;
use crate::core::memory::Lifecycle;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryItem {
    pub id:          uuid::Uuid,
    pub lifecycle:   Lifecycle,
    pub content:     String,
    pub source:      Option<String>,
    pub session_id:  Option<uuid::Uuid>,
    pub tags:        Vec<String>,
    pub created_at:  i64,
    pub updated_at:  i64,
    pub accessed_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MemoryDraft {
    pub lifecycle:  Lifecycle,
    pub content:    String,
    pub tags:       Vec<String>,
    pub session_id: Option<uuid::Uuid>,
    pub source:     Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub layer:        Option<Lifecycle>,
    pub session:      Option<uuid::Uuid>,
    pub since_nanos:  Option<i64>,
    pub limit:        u32,
}

fn now_nanos() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn parse_lifecycle(s: &str) -> MemResult<Lifecycle> {
    s.parse()
}

fn row_to_item(row: &Row<'_>) -> rusqlite::Result<MemoryItem> {
    let id_s: String = row.get("id")?;
    let lifecycle_s: String = row.get("lifecycle")?;
    let session_s: Option<String> = row.get("session_id")?;
    let tags_s: String = row.get("tags")?;
    Ok(MemoryItem {
        id:          ids::parse(&id_s).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))))?,
        lifecycle:   parse_lifecycle(&lifecycle_s).map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))))?,
        content:     row.get("content")?,
        source:      row.get("source")?,
        session_id:  session_s.map(|s| ids::parse(&s)).transpose().map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))))?,
        tags:        serde_json::from_str(&tags_s).map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))))?,
        created_at:  row.get("created_at")?,
        updated_at:  row.get("updated_at")?,
        accessed_at: row.get("accessed_at")?,
    })
}

pub fn insert(conn: &Connection, draft: &MemoryDraft) -> MemResult<uuid::Uuid> {
    if draft.content.is_empty() {
        return Err(MemError::InvalidArgument("content cannot be empty".into()));
    }
    let id = ids::new_v7();
    let ts = now_nanos();
    let tags_json = serde_json::to_string(&draft.tags)?;
    conn.execute(
        "INSERT INTO memories (id, lifecycle, content, source, session_id, tags, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        rusqlite::params![
            id.to_string(),
            draft.lifecycle.to_string(),
            draft.content,
            draft.source,
            draft.session_id.map(|u| u.to_string()),
            tags_json,
            ts,
        ],
    )?;
    Ok(id)
}

pub fn get(conn: &Connection, id: uuid::Uuid) -> MemResult<MemoryItem> {
    let row = conn
        .query_row(
            "SELECT id, lifecycle, content, source, session_id, tags, created_at, updated_at, accessed_at \
             FROM memories WHERE id = ?1",
            rusqlite::params![id.to_string()],
            row_to_item,
        )
        .optional()?;
    row.ok_or_else(|| MemError::NotFound(id.to_string()))
}

pub fn delete(conn: &Connection, id: uuid::Uuid) -> MemResult<()> {
    let n = conn.execute(
        "DELETE FROM memories WHERE id = ?1",
        rusqlite::params![id.to_string()],
    )?;
    if n == 0 { Err(MemError::NotFound(id.to_string())) } else { Ok(()) }
}

pub fn set_lifecycle(conn: &Connection, id: uuid::Uuid, target: Lifecycle) -> MemResult<MemoryItem> {
    let current = get(conn, id)?;
    if !current.lifecycle.can_transition_to(target) {
        return Err(MemError::InvalidTransition { from: current.lifecycle, to: target });
    }
    let ts = now_nanos();
    let n = conn.execute(
        "UPDATE memories SET lifecycle = ?1, updated_at = ?2 WHERE id = ?3 AND lifecycle = ?4",
        rusqlite::params![target.to_string(), ts, id.to_string(), current.lifecycle.to_string()],
    )?;
    if n == 0 {
        return Err(MemError::NotFound(id.to_string()));
    }
    get(conn, id)
}

pub fn list(conn: &Connection, filter: ListFilter) -> MemResult<Vec<MemoryItem>> {
    let mut sql = String::from(
        "SELECT id, lifecycle, content, source, session_id, tags, created_at, updated_at, accessed_at \
         FROM memories WHERE 1=1",
    );
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(layer) = filter.layer {
        sql.push_str(" AND lifecycle = ?");
        binds.push(Box::new(layer.to_string()));
    }
    if let Some(sid) = filter.session {
        sql.push_str(" AND session_id = ?");
        binds.push(Box::new(sid.to_string()));
    }
    if let Some(since) = filter.since_nanos {
        sql.push_str(" AND created_at >= ?");
        binds.push(Box::new(since));
    }
    sql.push_str(" ORDER BY created_at DESC");
    let limit = if filter.limit == 0 { 100 } else { filter.limit };
    sql.push_str(&format!(" LIMIT {}", limit.min(1000)));

    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| &**b as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_item)?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

pub fn resolve_id(conn: &Connection, id_or_prefix: &str) -> MemResult<uuid::Uuid> {
    if id_or_prefix.is_empty() {
        return Err(MemError::InvalidId("empty id".into()));
    }
    // Try full UUID first.
    if id_or_prefix.len() >= 32
        && let Ok(u) = uuid::Uuid::parse_str(id_or_prefix)
    {
        // Verify it exists.
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM memories WHERE id = ?1",
                rusqlite::params![u.to_string()],
                |r| r.get::<_, i32>(0).map(|_| true),
            )
            .optional()?
            .unwrap_or(false);
        if exists { return Ok(u); }
    }
    // Prefix search — must match exactly one row.
    let pattern = format!("{}%", id_or_prefix);
    let mut stmt = conn.prepare("SELECT id FROM memories WHERE id LIKE ?1 ORDER BY id LIMIT 2")?;
    let rows = stmt.query_map(rusqlite::params![pattern], |r| r.get::<_, String>(0))?;
    let mut hits: Vec<String> = Vec::new();
    for r in rows { hits.push(r?); }
    match hits.len() {
        0 => Err(MemError::NotFound(id_or_prefix.to_string())),
        1 => uuid::Uuid::parse_str(&hits[0])
            .map_err(|_| MemError::InvalidId(hits[0].clone())),
        _ => Err(MemError::InvalidId(format!("ambiguous prefix: {id_or_prefix}"))),
    }
}

pub fn search(conn: &Connection, query: &str, filter: ListFilter) -> MemResult<Vec<MemoryItem>> {
    if query.trim().is_empty() {
        return Err(MemError::InvalidArgument("search query cannot be empty".into()));
    }
    // FTS5 MATCH — assume caller has not injected FTS5 operators; quote-escape
    // the whole query to neutralize syntax. Strip surrounding quotes first.
    let safe = query.replace('"', "\"\"");
    let fts_query = format!("\"{safe}\"");

    let mut sql = String::from(
        "SELECT m.id, m.lifecycle, m.content, m.source, m.session_id, m.tags, m.created_at, m.updated_at, m.accessed_at \
         FROM memories_fts f \
         JOIN memories m ON m.rowid = f.rowid \
         WHERE memories_fts MATCH ?1",
    );
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(fts_query)];
    if let Some(layer) = filter.layer {
        sql.push_str(" AND m.lifecycle = ?");
        binds.push(Box::new(layer.to_string()));
    }
    if let Some(sid) = filter.session {
        sql.push_str(" AND m.session_id = ?");
        binds.push(Box::new(sid.to_string()));
    }
    if let Some(since) = filter.since_nanos {
        sql.push_str(" AND m.created_at >= ?");
        binds.push(Box::new(since));
    }
    sql.push_str(" ORDER BY f.rank");
    let limit = if filter.limit == 0 { 20 } else { filter.limit };
    sql.push_str(&format!(" LIMIT {}", limit.min(1000)));

    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| &**b as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_item)?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

pub fn count_by_layer(conn: &Connection) -> MemResult<std::collections::HashMap<Lifecycle, u64>> {
    let mut stmt = conn.prepare("SELECT lifecycle, count(*) FROM memories GROUP BY lifecycle")?;
    let rows = stmt.query_map([], |r| {
        let l: String = r.get(0)?;
        let c: i64 = r.get(1)?;
        Ok((l, c))
    })?;
    let mut out = std::collections::HashMap::new();
    for r in rows {
        let (l, c) = r?;
        let lc = l.parse::<Lifecycle>()?;
        out.insert(lc, c as u64);
    }
    Ok(out)
}
