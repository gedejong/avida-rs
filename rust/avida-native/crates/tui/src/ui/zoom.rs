//! The organism inspector (the viewer's zoom screen): phenotype, tasks,
//! the virtual CPU's registers, heads and stacks, and a memory listing
//! with the instruction pointer.

use avida_viewmodel::{format_number, mem_flags, EnvView, OrganismView, ThreadView};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::util::{block, born_at, clip_lines, kv, trunc, DIM};
use crate::app::App;
use crate::color::{hue, Hue};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let Some(frame) = app.frame.as_ref() else {
        f.render_widget(Paragraph::new(super::FIRST_FRAME).style(DIM), area);
        return;
    };
    let Some(org) = frame.inspect.as_ref() else {
        let text = match (app.selection, app.world_size()) {
            (Some(c), Some((w, _))) => format!(
                " Cell ({},{}) is empty. Move the cursor (arrows, hjkl) or press C for the next organism.",
                c as usize % w,
                c as usize / w
            ),
            _ => " No organism inspected. Press Enter on the map, or C to pick the next organism."
                .to_string(),
        };
        f.render_widget(
            Paragraph::new(text)
                .style(DIM)
                .wrap(Wrap { trim: true })
                .block(block("Organism")),
            area,
        );
        return;
    };
    let tid = app.zoom_thread.min(org.cpu.threads.len().saturating_sub(1));
    let thread = org.cpu.threads.get(tid);

    if area.width >= 120 {
        let [left, right] =
            Layout::horizontal([Constraint::Length(64), Constraint::Min(30)]).areas(area);
        let cpu_h = cpu_height(thread, area.height);
        let [top, tasks, cpu] = Layout::vertical([
            Constraint::Length(8),
            Constraint::Min(4),
            Constraint::Length(cpu_h),
        ])
        .areas(left);
        draw_phenotype(f, org, top);
        draw_tasks(f, org, &frame.env, tasks);
        draw_cpu(f, org, thread, tid, cpu, stacks_shown(thread, area.height));
        draw_memory(f, app, org, thread, right);
    } else {
        let cpu_h = cpu_height(thread, area.height);
        let top_h = if area.height >= 20 { 8 } else { 7 };
        let [top, cpu, mem] = Layout::vertical([
            Constraint::Length(top_h),
            Constraint::Length(cpu_h),
            Constraint::Min(3),
        ])
        .areas(area);
        let [phen, tasks] =
            Layout::horizontal([Constraint::Length(42), Constraint::Min(10)]).areas(top);
        draw_phenotype(f, org, phen);
        draw_tasks(f, org, &frame.env, tasks);
        draw_cpu(f, org, thread, tid, cpu, stacks_shown(thread, area.height));
        draw_memory(f, app, org, thread, mem);
    }
}

/// Stacks listed: all of them when there is room, else the active one.
fn stacks_shown(thread: Option<&ThreadView>, body_height: u16) -> usize {
    let n = thread.map_or(1, |t| t.stacks.len().clamp(1, 3));
    if body_height >= 28 {
        n
    } else {
        1
    }
}

fn cpu_height(thread: Option<&ThreadView>, body_height: u16) -> u16 {
    let stacks = stacks_shown(thread, body_height) as u16;
    let labels = thread.is_some_and(|t| !t.read_label.is_empty() || !t.next_label.is_empty());
    let stacks = stacks + u16::from(labels);
    // registers, heads, labels?, stacks, i/o, plus the border
    4 + stacks
}

fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

fn draw_phenotype(f: &mut Frame, o: &OrganismView, area: Rect) {
    let p = &o.phenotype;
    let lines = vec![
        Line::from(vec![
            Span::styled("genotype ", DIM),
            Span::styled(
                if o.genotype_name.is_empty() {
                    "-".to_string()
                } else {
                    o.genotype_name.clone()
                },
                Style::new()
                    .add_modifier(Modifier::BOLD)
                    .fg(hue(Hue::Magenta)),
            ),
        ]),
        Line::raw(format!(
            "merit {}  fitness {}  bonus {}",
            format_number(p.merit),
            format_number(p.fitness),
            format_number(p.cur_bonus)
        )),
        Line::raw(format!(
            "gestation {}  age {}  generation {}",
            p.gestation_time, p.age, p.generation
        )),
        Line::raw(format!(
            "length {}  copied {}  executed {}",
            p.genome_length, p.copied_size, p.executed_size
        )),
        Line::raw(format!(
            "cycles {}  time {}  divides {}  born {}",
            p.cpu_cycles_used,
            p.time_used,
            p.num_divides,
            born_at(p.birth_update)
        )),
        Line::raw(format!(
            "breeds true {}  modified {}  threads {}",
            yes(p.parent_true),
            yes(p.is_modified),
            o.cpu.threads.len()
        )),
    ];
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2)))).block(block(
            &format!(
                "Organism #{} in cell {} ({},{})",
                o.org_id, o.cell, o.x, o.y
            ),
        )),
        area,
    );
}

