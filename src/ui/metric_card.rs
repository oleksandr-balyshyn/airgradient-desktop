//! A card showing one metric's current reading and its history as a chart.
//!
//! The same component serves two places, differing only in chart height: the
//! main view uses a tall one for PM2.5, and the history view uses a short one for
//! every metric.

use relm4::gtk::prelude::*;
use relm4::{gtk, prelude::*};

use super::chart::{summary, Chart, ChartInput, Summary};
use super::metrics::Metric;
use super::status::UNKNOWN_CLASS;
use super::trend::format_metric_value;

/// Which metric to show, and how tall to draw its chart.
pub struct MetricCardInit {
    pub metric: &'static Metric,
    pub chart_height: i32,
}

#[derive(Debug)]
pub enum MetricCardInput {
    Show {
        /// Latest reading, or `None` when the sensor is not reporting.
        current: Option<f32>,
        /// Unit reported alongside the reading, when it differs from the default.
        unit: Option<String>,
        /// Readings over time, oldest first.
        series: Vec<f32>,
    },
}

pub struct MetricCard {
    metric: &'static Metric,
    unit: String,
    current: Option<f32>,
    /// Lowest, mean and highest reading in the plotted range.
    range: Option<Summary>,
    status_class: &'static str,
    chart: Controller<Chart>,
}

impl MetricCard {
    fn css_classes(&self) -> [&str; 4] {
        ["card", "metric-card", "history-card", self.status_class]
    }

    fn value_text(&self) -> String {
        self.current
            .map_or_else(|| "--".to_string(), format_metric_value)
    }

    /// The summary line under the chart, for example `low 12 · avg 20 · high 34`.
    fn range_text(&self) -> String {
        match self.range {
            Some(Summary { low, mean, high }) => format!(
                "low {} · avg {} · high {}",
                format_metric_value(low),
                format_metric_value(mean),
                format_metric_value(high)
            ),
            None => "No readings recorded yet".to_string(),
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for MetricCard {
    type Init = MetricCardInit;
    type Input = MetricCardInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            set_hexpand: true,
            #[watch]
            set_css_classes: &model.css_classes(),

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Image {
                    set_icon_name: Some(model.metric.icon),
                    set_pixel_size: 20,
                    add_css_class: "metric-icon",
                },

                gtk::Label {
                    set_label: model.metric.title,
                    set_halign: gtk::Align::Start,
                    add_css_class: "metric-title",
                },

                gtk::Box {
                    set_size_request: (8, 8),
                    set_valign: gtk::Align::Center,
                    add_css_class: "status-dot",
                },

                // Pushes the reading to the right-hand edge of the card.
                gtk::Box {
                    set_hexpand: true,
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.value_text(),
                    set_halign: gtk::Align::End,
                    add_css_class: "history-value",
                },

                gtk::Label {
                    #[watch]
                    set_label: model.unit.as_str(),
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Baseline,
                    add_css_class: "metric-unit",
                },
            },

            #[local_ref]
            chart_widget -> gtk::Box {},

            gtk::Label {
                #[watch]
                set_label: &model.range_text(),
                set_halign: gtk::Align::Start,
                set_css_classes: &["metric-unit", "trend-context"],
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            metric: init.metric,
            unit: init.metric.unit.to_string(),
            current: None,
            range: None,
            status_class: UNKNOWN_CLASS,
            chart: Chart::builder().launch(init.chart_height).detach(),
        };

        let chart_widget = model.chart.widget();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            MetricCardInput::Show {
                current,
                unit,
                series,
            } => {
                self.current = current;
                if let Some(unit) = unit {
                    self.unit = unit;
                }
                self.status_class = self.metric.status_class(current);
                self.range = summary(&series);
                self.chart.emit(ChartInput::SetSeries(series));
            }
        }
    }
}
