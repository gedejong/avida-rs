//! The open-endedness screen: the MODES measures and the genealogy measures
//! as small charts, one per measure, in the order of the series that
//! `avida_viewmodel::open_ended` builds (the same series the desktop
//! interface draws). The charts of a group share their x range. Up and Down
//! choose a chart, whose meaning is written under the charts; Left and Right
//! switch to the next group (one per MODES instance, one per metrics file).
//! A terminal has no hover, so the newest value is in each title.

use avida_viewmodel::{format_number, OeChart, OeGroup, OeView, PLAYBACK_NOTE};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph, Wrap};
use ratatui::Frame;

use super::charts::{series_color, series_marker};
use super::util::{accent, block, clip_lines, trunc, BOLD, DIM};
use crate::app::App;

/// The fewest rows a chart tile gets: a title row and a plot with its axis.
const TILE_MIN_H: u16 = 6;
/// Two columns of charts from this width on.
const TWO_COLUMNS_AT: u16 = 100;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let blk = block("Open-endedness");
    let inner = blk.inner(area);
    f.render_widget(blk, area);
    if app.playback.is_some() {
        f.render_widget(
            Paragraph::new(PLAYBACK_NOTE).wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    let view = app.open_ended_view();
    if view.is_unconfigured() {
        unconfigured(f, &view, inner);
        return;
    }
    let group_i = app.oe_group.min(view.groups.len() - 1);
    let g = &view.groups[group_i];

    let tall = inner.height >= 24;
    let notice_h = u16::from(view.notice.is_some() && tall) * 2;
    let explain_h = if tall { 4 } else { 3 };
    // The caption (file, resolution, filter mode) is left out of a short
    // terminal; the heading already says the lag and the filter.
    let caption_h = if tall { 3 } else { 0 };
    let [head, caption, body, explain, notice] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(caption_h),
        Constraint::Min(1),
        Constraint::Length(explain_h),
        Constraint::Length(notice_h),
    ])
    .areas(inner);

    let mut title = vec![Span::styled(
        trunc(&g.title, usize::from(head.width).saturating_sub(12)),
        BOLD,
    )];
    if view.groups.len() > 1 {
        title.push(Span::styled(
            format!("  < {}/{} >", group_i + 1, view.groups.len()),
            DIM,
        ));
    }
    f.render_widget(Paragraph::new(Line::from(title)), head);

    let mut text = String::new();
    if let (Some(intro), true) = (&view.intro, g.title.starts_with("MODES")) {
        if inner.height >= 32 {
            text.push_str(intro);
            text.push(' ');
        }
    }
    text.push_str(&g.caption);
    f.render_widget(
        Paragraph::new(text).style(DIM).wrap(Wrap { trim: true }),
        caption,
    );

    if let Some(note) = &g.note {
        let mut text = note.clone();
        if let Some(intro) = &view.intro {
            text = format!("{intro}\n\n{text}");
        }
        f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), body);
    } else {
        let focus = app.oe_focus.min(g.charts.len().saturating_sub(1));
        draw_tiles(f, g, focus, body, app.color_mode.is_mono());
        if let Some(c) = g.charts.get(focus) {
            f.render_widget(
                Paragraph::new(c.explanation).wrap(Wrap { trim: true }),
                explain,
            );
        }
    }
    if let Some(n) = &view.notice {
        if notice_h > 0 {
            let mut lines = vec![Line::from(Span::styled(
                trunc(n, usize::from(notice.width)),
                DIM,
            ))];
            lines.push(Line::from(Span::styled(
                view.example_lines.join("   "),
                accent(),
            )));
            f.render_widget(
                Paragraph::new(clip_lines(lines, usize::from(notice.width))),
                notice,
            );
        }
    }
}

