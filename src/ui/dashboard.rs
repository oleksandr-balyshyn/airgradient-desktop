//! Dashboard page.
//!
//! This component owns every card on the page as a child component. When a new
//! measurement arrives it does one job: turn the raw snapshot into per-card
//! readings (value, unit, status class, trend) and hand each card its own slice.
//! The cards themselves know nothing about AirGradient payloads or thresholds.

use std::rc::Rc;

use relm4::gtk::prelude::*;
use relm4::{gtk, prelude::*};

use super::aqi_card::{AqiCard, AqiCardInput};
use super::environment_card::{EnvironmentCard, EnvironmentCardInput, EnvironmentKind};
use super::metric_card::{MetricCard, MetricCardInit, MetricCardInput};
use super::metrics::{self, CO2_ID, NOX_ID, PM003_COUNT_ID, PM10_ID, PM1_ID, PM25_ID, TVOC_ID};
use super::sensor_card::{CardSize, Reading, SensorCard, SensorCardInit, SensorCardInput};
use super::trend::{Preference, Trend};
use crate::sensors::comfort::ComfortThresholds;
use crate::sensors::{AirMeasureSnapshot, GasUnit};

/// Unit label for a gas reading, defaulting to the sensor index used by
/// AirGradient's own firmware when a payload did not say.
fn gas_unit_label(unit: Option<GasUnit>) -> &'static str {
    unit.unwrap_or(GasUnit::Index).as_str()
}

/// Unit shared by every particulate mass reading.
const MICROGRAMS: &str = "µg/m³";

/// Height of the PM2.5 history chart on the main view.
///
/// Taller than the history tab's charts because this one is the only chart on the
/// page and has the room.
const PM25_CHART_HEIGHT: i32 = 132;

#[derive(Debug)]
pub enum DashboardInput {
    /// A freshly parsed measurement to display.
    Show(Box<AirMeasureSnapshot>),
    /// Text for the "Server URL: ..." line.
    SetServerUrl(Option<String>),
    /// Text for the status line under the cards.
    SetStatus(String),
    /// Recorded readings for the PM2.5 chart, oldest first.
    ShowHistory(Rc<Vec<AirMeasureSnapshot>>),
    /// The comfort ranges the user chose, from startup or from a settings save.
    SetComfort(ComfortThresholds),
}

pub struct Dashboard {
    aqi: Controller<AqiCard>,
    temperature: Controller<EnvironmentCard>,
    humidity: Controller<EnvironmentCard>,
    co2: Controller<SensorCard>,
    tvoc: Controller<SensorCard>,
    nox: Controller<SensorCard>,
    pm003_count: Controller<SensorCard>,
    pm1: Controller<SensorCard>,
    pm25: Controller<SensorCard>,
    pm10: Controller<SensorCard>,
    /// PM2.5 over time, the one chart on the main view.
    pm25_history: Controller<MetricCard>,
    /// The previous snapshot, kept only to compute trend arrows.
    previous: Option<AirMeasureSnapshot>,
    /// Ranges the temperature and humidity cards are judged against.
    comfort: ComfortThresholds,
    /// The latest reading, kept so the two environment cards can be repainted
    /// when the comfort ranges change without waiting for the next fetch.
    latest: Option<AirMeasureSnapshot>,
    server_text: String,
    status_text: String,
}

impl Dashboard {
    /// Send one sensor card its reading.
    ///
    /// `metric_id` names the entry in `ui::metrics` that decides how the value
    /// is coloured, so the thresholds behind the main view and the History tab
    /// cannot drift apart. `unit` is the label the card shows next to the number
    /// (`None` keeps the fixed label the card was built with); `trend_unit` is
    /// the one written into the "↑ +42 ppm" line.
    ///
    /// Every reading handled here is a pollutant, so falling is an improvement.
    fn show_sensor(
        card: &Controller<SensorCard>,
        metric_id: &str,
        value: Option<f32>,
        previous: Option<f32>,
        unit: Option<&str>,
        trend_unit: &str,
    ) {
        card.emit(SensorCardInput::Show(Reading {
            value,
            unit: unit.map(str::to_string),
            status_class: metrics::find(metric_id).status_class(value),
            trend: Trend::between(value, previous, trend_unit, Preference::LowerIsBetter),
        }));
    }

