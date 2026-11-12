//! The genotype table, most abundant first, with the dominant genome
//! underneath.

use avida_viewmodel::text::genotype_char;
use avida_viewmodel::{format_number, Palette};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;

use super::util::{block, DIM};
use super::waiting;
use crate::app::{App, GenoView};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let Some(frame) = app.frame.as_ref() else {
        waiting(f, area, " Waiting for the first frame...");
        return;
    };
    match app.geno_view {
        GenoView::Abundance => return super::abundance::draw(f, app, area),
        GenoView::Lineage => return super::lineage::draw(f, app, area),
        GenoView::Table => {}
    }
    let genome_h = if area.height >= 16 { 5 } else { 0 };
    let [table_area, genome_area] =
        Layout::vertical([Constraint::Min(4), Constraint::Length(genome_h)]).areas(area);

    let wide = area.width >= 100;
    let mut header = vec!["", "#", "name", "count", "born", "depth", "len", "fitness"];
    if wide {
        header.extend(["merit", "gest", "births", "thr"]);
    }
    let header = Row::new(header.into_iter().map(Cell::from)).style(
        Style::new()
            .add_modifier(Modifier::BOLD)
            .fg(ratatui::style::Color::Magenta),
    );
    let rows: Vec<Row> = frame
        .genotypes
        .iter()
        .map(|g| {
            let sym = frame.map.as_ref().map_or(' ', |m| genotype_char(m, g.id));
            let slot = frame.map.as_ref().and_then(|m| m.genotype_slot(g.id));
            let col = Palette::OkabeIto.category(slot);
            let mut cells = vec![
                Cell::from(sym.to_string())
                    .style(Style::new().fg(app.color_mode.color([col[0], col[1], col[2]]))),
                Cell::from((g.rank + 1).to_string()),
                Cell::from(g.name.clone()),
                Cell::from(g.abundance.to_string()),
                Cell::from(g.update_born.to_string()),
                Cell::from(g.depth.to_string()),
                Cell::from(g.genome_len.to_string()),
                Cell::from(format_number(g.ave_fitness)),
            ];
            if wide {
                cells.extend([
                    Cell::from(format_number(g.ave_merit)),
                    Cell::from(format_number(g.ave_gestation)),
                    Cell::from(g.last_births.to_string()),
                    Cell::from(if g.threshold { "*" } else { "" }),
                ]);
            }
            Row::new(cells)
        })
        .collect();
    let mut widths = vec![
        Constraint::Length(1),
        Constraint::Length(4),
        Constraint::Length(14),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(10),
    ];
    if wide {
        widths.extend([
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(4),
        ]);
    }
    let n = frame.genotypes.len();
    let title = format!(
        "Genotypes ({n}{}) v: abundance, lineage",
        if n == crate::app::GENOTYPE_LIST_LEN {
            ", top"
        } else {
            ""
        }
    );
    let table = Table::new(rows, widths)
        .header(header)
        .block(block(&title))
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▸");
    let mut state = TableState::default();
    if n > 0 {
        state.select(Some(app.geno_sel.min(n - 1)));
    }
    f.render_stateful_widget(table, table_area, &mut state);

    if genome_h > 0 {
        let sel = frame.genotypes.get(app.geno_sel);
        let (title, text) = match (&frame.dominant, sel) {
            (Some(d), Some(g)) if d.row.id == g.id => {
                (format!("Genome of {}", g.name), d.genome.clone())
            }
            (Some(d), _) => (
                format!("Dominant genome ({})", d.row.name),
                d.genome.clone(),
            ),
            _ => ("Genome".to_string(), String::new()),
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(text, DIM)))
                .wrap(Wrap { trim: false })
                .block(block(&title)),
            genome_area,
        );
    }
}
