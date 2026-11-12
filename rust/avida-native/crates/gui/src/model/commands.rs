//! The command palette: every action of the application by name, a fuzzy
//! search over them, and carrying one out.

use std::path::PathBuf;

use avida_session::{LayerId, RunState};
use avida_viewmodel::{Legend, Palette, Scale};

use crate::model::layers::{layer_title, MapSettings};
use crate::model::vision::ColorVision;
use crate::model::{display_name, AppState, ChartTab, Request, SideTab, SpeedSetting};
use crate::settings::{Settings, ThemeChoice};

/// Something the palette can do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    RunPause,
    StepUpdate,
    StepSlice,
    FocusRunTo,
    Speed(SpeedSetting),
    SavePopulation,
    StepOrganism,
    RunToBreakpoint,
    FollowOffspring(bool),
    ClearBreakpoints,
    Layer(LayerId),
    Palette(Palette),
    Vision(ColorVision),
    Scale(Scale),
    ToggleLock,
    FitMap,
    ClearSelection,
    ClearHighlight,
    HighlightDominant,
    InspectDominant,
    Tab(SideTab),
    Chart(ChartTab),
    ToggleLog,
    Shortcuts,
    Theme(ThemeChoice),
    Open,
    NewExperiment,
    OpenRecent(PathBuf),
    Restart,
    Close,
    ExportMap,
    ExportCsv,
    ExportCharts,
    ExportGenotypes,
    CopyLog,
}

/// An action as the palette lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub action: Action,
    pub title: String,
    pub group: &'static str,
    /// Keyboard shortcut, as the shortcut overlay writes it.
    pub shortcut: String,
    /// Words that find the item without being in its title.
    pub keywords: &'static str,
}

fn item(action: Action, group: &'static str, title: impl Into<String>) -> Item {
    Item {
        action,
        title: title.into(),
        group,
        shortcut: String::new(),
        keywords: "",
    }
}

impl Item {
    fn key(mut self, shortcut: &str) -> Item {
        self.shortcut = crate::model::format::shortcut(shortcut);
        self
    }

    fn words(mut self, keywords: &'static str) -> Item {
        self.keywords = keywords;
        self
    }
}

