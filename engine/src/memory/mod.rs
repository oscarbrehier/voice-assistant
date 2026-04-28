use std::path::Path;

use rusqlite::{Connection, params};

pub struct MemoryManager {
    conn: Connection,
}

impl MemoryManager {
    pub fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;

        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        conn.execute_batch(
            "
                CREATE TABLE IF NOT EXISTS memories (
                    key TEXT PRIMARY KEY, 
                    value TEXT
                );
                CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                    key, value, content='memories'
                );
            ",
        )?;
        
        Ok(Self { conn })
    }

    pub fn save(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO memories (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;

        Ok(())
    }

    pub fn get_relevant_memories(&self, user_input: &str) -> anyhow::Result<Vec<String>> {
        let clean_query = user_input
            .split_whitespace()
            .map(|w| {
                w.chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>()
            })
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" OR ");

        if clean_query.is_empty() {
            return Ok(vec![]);
        }

        let mut stmt = self.conn.prepare(
            "SELECT key, value FROM memories_fts 
                WHERE memories_fts MATCH ? 
                ORDER BY rank 
                LIMIT 10",
        )?;

        let relevant = stmt
            .query_map([clean_query], |row| {
                let key: String = row.get(0)?;
                let value: String = row.get(1)?;
                Ok(format!("{}: {}", key, value))
            })?
            .collect::<Result<Vec<String>, rusqlite::Error>>()?;

        Ok(relevant)
    }
}
