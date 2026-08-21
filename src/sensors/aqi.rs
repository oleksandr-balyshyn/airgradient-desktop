//! US EPA Air Quality Index for PM2.5.
//!
//! Turning a particulate concentration into an AQI is more than one lookup, and
//! the two halves are easy to get wrong in different ways, so both live here.
//!
//! `pm25_to_aqi` maps a single concentration onto the index using the EPA's
//! published breakpoints. `nowcast` is what makes a *live* reading meaningful:
//! the index is defined over a 24-hour average, so feeding it one instantaneous
//! sample overstates a brief event badly. Lighting a stove can put an indoor
//! sensor into "Unhealthy" for two minutes, which is not what the AQI means.
//!
//! AirNow and PurpleAir both solve that with the EPA's NowCast, a weighted
//! average of the last 12 hours that leans on recent hours more heavily when
//! readings are changing quickly. That is the algorithm implemented below.

/// One row of the breakpoint table: a concentration band and the index band it
/// maps onto.
struct Band {
    concentration_low: f32,
    concentration_high: f32,
    aqi_low: f32,
    aqi_high: f32,
}

/// EPA PM2.5 breakpoints, as revised in 2024.
///
/// The revision took effect on 6 May 2024 alongside the annual standard moving
/// from 12.0 to 9.0 ug/m3. It lowered the top of "Good" from 12.0 to 9.0 and
/// tightened the Unhealthy, Very Unhealthy and Hazardous bands, merging the two
/// old Hazardous rows into one. Anything computed against the pre-2024 table
/// reads healthier than it should.
const PM25_BANDS: [Band; 6] = [
    Band {
        concentration_low: 0.0,
        concentration_high: 9.0,
        aqi_low: 0.0,
        aqi_high: 50.0,
    },
    Band {
        concentration_low: 9.1,
        concentration_high: 35.4,
        aqi_low: 51.0,
        aqi_high: 100.0,
    },
    Band {
        concentration_low: 35.5,
        concentration_high: 55.4,
        aqi_low: 101.0,
        aqi_high: 150.0,
    },
    Band {
        concentration_low: 55.5,
        concentration_high: 125.4,
        aqi_low: 151.0,
        aqi_high: 200.0,
    },
    Band {
        concentration_low: 125.5,
        concentration_high: 225.4,
        aqi_low: 201.0,
        aqi_high: 300.0,
    },
    Band {
        concentration_low: 225.5,
        concentration_high: 325.4,
        aqi_low: 301.0,
        aqi_high: 500.0,
    },
];

/// Index reported for concentrations above the top of the table.
const MAX_AQI: f32 = 500.0;

/// The EPA publishes breakpoints to one decimal place and specifies that
/// concentrations are truncated to that precision before the lookup.
const CONCENTRATION_PRECISION: f32 = 10.0;

/// Hours of history the NowCast looks back over.
pub const NOWCAST_HOURS: usize = 12;

/// Floor for the NowCast weight.
///
/// The weight falls as readings become more variable, which shifts emphasis onto
/// the most recent hours. The EPA floors it at one half so that even a wildly
/// swinging series still averages over several hours instead of collapsing onto
/// the newest reading alone.
const MIN_NOWCAST_WEIGHT: f32 = 0.5;

/// One of the six named bands of the US AQI scale.
///
/// The band boundaries are a single fact about the scale, so they are stated
/// once, here, and everything that needs to say something about an index value
/// starts from a band: `thresholds::aqi_status_color` turns it into a palette
/// slot, `ui::status::aqi_class` into a CSS class, and the AQI card into the
/// band's name and its one-sentence explanation. Before this existed the same
/// five numbers were written out in three separate modules, so revising the
/// scale meant finding all three.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AqiBand {
    Good,
    Moderate,
    UnhealthyForSensitiveGroups,
    Unhealthy,
    VeryUnhealthy,
    Hazardous,
}

impl AqiBand {
    /// The band an index value falls into.
    ///
    /// Each boundary belongs to the lower band: an AQI of exactly 50 is still
    /// "Good", which is how the EPA publishes the scale.
    pub fn of(value: f32) -> Self {
        match value {
            value if value <= 50.0 => Self::Good,
            value if value <= 100.0 => Self::Moderate,
            value if value <= 150.0 => Self::UnhealthyForSensitiveGroups,
            value if value <= 200.0 => Self::Unhealthy,
            value if value <= 300.0 => Self::VeryUnhealthy,
            _ => Self::Hazardous,
        }
    }
}

