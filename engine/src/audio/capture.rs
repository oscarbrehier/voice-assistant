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
    State,
    audio::{
        Packet,
        utils::{has_speech, to_mono},
    },
};

pub type AudioBuffer = Arc<Mutex<VecDeque<f32>>>;

struct CircularBuffer {
    buffer: Vec<f32>,
    write_index: usize,
    full: bool,
}

impl CircularBuffer {
    fn new(durations_secs: f32, sample_rate: usize) -> Self {
        let capacity = (durations_secs * sample_rate as f32) as usize;
        Self {
            buffer: vec![0.0; capacity],
            write_index: 0,
            full: false,
        }
    }

    fn push(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.buffer[self.write_index] = sample;
            self.write_index = (self.write_index + 1) % self.buffer.len();

            if self.write_index == 0 {
                self.full = true;
            }
        }
    }

    fn get_all(&self) -> Vec<f32> {
        if !self.full {
            self.buffer[..self.write_index].to_vec()
        } else {
            let mut result = Vec::with_capacity(self.buffer.len());

            result.extend_from_slice(&self.buffer[self.write_index..]);
            result.extend_from_slice(&self.buffer[..self.write_index]);
            result
        }
    }
}

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
    audio_buffer: AudioBuffer,
    tx: broadcast::Sender<Packet>,
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

    let wake_word_duration_secs = 1.5;
    let mut circular_buffer = CircularBuffer::new(wake_word_duration_secs, sample_rate);

    let mut last_speech_instant = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(20);

    let max_speech_duration_secs = 30;
    let max_speech_samples = channels * sample_rate * max_speech_duration_secs;

    while running.load(Ordering::SeqCst) {
        thread::sleep(std::time::Duration::from_millis(10));

        let current_state = state.load(Ordering::SeqCst);

        if current_state == State::Processing as u8 || current_state == State::Speaking as u8 {
            let mut queue = audio_buffer.lock().unwrap();
            queue.clear();
            drop(queue);
            speech_buffer.clear();
            circular_buffer = CircularBuffer::new(wake_word_duration_secs, sample_rate);
            continue;
        }

        let mut queue = audio_buffer.lock().unwrap();

        if queue.len() >= pulse_chunk_size {
            let pulse_samples: Vec<f32> =
                queue.iter().rev().take(pulse_chunk_size).copied().collect();
            let pulse_mono = to_mono(&pulse_samples, channels);

            let _ = tx.send(Packet::Pulse(pulse_mono));
        }

        if current_state == State::Active as u8 {
            if last_speech_instant.elapsed() > timeout {
                State::broadcast(State::Idle, &state, &tx);
            }
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

            circular_buffer.push(&mono);

            let current_state = state.load(Ordering::SeqCst);

            if has_speech(&mono, 0.015) {
                last_speech_instant = std::time::Instant::now();

                if current_state == State::Idle as u8 {
                    State::broadcast(State::Recording, &state, &tx);
                }

                let updated_state = state.load(Ordering::SeqCst);

                if updated_state == State::Recording as u8 && circular_buffer.full {
                    let wake_word_audio = circular_buffer.get_all();
                    let _ = tx.send(Packet::WakeWordCheck(wake_word_audio));
                    circular_buffer = CircularBuffer::new(wake_word_duration_secs, sample_rate);
                }

                if updated_state == State::Active as u8 {
                    if speech_buffer.len() + mono.len() > max_speech_samples {
                        if !speech_buffer.is_empty() {
                            let _ = tx.send(Packet::Speech(std::mem::take(&mut speech_buffer)));
                        }

                        speech_buffer = mono;
                        silence_counter = 0;
                    } else {
                        speech_buffer.extend(mono);
                        silence_counter = 0;
                    }
                }
            } else {
                if !speech_buffer.is_empty() {
                    silence_counter += 1;

                    if silence_counter >= silence_threshold_chunks {
                        let _ = tx.send(Packet::Speech(std::mem::take(&mut speech_buffer)));

                        if current_state == State::Recording as u8 {
                            State::broadcast(State::Idle, &state, &tx);
                        }

                        silence_counter = 0;
                    } else {
                        speech_buffer.extend(mono);
                    }
                } else {
                    silence_counter = 0;
                }
            }
        }

        // can be removed (fallback for the time being)
        if speech_buffer.len() > max_speech_samples {
            println!(
                "buffer safety triggered: cleared {} samples",
                speech_buffer.len()
            );
            speech_buffer.clear();
            State::broadcast(State::Idle, &state, &tx);
        }
    }
}
