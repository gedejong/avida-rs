//! The environment screen: resources, reactions and tasks.

use avida_viewmodel::format_number;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::util::{block, DIM};
use super::waiting;
use crate::app::{App, EnvTab};

fn geometry_name(g: i32) -> &'static str {
    match g {
        0 => "global",
        1 => "grid",
        2 => "torus",
        3 => "cell",
        4 => "demes",
        5 => "partial",
        _ => "spatial",
    }
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let Some(env) = app.env() else {
        waiting(f, area, " Waiting for the first frame...");
        return;
    };
    let [tabs, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(2)]).areas(area);
    let mut spans = vec![Span::raw(" ")];
    for t in EnvTab::ALL {
        let n = match t {
            EnvTab::Resources => env.resources.len(),
            EnvTab::Reactions => env.reactions.len(),
            EnvTab::Tasks => env.tasks.len(),
        };
        let style = if t == app.env_tab {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
        };
        spans.push(Span::styled(format!(" {} ({n}) ", t.title()), style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled("  Left/Right switch", DIM));
    f.render_widget(Paragraph::new(Line::from(spans)), tabs);

    let head = Style::new().add_modifier(Modifier::BOLD).fg(Color::Magenta);
    let (title, header, rows, widths): (&str, Vec<&str>, Vec<Row>, Vec<Constraint>) =
        match app.env_tab {
            EnvTab::Resources => (
                "Resources",
                vec!["name", "geometry", "quantity", "inflow", "outflow"],
                env.resources
                    .iter()
                    .map(|r| {
                        Row::new(vec![
                            r.name.clone(),
                            geometry_name(r.geometry).to_string(),
                            format_number(r.quantity),
                            r.inflow.map_or("-".into(), format_number),
                            r.outflow.map_or("-".into(), format_number),
                        ])
                    })
                    .collect(),
                vec![
                    Constraint::Min(14),
                    Constraint::Length(10),
                    Constraint::Length(12),
                    Constraint::Length(10),
                    Constraint::Length(10),
                ],
            ),
            EnvTab::Reactions => (
                "Reactions",
                vec!["name", "task", "value", "last count"],
                env.reactions
                    .iter()
                    .map(|r| {
                        let task = r
                            .task
                            .and_then(|k| env.tasks.get(k))
                            .map_or("-".to_string(), |t| t.name.clone());
                        Row::new(vec![
                            r.name.clone(),
                            task,
                            format_number(r.value),
                            r.last_count.to_string(),
                        ])
                    })
                    .collect(),
                vec![
                    Constraint::Min(14),
                    Constraint::Length(14),
                    Constraint::Length(10),
                    Constraint::Length(10),
                ],
            ),
            EnvTab::Tasks => (
                "Tasks",
                vec!["name", "description", "last count"],
                env.tasks
                    .iter()
                    .map(|t| {
                        Row::new(vec![
                            t.name.clone(),
                            t.desc.clone(),
                            t.last_count.to_string(),
                        ])
                    })
                    .collect(),
                vec![
                    Constraint::Min(14),
                    Constraint::Min(14),
                    Constraint::Length(10),
                ],
            ),
        };
    let total = rows.len();
    let visible = usize::from(body.height.saturating_sub(3)).max(1);
    let scroll = app.env_scroll.min(total.saturating_sub(visible));
    let shown: Vec<Row> = rows.into_iter().skip(scroll).take(visible).collect();
    let note = if total > visible {
        format!(
            "{title} {}-{} of {total}",
            scroll + 1,
            (scroll + visible).min(total)
        )
    } else {
        title.to_string()
    };
    let table = Table::new(shown, widths)
        .header(Row::new(header.into_iter().map(Cell::from)).style(head))
        .block(block(&note));
    if total == 0 {
        f.render_widget(
            Paragraph::new(format!(
                "The environment defines no {}.",
                title.to_lowercase()
            ))
            .style(DIM)
            .block(block(title)),
            body,
        );
    } else {
        f.render_widget(table, body);
    }
}
