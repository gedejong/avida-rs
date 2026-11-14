//! The state of linked filtering ("brushing") across the charts and the
//! map: the organism filter, the subset it selects in the newest frame, the
//! subset's own time series, the scatter plot's axes and the time-series
//! options.
//!
//! Everything here is view state. It is never sent to the session as
//! anything but the layers the conditions need (a subscription), and it is
//! cleared when another run is loaded.

use std::sync::Arc;

use avida_session::{Frame, LayerId};
use avida_viewmodel::{subset_row, Condition, Filter, Prop, Selection, SubsetRow};

use crate::model::format;
use crate::model::layers::{genotype_name, layer_title};
use crate::model::metrics::Metric;
use crate::model::scan::PlaybackScan;

/// Rows of the subset's series kept (one per frame).
const SUBSET_CAP: usize = 50_000;

/// The subset's means, frame by frame, from the moment the filter was set.
///
/// A live run does not store the past per organism, so the line cannot
/// begin earlier; in a playback it holds the frames that were shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubsetHistory {
    rows: Vec<SubsetRow>,
    /// The update the first row was recorded at: where the line starts.
    started: Option<i32>,
}

impl SubsetHistory {
    pub fn rows(&self) -> &[SubsetRow] {
        &self.rows
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn started_at(&self) -> Option<i32> {
        self.started
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.started = None;
    }

    /// Adds a frame's row. A row for an update already held replaces it; a
    /// row for an earlier update (a seek backwards) drops the later rows.
    pub fn push(&mut self, row: SubsetRow) {
        while self.rows.last().is_some_and(|r| r.update > row.update) {
            self.rows.pop();
        }
        if self.rows.last().is_some_and(|r| r.update == row.update) {
            self.rows.pop();
        }
        if self.started.is_none() {
            self.started = Some(row.update);
        }
        self.rows.push(row);
        if self.rows.len() > SUBSET_CAP {
            self.rows.drain(..SUBSET_CAP / 10);
        }
    }

    /// Fills the history with rows computed elsewhere (every frame of a
    /// recording).
    pub fn replace(&mut self, rows: Vec<SubsetRow>) {
        self.started = rows.first().map(|r| r.update);
        self.rows = rows;
    }
}

/// What a time-series metric is for the selected organisms, if there is
/// such a thing.
pub fn subset_value(metric: Metric, r: &SubsetRow) -> Option<f64> {
    let v = match metric {
        Metric::Organisms => f64::from(r.organisms),
        Metric::Genotypes => f64::from(r.genotypes),
        Metric::AveFitness => r.mean_of(Prop::Fitness),
        Metric::MaxFitness => r.max_fitness,
        Metric::AveMerit => r.mean_of(Prop::Merit),
        Metric::MaxMerit => r.max_merit,
        Metric::AveGestation => r.mean_of(Prop::Gestation),
        Metric::AveGeneration => r.mean_of(Prop::Generation),
        Metric::AveAge => r.mean_of(Prop::Age),
        Metric::AveGenomeSize => r.mean_of(Prop::GenomeLen),
        Metric::Threads => r.mean_of(Prop::Threads) * f64::from(r.organisms),
        Metric::ThresholdGenotypes
        | Metric::Births
        | Metric::Deaths
        | Metric::BreedTrue
        | Metric::Executed
        | Metric::TotalOrganisms => return None,
    };
    Some(v)
}

/// Whether a metric has a counterpart for the selected organisms.
pub fn has_subset_value(metric: Metric) -> bool {
    subset_value(metric, &blank_row()).is_some()
}

fn blank_row() -> SubsetRow {
    SubsetRow {
        update: 0,
        organisms: 0,
        genotypes: 0,
        mean: [f64::NAN; 8],
        max_fitness: f64::NAN,
        max_merit: f64::NAN,
    }
}

/// How the time-series charts average their points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Smoothing {
    #[default]
    Off,
    /// A moving average over this many points.
    Window(u32),
}

impl Smoothing {
    pub const CHOICES: [Smoothing; 4] = [
        Smoothing::Off,
        Smoothing::Window(5),
        Smoothing::Window(20),
        Smoothing::Window(100),
    ];

    pub fn label(self) -> String {
        match self {
            Smoothing::Off => "Off".to_string(),
            Smoothing::Window(n) => n.to_string(),
        }
    }

    pub fn window(self) -> usize {
        match self {
            Smoothing::Off => 1,
            Smoothing::Window(n) => n.max(1) as usize,
        }
    }
}

/// The time-series controls.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SeriesOptions {
    /// A logarithmic value axis (values above zero only).
    pub log_y: bool,
    pub smoothing: Smoothing,
}

