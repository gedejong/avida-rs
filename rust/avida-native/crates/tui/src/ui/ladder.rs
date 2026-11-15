//! The ladder screen: the environment's reactions in file order as rungs,
//! the top rung first, one line each. A line is the rung's name, a strip of
//! the share of the organisms that performed its task over the updates (eight
//! heights per cell, 0 to the whole population, all strips on one x axis), and
//! the count and share now. `o` in a strip is the update at which any
//! organism first performed the rung, `*` the update at which 1 % of the
//! organisms first did (`@` when both fall in one cell). A rung nobody has performed is a row of dots and the
//! word "never"; one performed before and by nobody now keeps its past and
//! says "lost".
//!
//! Up and Down choose a rung, whose dates are written under the table; Enter
//! makes "performs this task" a condition of the filter. With a filter set
//! the matching organisms' count is added to each line. A terminal has no
//! hover, so what the desktop interface says in tooltips is the selected
//! rung's lines. The series are those of `avida_viewmodel::ladder`.

use avida_viewmodel::{format_number, LadderView, Rung, RungState};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::util::{block, clip_lines, thousands, trunc, BOLD, DIM};
use crate::app::App;
use crate::color::{hue, Hue};

const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// The narrowest strip worth drawing.
const MIN_STRIP: usize = 12;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let area = super::filter_bar::take(f, app, area);
    let title = if app.playback.is_some() {
        "Ladder (one point per recorded frame)"
    } else {
        "Ladder"
    };
    let blk = block(title);
    let inner = blk.inner(area);
    f.render_widget(blk, area);
    if app.ladder.is_empty() {
        f.render_widget(
            Paragraph::new("No finished updates yet. Step or run the simulation.").style(DIM),
            inner,
        );
        return;
    }
    let view = app.ladder_view();
    if view.is_empty() {
        f.render_widget(
            Paragraph::new("This environment has no reaction with a task.").style(DIM),
            inner,
        );
        return;
    }
    let sel = app.ladder_sel.min(view.rungs.len() - 1);
    let detail_h: u16 = if inner.height >= 14 { 3 } else { 2 };
    // The axis sits right under the last rung; what is left is empty.
    let table_h = (view.rungs.len() as u16).min(inner.height.saturating_sub(2 + detail_h).max(1));
    let [head, table, axis, detail, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(table_h),
        Constraint::Length(1),
        Constraint::Length(detail_h),
        Constraint::Min(0),
    ])
    .areas(inner);
    f.render_widget(
        Paragraph::new(headline(&view, usize::from(head.width))),
        head,
    );
    draw_rows(f, app, &view, sel, table, axis);
    f.render_widget(
        Paragraph::new(details(app, &view, sel))
            .wrap(Wrap { trim: true })
            .style(DIM),
        detail,
    );
}

fn headline(view: &LadderView, w: usize) -> Line<'static> {
    let name = |i: Option<usize>| i.map_or("none yet".to_string(), |i| view.rungs[i].name.clone());
    let spans = vec![
        Span::styled(" Held now: ", DIM),
        Span::styled(name(view.highest_held), BOLD),
        Span::styled("   Reached: ", DIM),
        Span::styled(name(view.highest_reached), BOLD),
    ];
    clip_lines(vec![Line::from(spans)], w)
        .pop()
        .unwrap_or_default()
}

/// The columns of a line: marker, name, strip, numbers.
struct Cols {
    name: usize,
    strip: usize,
}

