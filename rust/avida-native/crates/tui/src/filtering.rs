//! The organism filter ("brushing") of the terminal interface: the filter,
//! the subset it selects in the newest frame, the subset's own time series,
//! and the "Filter organisms" dialog that edits it from the keyboard.
//!
//! The model is `avida_viewmodel::Filter`, shared with the desktop front
//! end. Everything here is view state. The filter changes which layers the
//! application asks the session for (a subscription) and nothing else: it
//! never reaches the simulation, an output file or a recording, and it is
//! cleared when another run starts.

use std::sync::Arc;

use avida_session::{Frame, LayerId};
use avida_viewmodel::{
    format_number, subset_row, Condition, Filter, MapFrame, Prop, Selection, SubsetRow,
    UNCLASSIFIED,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::Key;
use crate::app::{App, ToastKind};
use crate::metrics::Metric;
use crate::ui::util::thousands;

/// Rows of the subset's series kept (one per frame).
const SUBSET_CAP: usize = 50_000;

/// The subset's means, frame by frame, from the moment the filter was set.
///
/// A live run keeps nothing per organism from the past, so the line cannot
/// begin earlier; in a playback it holds the frames that were shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubsetHistory {
    rows: Vec<SubsetRow>,
}

impl SubsetHistory {
    pub fn rows(&self) -> &[SubsetRow] {
        &self.rows
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn clear(&mut self) {
        self.rows.clear();
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
        self.rows.push(row);
        if self.rows.len() > SUBSET_CAP {
            self.rows.drain(..SUBSET_CAP / 10);
        }
    }
}

/// What a time-series measure is for the selected organisms, if there is
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

/// Whether a measure has a counterpart for the selected organisms.
pub fn has_subset_value(metric: Metric) -> bool {
    let blank = SubsetRow {
        update: 0,
        organisms: 0,
        genotypes: 0,
        mean: [f64::NAN; 8],
        max_fitness: f64::NAN,
        max_merit: f64::NAN,
    };
    subset_value(metric, &blank).is_some()
}

// --- the dialog ---------------------------------------------------------

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

    fn step(self, d: i32) -> Comparison {
        let i = Comparison::ALL.iter().position(|&c| c == self).unwrap_or(0) as i32;
        Comparison::ALL[(i + d).rem_euclid(3) as usize]
    }
}

/// What a condition can be about, in the order the list shows them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Performs,
    NotPerforms,
    Prop(Prop),
    Genotype,
    Family,
    Region,
}

impl Kind {
    pub fn all() -> Vec<Kind> {
        let mut v = vec![Kind::Performs, Kind::NotPerforms];
        v.extend(Prop::ALL.into_iter().map(Kind::Prop));
        v.extend([Kind::Genotype, Kind::Family, Kind::Region]);
        v
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Performs => "Performs task",
            Kind::NotPerforms => "Does not perform task",
            Kind::Prop(p) => p.label(),
            Kind::Genotype => "Genotype",
            Kind::Family => "Family",
            Kind::Region => "Region of the map",
        }
    }

    /// One line about what choosing the kind does.
    pub fn note(self) -> &'static str {
        match self {
            Kind::Performs => "Only organisms that performed a task in their last gestation.",
            Kind::NotPerforms => "Only organisms that did not perform a task.",
            Kind::Prop(p) => p.description(),
            Kind::Genotype => "The genotype of the inspected organism (else of the cursor cell).",
            Kind::Family => "A family of the Ancestry layer, named after its root genotype.",
            Kind::Region => "A rectangle of cells, from the cells in view or typed.",
        }
    }
}

/// Which step of "Add condition" the dialog is at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    /// Choosing what the condition is about.
    Kind,
    /// Choosing a task (for "performs" or "does not perform").
    Task(bool),
    /// Choosing a family.
    Family,
    /// Typing the numbers of a comparison.
    Prop(Prop),
    /// Typing the four corners of a rectangle.
    Region,
}

/// The two lists of the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Add,
    Conditions,
}

/// The "Filter organisms" dialog's state.
#[derive(Clone, Debug, PartialEq)]
pub struct Dialog {
    pub tab: Tab,
    pub stage: Stage,
    /// Row of the kind list.
    pub kind_sel: usize,
    /// Row of the task, family or condition list.
    pub sel: usize,
    pub comparison: Comparison,
    /// The number fields of the stage.
    pub fields: Vec<String>,
    pub field: usize,
    /// What is wrong with the last attempt to add.
    pub error: Option<String>,
}

/// Labels of the region's four fields.
pub const REGION_FIELDS: [&str; 4] = ["x from", "y from", "x to", "y to"];

impl Dialog {
    pub fn new(tab: Tab) -> Dialog {
        Dialog {
            tab,
            stage: Stage::Kind,
            kind_sel: 0,
            sel: 0,
            comparison: Comparison::AtLeast,
            fields: Vec::new(),
            field: 0,
            error: None,
        }
    }

