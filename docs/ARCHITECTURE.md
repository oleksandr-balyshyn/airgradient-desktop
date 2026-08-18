# Architecture

This document explains how Air Monitor is structured and how data moves through
the app. It is written for contributors who may be new to Rust, GTK, libadwaita,
or Relm4.

## The Short Version

The app is built with [Relm4](https://relm4.org/book/stable/), which is an
implementation of the Elm architecture on top of GTK 4. Three rules follow from
that, and most of the design falls out of them:

1. **The screen is made of components.** A component is a struct holding a little
   state (its *model*), plus a declaration of its widgets (its *view*). Parts of
   the view marked `#[watch]` are recalculated whenever the model changes, so
   nothing in this codebase ever calls "now go and update that label".
2. **Components talk by messages, not by sharing state.** Each has an input enum
   (what it accepts) and an output enum (what it reports upwards). There is no
   shared mutable state in the UI at all — no `Rc<RefCell<T>>`.
3. **Only the root component does input/output.** It owns the config file, the
   measurement history, the HTTP fetches, the alert policy, and the tray icon.
   Everything else is handed finished results.

## Startup

```text
main.rs
  -> app::run()
       reads config.json
       registers the embedded icon resources
       loads dashboard.css
    -> RelmApp::run::<ui::app::App>(config)
      -> App::init()
           applies the saved theme
           launches the child components
           assembles the page stack and the tab stack
           starts the one-second ticker
           fetches immediately if a device is configured
```

`RelmApp::new()` initialises GTK and libadwaita, which is why the icons and
stylesheet can be registered before the window exists. `visible_on_activate(false)`
is how "start minimized" is honoured: the window is built but never presented.

## The Component Tree

```text
App                              root: window, header bar, navigation, I/O
├── Welcome                      onboarding, until a device URL is saved
├── ViewStack                    the two top-level tabs
│   ├── Dashboard                "Main" tab
│   │   ├── AqiCard
│   │   ├── EnvironmentCard ×2   temperature, humidity
│   │   ├── SensorCard ×6        CO2, TVOC, NOx, PM0.3, PM1, PM2.5, PM10
│   │   └── MetricCard           PM2.5 over time
│   │       └── Chart
│   └── HistoryView              "History" tab
│       └── MetricCard × N       one per entry in ui::metrics::METRICS
│           └── Chart
├── Settings
└── Help
```

Two stacks, for two different jobs:

- A `gtk::Stack` switches between whole pages — onboarding, the measurement view,
  Settings, Help. Pages are identified by `state::Page`, and the root switches
  them with a single `#[watch] set_visible_child_name: model.page.id()`.
- An `adw::ViewStack` holds the Main and History tabs. It is a `ViewStack` rather
  than a plain `Stack` because that is what an `AdwViewSwitcher` binds to, which
  gives the header bar the standard GNOME tab control for free.

The switcher is only shown while the measurement view is visible; Settings, Help
and onboarding show the app name instead, because tabs would mean nothing there.

## Data Flow

A refresh, end to end:

```text
ticker fires (once a second)
  -> enough seconds have passed?
    -> App::start_fetch()
         sender.spawn_oneshot_command(...)          background thread
           -> device::fetch_current_measurements()
                HTTP GET {base_url}/measures/current
                sensors::parse_air_measurements()
      -> AppCommand::Fetched{ trigger, Ok(snapshot) }   back on the UI thread
           history.push(Sample::now(snapshot))            scheduled fetches only
           history.save()
           alerts.evaluate(&snapshot)  -> notifications::send_air_quality_notification()
           dashboard.emit(DashboardInput::Show(snapshot))
           dashboard.emit(DashboardInput::ShowHistory(history))
           history_view.emit(HistoryViewInput::Show(history))
```

`fetch_current_measurements` is blocking, so it must never run on the UI thread.
`spawn_oneshot_command` puts it on Relm4's blocking thread pool and delivers the
result back as a message, which is the whole reason the UI stays responsive while
a device is slow or unreachable.

The recorded history is shared with both views as one reference-counted
allocation, because copying a day of readings twice per refresh would be waste.

Only the automatic refresh records. A fetch started because someone pressed
Refresh or reopened the dashboard is displayed like any other, but it is not
written to the history and it does not restart the interval countdown. Recording
it would add a point at whatever moment the button was clicked, so the spacing of
the series would describe the clicking rather than the air; restarting the
countdown would let repeated clicks postpone the recorded readings indefinitely.
`FetchTrigger` carries that distinction from the call site to the result handler.

## Timers

There is exactly one timer: a one-second ticker, registered against the
component's shutdown receiver so it dies with the component instead of leaking.

It drives two things:

- the "Last updated: 17s ago" label, and
- the automatic refresh, which fires once `seconds_since_fetch` reaches the
  configured interval.

Using one ticker for both means changing the refresh interval in Settings needs
no timer bookkeeping at all: the next tick simply compares against the new
number.

## Modules That Do Not Know About GTK

These are plain Rust, importing nothing from GTK, which is what makes them
testable without a display. Most of the test suite lives here.

| Module | Responsibility |
| --- | --- |
| `sensors::air_quality` | Parse device JSON into `AirMeasureSnapshot` |
| `sensors::aqi` | EPA breakpoints and the NowCast smoothing |
| `sensors::thresholds` | Classify a reading as green/yellow/orange/red |
| `device` | Normalize base URLs, perform the HTTP request |
| `config` | Read and write `config.json` |
| `history` | The measurement ring buffer and its file |
| `alerts` | Decide which readings deserve a notification |
| `theme` | The 57 palettes and the CSS they generate |
| `ui::trend` | Trend text and value formatting |
| `ui::status` | Map a sensor status to a CSS class |
| `ui::metrics` | The table of what the app measures |
| `ui::chart` (lower half) | Chart bounds, downsampling, coordinates |

The rule to preserve: **domain logic decides what a reading means; the UI decides
how it looks.** `thresholds` returns `StatusColor::Red`; `ui::status` turns that
into the class name `"status-red"`; `dashboard.css` decides what red looks like.

## The Air Quality Index

Two details make the difference between a plausible AQI and a correct one.

The index is computed from `pm02Compensated`, the device's own corrected
particulate reading, not the raw `pm02`. AirGradient applies its batch and EPA
corrections to produce it, and it is what the vendor's dashboard shows. Current
firmware quantizes the raw field to whole micrograms, so a room genuinely at
3 ug/m3 reports `pm02: 0` and would otherwise display an AQI of 0.

The displayed value is the EPA **NowCast** rather than a direct conversion of the
latest reading. The AQI is defined over a 24-hour average, so converting one
instantaneous sample overstates brief events: two minutes of frying can push an
indoor sensor into "Unhealthy" when the air over any meaningful period was fine.
The NowCast is a weighted average of the last 12 hourly means, weighted toward
recent hours when readings are changing, which is what AirNow and PurpleAir
report. Sustained pollution still shows in full; only short spikes are damped.
Until two hours of history exist the NowCast is undefined and the instantaneous
value is shown.

The breakpoints are the 2024 revision, effective 6 May 2024, which lowered the
top of "Good" from 12.0 to 9.0 ug/m3 and tightened the bands above it.

## Missing Readings

Sensor values are `Option<f32>` throughout, because AirGradient models and
firmware versions expose different fields. A missing value is `None` and displays
as `--`. It is never coerced to `0`, and it is skipped rather than zeroed when
building a chart series, because zero is a legitimate measurement and would draw
a false dip.

## Themes

`theme.rs` describes a theme as three colours — background, foreground, accent —
and derives everything else by mixing. Cards, header bars and popovers are the
background nudged towards the foreground by fixed amounts; text on an accent
button is black or white depending on the accent's brightness.

The result is emitted as libadwaita `@define-color` overrides and loaded into one
reusable `CssProvider` by `ui::theming`. Reusing a single provider is deliberate:
adding a new provider per switch would stack them up and leave old themes
fighting the current one.

`dashboard.css` is written in terms of those named colours (`@card_fg_color`,
`@window_bg_color`), never literal greys, which is what lets every theme work.
The exceptions are air-quality status colours and trend colours, which are fixed
on purpose — red has to mean "bad air" in every theme.

## Storage

Two files, deliberately separate:

```text
$XDG_CONFIG_HOME/airgradient-desktop/config.json    settings the user chose
$XDG_DATA_HOME/airgradient-desktop/history.json     measurements the app recorded
```

Configuration failures are surfaced to the user (the Settings status line
explains why defaults were loaded). History failures are not: a missing, corrupt
or unwritable history yields an empty history and a line on stderr, because
losing recorded readings is not worth interrupting someone looking at live air
quality.

## Notifications

`alerts.rs` decides *whether* to notify — it requires two consecutive bad
readings, applies a 20-minute cooldown per alert kind, and re-fires early only
when a reading escalates to a worse severity. It knows nothing about GTK or D-Bus.

`notifications.rs` decides *how* to deliver, using `gio::Notification` through the
application's own ID. That is not an arbitrary choice: under strict snap
confinement there is no `notify-send` binary to shell out to, and raw
`org.freedesktop.Notifications` D-Bus calls can be silently dropped by the shell
for senders it does not recognise as a real app.

## Background Operation

Closing the window hides it rather than quitting. The root component holds a
`gio::ApplicationHoldGuard`, which keeps the process alive with no visible window
so polling and alerts continue from the tray. Quitting is an explicit action from
the tray menu or the `app.quit` action.

The tray itself runs on its own thread, owned by `ksni`. It communicates by
sending `AppInput` messages down a Relm4 sender, which is thread-safe — there is
no polling and no shared state between the two threads.

## GTK Resources

`build.rs` runs `glib-compile-resources` to compile `resources/icons/` into a
GResource bundle. `ui::register_resources()` registers it and adds it to the icon
theme search path, which is what allows:

```rust
gtk::Image::from_icon_name("airgradient-co2-symbolic")
```

instead of loading SVG files at runtime.
