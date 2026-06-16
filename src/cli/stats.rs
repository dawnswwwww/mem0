use clap::Args as ClapArgs;
use rusqlite::Connection;

use crate::core::error::MemResult;
use crate::core::memory::Lifecycle;
use crate::store::memories;

#[derive(ClapArgs, Debug)]
pub struct Args {}

pub fn run(conn: &Connection, _args: Args, json: bool) -> MemResult<()> {
    let counts = memories::count_by_layer(conn)?;
    if json {
        let mut obj = serde_json::Map::new();
        for lc in Lifecycle::ALL {
            obj.insert(lc.to_string(), serde_json::json!(counts.get(&lc).copied().unwrap_or(0)));
        }
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(obj))?);
    } else {
        for lc in Lifecycle::ALL {
            println!("{:>8}: {}", lc, counts.get(&lc).copied().unwrap_or(0));
        }
    }
    Ok(())
}