    /// Number fields in use at this stage.
    pub fn field_count(&self) -> usize {
        match self.stage {
            Stage::Prop(_) => {
                if self.comparison == Comparison::Between {
                    2
                } else {
                    1
                }
            }
            Stage::Region => 4,
            _ => 0,
        }
    }

    /// Moves to stage `s` with fresh fields.
    fn enter(&mut self, s: Stage, fields: Vec<String>) {
        self.stage = s;
        self.sel = 0;
        self.fields = fields;
        self.field = 0;
        self.error = None;
    }

    /// Back to the kind list.
    fn back(&mut self) {
        self.enter(Stage::Kind, Vec::new());
    }

    /// Types into the focused field. Numbers only.
    fn type_char(&mut self, c: char) {
        if !(c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | '+' | 'e' | 'E')) {
            return;
        }
        let i = self.field;
        if let Some(f) = self.fields.get_mut(i) {
            if f.chars().count() < 14 {
                f.push(c);
                self.error = None;
            }
        }
    }

    fn backspace(&mut self) {
        let i = self.field;
        if let Some(f) = self.fields.get_mut(i) {
            f.pop();
            self.error = None;
        }
    }

    fn clear_field(&mut self) {
        let i = self.field;
        if let Some(f) = self.fields.get_mut(i) {
            f.clear();
            self.error = None;
        }
    }

    fn move_field(&mut self, d: i32) {
        let n = self.field_count().max(1) as i32;
        self.field = (self.field as i32 + d).rem_euclid(n) as usize;
    }

    /// The numbers typed, when they make sense for the comparison: the
    /// lower and the upper bound, either of which may be open.
    pub fn numbers(&self) -> Option<(Option<f64>, Option<f64>)> {
        let parse = |i: usize| {
            self.fields
                .get(i)?
                .trim()
                .replace(',', ".")
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
        };
        match self.comparison {
            Comparison::AtLeast => parse(0).map(|a| (Some(a), None)),
            Comparison::AtMost => parse(0).map(|a| (None, Some(a))),
            Comparison::Between => Some((Some(parse(0)?), Some(parse(1)?))),
        }
    }

    /// The rectangle typed: `(x0, y0, x1, y1)`.
    pub fn rectangle(&self) -> Option<(i32, i32, i32, i32)> {
        let p = |i: usize| self.fields.get(i)?.trim().parse::<i32>().ok();
        Some((p(0)?, p(1)?, p(2)?, p(3)?))
    }
}

// --- the state ----------------------------------------------------------

/// The filter and everything drawn from it.
#[derive(Debug, Default)]
pub struct Filtering {
    pub filter: Filter,
    /// Changes with every edit of the filter.
    pub version: u64,
    selection: Option<Arc<Selection>>,
    pub subset: SubsetHistory,
    pub dialog: Option<Dialog>,
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

    /// Recomputes the selection for `frame` and, for a new frame (`record`),
    /// adds the subset's row.
    pub fn refresh(&mut self, frame: Option<&Frame>, record: bool) {
        self.selection = None;
        if self.filter.is_empty() {
            return;
        }
        let Some((frame, map)) = frame.and_then(|f| Some((f, f.map.as_ref()?))) else {
            return;
        };
        let sel = self.filter.select(map);
        // A row computed while a layer is missing would count the
        // organisms the filter could not judge.
        if record && sel.ignored.is_empty() {
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

    /// Back to nothing: no conditions, no subset series, no dialog.
    pub fn reset(&mut self) {
        *self = Filtering::default();
    }

    /// The layers the filter and the open dialog need beyond the map's
    /// own. While the filter is active the subset's series reads every
    /// property; the dialog needs the chosen property's distribution or the
    /// families.
    pub fn layers(&self) -> Vec<LayerId> {
        let mut v: Vec<LayerId> = Vec::new();
        let mut add = |l: LayerId| {
            if !v.contains(&l) {
                v.push(l);
            }
        };
        if self.is_active() {
            for p in Prop::ALL {
                add(p.layer());
            }
            for l in self.filter.layers() {
                add(l);
            }
        }
        if let Some(d) = &self.dialog {
            match d.stage {
                Stage::Prop(p) if d.tab == Tab::Add => add(p.layer()),
                Stage::Family if d.tab == Tab::Add => add(LayerId::Ancestry),
                _ => {}
            }
        }
        v
    }
}

// --- text ---------------------------------------------------------------

/// How a genotype is named: Avida gives every genotype below the threshold
/// the placeholder `100-no_name`, so the id tells them apart.
pub fn genotype_name(frame: Option<&Frame>, id: i32) -> String {
    if id == UNCLASSIFIED {
        return "unclassified".to_string();
    }
    let row = frame.and_then(|f| f.genotypes.iter().find(|g| g.id == id));
    match row {
        Some(g) if g.name.is_empty() => format!("#{id}"),
        Some(g) if g.name.ends_with("no_name") => format!("{} #{id}", g.name),
        Some(g) => g.name.clone(),
        None => format!("#{id}"),
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

fn number(prop: Prop, v: f64) -> String {
    if prop.is_whole() {
        thousands(v.round() as i64)
    } else {
        format_number(v)
    }
}

/// The text of a condition, as the desktop front end's chips word it.
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
        Condition::Genotype { id } => format!("Genotype {}", genotype_name(frame, *id)),
        Condition::Family { root } => format!("Family of {}", genotype_name(frame, *root)),
        Condition::Region { x0, y0, x1, y1 } => {
            format!("Cells x {x0} to {x1}, y {y0} to {y1}")
        }
    }
}

/// "N of M organisms match (P%)", or "No organism matches". `short` drops
/// the word "organisms".
pub fn summary_text(sel: &Selection, short: bool) -> String {
    if sel.matched == 0 && sel.occupied > 0 {
        return "No organism matches".to_string();
    }
    let share = sel.share() * 100.0;
    let p = if sel.matched > 0 && share < 1.0 {
        "<1".to_string()
    } else {
        format!("{share:.0}")
    };
    format!(
        "{} of {}{} match ({p}%)",
        thousands(sel.matched as i64),
        thousands(sel.occupied as i64),
        if short { "" } else { " organisms" },
    )
}

/// The smallest and largest finite values of a property's layer.
pub fn prop_range(map: &MapFrame, p: Prop) -> Option<(f64, f64)> {
    let values = map.layer(p.layer())?;
    let mut it = values
        .iter()
        .filter(|v| v.is_finite())
        .map(|&v| f64::from(v));
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), v| (lo.min(v), hi.max(v))))
}

