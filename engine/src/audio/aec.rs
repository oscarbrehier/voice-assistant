use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError},
    },
    time::Duration,
};

use aec3::voip::VoipAec3;

use crate::audio::{capture::AudioBuffer, wav_dump::WavDump};

pub fn run_aec_loop(
    running: Arc<AtomicBool>,
    raw_mic: AudioBuffer,
    mic_channels: usize,
    cleaned: AudioBuffer,
    capture_rate: u32,
    render_rate: u32,
    render_rx: Receiver<Vec<f32>>,
) {
    let capture_frame = (capture_rate / 100) as usize;
    let render_frame = (render_rate / 100) as usize;

    let mut aec = match VoipAec3::builder(capture_rate as usize, 1, 1)
        .render_sample_rate_hz(render_rate as usize)
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

    let mut render_acc: VecDeque<f32> = VecDeque::with_capacity(render_frame * 4);
    let mut capture_acc: VecDeque<f32> = VecDeque::with_capacity(capture_frame * 4);

    let mut out_frame = vec![0.0f32; capture_frame];

    let raw_dump = Arc::new(WavDump::new("aec_raw.wav", capture_rate).ok());
    let render_dump = Arc::new(WavDump::new("aec_render.wav", render_rate).ok());
    let cleaned_dump = Arc::new(WavDump::new("aec_cleaned.wav", capture_rate).ok());

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

        while capture_acc.len() >= capture_frame {
            let mut cap_frame = Vec::with_capacity(capture_frame);
            for _ in 0..capture_frame {
                cap_frame.push(capture_acc.pop_front().unwrap());
            }

            let r_frame: Vec<f32> = if render_acc.len() >= render_frame {
                let mut rf = Vec::with_capacity(render_frame);
                for _ in 0..render_frame {
                    rf.push(render_acc.pop_front().unwrap());
                }
                rf
            } else {
                vec![0.0; render_frame]
            };

            if let Some(d) = raw_dump.as_ref().as_ref() {
                d.write_samples(&cap_frame);
            }
            if let Some(d) = render_dump.as_ref().as_ref() {
                d.write_samples(&r_frame);
            }

            match aec.process(&cap_frame, Some(&r_frame), false, &mut out_frame) {
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
