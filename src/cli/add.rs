use std::io::{IsTerminal, Read};

use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::{MemError, MemResult};
use crate::core::memory::Lifecycle;
use crate::output::format;
use crate::store::memories::{self, MemoryDraft};
use crate::store::vectors;

/// If stdin is piped, parse `{"embedding":[...]}`. If stdin is a terminal (no pipe),
/// return `Ok(None)` so text-only `add` is unchanged. A piped-but-invalid payload is
/// an error.
fn maybe_read_vector() -> MemResult<Option<Vec<f32>>> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Ok(None);
    }
    let mut raw = String::new();
    stdin.read_to_string(&mut raw)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let v: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|e| MemError::EmbeddingParseError(e.to_string()))?;
    let arr = v
        .get("embedding")
        .and_then(|e| e.as_array())
        .ok_or_else(|| MemError::EmbeddingParseError("missing 'embedding' array".into()))?;
    let out: Vec<f32> = arr
        .iter()
        .map(|x| {
            x.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| MemError::EmbeddingParseError("embedding has non-numeric element".into()))
        })
        .collect::<MemResult<_>>()?;
    Ok(Some(out))
}

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub content: Vec<String>,
    #[arg(long, value_enum)]
    pub to: Lifecycle,
    #[arg(long)]
    pub tag: Vec<String>,
    #[arg(long)]
    pub session: Option<String>,
}

pub fn run(conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    let content = args.content.join(" ");
    if content.is_empty() {
        return Err(MemError::InvalidArgument("content cannot be empty".into()));
    }

    // Resolve session name -> id (only meaningful for episodic; but allow for working too).
    let session_id = match args.session.as_deref() {
        Some(name) => Some(crate::store::sessions::get(conn, name)?.id),
        None => None,
    };

    // working -> episodic requires a session
    if args.to == Lifecycle::Episodic && session_id.is_none() {
        return Err(MemError::InvalidArgument(
            "--to=episodic requires --session=<name>".into(),
        ));
    }

    let draft = MemoryDraft {
        lifecycle:  args.to,
        content,
        tags:       args.tag,
        session_id,
        source:     Some("cli".into()),
    };

    // Read the vector FIRST (outside the transaction) so we don't hold BEGIN
    // across stdin IO. Then insert+upsert atomically: an upsert failure rolls
    // back the memory row.
    let vec_opt = maybe_read_vector()?;

    conn.execute_batch("BEGIN")?;
    let result = (|| -> MemResult<_> {
        let id = memories::insert(conn, &draft)?;
        let item = memories::get(conn, id)?;
        if let Some(vec) = &vec_opt {
            let rowid = conn.last_insert_rowid();
            vectors::upsert(conn, rowid, vec)?;
        }
        Ok(item)
    })();
    let item = match result {
        Ok(item) => {
            conn.execute_batch("COMMIT")?;
            item
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&format::memory_json(&item))?);
    } else {
        println!("stored {} as {}", &item.id.to_string()[..8], item.lifecycle);
    }
    Ok(())
}
