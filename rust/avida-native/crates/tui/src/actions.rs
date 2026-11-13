//! The one table of key bindings.
//!
//! Every key the application reacts to is a row of [`BINDINGS`]. Input
//! handling resolves keys through [`lookup`], the command palette lists the
//! rows with `palette` set, and the help overlay prints all of them, so the
//! three cannot drift apart: a key that does something is in the table or
//! it does nothing.

use avida_session::LayerId;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::Screen;

/// A key as the table writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    /// Control plus a lowercase letter.
    Ctrl(char),
    Code(KeyCode),
}

impl Key {
    /// The table's view of a key event (`None` for keys no row can name).
    pub fn from_event(ev: &KeyEvent) -> Option<Key> {
        match ev.code {
            KeyCode::Char(c) if ev.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Key::Ctrl(c.to_ascii_lowercase()))
            }
            KeyCode::Char(c) => Some(Key::Char(c)),
            code => Some(Key::Code(code)),
        }
    }

    pub fn label(self) -> String {
        match self {
            Key::Char(' ') => "Space".to_string(),
            Key::Char(c) => c.to_string(),
            Key::Ctrl(c) => format!("Ctrl-{}", c.to_ascii_uppercase()),
            Key::Code(code) => match code {
                KeyCode::Up => "Up".into(),
                KeyCode::Down => "Down".into(),
                KeyCode::Left => "Left".into(),
                KeyCode::Right => "Right".into(),
                KeyCode::PageUp => "PgUp".into(),
                KeyCode::PageDown => "PgDn".into(),
                KeyCode::Home => "Home".into(),
                KeyCode::End => "End".into(),
                KeyCode::Enter => "Enter".into(),
                KeyCode::Esc => "Esc".into(),
                KeyCode::Tab => "Tab".into(),
                KeyCode::BackTab => "S-Tab".into(),
                KeyCode::Delete => "Del".into(),
                KeyCode::Backspace => "Bksp".into(),
                KeyCode::F(n) => format!("F{n}"),
                other => format!("{other:?}"),
            },
        }
    }
}

/// The keys of a row as a short label. A letter that appears in both cases
/// is written once, in capitals (the viewer ignored case).
pub fn keys_label(keys: &[Key]) -> String {
    let mut out: Vec<String> = Vec::new();
    for k in keys {
        if let Key::Char(c) = k {
            let capital = c.to_uppercase().next().unwrap_or(*c);
            if c.is_lowercase() && keys.contains(&Key::Char(capital)) {
                // Written with its capital.
                continue;
            }
        }
        out.push(k.label());
    }
    out.join(" ")
}

/// Where a row applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Global,
    /// Only on these screens, where it takes precedence over a global row
    /// with the same key.
    Screens(&'static [Screen]),
}

/// Something the user can ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    RunPause,
    StepUpdate,
    StepSlice,
    Faster,
    Slower,
    SavePopulation,
    ExtractGenome,
    Restart,
    Goto(Screen),
    NextScreen,
    PrevScreen,
    Back,
    ChooseNext,
    CyclePalette,
    ToggleScale,
    BigCells,
    PrevLayer,
    NextLayer,
    /// Palette only: a particular map layer.
    Layer(LayerId),
    Dir(i32, i32),
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    CycleThread,
    DebugStep,
    DebugRun,
    DebugBreakpoint,
    DebugClearBreakpoints,
    DebugFollowOffspring,
    DebugFollowIp,
    DebugCycleSpace,
    ConfigFilter,
    ConfigEdit,
    ConfigChangedOnly,
    ConfigRevert,
    GenoView,
    GenoLineage,
    LogProblemsOnly,
    Analyze,
    OpenPalette,
    Help,
    Quit,
    QuitNow,
    Redraw,
}

/// One row of the table.
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub action: Action,
    /// Help heading.
    pub section: &'static str,
    pub title: &'static str,
    pub keys: &'static [Key],
    pub scope: Scope,
    /// Listed in the command palette.
    pub palette: bool,
    /// Extra words the palette's search matches.
    pub keywords: &'static str,
}

