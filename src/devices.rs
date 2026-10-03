//! Ok, we need to fetch all the devices from pulseaudio, we can use pulsectl for this.
//!
//! I wanted to use cpal for this entire thing, but last time I tried pulseaudio wasn't properly
//! supported, so only listed ALSA devices, and no nodes (such as virtual ones) created by Pulse.
//! 
//! This *MAY* have changed, but for now I'll stick with what I know.

use anyhow::{Result, anyhow};
use pulsectl::controllers::{DeviceControl, SinkController, SourceController};
use pulsectl::controllers::types::DeviceInfo;
use serde_json::{Value, json};

struct Device {
    name: String,
    label: String,
    is_monitor: bool,
}

/// Capture Sources
pub fn sources() -> Value {
    let list = sources_list().unwrap_or_else(|e| {
        log::warn!("Unable to list sources: {e}");
        vec![]
    });

    let inputs: Vec<Value> = list.iter().filter(|d| !d.is_monitor).map(item).collect();
    let monitors: Vec<Value> = list.iter().filter(|d| d.is_monitor).map(item).collect();
    json!([
        { "value": "@DEFAULT_SOURCE@", "label": "System default input" },
        { "value": "@DEFAULT_MONITOR@", "label": "System default output (what you hear)" },
        { "label": "Inputs", "children": inputs },
        { "label": "Output monitors", "children": monitors },
    ])
}

/// Playback Sinks
pub fn sinks() -> Value {
    let list = sinks_list().unwrap_or_else(|e| {
        log::warn!("Unable to list sinks: {e}");
        vec![]
    });

    let outputs: Vec<Value> = list.iter().map(item).collect();
    json!([
        { "value": "@DEFAULT_SINK@", "label": "System default output" },
        { "label": "Outputs", "children": outputs },
    ])
}

fn sources_list() -> Result<Vec<Device>> {
    let mut controller = SourceController::create().map_err(|e| anyhow!("{e:?}"))?;
    let devices = controller.list_devices().map_err(|e| anyhow!("{e:?}"))?;

    get_devices(devices)
}

fn sinks_list() -> Result<Vec<Device>> {
    let mut controller = SinkController::create().map_err(|e| anyhow!("{e:?}"))?;
    let devices = controller.list_devices().map_err(|e| anyhow!("{e:?}"))?;

    get_devices(devices)
}

fn get_devices(devices: Vec<DeviceInfo>) -> Result<Vec<Device>>  {
    Ok(devices
        .into_iter()
        .filter_map(|device| {
            let name = device.name?;
            let label = device.description.unwrap_or_else(|| name.clone());

            let device = Device {
                name,
                label,
                is_monitor: device.monitor.is_some(),
            };

            Some(device)
        })
        .collect())
}

fn item(d: &Device) -> Value {
    json!({ "value": d.name, "label": d.label })
}