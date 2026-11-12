//! The statistics screen: current values with a sparkline each, the
//! dominant genotype and the tasks performed.

use avida_viewmodel::format_number;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::util::{block, clip_lines, thousands, trunc, DIM, HEADING};
use crate::app::App;
use crate::metrics::{hbar, sparkline, Metric};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let rows = Metric::STATS.len() as u16;
    let top_h = (rows + 3).min(area.height.saturating_sub(4).max(4));
    let [top, bottom] =
        Layout::vertical([Constraint::Length(top_h), Constraint::Min(0)]).areas(area);
    draw_population(f, app, top);
    if bottom.height >= 3 {
        draw_tasks(f, app, bottom);
    }
}

fn draw_population(f: &mut Frame, app: &App, area: Rect) {
    let inner_w = usize::from(area.width.saturating_sub(2));
    let mut lines: Vec<Line> = Vec::new();

    if let Some(d) = app.frame.as_ref().and_then(|f| f.dominant.as_ref()) {
        let r = &d.row;
        lines.push(Line::from(vec![
            Span::styled("dominant ", DIM),
            Span::styled(r.name.clone(), HEADING),
            Span::raw(format!(
                "  x{}  length {}  fitness {}  born u{}",
                r.abundance,
                r.genome_len,
                format_number(r.ave_fitness),
                r.update_born
            )),
        ]));
    }

    let label_w = 20usize;
    let val_w = 11usize;
    let spark_w = inner_w.saturating_sub(label_w + val_w + 2);
    let rows = app.history.rows();
    let latest = app.latest_stats();
    for m in Metric::STATS {
        let value = latest.map_or_else(|| "-".to_string(), |r| m.format(m.value(r)));
        let series: Vec<f64> = rows.iter().map(|r| m.value(r)).collect();
        lines.push(Line::from(vec![
            Span::styled(format!("{:<label_w$}", m.label()), DIM),
            Span::styled(
                format!("{value:>val_w$}"),
                Style::new().add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(sparkline(&series, spark_w), Style::new().fg(Color::Cyan)),
        ]));
    }
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
            .block(block("Population")),
        area,
    );
}

fn draw_tasks(f: &mut Frame, app: &App, area: Rect) {
    let Some(env) = app.env() else {
        return;
    };
    let orgs = app.latest_stats().map_or(0, |r| r.orgs).max(1);
    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    let col_w: u16 = 30;
    let cols = (inner.width / col_w).max(1);
    let per_page = usize::from(cols) * usize::from(inner.height.max(1));
    let n = env.tasks.len();
    let offset = app.stats_task_offset.min(n.saturating_sub(1));
    let title = if n == 0 {
        "Tasks (none defined)".to_string()
    } else if n > per_page {
        format!(
            "Tasks {}-{} of {}  (Left/Right pages)",
            offset + 1,
            (offset + per_page).min(n),
            n
        )
    } else {
        "Tasks".to_string()
    };
    f.render_widget(block(&title), area);
    let name_w = usize::from(col_w).saturating_sub(15);
    for (i, t) in env.tasks.iter().enumerate().skip(offset).take(per_page) {
        let k = i - offset;
        let col = (k / usize::from(inner.height.max(1))) as u16;
        let row = (k % usize::from(inner.height.max(1))) as u16;
        let r = Rect {
            x: inner.x + col * col_w,
            y: inner.y + row,
            width: col_w.min(inner.width - col * col_w),
            height: 1,
        };
        let frac = f64::from(t.last_count) / f64::from(orgs);
        let line = Line::from(vec![
            Span::raw(format!("{:<name_w$}", trunc(&t.name, name_w))),
            Span::styled(
                format!("{:>6}", thousands(i64::from(t.last_count))),
                Style::new().add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(hbar(frac, 6), Style::new().fg(Color::Green)),
        ]);
        f.render_widget(Paragraph::new(line), r);
    }
}
