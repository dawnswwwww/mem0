//! One-off migration: re-embed every memory with the built-in model.
//!
//! Use when the DB is locked to an incompatible dimension (e.g. vectors from a
//! previous external embedder) and you switch to the built-in `embed` model.
//! Drops the old `memories_vec` + dimension lock, then re-embeds all `memories`
//! rows in one batch (model loaded once) and re-inserts their vectors.
//!
//! Build: `cargo build --features embed --example reembed`
//! Run:   `./target/debug/examples/reembed "<path/to/mem0.db>"`
//!
//! Text memories are preserved; only their (incompatible) vectors are replaced.

#![cfg(feature = "embed")]

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = std::env::args()
        .nth(1)
        .expect("usage: reembed <path/to/mem0.db>");

    let conn = mem0::store::db::open(std::path::Path::new(&db))?;
    mem0::store::db::migrate(&conn)?;

    // 1. Reset vector state: drop the old (incompatible-dim) virtual table and the
    //    dimension lock. ensure_vec_table recreates `memories_vec` at the new dim on
    //    the first upsert below.
    let _ = conn.execute_batch("DROP TABLE IF EXISTS memories_vec");
    conn.execute("DELETE FROM meta WHERE key = 'embedding_dim'", [])?;

    // 2. Enumerate every memory's rowid + content.
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT rowid, content FROM memories ORDER BY rowid")?;
        let mapped = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let n = rows.len();
    if n == 0 {
        println!("no memories to re-embed; dim lock cleared.");
        return Ok(());
    }

    // 3. Embed all contents in ONE batch (one model load).
    let texts: Vec<&str> = rows.iter().map(|(_, c)| c.as_str()).collect();
    eprintln!("embedding {n} memories with multilingual-e5-small ...");
    let vecs = mem0::embed::embed_batch(
        &texts,
        mem0::embed::Role::Passage,
        mem0::embed::ModelChoice::DEFAULT,
    )?;

    // 4. Upsert each vector (first call recreates memories_vec at 384-dim).
    for ((rowid, _), vec) in rows.iter().zip(vecs.iter()) {
        mem0::store::vectors::upsert(&conn, *rowid, vec)?;
    }

    let dim: String =
        conn.query_row("SELECT value FROM meta WHERE key='embedding_dim'", [], |r| r.get(0))?;
    println!("re-embedded {n} memories; memories_vec now at {dim}-dim.");
    Ok(())
}
