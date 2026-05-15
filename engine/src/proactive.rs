use std::{collections::{HashMap, VecDeque}, time::{Duration, Instant}};

use serde::Serialize;
use tokio::{sync::broadcast};
use tracing_subscriber::fmt::format;

use crate::{worker::{Packet, Urgency}, state::SharedContext};

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize)]
pub enum TriggerKind {
    CpuSustained
}

trait Trigger: Send {
    fn kind(&self) -> TriggerKind;
    fn check(&mut self, state: &SharedContext) -> Option<TriggerFire>;
}

struct TriggerFire {
    context: String,
    urgency: Urgency
}

struct CpuSustainedTrigger {
    window: VecDeque<f64>,
    active: bool,
    threshold: f64,
    window_size: usize
}

impl CpuSustainedTrigger {
    fn new() -> Self {
        Self {
            window: VecDeque::with_capacity(30),
            active: false,
            threshold: 40.0,
            window_size: 30
        }
    }
}

impl Trigger for CpuSustainedTrigger {
    fn kind(&self) -> TriggerKind { TriggerKind::CpuSustained }

    fn check(&mut self, state: &SharedContext) -> Option<TriggerFire> {
        let cpu_load = state.telemetry.read().cpu_load;

        self.window.push_back(cpu_load);
        if self.window.len() > self.window_size {
            self.window.pop_front();
        }

        if self.window.len() < self.window_size {
            return None ;
        }

        let avg = self.window.iter().sum::<f64>() / self.window.len() as f64;
        let is_high = avg >= self.threshold;

        if is_high && !self.active {
            self.active = true;

            let processes_str = state.telemetry.read().top_processes
                .iter()
                .map(|p| format!("{}: ({:.0}%)", p.name, p.cpu_percent))
                .collect::<Vec<_>>()
                .join(", ");
            
            return Some(TriggerFire {
                context: format!("CPU averaged {:.0}% over the last minute. Top processes: {}", avg, processes_str),
                urgency: Urgency::Normal,
            });
        }

        if !is_high && self.active {
            self.active = false;
        }

        None
    }
}

pub async fn run_loop(state: SharedContext, tx: broadcast::Sender<Packet>) {
    let mut triggers: Vec<Box<dyn Trigger>> = vec![
        Box::new(CpuSustainedTrigger::new())
        ];

    let mut last_fired: HashMap<TriggerKind, Instant> = HashMap::new();
    let cooldown = Duration::from_secs(600);
    
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;

        for trigger in &mut triggers {
            if let Some(fire) = trigger.check(&state) {
                let kind = trigger.kind();

                let cooldown_ok = last_fired.get(&kind)
                    .map(|t| t.elapsed() >= cooldown)
                    .unwrap_or(true);

                if !cooldown_ok { continue };

                let _ = tx.send(Packet::ProactiveTrigger { kind: kind.clone(), context: fire.context, urgency: fire.urgency });
                last_fired.insert(kind, Instant::now());
            }
        }
    }
}
