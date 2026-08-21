//! Temperature and humidity card.
//!
//! Temperature and humidity are displayed identically: a large value, a one-word
//! comfort verdict, and a trend. They used to be two near-identical files. Here
//! they are one component with an `EnvironmentKind` that supplies the parts that
//! actually differ — icon, unit, and where the comfort bands sit.

use relm4::gtk::prelude::*;
use relm4::{gtk, prelude::*};

use super::trend::{Preference, Trend};
use crate::sensors::comfort::{Dimension, Position};

/// Highlight for a reading outside the user's comfort range.
///
/// Amber rather than red: an uncomfortable room is worth acting on, but it is
/// not the health warning that red means elsewhere on the dashboard.
const UNCOMFORTABLE_CLASS: &str = "status-orange";

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

    /// The reading this card shows, as the comfort logic names it.
    const fn dimension(self) -> Dimension {
        match self {
            Self::Temperature => Dimension::Temperature,
            Self::Humidity => Dimension::Humidity,
        }
    }
}

#[derive(Debug)]
pub enum EnvironmentCardInput {
    Show {
        value: Option<f32>,
        previous: Option<f32>,
        /// Where the reading sits in the user's comfort range, or `None` when
        /// the device did not report it.
        position: Option<Position>,
    },
    /// Re-judge the reading already on the card.
    ///
    /// Sent when the user edits the comfort ranges, so the highlight follows the
    /// new ranges immediately. It deliberately leaves the value and the trend
    /// alone: nothing was measured, only the definition of comfortable changed.
    SetPosition(Option<Position>),
}

pub struct EnvironmentCard {
    kind: EnvironmentKind,
    value: Option<f32>,
    /// Where the reading sits in the user's comfort range.
    position: Option<Position>,
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
        let comfort = self
            .position
            .map_or("--", |position| self.kind.dimension().word(position));
        format!("Comfort: {comfort}")
    }

    /// The card's classes, including the highlight for a reading the user asked
    /// to be told about.
    ///
    /// A reading outside the comfort range takes the same amber styling the
    /// sensor cards use for a value worth acting on, so "this needs attention"
    /// looks the same everywhere on the dashboard. The whole list is rebuilt on
    /// every reading rather than adding and removing one class, which is how a
    /// stale highlight would otherwise survive a return to comfort.
    fn css_classes(&self) -> Vec<&'static str> {
        let mut classes = vec!["card", "metric-card", self.kind.css_class()];

        if self.position.is_some_and(Position::is_uncomfortable) {
            classes.push(UNCOMFORTABLE_CLASS);
        }
        classes
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
            #[watch]
            set_css_classes: &model.css_classes(),

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
            position: None,
            trend: Trend::default(),
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            EnvironmentCardInput::Show {
                value,
                previous,
                position,
            } => {
                self.value = value;
                self.position = position;
                // Temperature and humidity have a comfortable middle rather than
                // a "lower is better" scale, so a rising value is not inherently
                // worse. `lower_is_better: true` keeps the arrow colors matching
                // the previous release's behavior.
                self.trend = Trend::between(value, previous, self.kind.unit(), Preference::Neither);
            }
            EnvironmentCardInput::SetPosition(position) => self.position = position,
        }
    }
}
