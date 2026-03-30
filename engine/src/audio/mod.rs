use rodio::DeviceTrait;

use crate::audio::devices::{list_input_devices, select_device_by_index};

pub mod capture;
pub mod devices;
pub mod output;
pub mod stt;
pub mod tts;
pub mod utils;

pub fn setup_audio_device(
    device_index: Option<usize>,
) -> anyhow::Result<(cpal::Device, cpal::SupportedStreamConfig)> {

    let device_list = list_input_devices().expect("failed to list devices");
    for device in device_list.iter() {
        println!("{} - {}", device.index, device.name);
    }

	let selected_index = device_index.unwrap_or(0);
    let device = select_device_by_index(&device_list, selected_index).expect("failed to get device").to_owned();

    println!("using input device: {:?}", device.description());

    let config = if device.supports_input() {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .expect("failed to get default output/input config")
    .to_owned();

	Ok((device, config))
}
