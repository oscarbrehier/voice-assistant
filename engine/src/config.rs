use std::{fs, path::Path};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub name: String,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        println!("Attempting to open: {:?}", path.as_ref());

        let content = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file: {e}"))?;

        let config: Config = serde_json::from_str(&content)?;

        Ok(config)
    }
}
