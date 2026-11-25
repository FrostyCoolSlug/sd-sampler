//! A disk-backed cache of computed waveform peaks and normalised gain.
//!
//! Both peak computation and EBU R128 gain calculation decode the whole sample, which is
//! expensive for long clips, and both are only needed once per file: a re-added (or shared)
//! sample resolves to the same values. The in-memory caches elsewhere don't survive a
//! restart, so results are stored under the user data directory (not in the samples directory
//! itself, which may be pointed anywhere) and keyed on the source file's path, modification
//! time and size, so entries are self-validating: a stale or hash-colliding entry is dropped
//! on read rather than trusted. [`WaveCache::cleanup`] removes everything that no longer
//! matches its source and is called on startup.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use directories::BaseDirs;
use log::debug;
use serde::{Deserialize, Serialize};

use crate::audio::processors::peaks::{compute_peaks, WAVE_BUCKETS};

/// A single cache entry. `path`, `mtime` and `size` are verified against the source on read so
/// a stale (or hash-colliding) entry is dropped rather than trusted.
#[derive(Serialize, Deserialize)]
struct Entry {
    path: String,
    mtime: u64,
    size: u64,
    peaks: Vec<f64>,

    /// Linear gain that brings the file to -23dB LUFS EBU R128, if it's been computed.
    #[serde(default)]
    gain: Option<f64>,
}

/// Safety cap on cache entries so a huge samples directory can't grow the cache without bound.
const MAX_ENTRIES: usize = 4096;

#[derive(Clone)]
pub struct WaveCache {
    dir: PathBuf,
}

impl WaveCache {
    pub fn new() -> Self {
        let dir = BaseDirs::new()
            .expect("Unable to Load Base Directories")
            .data_dir()
            .join("sd-sampler")
            .join(".wavecache");

        Self::in_dir(dir)
    }

    fn in_dir(dir: PathBuf) -> Self {
        fs::create_dir_all(&dir).expect("Unable to Create Waveform Cache Directory");

        Self { dir }
    }

    /// Peaks for `path` at the canonical resolution, from disk if we already have them,
    /// otherwise by decoding the file. Failures are not cached, so a transient error retries
    /// next time.
    pub fn compute_or_get(&self, path: &Path) -> Result<Vec<f64>> {
        if let Some(peaks) = self.load(path) {
            return Ok(peaks);
        }

        let peaks = compute_peaks(path, WAVE_BUCKETS)?;
        if !peaks.is_empty() {
            self.store(path, &peaks, None);
        }
        Ok(peaks)
    }

    /// A disk hit for `path`'s peaks, if one exists and still matches its source.
    pub fn load(&self, path: &Path) -> Option<Vec<f64>> {
        let peaks = self.read_entry(path)?.peaks;
        // A gain-only entry has no peaks yet; treat it as a miss so they get computed.
        if peaks.is_empty() {
            return None;
        }
        Some(peaks)
    }

    /// A disk hit for `path`'s normalised gain, if one exists and still matches its source.
    pub fn gain(&self, path: &Path) -> Option<f64> {
        self.read_entry(path)?.gain
    }

    /// Store a computed normalised gain for `path`, merging with any peaks already cached.
    pub fn store_gain(&self, path: &Path, gain: f64) {
        self.store(path, &[], Some(gain));
    }

    /// Read and validate the entry for `path`, if one exists. A stale or hash-colliding entry
    /// is dropped rather than trusted.
    fn read_entry(&self, path: &Path) -> Option<Entry> {
        let file = self.dir.join(key(path));
        let bytes = fs::read(&file).ok()?;
        let entry: Entry = serde_json::from_slice(&bytes).ok()?;

        // The stored path guards against a hash collision resolving to the wrong sample.
        if entry.path != path.to_string_lossy() || !source_matches(&entry, path) {
            let _ = fs::remove_file(&file);
            return None;
        }

        Some(entry)
    }

    /// Drop entries whose source has gone away or changed since they were written, remove any
    /// orphaned temp files from an interrupted write, and cap the total number of entries.
    /// Called on startup.
    pub fn cleanup(&self) {
        let Ok(rd) = fs::read_dir(&self.dir) else {
            return;
        };

        // (mtime, path) of every live entry, for the size cap below.
        let mut live: Vec<(SystemTime, PathBuf)> = Vec::new();

        for entry in rd.filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if ext == "tmp" {
                // Orphaned temp file from an interrupted write.
                let _ = fs::remove_file(&path);
                continue;
            }
            if ext != "json" {
                continue;
            }

            let keep = fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Entry>(&bytes).ok())
                .is_some_and(|entry| source_matches(&entry, Path::new(&entry.path)));

            if !keep {
                let _ = fs::remove_file(&path);
                continue;
            }

            if let Ok(meta) = entry.metadata()
                && let Ok(mtime) = meta.modified()
            {
                live.push((mtime, path));
            }
        }

        // Cap: keep the most recently written entries.
        if live.len() > MAX_ENTRIES {
            live.sort();
            for (_, path) in live.iter().take(live.len() - MAX_ENTRIES) {
                let _ = fs::remove_file(path);
            }
        }

        debug!("Waveform Cache Cleanup Complete, {} Entries Live", live.len());
    }

    /// Write an entry for `path`, merging with any existing valid one so a peaks-only or
    /// gain-only computation doesn't clobber the other half.
    fn store(&self, path: &Path, peaks: &[f64], gain: Option<f64>) {
        let Ok(meta) = fs::metadata(path) else {
            return;
        };

        // Merge with what's already on disk (if it still matches the source).
        let existing = self.read_entry(path);
        let merged_peaks = if !peaks.is_empty() {
            peaks.to_vec()
        } else {
            existing.as_ref().map(|e| e.peaks.clone()).unwrap_or_default()
        };
        let merged_gain = gain.or_else(|| existing.and_then(|e| e.gain));

        if merged_peaks.is_empty() && merged_gain.is_none() {
            return;
        }

        let entry = Entry {
            path: path.to_string_lossy().into_owned(),
            mtime: stamp(&meta),
            size: meta.len(),
            peaks: merged_peaks,
            gain: merged_gain,
        };

        // Write to a temp file and rename so a crash mid-write can't leave a half entry.
        let final_path = self.dir.join(key(path));
        let tmp = self
            .dir
            .join(format!("{}.tmp{:08x}", key(path), fastrand::u64(0..u32::MAX as u64)));

        if fs::write(&tmp, serde_json::to_vec(&entry).unwrap_or_default()).is_ok() {
            let _ = fs::rename(&tmp, &final_path);
        } else {
            let _ = fs::remove_file(&tmp);
        }
    }
}

/// True if the source file for `entry` still exists with the same modification time and size.
fn source_matches(entry: &Entry, path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(meta) => (entry.mtime, entry.size) == (stamp(&meta), meta.len()),
        Err(_) => false,
    }
}

/// The file's modification time as nanoseconds since the Unix epoch (0 if unobtainable).
fn stamp(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// A stable per-path filename. FNV-1a; a collision is harmless because every entry stores its
/// source path and is re-validated on read.
fn key(path: &Path) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in path.to_string_lossy().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}.json", h)
}
