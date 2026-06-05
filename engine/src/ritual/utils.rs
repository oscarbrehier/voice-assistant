use std::time::Duration;

use chrono::{DateTime, Local, Timelike};

pub fn continuity_note_for_gap(gap: Duration, now: DateTime<Local>) -> String {
    let minutes = gap.as_secs() / 60;
    let hours = minutes / 60;
    let time_of_day = match now.hour() {
        5..=11 => "morning",
        12..=17 => "afternoon",
        18..=22 => "evening",
        _ => "late",
    };

    if minutes < 5 {
        return String::new();
    }

    let strength = match minutes {
        0..=30 => "Acknowledgement is optional and should be very brief if present.",
        31..=120 => "Open with a brief casual acknowledgement before answering.",
        121..=360 => "Open with a short greeting before answering.",
        _ => "The user is returning after a long gap. Open with a warm greeting before answering.",
    };

    let descriptor = match minutes {
        0..=30 => "a few minutes".to_string(),
        31..=120 => format!("about {} hour(s)", hours.max(1)),
        _ => format!("about {} hours", hours),
    };

    format!(
        "The user is returning after {}. Current time of day: {}. {} \
         Keep it conversational, a few words at most. Don't dwell on the gap \
         or apologize for it. Sound like a friend who happens to know what time it is.",
        descriptor, time_of_day, strength,
    )
}
