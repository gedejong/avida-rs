//! A screen sweep for design review: every screen, overlay and run state of
//! the terminal interface, at 80x24, 100x30 and 160x50, in each colour mode
//! (24-bit, 256 colours, none). It asserts little; it writes, under
//! `$CARGO_TARGET_TMPDIR/tui-screens/<mode>/<size>/`, one plain text file
//! and one file with ANSI colour escapes per state, and under
//! `$CARGO_TARGET_TMPDIR/tui-screens/contrast-<mode>.txt` the colour pairs the
//! interface draws whose contrast is below 4.5:1.
//!
//! It runs a short experiment, so it is left out of the default run:
//! `cargo test -p avida-tui --test screens -- --ignored --nocapture`.

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use avida_session::{RunState, SessionEvent};
use avida_tui::actions::Action;
use avida_tui::app::{EnvTab, GenoView, Screen};
use avida_tui::color::{set_theme, Background, ColorMode};
use avida_tui::runner::Driver;
use avida_tui::ui;
use common::fresh_config;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;

const SIZES: [(u16, u16); 3] = [(80, 24), (100, 30), (160, 50)];
/// Directory name, colour depth and the terminal background the hues are
/// chosen for (the interface never paints the background itself).
const MODES: [(&str, ColorMode, Background); 7] = [
    ("truecolor", ColorMode::TrueColor, Background::Dark),
    ("256", ColorMode::Indexed256, Background::Dark),
    ("16", ColorMode::Ansi16, Background::Dark),
    ("truecolor-light", ColorMode::TrueColor, Background::Light),
    ("256-light", ColorMode::Indexed256, Background::Light),
    ("16-light", ColorMode::Ansi16, Background::Light),
    ("mono", ColorMode::Mono, Background::Dark),
];

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("tui-screens")
}

// --- driving -------------------------------------------------------------

