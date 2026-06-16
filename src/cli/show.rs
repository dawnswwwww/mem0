use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::MemResult;
use crate::output::format;
use crate::store::memories;

#[derive(ClapArgs, Debug)]
pub struct Args { pub id: String }

pub fn run(conn: &Connection, args: Args, json: bool) -> MemResult<()> {
    let id = memories::resolve_id(conn, &args.id)?;
    let m = memories::get(conn, id)?;
    println!("{}", serde_json::to_string_pretty(&format::memory_json(&m))?);
    let _ = json; // both modes use the same JSON output for show
    Ok(())
}
