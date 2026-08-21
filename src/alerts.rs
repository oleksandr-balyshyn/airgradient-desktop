//! Air-quality alert policy.
//!
//! This module decides when sensor measurements should become notifications.
//! It deliberately does not know about GTK, D-Bus, or desktop notification
//! APIs; the UI adapter owns delivery.

use std::collections::HashMap;
use std::time::Instant;

use crate::sensors::comfort::ComfortThresholds;
use crate::sensors::AirMeasureSnapshot;

const ALERT_COOLDOWN_SECS: u64 = 20 * 60;
const ALERT_CONSECUTIVE_READINGS: u8 = 2;
/// Failed fetches in a row before the device is reported as offline.
///
/// A single miss is normal on a busy wifi network, so alerting on one would make
/// the notification meaningless.
const OFFLINE_AFTER_FAILURES: u8 = 3;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
enum AlertKind {
    Co2,
    Aqi,
    Pm25,
    Tvoc,
    Nox,
    HumidityHigh,
    Comfort,
    DeviceOffline,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub enum AlertSeverity {
    Notice,
    Warning,
    Critical,
}

pub struct AlertNotification {
    pub id: String,
    pub title: String,
    pub body: String,
    pub severity: AlertSeverity,
}

pub struct AlertMonitor {
    enabled: bool,
    /// The temperature and humidity ranges the user chose.
    comfort: ComfortThresholds,
    consecutive: HashMap<AlertKind, u8>,
    active_severity: HashMap<AlertKind, AlertSeverity>,
    last_sent: HashMap<AlertKind, Instant>,
    fetch_failures: u8,
}

impl AlertMonitor {
    pub fn new(enabled: bool, comfort: ComfortThresholds) -> Self {
        Self {
            enabled,
            comfort,
            consecutive: HashMap::new(),
            active_severity: HashMap::new(),
            last_sent: HashMap::new(),
            fetch_failures: 0,
        }
    }

    /// Replace the comfort ranges, after the user edits them in Settings.
    ///
    /// The run of consecutive readings collected under the old ranges is dropped:
    /// a room that was "too warm" only because the old maximum was lower has not
    /// been uncomfortable at all, and counting those readings towards an alert
    /// under the new ranges would report a problem the user just defined away.
    pub fn set_comfort(&mut self, comfort: ComfortThresholds) {
        if comfort == self.comfort {
            return;
        }

        self.comfort = comfort;
        self.consecutive.remove(&AlertKind::Comfort);
        self.active_severity.remove(&AlertKind::Comfort);
        self.last_sent.remove(&AlertKind::Comfort);
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.consecutive.clear();
            self.active_severity.clear();
            self.last_sent.clear();
            self.fetch_failures = 0;
        }
    }

    pub fn evaluate(&mut self, snapshot: &AirMeasureSnapshot) -> Vec<AlertNotification> {
        self.evaluate_at(snapshot, Instant::now())
    }

    pub fn evaluate_at(
        &mut self,
        snapshot: &AirMeasureSnapshot,
        now: Instant,
    ) -> Vec<AlertNotification> {
        if !self.enabled {
            return Vec::new();
        }

        // A reading arrived, so whatever run of failed fetches came before it is
        // over and the device is not offline.
        self.fetch_failures = 0;

        let mut alerts = Vec::new();
        for rule in RULES {
            let severity = (rule.read)(snapshot).and_then(rule.classify);
            let text = rule.text;
            self.push_if_alert(&mut alerts, rule.kind, severity, now, |severity| {
                let (title, body) = text(severity);
                (title.to_string(), body.to_string())
            });
        }
        self.push_comfort_alert(&mut alerts, snapshot, now);
        alerts
    }

    /// Judge temperature and humidity together against the user's own ranges.
    ///
    /// This is separate from `RULES` because it is the only alert that depends
    /// on two readings at once, and that is the point of it: what to do about a
    /// room depends on the pair. A warm, humid room wants the air conditioning,
    /// while a cool, humid one wants heating and a dehumidifier, and an alert
    /// about either reading on its own could not say so.
    ///
    /// Both readings outside their ranges is a `Warning`; one is a `Notice`. That
    /// also means a room that turns muggy after merely being warm escalates, and
    /// escalation bypasses the cooldown, so the more useful advice arrives at
    /// once instead of up to twenty minutes later.
    fn push_comfort_alert(
        &mut self,
        alerts: &mut Vec<AlertNotification>,
        snapshot: &AirMeasureSnapshot,
        now: Instant,
    ) {
        let comfort = self.comfort.assess(snapshot.temperature, snapshot.humidity);

        let severity = if comfort.is_doubly_uncomfortable() {
            Some(AlertSeverity::Warning)
        } else if comfort.is_uncomfortable() {
            Some(AlertSeverity::Notice)
        } else {
            None
        };

        self.push_if_alert(alerts, AlertKind::Comfort, severity, now, |_| {
            (comfort.headline().to_string(), comfort.advice().to_string())
        });
    }

