//! Plugin-wide settings
//!
//! Honestly, there are only two of them, sample pre-buffer and sample location, and that second
//! one is questionable :D

use crate::engine::engine;
use directories::BaseDirs;
use log::{debug, info};
use openaction::global_events::{DidReceiveGlobalSettingsEvent, GlobalEventHandler};
use openaction::{OpenActionResult, async_trait, get_global_settings, set_global_settings};
use serde::{Deserialize, Serialize};
use std::fs::create_dir_all;
use std::path::PathBuf;

// There are gonna be a few of these, just helps prevent unneeded wrapping :p
type Event = DidReceiveGlobalSettingsEvent;

pub const DEFAULT_DEVICE: &str = "@DEFAULT_DEVICE@";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalSettings {
    pub pre_buffer_ms: u32,
    pub samples_directory: String,

    pub capture_device: String,
    pub playback_device: String,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            pre_buffer_ms: 0,
            samples_directory: "".to_string(),
            capture_device: DEFAULT_DEVICE.to_string(),
            playback_device: DEFAULT_DEVICE.to_string(),
        }
    }
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

        let settings = if settings.samples_directory.is_empty() {
            info!("Samples Directory not set, Setting Default");

            let dir = BaseDirs::new().expect("Unable to Load Base Directories");
            let data_dir = dir.data_dir();

            let sample_path = data_dir.join("sd-sampler");
            create_dir_all(&sample_path).expect("Failed to Create Directory");

            let mut update = settings.clone();
            update.samples_directory = sample_path.clone().into_string().expect("Failed");

            debug!("Saving New Path: {:?}", sample_path);
            set_global_settings(serde_json::to_value(&update)?).await?;

            update
        } else {
            settings
        };

        // Ok, we need to start or fetch the audio engine at this point
        let audio_engine = engine();

        // Then set the Sample Path
        let sample_path = PathBuf::from(settings.samples_directory);
        if sample_path.exists() {
            audio_engine.set_samples_dir(sample_path);
        }

        audio_engine.set_pre_buffer_ms(settings.pre_buffer_ms);

        let playback = settings.playback_device;
        let playback = (playback != DEFAULT_DEVICE).then_some(playback);
        audio_engine.set_playback_device(playback);

        let capture = settings.capture_device;
        let capture = (capture != DEFAULT_DEVICE).then_some(capture);
        audio_engine.set_recording_device(capture);

        // Drop any waveform cache entries whose samples have gone away or changed since last run.
        audio_engine.wave_cache().cleanup();

        Ok(())
    }
}
