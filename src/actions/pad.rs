//! Action code for the Sample / Test Action
use crate::actions::alert_on_error;
use crate::audio::{AudioFile, normalised_gain};
use crate::devices::{sinks, sources};
use crate::engine::{PlaybackState, engine};
use crate::globals::GLOBALS;
use crate::svg;
use crate::svg::{PadState, PadView};
use anyhow::anyhow;
use chrono::Local;
use log::warn;
use openaction::{
    Action, Instance, InstanceId, OpenActionResult, async_trait, get_instance, set_global_settings,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

const FADE_OUT_MILLIS: u32 = 500;

// This is a cache of pads whose "add file" request is being processed, we don't want to store
// this in the settings (would break on restart), but we also want to make sure that the gain
// calculation process doesn't run twice simultaneously.
static ADDING: LazyLock<Mutex<HashSet<InstanceId>>> = LazyLock::new(Default::default);

// This allows us to pull the 'current' settings for an instance at any point, because certain
// tasks may thread off, there's an opportunity for settings to change whilst that's happening;
// this store makes sure those threads can get the 'current' settings when they complete.
type SettingsLock = LazyLock<Mutex<HashMap<InstanceId, PadActionSettings>>>;
static SETTINGS: SettingsLock = LazyLock::new(Default::default);

fn remember(instance_id: &InstanceId, settings: &PadActionSettings) {
    let id = instance_id.clone();
    SETTINGS.lock().unwrap().insert(id, settings.clone());
}

fn recall(instance_id: &InstanceId) -> Option<PadActionSettings> {
    SETTINGS.lock().unwrap().get(instance_id).cloned()
}

fn forget(instance_id: &InstanceId) {
    SETTINGS.lock().unwrap().remove(instance_id);
}

async fn save(i: &Instance, settings: &PadActionSettings) -> OpenActionResult<()> {
    remember(&i.instance_id, settings);
    i.set_settings(settings).await
}

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

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PadActionSettings {
    pub name: String,

    pub function: PlaybackMode,
    pub order: PlayOrder,

    pub tracks: Vec<Track>,

    #[serde(default)]
    pub cursor: usize,

    // A File Being Added
    pub add_file: String,
}

#[async_trait]
impl Action for PadAction {
    const UUID: &'static str = "com.frostycoolslug.sd-sampler.pad";
    type Settings = PadActionSettings;

    async fn will_appear(&self, i: &Instance, s: &Self::Settings) -> OpenActionResult<()> {
        self.did_receive_settings(i, s).await?;
        draw_pad(i, s, false).await
    }

    async fn will_disappear(&self, i: &Instance, _s: &Self::Settings) -> OpenActionResult<()> {
        forget(&i.instance_id);
        Ok(())
    }

    async fn property_inspector_did_appear(
        &self,
        i: &Instance,
        _s: &Self::Settings,
    ) -> OpenActionResult<()> {
        // Realistically, this will always be Some as the initial population happens during
        // plugin load. Not gonna straight unwrap() it though, that would be insane :D
        if let Some(settings) = GLOBALS.current() {
            let _ = i
                .send_to_property_inspector(&json!({
                    "event": "globalSettings",
                    "settings": settings,
                }))
                .await;
        }
        Ok(())
    }

    async fn key_down(&self, i: &Instance, s: &Self::Settings) -> OpenActionResult<()> {
        let id = i.instance_id.clone();
        let local_engine = engine();

        // Ok, firstly, if clear is armed, we don't do anything (handled on key up)
        if local_engine.clear_armed() {
            return Ok(());
        }

        // If add mode is armed, hold to record a new sample onto this pad
        if local_engine.add_armed() {
            // Disarm the append / add behaviour on a new pad
            if s.tracks.is_empty() {
                local_engine.set_add_armed(false);
            }

            let name = format!("Recording_{}.wav", Local::now().format("%Y-%m-%dT%H%M%S"));
            let result = local_engine.record(&id, name.into());
            alert_on_error(i, result).await?;
            return Ok(());
        }

        match self.get_press(&id, s) {
            PressAction::Record => {
                let id = i.instance_id.clone();
                let name = format!("Recording_{}.wav", Local::now().format("%Y-%m-%dT%H%M%S"));
                let result = engine().record(&id, name.into());
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
                            let _ = save(i, &settings).await?;

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

            return save(i, &cleared).await;
        }

        if engine_local.add_armed() {
            let was_recording = engine_local.is_recording(&id);
            engine_local.set_add_armed(false);

            // If we were recording, stop and append the new sample to the end of the stack
            if was_recording {
                let result = engine_local.stop_recording(&id);
                match alert_on_error(i, result).await? {
                    Some(Some(recorded)) => {
                        let track = Track::new(recorded.file, recorded.gain);

                        let mut updated = s.clone();
                        updated.tracks.push(track);

                        save(i, &updated).await?;
                    }
                    Some(None) => i.show_alert().await?,
                    None => {}
                }
            }

            return Ok(());
        }

        // If a pad with samples is still recording (e.g. add mode was disarmed while it was
        // held), discard the in-progress recording rather than letting it run on
        if !s.tracks.is_empty() && engine_local.is_recording(&id) {
            let _ = engine_local.stop_recording(&id);
        }

        match self.get_release(&id, s) {
            ReleaseAction::StopRecording => {
                let result = engine_local.stop_recording(&id);
                match alert_on_error(i, result).await? {
                    Some(Some(recorded)) => {
                        let track = Track::new(recorded.file, recorded.gain);

                        let mut updated = s.clone();
                        updated.tracks.push(track);

                        save(i, &updated).await?;
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

    async fn did_receive_settings(&self, i: &Instance, s: &Self::Settings) -> OpenActionResult<()> {
        remember(&i.instance_id, s);

        let id = i.instance_id.clone();
        let file = s.add_file.trim().to_string();

        if file.is_empty() || !ADDING.lock().unwrap().insert(id.clone()) {
            return Ok(());
        }

        tokio::spawn(async move {
            let sample = engine().samples_dir().join(file.clone());
            let cache = engine().wave_cache().clone();
            let gain = blocking(move || match cache.gain(&sample) {
                Some(gain) => Ok(gain),
                None => {
                    let gain = normalised_gain(sample.to_path_buf())?;
                    cache.store_gain(&sample, gain);
                    Ok(gain)
                }
            })
            .await;
            ADDING.lock().unwrap().remove(&id);

            let Some(instance) = get_instance(id.clone()).await else {
                return;
            };

            let Some(mut settings) = recall(&id) else {
                return;
            };

            settings.add_file.clear();
            match gain {
                Ok(gain) => settings.tracks.push(Track::new(file, gain)),
                Err(e) => {
                    warn!("Failed to Add {file}: {e:#}");
                    let _ = instance.show_alert().await;
                }
            }
            let _ = save(&instance, &settings).await;
        });
        Ok(())
    }

    async fn send_to_plugin(
        &self,
        instance: &Instance,
        settings: &Self::Settings,
        payload: &Value,
    ) -> OpenActionResult<()> {
        let event = payload.get("event").and_then(Value::as_str).unwrap_or("");
        let items = if event == "getPeaks" {
            peaks_for_tracks(&settings.tracks).await.ok()
        } else {
            datasource(event).await
        };

        if let Some(items) = items {
            let items = json!({ "event": event, "items": items });
            instance.send_to_property_inspector(items).await?;
        }
        Ok(())
    }
}

/// Peaks for every track on this pad, served from the disk-backed cache (computing on a miss)
/// so the property inspector can draw trim waveforms without them living in the settings.
async fn peaks_for_tracks(tracks: &[Track]) -> anyhow::Result<Value> {
    let dir = engine().samples_dir();
    let cache = engine().wave_cache().clone();
    let files: Vec<String> = tracks.iter().map(|t| t.file.clone()).collect();

    blocking(move || {
        let mut seen = HashSet::new();
        let mut items = Vec::new();
        for file in files {
            if !seen.insert(file.clone()) {
                continue;
            }
            if let Ok(peaks) = cache.compute_or_get(&dir.join(&file)) {
                items.push(json!({ "file": file, "peaks": peaks }));
            }
        }
        Ok(Value::Array(items))
    })
    .await
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
        "getSamples" => {
            blocking(|| {
                let files = engine().samples();
                let map = |f| json!({"value": f, "label": f});
                Ok(Value::Array(files.into_iter().map(map).collect()))
            })
            .await
        }
        "selectSampleDir" => {
            let file_dialog = rfd::FileDialog::new();
            if let Some(dir) = file_dialog.pick_folder() {
                let dir = dir.to_string_lossy().to_string();

                let mut settings = engine().to_global_settings();
                settings.samples_directory = dir;

                let _ = set_global_settings(settings).await;
            }
            return None;
        }
        "openSampleDir" => {
            let _ = open::that_detached(engine().samples_dir());
            return None;
        }
        _ => return None,
    };
    items.ok()
}

/// Audio files in the samples directory.
pub fn samples(files: Vec<String>) -> Value {
    Value::Array(
        files
            .into_iter()
            .map(|f| json!({ "value": f, "label": f }))
            .collect(),
    )
}

/// A Function to push blocking tasks off our current thread
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| anyhow!("{e}"))?
}

/// A function which updates the display icon on pads
pub async fn draw_all_pads(blink: bool) {
    // We can use Settings to grab the currently visible pads
    let pads: Vec<InstanceId> = SETTINGS.lock().unwrap().keys().cloned().collect();
    for instance_id in pads {
        let Some(instance) = get_instance(instance_id.clone()).await else {
            continue;
        };
        let Some(settings) = recall(&instance_id) else {
            continue;
        };

        // If we can't draw a pad, we can just fail quietly
        let _ = draw_pad(&instance, &settings, blink).await;
    }
}

async fn draw_pad(instance: &Instance, s: &PadActionSettings, blink: bool) -> OpenActionResult<()> {
    // We need to generate our SVG from our state and settings.. First, state.
    let id = &instance.instance_id;

    let engine_local = engine();
    let recording = engine_local.recording_elapsed(id);
    let playing = engine_local.elapsed(id);

    let state = if recording.is_some() {
        PadState::Recording
    } else if playing.is_some() {
        PadState::Playing
    } else if s.tracks.is_empty() {
        PadState::Empty
    } else if engine_local.clear_armed() {
        PadState::ClearArmed
    } else if engine_local.add_armed() {
        PadState::RecordArmed
    } else {
        PadState::Idle
    };

    // For the waveform, we show either the active file, or the first file.
    let base = engine_local.samples_dir();
    let current = engine_local.active_playback_file(id);
    let shown = current
        .as_ref()
        .and_then(|p| s.tracks.iter().find(|t| base.join(t.file.clone()) == *p))
        .or_else(|| s.tracks.first());

    let (trim, wave) = match shown {
        Some(t) => ((t.start, t.end), engine_local.waveform(&t.file)),
        None => ((0.0, 100.0), None),
    };

    let view = PadView {
        state,
        name: s.name.clone(),
        badge: s.function.badge().to_string(),
        count: s.tracks.len(),
        wave,
        trim,
        progress: engine_local.elapsed(id),
        secs: recording.map(|d| d.as_secs_f64()).unwrap_or(0.0),
        blink,
    };

    // Grab the SVG Data
    let svg = svg::data_url(&svg::pad(&view));
    instance.set_image(Some(svg), None).await
}
