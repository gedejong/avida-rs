//! The top-navigation trace (avida-core/source at 47f13dadb):
//! `cPopulation::SetTopNavQ` and the `topnav_q` part of
//! `cPopulation::UpdateQs`, `cStats::UpdateTopNavTrace` and
//! `cStats::PrintTopNavTrace` (cStats.cc), behind the actions
//! `PrintTopNavTrace`, `PrintNavTrace` and `FlushTopNavTrace`
//! (PopulationActions.cc).
//!
//! `SetTopNavQ` switches `m_topnavtrace` on in every living organism. From
//! then on the experimental, GP8 and BCR hardware record the cell, facing
//! and update of each recorded execution beside the micro trace
//! (`RecordNavTrace`, hardware crate); the heads CPU and TransSMT record
//! nothing. Each reproduction of a queued organism offers it to
//! `UpdateTopNavTrace`, and once the queue holds at most one organism the
//! best one is printed to `navtrace.dat` and the queue is emptied.
//!
//! `cStats::firstnavtrace` is left out: only `cActionPrintNavTrace` sets it,
//! and that class is never registered (QUIRK: `PrintNavTrace` is registered
//! as `cActionPrintTopNavTrace`), so it is always false.

use avida_genome::Genome;
use avida_hardware::mini_trace::LentTraces;
use avida_hardware::organism::{OrgState, OrgUid};
use avida_systematics::unit::GroupRef;

use super::{comma_list, opinion_of};
use crate::world::World;

/// `cPopulation::topnav_q` and the `top*` members of `cStats`.
#[derive(Debug)]
pub struct TopNavState {
    /// `topnav_q`: the organisms still followed.
    pub queue: Vec<OrgUid>,
    /// `topreac`
    reac: i32,
    /// `topcycle`
    cycle: i32,
    /// `topgenid`
    genid: i32,
    /// `topid`
    id: i32,
    /// `topbirthud`
    birthud: i32,
    /// `toprepro`
    repro: i32,
    /// `topgenome`
    genome: Option<Genome>,
    /// `toptrace`
    trace: Vec<u8>,
    /// `topnavtraceloc`
    loc: Vec<i32>,
    /// `topnavtracefacing`
    facing: Vec<i32>,
    /// `topnavtraceupdate`
    update: Vec<i32>,
    /// `topstart`
    start: i32,
    /// `topreactions`
    reactions: Vec<i32>,
    /// `topreactioncycles`
    reaction_cycles: Vec<i32>,
    /// `topreactionexecs`
    reaction_execs: Vec<i32>,
    /// `toptarget`
    target: i32,
    /// `topgroup`
    group: i32,
}

impl Default for TopNavState {
    /// The `cStats` constructor sets `topreac` and `topcycle` to -1 and
    /// leaves the rest uninitialised; nothing reads them before
    /// `UpdateTopNavTrace` has set them.
    fn default() -> Self {
        TopNavState {
            queue: Vec::new(),
            reac: -1,
            cycle: -1,
            genid: 0,
            id: 0,
            birthud: 0,
            repro: 0,
            genome: None,
            trace: Vec::new(),
            loc: Vec::new(),
            facing: Vec::new(),
            update: Vec::new(),
            start: 0,
            reactions: Vec::new(),
            reaction_cycles: Vec::new(),
            reaction_execs: Vec::new(),
            target: 0,
            group: 0,
        }
    }
}

/// The parent whose `UpdateQs(parent, true)` is running: it is out of its
/// cell while it divides, so `PrintTopNavTrace`'s loop over the living
/// organisms takes it from here.
#[derive(Clone, Copy)]
pub(super) struct NavRunning<'a> {
    pub cell: i32,
    pub org: &'a OrgState,
    pub g: Option<GroupRef>,
    pub lent: &'a LentTraces,
}

/// `dst.Resize(src.size()); dst.SetAll(-1)`, then the first `n` entries
/// of `src`. `None` when `src` is shorter than `n` (the C++ reads and
/// writes past the end).
fn copy_prefix(src: &[i32], n: usize) -> Option<Vec<i32>> {
    if src.len() < n {
        return None;
    }
    let mut v = vec![-1; src.len()];
    v[..n].copy_from_slice(&src[..n]);
    Some(v)
}

impl World {
    /// `cPopulation::SetTopNavQ()`: follow every living organism.
    pub(super) fn set_top_nav_q(&mut self) {
        let live = self.pop.live_org_list.clone();
        let mut queue = Vec::with_capacity(live.len());
        for cell in live {
            let Some(org) = self
                .pop
                .orgs
                .get_mut(cell as usize)
                .and_then(Option::as_mut)
            else {
                self.set_fatal("SetTopNavQ: a living organism is not in its cell");
                return;
            };
            org.hardware.set_top_nav_trace(true);
            queue.push(org.state.uid);
        }
        self.mini_trace.top_nav.queue = queue;
        self.mini_trace.any_traced = true;
    }

