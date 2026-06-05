use chrono::{DateTime, Datelike, Local, Timelike, Weekday};
use std::time::SystemTime;

pub fn pick_idle_thought(now: DateTime<Local>, current_project: Option<&str>) -> Option<String> {
    let hour = now.hour();
    let weekday = now.weekday();
    let mut candidates: Vec<&'static str> = Vec::new();

    match hour {
        5..=7 => candidates.extend(EARLY_MORNING),
        8..=11 => candidates.extend(MORNING),
        12..=13 => candidates.extend(MIDDAY),
        14..=17 => candidates.extend(AFTERNOON),
        18..=20 => candidates.extend(EVENING),
        21..=23 => candidates.extend(LATE_EVENING),
        0..=3 => candidates.extend(LATE_NIGHT),
        _ => {}
    }

    match weekday {
        Weekday::Mon => candidates.extend(MONDAY),
        Weekday::Fri => candidates.extend(FRIDAY),
        Weekday::Sat | Weekday::Sun => candidates.extend(WEEKEND),
        Weekday::Wed => candidates.extend(MIDWEEK),
        _ => {}
    }

    if let Some(_proj) = current_project {
        candidates.extend(WITH_PROJECT);
    }

    candidates.extend(AMBIENT);

    if candidates.is_empty() {
        return None;
    }

    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as usize)
        .unwrap_or(0);

    Some(candidates[seed % candidates.len()].to_string())
}

const EARLY_MORNING: &[&str] = &["Morning.", "Early.", "Quiet hour.", "Sun's coming up."];

const MORNING: &[&str] = &["Morning's getting on.", "Coffee weather.", "Quiet morning."];

const MIDDAY: &[&str] = &["Lunchtime somewhere.", "Past noon.", "Halfway through."];

const AFTERNOON: &[&str] = &["Quiet afternoon.", "Afternoon light.", "Slow afternoon."];

const EVENING: &[&str] = &["Evening.", "Day's winding down.", "Light's going."];

const LATE_EVENING: &[&str] = &["Late.", "Quiet evening.", "Past most people's bedtime."];

const LATE_NIGHT: &[&str] = &["Small hours.", "Late night.", "Quiet hour."];

const MONDAY: &[&str] = &["Monday again.", "Start of the week."];

const MIDWEEK: &[&str] = &["Wednesday.", "Midweek."];

const FRIDAY: &[&str] = &["Friday.", "Almost the weekend."];

const WEEKEND: &[&str] = &["Weekend.", "Saturday pace.", "No rush today."];

const WITH_PROJECT: &[&str] = &["Steady work.", "Productive stretch.", "Focused day."];

const AMBIENT: &[&str] = &["Quiet.", "Still here.", "All quiet."];
