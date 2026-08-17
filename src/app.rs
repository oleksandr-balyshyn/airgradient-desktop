//! Application bootstrap.
//!
//! This module turns a process launch into a running Relm4 application: read the
//! saved configuration, register icons and styles, then hand control to the root
//! component in `ui::app`.

use relm4::RelmApp;

use crate::app_info::APP_ID;
use crate::config::read_config;
use crate::ui::{self, app::App};

/// Command-line flags that mean "start hidden in the tray".
///
/// Desktop environments and autostart entries use different spellings, so all
/// the common ones are accepted.
const MINIMIZED_FLAGS: [&str; 4] = [
    "--minimized",
    "--background",
    "--hidden",
    "--start-minimized",
];

pub fn run() {
    let started_minimized = std::env::args().any(|arg| MINIMIZED_FLAGS.contains(&arg.as_str()));
    let loaded = read_config();
    // Starting hidden is either a launch flag or a saved preference.
    let visible_on_start = !(started_minimized || loaded.config.start_minimized);

    let app = RelmApp::new(APP_ID);
    // Resources must be registered before any widget asks for an icon by name,
    // and the stylesheet before any widget is styled, so both happen here rather
    // than inside the root component.
    ui::register_resources();
    relm4::set_global_css(ui::DASHBOARD_CSS);
    app.visible_on_activate(visible_on_start).run::<App>(loaded);
}
