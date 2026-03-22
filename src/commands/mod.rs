use std::{collections::HashMap, fs};

use regex::Regex;
use serde::Deserialize;

use crate::actions::Action;


#[derive(Debug, Deserialize)]
struct StaticCommand {
	action: String,
	triggers: Vec<String>
}

#[derive(Debug, Deserialize)]
struct DynamicCommand {
	action: String,
	patterns: Vec<String>,
	regex: String,
	arg_type: String
}

#[derive(Debug, Deserialize)]
struct CommandConfig {
	static_commands: Vec<StaticCommand>,
	dynamic_commands: Vec<DynamicCommand>
}

struct CompiledCommand {
	regex: Regex,
	action_type: String,
	arg_type: String
}

pub struct CommandMatcher {
	static_triggers: HashMap<String, String>,
	dynamic_commands: Vec<CompiledCommand>
}


impl CommandMatcher {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
		let json = fs::read_to_string(path)?;
		let config: CommandConfig = serde_json::from_str(&json)?;

		let mut static_triggers = HashMap::new();
		for cmd in config.static_commands {
			for trigger in cmd.triggers {
				static_triggers.insert(trigger.to_lowercase(), cmd.action.clone());
			}
		}

		let mut dynamic_commands = Vec::new();
		for cmd in config.dynamic_commands {
			let regex = Regex::new(&cmd.regex)?;
			dynamic_commands.push(CompiledCommand {
				regex,
				action_type: cmd.action,
				arg_type: cmd.arg_type
			});
		}

		Ok(Self {
			static_triggers,
			dynamic_commands
		})
	}

	pub fn match_command(&self, text: &str) -> Action {

		let text_lower = text.to_lowercase();

		for (trigger, action_type) in &self.static_triggers {
			if text_lower.contains(trigger) {
				return self.build_action(&action_type, None);
			}
		}

		for cmd in &self.dynamic_commands {
			if let Some(captures) = cmd.regex.captures(&text_lower) {
				if let Some(arg) = captures.get(1) {
					let arg_value = arg.as_str().trim().to_string();
					if !arg_value.is_empty() {
						return self.build_action(&cmd.action_type, Some(arg_value));
					}
				}
			}
		}

		Action::Unknown

	}

	fn build_action(&self, action_type: &str, arg: Option<String>) -> Action {
		match action_type {
			"PlayMusic" => Action::PlayMusic,
			_ => Action::Unknown
		}
	}
}
