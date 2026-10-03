//! Action code for the Sample / Test Action

use crate::actions::alert_on_error;
use crate::audio::AudioFile;
use crate::devices::{sinks, sources};
use crate::engine::{PlaybackState, engine};
use anyhow::anyhow;
use chrono::Local;
use openaction::{Action, Instance, InstanceId, OpenActionResult, async_trait};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cmp::PartialEq;

const FADE_OUT_MILLIS: u32 = 500;

#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum PlaybackMode {
    #[default]
    PlayNext,
    PlayStop,
    PlayFade,
    StopOnRelease,
    FadeOnRelease,
    Loop,
}

impl PlaybackMode {
    pub fn badge(&self) -> &'static str {
        match self {
            PlaybackMode::PlayNext => "NEXT",
            PlaybackMode::PlayStop => "PLAY/STOP",
            PlaybackMode::PlayFade => "PLAY/FADE",
            PlaybackMode::StopOnRelease => "HOLD",
            PlaybackMode::FadeOnRelease => "HOLD/FADE",
            PlaybackMode::Loop => "LOOP",
        }
    }
}

#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize)]
pub enum PlayOrder {
    #[default]
    Sequential,
    Random,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// File Path (relative to sample directory)
    pub file: String,

    /// Start and End Percentage Points
    pub start: f64,
    pub end: f64,

    /// Linear gain that brings the file to -23dB LUFS EBU R128
    pub gain: f64,
}

impl Track {
    pub fn new(file: String, gain: f64) -> Self {
        Self {
            file,
            start: 0.0,
            end: 100.0,
            gain,
        }
    }

    fn to_audio_file(&self, fade: Option<u32>) -> AudioFile {
        AudioFile {
            file: self.file.clone().into(),
            fade_on_stop: fade,
            gain: Some(self.gain),
            start_pct: Some(self.start),
            stop_pct: Some(self.end),
        }
    }
}

/// Defines the next action to be performed on press
#[derive(Debug, PartialEq, Eq)]
pub enum PressAction {
    Record,
    Play { looping: bool, fade: bool },
    Stop,
}

/// Defines the next action to be performed on release
#[derive(Debug, PartialEq, Eq)]
pub enum ReleaseAction {
    StopRecording,
    Stop,
    Nothing,
}

pub struct PadAction;

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct PadActionSettings {
    pub name: String,

    pub function: PlaybackMode,
    pub order: PlayOrder,

    pub tracks: Vec<Track>,
    pub cursor: usize,
}

#[async_trait]
impl Action for PadAction {
    const UUID: &'static str = "com.frostycoolslug.sd-sampler.pad";
    type Settings = PadActionSettings;

    async fn key_down(&self, i: &Instance, s: &Self::Settings) -> OpenActionResult<()> {
        let id = i.instance_id.clone();
        let local_engine = engine();

        // Ok, firstly, if clear is armed, we don't do anything (handled on key up)
        if local_engine.clear_armed() {
            return Ok(());
        }

        match self.get_press(&id, s) {
            PressAction::Record => {
                let id = i.instance_id.clone();
                let name = format!("Recording_{}.wav", Local::now().format("%Y-%m-%dT%H%M%S"));

                let result = blocking(move || engine().record(&id, name.into())).await;
                alert_on_error(i, result).await?;
            }
            PressAction::Play { looping, fade } => {
                let result = {
                    let tracks = s.tracks.len();
                    let track = match s.order {
                        PlayOrder::Sequential => {
                            let cursor = if s.cursor >= tracks { 0 } else { s.cursor };

                            // Increment the Cursor
                            let settings = PadActionSettings {
                                cursor: cursor + 1,
                                ..s.clone()
                            };
                            let _ = i.set_settings(&settings);

                            &s.tracks[cursor]
                        }
                        PlayOrder::Random => {
                            let track = fastrand::usize(0..tracks);
                            &s.tracks[track]
                        }
                    };

                    let file = track.to_audio_file(fade.then_some(FADE_OUT_MILLIS));
                    local_engine.play(&id, file, looping)
                };
                alert_on_error(i, result).await?;
            }
            PressAction::Stop => {
                local_engine.stop(&id, false);
            }
        }

        Ok(())
    }

    async fn key_up(&self, i: &Instance, s: &Self::Settings) -> OpenActionResult<()> {
        let id = i.instance_id.clone();
        let engine_local = engine();

        if engine_local.clear_armed() {
            engine_local.set_clear_armed(false);
            engine_local.stop(&id, true);

            let mut cleared = s.clone();
            cleared.tracks.clear();

            return i.set_settings(&cleared).await;
        }

        match self.get_release(&id, s) {
            ReleaseAction::StopRecording => {
                let result = engine_local.stop_recording(&id);
                match alert_on_error(i, result).await? {
                    Some(Some(recorded)) => {
                        let track = Track::new(recorded.file, recorded.gain);

                        let mut updated = s.clone();
                        updated.tracks.push(track);

                        i.set_settings(&updated).await?;
                    }
                    Some(None) => i.show_alert().await?,
                    None => {}
                }
            }
            ReleaseAction::Stop => {
                engine_local.stop(&id, false);
            }
            ReleaseAction::Nothing => {}
        }

        Ok(())
    }

    async fn send_to_plugin(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
        payload: &Value,
    ) -> OpenActionResult<()> {
        let event = payload.get("event").and_then(Value::as_str).unwrap_or("");
        if let Some(items) = datasource(event).await {
            let items = json!({ "event": event, "items": items });
            instance.send_to_property_inspector(items).await?;
        }
        Ok(())
    }
}

impl PadAction {
    pub fn get_press(&self, id: &InstanceId, s: &PadActionSettings) -> PressAction {
        if s.tracks.is_empty() {
            return PressAction::Record;
        }

        let state = engine().get_playback_state(id);
        match s.function {
            PlaybackMode::PlayNext | PlaybackMode::StopOnRelease | PlaybackMode::FadeOnRelease => {
                PressAction::Play {
                    looping: false,
                    fade: s.function == PlaybackMode::FadeOnRelease,
                }
            }
            PlaybackMode::PlayStop | PlaybackMode::PlayFade | PlaybackMode::Loop => {
                if state == PlaybackState::Playing {
                    PressAction::Stop
                } else {
                    PressAction::Play {
                        looping: s.function == PlaybackMode::Loop,
                        fade: s.function == PlaybackMode::PlayFade,
                    }
                }
            }
        }
    }

    pub fn get_release(&self, id: &InstanceId, s: &PadActionSettings) -> ReleaseAction {
        let is_recording = engine().is_recording(id);

        if s.tracks.is_empty() {
            return if is_recording {
                ReleaseAction::StopRecording
            } else {
                ReleaseAction::Nothing
            };
        }

        match s.function {
            PlaybackMode::StopOnRelease | PlaybackMode::FadeOnRelease => ReleaseAction::Stop,
            _ => ReleaseAction::Nothing,
        }
    }
}

async fn datasource(event: &str) -> Option<Value> {
    let items = match event {
        "getSources" => blocking(|| Ok(sources())).await,
        "getSinks" => blocking(|| Ok(sinks())).await,
        _ => return None,
    };
    items.ok()
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| anyhow!("{e}"))?
}
