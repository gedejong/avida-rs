//! The knockout map and the reuse measures (a port-only extension, not in the
//! C++; the actions are `PrintKnockoutMap` and `PrintReuseData`, the
//! on-request analysis of the front ends is [`crate::World::knockout_map_of`]).
//!
//! This file holds the pure part: the data types, the classification of a
//! site from two test results, the measures, the static resolution of `call`
//! targets, and the text of the output files. `knockout_run.rs` runs the test
//! CPU.
//!
//! # The instrument
//!
//! A knockout replaces one site of a genome by the NULL instruction (the set's
//! `ActivateNullInst`, which `cAnalyzeGenotype::CalcKnockouts`, the
//! functional-modularity analysis and `PrintMODESData`'s skeletons use; the
//! instruction does nothing and is never produced by mutation), runs the
//! genome in the test CPU and compares with the unmodified genome run in the
//! same conditions. Lenski, Ofria, Pennock and Adami, "The evolutionary origin
//! of complex features", Nature 423, 2003, used knockouts to map which sites
//! the EQU computation depended on.
//!
//! # Site classes
//!
//! A site is, in this order:
//!
//! * **lethal** when the knockout no longer replicates (test-CPU fitness 0);
//! * **shared** when the knockout loses two or more of the tasks the base
//!   organism performs;
//! * **task** when it loses exactly one;
//! * **speed** when it loses no task but changes the fitness (usually lowers
//!   it);
//! * **neutral** when nothing measurable changes.
//!
//! A task counts as performed when the test organism's last-gestation count
//! of it is above zero; "lost" is a base-performed task whose count is zero
//! in the knockout.
//!
//! # The measures
//!
//! For an organism that performs `k >= 2` tasks: the set `S_t` of a task `t`
//! is the non-lethal sites whose knockout loses `t` (a lethal knockout loses
//! everything and says nothing about a task). Then
//!
//! * **shared sites** = the number of sites in two or more `S_t`;
//! * **pairwise overlap** `J(t,u) = |S_t & S_u| / |S_t | S_u|` (Jaccard; not
//!   defined when both sets are empty, i.e. both tasks are fully redundant);
//! * **reuse index** = the mean of the defined `J(t,u)` over the `k(k-1)/2`
//!   pairs (not defined when `k < 2` or no pair is defined);
//! * **sites per task** = `|S_t|`.
//!
//! Known weaknesses: (1) a knockout that breaks an early step of a pipeline
//! removes every later task, so a site "serves" tasks that only depend on it
//! as a predecessor and not as a subroutine they call; the measure cannot
//! tell a shared subroutine from a shared prefix. (2) Redundancy hides sites:
//! a computation present twice loses nothing when one copy is knocked out, so
//! a reused module can show no shared sites, and tasks with duplicated code
//! look separate. (3) A task that needs a site only through its fitness
//! (not through the count) is not seen. (4) One knockout at a time: epistasis
//! between two sites is not seen. (5) A task count of zero in the test CPU
//! may be because the test inputs gave the task no chance (the test uses the
//! default fixed inputs, which contain every combination of the three
//! inputs' bits in the key positions, so the nine logic tasks are all
//! testable).
//!
//! **Call sites** (when the instruction set has `call`): the number of sites
//! that are `call`; the number of distinct targets, found statically the way
//! `Inst_Call` finds them (the complement of the label of nops after the
//! `call`, searched forward without wrapping; a `call` with no label jumps to
//! the register BX, which is not known statically and counts as one target of
//! its own, the "dynamic" one); and how many call sites are non-neutral (any
//! class other than neutral, lethal included).

use std::fmt::Write as _;

/// A site's class (see the module documentation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SiteClass {
    Lethal,
    Neutral,
    /// Loses exactly one task.
    Task,
    /// Loses two or more tasks.
    Shared,
    /// Loses no task but changes the fitness.
    Speed,
}

impl SiteClass {
    /// Every class, in display order.
    pub const ALL: [SiteClass; 5] = [
        SiteClass::Lethal,
        SiteClass::Shared,
        SiteClass::Task,
        SiteClass::Speed,
        SiteClass::Neutral,
    ];

