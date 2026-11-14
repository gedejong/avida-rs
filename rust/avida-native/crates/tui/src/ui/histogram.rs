//! The histogram screen: genotype abundance (as the ncurses viewer's
//! `#` chart) or the distribution of the selected layer's values.

use avida_session::LayerId;
use avida_viewmodel::text::genotype_char;
use avida_viewmodel::{format_number, Histogram, Palette};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::util::{block, clip_lines, thousands, trunc, DIM};
use super::waiting;
use crate::app::{layer_title, App};
use crate::color::{hue, Hue};
use crate::metrics::hbar;

/// Bins that fit a body of `height` rows.
pub fn histogram_bins(height: u16) -> usize {
    usize::from(height.saturating_sub(5)).clamp(4, 30)
}

/// A bar for `total` of `max` with `sub` of it matching: the matching
/// part solid in the filter's hue, the rest shaded in `rest`'s colour. The
/// two differ in glyph, so they stay apart without colour.
fn overlay(total: f64, sub: f64, max: f64, width: usize, rest: Style) -> [Span<'static>; 2] {
    let max = max.max(1.0);
    let solid = hbar(sub / max, width);
    let cells = ((total / max) * width as f64).ceil() as usize;
    let shaded = "░".repeat(cells.min(width).saturating_sub(solid.chars().count()));
    [
        Span::styled(
            if solid.is_empty() && sub > 0.0 {
                "▏".to_string()
            } else {
                solid
            },
            Style::new().fg(hue(Hue::Magenta)),
        ),
        Span::styled(shaded, rest),
    ]
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let area = super::filter_bar::take(f, app, area);
    let Some(frame) = app.frame.as_ref() else {
        waiting(f, area, super::FIRST_FRAME);
        return;
    };
    let Some(map) = frame.map.as_ref() else {
        waiting(f, area, super::FIRST_FRAME);
        return;
    };
    let title = layer_title(app.env(), app.layer);
    let inner_w = usize::from(area.width.saturating_sub(2));
    let mut lines: Vec<Line> = Vec::new();

    if app.layer == LayerId::Genotype {
        let rows = usize::from(area.height.saturating_sub(4));
        let max = frame.genotypes.first().map_or(1, |g| g.abundance).max(1);
        let label_w = 22usize;
        let bar_w = inner_w.saturating_sub(label_w + 8).max(4);
        // Bars are to scale; a small population does not stretch across a
        // wide terminal, eight cells per organism at most.
        let sel = app.filtering.selection();
        let bar_w = if sel.is_some() {
            bar_w.saturating_sub(8).max(4)
        } else {
            bar_w
        };
        let full = bar_w.min(usize::try_from(max).unwrap_or(1) * 8);
        let mut in_subset: std::collections::HashMap<i32, u32> = std::collections::HashMap::new();
        if let Some(s) = sel {
            for (&g, _) in map.genotype_id.iter().zip(&s.mask).filter(|(_, &m)| m) {
                *in_subset.entry(g).or_insert(0) += 1;
            }
        }
        for g in frame.genotypes.iter().take(rows) {
            let slot = map.genotype_slot(g.id);
            let col = Palette::OkabeIto.category(slot);
            let own = Style::new().fg(app.color_mode.color([col[0], col[1], col[2]]));
            if sel.is_some() {
                let n = in_subset.get(&g.id).copied().unwrap_or(0);
                let [solid, shaded] = overlay(
                    f64::from(g.abundance),
                    f64::from(n),
                    f64::from(max),
                    full,
                    own,
                );
                lines.push(Line::from(vec![
                    Span::styled(
                        genotype_char(map, g.id).to_string(),
                        Style::new()
                            .fg(app.color_mode.ink_on([col[0], col[1], col[2]]))
                            .bg(app.color_mode.color([col[0], col[1], col[2]])),
                    ),
                    Span::raw(" "),
                    Span::raw(format!(
                        "{:<w$}",
                        trunc(&g.name, label_w - 2),
                        w = label_w - 2
                    )),
                    solid,
                    shaded,
                    Span::raw(format!(" {} of {}", thousands(i64::from(n)), g.abundance)),
                ]));
                continue;
            }
            let bar = hbar(f64::from(g.abundance) / f64::from(max), full);
            lines.push(Line::from(vec![
                Span::styled(
                    genotype_char(map, g.id).to_string(),
                    Style::new()
                        .fg(app.color_mode.ink_on([col[0], col[1], col[2]]))
                        .bg(app.color_mode.color([col[0], col[1], col[2]])),
                ),
                Span::raw(" "),
                Span::raw(format!(
                    "{:<w$}",
                    trunc(&g.name, label_w - 2),
                    w = label_w - 2
                )),
                Span::styled(
                    if bar.is_empty() {
                        "▏".to_string()
                    } else {
                        bar
                    },
                    Style::new().fg(app.color_mode.color([col[0], col[1], col[2]])),
                ),
                Span::raw(format!(" {}", thousands(i64::from(g.abundance)))),
            ]));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!(
                "Bars are drawn to scale. The longest stands for {} organism{}.",
                thousands(i64::from(max)),
                if max == 1 { "" } else { "s" },
            ),
            DIM,
        ));
        if sel.is_some() {
            lines.push(Line::styled("█ matching the filter, ░ the others.", DIM));
        }
        f.render_widget(
            Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
                .block(block("Genotype abundance")),
            area,
        );
        return;
    }

    if app.layer == LayerId::Ancestry {
        let max = map.families.first().map_or(1, |f| f.organisms).max(1);
        let label_w = 22usize;
        let bar_w = inner_w.saturating_sub(label_w + 8).max(4);
        let full = bar_w.min(usize::try_from(max).unwrap_or(1) * 8);
        for fam in map.families.iter() {
            let col = Palette::OkabeIto.category(Some(fam.slot));
            let bar = hbar(f64::from(fam.organisms) / f64::from(max), full);
            lines.push(Line::from(vec![
                Span::raw(format!(
                    "{:<w$}",
                    trunc(&format!("family #{}", fam.root), label_w - 2),
                    w = label_w
                )),
                Span::styled(
                    if bar.is_empty() {
                        "▏".to_string()
                    } else {
                        bar
                    },
                    Style::new().fg(app.color_mode.color([col[0], col[1], col[2]])),
                ),
                Span::raw(format!(" {}", thousands(i64::from(fam.organisms)))),
            ]));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "A family is named after the genotype at its root.",
            DIM,
        ));
        f.render_widget(
            Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
                .block(block("Family sizes")),
            area,
        );
        return;
    }

    let Some(values) = map.layer(app.layer) else {
        waiting(f, area, " Waiting for the layer's values...");
        return;
    };
    let scale = if App::scale_applies(app.layer) {
        app.scale()
    } else {
        avida_viewmodel::Scale::Linear
    };
    let bins = histogram_bins(area.height);
    let h = Histogram::new(values, bins, scale);
    let edges = h.bin_edges();
    let max = h.max_count().max(1);
    let label_w = 21usize;
    let sel = app.filtering.selection();
    let subset = sel.map(|s| h.counts_where(values, &s.mask));
    let extra = if subset.is_some() { 9 } else { 0 };
    let bar_w = inner_w.saturating_sub(label_w + 9 + extra).max(4);
    let log_note = if h.scale == avida_viewmodel::Scale::Log {
        ", log bins"
    } else {
        ""
    };
    for (i, &c) in h.counts.iter().enumerate() {
        let lo = edges.get(i).copied().unwrap_or(h.min);
        let hi = edges.get(i + 1).copied().unwrap_or(h.max);
        if let Some(sub) = &subset {
            let n = sub.get(i).copied().unwrap_or(0);
            let [solid, shaded] = overlay(
                f64::from(c),
                f64::from(n),
                f64::from(max),
                bar_w,
                Style::new().fg(hue(Hue::Cyan)),
            );
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:>9} .. {:<9}", format_number(lo), format_number(hi)),
                    DIM,
                ),
                solid,
                shaded,
                Span::raw(format!(
                    " {} of {}",
                    thousands(i64::from(n)),
                    thousands(i64::from(c))
                )),
            ]));
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:>9} .. {:<9}", format_number(lo), format_number(hi)),
                DIM,
            ),
            Span::styled(
                hbar(f64::from(c) / f64::from(max), bar_w),
                Style::new().fg(hue(Hue::Cyan)),
            ),
            Span::raw(format!(" {}", thousands(i64::from(c)))),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            "{} organisms counted{}{}",
            thousands(h.total() as i64),
            if h.skipped > 0 {
                format!(", {} without a value", h.skipped)
            } else {
                String::new()
            },
            log_note
        ),
        DIM,
    ));
    if subset.is_some() {
        lines.push(Line::styled("█ matching the filter, ░ the others.", DIM));
    }
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
            .block(block(&format!("Histogram: {title}"))),
        area,
    );
}
