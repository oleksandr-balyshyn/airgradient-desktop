//! Shared UI vocabulary.
//!
//! Persisted settings live in `config.rs`, and everything else about the running
//! app is held by the Relm4 component that owns it. What remains here are the two
//! small enums more than one component needs to agree on.

/// The pages the window can show.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Page {
    Welcome,
    Dashboard,
    Settings,
    Help,
}

impl Page {
    /// Stable name used to identify the page inside a `gtk::Stack`.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::Dashboard => "dashboard",
            Self::Settings => "settings",
            Self::Help => "help",
        }
    }

    /// Human-readable page title.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::Dashboard => "Dashboard",
            Self::Settings => "Settings",
            Self::Help => "Help",
        }
    }
}

/// Whether to follow the desktop's light/dark preference or override it.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}
