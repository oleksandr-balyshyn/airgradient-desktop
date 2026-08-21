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

    /// The band this dimension starts out treating as comfortable.
    ///
    /// 18-26 °C and 40-60 % are the bands the dashboard cards used before these
    /// became configurable, so someone upgrading sees exactly what they saw
    /// before until they change them. They are also close to the usual
    /// indoor-comfort guidance, which makes them a reasonable place for a new
    /// user to start.
    pub const fn default_range(self) -> ComfortRange {
        match self {
            Self::Temperature => ComfortRange {
                min: 18.0,
                max: 26.0,
            },
            Self::Humidity => ComfortRange {
                min: 40.0,
                max: 60.0,
            },
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ComfortThresholds {
    pub temperature: ComfortRange,
    pub humidity: ComfortRange,
}

impl Default for ComfortThresholds {
    fn default() -> Self {
        Self {
            temperature: Dimension::Temperature.default_range(),
            humidity: Dimension::Humidity.default_range(),
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

impl<'de> Deserialize<'de> for ComfortThresholds {
    /// Load both ranges, checking each against the limits of its own dimension.
    ///
    /// This is written by hand rather than derived because a stored range is
    /// just two numbers: nothing in `{"min":10,"max":140}` says whether it is a
    /// temperature or a humidity, and only the field it arrived in knows. Going
    /// through `ComfortRange::new` here means a config file gets exactly the
    /// checks the settings form gets — a humidity above 100% is refused rather
    /// than quietly loaded, and `config` reports it like any other unreadable
    /// setting, falling back to defaults with a message on the Settings page.
    ///
    /// Either field may be missing, which is what a config file written before
    /// comfort ranges existed looks like.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct StoredRange {
            min: f32,
            max: f32,
        }

        #[derive(Deserialize)]
        struct Stored {
            #[serde(default)]
            temperature: Option<StoredRange>,
            #[serde(default)]
            humidity: Option<StoredRange>,
        }

        fn range<E: serde::de::Error>(
            dimension: Dimension,
            stored: Option<StoredRange>,
        ) -> Result<ComfortRange, E> {
            match stored {
                Some(StoredRange { min, max }) => {
                    ComfortRange::new(dimension, min, max).map_err(E::custom)
                }
                None => Ok(dimension.default_range()),
            }
        }

        let stored = Stored::deserialize(deserializer)?;
        Ok(Self {
            temperature: range(Dimension::Temperature, stored.temperature)?,
            humidity: range(Dimension::Humidity, stored.humidity)?,
        })
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

    /// What to say about the room, and what would fix it.
    ///
    /// Both come out of one match so they cannot drift apart. Written as two
    /// separate matches over the same nine cases, adding a case to one and
    /// forgetting the other would produce a notification whose advice does not
    /// match its title.
    ///
    /// The pairs are the interesting part. Air conditioning cools *and* dries,
    /// so it is the single answer to a warm humid room; a warm dry room wants
    /// cooling without losing more moisture; a cool humid room is where a
    /// dehumidifier earns its place, because heating alone would leave the damp.
    const fn recommendation(self) -> Recommendation {
        use Position::{Above, Below};

        match (self.temperature, self.humidity) {
            (Some(Above), Some(Above)) => Recommendation {
                headline: "The room is warm and humid",
                advice: "Turn on the air conditioning — it cools and dehumidifies in one go.",
            },
            (Some(Above), Some(Below)) => Recommendation {
                headline: "The room is warm and dry",
                advice: "Cool the room, and run a humidifier so cooling does not dry it further.",
            },
            (Some(Below), Some(Above)) => Recommendation {
                headline: "The room is cool and humid",
                advice: "Warm the room and run a dehumidifier; heating alone will leave it damp.",
            },
            (Some(Below), Some(Below)) => Recommendation {
                headline: "The room is cool and dry",
                advice: "Warm the room and run a humidifier — heating dries the air further.",
            },
            (Some(Above), _) => Recommendation {
                headline: "The room is warm",
                advice: "Cool the room with air conditioning or a fan.",
            },
            (Some(Below), _) => Recommendation {
                headline: "The room is cool",
                advice: "Warm the room.",
            },
            (_, Some(Above)) => Recommendation {
                headline: "The room is humid",
                advice: "Run a dehumidifier or ventilate to bring the moisture down.",
            },
            (_, Some(Below)) => Recommendation {
                headline: "The room is dry",
                advice: "Run a humidifier to bring the moisture up.",
            },
            _ => Recommendation {
                headline: "The room is comfortable",
                advice: "Nothing to do — temperature and humidity are both in range.",
            },
        }
    }

    /// Short description of the room, for example "The room is warm and humid".
    pub const fn headline(self) -> &'static str {
        self.recommendation().headline
    }

    /// What to do about it, in terms of appliances someone actually owns.
    pub const fn advice(self) -> &'static str {
        self.recommendation().advice
    }
}

/// A description of the room paired with the action that would fix it.
///
/// Private: outside this module the two are read one at a time, as a
/// notification's title and body.
struct Recommendation {
    headline: &'static str,
    advice: &'static str,
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
    fn stored_ranges_round_trip_through_json() {
        let stored = ComfortThresholds {
            temperature: ComfortRange::new(Dimension::Temperature, 19.5, 24.0)
                .expect("valid range"),
            humidity: ComfortRange::new(Dimension::Humidity, 35.0, 55.0).expect("valid range"),
        };
        let raw = serde_json::to_string(&stored).expect("thresholds should serialize");

        let restored: ComfortThresholds =
            serde_json::from_str(&raw).expect("thresholds should load");

        assert_eq!(restored, stored);
    }

    #[test]
    fn an_inverted_range_in_a_config_file_is_refused() {
        let err = serde_json::from_str::<ComfortThresholds>(
            r#"{"temperature":{"min":30.0,"max":10.0},"humidity":{"min":40.0,"max":60.0}}"#,
        )
        .expect_err("an inverted stored range must not load");

        assert!(err.to_string().contains("must be below"));
    }

    #[test]
    fn a_stored_humidity_outside_the_percentage_scale_is_refused() {
        // The settings form cannot produce this, but a hand-edited config file
        // can, and it would call every real reading uncomfortably dry.
        let err = serde_json::from_str::<ComfortThresholds>(
            r#"{"temperature":{"min":18.0,"max":26.0},"humidity":{"min":40.0,"max":500.0}}"#,
        )
        .expect_err("humidity above 100% must not load");

        assert!(err.to_string().contains("outside the allowed"));
    }

    #[test]
    fn a_config_file_from_before_comfort_ranges_loads_with_the_defaults() {
        let loaded: ComfortThresholds =
            serde_json::from_str("{}").expect("an empty object should load");

        assert_eq!(loaded, ComfortThresholds::default());
    }

    #[test]
    fn one_stored_range_is_enough_and_the_other_defaults() {
        let loaded: ComfortThresholds =
            serde_json::from_str(r#"{"humidity":{"min":30.0,"max":70.0}}"#)
                .expect("a partial object should load");

        assert_eq!(loaded.humidity.min(), 30.0);
        assert_eq!(loaded.temperature, ComfortThresholds::default().temperature);
    }
}
