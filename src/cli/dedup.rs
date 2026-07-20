use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::MemResult;
use crate::store::memories;

#[derive(ClapArgs, Debug)]
pub struct Args {}

/// Collapse existing in-scope duplicate memories (keep oldest, merge tags,
/// delete the rest). Useful after enabling dedup to clean up rows added before
/// it existed, or after `--no-dedup` adds.
pub fn run(conn: &Connection, _args: Args, json: bool) -> MemResult<()> {
    let n = memories::collapse_duplicates(conn)?;
    if json {
        println!("{{\"collapsed\":{n}}}");
    } else {
        println!("collapsed {n} duplicate row(s)");
    }
    Ok(())
}
