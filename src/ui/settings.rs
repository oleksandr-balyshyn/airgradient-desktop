//! Settings page.
//!
//! The page keeps its own copy of every editable value in the model, updated by
//! one input message per widget. Nothing reads a widget back out at save time,
//! which means "what the user typed" and "what gets validated" can never drift
//! apart, and the validation logic is reachable from a test.
//!
//! Saving is not done here. The page validates the input and emits
//! `SettingsOutput::Save` with a ready-to-persist `AppConfig`; the root component
//! owns writing the file and restarting the refresh timer.

use relm4::adw::prelude::*;
use relm4::{adw, gtk, prelude::*};

use crate::config::{
    AppConfig, RefreshInterval, MAX_REFRESH_INTERVAL_SECS, MIN_REFRESH_INTERVAL_SECS,
};
use crate::device::DeviceBaseUrl;
use crate::sensors::comfort::{ComfortRange, ComfortThresholds, Dimension};
use crate::theme::{self, Theme};

/// What the settings page needs to render its initial values.
#[derive(Debug, Clone)]
pub struct SettingsInit {
    pub server_url: Option<String>,
    pub refresh_interval: RefreshInterval,
    pub notifications_enabled: bool,
    pub start_minimized: bool,
    pub comfort: ComfortThresholds,
    pub theme_id: String,
    /// Message explaining why defaults were loaded, if anything went wrong.
    pub status: String,
}

#[derive(Debug)]
pub enum SettingsInput {
    UrlChanged(String),
    IntervalChanged(f64),
    NotificationsToggled(bool),
    StartMinimizedToggled(bool),
    /// One end of one comfort range was moved.
    ComfortBoundChanged {
        dimension: Dimension,
        bound: Bound,
        value: f64,
    },
    ThemeSelected(u32),
    TestNotification,
    Save,
    /// Replace the status line, used by the root component to report results.
    SetStatus(String),
}

#[derive(Debug)]
pub enum SettingsOutput {
    /// The user saved a valid configuration.
    Save(Box<AppConfig>),
    /// The user picked a different theme. Applied immediately so the change is
    /// visible while browsing the list; only written to disk on save.
    ThemeChanged(&'static Theme),
    /// The user asked for a sample notification.
    TestNotification,
}

/// Which end of a comfort range a spin row edits.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Bound {
    Min,
    Max,
}

/// One comfort range as the two spin rows currently hold it.
///
/// Loose numbers rather than a `ComfortRange` because the ends are edited one
/// at a time, and dragging the minimum above the maximum has to be allowed to
/// happen before it is tidied up on save. Turning the pair into a checked range
/// is `to_range`'s job, at the moment the user asks to save.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RangeForm {
    min: f64,
    max: f64,
}

impl RangeForm {
    fn from_range(range: ComfortRange) -> Self {
        Self {
            min: f64::from(range.min()),
            max: f64::from(range.max()),
        }
    }

    fn bound_mut(&mut self, bound: Bound) -> &mut f64 {
        match bound {
            Bound::Min => &mut self.min,
            Bound::Max => &mut self.max,
        }
    }

    /// The checked range these two numbers describe.
    ///
    /// Clamped rather than rejected: two spin buttons can be dragged past each
    /// other, and refusing to save at that moment would be a worse answer than
    /// using the pair the right way round.
    fn to_range(self, dimension: Dimension) -> ComfortRange {
        ComfortRange::clamped(dimension, self.min as f32, self.max as f32)
    }
}

pub struct Settings {
    url_text: String,
    interval_secs: f64,
    notifications_enabled: bool,
    start_minimized: bool,
    temperature: RangeForm,
    humidity: RangeForm,
    theme_id: String,
    status: String,
}

impl Settings {
    /// Turn the current form contents into a config, or explain what is wrong.
    ///
    /// The user types a loose string such as `192.168.1.201`, while the config
    /// file stores a normalized base URL such as `http://192.168.1.201`. Doing
    /// that conversion here means the fetch code never has to guess, and an
    /// unusable value is never written to disk.
    /// How far a comfort spin row may be moved.
    ///
    /// Taken from the dimension rather than written into the widget, so the
    /// range a user can pick and the range a config file may contain are the
    /// same numbers stated once.
    fn limit(&self, dimension: Dimension, bound: Bound) -> f64 {
        let (lowest, highest) = dimension.limits();
        f64::from(match bound {
            Bound::Min => lowest,
            Bound::Max => highest,
        })
    }