/// The debugger step's title on hardware whose CPU cycle runs several
/// micro-operations (BCR, GP8), where a step can move the IP more than one
/// position. Elsewhere a step is one instruction, as the table says.
pub const DEBUG_STEP_MICRO_OPS: &str = "Step one CPU cycle of several micro-operations";

/// The title `b` shows when the watched organism's CPU cycle runs several
/// micro-operations (`micro_ops`) or not.
pub fn title_for(b: &Binding, micro_ops: bool) -> &'static str {
    if micro_ops && b.action == Action::DebugStep {
        DEBUG_STEP_MICRO_OPS
    } else {
        b.title
    }
}

const fn c(ch: char) -> Key {
    Key::Char(ch)
}

const fn code(k: KeyCode) -> Key {
    Key::Code(k)
}

const fn row(
    action: Action,
    section: &'static str,
    title: &'static str,
    keys: &'static [Key],
    scope: Scope,
    palette: bool,
    keywords: &'static str,
) -> Binding {
    Binding {
        action,
        section,
        title,
        keys,
        scope,
        palette,
        keywords,
    }
}

const G: Scope = Scope::Global;
const DEBUG: Scope = Scope::Screens(&[Screen::Debug]);
const CONFIG: Scope = Scope::Screens(&[Screen::Config]);
const GENO: Scope = Scope::Screens(&[Screen::Genotypes]);
const LOG: Scope = Scope::Screens(&[Screen::Log]);
const THREADS: Scope = Scope::Screens(&[Screen::Zoom, Screen::Debug]);

