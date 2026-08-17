//! Temperature and humidity card.
//!
//! Temperature and humidity are displayed identically: a large value, a one-word
//! comfort verdict, and a trend. They used to be two near-identical files. Here
//! they are one component with an `EnvironmentKind` that supplies the parts that
//! actually differ — icon, unit, and where the comfort bands sit.

use relm4::gtk::prelude::*;
use relm4::{gtk, prelude::*};

use super::trend::Trend;

/// Which environmental reading a card shows.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum EnvironmentKind {
    Temperature,
    Humidity,
}

impl EnvironmentKind {
    const fn title(self) -> &'static str {
        match self {
            Self::Temperature => "Temperature",
            Self::Humidity => "Humidity",
        }
    }

    const fn icon_name(self) -> &'static str {
        match self {
            Self::Temperature => "airgradient-temperature-symbolic",
            Self::Humidity => "airgradient-humidity-symbolic",
        }
    }

    /// Card-specific CSS class, which supplies the soft background gradient.
    const fn css_class(self) -> &'static str {
        match self {
            Self::Temperature => "temperature-widget",
            Self::Humidity => "humidity-widget",
        }
    }

    /// Unit shown after the value and in the trend label.
    const fn unit(self) -> &'static str {
        match self {
            Self::Temperature => "°C",
            Self::Humidity => "%",
        }
    }

    /// Plain-language comfort verdict.
    ///
    /// These bands are deliberately simple UI labels for quick scanning. They
    /// are not medical or HVAC recommendations.
    fn comfort(self, value: f32) -> &'static str {
        match self {
            Self::Temperature => match value {
                value if value < 18.0 => "Cool",
                value if value <= 26.0 => "Comfortable",
                _ => "Warm",
            },
            Self::Humidity => match value {
                value if value < 40.0 => "Dry",
                value if value <= 60.0 => "Comfortable",
                _ => "Humid",
            },
        }
    }
}

#[derive(Debug)]
pub enum EnvironmentCardInput {
    Show {
        value: Option<f32>,
        previous: Option<f32>,
    },
}

pub struct EnvironmentCard {
    kind: EnvironmentKind,
    value: Option<f32>,
    trend: Trend,
}

impl EnvironmentCard {
    fn value_text(&self) -> String {
        match self.value {
            Some(value) => format!("{value:.0}{}", self.kind.unit()),
            None => "--".to_string(),
        }
    }

    fn comfort_text(&self) -> String {
        let comfort = self.value.map_or("--", |value| self.kind.comfort(value));
        format!("Comfort: {comfort}")
    }
}

#[relm4::component(pub)]
impl SimpleComponent for EnvironmentCard {
    type Init = EnvironmentKind;
    type Input = EnvironmentCardInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 16,
            set_hexpand: true,
            set_vexpand: true,
            set_css_classes: &["card", "metric-card", model.kind.css_class()],

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_valign: gtk::Align::Center,
                add_css_class: "metric-icon",

                gtk::Image {
                    set_icon_name: Some(model.kind.icon_name()),
                    set_pixel_size: 40,
                    set_tooltip_text: Some(model.kind.title()),
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
                    set_label: model.kind.title(),
                    set_halign: gtk::Align::Start,
                    add_css_class: "metric-title",
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.value_text(),
                    set_halign: gtk::Align::Start,
                    add_css_class: "large-value",
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.comfort_text(),
                    set_halign: gtk::Align::Start,
                    add_css_class: "metric-unit",
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_margin_top: 4,

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
        kind: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            kind,
            value: None,
            trend: Trend::default(),
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            EnvironmentCardInput::Show { value, previous } => {
                self.value = value;
                // Temperature and humidity have a comfortable middle rather than
                // a "lower is better" scale, so a rising value is not inherently
                // worse. `lower_is_better: true` keeps the arrow colors matching
                // the previous release's behavior.
                self.trend = Trend::between(value, previous, self.kind.unit(), true);
            }
        }
    }
}
