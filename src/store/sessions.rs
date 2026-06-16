use rusqlite::{Connection, OptionalExtension, Row};

use crate::core::error::{MemError, MemResult};
use crate::core::ids;

#[derive(Debug, Clone)]
pub struct Session {
    pub id:         uuid::Uuid,
    pub name:       String,
    pub created_at: i64,
    pub closed_at:  Option<i64>,
}

fn now_nanos() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn row_to_session(row: &Row<'_>) -> rusqlite::Result<Session> {
    let id_s: String = row.get("id")?;
    let id = ids::parse(&id_s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
        )
    })?;
    Ok(Session {
        id,
        name:       row.get("name")?,
        created_at: row.get("created_at")?,
        closed_at:  row.get("closed_at")?,
    })
}

pub fn new(conn: &Connection, name: &str) -> MemResult<Session> {
    if name.trim().is_empty() {
        return Err(MemError::InvalidArgument("session name cannot be empty".into()));
    }
    let id = ids::new_v7();
    let ts = now_nanos();
    conn.execute(
        "INSERT INTO sessions (id, name, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![id.to_string(), name, ts],
    )?;
    Ok(Session {
        id,
        name: name.to_string(),
        created_at: ts,
        closed_at: None,
    })
}

pub fn get(conn: &Connection, id_or_name: &str) -> MemResult<Session> {
    // Try full UUID first
    if let Ok(uuid) = ids::parse(id_or_name) {
        let row = conn
            .query_row(
                "SELECT id, name, created_at, closed_at FROM sessions WHERE id = ?1",
                rusqlite::params![uuid.to_string()],
                row_to_session,
            )
            .optional()?;
        if let Some(s) = row {
            return Ok(s);
        }
    }
    // Try id prefix (UUIDv7 first 8 hex chars are unique in practice)
    let prefix_like = format!("{id_or_name}%");
    let by_prefix = conn
        .query_row(
            "SELECT id, name, created_at, closed_at FROM sessions WHERE id LIKE ?1 LIMIT 1",
            rusqlite::params![prefix_like],
            row_to_session,
        )
        .optional()?;
    if let Some(s) = by_prefix {
        return Ok(s);
    }
    // Try name
    let by_name = conn
        .query_row(
            "SELECT id, name, created_at, closed_at FROM sessions WHERE name = ?1",
            rusqlite::params![id_or_name],
            row_to_session,
        )
        .optional()?;
    by_name.ok_or_else(|| MemError::NotFound(id_or_name.to_string()))
}

pub fn list(conn: &Connection) -> MemResult<Vec<Session>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, created_at, closed_at FROM sessions ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], row_to_session)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn close(conn: &Connection, id_or_name: &str) -> MemResult<()> {
    let s = get(conn, id_or_name)?;
    let ts = now_nanos();
    let updated = conn.execute(
        "UPDATE sessions SET closed_at = ?1 WHERE id = ?2",
        rusqlite::params![ts, s.id.to_string()],
    )?;
    if updated == 0 {
        return Err(MemError::NotFound(id_or_name.to_string()));
    }
    Ok(())
}
