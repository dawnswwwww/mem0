use rusqlite::Connection;
use crate::core::MemResult;

#[derive(clap::Args, Debug)]
pub struct Args {
    pub content: Vec<String>,
    #[arg(long)]
    pub to: String,
    #[arg(long)]
    pub tag: Vec<String>,
    #[arg(long)]
    pub session: Option<String>,
}

pub fn run(_conn: &Connection, _args: Args, _json: bool) -> MemResult<()> {
    unimplemented!("mem0 add — implementation lands in Task 18")
}
