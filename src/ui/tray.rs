//! StatusNotifier tray icon.
//!
//! The tray lives on its own thread, owned by the `ksni` crate. It talks to the
//! UI by sending ordinary `AppInput` messages down a Relm4 sender, which is
//! thread-safe. That replaces the older approach of an `mpsc` channel drained by
//! a 250 ms GLib poll timer: there is no polling here, and no shared mutable
//! state between the two threads.

use std::io::Cursor;
use std::sync::OnceLock;

use relm4::gtk::gdk_pixbuf::Pixbuf;
use relm4::Sender;

use super::app::AppInput;
use crate::app_info::APP_NAME;

struct AirMonitorTray {
    sender: Sender<AppInput>,
}

impl AirMonitorTray {
    fn send(&self, message: AppInput) {
        // A failed send means the UI is already gone, which is not an error
        // worth reporting from a tray click.
        let _ = self.sender.send(message);
    }
}

impl ksni::Tray for AirMonitorTray {
    fn id(&self) -> String {
        "airgradient-desktop".into()
    }

    fn title(&self) -> String {
        APP_NAME.into()
    }

    fn icon_name(&self) -> String {
        "airgradient-desktop".into()
    }

    /// Supply the icon as raw pixels as well as by name.
    ///
    /// Under strict snap or Flatpak confinement the tray host often cannot read
    /// the app's icon theme, so a named icon alone can end up blank. Sending the
    /// bytes we embedded at compile time always works.
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        static TRAY_ICON: OnceLock<ksni::Icon> = OnceLock::new();

        vec![TRAY_ICON.get_or_init(load_tray_icon).clone()]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(AppInput::ShowWindow);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{MenuItem, StandardItem};

        vec![
            StandardItem {
                label: "Show Dashboard".into(),
                icon_name: "go-home-symbolic".into(),
                activate: Box::new(|tray: &mut Self| tray.send(AppInput::ShowWindow)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Hide Window".into(),
                icon_name: "window-close-symbolic".into(),
                activate: Box::new(|tray: &mut Self| tray.send(AppInput::HideWindow)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| tray.send(AppInput::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Decode the embedded PNG into the ARGB byte layout StatusNotifier expects.
fn load_tray_icon() -> ksni::Icon {
    let pixbuf = Pixbuf::from_read(Cursor::new(
        include_bytes!("../../assets/airgradient-tray.png").as_slice(),
    ))
    .expect("embedded tray icon should be a valid PNG");

    let width = pixbuf.width();
    let height = pixbuf.height();
    let channels = pixbuf.n_channels() as usize;
    let rowstride = pixbuf.rowstride() as usize;
    let pixels = pixbuf.read_pixel_bytes();
    let pixels = pixels.as_ref();
    let mut data = Vec::with_capacity(width as usize * height as usize * 4);

    for y in 0..height as usize {
        let row_start = y * rowstride;
        let row_end = row_start + width as usize * channels;
        for pixel in pixels[row_start..row_end].chunks_exact(channels) {
            let [red, green, blue] = [pixel[0], pixel[1], pixel[2]];
            let alpha = if channels >= 4 { pixel[3] } else { 255 };
            data.extend_from_slice(&[alpha, red, green, blue]);
        }
    }

    ksni::Icon {
        width,
        height,
        data,
    }
}

/// Owns the tray thread and shuts it down when the app closes.
pub struct TrayHandle {
    handle: ksni::blocking::Handle<AirMonitorTray>,
}

impl Drop for TrayHandle {
    fn drop(&mut self) {
        self.handle.shutdown().wait();
    }
}

/// Start the tray icon, returning `None` when no tray host is available.
///
/// A missing tray is normal — plain GNOME has no StatusNotifier host unless the
/// AppIndicator extension is installed — so this is a warning, not a failure.
pub fn install(sender: Sender<AppInput>) -> Option<TrayHandle> {
    let tray = AirMonitorTray { sender };

    match ksni::blocking::TrayMethods::assume_sni_available(tray, true).spawn() {
        Ok(handle) => Some(TrayHandle { handle }),
        Err(err) => {
            eprintln!("System tray unavailable: {err}");
            None
        }
    }
}
