//! This is the main audio engine manager.
//!
//! It's basically a shared state between the audio, the global settings, and the actions, and
//! is responsible for controlling and managing all playback and recording behaviours.
use crate::audio::player::{Player, PlayerState};
use crate::audio::recorder::{BufferedRecorder, RecorderState};

use anyhow::{Result, bail};

use crate::audio::AudioFile;
use directories::BaseDirs;
use log::warn;
use openaction::InstanceId;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread::JoinHandle;
use std::time::Instant;

struct Source {
    recorder: Arc<BufferedRecorder>,
    holders: HashSet<String>,
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

    sources: Mutex<HashMap<String, Source>>,

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

            sources: Mutex::new(HashMap::new()),
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
        self.playback_device.read().unwrap().clone()
    }

    pub fn set_recording_device(&self, device: Option<String>) {
        if *self.recording_device.read().unwrap() == device {
            return;
        }

        // TODO: Teardown and Recreate Recording Handler
        *self.recording_device.write().unwrap() = device;
    }

    /////////////////////////////////////////////////////////////////////
    // Playback
    /////////////////////////////////////////////////////////////////////

    /// Plays a Sample
    pub fn play(&self, key: &InstanceId, mut audio: AudioFile, looping: bool) -> Result<()> {
        audio.file = self.samples_dir().join(&audio.file);
        if !audio.file.is_file() {
            bail!("Sample not found: {}", audio.file.display());
        }

        if let Some(current) = self.active_playback_file(key) {
            if current == audio.file {
                self.restart(key);
                return Ok(());
            }
            self.stop(key, true);
        }

        let mut player = Player::new(audio, self.playback_device())?;
        let state = player.get_state();

        let error_slot = state.error.clone();
        let handle = std::thread::Builder::new()
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

        self.playing.lock().unwrap().insert(key.clone(), playback);
        Ok(())
    }

    /// Restarts playback of a Sample
    fn restart(&self, key: &InstanceId) {
        if let Some(handle) = self.playing.lock().unwrap().get(key) {
            handle.state.restart_track.store(true, Ordering::Relaxed);
        }
    }

    /// Stops playing back a sample (optional force stop)
    fn stop(&self, key: &InstanceId, force: bool) {
        if let Some(player) = self.playing.lock().unwrap().get(key) {
            let currently_stopping = player.state.stopping.load(Ordering::Relaxed);

            player.state.stopping.store(true, Ordering::Relaxed);
            if force || currently_stopping {
                player.state.force_stop.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Force stop all samples from playing
    fn stop_all(&self) {
        for instance in self.playing.lock().unwrap().keys() {
            self.stop(instance, true);
        }
    }

    /// Returns the Actively playing back file for an instance
    pub fn active_playback_file(&self, key: &InstanceId) -> Option<PathBuf> {
        let map = self.playing.lock().unwrap();
        Some(map.get(key)?.state.playing_file.clone())
    }

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
    }
}
