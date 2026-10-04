//! This is the main audio engine manager.
//!
//! It's basically a shared state between the audio, the global settings, and the actions, and
//! is responsible for controlling and managing all playback and recording behaviours.
use crate::audio::player::{Player, PlayerState};
use crate::audio::recorder::{BufferedRecorder, RecorderState};

use anyhow::{Result, bail};

use crate::audio::AudioFile;
use directories::BaseDirs;
use log::{debug, error, info, warn};
use openaction::InstanceId;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;
use std::thread::JoinHandle;
use std::time::Instant;
use walkdir::WalkDir;
use crate::globals::{GlobalSettings, DEFAULT_DEVICE};

#[derive(Debug, Eq, PartialEq)]
pub enum PlaybackState {
    Playing,
    Stopping,
    Finished,
    None,
}

struct BackgroundRecorder {
    recorder: Arc<BufferedRecorder>,
    handle: JoinHandle<()>,
}

/// An actively occurring Playback
struct ActivePlayback {
    state: PlayerState,
    handle: JoinHandle<()>,
}

/// An actively occurring Recording
struct ActiveRecording {
    path: PathBuf,
    state: RecorderState,
    handle: JoinHandle<Result<()>>,

    // Time this recording was started
    started: Instant,
}

/// A Recorded item, relative to the Samples Directory
pub struct Recorded {
    pub file: String,
    pub gain: f64,
}

#[derive(Default)]
// A cache of the sample peaks used for trimming
struct PeakCache {
    done: HashMap<String, Vec<f32>>,
    pending: HashSet<String>,
}

pub struct Engine {
    pre_buffer_ms: AtomicU32,
    samples_dir: RwLock<PathBuf>,

    playback_device: RwLock<Option<String>>,
    recording_device: RwLock<Option<String>>,

    background_record: Mutex<Option<BackgroundRecorder>>,

    /// Actively Playing Monitor
    playing: Mutex<HashMap<InstanceId, ActivePlayback>>,

    /// Actively Recording Monitor
    recording: Mutex<HashMap<InstanceId, ActiveRecording>>,

    cursors: Mutex<HashMap<InstanceId, usize>>,
    peaks: Arc<Mutex<PeakCache>>,

    /// Whether the 'Clear' action is active
    clear_armed: AtomicBool,

    /// The last instance that was pressed
    last_pressed: Mutex<Option<InstanceId>>,
}

static ENGINE: OnceLock<Arc<Engine>> = OnceLock::new();
pub fn engine() -> Arc<Engine> {
    ENGINE.get_or_init(|| Arc::new(Engine::new())).clone()
}

fn default_samples_dir() -> PathBuf {
    let dir = BaseDirs::new().expect("Unable to Load Base Directories");
    let data_dir = dir.data_dir();

    data_dir.join("sd-sampler")
}

impl Engine {
    fn new() -> Self {
        Self {
            pre_buffer_ms: AtomicU32::new(0),
            samples_dir: RwLock::new(default_samples_dir()),
            playback_device: RwLock::new(None),
            recording_device: RwLock::new(None),

            background_record: Mutex::new(None),

            playing: Mutex::new(HashMap::new()),
            recording: Mutex::new(HashMap::new()),
            cursors: Mutex::new(HashMap::new()),
            peaks: Arc::new(Mutex::new(PeakCache::default())),
            clear_armed: AtomicBool::new(false),
            last_pressed: Mutex::new(None),
        }
    }

    /////////////////////////////////////////////////////////////////////
    // Configuration
    /////////////////////////////////////////////////////////////////////

    pub fn pre_buffer_ms(&self) -> u32 {
        self.pre_buffer_ms.load(Ordering::Relaxed)
    }

    pub fn set_pre_buffer_ms(&self, ms: u32) {
        if self.pre_buffer_ms.swap(ms, Ordering::Relaxed) == ms {
            return;
        }

        // We need to teardown and Restart the Recording Thread
        self.stop_recorder();
        self.spawn_recorder();
    }

    /// Get the Samples Directory
    pub fn samples_dir(&self) -> PathBuf {
        self.samples_dir.read().unwrap().clone()
    }

    pub fn set_samples_dir(&self, dir: PathBuf) {
        *self.samples_dir.write().unwrap() = dir;
    }

    pub fn playback_device(&self) -> Option<String> {
        self.playback_device.read().unwrap().clone()
    }

    pub fn set_playback_device(&self, device: Option<String>) {
        if *self.playback_device.read().unwrap() == device {
            return;
        }

        // Ok, we need to stop all current playback, and update the device
        self.stop_all();
        *self.playback_device.write().unwrap() = device;
    }