/// The actions that can be carried out now, in display order.
pub fn catalog(state: &AppState, settings: &Settings) -> Vec<Item> {
    let mut v = Vec::new();
    let ready = state.is_ready();
    let can = state.can_advance();
    if ready {
        if can {
            let running = state.run_state() == Some(RunState::Running);
            v.push(
                item(
                    Action::RunPause,
                    "Run",
                    if running { "Pause" } else { "Run" },
                )
                .key("Space")
                .words("start stop play resume"),
            );
            v.push(item(Action::StepUpdate, "Run", "Step one update").key("."));
            v.push(item(Action::StepSlice, "Run", "Step one time slice").key(","));
            v.push(item(Action::FocusRunTo, "Run", "Run to update…").words("goto until"));
            for (label, s) in [
                ("Speed: as fast as possible", SpeedSetting::default()),
                (
                    "Speed: 10 updates per second",
                    SpeedSetting {
                        limited: true,
                        ups: 10.0,
                    },
                ),
                (
                    "Speed: 100 updates per second",
                    SpeedSetting {
                        limited: true,
                        ups: 100.0,
                    },
                ),
            ] {
                v.push(item(Action::Speed(s), "Run", label).words("pace throttle slow fast"));
            }
        }
        if state.run_state() != Some(RunState::Failed) {
            v.push(item(Action::SavePopulation, "Run", "Save population").key("Cmd S"));
        }
        if can && state.debugger.is_alive() && state.selection.is_some() {
            v.push(
                item(
                    Action::StepOrganism,
                    "Debugger",
                    "Step the watched organism",
                )
                .key("I")
                .words("instruction debug next"),
            );
            if !state.debugger.breakpoints.is_empty() {
                v.push(
                    item(Action::RunToBreakpoint, "Debugger", "Run to breakpoint")
                        .key("B")
                        .words("debug continue"),
                );
            }
        }
        if state.selection.is_some() {
            let on = state.debugger.follow_offspring;
            v.push(
                item(
                    Action::FollowOffspring(!on),
                    "Debugger",
                    if on {
                        "Stop following offspring"
                    } else {
                        "Follow offspring"
                    },
                )
                .words("child divide"),
            );
        }
        if !state.debugger.breakpoints.is_empty() {
            v.push(item(
                Action::ClearBreakpoints,
                "Debugger",
                "Clear breakpoints",
            ));
        }

        let env = state
            .frame
            .as_ref()
            .map(|f| f.env.clone())
            .unwrap_or_default();
        for (i, layer) in state.layers().into_iter().enumerate() {
            let mut it = item(
                Action::Layer(layer),
                "Map",
                format!("Layer: {}", layer_title(&env, layer)),
            )
            .words("map show colour");
            if i < 9 {
                it = it.key(&(i + 1).to_string());
            }
            v.push(it);
        }
        if state.map.layer != LayerId::Genotype {
            for p in [Palette::Viridis, Palette::Cividis, Palette::OkabeIto] {
                v.push(
                    item(
                        Action::Palette(p),
                        "Map",
                        format!("Palette: {}", palette_name(p)),
                    )
                    .words("colour color scheme"),
                );
            }
        }
        for vis in ColorVision::ALL {
            v.push(
                item(
                    Action::Vision(vis),
                    "Map",
                    format!("Colour vision: {}", vis.label()),
                )
                .words("colorblind color blind cvd simulate preview"),
            );
        }
        if MapSettings::scale_applies(state.map.layer) {
            v.push(item(Action::Scale(Scale::Linear), "Map", "Scale: linear"));
            v.push(item(Action::Scale(Scale::Log), "Map", "Scale: logarithmic"));
            let locked = state.map.is_locked(state.map.layer);
            v.push(
                item(
                    Action::ToggleLock,
                    "Map",
                    if locked {
                        "Unlock the colour range"
                    } else {
                        "Lock the colour range"
                    },
                )
                .words("legend"),
            );
        }
        v.push(item(Action::FitMap, "Map", "Fit the map to the view").key("F"));
        if state.selection.is_some() {
            v.push(item(Action::ClearSelection, "Map", "Clear selection").key("Esc"));
        }
        if state.highlight.is_some() {
            v.push(item(Action::ClearHighlight, "Genotypes", "Clear highlight"));
        }
        if state.frame.as_ref().is_some_and(|f| f.dominant.is_some()) {
            v.push(
                item(
                    Action::HighlightDominant,
                    "Genotypes",
                    "Highlight the dominant genotype",
                )
                .words("lineage ancestry trace"),
            );
            v.push(
                item(
                    Action::InspectDominant,
                    "Genotypes",
                    "Inspect an organism of the dominant genotype",
                )
                .words("debug select"),
            );
        }
        for tab in SideTab::ALL {
            v.push(item(Action::Tab(tab), "View", format!("Show {}", tab.title())).words("tab"));
        }
        for c in ChartTab::ALL {
            v.push(item(Action::Chart(c), "View", format!("Chart: {}", c.title())).words("plot"));
        }
        v.push(item(
            Action::ToggleLog,
            "View",
            if state.show_log {
                "Hide the log"
            } else {
                "Show the log"
            },
        ));
    }
    v.push(item(Action::Shortcuts, "View", "Keyboard shortcuts").key("?"));
    for t in [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark] {
        if t != settings.theme {
            v.push(item(Action::Theme(t), "View", t.label()).words("appearance mode"));
        }
    }
    v.push(item(Action::Open, "Experiment", "Open experiment…").key("Cmd O"));
    v.push(
        item(
            Action::NewExperiment,
            "Experiment",
            "New experiment from the default settings…",
        )
        .key("Cmd N")
        .words("create start fresh default config"),
    );
    for dir in &settings.recent {
        v.push(
            item(
                Action::OpenRecent(dir.clone()),
                "Experiment",
                format!("Open recent: {}", display_name(dir)),
            )
            .words("folder"),
        );
    }
    if ready {
        v.push(
            item(Action::Restart, "Experiment", "Restart experiment").words("reload again reset"),
        );
        v.push(item(Action::ExportMap, "Experiment", "Export map as PNG").words("save image"));
        v.push(
            item(Action::ExportCsv, "Experiment", "Export statistics as CSV").words("save data"),
        );
        v.push(
            item(Action::ExportCharts, "Experiment", "Save the charts as PNG")
                .words("export image picture plot"),
        );
        v.push(
            item(
                Action::ExportGenotypes,
                "Experiment",
                "Export the genotype table as CSV",
            )
            .words("save data genomes sequences"),
        );
        v.push(item(Action::CopyLog, "Experiment", "Copy the log").words("clipboard"));
        v.push(item(Action::Close, "Experiment", "Close experiment"));
    }
    v
}

