use async_trait::async_trait;
use cpal::traits::HostTrait;
use rodio::DeviceTrait;

use crate::llm::{
    FunctionDefinition,
    tools::{Tool, ToolContext, ToolOutcome},
};

pub struct ChangeOutputTool;
pub struct ListOutputsTool;

#[async_trait]
impl Tool for ChangeOutputTool {
    fn name(&self) -> &'static str {
        "change_output"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name().to_string(),
            description: "".to_string(),
            parameters: serde_json::json!({}),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        Ok(ToolOutcome::ok("hello".to_string()))
    }
}

#[async_trait]
impl Tool for ListOutputsTool {
    fn name(&self) -> &'static str {
        "list_outputs"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name().to_string(),
            description: "".to_string(),
            parameters: serde_json::json!({}),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
    
        let host = cpal::default_host();
        
        let formatted_devices = host
            .output_devices()?
            .filter_map(|d| {
                if let Ok(d) = d.description() {
                    return Some(format!("- {}", d.name()));
                }
                None
            })
            .collect::<Vec<String>>()
           	.join("\n");

        Ok(ToolOutcome::ok(formatted_devices))
    }
}
