//! Measurement history.
//!
//! The dashboard only ever needed the previous reading, to draw a trend arrow.
//! The charts need a lot more than that, and they need it to survive a restart —
//! a history view that empties every time the app is reopened is not much of a
//! history.
//!
//! So samples are kept in a fixed-size ring and mirrored to a JSON file. This is
//! deliberately *not* stored in `config.json`: that file holds user preferences
//! the user chose, while this is recorded data the app produced. Mixing them
//! would mean a corrupt history could cost you your settings.
//!
//! The file lives under the XDG *data* directory rather than the config
//! directory, which is the convention for exactly this distinction.

use std::collections::VecDeque;
use std::fs::read_to_string;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::sensors::AirMeasureSnapshot;
use crate::storage::{write_atomically, xdg_app_dir};

/// How many samples to keep.
///
/// At the default 30-second refresh this is a day of readings, which is what
/// makes the history view worth looking at. It is a count rather than a duration
/// because the refresh interval is configurable: a 5-second interval keeps four
/// hours, a 5-minute interval keeps ten days.
pub const CAPACITY: usize = 2_880;

/// Seconds in an hour, used to bucket samples for hourly means.
const SECONDS_PER_HOUR: u64 = 3_600;

/// One recorded measurement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// Seconds since the Unix epoch.
    ///
    /// Stored as a plain number rather than a `SystemTime` so the file stays
    /// readable and portable.
    pub recorded_at: u64,
    pub snapshot: AirMeasureSnapshot,
}

impl Sample {
    /// Record a snapshot as having been taken now.
    pub fn now(snapshot: AirMeasureSnapshot) -> Self {
        Self {
            recorded_at: unix_seconds(SystemTime::now()),
            snapshot,
        }
    }
}

/// A bounded, persistable series of measurements, oldest first.
#[derive(Debug, Clone)]
pub struct History {
    samples: VecDeque<Sample>,
    capacity: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::with_capacity(CAPACITY)
    }
}

impl History {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            // A capacity of zero would make `push` a no-op and silently break
            // every chart, so treat it as "keep one".
            capacity: capacity.max(1),
            samples: VecDeque::new(),
        }
    }

    /// Add a sample, discarding the oldest when full.
    pub fn push(&mut self, sample: Sample) {
        self.samples.push_back(sample);
        while self.samples.len() > self.capacity {
            self.samples.pop_front();
        }
    }

    pub fn samples(&self) -> &VecDeque<Sample> {
        &self.samples
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The most recently recorded snapshot.
    pub fn latest(&self) -> Option<&AirMeasureSnapshot> {
        self.samples.back().map(|sample| &sample.snapshot)
    }

    /// Every recorded snapshot, oldest first.
    pub fn snapshots(&self) -> Vec<AirMeasureSnapshot> {
        self.samples
            .iter()
            .map(|sample| sample.snapshot.clone())
            .collect()
    }

    /// Mean of one metric for each of the last `hours` hours, most recent first.
    ///
    /// An hour with no readings is `None` rather than zero, so a gap in
    /// recording is not mistaken for perfectly clean air. This is the shape the
    /// EPA NowCast expects; see `sensors::aqi::nowcast`.
    pub fn hourly_means(
        &self,
        now: u64,
        hours: usize,
        metric: impl Fn(&AirMeasureSnapshot) -> Option<f32>,
    ) -> Vec<Option<f32>> {
        let mut totals = vec![(0.0_f32, 0_usize); hours];

        for sample in &self.samples {
            let Some(value) = metric(&sample.snapshot) else {
                continue;
            };
            // A sample stamped in the future, which a clock change can produce,
            // counts as belonging to the current hour rather than overflowing.
            let hours_ago = (now.saturating_sub(sample.recorded_at) / SECONDS_PER_HOUR) as usize;
            let Some(bucket) = totals.get_mut(hours_ago) else {
                continue;
            };
            bucket.0 += value;
            bucket.1 += 1;
        }

        totals
            .into_iter()
            .map(|(sum, count)| (count > 0).then(|| sum / count as f32))
            .collect()
    }

    /// Read history from disk.
    ///
    /// A missing or unreadable file is not an error: history is recorded data
    /// that the app can simply start collecting again, so anything unusable
    /// yields an empty history rather than blocking startup.
    pub fn load_from_path(path: &Path, capacity: usize) -> Self {
        let mut history = Self::with_capacity(capacity);

        let raw = match read_to_string(path) {
            Ok(raw) => raw,
            // A first launch has no file yet, which is ordinary and silent.
            Err(err) if err.kind() == io::ErrorKind::NotFound => return history,
            Err(err) => {
                eprintln!(
                    "Could not read measurement history from {}: {err}",
                    path.display()
                );
                return history;
            }
        };

        let samples = match serde_json::from_str::<Vec<Sample>>(&raw) {
            Ok(samples) => samples,
            Err(err) => {
                // Starting over is the right recovery -- these are readings the
                // app can collect again -- but saying so matters, because the
                // next save overwrites the unreadable file and whatever was in
                // it is gone for good.
                eprintln!(
                    "Measurement history at {} is unreadable and will be replaced: {err}",
                    path.display()
                );
                return history;
            }
        };

        for sample in samples {
            history.push(sample);
        }
        history
    }

    /// Read history from the standard location.
    pub fn load() -> Self {
        Self::load_from_path(&history_path(), CAPACITY)
    }

    pub fn save_to_path(&self, path: &Path) -> io::Result<()> {
        let raw = serde_json::to_string(&self.samples).map_err(io::Error::other)?;
        write_atomically(path, &raw)
    }

    /// Write history to the standard location.
    pub fn save(&self) -> io::Result<()> {
        self.save_to_path(&history_path())
    }
}

