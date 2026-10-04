use crate::audio::player::Player;
use anyhow::{Result, bail};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

pub mod backend;
pub mod player;
pub mod recorder;
pub mod ringbuffer;

mod cpal;

#[derive(Debug, Clone)]
pub struct AudioFile {
    pub file: PathBuf,
    pub fade_on_stop: Option<u32>,
    pub gain: Option<f64>,
    pub start_pct: Option<f64>,
    pub stop_pct: Option<f64>,
}

// Simple function that performs gain calculation
pub fn normalised_gain(path: PathBuf) -> Result<f64> {
    let file = AudioFile {
        file: path,
        fade_on_stop: None,
        gain: None,
        start_pct: None,
        stop_pct: None,
    };

    let mut player = Player::new(file, None)?;
    let state = player.get_state();

    player.calculate_gain();

    if let Some(e) = state.error.lock().unwrap().take() {
        bail!(e);
    }

    Ok(state.calculated_gain.load(Ordering::Relaxed))
}
