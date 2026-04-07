use rodio::DeviceTrait;
use serde::Serialize;

use crate::{State, audio::devices::{list_input_devices, select_device_by_index}};

pub mod capture;
pub mod devices;
pub mod output;
pub mod stt;
pub mod tts;
pub mod utils;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "content")]
pub enum Packet {
    Pulse(Vec<f32>),
    Speech(Vec<f32>),
    WakeWordCheck(Vec<f32>),
    Volume(f32),
    Transcription(String),
    State(State)
}

impl Packet {
    pub fn process(self) -> Packet {
        if let Packet::Pulse(samples) = self {
            let peak = samples.iter().map(|s| s.abs()).fold(0.0, f32::max);

            let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();
            let rms = (sum_squares / samples.len() as f32).sqrt();

            let combined = (rms * 0.7) + (peak * 0.3);
            let sensitivity = 20.0;
            let volume = (combined * sensitivity).powf(0.6).clamp(0.0, 1.0);

            return Packet::Volume(volume);
        }
        self
    }
}

pub fn setup_audio_device(
    device_index: Option<usize>,
) -> anyhow::Result<(cpal::Device, cpal::SupportedStreamConfig)> {
    let device_list = list_input_devices().expect("failed to list devices");
    for device in device_list.iter() {
        println!("{} - {}", device.index, device.name);
    }

    let selected_index = device_index.unwrap_or(0);
    let device = select_device_by_index(&device_list, selected_index)
        .expect("failed to get device")
        .to_owned();

    println!("using input device: {:?}", device.description());

    let config = if device.supports_input() {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .expect("failed to get default output/input config")
    .to_owned();

    Ok((device, config))
}
