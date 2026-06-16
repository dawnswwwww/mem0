use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::MemResult;

#[derive(ClapArgs, Debug)]
pub struct Args {}

pub fn run(conn: &Connection, _args: Args, _json: bool) -> MemResult<()> {
    conn.execute_batch("VACUUM")?;
    println!("compacted");
    Ok(())
}
