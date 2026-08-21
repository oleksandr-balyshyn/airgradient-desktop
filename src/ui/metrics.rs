//! The list of things the app measures.
//!
//! Both the history view and the PM2.5 chart on the main view need the same
//! facts about a reading: what to call it, what unit it is in, which icon it
//! uses, how to pull it out of a snapshot, and how to judge whether the value is
//! good or bad. Writing that once as a table means the history view is a loop
//! over `METRICS` rather than ten near-identical blocks, and adding a sensor is
//! one entry here.

use super::status;
use crate::sensors::thresholds::{
    aqi_status_color, co2_status_color, nox_status_color, pm25_status_color, tvoc_status_color,
    StatusColor,
};
use crate::sensors::AirMeasureSnapshot;

/// Accent for readings with no agreed indoor health thresholds.
const NEUTRAL_CLASS: &str = "status-blue";
const COARSE_CLASS: &str = "status-orange";

/// One measurable quantity.
pub struct Metric {
    /// Stable identifier, used to look a metric up.
    pub id: &'static str,
    pub title: &'static str,
    /// Unit shown when a reading does not carry its own.
    pub unit: &'static str,
    pub icon: &'static str,
    /// How to read this metric out of a snapshot.
    read: fn(&AirMeasureSnapshot) -> Option<f32>,
    /// How to judge the value, where thresholds exist.
    ///
    /// `None` means there are no widely agreed indoor breakpoints for this
    /// reading, so it gets a fixed colour rather than a misleading verdict.
    classify: Option<fn(f32) -> StatusColor>,
    /// Colour used when `classify` is `None`.
    fixed_class: &'static str,
}

impl Metric {
    pub fn value(&self, snapshot: &AirMeasureSnapshot) -> Option<f32> {
        (self.read)(snapshot)
    }

    /// CSS class describing how this value should look.
    pub fn status_class(&self, value: Option<f32>) -> &'static str {
        match self.classify {
            Some(classify) => status::class_for(value, classify),
            None => self.fixed_class,
        }
    }

    /// This metric's readings across a run of snapshots, oldest first.
    ///
    /// Snapshots where the sensor reported nothing are left out rather than
    /// counted as zero, which would draw a false dip through the chart.
    pub fn series(&self, snapshots: &[AirMeasureSnapshot]) -> Vec<f32> {
        snapshots
            .iter()
            .filter_map(|snapshot| self.value(snapshot))
            .collect()
    }
}

/// Stable identifiers, one per entry in `METRICS`.
///
/// Call sites name a metric through these rather than through a bare string, so
/// a typo is a compile error instead of a panic inside `find`.
pub const AQI_ID: &str = "aqi";
pub const CO2_ID: &str = "co2";
pub const TEMPERATURE_ID: &str = "temperature";
pub const HUMIDITY_ID: &str = "humidity";
pub const TVOC_ID: &str = "tvoc";
pub const NOX_ID: &str = "nox";
pub const PM003_COUNT_ID: &str = "pm003_count";
pub const PM1_ID: &str = "pm1";
/// Identifier of the metric charted on the main view.
pub const PM25_ID: &str = "pm25";
pub const PM10_ID: &str = "pm10";

/// Every metric, in the order the history view shows them.
///
/// Ordering is deliberate: the two headline numbers first, then comfort, then
/// gases, then particulates from finest to coarsest.
pub const METRICS: &[Metric] = &[
    Metric {
        id: AQI_ID,
        title: "Air Quality Index",
        unit: "AQI",
        icon: "airgradient-air-quality-symbolic",
        read: |snapshot| snapshot.aqi,
        classify: Some(aqi_status_color),
        fixed_class: NEUTRAL_CLASS,
    },
    Metric {
        id: CO2_ID,
        title: "CO₂",
        unit: "ppm",
        icon: "airgradient-co2-symbolic",
        read: |snapshot| snapshot.co2,
        classify: Some(co2_status_color),
        fixed_class: NEUTRAL_CLASS,
    },
    Metric {
        id: TEMPERATURE_ID,
        title: "Temperature",
        unit: "°C",
        icon: "airgradient-temperature-symbolic",
        read: |snapshot| snapshot.temperature,
        classify: None,
        fixed_class: NEUTRAL_CLASS,
    },
    Metric {
        id: HUMIDITY_ID,
        title: "Humidity",
        unit: "%",
        icon: "airgradient-humidity-symbolic",
        read: |snapshot| snapshot.humidity,
        classify: None,
        fixed_class: NEUTRAL_CLASS,
    },
    Metric {
        id: TVOC_ID,
        title: "TVOC",
        unit: "index",
        icon: "airgradient-voc-symbolic",
        read: |snapshot| snapshot.tvoc,
        classify: Some(tvoc_status_color),
        fixed_class: NEUTRAL_CLASS,
    },
    Metric {
        id: NOX_ID,
        title: "NOx",
        unit: "index",
        icon: "airgradient-nox-symbolic",
        read: |snapshot| snapshot.nox,
        classify: Some(nox_status_color),
        fixed_class: NEUTRAL_CLASS,
    },
    Metric {
        id: PM003_COUNT_ID,
        title: "PM₀.₃ Count",
        unit: "count",
        icon: "airgradient-particles-symbolic",
        read: |snapshot| snapshot.pm003_count,
        classify: None,
        fixed_class: NEUTRAL_CLASS,
    },
    Metric {
        id: PM1_ID,
        title: "PM₁.₀",
        unit: "µg/m³",
        icon: "airgradient-particles-symbolic",
        read: |snapshot| snapshot.pm1,
        classify: None,
        fixed_class: NEUTRAL_CLASS,
    },
    Metric {
        id: PM25_ID,
        title: "PM₂.₅",
        unit: "µg/m³",
        icon: "airgradient-particles-symbolic",
        read: |snapshot| snapshot.pm25,
        classify: Some(pm25_status_color),
        fixed_class: NEUTRAL_CLASS,
    },
    Metric {
        id: PM10_ID,
        title: "PM₁₀",
        unit: "µg/m³",
        icon: "airgradient-particles-symbolic",
        read: |snapshot| snapshot.pm10,
        classify: None,
        fixed_class: COARSE_CLASS,
    },
];

