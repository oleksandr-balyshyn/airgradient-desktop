//! AirGradient local-server payload parsing.
//!
//! AirGradient devices can expose slightly different field names depending on
//! hardware model and firmware. The parser accepts several candidate keys for
//! each measurement and returns a normalized `AirMeasureSnapshot` for the UI.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::aqi::pm25_to_aqi;

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
    let (nox, nox_unit) = extract_gas(raw, &["nox", "no2", "nox_ppb"], &["noxIndex", "nox_index"]);

    let (tvoc, tvoc_unit) = extract_gas(
        raw,
        &["tvoc", "tvoc_ppb", "tvoc_ppm", "voc"],
        &["tvocIndex", "tvoc_index"],
    );
    // Prefer the device's compensated particulate reading for the same reason
    // temperature and humidity do below: AirGradient applies its batch and EPA
    // corrections to it, and it is what the vendor's own dashboard reports. The
    // raw `pm02` is quantized to whole micrograms on current firmware, so a room
    // at 3.1 ug/m3 reports 0 there and would show an AQI of 0.
    let pm25 = extract_measurement_value(
        raw,
        &[
            "pm02Compensated",
            "pm25Compensated",
            "pm02_compensated",
            "pm02",
            "pm2_5",
            "pm25",
            "pm2.5",
        ],
    );

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
            .or_else(|| pm25.map(pm25_to_aqi)),
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

/// Read a gas reading together with the unit it is expressed in.
///
/// The concentration keys are tried first, then the index keys, and the unit is
/// taken from whichever list actually supplied the value. Deciding the unit any
/// other way lets a reading and its unit come from different keys: a payload
/// carrying both `tvoc_ppb` and `tvocIndex` would otherwise report the ppb
/// number labelled as an index. A missing reading has no unit at all.
fn extract_gas(
    raw: &Value,
    ppb_keys: &[&str],
    index_keys: &[&str],
) -> (Option<f32>, Option<GasUnit>) {
    if let Some(value) = extract_measurement_value(raw, ppb_keys) {
        return (Some(value), Some(GasUnit::Ppb));
    }
    if let Some(value) = extract_measurement_value(raw, index_keys) {
        return (Some(value), Some(GasUnit::Index));
    }
    (None, None)
}

/// Return the first numeric value found under any candidate key.
///
/// This searches top-level keys first, then recursively searches nested objects
/// and arrays. That makes the parser tolerant of payloads that wrap sensor
/// values in a `measurements` object.
fn extract_measurement_value(raw: &Value, candidates: &[&str]) -> Option<f32> {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{parse_air_measurements, GasUnit};

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
        // 7 ug/m3 against the 2024 bands, where "Good" tops out at 9.0 rather
        // than the 12.0 used before the revision.
        assert_eq!(snapshot.aqi.map(|value| value.round()), Some(39.0));
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

    #[test]
    fn mixed_gas_keys_take_the_unit_from_the_key_that_supplied_the_value() {
        // A payload carrying both spellings must not label the ppb reading as an
        // index: the value is read concentration-first, so the unit has to be too.
        let snapshot = parse_air_measurements(&json!({
            "tvoc_ppb": 240,
            "tvocIndex": 100,
            "nox_ppb": 18,
            "noxIndex": 2,
        }));

        assert_eq!(snapshot.tvoc, Some(240.0));
        assert_eq!(snapshot.tvoc_unit, Some(GasUnit::Ppb));
        assert_eq!(snapshot.nox, Some(18.0));
        assert_eq!(snapshot.nox_unit, Some(GasUnit::Ppb));
    }

    #[test]
    fn gas_keys_that_only_carry_an_index_are_labelled_as_an_index() {
        let snapshot = parse_air_measurements(&json!({ "tvocIndex": 100, "noxIndex": 2 }));

        assert_eq!(snapshot.tvoc, Some(100.0));
        assert_eq!(snapshot.tvoc_unit, Some(GasUnit::Index));
        assert_eq!(snapshot.nox, Some(2.0));
        assert_eq!(snapshot.nox_unit, Some(GasUnit::Index));
    }
}