// --- the App ------------------------------------------------------------

impl App {
    /// The organism the genotype condition would use: the inspected one,
    /// else the one under the cursor.
    pub fn filter_genotype(&self) -> Option<i32> {
        let map = self.frame.as_ref()?.map.as_ref()?;
        let cell = match self.selection {
            Some(c) => usize::try_from(c).ok()?,
            None => {
                let (w, _) = self.world_size()?;
                self.cursor.1 * w + self.cursor.0
            }
        };
        let g = *map.genotype_id.get(cell)?;
        (g >= 0).then_some(g)
    }

    /// Opens the dialog, on the list of conditions when `conditions`.
    pub fn open_filter(&mut self, conditions: bool) {
        if self.frame.as_ref().and_then(|f| f.map.as_ref()).is_none() {
            self.toast(
                ToastKind::Info,
                "The filter needs the first view of the world; wait a moment.".to_string(),
            );
            return;
        }
        let tab = if conditions && self.filtering.is_active() {
            Tab::Conditions
        } else {
            Tab::Add
        };
        self.filtering.dialog = Some(Dialog::new(tab));
        self.sync_subscription();
    }

    /// Removes every condition.
    pub fn clear_filter(&mut self) {
        if self.filtering.is_active() {
            self.edit_filter(Filter::clear);
            self.toast(ToastKind::Info, "Filter cleared.".to_string());
        } else {
            self.toast(ToastKind::Info, "No filter is set.".to_string());
        }
    }

    /// Changes the filter: the selection is recomputed for the newest
    /// frame, the layers the conditions read are subscribed to and the
    /// subset's series starts over. Nothing else is sent.
    pub fn edit_filter(&mut self, f: impl FnOnce(&mut Filter)) {
        let v = self.filtering.version;
        self.filtering.edit(f);
        if self.filtering.version != v {
            let frame = self.frame.clone();
            self.filtering.refresh(frame.as_deref(), true);
            self.sync_subscription();
        }
    }

    /// Sends the subscription when the layers it asks for changed (the
    /// filter or the dialog needs others). The inspected cell and the
    /// genotype lineage have commands of their own.
    pub fn sync_subscription(&mut self) {
        if self.restarting.is_some() {
            return;
        }
        let now = self.subscription().layers;
        if now != self.last_layers {
            self.subscribe();
        }
    }

    /// Keys while the dialog is open.
    pub(crate) fn on_filter_key(&mut self, key: KeyEvent, k: Key) {
        let Some(mut d) = self.filtering.dialog.take() else {
            return;
        };
        let keep = self.filter_key(&mut d, key, k);
        if keep {
            self.filtering.dialog = Some(d);
        }
        self.sync_subscription();
    }

