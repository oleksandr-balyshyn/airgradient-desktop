//! Large Air Quality Index card.
//!
//! AQI is the headline number on the dashboard, so it gets a dedicated layout
//! with the scale name ("Moderate") and a sentence explaining what that means,
//! instead of reusing the compact `SensorCard`.

use relm4::gtk::prelude::*;
use relm4::{gtk, prelude::*};

use super::status::aqi_class;
use super::trend::{format_metric_value, Trend};

#[derive(Debug)]
pub enum AqiCardInput {
    Show {
        value: Option<f32>,
        previous: Option<f32>,
    },
}

pub struct AqiCard {
    value: Option<f32>,
    trend: Trend,
}

impl AqiCard {
    fn value_text(&self) -> String {
        self.value
            .map_or_else(|| "--".to_string(), format_metric_value)
    }

    fn level_text(&self) -> &'static str {
        self.value.map_or("Unknown", aqi_level)
    }

    fn description_text(&self) -> &'static str {
        self.value
            .map_or("Waiting for a measurement.", aqi_description)
    }
}

#[relm4::component(pub)]
impl SimpleComponent for AqiCard {
    type Init = ();
    type Input = AqiCardInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 18,
            set_hexpand: true,
            set_vexpand: true,
            #[watch]
            set_css_classes: &["card", "aqi-card", aqi_class(model.value)],

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_valign: gtk::Align::Center,
                add_css_class: "aqi-icon",

                gtk::Image {
                    set_icon_name: Some("airgradient-air-quality-symbolic"),
                    set_pixel_size: 56,
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 4,
                set_hexpand: true,
                set_valign: gtk::Align::Center,

                gtk::Label {
                    set_label: "Air Quality Index",
                    set_halign: gtk::Align::Start,
                    add_css_class: "metric-title",
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_valign: gtk::Align::Baseline,

                    gtk::Label {
                        #[watch]
                        set_label: &model.value_text(),
                        set_halign: gtk::Align::Start,
                        add_css_class: "aqi-value",
                    },

                    gtk::Label {
                        #[watch]
                        set_label: model.level_text(),
                        set_halign: gtk::Align::Start,
                        add_css_class: "aqi-level",
                    },
                },

                gtk::Label {
                    #[watch]
                    set_label: model.description_text(),
                    set_halign: gtk::Align::Start,
                    set_wrap: true,
                    add_css_class: "metric-unit",
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_margin_top: 6,

                    gtk::Label {
                        #[watch]
                        set_label: model.trend.text.as_str(),
                        #[watch]
                        set_css_classes: &["trend-value", model.trend.direction.css_class()],
                        set_halign: gtk::Align::Start,
                    },

                    gtk::Label {
                        set_label: "from last reading",
                        set_halign: gtk::Align::Start,
                        set_css_classes: &["metric-unit", "trend-context"],
                    },
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            value: None,
            trend: Trend::default(),
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            AqiCardInput::Show { value, previous } => {
                self.value = value;
                self.trend = Trend::between(value, previous, "AQI", true);
            }
        }
    }
}

/// Name of the US AQI band a value falls into.
fn aqi_level(value: f32) -> &'static str {
    match value {
        value if value <= 50.0 => "Good",
        value if value <= 100.0 => "Moderate",
        value if value <= 150.0 => "Unhealthy for Sensitive Groups",
        value if value <= 200.0 => "Unhealthy",
        value if value <= 300.0 => "Very Unhealthy",
        _ => "Hazardous",
    }
}

/// One-sentence explanation of what the band means for the people in the room.
fn aqi_description(value: f32) -> &'static str {
    match value {
        value if value <= 50.0 => {
            "Air quality is satisfactory, and air pollution poses little or no risk."
        }
        value if value <= 100.0 => {
            "Air quality is acceptable, but unusually sensitive people may notice effects."
        }
        value if value <= 150.0 => "Sensitive groups may experience health effects.",
        value if value <= 200.0 => {
            "Some members of the general public may experience health effects."
        }
        value if value <= 300.0 => {
            "Health alert: risk of health effects is increased for everyone."
        }
        _ => "Health warning: everyone is more likely to be affected.",
    }
}

#[cfg(test)]
mod tests {
    use super::{aqi_description, aqi_level};

    #[test]
    fn levels_cover_the_whole_scale() {
        assert_eq!(aqi_level(0.0), "Good");
        assert_eq!(aqi_level(50.0), "Good");
        assert_eq!(aqi_level(100.0), "Moderate");
        assert_eq!(aqi_level(150.0), "Unhealthy for Sensitive Groups");
        assert_eq!(aqi_level(200.0), "Unhealthy");
        assert_eq!(aqi_level(300.0), "Very Unhealthy");
        assert_eq!(aqi_level(301.0), "Hazardous");
    }

    #[test]
    fn every_level_has_a_description() {
        for value in [0.0, 75.0, 125.0, 175.0, 250.0, 400.0] {
            assert!(!aqi_description(value).is_empty());
        }
    }
}
