use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, TryRecvError},
    },
    time::Duration,
};

use aec3::voip::VoipAec3;

use crate::audio::{capture::AudioBuffer, wav_dump::WavDump};

const CAPTURE_RATE: u32 = 48_000;
const RENDER_RATE: u32 = 24_000;
const CAPTURE_FRAME: usize = 480;
const RENDER_FRAME: usize = 240;

pub fn run_aec_loop(
    running: Arc<AtomicBool>,
    raw_mic: AudioBuffer,
    mic_channels: usize,
    cleaned: AudioBuffer,
    render_rx: Receiver<Vec<f32>>,
) {
    let mut aec = match VoipAec3::builder(CAPTURE_RATE as usize, 1, 1)
        .render_sample_rate_hz(24_000)
        .enable_high_pass(true)
        .enable_noise_suppression(true)
        .build()
    {
        Ok(a) => a,
        Err(e) => {
            eprintln!("AEC failed to build the pipeline: {:?}", e);
            return;
        }
    };

    let mut render_acc: VecDeque<f32> = VecDeque::with_capacity(RENDER_FRAME * 4);
    let mut capture_acc: VecDeque<f32> = VecDeque::with_capacity(CAPTURE_FRAME * 4);

    let mut out_frame = vec![0.0f32; CAPTURE_FRAME];

    let raw_dump = Arc::new(WavDump::new("aec_raw.wav", CAPTURE_RATE).ok());
    let render_dump = Arc::new(WavDump::new("aec_render.wav", RENDER_RATE).ok());
    let cleaned_dump = Arc::new(WavDump::new("aec_cleaned.wav", CAPTURE_RATE).ok());

    while running.load(Ordering::SeqCst) {
        loop {
            match render_rx.try_recv() {
                Ok(chunk) => render_acc.extend(chunk),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        {
            let mut q = raw_mic.lock().unwrap();

            if mic_channels == 1 {
                while let Some(s) = q.pop_front() {
                    capture_acc.push_back(s);
                }
            } else {
                let complete_frames = q.len() / mic_channels;

                for _ in 0..complete_frames {
                    let mut sum = 0.0f32;

                    for _ in 0..mic_channels {
                        sum += q.pop_front().unwrap();
                    }

                    capture_acc.push_back(sum / mic_channels as f32);
                }
            }
        }

        while capture_acc.len() >= CAPTURE_FRAME {
            let mut cap_frame = Vec::with_capacity(CAPTURE_FRAME);
            for _ in 0..CAPTURE_FRAME {
                cap_frame.push(capture_acc.pop_front().unwrap());
            }

            let render_frame: Vec<f32> = if render_acc.len() >= RENDER_FRAME {
                let mut rf = Vec::with_capacity(RENDER_FRAME);
                for _ in 0..RENDER_FRAME {
                    rf.push(render_acc.pop_front().unwrap());
                }
                rf
            } else {
                vec![0.0; RENDER_FRAME]
            };

            if let Some(d) = raw_dump.as_ref().as_ref() {
                d.write_samples(&cap_frame);
            }
            if let Some(d) = render_dump.as_ref().as_ref() {
                d.write_samples(&render_frame);
            }

            match aec.process(&cap_frame, Some(&render_frame), false, &mut out_frame) {
                Ok(_metrics) => {
                    if let Some(d) = cleaned_dump.as_ref().as_ref() {
                        d.write_samples(&out_frame);
                    }

                    let mut out = cleaned.lock().unwrap();
                    out.extend(out_frame.iter().copied());
                }
                Err(e) => {
                    eprintln!("AEC process error: {:?}", e);

                    let mut out = cleaned.lock().unwrap();
                    out.extend(cap_frame.iter().copied());
                }
            }
        }

        std::thread::sleep(Duration::from_millis(5));
    }
}
