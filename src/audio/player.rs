//! This is lifted pretty much verbatim from the GoXLR Utility... If it ain't broke...

use crate::audio::AudioFile;
use crate::audio::backend::{AudioSpecification, get_output};
use anyhow::bail;
use anyhow::{Result, anyhow};
use atomic_float::AtomicF64;
use ebur128::{EbuR128, Mode};
use log::debug;
use std::fs::File;
use std::io;
use std::io::ErrorKind::UnexpectedEof;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo, SeekedTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::units::Timestamp;
use symphonia::default::get_codecs;

pub struct Player {
    file: PathBuf,
    format: Box<dyn FormatReader>,

    volume: f32,

    // I should really introduce messaging for this..
    stopping: Arc<AtomicBool>,
    force_stop: Arc<AtomicBool>,
    restart_track: Arc<AtomicBool>,

    device: Option<String>,
    fade_duration: Option<u32>,
    start_pct: Option<f64>,
    stop_pct: Option<f64>,
    gain: Option<f64>,

    progress: Arc<AtomicU8>,
    error: Arc<Mutex<Option<String>>>,

    // Used for processing Gain..
    process_only: bool,
    normalized_gain: Arc<AtomicF64>,
}

impl Player {
    pub fn new(audio: AudioFile, device: Option<String>) -> Result<Self> {
        let probe_result = Player::load_file(&audio.file);
        if probe_result.is_err() {
            return Err(anyhow!("Unable to Probe Audio File"));
        }

        Ok(Self {
            file: audio.file.clone(),

            format: probe_result?,
            volume: 1.0_f32,
            stopping: Arc::new(AtomicBool::new(false)),
            force_stop: Arc::new(AtomicBool::new(false)),
            restart_track: Arc::new(AtomicBool::new(false)),

            progress: Arc::new(AtomicU8::new(0)),
            error: Arc::new(Mutex::new(None)),

            device,
            fade_duration: audio.fade_on_stop,
            start_pct: audio.start_pct,
            stop_pct: audio.stop_pct,
            gain: audio.gain,

            process_only: false,
            normalized_gain: Arc::new(AtomicF64::new(1.0)),
        })
    }

    fn load_file(file: &PathBuf) -> symphonia::core::errors::Result<Box<dyn FormatReader>> {
        // Use the file extension to get a type hint..
        let mut hint = Hint::new();
        if let Some(extension) = file.extension()
            && let Some(extension_str) = extension.to_str()
        {
            hint.with_extension(extension_str);
        }

        let media_source = Box::new(File::open(file).map_err(Error::IoError)?);
        let stream = MediaSourceStream::new(media_source, Default::default());

        let format_options = Default::default();
        let metadata_options = Default::default();

        symphonia::default::get_probe().probe(&hint, stream, format_options, metadata_options)
    }

    /// Builds a seek target for a frame position within the given track.
    fn seek_to(frame: u64, track_id: u32) -> SeekTo {
        SeekTo::Timestamp {
            ts: Timestamp::new(frame as i64),
            track_id,
        }
    }

    /// Converts the position a seek landed on into a (channel interleaved) sample count.
    fn seeked_samples(seeked_to: SeekedTo, channels: usize) -> u64 {
        // The timestamp is signed as it may be negative when it lands in encoder delay.
        seeked_to.actual_ts.get().max(0) as u64 * channels as u64
    }

    pub fn calculate_gain(&mut self) {
        self.process_only = true;

        let result = self.play();
        if let Err(error) = result {
            let mut res = self.error.lock().unwrap();
            *res = Some(error.to_string());
        }
    }

    pub fn play_loop(&mut self) -> Result<()> {
        self.play_internal(true)
    }

    pub fn play(&mut self) -> Result<()> {
        self.play_internal(false)
    }

