use std::path::Path;
use std::sync::Once;

use rusqlite::{Connection, OpenFlags};

use crate::core::MemResult;

/// Register the sqlite-vec extension globally. Idempotent via `Once`. Must run
/// before any `Connection::open_*`; `open()` calls it first.
/// `register_auto_extension` makes every subsequently opened connection
/// auto-load the `vec0` module and surfaces registration failure immediately.
fn install_sqlite_vec() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        // SAFETY: `register_auto_extension` is unsafe because the auto-extension
        // must not open/close dbs or reentrantly mutate the extension list;
        // sqlite-vec's `sqlite3_vec_init` does neither. The transmute widens
        // the no-arg extern "C" declaration to the documented SQLite init
        // signature (`RawAutoExtension`).
        unsafe {
            rusqlite::auto_extension::register_auto_extension(
                std::mem::transmute::<*const (), rusqlite::auto_extension::RawAutoExtension>(
                    sqlite_vec::sqlite3_vec_init as *const (),
                ),
            )
        }
        .expect("register sqlite-vec auto extension");
    });
}

pub fn open(path: &Path) -> MemResult<Connection> {
    install_sqlite_vec();
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
