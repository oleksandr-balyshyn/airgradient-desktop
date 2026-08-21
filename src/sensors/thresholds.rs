//! Sensor threshold status mapping.
//!
//! These helpers classify measurements into semantic palette slots. The GTK UI
//! converts those slots into concrete `gdk::RGBA` values at the presentation
//! boundary.

use super::aqi::AqiBand;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum StatusColor {
    Green,
    Yellow,
    Orange,
    Red,
    Purple,
    Gray,
}

pub fn co2_status_color(value: f32) -> StatusColor {
    // CO2 thresholds: excellent/acceptable/moderate/high.
    match value {
        x if x < 800.0 => StatusColor::Green,
        x if x < 1200.0 => StatusColor::Yellow,
        x if x < 2000.0 => StatusColor::Orange,
        _ => StatusColor::Red,
    }
}

pub fn pm25_status_color(value: f32) -> StatusColor {
    // The cut points are the tops of the Good, Moderate and "unhealthy for
    // sensitive groups" bands of `sensors::aqi::PM25_BANDS` (EPA, revised 2024),
    // so the PM2.5 card and the AQI card cannot disagree about the same reading.
    // The four slots stay Green/Yellow/Orange/Red rather than routing through
    // `aqi_status_color`, which would add Purple and Gray to this card.
    match value {
        x if x < 9.1 => StatusColor::Green,
        x if x < 35.5 => StatusColor::Yellow,
        x if x < 55.5 => StatusColor::Orange,
        _ => StatusColor::Red,
    }
}

pub fn tvoc_status_color(value: f32) -> StatusColor {
    match value {
        x if x < 65.0 => StatusColor::Green,
        x if x < 220.0 => StatusColor::Yellow,
        x if x < 660.0 => StatusColor::Orange,
        _ => StatusColor::Red,
    }
}

pub fn nox_status_color(value: f32) -> StatusColor {
    match value {
        x if x < 20.0 => StatusColor::Green,
        x if x < 50.0 => StatusColor::Yellow,
        x if x < 150.0 => StatusColor::Orange,
        _ => StatusColor::Red,
    }
}

pub fn aqi_status_color(value: f32) -> StatusColor {
    // The band boundaries themselves belong to `sensors::aqi`, which owns the
    // scale; this function only says which palette slot each band gets.
    match AqiBand::of(value) {
        AqiBand::Good => StatusColor::Green,
        AqiBand::Moderate => StatusColor::Yellow,
        AqiBand::UnhealthyForSensitiveGroups => StatusColor::Orange,
        AqiBand::Unhealthy => StatusColor::Red,
        AqiBand::VeryUnhealthy => StatusColor::Purple,
        AqiBand::Hazardous => StatusColor::Gray,
    }
}

#[cfg(test)]
mod tests {
    use super::{aqi_status_color, co2_status_color, pm25_status_color, StatusColor};

    #[test]
    fn co2_thresholds_classify_boundary_values() {
        assert_eq!(co2_status_color(799.9), StatusColor::Green);
        assert_eq!(co2_status_color(800.0), StatusColor::Yellow);
        assert_eq!(co2_status_color(1200.0), StatusColor::Orange);
        assert_eq!(co2_status_color(2000.0), StatusColor::Red);
    }

    #[test]
    fn pm25_thresholds_classify_boundary_values() {
        assert_eq!(pm25_status_color(9.0), StatusColor::Green);
        assert_eq!(pm25_status_color(9.1), StatusColor::Yellow);
        // 12.0 was Green under the pre-2024 breakpoints. It is Moderate on the
        // AQI scale, so it is Yellow here too — see
        // `aqi::tests::the_revised_bands_are_stricter_than_the_old_ones`.
        assert_eq!(pm25_status_color(12.0), StatusColor::Yellow);
        assert_eq!(pm25_status_color(35.4), StatusColor::Yellow);
        assert_eq!(pm25_status_color(35.5), StatusColor::Orange);
        assert_eq!(pm25_status_color(55.4), StatusColor::Orange);
        assert_eq!(pm25_status_color(55.5), StatusColor::Red);
    }

    #[test]
    fn aqi_thresholds_cover_full_palette() {
        assert_eq!(aqi_status_color(50.0), StatusColor::Green);
        assert_eq!(aqi_status_color(100.0), StatusColor::Yellow);
        assert_eq!(aqi_status_color(150.0), StatusColor::Orange);
        assert_eq!(aqi_status_color(200.0), StatusColor::Red);
        assert_eq!(aqi_status_color(300.0), StatusColor::Purple);
        assert_eq!(aqi_status_color(301.0), StatusColor::Gray);
    }
}
