use std::path::PathBuf;

pub mod player;
pub mod backend;
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