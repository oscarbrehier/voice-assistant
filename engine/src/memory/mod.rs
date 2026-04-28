use std::path::Path;

use rusqlite::{Connection, params};

pub struct MemoryManager {
    conn: Connection,
}

impl MemoryManager {
    pub fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;

        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(key, value)",
            [],
        )?;

        Ok(Self { conn })
    }

    pub fn save(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO memories_fts (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;

        Ok(())
    }

    pub fn get_all_memories(&self) -> anyhow::Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM memories_fts")?;

        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut results = Vec::new();

        for item in rows {
            results.push(item?);
        }

        Ok(results)
    }

    pub fn get_relevant_memories(&self, user_input: &str) -> anyhow::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT value FROM memories_fts WHERE memories_fts MATCH ? LIMIT 10",
        )?;

        let rows = stmt.query_map([user_input], |row| row.get(0))?;

        let mut relevant = Vec::new();
        for item in rows {
            if let Ok(val) = item {
                relevant.push(val);
            }
        }

        Ok(relevant)
    }
}