    pub fn recording_device(&self) -> Option<String> {
        self.recording_device.read().unwrap().clone()
    }

    pub fn set_recording_device(&self, device: Option<String>) {
        let is_running = self.background_record.lock().unwrap().is_some();
        if *self.recording_device.read().unwrap() == device && is_running {
            return;
        }

        self.stop_recorder();
        *self.recording_device.write().unwrap() = device;
        self.spawn_recorder();
    }

    /////////////////////////////////////////////////////////////////////
    // Playback
    /////////////////////////////////////////////////////////////////////

    /// Plays a Sample
    pub fn play(&self, instance: &InstanceId, mut audio: AudioFile, looping: bool) -> Result<()> {
        audio.file = self.samples_dir().join(&audio.file);
        if !audio.file.is_file() {
            bail!("Sample not found: {}", audio.file.display());
        }

        if let Some(current) = self.active_playback_file(instance) {
            if current == audio.file {
                self.restart(instance);
                return Ok(());
            }
            self.stop(instance, true);
        }

        let mut player = Player::new(audio, self.playback_device())?;
        let state = player.get_state();

        let error_slot = state.error.clone();
        let handle = thread::Builder::new()
            .name("sampler-play".into())
            .spawn(move || {
                let result = if looping {
                    player.play_loop()
                } else {
                    player.play()
                };
                if let Err(e) = result {
                    *error_slot.lock().unwrap() = Some(e.to_string());
                }
            })?;

        let playback = ActivePlayback { state, handle };

        let key = instance.clone();
        self.playing.lock().unwrap().insert(key, playback);
        Ok(())
    }

    /// Restarts playback of a Sample
    fn restart(&self, key: &InstanceId) {
        if let Some(handle) = self.playing.lock().unwrap().get(key) {
            handle.state.restart_track.store(true, Ordering::Relaxed);
        }
    }

