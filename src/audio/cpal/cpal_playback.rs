use crate::audio::backend::{AudioOutput, AudioSpecification, OpenOutputStream};
use crate::audio::cpal::cpal_config::CpalConfiguration;
use anyhow::{Result, bail};
use cpal::Stream;
use cpal::traits::{DeviceTrait, StreamTrait};
use log::warn;
use rb::{Producer, RB, RbConsumer, RbInspector, RbProducer, SpscRb};
use std::panic::catch_unwind;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

// Create a 50ms Buffer Size for playback, this should be short enough to ensure there aren't
// any obvious delays when playing samples.
const BUFFER_SIZE: usize = 50;

pub(crate) struct CpalPlayback {
    stream: Option<Stream>,
    stream_closed: Arc<AtomicBool>,

    buffer: SpscRb<f32>,
    buffer_producer: Producer<f32>,

    // Resampler Related Variables..
    resampler: Option<CpalResampler>,
}

struct CpalResampler {
    input_buffer: Vec<f32>,
    input: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    interleaved: Vec<f32>,
}

impl OpenOutputStream for CpalPlayback {
    fn open(spec: AudioSpecification) -> Result<Box<dyn AudioOutput>> {
        let device = match catch_unwind(|| CpalConfiguration::get_device(spec.device, false)) {
            Ok(Ok(device)) => device,
            Ok(Err(e)) => bail!("Error Fetching Device: {}", e),
            Err(e) => bail!("PANIC attempting to Fetch Device! {:#?}", e),
        };

        let config = if cfg!(target_os = "windows") {
            // Windows expects the file to be resampled to the output config, so we can't use the
            // input audio. Instead, we gotta resample.
            match device.default_output_config() {
                Ok(output) => output.config(),
                Err(e) => bail!("Error Attempting to Default Stream: {:#?}", e),
            }
        } else {
            // Linux / MacOS will resample internally
            cpal::StreamConfig {
                channels: spec.channels,
                sample_rate: spec.rate,
                buffer_size: cpal::BufferSize::Fixed(64),
            }
        };

        // Before we go any further, is the channel count of the audio correct?
        if spec.channels != 2 {
            bail!("Only stereo audio is supported");
        }

        // Calculate the buffer size based on the sample count in BUFFER_SIZE milliseconds..
        let size = (BUFFER_SIZE * config.sample_rate as usize) / 1000;
        let buffer_length = size * config.channels as usize;

        // Create the Actual Buffer
        let buffer = SpscRb::<f32>::new(buffer_length);
        let buffer_producer = buffer.producer();
        let buffer_consumer = buffer.consumer();

        // Prepare a bool to close the reader if CPAL throws an error..
        let stream_closed = Arc::new(AtomicBool::new(false));
        let stream_closed_inner = stream_closed.clone();

        let stream = device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Read from the ring buffer, and write them to the data array
                let written = buffer_consumer.read(data).unwrap_or(0);

                // Data expects a certain number of samples, if we didn't get enough from above,
                // mute anything afterwards as we're probably EoS
                data[written..].iter_mut().for_each(|s| *s = 0.0);
            },
            move |e| {
                warn!("Error on Playback Stream, Stopping.. {}", e);
                stream_closed_inner.store(true, Ordering::Relaxed);
            },
            Some(Duration::from_millis(500)),
        )?;
        stream.play()?;

        // Do we need to resample?
        let resampler = if spec.rate != config.sample_rate {
            bail!("Resample is Required, and not implemented.");
        } else {
            None
        };

        Ok(Box::new(Self {
            stream: Some(stream),
            stream_closed,

            buffer,
            buffer_producer,

            resampler,
        }))
    }
}

impl AudioOutput for CpalPlayback {
    fn write(&mut self, samples: &[f32]) -> Result<()> {
        if self.stream_closed.load(Ordering::Relaxed) {
            bail!("Stream has been closed");
        }

        // Do nothing if there are no samples sent..
        if samples.is_empty() {
            return Ok(());
        }

        let out_samples = if let Some(_) = &mut self.resampler {
            bail!("Resampler Needed!");
        } else {
            // No resampler needed, send back the samples
            samples
        };

        let mut position = 0;
        while let Some(written) = self
            .buffer_producer
            .write_blocking(out_samples.split_at(position).1)
        {
            position += written;
        }

        Ok(())
    }

    fn flush(&mut self) {
        if let Some(resampler) = &mut self.resampler {
            let length = resampler.input_buffer.len();
            let capacity = resampler.input_buffer.capacity();

            if length != 0 {
                // There's some stuff left in the buffer, fill the buffer to capacity to
                // flush it through to the output buffer.
                let filling_samples = vec![0.; capacity - length];
                let _ = self.write(&filling_samples);
            }
        }

        // Make sure the playback buffer is empty, to prevent premature pausing at
        // the end of playback
        while !self.buffer.is_empty() {
            // Make sure the Stream hasn't closed while handling the buffer..
            if self.stream_closed.load(Ordering::Relaxed) {
                return;
            }

            // Wait briefly for samples to flush
            std::thread::sleep(Duration::from_millis(5));
        }

        if let Some(stream) = &self.stream {
            let _ = stream.pause();
        }
    }

    fn stop(&mut self) {
        // We're going to take the stream and drop it, this should stop playback immediately.
        self.stream_closed.store(true, Ordering::Relaxed);
        self.stream.take();
    }
}