    pub fn record_fetch_error(&mut self, error: &str) -> Option<AlertNotification> {
        self.record_fetch_error_at(error, Instant::now())
    }

    pub fn record_fetch_error_at(
        &mut self,
        error: &str,
        now: Instant,
    ) -> Option<AlertNotification> {
        if !self.enabled {
            return None;
        }

        self.fetch_failures = self.fetch_failures.saturating_add(1);
        if self.fetch_failures < OFFLINE_AFTER_FAILURES {
            return None;
        }

        self.make_alert(
            AlertKind::DeviceOffline,
            AlertSeverity::Warning,
            now,
            "AirGradient device is unreachable",
            &format!("No fresh sensor data after repeated attempts. Last error: {error}"),
        )
    }

    /// Apply one rule's verdict to the running alert state.
    ///
    /// A reading that is fine clears the rule's history, so a value that dips in
    /// and out of the threshold has to cross it for `ALERT_CONSECUTIVE_READINGS`
    /// readings in a row again before it alerts.
    fn push_if_alert(
        &mut self,
        alerts: &mut Vec<AlertNotification>,
        kind: AlertKind,
        severity: Option<AlertSeverity>,
        now: Instant,
        text: impl FnOnce(AlertSeverity) -> (String, String),
    ) {
        let Some(severity) = severity else {
            self.consecutive.remove(&kind);
            self.active_severity.remove(&kind);
            return;
        };

        let count = self.consecutive.entry(kind).or_insert(0);
        *count = count.saturating_add(1);
        if *count < ALERT_CONSECUTIVE_READINGS {
            return;
        }

        let (title, body) = text(severity);
        if let Some(alert) = self.make_alert(kind, severity, now, &title, &body) {
            alerts.push(alert);
        }
    }

    fn make_alert(
        &mut self,
        kind: AlertKind,
        severity: AlertSeverity,
        now: Instant,
        title: &str,
        body: &str,
    ) -> Option<AlertNotification> {
        let escalated = self
            .active_severity
            .get(&kind)
            .is_some_and(|active| severity > *active);
        let cooled_down = self.last_sent.get(&kind).is_none_or(|last| {
            now.saturating_duration_since(*last).as_secs() >= ALERT_COOLDOWN_SECS
        });

        if !(escalated || cooled_down) {
            return None;
        }

        self.active_severity.insert(kind, severity);
        self.last_sent.insert(kind, now);
        Some(AlertNotification {
            id: format!("airgradient-{kind:?}").to_lowercase(),
            title: title.to_string(),
            body: body.to_string(),
            severity,
        })
    }
}

/// One thing the monitor watches.
///
/// Every watched reading needs the same four facts: which alert it is, where to
/// find the value in a snapshot, how severe that value is, and what to tell the
/// user. Writing them as a table means `evaluate_at` is a loop rather than seven
/// near-identical blocks, and watching one more reading is one entry here.
struct AlertRule {
    kind: AlertKind,
    /// How to read this rule's value out of a snapshot.
    read: fn(&AirMeasureSnapshot) -> Option<f32>,
    /// How bad that value is, or `None` when it needs no alert.
    classify: fn(f32) -> Option<AlertSeverity>,
    /// Notification title and body for a given severity.
    text: fn(AlertSeverity) -> (&'static str, &'static str),
}

