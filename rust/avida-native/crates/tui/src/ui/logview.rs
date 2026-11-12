//! The log pane, fed by session events: set-up output, warnings, births of
//! the watched organism, notices and the per-update status lines.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::util::{block, clip_line, DIM};
use crate::app::App;
use crate::log::{LogKind, LogLine};

/// How a line of this kind is drawn. Colours are the terminal's own named
/// ones, and each kind also differs by attribute, so the log reads without
/// colour.
fn style(kind: LogKind) -> Style {
    match kind {
        LogKind::Info => Style::new().add_modifier(Modifier::BOLD),
        LogKind::Output => Style::new(),
        LogKind::Stderr => Style::new().fg(Color::Magenta),
        LogKind::Error => Style::new()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD),
        LogKind::Fatal => Style::new()
            .fg(Color::White)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD),
    }
}

/// The marker before a line, so kinds differ without colour too.
fn marker(kind: LogKind) -> &'static str {
    match kind {
        LogKind::Info => "* ",
        LogKind::Output => "  ",
        LogKind::Stderr => "! ",
        LogKind::Error | LogKind::Fatal => "!! ",
    }
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let h = usize::from(area.height.saturating_sub(2)).max(1);
    let w = usize::from(area.width.saturating_sub(2));
    let only = app.log_problems_only;
    let n = if only {
        app.log.iter().filter(|l| l.kind.is_problem()).count()
    } else {
        app.log.len()
    };
    app.log_view_h = h;
    app.log_shown = n;
    let max_start = n.saturating_sub(h);
    let start = if app.log_follow {
        max_start
    } else {
        app.log_scroll.min(max_start)
    };
    let pick = |l: &&LogLine| !only || l.kind.is_problem();
    let lines: Vec<Line> = app
        .log
        .iter()
        .filter(pick)
        .skip(start)
        .take(h)
        .map(|l| {
            let text = format!("{}{}", marker(l.kind), l.text);
            clip_line(Line::styled(text, style(l.kind)), w)
        })
        .collect();
    let problems = app.log.problems();
    let what = if only { "Warnings and errors" } else { "Log" };
    let title = if n == 0 {
        format!("{what} (empty)")
    } else {
        format!(
            "{what} {}-{} of {n}{}",
            start + 1,
            (start + h).min(n),
            if app.log_follow { ", following" } else { "" }
        )
    };
    let title = if problems > 0 && !only {
        format!("{title}, {problems} warning{}", plural(problems))
    } else {
        title
    };
    let body = if lines.is_empty() {
        let msg = if only {
            "No warnings or errors. Press v to show everything."
        } else {
            "Nothing logged yet."
        };
        Paragraph::new(msg).style(DIM)
    } else {
        Paragraph::new(lines)
    };
    f.render_widget(body.block(block(&title)), area);
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}
