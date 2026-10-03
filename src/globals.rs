//! Plugin-wide settings
//!
//! Honestly, there are only two of them, sample pre-buffer and sample location, and that second
//! one is questionable :D

use log::info;
use openaction::global_events::{DidReceiveGlobalSettingsEvent, GlobalEventHandler};
use openaction::{OpenActionResult, async_trait, get_global_settings};
use serde::{Deserialize, Serialize};

// There are gonna be a few of these, just helps prevent unneeded wrapping :p
type Event = DidReceiveGlobalSettingsEvent;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalSettings {
    pub pre_buffer_ms: u32,
    pub samples_directory: String,
}

pub struct Globals;
pub static GLOBALS: Globals = Globals;

#[async_trait]
impl GlobalEventHandler for Globals {
    async fn plugin_ready(&self) -> OpenActionResult<()> {
        get_global_settings().await
    }

    async fn did_receive_global_settings(&self, e: Event) -> OpenActionResult<()> {
        let settings = e.payload.settings;
        let settings: GlobalSettings = serde_json::from_value(settings).unwrap_or_default();

        info!("Loaded Settings: {:#?}", settings);

        // Ok, we need to start or fetch the audio engine at this point

        // Then set the Sample Path

        // Then set the prebuffer

        Ok(())
    }
}
