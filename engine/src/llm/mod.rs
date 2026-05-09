use std::{collections::HashMap, fs, path::Path};

use anyhow::Ok;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    actions::datetime::get_time,
    commands::CommandConfig,
    config::Config,
    llm::{
        history::ConversationHistory,
        mistral::{call_mistral_with_history, call_mistral_with_tools},
    },
    memory::{MemoryManager, MemoryType},
    state::SharedContext,
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
        let system_prompt_template = fs::read_to_string(prompt_path)
            .expect("System prompt template file not found in config");
        // let system_prompt_template = generate_system_prompt(prompt_path, config, commands)
        //     .expect("Failed to generate system prompt");

        let history = ConversationHistory::new();

        let core_identity_cache = memory.get_core_identity()?.join("\n");

        println!("CORE IDENTITY CACHE (NEW): {}", core_identity_cache);

        Ok(Self {
            history,
            system_prompt_template,
            core_identity_cache,
            needs_identity_refresh: false,
        })
    }

    #[instrument(skip(self, text, global_ctx, core_identity, relevant_memories), fields(input = %text))]
    pub async fn generate(
        &mut self,
        text: &str,
        global_ctx: &SharedContext,
        core_identity: Vec<String>,
        relevant_memories: Vec<String>,
        tools: Vec<Tool>,
    ) -> anyhow::Result<String> {
        if !core_identity.is_empty() {
            self.core_identity_cache = core_identity.join("\n");
            self.needs_identity_refresh = false;
        }

        let situational_str = if relevant_memories.is_empty() {
            "No specific situational memories found for this query.".to_string()
        } else {
            relevant_memories.join("\n")
        };

        let vitals_str = global_ctx.get_vitals_snapshot();

        let final_system_prompt = self
            .system_prompt_template
            .replace("{{vitals}}", &vitals_str)
            .replace("{{core_identity}}", &self.core_identity_cache)
            .replace("{{retrieved_memories}}", &situational_str);

        self.history.add_user_input(text);

        let max_iterations = 5;

        println!("text: {}", text);

        for iteration in 0..max_iterations {
            let response = call_mistral_with_tools(
                final_system_prompt.clone(),
                &mut self.history.messages,
                tools.clone(),
            )
            .await?;

            println!("response: {:?}", response);

            let choice = &response.choices[0];

            match choice.finish_reason.as_str() {
                "stop" => {
                    let content = choice
                        .message
                        .content
                        .clone()
                        .unwrap_or_else(|| "I'm not sure how to response".to_string());

                    self.history
                        .add_assistant_response(Some(content.clone()), None);
                    return Ok(content);
                }
                "tool_calls" => {
                    let tool_calls = choice
                        .message
                        .tool_calls
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("No tool calls in response"))?;

                    self.history.add_assistant_response(
                        choice.message.content.clone(),
                        Some(tool_calls.clone()),
                    );

                    for tool_call in tool_calls {
                        let result = self.execute_tool(tool_call, global_ctx).await?;

                        self.history.add_tool_result(
                            tool_call.id.clone(),
                            tool_call.function.name.clone(),
                            result,
                        );
                    }
                }
                other => {
                    anyhow::bail!("Unexpected finish reason: {}", other);
                }
            }
        }

        Err(anyhow::anyhow!("Max iterations reached"))
    }

    async fn execute_tool(
        &self,
        tool_call: &ToolCall,
        global_ctx: &SharedContext,
    ) -> anyhow::Result<String> {
        let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)?;

        match tool_call.function.name.as_str() {
            "get_time" => {
                use chrono::Local;
                let now = Local::now();
                Ok(now.format("%I:%M %p").to_string())
            }
            _ => Err(anyhow::anyhow!("Unknow tool: {}", tool_call.function.name)),
        }
    }

    pub fn mark_identity_dirty(&mut self) {
        self.needs_identity_refresh = true;
    }
}

// fn generate_system_prompt<P: AsRef<Path>>(
//     prompt_path: P,
//     config: &Config,
//     commands: &CommandConfig,
// ) -> anyhow::Result<String> {
//     let mut commands_str = String::new();
//     let system_prompt =
//         fs::read_to_string(prompt_path).expect("System prompt template file not found in config");

//     for command in &commands.static_commands {
//         commands_str.push_str(&format!("- {}: {}\n", command.action, command.description));
//     }

//     for command in &commands.dynamic_commands {
//         let param_placeholder = command
//             .arg_types
//             .iter()
//             .map(|arg| format!("{{{}}}", arg))
//             .collect::<Vec<_>>()
//             .join(" ");

//         commands_str.push_str(&format!(
//             "- {} {}: {}\n",
//             command.action, param_placeholder, command.description
//         ));
//     }

//     let system_prompt = system_prompt
//         .replace("{{name}}", &config.name)
//         .replace("{{actions}}", &commands_str);

//     Ok(system_prompt)
// }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "user")]
    User { content: String },

    #[serde(rename = "assistant")]
    Assistant {
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },

    #[serde(rename = "tool")]
    Tool {
        content: String,
        tool_call_id: String,
        name: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: FunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize)]
pub struct MistralRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    pub tool_choice: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct MistralResponse {
    pub id: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: ResponseMessage,
    pub finish_reason: String,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}
