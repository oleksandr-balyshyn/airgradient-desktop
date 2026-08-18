//! Root component: the window, the header bar, navigation, and the refresh loop.
//!
//! This is the only component that talks to the outside world. It owns the
//! config file, the alert policy, the HTTP fetches, and the tray icon, and it
//! passes finished results down to child components as messages. The children
//! (dashboard, settings, welcome, help) never do I/O themselves.
//!
//! Two things run on a schedule, and both are driven by a single one-second
//! "tick" command rather than by several GLib timers:
//!
//! * the "Last updated: 17s ago" label in the header, and
//! * the automatic refresh, which fires when enough ticks have passed.
//!
//! Using one ticker means changing the refresh interval in Settings needs no
//! timer bookkeeping at all — the next tick simply compares against the new
//! interval.

use std::rc::Rc;
use std::time::{Duration, Instant};

use relm4::adw::prelude::*;
use relm4::gtk::gio;
use relm4::{adw, gtk, prelude::*};

use super::dashboard::{Dashboard, DashboardInput};
use super::help::Help;
use super::history_view::{HistoryView, HistoryViewInput};
use super::settings::{Settings, SettingsInit, SettingsInput, SettingsOutput};
use super::theming;
use super::tray::{self, TrayHandle};
use super::welcome::{Welcome, WelcomeOutput};
use crate::alerts::{AlertMonitor, AlertNotification, AlertSeverity};
use crate::app_info::APP_NAME;
use crate::config::{self, AppConfig, LoadedConfig};
use crate::device::{fetch_current_measurements, DeviceBaseUrl};
use crate::history::{History, Sample};
use crate::notifications::send_air_quality_notification;
use crate::sensors::AirMeasureSnapshot;
use crate::state::Page;
use crate::theme::{self, Theme};

/// Names of the two top-level tabs inside the measurement view.
const MAIN_TAB: &str = "main";
const HISTORY_TAB: &str = "history";

const DEFAULT_WIDTH: i32 = 1180;
const DEFAULT_HEIGHT: i32 = 780;

