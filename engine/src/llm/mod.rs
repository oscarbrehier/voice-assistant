use std::{fs, path::Path, sync::Arc};

use anyhow::Ok;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    config::Config, llm::{history::ConversationHistory, mistral::call_mistral_with_tools, tools::{ToolContext, ToolRegistry, time::GetTimeTool}}, memory::{MemoryManager, MemoryType}, state::SharedContext
};

pub mod history;
pub mod mistral;
pub mod tools;

pub struct LLMEngine {
    history: ConversationHistory,
    system_prompt_template: String,
    core_identity_cache: String,
    pub needs_identity_refresh: bool,
    memory: Arc<std::sync::Mutex<MemoryManager>>,
    tools: ToolRegistry
}

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

impl LLMEngine {
    pub fn new<P: AsRef<Path>>(
        prompt_path: P,
        memory: Arc<std::sync::Mutex<MemoryManager>>,
        config: Config
    ) -> anyhow::Result<Self> {
        let system_prompt_template = fs::read_to_string(prompt_path)
            .expect("System prompt template file not found in config");

        let system_prompt_template = system_prompt_template.replace("{{name}}", &config.name);

        let history = ConversationHistory::new();

        let core_identity_cache = {
            let m = memory
                .lock()
                .map_err(|_| anyhow::anyhow!("Lock poisonned"))?;
            m.get_core_identity()?.join("\n")
        };

        println!("CORE IDENTITY CACHE (NEW): {}", core_identity_cache);

        let mut tools = ToolRegistry::new();
        tools.register(GetTimeTool);
        
        Ok(Self {
            history,
            system_prompt_template,
            core_identity_cache,
            needs_identity_refresh: false,
            memory,
            tools,
        })
    }

    #[instrument(skip(self, text, global_ctx, core_identity, relevant_memories), fields(input = %text))]
    pub async fn generate(
        &mut self,
        text: &str,
        global_ctx: &SharedContext,
        core_identity: Vec<String>,
        relevant_memories: Vec<String>,
    ) -> anyhow::Result<String> {
        if !core_identity.is_empty() {
            self.core_identity_cache = core_identity.join("\n");
            self.needs_identity_refresh = false;
        }

        if self.needs_identity_refresh {
            let fresh_core = {
                let lock = self
                    .memory
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
                lock.get_core_identity()?
            };
            self.core_identity_cache = fresh_core.join("\n");
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

        let tool_defs = self.tools.definitions();
        
        let max_iterations = 5;

        for _iteration in 0..max_iterations {
            let response = call_mistral_with_tools(
                final_system_prompt.clone(),
                &mut self.history.messages,
                tool_defs.clone(),
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
        &mut self,
        tool_call: &ToolCall,
        global_ctx: &SharedContext,
    ) -> anyhow::Result<String> {
        let tool = self.tools.get(&tool_call.function.name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", tool_call.function.name))?;

        let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)?;
        let ctx = ToolContext {
            global_ctx,
            memory: Arc::clone(&self.memory)
        };

        let outcome = tool.execute(args, &ctx).await?;

        if outcome.needs_identity_refresh {
            self.needs_identity_refresh = true;
        }

        Ok(outcome.result)
        
        // match tool_call.function.name.as_str() {
        //     "get_time" => {
        //         use chrono::Local;
        //         let now = Local::now();
        //         Ok(now.format("%R").to_string())
        //     }
        //     "search_memories" => {
        //         let query = args["query"]
        //             .as_str()
        //             .ok_or_else(|| anyhow::anyhow!("Missing query"))?;

        //         let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(3) as usize;

        //         let results = {
        //             let lock = self
        //                 .memory
        //                 .lock()
        //                 .map_err(|_| anyhow::anyhow!("Lock poisonned"))?;
        //             lock.search(query, Some(limit))?
        //         };

        //         if results.is_empty() {
        //             Ok(format!("No memories found related to '{}'", query))
        //         } else {
        //             let formatted = results
        //                 .iter()
        //                 .map(|(key, value)| format!("{}: {}", key, value))
        //                 .collect::<Vec<_>>()
        //                 .join("\n");
        //             Ok(formatted)
        //         }
        //     }
        //     "query_memory" => {
        //         let query = args["key"]
        //             .as_str()
        //             .ok_or_else(|| anyhow::anyhow!("Missing key"))?;

        //         let result = {
        //             let lock = self
        //                 .memory
        //                 .lock()
        //                 .map_err(|_| anyhow::anyhow!("Lock poisonned"))?;
        //             lock.get(query)?
        //         };

        //         Ok(result)
        //     }
        //     "save_memory" => {
        //         let key = args["key"]
        //             .as_str()
        //             .ok_or_else(|| anyhow::anyhow!("Missing key"))?;
        //         let value = args["value"]
        //             .as_str()
        //             .ok_or_else(|| anyhow::anyhow!("Missing value"))?;
        //         let memory_type_str = args["memory_type"]
        //             .as_str()
        //             .ok_or_else(|| anyhow::anyhow!("Missing memory_type"))?;

        //         let memory_type = match memory_type_str {
        //             "identity" => MemoryType::Identity,
        //             "situational" => MemoryType::Situational,
        //             _ => anyhow::bail!("Invalid memory_type: must be 'identiy' or 'situational'"),
        //         };

        //         {
        //             let lock = self
        //                 .memory
        //                 .lock()
        //                 .map_err(|_| anyhow::anyhow!("Lock poisonned"))?;
        //             lock.save(key, value, memory_type.clone())?
        //         };

        //         if memory_type == MemoryType::Identity {
        //             self.needs_identity_refresh = true;
        //         }

        //         Ok(format!(
        //             "Saved {:?} memory: {} = {}",
        //             memory_type, key, value
        //         ))
        //     }
        //     _ => Err(anyhow::anyhow!("Unknow tool: {}", tool_call.function.name)),
        // }
    }
}
