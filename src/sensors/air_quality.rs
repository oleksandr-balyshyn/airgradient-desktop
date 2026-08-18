//! AirGradient local-server payload parsing.
//!
//! AirGradient devices can expose slightly different field names depending on
//! hardware model and firmware. The parser accepts several candidate keys for
//! each measurement and returns a normalized `AirMeasureSnapshot` for the UI.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Unit a gas sensor reports in.
///
/// AirGradient's own firmware exposes VOC and NOx as a unitless SGP sensor
/// *index*, while some compatible local-server implementations report a
/// concentration in parts per billion. The reading means different things in
/// each case, so the unit travels with the value instead of being guessed at
/// display time.
///
/// This is an enum rather than a `&'static str` because snapshots are written to
/// the history file, and a borrowed string cannot be deserialized back.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GasUnit {
    /// Unitless sensor index.
    Index,
    /// Parts per billion.
    Ppb,
}

impl GasUnit {
    /// Short label shown on a card.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Ppb => "ppb",
        }
    }
}

/// One parsed measurement response from `/measures/current`.
///
/// Every sensor value is optional because not all AirGradient models expose the
/// same fields. In the UI, `None` is displayed as `--`; it is not treated as
/// zero because zero can be a valid real measurement.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AirMeasureSnapshot {
    pub temperature: Option<f32>,
    pub humidity: Option<f32>,
    pub aqi: Option<f32>,
    pub co2: Option<f32>,
    pub nox: Option<f32>,
    pub nox_unit: Option<GasUnit>,
    pub tvoc: Option<f32>,
    pub tvoc_unit: Option<GasUnit>,
    pub pm1: Option<f32>,
    pub pm25: Option<f32>,
    pub pm10: Option<f32>,
    pub pm003_count: Option<f32>,
}

pub fn parse_air_measurements(raw: &Value) -> AirMeasureSnapshot {
    // AirGradient's current firmware exposes `noxIndex`, but accepting common
    // alternatives keeps the app usable if payload names change or if users test
    // with compatible local-server implementations.
    let nox = extract_measurement_value(raw, &["nox", "no2", "nox_ppb"])
        .or_else(|| extract_measurement_value(raw, &["noxIndex", "nox_index"]));
    let nox_unit = gas_unit(nox, raw, &["noxIndex", "nox_index"]);

    let tvoc = extract_measurement_value(raw, &["tvoc", "tvoc_ppb", "tvoc_ppm", "voc"])
        .or_else(|| extract_measurement_value(raw, &["tvocIndex", "tvoc_index"]));
    let tvoc_unit = gas_unit(tvoc, raw, &["tvocIndex", "tvoc_index"]);
    let pm25 = extract_measurement_value(raw, &["pm02", "pm2_5", "pm25", "pm2.5"]);

    AirMeasureSnapshot {
        // Prefer compensated temperature/humidity when available because the
        // device can apply model-specific correction before exposing values.
        temperature: extract_measurement_value(
            raw,
            &[
                "atmpCompensated",
                "temperatureCompensated",
                "temperature_compensated",
                "atmp",
                "temperature",
                "temp",
                "temp_c",
                "temperature_c",
                "temperatureC",
            ],
        ),
        humidity: extract_measurement_value(
            raw,
            &[
                "rhumCompensated",
                "humidityCompensated",
                "humidity_compensated",
                "rhum",
                "humidity",
                "hum",
                "relative_humidity",
                "rh",
                "humidity_pct",
            ],
        ),
        aqi: extract_measurement_value(raw, &["aqi", "air_quality_index"])
            .or_else(|| pm25.map(pm25_to_us_aqi)),
        co2: extract_measurement_value(raw, &["rco2", "co2", "co2_ppm"]),
        nox,
        nox_unit,
        tvoc,
        tvoc_unit,
        pm1: extract_measurement_value(raw, &["pm1", "pm1.0", "pm01", "pm_1_0"]),
        pm25,
        pm10: extract_measurement_value(raw, &["pm10", "pm10_0"]),
        pm003_count: extract_measurement_value(raw, &["pm003Count", "pm003_count", "pm0_3_count"]),
    }
}

/// Decide which unit a gas reading is in.
///
/// If the payload used one of the `*Index` key names the value is an index;
/// anything else is treated as a concentration in ppb. A missing reading has no
/// unit at all.
fn gas_unit(value: Option<f32>, raw: &Value, index_keys: &[&str]) -> Option<GasUnit> {
    value.map(|_| {
        if has_any_key(raw, index_keys) {
            GasUnit::Index
        } else {
            GasUnit::Ppb
        }
    })
}

