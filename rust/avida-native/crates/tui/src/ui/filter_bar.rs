//! The one-line filter bar above the map and the charts: the conditions as
//! short labels and how many organisms match.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::util::{heading, trunc, DIM};
use crate::app::App;
use crate::filtering::{condition_label, summary_text};

/// Marks the labels of conditions the frame has no values for.
const NO_DATA: &str = " (no data)";

/// The bar's spans for `width` columns, or nothing while the filter is
/// empty. The count keeps its place at the right; the conditions give way
/// to it, then the word "organisms" does.
pub fn bar_spans(app: &App, width: usize) -> Option<Vec<Span<'static>>> {
    if !app.filtering.is_active() {
        return None;
    }
    let frame = app.frame.as_deref();
    let sel = app.filtering.selection();
    let labels: Vec<String> = app
        .filtering
        .filter
        .conditions()
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut s = condition_label(c, frame);
            if sel.is_some_and(|s| s.ignored.contains(&i)) {
                s.push_str(NO_DATA);
            }
            s
        })
        .collect();
    let prefix = " Filter: ";
    let used = prefix.chars().count();
    let nothing = sel.is_some_and(|s| s.matched == 0 && s.occupied > 0);

    let mut summary = sel.map_or_else(
        || "waiting for values".to_string(),
        |s| summary_text(s, false),
    );
    let room = |summary: &str| width.saturating_sub(used + summary.chars().count() + 2);
    // The longer count first; the shorter one when the conditions would
    // have under twenty columns.
    if room(&summary) < 20 {
        if let Some(s) = sel {
            summary = summary_text(s, true);
        }
    }
    let avail = room(&summary);
    let joined = labels.join(" · ");
    let text = if joined.chars().count() <= avail {
        joined
    } else if avail >= 12 {
        trunc(&joined, avail)
    } else {
        let n = labels.len();
        trunc(
            &format!("{n} condition{}", if n == 1 { "" } else { "s" }),
            avail,
        )
    };
    let gap = width.saturating_sub(used + text.chars().count() + summary.chars().count() + 1);
    let summary_style = if nothing {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        DIM
    };
    Some(vec![
        Span::styled(prefix, heading()),
        Span::raw(text),
        Span::raw(" ".repeat(gap)),
        Span::styled(summary, summary_style),
    ])
}

/// Draws the bar on the first row of `area` while a filter is set and
/// returns what is left for the screen.
pub fn take(f: &mut Frame, app: &App, area: Rect) -> Rect {
    if area.height < 4 {
        return area;
    }
    let Some(spans) = bar_spans(app, usize::from(area.width)) else {
        return area;
    };
    let [bar, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    f.render_widget(Paragraph::new(Line::from(spans)), bar);
    rest
}