    /// Stops playing back a sample (optional force stop)
    pub fn stop(&self, instance: &InstanceId, force: bool) {
        if let Some(player) = self.playing.lock().unwrap().get(instance) {
            let currently_stopping = player.state.stop.load(Ordering::Relaxed);

            player.state.stop.store(true, Ordering::Relaxed);
            if force || currently_stopping {
                player.state.force_stop.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Force stop all samples from playing
    fn stop_all(&self) {
        let instances: Vec<InstanceId> = self.playing.lock().unwrap().keys().cloned().collect();
        for instance in instances {
            self.stop(&instance, true);
        }
    }

    /// Returns the Actively playing back file for an instance
    pub fn active_playback_file(&self, key: &InstanceId) -> Option<PathBuf> {
        let map = self.playing.lock().unwrap();
        Some(map.get(key)?.state.playing_file.clone())
    }

    pub fn get_playback_state(&self, key: &InstanceId) -> PlaybackState {
        let map = self.playing.lock().unwrap();
        if let Some(player) = map.get(key) {
            if player.handle.is_finished() {
                return PlaybackState::Finished;
            }

            if player.state.stop.load(Ordering::Relaxed) {
                return PlaybackState::Stopping;
            }

            return PlaybackState::Playing;
        }

        PlaybackState::None
    }

    /////////////////////////////////////////////////////////////////////
    // Recording Background
    /////////////////////////////////////////////////////////////////////

    pub fn spawn_recorder(&self) {
        info!("Starting Background Recorder..");

        // Should be simple, we take the information we have, spawn up our thread, and let it run
        let device = self.recording_device();
        debug!("Device: {:?}", device);

        let buffer = self.pre_buffer_ms() as usize;

        let Ok(recorder) = BufferedRecorder::new(device, buffer) else {
            warn!("Failed to spawn BufferedRecorder");
            return;
        };

        let arc_recorder = Arc::new(recorder);
        let inner_recorder = arc_recorder.clone();

        debug!("Spawning Background Recording Thread..");
        let handle = thread::spawn(move || inner_recorder.listen());
        let background_recoder = BackgroundRecorder {
            recorder: arc_recorder,
            handle,
        };

        info!("Background Recorder Running..");
        *self.background_record.lock().unwrap() = Some(background_recoder);
    }

    pub fn stop_recorder(&self) {
        info!("Stopping Background Recorder");
        if let Some(bg) = self.background_record.lock().unwrap().take() {
            info!("Background Recorder Found, Triggering Stop..");
            bg.recorder.as_ref().stop();

            info!("Waiting for Background Recorder Stop to Complete..");
            let _ = bg.handle.join();

            info!("Background Recorder Stopped.");
        }

        self.stop_all_recordings();
    }

    /////////////////////////////////////////////////////////////////////
    // Recording Tasks
    /////////////////////////////////////////////////////////////////////

    pub fn record(&self, instance: &InstanceId, path: PathBuf) -> Result<()> {
        if let Some(recorder) = &*self.background_record.lock().unwrap() {
            let handler_inner = recorder.recorder.clone();

            let state = handler_inner.new_state();
            let state_inner = state.clone();

            let path = self.samples_dir().join(path);
            let path_inner = path.clone();

            let handle = thread::spawn(move || handler_inner.record(&path_inner, state_inner));
            let recording = ActiveRecording {
                path,
                state,
                handle,
                started: Instant::now(),
            };

            let key = instance.clone();
            self.recording.lock().unwrap().insert(key, recording);

            return Ok(());
        }
        bail!("Recorder Not Available");
    }

    pub fn stop_recording(&self, instance: &InstanceId) -> Result<Option<Recorded>> {
        let recording = match self.recording.lock().unwrap().remove(instance) {
            Some(r) => r,
            None => return Ok(None),
        };

        recording.state.stop.store(true, Ordering::Relaxed);
        let outcome = recording.handle.join();

        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                error!("Recording Failed: {e}");
                return Ok(None);
            }
            Err(_) => {
                error!("Recording thread panicked");
                return Ok(None);
            }
        }

        if !recording.path.exists() {
            return Ok(None);
        }

        let dir = self.samples_dir();
        let file = recording
            .path
            .strip_prefix(&dir)
            .unwrap_or(&recording.path)
            .to_string_lossy()
            .to_string();

        Ok(Some(Recorded {
            file,
            gain: recording.state.gain.load(Ordering::Relaxed),
        }))
    }

    pub fn is_recording(&self, instance: &InstanceId) -> bool {
        self.recording.lock().unwrap().contains_key(instance)
    }

    /// Force stop all samples from playing
    fn stop_all_recordings(&self) {
        let instances: Vec<InstanceId> = self.recording.lock().unwrap().keys().cloned().collect();
        for instance in instances {
            self.stop_recording(&instance);
        }
    }

    /////////////////////////////////////////////////////////////////////
    // Other Settings
    /////////////////////////////////////////////////////////////////////
    pub(crate) fn clear_armed(&self) -> bool {
        self.clear_armed.load(Ordering::Relaxed)
    }

    pub(crate) fn set_clear_armed(&self, armed: bool) {
        self.clear_armed.store(armed, Ordering::Relaxed)
    }

    pub(crate) fn to_global_settings(&self) -> GlobalSettings {
        GlobalSettings {
            pre_buffer_ms: self.pre_buffer_ms(),
            samples_directory: self.samples_dir().to_string_lossy().to_string(),
            capture_device: self.recording_device().unwrap_or(DEFAULT_DEVICE.into()),
            playback_device: self.playback_device().unwrap_or(DEFAULT_DEVICE.into()),
        }
    }

    /////////////////////////////////////////////////////////////////////
    // Samples
    /////////////////////////////////////////////////////////////////////
    pub fn samples(&self) -> Vec<String> {
        let dir = self.samples_dir();

        let mut out: Vec<String> = WalkDir::new(&dir)
            .max_depth(4)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                matches!(
                    e.path().extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()).as_deref(),
                    Some("wav") | Some("ogg") | Some("flac") | Some("mp3")
                )
            })
            .filter_map(|e| e.path().strip_prefix(&dir).ok().map(|p| p.to_string_lossy().into_owned()))
            .collect();

        out.sort();
        out
    }


    ////
    /// Used for Cleaning Up anything that may have completed
    pub fn cleanup(&self) {
        let mut map = self.playing.lock().unwrap();
        map.retain(|_, player| {
            if player.handle.is_finished() {
                // Lets check the error slot, and see if we errored out.
                if let Some(e) = player.state.error.lock().unwrap().take() {
                    let file = player.state.playing_file.display();
                    warn!("Playback of {file} ended with error: {e}");
                }

                // Finished Regardless, time to clean.
                return false;
            }
            true
        });

        let mut map = self.recording.lock().unwrap();
        map.retain(|_, recorder| {
            if recorder.handle.is_finished() {
                return false;
            }
            true
        });

        if let Some(check) = &*self.background_record.lock().unwrap() {
            if check.handle.is_finished() {
                warn!("Background Recorder Stopped Unexpectedly!");
            }
        }
    }
}
