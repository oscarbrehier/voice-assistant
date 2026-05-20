use strsim::jaro_winkler;

use crate::integrations::obsidian::{NoteEntry, Project};

pub trait Searchable {
    fn name(&self) -> &str;
}

impl Searchable for NoteEntry {
    fn name(&self) -> &str {
        &self.display_name
    }
}

impl Searchable for Project {
    fn name(&self) -> &str {
        &self.display_name
    }
}

pub fn fuzzy_search<T: Searchable + Clone>(query: &str, items: &[T]) -> Vec<T> {
    let query_lower = query.to_lowercase();

    let mut matches: Vec<(f64, T)> = Vec::new();

    for item in items {
        let name_lower = item.name().to_lowercase();

        let score = if name_lower.contains(&query_lower) {
            let position = name_lower.find(&query_lower).unwrap() as f64;
            let len = name_lower.len() as f64;
            0.95_f64 + (0.05_f64 * (1.0_f64 - (position / len)))
        } else {
            let query_words: Vec<&str> = query_lower.split_whitespace().collect();
            let name_words: Vec<&str> = name_lower.split_whitespace().collect();

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
                jaro_winkler(&query_lower, &name_lower)
            }
        };

        if score > 0.7 {
            matches.push((score, item.clone()));
        }
    }

    matches.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    
        let best_matches = matches
            .into_iter()
            .take(5)
            .map(|(_, item)| item)
            .collect::<Vec<T>>();

    best_matches
}
