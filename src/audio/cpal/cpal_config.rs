use crate::globals::DEFAULT_DEVICE;
use anyhow::{Result, anyhow, bail};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, DeviceId, default_host};
use std::str::FromStr;

pub struct CpalConfiguration {}

impl CpalConfiguration {
    pub(crate) fn get_device(device: Option<String>, input: bool) -> Result<Device> {
        let host = default_host();

        if let Some(device) = device
            && device != DEFAULT_DEVICE
        {
            let id = DeviceId::from_str(&device)?;

            if let Some(device) = host.device_by_id(&id) {
                return Ok(device);
            }
        }

        let dev = if input {
            host.default_input_device()
        } else {
            host.default_output_device()
        };
        dev.ok_or_else(|| anyhow!("Failed to Locate Default Device"))
    }
}