fn wait(d: &mut Driver, what: &str, cond: impl Fn(&Driver) -> bool) {
    let limit = Instant::now() + Duration::from_secs(180);
    loop {
        d.pump();
        if cond(d) {
            return;
        }
        assert!(Instant::now() < limit, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Pumps until the frame count has not changed for a while.
fn settle(d: &mut Driver) {
    let mut last = d.app.frames_applied;
    let mut quiet = 0;
    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(30));
        d.pump();
        if d.app.frames_applied == last {
            quiet += 1;
            if quiet >= 4 {
                return;
            }
        } else {
            quiet = 0;
            last = d.app.frames_applied;
        }
    }
}

fn press(d: &mut Driver, code: KeyCode) {
    d.app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    d.pump();
}

fn typed(d: &mut Driver, text: &str) {
    for c in text.chars() {
        press(d, KeyCode::Char(c));
    }
}

fn go(d: &mut Driver, screen: Screen) {
    d.app.goto(screen);
    d.pump();
    settle(d);
}

// --- recording -----------------------------------------------------------

struct Shots {
    mode: &'static str,
    light: bool,
    count: usize,
    terminals: Vec<(u16, u16, Terminal<TestBackend>)>,
    pairs: BTreeMap<(String, String), String>,
}

impl Shots {
    fn new(mode: &'static str, light: bool) -> Shots {
        let terminals = SIZES
            .iter()
            .map(|&(w, h)| (w, h, Terminal::new(TestBackend::new(w, h)).unwrap()))
            .collect();
        Shots {
            mode,
            light,
            count: 0,
            terminals,
            pairs: BTreeMap::new(),
        }
    }

    /// Draws the app at every size and writes the files.
    fn shot(&mut self, d: &mut Driver, name: &str) {
        self.count += 1;
        let file = format!("{:02}-{name}", self.count);
        for (w, h, t) in &mut self.terminals {
            let dir = out_dir().join(self.mode).join(format!("{w}x{h}"));
            fs::create_dir_all(&dir).unwrap();
            let done = t.draw(|f| ui::draw(f, &mut d.app)).unwrap();
            let buf = done.buffer.clone();
            fs::write(dir.join(format!("{file}.txt")), plain(&buf)).unwrap();
            fs::write(dir.join(format!("{file}.ansi")), ansi(&buf)).unwrap();
            if self.mode != "mono" {
                collect_pairs(
                    &buf,
                    self.light,
                    &format!("{name} {w}x{h}"),
                    &mut self.pairs,
                );
            }
        }
    }
}

fn plain(buf: &Buffer) -> String {
    let a = buf.area;
    let mut out = String::new();
    for y in 0..a.height {
        let mut line = String::new();
        for x in 0..a.width {
            line.push_str(buf[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn sgr_colour(c: Color, bg: bool) -> Option<String> {
    let base = if bg { 40 } else { 30 };
    let n = |i: u8, bright: bool| {
        let b = if bright { base + 60 } else { base };
        Some((b + i).to_string())
    };
    match c {
        Color::Reset => None,
        Color::Black => n(0, false),
        Color::Red => n(1, false),
        Color::Green => n(2, false),
        Color::Yellow => n(3, false),
        Color::Blue => n(4, false),
        Color::Magenta => n(5, false),
        Color::Cyan => n(6, false),
        Color::Gray => n(7, false),
        Color::DarkGray => n(0, true),
        Color::LightRed => n(1, true),
        Color::LightGreen => n(2, true),
        Color::LightYellow => n(3, true),
        Color::LightBlue => n(4, true),
        Color::LightMagenta => n(5, true),
        Color::LightCyan => n(6, true),
        Color::White => n(7, true),
        Color::Rgb(r, g, b) => Some(format!("{};2;{r};{g};{b}", if bg { 48 } else { 38 })),
        Color::Indexed(i) => Some(format!("{};5;{i}", if bg { 48 } else { 38 })),
    }
}

fn ansi(buf: &Buffer) -> String {
    let a = buf.area;
    let mut out = String::new();
    for y in 0..a.height {
        let mut prev = None;
        for x in 0..a.width {
            let c = &buf[(x, y)];
            let key = (c.fg, c.bg, c.modifier);
            if prev != Some(key) {
                let mut codes = vec!["0".to_string()];
                for (m, code) in [
                    (Modifier::BOLD, "1"),
                    (Modifier::DIM, "2"),
                    (Modifier::ITALIC, "3"),
                    (Modifier::UNDERLINED, "4"),
                    (Modifier::REVERSED, "7"),
                ] {
                    if c.modifier.contains(m) {
                        codes.push(code.to_string());
                    }
                }
                codes.extend(sgr_colour(c.fg, false));
                codes.extend(sgr_colour(c.bg, true));
                let _ = write!(out, "\x1b[{}m", codes.join(";"));
                prev = Some(key);
            }
            out.push_str(c.symbol());
        }
        out.push_str("\x1b[0m\n");
    }
    out
}

// --- contrast ------------------------------------------------------------

/// The xterm default values of the sixteen terminal colours.
fn rgb_of(c: Color) -> Option<[u8; 3]> {
    Some(match c {
        Color::Black => [0, 0, 0],
        Color::Red => [205, 0, 0],
        Color::Green => [0, 205, 0],
        Color::Yellow => [205, 205, 0],
        Color::Blue => [0, 0, 238],
        Color::Magenta => [205, 0, 205],
        Color::Cyan => [0, 205, 205],
        Color::Gray => [229, 229, 229],
        Color::DarkGray => [127, 127, 127],
        Color::LightRed => [255, 0, 0],
        Color::LightGreen => [0, 255, 0],
        Color::LightYellow => [255, 255, 0],
        Color::LightBlue => [92, 92, 255],
        Color::LightMagenta => [255, 0, 255],
        Color::LightCyan => [0, 255, 255],
        Color::White => [255, 255, 255],
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Indexed(i) => return Some(indexed_rgb(i)),
        Color::Reset => return None,
    })
}

fn indexed_rgb(i: u8) -> [u8; 3] {
    const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match i {
        0..=15 => {
            let named = [
                Color::Black,
                Color::Red,
                Color::Green,
                Color::Yellow,
                Color::Blue,
                Color::Magenta,
                Color::Cyan,
                Color::Gray,
                Color::DarkGray,
                Color::LightRed,
                Color::LightGreen,
                Color::LightYellow,
                Color::LightBlue,
                Color::LightMagenta,
                Color::LightCyan,
                Color::White,
            ];
            rgb_of(named[usize::from(i)]).unwrap_or([0, 0, 0])
        }
        16..=231 => {
            let k = usize::from(i) - 16;
            [CUBE[k / 36], CUBE[k / 6 % 6], CUBE[k % 6]]
        }
        _ => {
            let v = 8 + 10 * (i - 232);
            [v, v, v]
        }
    }
}

fn lum(c: [u8; 3]) -> f64 {
    let f = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])
}

fn ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Text cells (not blank, not box drawing or block characters) whose
/// foreground is explicit: against an explicit background, or, with the
/// terminal's own background, against black and against white.
fn collect_pairs(
    buf: &Buffer,
    light: bool,
    at: &str,
    pairs: &mut BTreeMap<(String, String), String>,
) {
    let a = buf.area;
    for y in 0..a.height {
        for x in 0..a.width {
            let c = &buf[(x, y)];
            let s = c.symbol();
            let Some(ch) = s.chars().next() else { continue };
            if !ch.is_alphanumeric() {
                continue;
            }
            let Some(fg) = rgb_of(c.fg) else { continue };
            let reversed = c.modifier.contains(Modifier::REVERSED);
            if reversed {
                continue;
            }
            let backs: Vec<(&str, [u8; 3])> = match rgb_of(c.bg) {
                Some(bg) => vec![("its background", bg)],
                None if c.bg == Color::Reset => {
                    if light {
                        vec![("white terminal", [255, 255, 255])]
                    } else {
                        vec![("black terminal", [0, 0, 0])]
                    }
                }
                None => continue,
            };
            for (what, bg) in backs {
                let r = ratio(fg, bg);
                if r < 4.5 {
                    let key = (
                        format!("{:?} on {:?} ({what})", c.fg, c.bg),
                        format!("{r:.2}:1"),
                    );
                    pairs
                        .entry(key)
                        .or_insert_with(|| format!("{at}, e.g. {s:?} at {x},{y}"));
                }
            }
        }
    }
}

// --- the sweep -----------------------------------------------------------

fn main_screens(s: &mut Shots, d: &mut Driver, tag: &str) {
    for screen in [
        Screen::Map,
        Screen::Stats,
        Screen::Charts,
        Screen::Genotypes,
        Screen::Zoom,
        Screen::Debug,
        Screen::Environment,
        Screen::Histogram,
        Screen::Log,
        Screen::Config,
        Screen::Options,
        Screen::Blank,
    ] {
        go(d, screen);
        s.shot(d, &format!("{tag}-{}", screen.title().to_lowercase()));
    }
    go(d, Screen::Map);
}

fn sweep(mode: &'static str, color: ColorMode, background: Background) {
    set_theme(background, color);
    let dir = fresh_config(&format!("screens_{mode}"));
    let mut d = Driver::start(dir, Vec::new(), color).unwrap_or_else(|e| panic!("start: {e}"));
    d.freeze_clock(0.0);
    let mut s = Shots::new(mode, background == Background::Light);

    // The window before the world has published anything.
    let mut early = Driver::start(
        fresh_config(&format!("screens_{mode}_early")),
        Vec::new(),
        color,
    )
    .unwrap();
    early.freeze_clock(0.0);
    early.app.frame = None;
    s.shot(&mut early, "start-no-frame");
    early.app.goto(Screen::Stats);
    s.shot(&mut early, "start-no-frame-stats");
    early.app.goto(Screen::Zoom);
    s.shot(&mut early, "start-no-frame-zoom");
    early.shutdown(Duration::from_secs(5));

    wait(&mut d, "the first frame", |d| {
        d.app.frame.as_ref().is_some_and(|f| f.map.is_some())
    });
    settle(&mut d);
    d.app.select(Some(1830));
    d.pump();
    settle(&mut d);
    main_screens(&mut s, &mut d, "ready");

    // Three single steps, then a run to about update 40.
    for _ in 0..3 {
        d.app.perform(Action::StepUpdate, false);
        d.pump();
    }
    wait(&mut d, "three updates", |d| d.app.history.len() >= 3);
    settle(&mut d);
    s.shot(&mut d, "stepped-map");
    d.app.perform(Action::RunPause, false);
    wait(&mut d, "update 40", |d| {
        d.app.current_update().unwrap_or(0) >= 40
    });
    d.app.perform(Action::RunPause, false);
    wait(&mut d, "pause", |d| d.app.run_state() == RunState::Paused);
    settle(&mut d);
    d.app.perform(Action::ChooseNext, false);
    d.pump();
    settle(&mut d);
    assert!(
        d.app.frame.as_ref().is_some_and(|f| f.inspect.is_some()),
        "an organism is inspected"
    );
    main_screens(&mut s, &mut d, "paused");

    // Map layers, scale, zoom.
    go(&mut d, Screen::Map);
    let layers = d.app.layers();
    for (i, l) in layers.iter().enumerate() {
        d.app.set_layer(*l);
        d.pump();
        settle(&mut d);
        s.shot(&mut d, &format!("map-layer{i:02}"));
    }
    d.app.perform(Action::ToggleScale, false);
    settle(&mut d);
    s.shot(&mut d, "map-last-layer-scale-toggled");
    d.app.set_layer(layers[0]);
    d.app.perform(Action::BigCells, false);
    settle(&mut d);
    s.shot(&mut d, "map-big-cells");
    d.app.perform(Action::BigCells, false);
    d.app.perform(Action::CyclePalette, false);
    settle(&mut d);
    s.shot(&mut d, "map-palette-two");
    d.app.perform(Action::CyclePalette, false);
    d.app.cursor = (5, 5);
    settle(&mut d);
    s.shot(&mut d, "map-cursor-on-empty-cell");

    // Genotypes in their three views.
    go(&mut d, Screen::Genotypes);
    for (i, v) in [GenoView::Table, GenoView::Abundance, GenoView::Lineage]
        .into_iter()
        .enumerate()
    {
        d.app.geno_view = v;
        settle(&mut d);
        s.shot(&mut d, &format!("genotypes-view{i}"));
    }
    d.app.geno_view = GenoView::Table;

    // Charts: more series, log scale.
    go(&mut d, Screen::Charts);
    for _ in 0..4 {
        press(&mut d, KeyCode::Down);
        press(&mut d, KeyCode::Enter);
    }
    s.shot(&mut d, "charts-five-series");
    d.app.perform(Action::ToggleScale, false);
    s.shot(&mut d, "charts-log-scale");
    d.app.perform(Action::ToggleScale, false);

    // Environment tabs.
    go(&mut d, Screen::Environment);
    for (i, t) in EnvTab::ALL.into_iter().enumerate() {
        d.app.env_tab = t;
        s.shot(&mut d, &format!("env-tab{i}"));
    }
    d.app.env_tab = EnvTab::Resources;

    // Debugger: a step, a breakpoint, the offspring.
    go(&mut d, Screen::Debug);
    press(&mut d, KeyCode::Char('i'));
    settle(&mut d);
    s.shot(&mut d, "debug-after-step");
    for _ in 0..4 {
        press(&mut d, KeyCode::Down);
    }
    press(&mut d, KeyCode::Char('b'));
    s.shot(&mut d, "debug-breakpoint");
    press(&mut d, KeyCode::Char('g'));
    settle(&mut d);
    s.shot(&mut d, "debug-run-to-breakpoint");
    go(&mut d, Screen::Zoom);
    d.app.zoom_scroll = 12;
    s.shot(&mut d, "zoom-scrolled");
    d.app.zoom_scroll = 0;

    // Configuration: filter, edit, changed only, restart prompt.
    go(&mut d, Screen::Config);
    press(&mut d, KeyCode::Char('/'));
    s.shot(&mut d, "config-filter-empty");
    typed(&mut d, "WORLD");
    s.shot(&mut d, "config-filter-typed");
    press(&mut d, KeyCode::Enter);
    s.shot(&mut d, "config-filtered");
    press(&mut d, KeyCode::Char('e'));
    s.shot(&mut d, "config-edit");
    typed(&mut d, "x");
    s.shot(&mut d, "config-edit-bad-value");
    press(&mut d, KeyCode::Backspace);
    press(&mut d, KeyCode::Backspace);
    typed(&mut d, "30");
    press(&mut d, KeyCode::Enter);
    s.shot(&mut d, "config-edited");
    press(&mut d, KeyCode::Char('c'));
    s.shot(&mut d, "config-changed-only");
    press(&mut d, KeyCode::Char('R'));
    s.shot(&mut d, "config-restart-prompt");
    press(&mut d, KeyCode::Char('n'));
    press(&mut d, KeyCode::Delete);
    press(&mut d, KeyCode::Char('c'));
    press(&mut d, KeyCode::Esc);

    // Log, with warnings only.
    go(&mut d, Screen::Log);
    press(&mut d, KeyCode::Char('v'));
    s.shot(&mut d, "log-warnings-only");
    press(&mut d, KeyCode::Char('v'));
    press(&mut d, KeyCode::Home);
    s.shot(&mut d, "log-top");
    press(&mut d, KeyCode::End);

    // Overlays and prompts, over the map.
    go(&mut d, Screen::Map);
    d.app.show_help = true;
    s.shot(&mut d, "help");
    d.app.help_scroll = 12;
    s.shot(&mut d, "help-scrolled");
    d.app.help_scroll = 10_000;
    s.shot(&mut d, "help-scrolled-to-end");
    d.app.help_scroll = 0;
    d.app.show_help = false;
    press(&mut d, KeyCode::Char(':'));
    s.shot(&mut d, "palette-empty");
    typed(&mut d, "gen");
    s.shot(&mut d, "palette-filtered");
    typed(&mut d, "zzzz");
    s.shot(&mut d, "palette-no-match");
    press(&mut d, KeyCode::Esc);
    d.app.confirm_quit = true;
    s.shot(&mut d, "confirm-quit");
    d.app.confirm_quit = false;
    d.app.notify(
        avida_tui::app::ToastKind::Success,
        "Population saved to /tmp/example/detail.spop".into(),
    );
    s.shot(&mut d, "toast-success");
    d.app.notify(
        avida_tui::app::ToastKind::Error,
        "Cannot write /tmp/example: permission denied".into(),
    );
    s.shot(&mut d, "toast-error");
    d.app.toasts.clear();

    // Running: slow it down so the state holds still.
    for _ in 0..8 {
        d.app.perform(Action::Slower, false);
    }
    let before = d.app.current_update().unwrap_or(0);
    d.app.perform(Action::RunPause, false);
    wait(&mut d, "a running update", |d| {
        d.app.run_state() == RunState::Running && d.app.current_update().unwrap_or(0) > before
    });
    s.shot(&mut d, "running-map");
    go(&mut d, Screen::Stats);
    s.shot(&mut d, "running-stats");
    go(&mut d, Screen::Charts);
    s.shot(&mut d, "running-charts");
    go(&mut d, Screen::Map);
    d.app.perform(Action::RunPause, false);
    wait(&mut d, "pause", |d| d.app.run_state() == RunState::Paused);

    // Finished.
    for _ in 0..12 {
        d.app.perform(Action::Faster, false);
    }
    d.app.perform(Action::RunPause, false);
    wait(&mut d, "the run to finish", |d| {
        d.app.run_state() == RunState::Finished
    });
    settle(&mut d);
    main_screens(&mut s, &mut d, "finished");
    d.app.perform(Action::RunPause, false);
    s.shot(&mut d, "finished-run-key-pressed");

    // Failed: the session reports a fatal error.
    d.app.apply_events(vec![SessionEvent::Fatal(
        "unsupported: SLICING_METHOD=4 (C++: cPopulation::ProcessStep)\nThe run cannot go on."
            .to_string(),
    )]);
    d.pump();
    for screen in [Screen::Map, Screen::Stats, Screen::Log, Screen::Options] {
        go(&mut d, screen);
        s.shot(&mut d, &format!("failed-{}", screen.title().to_lowercase()));
    }
    d.app.show_help = true;
    s.shot(&mut d, "failed-help");

    d.shutdown(Duration::from_secs(5));

    if mode != "mono" {
        let mut text = String::from(
            "Pairs of explicit colours whose contrast is below 4.5:1 (named colours at their xterm values).\n\
             Terminal-default backgrounds are tested against black (dark modes) or white (light modes).\n\n",
        );
        for ((pair, r), at) in &s.pairs {
            let _ = writeln!(text, "{r:>8}  {pair}  {at}");
        }
        fs::write(out_dir().join(format!("contrast-{mode}.txt")), text).unwrap();
    }
}

#[test]
#[ignore = "a screen sweep for design review; run it explicitly"]
fn every_screen_in_every_colour_mode_and_size() {
    let _ = fs::remove_dir_all(out_dir());
    fs::create_dir_all(out_dir()).unwrap();
    for (name, mode, background) in MODES {
        sweep(name, mode, background);
    }
    eprintln!("wrote {}", out_dir().display());
}
