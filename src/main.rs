use clap::Parser;
use cpal::{
    traits::{DeviceTrait},
};

use crate::audio::{
    capture::init_audio_capture,
    devices::{list_input_devices, select_device_by_index},
};

mod audio;
mod stt;

#[derive(Parser, Debug)]
#[command(version, about = "")]
struct Opt {
    #[arg(short, long)]
    device: Option<usize>,
}

fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();
    let device_index = opt.device.unwrap_or(0);

    let device_list = list_input_devices().expect("failed to list devices");

    for device in device_list.iter() {
        println!("{} - {}", device.index, device.name);
    }

    let device = select_device_by_index(&device_list, device_index).expect("failed to get device");

    println!("using input device: {:?}", device.description());

    let config = if device.supports_input() {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .expect("failed to get default output/input config")
    .to_owned();

    let (stream, writer) = init_audio_capture(&device, config, "recorded.wav")
        .expect("failed to init audio capture");

    std::thread::sleep(std::time::Duration::from_secs(10));
    drop(stream);
    writer.lock().unwrap().take().unwrap().finalize()?;

    Ok(())
}