/// The scatter plot's axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScatterSettings {
    pub x: Prop,
    pub y: Prop,
    pub log_x: bool,
    pub log_y: bool,
}

impl Default for ScatterSettings {
    fn default() -> Self {
        ScatterSettings {
            x: Prop::GenomeLen,
            y: Prop::Fitness,
            log_x: false,
            log_y: true,
        }
    }
}

/// What the "Add condition" form is about to add.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DraftKind {
    Performs,
    NotPerforms,
    Prop(Prop),
    Genotype,
    Family,
}

/// How a numeric condition compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    AtLeast,
    AtMost,
    Between,
}

impl Comparison {
    pub const ALL: [Comparison; 3] = [Comparison::AtLeast, Comparison::AtMost, Comparison::Between];

    pub fn label(self) -> &'static str {
        match self {
            Comparison::AtLeast => "at least",
            Comparison::AtMost => "at most",
            Comparison::Between => "between",
        }
    }
}

/// The "Add condition" form's fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub kind: DraftKind,
    pub comparison: Comparison,
    pub task: usize,
    pub value: String,
    pub value2: String,
    pub family: Option<i32>,
}

impl Default for Draft {
    fn default() -> Self {
        Draft {
            kind: DraftKind::Prop(Prop::Fitness),
            comparison: Comparison::AtLeast,
            task: 0,
            value: String::new(),
            value2: String::new(),
            family: None,
        }
    }
}

impl Draft {
    /// The numbers typed, when they make sense for the comparison.
    pub fn numbers(&self) -> Option<(Option<f64>, Option<f64>)> {
        let parse = |s: &str| s.trim().replace(',', ".").parse::<f64>().ok();
        let a = parse(&self.value).filter(|v| v.is_finite());
        let b = parse(&self.value2).filter(|v| v.is_finite());
        match self.comparison {
            Comparison::AtLeast => a.map(|a| (Some(a), None)),
            Comparison::AtMost => a.map(|a| (None, Some(a))),
            Comparison::Between => Some((Some(a?), Some(b?))),
        }
    }
}

/// The filter and everything drawn from it.
#[derive(Debug, Default)]
pub struct Filtering {
    pub filter: Filter,
    /// Changes with every edit of the filter.
    pub version: u64,
    selection: Option<Arc<Selection>>,
    pub subset: SubsetHistory,
    pub draft: Draft,
    pub scatter: ScatterSettings,
    pub series: SeriesOptions,
    /// The next drag on the map draws a region.
    pub region_mode: bool,
    /// The "Add condition" form is open (set by the filter bar each frame;
    /// the family list needs the Ancestry layer while it is).
    pub add_open: bool,
    /// The user opened the "Add condition" form; it stays open, inline
    /// under the chips, until a condition is added or it is closed.
    pub form_open: bool,
    /// The worker reading a recording for the subset's series.
    scan: Option<PlaybackScan>,
}

impl Filtering {
    /// The organisms the filter selects in the newest frame; `None` while
    /// the filter is empty (everything is selected then).
    pub fn selection(&self) -> Option<&Arc<Selection>> {
        self.selection.as_ref()
    }

    pub fn is_active(&self) -> bool {
        !self.filter.is_empty()
    }

    /// Recomputes the selection for `frame` (and adds the subset's row for
    /// a new frame when `record`).
    pub fn refresh(&mut self, frame: Option<&Frame>, record: bool) {
        self.selection = None;
        if self.filter.is_empty() {
            return;
        }
        let Some((frame, map)) = frame.and_then(|f| Some((f, f.map.as_ref()?))) else {
            return;
        };
        let sel = self.filter.select(map);
        if record {
            self.subset.push(subset_row(frame.update, map, &sel));
        }
        self.selection = Some(Arc::new(sel));
    }

    /// Edits the filter; the subset's series starts again.
    pub fn edit(&mut self, f: impl FnOnce(&mut Filter)) {
        let before = self.filter.clone();
        f(&mut self.filter);
        if self.filter != before {
            self.version += 1;
            self.subset.clear();
        }
    }

    /// Starts (or stops, for an empty filter or a live run) the scan of a
    /// recording for the subset's series.
    pub fn restart_scan(&mut self, recording: Option<std::path::PathBuf>) {
        self.scan = match recording {
            Some(path) if !self.filter.is_empty() => {
                Some(PlaybackScan::start(path, self.filter.clone()))
            }
            _ => None,
        };
    }

    /// Takes in what the scan has read; true while it is still running.
    pub fn poll_scan(&mut self) -> bool {
        let Some(scan) = &mut self.scan else {
            return false;
        };
        for row in scan.poll() {
            self.subset.push(row);
        }
        !scan.is_done()
    }

