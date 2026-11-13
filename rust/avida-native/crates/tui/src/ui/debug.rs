//! The organism debugger: registers, heads and stacks of the watched
//! organism, a scrolling memory listing with the instruction pointer and
//! breakpoints marked, and what the last step did.

use avida_viewmodel::{OrganismView, ThreadView};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::util::{accent, block, clip_line, clip_lines, trunc, BOLD, DIM};
use super::zoom::{flag_char, nums};
use crate::app::App;
use crate::color::{hue, Hue};
use crate::debugger::{Debugger, Fate};

/// A value that changed since the last step.
fn changed_style() -> Style {
    Style::new()
        .fg(hue(Hue::Magenta))
        .add_modifier(Modifier::BOLD)
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    if app.frame.is_none() {
        f.render_widget(Paragraph::new(super::FIRST_FRAME).style(DIM), area);
        return;
    }
    if app.debugger.current.is_none() {
        let text = match (app.selection, app.world_size()) {
            (Some(c), Some((w, _))) => format!(
                " Cell ({},{}) is empty. Press Esc, move the cursor to an organism and press Enter (or C for the next one).",
                c as usize % w,
                c as usize / w
            ),
            _ => " No organism watched. Press Enter on an occupied cell of the map, or C for the next organism."
                .to_string(),
        };
        f.render_widget(
            Paragraph::new(text)
                .style(DIM)
                .wrap(Wrap { trim: true })
                .block(block("Debugger")),
            area,
        );
        return;
    }
    // On a very short terminal the status lines give way to the listing.
    let status_h = if area.height < 10 { 0 } else { 2 };
    let [main, status] =
        Layout::vertical([Constraint::Min(4), Constraint::Length(status_h)]).areas(area);
    let wide = main.width >= 72;
    let (cpu_area, mem_area) = if wide {
        // Wider terminals give the registers more room, so the stacks are
        // listed in full instead of cut at the edge.
        let left_w = if main.width >= 140 { 52 } else { 34 };
        let [l, r] =
            Layout::horizontal([Constraint::Length(left_w), Constraint::Min(30)]).areas(main);
        (l, r)
    } else {
        let h = if main.height >= 12 {
            6
        } else if main.height >= 8 {
            5
        } else {
            3
        };
        let [t, m] = Layout::vertical([Constraint::Length(h), Constraint::Min(3)]).areas(main);
        (t, m)
    };
    let d = &app.debugger;
    if let Some(org) = d.current.as_ref() {
        draw_cpu(f, d, org, cpu_area, wide);
    }
    if status_h > 0 {
        draw_status(f, app, status);
    }
    draw_memory(f, app, mem_area);
}

fn mark(changed: bool) -> &'static str {
    if changed {
        "*"
    } else {
        " "
    }
}

/// A head name for a narrow column: `IP`, `Rd`, `Wr`, `Fl`, `Fl2`, ...
fn short_head(name: &str) -> String {
    match name {
        "Read" => "Rd".to_string(),
        "Write" => "Wr".to_string(),
        n => n.replacen("Flow", "Fl", 1),
    }
}

fn head_lines(d: &Debugger, t: &ThreadView) -> Vec<Line<'static>> {
    let names: Vec<String> = d
        .current
        .as_ref()
        .map(|o| o.cpu.head_names.iter().map(|n| short_head(n)).collect())
        .unwrap_or_default();
    let name_w = names.iter().map(|n| n.chars().count()).max().unwrap_or(2);
    let moved = d.moved_heads();
    let items: Vec<Span<'static>> = t
        .heads
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let n = names.get(i).map_or("Hd", String::as_str);
            let pos = if h.mem_space == 0 {
                format!("{}", h.pos)
            } else {
                format!("{}:{}", h.mem_space, h.pos)
            };
            let style = if moved.get(i).copied().unwrap_or(false) {
                changed_style()
            } else {
                Style::new()
            };
            Span::styled(format!("{n:<name_w$} {pos:<6}{}", mark(false)), style)
        })
        .collect();
    items.chunks(2).map(|c| Line::from(c.to_vec())).collect()
}

