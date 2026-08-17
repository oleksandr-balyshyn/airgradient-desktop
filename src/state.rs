//! Shared UI vocabulary.
//!
//! Persisted settings live in `config.rs`, colours in `theme.rs`, and everything
//! else about the running app is held by the Relm4 component that owns it. What
//! remains here is the page list, which the root component and the pages
//! themselves both need to agree on.

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