    /// Whether a recording is being read for the subset's series.
    pub fn scanning(&self) -> bool {
        self.scan.as_ref().is_some_and(|s| !s.is_done())
    }

    /// Back to nothing: no conditions, no subset series.
    pub fn reset(&mut self) {
        *self = Filtering {
            scatter: self.scatter,
            series: self.series,
            ..Filtering::default()
        };
    }

    /// The layers the filter, the scatter plot and the subset's series
    /// need beyond the map's own layer. `scatter` is whether the scatter
    /// plot is shown; `families` whether the form is listing families.
    pub fn layers(&self, scatter: bool, families: bool) -> Vec<LayerId> {
        let mut v: Vec<LayerId> = Vec::new();
        let mut add = |l: LayerId| {
            if !v.contains(&l) {
                v.push(l);
            }
        };
        if self.is_active() {
            // The subset's series reads every property.
            for p in Prop::ALL {
                add(p.layer());
            }
            for l in self.filter.layers() {
                add(l);
            }
        }
        if scatter {
            add(self.scatter.x.layer());
            add(self.scatter.y.layer());
        }
        if families {
            add(LayerId::Ancestry);
        }
        if self.form_open {
            // The form's range slider shows the property's distribution.
            if let DraftKind::Prop(p) = self.draft.kind {
                add(p.layer());
            }
        }
        v
    }
}

/// The chip text of a condition.
pub fn condition_label(c: &Condition, frame: Option<&Frame>) -> String {
    match c {
        Condition::Task { task, performs } => {
            let name = task_name(frame, *task);
            if *performs {
                format!("Performs task {name}")
            } else {
                format!("Does not perform task {name}")
            }
        }
        Condition::Range { prop, lo, hi } => match (lo.is_finite(), hi.is_finite()) {
            (true, true) if lo == hi => format!("{} = {}", prop.label(), number(*prop, *lo)),
            (true, true) => format!(
                "{} {} to {}",
                prop.label(),
                number(*prop, *lo),
                number(*prop, *hi)
            ),
            (true, false) => format!("{} ≥ {}", prop.label(), number(*prop, *lo)),
            (false, true) => format!("{} ≤ {}", prop.label(), number(*prop, *hi)),
            (false, false) => prop.label().to_string(),
        },
        Condition::Genotype { id } => match frame {
            Some(f) => format!("Genotype {}", genotype_name(f, *id)),
            None => format!("Genotype #{id}"),
        },
        Condition::Family { root } => match frame {
            Some(f) => format!("Family of {}", genotype_name(f, *root)),
            None => format!("Family #{root}"),
        },
        Condition::Region { x0, y0, x1, y1 } => {
            format!("Cells x {x0} to {x1}, y {y0} to {y1}")
        }
    }
}

/// The sentence a chip's tooltip starts with.
pub fn condition_sentence(c: &Condition, frame: Option<&Frame>) -> String {
    match c {
        Condition::Task { task, performs } => {
            let name = task_name(frame, *task);
            if *performs {
                format!("Only organisms that performed {name} in their last gestation.")
            } else {
                format!("Only organisms that did not perform {name} in their last gestation.")
            }
        }
        Condition::Range { prop, lo, hi } => {
            let what = prop.label().to_lowercase();
            match (lo.is_finite(), hi.is_finite()) {
                (true, true) => format!(
                    "Only organisms whose {what} is between {} and {}.",
                    number(*prop, *lo),
                    number(*prop, *hi)
                ),
                (true, false) => format!(
                    "Only organisms whose {what} is at least {}.",
                    number(*prop, *lo)
                ),
                (false, true) => format!(
                    "Only organisms whose {what} is at most {}.",
                    number(*prop, *hi)
                ),
                (false, false) => format!("Every organism with a {what}."),
            }
        }
        Condition::Genotype { id } => match frame {
            Some(f) => format!("Only organisms of genotype {}.", genotype_name(f, *id)),
            None => format!("Only organisms of genotype #{id}."),
        },
        Condition::Family { root } => format!(
            "Only organisms in the family rooted at genotype {}, as the Ancestry map layer groups them.",
            match frame {
                Some(f) => genotype_name(f, *root),
                None => format!("#{root}"),
            }
        ),
        Condition::Region { x0, y0, x1, y1 } => format!(
            "Only organisms in the cells from column {x0} to {x1} and row {y0} to {y1}."
        ),
    }
}

fn number(prop: Prop, v: f64) -> String {
    if prop.is_whole() {
        format::count(v.round() as i64)
    } else {
        format::real(v)
    }
}

