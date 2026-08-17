//! Applying a theme to the running app.
//!
//! `crate::theme` decides what colours a theme has; this module is the thin GTK
//! layer that puts them on screen. Two things happen when a theme is applied:
//!
//! 1. libadwaita is told whether to use its light or dark styling, and
//! 2. the theme's `@define-color` overrides are loaded into one dedicated
//!    stylesheet provider.
//!
//! That provider is created once and reused. Loading new CSS into the *same*
//! provider is what makes switching themes instant and repeatable — adding a new
//! provider per switch would stack them up, and the colours from earlier themes
//! would keep fighting the current one.

use std::cell::OnceCell;

use relm4::adw;
use relm4::gtk;

use crate::theme::{Theme, Variant};

thread_local! {
    /// The single provider holding the active theme's colours.
    static THEME_PROVIDER: OnceCell<gtk::CssProvider> = const { OnceCell::new() };
}

/// Priority above the app's own stylesheet so theme colours win.
///
/// `dashboard.css` is loaded at application priority. Theme overrides have to
/// outrank it, but must stay below any user stylesheet.
const THEME_PRIORITY: u32 = gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1;

/// Apply a theme's colour scheme and colours.
pub fn apply(theme: &Theme) {
    adw::StyleManager::default().set_color_scheme(match theme.variant {
        Variant::System => adw::ColorScheme::Default,
        Variant::Light => adw::ColorScheme::ForceLight,
        Variant::Dark => adw::ColorScheme::ForceDark,
    });

    // An empty stylesheet is what returns a theme to libadwaita's own palette,
    // so the `None` case still has to load something.
    let css = theme.css().unwrap_or_default();
    with_provider(|provider| provider.load_from_string(&css));
}

/// Run something with the shared provider, creating and installing it on first use.
fn with_provider(action: impl FnOnce(&gtk::CssProvider)) {
    THEME_PROVIDER.with(|cell| {
        let provider = cell.get_or_init(|| {
            let provider = gtk::CssProvider::new();
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_add_provider_for_display(&display, &provider, THEME_PRIORITY);
            }
            provider
        });
        action(provider);
    });
}