/// Return the first numeric value found under any candidate key.
///
/// This searches top-level keys first, then recursively searches nested objects
/// and arrays. That makes the parser tolerant of payloads that wrap sensor
/// values in a `measurements` object.
pub fn extract_measurement_value(raw: &Value, candidates: &[&str]) -> Option<f32> {
    candidates.iter().find_map(|name| {
        if let Some(value) = raw.get(*name).and_then(as_f32) {
            return Some(value);
        }

        let lower = name.to_lowercase();
        if let Some(value) = raw.get(lower.as_str()).and_then(as_f32) {
            return Some(value);
        }

        raw.as_object().and_then(|obj| {
            obj.values()
                .find_map(|value| find_nested_key(value, name))
                .or_else(|| {
                    obj.values()
                        .find_map(|value| find_nested_key(value, lower.as_str()))
                })
        })
    })
}

fn as_f32(v: &Value) -> Option<f32> {
    match v {
        Value::Number(num) => num.to_string().parse::<f32>().ok(),
        Value::String(raw) => raw.parse::<f32>().ok(),
        _ => None,
    }
}

fn find_nested_key(raw: &Value, key: &str) -> Option<f32> {
    match raw {
        Value::Object(object) => {
            if let Some(value) = object.get(key) {
                return as_f32(value);
            }
            let lower = key.to_lowercase();
            if let Some(value) = object.get(&lower) {
                return as_f32(value);
            }
            for value in object.values() {
                if let Some(found) = find_nested_key(value, key) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => {
            for item in items {
                if let Some(found) = find_nested_key(item, key) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn has_any_key(raw: &Value, candidates: &[&str]) -> bool {
    candidates.iter().any(|key| {
        let lower = key.to_lowercase();
        has_nested_key(raw, key) || has_nested_key(raw, lower.as_str())
    })
}

fn has_nested_key(raw: &Value, key: &str) -> bool {
    match raw {
        Value::Object(object) => {
            object.contains_key(key) || object.values().any(|value| has_nested_key(value, key))
        }
        Value::Array(items) => items.iter().any(|value| has_nested_key(value, key)),
        _ => false,
    }
}

/// Convert a PM2.5 concentration in ug/m3 to a US EPA Air Quality Index value.
///
/// Used only when the device does not report an AQI directly, which is the
/// common case: current AirGradient firmware exposes particulate concentrations
/// and no AQI field, so this is where almost every AQI the app displays is
/// produced.
fn pm25_to_us_aqi(pm25: f32) -> f32 {
    /// EPA breakpoints as (concentration low, concentration high, AQI low, AQI
    /// high). Each row maps a concentration band onto an AQI band, and the value
    /// is interpolated linearly between the two.
    const BREAKPOINTS: [(f32, f32, f32, f32); 6] = [
        (0.0, 12.0, 0.0, 50.0),
        (12.1, 35.4, 51.0, 100.0),
        (35.5, 55.4, 101.0, 150.0),
        (55.5, 150.4, 151.0, 200.0),
        (150.5, 250.4, 201.0, 300.0),
        (250.5, 500.4, 301.0, 500.0),
    ];
    /// AQI reported for anything above the top of the table.
    const MAX_AQI: f32 = 500.0;
    /// The EPA publishes its breakpoints to one decimal place.
    const CONCENTRATION_PRECISION: f32 = 10.0;

    if pm25 <= 0.0 {
        return 0.0;
    }

    // The bands are contiguous only at one decimal place: the first ends at 12.0
    // and the second begins at 12.1, so a raw reading of 12.05 sits between them
    // and belongs to neither. Truncating to the published precision first, as the
    // EPA method specifies, is what closes those gaps.
    let concentration = (pm25 * CONCENTRATION_PRECISION).floor() / CONCENTRATION_PRECISION;

    for (c_low, c_high, aqi_low, aqi_high) in BREAKPOINTS {
        if concentration <= c_high {
            // Clamping to the band's own floor makes falling through impossible
            // even if float rounding leaves a truncated value just below it.
            let inside_band = concentration.max(c_low);
            return ((aqi_high - aqi_low) / (c_high - c_low)) * (inside_band - c_low) + aqi_low;
        }
    }

    MAX_AQI
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{parse_air_measurements, pm25_to_us_aqi, GasUnit};

    #[test]
    fn parses_airgradient_local_server_payload() {
        let payload = json!({
            "wifi": -46,
            "serialno": "ecda3b1eaaaf",
            "rco2": 447,
            "pm01": 3,
            "pm02": 7,
            "pm10": 8,
            "pm003Count": 442,
            "atmp": 25.87,
            "atmpCompensated": 24.47,
            "rhum": 43,
            "rhumCompensated": 49,
            "tvocIndex": 100,
            "tvocRaw": 33051,
            "noxIndex": 1,
            "noxRaw": 16307
        });

        let snapshot = parse_air_measurements(&payload);

        assert_eq!(snapshot.co2, Some(447.0));
        assert_eq!(snapshot.pm1, Some(3.0));
        assert_eq!(snapshot.pm25, Some(7.0));
        assert_eq!(snapshot.pm10, Some(8.0));
        assert_eq!(snapshot.pm003_count, Some(442.0));
        assert_eq!(snapshot.temperature, Some(24.47));
        assert_eq!(snapshot.humidity, Some(49.0));
        assert_eq!(snapshot.tvoc, Some(100.0));
        assert_eq!(snapshot.tvoc_unit, Some(GasUnit::Index));
        assert_eq!(snapshot.nox, Some(1.0));
        assert_eq!(snapshot.nox_unit, Some(GasUnit::Index));
        assert_eq!(snapshot.aqi.map(|value| value.round()), Some(29.0));
    }

    #[test]
    fn parses_nested_payloads_with_numeric_strings() {
        let payload = json!({
            "device": {
                "measurements": [
                    {
                        "rco2": "812",
                        "pm02": "13.2",
                        "atmpCompensated": "22.4",
                        "rhumCompensated": "45.5"
                    },
                    {
                        "tvocIndex": "110",
                        "noxIndex": "3",
                        "pm003Count": "1200"
                    }
                ]
            }
        });

        let snapshot = parse_air_measurements(&payload);

        assert_eq!(snapshot.co2, Some(812.0));
        assert_eq!(snapshot.pm25, Some(13.2));
        assert_eq!(snapshot.temperature, Some(22.4));
        assert_eq!(snapshot.humidity, Some(45.5));
        assert_eq!(snapshot.tvoc, Some(110.0));
        assert_eq!(snapshot.tvoc_unit, Some(GasUnit::Index));
        assert_eq!(snapshot.nox, Some(3.0));
        assert_eq!(snapshot.nox_unit, Some(GasUnit::Index));
        assert_eq!(snapshot.pm003_count, Some(1200.0));
    }

    /// Every concentration must land in some band.
    ///
    /// The EPA bands are contiguous only to one decimal place, so a reading that
    /// falls between two of them -- 12.05 sits above the first band's 12.0 and
    /// below the second's 12.1 -- used to match no row at all and drop through to
    /// the "off the top of the table" result of 500, the worst AQI there is. A
    /// clean 12.05 ug/m3 was reported as hazardous, which also fired a critical
    /// air-quality alert.
    #[test]
    fn concentrations_between_published_bands_stay_in_the_lower_band() {
        for (concentration, expected) in [
            (12.05, 50.0),
            (35.45, 100.0),
            (55.45, 150.0),
            (150.45, 200.0),
            (250.45, 300.0),
        ] {
            assert_eq!(
                pm25_to_us_aqi(concentration),
                expected,
                "pm2.5 of {concentration} should read as AQI {expected}"
            );
        }
    }

    #[test]
    fn aqi_never_exceeds_the_band_a_reading_belongs_to() {
        // Sweep the whole plausible sensor range in 0.01 steps: no input may
        // produce an AQI that jumps outside the band its neighbours are in.
        let mut previous = 0.0_f32;
        for step in 0..60_000 {
            let concentration = step as f32 * 0.01;
            let aqi = pm25_to_us_aqi(concentration);

            assert!(
                (0.0..=500.0).contains(&aqi),
                "pm2.5 of {concentration} produced an out-of-range AQI of {aqi}"
            );
            assert!(
                aqi >= previous - 0.5,
                "AQI fell from {previous} to {aqi} as pm2.5 rose to {concentration}"
            );
            previous = aqi;
        }
    }

    #[test]
    fn aqi_covers_the_documented_boundaries() {
        assert_eq!(pm25_to_us_aqi(0.0), 0.0);
        assert_eq!(
            pm25_to_us_aqi(-5.0),
            0.0,
            "a negative reading is not hazardous"
        );
        assert_eq!(pm25_to_us_aqi(12.0), 50.0);
        assert_eq!(pm25_to_us_aqi(12.1), 51.0);
        assert_eq!(pm25_to_us_aqi(35.4), 100.0);
        assert_eq!(pm25_to_us_aqi(500.4), 500.0);
        assert_eq!(
            pm25_to_us_aqi(900.0),
            500.0,
            "above the table is the maximum"
        );
    }
}
