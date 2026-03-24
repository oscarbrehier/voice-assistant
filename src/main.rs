use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    }
};

use clap::Parser;
use tokio::runtime::Runtime;

use crate::{
    audio::{
        capture::{init_audio_capture, run_vad_loop},
        setup_audio_device,
        stt::spawn_transcription_worker,
    },
    commands::CommandMatcher,
    llm::LLMEngine,
    stt::stt_service::STTService,
};

mod actions;
mod audio;
mod commands;
mod llm;
mod stt;

#[derive(Parser, Debug)]
#[command(version, about = "")]
struct Opt {
    #[arg(short, long)]
    device: Option<usize>,
}

type AudioQueue = Arc<Mutex<VecDeque<f32>>>;

#[derive(PartialEq)]
enum State {
    Silence,
    Recording,
}

fn main() -> Result<(), anyhow::Error> {
    dotenv::dotenv().ok();

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        running_clone.store(false, Ordering::SeqCst);
    })
    .expect("failed to set ctrlc handler");

    let opt = Opt::parse();

    let (device, config) = setup_audio_device(opt.device)?;
    let sample_rate = config.sample_rate() as usize;
    let channels = config.channels() as usize;
    
    let stt = STTService::new()?;
    let rt = Runtime::new()?;
    let command_matcher = CommandMatcher::from_file("config/commands.json")?;
    let llm_engine = LLMEngine::new(&command_matcher.config);

    let (stream, audio_buffer) =
        init_audio_capture(&device, config).expect("failed to init audio capture");
    let (tx, rx) = mpsc::channel::<Vec<f32>>();

    let worker_handle =
        spawn_transcription_worker(rx, stt, command_matcher, llm_engine, rt, sample_rate);

    run_vad_loop(running, audio_buffer, tx, sample_rate, channels);

    drop(stream);

    if let Err(e) = worker_handle.join() {
        eprintln!("Transcription thread panicked: {:?}", e);
    }

    Ok(())
}
