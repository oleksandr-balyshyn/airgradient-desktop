//! The pages the main window can show.
//!
//! `Page::id()` returns the name a page's widget is registered under in the root
//! `gtk::Stack`, so switching pages is a lookup by a value the compiler checks
//! rather than a bare string spelled out twice. `ui::app` owns that stack and
//! the current page, and is the only consumer: the page components themselves
//! never refer to this enum.

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
}
