use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::{MemError, MemResult};
use crate::core::memory::Lifecycle;
use crate::output::format;
use crate::store::memories::{self, MemoryDraft};

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
    let id = memories::insert(conn, &draft)?;
    let item = memories::get(conn, id)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&format::memory_json(&item))?);
    } else {
        println!("stored {} as {}", &id.to_string()[..8], item.lifecycle);
    }
    Ok(())
}