fn draw_rows(f: &mut Frame, app: &App, view: &LadderView, sel: usize, area: Rect, axis: Rect) {
    let w = usize::from(area.width);
    let with_subset = view.subset_orgs.is_some();
    let name_w = view
        .rungs
        .iter()
        .map(|r| r.name.chars().count())
        .max()
        .unwrap_or(4)
        .clamp(4, 12);
    // marker (3) + name + gap + strip + gap + count (7) + share (5) [+ subset (8)]
    let right = 3 + 7 + 1 + 5 + if with_subset { 9 } else { 0 };
    let strip = w.saturating_sub(3 + name_w + 1 + right + 1).max(MIN_STRIP);
    let cols = Cols {
        name: name_w,
        strip,
    };
    let (u0, u1) = (
        f64::from(view.updates.first().copied().unwrap_or(0)),
        f64::from(view.updates.last().copied().unwrap_or(1)),
    );
    let u1 = if u1 <= u0 { u0 + 1.0 } else { u1 };

    // The rungs top first; a page of them with the chosen one in view.
    let n = view.rungs.len();
    let rows = usize::from(area.height).max(1);
    let shown = rows.min(n);
    // Pages of `shown` rungs counted from the top; the last page is the
    // bottom rungs, so it is never half empty.
    let pos = n - 1 - sel;
    let first = ((pos / shown) * shown).min(n - shown);
    let green = Style::new().fg(hue(Hue::Green));
    let magenta = Style::new().fg(hue(Hue::Magenta));
    let mut lines: Vec<Line> = Vec::new();
    for p in first..first + shown {
        let i = n - 1 - p;
        let rung = &view.rungs[i];
        let chosen = i == sel;
        let mark = app.filtering.filter.task_state(rung.task);
        let mut spans: Vec<Span> = vec![
            Span::raw(if chosen { ">" } else { " " }),
            Span::raw(match mark {
                Some(true) => "+",
                Some(false) => "-",
                None => " ",
            }),
            Span::raw(" "),
        ];
        let nm = format!("{:<w$}", trunc(&rung.name, cols.name), w = cols.name);
        spans.push(Span::styled(
            nm,
            match (chosen, rung.state) {
                (true, _) => Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                (_, RungState::NeverReached) => DIM,
                _ => Style::new(),
            },
        ));
        spans.push(Span::raw(" "));
        let (text, never) = strip_of(view, rung, &cols, u0, u1);
        spans.push(Span::styled(text, if never { DIM } else { green }));
        spans.push(Span::raw(" "));
        match rung.state {
            RungState::NeverReached => spans.push(Span::styled(format!("{:>13}", "never"), DIM)),
            RungState::Lost => spans.push(Span::styled(format!("{:>13}", "lost"), DIM)),
            RungState::Present => {
                spans.push(Span::styled(
                    format!("{:>7}", thousands(i64::from(rung.now))),
                    BOLD,
                ));
                spans.push(Span::raw(format!(
                    " {:>5}",
                    percent(rung.percent(view.orgs_now))
                )));
            }
        }
        if let Some(s) = rung.subset {
            spans.push(Span::styled(
                format!(" {:>8}", thousands(i64::from(s))),
                magenta,
            ));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(clip_lines(lines, w)), area);

    // The shared axis under the strips.
    let lead = 3 + cols.name + 1;
    let mut text = vec![' '; w];
    let mut put = |at: usize, s: &str| {
        for (k, ch) in s.chars().enumerate() {
            if let Some(c) = text.get_mut(at + k) {
                *c = ch;
            }
        }
    };
    let a = format_number(u0);
    let m = format_number((u0 + u1) / 2.0);
    let b = format_number(u1);
    put(lead, &a);
    put(lead + cols.strip.saturating_sub(m.chars().count()) / 2, &m);
    put(lead + cols.strip.saturating_sub(b.chars().count()), &b);
    put(lead + cols.strip + 1, "update");
    let axis_line: String = text.into_iter().collect();
    f.render_widget(Paragraph::new(axis_line).style(DIM), axis);
}

/// A rung's strip as text of `cols.strip` cells, and whether it is all dots.
/// A column shows the largest share of the updates it covers, the share
/// before it where none falls in it; a share above zero is at least one
/// eighth tall; zero is a dot.
fn strip_of(view: &LadderView, rung: &Rung, cols: &Cols, u0: f64, u1: f64) -> (String, bool) {
    let w = cols.strip;
    if rung.state == RungState::NeverReached {
        return ("·".repeat(w), true);
    }
    let n = view.updates.len();
    let mut out: Vec<char> = Vec::with_capacity(w);
    let mut j = 0;
    let mut held = 0.0_f64;
    for c in 0..w {
        let edge = u0 + (u1 - u0) * (c + 1) as f64 / w as f64;
        let mut best: Option<f64> = None;
        while j < n && f64::from(view.updates[j]) <= edge {
            let o = view.orgs[j];
            let s = if o > 0 {
                f64::from(rung.counts[j]) / f64::from(o)
            } else {
                0.0
            };
            best = Some(best.map_or(s, |b: f64| b.max(s)));
            j += 1;
        }
        if let Some(b) = best {
            held = b;
        }
        out.push(if held <= 0.0 {
            '·'
        } else {
            LEVELS[((held * 8.0).ceil() as usize).clamp(1, 8) - 1]
        });
    }
    // The dates replace the cell they fall in.
    let col_of = |u: i32| -> usize {
        (((f64::from(u) - u0) / (u1 - u0) * w as f64) as usize).min(w.saturating_sub(1))
    };
    // Both in one cell: `@`.
    if let Some(u) = rung.first_reached {
        out[col_of(u)] = 'o';
    }
    if let Some(u) = rung.first_held {
        let c = col_of(u);
        out[c] = if out[c] == 'o' { '@' } else { '*' };
    }
    (out.into_iter().collect(), false)
}

fn percent(p: f64) -> String {
    if p >= 10.0 || p == 0.0 {
        format!("{p:.0}%")
    } else {
        format!("{p:.1}%")
    }
}

/// What the desktop hover says, for the chosen rung.
fn details(app: &App, view: &LadderView, sel: usize) -> Vec<Line<'static>> {
    let rung = &view.rungs[sel];
    let when = |u: Option<i32>| {
        u.map_or("not yet".to_string(), |u| {
            format!("update {}", thousands(i64::from(u)))
        })
    };
    let mut first = format!(
        "{}: first performed (o) {}, first held by 1% or more (*) {}.",
        rung.name,
        when(rung.first_reached),
        when(rung.first_held),
    );
    if let (Some(s), Some(m)) = (rung.subset, view.subset_orgs) {
        first.push_str(&format!(
            " Matching organisms now: {} of {}.",
            thousands(i64::from(s)),
            thousands(i64::from(m))
        ));
    }
    if !rung.shares_task_with.is_empty() {
        first.push_str(&format!(
            " Same task as {}: the counts are the same.",
            rung.shares_task_with.join(", ")
        ));
    }
    let act = match app.filtering.filter.task_state(rung.task) {
        Some(true) => "The filter keeps organisms that perform it; Enter removes that condition.",
        _ => "Enter keeps only organisms that perform it.",
    };
    vec![Line::from(first), Line::from(act)]
}

