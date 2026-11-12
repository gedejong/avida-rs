//! Mini and micro traces: the trace queues of `cPopulation`
//! (avida-core/source/main/cPopulation.cc at 47f13dadb: `SetMiniTraceQueue`,
//! `AppendMiniTraces`, `LoadMiniTraceQ`, `SetTraceQ`, `SetRandom*TraceQ`,
//! `SetNext{Prey,Pred}Q`, `TestForMiniTrace`, `SetupMiniTrace`,
//! `PrintMiniTraceGenome`, `UpdateQs`, `AppendRecordReproQ`), the trace
//! printers of `cStats` (`PrintMicroTraces`, `PrintMiniTraceReactions`,
//! `PrintReproData`), the `cHardwareBase` trace calls the population makes
//! (`SetMiniTrace`, `SetMicroTrace`, `PrintMicroTrace`, `DeleteMiniTrace`)
//! and the actions `PrintMiniTraces`, `PrintMicroTraces`, `LoadMiniTraceQ`
//! and `PrintReproData` (PopulationActions.cc). The top-navigation trace
//! (`PrintTopNavTrace`, `PrintNavTrace`, `FlushTopNavTrace`) is in
//! [`nav_trace`].
//!
//! # Reaching an organism's traces
//! The traces live in the organism's hardware ([`LentTraces`] are the
//! `cHardwareBase` members involved). For an organism that is not running,
//! they are lent out of its hardware while the population works on them.
//! A dividing parent is running; its hardware lends them for the duration
//! of the divide ([`avida_hardware::mini_trace::divide_lending_traces`]),
//! and they wait in [`MiniTraceState::lent`]. The only other way a running
//! organism meets this code is `KillOrganism` outside a divide (old age,
//! an instruction that kills it): then `UpdateQs(org, false)` waits for the
//! end of the step, when the organism's hardware is reachable again. The
//! micro trace it prints is the same then (a killed organism records
//! nothing more), its mini trace receives nothing more (see the comment at
//! the `ToDelete` check of the experimental hardware), and `microtrace.dat`
//! lines that other deaths print meanwhile are held back so the file keeps
//! the C++ order.

mod nav_trace;

use avida_apto::AvidaRng;
use avida_config::feedback::Feedback;
use avida_config::init_file::InitFile;
use avida_config::string_util::as_int;
use avida_genome::{ArgContainer, ArgSchema, Genome};
use avida_hardware::mini_trace::{LentTraces, MiniTracePrinter};
use avida_hardware::organism::{OrgState, Organism};
use avida_hardware::test_cpu::TestCpu;
use avida_phenotype::phenotype::DIVIDE_METHOD_SPLIT;
use avida_systematics::unit::{GroupMembership, GroupRef};

use crate::world::{make_test_world, World};

/// Action names (the registered spellings).
pub const MINI_TRACE_ACTIONS: &[&str] = &[
    "PrintMiniTraces",
    "PrintMicroTraces",
    "LoadMiniTraceQ",
    "PrintReproData",
    "PrintTopNavTrace",
    "PrintNavTrace",
    "FlushTopNavTrace",
];

/// The trace members of `cPopulation`.
#[derive(Debug, Default)]
pub struct MiniTraceState {
    /// `minitrace_queue`: genotype ids waiting for an organism to trace.
    pub queue: Vec<i32>,
    /// `print_mini_trace_genomes`
    pub print_genomes: bool,
    /// `print_mini_trace_reacs`
    pub print_reacs: bool,
    /// `use_micro_traces`
    pub use_micro: bool,
    /// `m_next_prey_q`
    pub next_prey_q: i32,
    /// `m_next_pred_q`
    pub next_pred_q: i32,
    /// `repro_q`: the organisms `PrintReproData` follows.
    pub repro_q: Vec<avida_hardware::organism::OrgUid>,
    /// Some organism has had a trace switched on (until then `UpdateQs`
    /// has nothing to do).
    any_traced: bool,
    /// The traces of the running parent while it divides.
    pub(crate) lent: Option<LentTraces>,
    /// The running organism was killed outside a divide: its
    /// `UpdateQs(org, false)` runs when the step ends.
    deferred_kill: bool,
    /// `microtrace.dat` lines printed while `deferred_kill` is pending.
    held_micro: Vec<String>,
    /// `InjectGroup ... trace=1`: the inject now in progress ends with
    /// `SetupMiniTrace` on the injected organism.
    pub(crate) inject_trace: bool,
    /// `topnav_q` and the `top*` members of `cStats`.
    pub(crate) top_nav: nav_trace::TopNavState,
}

/// A parsed action. Field names follow the C++ members (`m_` dropped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiniTraceAction {
    /// `cActionPrintMiniTraces`
    PrintMiniTraces {
        random: bool,
        save_dominants: bool,
        save_groups: bool,
        save_foragers: bool,
        orgs_per: i32,
        max_samples: i32,
        print_genomes: bool,
        print_reacs: bool,
    },
    /// `cActionPrintMicroTraces`
    PrintMicroTraces {
        random: bool,
        rand_prey: bool,
        rand_pred: bool,
        next_prey: i32,
        next_pred: i32,
        save_dominants: bool,
        save_groups: bool,
        save_foragers: bool,
        orgs_per: i32,
        max_samples: i32,
        print_genomes: bool,
        print_reacs: bool,
    },
    /// `cActionLoadMiniTraceQ`
    LoadMiniTraceQ {
        filename: String,
        orgs_per: i32,
        print_genomes: bool,
    },
    /// `cActionPrintReproData`
    PrintReproData,
    /// `cActionPrintTopNavTrace` (QUIRK: also registered as
    /// `PrintNavTrace`)
    PrintTopNavTrace,
    /// `cActionFlushTopNavTrace`
    FlushTopNavTrace,
}