/// Every binding, in help order.
pub static BINDINGS: &[Binding] = &[
    row(
        Action::RunPause,
        "Run",
        "Run or pause",
        &[c(' '), c('P')],
        G,
        true,
        "start stop play resume",
    ),
    row(
        Action::StepUpdate,
        "Run",
        "Step one update",
        &[c('.'), c('N'), c('n')],
        G,
        true,
        "next",
    ),
    row(
        Action::StepSlice,
        "Run",
        "Step one time slice",
        &[c(',')],
        G,
        true,
        "",
    ),
    row(
        Action::Faster,
        "Run",
        "Faster",
        &[c('+'), c('=')],
        G,
        true,
        "speed",
    ),
    row(
        Action::Slower,
        "Run",
        "Slower",
        &[c('-'), c('_')],
        G,
        true,
        "speed",
    ),
    row(
        Action::SavePopulation,
        "Run",
        "Save population",
        &[c('W'), c('w')],
        G,
        true,
        "write spop",
    ),
    row(
        Action::ExtractGenome,
        "Run",
        "Extract the inspected genome to a file",
        &[c('X'), c('x')],
        G,
        true,
        "org write",
    ),
    row(
        Action::Restart,
        "Run",
        "Restart the run (with config overrides)",
        &[c('R'), c('r')],
        G,
        true,
        "reload again reset new seed",
    ),
    row(
        Action::Goto(Screen::Map),
        "Screens",
        "Go to the map",
        &[c('M'), c('m')],
        G,
        true,
        "world cells",
    ),
    row(
        Action::Goto(Screen::Stats),
        "Screens",
        "Go to statistics",
        &[c('S'), c('s')],
        G,
        true,
        "numbers tasks",
    ),
    row(
        Action::Goto(Screen::Charts),
        "Screens",
        "Go to charts",
        &[c('G'), c('g')],
        G,
        true,
        "plot graph series",
    ),
    row(
        Action::Goto(Screen::Genotypes),
        "Screens",
        "Go to genotypes, abundance and lineage",
        &[c('T'), c('t')],
        G,
        true,
        "table phylogeny ancestry",
    ),
    row(
        Action::Goto(Screen::Zoom),
        "Screens",
        "Go to the organism inspector",
        &[c('Z'), c('z')],
        G,
        true,
        "phenotype cpu memory",
    ),
    row(
        Action::Goto(Screen::Debug),
        "Screens",
        "Go to the organism debugger",
        &[c('U'), c('u')],
        G,
        true,
        "debug step breakpoint registers",
    ),
    row(
        Action::Goto(Screen::Environment),
        "Screens",
        "Go to the environment",
        &[c('E'), c('e')],
        G,
        true,
        "resources reactions tasks",
    ),
    row(
        Action::Goto(Screen::Histogram),
        "Screens",
        "Go to the histogram",
        &[c('H')],
        G,
        true,
        "distribution",
    ),
    row(
        Action::Goto(Screen::Log),
        "Screens",
        "Go to the log",
        &[c('D'), c('d')],
        G,
        true,
        "messages output",
    ),
    row(
        Action::Goto(Screen::Config),
        "Screens",
        "Go to the configuration",
        &[c('F'), c('f')],
        G,
        true,
        "settings avida.cfg override restart",
    ),
    row(
        Action::Goto(Screen::Options),
        "Screens",
        "Go to the options",
        &[c('O'), c('o')],
        G,
        true,
        "",
    ),
    row(
        Action::Goto(Screen::Blank),
        "Screens",
        "Blank the display",
        &[c('B'), c('b')],
        G,
        true,
        "",
    ),
    row(
        Action::Analyze,
        "Screens",
        "Analyze mode (not in the live viewer)",
        &[c('A'), c('a')],
        G,
        false,
        "",
    ),
    row(
        Action::NextScreen,
        "Screens",
        "Next screen",
        &[code(KeyCode::Tab)],
        G,
        false,
        "",
    ),
    row(
        Action::PrevScreen,
        "Screens",
        "Previous screen",
        &[code(KeyCode::BackTab)],
        G,
        false,
        "",
    ),
    row(
        Action::Back,
        "Screens",
        "Back to the map, or deselect",
        &[code(KeyCode::Esc)],
        G,
        false,
        "",
    ),
    row(
        Action::Dir(0, -1),
        "Move",
        "Up (Shift: 5 cells on the map)",
        &[code(KeyCode::Up), c('k')],
        G,
        false,
        "",
    ),
    row(
        Action::Dir(0, 1),
        "Move",
        "Down",
        &[code(KeyCode::Down), c('j')],
        G,
        false,
        "",
    ),
    row(
        Action::Dir(-1, 0),
        "Move",
        "Left",
        &[code(KeyCode::Left), c('h')],
        G,
        false,
        "",
    ),
    row(
        Action::Dir(1, 0),
        "Move",
        "Right",
        &[code(KeyCode::Right), c('l')],
        G,
        false,
        "",
    ),
    row(
        Action::PageUp,
        "Move",
        "Page up",
        &[code(KeyCode::PageUp)],
        G,
        false,
        "",
    ),
    row(
        Action::PageDown,
        "Move",
        "Page down",
        &[code(KeyCode::PageDown)],
        G,
        false,
        "",
    ),
    row(
        Action::Home,
        "Move",
        "Top (log, genotypes, memory)",
        &[code(KeyCode::Home)],
        G,
        false,
        "",
    ),
    row(
        Action::End,
        "Move",
        "Bottom (log follows)",
        &[code(KeyCode::End)],
        G,
        false,
        "",
    ),
    row(
        Action::Enter,
        "Move",
        "Inspect the cell / pick the row",
        &[code(KeyCode::Enter)],
        G,
        false,
        "",
    ),
    row(
        Action::ChooseNext,
        "Map",
        "Inspect the next organism",
        &[c('C'), c('c')],
        G,
        true,
        "choose select",
    ),
    row(
        Action::PrevLayer,
        "Map",
        "Previous layer",
        &[c('[')],
        G,
        true,
        "colour",
    ),
    row(
        Action::NextLayer,
        "Map",
        "Next layer",
        &[c(']')],
        G,
        true,
        "colour",
    ),
    row(
        Action::CyclePalette,
        "Map",
        "Switch the palette",
        &[c('p')],
        G,
        true,
        "colour color viridis cividis",
    ),
    row(
        Action::ToggleScale,
        "Map",
        "Linear or logarithmic scale",
        &[c('L')],
        G,
        true,
        "log",
    ),
    row(
        Action::BigCells,
        "Map",
        "Wide cells (symbols)",
        &[c('i'), c('I')],
        G,
        true,
        "size",
    ),
    row(
        Action::CycleThread,
        "Inspector",
        "Show the next thread",
        &[c('t')],
        THREADS,
        true,
        "cpu",
    ),
    row(
        Action::DebugStep,
        "Debug",
        "Step the watched organism one instruction",
        &[c('i'), c('I')],
        DEBUG,
        true,
        "instruction next single",
    ),
    row(
        Action::DebugBreakpoint,
        "Debug",
        "Set or clear a breakpoint on the selected line",
        &[c('b'), code(KeyCode::Enter)],
        DEBUG,
        true,
        "toggle",
    ),
    row(
        Action::DebugRun,
        "Debug",
        "Run to the next breakpoint",
        &[c('g')],
        DEBUG,
        true,
        "continue go",
    ),
    row(
        Action::DebugFollowOffspring,
        "Debug",
        "Follow offspring on or off",
        &[c('f')],
        DEBUG,
        true,
        "child divide",
    ),
    row(
        Action::DebugClearBreakpoints,
        "Debug",
        "Clear all breakpoints",
        &[code(KeyCode::Delete)],
        DEBUG,
        true,
        "remove",
    ),
    row(
        Action::DebugFollowIp,
        "Debug",
        "Select the instruction pointer and follow it",
        &[code(KeyCode::Home)],
        DEBUG,
        true,
        "ip",
    ),
    row(
        Action::DebugCycleSpace,
        "Debug",
        "Show the next memory space",
        &[c('y'), c('Y')],
        DEBUG,
        true,
        "memory space parasite offspring",
    ),
    row(
        Action::GenoView,
        "Genotypes",
        "Table, abundance chart or lineage",
        &[c('v')],
        GENO,
        true,
        "view switch",
    ),
    row(
        Action::GenoLineage,
        "Genotypes",
        "Trace the lineage of the selected genotype",
        &[c('l')],
        GENO,
        true,
        "ancestry phylogeny",
    ),
    row(
        Action::LogProblemsOnly,
        "Log",
        "Show only warnings and errors, or everything",
        &[c('v')],
        LOG,
        true,
        "stderr filter problems",
    ),
    row(
        Action::ConfigFilter,
        "Config",
        "Filter the settings",
        &[c('/')],
        CONFIG,
        true,
        "search find",
    ),
    row(
        Action::ConfigEdit,
        "Config",
        "Edit the selected setting",
        &[c('e'), code(KeyCode::Enter)],
        CONFIG,
        true,
        "change value override",
    ),
    row(
        Action::ConfigChangedOnly,
        "Config",
        "Show only changed settings",
        &[c('c')],
        CONFIG,
        true,
        "modified",
    ),
    row(
        Action::ConfigRevert,
        "Config",
        "Drop the override of the selected setting",
        &[code(KeyCode::Delete)],
        CONFIG,
        true,
        "undo",
    ),
    row(
        Action::OpenPalette,
        "Other",
        "Command palette",
        &[c(':'), Key::Ctrl('p')],
        G,
        false,
        "",
    ),
    row(
        Action::Help,
        "Other",
        "This help",
        &[c('?'), code(KeyCode::F(1))],
        G,
        true,
        "keys shortcuts",
    ),
    row(
        Action::Quit,
        "Other",
        "Quit (asks first)",
        &[c('q')],
        G,
        true,
        "exit",
    ),
    row(
        Action::QuitNow,
        "Other",
        "Quit now",
        &[c('Q'), Key::Ctrl('c')],
        G,
        false,
        "",
    ),
    row(
        Action::Redraw,
        "Other",
        "Redraw the screen",
        &[Key::Ctrl('l')],
        G,
        true,
        "refresh clear",
    ),
];

