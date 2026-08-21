//! Comfort thresholds for temperature and humidity.
//!
//! Pollutants are judged against published health limits, so the app decides for
//! itself what "too much CO2" means. Comfort is not like that: the temperature
//! and humidity a room should sit at is a personal preference, and the point of
//! knowing it is to act on it — turn on the air conditioning, run the
//! humidifier. So the user picks the two ranges in Settings, and this module
//! turns a pair of readings plus those ranges into a plain-language verdict.
//!
//! The two readings are judged *together* on purpose. A room at 27 °C is only
//! mildly unpleasant; a room at 27 °C and 75 % humidity is muggy, and the useful
//! advice is different — an air conditioner deals with both at once, while a
//! dehumidifier alone would leave it hot. The combination is what the
//! notification talks about.
//!
//! Like the rest of `sensors`, this is pure logic: no GTK, no config file, no
//! notifications. `config` stores the ranges, `alerts` decides when to notify
//! about them, and the UI decides how an uncomfortable card looks.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

/// Coldest and hottest temperature, in °C, a range may be set to.
///
/// The bounds are deliberately wide: they exist to catch a typo or a corrupt
/// config file, not to tell anyone what temperature to like.
pub const MIN_TEMPERATURE_C: f32 = -20.0;
pub const MAX_TEMPERATURE_C: f32 = 60.0;

/// Relative humidity is a percentage, so its range is the whole scale.
pub const MIN_HUMIDITY_PCT: f32 = 0.0;
pub const MAX_HUMIDITY_PCT: f32 = 100.0;

/// Where a reading sits relative to a comfort range.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Position {
    /// Below the range: too cold, or too dry.
    Below,
    /// Inside the range, endpoints included.
    Inside,
    /// Above the range: too warm, or too humid.
    Above,
}

impl Position {
    /// Whether this reading is one the user asked to be told about.
    pub const fn is_uncomfortable(self) -> bool {
        !matches!(self, Self::Inside)
    }
}

/// Which reading a range describes.
///
/// The same range logic serves both, but the words differ: a reading under the
/// range is "Cool" for temperature and "Dry" for humidity.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Dimension {
    Temperature,
    Humidity,
}

impl Dimension {
    /// One-word description of where a reading sits, for a card.
    pub const fn word(self, position: Position) -> &'static str {
        match (self, position) {
            (_, Position::Inside) => "Comfortable",
            (Self::Temperature, Position::Below) => "Cool",
            (Self::Temperature, Position::Above) => "Warm",
            (Self::Humidity, Position::Below) => "Dry",
            (Self::Humidity, Position::Above) => "Humid",
        }
    }

    /// Widest range this dimension accepts, as (low, high).
    pub const fn limits(self) -> (f32, f32) {
        match self {
            Self::Temperature => (MIN_TEMPERATURE_C, MAX_TEMPERATURE_C),
            Self::Humidity => (MIN_HUMIDITY_PCT, MAX_HUMIDITY_PCT),
        }
    }
}

/// A range a reading is comfortable inside, endpoints included.
///
/// Constructing one always checks the ordering, so a range in hand is a usable
/// one. That matters because these come from a settings form and from a config
/// file a user may have edited by hand, and a range with its ends swapped would
/// report every possible reading as uncomfortable.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ComfortRange {
    min: f32,
    max: f32,
}

impl ComfortRange {
    /// Build a range, rejecting one that is inverted or outside the dimension's
    /// limits.
    pub fn new(dimension: Dimension, min: f32, max: f32) -> Result<Self, ComfortRangeError> {
        let (lowest, highest) = dimension.limits();

        if !min.is_finite() || !max.is_finite() {
            return Err(ComfortRangeError::NotANumber);
        }
        if min >= max {
            return Err(ComfortRangeError::Inverted { min, max });
        }
        if min < lowest || max > highest {
            return Err(ComfortRangeError::OutOfBounds {
                lowest,
                highest,
                min,
                max,
            });
        }

        Ok(Self { min, max })
    }

