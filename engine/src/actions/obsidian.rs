use std::{path::PathBuf, time::SystemTime};

use anyhow::{Context, anyhow};
use strsim::jaro_winkler;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use walkdir::WalkDir;

pub struct VaultConfig {
    pub root_path: PathBuf,
}

impl VaultConfig {
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            root_path: root_path.canonicalize().unwrap_or(root_path),
        }
    }

    pub fn resolve_safe_path(&self, relative_name: &str) -> anyhow::Result<PathBuf> {
        let mut target = self.root_path.clone();
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
pub struct NoteEntry {
    pub display_name: String,
    pub path: PathBuf,
    pub modified_time: SystemTime,
}

pub fn list_vault_index(config: &VaultConfig) -> anyhow::Result<Vec<NoteEntry>> {
    let mut note_entries = Vec::new();

    for entry in WalkDir::new(&config.root_path)
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

pub fn clean_path_display(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

pub fn search_notes(query: &str, index: &[NoteEntry]) -> anyhow::Result<Vec<NoteEntry>> {
    let query_lower = query.to_lowercase();

    let mut matches: Vec<(f64, NoteEntry)> = Vec::new();

    for entry in index {
        let display_name_lower = entry.display_name.to_lowercase();

        let score = if display_name_lower.contains(&query_lower) {
            let position = display_name_lower.find(&query_lower).unwrap() as f64;
            let len = display_name_lower.len() as f64;

            0.95_f64 + (0.05_f64 * (1.0_f64 - (position / len)))
        } else {
            let query_words: Vec<&str> = query_lower.split_whitespace().collect();
            let name_words: Vec<&str> = display_name_lower.split_whitespace().collect();

            let mut total_score = 0.0;
            let mut matched_words = 0;

            for q_word in &query_words {
                let mut best_word_score = 0.0_f64;

                for n_word in &name_words {
                    let word_score = jaro_winkler(q_word, n_word);
                    best_word_score = best_word_score.max(word_score);
                }

                if best_word_score > 0.7 {
                    total_score += best_word_score;
                    matched_words += 1;
                }
            }

            if matched_words > 0 {
                total_score / query_words.len() as f64
            } else {
                jaro_winkler(&query_lower, &display_name_lower)
            }
        };

        if score > 0.7 {
            matches.push((score, entry.clone()));
        }
    }

    matches.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    let best_matches = matches
        .into_iter()
        .take(5)
        .map(|(_, entry)| entry)
        .collect();

    Ok(best_matches)
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
    config: &VaultConfig,
    title: &str,
    content: &str,
) -> anyhow::Result<PathBuf> {
    let safe_path = config.resolve_safe_path(title)?;

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
