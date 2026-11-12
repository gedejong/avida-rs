//! Preferences kept between runs of the program (eframe storage).

use std::path::{Path, PathBuf};

use avida_viewmodel::{LayerId, Palette};

use crate::model::debugger::OrgTab;
use crate::model::genotypes::GenoSort;
use crate::model::log::LogFilter;
use crate::model::metrics::Metric;
use crate::model::vision::ColorVision;
use crate::model::{AppState, ChartTab, SideTab, SpeedSetting};

/// The storage key.
pub const STORAGE_KEY: &str = "avida-gui/settings";
/// Recent folders remembered.
pub const MAX_RECENT: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeChoice {
    /// Follow the operating system.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    /// The next choice of the theme button.
    pub fn next(self) -> ThemeChoice {
        match self {
            ThemeChoice::System => ThemeChoice::Light,
            ThemeChoice::Light => ThemeChoice::Dark,
            ThemeChoice::Dark => ThemeChoice::System,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "System theme",
            ThemeChoice::Light => "Light theme",
            ThemeChoice::Dark => "Dark theme",
        }
    }

    pub fn preference(self) -> egui::ThemePreference {
        match self {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        }
    }
}

/// A palette as stored (the viewmodel type has no serde).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PaletteChoice {
    #[default]
    Viridis,
    Cividis,
    OkabeIto,
}

impl From<PaletteChoice> for Palette {
    fn from(p: PaletteChoice) -> Palette {
        match p {
            PaletteChoice::Viridis => Palette::Viridis,
            PaletteChoice::Cividis => Palette::Cividis,
            PaletteChoice::OkabeIto => Palette::OkabeIto,
        }
    }
}

impl From<Palette> for PaletteChoice {
    fn from(p: Palette) -> PaletteChoice {
        match p {
            Palette::Viridis => PaletteChoice::Viridis,
            Palette::Cividis => PaletteChoice::Cividis,
            Palette::OkabeIto => PaletteChoice::OkabeIto,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeChoice,
    /// Most recent first.
    pub recent: Vec<PathBuf>,
    pub palette: PaletteChoice,
    pub side_tab: SideTab,
    pub chart_tab: ChartTab,
    pub chart_metrics: Vec<Metric>,
    pub geno_sort: GenoSort,
    pub speed: SpeedSetting,
    pub show_log: bool,
    pub log_filter: LogFilter,
    /// The organism tab's view.
    pub org_tab: OrgTab,
    /// The memory listing keeps the instruction pointer in view.
    pub follow_ip: bool,
    /// The map layer, by [`LayerId::key`]; only the layers every world
    /// has are remembered (task and resource layers depend on the
    /// environment).
    pub map_layer: String,
    /// The colour-vision preview of the map.
    pub vision: ColorVision,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeChoice::System,
            recent: Vec::new(),
            palette: PaletteChoice::Viridis,
            side_tab: SideTab::Stats,
            chart_tab: ChartTab::TimeSeries,
            chart_metrics: Metric::DEFAULT_CHARTS.to_vec(),
            geno_sort: GenoSort::default(),
            speed: SpeedSetting::default(),
            show_log: false,
            log_filter: LogFilter::All,
            org_tab: OrgTab::Cpu,
            follow_ip: true,
            map_layer: LayerId::Genotype.key(),
            vision: ColorVision::Normal,
        }
    }
}

impl Settings {
    /// Puts `dir` first in the recent list.
    pub fn remember(&mut self, dir: &Path) {
        self.recent.retain(|d| d != dir);
        self.recent.insert(0, dir.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }

    pub fn forget(&mut self, dir: &Path) {
        self.recent.retain(|d| d != dir);
    }

    /// Copies the stored preferences into a fresh state.
    pub fn apply_to(&self, state: &mut AppState) {
        state.map.palette = self.palette.into();
        state.side_tab = self.side_tab;
        state.chart_tab = self.chart_tab;
        if !self.chart_metrics.is_empty() {
            state.chart_metrics = self.chart_metrics.clone();
        }
        state.geno_sort = self.geno_sort;
        state.speed = self.speed;
        state.show_log = self.show_log;
        state.log_filter = self.log_filter;
        state.debugger.tab = self.org_tab;
        state.debugger.follow_ip = self.follow_ip;
        if let Some(&layer) = LayerId::FIXED.iter().find(|l| l.key() == self.map_layer) {
            state.map.layer = layer;
        }
        state.map.vision = self.vision;
    }

    /// Reads the preferences back out of the state.
    pub fn capture(&mut self, state: &AppState) {
        self.palette = state.map.palette.into();
        self.side_tab = state.side_tab;
        self.chart_tab = state.chart_tab;
        self.chart_metrics = state.chart_metrics.clone();
        self.geno_sort = state.geno_sort;
        self.speed = state.speed;
        self.show_log = state.show_log;
        self.log_filter = state.log_filter;
        self.org_tab = state.debugger.tab;
        self.follow_ip = state.debugger.follow_ip;
        if LayerId::FIXED.contains(&state.map.layer) {
            self.map_layer = state.map.layer.key();
        }
        self.vision = state.map.vision;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_folders_are_deduplicated_and_capped() {
        let mut s = Settings::default();
        for i in 0..12 {
            s.remember(Path::new(&format!("/e/{i}")));
        }
        s.remember(Path::new("/e/5"));
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.recent[0], PathBuf::from("/e/5"));
        assert_eq!(
            s.recent.iter().filter(|p| **p == Path::new("/e/5")).count(),
            1
        );
        s.forget(Path::new("/e/5"));
        assert_eq!(s.recent[0], PathBuf::from("/e/11"));
    }

    #[test]
    fn preferences_round_trip_through_the_state() {
        let mut s = Settings {
            palette: PaletteChoice::Cividis,
            side_tab: SideTab::Genotypes,
            show_log: true,
            org_tab: OrgTab::Io,
            follow_ip: false,
            ..Settings::default()
        };
        let mut state = AppState::default();
        s.apply_to(&mut state);
        assert_eq!(state.map.palette, Palette::Cividis);
        assert_eq!(state.side_tab, SideTab::Genotypes);
        assert_eq!(state.debugger.tab, OrgTab::Io);
        assert!(!state.debugger.follow_ip);
        state.side_tab = SideTab::Charts;
        state.map.layer = LayerId::Fitness;
        state.map.vision = ColorVision::Deuteranopia;
        s.capture(&state);
        assert_eq!(s.side_tab, SideTab::Charts);
        assert_eq!(s.map_layer, "fitness");
        let mut fresh = AppState::default();
        s.apply_to(&mut fresh);
        assert_eq!(fresh.map.layer, LayerId::Fitness, "the layer is remembered");
        assert_eq!(fresh.map.vision, ColorVision::Deuteranopia);
        // A task layer depends on the environment: the last fixed layer
        // stays remembered.
        state.map.layer = LayerId::Task(3);
        s.capture(&state);
        assert_eq!(s.map_layer, "fitness");
        assert!(s.show_log);
        assert_eq!(ThemeChoice::Dark.next(), ThemeChoice::System);
    }
}