/// How often the shared ticker fires.
const TICK: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub enum AppInput {
    /// Show a specific page.
    Navigate(Page),
    /// Show whichever page is "home" right now: the dashboard once a device is
    /// configured, otherwise onboarding.
    GoHome,
    /// Fetch measurements now.
    Refresh,
    /// Bring the window back from the tray or from a notification click.
    ShowWindow,
    /// Hide the window but keep polling in the background.
    HideWindow,
    /// Exit for real.
    Quit,
    /// Persist a validated configuration from the settings page.
    SaveConfig(Box<AppConfig>),
    /// Preview a theme immediately, without saving it.
    ThemeChanged(&'static Theme),
    /// Send a sample notification so the user can verify delivery.
    SendTestNotification,
}

#[derive(Debug)]
pub enum AppCommand {
    /// One second has passed.
    Tick,
    /// A fetch finished, successfully or not.
    Fetched(Result<Box<AirMeasureSnapshot>, String>),
}

pub struct App {
    page: Page,
    server_url: Option<DeviceBaseUrl>,
    /// Seconds between automatic refreshes.
    refresh_interval_secs: u64,
    /// Ticks counted since the last fetch finished.
    seconds_since_fetch: u64,
    /// Whether a fetch is already on the blocking pool.
    ///
    /// The shortest refresh interval (5s) is under the request timeout (8s), so
    /// a slow device would otherwise stack up overlapping requests, and an early
    /// reply arriving after a late one would be recorded as the newer reading.
    fetch_in_flight: bool,
    last_updated: Option<Instant>,
    /// Recorded measurements, kept across restarts.
    history: History,
    alerts: AlertMonitor,
    dashboard: Controller<Dashboard>,
    history_view: Controller<HistoryView>,
    settings: Controller<Settings>,
    /// Held only to keep the page alive; dropping a controller destroys its
    /// component and would leave an empty page behind in the stack.
    _welcome: Controller<Welcome>,
    _help: Controller<Help>,
    /// Keeps the application alive while the window is hidden in the tray.
    _hold: gio::ApplicationHoldGuard,
    /// `None` when the desktop has no StatusNotifier host.
    _tray: Option<TrayHandle>,
}

impl App {
    fn has_server_url(&self) -> bool {
        self.server_url.is_some()
    }

    /// Page to show when the user asks for "home".
    fn home_page(&self) -> Page {
        if self.has_server_url() {
            Page::Dashboard
        } else {
            Page::Welcome
        }
    }

    /// Human-readable age of the most recent successful fetch.
    fn last_updated_text(&self) -> String {
        let Some(last) = self.last_updated else {
            return "Last updated: not yet".to_string();
        };

        let seconds = Instant::now().saturating_duration_since(last).as_secs();
        match seconds {
            0..=4 => "Last updated: just now".to_string(),
            5..=59 => format!("Last updated: {seconds}s ago"),
            _ => format!("Last updated: {}m {}s ago", seconds / 60, seconds % 60),
        }
    }

    /// Start a fetch on a background thread, if a device is configured.
    ///
    /// `fetch_current_measurements` blocks, so it must not run on the UI thread.
    /// `spawn_oneshot_command` puts it on Relm4's blocking thread pool and
    /// delivers the result back as an `AppCommand::Fetched` message.
    /// A fetch already running is left to finish rather than being duplicated;
    /// its result is just as fresh as a second request's would be.
    fn start_fetch(&mut self, sender: &ComponentSender<Self>) {
        if self.fetch_in_flight {
            return;
        }

        let Some(base_url) = self.server_url.clone() else {
            self.dashboard.emit(DashboardInput::SetStatus(
                "No server URL configured.".into(),
            ));
            return;
        };

        self.dashboard
            .emit(DashboardInput::SetStatus("Fetching measurements...".into()));
        self.fetch_in_flight = true;
        sender.spawn_oneshot_command(move || {
            AppCommand::Fetched(
                fetch_current_measurements(&base_url)
                    .map(Box::new)
                    .map_err(|err| err.to_string()),
            )
        });
    }

    /// Hand an alert to the desktop, reporting delivery failures on stderr.
    fn deliver(alert: AlertNotification) {
        if let Err(err) = send_air_quality_notification(&relm4::main_adw_application(), alert) {
            eprintln!("System notification failed: {err}");
        }
    }
}

#[relm4::component(pub)]
impl Component for App {
    type Init = LoadedConfig;
    type Input = AppInput;
    type Output = ();
    type CommandOutput = AppCommand;

    view! {
        adw::ApplicationWindow {
            set_title: Some(APP_NAME),
            set_default_width: DEFAULT_WIDTH,
            set_default_height: DEFAULT_HEIGHT,

            // Closing the window means "keep running in the tray". Quitting is
            // an explicit action from the tray menu or the app action.
            connect_close_request[sender] => move |_| {
                sender.input(AppInput::HideWindow);
                gtk::glib::Propagation::Stop
            },

            adw::ToolbarView {
                add_top_bar = &adw::HeaderBar {
                    add_css_class: "flat",

                    #[wrap(Some)]
                    set_title_widget = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::Center,

                        // Tabs are only meaningful on the measurement pages, so
                        // Settings, Help and onboarding show the app name instead.
                        adw::ViewSwitcher {
                            set_stack: Some(view_stack),
                            set_policy: adw::ViewSwitcherPolicy::Wide,
                            #[watch]
                            set_visible: model.page == Page::Dashboard,
                        },

                        gtk::Label {
                            set_label: APP_NAME,
                            add_css_class: "title",
                            #[watch]
                            set_visible: model.page != Page::Dashboard,
                        },
                    },

                    pack_start = &gtk::Button {
                        set_icon_name: "go-home-symbolic",
                        set_tooltip_text: Some("Home"),
                        connect_clicked => AppInput::GoHome,
                    },

                    pack_start = &gtk::Button {
                        set_icon_name: "view-refresh-symbolic",
                        set_tooltip_text: Some("Refresh measurements"),
                        connect_clicked => AppInput::Refresh,
                    },

                    pack_start = &gtk::Label {
                        #[watch]
                        set_label: &model.last_updated_text(),
                        add_css_class: "dim-label",
                    },

                    pack_end = &gtk::Button {
                        set_icon_name: "help-about-symbolic",
                        set_tooltip_text: Some("Help"),
                        connect_clicked => AppInput::Navigate(Page::Help),
                    },

                    pack_end = &gtk::Button {
                        set_icon_name: "preferences-system-symbolic",
                        set_tooltip_text: Some("Settings"),
                        connect_clicked => AppInput::Navigate(Page::Settings),
                    },
                },

                #[wrap(Some)]
                set_content = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_margin_all: 12,
                    set_vexpand: true,
                    add_css_class: "app-shell",

                    #[local_ref]
                    stack -> gtk::Stack {
                        set_vexpand: true,
                        set_hexpand: true,
                        set_transition_type: gtk::StackTransitionType::Crossfade,
                        #[watch]
                        set_visible_child_name: model.page.id(),
                    },
                },
            },
        }
    }

    fn init(
        loaded: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let LoadedConfig {
            config,
            startup_notice,
        } = loaded;

        // Apply the saved theme before any widget exists, so the window is
        // never painted in the wrong colours and then corrected.
        theming::apply(theme::find(&config.theme));

        let dashboard = Dashboard::builder().launch(()).detach();
        let history_view = HistoryView::builder().launch(()).detach();
        let settings = Settings::builder()
            .launch(SettingsInit {
                server_url: config
                    .server_url
                    .as_ref()
                    .map(|url| url.as_str().to_string()),
                refresh_interval: config.refresh_interval,
                notifications_enabled: config.notifications_enabled,
                start_minimized: config.start_minimized,
                theme_id: config.theme.clone(),
                status: startup_notice.as_ref().map_or_else(
                    || "Enter a URL like http://192.168.1.201".to_string(),
                    |notice| notice.user_message(),
                ),
            })
            .forward(sender.input_sender(), |output| match output {
                SettingsOutput::Save(config) => AppInput::SaveConfig(config),
                SettingsOutput::ThemeChanged(mode) => AppInput::ThemeChanged(mode),
                SettingsOutput::TestNotification => AppInput::SendTestNotification,
            });
        let welcome =
            Welcome::builder()
                .launch(startup_notice)
                .forward(sender.input_sender(), |output| match output {
                    WelcomeOutput::OpenSettings => AppInput::Navigate(Page::Settings),
                });
        let help = Help::builder().launch(()).detach();

        // The two top-level tabs. `ViewStack` rather than `gtk::Stack` because it
        // is what an `AdwViewSwitcher` binds to, giving the header the standard
        // GNOME tab control for free.
        let view_stack = adw::ViewStack::new();
        view_stack.add_titled_with_icon(
            dashboard.widget(),
            Some(MAIN_TAB),
            "Main",
            "view-grid-symbolic",
        );
        view_stack.add_titled_with_icon(
            history_view.widget(),
            Some(HISTORY_TAB),
            "History",
            "document-open-recent-symbolic",
        );

        // The outer stack is assembled here rather than in `view!` because each
        // page needs a stable name to switch to, and those names come from `Page`.
        let stack = gtk::Stack::new();
        stack.add_named(welcome.widget(), Some(Page::Welcome.id()));
        stack.add_named(&view_stack, Some(Page::Dashboard.id()));
        stack.add_named(settings.widget(), Some(Page::Settings.id()));
        stack.add_named(help.widget(), Some(Page::Help.id()));

        let mut model = Self {
            // A configured device means there is data worth showing immediately.
            page: if config.server_url.is_some() {
                Page::Dashboard
            } else {
                Page::Welcome
            },
            server_url: config.server_url.clone(),
            refresh_interval_secs: config.refresh_interval.as_secs(),
            seconds_since_fetch: 0,
            fetch_in_flight: false,
            last_updated: None,
            history: History::load(),
            alerts: AlertMonitor::new(config.notifications_enabled),
            dashboard,
            history_view,
            settings,
            _welcome: welcome,
            _help: help,
            _hold: relm4::main_application().hold(),
            _tray: tray::install(sender.input_sender().clone()),
        };

        // Showing the last recorded reading straight away means the dashboard is
        // populated while the first fetch is still in flight, instead of showing
        // a screen full of dashes for a second or two.
        if let Some(latest) = model.history.latest() {
            model
                .dashboard
                .emit(DashboardInput::Show(Box::new(latest.clone())));
        }
        model.publish_history();

        model.dashboard.emit(DashboardInput::SetServerUrl(
            config
                .server_url
                .as_ref()
                .map(|url| url.as_str().to_string()),
        ));

        install_app_actions(&sender);

        // One ticker drives both the "last updated" text and auto-refresh. It is
        // registered with the component's shutdown receiver so it stops when the
        // component does, instead of leaking a timer.
        sender.command(|out, shutdown| {
            shutdown
                .register(async move {
                    let mut ticker = relm4::tokio::time::interval(TICK);
                    loop {
                        ticker.tick().await;
                        if out.send(AppCommand::Tick).is_err() {
                            break;
                        }
                    }
                })
                .drop_on_shutdown()
        });

        if model.has_server_url() {
            model.start_fetch(&sender);
        }

        let stack = &stack;
        let view_stack = &view_stack;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            AppInput::Navigate(page) => {
                self.page = page;
                // Returning to the dashboard is an explicit request to see
                // current data, so refresh while the user is looking at it.
                if page == Page::Dashboard && self.has_server_url() {
                    self.start_fetch(&sender);
                }
            }
            AppInput::GoHome => sender.input(AppInput::Navigate(self.home_page())),
            AppInput::Refresh => self.start_fetch(&sender),
            AppInput::ShowWindow => {
                self.page = if self.has_server_url() {
                    Page::Dashboard
                } else {
                    Page::Settings
                };
                root.present();
            }
            AppInput::HideWindow => root.set_visible(false),
            AppInput::Quit => relm4::main_application().quit(),
            AppInput::ThemeChanged(theme) => theming::apply(theme),
            AppInput::SendTestNotification => {
                let result = send_air_quality_notification(
                    &relm4::main_adw_application(),
                    AlertNotification {
                        id: "airgradient-test-notification".into(),
                        title: "Air Monitor test notification".into(),
                        body: "Notifications are working. Click this notification to open the \
                               dashboard."
                            .into(),
                        severity: AlertSeverity::Notice,
                    },
                );
                self.settings.emit(SettingsInput::SetStatus(match result {
                    Ok(()) => "Test notification sent.".to_string(),
                    Err(err) => format!("Test notification failed: {err}"),
                }));
            }
            AppInput::SaveConfig(config) => self.save_config(*config, &sender),
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AppCommand::Tick => {
                self.seconds_since_fetch = self.seconds_since_fetch.saturating_add(1);
                if self.has_server_url() && self.seconds_since_fetch >= self.refresh_interval_secs {
                    self.start_fetch(&sender);
                }
            }
            AppCommand::Fetched(Ok(snapshot)) => {
                self.finish_fetch();
                self.last_updated = Some(Instant::now());
                self.record(snapshot.as_ref().clone());
                for alert in self.alerts.evaluate(&snapshot) {
                    Self::deliver(alert);
                }
                self.dashboard.emit(DashboardInput::Show(snapshot));
            }
            AppCommand::Fetched(Err(err)) => {
                self.finish_fetch();
                self.dashboard
                    .emit(DashboardInput::SetStatus(format!("Fetch failed: {err}")));
                if let Some(alert) = self.alerts.record_fetch_error(&err) {
                    Self::deliver(alert);
                }
            }
        }
    }
}

