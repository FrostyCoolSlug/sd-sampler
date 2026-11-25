use anyhow::{Result, anyhow};
use cpal::traits::HostTrait;
use cpal::{Device, DeviceId};
use log::info;
use std::str::FromStr;
use std::sync::OnceLock;

/// A process wide cpal Host, so we're not establishing new connections for every
/// lookup.
static HOST: OnceLock<cpal::Host> = OnceLock::new();

/// The shared audio host, created lazily on first use.
pub(crate) fn host() -> &'static cpal::Host {
    HOST.get_or_init(cpal::default_host)
}

pub struct CpalConfiguration {}

impl CpalConfiguration {
    pub(crate) fn get_device(device: Option<String>, input: bool) -> Result<Device> {
        let host = host();

        if let Some(device) = device {
            let id = DeviceId::from_str(&device)?;
            if let Some(device) = host.device_by_id(&id) {
                return Ok(device);
            }
        }

        info!("Finding Default Device..");
        let dev = if input {
            host.default_input_device()
        } else {
            host.default_output_device()
        };
        dev.ok_or_else(|| anyhow!("Failed to Locate Default Device"))
    }
}