    /// The `topnav_q` part of `UpdateQs(org, reproduced)`.
    pub(super) fn update_top_nav_q(
        &mut self,
        org: &OrgState,
        g: Option<GroupRef>,
        lent: &mut LentTraces,
        reproduced: bool,
        split: bool,
    ) {
        if !lent.topnavtrace || self.mini_trace.top_nav.queue.is_empty() {
            return;
        }
        let Some(i) = self
            .mini_trace
            .top_nav
            .queue
            .iter()
            .position(|&u| u == org.uid)
        else {
            return;
        };
        if reproduced {
            // Only a dividing parent reproduces, and it is the running
            // organism.
            let cell = self.pop.running.as_ref().map_or(-1, |r| r.cell_id);
            let running = NavRunning {
                cell,
                org,
                g,
                lent: &*lent,
            };
            self.update_top_nav_trace(org, g, lent, false, Some(running));
        }
        // `UpdateTopNavTrace` may have printed and emptied the queue.
        if (!reproduced || split) && !self.mini_trace.top_nav.queue.is_empty() {
            self.mini_trace.top_nav.queue.swap_remove(i);
            lent.topnavtrace = false;
        }
    }

    /// `cStats::UpdateTopNavTrace(org, force_update)`: keep `org` as the
    /// best so far if, among the organisms with the highest reaction
    /// triggered, it reproduced in the fewest cycles; print once at most
    /// one organism is still followed.
    fn update_top_nav_trace(
        &mut self,
        org: &OrgState,
        g: Option<GroupRef>,
        lent: &LentTraces,
        force_update: bool,
        running: Option<NavRunning<'_>>,
    ) {
        let p = &org.phenotype;
        let reaction_count = &p.cur_reaction_count;
        let best_reac = reaction_count
            .iter()
            .rposition(|&c| c > 0)
            .map_or(-1, |i| i as i32);
        let cycle = p.time_used;
        let top = &self.mini_trace.top_nav;
        // QUIRK: an organism whose best reaction is lower than the best so
        // far still wins when it took fewer cycles; one without any
        // reaction never wins while `topreac` is -1.
        let new_winner = if best_reac >= top.reac {
            (best_reac == top.reac && cycle < top.cycle) || best_reac > top.reac
        } else {
            cycle < top.cycle
        };
        if new_winner || force_update {
            let Some(g) = g else {
                self.set_fatal("UpdateTopNavTrace on an organism without a genotype");
                return;
            };
            let n = lent.microtracer.len();
            let (Some(loc), Some(facing), Some(update)) = (
                copy_prefix(&lent.navtraceloc, n),
                copy_prefix(&lent.navtracefacing, n),
                copy_prefix(&lent.navtraceupdate, n),
            ) else {
                self.set_fatal(
                    "UpdateTopNavTrace: the micro trace is longer than the navigation trace \
                     (C++: reads and writes past the end of an array)",
                );
                return;
            };
            let nr = reaction_count.len();
            let (Some(reaction_cycles), Some(reaction_execs)) = (
                copy_prefix(&p.first_reaction_cycles, nr),
                copy_prefix(&p.first_reaction_execs, nr),
            ) else {
                self.set_fatal(
                    "UpdateTopNavTrace: fewer first-reaction entries than reactions \
                     (C++: reads past the end of an array)",
                );
                return;
            };
            let genid = self.systematics.group_id(g).unwrap_or(-1);
            let genome =
                Genome::from_string(&self.systematics.property(g, "genome").string_value());
            let top = &mut self.mini_trace.top_nav;
            top.reac = best_reac;
            top.cycle = cycle;
            top.genid = genid;
            top.id = org.id;
            top.birthud = p.birth_update;
            top.repro = p.num_execs;
            top.genome = Some(genome);
            top.trace = lent.microtracer.clone();
            top.loc = loc;
            top.facing = facing;
            top.update = update;
            top.start = p.num_execs - n as i32;
            top.reaction_cycles = reaction_cycles;
            top.reaction_execs = reaction_execs;
            top.reactions = reaction_count.clone();
            top.target = org.parent_ft;
            top.group = opinion_of(org).unwrap_or(org.parent_group);
        }
        if self.mini_trace.top_nav.queue.len() <= 1 {
            if reaction_count.is_empty() && !self.pop.live_org_list.is_empty() {
                // `topreactions` stays empty, so `PrintTopNavTrace` updates
                // the living organisms again, which lands here again.
                self.set_fatal(
                    "PrintTopNavTrace in an environment without reactions \
                     (C++: PrintTopNavTrace and UpdateTopNavTrace recurse until the stack \
                     overflows)",
                );
                return;
            }
            self.print_top_nav_trace(true, running);
        }
    }