pub fn palette_name(p: Palette) -> &'static str {
    match p {
        Palette::Viridis => "Viridis",
        Palette::Cividis => "Cividis",
        Palette::OkabeIto => "Okabe–Ito",
    }
}

/// A fuzzy match of a query against a title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    /// Byte offsets of the matched characters in the title.
    pub positions: Vec<usize>,
}

fn is_boundary(prev: Option<char>, c: char) -> bool {
    match prev {
        None => true,
        Some(p) => !p.is_alphanumeric() || (p.is_lowercase() && c.is_uppercase()) || p == ':',
    }
}

/// Matches the query's characters, in order and ignoring case, against
/// `text`. Matches at the start of words and runs of consecutive
/// characters score higher; gaps cost a little. `None` if some character
/// of the query is not found.
pub fn fuzzy_match(query: &str, text: &str) -> Option<Match> {
    let q: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if q.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    // Greedy from each possible start; keep the best.
    let mut best: Option<Match> = None;
    for start in 0..chars.len() {
        if chars[start].1.to_lowercase().next() != Some(q[0]) {
            continue;
        }
        let mut positions = Vec::with_capacity(q.len());
        let mut score = 0;
        let mut qi = 0;
        let mut last: Option<usize> = None;
        for (i, &(off, c)) in chars.iter().enumerate().skip(start) {
            if qi == q.len() {
                break;
            }
            if c.to_lowercase().next() == Some(q[qi]) {
                let prev = i.checked_sub(1).map(|j| chars[j].1);
                score += 1;
                if is_boundary(prev, c) {
                    score += 10;
                }
                match last {
                    Some(l) if l + 1 == i => score += 8,
                    Some(l) => score -= ((i - l - 1) as i32).min(6),
                    None => {}
                }
                if i == 0 {
                    score += 6;
                }
                positions.push(off);
                last = Some(i);
                qi += 1;
            }
        }
        if qi == q.len() && best.as_ref().is_none_or(|b| score > b.score) {
            best = Some(Match { score, positions });
        }
    }
    best
}

/// A hit of [`search`]: the item's index in the catalog and the match on
/// its title (no positions when only its group or keywords matched).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub index: usize,
    pub matched: Match,
}

/// The items matching `query`, best first (catalog order on ties and for
/// an empty query).
pub fn search(items: &[Item], query: &str) -> Vec<Hit> {
    let mut hits: Vec<Hit> = items
        .iter()
        .enumerate()
        .filter_map(|(index, it)| {
            let on_title = fuzzy_match(query, &it.title);
            let matched = on_title.or_else(|| {
                let alt = format!("{} {}", it.group, it.keywords);
                fuzzy_match(query, &alt).map(|m| Match {
                    score: m.score / 3 - 20,
                    positions: Vec::new(),
                })
            })?;
            Some(Hit { index, matched })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.matched
            .score
            .cmp(&a.matched.score)
            .then(a.index.cmp(&b.index))
    });
    hits
}

