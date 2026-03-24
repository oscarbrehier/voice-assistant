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
                return self.build_action(&action_type, None);
            }
        }

        for cmd in &self.dynamic_commands {
            if let Some(captures) = cmd.regex.captures(&text_lower) {
                let mut params = HashMap::new();

                for (i, arg_type) in cmd.arg_types.iter().enumerate() {
                    if let Some(arg) = captures.get(i + 1) {
                        let arg_value = arg.as_str().trim().to_string();
                        if !arg_value.is_empty() {
                            params.insert(arg_type.clone(), arg_value);
                        }
                    }
                }

                return self.build_action(&cmd.action_type, Some(params));
            }
        }

        Action::Unknown
    }

    fn build_action(&self, action_type: &str, params: Option<HashMap<String, String>>) -> Action {
        match action_type {
            "PlayMusic" => Action::PlayMusic,
            "OpenApp" => {
                let app = params.and_then(|p| p.get("app").cloned());
                match app {
                    Some(app) => Action::OpenApp(app),
                    None => Action::Unknown,
                }
            }
            "GetTime" => Action::GetTime,
            _ => Action::Unknown,
        }
    }
}
