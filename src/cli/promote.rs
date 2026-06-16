use rusqlite::Connection;
use crate::core::MemResult;

#[derive(clap::Args, Debug)]
pub struct Args {
    pub id: String,
    #[arg(long, default_value = "semantic")]
    pub to: String,
    #[arg(long)]
    pub session: Option<String>,
}

pub fn run(_conn: &Connection, _args: Args, _json: bool) -> MemResult<()> { unimplemented!("mem0 promote — Task 22") }
