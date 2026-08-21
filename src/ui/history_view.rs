//! History tab: every metric as a card with its own chart.
//!
//! The main view answers "what is the air like right now". This one answers "what
//! has it been doing", with the same set of readings but each drawn over time.
//!
//! The cards come from looping over `metrics::METRICS`, so this view gains a card
//! automatically when a metric is added to that table.

use std::rc::Rc;

use relm4::gtk::prelude::*;
use relm4::{adw, gtk, prelude::*};

use super::metric_card::{MetricCard, MetricCardInit, MetricCardInput};
use super::metrics::METRICS;
use crate::sensors::AirMeasureSnapshot;

/// Height of each card's chart.
const CHART_HEIGHT: i32 = 64;

/// How many cards sit side by side on a wide window.
const CARDS_PER_LINE: u32 = 2;

#[derive(Debug)]
pub enum HistoryViewInput {
    /// Every recorded snapshot, oldest first.
    ///
    /// Shared by reference because the same recording is handed to both views on
    /// every refresh, and copying a day of readings twice per refresh would be
    /// pure waste.
    Show(Rc<Vec<AirMeasureSnapshot>>),
}

pub struct HistoryView {
    /// One card per metric, in `METRICS` order.
    cards: Vec<Controller<MetricCard>>,
    recorded: usize,
}

impl HistoryView {
    fn summary_text(&self) -> String {
        match self.recorded {
            0 => "No readings recorded yet. Charts appear as measurements arrive.".to_string(),
            1 => "1 reading recorded.".to_string(),
            count => format!("{count} readings recorded, oldest first."),
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for HistoryView {
    type Init = ();
    type Input = HistoryViewInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_vexpand: true,

            gtk::ScrolledWindow {
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vexpand: true,

                adw::Clamp {
                    set_maximum_size: 1100,
                    set_tightening_threshold: 700,
                    set_hexpand: true,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
                        set_margin_all: 24,
                        add_css_class: "dashboard-page",

                        gtk::Label {
                            #[watch]
                            set_label: &model.summary_text(),
                            set_halign: gtk::Align::Start,
                            add_css_class: "dim-label",
                        },

                        #[local_ref]
                        card_grid -> gtk::FlowBox {
                            set_row_spacing: 12,
                            set_column_spacing: 12,
                            set_hexpand: true,
                            set_halign: gtk::Align::Fill,
                            set_valign: gtk::Align::Start,
                            set_selection_mode: gtk::SelectionMode::None,
                            set_min_children_per_line: 1,
                            set_max_children_per_line: CARDS_PER_LINE,
                            set_homogeneous: true,
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
        let model = Self {
            cards: METRICS
                .iter()
                .map(|metric| {
                    MetricCard::builder()
                        .launch(MetricCardInit {
                            metric,
                            chart_height: CHART_HEIGHT,
                        })
                        .detach()
                })
                .collect(),
            recorded: 0,
        };

        // The grid is filled here rather than in `view!` because the number of
        // cards comes from the metric table, not from the layout.
        let card_grid = gtk::FlowBox::new();
        for card in &model.cards {
            card_grid.insert(card.widget(), -1);
        }

        let card_grid = &card_grid;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            HistoryViewInput::Show(snapshots) => {
                self.recorded = snapshots.len();

                for (card, metric) in self.cards.iter().zip(METRICS) {
                    card.emit(MetricCardInput::show(metric, &snapshots));
                }
            }
        }
    }
}
