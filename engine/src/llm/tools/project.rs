use async_trait::async_trait;

use crate::{
    integrations::obsidian::{
        clean_path_display, get_current_project, list_projects, search_projects,
    },
    llm::{
        FunctionDefinition,
        tools::{Tool, ToolContext, ToolOutcome},
    },
};

pub struct GetProjectsTool;
pub struct SetProjectTool;
pub struct GetCurrentProjectTool;

#[async_trait]
impl Tool for GetProjectsTool {
    fn name(&self) -> &'static str {
        "get_projects"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name().to_string(),
            description: "List all available projects inside the workspace vault. \
                Use this when the user asks 'what projects do I have?', 'show my projects', \
                or needs to review their options before switching tasks."
                .to_string(),
            parameters: serde_json::json!({}),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let projects = list_projects(&ctx.vault_config)?;

        let formatted = projects
            .iter()
            .map(|n| format!("- \"{}\" ({})", n.display_name, clean_path_display(&n.path)))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolOutcome::ok(formatted))
    }
}

#[async_trait]
impl Tool for GetCurrentProjectTool {
    fn name(&self) -> &'static str {
        "get_current_project"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name().to_string(),
            description: "Retrieve the name and location of the project currently active and in-focus. \
                Use this when the user asks 'what am I working on right now?' or 'what is the current project?'".to_string(),
            parameters: serde_json::json!({}),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let project = get_current_project(&ctx.memory)?;

        match project {
            Some(p) => Ok(ToolOutcome::ok(format!(
                "- \"{}\" ({})",
                p.display_name,
                clean_path_display(&p.path)
            ))),
            None => Ok(ToolOutcome::ok("No active project currently set")),
        }
    }
}

#[async_trait]
impl Tool for SetProjectTool {
    fn name(&self) -> &'static str {
        "set_project"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name().to_string(),
            description: "Set the current working project. \
				Scopes note operations to that project's folder. \
				Use when the user says 'let's work on X', 'switch to project Y', \
				'I'm working on Z now'. Pass the project name."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "project_name": {
                         "type": "string",
                         "description": "The name of the project"
                    }
                },
                "required": ["project_name"]
            }),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let name = args["project_name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing project name"))?;

        let projects = list_projects(&ctx.vault_config)?;
        let matches = search_projects(name, &projects);

        match matches.first() {
            Some(p) => {
                let path_str = p.path.to_string_lossy().into_owned();
                {
                    let lock = ctx
                        .memory
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Memory lock poisoned"))?;
                    lock.state_set("current_project", &path_str)?;
                }
                Ok(ToolOutcome::ok(format!(
                    "Now working on project '{}'.",
                    p.display_name
                )))
            }
            None => Ok(ToolOutcome::ok(format!(
                "No project named '{}' found. Existing projects: {}. Want me to create it?",
                name,
                projects
                    .iter()
                    .map(|p| p.display_name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }
}
