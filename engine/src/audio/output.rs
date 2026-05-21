use cpal::traits::HostTrait;
use rodio::{Decoder, DeviceSinkBuilder, DeviceTrait, MixerDeviceSink, Player};
use std::{io::BufReader, time::Duration};
use strsim::jaro_winkler;

use crate::state::SharedContext;

fn open_output(name: Option<&str>) -> anyhow::Result<MixerDeviceSink> {
    let device: Option<cpal::Device> = if let Some(name) = name {
        let host = cpal::default_host();
        let devices = host.output_devices()?;
        let query_lower = name.to_lowercase();
        let mut best_match: Option<(f64, cpal::Device)> = None;

        for candidate in devices {
            if let Ok(device) = candidate.description() {
                let device_name = device.name().to_lowercase();
                let score = if device_name.contains(&query_lower) {
                    0.9 + (0.1 * (query_lower.len() as f64 / device_name.len() as f64))
                } else {
                    jaro_winkler(&query_lower, &device_name)
                };

                if score > 0.7 && best_match.as_ref().map_or(true, |(best, _)| score > *best) {
                    best_match = Some((score, candidate));
                }
            }
        }

        let matched = best_match
            .map(|(_, d)| d)
            .ok_or_else(|| anyhow::anyhow!("No output device found matching: {}", name))?;
        Some(matched)
    } else {
        None
    };

    match device {
        Some(device) => DeviceSinkBuilder::from_device(device)
            .map_err(|e| anyhow::anyhow!("Failed to open audio device: {}", e))?
            .open_sink_or_fallback()
            .map_err(|e| anyhow::anyhow!("Failed to open audio device: {}", e)),
        None => DeviceSinkBuilder::open_default_sink()
            .map_err(|e| anyhow::anyhow!("Failed to open audio device: {}", e)),
    }
}

pub fn play_mp3_audio(path: &str, context: SharedContext) -> anyhow::Result<()> {
    let sink_handle = open_output(context.audio_devices.read().output.as_deref())?;

    let player = Player::connect_new(sink_handle.mixer());

    let file = std::fs::File::open(path)?;
    let source = Decoder::new(BufReader::new(file))
        .map_err(|e| anyhow::anyhow!("Failed to decode mp3: {e}"))?;

    player.append(source);

    {
        let mut lock = context.audio_player.write();
        *lock = Some(player);
    }

    loop {
        let (still_active, is_finished) = {
            let lock = context.audio_player.read();
            match &*lock {
                Some(p) => (true, p.len() == 0),
                None => (false, true),
            }
        };

        if !still_active || is_finished {
            break;
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    let mut lock = context.audio_player.write();
    *lock = None;

    Ok(())
}
