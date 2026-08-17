//! Line chart for a series of readings.
//!
//! The chart is drawn with Cairo onto a `gtk::DrawingArea`, because a sparkline
//! is a handful of line segments and pulling in a plotting library for that would
//! cost far more than it saves.
//!
//! The arithmetic that turns readings into coordinates is separated out into
//! plain functions at the bottom of this file. That is where the interesting
//! decisions live — how to handle a flat series, how many points are worth
//! drawing — and keeping them free of GTK means they are covered by tests
//! instead of only by looking at the screen.

use relm4::gtk::cairo;
use relm4::gtk::prelude::*;
use relm4::{gtk, prelude::*};

/// A chart wider than this many pixels still only gets this many points.
///
/// A day of readings is thousands of samples, but a card is a few hundred pixels
/// wide. Drawing every sample would put several of them in each pixel column,
/// costing time and producing a noisier line than the data justifies.
const MAX_POINTS: usize = 240;

/// Opacity of the filled area under the line.
const FILL_ALPHA: f64 = 0.16;
const LINE_WIDTH: f64 = 2.0;
/// Radius of the dot marking the most recent reading.
const MARKER_RADIUS: f64 = 2.5;

/// A chart needs at least two readings before there is a line to draw.
const MIN_POINTS: usize = 2;

#[derive(Debug)]
pub enum ChartInput {
    /// Replace the plotted series, oldest reading first.
    SetSeries(Vec<f32>),
}

pub struct Chart {
    values: Vec<f32>,
    height: i32,
}

impl Chart {
    fn has_enough_data(&self) -> bool {
        self.values.len() >= MIN_POINTS
    }
}

#[relm4::component(pub)]
impl Component for Chart {
    /// Height of the plot in pixels.
    type Init = i32;
    type Input = ChartInput;
    type Output = ();
    /// The chart runs no background work of its own; the root component fetches.
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_hexpand: true,

            #[name = "area"]
            gtk::DrawingArea {
                set_content_height: model.height,
                set_hexpand: true,
                add_css_class: "chart",
                #[watch]
                set_visible: model.has_enough_data(),
            },

            gtk::Label {
                set_label: "Collecting readings…",
                set_halign: gtk::Align::Start,
                #[watch]
                set_visible: !model.has_enough_data(),
                set_css_classes: &["metric-unit", "dim-label"],
            },
        }
    }

    fn init(
        height: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            values: Vec::new(),
            height,
        };

        let widgets = view_output!();
        install_draw_func(&widgets.area, model.values.clone());
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            ChartInput::SetSeries(values) => self.values = values,
        }
    }

    /// Update the model, the declared view, and then the Cairo drawing.
    ///
    /// The drawing needs this hook rather than `update_view` because the macro
    /// generates that method from the `view!` block; `update_view` below is the
    /// generated one, which is why it has to be called explicitly here.
    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(message, sender.clone(), root);
        self.update_view(widgets, sender);
        install_draw_func(&widgets.area, self.values.clone());
    }
}

/// Point a drawing area at a series.
///
/// Cairo draw functions are closures GTK calls later, so they have to own their
/// data. Handing each one a fresh copy of the readings keeps the drawing code
/// free of shared mutable state (no `Rc<RefCell<_>>`), and the copy costs
/// nothing at this rate: it happens once per refresh, not once per frame.
fn install_draw_func(area: &gtk::DrawingArea, values: Vec<f32>) {
    area.set_draw_func(move |area, context, width, height| {
        draw_series(area, context, f64::from(width), f64::from(height), &values);
    });
    area.queue_draw();
}