impl App {
    /// Mark the outstanding fetch as done and restart the interval countdown.
    ///
    /// The countdown runs from when a fetch finishes, not from when it starts, so
    /// the configured interval is the gap between requests. Counting from the
    /// start would let a device slower than the interval be polled continuously.
    fn finish_fetch(&mut self) {
        self.fetch_in_flight = false;
        self.seconds_since_fetch = 0;
    }

    /// Hand the recorded history to both views.
    ///
    /// One allocation is shared between them: a day of readings copied twice per
    /// refresh would be pure waste.
    fn publish_history(&self) {
        let snapshots = Rc::new(self.history.snapshots());
        self.dashboard
            .emit(DashboardInput::ShowHistory(Rc::clone(&snapshots)));
        self.history_view.emit(HistoryViewInput::Show(snapshots));
    }

    /// Add a reading to the history and write it out.
    ///
    /// A failed write is reported but not surfaced in the UI: losing recorded
    /// history is not worth interrupting someone who is looking at live air
    /// quality, and the next successful write recovers everything still in memory.
    fn record(&mut self, snapshot: AirMeasureSnapshot) {
        self.history.push(Sample::now(snapshot));

        if let Err(err) = self.history.save() {
            eprintln!("Could not save measurement history: {err}");
        }

        self.publish_history();
    }