    /// Applies a key to the dialog `d`; false when it closes.
    fn filter_key(&mut self, d: &mut Dialog, key: KeyEvent, k: Key) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let up = matches!(k, Key::Code(KeyCode::Up));
        let down = matches!(k, Key::Code(KeyCode::Down));
        let conds = self.filtering.filter.len();
        if d.tab == Tab::Conditions {
            let last = conds.saturating_sub(1);
            d.sel = d.sel.min(last);
            match k {
                Key::Code(KeyCode::Esc) | Key::Char('q') => return false,
                Key::Code(KeyCode::Tab | KeyCode::BackTab) => d.tab = Tab::Add,
                Key::Code(KeyCode::Up) | Key::Char('k') => d.sel = d.sel.saturating_sub(1),
                Key::Code(KeyCode::Down) | Key::Char('j') => d.sel = (d.sel + 1).min(last),
                Key::Code(KeyCode::Delete | KeyCode::Backspace) | Key::Char('x') => {
                    let i = d.sel;
                    self.edit_filter(|f| f.remove(i));
                    if self.filtering.filter.is_empty() {
                        d.tab = Tab::Add;
                    }
                    d.sel = d.sel.min(self.filtering.filter.len().saturating_sub(1));
                }
                Key::Char('c') => {
                    self.edit_filter(Filter::clear);
                    d.tab = Tab::Add;
                    d.sel = 0;
                }
                _ => {}
            }
            return true;
        }
        match d.stage {
            Stage::Kind => {
                let n = Kind::all().len();
                match k {
                    Key::Code(KeyCode::Esc) | Key::Char('q') => return false,
                    Key::Code(KeyCode::Tab | KeyCode::BackTab) if conds > 0 => {
                        d.tab = Tab::Conditions;
                        d.sel = 0;
                    }
                    Key::Code(KeyCode::Up) | Key::Char('k') => {
                        d.kind_sel = d.kind_sel.saturating_sub(1);
                    }
                    Key::Code(KeyCode::Down) | Key::Char('j') => {
                        d.kind_sel = (d.kind_sel + 1).min(n - 1);
                    }
                    Key::Code(KeyCode::Home) => d.kind_sel = 0,
                    Key::Code(KeyCode::End) => d.kind_sel = n - 1,
                    Key::Code(KeyCode::Enter) => return self.filter_choose_kind(d),
                    _ => {}
                }
            }
            Stage::Task(performs) => {
                let n = self.frame.as_ref().map_or(0, |f| f.env.tasks.len());
                match k {
                    Key::Code(KeyCode::Esc) => d.back(),
                    _ if up => d.sel = d.sel.saturating_sub(1),
                    _ if down => d.sel = (d.sel + 1).min(n.saturating_sub(1)),
                    Key::Char('k') => d.sel = d.sel.saturating_sub(1),
                    Key::Char('j') => d.sel = (d.sel + 1).min(n.saturating_sub(1)),
                    Key::Code(KeyCode::Enter) if n > 0 => {
                        let task = d.sel.min(n - 1);
                        self.edit_filter(|f| f.set(Condition::Task { task, performs }));
                        return false;
                    }
                    _ => {}
                }
            }
            Stage::Family => {
                let fams: Vec<i32> = self
                    .frame
                    .as_ref()
                    .and_then(|f| f.map.as_ref())
                    .map(|m| m.families.iter().map(|f| f.root).collect())
                    .unwrap_or_default();
                let n = fams.len();
                match k {
                    Key::Code(KeyCode::Esc) => d.back(),
                    _ if up => d.sel = d.sel.saturating_sub(1),
                    _ if down => d.sel = (d.sel + 1).min(n.saturating_sub(1)),
                    Key::Char('k') => d.sel = d.sel.saturating_sub(1),
                    Key::Char('j') => d.sel = (d.sel + 1).min(n.saturating_sub(1)),
                    Key::Code(KeyCode::Enter) if n > 0 => {
                        let root = fams[d.sel.min(n - 1)];
                        self.edit_filter(|f| f.set(Condition::Family { root }));
                        return false;
                    }
                    _ => {}
                }
            }
            Stage::Prop(_) | Stage::Region => match k {
                Key::Code(KeyCode::Esc) => d.back(),
                Key::Code(KeyCode::Left) if matches!(d.stage, Stage::Prop(_)) => {
                    d.comparison = d.comparison.step(-1);
                    d.field = d.field.min(d.field_count() - 1);
                    d.error = None;
                }
                Key::Code(KeyCode::Right) if matches!(d.stage, Stage::Prop(_)) => {
                    d.comparison = d.comparison.step(1);
                    d.field = d.field.min(d.field_count() - 1);
                    d.error = None;
                }
                Key::Code(KeyCode::Tab | KeyCode::Down) => d.move_field(1),
                Key::Code(KeyCode::BackTab | KeyCode::Up) => d.move_field(-1),
                Key::Code(KeyCode::Backspace) => d.backspace(),
                Key::Ctrl('u') => d.clear_field(),
                Key::Code(KeyCode::Enter) => return !self.filter_commit(d),
                Key::Char(c) if !ctrl => d.type_char(c),
                _ => {}
            },
        }
        true
    }

    /// Enter on the kind list. False closes the dialog.
    fn filter_choose_kind(&mut self, d: &mut Dialog) -> bool {
        let kind = Kind::all()[d.kind_sel.min(Kind::all().len() - 1)];
        match kind {
            Kind::Performs | Kind::NotPerforms => {
                let n = self.frame.as_ref().map_or(0, |f| f.env.tasks.len());
                if n == 0 {
                    d.error = Some("This environment rewards no tasks.".to_string());
                } else {
                    d.enter(Stage::Task(kind == Kind::Performs), Vec::new());
                }
            }
            Kind::Prop(p) => d.enter(Stage::Prop(p), vec![String::new(), String::new()]),
            Kind::Family => d.enter(Stage::Family, Vec::new()),
            Kind::Genotype => match self.filter_genotype() {
                Some(id) => {
                    self.edit_filter(|f| f.set(Condition::Genotype { id }));
                    return false;
                }
                None => {
                    d.error = Some(
                        "Inspect an organism first: put the cursor on one and press Enter."
                            .to_string(),
                    );
                }
            },
            Kind::Region => {
                let (w, h) = self.world_size().unwrap_or((1, 1));
                let (x0, y0, x1, y1) = match self.geom {
                    Some(g) if g.vis_w > 0 && g.vis_h > 0 => {
                        (g.ox, g.oy, g.ox + g.vis_w - 1, g.oy + g.vis_h - 1)
                    }
                    _ => (0, 0, w - 1, h - 1),
                };
                d.enter(
                    Stage::Region,
                    vec![
                        x0.to_string(),
                        y0.to_string(),
                        x1.to_string(),
                        y1.to_string(),
                    ],
                );
            }
        }
        true
    }

    /// Enter in a number stage: adds the condition, or says what is wrong.
    /// True when it was added.
    fn filter_commit(&mut self, d: &mut Dialog) -> bool {
        match d.stage {
            Stage::Prop(p) => {
                let Some((lo, hi)) = d.numbers() else {
                    d.error = Some(
                        if d.comparison == Comparison::Between {
                            "Type two numbers."
                        } else {
                            "Type a number."
                        }
                        .to_string(),
                    );
                    return false;
                };
                let (lo, hi) = (lo.unwrap_or(f64::NEG_INFINITY), hi.unwrap_or(f64::INFINITY));
                self.edit_filter(|f| f.set(Condition::range(p, lo, hi)));
                true
            }
            Stage::Region => {
                let Some((x0, y0, x1, y1)) = d.rectangle() else {
                    d.error = Some("Type four whole numbers.".to_string());
                    return false;
                };
                self.edit_filter(|f| f.set(Condition::region((x0, y0), (x1, y1))));
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{app_with, frame_with_values};
    use avida_session::Command;
    use ratatui::crossterm::event::{KeyEventKind, KeyEventState};

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent {
            code: c,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn type_text(a: &mut App, s: &str) {
        for c in s.chars() {
            a.on_key(key(KeyCode::Char(c)));
        }
    }

    fn app() -> App {
        let mut a = app_with(frame_with_values());
        a.take_commands();
        a
    }

    fn kind_row(k: Kind) -> usize {
        Kind::all().iter().position(|&x| x == k).unwrap()
    }

    fn choose(a: &mut App, k: Kind) {
        for _ in 0..kind_row(k) {
            a.on_key(key(KeyCode::Down));
        }
        a.on_key(key(KeyCode::Enter));
    }

    #[test]
    fn the_kind_list_has_the_desktop_front_ends_choices() {
        let labels: Vec<&str> = Kind::all().into_iter().map(Kind::label).collect();
        assert_eq!(labels.len(), 13);
        assert_eq!(labels[0], "Performs task");
        assert_eq!(labels[1], "Does not perform task");
        assert_eq!(labels[2], "Fitness");
        assert_eq!(labels[10], "Genotype");
    }

    #[test]
    fn a_range_is_typed_in_three_comparisons() {
        let mut a = app();
        a.open_filter(false);
        choose(&mut a, Kind::Prop(Prop::Fitness));
        assert_eq!(
            a.filtering.dialog.as_ref().unwrap().stage,
            Stage::Prop(Prop::Fitness)
        );
        // An empty field says what to type and adds nothing.
        a.on_key(key(KeyCode::Enter));
        let d = a.filtering.dialog.as_ref().unwrap();
        assert_eq!(d.error.as_deref(), Some("Type a number."));
        assert!(!a.filtering.is_active());
        // Letters are not numbers.
        type_text(&mut a, "x1,5");
        assert_eq!(a.filtering.dialog.as_ref().unwrap().fields[0], "1,5");
        a.on_key(key(KeyCode::Enter));
        assert!(a.filtering.dialog.is_none());
        assert_eq!(
            a.filtering.filter.conditions(),
            [Condition::range(Prop::Fitness, 1.5, f64::INFINITY)]
        );

        // At most replaces the range of the same property.
        a.open_filter(false);
        choose(&mut a, Kind::Prop(Prop::Fitness));
        a.on_key(key(KeyCode::Right));
        type_text(&mut a, "3");
        a.on_key(key(KeyCode::Enter));
        assert_eq!(
            a.filtering.filter.conditions(),
            [Condition::range(Prop::Fitness, f64::NEG_INFINITY, 3.0)]
        );

        // Between needs two numbers, in either order.
        a.open_filter(false);
        choose(&mut a, Kind::Prop(Prop::Fitness));
        a.on_key(key(KeyCode::Right));
        a.on_key(key(KeyCode::Right));
        type_text(&mut a, "4");
        a.on_key(key(KeyCode::Enter));
        assert_eq!(
            a.filtering.dialog.as_ref().unwrap().error.as_deref(),
            Some("Type two numbers.")
        );
        a.on_key(key(KeyCode::Tab));
        type_text(&mut a, "2");
        a.on_key(key(KeyCode::Enter));
        assert_eq!(
            a.filtering.filter.conditions(),
            [Condition::range(Prop::Fitness, 2.0, 4.0)]
        );
        assert_eq!(a.filtering.selection().map(|s| s.matched), Some(2));
    }

    #[test]
    fn escape_goes_back_a_step_and_then_closes() {
        let mut a = app();
        a.open_filter(false);
        choose(&mut a, Kind::Prop(Prop::Merit));
        a.on_key(key(KeyCode::Esc));
        assert_eq!(a.filtering.dialog.as_ref().unwrap().stage, Stage::Kind);
        a.on_key(key(KeyCode::Esc));
        assert!(a.filtering.dialog.is_none());
        assert!(!a.filtering.is_active());
    }

    #[test]
    fn a_task_condition_comes_from_the_task_list() {
        let mut a = app();
        a.open_filter(false);
        choose(&mut a, Kind::NotPerforms);
        assert_eq!(
            a.filtering.dialog.as_ref().unwrap().stage,
            Stage::Task(false)
        );
        a.on_key(key(KeyCode::Enter));
        assert_eq!(
            a.filtering.filter.conditions(),
            [Condition::Task {
                task: 0,
                performs: false
            }]
        );
        // Cell 0 performs the one task, so cells 1 and 3 match.
        assert_eq!(a.filtering.selection().map(|s| s.matched), Some(2));
    }

    #[test]
    fn the_genotype_is_the_inspected_organisms() {
        let mut a = app();
        // Cell 3 is inspected in the fixture and holds genotype 7.
        a.open_filter(false);
        choose(&mut a, Kind::Genotype);
        assert!(a.filtering.dialog.is_none());
        assert_eq!(
            a.filtering.filter.conditions(),
            [Condition::Genotype { id: 7 }]
        );
        // Nothing inspected and the cursor on an empty cell: it says so.
        let mut b = app();
        b.select(None);
        b.cursor = (2, 0);
        b.open_filter(false);
        choose(&mut b, Kind::Genotype);
        assert!(b
            .filtering
            .dialog
            .as_ref()
            .unwrap()
            .error
            .as_deref()
            .unwrap()
            .starts_with("Inspect an organism"));
        // The cursor on an organism will do.
        b.cursor = (1, 0);
        b.on_key(key(KeyCode::Enter));
        assert_eq!(
            b.filtering.filter.conditions(),
            [Condition::Genotype { id: 8 }]
        );
    }

    #[test]
    fn the_region_starts_as_the_cells_in_view_and_is_typed_over() {
        let mut a = app();
        a.open_filter(false);
        choose(&mut a, Kind::Region);
        let d = a.filtering.dialog.as_ref().unwrap();
        assert_eq!(d.stage, Stage::Region);
        // No map has been drawn: the whole 3x2 world.
        assert_eq!(d.fields, ["0", "0", "2", "1"]);
        a.on_key(key(KeyCode::Tab));
        a.on_key(key(KeyCode::Tab));
        a.on_key(key(KeyCode::Backspace));
        type_text(&mut a, "0");
        a.on_key(key(KeyCode::Enter));
        assert_eq!(
            a.filtering.filter.conditions(),
            [Condition::Region {
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 1
            }]
        );
        assert_eq!(a.filtering.selection().map(|s| s.matched), Some(2));
        // Words are not numbers.
        a.open_filter(false);
        choose(&mut a, Kind::Region);
        a.on_key(key(KeyCode::Backspace));
        a.on_key(key(KeyCode::Enter));
        assert_eq!(
            a.filtering.dialog.as_ref().unwrap().error.as_deref(),
            Some("Type four whole numbers.")
        );
    }

    #[test]
    fn conditions_are_removed_from_the_list_and_cleared() {
        let mut a = app();
        a.edit_filter(|f| {
            f.set(Condition::Genotype { id: 7 });
            f.set(Condition::range(Prop::Fitness, 0.0, 9.0));
        });
        a.open_filter(true);
        assert_eq!(a.filtering.dialog.as_ref().unwrap().tab, Tab::Conditions);
        a.on_key(key(KeyCode::Delete));
        assert_eq!(
            a.filtering.filter.conditions(),
            [Condition::range(Prop::Fitness, 0.0, 9.0)]
        );
        a.on_key(key(KeyCode::Char('x')));
        assert!(!a.filtering.is_active());
        // With nothing left the dialog goes back to adding.
        assert_eq!(a.filtering.dialog.as_ref().unwrap().tab, Tab::Add);
        // `c` clears a longer list at once.
        a.edit_filter(|f| {
            f.set(Condition::Genotype { id: 7 });
            f.set(Condition::range(Prop::Fitness, 0.0, 9.0));
        });
        a.on_key(key(KeyCode::Tab));
        a.on_key(key(KeyCode::Char('c')));
        assert!(!a.filtering.is_active());
        // Tab on the kind list opens the conditions only when there are some.
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.filtering.dialog.as_ref().unwrap().tab, Tab::Add);
    }

    #[test]
    fn clearing_from_the_keyboard_and_the_palette_action() {
        let mut a = app();
        a.edit_filter(|f| f.set(Condition::Genotype { id: 7 }));
        a.on_key(key(KeyCode::Char('\\')));
        assert!(!a.filtering.is_active());
        assert!(a.filtering.selection().is_none());
        a.on_key(key(KeyCode::Char('/')));
        assert!(a.filtering.dialog.is_some());
    }

    #[test]
    fn filtering_only_changes_the_subscription_and_ends_with_the_run() {
        let mut a = app();
        a.edit_filter(|f| f.set(Condition::Genotype { id: 7 }));
        let cmds = a.take_commands();
        assert_eq!(cmds.len(), 1, "nothing but a subscription: {cmds:?}");
        match &cmds[0] {
            Command::Subscribe(sub) => {
                assert_eq!(sub.layers[0], LayerId::Genotype, "the map's layer first");
                for p in Prop::ALL {
                    assert!(sub.layers.contains(&p.layer()), "{p:?}");
                }
                assert_eq!(sub.layers.len(), 9);
                assert_eq!(sub.inspect_cell, Some(3));
            }
            c => panic!("{c:?}"),
        }
        assert!(a.filtering.selection().is_some());
        // Editing again with the same layers sends nothing.
        a.edit_filter(|f| f.set(Condition::Genotype { id: 8 }));
        assert!(a.take_commands().is_empty());
        // The dialog on a family needs the Ancestry layer, and gives it up.
        a.open_filter(false);
        choose(&mut a, Kind::Family);
        let cmds = a.take_commands();
        assert!(
            matches!(&cmds[..], [Command::Subscribe(s)] if s.layers.contains(&LayerId::Ancestry))
        );
        a.on_key(key(KeyCode::Esc));
        a.on_key(key(KeyCode::Esc));
        let cmds = a.take_commands();
        assert!(
            matches!(&cmds[..], [Command::Subscribe(s)] if !s.layers.contains(&LayerId::Ancestry))
        );
        // Clearing drops the property layers.
        a.clear_filter();
        let cmds = a.take_commands();
        assert!(
            matches!(&cmds[..], [Command::Subscribe(s)] if s.layers == vec![LayerId::Genotype])
        );
        // Another run starts without a filter.
        a.edit_filter(|f| f.set(Condition::Genotype { id: 7 }));
        a.reset_run(
            vec![],
            Arc::new(crate::config_view::tests::setup(Vec::new())),
        );
        assert!(!a.filtering.is_active() && a.filtering.selection().is_none());
        assert!(a.filtering.subset.is_empty());
    }

    #[test]
    fn a_property_dialog_subscribes_to_its_layer_until_it_closes() {
        let mut a = app();
        a.open_filter(false);
        choose(&mut a, Kind::Prop(Prop::Age));
        let cmds = a.take_commands();
        assert!(
            matches!(&cmds[..], [Command::Subscribe(s)] if s.layers == vec![LayerId::Genotype, LayerId::Age]),
            "{cmds:?}"
        );
        a.on_key(key(KeyCode::Esc));
        a.on_key(key(KeyCode::Esc));
        let cmds = a.take_commands();
        assert!(
            matches!(&cmds[..], [Command::Subscribe(s)] if s.layers == vec![LayerId::Genotype])
        );
    }

    #[test]
    fn the_subset_series_starts_with_the_filter_and_follows_frames() {
        let mut a = app();
        a.edit_filter(|f| f.set(Condition::Genotype { id: 7 }));
        assert_eq!(a.filtering.subset.rows().len(), 1);
        assert_eq!(a.filtering.subset.rows()[0].organisms, 2);
        let mut next = frame_with_values();
        next.update = 13;
        a.apply_frame(Arc::new(next));
        assert_eq!(a.filtering.subset.rows().len(), 2);
        a.edit_filter(|f| f.set(Condition::Genotype { id: 8 }));
        assert_eq!(
            a.filtering.subset.rows().len(),
            1,
            "a new filter starts over"
        );
        assert_eq!(a.filtering.subset.rows()[0].organisms, 1);
    }

    #[test]
    fn a_condition_without_its_layer_is_left_out_and_the_series_waits() {
        let mut a = app();
        // The fixture frame has no merit layer.
        a.edit_filter(|f| f.set(Condition::range(Prop::Merit, 0.0, 1.0)));
        let sel = a.filtering.selection().unwrap();
        assert_eq!(sel.ignored, vec![0]);
        assert!(a.filtering.subset.is_empty());
    }

    #[test]
    fn history_replaces_and_rewinds() {
        let mut h = SubsetHistory::default();
        let row = |u: i32, n: u32| SubsetRow {
            update: u,
            organisms: n,
            genotypes: 0,
            mean: [f64::NAN; 8],
            max_fitness: f64::NAN,
            max_merit: f64::NAN,
        };
        h.push(row(10, 1));
        h.push(row(20, 2));
        h.push(row(20, 3));
        assert_eq!(h.rows().len(), 2);
        assert_eq!(h.rows()[1].organisms, 3);
        h.push(row(15, 4));
        assert_eq!(
            h.rows().iter().map(|r| r.update).collect::<Vec<_>>(),
            [10, 15]
        );
    }

    #[test]
    fn only_measures_with_an_organism_counterpart_have_a_subset_line() {
        assert!(has_subset_value(Metric::AveFitness));
        assert!(has_subset_value(Metric::Organisms));
        assert!(!has_subset_value(Metric::Births));
        assert!(!has_subset_value(Metric::TotalOrganisms));
    }

    #[test]
    fn labels_use_the_desktop_front_ends_words() {
        let f = frame_with_values();
        let l = |c: Condition| condition_label(&c, Some(&f));
        assert_eq!(
            l(Condition::Task {
                task: 0,
                performs: true
            }),
            "Performs task Nand"
        );
        assert_eq!(
            l(Condition::Task {
                task: 0,
                performs: false
            }),
            "Does not perform task Nand"
        );
        assert_eq!(
            l(Condition::range(Prop::Fitness, 0.5, f64::INFINITY)),
            "Fitness ≥ 0.5"
        );
        assert_eq!(
            l(Condition::range(Prop::Merit, f64::NEG_INFINITY, 12.0)),
            "Merit ≤ 12"
        );
        assert_eq!(
            l(Condition::range(Prop::GenomeLen, 80.0, 105.0)),
            "Genome length 80 to 105"
        );
        assert_eq!(
            l(Condition::range(Prop::GenomeLen, 1500.0, 1500.0)),
            "Genome length = 1,500"
        );
        assert_eq!(l(Condition::Genotype { id: 7 }), "Genotype 050-aaaaa");
        assert_eq!(l(Condition::Genotype { id: 99 }), "Genotype #99");
        assert_eq!(l(Condition::Family { root: 8 }), "Family of 051-bbbbb");
        assert_eq!(
            l(Condition::region((4, 9), (1, 2))),
            "Cells x 1 to 4, y 2 to 9"
        );
        assert_eq!(
            condition_label(
                &Condition::Task {
                    task: 3,
                    performs: true
                },
                None
            ),
            "Performs task task 3"
        );
    }

    #[test]
    fn the_summary_counts_and_says_when_nothing_matches() {
        let sel = |matched, occupied| Selection {
            mask: Vec::new(),
            matched,
            occupied,
            ignored: Vec::new(),
            active: true,
        };
        assert_eq!(
            summary_text(&sel(12, 400), false),
            "12 of 400 organisms match (3%)"
        );
        assert_eq!(summary_text(&sel(12, 400), true), "12 of 400 match (3%)");
        assert_eq!(
            summary_text(&sel(1500, 3000), false),
            "1,500 of 3,000 organisms match (50%)"
        );
        assert_eq!(
            summary_text(&sel(1, 400), false),
            "1 of 400 organisms match (<1%)"
        );
        assert_eq!(summary_text(&sel(0, 400), false), "No organism matches");
        assert_eq!(
            summary_text(&sel(0, 0), false),
            "0 of 0 organisms match (0%)"
        );
    }

    #[test]
    fn a_property_shows_its_current_range() {
        let f = frame_with_values();
        let map = f.map.as_ref().unwrap();
        assert_eq!(prop_range(map, Prop::Fitness), Some((1.0, 4.0)));
        assert_eq!(prop_range(map, Prop::Age), None);
    }
}