/// The constructor of `cAction<name>`, or `None` when `name` is not one of
/// [`MINI_TRACE_ACTIONS`]. As in the other schema-based actions, a failed
/// argument load keeps the member defaults.
pub fn parse(name: &str, args: &str, feedback: &mut Feedback) -> Option<MiniTraceAction> {
    let bool_entry = |schema: &mut ArgSchema, n: &str, idx: i32, def: i32| {
        schema.add_int_range_default(n, idx, 0, 1, def);
    };
    Some(match name {
        "PrintMiniTraces" => {
            let mut schema = ArgSchema::new(':', '=', false);
            bool_entry(&mut schema, "random", 0, 0);
            bool_entry(&mut schema, "save_dominants", 1, 0);
            bool_entry(&mut schema, "save_groups", 2, 0);
            bool_entry(&mut schema, "save_foragers", 3, 0);
            schema.add_int("orgs_per", 4, 1);
            schema.add_int("max_samples", 5, 0);
            bool_entry(&mut schema, "print_genomes", 6, 1);
            bool_entry(&mut schema, "print_reacs", 7, 0);
            let c = ArgContainer::load(args, &schema, feedback);
            let int = |i: i32, def: i32| c.as_ref().map_or(def, |c| c.get_int(i));
            MiniTraceAction::PrintMiniTraces {
                random: int(0, 0) != 0,
                save_dominants: int(1, 0) != 0,
                save_groups: int(2, 0) != 0,
                save_foragers: int(3, 0) != 0,
                orgs_per: int(4, 1),
                max_samples: int(5, 0),
                print_genomes: int(6, 1) != 0,
                print_reacs: int(7, 0) != 0,
            }
        }
        "PrintMicroTraces" => {
            let mut schema = ArgSchema::new(':', '=', false);
            bool_entry(&mut schema, "random", 0, 0);
            bool_entry(&mut schema, "rand_prey", 1, 0);
            bool_entry(&mut schema, "rand_pred", 2, 0);
            schema.add_int("next_prey", 3, 0);
            schema.add_int("next_pred", 4, 0);
            bool_entry(&mut schema, "save_dominants", 5, 0);
            bool_entry(&mut schema, "save_groups", 6, 0);
            bool_entry(&mut schema, "save_foragers", 7, 0);
            schema.add_int("orgs_per", 8, 1);
            schema.add_int("max_samples", 9, 0);
            bool_entry(&mut schema, "print_genomes", 10, 0);
            bool_entry(&mut schema, "print_reacs", 11, 0);
            let c = ArgContainer::load(args, &schema, feedback);
            let int = |i: i32, def: i32| c.as_ref().map_or(def, |c| c.get_int(i));
            MiniTraceAction::PrintMicroTraces {
                random: int(0, 0) != 0,
                rand_prey: int(1, 0) != 0,
                rand_pred: int(2, 0) != 0,
                next_prey: int(3, 0),
                next_pred: int(4, 0),
                save_dominants: int(5, 0) != 0,
                save_groups: int(6, 0) != 0,
                save_foragers: int(7, 0) != 0,
                orgs_per: int(8, 1),
                max_samples: int(9, 0),
                print_genomes: int(10, 1) != 0,
                print_reacs: int(11, 0) != 0,
            }
        }
        "LoadMiniTraceQ" => {
            // QUIRK: `orgs_per` and the string `file` share index 0 of their
            // kinds, which is harmless; `print_reacs` (int 2) is parsed but
            // `Process` passes false.
            let mut schema = ArgSchema::new(':', '=', false);
            schema.add_string("file", 0, "genotype_ids");
            schema.add_int("orgs_per", 0, 1);
            bool_entry(&mut schema, "print_genomes", 1, 1);
            bool_entry(&mut schema, "print_reacs", 2, 0);
            match ArgContainer::load(args, &schema, feedback) {
                Some(c) => MiniTraceAction::LoadMiniTraceQ {
                    filename: c.get_string(0).to_string(),
                    orgs_per: c.get_int(0),
                    print_genomes: c.get_int(1) != 0,
                },
                None => MiniTraceAction::LoadMiniTraceQ {
                    filename: String::new(),
                    orgs_per: 1,
                    print_genomes: true,
                },
            }
        }
        "PrintReproData" => MiniTraceAction::PrintReproData,
        // The constructors ignore their arguments.
        "PrintTopNavTrace" | "PrintNavTrace" => MiniTraceAction::PrintTopNavTrace,
        "FlushTopNavTrace" => MiniTraceAction::FlushTopNavTrace,
        _ => return None,
    })
}

/// `HasOpinion() ? GetOpinion().first`
fn opinion_of(org: &OrgState) -> Option<i32> {
    org.opinions
        .as_ref()
        .and_then(|l| l.last())
        .map(|&(opinion, _)| opinion)
}

/// `"a,b,c"` (the `cStats` reaction lists).
fn comma_list(v: &[i32]) -> String {
    v.iter().map(i32::to_string).collect::<Vec<_>>().join(",")
}

/// `Systematics::Arbiter::Iterator` over the genotypes: `Next()` returns
/// the next genotype, then NULL for ever.
struct GenotypeCursor {
    groups: Vec<GroupRef>,
    pos: usize,
}

impl GenotypeCursor {
    fn next(&mut self) -> Option<GroupRef> {
        let g = self.groups.get(self.pos).copied();
        if g.is_some() {
            self.pos += 1;
        }
        g
    }
}

/// `ft_check_counts[k]--` and, at zero, the swap-and-pop of `k` from both
/// lists.
fn count_down(to_use: &mut Vec<i32>, counts: &mut Vec<i32>, k: usize) {
    counts[k] -= 1;
    if counts[k] == 0 {
        to_use.swap_remove(k);
        counts.swap_remove(k);
    }
}

