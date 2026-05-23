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
            description: "Switch which speaker or output device the assistant speaks through. \
                Use when the user says 'switch to my headphones', 'use the speakers', \
                'output to X'. Pass the device name they mention. If it fails, call \
                list_outputs to see what's available."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The output device the user mentioned, e.g. 'headphones', 'speakers'."
                    }
                },
                "required": ["name"]
            }),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let output_name = args["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Output device name not found"))?;

        {
            let mut lock = ctx.global_ctx.audio_devices.write();
            lock.change_output(output_name)?;
        }

        Ok(ToolOutcome::ok(format!(
            "Output switched to {}.",
            output_name
        )))
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
            description: "List the available audio output devices. \
                Use when the user asks what devices are available, or after a device \
                switch fails so you can tell them their options."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let host = cpal::default_host();

        let formatted_devices = host
            .output_devices()?
            .filter_map(|d| {
                d.description()
                    .ok()
                    .map(|desc| format!("- {}", desc.name()))
            })
            .collect::<Vec<String>>()
            .join("\n");

        if formatted_devices.is_empty() {
            return Ok(ToolOutcome::ok("No output devices found.".to_string()));
        }

        Ok(ToolOutcome::ok(formatted_devices))
    }
}