/// The palette's own state.
#[derive(Clone, Debug, Default)]
pub struct PaletteState {
    pub open: bool,
    pub query: String,
    /// Index into the current hits.
    pub selected: usize,
}

impl PaletteState {
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// Moves the selection by `delta`, wrapping, among `n` hits.
    pub fn move_selection(&mut self, delta: i32, n: usize) {
        if n == 0 {
            self.selected = 0;
            return;
        }
        let n = n as i32;
        self.selected = (self.selected as i32 + delta).rem_euclid(n) as usize;
    }
}

/// Carries out `action`. `legend` is the legend the map shows now (for
/// locking the colour range).
pub fn perform(
    state: &mut AppState,
    settings: &mut Settings,
    action: Action,
    legend: Option<&Legend>,
) {
    match action {
        Action::RunPause => state.toggle_run(),
        Action::StepUpdate => state.step_update(),
        Action::StepSlice => state.step_slice(),
        Action::FocusRunTo => state.focus_run_to = true,
        Action::Speed(s) => state.set_speed(s),
        Action::SavePopulation => state.save_population(),
        Action::StepOrganism => state.debug_step(),
        Action::RunToBreakpoint => state.debug_run_to_breakpoint(),
        Action::FollowOffspring(on) => state.set_follow_offspring(on),
        Action::ClearBreakpoints => state.debugger.breakpoints.clear(),
        Action::Layer(l) => state.set_layer(l),
        Action::Palette(p) => {
            state.map.palette = p;
            state.map_dirty = true;
        }
        Action::Vision(v) => state.set_vision(v),
        Action::Scale(s) => {
            let layer = state.map.layer;
            state.map.set_scale(layer, s);
            state.map_dirty = true;
        }
        Action::ToggleLock => {
            let layer = state.map.layer;
            if state.map.is_locked(layer) {
                state.map.unlock(layer);
            } else if let Some(l) = legend {
                state.map.lock(layer, l);
            }
            state.map_dirty = true;
        }
        Action::FitMap => state.viewport.auto_fit = true,
        Action::ClearSelection => state.select_cell(None),
        Action::ClearHighlight => state.clear_highlight(),
        Action::HighlightDominant => {
            if let Some(id) = state.dominant_id() {
                state.set_highlight(Some(id));
                state.side_tab = SideTab::Genotypes;
            }
        }
        Action::InspectDominant => state.inspect_dominant(),
        Action::Tab(t) => state.side_tab = t,
        Action::Chart(c) => {
            state.chart_tab = c;
            state.side_tab = SideTab::Charts;
        }
        Action::ToggleLog => state.show_log = !state.show_log,
        Action::Shortcuts => state.show_shortcuts = true,
        Action::Theme(t) => settings.theme = t,
        Action::Open => state.request(Request::PickFolder),
        Action::NewExperiment => state.request(Request::NewExperiment),
        Action::OpenRecent(dir) => state.request(Request::Open(dir)),
        Action::Restart => state.restart(None),
        Action::Close => state.request(Request::Close),
        Action::ExportMap => state.request(Request::ExportMap),
        Action::ExportCsv => state.request(Request::ExportCsv),
        Action::ExportCharts => state.request(Request::ExportCharts),
        Action::ExportGenotypes => state.request(Request::ExportGenotypes),
        Action::CopyLog => state.request(Request::CopyLog),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matching_prefers_word_starts_and_runs() {
        let m = fuzzy_match("sp", "Step one update").unwrap();
        assert_eq!(m.positions, vec![0, 3]);
        assert!(fuzzy_match("xyz", "Step one update").is_none());
        let fit = fuzzy_match("fit", "Fit the map to the view").unwrap();
        let mid = fuzzy_match("fit", "Layer: Fitness").unwrap();
        assert!(fit.score > mid.score, "{fit:?} {mid:?}");
        // Word starts beat scattered letters.
        let a = fuzzy_match("lf", "Layer: Fitness").unwrap();
        let b = fuzzy_match("lf", "Colour vision: Normal").unwrap_or(Match {
            score: i32::MIN,
            positions: Vec::new(),
        });
        assert!(a.score > b.score);
        assert_eq!(fuzzy_match("", "x").unwrap().score, 0);
        // Case does not matter; spaces in the query are ignored.
        assert!(fuzzy_match("SAVE pop", "Save population").is_some());
    }

    #[test]
    fn the_catalog_follows_the_state_and_search_ranks_it() {
        let mut state = AppState::default();
        let settings = Settings {
            recent: vec![PathBuf::from("/x/heads/config")],
            ..Settings::default()
        };
        let items = catalog(&state, &settings);
        assert!(
            items.iter().all(|i| i.action != Action::RunPause),
            "no run yet"
        );
        assert!(items
            .iter()
            .any(|i| i.title == "Open recent: heads / config"));

        state.begin_opening(crate::model::Experiment {
            dir: PathBuf::from("/x/heads/config"),
            args: Vec::new(),
            update_limit: None,
        });
        state.opened();
        state.apply_status(crate::model::fixtures::status(RunState::Paused, 3));
        state.apply_frame(std::sync::Arc::new(crate::model::fixtures::frame_with_map()));
        let items = catalog(&state, &settings);
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert!(titles.contains(&"Run"));
        assert!(titles.contains(&"Layer: Fitness"));
        assert!(titles.contains(&"Layer: Task: Nand"));
        let hits = search(&items, "fitness");
        assert_eq!(items[hits[0].index].title, "Layer: Fitness");
        let hits = search(&items, "fit");
        assert_eq!(items[hits[0].index].title, "Fit the map to the view");
        let hits = search(&items, "dark");
        assert_eq!(
            items[hits[0].index].action,
            Action::Theme(ThemeChoice::Dark)
        );
        // Keywords find items whose titles do not match.
        let hits = search(&items, "colorblind");
        assert!(matches!(items[hits[0].index].action, Action::Vision(_)));
        let run = items.iter().find(|i| i.action == Action::RunPause).unwrap();
        assert_eq!(run.shortcut, "Space");
    }

    #[test]
    fn performing_actions_changes_the_state_or_queues_commands() {
        let mut state = AppState::default();
        let mut settings = Settings::default();
        state.begin_opening(crate::model::Experiment {
            dir: PathBuf::from("/x/heads/config"),
            args: Vec::new(),
            update_limit: None,
        });
        state.opened();
        state.apply_frame(std::sync::Arc::new(crate::model::fixtures::frame_with_map()));
        state.apply_status(crate::model::fixtures::status(RunState::Paused, 3));
        state.take_commands();
        perform(&mut state, &mut settings, Action::RunPause, None);
        assert_eq!(state.take_commands(), vec![avida_session::Command::Run]);
        perform(
            &mut state,
            &mut settings,
            Action::Theme(ThemeChoice::Dark),
            None,
        );
        assert_eq!(settings.theme, ThemeChoice::Dark);
        perform(
            &mut state,
            &mut settings,
            Action::Layer(LayerId::Fitness),
            None,
        );
        assert_eq!(state.map.layer, LayerId::Fitness);
        let legend = Legend::new(0.0, 2.0, Scale::Linear);
        perform(&mut state, &mut settings, Action::ToggleLock, Some(&legend));
        assert!(state.map.is_locked(LayerId::Fitness));
        perform(
            &mut state,
            &mut settings,
            Action::Chart(ChartTab::Tasks),
            None,
        );
        assert_eq!(
            (state.side_tab, state.chart_tab),
            (SideTab::Charts, ChartTab::Tasks)
        );
        perform(&mut state, &mut settings, Action::Close, None);
        assert_eq!(state.take_requests(), vec![Request::Close]);
        let mut p = PaletteState::default();
        p.open();
        p.move_selection(-1, 5);
        assert_eq!(p.selected, 4);
        p.move_selection(2, 5);
        assert_eq!(p.selected, 1);
    }
}
