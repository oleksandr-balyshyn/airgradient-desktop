# Air Monitor

Air Monitor is a GTK 4 + libadwaita desktop dashboard for an AirGradient device that exposes the local-server endpoint at `/measures/current`. It is written in Rust using [Relm4](https://relm4.org/book/stable/).

The app is intentionally small and direct: configure the device URL, fetch the current measurement payload, normalize it into a Rust data model, and render the result as a GNOME-style air quality dashboard.

## Why This Exists

I use an AirGradient ONE at my desk because indoor air quality is one of those things that is easy to ignore until it starts affecting focus, comfort, and health. The device itself is a compact indoor air quality monitor with sensors for PM2.5, CO2, TVOC, NOx, temperature, and humidity. It is also open-source friendly and exposes local readings, which makes it a good fit for a small desktop companion app.

The official AirGradient web application is good, but it is not quite the interface I want while working at a PC. The display on the device is also too small to glance at comfortably from a normal sitting position, especially when I just want to know whether CO2 is climbing, humidity is off, or the air needs attention. Air Monitor exists to put those numbers directly on the desktop, in a readable layout, without having to open a browser tab or walk over to the device.

![AirGradient ONE devices](docs/image.png)

## Screenshots

These predate the tabbed interface and the theme list, so they show the older
single-page layout.

![Air Monitor dashboard](docs/Screenshot-1.png)

![Air Monitor settings](docs/Screenshot-2.png)

![Air Monitor notification and background mode](docs/Screenshot-3.png)

## What It Shows

The window has two tabs, switched from the header bar.

**Main** shows the current readings:

- Air Quality Index (AQI)
- temperature and humidity
- CO2, TVOC, and NOx
- PM0.3 count, PM1.0, PM2.5, and PM10
- trend indicators comparing each reading with the previous one
- a chart of recorded PM2.5, the reading most worth watching over time
- the last successful update time

**History** shows every one of those metrics as a card with its own chart and a
low/average/high summary of the recorded range.

Recorded measurements are kept across restarts, so the charts are not empty when
you reopen the app. Pressure is not exposed by the local-server payload and is
not shown.

## How It Works

At a high level:

1. The user configures a local AirGradient device URL in Settings.
2. The app stores the URL in the XDG config directory.
3. The app fetches `{server_url}/measures/current` on a timer.
4. The JSON payload is converted into `AirMeasureSnapshot`.
5. The snapshot is appended to the measurement history and written to the XDG data directory.
6. The snapshot and the history are sent as messages to the UI components, which redraw themselves.

For a deeper explanation, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Linux Requirements

Air Monitor targets Linux only.

Runtime dependencies for the raw binary:

- GTK 4 runtime
- libadwaita runtime
- GLib/GIO and GDK Pixbuf runtime libraries
- hicolor icon theme
- CA certificates for HTTP client TLS support
- a desktop notification service
- a StatusNotifier/AppIndicator-compatible tray host if you want the tray icon

Debian/Ubuntu runtime example:

```bash
sudo apt install libgtk-4-1 libadwaita-1-0 libglib2.0-0 libgdk-pixbuf-2.0-0 hicolor-icon-theme ca-certificates libnotify-bin
```

GNOME users may also need the AppIndicator/KStatusNotifier shell extension for tray icons:

```bash
sudo apt install gnome-shell-extension-appindicator
```

Build dependencies:

- Rust toolchain, stable channel
- GTK 4 development files
- Libadwaita 1.4 or newer development files
- `pkg-config`
- `glib-compile-resources`, normally installed with GLib development tools

Debian/Ubuntu build example:

```bash
sudo apt install pkg-config libgtk-4-dev libadwaita-1-dev libglib2.0-dev build-essential
```

Packaging dependencies used by the GitHub Actions release job:

```bash
sudo apt install flatpak flatpak-builder appstream desktop-file-utils patchelf file wget libfuse2t64
```

Snap packages are built in a dedicated `snap.yml` GitHub Actions workflow with Snapcraft and the GNOME
Snapcraft extension, running on every push and pull request as a build check. Tagged releases and manual
`workflow_dispatch` runs also publish to the Snap Store (`stable` on tag pushes, or a chosen channel via
dispatch), provided package-scoped `SNAPCRAFT_STORE_CREDENTIALS` is configured.

## Run Locally

```bash
cargo build
cargo run
```

Run tests:

```bash
cargo test
```

Run a release build:

```bash
cargo build --release
```

## Linux Release Artifacts

Tagged releases build and publish Linux-only `amd64` and `arm64` artifacts from GitHub Actions:

- `airgradient-desktop-<version>-linux-<arch>`: the plain executable produced by `cargo build --release`, where `<arch>` is `amd64` or `arm64`. This is the smallest artifact and still requires the runtime dependencies listed above. It does not include desktop launcher files or icons.
- `airgradient-desktop-<version>-linux-<arch>.tar.gz`: raw release binary plus desktop file, metainfo, application icons, and tray icon assets. This still requires the runtime dependencies listed above.
- `airgradient-desktop-<version>-linux-<arch>.flatpak`: Flatpak bundle built against the GNOME runtime. The Flatpak installs the desktop launcher, metainfo, application icon, fallback icon name, and tray icon asset inside the sandbox. It also grants network, notifications, and StatusNotifier watcher access for the AirGradient HTTP endpoint, notifications, and tray registration.
- `airgradient-desktop-<version>-linux-<arch>.AppImage`: self-contained AppImage produced from an AppDir with bundled GTK/libadwaita dependencies where `linuxdeploy` can collect them. It includes the desktop launcher, metainfo, application icons, and tray icon asset. AppImages may still need host graphics, desktop session, D-Bus, and FUSE support.
- `airgradient-desktop-<version>-linux-<arch>.snap`: strict Snap package built on `core24` with the GNOME extension. It includes the desktop launcher, AppStream metadata, application icons, tray icon asset, network access, the `unity7` plug for the system tray icon, and the D-Bus application slot for single-instance activation.

Each tagged release also uploads unversioned aliases for stable latest-download URLs. For example:

```text
https://github.com/oleksandr-balyshyn/airgradient-desktop/releases/latest/download/airgradient-desktop-linux-amd64.snap
https://github.com/oleksandr-balyshyn/airgradient-desktop/releases/latest/download/airgradient-desktop-linux-arm64.snap
```

Install a Flatpak bundle locally:

```bash
flatpak install --user ./airgradient-desktop-0.1.2-linux-amd64.flatpak
flatpak run com.airgradient.desktop
```

Run an AppImage:

```bash
chmod +x airgradient-desktop-0.1.2-linux-amd64.AppImage
./airgradient-desktop-0.1.2-linux-amd64.AppImage
```

Install a downloaded Snap package locally:

```bash
sudo snap install --dangerous ./airgradient-desktop-0.1.2-linux-amd64.snap
airgradient-desktop
```

## Configure A Device

Open Settings and enter the device base URL. These forms are accepted:

- `http://192.168.1.201/`
- `http://192.168.1.201`
- `192.168.1.201`
- `http://192.168.1.201:80`

The app normalizes the value before saving it. For example, `192.168.1.201` becomes `http://192.168.1.201`.

Configuration is stored at:

```text
$XDG_CONFIG_HOME/airgradient-desktop/config.json
```

If `XDG_CONFIG_HOME` is not set, the fallback is:

```text
$HOME/.config/airgradient-desktop/config.json
```

Recorded measurements are stored separately, under the XDG *data* directory:

```text
$XDG_DATA_HOME/airgradient-desktop/history.json
```

or, as a fallback:

```text
$HOME/.local/share/airgradient-desktop/history.json
```

They are kept apart from the configuration on purpose: `config.json` holds
choices you made, while `history.json` holds data the app recorded. A corrupt or
deleted history costs you charts, never settings. The file keeps the most recent
2,880 readings, which is a day at the default 30-second refresh.

## Project Layout

```text
src/
  main.rs                 Program entry point.
  app.rs                  Reads config, then starts the Relm4 application.
  app_info.rs             Shared application ID and display name.
  config.rs               Reads/writes user configuration.
  device.rs               Normalizes device URLs and fetches measurements.
  history.rs              Recorded measurements, persisted to the data directory.
  theme.rs                The 57 colour themes and the CSS they generate.
  alerts.rs               Decides which readings deserve a notification.
  notifications.rs        Delivers those notifications to the desktop.
  state.rs                The page list shared by the UI components.
  sensors/
    air_quality.rs        Parses AirGradient JSON into typed values.
    thresholds.rs         Classifies sensor values into semantic statuses.
  ui/
    app.rs                Root component: window, header, tabs, refresh loop.
    dashboard.rs          Main tab.
    history_view.rs       History tab.
    metrics.rs            Table of what the app measures.
    metric_card.rs        A metric with its chart, used by both tabs.
    chart.rs              Cairo line chart plus its coordinate maths.
    sensor_card.rs        Reusable pollutant metric card.
    aqi_card.rs           Large AQI card.
    environment_card.rs   Temperature and humidity card.
    settings.rs           Settings page.
    welcome.rs            Onboarding page.
    help.rs               Help page.
    theming.rs            Loads a theme's colours into GTK.
    status.rs             Maps sensor status to CSS classes.
    trend.rs              Trend arithmetic and value formatting.
    tray.rs               StatusNotifier tray integration.
assets/
  dashboard.css           GTK CSS for the cards and charts.
resources/
  airgradient.gresource.xml
  icons/                  Symbolic SVG icons embedded into the binary.
docs/
  ARCHITECTURE.md         How data and UI state move through the app.
  DEVELOPMENT.md          Development workflow and common tasks.
  LOCAL_SERVER.md         AirGradient payload handling notes.
```

## Important Rust Concepts Used Here

This codebase is a useful Rust/Relm4 learning project because it uses common
patterns in a small app:

- `Option<T>` means a sensor value may be missing from the JSON payload. A
  missing reading is `None`, never `0`, because zero is a valid measurement.
- `Result<T, String>` is used when a URL, network request, or JSON parse can fail.
- Each part of the screen is a Relm4 **component**: a small struct holding its own
  state, plus a `view!` block declaring its widgets. Fields marked `#[watch]` are
  re-evaluated whenever that state changes, so no code ever has to remember to
  update a label.
- Components communicate only by **messages** — an input enum for what a component
  accepts, an output enum for what it reports upwards. They never share mutable
  state, so there is no `Rc<RefCell<T>>` anywhere in the UI.
- Blocking HTTP work runs on a background thread pool via
  `sender.spawn_oneshot_command()`, and its result arrives back as a message.
- Pure logic (trend text, colour mixing, chart coordinates, thresholds) lives in
  modules that never import GTK, which is what makes it unit-testable without a
  display.

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for more detail.

## Theme Support

Settings has a Theme dropdown with 57 entries:

- **System Default** follows the desktop's light or dark preference.
- **Adwaita Light** and **Adwaita Dark** force libadwaita's own palette.
- 54 named themes — Catppuccin, Nord, Dracula, Gruvbox, Solarized, Tokyo Night,
  Everforest, Rosé Pine, One Dark, Monokai, Material, Ayu, Kanagawa, GitHub and
  others — each setting its own colours.

Themes preview as you move through the list and are saved only when you press
Save, so trying them costs nothing.

A theme is defined by just three colours: a background, a foreground, and an
accent. Everything else libadwaita needs is derived by mixing them, and the
result is applied as `@define-color` overrides, so every widget in the app
follows the theme. Adding one is a single line in `src/theme.rs`.

Air-quality status colours (green through red) and trend colours are deliberately
*not* themed: red has to mean "bad air" in every theme.

## Roadmap

- Add a time-range selector to the History tab.
- Implement a lightweight backend proxy so the desktop app does not have to fetch directly from the device.
- Make the backend proxy available as a public server option.
- Add a web UI that can run in kiosk mode on a TV or screensaver display.
- Implement air quality notifications for Discord and Telegram.

## Dependency Compatibility

Relm4 owns the entire gtk-rs dependency train. The app does **not** declare
`gtk4`, `libadwaita`, `glib`, `gio` or `gdk-pixbuf` itself — it uses Relm4's
re-exports (`relm4::gtk`, `relm4::adw`, `relm4::gtk::gio`, and so on). That makes
it impossible to link two incompatible gtk-rs versions by accident.

```toml
relm4 = { version = "0.11", features = ["libadwaita", "macros", "gnome_45"] }
```

The `gnome_45` feature selects the API level: libadwaita 1.4 bindings
(`AdwToolbarView`, `AdwViewSwitcher`, `AdwEntryRow`, `AdwSpinRow`) and GTK 4.12
bindings. It is deliberately not higher, because the GitHub Actions Ubuntu 24.04
image ships GTK 4.14 and libadwaita 1.5 — anything above this level would build
on a current desktop and fail in CI.

Upgrading Relm4 upgrades the whole train at once; that is the intended way to
move versions here.

## Install Notes

After building a release binary:

```bash
sudo install -Dm755 target/release/airgradient-desktop /usr/local/bin/airgradient-desktop
sudo install -Dm644 assets/com.airgradient.desktop.desktop /usr/share/applications/com.airgradient.desktop.desktop
sudo install -Dm644 assets/com.airgradient.desktop.metainfo.xml /usr/share/metainfo/com.airgradient.desktop.metainfo.xml
sudo install -Dm644 assets/airgradient-desktop.svg /usr/share/icons/hicolor/scalable/apps/com.airgradient.desktop.svg
sudo install -Dm644 assets/airgradient-desktop.svg /usr/share/icons/hicolor/scalable/apps/airgradient-desktop.svg
sudo install -Dm644 assets/airgradient-desktop.png /usr/share/icons/hicolor/256x256/apps/com.airgradient.desktop.png
sudo install -Dm644 assets/airgradient-tray.png /usr/share/icons/hicolor/256x256/status/airgradient-tray.png
sudo install -Dm644 assets/airgradient-tray.png /usr/share/icons/hicolor/256x256/status/airgradient-desktop.png
sudo install -Dm644 assets/airgradient-tray.png /usr/share/icons/hicolor/256x256/status/com.airgradient.desktop.png
sudo update-desktop-database /usr/share/applications
```

## References

- [AirGradient local-server documentation](https://github.com/airgradienthq/arduino/blob/master/docs/local-server.md)
- [AirGradient ONE indoor air quality monitor](https://www.airgradient.com/airgradient-one/)
- [Relm4 book](https://relm4.org/book/stable/)
- [Relm4 API documentation](https://docs.rs/relm4/)
- [gtk-rs documentation](https://gtk-rs.org/)
- [gtk4-rs book](https://gtk-rs.org/gtk4-rs/git/book/)
- [GNOME Human Interface Guidelines](https://developer.gnome.org/hig/)
