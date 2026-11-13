//! The environment screen: resources, reactions and tasks.

use avida_viewmodel::format_number;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::util::{block, DIM};
use super::waiting;
use crate::app::{App, EnvTab};
use crate::color::{hue, Hue};

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
        waiting(f, area, super::FIRST_FRAME);
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
    spans.push(Span::styled("  Left and Right switch the list", DIM));
    f.render_widget(Paragraph::new(Line::from(spans)), tabs);

    let head = Style::new()
        .add_modifier(Modifier::BOLD)
        .fg(hue(Hue::Magenta));
    // Each tab is a list of text rows; `numeric` marks the right-aligned columns.
    let (title, header, data, numeric): (&str, Vec<&str>, Vec<Vec<String>>, Vec<bool>) =
        match app.env_tab {
            EnvTab::Resources => (
                "Resources",
                vec!["name", "geometry", "quantity", "inflow", "outflow"],
                env.resources
                    .iter()
                    .map(|r| {
                        vec![
                            r.name.clone(),
                            geometry_name(r.geometry).to_string(),
                            format_number(r.quantity),
                            r.inflow.map_or("-".into(), format_number),
                            r.outflow.map_or("-".into(), format_number),
                        ]
                    })
                    .collect(),
                vec![false, false, true, true, true],
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
                        vec![
                            r.name.clone(),
                            task,
                            format_number(r.value),
                            r.last_count.to_string(),
                        ]
                    })
                    .collect(),
                vec![false, false, true, true],
            ),
            EnvTab::Tasks => (
                "Tasks",
                vec!["name", "description", "last count"],
                env.tasks
                    .iter()
                    .map(|t| vec![t.name.clone(), t.desc.clone(), t.last_count.to_string()])
                    .collect(),
                vec![false, false, true],
            ),
        };
    // Columns are as wide as their longest entry and sit close together;
    // the last text column takes whatever is left.
    let widths: Vec<Constraint> = (0..header.len())
        .map(|c| {
            let w = data
                .iter()
                .map(|r| r[c].chars().count())
                .chain(std::iter::once(header[c].chars().count()))
                .max()
                .unwrap_or(1);
            Constraint::Length(w.min(60) as u16)
        })
        .collect();
    let total = data.len();
    let rows: Vec<Row> = data
        .into_iter()
        .map(|r| {
            Row::new(r.into_iter().zip(&numeric).map(|(t, &num)| {
                if num {
                    Cell::from(Line::from(t).right_aligned())
                } else {
                    Cell::from(t)
                }
            }))
        })
        .collect();
    let header_row = Row::new(header.iter().zip(&numeric).map(|(&t, &num)| {
        if num {
            Cell::from(Line::from(t).right_aligned())
        } else {
            Cell::from(t)
        }
    }))
    .style(head);
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
    // Spread the gaps over a wider panel, up to 100 columns of table.
    let used: usize = widths
        .iter()
        .map(|c| match c {
            Constraint::Length(n) => usize::from(*n),
            _ => 0,
        })
        .sum();
    let gaps = header.len().saturating_sub(1).max(1);
    let spacing = (usize::from(body.width.saturating_sub(4)).min(100)).saturating_sub(used) / gaps;
    let table = Table::new(shown, widths)
        .column_spacing(spacing.clamp(3, 12) as u16)
        .header(header_row)
        .block(block(&note));
    if total == 0 {
        f.render_widget(
            Paragraph::new(format!(
                " The environment defines no {}.",
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