    /// Persist a validated configuration and apply it to the running app.
    fn save_config(&mut self, config: AppConfig, sender: &ComponentSender<Self>) {
        if let Err(err) = config::write_config(&config) {
            self.settings
                .emit(SettingsInput::SetStatus(format!("Failed to save: {err}")));
            return;
        }

        self.server_url = config.server_url.clone();
        self.refresh_interval_secs = config.refresh_interval.as_secs();
        self.alerts.set_enabled(config.notifications_enabled);
        self.last_updated = None;

        let url_text = config
            .server_url
            .as_ref()
            .map(|url| url.as_str().to_string());
        self.dashboard
            .emit(DashboardInput::SetServerUrl(url_text.clone()));

        match url_text {
            Some(_) => {
                self.page = Page::Dashboard;
                self.settings.emit(SettingsInput::SetStatus(
                    "Saved. Refreshing dashboard.".into(),
                ));
                self.start_fetch(sender);
            }
            None => {
                self.page = Page::Welcome;
                self.dashboard
                    .emit(DashboardInput::SetStatus("Server URL removed.".into()));
                self.settings.emit(SettingsInput::SetStatus(
                    "Cleared URL. Returning to Welcome.".into(),
                ));
            }
        }
    }
}

/// Register the application-level actions used outside the window.
///
/// `app.show-dashboard` is what a notification's default action and its "Open
/// Dashboard" button activate, so clicking an alert brings the app forward.
fn install_app_actions(sender: &ComponentSender<App>) {
    let app = relm4::main_application();

    let show_dashboard = gio::SimpleAction::new("show-dashboard", None);
    show_dashboard.connect_activate({
        let sender = sender.input_sender().clone();
        move |_, _| {
            let _ = sender.send(AppInput::ShowWindow);
        }
    });
    app.add_action(&show_dashboard);

    let quit = gio::SimpleAction::new("quit", None);
    quit.connect_activate({
        let sender = sender.input_sender().clone();
        move |_, _| {
            let _ = sender.send(AppInput::Quit);
        }
    });
    app.add_action(&quit);
}
