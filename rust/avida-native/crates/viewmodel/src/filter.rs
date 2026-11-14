//! Organism filters for linked charts and map ("brushing"): a set of
//! conditions joined by AND over per-organism properties a [`MapFrame`]
//! carries, the subset of organisms they select, and what the charts draw
//! from it (histogram counts, task shares, a scatter of two properties, one
//! row of subset means for a time series).
//!
//! A filter is view state. Nothing here reads a world, changes what is
//! captured or reaches a data file; the layers it needs are named by
//! [`Filter::layers`] so a front end can subscribe to them.

use std::collections::HashSet;

use crate::ancestry::decode;
use crate::legend::Scale;
use crate::model::{LayerId, MapFrame};

/// A numeric per-organism property: one map layer with a magnitude.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prop {
    Fitness,
    Merit,
    Gestation,
    GenomeLen,
    Age,
    Generation,
    TaskCount,
    Threads,
}

impl Prop {
    /// Every property, in the order a picker lists them.
    pub const ALL: [Prop; 8] = [
        Prop::Fitness,
        Prop::Merit,
        Prop::Gestation,
        Prop::GenomeLen,
        Prop::Age,
        Prop::Generation,
        Prop::TaskCount,
        Prop::Threads,
    ];

    /// Position in [`Prop::ALL`].
    pub fn index(self) -> usize {
        Prop::ALL.iter().position(|&p| p == self).unwrap_or(0)
    }

    /// The map layer that holds the property.
    pub fn layer(self) -> LayerId {
        match self {
            Prop::Fitness => LayerId::Fitness,
            Prop::Merit => LayerId::Merit,
            Prop::Gestation => LayerId::Gestation,
            Prop::GenomeLen => LayerId::GenomeLen,
            Prop::Age => LayerId::Age,
            Prop::Generation => LayerId::Generation,
            Prop::TaskCount => LayerId::TaskCount,
            Prop::Threads => LayerId::Threads,
        }
    }

    /// The property a layer shows, if it is one.
    pub fn from_layer(layer: LayerId) -> Option<Prop> {
        Prop::ALL.into_iter().find(|p| p.layer() == layer)
    }

    pub fn label(self) -> &'static str {
        match self {
            Prop::Fitness => "Fitness",
            Prop::Merit => "Merit",
            Prop::Gestation => "Gestation time",
            Prop::GenomeLen => "Genome length",
            Prop::Age => "Age",
            Prop::Generation => "Generation",
            Prop::TaskCount => "Tasks performed",
            Prop::Threads => "Threads",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Prop::Fitness => "Merit divided by gestation time.",
            Prop::Merit => "The organism's share of CPU time.",
            Prop::Gestation => "CPU cycles the organism needed for its last offspring.",
            Prop::GenomeLen => "Instructions in the genome.",
            Prop::Age => "Updates the organism has lived.",
            Prop::Generation => "Ancestors between the founder and the organism.",
            Prop::TaskCount => "Distinct tasks performed in the last gestation.",
            Prop::Threads => "CPU threads the organism runs.",
        }
    }

    /// Whether values are whole numbers.
    pub fn is_whole(self) -> bool {
        !matches!(self, Prop::Fitness | Prop::Merit)
    }

    /// The scale a chart starts with.
    pub fn default_scale(self) -> Scale {
        self.layer().default_scale()
    }
}

/// One condition of a [`Filter`].
#[derive(Clone, Debug, PartialEq)]
pub enum Condition {
    /// The occupant did (`performs`) or did not perform task `task` in its
    /// last gestation.
    Task { task: usize, performs: bool },
    /// `lo <= value <= hi`; an infinite bound is open.
    Range { prop: Prop, lo: f64, hi: f64 },
    /// The occupant belongs to this genotype.
    Genotype { id: i32 },
    /// The occupant is in the family (of the ancestry layer) whose root is
    /// this genotype.
    Family { root: i32 },
    /// The cell lies in a rectangle of cells, both corners included.
    Region { x0: i32, y0: i32, x1: i32, y1: i32 },
}

