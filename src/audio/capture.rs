use std::{
    collections::VecDeque, fs::File, io::BufWriter, sync::{Arc, Mutex}
};

use cpal::{
    Device, FromSample, Sample, Stream, SupportedStreamConfig,
    traits::{DeviceTrait, StreamTrait},
};

type WavWriterHandle = Arc<Mutex<Option<hound::WavWriter<BufWriter<File>>>>>;
pub type AudioBuffer = Arc<Mutex<VecDeque<f32>>>;

fn write_input_data<T, U>(input: &[T], writer: &WavWriterHandle)
where
    T: Sample,
    U: Sample + hound::Sample + FromSample<T>,
{
    if let Ok(mut guard) = writer.try_lock() {
        if let Some(writer) = guard.as_mut() {
            for &sample in input.iter() {
                let sample: U = U::from_sample(sample);
                if let Err(e) = writer.write_sample(sample) {
                    eprintln!("Failed to write sample: {}", e);
                }
            }
        }
    }
}

fn sample_format(format: cpal::SampleFormat) -> hound::SampleFormat {
    if format.is_float() {
        hound::SampleFormat::Float
    } else {
        hound::SampleFormat::Int
    }
}

fn wav_spec_from_config(config: &cpal::SupportedStreamConfig) -> hound::WavSpec {
    hound::WavSpec {
        channels: config.channels() as _,
        sample_rate: config.sample_rate() as _,
        bits_per_sample: (config.sample_format().sample_size() * 8) as _,
        sample_format: sample_format(config.sample_format()),
    }
}

pub fn init_audio_capture(
    device: &Device,
    config: SupportedStreamConfig,
) -> Result<(Stream, AudioBuffer), anyhow::Error> {

    let audio_buffer = Arc::new(Mutex::new(VecDeque::new()));
    let audio_buffer_clone = audio_buffer.clone();

    let err_fn = move |err| {
        eprintln!("An error occurred during stream: {err}");
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::I8 => device.build_input_stream(
            &config.into(),
            move |data: &[i8], _: &_| {
                let mut buffer = audio_buffer_clone.lock().unwrap();
                buffer.extend(data.iter().map(|&sample| sample as f32 / 128.0));
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &_| {
                let mut buffer = audio_buffer_clone.lock().unwrap();
                buffer.extend(data.iter().map(|&sample| sample as f32 / 32768.0));
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I32 => device.build_input_stream(
            &config.into(),
            move |data: &[i32], _: &_| {
                let mut buffer = audio_buffer_clone.lock().unwrap();
                buffer.extend(data.iter().map(|&sample| sample as f32 / 32768.0));
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &_| {
                let mut buffer = audio_buffer_clone.lock().unwrap();
                buffer.extend(data.iter().copied());
            },
            err_fn,
            None,
        )?,
        sample_format => {
            return Err(anyhow::Error::msg(format!(
                "Unsupported sample format: {sample_format}"
            )));
        }
    };

    stream.play()?;

    Ok((stream, audio_buffer))
}
