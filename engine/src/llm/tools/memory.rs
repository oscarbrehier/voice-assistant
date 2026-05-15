use async_trait::async_trait;

use crate::{
    llm::{
        FunctionDefinition,
        tools::{Tool, ToolContext, ToolOutcome},
    },
    memory::MemoryType,
};

pub struct SaveMemoryTool;
pub struct QueryMemoryTool;
pub struct SearchMemoryTool;

#[async_trait]
impl Tool for SaveMemoryTool {
    fn name(&self) -> &'static str {
        "save_memory"
    }

    fn definition(&self) -> crate::llm::FunctionDefinition {
        FunctionDefinition {
            name: "save_memory".to_string(),
            description: "Store a new memory or update an existing one".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                  "key": {
                    "type": "string",
                    "description": "Unique identifier for this memory"
                  },
                  "value": {
                    "type": "string",
                    "description": "The information to store"
                  },
                  "memory_type": {
                    "type": "string",
                    "enum": ["identity", "situational"],
                    "description": "Type of memory"
                  }
                },
                "required": ["key", "value", "memory_type"]
            }),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let key = args["key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing key"))?;
        let value = args["value"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing value"))?;
        let memory_type_str = args["memory_type"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing memory_type"))?;

        let memory_type = match memory_type_str {
            "identity" => MemoryType::Identity,
            "situational" => MemoryType::Situational,
            _ => anyhow::bail!("Invalid memory_type: must be 'identiy' or 'situational'"),
        };

        {
            let lock = ctx
                .memory
                .lock()
                .map_err(|_| anyhow::anyhow!("Lock poisonned"))?;
            lock.save(key, value, memory_type.clone())?
        };

        let needs_refresh = memory_type == MemoryType::Identity;

        Ok(ToolOutcome {
            result: format!("Saved {:?} memory: {} = {}", memory_type, key, value),
            needs_identity_refresh: needs_refresh,
        })
    }
}

#[async_trait]
impl Tool for QueryMemoryTool {
    fn name(&self) -> &'static str {
        "query_memory"
    }

    fn definition(&self) -> crate::llm::FunctionDefinition {
        FunctionDefinition {
            name: "query_memory".to_string(),
            description: "Get memory by EXACT key name. ONLY use this if you already know the exact key from a previous search_memories result. If unsure, use search_memories instead.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                  "key": {
                    "type": "string",
                    "description": "Exact memory key"
                  }
                },
                "required": ["key"]
            }),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let query = args["key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing key"))?;

        let result = {
            let lock = ctx
                .memory
                .lock()
                .map_err(|_| anyhow::anyhow!("Lock poisonned"))?;
            lock.get(query)?
        };

        Ok(ToolOutcome {
            result,
            needs_identity_refresh: false,
        })
    }
}

#[async_trait]
impl Tool for SearchMemoryTool {
    fn name(&self) -> &'static str {
        "search_memories"
    }

    fn definition(&self) -> crate::llm::FunctionDefinition {
        FunctionDefinition {
            name: "search_memories".to_string(),
            description: "Search for memories using natural language. ALWAYS use this first when looking for user information. Works even if you don't know the exact key name.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                  "query": {
                    "type": "string",
                    "description": "What to search for"
                  }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing query"))?;

        let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(3) as usize;

        let results = {
            let lock = ctx
                .memory
                .lock()
                .map_err(|_| anyhow::anyhow!("Lock poisonned"))?;
            lock.search(query, Some(limit))?
        };

        let result = if results.is_empty() {
            format!("No memories found related to '{}'", query)
        } else {
            results
                .iter()
                .map(|(key, value)| format!("{}: {}", key, value))
                .collect::<Vec<_>>()
                .join("\n")
        };

        Ok(ToolOutcome {
            result,
            needs_identity_refresh: false,
        })
    }
}