    /// A one-letter code (the terminal listing).
    pub fn letter(self) -> char {
        match self {
            SiteClass::Lethal => 'L',
            SiteClass::Neutral => '.',
            SiteClass::Task => 'T',
            SiteClass::Shared => 'S',
            SiteClass::Speed => 'R',
        }
    }

    /// The word used in the data file.
    pub fn name(self) -> &'static str {
        match self {
            SiteClass::Lethal => "lethal",
            SiteClass::Neutral => "neutral",
            SiteClass::Task => "task",
            SiteClass::Shared => "shared",
            SiteClass::Speed => "speed",
        }
    }
}

/// What one run of a genome in the test CPU gave.
#[derive(Clone, Debug, PartialEq)]
pub struct TestOutcome {
    /// `GetGenotypeFitness()`
    pub fitness: f64,
    /// `GetLastTaskCount()` of the test organism, one per environment task.
    pub task_counts: Vec<i32>,
    /// The inputs the test CPU used.
    pub inputs: Vec<i32>,
}

impl TestOutcome {
    /// Whether the genome replicates (test-CPU fitness above 0).
    pub fn viable(&self) -> bool {
        self.fitness > 0.0
    }
}

/// One site of the map.
#[derive(Clone, Debug, PartialEq)]
pub struct SiteKnockout {
    pub class: SiteClass,
    /// The knockout's fitness over the base fitness (0 when lethal).
    pub rel_fitness: f64,
    /// Indices (into [`KnockoutMap::tasks`]) of the base tasks the knockout
    /// no longer performs; for a lethal site, every task the base performs.
    pub lost: Vec<usize>,
}

/// A `call` site and where it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub pos: usize,
    /// The position of the label the call reaches; `None` for a `call`
    /// without a label (it jumps to BX) or whose label is not found.
    pub target: Option<usize>,
    /// Whether the call has no label at all (jump to BX).
    pub no_label: bool,
}

/// The knockout map of one genome, and what it is a map of.
#[derive(Clone, Debug, PartialEq)]
pub struct KnockoutMap {
    /// The genotype id when the genome is a tracker genotype's, else -1.
    pub genotype_id: i32,
    /// Update at which it was computed.
    pub update: i32,
    /// The instruction name of every site.
    pub inst_names: Vec<String>,
    /// The one-character symbol of every site.
    pub symbols: String,
    /// The names of the environment's tasks.
    pub tasks: Vec<String>,
    /// Whether the base genome replicates in the test CPU. If not, `sites`
    /// is empty and nothing else is defined.
    pub base_viable: bool,
    pub base_fitness: f64,
    /// Indices of the tasks the base organism performs.
    pub performed: Vec<usize>,
    /// The inputs of the test (identical for the base and every knockout).
    pub inputs: Vec<i32>,
    pub sites: Vec<SiteKnockout>,
    /// `Some` when the instruction set has `call`.
    pub calls: Option<Vec<CallSite>>,
}

/// The reuse measures of a map (see the module documentation).
#[derive(Clone, Debug, PartialEq)]
pub struct ReuseMeasures {
    /// The number of tasks the base performs.
    pub tasks_performed: usize,
    /// `None` when fewer than two tasks are performed.
    pub shared_sites: Option<usize>,
    /// The mean pairwise Jaccard overlap; `None` when not defined.
    pub reuse_index: Option<f64>,
    /// `(task a, task b, overlap)` for a < b in the performed tasks; the
    /// overlap is `None` for two empty site sets.
    pub pairs: Vec<(usize, usize, Option<f64>)>,
    /// `(task, |S_t|)` for the performed tasks.
    pub sites_per_task: Vec<(usize, usize)>,
    /// Counts per class, in [`SiteClass::ALL`] order.
    pub class_counts: [usize; 5],
    /// `Some` when the set has `call`.
    pub calls: Option<CallMeasures>,
}

