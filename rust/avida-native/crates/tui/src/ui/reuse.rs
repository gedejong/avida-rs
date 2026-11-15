//! The knockout map in the terminal (`v` on the debugger screen): the
//! measures, the genome as one class letter per site, and a listing with a
//! row per site and a column per task the organism performs. Definitions
//! are in `avida_world::knockout`.

use avida_viewmodel::{reuse_summary, KnockoutMap, SiteClass};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::util::{block, clip_line, trunc, DIM};
use crate::app::App;
use crate::color::{hue, Hue};

/// The colour of a class letter.
fn class_style(c: SiteClass) -> Style {
    match c {
        SiteClass::Lethal => Style::new().fg(hue(Hue::Red)),
        SiteClass::Shared => Style::new().fg(hue(Hue::Cyan)).add_modifier(Modifier::BOLD),
        SiteClass::Task => Style::new().fg(hue(Hue::Green)),
        SiteClass::Speed => Style::new().fg(hue(Hue::Yellow)),
        SiteClass::Neutral => DIM,
    }
}

/// The names of the performed tasks as column heads, `width` wide each.
fn heads(map: &KnockoutMap, width: usize) -> Vec<String> {
    map.performed
        .iter()
        .map(|&k| {
            let n = map.tasks.get(k).map_or("?", String::as_str).to_uppercase();
            format!("{:<width$}", trunc(&n, width))
        })
        .collect()
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(org_id) = app.debugger.org_id else {
        return;
    };
    let w = usize::from(area.width.saturating_sub(2));
    let title = format!("Reuse of organism #{org_id} (v: memory, Up/Down: sites)");
    let blk = block(&title);
    let inner = blk.inner(area);
    f.render_widget(blk, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let one = |text: String, style: Style| {
        Paragraph::new(Line::styled(trunc(&text, w), style)).wrap(Wrap { trim: false })
    };
    let reuse = &app.debugger.reuse;
    if reuse.pending == Some(org_id) {
        f.render_widget(
            one(" Testing every site of the genome...".into(), DIM),
            inner,
        );
        return;
    }
    let Some(reply) = reuse.answer_for(org_id) else {
        f.render_widget(one(" No map yet for this organism.".into(), DIM), inner);
        return;
    };
    let map = match &reply.result {
        Ok(m) => m,
        Err(e) => {
            f.render_widget(
                Paragraph::new(format!(" {e}")).wrap(Wrap { trim: true }),
                inner,
            );
            return;
        }
    };
    if !map.base_viable {
        f.render_widget(
            one(
                " The genome does not replicate in the test CPU, so there is no map.".into(),
                DIM,
            ),
            inner,
        );
        return;
    }
    let m = map.measures();
    let len = map.sites.len();
    let sel = reuse.sel.min(len.saturating_sub(1));

    // Header: measures, per-task counts, the class legend.
    let mut lines: Vec<Line> = Vec::new();
    // The measures without the call indicators (the next line has them).
    let summary = reuse_summary(&m);
    let measures = summary.split(", ").take(3).collect::<Vec<_>>().join(", ");
    lines.push(Line::raw(format!(
        " {measures} - {} test runs, {} ms",
        len + 1,
        reply.elapsed.as_millis()
    )));
    let per_task = m
        .sites_per_task
        .iter()
        .map(|(k, n)| format!("{} {n}", map.tasks.get(*k).map_or("?", String::as_str)))
        .collect::<Vec<_>>()
        .join(", ");
    let calls = summary.split(", ").skip(3).collect::<Vec<_>>().join(", ");
    if !per_task.is_empty() {
        lines.push(Line::styled(format!(" sites per task: {per_task}"), DIM));
    }
    if !calls.is_empty() {
        lines.push(Line::styled(format!(" {calls}"), DIM));
    }
    let mut legend = vec![Span::raw(" ")];
    for (i, c) in SiteClass::ALL.iter().enumerate() {
        let name = match c {
            SiteClass::Lethal => "lethal",
            SiteClass::Shared => "shared",
            SiteClass::Task => "one task",
            SiteClass::Speed => "speed",
            SiteClass::Neutral => "neutral",
        };
        legend.push(Span::styled(c.letter().to_string(), class_style(*c)));
        legend.push(Span::raw(format!(" {name} {}  ", m.class_counts[i])));
    }
    lines.push(clip_line(Line::from(legend), w));

    // The genome as class letters, wrapped, at most three rows.
    let per_row = w.saturating_sub(1).max(1);
    let strip_rows = len.div_ceil(per_row).min(3);
    for r in 0..strip_rows {
        let from = r * per_row;
        let to = (from + per_row).min(len);
        let mut spans = vec![Span::raw(" ")];
        for i in from..to {
            let s = if i == sel {
                class_style(map.sites[i].class).add_modifier(Modifier::REVERSED)
            } else {
                class_style(map.sites[i].class)
            };
            spans.push(Span::styled(map.sites[i].class.letter().to_string(), s));
        }
        lines.push(Line::from(spans));
    }
    // The selected site in words.
    let detail = map.site_text(sel);
    lines.push(Line::styled(
        format!(" {}", trunc(&detail, w.saturating_sub(1))),
        Style::new(),
    ));

    let header_h = lines.len() as u16;
    let list_h = usize::from(inner.height.saturating_sub(header_h + 1));
    let head_area = Rect {
        height: header_h.min(inner.height),
        ..inner
    };
    f.render_widget(Paragraph::new(lines), head_area);
    if list_h == 0 {
        return;
    }

    // The listing: position, class, instruction, fitness, a column per task.
    let col = 4usize;
    let inst_w = map
        .inst_names
        .iter()
        .map(|n| n.chars().count())
        .max()
        .unwrap_or(4)
        .clamp(4, 14);
    let fixed = 1 + 4 + 1 + 1 + 1 + inst_w + 1 + 5 + 1;
    let room_cols = w.saturating_sub(fixed) / (col + 1);
    let shown_tasks = map.performed.len().min(room_cols.max(1));
    let head_names: Vec<String> = heads(map, col).into_iter().take(shown_tasks).collect();
    let mut head = format!(" {:>4} {} {:<inst_w$} {:>5} ", "site", "c", "inst", "fit");
    for n in &head_names {
        head.push_str(n);
        head.push(' ');
    }
    let rows_y = inner.y + header_h;
    f.render_widget(
        Paragraph::new(Line::styled(trunc(&head, w), DIM)),
        Rect {
            y: rows_y,
            height: 1,
            ..inner
        },
    );
    // Keep the selection in view.
    let top = {
        let t = app.debugger.reuse.top;
        let t = if sel < t {
            sel
        } else if sel >= t + list_h {
            sel + 1 - list_h
        } else {
            t
        };
        t.min(len.saturating_sub(list_h))
    };
    let sets: Vec<Vec<usize>> = map
        .performed
        .iter()
        .take(shown_tasks)
        .map(|&k| map.task_sites(k))
        .collect();
    let mut rows: Vec<Line> = Vec::new();
    for i in top..(top + list_h).min(len) {
        let s = &map.sites[i];
        let marker = if i == sel { ">" } else { " " };
        let mut spans = vec![
            Span::raw(format!("{marker}{i:>4} ")),
            Span::styled(s.class.letter().to_string(), class_style(s.class)),
            Span::raw(format!(
                " {:<inst_w$} {:>4}% ",
                trunc(&map.inst_names[i], inst_w),
                (s.rel_fitness * 100.0).round() as i64
            )),
        ];
        for set in &sets {
            let on = set.contains(&i);
            spans.push(Span::styled(
                format!("{:<col$} ", if on { "#" } else { "." }),
                if on {
                    Style::new().add_modifier(Modifier::BOLD)
                } else {
                    DIM
                },
            ));
        }
        let mut line = clip_line(Line::from(spans), w);
        if i == sel {
            line = line.style(Style::new().add_modifier(Modifier::REVERSED));
        }
        rows.push(line);
    }
    app.debugger.reuse.top = top;
    f.render_widget(
        Paragraph::new(rows),
        Rect {
            y: rows_y + 1,
            height: inner.height.saturating_sub(header_h + 1),
            ..inner
        },
    );
}
