use cpal::traits::HostTrait;
use rodio::{Decoder, DeviceSinkBuilder, DeviceTrait, MixerDeviceSink, Player, Source};
use std::{
    io::BufReader,
    num::NonZero,
    sync::{Arc, atomic::AtomicUsize},
    time::Duration,
};
use strsim::jaro_winkler;

use crate::state::SharedContext;

struct TappedSource<S, F>
where
    S: Source<Item = f32>,
    F: FnMut(f32),
{
    inner: S,
    tap: F,
}

impl<S, F> Iterator for TappedSource<S, F>
where
    S: Source<Item = f32>,
    F: FnMut(f32),
{
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let s = self.inner.next()?;
        (self.tap)(s);
        Some(s)
    }
}

impl<S, F> Source for TappedSource<S, F>
where
    S: Source<Item = f32>,
    F: FnMut(f32),
{
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> NonZero<u16> {
        self.inner.channels()
    }
    fn sample_rate(&self) -> NonZero<u32> {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
}

pub fn get_device_from_name(name: &str) -> anyhow::Result<cpal::Device> {
    let host = cpal::default_host();
    let query_lower = name.to_lowercase();

    let mut best_match: Option<(f64, cpal::Device)> = None;

    for candidate in host.output_devices()? {
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

    best_match
        .map(|(_, d)| d)
        .ok_or_else(|| anyhow::anyhow!("No output device found matching: {}", name))
}

fn open_output(device: Option<cpal::Device>) -> anyhow::Result<MixerDeviceSink> {
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
    let device = context.audio_devices.read().output.clone();
    let sink_handle = open_output(device)?;

    let player = Player::connect_new(sink_handle.mixer());

    let file = std::fs::File::open(path)?;
    let source = Decoder::new(BufReader::new(file))
        .map_err(|e| anyhow::anyhow!("Failed to decode mp3: {e}"))?;

    let render_tx = context.aec_render_tx.clone();
    let mut render_chunk: Vec<f32> = Vec::with_capacity(480);

    let tapped = TappedSource {
        inner: source,
        tap: move |s: f32| {
            render_chunk.push(s);
            render_chunk.push(s);
            if render_chunk.len() >= 480 {
                let chunk = std::mem::replace(&mut render_chunk, Vec::with_capacity(480));
                let _ = render_tx.try_send(chunk);
            }
        },
    };

    player.append(tapped);


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