impl World {
    /// `Process` of a [`MiniTraceAction`].
    pub(crate) fn run_mini_trace_action(&mut self, action: &MiniTraceAction) -> Result<(), String> {
        match action {
            MiniTraceAction::PrintMiniTraces {
                random,
                save_dominants,
                save_groups,
                save_foragers,
                orgs_per,
                max_samples,
                print_genomes,
                print_reacs,
            } => {
                let target = if *random {
                    self.set_random_trace_q(
                        *max_samples,
                        self.pop.live_org_list.len() as i32,
                        |_| true,
                    )?
                } else {
                    self.set_trace_q(
                        *save_dominants,
                        *save_groups,
                        *save_foragers,
                        *orgs_per,
                        *max_samples,
                    )
                };
                self.set_mini_trace_queue(target, *print_genomes, *print_reacs, false);
                Ok(())
            }
            MiniTraceAction::PrintMicroTraces {
                random,
                rand_prey,
                rand_pred,
                next_prey,
                next_pred,
                save_dominants,
                save_groups,
                save_foragers,
                orgs_per,
                max_samples,
                print_genomes,
                print_reacs,
            } => {
                let st = &mut self.mini_trace;
                // `SetNextPreyQ` / `SetNextPredQ(num, genomes, reacs, true)`
                if *next_prey != 0 {
                    st.next_prey_q = *next_prey;
                    st.print_genomes = *print_genomes;
                    st.print_reacs = *print_reacs;
                    st.use_micro = true;
                }
                if *next_pred != 0 {
                    st.next_pred_q = *next_pred;
                    st.print_genomes = *print_genomes;
                    st.print_reacs = *print_reacs;
                    st.use_micro = true;
                }
                let mut target = Vec::new();
                if *rand_prey || *rand_pred {
                    // `IsPreyFT()`: forage target above -2.
                    if *rand_prey {
                        let bound = self.pop.num_prey_organisms;
                        target =
                            self.set_random_trace_q(*max_samples, bound, |o| o.forage_target > -2)?;
                    }
                    if *rand_pred {
                        let bound = self.pop.num_pred_organisms;
                        let pred = self
                            .set_random_trace_q(*max_samples, bound, |o| o.forage_target <= -2)?;
                        target.extend(pred);
                    }
                } else if *random {
                    target = self.set_random_trace_q(
                        *max_samples,
                        self.pop.live_org_list.len() as i32,
                        |_| true,
                    )?;
                } else {
                    target = self.set_trace_q(
                        *save_dominants,
                        *save_groups,
                        *save_foragers,
                        *orgs_per,
                        *max_samples,
                    );
                }
                // QUIRK: `AppendMiniTraces` defaults `use_micro` to false, so
                // the genotypes queued here get mini traces, and the micro
                // traces of the `next_*` organisms born later turn into mini
                // traces as well.
                if !target.is_empty() {
                    self.append_mini_traces(target, *print_genomes, *print_reacs, false);
                }
                Ok(())
            }
            MiniTraceAction::LoadMiniTraceQ {
                filename,
                orgs_per,
                print_genomes,
            } => {
                self.load_mini_trace_q(filename, *orgs_per, *print_genomes, false);
                Ok(())
            }
            MiniTraceAction::PrintReproData => {
                // `AppendRecordReproQ(org)` for every living organism.
                let live: Vec<i32> = self.pop.live_org_list.clone();
                for cell in live {
                    let Some(org) = self.pop.orgs[cell as usize].as_mut() else {
                        continue;
                    };
                    self.mini_trace.repro_q.push(org.state.uid);
                    org.hardware.set_repro_trace(true);
                    self.mini_trace.any_traced = true;
                }
                Ok(())
            }
            MiniTraceAction::PrintTopNavTrace => {
                self.set_top_nav_q();
                Ok(())
            }
            MiniTraceAction::FlushTopNavTrace => {
                self.print_top_nav_trace(true, None);
                Ok(())
            }
        }
    }

    // ------------------------------------------------------------------
    // Queues
    // ------------------------------------------------------------------

    /// `SetMiniTraceQueue(new_queue, print_genomes, print_reacs, use_micro)`
    pub fn set_mini_trace_queue(
        &mut self,
        new_queue: Vec<i32>,
        print_genomes: bool,
        print_reacs: bool,
        use_micro: bool,
    ) {
        let st = &mut self.mini_trace;
        st.queue = new_queue;
        st.print_genomes = print_genomes;
        st.print_reacs = print_reacs;
        st.use_micro = use_micro;
    }

    /// `AppendMiniTraces(new_queue, print_genomes, print_reacs, use_micro)`
    pub fn append_mini_traces(
        &mut self,
        new_queue: Vec<i32>,
        print_genomes: bool,
        print_reacs: bool,
        use_micro: bool,
    ) {
        let st = &mut self.mini_trace;
        st.queue.extend(new_queue);
        st.print_genomes = print_genomes;
        st.print_reacs = print_reacs;
        st.use_micro = use_micro;
    }

    /// `LoadMiniTraceQ(filename, orgs_per, print_genomes, print_reacs)`:
    /// queue `orgs_per` organisms of each genotype id listed in the file
    /// (column `id`), less those already queued.
    pub fn load_mini_trace_q(
        &mut self,
        filename: &str,
        orgs_per: i32,
        print_genomes: bool,
        print_reacs: bool,
    ) {
        let input = InitFile::new(filename, &self.working_dir, None, None);
        if !input.was_opened() {
            // `m_world->GetDriver().Feedback()` prints with `printf`.
            for line in input.feedback().render().lines() {
                self.action_state.stdout.push_str(line);
                self.action_state.stdout.push('\n');
            }
        }
        let queue = self.mini_trace.queue.clone();
        let mut bg_id_list = Vec::new();
        for line_id in 0..input.num_lines() as i32 {
            let line = input.get_line_as_dict(line_id);
            let gen_id_num = line.get("id").map_or(0, |s| as_int(s));
            // skip if enough already in the existing trace queue
            let mut add_num = orgs_per;
            for &q in &queue {
                if q == gen_id_num {
                    add_num -= 1;
                    if add_num <= 0 {
                        break;
                    }
                }
            }
            for _ in 0..add_num {
                bg_id_list.push(gen_id_num);
            }
        }
        if !queue.is_empty() {
            self.append_mini_traces(bg_id_list, print_genomes, print_reacs, false);
        } else {
            self.set_mini_trace_queue(bg_id_list, print_genomes, print_reacs, false);
        }
    }

