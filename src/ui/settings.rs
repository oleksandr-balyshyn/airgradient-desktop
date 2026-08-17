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
use crate::state::ThemeMode;

/// What the settings page needs to render its initial values.
#[derive(Debug, Clone)]
pub struct SettingsInit {
    pub server_url: Option<String>,
    pub refresh_interval: RefreshInterval,
    pub notifications_enabled: bool,
    pub start_minimized: bool,
    pub theme_mode: ThemeMode,
    /// Message explaining why defaults were loaded, if anything went wrong.
    pub status: String,
}

#[derive(Debug)]
pub enum SettingsInput {
    UrlChanged(String),
    IntervalChanged(f64),
    NotificationsToggled(bool),
    StartMinimizedToggled(bool),
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
    /// The user picked a different appearance. Applied immediately, not saved.
    ThemeChanged(ThemeMode),
    /// The user asked for a sample notification.
    TestNotification,
}

pub struct Settings {
    url_text: String,
    interval_secs: f64,
    notifications_enabled: bool,
    start_minimized: bool,
    theme_mode: ThemeMode,
    status: String,
}

impl Settings {
    /// Turn the current form contents into a config, or explain what is wrong.
    ///
    /// The user types a loose string such as `192.168.1.201`, while the config
    /// file stores a normalized base URL such as `http://192.168.1.201`. Doing
    /// that conversion here means the fetch code never has to guess, and an
    /// unusable value is never written to disk.
    fn validate(&self) -> Result<AppConfig, String> {
        let server_url =
            DeviceBaseUrl::parse(&self.url_text).map_err(|err| format!("Invalid URL: {err}"))?;

        let seconds = (self.interval_secs.round() as u64)
            .clamp(MIN_REFRESH_INTERVAL_SECS, MAX_REFRESH_INTERVAL_SECS);
        let refresh_interval =
            RefreshInterval::new(seconds).map_err(|err| format!("Invalid interval: {err}"))?;

        Ok(AppConfig {
            server_url,
            refresh_interval,
            notifications_enabled: self.notifications_enabled,
            start_minimized: self.start_minimized,
        })
    }
}

/// Order of the entries in the appearance dropdown.
const THEME_CHOICES: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

/// Index of a theme mode in the dropdown.
fn theme_index(mode: ThemeMode) -> u32 {
    THEME_CHOICES
        .iter()
        .position(|choice| *choice == mode)
        .unwrap_or(0) as u32
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
                set_description: Some("GNOME apps should follow the system style by default."),

                adw::ComboRow {
                    set_title: "Style",
                    set_subtitle: "Use the system preference or force a light or dark appearance",
                    set_model: Some(&gtk::StringList::new(&["System", "Light", "Dark"])),
                    set_selected: theme_index(model.theme_mode),
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
                    set_subtitle: "Notify when CO2, AQI, particles, VOC, NOx, or humidity need attention",

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

                adw::ActionRow {
                    set_title: "Save Settings",
                    set_subtitle: "Save the server URL and restart the refresh timer",
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
            theme_mode: init.theme_mode,
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
            SettingsInput::ThemeSelected(index) => {
                let mode = THEME_CHOICES
                    .get(index as usize)
                    .copied()
                    .unwrap_or(ThemeMode::System);
                self.theme_mode = mode;
                let _ = sender.output(SettingsOutput::ThemeChanged(mode));
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
    use super::{theme_index, Settings};
    use crate::config::{RefreshInterval, MIN_REFRESH_INTERVAL_SECS};
    use crate::state::ThemeMode;

    fn form(url: &str, interval_secs: f64) -> Settings {
        Settings {
            url_text: url.to_string(),
            interval_secs,
            notifications_enabled: true,
            start_minimized: false,
            theme_mode: ThemeMode::System,
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
    fn theme_index_matches_dropdown_order() {
        assert_eq!(theme_index(ThemeMode::System), 0);
        assert_eq!(theme_index(ThemeMode::Light), 1);
        assert_eq!(theme_index(ThemeMode::Dark), 2);
    }
}
