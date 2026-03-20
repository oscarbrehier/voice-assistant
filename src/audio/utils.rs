pub fn wav_to_f32(path: &str) -> Result<(Vec<f32>, u32), anyhow::Error> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let sample_format = spec.sample_format;
    let channels = spec.channels;

    let samples: Vec<f32> = match sample_format {
        hound::SampleFormat::Float => reader.into_samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let max_int = i16::MAX as f32;
            reader
                .into_samples::<i16>()
                .map(|s| s.unwrap() as f32 / max_int)
                .collect()
        }
    };

    let mono_samples = if channels == 2 {
        samples
            .chunks(2)
            .map(|chunk| (chunk[0] + chunk[1]) / 2.0)
            .collect()
    } else {
        samples
    };

    Ok((mono_samples, spec.sample_rate))
}

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
