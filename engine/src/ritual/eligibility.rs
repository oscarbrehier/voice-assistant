use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Local, Timelike};

use crate::memory::MemoryManager;

const STATE_KEY: &str = "last_greeting_at";

pub fn is_first_launch_today(
    memory: &Arc<Mutex<MemoryManager>>,
    day_boundary_hour: u32,
) -> anyhow::Result<bool> {
    let last_greeting_at = {
        let lock = memory
            .lock()
            .map_err(|_| anyhow::anyhow!("Memory mutex poisoned"))?;
        lock.state_get_timestamp(STATE_KEY)?
    };

    match last_greeting_at {
        Some(t) => {
            let most_recent_boundary = most_recent_day_boundary(Local::now(), day_boundary_hour);
            return Ok(t < most_recent_boundary);
        }
        None => Ok(true)
    }
}

pub fn record_greeting_now(memory: &Arc<Mutex<MemoryManager>>) -> anyhow::Result<()> {
    let lock = memory.lock().map_err(|_| anyhow::anyhow!("Memory mutex poisonned"))?;
    lock.state_set_timestamp(STATE_KEY, Local::now())?;
    
    Ok(())
}

fn most_recent_day_boundary(now: DateTime<Local>, boundary_hour: u32) -> DateTime<Local> {
    let today_boundary = now
        .with_hour(boundary_hour)
        .and_then(|t| t.with_minute(0))
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .expect("Invalid time parameters");

    if now < today_boundary {
        today_boundary - Duration::days(1)
    } else {
        today_boundary
    }
}