    /// Build a range from values that may be out of order or out of bounds.
    ///
    /// The settings form uses this: two spin buttons can be dragged past each
    /// other, and refusing to save is a worse answer than quietly using the
    /// pair the right way round.
    pub fn clamped(dimension: Dimension, min: f32, max: f32) -> Self {
        let (lowest, highest) = dimension.limits();
        let low = min.min(max).clamp(lowest, highest);
        let high = min.max(max).clamp(lowest, highest);

        // Two identical values would leave no comfortable reading at all, so the
        // range is opened up by the smallest step the settings form can produce.
        if low >= high {
            let widened = (low + 1.0).min(highest);
            return Self {
                min: widened - 1.0,
                max: widened,
            };
        }

        Self {
            min: low,
            max: high,
        }
    }

    pub const fn min(self) -> f32 {
        self.min
    }

    pub const fn max(self) -> f32 {
        self.max
    }

    /// Where a reading sits relative to this range.
    pub fn position(self, value: f32) -> Position {
        if value < self.min {
            Position::Below
        } else if value > self.max {
            Position::Above
        } else {
            Position::Inside
        }
    }
}

/// Why a comfort range was rejected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ComfortRangeError {
    Inverted {
        min: f32,
        max: f32,
    },
    OutOfBounds {
        lowest: f32,
        highest: f32,
        min: f32,
        max: f32,
    },
    NotANumber,
}

impl fmt::Display for ComfortRangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inverted { min, max } => write!(
                f,
                "comfort range minimum {min} must be below its maximum {max}"
            ),
            Self::OutOfBounds {
                lowest,
                highest,
                min,
                max,
            } => write!(
                f,
                "comfort range {min}-{max} is outside the allowed {lowest}-{highest}"
            ),
            Self::NotANumber => f.write_str("comfort range bounds must be numbers"),
        }
    }
}

impl std::error::Error for ComfortRangeError {}

/// Both comfort ranges, as chosen by the user.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ComfortThresholds {
    #[serde(default = "default_temperature_range")]
    pub temperature: ComfortRange,
    #[serde(default = "default_humidity_range")]
    pub humidity: ComfortRange,
}

impl Default for ComfortThresholds {
    fn default() -> Self {
        Self {
            temperature: default_temperature_range(),
            humidity: default_humidity_range(),
        }
    }
}

impl ComfortThresholds {
    /// Judge a pair of readings against both ranges.
    pub fn assess(self, temperature: Option<f32>, humidity: Option<f32>) -> Comfort {
        Comfort {
            temperature: temperature.map(|value| self.temperature.position(value)),
            humidity: humidity.map(|value| self.humidity.position(value)),
        }
    }
}

/// The default comfortable temperature band, in °C.
///
/// 18–26 °C and 40–60 % are the bands the dashboard cards used before these
/// became configurable, so someone upgrading sees exactly what they saw before
/// until they change them. They are also close to the usual indoor-comfort
/// guidance, which makes them a reasonable place for a new user to start.
fn default_temperature_range() -> ComfortRange {
    ComfortRange {
        min: 18.0,
        max: 26.0,
    }
}

fn default_humidity_range() -> ComfortRange {
    ComfortRange {
        min: 40.0,
        max: 60.0,
    }
}

impl<'de> Deserialize<'de> for ComfortRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// The stored shape, without the ordering rule attached.
        #[derive(Deserialize)]
        struct Stored {
            min: f32,
            max: f32,
        }

        let Stored { min, max } = Stored::deserialize(deserializer)?;
        if !min.is_finite() || !max.is_finite() || min >= max {
            return Err(serde::de::Error::custom(ComfortRangeError::Inverted {
                min,
                max,
            }));
        }
        Ok(Self { min, max })
    }
}

/// How comfortable the room is, on both dimensions at once.
///
/// A dimension is `None` when the device did not report that reading, which is
/// not the same as it being comfortable: a monitor with no humidity sensor
/// should not be told its humidity is fine.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Comfort {
    pub temperature: Option<Position>,
    pub humidity: Option<Position>,
}

impl Comfort {
    /// Whether either reading is outside its range.
    pub fn is_uncomfortable(self) -> bool {
        self.temperature.is_some_and(Position::is_uncomfortable)
            || self.humidity.is_some_and(Position::is_uncomfortable)
    }