/// The current time, in seconds since the Unix epoch.
pub fn unix_now() -> u64 {
    unix_seconds(SystemTime::now())
}

/// Seconds since the Unix epoch, clamped at the epoch itself.
///
/// A clock set before 1970 is not worth a failure path in a chart, so it reads
/// as "the beginning of time" instead.
fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

/// Where the history file lives.
pub fn history_path() -> PathBuf {
    data_dir().join("history.json")
}

/// The app's XDG data directory.
///
/// History is data the app recorded, not a setting the user chose, so it lives
/// under `$XDG_DATA_HOME` rather than beside `config.json`.
pub fn data_dir() -> PathBuf {
    xdg_app_dir("XDG_DATA_HOME", ".local/share")
}

#[cfg(test)]
mod tests {
    use super::{History, Sample};
    use crate::sensors::AirMeasureSnapshot;

    fn snapshot(co2: f32) -> AirMeasureSnapshot {
        AirMeasureSnapshot {
            co2: Some(co2),
            ..Default::default()
        }
    }

    /// The CO2 readings a history holds, oldest first.
    ///
    /// Charts pull a series out through `ui::metrics`; this is just a compact
    /// way for these tests to say what a history ended up containing.
    fn co2_series(history: &History) -> Vec<f32> {
        history
            .samples()
            .iter()
            .filter_map(|sample| sample.snapshot.co2)
            .collect()
    }

    fn sample(recorded_at: u64, co2: f32) -> Sample {
        Sample {
            recorded_at,
            snapshot: snapshot(co2),
        }
    }

    #[test]
    fn samples_are_kept_in_order() {
        let mut history = History::with_capacity(10);
        history.push(sample(1, 400.0));
        history.push(sample(2, 500.0));

        assert_eq!(history.len(), 2);
        assert_eq!(co2_series(&history), vec![400.0, 500.0]);
        assert_eq!(history.latest(), Some(&snapshot(500.0)));
    }

    #[test]
    fn oldest_samples_are_discarded_when_full() {
        let mut history = History::with_capacity(3);
        for second in 1..=5_u64 {
            history.push(sample(second, second as f32 * 100.0));
        }

        assert_eq!(history.len(), 3);
        assert_eq!(co2_series(&history), vec![300.0, 400.0, 500.0]);
    }

    #[test]
    fn zero_capacity_still_keeps_the_latest_sample() {
        let mut history = History::with_capacity(0);
        history.push(sample(1, 400.0));

        assert_eq!(history.len(), 1);
    }

    #[test]
    fn missing_readings_are_stored_without_being_zeroed() {
        let mut history = History::with_capacity(10);
        history.push(sample(1, 400.0));
        history.push(Sample {
            recorded_at: 2,
            snapshot: AirMeasureSnapshot::default(),
        });
        history.push(sample(3, 600.0));

        assert_eq!(co2_series(&history), vec![400.0, 600.0]);
    }

    #[test]
    fn round_trips_through_a_file() {
        let path = temp_path("round-trip");
        let mut history = History::with_capacity(10);
        history.push(sample(1, 400.0));
        history.push(sample(2, 500.0));

        history.save_to_path(&path).expect("history should write");
        let loaded = History::load_from_path(&path, 10);

        assert_eq!(loaded.samples(), history.samples());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn loading_applies_the_current_capacity() {
        // Shrinking the retention limit between releases must not resurrect a
        // longer history than the new limit allows.
        let path = temp_path("capacity");
        let mut history = History::with_capacity(10);
        for second in 1..=10_u64 {
            history.push(sample(second, second as f32));
        }
        history.save_to_path(&path).expect("history should write");

        let loaded = History::load_from_path(&path, 4);

        assert_eq!(loaded.len(), 4);
        assert_eq!(co2_series(&loaded), vec![7.0, 8.0, 9.0, 10.0]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn hourly_means_average_each_hour_separately() {
        const HOUR: u64 = 3_600;
        let mut history = History::default();
        // Two readings in the current hour and one three hours back.
        for (recorded_at, pm25) in [
            (1_000 * HOUR, 10.0),
            (1_000 * HOUR, 20.0),
            (997 * HOUR, 4.0),
        ] {
            history.push(Sample {
                recorded_at,
                snapshot: AirMeasureSnapshot {
                    pm25: Some(pm25),
                    ..AirMeasureSnapshot::default()
                },
            });
        }

        let means = history.hourly_means(1_000 * HOUR, 5, |snapshot| snapshot.pm25);

        assert_eq!(
            means[0],
            Some(15.0),
            "the current hour averages its two readings"
        );
        assert_eq!(means[1], None, "an hour with no readings stays empty");
        assert_eq!(means[2], None);
        assert_eq!(means[3], Some(4.0));
        assert_eq!(means[4], None);
    }

    #[test]
    fn hourly_means_skip_readings_the_sensor_did_not_report() {
        let mut history = History::default();
        history.push(Sample {
            recorded_at: 3_600,
            snapshot: AirMeasureSnapshot::default(),
        });

        let means = history.hourly_means(3_600, 2, |snapshot| snapshot.pm25);

        assert_eq!(means[0], None, "a missing reading must not count as zero");
    }

    #[test]
    fn a_missing_file_yields_an_empty_history() {
        let loaded = History::load_from_path(&temp_path("absent"), 10);

        assert!(loaded.is_empty());
    }

    #[test]
    fn a_corrupt_file_yields_an_empty_history() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "{ not json").expect("file should write");

        let loaded = History::load_from_path(&path, 10);

        assert!(loaded.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("airgradient-history-{name}-{nanos}.json"))
    }
}
