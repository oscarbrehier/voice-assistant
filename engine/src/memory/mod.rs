use std::path::Path;

use rusqlite::{Connection, params};

pub struct MemoryManager {
    conn: Connection,
}

impl MemoryManager {
    fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY,
                key TEXT NOT NULL UNIQUE,
                value TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
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

    pub fn get_all_memories(&self) -> anyhow::Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM memories")?;

        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut results = Vec::new();

        for item in rows {
            results.push(item?);
        }

        Ok(results)
    }

    pub fn get_relevant_memories(&self, user_input: &str) -> anyhow::Result<Vec<String>> {
        let words: Vec<&str> = user_input.split_whitespace().collect();
        if words.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders = words
            .iter()
            .map(|_| "(value LIKE ? OR key LIKE ?)")
            .collect::<Vec<_>>()
            .join("OR");
        let query = format!(
            "SELECT DISTINCT value FROM memories WHERE {} LIMIT 10",
            placeholders
        );

        let mut stmt = self.conn.prepare(&query)?;

        let params: Vec<String> = words
            .iter()
            .flat_map(|word| vec![format!("%{}%", word), format!("%{}%", word)])
            .collect();

		let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

		let rows = stmt.query_map(rusqlite::params_from_iter(params_refs), |row| row.get(0))?;

        let mut relevant = Vec::new();

		for item in rows {
			if let Ok(val) = item {
				relevant.push(val);
			}
		}

        Ok(relevant)
    }
}