    /// Store one edited comfort bound.
    fn set_comfort_bound(&mut self, dimension: Dimension, bound: Bound, value: f64) {
        let form = match dimension {
            Dimension::Temperature => &mut self.temperature,
            Dimension::Humidity => &mut self.humidity,
        };
        *form.bound_mut(bound) = value;
    }

    fn validate(&self) -> Result<AppConfig, String> {
        let server_url =
            DeviceBaseUrl::parse(&self.url_text).map_err(|err| format!("Invalid URL: {err}"))?;

        // The spin row already limits the range, so a value outside it means the
        // form and the config disagree rather than that the user asked for
        // something impossible. Clamping keeps the bounds defined in one place.
        let refresh_interval = RefreshInterval::clamped(self.interval_secs.round() as u64);

        let comfort = ComfortThresholds {
            temperature: self.temperature.to_range(Dimension::Temperature),
            humidity: self.humidity.to_range(Dimension::Humidity),
        };

        Ok(AppConfig {
            server_url,
            refresh_interval,
            notifications_enabled: self.notifications_enabled,
            start_minimized: self.start_minimized,
            comfort,
            theme: self.theme_id.clone(),
        })
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Settings {
    type Init = SettingsInit;
    type Input = SettingsInput;
    type Output = SettingsOutput;

    view! {
        adw::PreferencesPage {
            set_title: "Settings",
            set_icon_name: Some("preferences-system-symbolic"),

            adw::PreferencesGroup {
                set_title: "Appearance",
                set_description: Some(
                    "System Default follows the desktop's light or dark preference. \
                     The other themes set their own colours.",
                ),

                adw::ComboRow {
                    set_title: "Theme",
                    set_subtitle: "Applied while you browse the list; save to keep it",
                    set_model: Some(&gtk::StringList::new(&theme::names())),
                    set_selected: theme::index_of(&model.theme_id),
                    connect_selected_notify[sender] => move |row| {
                        sender.input(SettingsInput::ThemeSelected(row.selected()));
                    },
                },
            },

            adw::PreferencesGroup {
                set_title: "Device",
                set_description: Some("Configure the AirGradient local-server endpoint."),

                adw::EntryRow {
                    set_title: "Local-server Base URL",
                    set_text: model.url_text.as_str(),

                    add_prefix = &gtk::Image {
                        set_icon_name: Some("network-wired-symbolic"),
                    },

                    connect_changed[sender] => move |row| {
                        sender.input(SettingsInput::UrlChanged(row.text().to_string()));
                    },
                },

                adw::SpinRow {
                    set_title: "Refresh Interval",
                    set_subtitle: "Seconds between automatic measurement updates",
                    set_tooltip_text: Some("Refresh interval in seconds. Minimum value is 5 seconds."),
                    set_adjustment: Some(&gtk::Adjustment::new(
                        model.interval_secs,
                        MIN_REFRESH_INTERVAL_SECS as f64,
                        MAX_REFRESH_INTERVAL_SECS as f64,
                        1.0,
                        10.0,
                        0.0,
                    )),
                    set_numeric: true,
                    connect_value_notify[sender] => move |row| {
                        sender.input(SettingsInput::IntervalChanged(row.value()));
                    },
                },

                adw::ActionRow {
                    set_title: "Air Quality Notifications",
                    set_subtitle: "Notify when CO2, AQI, particles, VOC, NOx, damp air, or your comfort ranges need attention",

                    #[name = "notifications_switch"]
                    add_suffix = &gtk::Switch {
                        set_valign: gtk::Align::Center,
                        set_active: model.notifications_enabled,
                        connect_active_notify[sender] => move |switch| {
                            sender.input(SettingsInput::NotificationsToggled(switch.is_active()));
                        },
                    },

                    set_activatable_widget: Some(&notifications_switch),
                },

                adw::ActionRow {
                    set_title: "Start Minimized",
                    set_subtitle: "Start hidden and keep polling in the background on next launch",

                    #[name = "start_minimized_switch"]
                    add_suffix = &gtk::Switch {
                        set_valign: gtk::Align::Center,
                        set_active: model.start_minimized,
                        connect_active_notify[sender] => move |switch| {
                            sender.input(SettingsInput::StartMinimizedToggled(switch.is_active()));
                        },
                    },

                    set_activatable_widget: Some(&start_minimized_switch),
                },

                adw::ActionRow {
                    set_title: "Test Notification",
                    set_subtitle: "Send a sample alert and test click-to-open behavior",

                    #[name = "test_notification_button"]
                    add_suffix = &gtk::Button {
                        set_label: "Send",
                        set_valign: gtk::Align::Center,
                        add_css_class: "suggested-action",
                        connect_clicked => SettingsInput::TestNotification,
                    },

                    set_activatable_widget: Some(&test_notification_button),
                },

            },

            adw::PreferencesGroup {
                set_title: "Comfort",
                set_description: Some(
                    "The temperature and humidity you find comfortable. Readings outside \
                     these ranges highlight the Temperature and Humidity cards, and — with \
                     Air Quality Notifications on — send an alert saying what would fix it, \
                     such as running the air conditioning or a humidifier.",
                ),

                adw::SpinRow {
                    set_title: "Coolest Comfortable Temperature",
                    set_subtitle: "Below this, the room is reported as cool (°C)",
                    set_adjustment: Some(&gtk::Adjustment::new(
                        model.temperature.min,
                        model.limit(Dimension::Temperature, Bound::Min),
                        model.limit(Dimension::Temperature, Bound::Max),
                        0.5,
                        1.0,
                        0.0,
                    )),
                    set_digits: 1,
                    set_numeric: true,
                    connect_value_notify[sender] => move |row| {
                        sender.input(SettingsInput::ComfortBoundChanged {
                            dimension: Dimension::Temperature,
                            bound: Bound::Min,
                            value: row.value(),
                        });
                    },
                },

                adw::SpinRow {
                    set_title: "Warmest Comfortable Temperature",
                    set_subtitle: "Above this, the room is reported as warm (°C)",
                    set_adjustment: Some(&gtk::Adjustment::new(
                        model.temperature.max,
                        model.limit(Dimension::Temperature, Bound::Min),
                        model.limit(Dimension::Temperature, Bound::Max),
                        0.5,
                        1.0,
                        0.0,
                    )),
                    set_digits: 1,
                    set_numeric: true,
                    connect_value_notify[sender] => move |row| {
                        sender.input(SettingsInput::ComfortBoundChanged {
                            dimension: Dimension::Temperature,
                            bound: Bound::Max,
                            value: row.value(),
                        });
                    },
                },

                adw::SpinRow {
                    set_title: "Driest Comfortable Humidity",
                    set_subtitle: "Below this, the room is reported as dry (%)",
                    set_adjustment: Some(&gtk::Adjustment::new(
                        model.humidity.min,
                        model.limit(Dimension::Humidity, Bound::Min),
                        model.limit(Dimension::Humidity, Bound::Max),
                        1.0,
                        5.0,
                        0.0,
                    )),
                    set_numeric: true,
                    connect_value_notify[sender] => move |row| {
                        sender.input(SettingsInput::ComfortBoundChanged {
                            dimension: Dimension::Humidity,
                            bound: Bound::Min,
                            value: row.value(),
                        });
                    },
                },

                adw::SpinRow {
                    set_title: "Most Humid Comfortable Humidity",
                    set_subtitle: "Above this, the room is reported as humid (%)",
                    set_adjustment: Some(&gtk::Adjustment::new(
                        model.humidity.max,
                        model.limit(Dimension::Humidity, Bound::Min),
                        model.limit(Dimension::Humidity, Bound::Max),
                        1.0,
                        5.0,
                        0.0,
                    )),
                    set_numeric: true,
                    connect_value_notify[sender] => move |row| {
                        sender.input(SettingsInput::ComfortBoundChanged {
                            dimension: Dimension::Humidity,
                            bound: Bound::Max,
                            value: row.value(),
                        });
                    },
                },
            },

            adw::PreferencesGroup {
                adw::ActionRow {
                    set_title: "Save Settings",
                    set_subtitle: "Save every setting on this page and restart the refresh timer",
                    set_activatable: true,
                    connect_activated => SettingsInput::Save,

                    add_suffix = &gtk::Image {
                        set_icon_name: Some("document-save-symbolic"),
                    },
                },
            },

            adw::PreferencesGroup {
                adw::ActionRow {
                    set_title: "Status",
                    #[watch]
                    set_subtitle: model.status.as_str(),
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            url_text: init.server_url.unwrap_or_default(),
            interval_secs: init.refresh_interval.as_secs() as f64,
            notifications_enabled: init.notifications_enabled,
            start_minimized: init.start_minimized,
            temperature: RangeForm::from_range(init.comfort.temperature),
            humidity: RangeForm::from_range(init.comfort.humidity),
            theme_id: init.theme_id,
            status: init.status,
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            SettingsInput::UrlChanged(text) => self.url_text = text,
            SettingsInput::IntervalChanged(seconds) => self.interval_secs = seconds,
            SettingsInput::NotificationsToggled(enabled) => self.notifications_enabled = enabled,
            SettingsInput::StartMinimizedToggled(enabled) => self.start_minimized = enabled,
            SettingsInput::ComfortBoundChanged {
                dimension,
                bound,
                value,
            } => self.set_comfort_bound(dimension, bound, value),
            SettingsInput::ThemeSelected(index) => {
                let theme = theme::at_index(index);
                self.theme_id = theme.id.to_string();
                let _ = sender.output(SettingsOutput::ThemeChanged(theme));
            }
            SettingsInput::TestNotification => {
                let _ = sender.output(SettingsOutput::TestNotification);
            }
            SettingsInput::Save => match self.validate() {
                Ok(config) => {
                    let _ = sender.output(SettingsOutput::Save(Box::new(config)));
                }
                Err(err) => self.status = err,
            },
            SettingsInput::SetStatus(status) => self.status = status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RangeForm, Settings};
    use crate::config::{RefreshInterval, MIN_REFRESH_INTERVAL_SECS};
    use crate::theme::DEFAULT_THEME_ID;

    fn form(url: &str, interval_secs: f64) -> Settings {
        Settings {
            url_text: url.to_string(),
            interval_secs,
            notifications_enabled: true,
            start_minimized: false,
            temperature: RangeForm {
                min: 18.0,
                max: 26.0,
            },
            humidity: RangeForm {
                min: 40.0,
                max: 60.0,
            },
            theme_id: DEFAULT_THEME_ID.to_string(),
            status: String::new(),
        }
    }

    #[test]
    fn valid_form_normalizes_the_url() {
        let config = form("192.168.1.201", 30.0)
            .validate()
            .expect("form should be valid");

        assert_eq!(
            config.server_url.as_ref().map(|url| url.as_str()),
            Some("http://192.168.1.201")
        );
        assert_eq!(config.refresh_interval, RefreshInterval::new(30).unwrap());
    }

    #[test]
    fn empty_url_saves_as_not_configured() {
        let config = form("  ", 30.0).validate().expect("empty URL is allowed");

        assert!(config.server_url.is_none());
    }

    #[test]
    fn unsupported_scheme_is_reported_to_the_user() {
        let err = form("ftp://device.local", 30.0)
            .validate()
            .expect_err("ftp should be rejected");

        assert!(err.starts_with("Invalid URL:"), "unexpected message: {err}");
    }

    #[test]
    fn too_small_interval_is_clamped_rather_than_rejected() {
        let config = form("192.168.1.201", 1.0)
            .validate()
            .expect("interval should be clamped");

        assert_eq!(config.refresh_interval.as_secs(), MIN_REFRESH_INTERVAL_SECS);
    }

    #[test]
    fn saved_config_carries_the_comfort_ranges() {
        let mut form = form("192.168.1.201", 30.0);
        form.temperature = RangeForm {
            min: 19.0,
            max: 23.0,
        };

        let config = form.validate().expect("form should be valid");

        assert_eq!(config.comfort.temperature.min(), 19.0);
        assert_eq!(config.comfort.temperature.max(), 23.0);
    }

    #[test]
    fn comfort_bounds_dragged_past_each_other_are_saved_the_right_way_round() {
        // Both spin rows are free to move, so the minimum can end up above the
        // maximum mid-edit. Saving then must not store a range that calls every
        // reading uncomfortable.
        let mut form = form("192.168.1.201", 30.0);
        form.humidity = RangeForm {
            min: 65.0,
            max: 35.0,
        };

        let config = form.validate().expect("swapped bounds should still save");

        assert_eq!(config.comfort.humidity.min(), 35.0);
        assert_eq!(config.comfort.humidity.max(), 65.0);
    }

    #[test]
    fn saved_config_carries_the_selected_theme() {
        let mut form = form("192.168.1.201", 30.0);
        form.theme_id = "nord".to_string();

        let config = form.validate().expect("form should be valid");

        assert_eq!(config.theme, "nord");
    }
}
