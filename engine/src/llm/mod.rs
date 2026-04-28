use std::{collections::HashMap, fs, path::Path, time::{SystemTime, UNIX_EPOCH}};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{Opt, commands::CommandConfig, config::Config, llm::{history::ConversationHistory, mistral::call_mistral_with_history}};

pub mod history;
pub mod mistral;

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryEntry {
	pub key: String,
	pub value: String
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LLMResponse {
	pub(crate) action: Option<String>,
	pub(crate) message: String,
	#[serde(default)]
	pub(crate) params: Option<HashMap<String, String>>,
	pub(crate) save_to_memory: Option<MemoryEntry>
}

pub struct LLMEngine {
	last_updated: u64,
	history: ConversationHistory,
	system_prompt_template: String,
}

impl LLMEngine {
	pub fn new<P: AsRef<Path>>(prompt_path: P, config: &Config, commands: &CommandConfig) -> Self {

		let system_prompt_template = generate_system_prompt(prompt_path, config, commands)
			.expect("Failed to generate system prompt");

		let last_updated = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs();

		let history = ConversationHistory::new();

		Self {
			last_updated,
			history,
			system_prompt_template
		}

	}

	#[instrument(skip(self, text, relevant_memories), fields(input = %text))]
	pub async fn generate(&mut self, text: &str, relevant_memories: Vec<String>) -> anyhow::Result<LLMResponse> {

		self.history.add_user_input(text);
		
		for message in &self.history.messages {
			println!("{} - {}\n\n", message.role, message.content);
		}

		let (response, raw_json) = call_mistral_with_history(&self.system_prompt_template, &mut self.history, relevant_memories).await?;

		self.history.add_assistant_response(&raw_json);

		Ok(response)

	}
}

fn generate_system_prompt<P: AsRef<Path>>(prompt_path: P, config: &Config, commands: &CommandConfig) -> anyhow::Result<String> {

	let mut commands_str = String::new();
	let system_prompt = fs::read_to_string(prompt_path)
		.expect("System prompt template file not found in config");

	for command in &commands.static_commands {
		commands_str.push_str(&format!("- {}: {}\n", command.action, command.description));
	}

	for command in &commands.dynamic_commands {

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

	let system_prompt = system_prompt
		.replace("{{name}}", &config.name)
		.replace("{{actions}}", &commands_str);
	
	Ok(system_prompt)

}