    fn play_internal(&mut self, looping: bool) -> Result<()> {
        let reader = &mut self.format;

        // Grab the Track and it's ID
        let track = match reader.default_track(TrackType::Audio) {
            Some(track) => track,
            None => bail!("Unable to find Default Track"),
        };
        let track_id = track.id;

        // Grab the Audio Codec Parameters for the track
        let audio_params = match track.codec_params.as_ref().and_then(|params| params.audio()) {
            Some(params) => params,
            None => bail!("Unable to find Audio Codec Parameters"),
        };

        // The per-sample volume change when fading.
        let mut fade_amount: Option<f32> = None;

        // Sample Start and Stop positions..
        let mut first_frame: Option<u64> = None;
        let mut stop_sample: Option<u64> = None;

        let sample_rate = audio_params.sample_rate;
        let frames = track.num_frames;

        let mut ebu_r128 = None;

        let channels = match audio_params.channels.as_ref() {
            None => bail!("Unable to obtain channel count"),
            Some(channels) => channels.count(),
        };

        if channels > 2 {
            bail!("The Sample Player only Supports Mono and Stereo Samples");
        }

        if let Some(rate) = sample_rate {
            if self.process_only {
                ebu_r128 = Some(EbuR128::new(channels as u32, rate, Mode::I)?);
            } else {
                if let Some(fade_duration) = self.fade_duration {
                    // When fading out, we need work out the number of samples related to the fade
                    // duration, so (rate * duration) should give us the expected frame count, but
                    // we also need to multiply by the channel count to get the sample count.
                    //
                    // Once we have that calculated, we need to convert it to a volume multiplier,
                    // so for example, 48,000hz * 0.5 * 2 channels = 48,000 samples
                    // 1.0 / 48000 = 0.00002(ish), and we'll subtract that from the volume
                    // (between 1 and 0) for every sample received.
                    //
                    // This should have the volume at 0 after 48,000 iterations, at which point
                    // we terminate.
                    fade_amount = Some(
                        1.0 / ((rate as f32 * (fade_duration as f32 / 1000f32)) * channels as f32),
                    );
                }

                if let Some(frames) = frames {
                    if let Some(start_pct) = self.start_pct {
                        first_frame = Some(((frames as f64 / 100.0) * start_pct).round() as u64);
                        debug!(
                            "Starting Sample: {}",
                            first_frame.unwrap() * channels as u64
                        );
                    }

                    if let Some(stop_pct) = self.stop_pct {
                        stop_sample = Some(
                            ((frames as f64 / 100.0) * stop_pct).round() as u64 * channels as u64,
                        );
                        debug!("Stop Sample: {}", stop_sample.unwrap());
                    }
                }
            }
        } else {
            bail!("Unable to Determine the Audio File's Sample Rate");
        }

        // Audio Output Device..
        let mut audio_output = None;

        // Create a Decoder..
        let decoder_opts = AudioDecoderOptions::default().gapless(false);
        let mut decoder = get_codecs().make_audio_decoder(audio_params, &decoder_opts)?;

        // Prepare the Sample Buffer..
        let mut sample_buffer: Option<Vec<f32>> = None;

        // Start the Processed Sample Count..
        let mut samples_processed = if let Some(frame) = first_frame {
            let seek_time = Player::seek_to(frame, track_id);

            match reader.seek(SeekMode::Accurate, seek_time) {
                Ok(seeked_to) => Player::seeked_samples(seeked_to, channels),
                Err(_) => 0,
            }
        } else {
            0
        };

        let mut mono_playback = false;

        // Keep the decoder and output stream alive when looping. Re-opening either at every
        // EOF leaves the output without data and produces an audible gap.
        let loop_start = first_frame.unwrap_or_default();

        // Loop over the input file..
        let result = 'main: loop {
            let packet = match reader.next_packet() {
                Ok(packet) => packet,
                Err(Error::IoError(error)) if error.kind() == UnexpectedEof => None,
                Err(err) => break Err(err),
            };

            // No packet means we've hit the end of the stream.
            let Some(packet) = packet else {
                if looping && !self.stopping.load(Ordering::Relaxed) {
                    let seek_time = Player::seek_to(loop_start, track_id);

                    match reader.seek(SeekMode::Accurate, seek_time) {
                        Ok(seeked_to) => {
                            decoder.reset();
                            samples_processed = Player::seeked_samples(seeked_to, channels);
                            continue;
                        }
                        Err(seek_error) => break Err(seek_error),
                    }
                }
                break Ok(());
            };

            if packet.track_id != track_id {
                // Doesn't belong to us.
                continue;
            }

            match decoder.decode(&packet) {
                Ok(decoded) => {
                    // Is this the first decoded packet?
                    if audio_output.is_none() && sample_buffer.is_none() {
                        let spec_rate = decoded.spec().rate();
                        let spec_channels = decoded.spec().channels().count();
                        let mut out_channels = spec_channels;

                        // This is a mono audio file, we need to replace the spec with a Stereo
                        // spec, then during processing, duplicate and interlace the samples.
                        if spec_channels == 1 {
                            mono_playback = true;
                            out_channels = 2;
                        }

                        let capacity = decoded.capacity();
                        sample_buffer = Some(Vec::<f32>::with_capacity(capacity * spec_channels));

                        if !self.process_only {
                            let audio_spec = AudioSpecification {
                                device: self.device.clone(),
                                rate: spec_rate,
                                channels: out_channels,
                                buffer: capacity,
                            };

                            audio_output.replace(get_output(audio_spec)?);
                        }
                    }

                    if let Some(ref mut buf) = sample_buffer {
                        // Grab out the samples..
                        buf.clear();
                        decoded.copy_to_vec_interleaved(buf);
                        let mut samples = buf.to_vec();

                        if mono_playback {
                            let mut buffer = vec![];
                            samples.iter().for_each(|s| {
                                buffer.push(*s);
                                buffer.push(*s)
                            });
                            samples = buffer;
                        }

                        if let Some(ref mut ebu_r128) = ebu_r128 {
                            ebu_r128.add_frames_f32(samples.as_slice())?;
                            samples_processed += samples.len() as u64;

                            let progress = Player::processed(frames, samples_processed, channels);
                            if self.progress.load(Ordering::Relaxed) != progress {
                                self.progress.store(progress, Ordering::Relaxed);
                            }

                            // Skip straight to the next packet..
                            continue;
                        }

                        // Apply any gain to the samples..
                        if let Some(gain) = self.gain {
                            for sample in samples.iter_mut() {
                                *sample *= gain as f32;
                            }
                        }

                        if self.stopping.load(Ordering::Relaxed) {
                            if self.force_stop.load(Ordering::Relaxed) {
                                debug!("Force Stop Requested, terminating.");
                                break 'main Ok(());
                            }

                            if let Some(fade_amount) = fade_amount {
                                // Technically, this is a little weird, we don't do a 'per-channel' check on the samples,
                                // so each channel will have a slightly different volume, for now it's small enough to not
                                // actually notice :p

                                for sample in samples.iter_mut() {
                                    *sample *= self.volume;

                                    self.volume -= fade_amount;
                                    if self.volume < 0.0 {
                                        break 'main Ok(());
                                    }
                                }
                            } else {
                                // No fade duration, clear out sample buffer and end.
                                debug!("Stop Requested, No Fade Out set, Stopping Playback.");
                                break 'main Ok(());
                            }
                        }

                        // Flush the samples to the Audio Stream..
                        if let Some(audio_output) = &mut audio_output
                            && let Err(e) = audio_output.write(&samples)
                        {
                            break 'main Err(Error::IoError(io::Error::other(e.to_string())));
                        }

                        samples_processed += samples.len() as u64;

                        // Calculate the Current Processing Percent..
                        let progress = Player::processed(frames, samples_processed, channels);
                        if self.progress.load(Ordering::Relaxed) != progress {
                            self.progress.store(progress, Ordering::Relaxed);
                        }

                        if let Some(stop_sample) = stop_sample
                            && samples_processed >= stop_sample
                        {
                            if looping && !self.stopping.load(Ordering::Relaxed) {
                                let seek_time = Player::seek_to(loop_start, track_id);

                                match reader.seek(SeekMode::Accurate, seek_time) {
                                    Ok(seeked_to) => {
                                        decoder.reset();
                                        samples_processed =
                                            Player::seeked_samples(seeked_to, channels);
                                        continue 'main;
                                    }
                                    Err(error) => break 'main Err(error),
                                }
                            }
                            break Ok(());
                        }

                        if self.restart_track.load(Ordering::Relaxed) {
                            // We've been prompted to restart the current track..
                            let start_frame = first_frame.unwrap_or_default();

                            let seek_time = Player::seek_to(start_frame, track_id);

                            // Seek the reader backwards to the start..
                            samples_processed = match reader.seek(SeekMode::Accurate, seek_time) {
                                Ok(seeked_to) => Player::seeked_samples(seeked_to, channels),
                                Err(e) => {
                                    debug!("Error Seeking: {}", e);
                                    0
                                }
                            };

                            // Set the back to FALSE
                            self.restart_track.store(false, Ordering::Relaxed);
                        }
                    }
                }
                Err(err) => break Err(err),
            }
        };

