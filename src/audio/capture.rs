use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use cpal::{
    Device, Stream, SupportedStreamConfig,
    traits::{DeviceTrait, StreamTrait},
};
use tracing::{Level, span};

use crate::{
    AudioQueue, State,
    audio::utils::{has_speech, to_mono},
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
                buffer.extend(data.iter().map(|&sample| sample as f32 / 32768.0));
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
    tx: mpsc::Sender<Vec<f32>>,
    sample_rate: usize,
    channels: usize,
    assistant_active: Arc<AtomicBool>,
) {
    let _loop_span = span!(Level::INFO, "vad_loop").entered();

    let chunk_duration_spec = 2;
    let overlap_duration = 0.25;

    let chunk_size = sample_rate * channels * chunk_duration_spec as usize;
    let overlap_size = sample_rate * channels * overlap_duration as usize;

    let mut state: State = State::Silence;
    let mut speech_buffer: Vec<f32> = Vec::new();
    let silence_threshold_chunks = 1;
    let mut silence_counter = 0;

    while running.load(Ordering::SeqCst) {
        thread::sleep(std::time::Duration::from_millis(100));

        let mut queue = audio_buffer.lock().unwrap();

        if assistant_active.load(Ordering::SeqCst) {
            queue.clear();
            speech_buffer.clear();
            state = State::Silence;
            continue;
        }

        if queue.len() >= chunk_size {
            let processing_span = span!(Level::DEBUG, "processing_audio_chunk").entered();

            let drain_size = chunk_size - overlap_size;
            let chunk: Vec<f32> = queue.drain(..drain_size).collect();

            let overlap: Vec<f32> = queue.iter().take(overlap_size).copied().collect();

            let mut full_chunk = chunk;
            full_chunk.extend(overlap);

            drop(queue);

            let mono = to_mono(&full_chunk, channels);

            if has_speech(&mono, 0.005) {
                if state == State::Silence {
                    state = State::Recording;
                }

                speech_buffer.extend(mono);
                silence_counter = 0;
            } else {
                if state == State::Recording {
                    silence_counter += 1;

                    if silence_counter >= silence_threshold_chunks {
                        if tx.send(std::mem::take(&mut speech_buffer)).is_err() {
                            break;
                        }

                        state = State::Silence;
                    } else {
                        speech_buffer.extend(mono);
                    }
                }
            }
        }
    }
}
