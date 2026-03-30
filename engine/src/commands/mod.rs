use std::{collections::HashMap, fs};

use regex::Regex;
use serde::Deserialize;

use crate::actions::Action;

#[derive(Debug, Deserialize, Clone)]
pub struct StaticCommand {
    pub action: String,
    pub description: String,
    triggers: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DynamicCommand {
    pub action: String,
    pub description: String,
    patterns: Vec<String>,
    regex: String,
    pub arg_types: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CommandConfig {
    pub static_commands: Vec<StaticCommand>,
    pub dynamic_commands: Vec<DynamicCommand>,
}

struct CompiledCommand {
    regex: Regex,
    action_type: String,
    arg_types: Vec<String>,
}

pub struct CommandMatcher {
    pub config: CommandConfig,
    static_triggers: HashMap<String, String>,
    dynamic_commands: Vec<CompiledCommand>,
}

impl CommandMatcher {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let json = fs::read_to_string(path)?;
        let config: CommandConfig = serde_json::from_str(&json)?;
        let config_clone = config.clone();

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
                arg_types: cmd.arg_types,
            });
        }

        Ok(Self {
            config: config_clone,
            static_triggers,
            dynamic_commands,
        })
    }

    pub fn match_command(&self, text: &str) -> Action {
        let text_lower = text.trim_end_matches(['.', '!', '?']).to_lowercase();

        for (trigger, action_type) in &self.static_triggers {
            if text_lower.contains(trigger) {
                let val = serde_json::json!({
                    "action": action_type,
                    "params": null
                });

                return serde_json::from_value(val).unwrap_or(Action::Unknown);
            }
        }

        for cmd in &self.dynamic_commands {
            if let Some(captures) = cmd.regex.captures(&text_lower) {
                let mut map = serde_json::Map::new();

                for (i, arg_name) in cmd.arg_types.iter().enumerate() {
                    if let Some(arg) = captures.get(i + 1) {
                        let arg_value = arg.as_str().to_string();
                        if !arg_value.is_empty() {
                            map.insert(arg_name.clone(), serde_json::Value::String(arg_value));
                        }
                    }
                }

                let val = serde_json::json!({
                    "action": cmd.action_type,
                    "params": map
                });

                return serde_json::from_value(val).unwrap_or(Action::Unknown);
            }
        }

        Action::Unknown
    }
}
