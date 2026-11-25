//! This is lifted from the GoXLR Utility (audio.rs) with a couple of minor tweaks.

use anyhow::Result;

pub struct AudioSpecification {
    pub device: Option<String>,

    pub rate: u32,
    pub channels: usize,

    /// Packet size in frames (used to size the playback buffer).
    pub buffer: usize,
}

pub trait OpenOutputStream {
    fn open(spec: AudioSpecification) -> Result<Box<dyn AudioOutput>>;
}

pub trait OpenInputStream {
    fn open(spec: AudioSpecification) -> Result<Box<dyn AudioInput>>;
}

pub trait AudioOutput {
    fn write(&mut self, samples: &[f32]) -> Result<()>;
    fn flush(&mut self);
    fn stop(&mut self);
}

pub trait AudioInput {
    fn read(&mut self) -> Result<Vec<f32>>;
    fn flush(&mut self);
}

pub(crate) fn get_output(spec: AudioSpecification) -> Result<Box<dyn AudioOutput>> {
    use crate::audio::cpal::cpal_playback::CpalPlayback;
    CpalPlayback::open(spec)
}

pub(crate) fn get_input(spec: AudioSpecification) -> Result<Box<dyn AudioInput>> {
    use crate::audio::cpal::cpal_record::CpalRecord;
    CpalRecord::open(spec)
}