/// The `call` indicators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallMeasures {
    pub call_sites: usize,
    /// Distinct targets (a `call` without a label counts as one dynamic
    /// target).
    pub distinct_targets: usize,
    /// Call sites of any class other than neutral.
    pub non_neutral: usize,
}

/// Classify one site from the base and the knockout's test results.
pub fn classify(base: &TestOutcome, ko: &TestOutcome) -> SiteKnockout {
    let performed: Vec<usize> = (0..base.task_counts.len())
        .filter(|&t| base.task_counts[t] > 0)
        .collect();
    if !ko.viable() {
        return SiteKnockout {
            class: SiteClass::Lethal,
            rel_fitness: 0.0,
            lost: performed,
        };
    }
    let lost: Vec<usize> = performed
        .into_iter()
        .filter(|&t| ko.task_counts.get(t).copied().unwrap_or(0) <= 0)
        .collect();
    let class = match lost.len() {
        0 if ko.fitness == base.fitness => SiteClass::Neutral,
        0 => SiteClass::Speed,
        1 => SiteClass::Task,
        _ => SiteClass::Shared,
    };
    SiteKnockout {
        class,
        rel_fitness: ko.fitness / base.fitness,
        lost,
    }
}

/// The sites of the map when the base does not replicate: none.
pub fn sites_from(
    base: &TestOutcome,
    len: usize,
    run: &mut dyn FnMut(usize) -> Result<TestOutcome, String>,
) -> Result<Vec<SiteKnockout>, String> {
    let mut sites = Vec::with_capacity(len);
    for i in 0..len {
        let ko = run(i)?;
        if ko.inputs != base.inputs {
            return Err(format!(
                "knockout of site {i} was tested on other inputs than the base organism \
                 ({:?} against {:?})",
                ko.inputs, base.inputs
            ));
        }
        sites.push(classify(base, &ko));
    }
    Ok(sites)
}

impl KnockoutMap {
    /// The sites in `S_t`: the non-lethal sites that lose task `t`.
    pub fn task_sites(&self, t: usize) -> Vec<usize> {
        self.sites
            .iter()
            .enumerate()
            .filter(|(_, s)| s.class != SiteClass::Lethal && s.lost.contains(&t))
            .map(|(i, _)| i)
            .collect()
    }

    /// The measures.
    pub fn measures(&self) -> ReuseMeasures {
        let k = self.performed.len();
        let mut class_counts = [0usize; 5];
        for s in &self.sites {
            let i = SiteClass::ALL
                .iter()
                .position(|&c| c == s.class)
                .unwrap_or(0);
            class_counts[i] += 1;
        }
        let sets: Vec<Vec<usize>> = self.performed.iter().map(|&t| self.task_sites(t)).collect();
        let sites_per_task = self
            .performed
            .iter()
            .zip(&sets)
            .map(|(&t, s)| (t, s.len()))
            .collect();
        let mut pairs = Vec::new();
        for a in 0..k {
            for b in a + 1..k {
                let inter = sets[a].iter().filter(|s| sets[b].contains(s)).count();
                let union = sets[a].len() + sets[b].len() - inter;
                let j = (union > 0).then(|| inter as f64 / union as f64);
                pairs.push((self.performed[a], self.performed[b], j));
            }
        }
        let (shared_sites, reuse_index) = if k >= 2 && self.base_viable {
            let shared = self
                .sites
                .iter()
                .filter(|s| s.class != SiteClass::Lethal && s.lost.len() >= 2)
                .count();
            let defined: Vec<f64> = pairs.iter().filter_map(|p| p.2).collect();
            let mean =
                (!defined.is_empty()).then(|| defined.iter().sum::<f64>() / defined.len() as f64);
            (Some(shared), mean)
        } else {
            (None, None)
        };
        let calls = self.calls.as_ref().map(|calls| {
            let mut targets: Vec<Option<usize>> = Vec::new();
            for c in calls {
                let t = if c.no_label { None } else { c.target };
                if !targets.contains(&t) {
                    targets.push(t);
                }
            }
            CallMeasures {
                call_sites: calls.len(),
                distinct_targets: targets.len(),
                non_neutral: calls
                    .iter()
                    .filter(|c| {
                        self.sites
                            .get(c.pos)
                            .is_some_and(|s| s.class != SiteClass::Neutral)
                    })
                    .count(),
            }
        });
        ReuseMeasures {
            tasks_performed: k,
            shared_sites,
            reuse_index,
            pairs,
            sites_per_task,
            class_counts,
            calls,
        }
    }

