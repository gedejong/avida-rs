//! The configuration screen: the effective settings (filterable), the
//! overrides typed so far, and what a restart would run.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;

use super::util::{block, trunc, ACCENT, BOLD, DIM};
use crate::app::App;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(setup) = app.setup.clone() else {
        f.render_widget(
            Paragraph::new(" The configuration is not available yet.").style(DIM),
            area,
        );
        return;
    };
    let vis = app.config.visible(&setup);
    let n = vis.len();
    if n > 0 {
        app.config.sel = app.config.sel.min(n - 1);
    } else {
        app.config.sel = 0;
    }
    let detail_h: u16 = if area.height >= 18 { 6 } else { 3 };
    let [head, list, detail] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(detail_h),
    ])
    .areas(area);

    // Filter line.
    let overrides = app.config.overrides.len();
    let mut spans = vec![
        Span::styled(" filter ", DIM),
        Span::styled(
            format!(
                "{}{}",
                app.config.query,
                if app.config.filtering { "_" } else { "" }
            ),
            if app.config.filtering {
                ACCENT.add_modifier(Modifier::BOLD)
            } else {
                BOLD
            },
        ),
    ];
    if app.config.changed_only {
        spans.push(Span::styled(
            "  [changed only]",
            Style::new().fg(Color::Magenta),
        ));
    }
    spans.push(Span::styled(
        format!(
            "  {n} of {} settings, {overrides} override{}",
            setup.settings.len(),
            if overrides == 1 { "" } else { "s" }
        ),
        DIM,
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), head);

    // The list.
    let wide = area.width >= 90;
    let value_w = if wide { 22 } else { 14 };
    let rows: Vec<Row> = vis
        .iter()
        .map(|&i| {
            let s = &setup.settings[i];
            let ov = app.config.override_of(s.name);
            let value = match ov {
                Some(v) => format!("{} > {v}", trunc(&s.value, value_w / 2 - 2)),
                None => s.value.clone(),
            };
            let style = if ov.is_some() {
                Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD)
            } else if s.is_changed() {
                Style::new().fg(Color::Green)
            } else {
                Style::new()
            };
            let mut cells = vec![
                Cell::from(s.name),
                Cell::from(trunc(&value, value_w)),
                Cell::from(trunc(&s.default, 12)),
            ];
            if wide {
                cells.push(Cell::from(s.group.trim_end_matches("_GROUP")));
            }
            Row::new(cells).style(style)
        })
        .collect();
    let mut widths = vec![
        Constraint::Length(26),
        Constraint::Length(value_w as u16),
        Constraint::Length(12),
    ];
    let mut header = vec!["setting", "value", "default"];
    if wide {
        widths.push(Constraint::Min(8));
        header.push("group");
    }
    let table = Table::new(rows, widths)
        .header(Row::new(header.into_iter().map(Cell::from)).style(DIM))
        .block(block("Settings"))
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▸");
    let mut state = TableState::default()
        .with_offset(app.config.offset)
        .with_selected((n > 0).then_some(app.config.sel));
    f.render_stateful_widget(table, list, &mut state);
    app.config.offset = state.offset();

    // Detail or the edit prompt.
    let mut lines: Vec<Line> = Vec::new();
    if let Some(e) = &app.config.edit {
        let kind = setup.setting(e.name).map_or("", |s| s.kind);
        lines.push(Line::from(vec![
            Span::styled(format!("{} ({kind}) = ", e.name), BOLD),
            Span::styled(
                format!("{}_", e.buffer),
                ACCENT.add_modifier(Modifier::BOLD),
            ),
        ]));
        match &e.error {
            Some(err) => lines.push(Line::styled(err.clone(), Style::new().fg(Color::Red))),
            None => lines.push(Line::styled(
                "Enter keeps the value, Esc cancels, Ctrl-U clears.",
                DIM,
            )),
        }
    } else if let Some(s) = vis.get(app.config.sel).map(|&i| &setup.settings[i]) {
        lines.push(Line::from(vec![
            Span::styled(s.name, BOLD),
            Span::styled(format!("  {} in {}", s.kind, s.group), DIM),
            if s.from_command_line {
                Span::styled("  set on the command line", Style::new().fg(Color::Cyan))
            } else {
                Span::raw("")
            },
        ]));
    }
    if overrides > 0 || detail_h > 4 {
        let args = app.restart_args();
        let cmd = if args.is_empty() {
            "(no arguments)".to_string()
        } else {
            args.join(" ")
        };
        lines.push(Line::from(vec![
            Span::styled("restart runs: ", DIM),
            Span::raw(cmd),
        ]));
    }
    if app.config.edit.is_none() && detail_h > 4 {
        if let Some(s) = vis.get(app.config.sel).map(|&i| &setup.settings[i]) {
            lines.push(Line::styled(s.desc.replace('\n', " "), DIM));
        }
    }
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(block("Setting (e edit, R restart)")),
        detail,
    );
}