    /// Send each card the part of the snapshot it is responsible for.
    fn distribute(&mut self, snapshot: &AirMeasureSnapshot) {
        let previous = self.previous.as_ref();

        self.aqi.emit(AqiCardInput::Show {
            value: snapshot.aqi,
            previous: previous.and_then(|previous| previous.aqi),
        });

        let comfort = self.comfort.assess(snapshot.temperature, snapshot.humidity);
        self.temperature.emit(EnvironmentCardInput::Show {
            value: snapshot.temperature,
            previous: previous.and_then(|previous| previous.temperature),
            position: comfort.temperature,
        });
        self.humidity.emit(EnvironmentCardInput::Show {
            value: snapshot.humidity,
            previous: previous.and_then(|previous| previous.humidity),
            position: comfort.humidity,
        });

        Self::show_sensor(
            &self.co2,
            CO2_ID,
            snapshot.co2,
            previous.and_then(|previous| previous.co2),
            None,
            "ppm",
        );

        // The gases report either a sensor index or a concentration depending on
        // the device, so their unit comes from the payload rather than the card.
        let tvoc_unit = gas_unit_label(snapshot.tvoc_unit);
        Self::show_sensor(
            &self.tvoc,
            TVOC_ID,
            snapshot.tvoc,
            previous.and_then(|previous| previous.tvoc),
            Some(tvoc_unit),
            tvoc_unit,
        );

        let nox_unit = gas_unit_label(snapshot.nox_unit);
        Self::show_sensor(
            &self.nox,
            NOX_ID,
            snapshot.nox,
            previous.and_then(|previous| previous.nox),
            Some(nox_unit),
            nox_unit,
        );

        Self::show_sensor(
            &self.pm003_count,
            PM003_COUNT_ID,
            snapshot.pm003_count,
            previous.and_then(|previous| previous.pm003_count),
            None,
            "count",
        );
        Self::show_sensor(
            &self.pm1,
            PM1_ID,
            snapshot.pm1,
            previous.and_then(|previous| previous.pm1),
            None,
            MICROGRAMS,
        );
        Self::show_sensor(
            &self.pm25,
            PM25_ID,
            snapshot.pm25,
            previous.and_then(|previous| previous.pm25),
            None,
            MICROGRAMS,
        );
        Self::show_sensor(
            &self.pm10,
            PM10_ID,
            snapshot.pm10,
            previous.and_then(|previous| previous.pm10),
            None,
            MICROGRAMS,
        );
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Dashboard {
    type Init = ();
    type Input = DashboardInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_vexpand: true,

            gtk::ScrolledWindow {
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vexpand: true,

                // `Clamp` keeps the content at a readable width on wide screens
                // instead of stretching the cards across the whole monitor.
                adw::Clamp {
                    set_maximum_size: 960,
                    set_tightening_threshold: 600,
                    set_hexpand: true,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 18,
                        set_margin_all: 24,
                        add_css_class: "dashboard-page",

                        gtk::Label {
                            #[watch]
                            set_label: model.server_text.as_str(),
                            set_halign: gtk::Align::Start,
                            add_css_class: "dim-label",
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,
                            set_hexpand: true,
                            set_homogeneous: true,
                            add_css_class: "dashboard-top",

                            #[local_ref]
                            aqi_widget -> gtk::Box {},

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 8,
                                set_hexpand: true,
                                add_css_class: "environment-stack",

                                #[local_ref]
                                temperature_widget -> gtk::Box {},

                                #[local_ref]
                                humidity_widget -> gtk::Box {},
                            },
                        },

                        // FlowBox wraps the cards at narrow widths instead of
                        // overflowing, without any manual column arithmetic.
                        gtk::FlowBox {
                            set_row_spacing: 12,
                            set_column_spacing: 12,
                            set_hexpand: true,
                            set_halign: gtk::Align::Fill,
                            set_valign: gtk::Align::Start,
                            set_selection_mode: gtk::SelectionMode::None,
                            set_min_children_per_line: 1,
                            set_max_children_per_line: 3,
                            set_homogeneous: true,
                            add_css_class: "gas-row",

                            #[local_ref]
                            co2_widget -> gtk::Box {},

                            #[local_ref]
                            tvoc_widget -> gtk::Box {},

                            #[local_ref]
                            nox_widget -> gtk::Box {},
                        },

                        gtk::FlowBox {
                            set_row_spacing: 12,
                            set_column_spacing: 12,
                            set_hexpand: true,
                            set_halign: gtk::Align::Fill,
                            set_valign: gtk::Align::Start,
                            set_selection_mode: gtk::SelectionMode::None,
                            set_min_children_per_line: 1,
                            set_max_children_per_line: 4,
                            set_homogeneous: true,
                            add_css_class: "particles-row",

                            #[local_ref]
                            pm003_widget -> gtk::Box {},

                            #[local_ref]
                            pm1_widget -> gtk::Box {},

                            #[local_ref]
                            pm25_widget -> gtk::Box {},

                            #[local_ref]
                            pm10_widget -> gtk::Box {},
                        },

                        #[local_ref]
                        pm25_history_widget -> gtk::Box {},

                        gtk::Label {
                            #[watch]
                            set_label: model.status_text.as_str(),
                            set_halign: gtk::Align::Start,
                            add_css_class: "dim-label",
                        },
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
        let gas_card = |title: &str, unit: &str, icon: &str| SensorCardInit {
            title: title.to_string(),
            unit: unit.to_string(),
            icon_name: icon.to_string(),
            size: CardSize::Narrow,
        };
        let particle_card = |title: &str, unit: &str| SensorCardInit {
            title: title.to_string(),
            unit: unit.to_string(),
            icon_name: "airgradient-particles-symbolic".to_string(),
            size: CardSize::Compact,
        };

        let model = Self {
            aqi: AqiCard::builder().launch(()).detach(),
            temperature: EnvironmentCard::builder()
                .launch(EnvironmentKind::Temperature)
                .detach(),
            humidity: EnvironmentCard::builder()
                .launch(EnvironmentKind::Humidity)
                .detach(),
            co2: SensorCard::builder()
                .launch(gas_card("CO₂", "ppm", "airgradient-co2-symbolic"))
                .detach(),
            tvoc: SensorCard::builder()
                .launch(gas_card("TVOC", "ppb", "airgradient-voc-symbolic"))
                .detach(),
            nox: SensorCard::builder()
                .launch(gas_card("NOx", "ppb", "airgradient-nox-symbolic"))
                .detach(),
            pm003_count: SensorCard::builder()
                .launch(particle_card("PM₀.₃ Count", "count"))
                .detach(),
            pm1: SensorCard::builder()
                .launch(particle_card("PM₁.₀", "µg/m³"))
                .detach(),
            pm25: SensorCard::builder()
                .launch(particle_card("PM₂.₅", "µg/m³"))
                .detach(),
            pm10: SensorCard::builder()
                .launch(particle_card("PM₁₀", "µg/m³"))
                .detach(),
            pm25_history: MetricCard::builder()
                .launch(MetricCardInit {
                    metric: metrics::find(PM25_ID),
                    chart_height: PM25_CHART_HEIGHT,
                })
                .detach(),
            previous: None,
            comfort: ComfortThresholds::default(),
            latest: None,
            server_text: "Server URL: Not configured".to_string(),
            status_text: "Press refresh to load current measurements.".to_string(),
        };

        let aqi_widget = model.aqi.widget();
        let temperature_widget = model.temperature.widget();
        let humidity_widget = model.humidity.widget();
        let co2_widget = model.co2.widget();
        let tvoc_widget = model.tvoc.widget();
        let nox_widget = model.nox.widget();
        let pm003_widget = model.pm003_count.widget();
        let pm1_widget = model.pm1.widget();
        let pm25_widget = model.pm25.widget();
        let pm10_widget = model.pm10.widget();
        let pm25_history_widget = model.pm25_history.widget();

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            DashboardInput::Show(snapshot) => {
                self.distribute(&snapshot);
                self.latest = Some(snapshot.as_ref().clone());
                self.previous = Some(*snapshot);
                self.status_text = "Latest measurements loaded.".to_string();
            }
            DashboardInput::SetServerUrl(url) => {
                self.server_text = match url {
                    Some(url) => format!("Server URL: {url}"),
                    None => "Server URL: Not configured".to_string(),
                };
            }
            DashboardInput::SetStatus(status) => self.status_text = status,
            DashboardInput::SetComfort(comfort) => {
                self.comfort = comfort;
                // Repaint the two cards now. Waiting for the next fetch would
                // leave the highlight describing the ranges the user just
                // replaced, for up to a full refresh interval.
                if let Some(latest) = self.latest.as_ref() {
                    let assessment = comfort.assess(latest.temperature, latest.humidity);
                    self.temperature
                        .emit(EnvironmentCardInput::SetPosition(assessment.temperature));
                    self.humidity
                        .emit(EnvironmentCardInput::SetPosition(assessment.humidity));
                }
            }
            DashboardInput::ShowHistory(snapshots) => {
                let metric = metrics::find(PM25_ID);
                self.pm25_history.emit(MetricCardInput::Show {
                    current: snapshots.last().and_then(|snapshot| metric.value(snapshot)),
                    unit: None,
                    series: metric.series(&snapshots),
                });
            }
        }
    }
}