/// Convert a PM2.5 concentration in ug/m3 to a US EPA AQI value.
pub fn pm25_to_aqi(concentration: f32) -> f32 {
    if concentration <= 0.0 {
        return 0.0;
    }

    // Truncating first is what makes the published bands contiguous: the first
    // ends at 9.0 and the second begins at 9.1, so an untruncated 9.05 would sit
    // between them and belong to neither.
    let truncated = (concentration * CONCENTRATION_PRECISION).floor() / CONCENTRATION_PRECISION;

    for band in &PM25_BANDS {
        if truncated <= band.concentration_high {
            // Clamping to the band's floor keeps a value that still lands in a
            // gap, through float rounding, inside this band rather than falling
            // through the table.
            let inside = truncated.max(band.concentration_low);
            let span =
                (band.aqi_high - band.aqi_low) / (band.concentration_high - band.concentration_low);
            return span * (inside - band.concentration_low) + band.aqi_low;
        }
    }

    MAX_AQI
}

/// The EPA NowCast: a weighted average of up to 12 hourly PM2.5 means.
///
/// `hourly_means[0]` is the current hour and later entries go further back. An
/// hour with no readings is `None`.
///
/// The weight is the ratio of the smallest hourly mean to the largest, floored
/// at one half. Steady air gives a weight near 1, which averages the whole
/// window evenly; rapidly changing air gives a small weight, which concentrates
/// the result on the last hour or two so the index keeps up with a real event
/// without chasing every spike.
///
/// Returns `None` when the two most recent hours are not both populated, which
/// is the EPA's own rule: without them there is no basis for a "now" cast.
pub fn nowcast(hourly_means: &[Option<f32>]) -> Option<f32> {
    let hours = &hourly_means[..hourly_means.len().min(NOWCAST_HOURS)];
    if !matches!(hours.first(), Some(Some(_))) || !matches!(hours.get(1), Some(Some(_))) {
        return None;
    }

    let readings: Vec<f32> = hours.iter().flatten().copied().collect();
    let highest = readings.iter().copied().fold(f32::MIN, f32::max);
    let lowest = readings.iter().copied().fold(f32::MAX, f32::min);

    // Perfectly clean air across the window leaves nothing to weight, and the
    // ratio below would divide by zero.
    if highest <= 0.0 {
        return Some(0.0);
    }

    let weight = (lowest / highest).max(MIN_NOWCAST_WEIGHT);

    let mut weighted_total = 0.0;
    let mut weight_total = 0.0;
    for (hours_ago, mean) in hours.iter().enumerate() {
        let Some(mean) = mean else {
            continue;
        };
        let factor = weight.powi(hours_ago as i32);
        weighted_total += factor * mean;
        weight_total += factor;
    }

    (weight_total > 0.0).then(|| weighted_total / weight_total)
}

#[cfg(test)]
mod tests {
    use super::AqiBand;

    #[test]
    fn aqi_bands_put_each_boundary_in_the_lower_band() {
        assert_eq!(AqiBand::of(0.0), AqiBand::Good);
        assert_eq!(AqiBand::of(50.0), AqiBand::Good);
        assert_eq!(AqiBand::of(50.1), AqiBand::Moderate);
        assert_eq!(AqiBand::of(100.0), AqiBand::Moderate);
        assert_eq!(AqiBand::of(150.0), AqiBand::UnhealthyForSensitiveGroups);
        assert_eq!(AqiBand::of(200.0), AqiBand::Unhealthy);
        assert_eq!(AqiBand::of(300.0), AqiBand::VeryUnhealthy);
        assert_eq!(AqiBand::of(300.1), AqiBand::Hazardous);
    }

    use super::{nowcast, pm25_to_aqi};