/// Every reading the monitor watches, in the order alerts are reported.
const RULES: &[AlertRule] = &[
    AlertRule {
        kind: AlertKind::Co2,
        read: |snapshot| snapshot.co2,
        classify: classify_co2,
        text: |severity| {
            match severity {
            AlertSeverity::Notice => (
                "CO2 is above 800 ppm",
                "Ventilation may be low. Open a window or increase fresh-air ventilation.",
            ),
            AlertSeverity::Warning => (
                "CO2 is high",
                "CO2 is above 1200 ppm. Ventilate now if possible or reduce room occupancy.",
            ),
            AlertSeverity::Critical => (
                "CO2 is very high",
                "CO2 is above 2000 ppm. Leave briefly or improve ventilation immediately if possible.",
            ),
        }
        },
    },
    AlertRule {
        kind: AlertKind::Aqi,
        read: |snapshot| snapshot.aqi,
        classify: classify_aqi,
        text: |severity| match severity {
            AlertSeverity::Notice => (
                "AQI is unhealthy for sensitive groups",
                "Reduce exposure if you are sensitive. Consider filtration or source control.",
            ),
            AlertSeverity::Warning => (
                "AQI is unhealthy",
                "Air quality may affect everyone. Reduce pollutant sources and improve filtration.",
            ),
            AlertSeverity::Critical => (
                "AQI is very unhealthy",
                "Limit exposure. Use filtration and avoid adding indoor pollution sources.",
            ),
        },
    },
    AlertRule {
        kind: AlertKind::Pm25,
        read: |snapshot| snapshot.pm25,
        classify: classify_pm25,
        text: |severity| {
            match severity {
            AlertSeverity::Notice => (
                "PM2.5 is elevated",
                "Run an air purifier or improve HVAC filtration; reduce cooking, smoke, or dust sources.",
            ),
            AlertSeverity::Warning => (
                "PM2.5 is high",
                "Particle pollution is high. Use filtration and avoid activities that create particles.",
            ),
            AlertSeverity::Critical => (
                "PM2.5 is very high",
                "Limit exposure and use strong filtration. Check whether outdoor smoke or indoor sources are present.",
            ),
        }
        },
    },
    AlertRule {
        kind: AlertKind::Tvoc,
        read: |snapshot| snapshot.tvoc,
        classify: classify_tvoc,
        text: |severity| {
            match severity {
            AlertSeverity::Notice | AlertSeverity::Warning => (
                "VOC level is elevated",
                "Ventilate and check recent sources: cleaning products, paint, adhesives, or hobby materials.",
            ),
            AlertSeverity::Critical => (
                "VOC level is high",
                "Ventilate now and remove or seal likely chemical sources if safe to do so.",
            ),
        }
        },
    },
    AlertRule {
        kind: AlertKind::Nox,
        read: |snapshot| snapshot.nox,
        classify: classify_nox,
        text: |severity| {
            match severity {
            AlertSeverity::Notice | AlertSeverity::Warning => (
                "NOx level is elevated",
                "If cooking or using combustion appliances, use exhaust ventilation or open a window.",
            ),
            AlertSeverity::Critical => (
                "NOx level is high",
                "Increase ventilation and check combustion sources such as gas cooking or heaters.",
            ),
        }
        },
    },
    AlertRule {
        kind: AlertKind::HumidityHigh,
        read: |snapshot| snapshot.humidity,
        classify: classify_damp,
        text: |_| {
            (
                "Humidity is very high",
                "Dehumidify or ventilate now and check for dampness or leaks: sustained \
                 humidity above 75% risks condensation and mold.",
            )
        },
    },
];

fn classify_co2(value: f32) -> Option<AlertSeverity> {
    if value > 2000.0 {
        Some(AlertSeverity::Critical)
    } else if value > 1200.0 {
        Some(AlertSeverity::Warning)
    } else if value > 800.0 {
        Some(AlertSeverity::Notice)
    } else {
        None
    }
}

fn classify_aqi(value: f32) -> Option<AlertSeverity> {
    if value > 200.0 {
        Some(AlertSeverity::Critical)
    } else if value > 150.0 {
        Some(AlertSeverity::Warning)
    } else if value > 100.0 {
        Some(AlertSeverity::Notice)
    } else {
        None
    }
}

fn classify_pm25(value: f32) -> Option<AlertSeverity> {
    if value > 150.0 {
        Some(AlertSeverity::Critical)
    } else if value > 55.0 {
        Some(AlertSeverity::Warning)
    } else if value > 35.0 {
        Some(AlertSeverity::Notice)
    } else {
        None
    }
}

fn classify_tvoc(value: f32) -> Option<AlertSeverity> {
    if value > 660.0 {
        Some(AlertSeverity::Critical)
    } else if value > 220.0 {
        Some(AlertSeverity::Warning)
    } else {
        None
    }
}

fn classify_nox(value: f32) -> Option<AlertSeverity> {
    if value > 150.0 {
        Some(AlertSeverity::Critical)
    } else if value > 50.0 {
        Some(AlertSeverity::Warning)
    } else {
        None
    }
}