impl Condition {
    /// A range with `lo <= hi` whatever order the ends came in.
    pub fn range(prop: Prop, a: f64, b: f64) -> Condition {
        Condition::Range {
            prop,
            lo: a.min(b),
            hi: a.max(b),
        }
    }

    /// A region with the corners put in order.
    pub fn region(a: (i32, i32), b: (i32, i32)) -> Condition {
        Condition::Region {
            x0: a.0.min(b.0),
            y0: a.1.min(b.1),
            x1: a.0.max(b.0),
            y1: a.1.max(b.1),
        }
    }

    /// Whether `other` is a condition of the same kind about the same
    /// thing (adding one replaces the other).
    fn same_subject(&self, other: &Condition) -> bool {
        match (self, other) {
            (Condition::Task { task: a, .. }, Condition::Task { task: b, .. }) => a == b,
            (Condition::Range { prop: a, .. }, Condition::Range { prop: b, .. }) => a == b,
            (Condition::Genotype { .. }, Condition::Genotype { .. })
            | (Condition::Family { .. }, Condition::Family { .. })
            | (Condition::Region { .. }, Condition::Region { .. }) => true,
            _ => false,
        }
    }

    fn layers(&self) -> Option<LayerId> {
        match self {
            Condition::Range { prop, .. } => Some(prop.layer()),
            Condition::Family { .. } => Some(LayerId::Ancestry),
            _ => None,
        }
    }
}

/// Conditions joined by AND. Empty, it selects every organism.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    conds: Vec<Condition>,
}

impl Filter {
    pub fn new() -> Filter {
        Filter::default()
    }

    pub fn conditions(&self) -> &[Condition] {
        &self.conds
    }

    pub fn is_empty(&self) -> bool {
        self.conds.is_empty()
    }

    pub fn len(&self) -> usize {
        self.conds.len()
    }

    /// Adds a condition; one about the same thing (the same task, the
    /// same property, the genotype, the family or the region) is replaced.
    pub fn set(&mut self, c: Condition) {
        match self.conds.iter().position(|o| o.same_subject(&c)) {
            Some(i) => self.conds[i] = c,
            None => self.conds.push(c),
        }
    }

    /// Sets one side of the range on `prop`, keeping the other side of an
    /// existing range.
    pub fn set_bound(&mut self, prop: Prop, lo: Option<f64>, hi: Option<f64>) {
        let (old_lo, old_hi) = self
            .range_of(prop)
            .unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
        self.set(Condition::range(
            prop,
            lo.unwrap_or(old_lo),
            hi.unwrap_or(old_hi),
        ));
    }

    /// The range on `prop`, if there is one.
    pub fn range_of(&self, prop: Prop) -> Option<(f64, f64)> {
        self.conds.iter().find_map(|c| match c {
            Condition::Range { prop: p, lo, hi } if *p == prop => Some((*lo, *hi)),
            _ => None,
        })
    }

    /// What the filter says about task `task`: `Some(true)` for "performs",
    /// `Some(false)` for "does not perform".
    pub fn task_state(&self, task: usize) -> Option<bool> {
        self.conds.iter().find_map(|c| match c {
            Condition::Task { task: t, performs } if *t == task => Some(*performs),
            _ => None,
        })
    }

    /// A click on a task: sets the condition, or removes it when it is
    /// already that one.
    pub fn toggle_task(&mut self, task: usize, performs: bool) {
        if self.task_state(task) == Some(performs) {
            self.conds
                .retain(|c| !matches!(c, Condition::Task { task: t, .. } if *t == task));
        } else {
            self.set(Condition::Task { task, performs });
        }
    }

    /// A click on a genotype's bar: the genotype becomes the condition, or
    /// stops being it when it already is.
    pub fn toggle_genotype(&mut self, id: i32) {
        if self.conds.contains(&Condition::Genotype { id }) {
            self.conds.retain(|c| *c != Condition::Genotype { id });
        } else {
            self.set(Condition::Genotype { id });
        }
    }

