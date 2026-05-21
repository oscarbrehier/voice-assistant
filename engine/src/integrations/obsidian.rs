use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{path::PathBuf, time::SystemTime};

use anyhow::{Context, Ok, anyhow};
use strsim::jaro_winkler;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use walkdir::WalkDir;

use crate::llm::tools::ToolContext;
use crate::memory::MemoryManager;
use crate::utils::search::fuzzy_search;

pub struct VaultConfig {
    pub root_path: PathBuf,
}

impl VaultConfig {
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            root_path: root_path.canonicalize().unwrap_or(root_path),
        }
    }

    pub fn resolve_safe_path(&self, base_dir: Option<&Path>, relative_name: &str) -> anyhow::Result<PathBuf> {
        let mut target = base_dir.unwrap_or(&self.root_path).to_path_buf();
        target.push(relative_name);

        if !target.to_string_lossy().ends_with(".md") {
            target.set_extension("md");
        }

        let canonical_target = target.canonicalize().unwrap_or(target.clone());
        if !canonical_target.starts_with(&self.root_path) {
            return Err(anyhow!("Security violation: path is outside vault bounds"));
        }

        Ok(canonical_target)
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    pub display_name: String,
    pub path: PathBuf,
}

pub fn list_projects(config: &VaultConfig) -> anyhow::Result<Vec<Project>> {
    let mut projects: Vec<Project> = Vec::new();

    for entry in std::fs::read_dir(&config.root_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(name) = path.file_name() {
                projects.push(Project {
                    display_name: name.to_string_lossy().to_owned().to_string(),
                    path,
                });
            }
        }
    }

    Ok(projects)
}

pub fn search_projects(query: &str, projects: &[Project]) -> Vec<Project> {
    fuzzy_search(query, projects)
}

pub fn get_current_project(memory: &Arc<Mutex<MemoryManager>>) -> anyhow::Result<Option<Project>> {
    let path_str = {
        let lock = memory
            .lock()
            .map_err(|_| anyhow::anyhow!("Memory lock poisoned"))?;
        lock.state_get("current_project")?
    };

    match path_str {
        Some(s) => {
            let path = PathBuf::from(&s);
            let display_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| s.clone());
            
            Ok(Some(Project { display_name, path }))
        }
        None => Ok(None),
    }
}

#[derive(Clone, Debug)]
pub struct NoteEntry {
    pub display_name: String,
    pub path: PathBuf,
    pub modified_time: SystemTime,
}

fn list_index_at(root: &Path) -> anyhow::Result<Vec<NoteEntry>> {
    let mut note_entries = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "md") {
            if let Some(filename) = path.file_name() {
                let display_name = filename.to_string_lossy().replace(".md", "");
                let metadata = std::fs::metadata(path)?;

                note_entries.push(NoteEntry {
                    display_name,
                    path: path.to_path_buf(),
                    modified_time: metadata.modified()?,
                });
            }
        }
    }
    Ok(note_entries)
}

pub fn list_vault_index(config: &VaultConfig) -> anyhow::Result<Vec<NoteEntry>> {
	list_index_at(&config.root_path)
}

pub fn scoped_index(ctx: &ToolContext) -> anyhow::Result<Vec<NoteEntry>> {
	match get_current_project(&ctx.memory)? {
		Some(project) => list_index_at(&project.path),
		None => list_vault_index(&ctx.vault_config)
	}
}

pub fn search_notes(query: &str, index: &[NoteEntry]) -> Vec<NoteEntry> {
    fuzzy_search(query, index)
}

pub fn clean_path_display(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

pub fn get_recent_notes(index: &[NoteEntry], limit: usize) -> anyhow::Result<Vec<NoteEntry>> {
    let mut index_v = index.to_vec();
    index_v.sort_by(|a, b| b.modified_time.cmp(&a.modified_time));
    Ok(index_v.iter().take(limit).cloned().collect())
}

pub async fn read_note_content(path: &PathBuf) -> anyhow::Result<String> {
    Ok(fs::read_to_string(path).await?)
}

pub async fn create_note(
	ctx: &ToolContext<'_>,
    title: &str,
    content: &str,
) -> anyhow::Result<PathBuf> {
	let target_base = match get_current_project(&ctx.memory)? {
		Some(project) => Some(project.path),
		None => None
	};
	
    let safe_path = ctx.vault_config.resolve_safe_path(target_base.as_deref(), title)?;

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&safe_path)
        .await
        .context("Could not create file (it might already exist)")?;

    file.write_all(content.as_bytes()).await?;
    Ok(safe_path)
}

pub async fn append_to_note(path: &PathBuf, new_content: &str) -> anyhow::Result<()> {
    let current_content = fs::read_to_string(path).await?;
    let prefix = if current_content.ends_with('\n') || current_content.is_empty() {
        ""
    } else {
        "\n"
    };

    let mut file = OpenOptions::new().append(true).open(path).await?;
    let formatted = format!(
        "{}{}",
        prefix,
        format_entry("[!info] Assistant Update", new_content)
    );

    file.write_all(formatted.as_bytes()).await?;
    Ok(())
}

pub async fn smart_append_to_section(
    path: &PathBuf,
    section_header: &str,
    new_content: &str,
) -> anyhow::Result<()> {
    let content = fs::read_to_string(path).await?;
    let lines: Vec<&str> = content.lines().collect();

    let section_idx = lines
        .iter()
        .position(|line| {
            line.trim_start_matches('#')
                .trim()
                .eq_ignore_ascii_case(section_header)
        })
        .ok_or_else(|| anyhow!("Section '{}' not found", section_header))?;

    let section_level = lines[section_idx].chars().take_while(|&c| c == '#').count();

    let section_end = lines
        .iter()
        .enumerate()
        .skip(section_idx + 1)
        .find(|(_, line)| {
            line.starts_with('#') && line.chars().take_while(|&c| c == '#').count() <= section_level
        })
        .map(|(idx, _)| idx)
        .unwrap_or(lines.len());

    let mut result = lines[..section_end].to_vec();
    result.push(new_content);
    result.extend_from_slice(&lines[section_end..]);

    fs::write(path, result.join("\n")).await?;
    Ok(())
}

fn format_entry(header: &str, text: &str) -> String {
    format!("> {}\n> {}\n", header, text)
}