    /// Whether both readings are outside their ranges.
    ///
    /// This is what separates "a bit warm" from "warm *and* muggy", and it is
    /// why the alert for the pair is more urgent than the alert for one of them.
    pub fn is_doubly_uncomfortable(self) -> bool {
        self.temperature.is_some_and(Position::is_uncomfortable)
            && self.humidity.is_some_and(Position::is_uncomfortable)
    }

    /// Short description of the room, for example "Warm and humid".
    pub fn headline(self) -> &'static str {
        use Position::{Above, Below};

        match (self.temperature, self.humidity) {
            (Some(Above), Some(Above)) => "The room is warm and humid",
            (Some(Above), Some(Below)) => "The room is warm and dry",
            (Some(Below), Some(Above)) => "The room is cool and humid",
            (Some(Below), Some(Below)) => "The room is cool and dry",
            (Some(Above), _) => "The room is warm",
            (Some(Below), _) => "The room is cool",
            (_, Some(Above)) => "The room is humid",
            (_, Some(Below)) => "The room is dry",
            _ => "The room is comfortable",
        }
    }

    /// What to do about it, in terms of appliances someone actually owns.
    ///
    /// The pairs matter here. Air conditioning cools *and* dries, so it is the
    /// single answer to a warm, humid room; a warm dry room wants cooling
    /// without losing more moisture; a cool humid room is the one where a
    /// dehumidifier earns its place, because heating alone would leave the damp.
    pub fn advice(self) -> &'static str {
        use Position::{Above, Below};

        match (self.temperature, self.humidity) {
            (Some(Above), Some(Above)) => {
                "Turn on the air conditioning — it cools and dehumidifies in one go."
            }
            (Some(Above), Some(Below)) => {
                "Cool the room, and run a humidifier so cooling does not dry it further."
            }
            (Some(Below), Some(Above)) => {
                "Warm the room and run a dehumidifier; heating alone will leave it damp."
            }
            (Some(Below), Some(Below)) => {
                "Warm the room and run a humidifier — heating dries the air further."
            }
            (Some(Above), _) => "Cool the room with air conditioning or a fan.",
            (Some(Below), _) => "Warm the room.",
            (_, Some(Above)) => "Run a dehumidifier or ventilate to bring the moisture down.",
            (_, Some(Below)) => "Run a humidifier to bring the moisture up.",
            _ => "Nothing to do — temperature and humidity are both in range.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Comfort, ComfortRange, ComfortRangeError, ComfortThresholds, Dimension, Position};

    fn thresholds() -> ComfortThresholds {
        ComfortThresholds::default()
    }

    #[test]
    fn a_range_includes_both_of_its_endpoints() {
        let range = ComfortRange::new(Dimension::Temperature, 18.0, 26.0).expect("valid range");

        assert_eq!(range.position(17.9), Position::Below);
        assert_eq!(range.position(18.0), Position::Inside);
        assert_eq!(range.position(26.0), Position::Inside);
        assert_eq!(range.position(26.1), Position::Above);
    }

    #[test]
    fn an_inverted_range_is_rejected() {
        let err = ComfortRange::new(Dimension::Temperature, 26.0, 18.0)
            .expect_err("a range must not be inverted");

        assert!(matches!(err, ComfortRangeError::Inverted { .. }));
    }

    #[test]
    fn a_range_outside_the_dimension_limits_is_rejected() {
        let err = ComfortRange::new(Dimension::Humidity, 10.0, 140.0)
            .expect_err("humidity above 100% is not a percentage");

        assert!(matches!(err, ComfortRangeError::OutOfBounds { .. }));
    }

    #[test]
    fn clamping_puts_swapped_bounds_back_in_order() {
        let range = ComfortRange::clamped(Dimension::Temperature, 26.0, 18.0);

        assert_eq!(range.min(), 18.0);
        assert_eq!(range.max(), 26.0);
    }

    #[test]
    fn clamping_keeps_a_collapsed_range_usable() {
        // Both spin buttons on the same number would otherwise call every
        // reading uncomfortable, including the one the user just picked.
        let range = ComfortRange::clamped(Dimension::Humidity, 50.0, 50.0);

        assert!(range.min() < range.max());
        assert_eq!(range.position(50.0), Position::Inside);
    }

    #[test]
    fn clamping_pulls_bounds_inside_the_dimension_limits() {
        let range = ComfortRange::clamped(Dimension::Humidity, -30.0, 400.0);

        assert_eq!(range.min(), 0.0);
        assert_eq!(range.max(), 100.0);
    }

    #[test]
    fn the_defaults_are_the_bands_the_cards_used_before() {
        let defaults = thresholds();

        assert_eq!(
            (defaults.temperature.min(), defaults.temperature.max()),
            (18.0, 26.0)
        );
        assert_eq!(
            (defaults.humidity.min(), defaults.humidity.max()),
            (40.0, 60.0)
        );
    }

    #[test]
    fn a_room_inside_both_ranges_needs_nothing_doing() {
        let comfort = thresholds().assess(Some(22.0), Some(50.0));

        assert!(!comfort.is_uncomfortable());
        assert_eq!(comfort.headline(), "The room is comfortable");
    }

    #[test]
    fn warm_and_humid_asks_for_the_air_conditioning() {
        let comfort = thresholds().assess(Some(29.0), Some(72.0));

        assert!(comfort.is_doubly_uncomfortable());
        assert_eq!(comfort.headline(), "The room is warm and humid");
        assert!(comfort.advice().contains("air conditioning"));
    }

    #[test]
    fn cool_and_dry_asks_for_heat_and_a_humidifier() {
        let comfort = thresholds().assess(Some(15.0), Some(28.0));

        assert!(comfort.is_doubly_uncomfortable());
        assert_eq!(comfort.headline(), "The room is cool and dry");
        assert!(comfort.advice().contains("humidifier"));
    }

    #[test]
    fn one_reading_out_of_range_is_uncomfortable_but_not_doubly_so() {
        let comfort = thresholds().assess(Some(22.0), Some(80.0));

        assert!(comfort.is_uncomfortable());
        assert!(!comfort.is_doubly_uncomfortable());
        assert_eq!(comfort.headline(), "The room is humid");
        assert!(comfort.advice().contains("dehumidifier"));
    }

    #[test]
    fn a_missing_reading_is_not_treated_as_comfortable() {
        // A device with no humidity sensor must not have its humidity called fine.
        let comfort = thresholds().assess(Some(29.0), None);

        assert_eq!(
            comfort,
            Comfort {
                temperature: Some(Position::Above),
                humidity: None,
            }
        );
        assert!(!comfort.is_doubly_uncomfortable());
        assert_eq!(comfort.headline(), "The room is warm");
    }

    #[test]
    fn a_device_reporting_neither_reading_says_nothing() {
        let comfort = thresholds().assess(None, None);

        assert!(!comfort.is_uncomfortable());
    }

    #[test]
    fn card_words_match_the_dimension() {
        assert_eq!(Dimension::Temperature.word(Position::Below), "Cool");
        assert_eq!(Dimension::Temperature.word(Position::Above), "Warm");
        assert_eq!(Dimension::Humidity.word(Position::Below), "Dry");
        assert_eq!(Dimension::Humidity.word(Position::Above), "Humid");
        assert_eq!(Dimension::Humidity.word(Position::Inside), "Comfortable");
    }

    #[test]
    fn a_stored_range_round_trips_through_json() {
        let range = ComfortRange::new(Dimension::Temperature, 19.5, 24.0).expect("valid range");
        let raw = serde_json::to_string(&range).expect("range should serialize");

        let restored: ComfortRange = serde_json::from_str(&raw).expect("range should load");

        assert_eq!(restored, range);
    }

    #[test]
    fn an_inverted_range_in_a_config_file_is_refused() {
        let err = serde_json::from_str::<ComfortRange>(r#"{"min":30.0,"max":10.0}"#)
            .expect_err("an inverted stored range must not load");

        assert!(err.to_string().contains("must be below"));
    }
}
