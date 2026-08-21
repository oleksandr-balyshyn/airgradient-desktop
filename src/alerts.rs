//! Air-quality alert policy.
//!
//! This module decides when sensor measurements should become notifications.
//! It deliberately does not know about GTK, D-Bus, or desktop notification
//! APIs; the UI adapter owns delivery.

use std::collections::HashMap;
use std::time::Instant;

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
    HumidityLow,
    HumidityHigh,
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
    consecutive: HashMap<AlertKind, u8>,
    active_severity: HashMap<AlertKind, AlertSeverity>,
    last_sent: HashMap<AlertKind, Instant>,
    fetch_failures: u8,
}

impl AlertMonitor {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            consecutive: HashMap::new(),
            active_severity: HashMap::new(),
            last_sent: HashMap::new(),
            fetch_failures: 0,
        }
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
            self.push_if_alert(&mut alerts, rule, severity, now);
        }
        alerts
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
        rule: &AlertRule,
        severity: Option<AlertSeverity>,
        now: Instant,
    ) {
        let Some(severity) = severity else {
            self.consecutive.remove(&rule.kind);
            self.active_severity.remove(&rule.kind);
            return;
        };

        let count = self.consecutive.entry(rule.kind).or_insert(0);
        *count = count.saturating_add(1);
        if *count < ALERT_CONSECUTIVE_READINGS {
            return;
        }

        let (title, body) = (rule.text)(severity);
        if let Some(alert) = self.make_alert(rule.kind, severity, now, title, body) {
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
        kind: AlertKind::HumidityLow,
        read: |snapshot| snapshot.humidity,
        classify: classify_humidity_low,
        text: |_| {
            (
                "Humidity is low",
                "Air is dry. Consider humidification if the room feels uncomfortable.",
            )
        },
    },
    AlertRule {
        kind: AlertKind::HumidityHigh,
        read: |snapshot| snapshot.humidity,
        classify: classify_humidity_high,
        text: |severity| match severity {
            AlertSeverity::Notice | AlertSeverity::Warning => (
                "Humidity is high",
                "Ventilate or dehumidify to reduce dampness and mold risk.",
            ),
            AlertSeverity::Critical => (
                "Humidity is very high",
                "Dehumidify or ventilate now and check for dampness or leaks.",
            ),
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

fn classify_humidity_low(value: f32) -> Option<AlertSeverity> {
    if value < 30.0 {
        Some(AlertSeverity::Notice)
    } else {
        None
    }
}

fn classify_humidity_high(value: f32) -> Option<AlertSeverity> {
    if value > 75.0 {
        Some(AlertSeverity::Critical)
    } else if value > 65.0 {
        Some(AlertSeverity::Notice)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{AlertMonitor, AlertSeverity};
    use crate::sensors::AirMeasureSnapshot;
    use std::time::{Duration, Instant};

    #[test]
    fn waits_for_consecutive_bad_readings_before_alerting() {
        let mut monitor = AlertMonitor::new(true);
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
        let mut monitor = AlertMonitor::new(true);
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
        let mut monitor = AlertMonitor::new(true);

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
        let mut monitor = AlertMonitor::new(true);
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
        let mut monitor = AlertMonitor::new(true);
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
    fn disabled_monitor_drops_alert_state() {
        let mut monitor = AlertMonitor::new(true);
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