/// The name of task `k` (its description, else its keyword).
pub fn task_name(frame: Option<&Frame>, k: usize) -> String {
    frame
        .and_then(|f| f.env.tasks.get(k))
        .map(|t| {
            if t.desc.is_empty() {
                t.name.clone()
            } else {
                t.desc.clone()
            }
        })
        .unwrap_or_else(|| format!("task {k}"))
}

/// The title of the layer a property shows (for chart labels).
pub fn prop_title(frame: Option<&Frame>, p: Prop) -> String {
    match frame {
        Some(f) => layer_title(&f.env, p.layer()),
        None => p.label().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::fixtures::frame_with_map;

    #[test]
    fn the_subset_series_replaces_and_rewinds() {
        let mut h = SubsetHistory::default();
        let row = |u: i32, n: u32| SubsetRow {
            organisms: n,
            update: u,
            ..blank_row()
        };
        h.push(row(10, 1));
        h.push(row(20, 2));
        h.push(row(20, 3));
        assert_eq!(h.rows().len(), 2);
        assert_eq!(h.rows()[1].organisms, 3);
        assert_eq!(h.started_at(), Some(10));
        h.push(row(15, 4));
        assert_eq!(
            h.rows().iter().map(|r| r.update).collect::<Vec<_>>(),
            [10, 15]
        );
        h.clear();
        assert!(h.is_empty() && h.started_at().is_none());
    }

    #[test]
    fn only_measures_with_an_organism_counterpart_have_a_subset_line() {
        assert!(has_subset_value(Metric::AveFitness));
        assert!(has_subset_value(Metric::Organisms));
        assert!(!has_subset_value(Metric::Births));
        assert!(!has_subset_value(Metric::TotalOrganisms));
    }

    #[test]
    fn editing_the_filter_restarts_the_subset_and_a_no_op_does_not() {
        let f = frame_with_map();
        let mut b = Filtering::default();
        b.edit(|f| f.set(Condition::Genotype { id: 7 }));
        assert_eq!(b.version, 1);
        b.refresh(Some(&f), true);
        assert_eq!(b.selection().map(|s| s.matched), Some(2));
        assert_eq!(b.subset.rows().len(), 1);
        assert_eq!(b.subset.rows()[0].organisms, 2);
        b.edit(|f| f.set(Condition::Genotype { id: 7 }));
        assert_eq!(b.version, 1, "the same condition changes nothing");
        assert_eq!(b.subset.rows().len(), 1);
        b.edit(|f| f.set(Condition::Genotype { id: 8 }));
        assert!(b.subset.is_empty());
        b.reset();
        assert!(!b.is_active() && b.selection().is_none());
    }

    #[test]
    fn layers_follow_the_filter_the_scatter_and_the_family_form() {
        let mut b = Filtering::default();
        assert!(b.layers(false, false).is_empty());
        assert_eq!(
            b.layers(true, false),
            vec![LayerId::GenomeLen, LayerId::Fitness]
        );
        assert_eq!(b.layers(false, true), vec![LayerId::Ancestry]);
        b.edit(|f| f.set(Condition::Family { root: 3 }));
        let l = b.layers(false, false);
        assert!(l.contains(&LayerId::Ancestry) && l.contains(&LayerId::Merit));
        assert_eq!(l.len(), 9);
    }

    #[test]
    fn labels_and_sentences_name_the_task_and_the_bounds() {
        let f = frame_with_map();
        let c = Condition::Task {
            task: 0,
            performs: true,
        };
        assert_eq!(condition_label(&c, Some(&f)), "Performs task Nand");
        assert!(condition_sentence(&c, Some(&f)).contains("performed Nand"));
        let r = Condition::range(Prop::Fitness, 2.0, f64::INFINITY);
        assert_eq!(condition_label(&r, Some(&f)), "Fitness ≥ 2");
        let r = Condition::range(Prop::GenomeLen, 50.0, 60.0);
        assert_eq!(condition_label(&r, Some(&f)), "Genome length 50 to 60");
        assert_eq!(
            condition_label(&Condition::Genotype { id: 7 }, Some(&f)),
            "Genotype 100-aaaaa"
        );
    }

    #[test]
    fn typed_numbers_need_the_right_count_for_the_comparison() {
        let mut d = Draft {
            value: "2,5".into(),
            ..Draft::default()
        };
        assert_eq!(d.numbers(), Some((Some(2.5), None)));
        d.comparison = Comparison::AtMost;
        assert_eq!(d.numbers(), Some((None, Some(2.5))));
        d.comparison = Comparison::Between;
        assert_eq!(d.numbers(), None);
        d.value2 = "7".into();
        assert_eq!(d.numbers(), Some((Some(2.5), Some(7.0))));
        d.value = "abc".into();
        assert_eq!(d.numbers(), None);
    }
}
