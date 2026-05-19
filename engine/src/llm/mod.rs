use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::{Context, Ok};
use chrono::{Datelike, Local};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    Opt,
    config::Config,
    llm::{
        history::ConversationHistory,
        mistral::{call_mistral_stateless, call_mistral_with_tools},
        tools::{
            ToolContext, ToolRegistry, memory::{QueryMemoryTool, SaveMemoryTool, SearchMemoryTool}, screen::LookAtScreen, time::GetTimeTool
        },
    },
    memory::{MemoryManager, MemoryType},
    state::SharedContext,
    worker::Urgency,
};

pub mod history;
pub mod mistral;
pub mod tools;

pub struct LLMEngine {
    history: ConversationHistory,
    system_prompt_template: String,
    proactive_prompt_template: String,
    greeting_prompt_template: String,
    core_identity_cache: String,
    pub needs_identity_refresh: bool,
    memory: Arc<std::sync::Mutex<MemoryManager>>,
    tools: ToolRegistry,
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
pub struct MistralResponse {}

#[derive(Debug, Deserialize, Default)]
pub struct MistralToolResponse {
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

pub fn load_prompt(dir: &Path, filename: &str, config: &Config) -> anyhow::Result<String> {
    let path = dir.join(filename);
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read prompt file: {}", path.display()))?;
    Ok(content.replace("{{name}}", &config.name))
}

impl LLMEngine {
    pub fn new<P: AsRef<Path>>(
        prompts_dir: P,
        memory: Arc<std::sync::Mutex<MemoryManager>>,
        config: Config,
    ) -> anyhow::Result<Self> {
        let prompts_dir = prompts_dir.as_ref().to_path_buf();

        let system_prompt_template = load_prompt(&prompts_dir, "system_prompt.md", &config)?;
        let proactive_prompt_template = load_prompt(&prompts_dir, "proactive_prompt.md", &config)?;
        let greeting_prompt_template = load_prompt(&prompts_dir, "greeting_prompt.md", &config)?;

        let core_identity_cache = Self::load_core_identity(&memory)?;
        let tools = Self::build_tool_registry();

        Ok(Self {
            history: ConversationHistory::new(),
            system_prompt_template,
            proactive_prompt_template,
            greeting_prompt_template,
            core_identity_cache,
            needs_identity_refresh: false,
            memory,
            tools,
        })
    }

    pub fn history_mut(&mut self) -> &mut ConversationHistory {
        &mut self.history
    }

    fn load_core_identity(memory: &Arc<Mutex<MemoryManager>>) -> anyhow::Result<String> {
        let lock = memory
            .lock()
            .map_err(|_| anyhow::anyhow!("Memory mutex poisoned"))?;
        Ok(lock.get_core_identity()?.join("\n"))
    }

    fn build_tool_registry() -> ToolRegistry {
        let mut tools = ToolRegistry::new();

        tools.register(GetTimeTool);
        tools.register(SearchMemoryTool);
        tools.register(QueryMemoryTool);
        tools.register(SaveMemoryTool);
        tools.register(LookAtScreen);
        
        tools
    }

    #[instrument(skip(self, text, global_ctx, core_identity, relevant_memories), fields(input = %text))]
    pub async fn generate(
        &mut self,
        text: &str,
        global_ctx: &SharedContext,
        core_identity: Vec<String>,
        relevant_memories: Vec<String>,
    ) -> anyhow::Result<String> {
        let overall_start = Instant::now();

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
            let iter_start = Instant::now();

            let response = call_mistral_with_tools(
                final_system_prompt.clone(),
                &mut self.history.messages,
                tool_defs.clone(),
            )
            .await?;

            let llm_elapsed = iter_start.elapsed();
            println!("LLM call complete: {}", llm_elapsed.as_millis());

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

                    println!(
                        "generate complete, total_ms = {}",
                        overall_start.elapsed().as_millis()
                    );
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
                        let tool_start = Instant::now();

                        let result = self.execute_tool(tool_call, global_ctx).await?;

                        println!(
                            "tool execution complete, elapsed_ms = {}",
                            tool_start.elapsed().as_millis()
                        );

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
        let tool = self
            .tools
            .get(&tool_call.function.name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", tool_call.function.name))?;

        let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)?;
        let ctx = ToolContext {
            global_ctx,
            memory: Arc::clone(&self.memory),
        };

        let outcome = tool.execute(args, &ctx).await?;

        if outcome.needs_identity_refresh {
            self.needs_identity_refresh = true;
        }

        Ok(outcome.result)
    }

    fn record_unsolicited_speech(&mut self, spoken: &str, trigger_description: &str) {
        self.history
            .add_assistant_response(Some(spoken.to_string()), None);

        if let Result::Ok(lock) = self.memory.lock() {
            let key = format!("unsolicited_speech_{}", chrono::Utc::now().timestamp());
            let entry = format!(
                "Spoke unsolicited ({}): \"{}\"",
                trigger_description, spoken
            );
            let _ = lock.save(&key, &entry, MemoryType::Situational);
        }
    }

    pub async fn generate_proactive(
        &mut self,
        context: &str,
        urgency: &Urgency,
    ) -> anyhow::Result<Option<String>> {
        let urgency_guidance = Self::urgency_guidance(urgency);

        let prompt = self
            .proactive_prompt_template
            .replace("{{context}}", context)
            .replace("{{urgency_guidance}}", urgency_guidance);

        let response = call_mistral_stateless(prompt, "proceed".into()).await?;
        let trimmed = response.trim().to_lowercase();

        if response.is_empty() || trimmed == "(silent)" || trimmed.starts_with("no response") {
            return Ok(None);
        }

        self.record_unsolicited_speech(&response, &format!("proactive trigger: {}", context));

        Ok(Some(response))
    }

    fn urgency_guidance(urgency: &Urgency) -> &'static str {
        match urgency {
            Urgency::Low => {
                "Low urgency — only speak if you have something genuinely interesting or useful to say. Silence is usually the better choice. Be casual."
            }
            Urgency::Normal => {
                "Normal urgency — speak if it's helpful, stay quiet if it's not actionable. Conversational tone."
            }
            Urgency::High => "High urgency — likely worth mentioning. Be direct but calm.",
            Urgency::Critical => {
                "Critical — speak. The user needs to know now. Be clear and immediate, not casual."
            }
        }
    }

    pub async fn generate_greeting(&mut self, config: &Config) -> anyhow::Result<Option<String>> {
        let current_time = Local::now();
        let weekday = current_time.date_naive().weekday();

        let prompt = self
            .greeting_prompt_template
            .replace("{{time}}", &current_time.format("%R").to_string())
            .replace("{{day_of_week}}", &weekday.to_string())
            .replace("{{date}}", &current_time.format("%-d %B").to_string())
            .replace("{{locale}}", &config.locale);

        let result = call_mistral_stateless(prompt, "proceed".into()).await?;

        if result.is_empty() {
            return Ok(None);
        }

        self.record_unsolicited_speech(&result, "startup greeting");

        println!("result: {}", result);

        Ok(Some(result))
    }
}
