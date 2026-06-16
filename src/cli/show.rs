use rusqlite::Connection;
use crate::core::MemResult;

#[derive(clap::Args, Debug)]
pub struct Args { pub id: String }

pub fn run(_conn: &Connection, _args: Args, _json: bool) -> MemResult<()> { unimplemented!("mem0 show — Task 20") }