#[cfg(test)]
mod tests {
    use crate::app::{App, Screen};
    use crate::fixtures::{app_with, frame_with_map};
    use avida_session::UpdateRow;
    use avida_viewmodel::{Condition, Frame, ReactionView, TaskView};
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;

    const NAMES: [&str; 9] = [
        "NOT", "NAND", "AND", "ORN", "OR", "ANDN", "NOR", "XOR", "EQU",
    ];

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let done = t.draw(|f| super::super::draw(f, app)).unwrap();
        let a = done.buffer.area;
        let mut out = String::new();
        for y in 0..a.height {
            let mut line = String::new();
            for x in 0..a.width {
                line.push_str(done.buffer[(x, y)].symbol());
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    /// The stock logic-9 reactions over a 3x2 world whose cell 0 performs
    /// tasks 0 and 1 and cell 3 task 0.
    fn frame() -> Frame {
        let mut f = frame_with_map();
        f.env.reactions = NAMES
            .iter()
            .enumerate()
            .map(|(k, n)| ReactionView {
                name: (*n).to_string(),
                task: Some(k),
                value: 1.0,
                last_count: 0,
            })
            .collect();
        f.env.tasks = NAMES
            .iter()
            .map(|n| TaskView {
                name: n.to_lowercase(),
                desc: (*n).to_string(),
                last_count: 0,
            })
            .collect();
        if let Some(m) = f.map.as_mut() {
            m.num_tasks = 9;
            m.task_bits = vec![0b11, 0, 0, 0b1, 0, 0].into();
        }
        f
    }

    /// A climb over 12 frames of 200 organisms: NOT from update 10 (held at
    /// once), NAND from 30 (held from 50), XOR at 60 for a moment (reached,
    /// then lost), nothing above.
    fn rows() -> Vec<UpdateRow> {
        (0..12)
            .map(|i| {
                let mut c = vec![0; 9];
                if i >= 1 {
                    c[0] = 20 + 6 * i;
                }
                if i == 3 {
                    c[1] = 1;
                }
                if i >= 5 {
                    c[1] = 2 * i;
                }
                if i == 6 {
                    c[7] = 1;
                }
                UpdateRow {
                    update: i * 10,
                    orgs: 200,
                    task_last_count: c,
                    ..UpdateRow::default()
                }
            })
            .collect()
    }

    fn climbing() -> App {
        let mut a = app_with(frame());
        a.apply_rows(rows());
        a.take_commands();
        a.on_key(key(KeyCode::Char('J')));
        assert_eq!(a.screen, Screen::Ladder);
        a
    }

    /// The rung's line: inside the box, the name right after the marker.
    fn line_of<'a>(t: &'a str, name: &str) -> &'a str {
        t.lines()
            .find(|l| {
                l.strip_prefix('│').is_some_and(|r| {
                    r.chars()
                        .skip(3)
                        .collect::<String>()
                        .split_whitespace()
                        .next()
                        == Some(name)
                })
            })
            .unwrap_or_else(|| panic!("no line for {name} in:\n{t}"))
    }

