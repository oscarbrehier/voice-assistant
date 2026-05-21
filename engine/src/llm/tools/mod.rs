use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;

use crate::{integrations::obsidian::VaultConfig, llm::FunctionDefinition, memory::MemoryManager, state::SharedContext};

pub mod time;
pub mod memory;
pub mod screen;
pub mod obsidian;
pub mod project;
pub mod audio;

pub struct ToolContext<'a> {
    pub global_ctx: &'a SharedContext,
    pub memory: Arc<std::sync::Mutex<MemoryManager>>,
    pub vault_config: Arc<VaultConfig>
}

#[derive(Default)]
pub struct ToolOutcome {
    pub result: String,
    pub needs_identity_refresh: bool
}

impl ToolOutcome {
    pub fn ok(result: impl Into<String>) -> Self {
        Self { result: result.into(), needs_identity_refresh: false }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn definition(&self) -> FunctionDefinition;
    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext<'_>) -> anyhow::Result<ToolOutcome>;
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<&'static str, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name(), Arc::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn definitions(&self) -> Vec<crate::llm::Tool> {
        self.tools.values()
            .map(|t| crate::llm::Tool {
                tool_type: "function".to_string(),
                function: t.definition()
            })
            .collect()
    }
}