use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::MemResult;
use crate::store::memories;

#[derive(ClapArgs, Debug)]
pub struct Args { pub id: String }

pub fn run(conn: &Connection, args: Args, _json: bool) -> MemResult<()> {
    let id = memories::resolve_id(conn, &args.id)?;
    memories::delete(conn, id)?;
    println!("deleted {}", &id.to_string()[..8]);
    Ok(())
}