/// What `key` does on `screen`: a row scoped to the screen wins over a
/// global one.
pub fn lookup(screen: Screen, key: Key) -> Option<Action> {
    let mut global = None;
    for b in BINDINGS {
        if !b.keys.contains(&key) {
            continue;
        }
        match b.scope {
            Scope::Screens(s) if s.contains(&screen) => return Some(b.action),
            Scope::Screens(_) => {}
            Scope::Global => {
                global.get_or_insert(b.action);
            }
        }
    }
    global
}

/// The keys that do `action` on `screen`, as a label.
pub fn label_for(action: Action, screen: Option<Screen>) -> String {
    for b in BINDINGS {
        if b.action != action {
            continue;
        }
        if let (Scope::Screens(s), Some(scr)) = (b.scope, screen) {
            if !s.contains(&scr) {
                continue;
            }
        }
        return keys_label(b.keys);
    }
    String::new()
}

/// The first key of the row that does `action` on `screen` (for hints).
pub fn first_key(action: Action, screen: Screen) -> String {
    BINDINGS
        .iter()
        .filter(|b| b.action == action)
        .find(|b| match b.scope {
            Scope::Global => true,
            Scope::Screens(s) => s.contains(&screen),
        })
        .and_then(|b| b.keys.first())
        .map(|k| k.label())
        .unwrap_or_default()
}

