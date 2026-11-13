//! Drawing. Every function takes a `&mut ratatui::Frame` (or a `Buffer`)
//! and the [`App`], so a `TestBackend` can render any screen.

mod abundance;
mod charts;
mod config;
mod debug;
mod env;
mod genotypes;
mod help;
mod histogram;
mod lineage;
mod logview;
mod map;
mod options;
mod palette;
mod stats;
mod util;
mod zoom;

use avida_session::RunState;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::actions::{first_key, Action};
use crate::app::{App, Screen, ToastKind};
use crate::color::{hue, Hue};
use util::{trunc, BOLD, DIM};

/// The smallest terminal the screens are laid out for.
pub const MIN_SIZE: (u16, u16) = (40, 10);

/// The narrowest the run name in the header is cut to.
const MIN_NAME: usize = 6;

pub use histogram::histogram_bins;
pub use map::{cell_summary, map_legend_spans};

/// Draws the whole application.
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.tab_hits.clear();
    if area.width < MIN_SIZE.0 || area.height < MIN_SIZE.1 {
        let msg = format!(
            "Terminal too small: {}x{} (need {}x{})",
            area.width, area.height, MIN_SIZE.0, MIN_SIZE.1
        );
        f.render_widget(Paragraph::new(msg), area);
        return;
    }
    let fatal = u16::from(app.fatal.is_some());
    let [header, tabs, banner, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(fatal),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(f, app, header);
    draw_tabs(f, app, tabs);
    if let Some(e) = &app.fatal {
        let first = e.lines().next().unwrap_or("");
        // The pointer to the log comes last in the sentence but is never the
        // part that is cut.
        let tail = format!("  Press {} for the log.", Screen::Log.key());
        let head = format!(" The run stopped: {first}");
        let room = usize::from(banner.width).saturating_sub(tail.chars().count());
        let text = format!("{}{tail}", pad_to(&trunc(&head, room), room));
        f.render_widget(
            Paragraph::new(trunc(&text, usize::from(banner.width))).style(
                Style::new()
                    .fg(Color::White)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            banner,
        );
    }

    match app.screen {
        Screen::Map => map::draw(f, app, body),
        Screen::Stats => stats::draw(f, app, body),
        Screen::Charts => charts::draw(f, app, body),
        Screen::Genotypes => genotypes::draw(f, app, body),
        Screen::Zoom => zoom::draw(f, app, body),
        Screen::Debug => debug::draw(f, app, body),
        Screen::Config => config::draw(f, app, body),
        Screen::Environment => env::draw(f, app, body),
        Screen::Histogram => histogram::draw(f, app, body),
        Screen::Log => logview::draw(f, app, body),
        Screen::Options => options::draw(f, app, body),
        Screen::Blank => {
            f.render_widget(
                Paragraph::new("Blank screen. Any screen key brings the display back.").style(DIM),
                body,
            );
        }
    }

    draw_footer(f, app, footer);
    if app.show_help {
        help::draw(f, app, area);
    }
    if let Some(p) = &app.cmd_palette {
        palette::draw(f, app, p, area);
    }
    if app.color_mode.is_mono() {
        strip_colors(f.buffer_mut());
    }
}

/// Removes every colour from the drawn screen. Where a cell had a
/// background (badges, bars, selections) it is drawn reversed instead, so
/// the structure survives without colour.
pub fn strip_colors(buf: &mut ratatui::buffer::Buffer) {
    for cell in &mut buf.content {
        if cell.bg != Color::Reset {
            cell.modifier.toggle(Modifier::REVERSED);
        }
        cell.fg = Color::Reset;
        cell.bg = Color::Reset;
        cell.underline_color = Color::Reset;
    }
}

/// `s` followed by spaces up to `w` columns.
fn pad_to(s: &str, w: usize) -> String {
    let n = s.chars().count();
    format!("{s}{}", " ".repeat(w.saturating_sub(n)))
}

/// The text and colours of the state badge.
pub fn badge(state: RunState) -> (&'static str, Color, Color) {
    match state {
        RunState::Running => (" RUNNING ", Color::Black, Color::Green),
        RunState::Paused => (" PAUSED ", Color::Black, Color::Yellow),
        RunState::Finished => (" FINISHED ", Color::White, Color::Blue),
        RunState::Failed => (" FAILED ", Color::White, Color::Red),
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    // Until the simulation has reported anything the run is not paused, it
    // has not started.
    let (text, fg, bg) = if app.status.is_none() && app.frame.is_none() && app.fatal.is_none() {
        (" STARTING ", Color::Black, Color::Gray)
    } else if app.playback.is_some() {
        if app.run_state() == RunState::Running {
            (" PLAYING ", Color::Black, Color::Green)
        } else {
            (" PLAYBACK ", Color::Black, Color::Cyan)
        }
    } else {
        badge(app.run_state())
    };
    let width = usize::from(area.width);

    // The right-hand items with their drop order: when the line is too
    // narrow the highest number goes first, and the update never does.
    let mut items: Vec<(u8, Span)> = Vec::new();
    if let Some(u) = app.current_update() {
        let s = match &app.playback {
            Some(p) => format!("update {}/{}", u.max(0), p.last_update.max(0)),
            None => format!("update {}", u.max(0)),
        };
        items.push((
            0,
            Span::styled(s, Style::new().add_modifier(Modifier::BOLD)),
        ));
        if let Some((done, total)) = app.status.as_ref().and_then(|s| s.slice) {
            items.push((
                5,
                Span::styled(format!(" (slice {done}/{total})"), Style::new()),
            ));
        }
    }
    if let Some(p) = &app.playback {
        items.push((
            1,
            Span::styled(
                format!("  frame {}/{}", p.index + 1, p.frames),
                Style::new(),
            ),
        ));
    }
    if let Some(r) = &app.recording {
        if r.active {
            items.push((
                1,
                Span::styled(
                    format!("  REC {} frames", r.frames),
                    Style::new().fg(hue(Hue::Red)),
                ),
            ));
        }
    }
    if let Some(r) = app.latest_stats() {
        items.push((2, Span::styled(format!("  orgs {}", r.orgs), Style::new())));
    }
    if let Some(ups) = app.rate.rate(app.now) {
        items.push((4, Span::styled(format!("  {ups:.1} ups"), DIM)));
    }
    items.push((
        3,
        Span::styled(format!("  speed {}", app.speed_label()), DIM),
    ));
    let problems = app.log.problems();
    if problems > 0 {
        let s = format!(
            "  {problems} warning{}",
            if problems == 1 { "" } else { "s" }
        );
        items.push((1, Span::styled(s, Style::new().fg(hue(Hue::Magenta)))));
    }
    let badge_w = text.chars().count();
    let right_len = |items: &[(u8, Span)]| -> usize {
        items.iter().map(|(_, s)| s.content.chars().count()).sum()
    };
    // " avida " + space + name + space + badge + space, and one spare column.
    while 7 + 1 + MIN_NAME + 1 + badge_w + 1 + right_len(&items) + 1 > width {
        let Some(drop) = items
            .iter()
            .enumerate()
            .filter(|(_, (p, _))| *p > 0)
            .max_by_key(|(_, (p, _))| *p)
            .map(|(i, _)| i)
        else {
            break;
        };
        items.remove(drop);
    }
    // Items sit in the order they were added, not the drop order.
    let right: Vec<Span> = items.into_iter().map(|(_, s)| s).collect();
    let right_len: usize = right.iter().map(|s| s.content.chars().count()).sum();

    // The same columns as the loop above counts, without the name.
    let fixed = 7 + 1 + 1 + badge_w + 1 + right_len + 1;
    let name_w = width.saturating_sub(fixed).clamp(MIN_NAME, 40);
    let mut spans = vec![
        Span::styled(
            " avida ",
            Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED),
        ),
        Span::raw(" "),
        Span::styled(
            trunc(&app.name, name_w),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            text,
            Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];
    // Right-align the status by padding.
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let pad = width.saturating_sub(used + right_len + 1);
    spans.push(Span::raw(" ".repeat(pad)));
    spans.extend(right);
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_tabs(f: &mut Frame, app: &mut App, area: Rect) {
    let mut spans = Vec::new();
    let mut x = area.x;
    let mut hits = Vec::new();
    spans.push(Span::raw(" "));
    x += 1;
    for (i, s) in Screen::TABS.iter().enumerate() {
        let label = format!("{} {}", s.key(), s.title());
        let w = label.chars().count() as u16;
        let style = if *s == app.screen {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
        };
        if x + w > area.x + area.width {
            break;
        }
        spans.push(Span::styled(label, style));
        hits.push((Rect::new(x, area.y, w, 1), *s));
        x += w;
        if i + 1 < Screen::TABS.len() {
            spans.push(Span::raw(" "));
            x += 1;
        }
    }
    app.tab_hits = hits;
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The actions a screen's footer advertises, with a short word each. The
/// keys come from the binding table, so a rebinding changes the footer too.
fn hint_actions(screen: Screen) -> &'static [(Action, &'static str)] {
    use Action::*;
    match screen {
        Screen::Map => &[
            (RunPause, "run/pause"),
            (StepUpdate, "step"),
            (StepSlice, "slice"),
            (NextLayer, "layer"),
            (ToggleScale, "scale"),
            (Enter, "inspect"),
        ],
        Screen::Stats => &[
            (RunPause, "run/pause"),
            (StepUpdate, "step"),
            (StepSlice, "slice"),
            (Faster, "speed"),
        ],
        Screen::Charts => &[
            (Dir(0, -1), "choose series"),
            (Enter, "toggle"),
            (RunPause, "run/pause"),
        ],
        Screen::Genotypes => &[
            (GenoView, "view"),
            (GenoLineage, "lineage"),
            (Enter, "find on map"),
            (PageDown, "page"),
        ],
        Screen::Zoom => &[
            (Dir(0, -1), "move"),
            (PageDown, "memory"),
            (CycleThread, "thread"),
            (ExtractGenome, "extract"),
            (StepSlice, "slice"),
        ],
        Screen::Debug => &[
            (DebugStep, "step"),
            (DebugBreakpoint, "breakpoint"),
            (DebugRun, "run to bp"),
            (DebugFollowOffspring, "offspring"),
            (DebugFollowIp, "IP"),
            (CycleThread, "thread"),
            (DebugCycleSpace, "space"),
        ],
        Screen::Environment => &[(Dir(1, 0), "switch list"), (Dir(0, 1), "scroll")],
        Screen::Histogram => &[
            (NextLayer, "layer"),
            (ToggleScale, "scale"),
            (RunPause, "run/pause"),
        ],
        Screen::Log => &[
            (PageDown, "scroll"),
            (LogProblemsOnly, "warnings only"),
            (End, "follow"),
            (Home, "top"),
        ],
        Screen::Config => &[
            (ConfigFilter, "filter"),
            (ConfigEdit, "edit"),
            (ConfigRevert, "drop"),
            (ConfigChangedOnly, "changed"),
            (Restart, "restart"),
        ],
        Screen::Options => &[
            (Faster, "speed"),
            (CyclePalette, "palette"),
            (SavePopulation, "save population"),
        ],
        Screen::Blank => &[(Goto(Screen::Map), "map"), (Goto(Screen::Stats), "stats")],
    }
}

/// The footer's key hints as (key, word) pairs: the screen's own, then
/// the palette and help.
fn hints(screen: Screen, can_advance: bool) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    if !can_advance {
        // A finished or failed run cannot run on; the useful key is restart.
        out.push((first_key(Action::Restart, screen), "restart".to_string()));
    }
    out.extend(
        hint_actions(screen)
            .iter()
            .filter(|(a, _)| {
                can_advance
                    || !matches!(a, Action::RunPause | Action::StepUpdate | Action::StepSlice)
            })
            .map(|&(a, word)| (first_key(a, screen), word.to_string())),
    );
    out.push((
        first_key(Action::OpenPalette, screen),
        "commands".to_string(),
    ));
    out.push((first_key(Action::Help, screen), "help".to_string()));
    out
}

/// The hints that fit in `w` columns, as spans (keys bold). Whole hints are
/// dropped from the end of the screen's own list, never cut in half; help
/// stays as long as anything does.
fn hint_spans(screen: Screen, can_advance: bool, w: usize) -> Vec<Span<'static>> {
    let mut hints = hints(screen, can_advance);
    let width = |h: &[(String, String)]| -> usize {
        1 + h
            .iter()
            .map(|(k, v)| k.chars().count() + 1 + v.chars().count() + 2)
            .sum::<usize>()
            .saturating_sub(2)
    };
    // Drop the last screen-specific hint first, then "commands".
    while width(&hints) > w && hints.len() > 1 {
        let at = if hints.len() > 2 { hints.len() - 3 } else { 0 };
        hints.remove(at);
    }
    let mut spans = vec![Span::raw(" ")];
    for (i, (k, v)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(k.clone(), BOLD));
        spans.push(Span::styled(format!(" {v}"), DIM));
    }
    spans
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let w = usize::from(area.width);
    if app.confirm_quit {
        f.render_widget(
            Paragraph::new(" Quit avida-tui? (y/n)")
                .style(Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)),
            area,
        );
        return;
    }
    if app.confirm_restart {
        let n = app.config.overrides.len();
        let text = format!(
            " Restart the run, discarding this one{}? (y/n)",
            if n == 0 {
                String::new()
            } else {
                format!(
                    ", with {n} changed setting{}",
                    if n == 1 { "" } else { "s" }
                )
            }
        );
        f.render_widget(
            Paragraph::new(trunc(&text, w))
                .style(Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)),
            area,
        );
        return;
    }
    if let Some(t) = app.toasts.last() {
        let style = match t.kind {
            ToastKind::Info => Style::new().fg(hue(Hue::Cyan)),
            ToastKind::Success => Style::new().fg(hue(Hue::Green)),
            ToastKind::Error => Style::new().fg(hue(Hue::Red)),
        };
        f.render_widget(
            Paragraph::new(trunc(&format!(" {}", t.text), w)).style(style),
            area,
        );
        return;
    }
    if let Some(p) = &app.playback {
        f.render_widget(Paragraph::new(scrub_line(p, w)), area);
        return;
    }
    f.render_widget(
        Paragraph::new(Line::from(hint_spans(app.screen, app.can_advance(), w))),
        area,
    );
}