    /// `SetRandomTraceQ(max_samples)` (`keep` = every organism),
    /// `SetRandomPreyTraceQ` (`keep` = `IsPreyFT()`) and
    /// `SetRandomPredTraceQ` (`keep` = `!IsPreyFT()`): genotype ids of
    /// living organisms drawn at random.
    ///
    /// QUIRK: every draw uses a fresh RNG seeded with 0, so every draw
    /// picks the same organism. The C++ loops forever once that organism
    /// is used (asking for more than one sample) or when it fails `keep`;
    /// that is an error here. The bound on the number of samples is the
    /// number of living organisms for `SetRandomTraceQ` and the prey or
    /// predator count for the other two (`bound`).
    fn set_random_trace_q(
        &mut self,
        max_samples: i32,
        bound: i32,
        keep: impl Fn(&OrgState) -> bool,
    ) -> Result<Vec<i32>, String> {
        let live: Vec<i32> = self.pop.live_org_list.clone();
        let mut max_bgs = 1;
        if max_samples != 0 {
            max_bgs = max_samples;
        }
        if max_samples > bound {
            max_bgs = bound;
        }
        let mut used = vec![false; live.len()];
        let mut bg_id_list = Vec::new();
        while (bg_id_list.len() as i32) < max_bgs {
            if live.is_empty() {
                return Err(
                    "random trace queue of an empty population (C++: reads past the \
                            live organism list)"
                        .to_string(),
                );
            }
            let mut rng = AvidaRng::new(0);
            let k = rng.get_int_range(0, live.len() as i32) as usize;
            let cell = live[k] as usize;
            let accepted = !used[k] && self.pop.orgs[cell].as_ref().is_some_and(|o| keep(&o.state));
            if !accepted {
                return Err(
                    "random trace queue: the seed-0 RNG redraws the same organism \
                            for ever (C++: cPopulation::SetRandomTraceQ loops forever)"
                        .to_string(),
                );
            }
            let id = self
                .systematics
                .genotype_of(&self.pop.membership[cell])
                .and_then(|g| self.systematics.group_id(g))
                .unwrap_or(-1);
            bg_id_list.push(id);
            used[k] = true;
        }
        Ok(bg_id_list)
    }

