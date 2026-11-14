//! The "Filter organisms" dialog: the kind list, the task, family and
//! number steps of "Add condition", and the list of conditions.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use super::util::{accent, block, centered, clip_lines, trunc, DIM};
use crate::app::App;
use crate::filtering::{
    condition_label, genotype_name, prop_range, task_name, Comparison, Dialog, Kind, Stage, Tab,
    REGION_FIELDS,
};
use avida_viewmodel::format_number;

const SELECTED: Style = Style::new().add_modifier(Modifier::REVERSED);

/// The rows of `items` that fit `rows`, scrolled so that `sel` is shown.
fn window(sel: usize, n: usize, rows: usize) -> std::ops::Range<usize> {
    let rows = rows.max(1);
    let top = (sel + 1).saturating_sub(rows).min(n.saturating_sub(rows));
    top..(top + rows).min(n)
}

fn list_line(text: String, selected: bool, width: usize) -> Line<'static> {
    let text = trunc(&text, width.saturating_sub(2));
    if selected {
        Line::styled(format!("▸ {text}"), SELECTED.add_modifier(Modifier::BOLD))
    } else {
        Line::raw(format!("  {text}"))
    }
}

pub fn draw(f: &mut Frame, app: &App, d: &Dialog, area: Rect) {
    let w = area.width.saturating_sub(2).min(64);
    let h = area.height.saturating_sub(2).clamp(8, 20);
    let r = centered(area, w, h);
    f.render_widget(Clear, r);
    let blk = block("Filter organisms");
    let inner = blk.inner(r);
    f.render_widget(blk, r);
    if inner.height < 4 {
        return;
    }
    let width = usize::from(inner.width);
    let conds = app.filtering.filter.len();

    // Header: the two lists.
    let tab = |label: String, on: bool| {
        if on {
            Span::styled(
                format!(" {label} "),
                Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
            )
        } else {
            Span::styled(format!(" {label} "), DIM)
        }
    };
    let header = Line::from(vec![
        tab("Add condition".to_string(), d.tab == Tab::Add),
        Span::raw(" "),
        tab(format!("Conditions ({conds})"), d.tab == Tab::Conditions),
    ]);
    f.render_widget(
        Paragraph::new(header),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let body = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 3);
    let notes = Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 2);
    let rows = usize::from(body.height);
    let frame = app.frame.as_deref();
    let mut lines: Vec<Line> = Vec::new();
    let mut note = String::new();
    let mut keys: String;

    if d.tab == Tab::Conditions {
        let sel = app.filtering.selection();
        let n = conds;
        let at = d.sel.min(n.saturating_sub(1));
        for i in window(at, n, rows) {
            let c = &app.filtering.filter.conditions()[i];
            let mut t = format!("{}. {}", i + 1, condition_label(c, frame));
            if sel.is_some_and(|s| s.ignored.contains(&i)) {
                t.push_str(" (no data)");
            }
            lines.push(list_line(t, i == at, width));
        }
        if n == 0 {
            lines.push(Line::styled("No conditions.", DIM));
        }
        keys = "Del removes  c clears all  Tab adds  Esc closes".to_string();
    } else {
        match d.stage {
            Stage::Kind => {
                let kinds = Kind::all();
                let at = d.kind_sel.min(kinds.len() - 1);
                for i in window(at, kinds.len(), rows) {
                    lines.push(list_line(kinds[i].label().to_string(), i == at, width));
                }
                note = kinds[at].note().to_string();
                keys = "Up/Down choose  Enter next".to_string();
                if conds > 0 {
                    keys.push_str("  Tab conditions");
                }
                keys.push_str("  Esc closes");
            }
            Stage::Task(performs) => {
                let n = frame.map_or(0, |f| f.env.tasks.len());
                let at = d.sel.min(n.saturating_sub(1));
                lines.push(Line::styled(
                    if performs {
                        "Organisms that performed:"
                    } else {
                        "Organisms that did not perform:"
                    },
                    DIM,
                ));
                for i in window(at, n, rows.saturating_sub(1)) {
                    let mut t = task_name(frame, i);
                    match app.filtering.filter.task_state(i) {
                        Some(true) => t.push_str("  (set: performs)"),
                        Some(false) => t.push_str("  (set: does not perform)"),
                        None => {}
                    }
                    lines.push(list_line(t, i == at, width));
                }
                keys = "Up/Down choose  Enter adds  Esc back".to_string();
            }
            Stage::Family => {
                let fams: Vec<_> = frame
                    .and_then(|f| f.map.as_ref())
                    .map(|m| m.families.to_vec())
                    .unwrap_or_default();
                let at = d.sel.min(fams.len().saturating_sub(1));
                if fams.is_empty() {
                    lines.push(Line::styled("Looking up the families...", DIM));
                }
                for i in window(at, fams.len(), rows) {
                    let fam = fams[i];
                    lines.push(list_line(
                        format!(
                            "Family of {}  ({} organisms)",
                            genotype_name(frame, fam.root),
                            fam.organisms
                        ),
                        i == at,
                        width,
                    ));
                }
                keys = "Up/Down choose  Enter adds  Esc back".to_string();
            }
            Stage::Prop(p) => {
                lines.push(Line::styled(
                    p.label().to_string(),
                    Style::new().add_modifier(Modifier::BOLD),
                ));
                lines.push(Line::styled(p.description().to_string(), DIM));
                let range = frame
                    .and_then(|f| f.map.as_ref())
                    .and_then(|m| prop_range(m, p));
                lines.push(Line::raw(match range {
                    Some((lo, hi)) => {
                        format!("Now from {} to {}.", format_number(lo), format_number(hi))
                    }
                    None => "Waiting for the property's values.".to_string(),
                }));
                lines.push(Line::raw(""));
                let mut spans = vec![Span::styled("Comparison ", DIM)];
                for c in Comparison::ALL {
                    if c == d.comparison {
                        spans.push(Span::styled(
                            format!("[{}]", c.label()),
                            Style::new().add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        spans.push(Span::raw(format!(" {} ", c.label())));
                    }
                    spans.push(Span::raw(" "));
                }
                lines.push(Line::from(spans));
                let names: &[&str] = if d.comparison == Comparison::Between {
                    &["From", "To"]
                } else {
                    &["Value"]
                };
                for (i, name) in names.iter().enumerate() {
                    lines.push(field_line(name, d, i));
                }
                keys = "Left/Right compare  Tab field  Enter adds  Esc back".to_string();
            }
            Stage::Region => {
                lines.push(Line::styled(
                    "Cells, both corners included. Starts as the cells in view.".to_string(),
                    DIM,
                ));
                if let Some((w, h)) = app.world_size() {
                    lines.push(Line::styled(format!("The world is {w} by {h} cells."), DIM));
                }
                for (i, name) in REGION_FIELDS.iter().enumerate() {
                    lines.push(field_line(name, d, i));
                }
                keys = "Tab field  Enter adds  Esc back".to_string();
            }
        }
    }
    if let Some(e) = &d.error {
        note = e.clone();
    }
    f.render_widget(Paragraph::new(clip_lines(lines, width)), body);
    let note_style = if d.error.is_some() {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        DIM
    };
    let hint = Line::styled(trunc(&keys, width), accent());
    f.render_widget(
        Paragraph::new(vec![Line::styled(trunc(&note, width), note_style), hint]),
        notes,
    );
}

fn field_line(name: &str, d: &Dialog, i: usize) -> Line<'static> {
    let on = d.field == i;
    let text = d.fields.get(i).cloned().unwrap_or_default();
    Line::from(vec![
        Span::raw(if on { "▸ " } else { "  " }),
        Span::styled(format!("{name:<7}"), DIM),
        Span::styled(
            format!("{text}{}", if on { "_" } else { "" }),
            Style::new().add_modifier(if on {
                Modifier::BOLD | Modifier::UNDERLINED
            } else {
                Modifier::empty()
            }),
        ),
    ])
}
