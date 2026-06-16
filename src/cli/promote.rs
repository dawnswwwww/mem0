use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::MemResult;
use crate::core::memory::Lifecycle;
use crate::store::memories;

#[derive(ClapArgs, Debug)]
pub struct Args {
    pub id: String,
    #[arg(long, default_value_t = Lifecycle::Semantic, value_enum)]
    pub to: Lifecycle,
    #[arg(long)]
    pub session: Option<String>,
}

pub fn run(conn: &Connection, args: Args, _json: bool) -> MemResult<()> {
    let id = memories::resolve_id(conn, &args.id)?;
    let _ = args.session;
    let updated = memories::set_lifecycle(conn, id, args.to)?;
    println!("promoted to {} ({})", updated.lifecycle, &updated.id.to_string()[..8]);
    Ok(())
}
