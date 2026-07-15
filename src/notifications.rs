//! Desktop notification delivery.
//!
//! The alert policy decides *what* should notify. This infrastructure module
//! owns *how* to deliver it: through GNotification, GLib's sandbox-aware
//! notification API. Unlike shelling out to `notify-send` or talking to
//! org.freedesktop.Notifications directly over D-Bus, GNotification is routed
//! through the app's own registered application ID, which is what lets it
//! work reliably under strict snap confinement (no `notify-send` binary is
//! bundled, and raw D-Bus notification calls can be silently swallowed by the
//! shell for senders it doesn't recognize as a real app).

use std::sync::atomic::{AtomicU64, Ordering};

use adw::prelude::ApplicationExt;

use crate::alerts::{AlertNotification, AlertSeverity};

static NOTIFICATION_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub fn send_air_quality_notification(
    app: &adw::Application,
    alert: AlertNotification,
) -> Result<(), String> {
    let notification = gio::Notification::new(&alert.title);
    notification.set_body(Some(&alert.body));
    notification.set_default_action("app.show-dashboard");
    notification.add_button("Open Dashboard", "app.show-dashboard");
    notification.set_priority(match alert.severity {
        AlertSeverity::Notice => gio::NotificationPriority::Normal,
        AlertSeverity::Warning => gio::NotificationPriority::High,
        AlertSeverity::Critical => gio::NotificationPriority::Urgent,
    });
    let unique_id = format!(
        "{}-{}",
        alert.id,
        NOTIFICATION_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    app.send_notification(Some(&unique_id), &notification);
    Ok(())
}
