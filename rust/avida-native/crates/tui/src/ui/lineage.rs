//! The ancestry of one genotype, founder first, with the fitness along it.

use avida_viewmodel::format_number;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use super::util::{block, trunc, BOLD, DIM};
use super::waiting;
use crate::app::App;
use crate::metrics::sparkline;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    if app.frame.is_none() {
        waiting(f, area, " Waiting for the first frame...");
        return;
    };
    let rows = app.lineage_rows();
    let Some(traced) = app.highlight else {
        f.render_widget(
            Paragraph::new(
                " No lineage traced. Press v to go back to the table, select a genotype and press l.",
            )
            .style(DIM)
            .block(block("Lineage")),
            area,
        );
        return;
    };
    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(format!(
                " Waiting for the lineage of genotype #{traced} (the next frame carries it)."
            ))
            .style(DIM)
            .block(block("Lineage")),
            area,
        );
        return;
    }
    let sum_h: u16 = if area.height >= 12 { 3 } else { 2 };
    let [sum, table_area] =
        Layout::vertical([Constraint::Length(sum_h), Constraint::Min(3)]).areas(area);

    let first = rows[0];
    let last = rows[rows.len() - 1];
    let w = usize::from(sum.width);
    let fit: Vec<f64> = rows
        .iter()
        .map(|g| {
            if g.ave_fitness.is_finite() {
                g.ave_fitness
            } else {
                0.0
            }
        })
        .collect();
    let mut lines = vec![
        Line::styled(
            trunc(
                &format!(
                    " {} #{}: {} genotype{} back to the founder, born u{}",
                    last.name,
                    last.id,
                    rows.len(),
                    if rows.len() == 1 { "" } else { "s" },
                    first.update_born
                ),
                w,
            ),
            BOLD,
        ),
        Line::styled(
            trunc(
                &format!(
                    " length {} > {}   fitness {} > {}   {}",
                    first.genome_len,
                    last.genome_len,
                    format_number(first.ave_fitness),
                    format_number(last.ave_fitness),
                    sparkline(&fit, 24)
                ),
                w,
            ),
            DIM,
        ),
    ];
    if sum_h > 2 {
        lines.push(Line::styled(
            " Up/Down choose  Enter find on the map  v next view",
            DIM,
        ));
    }
    f.render_widget(Paragraph::new(lines), sum);

    let header = Row::new(["depth", "name", "born", "len", "fitness", "alive"].map(Cell::from))
        .style(Style::new().add_modifier(Modifier::BOLD).fg(Color::Magenta));
    let body: Vec<Row> = rows
        .iter()
        .map(|g| {
            let alive = if g.abundance > 0 {
                g.abundance.to_string()
            } else {
                "-".to_string()
            };
            Row::new([
                Cell::from(g.depth.to_string()),
                Cell::from(g.name.clone()),
                Cell::from(format!("u{}", g.update_born)),
                Cell::from(g.genome_len.to_string()),
                Cell::from(format_number(g.ave_fitness)),
                Cell::from(alive),
            ])
        })
        .collect();
    let title = format!("Lineage of #{traced} (founder first)");
    let table = Table::new(
        body,
        [
            Constraint::Length(6),
            Constraint::Length(16),
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Length(7),
        ],
    )
    .header(header)
    .block(block(&title))
    .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
    .highlight_symbol("▸");
    let mut state = TableState::default();
    state.select(Some(app.lineage_sel.min(rows.len() - 1)));
    f.render_stateful_widget(table, table_area, &mut state);
}
