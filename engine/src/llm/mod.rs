use std::{
    collections::HashMap,
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    Opt,
    commands::CommandConfig,
    config::Config,
    llm::{history::ConversationHistory, mistral::call_mistral_with_history},
    memory::{MemoryManager, MemoryType},
};

pub mod history;
pub mod mistral;

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LLMResponse {
    pub(crate) action: Option<String>,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) params: Option<HashMap<String, String>>,
    pub(crate) save_to_memory: Option<MemoryEntry>,
}

pub struct LLMEngine {
    last_updated: u64,
    history: ConversationHistory,
    system_prompt_template: String,
    core_identity_cache: String,
    pub needs_identity_refresh: bool,
}

impl LLMEngine {
    pub fn new<P: AsRef<Path>>(
        prompt_path: P,
        config: &Config,
        commands: &CommandConfig,
        memory: &MemoryManager,
    ) -> anyhow::Result<Self> {
        let system_prompt_template = generate_system_prompt(prompt_path, config, commands)
            .expect("Failed to generate system prompt");

        let last_updated = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let history = ConversationHistory::new();

        let core_identity_cache = memory.get_core_identity()?.join("\n");

        Ok(Self {
            last_updated,
            history,
            system_prompt_template,
            core_identity_cache,
            needs_identity_refresh: false,
        })
    }

    #[instrument(skip(self, text, core_identity, relevant_memories), fields(input = %text))]
    pub async fn generate(
        &mut self,
        text: &str,
        core_identity: Vec<String>,
		relevant_memories: Vec<String>
    ) -> anyhow::Result<LLMResponse> {
        if core_identity.is_empty() {
            self.core_identity_cache = core_identity.join("\n");
			self.needs_identity_refresh = false;
        }

        let situational_str = if relevant_memories.is_empty() {
            "No specific situational memories found for this query.".to_string()
        } else {
            relevant_memories.join("\n")
        };

        let final_system_prompt = self
            .system_prompt_template
            .replace("{{core_identity}}", &self.core_identity_cache)
            .replace("{{retrieved_memories}}", &situational_str);

        self.history.add_user_input(text);

        for message in &self.history.messages {
            println!("{} - {}\n\n", message.role, message.content);
        }

        let (response, raw_json) =
            call_mistral_with_history(final_system_prompt, &mut self.history, relevant_memories)
                .await?;

        self.history.add_assistant_response(&raw_json);

        Ok(response)
    }

	pub fn mark_identity_dirty(&mut self) {
		self.needs_identity_refresh = true;
	}
}

fn generate_system_prompt<P: AsRef<Path>>(
    prompt_path: P,
    config: &Config,
    commands: &CommandConfig,
) -> anyhow::Result<String> {
    let mut commands_str = String::new();
    let system_prompt =
        fs::read_to_string(prompt_path).expect("System prompt template file not found in config");

    for command in &commands.static_commands {
        commands_str.push_str(&format!("- {}: {}\n", command.action, command.description));
    }

    for command in &commands.dynamic_commands {
        let param_placeholder = command
            .arg_types
            .iter()
            .map(|arg| format!("{{{}}}", arg))
            .collect::<Vec<_>>()
            .join(" ");

        commands_str.push_str(&format!(
            "- {} {}: {}\n",
            command.action, param_placeholder, command.description
        ));
    }

    let system_prompt = system_prompt
        .replace("{{name}}", &config.name)
        .replace("{{actions}}", &commands_str);

    Ok(system_prompt)
}
