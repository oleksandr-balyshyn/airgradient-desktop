//! Relm4 user interface.
//!
//! Every visible part of the app is a Relm4 component. `app` is the root: it owns
//! the window and is the only component that performs I/O. The rest are pure
//! views driven by messages:
//!
//! * `dashboard` is the Main tab: it arranges the cards and splits a measurement
//!   between them,
//! * `history_view` is the History tab, one `metric_card` per metric,
//! * `metrics` is the table of what the app measures, shared by both tabs,
//! * `chart` draws a series of readings with Cairo,
//! * `sensor_card`, `environment_card` and `aqi_card` render one reading each,
//! * `settings`, `welcome` and `help` are the other pages,
//! * `tray` bridges the StatusNotifier tray thread into the UI,
//! * `theming` loads the selected theme's colours into GTK, and
//! * `trend` and `status` are pure logic with no widgets, so they are testable.

pub mod app;
mod aqi_card;
mod chart;
mod dashboard;
mod environment_card;
mod help;
mod history_view;
mod metric_card;
mod metrics;
mod sensor_card;
mod settings;
mod status;
mod theming;
mod tray;
mod trend;
mod welcome;

use relm4::gtk;
use relm4::gtk::gio;

/// Path the compiled GResource bundle is registered under.
const ICON_RESOURCE_PATH: &str = "/com/airgradient/desktop/icons";

/// Make the embedded symbolic icons available to `gtk::Image::from_icon_name`.
///
/// `build.rs` compiles `resources/icons/` into a GResource bundle at build time.
/// Registering it here, and adding its path to the icon theme, is what lets the
/// rest of the code refer to icons by name (`airgradient-co2-symbolic`) instead
/// of loading SVG files from disk at runtime.
pub fn register_resources() {
    gio::resources_register_include!("airgradient.gresource")
        .expect("AirGradient resources should be compiled into the binary");

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_resource_path(ICON_RESOURCE_PATH);
    }
}

/// The dashboard stylesheet, embedded in the binary at compile time.
///
/// Because this is `include_str!`, editing the CSS requires a rebuild.
pub const DASHBOARD_CSS: &str = include_str!("../../assets/dashboard.css");
