use std::{collections::HashMap, fs, time::{SystemTime, UNIX_EPOCH}};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{Opt, commands::CommandConfig, llm::{history::ConversationHistory, mistral::call_mistral_with_history}};

pub mod history;
pub mod mistral;

#[derive(Debug, Serialize, Deserialize)]
pub struct LLMResponse {
	pub(crate) action: Option<String>,
	pub(crate) message: String,
	#[serde(default)]
	pub(crate) params: Option<HashMap<String, String>>
}

pub struct LLMEngine {
	last_updated: u64,
	history: ConversationHistory
}

impl LLMEngine {
	pub fn new(config: &CommandConfig) -> Self {

		let system_prompt = generate_system_prompt(&config)
			.expect("Failed to generate system prompt");

		let last_updated = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs();

		let history = ConversationHistory::new(&system_prompt);

		Self {
			last_updated,
			history
		}

	}

	#[instrument(skip(self, text), fields(input = %text))]
	pub async fn generate(&mut self, text: &str) -> anyhow::Result<LLMResponse> {

		self.history.add_user_input(text);
		
		for message in &self.history.messages {
			println!("{} - {}\n\n", message.role, message.content);
		}

		let (response, raw_json) = call_mistral_with_history(&mut self.history).await?;

		self.history.add_assistant_response(&raw_json);

		Ok(response)

	}
}

fn generate_system_prompt(config: &CommandConfig) -> anyhow::Result<String> {

	let mut commands_str = String::new();
	let system_prompt = fs::read_to_string("config/system_prompt.txt")
		.expect("System prompt template file not found in config");

	for command in &config.static_commands {
		commands_str.push_str(&format!("- {}: {}\n", command.action, command.description));
	}

	for command in &config.dynamic_commands {

		let param_placeholder = command.arg_types
			.iter()
			.map(|arg| format!("{{{}}}", arg))
			.collect::<Vec<_>>()
			.join(" ");

		commands_str.push_str(&format!(
			"- {} {}: {}\n",
			command.action,
			param_placeholder,
			command.description
		));
	}

	let system_prompt = system_prompt.replace("{{actions}}", &commands_str);
	
	Ok(system_prompt)

}