fn draw_tasks(f: &mut Frame, o: &OrganismView, env: &EnvView, area: Rect) {
    let h = usize::from(area.height.saturating_sub(2));
    let w = usize::from(area.width.saturating_sub(2));
    let p = &o.phenotype;
    let mut lines: Vec<Line> = Vec::new();
    let name_w = w.saturating_sub(10).clamp(4, 14);
    lines.push(Line::styled(
        format!("{:<name_w$} {:>4} {:>4}", "task", "now", "last"),
        DIM,
    ));
    let n = env.tasks.len();
    let room = h.saturating_sub(1);
    for (i, t) in env.tasks.iter().enumerate().take(room) {
        if room < n && i + 1 == room {
            lines.push(Line::styled(format!("+{} more", n - i), DIM));
            break;
        }
        let cur = p.cur_task_count.get(i).copied().unwrap_or(0);
        let last = p.last_task_count.get(i).copied().unwrap_or(0);
        let style = if last > 0 || cur > 0 {
            Style::new().fg(hue(Hue::Green))
        } else {
            DIM
        };
        lines.push(Line::styled(
            format!("{:<name_w$} {cur:>4} {last:>4}", trunc(&t.name, name_w)),
            style,
        ));
    }
    if n == 0 {
        lines.push(Line::styled("no tasks defined", DIM));
    }
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
            .block(block("Tasks")),
        area,
    );
}

pub(super) fn nums(v: &[i32], max: usize) -> String {
    let mut s: Vec<String> = v.iter().take(max).map(i32::to_string).collect();
    if v.len() > max {
        s.push("…".into());
    }
    s.join(" ")
}

fn draw_cpu(
    f: &mut Frame,
    o: &OrganismView,
    thread: Option<&ThreadView>,
    tid: usize,
    area: Rect,
    max_stacks: usize,
) {
    let mut lines: Vec<Line> = Vec::new();
    let cpu = &o.cpu;
    match thread {
        None => lines.push(Line::styled("no thread state", DIM)),
        Some(t) => {
            if t.regs.is_empty() {
                lines.push(Line::styled(
                    "registers are not exposed for this thread",
                    DIM,
                ));
            } else {
                let mut spans = Vec::new();
                for (i, r) in t.regs.iter().enumerate() {
                    let name = char::from(b'A' + (i % 26) as u8);
                    spans.push(Span::styled(format!("{name}X "), DIM));
                    spans.push(Span::styled(
                        format!("{r:<11}"),
                        Style::new().add_modifier(Modifier::BOLD),
                    ));
                }
                lines.push(Line::from(spans));
            }
            let names = &cpu.head_names;
            let heads: Vec<String> = t
                .heads
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let n = names.get(i).map_or("Head", String::as_str);
                    if h.mem_space != 0 {
                        format!("{n} {}:{}", h.mem_space, h.pos)
                    } else {
                        format!("{n} {}", h.pos)
                    }
                })
                .collect();
            let l = format!("heads  {}", heads.join("  "));
            lines.push(Line::raw(l));
            if !t.read_label.is_empty() || !t.next_label.is_empty() {
                lines.push(Line::raw(format!(
                    "labels read {}  next {}",
                    if t.read_label.is_empty() {
                        "-"
                    } else {
                        &t.read_label
                    },
                    if t.next_label.is_empty() {
                        "-"
                    } else {
                        &t.next_label
                    },
                )));
            }
            let mut order: Vec<usize> = (0..t.stacks.len()).collect();
            // With little room, the active stack goes first and alone.
            order.sort_by_key(|&i| i32::from(i as i32 != t.active_stack));
            for &si in order.iter().take(max_stacks) {
                let st = &t.stacks[si];
                let active = si as i32 == t.active_stack;
                let name = char::from(b'A' + (si % 26) as u8);
                lines.push(Line::raw(format!(
                    "stack {name}{} {}",
                    if active { "*" } else { " " },
                    nums(st, 10)
                )));
            }
            if t.stacks.is_empty() {
                lines.push(Line::styled("no stacks", DIM));
            }
        }
    }
    lines.push(kv(
        "i/o ",
        4,
        format!(
            "in {}  out {}",
            nums(&o.input_buf, 6),
            nums(&o.output_buf, 6)
        ),
    ));
    let title = format!(
        "CPU {} thread {}/{} (t cycles)",
        cpu.inst_set,
        tid + 1,
        cpu.threads.len().max(1)
    );
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(area.width.saturating_sub(2))))
            .block(block(&title)),
        area,
    );
}

