use std::path::Path;

use chrono::{Datelike, Local};

use crate::{config::Config, llm::{load_prompt, mistral::call_mistral_stateless}};

pub async fn generate(config: &Config) -> anyhow::Result<String> {

    let current_time = Local::now();
    let weekday = current_time.date_naive().weekday();
    
    let prompt = load_prompt(Path::new("config"), "greeting_prompt.md", &config.name)?
        .replace("{{time}}", &current_time.format("%R").to_string())
        .replace("{{day_of_week}}", &weekday.to_string())
        .replace("{{date}}", &current_time.format("%-d %B").to_string())
        .replace("{{locale}}", &config.locale);
    
    let result = call_mistral_stateless(prompt, "proceed".into()).await?;

    println!("result: {}", result);
    
    Ok(result)
    
}