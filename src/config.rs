use std::fs;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
	trigger_word: String
}

impl Config {
	pub fn load(config_path: &str) -> anyhow::Result<Self> {

		let content = fs::read_to_string(config_path)
			.map_err(|e| anyhow::anyhow!("Failed to read config file: {e}"))?;

		let config: Config = serde_json::from_str(&content)?;

		Ok(config)

	}
}