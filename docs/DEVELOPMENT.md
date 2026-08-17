# Development Guide

This guide explains common tasks and the Rust/Relm4 patterns used in this project.

## Common Commands

```bash
cargo fmt
cargo test
cargo build
cargo build --release
cargo run
```

Run these before committing behavior changes. CI runs exactly the same four with
`--locked`, so a failure here is a failure there:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo check --workspace --all-features
```

Run a single test by name:

```bash
cargo test parse_air_measurements       # substring match on the test name
cargo test --lib theme                  # every test in the theme module
cargo test --test config_persistence    # just that integration test file
```

Dependency checks use Cargo subcommands that may need to be installed locally.
Install them from outside the repository so they do not touch `Cargo.lock`:

```bash
(cd /tmp && cargo install cargo-audit cargo-deny --locked)
cargo audit
cargo deny check
```

CI runs both through GitHub Actions, so local development does not depend on
having them installed.

## Relm4 In One Page

The whole UI is Relm4 components. A component is four things:

```rust
// 1. The model: this component's state.
pub struct SensorCard {
    title: String,
    value: Option<f32>,
}

// 2. The input: messages it accepts. Must derive Debug.
#[derive(Debug)]
pub enum SensorCardInput {
    Show(Option<f32>),
}

#[relm4::component(pub)]
impl SimpleComponent for SensorCard {
    type Init = String;                 // what it needs to be created
    type Input = SensorCardInput;
    type Output = ();                   // messages it reports upwards

    // 3. The view: widgets, declared once.
    view! {
        gtk::Box {
            gtk::Label {
                set_label: model.title.as_str(),   // set once
            },
            gtk::Label {
                #[watch]
                set_label: &model.value_text(),    // re-run on every change
            },
        }
    }

    fn init(title: Self::Init, root: Self::Root, sender: ComponentSender<Self>)
        -> ComponentParts<Self>
    {
        let model = Self { title, value: None };
        let widgets = view_output!();   // builds the view! block above
        ComponentParts { model, widgets }
    }

    // 4. The update: the only place the model changes.
    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            SensorCardInput::Show(value) => self.value = value,
        }
    }
}
```

The important part is `#[watch]`. Anything marked with it is re-evaluated after
every model change, which is why there is no "update the label" code anywhere in
this codebase. If a value on screen can change, mark it `#[watch]` and read it
from the model — do not store a widget handle and set it later.

### Talking Between Components

A parent launches a child and keeps the returned `Controller`:

```rust
// Ignore this child's output.
let dashboard = Dashboard::builder().launch(()).detach();

// Or translate its output into the parent's own input.
let settings = Settings::builder()
    .launch(init)
    .forward(sender.input_sender(), |output| match output {
        SettingsOutput::Save(config) => AppInput::SaveConfig(config),
    });
```

Send a message to a child with `child.emit(...)`, send to yourself with
`sender.input(...)`, and report upwards with `sender.output(...)`.

Never reach into a child's state. If a parent needs to know something, the child
should say so with an output message.

### Placing A Child's Widgets

Take the child's root widget as a local reference before `view_output!()` and
place it with `#[local_ref]`:

```rust
let chart_widget = model.chart.widget();
let widgets = view_output!();
```

```rust
view! {
    gtk::Box {
        #[local_ref]
        chart_widget -> gtk::Box {},
    }
}
```

The same trick is used when a container's contents are decided by data rather
than by layout — see `history_view.rs`, which fills a `FlowBox` by looping over
`metrics::METRICS` and then hands the finished `FlowBox` to `view!`.

### Background Work

Never block the UI thread. `device::fetch_current_measurements` uses
`reqwest::blocking`, so it runs on a thread pool and reports back as a message:

```rust
sender.spawn_oneshot_command(move || {
    AppCommand::Fetched(fetch_current_measurements(&base_url).map_err(|e| e.to_string()))
});
```

The result arrives in `update_cmd`, on the UI thread, where it is safe to touch
widgets. A component that uses commands implements `Component` (with a
`CommandOutput` type) rather than `SimpleComponent`.

Recurring work uses a command registered against the shutdown receiver so it
stops with the component:

```rust
sender.command(|out, shutdown| {
    shutdown
        .register(async move {
            let mut ticker = relm4::tokio::time::interval(TICK);
            loop {
                ticker.tick().await;
                if out.send(AppCommand::Tick).is_err() {
                    break;
                }
            }
        })
        .drop_on_shutdown()
});
```

### Changing CSS Classes

Set the whole list at once instead of adding and removing single classes:

```rust
#[watch]
set_css_classes: &model.css_classes(),
```

Adding and removing individual classes is how stale state creeps in — an old
status class left behind makes the final colour depend on stylesheet ordering.