/// A row of the command palette.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteItem {
    pub action: Action,
    pub section: &'static str,
    pub title: String,
    pub keys: String,
    pub keywords: &'static str,
}

/// All palette rows: the table's, plus one per map layer.
pub fn palette_items(layers: &[(LayerId, String)]) -> Vec<PaletteItem> {
    let mut items: Vec<PaletteItem> = BINDINGS
        .iter()
        .filter(|b| b.palette)
        .map(|b| PaletteItem {
            action: b.action,
            section: b.section,
            title: b.title.to_string(),
            keys: keys_label(b.keys),
            keywords: b.keywords,
        })
        .collect();
    for (layer, title) in layers {
        items.push(PaletteItem {
            action: Action::Layer(*layer),
            section: "Map",
            title: format!("Layer: {title}"),
            keys: String::new(),
            keywords: "map show colour",
        });
    }
    items
}

/// A fuzzy score of `text` for `query` (`None`: not all letters appear in
/// order). Word starts and runs of consecutive letters score higher,
/// gaps lower; case and spaces in the query are ignored.
pub fn fuzzy(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let mut score = 0;
    let mut qi = 0;
    let mut last: Option<usize> = None;
    for (i, &ch) in t.iter().enumerate() {
        if qi < q.len() && ch == q[qi] {
            let start = i == 0 || !t[i - 1].is_alphanumeric();
            score += 10;
            if start {
                score += 15;
            }
            match last {
                Some(l) if l + 1 == i => score += 12,
                Some(l) => score -= (i - l - 1).min(6) as i32,
                None => score -= i.min(6) as i32,
            }
            last = Some(i);
            qi += 1;
        }
    }
    (qi == q.len()).then_some(score - (t.len() / 8) as i32)
}

/// Indices of `items` matching `query`, best first (ties keep table
/// order). A match in the keywords or the section counts less than one in
/// the title.
pub fn search(items: &[PaletteItem], query: &str) -> Vec<usize> {
    let mut scored: Vec<(i32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| {
            let a = fuzzy(query, &it.title);
            let b = fuzzy(query, it.keywords).map(|s| s - 25);
            let c = fuzzy(query, &format!("{} {}", it.section, it.title)).map(|s| s - 8);
            [a, b, c].into_iter().flatten().max().map(|s| (s, i))
        })
        .collect();
    scored.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}

