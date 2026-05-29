use parking_lot::RwLock;
use std::sync::{Arc, atomic::AtomicU8};

use serde::Serialize;

use crate::{
    audio::{capture::AudioBuffer, output::get_device_from_name, voice::SpeakerID},
    memory::MemoryManager,
};

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProcessSnapshot {
    pub name: String,
    pub cpu_percent: f32,
    pub memory_mb: u64,
}

#[derive(Default, Serialize, Clone)]
pub struct Vitals {
    pub cpu_load: f64,
    pub cpu_temperature: Option<u32>,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub timestamp: String,
    pub top_processes: Vec<ProcessSnapshot>,
}

pub struct AudioDevices {
    pub input: Option<String>,
    pub output: Option<cpal::Device>,
}

impl AudioDevices {
    pub fn change_output(&mut self, name: &str) -> anyhow::Result<()> {
        let device = get_device_from_name(name)?;
        self.output = Some(device);

        Ok(())
    }
}

pub struct GlobalContext {
    pub telemetry: RwLock<Vitals>,
    pub audio_player: RwLock<Option<rodio::Player>>,
    pub engine_state: Arc<AtomicU8>,
    pub speaker: RwLock<SpeakerID>,
    pub audio_devices: RwLock<AudioDevices>,
    pub cleaned_audio_buffer: AudioBuffer,
    pub aec_render_tx: std::sync::mpsc::SyncSender<Vec<f32>>
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
