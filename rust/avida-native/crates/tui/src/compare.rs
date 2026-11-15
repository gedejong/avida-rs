//! The comparison screen of `avida-tui --compare DIR`: a batch of runs
//! (`<arm>/seed-<N>/`) as tables. Nothing is simulated.
//!
//! Two pages, `Tab` switches. **Summary**: per rung, how many seeds of each
//! arm held it at some update (and at the end), and per measure the median
//! with the quartiles of each arm, each with the p-value and the sample
//! sizes when two arms are compared. **Runs**: one line per seed with its
//! state, last update and highest rung; the selected run's folder is
//! written under the list. The numbers are those of
//! `avida_viewmodel::compare`, shared with the desktop interface; the
//! definitions (a rung is held at 1 % of the organisms or more, the tests)
//! are there, and nothing here calls a difference significant.
//!
//! Keys: `Tab` (or `1`, `2`) the page, Up and Down (`j`, `k`) scroll or
//! choose a run, `f` finished runs only, `p` the next pair of arms (with
//! more than two), `r` read the folder again, `q` or Esc to quit.

use std::io;
use std::path::Path;
use std::time::Duration;

use avida_viewmodel::compare::{p_text, CompareState, Measure, Run, Spread};
use avida_viewmodel::format_number;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::color::{hue, Hue};
use crate::ui::util::{block, clip_line, trunc, BOLD, DIM};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Summary,
    Runs,
}

pub struct CompareApp {
    pub state: CompareState,
    pub page: Page,
    /// Lines the summary is scrolled.
    pub scroll: usize,
    /// The run chosen on the Runs page (position in the list shown).
    pub selected: usize,
    pub quit: bool,
    /// A line for the footer: what the last key did.
    pub message: Option<String>,
}

impl CompareApp {
    pub fn new(state: CompareState) -> CompareApp {
        CompareApp {
            state,
            page: Page::Summary,
            scroll: 0,
            selected: 0,
            quit: false,
            message: None,
        }
    }

    /// The runs the Runs page lists, in arm then seed order.
    pub fn listed(&self) -> Vec<(usize, usize, &Run)> {
        let mut v = Vec::new();
        for (a, arm) in self.state.batch.arms.iter().enumerate() {
            for (r, run) in arm.runs.iter().enumerate() {
                if self.state.takes(run) {
                    v.push((a, r, run));
                }
            }
        }
        v
    }

    /// The next pair of arms in the order (0,1), (0,2), (1,2), ...
    fn next_pair(&mut self) {
        let n = self.state.batch.arms.len();
        let (mut a, mut b) = self.state.pair;
        b += 1;
        if b >= n {
            a += 1;
            b = a + 1;
        }
        if b >= n {
            a = 0;
            b = 1;
        }
        self.state.set_pair(a, b);
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        self.message = None;
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        let count = self.listed().len();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Tab | KeyCode::BackTab => {
                self.page = match self.page {
                    Page::Summary => Page::Runs,
                    Page::Runs => Page::Summary,
                }
            }
            KeyCode::Char('1') => self.page = Page::Summary,
            KeyCode::Char('2') => self.page = Page::Runs,
            KeyCode::Down | KeyCode::Char('j') => match self.page {
                Page::Summary => self.scroll += 1,
                Page::Runs => self.selected = (self.selected + 1).min(count.saturating_sub(1)),
            },
            KeyCode::Up | KeyCode::Char('k') => match self.page {
                Page::Summary => self.scroll = self.scroll.saturating_sub(1),
                Page::Runs => self.selected = self.selected.saturating_sub(1),
            },
            KeyCode::Home => {
                self.scroll = 0;
                self.selected = 0;
            }
            KeyCode::End => self.selected = count.saturating_sub(1),
            KeyCode::Char('f') => {
                let on = !self.state.finished_only;
                self.state.set_finished_only(on);
                self.selected = 0;
                self.message = Some(if on {
                    "Finished runs only.".to_string()
                } else {
                    "All runs, running ones with their values so far.".to_string()
                });
            }
            KeyCode::Char('p') => {
                if self.state.batch.arms.len() > 2 {
                    self.next_pair();
                } else {
                    self.message = Some("Only two arms: they are the pair.".to_string());
                }
            }
            KeyCode::Char('r') => {
                self.message = Some(match self.state.reload() {
                    Ok(()) => "Read the folder again.".to_string(),
                    Err(e) => e,
                });
                self.selected = self.selected.min(self.listed().len().saturating_sub(1));
            }
            _ => {}
        }
    }
}

/// The end of `text` in at most `w` columns, with an ellipsis in front when
/// the start was cut: the end of a path says which folder it is.
fn tail(text: &str, w: usize) -> String {
    let n = text.chars().count();
    if n <= w || w < 2 {
        return text.to_string();
    }
    let rest: String = text.chars().skip(n - (w - 1)).collect();
    format!("…{rest}")
}

fn arm_style(i: usize) -> Style {
    Style::new()
        .fg(hue(Hue::ALL[(i + 2) % Hue::ALL.len()]))
        .add_modifier(Modifier::BOLD)
}

