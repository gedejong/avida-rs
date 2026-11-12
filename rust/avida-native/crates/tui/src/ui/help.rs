//! The key help overlay, generated from the binding table in
//! [`crate::actions`] (the same table the command palette lists and input
//! handling resolves keys with).

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use super::util::{block, centered, trunc, ACCENT, DIM, HEADING};
use crate::actions::{keys_label, title_for, Scope, BINDINGS};
use crate::app::App;

/// Key column width.
const KEY_W: usize = 14;

/// `(key label, text, screens)` per binding, with `("", heading, [])` rows
/// between sections; `micro_ops` when a step of the watched organism is a
/// CPU cycle of several micro-operations. Pure, so a test can compare it
/// with the table.
pub fn entries(micro_ops: bool) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut sections: Vec<&str> = Vec::new();
    for b in BINDINGS {
        if !sections.contains(&b.section) {
            sections.push(b.section);
        }
    }
    for sec in sections {
        out.push((String::new(), sec.to_string(), String::new()));
        for b in BINDINGS.iter().filter(|b| b.section == sec) {
            let screens = match b.scope {
                Scope::Global => String::new(),
                Scope::Screens(s) => s.iter().map(|s| s.title()).collect::<Vec<_>>().join(", "),
            };
            out.push((
                keys_label(b.keys),
                title_for(b, micro_ops).to_string(),
                screens,
            ));
        }
    }
    out
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = entries(app.steps_run_micro_ops());
    let w = area.width.saturating_sub(2).min(78);
    let h = area.height.saturating_sub(2).min(rows.len() as u16 + 2);
    let r = centered(area, w, h);
    f.render_widget(Clear, r);
    let visible = usize::from(h.saturating_sub(2));
    let max_scroll = rows.len().saturating_sub(visible);
    app.help_scroll = app.help_scroll.min(max_scroll);
    let title = if max_scroll > 0 {
        format!(
            "Keys {}-{} of {} (Up/Down scroll, Esc closes)",
            app.help_scroll + 1,
            (app.help_scroll + visible).min(rows.len()),
            rows.len()
        )
    } else {
        "Keys (Esc closes)".to_string()
    };
    let blk = block(&title);
    let inner = blk.inner(r);
    f.render_widget(blk, r);
    let text_w = usize::from(inner.width);
    let lines: Vec<Line> = rows
        .iter()
        .skip(app.help_scroll)
        .take(visible)
        .map(|(k, t, screens)| {
            if k.is_empty() {
                Line::styled(t.clone(), HEADING)
            } else {
                let room = text_w.saturating_sub(KEY_W + 1);
                let suffix = if screens.is_empty() {
                    String::new()
                } else {
                    format!(" ({screens})")
                };
                let main = trunc(t, room.saturating_sub(suffix.chars().count()));
                Line::from(vec![
                    Span::styled(
                        format!("{:<KEY_W$} ", trunc(k, KEY_W)),
                        ACCENT.add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(main, Style::new()),
                    Span::styled(suffix, DIM),
                ])
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Key;

    #[test]
    fn every_binding_appears_with_its_keys() {
        let rows = entries(false);
        for b in BINDINGS {
            assert!(
                rows.iter()
                    .any(|(k, t, _)| t == b.title && *k == keys_label(b.keys)),
                "{} missing from help",
                b.title
            );
        }
        let n_headings = rows.iter().filter(|(k, _, _)| k.is_empty()).count();
        assert_eq!(rows.len(), BINDINGS.len() + n_headings);
    }

    /// On BCR and GP8 hardware a step is a CPU cycle of several
    /// micro-operations, not one instruction; nothing else changes.
    #[test]
    fn the_step_key_says_what_a_step_is_on_the_hardware() {
        use crate::actions::DEBUG_STEP_MICRO_OPS;
        let plain = entries(false);
        let micro = entries(true);
        assert!(plain
            .iter()
            .any(|(_, t, _)| t == "Step the watched organism one instruction"));
        assert!(micro.iter().any(|(_, t, _)| t == DEBUG_STEP_MICRO_OPS));
        assert!(!micro.iter().any(|(_, t, _)| t.contains("one instruction")));
        let differ = plain.iter().zip(&micro).filter(|(a, b)| a != b).count();
        assert_eq!(differ, 1);
        // Fits the overlay's text column next to its "(Debug)" note.
        assert!(DEBUG_STEP_MICRO_OPS.chars().count() <= 78 - 2 - KEY_W - 1 - " (Debug)".len());
    }

    #[test]
    fn key_labels_fit_their_column() {
        for (k, _, _) in entries(false) {
            assert!(k.chars().count() <= KEY_W, "{k}");
        }
        assert!(Key::Char('a').label().len() == 1);
    }
}
