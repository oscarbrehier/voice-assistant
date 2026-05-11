use std::{path::Path, time::Instant};

use anyhow::Context;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

pub struct STT {
    ctx: WhisperContext,
    state: WhisperState,
}

impl STT {
    pub fn new<P: AsRef<Path>>(model_path: P) -> anyhow::Result<Self> {
        let mut params = WhisperContextParameters::default();
        params.use_gpu(true);

        let ctx = WhisperContext::new_with_params(model_path, params)?;

        let state = ctx.create_state()?;

        Ok(Self { ctx, state })
    }

    pub fn transcribe(&mut self, samples: &[f32]) -> anyhow::Result<String> {
        let total_start = Instant::now();

        println!("Starting transcription for {} samples", samples.len());

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        params.set_n_threads(4);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        params.set_translate(false);
        params.set_language(Some("en"));
        params.set_no_context(true);

        params.set_max_len(0);

        self.state
            .full(params, samples)
            .context("Failed to run whisper inference")?;

        let mut result = String::new();

        for segment in self.state.as_iter() {
            result.push_str(&segment.to_string());
        }

        let total_time = total_start.elapsed();
        println!(
            "Transcription complete: '{}' ({} samples in {:?})",
            result.trim(),
            samples.len(),
            total_time
        );

        Ok(result.trim().to_string())
    }
}
