use cpal::{
    Device,
    traits::{DeviceTrait, HostTrait},
};

pub struct InputDeviceInfo {
    pub index: usize,
    pub name: String,
    pub device: Device,
}

pub fn list_input_devices() -> Result<Vec<InputDeviceInfo>, anyhow::Error> {
    let host = cpal::default_host();
    let devices = host.devices()?.enumerate().collect::<Vec<_>>();

    let mut result = Vec::new();
    for (i, device) in devices {
        let name = device
            .description()
            .map(|desc| desc.name().to_string())
            .map_err(|e| anyhow::anyhow!("Failed to get device name: {}", e))?;

        result.push(InputDeviceInfo {
            index: i,
            name,
            device,
        });
    }

    Ok(result)
}

pub fn select_device_by_index(
    devices: &[InputDeviceInfo],
    index: usize,
) -> Result<&Device, anyhow::Error> {
    devices
        .get(index)
        .map(|device_info| &device_info.device)
        .ok_or_else(|| anyhow::anyhow!("Device index {} not found", 8))
}
