use std::f32::consts::PI;

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

pub fn has_speech(samples: &[f32], threshold: f32) -> bool {
    let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();
    let rms = (sum_squares / samples.len() as f32).sqrt();

    rms > threshold
}

pub struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl BiquadFilter {
    pub fn new_bandpass(f_center: f32, sample_rate: f32, q: f32) -> Self {
        let omega = 2.0 * std::f32::consts::PI * f_center / sample_rate;
        let alpha = omega.sin() / (2.0 * q);
        let cos_w = omega.cos();

        let a0 = 1.0 + alpha;

        Self {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let out = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * out + self.z2;
        self.z2 = self.b2 * x - self.a2 * out;
        out
    }
}
