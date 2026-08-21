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
use crate::sensors::{AirMeasureSnapshot, GasUnit};

/// Accent for readings with no agreed indoor health thresholds.
const NEUTRAL_CLASS: &str = "status-blue";
const COARSE_CLASS: &str = "status-orange";

/// How a reading's colour is decided.
#[derive(Clone, Copy)]
enum Judgement {
    /// Judged against published indoor breakpoints, so the colour follows the
    /// value.
    Thresholds(fn(f32) -> StatusColor),
    /// No widely agreed indoor breakpoints for this reading, so it gets one
    /// fixed colour rather than a misleading verdict.
    Fixed(&'static str),
}

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
    /// How this reading's colour is decided.
    judgement: Judgement,
    /// The unit the device reported for this reading, where it can vary.
    ///
    /// VOC and NOx arrive as a unitless sensor index from AirGradient's own
    /// firmware and as a concentration in ppb from some compatible local-server
    /// implementations, so the payload has the last word. Everything else has a
    /// fixed unit and uses `no_reported_unit`.
    read_unit: fn(&AirMeasureSnapshot) -> Option<GasUnit>,
}

/// The reader for metrics whose unit never varies.
fn no_reported_unit(_: &AirMeasureSnapshot) -> Option<GasUnit> {
    None
}

impl Metric {
    pub fn value(&self, snapshot: &AirMeasureSnapshot) -> Option<f32> {
        (self.read)(snapshot)
    }

    /// CSS class describing how this value should look.
    pub fn status_class(&self, value: Option<f32>) -> &'static str {
        match self.judgement {
            Judgement::Thresholds(classify) => status::class_for(value, classify),
            Judgement::Fixed(class) => class,
        }
    }

    /// The unit this reading arrived in, when the device said.
    ///
    /// `None` means the metric's own `unit` is the right label.
    pub fn reported_unit(&self, snapshot: &AirMeasureSnapshot) -> Option<&'static str> {
        (self.read_unit)(snapshot).map(GasUnit::as_str)
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
        judgement: Judgement::Thresholds(aqi_status_color),
        read_unit: no_reported_unit,
    },
    Metric {
        id: CO2_ID,
        title: "CO₂",
        unit: "ppm",
        icon: "airgradient-co2-symbolic",
        read: |snapshot| snapshot.co2,
        judgement: Judgement::Thresholds(co2_status_color),
        read_unit: no_reported_unit,
    },
    Metric {
        id: TEMPERATURE_ID,
        title: "Temperature",
        unit: "°C",
        icon: "airgradient-temperature-symbolic",
        read: |snapshot| snapshot.temperature,
        judgement: Judgement::Fixed(NEUTRAL_CLASS),
        read_unit: no_reported_unit,
    },
    Metric {
        id: HUMIDITY_ID,
        title: "Humidity",
        unit: "%",
        icon: "airgradient-humidity-symbolic",
        read: |snapshot| snapshot.humidity,
        judgement: Judgement::Fixed(NEUTRAL_CLASS),
        read_unit: no_reported_unit,
    },
    Metric {
        id: TVOC_ID,
        title: "TVOC",
        unit: "index",
        icon: "airgradient-voc-symbolic",
        read: |snapshot| snapshot.tvoc,
        judgement: Judgement::Thresholds(tvoc_status_color),
        read_unit: |snapshot| snapshot.tvoc_unit,
    },
    Metric {
        id: NOX_ID,
        title: "NOx",
        unit: "index",
        icon: "airgradient-nox-symbolic",
        read: |snapshot| snapshot.nox,
        judgement: Judgement::Thresholds(nox_status_color),
        read_unit: |snapshot| snapshot.nox_unit,
    },
    Metric {
        id: PM003_COUNT_ID,
        title: "PM₀.₃ Count",
        unit: "count",
        icon: "airgradient-particles-symbolic",
        read: |snapshot| snapshot.pm003_count,
        judgement: Judgement::Fixed(NEUTRAL_CLASS),
        read_unit: no_reported_unit,
    },
    Metric {
        id: PM1_ID,
        title: "PM₁.₀",
        unit: "µg/m³",
        icon: "airgradient-particles-symbolic",
        read: |snapshot| snapshot.pm1,
        judgement: Judgement::Fixed(NEUTRAL_CLASS),
        read_unit: no_reported_unit,
    },
    Metric {
        id: PM25_ID,
        title: "PM₂.₅",
        unit: "µg/m³",
        icon: "airgradient-particles-symbolic",
        read: |snapshot| snapshot.pm25,
        judgement: Judgement::Thresholds(pm25_status_color),
        read_unit: no_reported_unit,
    },
    Metric {
        id: PM10_ID,
        title: "PM₁₀",
        unit: "µg/m³",
        icon: "airgradient-particles-symbolic",
        read: |snapshot| snapshot.pm10,
        judgement: Judgement::Fixed(COARSE_CLASS),
        read_unit: no_reported_unit,
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
    use crate::sensors::{AirMeasureSnapshot, GasUnit};
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
    fn units_are_the_ones_both_tabs_show() {
        // The dashboard cards and their trend lines take their unit from here,
        // so these strings are user-visible on both tabs, not just internal.
        assert_eq!(find(CO2_ID).unit, "ppm");
        assert_eq!(find(PM25_ID).unit, "µg/m³");
        assert_eq!(find(PM003_COUNT_ID).unit, "count");
        // The gases default to the sensor index AirGradient's own firmware
        // reports; a device sending a concentration overrides it per reading.
        assert_eq!(find(TVOC_ID).unit, "index");
        assert_eq!(find(NOX_ID).unit, "index");
    }

    #[test]
    fn only_the_gases_take_their_unit_from_the_payload() {
        let reported = AirMeasureSnapshot {
            tvoc_unit: Some(GasUnit::Ppb),
            nox_unit: Some(GasUnit::Index),
            ..snapshot()
        };

        assert_eq!(find(TVOC_ID).reported_unit(&reported), Some("ppb"));
        assert_eq!(find(NOX_ID).reported_unit(&reported), Some("index"));
        // Everything else has a fixed unit, whatever the payload contains.
        assert_eq!(find(CO2_ID).reported_unit(&reported), None);
        assert_eq!(find(PM25_ID).reported_unit(&reported), None);
    }

    #[test]
    fn a_payload_that_names_no_gas_unit_falls_back_to_the_metrics_own() {
        assert_eq!(find(TVOC_ID).reported_unit(&snapshot()), None);
        assert_eq!(find(TVOC_ID).unit, "index");
    }

    #[test]
    fn the_charted_metric_exists() {
        assert_eq!(find(PM25_ID).title, "PM₂.₅");
    }
}
