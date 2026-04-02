use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    },
    thread,
};

use cpal::{
    Device, Stream, SupportedStreamConfig,
    traits::{DeviceTrait, StreamTrait},
};
use tokio::sync::broadcast;

use crate::{
    AudioQueue, State,
    audio::{
        AudioMessage,
        utils::{has_speech, to_mono},
    },
};

pub type AudioBuffer = Arc<Mutex<VecDeque<f32>>>;

pub fn init_audio_capture(
    device: &Device,
    config: SupportedStreamConfig,
) -> Result<(Stream, AudioBuffer), anyhow::Error> {
    let audio_buffer = Arc::new(Mutex::new(VecDeque::new()));
    let audio_buffer_clone = audio_buffer.clone();

    let err_fn = move |err| {
        eprintln!("An error occurred during stream: {err}");
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::I8 => device.build_input_stream(
            &config.into(),
            move |data: &[i8], _: &_| {
                let mut buffer = audio_buffer_clone.lock().unwrap();
                buffer.extend(data.iter().map(|&sample| sample as f32 / 128.0));
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &_| {
                let mut buffer = audio_buffer_clone.lock().unwrap();
                buffer.extend(data.iter().map(|&sample| sample as f32 / 32768.0));
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I32 => device.build_input_stream(
            &config.into(),
            move |data: &[i32], _: &_| {
                let mut buffer = audio_buffer_clone.lock().unwrap();
                buffer.extend(data.iter().map(|&sample| sample as f32 / 2147483648.0));
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &_| {
                let mut buffer = audio_buffer_clone.lock().unwrap();
                buffer.extend(data.iter().copied());
            },
            err_fn,
            None,
        )?,
        sample_format => {
            return Err(anyhow::Error::msg(format!(
                "Unsupported sample format: {sample_format}"
            )));
        }
    };

    stream.play()?;

    Ok((stream, audio_buffer))
}

pub fn run_vad_loop(
    running: Arc<AtomicBool>,
    audio_buffer: AudioQueue,
    tx: broadcast::Sender<AudioMessage>,
    sample_rate: usize,
    channels: usize,
    assistant_active: Arc<AtomicBool>,
    state: Arc<AtomicU8>,
) {
    let vad_chunk_duration_spec = 2;
    let pulse_chunk_duration_ms = 50;
    let overlap_duration = 0.25;

    let vad_chunk_size = sample_rate * channels * vad_chunk_duration_spec as usize;
    let pulse_chunk_size = (sample_rate * channels * pulse_chunk_duration_ms) / 1000;
    let overlap_size = sample_rate * channels * overlap_duration as usize;

    let mut speech_buffer: Vec<f32> = Vec::new();
    let silence_threshold_chunks = 1;
    let mut silence_counter = 0;

    let mut last_speech_instant = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(20);

    while running.load(Ordering::SeqCst) {
        thread::sleep(std::time::Duration::from_millis(10));

        let current_state = state.load(Ordering::SeqCst);

        if current_state == State::Active as u8 {
            if last_speech_instant.elapsed() > timeout {
                state.store(State::Idle as u8, Ordering::SeqCst);
            }
        }

        let mut queue = audio_buffer.lock().unwrap();

        if queue.len() >= pulse_chunk_size {
            let pulse_samples: Vec<f32> = queue.iter().take(pulse_chunk_size).copied().collect();
            let pulse_mono = to_mono(&pulse_samples, channels);

            let _ = tx.send(AudioMessage::Pulse(pulse_mono));
        }

        if queue.len() > (vad_chunk_size as f32 * 1.5) as usize {
            let to_drop = queue.len() - vad_chunk_size;
            queue.drain(..to_drop);
        }

        if assistant_active.load(Ordering::SeqCst) {
            last_speech_instant = std::time::Instant::now();

            queue.clear();
            speech_buffer.clear();
            continue;
        }

        if queue.len() >= vad_chunk_size {
            let drain_size = vad_chunk_size - overlap_size;
            let chunk: Vec<f32> = queue.drain(..drain_size).collect();

            let overlap: Vec<f32> = queue.iter().take(overlap_size).copied().collect();

            let mut full_chunk = chunk;
            full_chunk.extend(overlap);

            drop(queue);

            let mono = to_mono(&full_chunk, channels);

            if has_speech(&mono, 0.015) {
                last_speech_instant = std::time::Instant::now();

                if current_state == State::Idle as u8 {
                    state.store(State::Recording as u8, Ordering::SeqCst);
                }

                speech_buffer.extend(mono);
                silence_counter = 0;
            } else {
                if current_state == State::Recording as u8 || current_state == State::Active as u8 {
                    silence_counter += 1;

                    if silence_counter >= silence_threshold_chunks {
                        if !speech_buffer.is_empty() {
                            let _ =
                                tx.send(AudioMessage::Speech(std::mem::take(&mut speech_buffer)));
                        }

                        if current_state == State::Recording as u8 {
                            state.store(State::Idle as u8, Ordering::SeqCst);
                        }

                        silence_counter = 0;
                    } else {
                        speech_buffer.extend(mono);
                    }
                }
            }
        }
    }
}