pub(super) fn flag_char(flags: u8) -> (char, Color) {
    if flags & (mem_flags::MUTATED | mem_flags::POINT_MUT | mem_flags::COPY_MUT) != 0 {
        ('m', hue(Hue::Magenta))
    } else if flags & mem_flags::EXECUTED != 0 {
        ('x', hue(Hue::Green))
    } else if flags & mem_flags::COPIED != 0 {
        ('c', hue(Hue::Cyan))
    } else {
        (' ', Color::Reset)
    }
}

/// Where the memory listing starts and how many sites a page holds.
pub fn memory_page(
    sites: usize,
    rows: usize,
    cols: usize,
    ip: usize,
    follow: bool,
    scroll: usize,
) -> (usize, usize) {
    let page = (rows * cols).max(1);
    let start = if follow {
        (ip / page) * page
    } else {
        scroll.min(sites.saturating_sub(1))
    };
    (start, page)
}

fn draw_memory(
    f: &mut Frame,
    app: &App,
    o: &OrganismView,
    thread: Option<&ThreadView>,
    area: Rect,
) {
    let cpu = &o.cpu;
    let sites = cpu.memory.len();
    let inner_w = usize::from(area.width.saturating_sub(2));
    let rows = usize::from(area.height.saturating_sub(2)).max(1);
    let name_w = cpu
        .inst_names
        .iter()
        .map(|n| n.chars().count())
        .max()
        .unwrap_or(8)
        .clamp(6, 14);
    let entry_w = 4 + 1 + 3 + 1 + 1 + 1 + name_w + 1 + 1 + 2;
    let cols = (inner_w / entry_w).max(1);
    let head_pos: Vec<Option<usize>> = (0..4)
        .map(|i| {
            thread
                .and_then(|t| t.heads.get(i))
                .filter(|h| h.mem_space == 0)
                .and_then(|h| usize::try_from(h.pos).ok())
        })
        .collect();
    let ip = head_pos[0].unwrap_or(0);
    let (start, page) = memory_page(sites, rows, cols, ip, app.zoom_follow_ip, app.zoom_scroll);

    let mut lines: Vec<Line> = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut spans: Vec<Span> = Vec::new();
        for c in 0..cols {
            let pos = start + c * rows + r;
            let Some(m) = cpu.memory.get(pos) else {
                continue;
            };
            let letters = ["I", "R", "W", "F"];
            let marks: String = (0..4)
                .map(|i| {
                    if head_pos[i] == Some(pos) {
                        letters[i]
                    } else {
                        " "
                    }
                })
                .collect();
            let (fc, fcol) = flag_char(m.flags);
            let name = cpu.inst_name(pos);
            let text = format!(
                "{marks} {pos:>3} {} {:<name_w$} {fc} ",
                m.symbol(),
                trunc(name, name_w)
            );
            let mut style = Style::new();
            if head_pos[0] == Some(pos) {
                style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
            } else if fcol != Color::Reset {
                style = style.fg(fcol);
            }
            let text: String = text
                .chars()
                .chain(std::iter::repeat(' '))
                .take(entry_w)
                .collect();
            spans.push(Span::styled(text, style));
        }
        lines.push(Line::from(spans));
    }
    let pages = sites.div_ceil(page).max(1);
    let elsewhere = match thread.and_then(|t| t.heads.first()) {
        Some(h) if h.mem_space != 0 => {
            format!(", IP in space {} (see the debugger)", h.mem_space)
        }
        _ => String::new(),
    };
    let short = format!(
        "Memory {sites} sites, page {}/{pages}{elsewhere}",
        start / page + 1
    );
    let long = format!("{short}   I IP  R read  W write  F flow   x executed  c copied  m mutated");
    let title = if long.chars().count() + 4 <= inner_w {
        long
    } else {
        short
    };
    f.render_widget(
        Paragraph::new(lines).block(block(&trunc(&title, inner_w.saturating_sub(2)))),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_pages_follow_the_ip() {
        assert_eq!(memory_page(50, 10, 2, 5, true, 0), (0, 20));
        assert_eq!(memory_page(50, 10, 2, 25, true, 0), (20, 20));
        assert_eq!(memory_page(50, 10, 2, 25, false, 7), (7, 20));
        assert_eq!(memory_page(50, 10, 2, 25, false, 999), (49, 20));
    }
}
