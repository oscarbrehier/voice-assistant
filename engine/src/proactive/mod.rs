use std::{
    collections::{HashMap, VecDeque},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use chrono::Local;
use enigo::Key::Select;
use serde::Serialize;
use tokio::sync::broadcast;
use tracing_subscriber::fmt::format;

use crate::{
    State,
    state::SharedContext,
    worker::{Packet, ProactiveContent, Urgency},
};

pub mod idle_thoughts;

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize)]
pub enum TriggerKind {
    CpuSustained,
    IdleThought,
}

trait Trigger: Send {
    fn kind(&self) -> TriggerKind;
    fn check(&mut self, state: &SharedContext) -> Option<TriggerFire>;
}

struct TriggerFire {
    context: ProactiveContent,
    urgency: Urgency,
}

struct CpuSustainedTrigger {
    window: VecDeque<f64>,
    active: bool,
    threshold: f64,
    window_size: usize,
}

impl CpuSustainedTrigger {
    fn new() -> Self {
        Self {
            window: VecDeque::with_capacity(30),
            active: false,
            threshold: 85.0,
            window_size: 30,
        }
    }
}

impl Trigger for CpuSustainedTrigger {
    fn kind(&self) -> TriggerKind {
        TriggerKind::CpuSustained
    }

    fn check(&mut self, state: &SharedContext) -> Option<TriggerFire> {
        let cpu_load = state.telemetry.read().cpu_load;

        self.window.push_back(cpu_load);
        if self.window.len() > self.window_size {
            self.window.pop_front();
        }

        if self.window.len() < self.window_size {
            return None;
        }

        let avg = self.window.iter().sum::<f64>() / self.window.len() as f64;
        let is_high = avg >= self.threshold;

        if is_high && !self.active {
            self.active = true;

            let processes_str = state
                .telemetry
                .read()
                .top_processes
                .iter()
                .map(|p| format!("{}: ({:.0}%)", p.name, p.cpu_percent))
                .collect::<Vec<_>>()
                .join(", ");

            return Some(TriggerFire {
                context: ProactiveContent::LLMContext(format!(
                    "CPU averaged {:.0}% over the last minute. Top processes: {}",
                    avg, processes_str
                )),
                urgency: Urgency::Normal,
            });
        }

        if !is_high && self.active {
            self.active = false;
        }

        None
    }
}

struct IdleThoughtTrigger {
    check_interval: Duration,
    next_eligible_check: Instant,
    probability: f32,
    min_gap_since_utterance: Duration,
}

impl IdleThoughtTrigger {
    fn new() -> Self {
        Self {
            check_interval: Duration::from_secs(30 * 60),
            next_eligible_check: Instant::now() + Duration::from_secs(30 * 60),
            probability: 0.25,
            min_gap_since_utterance: Duration::from_secs(90 * 60),
        }
    }
}

impl Trigger for IdleThoughtTrigger {
    fn kind(&self) -> TriggerKind {
        TriggerKind::IdleThought
    }

    fn check(&mut self, state: &SharedContext) -> Option<TriggerFire> {
        let now = Instant::now();

        if now < self.next_eligible_check {
            return None;
        }

        self.next_eligible_check = now + self.check_interval;

        if state.engine_state.load(Ordering::SeqCst) != State::Idle as u8 {
            return None;
        }

        let last_utterance = *state.last_utterance_at.read();
        if now.duration_since(last_utterance) < self.min_gap_since_utterance {
            return None;
        }

        let roll: f32 = rand::random();
        if roll > self.probability {
            return None;
        }

        let phrase = idle_thoughts::pick_idle_thought(chrono::Local::now(), None)?;

        Some(TriggerFire {
            context: ProactiveContent::Spoken(phrase),
            urgency: Urgency::Low,
        })
    }
}

pub async fn run_loop(state: SharedContext, tx: broadcast::Sender<Packet>) {
    let mut triggers: Vec<Box<dyn Trigger>> = vec![
        Box::new(CpuSustainedTrigger::new()),
        Box::new(IdleThoughtTrigger::new()),
    ];

    let mut last_fired: HashMap<TriggerKind, Instant> = HashMap::new();
    let cooldown = Duration::from_secs(600);

    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;

        for trigger in &mut triggers {
            if let Some(fire) = trigger.check(&state) {
                let kind = trigger.kind();

                let cooldown_ok = last_fired
                    .get(&kind)
                    .map(|t| t.elapsed() >= cooldown)
                    .unwrap_or(true);

                if !cooldown_ok {
                    continue;
                };

                let _ = tx.send(Packet::ProactiveTrigger {
                    kind: kind.clone(),
                    context: fire.context,
                    urgency: fire.urgency,
                });

                last_fired.insert(kind, Instant::now());
            }
        }
    }
}