    /// What a hover or a status line says about one site: position,
    /// instruction, class, relative fitness and the tasks lost.
    pub fn site_text(&self, site: usize) -> String {
        let (Some(s), Some(name)) = (self.sites.get(site), self.inst_names.get(site)) else {
            return String::new();
        };
        let lost = |s: &SiteKnockout| {
            s.lost
                .iter()
                .map(|&t| self.tasks.get(t).map_or("?", String::as_str))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match s.class {
            SiteClass::Lethal => {
                format!("Site {site}, {name}: lethal, the knockout does not replicate")
            }
            SiteClass::Neutral => format!("Site {site}, {name}: neutral, nothing changes"),
            SiteClass::Speed => format!(
                "Site {site}, {name}: speed only, no task lost, fitness {:.0}% of the base",
                s.rel_fitness * 100.0
            ),
            SiteClass::Task | SiteClass::Shared => format!(
                "Site {site}, {name}: {}, loses {}, fitness {:.0}% of the base",
                s.class.name(),
                lost(s),
                s.rel_fitness * 100.0
            ),
        }
    }

    /// The tasks lost at `site` as a string of `0` and `1`, one character per
    /// environment task.
    pub fn lost_bits(&self, site: usize) -> String {
        let lost = self.sites.get(site).map(|s| &s.lost[..]).unwrap_or(&[]);
        (0..self.tasks.len())
            .map(|t| if lost.contains(&t) { '1' } else { '0' })
            .collect()
    }

    /// The map as the lines `PrintKnockoutMap` writes (comments, a column
    /// line, one line per site), each without a newline.
    pub fn file_lines(&self) -> Vec<String> {
        let mut v = Vec::new();
        let tasks = self.tasks.join(" ");
        v.push(format!(
            "# Knockout map: update {}, genotype {}, length {}, base fitness {}, base replicates {}",
            self.update,
            self.genotype_id,
            self.symbols.chars().count(),
            avida_apto::fmt_double(self.base_fitness),
            if self.base_viable { "yes" } else { "no" },
        ));
        let performed: Vec<&str> = self
            .performed
            .iter()
            .map(|&t| self.tasks[t].as_str())
            .collect();
        v.push(format!(
            "# Tasks performed by the base: {}",
            performed.join(" ")
        ));
        v.push(format!(
            "# Tasks (the bit string has one character per task, in this order; 1 = lost): {tasks}"
        ));
        if !self.base_viable {
            v.push("# The base organism does not replicate in the test CPU: no map.".to_string());
            return v;
        }
        v.push(format!(
            "# Test inputs (identical for the base and every knockout): {}",
            self.inputs
                .iter()
                .map(|i| format!("{i:#010x}"))
                .collect::<Vec<_>>()
                .join(" ")
        ));
        v.push("# Columns: site, instruction, class, relative fitness, tasks lost".to_string());
        for (i, s) in self.sites.iter().enumerate() {
            v.push(format!(
                "{} {} {} {} {}",
                i,
                self.inst_names[i],
                s.class.name(),
                avida_apto::fmt_double(s.rel_fitness),
                self.lost_bits(i)
            ));
        }
        v
    }
}

/// A float for a data file: `-1` for a measure that is not defined.
pub fn opt_f(v: Option<f64>) -> String {
    v.map_or_else(|| "-1".to_string(), avida_apto::fmt_double)
}

/// A count for a data file: `-1` for a measure that is not defined.
pub fn opt_n(v: Option<usize>) -> String {
    v.map_or_else(|| "-1".to_string(), |n| n.to_string())
}

/// A short text of the measures for a header line.
pub fn summary(m: &ReuseMeasures) -> String {
    let mut s = String::new();
    let _ = write!(s, "{} tasks", m.tasks_performed);
    match (m.shared_sites, m.reuse_index) {
        (Some(sh), idx) => {
            let _ = write!(s, ", {sh} shared sites, reuse index ");
            match idx {
                Some(i) => {
                    let _ = write!(s, "{i:.3}");
                }
                None => s.push_str("undefined"),
            }
        }
        (None, _) => s.push_str(", reuse undefined (needs two tasks)"),
    }
    if let Some(c) = m.calls {
        let _ = write!(
            s,
            ", {} call {}, {} {}, {} non-neutral",
            c.call_sites,
            if c.call_sites == 1 { "site" } else { "sites" },
            c.distinct_targets,
            if c.distinct_targets == 1 {
                "target"
            } else {
                "targets"
            },
            c.non_neutral
        );
    }
    s
}

// ----------------------------------------------------------------------
// Subroutine reuse
// ----------------------------------------------------------------------

/// "Subroutine reuse" (`PrintReuseSurvey`): the genome sites that execute in
/// the test CPU's base run while at least two distinct `call` sites are the
/// innermost active call (a site reached from two or more call sites), and
/// how many of them are non-neutral under knockout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubroutineReuse {
    /// Sites executed from two or more distinct innermost call sites.
    pub sites: usize,
    /// Of those, sites whose knockout class is not neutral (lethal
    /// included); `sites` when the map has no knockouts.
    pub non_neutral: usize,
}

