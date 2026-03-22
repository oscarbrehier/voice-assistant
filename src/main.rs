use std::{
    collections::VecDeque,
    fs::OpenOptions,
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use clap::Parser;
use cpal::traits::DeviceTrait;

use crate::{
    actions::{Action, execute_action}, audio::{
        capture::init_audio_capture,
        devices::{list_input_devices, select_device_by_index},
        utils::{has_speech, resample_to_16khz, to_mono},
    }, commands::CommandMatcher, stt::stt_service::STTService
};

mod audio;
mod stt;
mod actions;
mod commands;

#[derive(Parser, Debug)]
#[command(version, about = "")]
struct Opt {
    #[arg(short, long)]
    device: Option<usize>,
}

type AudioQueue = Arc<Mutex<VecDeque<f32>>>;

fn main() -> Result<(), anyhow::Error> {
    dotenv::dotenv().ok();

    let mut stt = STTService::new()?;

    let command_matcher = CommandMatcher::from_file("config/commands.json")?;

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    ctrlc::set_handler(move || {
        running_clone.store(false, Ordering::SeqCst);
    })
    .expect("failed to set ctrlc handler");

    let opt = Opt::parse();
    let device_index = opt.device.unwrap_or(0);

    let device_list = list_input_devices().expect("failed to list devices");

    for device in device_list.iter() {
        println!("{} - {}", device.index, device.name);
    }

    let device = select_device_by_index(&device_list, device_index).expect("failed to get device");

    println!("using input device: {:?}", device.description());

    let config = if device.supports_input() {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .expect("failed to get default output/input config")
    .to_owned();

    let sample_rate = config.sample_rate() as usize;
    let sample_rate_for_thread = sample_rate;

    let channels = config.channels() as usize;

    let (tx, rx) = mpsc::channel::<Vec<f32>>();

    let (stream, audio_buffer) =
        init_audio_capture(&device, config).expect("failed to init audio capture");

    let last_transcription = Arc::new(Mutex::new(String::new()));
    let last_transcription_clone = last_transcription.clone();

    let transcription_handle = thread::spawn(move || {
        while let Ok(chunk) = rx.recv() {
            let resampled = resample_to_16khz(&chunk, sample_rate_for_thread);

            match stt.transcribe(&resampled) {
                Ok(transcription) => {
                    let mut trimmed = transcription.trim().to_lowercase().to_string();

                    if !trimmed.is_empty() {
                        let mut last = last_transcription_clone.lock().unwrap();

                        let action = command_matcher.match_command(&trimmed);

                        println!("action: {:?}", action);

                        if action != Action::Unknown {
                            trimmed.push_str(&format!("command: {:?}", action));
                            let _ = execute_action(action);
                        }

                        if trimmed != *last {
                            println!("{}", trimmed);

                            if let Ok(mut file) = OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open("transcription.txt")
                            {
                                writeln!(file, "{}", trimmed).ok();
                            }

                            *last = trimmed.to_string();
                        }
                    }
                }
                Err(e) => eprintln!("transcription error: {}", e),
            }
        }
    });

    let chunk_duration_spec = 2;
    let overlap_duration = 0.25;

    let chunk_size = sample_rate * channels * chunk_duration_spec as usize;
    let overlap_size = sample_rate * channels * overlap_duration as usize;

    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_secs(chunk_duration_spec as u64));

        let mut queue = audio_buffer.lock().unwrap();

        if queue.len() >= chunk_size {
            let drain_size = chunk_size - overlap_size;
            let chunk: Vec<f32> = queue.drain(..drain_size).collect();

            let overlap: Vec<f32> = queue.iter().take(overlap_size).copied().collect();

            let mut full_chunk = chunk;
            full_chunk.extend(overlap);

            drop(queue);

            let mono = to_mono(&full_chunk, channels);

            if has_speech(&mono, 0.005) {
                if tx.send(mono).is_err() {
                    break;
                }
            }
        }
    }

    drop(tx);
    drop(stream);

    if let Err(e) = transcription_handle.join() {
        eprintln!("Transcription thread panicked: {:?}", e);
    }

    Ok(())
}
