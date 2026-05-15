use std::sync::{Arc, atomic::{AtomicU8}};
use parking_lot::RwLock;

use serde::Serialize;

use crate::{audio::voice::SpeakerID, memory::MemoryManager};

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProcessSnapshot {
    pub name: String,
    pub cpu_percent: f32,
    pub memory_mb: u64
}

#[derive(Default, Serialize, Clone)]
pub struct Vitals {
    pub cpu_load: f64,
    pub cpu_temperature: Option<u32>,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub timestamp: String,
    pub top_processes: Vec<ProcessSnapshot>
}

pub struct GlobalContext {
    pub telemetry: RwLock<Vitals>,
    pub audio_player: RwLock<Option<rodio::Player>>,
    pub engine_state: Arc<AtomicU8>,
    pub speaker: RwLock<SpeakerID>,
}

impl GlobalContext {
    pub fn get_vitals_snapshot(&self) -> String {

        let data = self.telemetry.read();
        
        format!(
            "CPU: {}% ({}°C) | RAM: {:.1}/{:.1}GB",
            data.cpu_load.round(),
            data.cpu_temperature.unwrap_or(0),
            data.ram_used_gb,
            data.ram_total_gb
        )
        
    }
}

pub type SharedContext = Arc<GlobalContext>;