pub fn resample_to_16khz(audio: &[f32], from_rate: usize) -> Vec<f32> {
    if from_rate == 16000 {
        return audio.to_vec();
    }

    let ratio = from_rate as f64 / 16000.0;
    let output_len = (audio.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = (src_pos - idx as f64) as f32;

        if idx + 1 < audio.len() {
            output.push(audio[idx] * (1.0 - frac) + audio[idx + 1] * frac);
        } else if idx < audio.len() {
            output.push(audio[idx]);
        }
    }

    output
}

pub fn to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 2 {
        samples.chunks(2).map(|c| (c[0] + c[1]) / 2.0).collect()
    } else {
        samples.to_vec()
    }
}

pub fn has_speech(audio: &[f32], threshold: f32) -> bool {
    let sum_squares: f32 = audio.iter().map(|&s| s * s).sum();
    let rms = (sum_squares / audio.len() as f32).sqrt();

    rms > threshold
}
