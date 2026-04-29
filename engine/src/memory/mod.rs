use std::path::Path;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct MemoryManager {
    conn: Connection,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MemoryType {
    Identity,
    Situational
}

impl MemoryManager {
    pub fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;

        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        conn.execute_batch(
            "
                CREATE TABLE IF NOT EXISTS identity (
                    key TEXT PRIMARY key,
                    value TEXT
                );
                CREATE TABLE IF NOT EXISTS memories (
                    key TEXT PRIMARY KEY, 
                    value TEXT
                );
                CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                    key, value, content='memories'
                );

                CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                    INSERT INTO memories_fts(rowid, key, value) VALUES (new.rowid, new.key, new.value);
                END;
                CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                    INSERT INTO memories_fts(memories_fts, rowid, key, value) VALUES('delete', old.rowid, old.key, old.value);
                END;
                CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                    INSERT INTO memories_fts(memories_fts, rowid, key, value) VALUES('delete', old.rowid, old.key, old.value);
                    INSERT INTO memories_fts(rowid, key, value) VALUES (new.rowid, new.key, new.value);
                END;
            ",
        )?;

        Ok(Self { conn })
    }

    pub fn save(&self, key: &str, value: &str, memory_type: MemoryType) -> anyhow::Result<()> {
        match memory_type {
            MemoryType::Identity => self.save_identity(key, value)?,
            MemoryType::Situational => self.save_situational(key, value)?
        };
        Ok(())
    }

    pub fn save_identity(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO identity (key, value) VALUES (?1, ?2)", 
            params![key, value]
        )?;

        Ok(())
    }

    pub fn save_situational(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO memories (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;

        Ok(())
    }

    pub fn get_core_identity(&self) -> anyhow::Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM identity")?;

        let items = stmt
            .query_map([], |row| {
                let key: String = row.get(0)?;
                let value: String = row.get(1)?;
                Ok(format!("{}: {}", key, value))
            })?
            .collect::<Result<Vec<String>, rusqlite::Error>>()?;

        Ok(items)
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