fn draw_cpu(f: &mut Frame, d: &Debugger, o: &OrganismView, area: Rect, wide: bool) {
    let (cur, _) = d.threads();
    let tid = d.thread_index();
    if area.height <= 3 {
        // One line: registers and the instruction pointer.
        let changed = d.changed_registers();
        let mut spans: Vec<Span> = Vec::new();
        if let Some(t) = cur {
            for (i, r) in t.regs.iter().enumerate() {
                let name = char::from(b'A' + (i % 26) as u8);
                let ch = changed.get(i).copied().unwrap_or(false);
                spans.push(Span::styled(format!("{name}X "), DIM));
                spans.push(Span::styled(
                    format!("{r}{} ", mark(ch)),
                    if ch { changed_style() } else { BOLD },
                ));
            }
            if let Some(h) = t.heads.first() {
                spans.push(Span::styled(format!("IP {}", h.pos), BOLD));
            }
        }
        let title = format!("Org #{} (t thread)", o.org_id);
        f.render_widget(
            Paragraph::new(clip_line(
                Line::from(spans),
                usize::from(area.width.saturating_sub(2)),
            ))
            .block(block(&title)),
            area,
        );
        return;
    }
    let mut lines: Vec<Line> = Vec::new();
    let w = usize::from(area.width.saturating_sub(2));
    // About three columns for each value fit next to "stk A* ".
    let stack_items = (w.saturating_sub(8) / 3).max(3);
    let alive = match &d.fate {
        Fate::Alive => String::new(),
        Fate::Died { update } => format!("  DIED u{update}"),
        Fate::Replaced { by, update } => format!("  REPLACED by #{by} u{update}"),
    };
    if wide {
        lines.push(Line::from(vec![
            Span::styled(
                if o.genotype_name.is_empty() {
                    "-".to_string()
                } else {
                    o.genotype_name.clone()
                },
                BOLD.fg(hue(Hue::Magenta)),
            ),
            Span::styled(format!(" #{}", o.org_id), DIM),
            Span::styled(alive, Style::new().fg(hue(Hue::Red))),
        ]));
        lines.push(Line::styled(
            format!("age {}  gen {}", o.phenotype.age, o.phenotype.generation),
            DIM,
        ));
        lines.push(Line::styled(
            format!(
                "cycles {} ({} lifetime)",
                o.phenotype.cpu_cycles_used, o.phenotype.lifetime_cpu_cycles
            ),
            DIM,
        ));
    }
    match cur {
        None => lines.push(Line::styled("no thread state", DIM)),
        Some(t) => {
            let changed = d.changed_registers();
            if t.regs.is_empty() {
                lines.push(Line::styled("registers not exposed", DIM));
            } else if wide && t.regs.len() <= 6 {
                for (i, r) in t.regs.iter().enumerate() {
                    let name = char::from(b'A' + (i % 26) as u8);
                    let ch = changed.get(i).copied().unwrap_or(false);
                    lines.push(Line::from(vec![
                        Span::styled(format!("{name}X "), DIM),
                        Span::styled(format!("{r:>12}"), if ch { changed_style() } else { BOLD }),
                        Span::styled(mark(ch), changed_style()),
                    ]));
                }
            } else {
                // Four registers to a line when the hardware has many (BCR
                // has twelve), so none is clipped.
                let per_line = if wide && t.regs.len() > 6 {
                    4
                } else {
                    t.regs.len()
                };
                for (c, chunk) in t.regs.chunks(per_line.max(1)).enumerate() {
                    let mut spans = Vec::new();
                    for (j, r) in chunk.iter().enumerate() {
                        let i = c * per_line + j;
                        let name = char::from(b'A' + (i % 26) as u8);
                        let ch = changed.get(i).copied().unwrap_or(false);
                        spans.push(Span::styled(format!("{name}X "), DIM));
                        spans.push(Span::styled(
                            format!("{r}{} ", mark(ch)),
                            if ch { changed_style() } else { BOLD },
                        ));
                    }
                    lines.push(Line::from(spans));
                }
            }
            if wide {
                lines.extend(head_lines(d, t));
            } else {
                let heads: Vec<String> = t
                    .heads
                    .iter()
                    .zip(&o.cpu.head_names)
                    .take(4)
                    .map(|(h, n)| format!("{} {}", short_head(n), h.pos))
                    .collect();
                lines.push(Line::raw(heads.join("  ")));
            }
            if wide && !t.status.is_empty() {
                // One part to a line: the column is narrow.
                for (i, part) in t.status.split(", ").enumerate() {
                    lines.push(Line::raw(if i == 0 {
                        format!("state {part}")
                    } else {
                        format!("      {part}")
                    }));
                }
            }
            if !t.read_label.is_empty() || !t.next_label.is_empty() {
                lines.push(Line::raw(format!(
                    "labels {} / {}",
                    if t.read_label.is_empty() {
                        "-"
                    } else {
                        &t.read_label
                    },
                    if t.next_label.is_empty() {
                        "-"
                    } else {
                        &t.next_label
                    }
                )));
            }
            let mut order: Vec<usize> = (0..t.stacks.len()).collect();
            order.sort_by_key(|&i| i32::from(i as i32 != t.active_stack));
            let max = if wide { 2 } else { 1 };
            for &si in order.iter().take(max) {
                let name = char::from(b'A' + (si % 26) as u8);
                let act = if si as i32 == t.active_stack {
                    "*"
                } else {
                    " "
                };
                lines.push(Line::raw(format!(
                    "stk {name}{act} {}",
                    nums(&t.stacks[si], stack_items)
                )));
            }
        }
    }
    if wide {
        lines.push(Line::raw(format!(
            "in  {}",
            nums(&o.input_buf, stack_items)
        )));
        lines.push(Line::raw(format!(
            "out {}",
            nums(&o.output_buf, stack_items)
        )));
    }
    let title = format!(
        "{} thread {}/{} (t)",
        trunc(&format!("Org #{}", o.org_id), w.saturating_sub(18)),
        tid + 1,
        o.cpu.threads.len().max(1)
    );
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
            .block(block(&title)),
        area,
    );
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let d = &app.debugger;
    let w = usize::from(area.width);
    let bps: Vec<String> = d
        .breakpoints
        .iter()
        .map(|&(s, p)| {
            if s == 0 {
                p.to_string()
            } else {
                format!("{s}:{p}")
            }
        })
        .collect();
    let line1 = format!(
        " breakpoints: {}   follow offspring: {}   follow IP: {}",
        if bps.is_empty() {
            "none".to_string()
        } else {
            bps.join(" ")
        },
        if d.follow_offspring { "on" } else { "off" },
        if d.follow_ip { "on" } else { "off" },
    );
    let note = match (&d.fate, &d.last_stop) {
        (Fate::Died { update }, _) => {
            format!(" The organism died at update {update}. Select another one on the map.")
        }
        (Fate::Replaced { by, update }, _) => {
            format!(" Organism #{by} took over the cell at update {update}.")
        }
        (_, Some(n)) => {
            let cycles = d
                .cycles_since_previous()
                .filter(|&c| c > 0)
                .map(|c| format!(" (+{c} cycles)"))
                .unwrap_or_default();
            let mid = if app.status.as_ref().is_some_and(|s| s.mid_slice) {
                ", inside a time slice"
            } else {
                ""
            };
            format!(" {}{cycles}{mid}", n.text())
        }
        _ => match &d.current {
            Some(o) => format!(
                " Watching organism #{} in cell ({},{}). {}",
                o.org_id,
                o.x,
                o.y,
                if o.cpu.cycle_runs_micro_ops() {
                    "A step runs one CPU cycle, which can move the IP more than one position."
                } else {
                    "A step runs one instruction."
                }
            ),
            None => String::new(),
        },
    };
    let style = if d.last_stop.as_ref().is_some_and(|n| n.is_problem()) || d.fate != Fate::Alive {
        Style::new().fg(hue(Hue::Red))
    } else {
        accent()
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(trunc(&line1, w), DIM),
            Line::styled(trunc(&note, w), style),
        ]),
        area,
    );
}