    /// `SetTraceQ(save_dominants, save_groups, save_foragers, orgs_per,
    /// max_samples)`: genotype ids to trace, the most abundant first, then
    /// one per forager type, then one per group, up to `max_samples`.
    ///
    /// QUIRK: the loops test `bg == it->Next()` and then take
    /// `bg = it->Next()`, so each step skips a genotype; the "no more to
    /// check" branch is never taken (a NULL from `Next()` differs from the
    /// current genotype and ends the scan through the NULL checks instead).
    pub fn set_trace_q(
        &mut self,
        save_dominants: bool,
        save_groups: bool,
        save_foragers: bool,
        orgs_per: i32,
        max_samples: i32,
    ) -> Vec<i32> {
        let mut bg_id_list: Vec<i32> = Vec::new();
        let (Some(role), Some(arbiter)) = (
            self.systematics.arbiter_for_role("genotype"),
            self.systematics.genotypes(),
        ) else {
            return bg_id_list;
        };
        let mut it = GenotypeCursor {
            groups: arbiter
                .iter_groups()
                .into_iter()
                .map(|handle| GroupRef { role, handle })
                .collect(),
            pos: 0,
        };
        let mut bg = it.next();
        let mut fts_to_use: Vec<i32> = Vec::new();
        let mut groups_to_use: Vec<i32> = Vec::new();
        let mut num_doms = 0;
        let mut fts_left = 0;
        let mut groups_left = 0;

        if save_dominants {
            num_doms = orgs_per;
        }

        // get forager types in pop
        let mut ft_check_counts: Vec<i32> = Vec::new();
        if save_foragers {
            let pps = self.cfg.pred_prey_switch;
            if pps == -2 || pps > -1 {
                fts_to_use.push(-3);
                fts_to_use.push(-2);
            }
            fts_to_use.push(-1); // account for -1 default's
            for &t in self.env.target_ids() {
                if t != -1 && t != -2 && t != -3 {
                    fts_to_use.push(t);
                }
            }
            ft_check_counts = vec![orgs_per; fts_to_use.len()];
            fts_left = orgs_per * fts_to_use.len() as i32;
        }

        // get groups in pop
        let mut group_check_counts: Vec<i32> = Vec::new();
        if save_groups {
            groups_to_use = self.formed_group_array();
            group_check_counts = vec![orgs_per; groups_to_use.len()];
            groups_left = orgs_per * groups_to_use.len() as i32;
        }

        let prop = |w: &World, g: GroupRef, name: &str| w.systematics.property(g, name);
        let threshold = |w: &World, g: GroupRef| prop(w, g, "threshold").bool_value();

        let max_bgs = if max_samples != 0 {
            max_samples
        } else {
            num_doms + orgs_per * fts_to_use.len() as i32 + orgs_per * groups_to_use.len() as i32
        };
        let mut doms_done = !save_dominants;
        let mut fts_done = !save_foragers;
        let mut grps_done = !save_groups;
        for i in 0..3 {
            if !((bg_id_list.len() as i32) < max_bgs && (!doms_done || !fts_done || !grps_done)) {
                continue;
            }
            if i == 0 && save_dominants && num_doms > 0 {
                for _ in 0..num_doms {
                    match bg {
                        Some(g) if threshold(self, g) || bg_id_list.is_empty() => {
                            bg_id_list.push(self.systematics.group_id(g).unwrap_or(-1));
                            if save_foragers {
                                let ft = prop(self, g, "last_forager_type").int_value();
                                if fts_left > 0 {
                                    if let Some(k) = fts_to_use.iter().position(|&f| f == ft) {
                                        count_down(&mut fts_to_use, &mut ft_check_counts, k);
                                        fts_left -= 1;
                                    }
                                }
                            }
                            if save_groups {
                                let grp = prop(self, g, "last_group_id").int_value();
                                if groups_left > 0 {
                                    if let Some(k) = groups_to_use.iter().position(|&x| x == grp) {
                                        count_down(&mut groups_to_use, &mut group_check_counts, k);
                                        groups_left -= 1;
                                    }
                                }
                            }
                            if Some(g) == it.next() {
                                // no more to check
                                doms_done = true;
                                break;
                            }
                            bg = it.next();
                        }
                        Some(_) => {
                            // no more above threshold
                            doms_done = true;
                            break;
                        }
                        None => {}
                    }
                }
            } else if i == 1 && save_foragers && fts_left > 0 {
                let mut j = 0;
                while j < fts_left {
                    match bg {
                        Some(g) if threshold(self, g) || bg_id_list.is_empty() => {
                            let ft = prop(self, g, "last_forager_type").int_value();
                            let mut found_one = false;
                            if let Some(k) = fts_to_use.iter().position(|&f| f == ft) {
                                bg_id_list.push(self.systematics.group_id(g).unwrap_or(-1));
                                count_down(&mut fts_to_use, &mut ft_check_counts, k);
                                found_one = true;
                            }
                            if save_groups {
                                let grp = prop(self, g, "last_group_id").int_value();
                                if groups_left > 0 {
                                    if let Some(k) = groups_to_use.iter().position(|&x| x == grp) {
                                        count_down(&mut groups_to_use, &mut group_check_counts, k);
                                        groups_left -= 1;
                                    }
                                }
                            }
                            if Some(g) == it.next() {
                                // no more to check
                                fts_done = true;
                                break;
                            }
                            bg = it.next();
                            if !found_one {
                                j -= 1;
                            }
                        }
                        Some(_) => {
                            // no more above threshold
                            fts_done = true;
                            break;
                        }
                        None => {}
                    }
                    j += 1;
                }
            } else if i == 2 && save_groups && groups_left > 0 {
                let mut j = 0;
                while j < groups_left {
                    match bg {
                        Some(g) if threshold(self, g) || bg_id_list.is_empty() => {
                            let grp = prop(self, g, "last_group_id").int_value();
                            let mut found_one = false;
                            if let Some(k) = groups_to_use.iter().position(|&x| x == grp) {
                                bg_id_list.push(self.systematics.group_id(g).unwrap_or(-1));
                                count_down(&mut groups_to_use, &mut group_check_counts, k);
                                found_one = true;
                            }
                            if Some(g) == it.next() {
                                // no more to check
                                grps_done = true;
                                break;
                            }
                            bg = it.next();
                            if !found_one {
                                j -= 1;
                            }
                        }
                        Some(_) => {
                            // no more above threshold
                            grps_done = true;
                            break;
                        }
                        None => {}
                    }
                    j += 1;
                }
                if grps_done {
                    break; // no more of last type we have
                }
            }
        }
        bg_id_list
    }

    // ------------------------------------------------------------------
    // Switching traces on
    // ------------------------------------------------------------------

    /// The genotype of the organism in `cell`, or of the running organism
    /// there once `KillOrganism` has taken its groups.
    fn trace_genotype(&self, cell: i32) -> Option<GroupRef> {
        match self.pop.running.as_ref() {
            Some(r) if r.cell_id == cell && r.removed => r
                .dead_membership
                .as_ref()
                .and_then(|m| self.systematics.genotype_of(m)),
            _ => self
                .systematics
                .genotype_of(self.pop.membership.get(cell as usize)?),
        }
    }

    /// Run `f` on the organism in `cell` (not the running one) with its
    /// hardware traces lent out.
    fn with_cell_traces(
        &mut self,
        cell: i32,
        f: impl FnOnce(&mut World, &OrgState, Option<GroupRef>, &mut LentTraces),
    ) {
        let c = cell as usize;
        let Some(mut org) = self.pop.orgs.get_mut(c).and_then(Option::take) else {
            return;
        };
        let g = self.trace_genotype(cell);
        let split_success = org.hardware.print_mini_trace_success(1);
        let mut lent = LentTraces::lend(org.hardware.base_mut(), split_success);
        f(self, &org.state, g, &mut lent);
        lent.restore(org.hardware.base_mut());
        self.pop.orgs[c] = Some(org);
    }

    /// Run `f` on the running parent's lent traces (inside a divide).
    /// `false` when no traces are lent.
    fn with_lent_traces(&mut self, f: impl FnOnce(&mut World, &mut LentTraces)) -> bool {
        let Some(mut lent) = self.mini_trace.lent.take() else {
            return false;
        };
        f(self, &mut lent);
        self.mini_trace.lent = Some(lent);
        true
    }