/// The empty state: no action in the events file.
fn unconfigured(f: &mut Frame, view: &OeView, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    if let Some(n) = &view.notice {
        lines.push(Line::from(n.as_str()));
    }
    lines.push(Line::default());
    for l in &view.example_lines {
        lines.push(Line::from(Span::styled(format!("  {l}"), accent())));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// The group's charts as a grid of tiles, scrolled by pages so that the
/// chosen chart is on screen.
fn draw_tiles(f: &mut Frame, g: &OeGroup, focus: usize, area: Rect, mono: bool) {
    let n = g.charts.len();
    let cols: usize = if area.width >= TWO_COLUMNS_AT { 2 } else { 1 };
    let rows_total = n.div_ceil(cols);
    let fit_h = area.height / rows_total.max(1) as u16;
    let (tile_h, visible_rows) = if fit_h >= TILE_MIN_H {
        (fit_h, rows_total)
    } else {
        // As many rows as fit at the least height, sharing the space.
        let v = usize::from((area.height / TILE_MIN_H).max(1)).min(rows_total);
        ((area.height / v as u16).max(1), v)
    };
    // Pages of `visible_rows` rows; the last page is the final rows, so it is
    // never half empty.
    let top_row = ((focus / cols / visible_rows) * visible_rows).min(rows_total - visible_rows);
    // The x range every chart of the group shares.
    let (mut x0, mut x1) = (f64::INFINITY, f64::NEG_INFINITY);
    for c in &g.charts {
        for &(x, _) in &c.points {
            x0 = x0.min(x);
            x1 = x1.max(x);
        }
    }
    if !x0.is_finite() || !x1.is_finite() {
        (x0, x1) = (0.0, 1.0);
    }
    if (x1 - x0).abs() < f64::EPSILON {
        x0 -= 0.5;
        x1 += 0.5;
    }
    let col_w = area.width / cols as u16;
    for row in 0..visible_rows {
        for col in 0..cols {
            let i = (top_row + row) * cols + col;
            let Some(c) = g.charts.get(i) else { continue };
            let tile = Rect {
                x: area.x + col as u16 * col_w,
                y: area.y + row as u16 * tile_h,
                // A gutter between the columns keeps the value labels of one
                // chart off the next one's axis.
                width: if col + 1 == cols {
                    area.width - col as u16 * col_w
                } else {
                    col_w - 3
                },
                height: tile_h,
            };
            // The x labels sit under the last row of tiles on screen (the
            // axis is shared), or under every tile when there is room.
            let last_row = row + 1 == visible_rows || (top_row + row + 1) * cols >= n;
            let show_x = last_row || tile_h >= 9;
            draw_tile(f, c, i == focus, (x0, x1), tile, show_x, mono);
        }
    }
    if visible_rows < rows_total {
        let pages = rows_total.div_ceil(visible_rows);
        let note = format!(
            " page {}/{} ",
            (top_row + visible_rows).div_ceil(visible_rows),
            pages
        );
        let w = note.chars().count() as u16;
        if area.width > w {
            f.render_widget(
                Paragraph::new(Span::styled(note, DIM)),
                Rect::new(area.x + area.width - w, area.y, w, 1),
            );
        }
    }
}

fn draw_tile(
    f: &mut Frame,
    c: &OeChart,
    focused: bool,
    (x0, x1): (f64, f64),
    area: Rect,
    show_x: bool,
    mono: bool,
) {
    if area.height < 2 || area.width < 8 {
        return;
    }
    let [title_row, plot] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let marker = if focused { "> " } else { "  " };
    let title_style = if focused {
        accent().add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        BOLD
    };
    let mut spans = vec![
        Span::raw(marker),
        Span::styled(format!(" {} ", c.title), title_style),
    ];
    if let Some((x, y)) = c.latest() {
        spans.push(Span::styled(
            format!(
                " {} at {} {}",
                format_number(y),
                c.x_label,
                format_number(x)
            ),
            DIM,
        ));
    }
    f.render_widget(
        Paragraph::new(clip_lines(
            vec![Line::from(spans)],
            usize::from(title_row.width),
        )),
        title_row,
    );

    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &(_, y) in &c.points {
        lo = lo.min(y);
        hi = hi.max(y);
    }
    if !lo.is_finite() || !hi.is_finite() {
        (lo, hi) = (0.0, 1.0);
    }
    if c.whole && lo > 0.0 {
        lo = 0.0;
    }
    if (hi - lo).abs() < f64::EPSILON {
        hi = lo + 1.0;
    }
    let data: Vec<(f64, f64)> = c.points.clone();
    let (m, _): (Marker, &str) = series_marker(0, mono);
    let ds = Dataset::default()
        .marker(m)
        .graph_type(GraphType::Line)
        .style(Style::new().fg(series_color(0)))
        .data(&data);
    let mut x_axis = Axis::default().bounds([x0, x1]).style(DIM);
    if show_x {
        x_axis = x_axis.title(Span::styled(c.x_label, DIM)).labels(vec![
            Span::raw(format_number(x0)),
            Span::raw(format_number((x0 + x1) / 2.0)),
            Span::raw(format_number(x1)),
        ]);
    }
    let y_axis = Axis::default().bounds([lo, hi]).style(DIM).labels(vec![
        Span::raw(format_number(lo)),
        Span::raw(format_number((lo + hi) / 2.0)),
        Span::raw(format_number(hi)),
    ]);
    f.render_widget(
        Chart::new(vec![ds])
            .hidden_legend_constraints((Constraint::Length(0), Constraint::Length(0)))
            .x_axis(x_axis)
            .y_axis(y_axis),
        plot,
    );
}
