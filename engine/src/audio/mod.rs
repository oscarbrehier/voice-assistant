use cpal::traits::HostTrait;
use rodio::DeviceTrait;
use serde::Serialize;

use crate::{
    State,
    audio::devices::{list_devices, select_device_by_index},
    proactive::TriggerKind,
};

pub mod aec;
pub mod capture;
pub mod devices;
pub mod enrollment;
pub mod onboarding;
pub mod output;
pub mod stt;
pub mod tts;
pub mod utils;
pub mod voice;
pub mod wav_dump;

pub struct AudioDeviceSelection {
    pub input_device: cpal::Device,
    pub input_config: cpal::SupportedStreamConfig,
    pub loopback_device: cpal::Device,
    pub loopback_config: cpal::SupportedStreamConfig,
}

pub fn setup_audio_device(
    input_device_index: Option<usize>,
    output_device_index: Option<usize>,
) -> anyhow::Result<AudioDeviceSelection> {
    let host = cpal::default_host();

    let input_devices: Vec<_> = host.input_devices()?.collect();
    println!("-- input devices --");
    for (i, d) in input_devices.iter().enumerate() {
        let name = d
            .description()
            .map(|desc| desc.name().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string());
        println!("  [{}] {}", i, name);
    }

    let input_index = input_device_index.unwrap_or(0);
    let input_device = input_devices
        .get(input_index)
        .ok_or_else(|| anyhow::anyhow!("input device at {} out of range", input_index))?
        .clone();

    let input_config = input_device
        .default_input_config()
        .map_err(|e| anyhow::anyhow!("failed to get input config: {}", e))?;

    let output_devices: Vec<_> = host.output_devices()?.collect();
    println!("-- output devices (for loopback) --");
    for (i, d) in output_devices.iter().enumerate() {
        let name = d
            .description()
            .map(|desc| desc.name().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string());
        println!("  [{}] {}", i, name);
    }

    let output_idx = output_device_index.unwrap_or(0);
    let loopback_device = output_devices
        .get(output_idx)
        .ok_or_else(|| anyhow::anyhow!("output device index {} out of range", output_idx))?
        .clone();
    let loopback_name = loopback_device.name().unwrap_or_default();
    println!("using loopback device: {}", loopback_name);

    let loopback_config = loopback_device
        .default_output_config()
        .map_err(|e| anyhow::anyhow!("failed to get output config for loopback: {}", e))?;

    Ok(AudioDeviceSelection {
        input_device,
        input_config,
        loopback_device,
        loopback_config,
    })
}