/// The palette's state while open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaletteState {
    pub query: String,
    pub selected: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_key_is_bound_twice_in_one_scope() {
        let screens = Screen::TABS
            .iter()
            .copied()
            .chain([Screen::Blank])
            .collect::<Vec<_>>();
        let mut seen: Vec<(Key, Option<Screen>)> = Vec::new();
        for b in BINDINGS {
            for &k in b.keys {
                match b.scope {
                    Scope::Global => {
                        assert!(
                            !seen.contains(&(k, None)),
                            "{k:?} is globally bound twice ({})",
                            b.title
                        );
                        seen.push((k, None));
                    }
                    Scope::Screens(s) => {
                        for &sc in s {
                            assert!(screens.contains(&sc));
                            assert!(
                                !seen.contains(&(k, Some(sc))),
                                "{k:?} bound twice on {sc:?} ({})",
                                b.title
                            );
                            seen.push((k, Some(sc)));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn scoped_rows_beat_global_ones() {
        assert_eq!(lookup(Screen::Map, Key::Char('i')), Some(Action::BigCells));
        assert_eq!(
            lookup(Screen::Debug, Key::Char('i')),
            Some(Action::DebugStep)
        );
        assert_eq!(
            lookup(Screen::Genotypes, Key::Char('l')),
            Some(Action::GenoLineage)
        );
        assert_eq!(lookup(Screen::Map, Key::Char('l')), Some(Action::Dir(1, 0)));
        assert_eq!(lookup(Screen::Map, Key::Char('~')), None);
    }

    #[test]
    fn labels_write_a_letter_once() {
        assert_eq!(keys_label(&[c('M'), c('m')]), "M");
        assert_eq!(keys_label(&[c('p')]), "p");
        assert_eq!(keys_label(&[c(' '), c('P')]), "Space P");
        assert_eq!(keys_label(&[c(':'), Key::Ctrl('p')]), ": Ctrl-P");
    }

    #[test]
    fn fuzzy_prefers_word_starts_and_runs() {
        assert!(fuzzy("zzz", "Go to statistics").is_none());
        let start = fuzzy("sl", "Step one time slice").unwrap();
        let inner = fuzzy("sl", "Reset the clock").unwrap_or(i32::MIN);
        assert!(start > inner);
        let run = fuzzy("brea", "Breakpoint").unwrap();
        let scattered = fuzzy("brea", "Back to the red eagle").unwrap();
        assert!(run > scattered);
        assert_eq!(fuzzy("", "x"), Some(0));
        assert!(fuzzy("SAVE pop", "Save population").is_some());
        let exact = fuzzy("map", "Go to the map").unwrap();
        let loose = fuzzy("map", "Go to the options, a menu, a pit").unwrap();
        assert!(exact > loose);
    }

    #[test]
    fn search_ranks_titles_above_keywords() {
        let items = palette_items(&[]);
        let hits = search(&items, "set breakpoint");
        assert!(!hits.is_empty());
        assert_eq!(items[hits[0]].action, Action::DebugBreakpoint);
        // A keyword finds a row whose title lacks the word.
        let hits = search(&items, "viridis");
        assert_eq!(items[hits[0]].action, Action::CyclePalette);
        let hits = search(&items, "zzzzqq");
        assert!(hits.is_empty());
    }

    #[test]
    fn layers_join_the_palette() {
        let items = palette_items(&[(LayerId::Fitness, "Fitness".to_string())]);
        assert!(items
            .iter()
            .any(|i| i.action == Action::Layer(LayerId::Fitness) && i.title == "Layer: Fitness"));
    }

    #[test]
    fn every_bound_action_is_reachable_from_some_screen() {
        for b in BINDINGS {
            let screen = match b.scope {
                Scope::Global => Screen::Map,
                Scope::Screens(s) => s[0],
            };
            for &k in b.keys {
                assert_eq!(lookup(screen, k), Some(b.action), "{}", b.title);
            }
        }
    }
}