/// Look up a metric by id.
///
/// # Panics
///
/// Panics if the id is not in `METRICS`. Ids are compile-time constants from this
/// module, so a failure here is a programming error rather than bad input.
pub fn find(id: &str) -> &'static Metric {
    METRICS
        .iter()
        .find(|metric| metric.id == id)
        .unwrap_or_else(|| panic!("unknown metric id: {id}"))
}

#[cfg(test)]
mod tests {
    use super::{
        find, AQI_ID, CO2_ID, HUMIDITY_ID, METRICS, NOX_ID, PM003_COUNT_ID, PM10_ID, PM1_ID,
        PM25_ID, TEMPERATURE_ID, TVOC_ID,
    };
    use crate::sensors::AirMeasureSnapshot;
    use crate::ui::status::UNKNOWN_CLASS;

    fn snapshot() -> AirMeasureSnapshot {
        AirMeasureSnapshot {
            aqi: Some(96.0),
            co2: Some(706.0),
            temperature: Some(27.8),
            humidity: Some(52.0),
            tvoc: Some(455.0),
            nox: Some(2.0),
            pm003_count: Some(1541.0),
            pm1: Some(23.5),
            pm25: Some(33.6),
            pm10: Some(37.6),
            ..Default::default()
        }
    }

    #[test]
    fn metric_ids_are_unique() {
        let mut ids: Vec<&str> = METRICS.iter().map(|metric| metric.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();

        assert_eq!(ids.len(), count, "duplicate metric id");
    }

    #[test]
    fn every_metric_reads_a_populated_snapshot() {
        // Catches a metric whose reader was wired to the wrong field and always
        // returns nothing.
        for metric in METRICS {
            assert!(
                metric.value(&snapshot()).is_some(),
                "{} read nothing from a full snapshot",
                metric.id
            );
        }
    }

    #[test]
    fn missing_readings_have_no_value() {
        for metric in METRICS {
            assert_eq!(metric.value(&AirMeasureSnapshot::default()), None);
        }
    }

    #[test]
    fn classified_metrics_report_a_status_and_unclassified_ones_a_fixed_colour() {
        assert_eq!(find(CO2_ID).status_class(Some(500.0)), "status-green");
        assert_eq!(find(CO2_ID).status_class(Some(2500.0)), "status-red");
        // No thresholds means the colour never changes with the value.
        assert_eq!(find(TEMPERATURE_ID).status_class(Some(5.0)), "status-blue");
        assert_eq!(find(TEMPERATURE_ID).status_class(Some(45.0)), "status-blue");
    }

    #[test]
    fn a_missing_reading_is_unknown_where_thresholds_exist() {
        assert_eq!(find(CO2_ID).status_class(None), UNKNOWN_CLASS);
    }

    #[test]
    fn series_skips_snapshots_without_the_reading() {
        let snapshots = vec![snapshot(), AirMeasureSnapshot::default(), snapshot()];

        assert_eq!(find(PM25_ID).series(&snapshots), vec![33.6, 33.6]);
    }

    #[test]
    fn every_id_constant_names_a_metric() {
        // `find` panics on an unknown id, so this is what keeps a renamed entry
        // from turning into a crash at runtime.
        for id in [
            AQI_ID,
            CO2_ID,
            TEMPERATURE_ID,
            HUMIDITY_ID,
            TVOC_ID,
            NOX_ID,
            PM003_COUNT_ID,
            PM1_ID,
            PM25_ID,
            PM10_ID,
        ] {
            assert_eq!(find(id).id, id);
        }
    }

    #[test]
    fn particulates_without_breakpoints_keep_their_fixed_colours() {
        // The main view reads these classes from here, so a change would repaint
        // the dashboard cards as well as the History tab.
        assert_eq!(find(PM003_COUNT_ID).status_class(Some(1.0)), "status-blue");
        assert_eq!(find(PM1_ID).status_class(Some(1.0)), "status-blue");
        assert_eq!(find(PM10_ID).status_class(Some(1.0)), "status-orange");
    }

    #[test]
    fn the_charted_metric_exists() {
        assert_eq!(find(PM25_ID).title, "PM₂.₅");
    }
}