/// The subroutine reuse from per-site counts of distinct innermost call
/// sites (`contexts`, see `avida_hardware::call_probe`) and the knockout
/// classes of the same sites (`classes` may be shorter, or empty when the
/// base was not viable; a site without a class is not counted non-neutral).
pub fn subroutine_reuse(contexts: &[usize], classes: &[SiteKnockout]) -> SubroutineReuse {
    let mut r = SubroutineReuse {
        sites: 0,
        non_neutral: 0,
    };
    for (i, &n) in contexts.iter().enumerate() {
        if n >= 2 {
            r.sites += 1;
            if classes
                .get(i)
                .is_some_and(|c| c.class != SiteClass::Neutral)
            {
                r.non_neutral += 1;
            }
        }
    }
    r
}

// ----------------------------------------------------------------------
// Call targets
// ----------------------------------------------------------------------

/// The `call` sites of a genome and their targets, as `Inst_Call` of the
/// heads CPU resolves them. `names[i]` is site `i`'s instruction name,
/// `nop_mods[i]` its register modifier when it is a nop, and `num_regs` the
/// modulus of label complements (`NUM_NOPS`).
pub fn call_sites(names: &[String], nop_mods: &[Option<i32>], num_regs: i32) -> Vec<CallSite> {
    let mut out = Vec::new();
    for (pos, name) in names.iter().enumerate() {
        if name != "call" {
            continue;
        }
        // `ReadLabel`: the nops that follow, at most `MAX_LENGTH` of them.
        let mut label = Vec::new();
        while label.len() < 10 {
            match nop_mods.get(pos + 1 + label.len()) {
                Some(Some(m)) => label.push(*m),
                _ => break,
            }
        }
        if label.is_empty() {
            out.push(CallSite {
                pos,
                target: None,
                no_label: true,
            });
            continue;
        }
        let complement: Vec<i32> = label.iter().map(|m| (m + 1) % num_regs.max(1)).collect();
        let target = find_label_forward(nop_mods, &complement, pos + label.len());
        out.push(CallSite {
            pos,
            target,
            no_label: false,
        });
    }
    out
}

