//! Reusable pollutant metric card.
//!
//! This is a Relm4 component, which means it owns a small piece of state (the
//! reading it is displaying) and a small widget tree, and the two are kept in
//! sync automatically: the `view!` block below declares the widgets *and* which
//! parts of them depend on the model. Anything marked `#[watch]` is re-evaluated
//! whenever the model changes, so there is no manual "go and update that label"
//! code anywhere.
//!
//! CO2, TVOC, NOx and all the particulate-matter readings use this one card.

use relm4::gtk::prelude::*;
use relm4::{gtk, prelude::*};

use super::status::UNKNOWN_CLASS;
use super::trend::{format_optional_metric_value, Trend};

/// How much visual weight a card gets.
///
/// The dashboard packs three gas cards on one row and four particulate cards on
/// another, so the same component needs to shrink without a separate widget tree
/// for each size.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CardSize {
    /// For the three-across gas row.
    Narrow,
    /// Smallest, for the four-across particulate row.
    Compact,
}

impl CardSize {
    const fn css_class(self) -> &'static str {
        match self {
            Self::Narrow => "narrow-sensor-card",
            Self::Compact => "compact-sensor-card",
        }
    }

    const fn icon_pixel_size(self) -> i32 {
        match self {
            Self::Narrow => 36,
            Self::Compact => 28,
        }
    }

    const fn spacing(self) -> i32 {
        match self {
            Self::Narrow => 10,
            Self::Compact => 8,
        }
    }
}

/// Everything the card needs before it has any data to show.
///
/// The title and icon are `&'static str` because they come from the metric
/// table and never change; only the unit can be replaced at runtime, by a
/// device that reports VOC or NOx as a concentration rather than an index.
#[derive(Debug, Clone, Copy)]
pub struct SensorCardInit {
    pub title: &'static str,
    pub unit: &'static str,
    pub icon_name: &'static str,
    pub size: CardSize,
}

/// One reading to display, already classified and formatted by the dashboard.
///
/// The card deliberately does not know about thresholds or JSON payloads. It is
/// handed a value, a unit, a CSS class, and a trend, and it renders them.
#[derive(Debug, Clone)]
pub struct Reading {
    pub value: Option<f32>,
    /// Some devices report VOC/NOx as an index rather than ppb, so the unit can
    /// change at runtime. `None` keeps the unit the card was created with.
    ///
    /// Static because every unit in the app is one of a handful of compile-time
    /// strings, whichever device reported it.
    pub unit: Option<&'static str>,
    pub status_class: &'static str,
    pub trend: Trend,
}

#[derive(Debug)]
pub enum SensorCardInput {
    Show(Reading),
}

pub struct SensorCard {
    title: &'static str,
    unit: &'static str,
    icon_name: &'static str,
    size: CardSize,
    value: Option<f32>,
    status_class: &'static str,
    trend: Trend,
}

impl SensorCard {
    /// Full class list for the card root.
    ///
    /// Relm4 sets the whole list at once rather than adding and removing single
    /// classes. That avoids the classic GTK bug where a stale status class is
    /// left behind and the final color depends on stylesheet ordering.
    fn css_classes(&self) -> [&str; 5] {
        [
            "card",
            "metric-card",
            "sensor-card",
            self.size.css_class(),
            self.status_class,
        ]
    }

    fn value_text(&self) -> String {
        format_optional_metric_value(self.value)
    }
}

#[relm4::component(pub)]
impl SimpleComponent for SensorCard {
    type Init = SensorCardInit;
    type Input = SensorCardInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: model.size.spacing(),
            set_halign: gtk::Align::Fill,
            set_valign: gtk::Align::Fill,
            #[watch]
            set_css_classes: &model.css_classes(),

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_valign: gtk::Align::Center,
                add_css_class: "metric-icon",

                gtk::Image {
                    set_icon_name: Some(model.icon_name),
                    set_pixel_size: model.size.icon_pixel_size(),
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 4,
                set_hexpand: true,
                set_valign: gtk::Align::Center,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,
                    set_hexpand: true,

                    gtk::Label {
                        set_label: model.title,
                        set_halign: gtk::Align::Start,
                        add_css_class: "metric-title",
                    },

                    // The status dot is a plain box colored entirely by CSS,
                    // which is why the card only ever passes a class name around.
                    gtk::Box {
                        set_size_request: (8, 8),
                        set_valign: gtk::Align::Center,
                        add_css_class: "status-dot",
                    },
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.value_text(),
                    set_halign: gtk::Align::Start,
                    add_css_class: "metric-value",
                },

                gtk::Label {
                    #[watch]
                    set_label: model.unit,
                    set_halign: gtk::Align::Start,
                    add_css_class: "metric-unit",
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_margin_top: 8,

                    gtk::Label {
                        #[watch]
                        set_label: model.trend.text.as_str(),
                        #[watch]
                        set_css_classes: &["trend-value", model.trend.direction.css_class()],
                        set_halign: gtk::Align::Start,
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            title: init.title,
            unit: init.unit,
            icon_name: init.icon_name,
            size: init.size,
            value: None,
            status_class: UNKNOWN_CLASS,
            trend: Trend::default(),
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            SensorCardInput::Show(reading) => {
                self.value = reading.value;
                if let Some(unit) = reading.unit {
                    self.unit = unit;
                }
                self.status_class = reading.status_class;
                self.trend = reading.trend;
            }
        }
    }
}
