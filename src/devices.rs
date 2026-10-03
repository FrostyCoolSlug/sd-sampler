//! Ok, we need to fetch all the devices, use cpal for this..

use anyhow::{Result, anyhow};
use cpal::Device;
use cpal::traits::{DeviceTrait, HostTrait};
use log::debug;
use serde_json::{Value, json};

#[derive(Debug)]
struct DisplayDevice {
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

    let mut result = vec![
        json!({ "value": "@DEFAULT_SOURCE@", "label": "System default input" }),
        json!({ "label": "Inputs", "children": inputs }),
    ];

    if !monitors.is_empty() {
        result.push(json!({ "label": "Output monitors", "children": monitors }));
    }

    json!(result)
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

fn item(d: &DisplayDevice) -> Value {
    json!({ "value": d.name, "label": d.label })
}

fn sinks_list() -> Result<Vec<DisplayDevice>> {
    let mut list: Vec<DisplayDevice> = vec![];

    let host = cpal::default_host();
    for device in host.output_devices()? {
        list.push(get_device(device)?);
    }
    Ok(list)
}

fn sources_list() -> Result<Vec<DisplayDevice>> {
    let mut list: Vec<DisplayDevice> = vec![];

    let host = cpal::default_host();
    for device in host.input_devices()? {
        list.push(get_device(device)?);
    }
    Ok(list)
}

fn get_device(device: Device) -> Result<DisplayDevice> {
    let name = device.id()?.to_string();
    let label = device.description()?.name().to_string();
    let is_monitor = name.clone().ends_with(".monitor");

    Ok(DisplayDevice {
        name,
        label,
        is_monitor,
    })
}
