use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::core::MemResult;

pub fn open(path: &Path) -> MemResult<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    // Pragmas must be applied per-connection.
    // Use update_and_check for journal_mode so a silent WAL fallback to DELETE
    // (e.g. on a read-only or network filesystem) surfaces as an error.
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
    debug_assert_eq!(mode.to_lowercase(), "wal");

    conn.pragma_update(None, "synchronous", 1_i64)?; // NORMAL = 1
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "busy_timeout", 5000_i64)?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> MemResult<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    conn.execute_batch("BEGIN")?;
    let result = (|| -> MemResult<()> {
        if version < 1 {
            crate::store::migrations::apply_v1_initial(conn)?;
            conn.pragma_update(None, "user_version", 1_i64)?;
        }
        if version < 2 {
            crate::store::migrations::apply_v2_v1_1(conn)?;
            conn.pragma_update(None, "user_version", 2_i64)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}