    /// The trace hook at the end of `ActivateOrganism` (the organism in
    /// `cell` survived its birth): the next prey or predator wanted by
    /// `PrintMicroTraces`, else a queued genotype.
    pub(crate) fn mini_trace_on_activate(&mut self, cell: i32) {
        let st = &self.mini_trace;
        if st.next_prey_q <= 0 && st.next_pred_q <= 0 && st.queue.is_empty() {
            return;
        }
        let Some(parent_ft) = self.pop.orgs[cell as usize]
            .as_ref()
            .map(|o| o.state.parent_ft)
        else {
            return;
        };
        if self.mini_trace.next_prey_q > 0 && parent_ft > -2 {
            self.setup_mini_trace_in_cell(cell);
            self.mini_trace.next_prey_q -= 1;
        } else if self.mini_trace.next_pred_q > 0 && parent_ft <= -2 {
            self.setup_mini_trace_in_cell(cell);
            self.mini_trace.next_pred_q -= 1;
        } else if !self.mini_trace.queue.is_empty() {
            self.with_cell_traces(cell, |w, org, g, lent| w.test_for_mini_trace(org, g, lent));
        }
    }

    /// `SetupMiniTrace(cell_array[cell].GetOrganism())`.
    pub(crate) fn setup_mini_trace_in_cell(&mut self, cell: i32) {
        if self.pop.orgs.get(cell as usize).is_none_or(Option::is_none) {
            self.set_fatal("SetupMiniTrace on an empty cell (C++: dereferences NULL)");
            return;
        }
        self.with_cell_traces(cell, |w, org, g, lent| w.setup_mini_trace(org, g, lent));
    }

    /// The end of `cPopulation::Inject` with `trace` set.
    pub(crate) fn mini_trace_after_inject(&mut self, cell: i32) {
        if std::mem::take(&mut self.mini_trace.inject_trace) {
            self.setup_mini_trace_in_cell(cell);
        }
    }

    /// `TestForMiniTrace(org)`: trace `org` if its genotype is queued, and
    /// take one instance of the genotype off the queue.
    fn test_for_mini_trace(&mut self, org: &OrgState, g: Option<GroupRef>, lent: &mut LentTraces) {
        let org_bg_id = g.and_then(|g| self.systematics.group_id(g)).unwrap_or(-1);
        if let Some(i) = self.mini_trace.queue.iter().position(|&q| q == org_bg_id) {
            self.mini_trace.queue.swap_remove(i);
            self.setup_mini_trace(org, g, lent);
        }
    }

    /// `SetupMiniTrace(org)`: a mini trace file (or a micro trace when
    /// `use_micro_traces`), plus the genome file when
    /// `print_mini_trace_genomes`.
    fn setup_mini_trace(&mut self, org: &OrgState, g: Option<GroupRef>, lent: &mut LentTraces) {
        let Some(g) = g else {
            self.set_fatal("SetupMiniTrace on an organism without a genotype");
            return;
        };
        self.mini_trace.any_traced = true;
        let target = org.parent_ft;
        let id = org.id;
        let group_id = opinion_of(org).unwrap_or(org.parent_group);
        let gen_id = self.systematics.group_id(g).unwrap_or(-1);
        let stem = format!(
            "org{id}-ud{}-grp{group_id}_ft{target}-gt{gen_id}",
            self.stats.update()
        );

        if !self.mini_trace.use_micro {
            // `SetMiniTrace(filename)`: the printer's
            // `Output::File::CreateWithPath` fails on a file that is open.
            match self.out.create_with_path(&format!("minitraces/{stem}.trc")) {
                Ok(file) => {
                    let genotype = self.systematics.property(g, "genotype").string_value();
                    lent.set_mini_trace(MiniTracePrinter::new(file, gen_id, genotype).into_ptr());
                }
                Err(e) => self.set_fatal(format!(
                    "SetMiniTrace: {e} (C++: cHardwareStatusPrinter keeps a NULL file and \
                     crashes on its first line)"
                )),
            }
        } else {
            lent.set_micro_trace();
        }

        if self.mini_trace.print_genomes {
            let gen_file = format!("minitraces/trace_genomes/{stem}.trcgeno");
            if let Err(e) = self.print_mini_trace_genome(g, &gen_file) {
                self.set_fatal(e);
            }
        }
    }

    /// `PrintMiniTraceGenome(org, filename)`: the genotype's genome as the
    /// test CPU prints it, run with its own RNG (seed 0) so the world's
    /// draws are untouched.
    fn print_mini_trace_genome(&mut self, g: GroupRef, filename: &str) -> Result<(), String> {
        let genome = Genome::from_string(&self.systematics.property(g, "genome").string_value());
        self.print_genome_file(&genome, filename)
    }

    /// `testcpu->PrintGenome(ctx2, genome, filename, update)` on a test CPU
    /// with its own RNG (seed 0).
    fn print_genome_file(&mut self, genome: &Genome, filename: &str) -> Result<(), String> {
        let update = self.stats.update();
        let mut df = self.out.create_with_path(filename)?;
        let mut rng = AvidaRng::new(0);
        let result = {
            let mut tw = make_test_world(
                &self.cfg,
                &mut self.env,
                &mut rng,
                &self.hw,
                &mut self.stats,
                &mut self.host,
                &*self.group_attack_fn,
                self.test_sterilize,
                &self.pop,
            );
            TestCpu::new(&mut tw).and_then(|mut cpu| {
                cpu.print_genome(&mut tw, genome, &mut df, filename, update, false, 0, -1, -1)
            })
        };
        self.flush_host();
        result
    }

    // ------------------------------------------------------------------
    // UpdateQs
    // ------------------------------------------------------------------

    /// `UpdateQs(parent, true)` at the start of `ActivateOffspring` (the
    /// running parent's traces are lent).
    pub(crate) fn mini_trace_parent_divides(&mut self, parent: &OrgState, parent_cell: i32) {
        if !self.mini_trace.any_traced {
            return;
        }
        let g = self.trace_genotype(parent_cell);
        let lent = self.with_lent_traces(|w, lent| {
            if lent.any() {
                w.update_qs(parent, g, lent, true);
            }
        });
        if !lent {
            self.set_fatal(
                "ActivateOffspring without the parent's traces (C++: cPopulation::UpdateQs)",
            );
        }
    }