/// The footer of a playback: where the frame on show sits in the
/// recording, and the keys that move it.
pub fn scrub_line(p: &avida_session::PlaybackInfo, width: usize) -> String {
    let keys = " , . step  { } 10  < > ends  Space play  ? help";
    let label = format!(" frame {}/{} ", p.index + 1, p.frames);
    let bar_w = width
        .saturating_sub(label.chars().count() + keys.chars().count() + 2)
        .clamp(8, 40);
    let last = p.frames.saturating_sub(1).max(1);
    let at = (p.index * (bar_w - 1)) / last;
    let bar: String = (0..bar_w)
        .map(|i| match i.cmp(&at) {
            std::cmp::Ordering::Less => '=',
            std::cmp::Ordering::Equal => '#',
            std::cmp::Ordering::Greater => '-',
        })
        .collect();
    trunc(&format!("[{bar}]{label}{keys}"), width)
}

/// What a screen says before the simulation has published its first view.
pub(crate) const FIRST_FRAME: &str =
    " Starting the simulation. The first view appears in a moment.";

/// Clears `area` and draws a centred message (used by screens that need a
/// frame before they can show anything).
pub(crate) fn waiting(f: &mut Frame, area: Rect, what: &str) {
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(what.to_string()).style(DIM), area);
}

#[cfg(test)]
mod tests;
