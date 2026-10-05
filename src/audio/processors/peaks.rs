//! A new file! This takes an audio file and returns all its peaks for the waveform.. This is a
//! streamlined version of the player because I didn't want a THIRD pass function for it.

use std::fs::File;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::TrackType;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::default::{get_codecs, get_probe};

pub fn compute_peaks(path: &Path, buckets: usize) -> Result<Vec<f64>> {
    let buckets = buckets.clamp(16, 4096);

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let stream = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
    let mut format = get_probe()
        .probe(&hint, stream, Default::default(), Default::default())
        .map_err(|e| anyhow!("Unable to Probe Audio File: {e}"))?;

    let (track_id, params) = match format.default_track(TrackType::Audio) {
        Some(t) => match t.codec_params.as_ref().and_then(|p| p.audio()) {
            Some(params) => (t.id, params.clone()),
            None => bail!("Default Track has no Audio Codec Parameters"),
        },
        None => bail!("Unable to find Default Track"),
    };

    let channels = params
        .channels
        .as_ref()
        .map(|c| c.count())
        .unwrap_or(2)
        .max(1);

    let decoder_opts = AudioDecoderOptions::default().gapless(false);
    let mut decoder = get_codecs().make_audio_decoder(&params, &decoder_opts)?;

    // One peak per 512 frames, collapsed to `buckets` afterwards.
    const BLOCK: usize = 512;
    let mut blocks: Vec<f64> = Vec::new();
    let mut current = 0f64;
    let mut in_block = 0usize;
    let mut samples: Vec<f64> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(Error::IoError(_)) => break,
            Err(Error::ResetRequired) => break,
            Err(e) => bail!(e),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(Error::DecodeError(_)) => continue,
            Err(_) => break,
        };
        samples.clear();
        decoded.copy_to_vec_interleaved(&mut samples);
        for frame in samples.chunks(channels) {
            for s in frame {
                current = current.max(s.abs());
            }
            in_block += 1;
            if in_block == BLOCK {
                blocks.push(current.min(1.0));
                current = 0.0;
                in_block = 0;
            }
        }
    }
    if in_block > 0 {
        blocks.push(current.min(1.0));
    }
    if blocks.is_empty() {
        return Ok(vec![0.0; buckets]);
    }

    let mut out = vec![0f64; buckets];
    for (i, o) in out.iter_mut().enumerate() {
        let a = i * blocks.len() / buckets;
        let b = (((i + 1) * blocks.len()) / buckets)
            .max(a + 1)
            .min(blocks.len());
        *o = blocks[a..b].iter().cloned().fold(0.0, f64::max);
    }
    Ok(out)
}
