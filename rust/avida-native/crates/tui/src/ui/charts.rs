//! The chart screen: selectable `UpdateRow` series on a ratatui `Chart`.

use avida_viewmodel::{decimate_min_max, format_number, series_of};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};
use ratatui::Frame;

use super::util::{block, clip_lines, trunc, DIM};
use crate::app::App;
use crate::color::{hue, Hue};
use crate::metrics::Metric;

pub const SERIES_HUES: [Hue; 6] = [
    Hue::Cyan,
    Hue::Yellow,
    Hue::Green,
    Hue::Magenta,
    Hue::LightRed,
    Hue::LightBlue,
];

/// The line marker of the i-th series. With colour the lines differ by
/// colour and all use braille; without it (`--colors none`) the marker is
/// what tells the series apart, so it changes with the series.
pub fn series_marker(i: usize, mono: bool) -> (Marker, &'static str) {
    if !mono {
        return (Marker::Braille, "⣿");
    }
    [
        (Marker::Braille, "⣿"),
        (Marker::Dot, "•"),
        (Marker::Block, "█"),
        (Marker::Bar, "▄"),
    ][i % 4]
}

/// The colour of the i-th selected series.
pub fn series_color(i: usize) -> Color {
    hue(SERIES_HUES[i % SERIES_HUES.len()])
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let (chart_area, list_area) = if area.width >= 80 {
        let [c, l] = Layout::horizontal([Constraint::Min(30), Constraint::Length(30)]).areas(area);
        (c, Some(l))
    } else {
        (area, None)
    };
    draw_chart(f, app, chart_area);
    if let Some(l) = list_area {
        draw_list(f, app, l);
    }
}

fn draw_list(f: &mut Frame, app: &App, area: Rect) {
    let h = usize::from(area.height.saturating_sub(2)).max(1);
    let start = app
        .chart_focus
        .saturating_sub(h / 2)
        .min(Metric::ALL.len().saturating_sub(h));
    let lines: Vec<Line> = Metric::ALL
        .iter()
        .enumerate()
        .skip(start)
        .take(h)
        .map(|(i, &m)| {
            let sel = app.chart_metrics.iter().position(|&x| x == m);
            let mark = match sel {
                Some(_) => "[x]",
                None => "[ ]",
            };
            let style = match sel {
                Some(k) => Style::new().fg(series_color(k)),
                None => Style::new(),
            };
            let style = if i == app.chart_focus {
                style.add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                style
            };
            Line::styled(
                format!(
                    "{mark} {}",
                    trunc(m.label(), usize::from(area.width).saturating_sub(7))
                ),
                style,
            )
        })
        .collect();
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
            .block(block("Series")),
        area,
    );
}

fn draw_chart(f: &mut Frame, app: &App, area: Rect) {
    let rows = app.history.rows();
    let title = if app.chart_log {
        "Charts (log axis)"
    } else {
        "Charts"
    };
    let blk = block(title);
    if rows.len() < 2 {
        f.render_widget(
            Paragraph::new("Waiting for two finished updates. Press . to step.")
                .style(DIM)
                .block(blk),
            area,
        );
        return;
    }
    let plot_w = usize::from(area.width.saturating_sub(12)).max(8);
    let max_points = plot_w * 2;
    let log = app.chart_log;
    let tx = |y: f64| if log { y.max(1e-12).log10() } else { y };

    let series: Vec<(Metric, Vec<(f64, f64)>)> = app
        .chart_metrics
        .iter()
        .map(|&m| {
            let (xs, ys) = series_of(rows, |r| m.value(r));
            let ys: Vec<f64> = if log {
                ys.into_iter()
                    .map(|y| if y > 0.0 { y } else { f64::NAN })
                    .collect()
            } else {
                ys
            };
            let pts: Vec<(f64, f64)> = decimate_min_max(&xs, &ys, max_points)
                .into_iter()
                .map(|(x, y)| (x, tx(y)))
                .collect();
            (m, pts)
        })
        .collect();

    let x0 = rows.first().map_or(0.0, |r| f64::from(r.update));
    let x1 = rows
        .last()
        .map_or(1.0, |r| f64::from(r.update))
        .max(x0 + 1.0);
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (_, pts) in &series {
        for &(_, y) in pts {
            lo = lo.min(y);
            hi = hi.max(y);
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        lo = 0.0;
        hi = 1.0;
    }
    if (hi - lo).abs() < f64::EPSILON {
        lo -= 0.5;
        hi += 0.5;
    } else if !log && lo > 0.0 && lo < hi * 0.5 {
        lo = 0.0;
    }
    let label = |y: f64| -> String {
        if log {
            format_number(10f64.powf(y))
        } else {
            format_number(y)
        }
    };

    let datasets: Vec<Dataset> = series
        .iter()
        .enumerate()
        .map(|(i, (m, pts))| {
            Dataset::default()
                .name(m.label())
                .marker(series_marker(i, app.color_mode.is_mono()).0)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(series_color(i)))
                .data(pts)
        })
        .collect();

    // The legend is drawn here rather than by the chart, which hides it
    // whenever the names do not fit a quarter of the width.
    let inner = blk.inner(area);
    f.render_widget(blk, area);
    let [legend_row, plot_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    let mut legend: Vec<Span> = Vec::new();
    for (i, (m, _)) in series.iter().enumerate() {
        if i > 0 {
            legend.push(Span::raw("  "));
        }
        let glyph = series_marker(i, app.color_mode.is_mono()).1;
        legend.push(Span::styled(glyph, Style::new().fg(series_color(i))));
        legend.push(Span::raw(format!(" {}", m.label())));
    }
    f.render_widget(
        Paragraph::new(clip_lines(
            vec![Line::from(legend)],
            usize::from(legend_row.width),
        )),
        legend_row,
    );

    let chart = Chart::new(datasets)
        .hidden_legend_constraints((Constraint::Length(0), Constraint::Length(0)))
        .x_axis(
            Axis::default()
                .title(Span::styled("update", DIM))
                .bounds([x0, x1])
                .labels(vec![
                    Span::raw(format_number(x0)),
                    Span::raw(format_number((x0 + x1) / 2.0)),
                    Span::raw(format_number(x1)),
                ])
                .style(DIM),
        )
        .y_axis(
            Axis::default()
                .bounds([lo, hi])
                .labels(vec![
                    Span::raw(label(lo)),
                    Span::raw(label((lo + hi) / 2.0)),
                    Span::raw(label(hi)),
                ])
                .style(DIM),
        );
    f.render_widget(chart, plot_area);
}