    #[test]
    fn every_rung_is_a_line_top_rung_first_at_both_sizes() {
        for (w, h) in [(100, 30), (80, 24)] {
            let mut a = climbing();
            let t = render(&mut a, w, h);
            let at = |n: &str| {
                let l = line_of(&t, n);
                t.lines().position(|x| std::ptr::eq(x, l)).unwrap()
            };
            for pair in NAMES.windows(2) {
                assert!(
                    at(pair[1]) < at(pair[0]),
                    "{} above {} at {w}x{h}:\n{t}",
                    pair[1],
                    pair[0]
                );
            }
            has(&t, "Held now: NAND");
            has(&t, "Reached: XOR");
            // NOT is held by 86 of 200 now, NAND by 22. NOT's two dates
            // fall in one cell, NAND's in two.
            let not = line_of(&t, "NOT");
            assert!(not.contains("86") && not.contains("43%"), "{not}");
            assert!(not.contains('@'), "dates marked: {not}");
            let nand = line_of(&t, "NAND");
            assert!(nand.contains('o') && nand.contains('*'), "{nand}");
            assert!(line_of(&t, "EQU").contains("never"), "{t}");
            assert!(line_of(&t, "XOR").contains("lost"), "{t}");
            assert!(line_of(&t, "EQU").contains("····"));
            // The shared axis and the date of the chosen (bottom) rung.
            has(&t, "update");
            has(
                &t,
                "NOT: first performed (o) update 10, first held by 1% or more (*) update 10.",
            );
            for l in t.lines() {
                assert!(l.chars().count() <= w as usize, "{l}");
            }
            has(&t, "J Ladder");
        }
    }

    fn has(t: &str, needle: &str) {
        assert!(t.contains(needle), "expected {needle:?} in:\n{t}");
    }

    #[test]
    fn up_and_down_choose_a_rung_and_enter_filters_on_it() {
        let mut a = climbing();
        // Up goes up the ladder.
        a.on_key(key(KeyCode::Up));
        a.on_key(key(KeyCode::Up));
        has(&render(&mut a, 100, 30), "AND: first performed (o) not yet");
        a.on_key(key(KeyCode::Down));
        let t = render(&mut a, 100, 30);
        has(&t, "NAND: first performed (o) update 30");
        a.on_key(key(KeyCode::Enter));
        assert_eq!(
            a.filtering.filter.conditions().to_vec(),
            vec![Condition::Task {
                task: 1,
                performs: true
            }]
        );
        let t = render(&mut a, 100, 30);
        has(&t, "Filter: Performs task NAND");
        // The subset's count joins every line: one of the three organisms
        // performs NAND (cell 0), none performs NOR.
        let nand = line_of(&t, "NAND");
        assert!(nand.contains(">+ NAND"), "{nand}");
        assert!(nand.contains(&format!("11%{:>9}", 1)), "{nand}");
        has(
            &t,
            "The filter keeps organisms that perform it; Enter removes that condition.",
        );
        a.on_key(key(KeyCode::Enter));
        assert!(a.filtering.filter.is_empty());
        assert!(!render(&mut a, 100, 30).contains("Filter:"));
    }

    #[test]
    fn a_short_terminal_pages_the_rungs_and_keeps_the_chosen_one_in_view() {
        let mut a = climbing();
        // 40x10 leaves one line for the rungs.
        let t = render(&mut a, 40, 10);
        has(&t, "NOT");
        assert!(!t.contains("EQU"), "{t}");
        for _ in 0..8 {
            a.on_key(key(KeyCode::Up));
        }
        let t = render(&mut a, 40, 10);
        has(&t, "EQU");
        assert!(!t.contains("NOT "), "{t}");
        for l in t.lines() {
            assert!(l.chars().count() <= 40, "{l}");
        }
    }

    #[test]
    fn the_tab_strip_keeps_the_current_screen_in_view_at_80_columns() {
        let mut a = climbing();
        let strip = |t: &str| t.lines().nth(1).unwrap().to_string();
        let t = render(&mut a, 80, 24);
        let s = strip(&t);
        assert!(s.contains("J Ladder"), "{s}");
        assert!(s.starts_with(" <"), "{s}");
        a.on_key(key(KeyCode::Char('M')));
        let s = strip(&render(&mut a, 80, 24));
        assert!(s.contains("M Map") && s.contains("K Open"), "{s}");
    }

    #[test]
    fn before_any_row_the_screen_says_so_and_an_environment_without_tasks_too() {
        let mut a = app_with(frame());
        a.on_key(key(KeyCode::Char('J')));
        has(&render(&mut a, 80, 24), "No finished updates yet");
        let mut a = app_with(frame_with_map());
        a.apply_rows(rows());
        a.on_key(key(KeyCode::Char('J')));
        has(&render(&mut a, 80, 24), "no reaction with a task");
    }

    #[test]
    fn the_help_and_the_palette_list_the_key() {
        let mut a = climbing();
        a.on_key(key(KeyCode::Char('?')));
        let t = render(&mut a, 100, 40);
        has(&t, "Go to the ladder of reactions");
        let _ = &mut a;
    }
}
