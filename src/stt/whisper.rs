use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::utils::resample_to_16khz;

pub fn transcribe(audio_data: &[f32], sample_rate: usize) {

    let model_path = "models/ggml-tiny-q8_0.bin";

    let audio_16khz = if sample_rate != 16000 {
        resample_to_16khz(audio_data, sample_rate)
    } else {
        audio_data.to_vec()
    };

    let context = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
        .expect("failed to load model");

    let mut params = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: 1.0 });

    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    params.set_language(Some("en"));
    params.set_suppress_nst(true);
    params.set_suppress_blank(true);

    params.set_temperature(0.0);
    params.set_max_initial_ts(1.0);
    params.set_thold_pt(0.01);

    let mut state = context.create_state().expect("failed to create state");

    state
        .full(params, &audio_16khz)
        .expect("failed to run model");

    for segment in state.as_iter() {
        println!("{}", segment)
    }
}
