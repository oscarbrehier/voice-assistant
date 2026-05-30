use std::path::Path;
use std::sync::Mutex;

use hound::{SampleFormat, WavSpec, WavWriter};

pub struct WavDump {
    writer: Mutex<Option<WavWriter<std::io::BufWriter<std::fs::File>>>>,
    sample_rate: u32,
}

impl WavDump {
    pub fn new<P: AsRef<Path>>(path: P, sample_rate: u32) -> anyhow::Result<Self> {
        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let writer = WavWriter::create(path, spec)?;
        Ok(Self {
            writer: Mutex::new(Some(writer)),
            sample_rate,
        })
    }

    pub fn write_samples(&self, samples: &[f32]) {
        let mut guard = self.writer.lock().unwrap();
        if let Some(w) = guard.as_mut() {
            for &s in samples {
                let _ = w.write_sample(s);
            }
        }
    }

    pub fn finalize(&self) {
        let mut guard = self.writer.lock().unwrap();
        if let Some(w) = guard.take() {
            let _ = w.finalize();
        }
    }
}

impl Drop for WavDump {
    fn drop(&mut self) {
        self.finalize();
    }
}