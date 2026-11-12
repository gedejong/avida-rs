//! A stacked chart of how many cells each of the most abundant genotypes
//! held over time, with the genotypes of the map's colour slots listed
//! beside it.

use avida_viewmodel::text::genotype_char;
use avida_viewmodel::Palette;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::util::{block, clip_lines, trunc, DIM};
use super::waiting;
use crate::app::App;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let Some(frame) = app.frame.as_ref() else {
        waiting(f, area, " Waiting for the first frame...");
        return;
    };
    let Some(map) = frame.map.as_ref() else {
        waiting(f, area, " This frame has no map.");
        return;
    };
    let legend_w: u16 = if area.width >= 70 { 30 } else { 0 };
    let [chart, legend] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(legend_w)]).areas(area);
    let blk = block("Abundance over time (v: next view)");
    let inner = blk.inner(chart);
    f.render_widget(blk, chart);

    let hist = &app.abundance;
    let samples = hist.samples();
    let slots: Vec<i32> = map.top_genotypes.to_vec();
    let bands = hist.bands(&slots);
    let ylab_w: u16 = 6;
    if samples.len() < 2 || inner.width <= ylab_w + 2 || inner.height < 3 {
        f.render_widget(
            Paragraph::new("Collecting samples: run a few updates (. steps one).").style(DIM),
            inner,
        );
    } else {
        let plot = Rect::new(
            inner.x + ylab_w,
            inner.y,
            inner.width - ylab_w,
            inner.height - 1,
        );
        let max = f64::from(hist.max_total().max(1));
        let (pw, ph) = (usize::from(plot.width), usize::from(plot.height));
        let n = samples.len();
        let buf = f.buffer_mut();
        for col in 0..pw {
            let idx = if pw > 1 { col * (n - 1) / (pw - 1) } else { 0 };
            for r in 0..ph {
                let v = max * (ph - r) as f64 / ph as f64 - max / (2.0 * ph as f64);
                let Some(b) = bands.iter().find(|b| b.top[idx] > v) else {
                    continue;
                };
                let rgba = Palette::OkabeIto.category(b.slot);
                let color = app.color_mode.color([rgba[0], rgba[1], rgba[2]]);
                let (x, y) = (plot.x + col as u16, plot.y + r as u16);
                buf[(x, y)].set_symbol("█").set_fg(color);
            }
        }
        let top = format!("{:>w$}", hist.max_total(), w = usize::from(ylab_w) - 1);
        let bottom = format!("{:>w$}", 0, w = usize::from(ylab_w) - 1);
        f.render_widget(
            Paragraph::new(top).style(DIM),
            Rect::new(inner.x, plot.y, ylab_w, 1),
        );
        f.render_widget(
            Paragraph::new(bottom).style(DIM),
            Rect::new(inner.x, plot.y + plot.height - 1, ylab_w, 1),
        );
        let first = samples.first().map_or(0, |s| s.update);
        let last = samples.last().map_or(0, |s| s.update);
        let l = format!("u{first}");
        let r = format!("u{last}");
        let gap = usize::from(plot.width).saturating_sub(l.len() + r.len());
        f.render_widget(
            Paragraph::new(format!("{l}{}{r}", " ".repeat(gap))).style(DIM),
            Rect::new(plot.x, plot.y + plot.height, plot.width, 1),
        );
    }

    if legend_w > 0 {
        let latest = samples.last();
        let mut lines: Vec<Line> = Vec::new();
        let rows = usize::from(legend.height.saturating_sub(2));
        for (slot, &g) in slots.iter().enumerate() {
            if lines.len() + 2 > rows {
                break;
            }
            let rgba = Palette::OkabeIto.category(Some(slot));
            let color = app.color_mode.color([rgba[0], rgba[1], rgba[2]]);
            let name = frame
                .genotypes
                .iter()
                .find(|r| r.id == g)
                .map_or_else(|| format!("#{g}"), |r| r.name.clone());
            let count = latest.map_or(0, |s| s.count_of(g));
            lines.push(Line::from(vec![
                Span::styled(genotype_char(map, g).to_string(), Style::new().fg(color)),
                Span::raw(format!(" {:<13}{count:>6}", trunc(&name, 13))),
            ]));
        }
        if let Some(s) = latest {
            let listed: u32 = s.counts.iter().map(|&(_, n)| n).sum();
            let rgba = Palette::OkabeIto.category(None);
            let color = app.color_mode.color([rgba[0], rgba[1], rgba[2]]);
            lines.push(Line::from(vec![
                Span::styled("+", Style::new().fg(color)),
                Span::raw(format!(
                    " {:<13}{:>6}",
                    "other",
                    s.total.saturating_sub(listed)
                )),
            ]));
        }
        f.render_widget(
            Paragraph::new(clip_lines(
                lines,
                usize::from(legend.width.saturating_sub(2)),
            ))
            .block(block("Genotypes")),
            legend,
        );
    }
}
