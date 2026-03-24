use rodio::{Decoder, DeviceSinkBuilder, Player};
use std::{
    io::{BufReader, Cursor},
    sync::Arc,
};

use cpal::{
    StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

pub fn play_pcm_audio(samples: &[i16]) -> anyhow::Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");

    let config = StreamConfig {
        channels: 1,
        sample_rate: 44100,
        buffer_size: cpal::BufferSize::Default,
    };

    let err_fn = |err| eprintln!("audio stream error: {}", err);

    let samples = Arc::new(samples.to_vec());
    let samples_clone = samples.clone();

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            for (i, &sample) in samples_clone.iter().enumerate() {
                if i < data.len() {
                    data[i] = sample as f32 / 32768.0;
                }
            }
        },
        err_fn,
        None,
    )?;

    stream.play()?;
    std::thread::sleep(std::time::Duration::from_secs(3));

    Ok(())
}

pub fn play_mp3_audio(path: &str) -> anyhow::Result<()> {
    let sink_handle = DeviceSinkBuilder::open_default_sink()
        .map_err(|e| anyhow::anyhow!("Failed to open audio device: {}", e))?;

    let player = Player::connect_new(sink_handle.mixer());

    let file = std::fs::File::open(path)?;
    let source = Decoder::new(BufReader::new(file))
        .map_err(|e| anyhow::anyhow!("Failed to decode mp3: {e}"))?;

    player.append(source);
    player.sleep_until_end();

    drop(sink_handle);

    Ok(())
}