    /// `cStats::PrintTopNavTrace(flush)`: the best organism to
    /// `navtrace.dat` and its genome to
    /// `topnav_genome/org<id>-ud<born>-grp<group>_ft<target>-gt<genotype>.navgeno`.
    pub(super) fn print_top_nav_trace(&mut self, flush: bool, running: Option<NavRunning<'_>>) {
        match self.out.static_with_path("navtrace.dat") {
            Ok(df) => {
                // QUIRK: the comments only reach the header of the first
                // call; every later call starts with an empty line.
                df.write_comment("Org That Reproduced the Fastest (fewest cycles) Among Orgs with the Highest Reaction ID");
                df.write_time_stamp();
                df.write_comment("GenotypeID");
                df.write_comment("OrgID");
                df.write_comment(
                    "Cycle at First Reproduction (parallel multithread execs = 1 cycle)",
                );
                df.write_comment("Exec Count at Trace Start");
                df.write_comment("Exec Count at First Reproduction");
                df.write_comment("Reaction Counts at First Reproduction");
                df.write_comment("CPU Cycle at First Trigger of Each Reaction");
                df.write_comment(
                    "Exec Count at First Trigger (== index into execution trace and nav traces)",
                );
                df.write_comment("");
                df.write_comment(
                    "Updates for each entry in each following trace (to match with res data)",
                );
                df.write_comment("CellIDs to First Reproduction");
                df.write_comment("OrgFacings to First Reproduction");
                df.write_comment("Execution Trace to First Reproduction");
                df.endl();
            }
            Err(e) => {
                self.set_fatal(e);
                return;
            }
        }

        // In case nobody has reproduced, every living organism in turn
        // (QUIRK: each may print itself first, when at most one organism
        // is followed; the last one is printed again below).
        if self.mini_trace.top_nav.reactions.is_empty() {
            let live = self.pop.live_org_list.clone();
            for cell in live {
                match running {
                    Some(r) if r.cell == cell => {
                        self.update_top_nav_trace(r.org, r.g, r.lent, true, running);
                    }
                    _ => {
                        if self.pop.orgs.get(cell as usize).is_none_or(Option::is_none) {
                            self.set_fatal(
                                "PrintTopNavTrace: a living organism is not in its cell",
                            );
                        }
                        self.with_cell_traces(cell, |w, org, g, lent| {
                            w.update_top_nav_trace(org, g, lent, true, running);
                        });
                    }
                }
                if self.fatal.borrow().is_some() {
                    return;
                }
                self.mini_trace.top_nav.cycle = -1;
            }
        }

        let top = &self.mini_trace.top_nav;
        if !top.reactions.is_empty() {
            if top.loc.is_empty() {
                self.set_fatal(
                    "PrintTopNavTrace with an empty navigation trace (C++: reads before an \
                     empty array)",
                );
                return;
            }
            let trace: String = top.trace.iter().map(|&b| b as char).collect();
            let line = format!(
                "{} {} {} {} {} {} {} {} {} {} {} {}\n",
                top.genid,
                top.id,
                top.cycle,
                top.start,
                top.repro,
                comma_list(&top.reactions),
                comma_list(&top.reaction_cycles),
                comma_list(&top.reaction_execs),
                comma_list(&top.update),
                comma_list(&top.loc),
                comma_list(&top.facing),
                trace
            );
            let genfile = format!(
                "topnav_genome/org{}-ud{}-grp{}_ft{}-gt{}.navgeno",
                top.id, top.birthud, top.group, top.target, top.genid
            );
            let genome = top.genome.clone();
            match self.out.static_with_path("navtrace.dat") {
                Ok(df) => {
                    let fp = df.ofstream();
                    let _ = fp.write_all(line.as_bytes());
                    let _ = fp.flush();
                }
                Err(e) => {
                    self.set_fatal(e);
                    return;
                }
            }
            if let Some(genome) = genome {
                if let Err(e) = self.print_genome_file(&genome, &genfile) {
                    self.set_fatal(e);
                    return;
                }
            }
        }
        if flush {
            let top = &mut self.mini_trace.top_nav;
            top.reac = -1;
            top.cycle = -1;
            top.queue.clear();
        }
    }
}