    #[test]
    fn uses_the_2024_band_boundaries() {
        assert_eq!(pm25_to_aqi(0.0), 0.0);
        assert_eq!(
            pm25_to_aqi(9.0),
            50.0,
            "top of Good moved from 12.0 to 9.0 in 2024"
        );
        assert_eq!(pm25_to_aqi(9.1), 51.0);
        assert_eq!(pm25_to_aqi(35.4), 100.0);
        assert_eq!(pm25_to_aqi(35.5), 101.0);
        assert_eq!(pm25_to_aqi(55.4), 150.0);
        assert_eq!(pm25_to_aqi(125.4), 200.0);
        assert_eq!(pm25_to_aqi(225.4), 300.0);
        assert_eq!(pm25_to_aqi(325.4), 500.0);
    }

    /// The 2024 revision made the same air read worse, which is the point of it.
    #[test]
    fn the_revised_bands_are_stricter_than_the_old_ones() {
        assert!(
            pm25_to_aqi(12.0) > 50.0,
            "12.0 ug/m3 was the top of Good before 2024 and is Moderate now"
        );
        assert!(
            pm25_to_aqi(150.0) > 200.0,
            "150 was Unhealthy before, Very Unhealthy now"
        );
    }

    #[test]
    fn concentrations_between_bands_do_not_fall_through() {
        // Every band boundary has a 0.1 gap above it; a reading inside one must
        // stay in the lower band rather than dropping out of the table.
        for (concentration, expected) in [
            (9.05, 50.0),
            (35.45, 100.0),
            (55.45, 150.0),
            (125.45, 200.0),
        ] {
            assert_eq!(pm25_to_aqi(concentration), expected);
        }
    }

    #[test]
    fn aqi_rises_with_concentration_and_stays_in_range() {
        let mut previous = 0.0_f32;
        for step in 0..40_000 {
            let concentration = step as f32 * 0.01;
            let aqi = pm25_to_aqi(concentration);
            assert!((0.0..=500.0).contains(&aqi), "{concentration} gave {aqi}");
            assert!(aqi >= previous - 0.5, "AQI fell from {previous} to {aqi}");
            previous = aqi;
        }
    }

    #[test]
    fn steady_air_averages_the_whole_window() {
        let hours = [Some(10.0); 12];

        assert_eq!(nowcast(&hours), Some(10.0));
    }

    /// A brief spike must not drag the index all the way up.
    ///
    /// This is the case that makes the NowCast worth having: one instantaneous
    /// reading of 200 is "Very Unhealthy", but a single bad hour after eleven
    /// clean ones is not what the AQI is meant to describe.
    #[test]
    fn a_single_bad_hour_is_damped_by_the_clean_ones() {
        let mut hours = [Some(5.0); 12];
        hours[0] = Some(200.0);

        let smoothed = nowcast(&hours).expect("two recent hours are present");

        assert!(
            smoothed < 200.0,
            "the spike should be damped, got {smoothed}"
        );
        assert!(
            smoothed > 5.0,
            "the spike should still move the index, got {smoothed}"
        );
        assert!(
            pm25_to_aqi(smoothed) < pm25_to_aqi(200.0),
            "the smoothed index must be below the instantaneous one"
        );
    }

    #[test]
    fn sustained_pollution_is_not_damped_away() {
        let hours = [Some(150.0); 12];

        assert_eq!(
            nowcast(&hours),
            Some(150.0),
            "real, sustained pollution must show in full"
        );
    }

    #[test]
    fn without_two_recent_hours_there_is_no_nowcast() {
        assert_eq!(nowcast(&[]), None);
        assert_eq!(nowcast(&[Some(10.0)]), None, "one hour is not enough");
        assert_eq!(
            nowcast(&[None, Some(10.0), Some(10.0)]),
            None,
            "the current hour is missing"
        );
        assert_eq!(
            nowcast(&[Some(10.0), None, Some(10.0)]),
            None,
            "the previous hour is missing"
        );
    }

    #[test]
    fn gaps_further_back_are_skipped_rather_than_counted_as_zero() {
        let with_gap = nowcast(&[Some(10.0), Some(10.0), None, Some(10.0)]);

        assert_eq!(
            with_gap,
            Some(10.0),
            "a missing hour must not pull the mean down"
        );
    }

    #[test]
    fn perfectly_clean_air_is_zero_rather_than_a_division_by_zero() {
        assert_eq!(nowcast(&[Some(0.0), Some(0.0)]), Some(0.0));
    }
}