    /// `TestForMiniTrace(parent_organism)` at the end of
    /// `ActivateOffspring` (a splitting divide that resets the inputs).
    pub(crate) fn mini_trace_parent_after_divide(&mut self, parent: &OrgState, parent_cell: i32) {
        if self.mini_trace.queue.is_empty() {
            return;
        }
        let g = self.trace_genotype(parent_cell);
        let lent = self.with_lent_traces(|w, lent| w.test_for_mini_trace(parent, g, lent));
        if !lent {
            self.set_fatal(
                "ActivateOffspring without the parent's traces (C++: cPopulation::TestForMiniTrace)",
            );
        }
    }

    /// `UpdateQs(organism, false)` in `KillOrganism(cell)`. `running` is
    /// the running organism's state when the caller holds it and `cell` is
    /// its cell.
    pub(crate) fn mini_trace_on_kill(
        &mut self,
        cell: i32,
        is_running: bool,
        running: Option<&OrgState>,
    ) {
        if !self.mini_trace.any_traced {
            return;
        }
        if !is_running {
            let traced = self.pop.orgs[cell as usize].as_ref().is_some_and(|o| {
                let b = o.hardware.base();
                b.minitrace || b.microtrace || b.reprotrace || b.topnavtrace
            });
            if traced {
                self.with_cell_traces(cell, |w, org, g, lent| w.update_qs(org, g, lent, false));
            }
            return;
        }
        let g = self.trace_genotype(cell);
        let done = match running {
            Some(org) if self.mini_trace.lent.is_some() => self.with_lent_traces(|w, lent| {
                if lent.any() {
                    w.update_qs(org, g, lent, false);
                }
            }),
            _ => false,
        };
        if !done {
            self.mini_trace.deferred_kill = true;
        }
    }

    /// The end of a step whose organism was killed outside a divide: its
    /// `UpdateQs(org, false)`, then the `microtrace.dat` lines held back
    /// meanwhile. `membership` is the organism's groups.
    #[inline]
    pub(crate) fn mini_trace_step_end(
        &mut self,
        org: Option<&mut Organism>,
        membership: Option<&GroupMembership>,
    ) {
        // Inlined test: every time slice ends here, a held kill is rare.
        if std::mem::take(&mut self.mini_trace.deferred_kill) {
            self.mini_trace_held_kill(org, membership);
        }
    }

    /// [`World::mini_trace_step_end`] with a kill held back.
    #[cold]
    #[inline(never)]
    fn mini_trace_held_kill(
        &mut self,
        org: Option<&mut Organism>,
        membership: Option<&GroupMembership>,
    ) {
        if let Some(org) = org {
            let g = membership.and_then(|m| self.systematics.genotype_of(m));
            let mut lent = LentTraces::lend(org.hardware.base_mut(), String::new());
            if lent.any() {
                self.update_qs(&org.state, g, &mut lent, false);
            }
            lent.restore(org.hardware.base_mut());
        }
        for line in std::mem::take(&mut self.mini_trace.held_micro) {
            self.write_micro_trace_line(&line);
        }
    }

    /// `UpdateQs(org, reproduced)`: take the organism out of the trace
    /// queues when it dies, or when it reproduces by splitting
    /// (`DIVIDE_METHOD` 1 with `RESET_INPUTS_ON_DIVIDE`).
    fn update_qs(
        &mut self,
        org: &OrgState,
        g: Option<GroupRef>,
        lent: &mut LentTraces,
        reproduced: bool,
    ) {
        let split =
            self.cfg.divide_method == DIVIDE_METHOD_SPLIT && self.cfg.reset_inputs_on_divide != 0;
        let gen_id = g.and_then(|g| self.systematics.group_id(g)).unwrap_or(-1);
        if !reproduced || split {
            // `PrintMicroTrace(gen_id)`
            if lent.microtrace {
                let birth_update = org.phenotype.birth_update;
                self.print_micro_traces(
                    &lent.microtracer,
                    birth_update,
                    org.id,
                    org.forage_target,
                    gen_id,
                );
                lent.microtrace = false;
            }
            // `DeleteMiniTrace(print_mini_trace_reacs, reproduced && split)`
            if lent.minitrace && self.mini_trace.print_reacs {
                self.print_mini_trace_reactions(org, gen_id);
            }
            lent.delete_mini_trace(reproduced && split);
        }
        if lent.reprotrace && !self.mini_trace.repro_q.is_empty() {
            if let Some(i) = self.mini_trace.repro_q.iter().position(|&u| u == org.uid) {
                if reproduced {
                    self.print_repro_data(org, gen_id);
                }
                if !reproduced || split {
                    self.mini_trace.repro_q.swap_remove(i);
                    lent.reprotrace = false;
                }
            }
        }
        self.update_top_nav_q(org, g, lent, reproduced, split);
    }

    // ------------------------------------------------------------------
    // cStats printers
    // ------------------------------------------------------------------

    /// `cStats::PrintMicroTraces(exec_trace, birth_update, org_id, ft,
    /// gen_id)`: one line of `microtrace.dat`.
    fn print_micro_traces(
        &mut self,
        exec_trace: &[u8],
        birth_update: i32,
        org_id: i32,
        ft: i32,
        gen_id: i32,
    ) {
        let death_update = self.stats.update();
        let mut line = format!("{death_update},{birth_update},{org_id},{gen_id},{ft},");
        line.extend(exec_trace.iter().map(|&b| b as char));
        line.push('\n');
        if self.mini_trace.deferred_kill {
            self.mini_trace.held_micro.push(line);
        } else {
            self.write_micro_trace_line(&line);
        }
    }