/// Paint the filled area, the line, and a dot on the latest reading.
///
/// The colour is taken from the widget's own CSS `color`, so the chart follows
/// whichever of the app's themes is active without being told about it.
fn draw_series(
    area: &gtk::DrawingArea,
    context: &cairo::Context,
    width: f64,
    height: f64,
    values: &[f32],
) {
    let Some(points) = plot_points(values, width, height) else {
        return;
    };
    let color = area.color();
    let (red, green, blue) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );

    // Filled area: the line, closed along the bottom edge.
    context.set_source_rgba(red, green, blue, FILL_ALPHA);
    context.move_to(points[0].0, height);
    for (x, y) in &points {
        context.line_to(*x, *y);
    }
    context.line_to(points[points.len() - 1].0, height);
    context.close_path();
    let _ = context.fill();

    context.set_source_rgba(red, green, blue, 0.9);
    context.set_line_width(LINE_WIDTH);
    context.move_to(points[0].0, points[0].1);
    for (x, y) in points.iter().skip(1) {
        context.line_to(*x, *y);
    }
    let _ = context.stroke();

    if let Some((x, y)) = points.last() {
        context.arc(*x, *y, MARKER_RADIUS, 0.0, std::f64::consts::PI * 2.0);
        let _ = context.fill();
    }
}

/// Lowest and highest value to plot.
///
/// A completely flat series has no range, which would mean dividing by zero when
/// scaling. Padding it out puts the line through the middle of the plot instead,
/// which is also the honest picture: nothing changed.
pub fn bounds(values: &[f32]) -> Option<(f32, f32)> {
    let first = *values.first()?;
    let (mut low, mut high) = (first, first);
    for value in values {
        low = low.min(*value);
        high = high.max(*value);
    }

    if (high - low).abs() < f32::EPSILON {
        let padding = if low.abs() < f32::EPSILON {
            1.0
        } else {
            low.abs() * 0.1
        };
        return Some((low - padding, high + padding));
    }
    Some((low, high))
}

/// Reduce a series to at most `max_points` evenly spaced readings.
///
/// The last reading is always kept, because on a live chart the newest value is
/// the one being looked at.
pub fn downsample(values: &[f32], max_points: usize) -> Vec<f32> {
    let max_points = max_points.max(MIN_POINTS);
    if values.len() <= max_points {
        return values.to_vec();
    }

    let step = (values.len() - 1) as f64 / (max_points - 1) as f64;
    (0..max_points)
        .map(|index| {
            let source = (index as f64 * step).round() as usize;
            values[source.min(values.len() - 1)]
        })
        .collect()
}

/// Turn readings into pixel coordinates, oldest on the left.
///
/// Returns `None` when there is nothing to draw: fewer than two readings, or a
/// widget that has not been given a size yet.
pub fn plot_points(values: &[f32], width: f64, height: f64) -> Option<Vec<(f64, f64)>> {
    if values.len() < MIN_POINTS || width <= 0.0 || height <= 0.0 {
        return None;
    }

    let points = downsample(values, MAX_POINTS);
    let (low, high) = bounds(&points)?;
    let range = f64::from(high - low);
    // Inset by half the line width so a reading at the very top or bottom is not
    // clipped in half by the edge of the widget.
    let inset = LINE_WIDTH / 2.0;
    let usable_height = (height - LINE_WIDTH).max(1.0);
    let step = width / (points.len() - 1) as f64;

    Some(
        points
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let fraction = (f64::from(*value) - f64::from(low)) / range;
                // Cairo's y axis grows downwards, so the highest reading is the
                // smallest y.
                (
                    index as f64 * step,
                    inset + (1.0 - fraction) * usable_height,
                )
            })
            .collect(),
    )
}

/// Lowest, mean and highest reading, for a chart's summary line.
pub fn summary(values: &[f32]) -> Option<(f32, f32, f32)> {
    let (low, high) = raw_range(values)?;
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    Some((low, mean, high))
}

/// True minimum and maximum, without the padding `bounds` adds for drawing.
fn raw_range(values: &[f32]) -> Option<(f32, f32)> {
    let first = *values.first()?;
    Some(values.iter().fold((first, first), |(low, high), value| {
        (low.min(*value), high.max(*value))
    }))
}

