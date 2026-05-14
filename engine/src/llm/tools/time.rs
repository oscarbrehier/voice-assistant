use async_trait::async_trait;
use chrono::Local;

use crate::llm::{FunctionDefinition, tools::{Tool, ToolContext, ToolOutcome}};

pub struct GetTimeTool;

#[async_trait]
impl Tool for GetTimeTool {
    fn name(&self) -> &'static str {
        "get_time"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: "get_time".to_string(),
            description: "Get the current local time as HH:MM.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(&self, _args: serde_json::Value, _ctx: &ToolContext<'_>) -> anyhow::Result<ToolOutcome> {
        Ok(ToolOutcome::ok(Local::now().format("%R").to_string()))
    }
}