    fn write_micro_trace_line(&mut self, line: &str) {
        let df = match self.out.static_with_path("microtrace.dat") {
            Ok(df) => df,
            Err(e) => {
                self.set_fatal(e);
                return;
            }
        };
        if !df.header_done() {
            df.write_comment("Trace Execution Data");
            df.write_time_stamp();
            df.write_comment("DeathUpdate");
            df.write_comment("BirthUpdate");
            df.write_comment("OrgID");
            df.write_comment("GenotypeID");
            df.write_comment("ForageTarget");
            df.endl();
        }
        let fp = df.ofstream();
        let _ = fp.write_all(line.as_bytes());
        let _ = fp.flush();
    }

    /// `cStats::PrintMiniTraceReactions(org)`: the organism's reaction
    /// counts, and the cycle and execution count of each reaction's first
    /// trigger, in a file of its own.
    fn print_mini_trace_reactions(&mut self, org: &OrgState, gen_id: i32) {
        let p = &org.phenotype;
        let (counts, cycles, execs) = (
            &p.cur_reaction_count,
            &p.first_reaction_cycles,
            &p.first_reaction_execs,
        );
        if counts.is_empty() || cycles.is_empty() || execs.is_empty() {
            self.set_fatal(
                "PrintMiniTraceReactions without reactions (C++: reads before an empty array)",
            );
            return;
        }
        let group_id = opinion_of(org).unwrap_or(self.cfg.default_group);
        let filename = format!(
            "minitraces/trace_reactions/org{}-ud{}-grp{}_ft{}-gt{}.trcreac",
            org.id, p.birth_update, group_id, org.forage_target, gen_id
        );
        let mut df = match self.out.create_with_path(&filename) {
            Ok(df) => df,
            Err(e) => {
                self.set_fatal(format!(
                    "PrintMiniTraceReactions: {e} (C++: dereferences a NULL file)"
                ));
                return;
            }
        };
        df.write_time_stamp();
        df.write_comment("Reaction Data for Traced Org to Date (death or end)");
        df.write_comment("OrgID");
        df.write_comment("Update Born");
        df.write_comment("Reaction Counts");
        df.write_comment("CPU Cycle at First Trigger of Each Reaction");
        df.write_comment(
            "Exec Count at First Trigger (== index into execution trace and nav traces)",
        );
        df.flush_comments();
        df.endl();
        let line = format!(
            "{} {} {} {} {}\n",
            org.id,
            p.birth_update,
            comma_list(counts),
            comma_list(cycles),
            comma_list(execs)
        );
        let fp = df.ofstream();
        let _ = fp.write_all(line.as_bytes());
        df.flush();
    }

    /// `cStats::PrintReproData(org)`: one line of `repro_data.dat` at the
    /// organism's first reproduction.
    fn print_repro_data(&mut self, org: &OrgState, gen_id: i32) {
        let p = &org.phenotype;
        if p.cur_reaction_count.is_empty() {
            self.set_fatal("PrintReproData without reactions (C++: reads before an empty array)");
            return;
        }
        let line = format!(
            "{} {} {} {} {} {} {}\n",
            self.stats.update(),
            gen_id,
            org.id,
            p.age,
            p.time_used,
            p.num_execs,
            comma_list(&p.cur_reaction_count)
        );
        let df = match self.out.static_with_path("repro_data.dat") {
            Ok(df) => df,
            Err(e) => {
                self.set_fatal(e);
                return;
            }
        };
        if !df.header_done() {
            df.write_comment("Org Data up to First Reproduction");
            df.write_time_stamp();
            df.write_comment("ReproUpdate");
            df.write_comment("GenotypeID");
            df.write_comment("OrgID");
            df.write_comment("Age (updates)");
            df.write_comment("TimeUsed (cycles)");
            df.write_comment("NumExecutions (attempted executions)");
            df.write_comment("ReactionCounts");
            df.endl();
        }
        let fp = df.ofstream();
        let _ = fp.write_all(line.as_bytes());
        let _ = fp.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_test_suite_arguments() {
        let mut fb = Feedback::new();
        assert_eq!(
            parse(
                "PrintMiniTraces",
                "save_dominants=1:save_foragers=1:print_genomes=1:orgs_per=1",
                &mut fb
            ),
            Some(MiniTraceAction::PrintMiniTraces {
                random: false,
                save_dominants: true,
                save_groups: false,
                save_foragers: true,
                orgs_per: 1,
                max_samples: 0,
                print_genomes: true,
                print_reacs: false,
            })
        );
        match parse(
            "PrintMicroTraces",
            "next_prey=1:next_pred=1:print_genomes=1",
            &mut fb,
        ) {
            Some(MiniTraceAction::PrintMicroTraces {
                next_prey,
                next_pred,
                print_genomes,
                random,
                ..
            }) => {
                assert_eq!(
                    (next_prey, next_pred, print_genomes, random),
                    (1, 1, true, false)
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(parse("Exit", "", &mut fb), None);
        // `PrintNavTrace` is registered as `cActionPrintTopNavTrace`; the
        // three ignore their arguments.
        for name in ["PrintTopNavTrace", "PrintNavTrace"] {
            assert_eq!(
                parse(name, "x=1", &mut fb),
                Some(MiniTraceAction::PrintTopNavTrace)
            );
        }
        assert_eq!(
            parse("FlushTopNavTrace", "", &mut fb),
            Some(MiniTraceAction::FlushTopNavTrace)
        );
    }

    #[test]
    fn genotype_cursor_ends_in_null() {
        let mut it = GenotypeCursor {
            groups: Vec::new(),
            pos: 0,
        };
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn count_down_swaps_out_exhausted_entries() {
        let mut to_use = vec![-3, -2, -1, 0];
        let mut counts = vec![1, 1, 2, 1];
        count_down(&mut to_use, &mut counts, 0);
        assert_eq!(
            (to_use.clone(), counts.clone()),
            (vec![0, -2, -1], vec![1, 1, 2])
        );
        count_down(&mut to_use, &mut counts, 2);
        assert_eq!((to_use, counts), (vec![0, -2, -1], vec![1, 1, 1]));
    }
}