/// `FindLabel_Forward`: the position of the first nop of the complement
/// label, searched forward from the instruction pointer `ip` (the last nop
/// of the call's own label), without wrapping.
fn find_label_forward(nop: &[Option<i32>], label: &[i32], ip: usize) -> Option<usize> {
    let size = nop.len();
    let l = label.len();
    let search_start = ip;
    let mut pos = ip + l;
    while pos < size {
        if nop[pos].is_some() {
            let mut start = pos;
            let mut end = pos + 1;
            while start > search_start && nop[start - 1].is_some() {
                start -= 1;
            }
            while end < size && nop[end].is_some() {
                end += 1;
            }
            if end - start >= l {
                for offset in start..=end - l {
                    if (0..l).all(|m| nop[offset + m] == Some(label[m])) {
                        return Some(offset);
                    }
                }
            }
            pos = end;
        }
        pos += l;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(fitness: f64, counts: &[i32]) -> TestOutcome {
        TestOutcome {
            fitness,
            task_counts: counts.to_vec(),
            inputs: vec![1, 2, 3],
        }
    }

    #[test]
    fn classes_follow_the_lost_tasks_before_the_fitness() {
        let base = out(2.0, &[1, 1, 1, 0]);
        let lethal = classify(&base, &out(0.0, &[0, 0, 0, 0]));
        assert_eq!(lethal.class, SiteClass::Lethal);
        assert_eq!(lethal.lost, vec![0, 1, 2]);
        assert_eq!(
            classify(&base, &out(2.0, &[1, 1, 1, 0])).class,
            SiteClass::Neutral
        );
        assert_eq!(
            classify(&base, &out(1.0, &[1, 1, 1, 0])).class,
            SiteClass::Speed
        );
        let one = classify(&base, &out(1.5, &[1, 0, 1, 0]));
        assert_eq!((one.class, one.lost.clone()), (SiteClass::Task, vec![1]));
        assert_eq!(one.rel_fitness, 0.75);
        // A task the base does not perform is never lost; a gained one is ignored.
        assert_eq!(
            classify(&base, &out(2.0, &[1, 1, 1, 5])).class,
            SiteClass::Neutral
        );
        assert_eq!(
            classify(&base, &out(2.0, &[0, 0, 1, 0])).class,
            SiteClass::Shared
        );
    }

    fn site(class: SiteClass) -> SiteKnockout {
        SiteKnockout {
            class,
            rel_fitness: 1.0,
            lost: Vec::new(),
        }
    }

    #[test]
    fn subroutine_reuse_counts_sites_with_two_or_more_contexts() {
        let classes = [
            site(SiteClass::Neutral),
            site(SiteClass::Lethal),
            site(SiteClass::Neutral),
            site(SiteClass::Speed),
            site(SiteClass::Shared),
        ];
        // Sites 1, 2 and 4 have two or more contexts, site 3 only one.
        let r = subroutine_reuse(&[0, 2, 3, 1, 2], &classes);
        assert_eq!((r.sites, r.non_neutral), (3, 2));
        // No knockout classes (base not viable): none counted non-neutral.
        let r = subroutine_reuse(&[0, 2, 3, 1, 2], &[]);
        assert_eq!((r.sites, r.non_neutral), (3, 0));
        // A straight-line run has no contexts.
        let r = subroutine_reuse(&[0, 0, 0], &classes);
        assert_eq!((r.sites, r.non_neutral), (0, 0));
    }

    fn map_of(classes: &[(SiteClass, &[usize])], performed: &[usize], tasks: usize) -> KnockoutMap {
        KnockoutMap {
            genotype_id: 1,
            update: 0,
            inst_names: vec!["x".into(); classes.len()],
            symbols: "a".repeat(classes.len()),
            tasks: (0..tasks).map(|t| format!("t{t}")).collect(),
            base_viable: true,
            base_fitness: 1.0,
            performed: performed.to_vec(),
            inputs: vec![],
            sites: classes
                .iter()
                .map(|(c, l)| SiteKnockout {
                    class: *c,
                    rel_fitness: 1.0,
                    lost: l.to_vec(),
                })
                .collect(),
            calls: None,
        }
    }

    #[test]
    fn measures_of_a_hand_made_map() {
        // Tasks 0,1,2. Site sets: S0 = {1,2,3}, S1 = {2,3}, S2 = {3,4}.
        let m = map_of(
            &[
                (SiteClass::Lethal, &[0, 1, 2]),
                (SiteClass::Task, &[0]),
                (SiteClass::Shared, &[0, 1]),
                (SiteClass::Shared, &[0, 1, 2]),
                (SiteClass::Task, &[2]),
                (SiteClass::Neutral, &[]),
            ],
            &[0, 1, 2],
            3,
        );
        let r = m.measures();
        assert_eq!(r.shared_sites, Some(2));
        assert_eq!(r.sites_per_task, vec![(0, 3), (1, 2), (2, 2)]);
        let j01 = 2.0 / 3.0;
        let j02 = 1.0 / 4.0;
        let j12 = 1.0 / 3.0;
        assert_eq!(r.pairs[0].2, Some(j01));
        assert_eq!(r.pairs[1].2, Some(j02));
        assert_eq!(r.pairs[2].2, Some(j12));
        assert!((r.reuse_index.unwrap() - (j01 + j02 + j12) / 3.0).abs() < 1e-12);
        assert_eq!(r.class_counts, [1, 2, 2, 0, 1]);
    }

    #[test]
    fn measures_are_undefined_below_two_tasks_and_for_empty_overlaps() {
        let m = map_of(
            &[(SiteClass::Task, &[0]), (SiteClass::Neutral, &[])],
            &[0],
            2,
        );
        let r = m.measures();
        assert_eq!((r.shared_sites, r.reuse_index), (None, None));
        let m = map_of(&[(SiteClass::Neutral, &[])], &[], 2);
        assert_eq!(m.measures().shared_sites, None);
        // Two performed tasks with no site at all (fully redundant): shared 0, index undefined.
        let m = map_of(&[(SiteClass::Neutral, &[])], &[0, 1], 2);
        let r = m.measures();
        assert_eq!((r.shared_sites, r.reuse_index), (Some(0), None));
        assert_eq!(r.pairs[0].2, None);
    }

    #[test]
    fn call_targets_follow_the_forward_complement_search() {
        // Six registers: label C C (2 2) complements to D D (3 3).
        let names: Vec<String> = [
            "call", "n", "n", "x", "call", "n", "n", "y", "call", "z", "n", "n", "ret",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let nop = |m: i32| Some(m);
        let mods = vec![
            None,
            nop(2),
            nop(2),
            None,
            None,
            nop(2),
            nop(2),
            None,
            None,
            None,
            nop(3),
            nop(3),
            None,
        ];
        let mut names = names;
        names[1] = "nop-C".into();
        names[2] = "nop-C".into();
        names[5] = "nop-C".into();
        names[6] = "nop-C".into();
        let calls = call_sites(&names, &mods, 6);
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].target, Some(10));
        assert_eq!(calls[1].target, Some(10));
        // The third call has no label: it jumps to BX.
        assert!(calls[2].no_label && calls[2].target.is_none());
        // A call after the subroutine does not find it (no wrapping).
        let names2: Vec<String> = ["nop-D", "nop-D", "call", "nop-C", "nop-C"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mods2 = vec![nop(3), nop(3), None, nop(2), nop(2)];
        assert_eq!(call_sites(&names2, &mods2, 6)[0].target, None);
        // With three registers the complement of C is A.
        let mods3 = vec![nop(0), nop(0), None, nop(2), nop(2)];
        assert_eq!(call_sites(&names2, &mods3, 3)[0].target, None);
    }

    #[test]
    fn file_lines_name_the_tasks_and_write_one_line_per_site() {
        let m = map_of(
            &[(SiteClass::Task, &[1]), (SiteClass::Neutral, &[])],
            &[1],
            3,
        );
        let l = m.file_lines();
        assert!(l.iter().any(|x| x.contains("t0 t1 t2")));
        assert_eq!(l.last().unwrap(), "1 x neutral 1 000");
        assert_eq!(l[l.len() - 2], "0 x task 1 010");
    }
}