    /// The same for the family rooted at genotype `root`.
    pub fn toggle_family(&mut self, root: i32) {
        if self.conds.contains(&Condition::Family { root }) {
            self.conds.retain(|c| *c != Condition::Family { root });
        } else {
            self.set(Condition::Family { root });
        }
    }

    /// Removes the range on `prop`, if any.
    pub fn clear_range(&mut self, prop: Prop) {
        self.conds
            .retain(|c| !matches!(c, Condition::Range { prop: p, .. } if *p == prop));
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.conds.len() {
            self.conds.remove(index);
        }
    }

    pub fn clear(&mut self) {
        self.conds.clear();
    }

    /// The map layers the conditions read, without repeats.
    pub fn layers(&self) -> Vec<LayerId> {
        let mut v: Vec<LayerId> = Vec::new();
        for l in self.conds.iter().filter_map(Condition::layers) {
            if !v.contains(&l) {
                v.push(l);
            }
        }
        v
    }

    /// The organisms of `map` that meet every condition. A condition whose
    /// layer is not in the frame (not captured, or not in a recording) is
    /// left out and listed in [`Selection::ignored`].
    pub fn select(&self, map: &MapFrame) -> Selection {
        let n = map.num_cells();
        let mut mask: Vec<bool> = map.genotype_id.iter().map(|&g| g != -1).collect();
        let occupied = mask.iter().filter(|&&m| m).count();
        let mut ignored = Vec::new();
        let w = usize::try_from(map.w).unwrap_or(0).max(1);
        for (i, c) in self.conds.iter().enumerate() {
            match c {
                Condition::Task { task, performs } => {
                    if *task >= map.num_tasks {
                        ignored.push(i);
                        continue;
                    }
                    for (cell, m) in mask.iter_mut().enumerate() {
                        *m &= map.task_done(cell, *task) == *performs;
                    }
                }
                Condition::Range { prop, lo, hi } => {
                    let Some(values) = map.layer(prop.layer()) else {
                        ignored.push(i);
                        continue;
                    };
                    for (m, &v) in mask.iter_mut().zip(values.iter()) {
                        let v = f64::from(v);
                        *m &= v.is_finite() && v >= *lo && v <= *hi;
                    }
                }
                Condition::Genotype { id } => {
                    for (m, &g) in mask.iter_mut().zip(map.genotype_id.iter()) {
                        *m &= g == *id;
                    }
                }
                Condition::Family { root } => {
                    let Some(values) = map.layer(LayerId::Ancestry) else {
                        ignored.push(i);
                        continue;
                    };
                    let slot = map
                        .families
                        .iter()
                        .find(|f| f.root == *root)
                        .map(|f| f.slot);
                    for (m, &v) in mask.iter_mut().zip(values.iter()) {
                        *m &= match (slot, decode(v)) {
                            (Some(s), Some((Some(of), _))) => of == s,
                            _ => false,
                        };
                    }
                }
                Condition::Region { x0, y0, x1, y1 } => {
                    for (cell, m) in mask.iter_mut().enumerate() {
                        let (x, y) = ((cell % w) as i32, (cell / w) as i32);
                        *m &= x >= *x0 && x <= *x1 && y >= *y0 && y <= *y1;
                    }
                }
            }
        }
        debug_assert_eq!(mask.len(), n);
        let matched = mask.iter().filter(|&&m| m).count();
        Selection {
            mask,
            matched,
            occupied,
            ignored,
            active: !self.conds.is_empty(),
        }
    }
}

/// The organisms a [`Filter`] selects, cell by cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    /// Per cell: the occupant meets every condition (false for empty cells).
    pub mask: Vec<bool>,
    /// Cells with `mask` set.
    pub matched: usize,
    /// Cells with an occupant.
    pub occupied: usize,
    /// Indices of the conditions that could not be applied.
    pub ignored: Vec<usize>,
    /// The filter had conditions.
    pub active: bool,
}