fn draw_memory(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = usize::from(area.height.saturating_sub(2)).max(1);
    let inner_w = usize::from(area.width.saturating_sub(2));
    let d = &mut app.debugger;
    let Some(org) = d.current.clone() else {
        return;
    };
    let space_id = d.space;
    let Some(space) = org.cpu.space(space_id).or(org.cpu.spaces.first()) else {
        return;
    };
    let sites = space.memory.len();
    d.sel = d.sel.min(sites.saturating_sub(1));
    if d.sel < d.top + 1 {
        d.top = d.sel.saturating_sub(1);
    }
    if d.sel + 2 > d.top + rows {
        d.top = d.sel + 2 - rows;
    }
    d.top = d.top.min(sites.saturating_sub(rows));
    let (top, sel) = (d.top, d.sel);
    let d = &app.debugger;
    let (cur, _) = d.threads();
    let head = |i: usize| {
        cur.and_then(|t| t.heads.get(i))
            .filter(|h| h.mem_space == space_id)
            .and_then(|h| usize::try_from(h.pos).ok())
    };
    let heads: [Option<usize>; 4] = [head(0), head(1), head(2), head(3)];
    // Flow heads beyond the first (BCR and GP8 have up to nine).
    let extra_flow = |pos: usize| {
        cur.is_some_and(|t| {
            t.heads
                .iter()
                .skip(4)
                .any(|h| h.mem_space == space_id && usize::try_from(h.pos) == Ok(pos))
        })
    };
    let name_w = org
        .cpu
        .inst_names
        .iter()
        .map(|n| n.chars().count())
        .max()
        .unwrap_or(8)
        .clamp(6, 14);
    let genome_len = if space_id == 0 {
        usize::try_from(org.phenotype.genome_length).unwrap_or(sites)
    } else {
        sites
    };
    let mut lines: Vec<Line> = Vec::with_capacity(rows);
    for pos in top..(top + rows).min(sites) {
        let m = &space.memory[pos];
        let bp = if d.breakpoints.contains(&(space_id, pos as i32)) {
            '●'
        } else {
            ' '
        };
        let ip = if heads[0] == Some(pos) { '>' } else { ' ' };
        let mut others: String = ["R", "W", "F"]
            .iter()
            .enumerate()
            .map(|(i, l)| if heads[i + 1] == Some(pos) { *l } else { " " })
            .collect();
        if extra_flow(pos) {
            others.push('+');
        }
        let (fc, fcol) = flag_char(m.flags);
        let text = format!(
            "{bp}{ip}{pos:>4} {} {:<name_w$} {fc} {others}",
            m.symbol(),
            trunc(org.cpu.cell_name(Some(m)), name_w)
        );
        let text = trunc(&text, inner_w.max(1));
        let mut style = Style::new();
        if pos >= genome_len {
            style = DIM;
        }
        if d.memory_changed(pos) {
            style = changed_style();
        } else if fcol != Color::Reset && pos < genome_len {
            style = style.fg(fcol);
        }
        if heads[0] == Some(pos) {
            style = style.add_modifier(Modifier::BOLD).fg(hue(Hue::Green));
        }
        if pos == sel {
            style = style.add_modifier(Modifier::REVERSED);
        }
        lines.push(Line::styled(text, style));
    }
    // Heads of this thread that sit in another space are named here
    // instead of being drawn at a wrong line.
    let elsewhere: Vec<String> = cur
        .map(|t| {
            t.heads
                .iter()
                .enumerate()
                .filter(|(_, h)| h.mem_space != space_id)
                .map(|(i, h)| {
                    format!(
                        "{} {}:{}",
                        org.cpu
                            .head_names
                            .get(i)
                            .map_or_else(|| "Hd".to_string(), |n| short_head(n)),
                        h.mem_space,
                        h.pos
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let name = if org.cpu.spaces.len() > 1 {
        format!("Memory space {} ({}), {sites} sites", space.id, space.label)
    } else {
        format!("Memory {sites} sites")
    };
    let mut title = format!("{name}  line {sel}");
    if !elsewhere.is_empty() {
        title.push_str(&format!("  other spaces: {}", elsewhere.join(" ")));
    }
    let legend = "  > IP  ● breakpoint  R W F + heads";
    if title.chars().count() + legend.chars().count() <= inner_w.saturating_sub(2) {
        title.push_str(legend);
    }
    let title = trunc(&title, inner_w.saturating_sub(2));
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
            .block(block(&title)),
        area,
    );
}
