//! The command palette overlay: a query line and the matching actions with
//! their key bindings.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use super::util::{accent, block, centered, trunc, DIM};
use crate::actions::PaletteState;
use crate::app::App;

pub fn draw(f: &mut Frame, app: &App, p: &PaletteState, area: Rect) {
    let hits = app.palette_hits(p);
    let w = area.width.saturating_sub(2).min(72);
    let h = area.height.saturating_sub(2).clamp(5, 18);
    let r = centered(area, w, h);
    f.render_widget(Clear, r);
    let blk = block("Commands (Enter runs, Esc closes)");
    let inner = blk.inner(r);
    f.render_widget(blk, r);
    if inner.height < 2 {
        return;
    }
    let prompt = Line::from(vec![
        Span::styled("> ", accent().add_modifier(Modifier::BOLD)),
        Span::raw(p.query.clone()),
        Span::styled("_", DIM),
    ]);
    f.render_widget(
        Paragraph::new(prompt),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let list = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);
    let rows = usize::from(list.height);
    let sel = p.selected.min(hits.len().saturating_sub(1));
    let top = (sel + 1).saturating_sub(rows);
    let width = usize::from(list.width);
    let mut lines: Vec<Line> = Vec::new();
    for (i, it) in hits.iter().enumerate().skip(top).take(rows) {
        let keys = trunc(&it.keys, 16);
        let title = format!("{}: {}", it.section, it.title);
        let room = width.saturating_sub(keys.chars().count() + 2);
        let title = trunc(&title, room);
        let pad = width.saturating_sub(title.chars().count() + keys.chars().count() + 1);
        let style = if i == sel {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        lines.push(Line::from(vec![
            Span::styled(title, style),
            Span::styled(" ".repeat(pad), style),
            Span::styled(keys, style.patch(DIM)),
            Span::styled(" ", style),
        ]));
    }
    if hits.is_empty() {
        lines.push(Line::styled("No action matches.", DIM));
    }
    f.render_widget(Paragraph::new(lines), list);
}