/// Humidity high enough to risk condensation and mold, whatever the user finds
/// comfortable.
///
/// Merely damp or merely dry air is a comfort matter, and the user's own
/// humidity range decides that now — alerting on a fixed band as well would
/// send two notifications about one reading, one of them ignoring the
/// preference the user just set. What stays here is the part that is not a
/// preference: above roughly 75% relative humidity, surfaces stay wet long
/// enough for mold, and that is worth saying even to someone who likes humid
/// air.
fn classify_damp(value: f32) -> Option<AlertSeverity> {
    if value > 75.0 {
        Some(AlertSeverity::Critical)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{AlertMonitor, AlertSeverity};
    use crate::sensors::comfort::{ComfortRange, ComfortThresholds, Dimension};
    use crate::sensors::AirMeasureSnapshot;
    use std::time::{Duration, Instant};

    /// A monitor with notifications on and the default comfort ranges.
    fn monitor() -> AlertMonitor {
        AlertMonitor::new(true, ComfortThresholds::default())
    }

    /// A snapshot of just temperature and humidity.
    fn room(temperature: f32, humidity: f32) -> AirMeasureSnapshot {
        AirMeasureSnapshot {
            temperature: Some(temperature),
            humidity: Some(humidity),
            ..Default::default()
        }
    }

    #[test]
    fn waits_for_consecutive_bad_readings_before_alerting() {
        let mut monitor = monitor();
        let snapshot = AirMeasureSnapshot {
            co2: Some(900.0),
            ..Default::default()
        };

        assert!(monitor.evaluate(&snapshot).is_empty());

        let alerts = monitor.evaluate(&snapshot);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Notice);
        assert_eq!(alerts[0].title, "CO2 is above 800 ppm");
    }

    #[test]
    fn suppresses_repeated_alert_until_cooldown_or_escalation() {
        let mut monitor = monitor();
        let notice = AirMeasureSnapshot {
            co2: Some(900.0),
            ..Default::default()
        };
        let critical = AirMeasureSnapshot {
            co2: Some(2100.0),
            ..Default::default()
        };

        assert!(monitor.evaluate(&notice).is_empty());
        assert_eq!(monitor.evaluate(&notice).len(), 1);
        assert!(monitor.evaluate(&notice).is_empty());

        let alerts = monitor.evaluate(&critical);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Critical);
    }

    #[test]
    fn records_offline_alert_after_repeated_fetch_errors() {
        let mut monitor = monitor();

        assert!(monitor.record_fetch_error("timeout").is_none());
        assert!(monitor.record_fetch_error("timeout").is_none());

        let alert = monitor
            .record_fetch_error("connection refused")
            .expect("third fetch failure should alert");
        assert_eq!(alert.severity, AlertSeverity::Warning);
        assert!(alert.body.contains("connection refused"));
    }

    #[test]
    fn cooldown_allows_repeated_alert_after_time_passes() {
        let mut monitor = monitor();
        let snapshot = AirMeasureSnapshot {
            pm25: Some(80.0),
            ..Default::default()
        };
        let start = Instant::now();

        assert!(monitor.evaluate_at(&snapshot, start).is_empty());
        assert_eq!(monitor.evaluate_at(&snapshot, start).len(), 1);
        assert!(monitor
            .evaluate_at(&snapshot, start + Duration::from_secs(60))
            .is_empty());

        let alerts = monitor.evaluate_at(&snapshot, start + Duration::from_secs(20 * 60));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Warning);
    }

    #[test]
    fn fetch_error_cooldown_uses_injected_time() {
        let mut monitor = monitor();
        let start = Instant::now();

        assert!(monitor.record_fetch_error_at("timeout", start).is_none());
        assert!(monitor.record_fetch_error_at("timeout", start).is_none());
        assert!(monitor.record_fetch_error_at("timeout", start).is_some());
        assert!(monitor
            .record_fetch_error_at("timeout", start + Duration::from_secs(60))
            .is_none());
        assert!(monitor
            .record_fetch_error_at("timeout", start + Duration::from_secs(20 * 60))
            .is_some());
    }

    #[test]
    fn a_warm_humid_room_alerts_about_the_pair() {
        let mut monitor = monitor();
        let muggy = room(29.0, 72.0);

        assert!(monitor.evaluate(&muggy).is_empty());
        let alerts = monitor.evaluate(&muggy);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].title, "The room is warm and humid");
        assert!(
            alerts[0].body.contains("air conditioning"),
            "the advice should name what fixes it: {}",
            alerts[0].body
        );
        assert_eq!(
            alerts[0].severity,
            AlertSeverity::Warning,
            "both readings out of range is worse than one"
        );
    }

    #[test]
    fn one_reading_out_of_range_is_only_a_notice() {
        let mut monitor = monitor();
        let dry = room(22.0, 25.0);

        assert!(monitor.evaluate(&dry).is_empty());
        let alerts = monitor.evaluate(&dry);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].title, "The room is dry");
        assert_eq!(alerts[0].severity, AlertSeverity::Notice);
    }

    #[test]
    fn a_comfortable_room_sends_nothing() {
        let mut monitor = monitor();
        let pleasant = room(22.0, 50.0);

        assert!(monitor.evaluate(&pleasant).is_empty());
        assert!(monitor.evaluate(&pleasant).is_empty());
    }

    #[test]
    fn a_room_turning_muggy_escalates_without_waiting_for_the_cooldown() {
        let mut monitor = monitor();
        let start = Instant::now();
        let warm = room(29.0, 50.0);
        let muggy = room(29.0, 72.0);

        assert!(monitor.evaluate_at(&warm, start).is_empty());
        assert_eq!(monitor.evaluate_at(&warm, start).len(), 1);

        let alerts = monitor.evaluate_at(&muggy, start + Duration::from_secs(60));

        assert_eq!(alerts.len(), 1, "a worse room should not wait 20 minutes");
        assert_eq!(alerts[0].severity, AlertSeverity::Warning);
        assert_eq!(alerts[0].title, "The room is warm and humid");
    }

    #[test]
    fn comfort_alerts_respect_the_users_own_ranges() {
        let mut monitor = AlertMonitor::new(
            true,
            ComfortThresholds {
                temperature: ComfortRange::new(Dimension::Temperature, 15.0, 30.0)
                    .expect("valid range"),
                humidity: ComfortRange::new(Dimension::Humidity, 20.0, 80.0).expect("valid range"),
            },
        );
        // Uncomfortable under the defaults, fine for someone who likes it warm.
        let warm = room(29.0, 72.0);

        assert!(monitor.evaluate(&warm).is_empty());
        assert!(monitor.evaluate(&warm).is_empty());
    }

    #[test]
    fn widening_the_ranges_drops_the_run_of_bad_readings() {
        let mut monitor = monitor();
        let warm = room(29.0, 50.0);

        assert!(monitor.evaluate(&warm).is_empty());

        // The user decides 29 °C is fine after all. The reading already counted
        // must not alert under the new range.
        monitor.set_comfort(ComfortThresholds {
            temperature: ComfortRange::new(Dimension::Temperature, 15.0, 30.0)
                .expect("valid range"),
            humidity: ComfortThresholds::default().humidity,
        });

        assert!(monitor.evaluate(&warm).is_empty());
        assert!(monitor.evaluate(&warm).is_empty());
    }

    #[test]
    fn a_device_without_a_humidity_sensor_is_judged_on_temperature_alone() {
        let mut monitor = monitor();
        let hot = AirMeasureSnapshot {
            temperature: Some(31.0),
            ..Default::default()
        };

        assert!(monitor.evaluate(&hot).is_empty());
        let alerts = monitor.evaluate(&hot);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].title, "The room is warm");
        assert_eq!(alerts[0].severity, AlertSeverity::Notice);
    }

    #[test]
    fn damp_air_still_warns_about_mold_on_top_of_the_comfort_alert() {
        // Above 75% is a condensation risk whatever the user finds comfortable,
        // so this one alert is not replaced by the comfort ranges.
        let mut monitor = monitor();
        let damp = room(22.0, 82.0);

        assert!(monitor.evaluate(&damp).is_empty());
        let alerts = monitor.evaluate(&damp);

        let titles: Vec<&str> = alerts.iter().map(|alert| alert.title.as_str()).collect();
        assert!(
            titles.contains(&"Humidity is very high"),
            "expected the mold warning, got {titles:?}"
        );
        assert!(titles.contains(&"The room is humid"));
    }

    #[test]
    fn merely_humid_air_alerts_once_not_twice() {
        // 72% used to trip a fixed "Humidity is high" notice as well, which meant
        // two notifications about one reading.
        let mut monitor = monitor();
        let humid = room(22.0, 72.0);

        assert!(monitor.evaluate(&humid).is_empty());
        let alerts = monitor.evaluate(&humid);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].title, "The room is humid");
    }

    #[test]
    fn disabled_monitor_drops_alert_state() {
        let mut monitor = monitor();
        let snapshot = AirMeasureSnapshot {
            pm25: Some(200.0),
            ..Default::default()
        };

        assert!(monitor.evaluate(&snapshot).is_empty());
        monitor.set_enabled(false);
        monitor.set_enabled(true);

        assert!(monitor.evaluate(&snapshot).is_empty());
    }
}