#[cfg(test)]
mod tests {
    use super::{bounds, downsample, plot_points, summary, MAX_POINTS};

    #[test]
    fn bounds_of_an_empty_series_are_undefined() {
        assert_eq!(bounds(&[]), None);
    }

    #[test]
    fn bounds_span_the_readings() {
        assert_eq!(bounds(&[3.0, 1.0, 7.0, 5.0]), Some((1.0, 7.0)));
    }

    #[test]
    fn flat_series_get_padded_bounds() {
        let (low, high) = bounds(&[10.0, 10.0, 10.0]).expect("flat series has bounds");

        assert!(
            low < 10.0 && high > 10.0,
            "expected padding, got {low}..{high}"
        );
    }

    #[test]
    fn flat_zero_series_gets_padded_bounds() {
        // Padding proportional to the value would be zero here, which would put
        // the divide-by-zero straight back.
        assert_eq!(bounds(&[0.0, 0.0]), Some((-1.0, 1.0)));
    }

    #[test]
    fn short_series_are_not_downsampled() {
        let values = [1.0, 2.0, 3.0];

        assert_eq!(downsample(&values, 10), values.to_vec());
    }

    #[test]
    fn downsampling_keeps_the_first_and_last_reading() {
        let values: Vec<f32> = (0..1000).map(|value| value as f32).collect();

        let reduced = downsample(&values, 100);

        assert_eq!(reduced.len(), 100);
        assert_eq!(reduced.first(), Some(&0.0));
        assert_eq!(reduced.last(), Some(&999.0));
    }

    #[test]
    fn downsampled_readings_stay_in_order() {
        let values: Vec<f32> = (0..500).map(|value| value as f32).collect();

        let reduced = downsample(&values, 50);

        assert!(reduced.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn nothing_is_plotted_without_two_readings() {
        assert_eq!(plot_points(&[], 100.0, 50.0), None);
        assert_eq!(plot_points(&[1.0], 100.0, 50.0), None);
    }

    #[test]
    fn nothing_is_plotted_into_a_zero_sized_widget() {
        assert_eq!(plot_points(&[1.0, 2.0], 0.0, 50.0), None);
        assert_eq!(plot_points(&[1.0, 2.0], 100.0, 0.0), None);
    }

    #[test]
    fn points_span_the_full_width() {
        let points = plot_points(&[1.0, 2.0, 3.0], 100.0, 50.0).expect("series is plottable");

        assert_eq!(points.len(), 3);
        assert_eq!(points[0].0, 0.0);
        assert_eq!(points[2].0, 100.0);
    }

    #[test]
    fn higher_readings_are_drawn_further_up() {
        let points = plot_points(&[1.0, 5.0], 100.0, 50.0).expect("series is plottable");

        // Cairo's y axis grows downwards, so "up" is a smaller number.
        assert!(
            points[1].1 < points[0].1,
            "expected the larger reading higher up: {points:?}"
        );
    }

    #[test]
    fn points_stay_inside_the_widget() {
        let values: Vec<f32> = (0..50).map(|value| value as f32).collect();

        for (_, y) in plot_points(&values, 200.0, 40.0).expect("series is plottable") {
            assert!(
                (0.0..=40.0).contains(&y),
                "point escaped the widget at y={y}"
            );
        }
    }

    #[test]
    fn long_series_are_capped_at_the_point_limit() {
        let values: Vec<f32> = (0..10_000).map(|value| value as f32).collect();

        let points = plot_points(&values, 500.0, 50.0).expect("series is plottable");

        assert_eq!(points.len(), MAX_POINTS);
    }

    #[test]
    fn summary_reports_low_mean_and_high() {
        let (low, mean, high) = summary(&[2.0, 4.0, 6.0]).expect("series has a summary");

        assert_eq!((low, high), (2.0, 6.0));
        assert_eq!(mean, 4.0);
    }

    #[test]
    fn an_empty_series_has_no_summary() {
        assert_eq!(summary(&[]), None);
    }
}
