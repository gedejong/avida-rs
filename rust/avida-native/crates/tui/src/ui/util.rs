//! Small drawing helpers shared by the screens.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::color::{hue, Hue};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};

/// Secondary text. The terminal's own dim attribute keeps it readable on
/// light and dark backgrounds alike (a fixed grey is invisible on some
/// colour schemes).
pub const DIM: Style = Style::new().add_modifier(Modifier::DIM);
pub const BOLD: Style = Style::new().add_modifier(Modifier::BOLD);
/// Accent colour (keys, prompts): cyan, darkened on light backgrounds.
pub fn accent() -> Style {
    Style::new().fg(hue(Hue::Cyan))
}
/// Headings and changed values: bold magenta.
pub fn heading() -> Style {
    Style::new()
        .fg(hue(Hue::Magenta))
        .add_modifier(Modifier::BOLD)
}

/// A rounded bordered block with a title.
pub fn block(title: &str) -> Block<'_> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(DIM)
        .title(Span::styled(format!(" {title} "), BOLD))
}

/// `s` cut to `w` characters, ending in an ellipsis when cut.
pub fn trunc(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n <= w {
        s.to_string()
    } else if w == 0 {
        String::new()
    } else {
        let mut out: String = s.chars().take(w - 1).collect();
        out.push('…');
        out
    }
}

/// `line` cut to `w` columns, ending in an ellipsis when something was cut.
/// The styles of the spans that survive are kept.
pub fn clip_line(line: Line<'_>, w: usize) -> Line<'static> {
    let total: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    let keep = if total <= w { w } else { w.saturating_sub(1) };
    let mut left = keep;
    let mut spans: Vec<Span<'static>> = Vec::new();
    for s in line.spans {
        if left == 0 {
            break;
        }
        let n = s.content.chars().count();
        if n <= left {
            left -= n;
            spans.push(Span::styled(s.content.into_owned(), s.style));
        } else {
            let cut: String = s.content.chars().take(left).collect();
            spans.push(Span::styled(cut, s.style));
            left = 0;
        }
    }
    if total > w && w > 0 {
        let style = spans.last().map_or(Style::new(), |s| s.style);
        spans.push(Span::styled("…", style));
    }
    Line {
        spans,
        style: line.style,
        alignment: line.alignment,
    }
}

/// Every line of `lines` cut to `w` columns with an ellipsis ([`clip_line`]).
pub fn clip_lines(lines: Vec<Line<'_>>, w: usize) -> Vec<Line<'static>> {
    lines.into_iter().map(|l| clip_line(l, w)).collect()
}

/// `label` padded to `w` then `value`.
pub fn kv<'a>(label: &str, w: usize, value: impl Into<Span<'a>>) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label:<w$}"), DIM),
        value.into(),
    ])
}

/// The rectangle of `w` x `h` centred in `area` (shrunk to fit).
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// When a genotype or organism was born: `u41`, or `start` for what the run
/// began with (Avida records those as born at update -1).
pub fn born_at(update: i32) -> String {
    if update < 0 {
        "start".to_string()
    } else {
        format!("u{update}")
    }
}

/// An integer with thousands separators.
pub fn thousands(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_and_separators() {
        assert_eq!(trunc("abcdef", 4), "abc…");
        assert_eq!(trunc("abc", 4), "abc");
        assert_eq!(trunc("abc", 0), "");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(thousands(12), "12");
        assert_eq!(thousands(-1000), "-1,000");
    }

    #[test]
    fn clipping_keeps_styles_and_ends_in_an_ellipsis() {
        let line = Line::from(vec![Span::styled("abc", BOLD), Span::raw("defgh")]);
        let text = |l: &Line| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        };
        let cut = clip_line(line.clone(), 6);
        assert_eq!(text(&cut), "abcde…");
        assert_eq!(cut.spans[0].style, BOLD);
        assert_eq!(text(&clip_line(line.clone(), 8)), "abcdefgh");
        assert_eq!(text(&clip_line(line.clone(), 2)), "a…");
        assert_eq!(text(&clip_line(line, 0)), "");
    }

    #[test]
    fn centring_stays_inside() {
        let r = centered(Rect::new(0, 0, 80, 24), 100, 10);
        assert_eq!((r.x, r.width), (0, 80));
        assert_eq!((r.y, r.height), (7, 10));
    }
}
