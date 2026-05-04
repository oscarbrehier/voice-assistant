use std::sync::{Arc, RwLock, atomic::{AtomicBool, AtomicU8}};

use serde::Serialize;

use crate::audio::voice::SpeakerID;

#[derive(Default, Serialize, Clone)]
pub struct Vitals {
    pub cpu_load: f64,
    pub cpu_temperature: Option<u32>,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub timestamp: String
}

pub struct GlobalContext {
    pub telemetry: Arc<RwLock<Vitals>>,
    pub audio_player: Arc<RwLock<Option<rodio::Player>>>,
    pub engine_state: Arc<AtomicU8>,
    pub speaker: Arc<parking_lot::RwLock<SpeakerID>>
}

impl GlobalContext {
    pub fn get_vitals_snapshot(&self) -> String {

        let data = self.telemetry.read().unwrap_or_else(|e| e.into_inner());
        
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