### Drawing With Cairo

`ui/chart.rs` is the one place with a `DrawingArea`. Cairo draw functions are
closures GTK calls later, so they must own their data; the chart hands each new
closure a fresh copy of the readings rather than sharing a `RefCell`. That
happens in `update_with_view`, because the `component` macro already generates
`update_view` from the `view!` block.

## Adding A New Sensor Value

1. Add a field to `AirMeasureSnapshot` in `src/sensors/air_quality.rs`.
2. Extend `parse_air_measurements()` with the possible JSON key names.
3. Add or reuse a threshold function in `src/sensors/thresholds.rs`.
4. Add an entry to `METRICS` in `src/ui/metrics.rs`. The History tab picks it up
   automatically — it is a loop over that table.
5. If it belongs on the Main tab too, add a card in `src/ui/dashboard.rs`.
6. Extend the parser test with a sample payload.

Use `Option<f32>` unless the value is guaranteed to exist in every supported
device payload. A missing reading is `None`, never `0`.

## Adding A New Theme

One line in `THEMES` in `src/theme.rs`:

```rust
Theme::dark("my-theme", "My Theme", 0x1e1e2e, 0xcdd6f4, 0x89b4fa),
//          id          name        background  foreground  accent
```

The id is written to `config.json`, so treat it as permanent. Everything else —
card, header bar, popover, dialog and accent-text colours — is derived by mixing.

The theme tests then check it automatically: enough contrast between text and
background, readable text on the accent, a background brightness that matches the
declared light/dark variant, and a unique id and name. If a palette is
unreadable, the test suite says so.

## Where Logic Belongs

Keep GTK out of anything that can be tested without a display. The split that
matters:

- **Domain decides meaning.** `thresholds` returns `StatusColor::Red`.
- **UI decides appearance.** `ui::status` maps that to `"status-red"`, and
  `dashboard.css` decides what red looks like.
- **Policy and delivery are separate.** `alerts.rs` decides whether to notify;
  `notifications.rs` decides how.
- **Only the root component does I/O.** Child components are handed results.

`ui::trend`, `ui::status`, `ui::metrics` and the lower half of `ui::chart` are
inside `ui/` but import no GTK, which is why they carry tests.

## CSS And Styling

Card and chart styling lives in `assets/dashboard.css`. It is embedded with
`include_str!`, so **editing the CSS requires a rebuild**.

Write rules in terms of libadwaita's named colours — `@card_fg_color`,
`@window_bg_color`, `alpha(@card_fg_color, 0.7)` — never literal greys. Themes
override those names, so a hardcoded colour is a colour that breaks in 56 of the
57 themes. The deliberate exceptions are air-quality status colours and trend
colours, which must stay fixed.

Prefer libadwaita's own widgets (`PreferencesPage`, `ActionRow`, `EntryRow`,
`SpinRow`, `ToolbarView`, `ViewSwitcher`) over hand-built equivalents. They come
correctly themed, accessible, and adaptive.

## Icons

Symbolic icons live in:

```text
resources/icons/scalable/status/
```

They should be monochrome SVGs and use `currentColor` where possible, so GTK can
recolor them for light, dark, and high-contrast themes.

After adding an icon:

1. Add the SVG file.
2. Add it to `resources/airgradient.gresource.xml`.
3. Add it to the `rerun-if-changed` list in `build.rs`.
4. Use it by icon name, without the `.svg` suffix.

## Configuration And Recorded Data

Two separate files:

```text
$XDG_CONFIG_HOME/airgradient-desktop/config.json    what the user chose
$XDG_DATA_HOME/airgradient-desktop/history.json     what the app recorded
```

Do not put runtime values in `config.json`. Do not put user preferences in
`history.json`. A corrupt history must never be able to cost someone their
settings.

To test with a throwaway configuration and history, override both directories:

```bash
XDG_CONFIG_HOME=/tmp/ag-config XDG_DATA_HOME=/tmp/ag-data cargo run
```

## Dependency Compatibility

Relm4 owns the gtk-rs train. Do not add `gtk4`, `libadwaita`, `glib`, `gio` or
`gdk-pixbuf` as direct dependencies — use `relm4::gtk`, `relm4::adw`,
`relm4::gtk::gio`, `relm4::gtk::gdk_pixbuf` and so on. Two gtk-rs versions in one
binary will not link.

The `gnome_45` feature fixes the API level at libadwaita 1.4 and GTK 4.12. Raising
it unlocks newer widgets but raises the minimum system version, and the CI image
(Ubuntu 24.04: GTK 4.14, libadwaita 1.5) is the ceiling that matters — code using
a newer API will build on a current desktop and fail in CI.