        if !self.force_stop.load(Ordering::Relaxed)
            && let Some(ref mut audio_output) = audio_output
        {
            // Always flush the last samples, unless forced to stop
            audio_output.flush();
        }

        // Stop the playback handler..
        if let Some(mut audio_output) = audio_output {
            audio_output.stop();
        }

        if let Some(ebu_r128) = ebu_r128 {
            // Calculate Gain..
            let mut loudness = ebu_r128.loudness_global()?;
            if loudness == f64::NEG_INFINITY {
                debug!("Unable to Obtain loudness in Mode I, trying M..");
                loudness = ebu_r128.loudness_momentary()?;
            }

            if loudness == f64::NEG_INFINITY {
                debug!("Unable to Obtain loudness in Mode M, Setting Default..");
                self.normalized_gain.store(1.0, Ordering::Relaxed);
            } else {
                let target = -23.0;
                let gain_db = target - loudness;
                let value = f64::powf(10., gain_db / 20.);

                self.normalized_gain.store(value, Ordering::Relaxed);
            }
        }
        decoder.finalize();
        result.map_err(|e| anyhow!(e))

    }

    fn processed(total_frames: Option<u64>, current_frame: u64, channels: usize) -> u8 {
        // Calculate the Current Processing Percent..
        if let Some(frames) = total_frames {
            let frames_processed = current_frame / channels as u64;
            let processed_ratio = frames_processed as f64 / frames as f64;
            let calc_percent = (processed_ratio * 100.) as u8;

            return calc_percent;
        }
        0
    }

    pub fn get_state(&self) -> PlayerState {
        PlayerState {
            playing_file: self.file.clone(),
            stop: self.stopping.clone(),
            force_stop: self.force_stop.clone(),
            restart_track: self.restart_track.clone(),
            progress: self.progress.clone(),
            error: self.error.clone(),
            calculated_gain: self.normalized_gain.clone(),
        }
    }
}

#[derive(Debug)]
pub struct PlayerState {
    // Note the file being played..
    pub playing_file: PathBuf,

    pub stop: Arc<AtomicBool>,
    pub force_stop: Arc<AtomicBool>,

    // Used for triggering a seek back to the beginning
    pub restart_track: Arc<AtomicBool>,

    // Generally read-only from the outside..
    pub progress: Arc<AtomicU8>,
    pub error: Arc<Mutex<Option<String>>>,

    // Specifically for calculating the gain..
    pub calculated_gain: Arc<AtomicF64>,
}
