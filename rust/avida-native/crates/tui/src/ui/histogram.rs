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

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
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
        let full = bar_w.min(usize::try_from(max).unwrap_or(1) * 8);
        for g in frame.genotypes.iter().take(rows) {
            let slot = map.genotype_slot(g.id);
            let col = Palette::OkabeIto.category(slot);
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
                if max == 1 { "" } else { "s" }
            ),
            DIM,
        ));
        f.render_widget(
            Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
                .block(block("Genotype abundance")),
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
    let bar_w = inner_w.saturating_sub(label_w + 9).max(4);
    let log_note = if h.scale == avida_viewmodel::Scale::Log {
        ", log bins"
    } else {
        ""
    };
    for (i, &c) in h.counts.iter().enumerate() {
        let lo = edges.get(i).copied().unwrap_or(h.min);
        let hi = edges.get(i + 1).copied().unwrap_or(h.max);
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
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
            .block(block(&format!("Histogram: {title}"))),
        area,
    );
}
