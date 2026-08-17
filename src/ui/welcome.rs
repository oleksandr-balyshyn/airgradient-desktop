//! Onboarding page shown until a device URL is configured.

use relm4::gtk::prelude::*;
use relm4::{adw, gtk, prelude::*};

use crate::config::ConfigStartupNotice;

/// Text shown when the app has never been configured.
const FIRST_RUN_DESCRIPTION: &str = "Configure the local-server base URL to start showing \
measurements. Accepted formats include http://192.168.1.201, 192.168.1.201, and \
http://192.168.1.201:80.";

/// Text shown when a config file exists but could not be used.
const RECOVERY_DESCRIPTION: &str = "Saved settings could not be loaded, so defaults are active. \
Open Settings to review and save a corrected configuration.";

#[derive(Debug)]
pub enum WelcomeOutput {
    OpenSettings,
}

pub struct Welcome {
    description: &'static str,
}

#[relm4::component(pub)]
impl SimpleComponent for Welcome {
    /// The startup notice decides which explanation the user gets.
    type Init = Option<ConfigStartupNotice>;
    type Input = ();
    type Output = WelcomeOutput;

    view! {
        adw::StatusPage {
            set_icon_name: Some("network-wireless-symbolic"),
            set_title: "Connect Device",
            set_description: Some(model.description),

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_halign: gtk::Align::Center,

                gtk::Button {
                    set_label: "Open Settings",
                    add_css_class: "suggested-action",
                    add_css_class: "pill",
                    connect_clicked[sender] => move |_| {
                        let _ = sender.output(WelcomeOutput::OpenSettings);
                    },
                },
            },
        }
    }

    fn init(
        notice: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            description: match notice {
                Some(ConfigStartupNotice::ReadFailed(_) | ConfigStartupNotice::ParseFailed(_)) => {
                    RECOVERY_DESCRIPTION
                }
                _ => FIRST_RUN_DESCRIPTION,
            },
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}