/// `text` padded or cut to `w` columns.
fn cell(text: &str, w: usize) -> String {
    let t = trunc(text, w);
    format!("{t:<w$}")
}

/// `61 (55-67)`, the median with its quartiles.
fn spread_text(s: &Spread, integer: bool) -> String {
    let f = |v: f64| {
        if integer {
            format!("{}", v.round() as i64)
        } else {
            format_number((v * 100.0).round() / 100.0)
        }
    };
    let (m, q1, q3) = (f(s.median), f(s.q1), f(s.q3));
    if s.n >= 2 && (q1 != m || q3 != m) {
        format!("{m} ({q1}-{q3})")
    } else {
        m
    }
}

const LABEL_W: usize = 24;
const P_W: usize = 14;

/// The lines of the Summary page for a panel `width` columns wide.
pub fn summary_lines(app: &CompareApp, width: usize) -> Vec<Line<'static>> {
    let st = &app.state;
    let b = &st.batch;
    let arms = b.arms.len().max(1);
    let have_p = st.comparison.is_some();
    let p_w = if have_p { P_W } else { 0 };
    let col_w = (width.saturating_sub(LABEL_W + p_w) / arms).clamp(8, 28);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let total = b.run_count();
    let done = b.finished_count();
    lines.push(Line::from(format!(
        "{total} runs in {} arms: {done} finished, {} running.{}",
        b.arms.len(),
        total - done,
        if st.finished_only {
            " Finished runs only."
        } else if total > done {
            " Running ones: values so far."
        } else {
            ""
        }
    )));
    for n in &b.notes {
        lines.push(Line::styled(n.clone(), DIM));
    }
    if let Some(c) = &st.comparison {
        lines.push(Line::styled(
            format!(
                "p for {} against {}: Fisher exact on rungs, Mann-Whitney U ({}) on measures.",
                b.arms[c.a].name,
                b.arms[c.b].name,
                c.measures
                    .iter()
                    .find_map(|m| m.test.map(|t| t.method.label()))
                    .unwrap_or("exact")
            ),
            DIM,
        ));
    }

    // Rungs.
    let head = |first: &str, with_n: bool| -> Line<'static> {
        let mut spans = vec![Span::styled(cell(first, LABEL_W), BOLD)];
        for (i, arm) in b.arms.iter().enumerate() {
            let n = st.summary[i].with_ladder;
            let text = if with_n {
                format!("{} n={n}", arm.name)
            } else {
                arm.name.clone()
            };
            spans.push(Span::styled(cell(&text, col_w), arm_style(i)));
        }
        if have_p {
            spans.push(Span::styled(cell("p", p_w), BOLD));
        }
        Line::from(spans)
    };
    if b.rungs.is_empty() {
        lines.push(Line::styled(
            "No rungs: no run has a readable tasks.dat and count.dat.",
            DIM,
        ));
    } else {
        lines.push(head("Rung: ever (end)", true));
        for rung in (0..b.rungs.len()).rev() {
            let mut spans = vec![Span::raw(cell(&b.rungs[rung], LABEL_W))];
            for s in &st.summary {
                let c = s.rungs[rung];
                let style = if c.ever == 0 { DIM } else { Style::new() };
                spans.push(Span::styled(
                    cell(&format!("{} ({})", c.ever, c.at_end), col_w),
                    style,
                ));
            }
            if let Some(c) = &st.comparison {
                spans.push(Span::styled(cell(&p_text(c.rungs[rung].p_ever), p_w), DIM));
            }
            lines.push(Line::from(spans));
        }
    }
    lines.push(Line::raw(""));

    // Measures.
    lines.push(head("Median (quartiles)", false));
    for m in Measure::ALL {
        let mut spans = vec![Span::raw(cell(m.label(), LABEL_W))];
        if !b.recorded(m) {
            spans.push(Span::styled(
                format!("not recorded: needs {}", m.file_hint()),
                DIM,
            ));
            lines.push(Line::from(spans));
            continue;
        }
        for s in &st.summary {
            let ms = &s.measures[m.index()];
            let text = match &ms.spread {
                Some(sp) => {
                    let mut t = spread_text(sp, m.integer());
                    if ms.n < s.runs && col_w >= t.chars().count() + 7 {
                        t.push_str(&format!(" n={}/{}", ms.n, s.runs));
                    }
                    t
                }
                None => "-".to_string(),
            };
            spans.push(Span::raw(cell(&text, col_w)));
        }
        if let Some(c) = &st.comparison {
            let t = match c.measures[m.index()].test {
                Some(r) => format!("{} n {},{}", p_text(r.p), r.n_a, r.n_b),
                None => "-".to_string(),
            };
            spans.push(Span::styled(cell(&t, p_w), DIM));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// The lines of the Runs page and the position of the selected one.
pub fn run_lines(app: &CompareApp, width: usize) -> (Vec<Line<'static>>, usize) {
    let listed = app.listed();
    let name_w = listed
        .iter()
        .map(|(_, _, r)| r.arm.chars().count())
        .max()
        .unwrap_or(4)
        .clamp(4, 14);
    let b = &app.state.batch;
    let rung = |i: Option<usize>, run: &Run| -> String {
        if !run.has_ladder() {
            "-".to_string()
        } else {
            i.map_or("none".to_string(), |i| b.rungs[i].clone())
        }
    };
    let mut lines = vec![Line::styled(
        format!(
            "  {} {:<6} {:<9} {:>12}  {:<16} {:<16}",
            cell("Arm", name_w),
            "Seed",
            "State",
            "Last update",
            "Highest at end",
            "Highest ever"
        ),
        BOLD,
    )];
    for (pos, (a, _, run)) in listed.iter().enumerate() {
        let marker = if pos == app.selected { "> " } else { "  " };
        let note = if run.problems.is_empty() {
            String::new()
        } else {
            format!("  ! {}", run.problems[0])
        };
        let text = format!(
            "{marker}{} {:<6} {:<9} {:>12}  {:<16} {:<16}{note}",
            cell(&run.arm, name_w),
            trunc(&run.seed, 6),
            if run.finished { "finished" } else { "running" },
            run.last_update
                .map_or("-".to_string(), avida_viewmodel::compare::grouped),
            trunc(&rung(run.end_rung, run), 16),
            trunc(&rung(run.ever_rung, run), 16),
        );
        let style = if pos == app.selected {
            arm_style(*a)
                .remove_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        lines.push(clip_line(Line::styled(text, style), width));
    }
    let sel = app.selected.min(listed.len().saturating_sub(1)) + 1;
    (lines, sel)
}

pub fn draw(f: &mut Frame, app: &CompareApp) {
    let area = f.area();
    let title = format!(
        "Compare runs: {}",
        crate::ui::util::trunc(&app.state.dir.display().to_string(), 60)
    );
    let blk = block(&title);
    let inner = blk.inner(area);
    f.render_widget(blk, area);
    if inner.height < 8 || inner.width < 40 {
        f.render_widget(
            Paragraph::new("The window is too small (80 x 24 is enough)."),
            inner,
        );
        return;
    }
    let [body, folder, foot] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(if app.page == Page::Runs { 2 } else { 0 }),
        Constraint::Length(1),
    ])
    .areas::<3>(inner);
    let width = usize::from(body.width);
    match app.page {
        Page::Summary => {
            let lines: Vec<Line> = summary_lines(app, width)
                .into_iter()
                .map(|l| clip_line(l, width))
                .collect();
            let max = lines.len().saturating_sub(usize::from(body.height));
            let off = app.scroll.min(max);
            f.render_widget(
                Paragraph::new(lines).scroll((u16::try_from(off).unwrap_or(0), 0)),
                body,
            );
        }
        Page::Runs => {
            let (lines, sel) = run_lines(app, width);
            let h = usize::from(body.height);
            // Keep the header and scroll the rows so that the selection shows.
            let rows = lines.len() - 1;
            let room = h.saturating_sub(1).max(1);
            let first = sel.saturating_sub(room).min(rows.saturating_sub(room));
            let mut shown = vec![lines[0].clone()];
            shown.extend(lines.into_iter().skip(1 + first).take(room));
            f.render_widget(Paragraph::new(shown), body);
            if let Some((_, _, run)) = app.listed().get(app.selected) {
                let text = vec![
                    Line::from(vec![
                        Span::styled("Folder ", DIM),
                        Span::raw(tail(
                            &run.dir.display().to_string(),
                            width.saturating_sub(7),
                        )),
                    ]),
                    Line::styled(run.status(), DIM),
                ];
                f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), folder);
            }
        }
    }
    let help = app.message.clone().unwrap_or_else(|| {
        match app.page {
            Page::Summary => {
                "Tab runs   Up/Down scroll   f finished only   p pair   r reload   q quit"
            }
            Page::Runs => "Tab summary   Up/Down choose   f finished only   r reload   q quit",
        }
        .to_string()
    });
    f.render_widget(
        Paragraph::new(clip_line(Line::styled(help, DIM), usize::from(foot.width))),
        foot,
    );
    let _ = Rect::default();
}

/// Runs the screen in the terminal until `q`, Esc or Ctrl-C.
pub fn run(dir: &Path) -> Result<(), String> {
    let state = CompareState::load(dir)?;
    let mut app = CompareApp::new(state);
    let mut terminal =
        crate::runner::enter_terminal().map_err(|e| format!("cannot start the terminal: {e}"))?;
    let _guard = crate::runner::TerminalGuard;
    let result = (|| -> io::Result<()> {
        let mut dirty = true;
        while !app.quit {
            if dirty {
                terminal
                    .draw(|f| draw(f, &app))
                    .map_err(|e| io::Error::other(e.to_string()))?;
                dirty = false;
            }
            if crate::runner::terminal_hung_up() {
                return Err(crate::runner::terminal_gone());
            }
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(k) => app.on_key(k),
                    Event::Resize(..) => {}
                    _ => continue,
                }
                dirty = true;
            }
        }
        Ok(())
    })();
    result.map_err(|e| e.to_string())
}