impl Selection {
    /// The share of organisms selected, 0 to 1 (0 with no organisms).
    pub fn share(&self) -> f64 {
        if self.occupied == 0 {
            0.0
        } else {
            self.matched as f64 / self.occupied as f64
        }
    }

    pub fn contains(&self, cell: usize) -> bool {
        self.mask.get(cell).copied().unwrap_or(false)
    }
}

/// Means of the selected organisms' properties in one frame, one row of the
/// subset's time series.
#[derive(Clone, Debug, PartialEq)]
pub struct SubsetRow {
    pub update: i32,
    pub organisms: u32,
    /// Distinct genotypes among them.
    pub genotypes: u32,
    /// Mean of each property ([`Prop::ALL`] order); NaN when the layer is
    /// missing or nobody is selected.
    pub mean: [f64; 8],
    pub max_fitness: f64,
    pub max_merit: f64,
}

impl SubsetRow {
    pub fn mean_of(&self, p: Prop) -> f64 {
        self.mean[p.index()]
    }
}

/// The subset's numbers in one frame.
pub fn subset_row(update: i32, map: &MapFrame, sel: &Selection) -> SubsetRow {
    let mut mean = [f64::NAN; 8];
    let mut max_fitness = f64::NAN;
    let mut max_merit = f64::NAN;
    for p in Prop::ALL {
        let Some(values) = map.layer(p.layer()) else {
            continue;
        };
        let (mut sum, mut n, mut max) = (0.0f64, 0u64, f64::NEG_INFINITY);
        for (&v, &m) in values.iter().zip(&sel.mask) {
            if m && v.is_finite() {
                sum += f64::from(v);
                n += 1;
                max = max.max(f64::from(v));
            }
        }
        if n > 0 {
            mean[p.index()] = sum / n as f64;
            match p {
                Prop::Fitness => max_fitness = max,
                Prop::Merit => max_merit = max,
                _ => {}
            }
        }
    }
    let genotypes: HashSet<i32> = map
        .genotype_id
        .iter()
        .zip(&sel.mask)
        .filter(|(_, &m)| m)
        .map(|(&g, _)| g)
        .collect();
    SubsetRow {
        update,
        organisms: sel.matched as u32,
        genotypes: genotypes.len() as u32,
        mean,
        max_fitness,
        max_merit,
    }
}

/// How many organisms perform each task, over the selected ones (or all
/// organisms with `mask` `None`).
pub fn task_counts(map: &MapFrame, mask: Option<&[bool]>) -> Vec<u32> {
    let mut counts = vec![0u32; map.num_tasks];
    for cell in 0..map.num_cells() {
        let on = match mask {
            Some(m) => m.get(cell).copied().unwrap_or(false),
            None => map.genotype_id[cell] != -1,
        };
        if !on {
            continue;
        }
        for (k, c) in counts.iter_mut().enumerate() {
            if map.task_done(cell, k) {
                *c += 1;
            }
        }
    }
    counts
}

/// One organism in a scatter plot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScatterPoint {
    pub cell: u32,
    pub x: f32,
    pub y: f32,
}

/// The organisms with a finite value for both properties. Empty when a
/// layer is missing.
pub fn scatter_points(map: &MapFrame, x: Prop, y: Prop) -> Vec<ScatterPoint> {
    let (Some(xs), Some(ys)) = (map.layer(x.layer()), map.layer(y.layer())) else {
        return Vec::new();
    };
    xs.iter()
        .zip(ys.iter())
        .enumerate()
        .filter(|(_, (a, b))| a.is_finite() && b.is_finite())
        .map(|(cell, (&x, &y))| ScatterPoint {
            cell: cell as u32,
            x,
            y,
        })
        .collect()
}

