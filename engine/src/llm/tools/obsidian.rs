use std::path::PathBuf;

use anyhow::Ok;
use async_trait::async_trait;
use tracing_subscriber::fmt::format;

use crate::{
    integrations::obsidian::{
        NoteEntry, VaultConfig, append_to_note, clean_path_display, create_note, get_current_project, get_recent_notes, list_vault_index, read_note_content, scoped_index, search_notes
    },
    llm::{
        FunctionDefinition,
        tools::{Tool, ToolContext, ToolOutcome, project},
    },
};

pub struct SearchNotesTool;
pub struct GetRecentNotesTool;
pub struct ReadNoteTool;
pub struct CreateNoteTool;
pub struct AppendToNoteTool;

fn resolve_note_path(path_str: &str, ctx: &ToolContext<'_>) -> anyhow::Result<PathBuf> {
    if path_str.contains('/') || path_str.contains('\\') {
        let p = PathBuf::from(path_str);
        let canonical = p
            .canonicalize()
            .map_err(|_| anyhow::anyhow!("Note not found: {}", path_str))?;
        if !canonical.starts_with(&ctx.vault_config.root_path) {
            anyhow::bail!("Path is outside the vault");
        }
        return Ok(canonical);
    }

    let index = scoped_index(&ctx)?;
    let name_stem = path_str.trim_end_matches(".md");

    let exact: Vec<&NoteEntry> = index
        .iter()
        .filter(|n| n.display_name.eq_ignore_ascii_case(name_stem))
        .collect();

    match exact.as_slice() {
        [single] => Ok(single.path.clone()),
        [] => anyhow::bail!(
            "No note named '{}' found. Use search_notes to locate it first.",
            name_stem
        ),
        multiple => {
            let folders = multiple
                .iter()
                .map(|n| format!("  {}", n.path.display()))
                .collect::<Vec<_>>()
                .join("\n");
            anyhow::bail!(
                "Multiple notes named '{}' exist:\n{}\nUse the full path to disambiguate.",
                name_stem,
                folders
            )
        }
    }
}

#[async_trait]
impl Tool for SearchNotesTool {
    fn name(&self) -> &'static str {
        "search_notes"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name().to_string(),
            description: "Search the user's Obsidian vault for notes matching a query. \
                Returns up to 5 best matches with their display names and paths. \
                Use this whenever the user references a note by name or topic — \
                'my project notes', 'the recipe one', 'where I wrote about X'. \
                The user rarely knows exact filenames."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to search for. Use natural keywords from the user's request."
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

        let index = scoped_index(&ctx)?;
        let matches = search_notes(query, &index);

        if matches.is_empty() {
            return Ok(ToolOutcome::ok(format!(
                "No notes found matching: '{}'",
                query
            )));
        }

        let formatted = matches
            .iter()
            .map(|n| format!("- \"{}\" ({})", n.display_name, clean_path_display(&n.path)))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolOutcome::ok(formatted))
    }
}

#[async_trait]
impl Tool for ReadNoteTool {
    fn name(&self) -> &'static str {
        "read_note"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name().to_string(),
            description: "Read the full contents of a specific note. \
                IMPORTANT: call search_notes FIRST to find the right note, then pass the path \
                from its result here. Only pass a bare display name if you are certain it is unambiguous. \
                Guessing filenames will fail."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The full path to the note (from a previous search_notes result), or the note's display name if confident there's no ambiguity."
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;

        let path = resolve_note_path(path_str, &ctx)?;
        let content = read_note_content(&path).await?;

        Ok(ToolOutcome::ok(content))
    }
}

#[async_trait]
impl Tool for CreateNoteTool {
    fn name(&self) -> &'static str {
        "create_note"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name().to_string(),
            description: "Create a new note in the vault with a title and content. \
                Use when the user asks to start a new note, jot something down, or save a thought. \
                Fails if a note with the same title already exists."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "The note title (will become the filename). Should be descriptive and human-readable."
                    },
                    "content": {
                        "type": "string",
                        "description": "The initial content of the note in markdown format. Can include headers, lists, etc."
                    }
                },
                "required": ["title", "content"]
            }),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let title = args["title"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing title"))?;

        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing content"))?;

        let path = create_note(&ctx, title, content).await?;

        Ok(ToolOutcome::ok(format!(
            "Create note '{}' at {}",
            title,
            path.display()
        )))
    }
}

#[async_trait]
impl Tool for AppendToNoteTool {
    fn name(&self) -> &'static str {
        "append_to_note"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name().to_string(),
            description: "Append new content to an existing note. \
                Use after search_notes confirms the note exists. \
                Content is added at the end of the note, marked as an assistant update."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the note (from search_notes) or its display name."
                    },
                    "content": {
                        "type": "string",
                        "description": "The text to append. Markdown formatting is supported."
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?;

        let content = args["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing content"))?;

        let path = resolve_note_path(path_str, &ctx)?;
        append_to_note(&path, content).await?;

        Ok(ToolOutcome::ok(format!(
            "Added content to '{}'",
            path.display()
        )))
    }
}

#[async_trait]
impl Tool for GetRecentNotesTool {
    fn name(&self) -> &'static str {
        "get_recent_notes"
    }

    fn definition(&self) -> FunctionDefinition {
        FunctionDefinition {
            name: self.name().to_string(),
            description: "List the user's most recently modified notes in their vault. \
                Use when the user asks 'what did I just write', 'what was I working on', \
                'show me my latest notes', or wants a sense of recent activity. \
                Returns up to `limit` notes (default 5, max 20), newest first."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Number of notes to return (default 5, max 20)."
                    }
                },
                "required": []
            }),
        }
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext<'_>,
    ) -> anyhow::Result<ToolOutcome> {
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(20) as usize;

        let index = scoped_index(&ctx)?;
        let result = get_recent_notes(&index, limit)?;

        if result.is_empty() {
            return Ok(ToolOutcome::ok(
                "The vault appears to be empty — no notes found.".to_string(),
            ));
        }

        let formatted = result
            .iter()
            .map(|n| format!("- \"{}\" ({})", n.display_name, clean_path_display(&n.path)))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolOutcome::ok(formatted))
    }
}