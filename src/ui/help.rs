//! Static help page explaining how to connect the app to a device.

use relm4::adw::prelude::*;
use relm4::{adw, prelude::*};

pub struct Help;

#[relm4::component(pub)]
impl SimpleComponent for Help {
    type Init = ();
    type Input = ();
    type Output = ();

    view! {
        adw::PreferencesPage {
            set_title: "Help",
            set_icon_name: Some("help-about-symbolic"),

            adw::PreferencesGroup {
                set_title: "Setup",
                set_description: Some("How to connect the app to an AirGradient local server."),

                adw::ActionRow {
                    set_title: "Configure the Server",
                    set_subtitle: "Open Settings and enter the local-server base URL.",
                },

                adw::ActionRow {
                    set_title: "Fetch Measurements",
                    set_subtitle: "Save settings to poll /measures/current automatically.",
                },

                adw::ActionRow {
                    set_title: "Refresh Manually",
                    set_subtitle: "Use the refresh button in the header bar for an immediate update.",
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}