/// At most `cap` points, chosen without randomness: the selected organisms
/// first (every one while they fit, else every k-th), then every k-th of
/// the rest to fill what is left. Cell order is kept.
pub fn thin_points(
    points: &[ScatterPoint],
    sel: Option<&Selection>,
    cap: usize,
) -> Vec<ScatterPoint> {
    if points.len() <= cap {
        return points.to_vec();
    }
    let chosen = |p: &ScatterPoint| sel.is_some_and(|s| s.contains(p.cell as usize));
    let picked: Vec<&ScatterPoint> = points.iter().filter(|p| chosen(p)).collect();
    let others: Vec<&ScatterPoint> = points.iter().filter(|p| !chosen(p)).collect();
    let stride = |v: &[&ScatterPoint], room: usize| -> Vec<ScatterPoint> {
        if room == 0 {
            return Vec::new();
        }
        let k = v.len().div_ceil(room).max(1);
        v.iter().step_by(k).map(|p| **p).collect()
    };
    let mut out = stride(&picked, cap);
    let room = cap.saturating_sub(out.len());
    out.extend(stride(&others, room));
    out.sort_by_key(|p| p.cell);
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::ancestry::Family;
    use crate::model::GenotypeCells;

    /// A 4 x 2 world. Cells 0, 1, 2, 4, 5 hold organisms: genotypes 7, 7,
    /// 8, 8, 9. Task 0 is performed by cells 0 and 2, task 1 by cell 5.
    fn map() -> MapFrame {
        let nan = f32::NAN;
        MapFrame {
            w: 4,
            h: 2,
            genotype_id: vec![7, 7, 8, -1, 8, 9, -1, -1].into(),
            top_genotypes: vec![7, 8, 9].into(),
            layers: vec![
                (
                    LayerId::Fitness,
                    Arc::from(vec![1.0, 2.0, 4.0, nan, 8.0, 16.0, nan, nan]),
                ),
                (
                    LayerId::GenomeLen,
                    Arc::from(vec![50.0, 50.0, 60.0, nan, 60.0, 70.0, nan, nan]),
                ),
                (
                    LayerId::Ancestry,
                    Arc::from(vec![0.2, 0.3, 1.1, nan, 1.4, 1.5, nan, nan]),
                ),
            ],
            num_tasks: 2,
            task_bits: vec![0b01, 0, 0b01, 0, 0, 0b10, 0, 0].into(),
            words_per_cell: 1,
            top_counts: vec![2, 2, 1].into(),
            genotype_cells: vec![
                GenotypeCells {
                    id: 7,
                    first_cell: 0,
                    cells: 2,
                },
                GenotypeCells {
                    id: 8,
                    first_cell: 2,
                    cells: 2,
                },
                GenotypeCells {
                    id: 9,
                    first_cell: 5,
                    cells: 1,
                },
            ]
            .into(),
            families: Arc::from(vec![
                Family {
                    root: 7,
                    organisms: 2,
                    slot: 0,
                },
                Family {
                    root: 8,
                    organisms: 3,
                    slot: 1,
                },
            ]),
        }
    }

    fn cells(sel: &Selection) -> Vec<usize> {
        (0..sel.mask.len()).filter(|&c| sel.mask[c]).collect()
    }

    #[test]
    fn an_empty_filter_selects_every_organism_and_no_empty_cell() {
        let s = Filter::new().select(&map());
        assert_eq!(cells(&s), vec![0, 1, 2, 4, 5]);
        assert_eq!((s.matched, s.occupied), (5, 5));
        assert!(!s.active);
        assert!((s.share() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn each_kind_of_condition_selects_what_it_says() {
        let m = map();
        let one = |c: Condition| {
            let mut f = Filter::new();
            f.set(c);
            cells(&f.select(&m))
        };
        assert_eq!(
            one(Condition::Task {
                task: 0,
                performs: true
            }),
            vec![0, 2]
        );
        assert_eq!(
            one(Condition::Task {
                task: 0,
                performs: false
            }),
            vec![1, 4, 5],
            "empty cells perform nothing but are not organisms"
        );
        assert_eq!(
            one(Condition::range(Prop::Fitness, 2.0, 8.0)),
            vec![1, 2, 4]
        );
        assert_eq!(
            one(Condition::range(Prop::GenomeLen, 60.0, 60.0)),
            vec![2, 4]
        );
        assert_eq!(one(Condition::Genotype { id: 8 }), vec![2, 4]);
        assert_eq!(one(Condition::Family { root: 8 }), vec![2, 4, 5]);
        assert_eq!(one(Condition::Family { root: 99 }), Vec::<usize>::new());
        assert_eq!(one(Condition::region((1, 0), (2, 1))), vec![1, 2, 5]);
        assert_eq!(one(Condition::region((2, 1), (1, 0))), vec![1, 2, 5]);
    }

    #[test]
    fn conditions_are_joined_by_and_and_counted() {
        let m = map();
        let mut f = Filter::new();
        f.set(Condition::Task {
            task: 0,
            performs: false,
        });
        f.set(Condition::range(Prop::Fitness, 3.0, f64::INFINITY));
        let s = f.select(&m);
        assert_eq!(cells(&s), vec![4, 5]);
        assert_eq!((s.matched, s.occupied), (2, 5));
        assert!((s.share() - 0.4).abs() < 1e-12);
        assert!(s.active);
        f.set(Condition::Genotype { id: 7 });
        assert_eq!(f.select(&m).matched, 0);
    }

    #[test]
    fn a_condition_without_its_layer_is_left_out_and_listed() {
        let mut m = map();
        m.layers.retain(|(l, _)| *l != LayerId::Ancestry);
        let mut f = Filter::new();
        f.set(Condition::Family { root: 8 });
        f.set(Condition::Genotype { id: 7 });
        f.set(Condition::range(Prop::Merit, 1.0, 2.0));
        let s = f.select(&m);
        assert_eq!(s.ignored, vec![0, 2]);
        assert_eq!(cells(&s), vec![0, 1]);
        f.set(Condition::Task {
            task: 5,
            performs: true,
        });
        assert!(f.select(&m).ignored.contains(&3));
    }

    #[test]
    fn setting_replaces_the_same_subject_and_tasks_toggle() {
        let mut f = Filter::new();
        f.set(Condition::range(Prop::Fitness, 1.0, 2.0));
        f.set(Condition::range(Prop::Age, 1.0, 2.0));
        f.set(Condition::range(Prop::Fitness, 5.0, 9.0));
        assert_eq!(f.len(), 2);
        assert_eq!(f.range_of(Prop::Fitness), Some((5.0, 9.0)));
        f.set_bound(Prop::Fitness, Some(6.0), None);
        assert_eq!(f.range_of(Prop::Fitness), Some((6.0, 9.0)));
        f.set_bound(Prop::Merit, None, Some(3.0));
        assert_eq!(f.range_of(Prop::Merit), Some((f64::NEG_INFINITY, 3.0)));
        f.set(Condition::region((0, 0), (1, 1)));
        f.set(Condition::region((2, 2), (3, 3)));
        assert_eq!(f.len(), 4);

        f.toggle_genotype(4);
        f.toggle_genotype(5);
        assert_eq!(f.len(), 5, "one genotype condition");
        f.toggle_genotype(5);
        assert_eq!(f.len(), 4);
        f.toggle_family(2);
        f.toggle_family(2);
        f.clear_range(Prop::Merit);
        assert_eq!(f.range_of(Prop::Merit), None);
        assert_eq!(f.len(), 3);
        f.set(Condition::range(Prop::Merit, 0.0, 1.0));
        f.set(Condition::region((0, 0), (1, 1)));
        f.toggle_task(1, true);
        assert_eq!(f.task_state(1), Some(true));
        f.toggle_task(1, false);
        assert_eq!(f.task_state(1), Some(false));
        f.toggle_task(1, false);
        assert_eq!(f.task_state(1), None);
        f.remove(0);
        assert_eq!(f.range_of(Prop::Fitness), None);
        f.clear();
        assert!(f.is_empty());
    }

    #[test]
    fn the_layers_a_filter_needs_are_named_once() {
        let mut f = Filter::new();
        f.set(Condition::range(Prop::Fitness, 1.0, 2.0));
        f.set(Condition::Family { root: 1 });
        f.set(Condition::Genotype { id: 1 });
        f.set(Condition::Task {
            task: 0,
            performs: true,
        });
        assert_eq!(f.layers(), vec![LayerId::Fitness, LayerId::Ancestry]);
    }

    #[test]
    fn the_histogram_overlay_counts_the_subset_in_the_same_bins() {
        use crate::series::Histogram;
        let m = map();
        let values = m.layer(LayerId::Fitness).unwrap();
        let h = Histogram::new(values, 4, Scale::Linear);
        let mut f = Filter::new();
        f.set(Condition::Genotype { id: 8 });
        let sel = f.select(&m);
        let sub = h.counts_where(values, &sel.mask);
        assert_eq!(sub.len(), 4);
        assert_eq!(sub.iter().sum::<u32>(), 2);
        for (s, all) in sub.iter().zip(&h.counts) {
            assert!(s <= all);
        }
        let none = h.counts_where(values, &Filter::new().select(&m).mask);
        assert_eq!(none, h.counts);
    }

    #[test]
    fn task_shares_compare_the_subset_with_everyone() {
        let m = map();
        assert_eq!(task_counts(&m, None), vec![2, 1]);
        let mut f = Filter::new();
        f.set(Condition::Genotype { id: 8 });
        let s = f.select(&m);
        assert_eq!(task_counts(&m, Some(&s.mask)), vec![1, 0]);
    }

    #[test]
    fn the_subset_row_holds_means_counts_and_maxima() {
        let m = map();
        let mut f = Filter::new();
        f.set(Condition::range(Prop::Fitness, 4.0, 16.0));
        let s = f.select(&m);
        let r = subset_row(30, &m, &s);
        assert_eq!((r.update, r.organisms, r.genotypes), (30, 3, 2));
        assert!((r.mean_of(Prop::Fitness) - 28.0 / 3.0).abs() < 1e-9);
        assert!((r.mean_of(Prop::GenomeLen) - 190.0 / 3.0).abs() < 1e-9);
        assert!(r.mean_of(Prop::Merit).is_nan(), "no merit layer");
        assert_eq!(r.max_fitness, 16.0);
        let mut none = Filter::new();
        none.set(Condition::Genotype { id: 1000 });
        let r = subset_row(1, &m, &none.select(&m));
        assert_eq!(r.organisms, 0);
        assert!(r.mean_of(Prop::Fitness).is_nan());
    }

    #[test]
    fn scatter_points_need_both_layers_and_thinning_keeps_the_selected() {
        let m = map();
        assert_eq!(scatter_points(&m, Prop::Fitness, Prop::GenomeLen).len(), 5);
        assert!(scatter_points(&m, Prop::Fitness, Prop::Merit).is_empty());
        let pts: Vec<ScatterPoint> = (0..1000)
            .map(|i| ScatterPoint {
                cell: i,
                x: i as f32,
                y: 1.0,
            })
            .collect();
        let sel = Selection {
            mask: (0..1000).map(|i| i % 100 == 3).collect(),
            matched: 10,
            occupied: 1000,
            ignored: Vec::new(),
            active: true,
        };
        let thin = thin_points(&pts, Some(&sel), 100);
        assert!(thin.len() <= 100);
        assert_eq!(
            thin.iter()
                .filter(|p| sel.contains(p.cell as usize))
                .count(),
            10
        );
        assert_eq!(thin, thin_points(&pts, Some(&sel), 100), "deterministic");
        assert!(thin.windows(2).all(|w| w[0].cell < w[1].cell));
        assert_eq!(thin_points(&pts, None, 2000).len(), 1000);
    }
}
