//! Organism handling of `cPopulation` (avida-core/source/main/cPopulation.cc
//! at 47f13dadb): inject, activation, birth, death, offspring placement,
//! the process step and the per-update statistics.
//!
//! These are methods on [`World`] rather than on `Population` because each
//! of them reaches the configuration, environment, RNG, stats and
//! systematics as well as the cells.
//!
//! # The running organism
//! `ProcessStep` takes the organism out of `pop.orgs` for the duration of
//! its instruction, so the hardware can hold `&mut` to it while the rest of
//! the world is borrowed through the interface. `pop.running` records its
//! cell. Functions that may act on the running organism (`KillOrganism` on
//! its cell, `ActivateOffspring`) take its state as an explicit argument.
//!
//! Deme bookkeeping is called from here and lives in [`crate::pop_demes`].
//!
//! The `ActivateOffspring` branches that read the parent's hardware
//! (`PRECALC_PHENOTYPE` with `RESET_INPUTS_ON_DIVIDE`, which tests the
//! parent's memory, in [`crate::precalc`], and `EPIGENETIC_METHOD`, which
//! copies its registers and stack into each offspring) get it from the
//! divide, which lends it (`OrgInterface::divide_with_traces`). Sexual birth
//! goes through the birth chamber
//! ([`crate::birth_chamber`]); the birth methods that place by time used or
//! dispersal are in [`crate::birth_energy`], `SET_FT_AT_BIRTH` in
//! [`crate::birth_forage`], vertical parasite transmission in
//! [`crate::parasites`]; the trace queues (`UpdateQs`, `SetupMiniTrace`)
//! live in [`crate::mini_trace`].

use avida_genome::{Genome, InstructionSequence};
use avida_hardware::hardware::Hardware;
use avida_hardware::nhardware::{EPIGENETIC_METHOD_BOTH, EPIGENETIC_METHOD_OFFSPRING};
use avida_hardware::organism::{transmission, OrgSource, OrgState, Organism};
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu, TestWorld};
use avida_phenotype::phenotype::DIVIDE_METHOD_SPLIT;
use avida_phenotype::Merit;
use avida_systematics::unit::{GestationInfo, GroupMembership, GroupRef, UnitInfo};
use avida_systematics::{GenomeTestMetrics, Source, TransmissionType};

use rayon::prelude::*;

use crate::groups::pending_key;
use crate::pop_interface::PopulationInterface;
use crate::population::{position_offspring as po, Running};
use crate::stats::Stats;
use crate::world::{make_test_world, World};

/// `ActivateOffspring` reads the parent's hardware on a divide that did not
/// lend it (only the plain `OrgInterface::divide`, which no hardware uses).
const PARENT_HW_STOP: &str = "unsupported: ActivateOffspring reads the parent's hardware \
     (EPIGENETIC_METHOD 1 or 3, or PRECALC_PHENOTYPE with RESET_INPUTS_ON_DIVIDE) on a divide \
     made without it";

/// `GenomeTestMetrics::GenomeTestMetrics(world, ctx, group)`: a default
/// `cCPUTestInfo` run of the genome.
pub(crate) fn compute_test_metrics(
    tw: &mut TestWorld<'_>,
    genome: &Genome,
) -> Result<GenomeTestMetrics, String> {
    let mut cpu = TestCpu::new(tw)?;
    let mut info = CpuTestInfo::default();
    cpu.test_genome(tw, &mut info, genome)?;
    let mut m = GenomeTestMetrics {
        is_viable: info.is_viable(),
        fitness: info.genotype_fitness(),
        colony_fitness: info.colony_fitness(),
        merit: 0.0,
        copied_size: 0,
        executed_size: 0,
        gestation_time: 0,
        task_counts: Vec::new(),
    };
    if let Some(ph) = info.test_phenotype(0) {
        m.merit = ph.merit.get_double();
        m.copied_size = ph.copied_size;
        m.executed_size = ph.executed_size;
        m.gestation_time = ph.gestation_time;
        m.task_counts = ph.last_task_count.clone();
    }
    Ok(m)
}

pub(crate) fn systematics_source(src: &OrgSource) -> Source {
    let t = match src.transmission_type {
        transmission::DIVISION => TransmissionType::Division,
        transmission::DUPLICATION => TransmissionType::Duplication,
        transmission::VERTICAL => TransmissionType::Vertical,
        transmission::HORIZONTAL => TransmissionType::Horizontal,
        _ => TransmissionType::Unknown,
    };
    Source::new(t, &src.arguments, src.external)
}

impl World {
    /// `cPopulationCell::IsOccupied()`
    pub fn is_occupied(&self, cell: i32) -> bool {
        let Ok(c) = usize::try_from(cell) else {
            return false;
        };
        if self.pop.orgs.get(c).is_some_and(|o| o.is_some()) {
            return true;
        }
        self.pop
            .running
            .as_ref()
            .is_some_and(|r| r.cell_id == cell && !r.removed)
    }

    /// `AdjustSchedule(cell, merit)`: the merit is scaled by the deme's
    /// merit when the cell's deme has one. A merit of NaN (the energy merit
    /// `donate-facing` and its kin compute with
    /// `NUM_CYCLES_EXC_BEFORE_0_ENERGY 0`) stops the run under the
    /// probabilistic scheduler, whose C++ tree descent never ends once the
    /// total weight is NaN.
    pub fn adjust_schedule(&mut self, cell: i32, merit: f64) {
        let merit = self.deme_scaled_merit(cell, merit);
        if merit.is_nan()
            && self.cfg.slicing_method == avida_apto::scheduler::slicing_method::SLICE_PROB_MERIT
        {
            self.set_fatal(
                "unsupported: a merit of NaN under SLICING_METHOD 1 (C++: \
                 Apto::Scheduler::Probabilistic::Next reads past the end of its weight tree)",
            );
        }
        self.pop.scheduler.adjust_priority(cell, merit);
        if let Some(p) = usize::try_from(cell)
            .ok()
            .and_then(|c| self.pop.sched_priority.get_mut(c))
        {
            *p = merit;
        }
    }

    /// `GenomeTestMetrics::GetMetrics(world, ctx, group)`.
    pub fn genome_test_metrics(&mut self, g: GroupRef) -> Option<GenomeTestMetrics> {
        let World {
            cfg,
            env,
            rng,
            hw,
            stats,
            host,
            systematics,
            group_attack_fn,
            test_sterilize,
            fatal,
            pop,
            ..
        } = self;
        let mut err = None;
        let metrics = systematics
            .genome_test_metrics(g, |genome| {
                let mut tw = make_test_world(
                    cfg,
                    env,
                    rng,
                    hw,
                    stats,
                    host,
                    &**group_attack_fn,
                    *test_sterilize,
                    pop,
                );
                match compute_test_metrics(&mut tw, genome) {
                    Ok(m) => m,
                    Err(e) => {
                        err = Some(e);
                        GenomeTestMetrics {
                            is_viable: false,
                            fitness: 0.0,
                            colony_fitness: 0.0,
                            merit: 0.0,
                            copied_size: 0,
                            executed_size: 0,
                            gestation_time: 0,
                            task_counts: Vec::new(),
                        }
                    }
                }
            })
            .cloned();
        if let Some(e) = err {
            let mut f = fatal.borrow_mut();
            if f.is_none() {
                *f = Some(e);
            }
        }
        self.flush_host();
        metrics
    }

    // ------------------------------------------------------------------
    // Live organism list
    // ------------------------------------------------------------------

    fn add_live_org(&mut self, cell: usize) {
        self.pop.live_org_list.push(cell as i32);
        let idx = self.pop.live_org_list.len() as i32 - 1;
        self.pop.live_index[cell] = idx;
        if let Some(org) = self.pop.orgs[cell].as_mut() {
            org.state.org_list_index = idx;
        }
    }

    fn remove_live_org(&mut self, cell: usize) {
        let idx = self.pop.live_index[cell];
        let Ok(i) = usize::try_from(idx) else {
            return;
        };
        let last = self.pop.live_org_list.len() - 1;
        let exist_cell = self.pop.live_org_list[last] as usize;
        self.pop.live_index[exist_cell] = idx;
        if let Some(org) = self.pop.orgs[exist_cell].as_mut() {
            org.state.org_list_index = idx;
        }
        self.pop.live_org_list.swap(i, last);
        self.pop.live_org_list.pop();
        self.pop.live_index[cell] = -1;
    }

    /// `avd_cpop_is_deadly_boundary(DEADLY_BOUNDARIES, WORLD_GEOMETRY, x, y,
    /// WORLD_X, WORLD_Y)`: the cell is an edge of a bounded grid whose
    /// edges kill.
    pub(crate) fn is_deadly_boundary(&self, cell: i32) -> bool {
        let (wx, wy) = (self.cfg.world_x, self.cfg.world_y);
        let (x, y) = (cell % wx, cell / wx);
        self.cfg.deadly_boundaries == 1
            && self.cfg.world_geometry == 1
            && (x == 0 || y == 0 || x == wx - 1 || y == wy - 1)
    }

    // ------------------------------------------------------------------
    // Death
    // ------------------------------------------------------------------

    /// `KillOrganism(cell, ctx)`. `running` is the state of the organism
    /// being run when the caller holds it (its cell may be `cell`).
    pub fn kill_organism(&mut self, cell: i32, running: Option<&mut OrgState>) {
        let Ok(c) = usize::try_from(cell) else {
            return;
        };
        if c >= self.pop.orgs.len() {
            return;
        }
        let is_running_cell = self
            .pop
            .running
            .as_ref()
            .is_some_and(|r| r.cell_id == cell && !r.removed);
        // do we actually have something to kill?
        if self.pop.orgs[c].is_none() && !is_running_cell {
            return;
        }
        if self.cfg.log_sleep_times && cell != 0 {
            self.set_fatal(crate::population::SLEEP_LOG_STOP);
        }

        // Statistics...
        self.stats.record_death();
        if self.cfg.use_avatars != 0 {
            let uid = if is_running_cell {
                self.pop.running.as_ref().map(|r| r.uid)
            } else {
                self.pop.orgs[c].as_ref().map(|o| o.state.uid)
            };
            if let Some(uid) = uid {
                self.kill_avatars(uid);
            }
        }
        // `const int ft = organism->GetForageTarget();`
        let ft = if is_running_cell {
            running.as_deref().map(|s| s.forage_target)
        } else {
            self.pop.orgs[c].as_ref().map(|o| o.state.forage_target)
        };
        self.count_pred_prey_death(ft);
        self.remove_live_org(c);
        // `UpdateQs(organism, false)`
        self.mini_trace_on_kill(cell, is_running_cell, running.as_deref());

        if is_running_cell {
            let mut notified = false;
            if let Some(state) = running {
                let mut iface = PopulationInterface {
                    w: self,
                    cell_id: cell,
                };
                state.notify_death(&mut iface);
                state.phenotype.to_delete = true;
                notified = true;
                self.deme_organism_death(cell, Some(&*state));
                let uid = state.uid;
                self.hgt_organism_killed(cell, uid, None, Some(&mut *state));
                // `in_cell.RemoveOrganism(ctx)`
                self.cell_remove_energy(cell, state.phenotype.energy_store);
            } else {
                self.deme_organism_death(cell, None);
                let uid = self.pop.running.as_ref().map(|r| r.uid).unwrap_or_default();
                self.hgt_organism_killed(cell, uid, None, None);
                if self.cfg.energy_enabled && self.cfg.frac_energy_transfer > 0.0 {
                    self.set_fatal(
                        "unsupported: FRAC_ENERGY_TRANSFER when the running organism is killed \
                         from where its state is out of reach",
                    );
                }
            }
            self.pop.num_organisms -= 1;
            let m = std::mem::take(&mut self.pop.membership[c]);
            if let Some(r) = self.pop.running.as_mut() {
                r.removed = true;
                r.dead_membership = Some(m);
                r.notify_pending = !notified;
            }
        } else if let Some(mut org) = self.pop.orgs[c].take() {
            {
                let mut iface = PopulationInterface {
                    w: self,
                    cell_id: cell,
                };
                org.notify_death(&mut iface);
            }
            self.deme_organism_death(cell, Some(&org.state));
            self.pop.num_organisms -= 1;
            self.hgt_organism_killed(
                cell,
                org.state.uid,
                Some(org.state.genome().representation()),
                running,
            );
            // `in_cell.RemoveOrganism(ctx)`
            self.cell_remove_energy(cell, org.state.phenotype.energy_store);
            // `delete organism`: its parasites, then the unit itself, leave
            // their groups (unless deme seeding marked it running).
            let m = std::mem::take(&mut self.pop.membership[c]);
            if let Some((org, m)) = self.pop.seed_limbo.keep(org, m) {
                self.delete_organism(org, Some(m));
            }
        }

        // Alert the scheduler that this cell has a 0 merit.
        self.adjust_schedule(cell, 0.0);
    }

    // ------------------------------------------------------------------
    // Activation
    // ------------------------------------------------------------------

    /// `ActivateOrganism(ctx, in_organism, target_cell, assign_group,
    /// is_inject)`. `membership` holds the organism's systematics groups;
    /// `running` as for [`World::kill_organism`].
    pub fn activate_organism(
        &mut self,
        mut org: Organism,
        membership: GroupMembership,
        target: i32,
        assign_group: bool,
        is_inject: bool,
        running: Option<&mut OrgState>,
    ) -> bool {
        let t = target as usize;
        {
            let mut iface = PopulationInterface {
                w: self,
                cell_id: target,
            };
            org.set_org_interface(&mut iface);
        }

        // Update the contents of the target cell.
        self.kill_organism(target, running);
        // An offspring that joined a group before it was placed takes its
        // cell as member key now that the cell's previous occupant is gone.
        if let Some(k) = self.pop.groups.activating.take() {
            self.pop.groups.rekey(k, target);
        }

        // `cPopulationCell::InsertOrganism`
        self.stats.m_spec_waste += self.pop.spec_state[t];
        self.pop.spec_state[t] = 0;
        self.host.reset_interface(t);
        self.host.prev_seen[t] = target;
        self.pop.orgs[t] = Some(Box::new(org));
        self.pop.membership[t] = membership;
        self.cell_insert_energy(target, None);
        self.add_live_org(t);

        // Setup the inputs in the target cell.
        self.env
            .setup_inputs(&mut self.rng, &mut self.pop.cells[t].inputs, true);

        // Precalculate the phenotype if requested
        if let Err(e) = self.precalc_phenotype(t) {
            self.set_fatal(e);
        }

        // Initialize the time-slice for this new organism.
        let merit = self.pop.orgs[t]
            .as_ref()
            .map_or(0.0, |o| o.state.phenotype.merit.get_double());
        self.adjust_schedule(target, merit);

        // Special handling for certain birth methods.
        if self.cfg.birth_method == po::FULL_SOUP_ELDEST {
            self.pop.reaper_queue.push_front(target);
        }

        // If neural networking, add input and output avatars.. @JJB**
        if self.cfg.use_avatars != 0 && self.cfg.neural_networking {
            if let Some(uid) = self.pop.orgs[t].as_ref().map(|o| o.state.uid) {
                // Add input avatar
                self.add_io_av(uid, target, 2, true, false);
                // Add output avatar
                self.add_io_av(uid, target, 2, false, true);
            }
        }

        // Keep track of statistics for organism counts...
        self.pop.num_organisms += 1;
        let ft = self.pop.orgs[t]
            .as_ref()
            .map_or(-1, |o| o.state.forage_target);
        self.count_pred_prey_birth(ft);
        self.deme_inc_org_count(target);

        let rewarded_instruction = self.cfg.merit_bonus_inst;
        let default_group = self.cfg.default_group;
        let inherit_opinion = self.cfg.inherit_opinion != 0;
        let genotype = self.systematics.genotype_of(&self.pop.membership[t]);
        if let Some(org) = self.pop.orgs[t].as_mut() {
            // Statistics...
            self.stats.record_birth(org.state.phenotype.parent_true);

            // count how many times MERIT_BONUS_INST is in the genome
            let mut num_rewarded_instructions = 0;
            if rewarded_instruction != -1 {
                let seq = org.state.genome().representation();
                for i in 0..seq.size() {
                    if seq.get(i).op() == rewarded_instruction {
                        num_rewarded_instructions += 1;
                    }
                }
            }
            org.state.phenotype.bonus_instruction_count = num_rewarded_instructions;
        }

        if assign_group {
            let op = if self.cfg.use_form_groups != 0 {
                self.assign_birth_group(target)
            } else {
                default_group
            };
            if let Some(org) = self.pop.orgs[t].as_mut() {
                org.state.phenotype.birth_cell_id = target;
                org.state.phenotype.birth_group_id = op;
                org.state.phenotype.birth_forager_type = org.state.forage_target;
                if let Some(g) = genotype {
                    if let Some(gt) = self
                        .systematics
                        .genotypes_mut()
                        .and_then(|a| a.genotype_mut(g.handle))
                    {
                        gt.set_last_birth_cell(target);
                        if inherit_opinion {
                            gt.set_last_group_id(op);
                        } else {
                            gt.set_last_group_id(org.state.parent_group);
                        }
                        gt.set_last_forager_type(org.state.parent_ft);
                    }
                }
            }
        }

        let mut org_survived = true;
        // For tolerance_window, we cheated by dumping doomed offspring into
        // cell (X * Y) - 1 ...now that we updated the stats, we need to kill
        // that org. QUIRK: any organism placed in that cell after update 0
        // dies, doomed or not.
        let doomed_cell = self.pop.world_x * self.pop.world_y - 1;
        if self.cfg.tolerance_window > 0 && target == doomed_cell && self.stats.update() > 0 {
            self.kill_organism(target, None);
            org_survived = false;
        }
        // Kill org born on deadly world boundaries (a second kill of an
        // already emptied doomed cell is a no-op, as in C++).
        if target >= 0 && self.is_deadly_boundary(target) {
            self.kill_organism(target, None);
            org_survived = false;
        }
        // don't kill our test org, just it's offspring
        if (self.cfg.birth_method == 12 || self.cfg.birth_method == 13) && !is_inject {
            self.kill_organism(target, None);
            org_survived = false;
        }
        // are there traces we need to test for?
        if org_survived {
            self.mini_trace_on_activate(target);
        }
        org_survived
    }

    /// `ActivateOffspring(ctx, offspring_genome, parent_organism)`: place
    /// the offspring of the running organism `parent`, which lives in
    /// `parent_cell`. `parent_hw` is its hardware (`GetHardware()`), which
    /// `EPIGENETIC_METHOD` and `PRECALC_PHENOTYPE` with
    /// `RESET_INPUTS_ON_DIVIDE` read. Returns whether the parent lives
    /// through it.
    pub fn activate_offspring(
        &mut self,
        offspring_genome: &Genome,
        parent: &mut OrgState,
        parent_hw: Option<&dyn Hardware>,
        parent_cell: i32,
    ) -> bool {
        self.flush_parasite_deaths();
        let pc = parent_cell as usize;
        let update = self.stats.update();
        let doomed_cell = self.pop.world_x * self.pop.world_y - 1;
        let mut is_doomed = false;

        // If divide method is split, parent will be reset to completely
        // tolerant: must remove their intolerance from the group's cached
        // total.
        if self.cfg.divide_method == DIVIDE_METHOD_SPLIT && self.cfg.tolerance_window > 0 {
            self.remove_split_parent_intolerance(parent);
        }

        // Update the parent's phenotype. This needs to be done before the
        // parent goes into the birth chamber or the merit doesn't get passed
        // onto the offspring correctly.
        self.mini_trace_parent_divides(parent, parent_cell);
        let parent_size = parent.genome().representation().size();
        parent.phenotype.divide_reset(
            &self.cfg,
            parent_size,
            update,
            &mut self.rng,
            &mut self.stats,
        );

        // `HOST_USE_GENOTYPE_FILE`: a random entry of the list
        // `LoadHostGenotypeList` filled, in place of the offspring's genome.
        let host_seq = if self.cfg.host_use_genotype_file != 0 {
            let n = self.actions2.env.host_genotype_list.len();
            if n == 0 {
                self.set_fatal(
                    "unsupported: HOST_USE_GENOTYPE_FILE with an empty host genotype list \
                     (C++: indexes past cPopulation::host_genotype_list)",
                );
                return true;
            }
            let k = self.rng.get_int(n as i32) as usize;
            InstructionSequence::from_string(&self.actions2.env.host_genotype_list[k])
        } else {
            offspring_genome.representation().clone()
        };
        let temp = Genome::new(
            parent.genome().hardware_type(),
            &parent.genome().properties(),
            host_seq,
        );
        let parent_membership = match self.pop.running.as_ref() {
            Some(r) if r.cell_id == parent_cell && r.removed => {
                r.dead_membership.clone().unwrap_or_default()
            }
            _ => self.pop.membership[pc].clone(),
        };

        // `birth_chamber.SubmitOffspring(...)`: zero, one or two offspring.
        let mut offspring =
            match self.submit_offspring(&temp, parent, parent_cell, &parent_membership) {
                Ok(o) => o,
                Err(e) => {
                    self.set_fatal(e);
                    return true;
                }
            };

        let mut target_cells: Vec<i32> = Vec::with_capacity(offspring.len());

        // Loop through choosing the later placement of each offspring in the
        // population.
        let mut parent_alive = true; // Will the parent live through this process?
        if self.cfg.death_prob_parent != 0.0 && self.rng.p(self.cfg.death_prob_parent) {
            parent_alive = false;
        }

        let allow_parent = self.cfg.allow_parent != 0;
        for (i, child) in offspring.iter_mut().enumerate() {
            let target = self.position_offspring(parent_cell, parent, allow_parent);
            // Catch the corner case where birth method = 3 and there are no
            // empty cells.
            if target == parent_cell && !allow_parent {
                target_cells.push(-1);
                continue;
            }
            target_cells.push(target);
            // If we replaced the parent, make a note of this.
            if target == parent_cell {
                parent_alive = false;
                if self.cfg.use_avatars != 0 {
                    self.remove_all_av(parent.uid);
                }
            }
            let child_merit = child.merit;
            let child_org = &mut child.org;
            let child = &mut child_org.state;

            // Update the mutation rates of the offspring.
            if self.cfg.mut_rate_source == 1 {
                // inherit from environment (cell)
                child
                    .mut_rates
                    .copy_from(&self.pop.cells[target as usize].mut_rates);
            } else {
                // inherit from parent
                child.mut_rates.copy_from(&parent.mut_rates);
                if child.mut_rates.meta.copy_mut_prob > 0.0 {
                    child.mut_rates.do_meta_copy_mut(&mut self.rng);
                }
            }

            // Update the phenotypes of each offspring....
            let child_size = child.genome().representation().size();
            child.phenotype.setup_offspring(
                &self.cfg,
                &parent.phenotype,
                child_size,
                &mut self.rng,
            );
            child.phenotype.merit = child_merit;
            child.lineage_label = parent.lineage_label;
            child.cclade_id = parent.cclade_id;

            // If inherited reputation is turned on, set the offspring's
            // reputation (and/or tag) to that of its parent.
            let inherit = self.cfg.inherit_reputation;
            if inherit == 2 || inherit == 3 {
                child.tag = parent.tag;
            }
            if inherit == 1 || inherit == 3 {
                let rep = if parent.has_opinion() {
                    parent.get_opinion().0
                } else {
                    0
                };
                if let Err(e) = child.set_opinion(&self.cfg, update, rep) {
                    self.set_fatal(e);
                }
            }

            // Host-controlled parasite virulence and vertical transmission.
            if self.cfg.virulence_source == 2 || self.cfg.full_vertical_trans != 0.0 {
                self.offspring_parasites(parent, child_org);
            }
            let child = &mut child_org.state;

            // If spatial groups are used, put the offspring in the parents'
            // group; if tolerances are used, check if the offspring is
            // born into the parent's group or immigrates into another one.
            if self.cfg.use_form_groups != 0
                && !self.offspring_group(parent, parent_cell, child, pending_key(i))
            {
                if let Some(t) = target_cells.last_mut() {
                    *t = doomed_cell;
                }
                is_doomed = true;
            }
            if self.cfg.set_ft_at_birth != 0 && self.set_ft_at_birth(parent, parent_cell, child) {
                parent_alive = false;
            }

            if parent.teach {
                child.parent_teacher = true;
            }
            child.parent_ft = parent.forage_target;
            child.p_merit = parent.phenotype.merit.get_double();
            child.parent_multi_threaded = parent.phenotype.is_multi_thread;
        }

        // If we're not about to kill the parent, do some extra work on it.
        if parent_alive {
            if parent.phenotype.merit.get_double() <= 0.0 || self.cfg.birth_method == 13 {
                // if the parent's merit is 0, kill it
                parent.phenotype.to_die = true;
                parent_alive = false;
            } else {
                // Reset inputs and re-calculate merit if required
                if self.cfg.reset_inputs_on_divide > 0 {
                    self.env
                        .setup_inputs(&mut self.rng, &mut self.pop.cells[pc].inputs, true);
                    if self.cfg.precalc_phenotype != 0 {
                        let r = match parent_hw {
                            Some(hw) => self.precalc_parent_phenotype(parent, hw, pc),
                            None => Err(PARENT_HW_STOP.to_string()),
                        };
                        if let Err(e) = r {
                            self.set_fatal(e);
                        }
                    }
                }
                self.adjust_schedule(parent_cell, parent.phenotype.merit.get_double());

                // In a local run, face the offspring toward the parent.
                let birth_method = self.cfg.birth_method;
                if !is_doomed && (birth_method < po::NUM_LOCAL || birth_method == po::PARENT_FACING)
                {
                    for &target in &target_cells {
                        if target != -1 && !self.pop.cells[target as usize].rotate(parent_cell) {
                            self.set_fatal(format!(
                                "offspring cell {target} is not connected to parent cell \
                                 {parent_cell} (C++: cPopulationCell::Rotate loops forever)"
                            ));
                        }
                    }
                }
            }
        }

        // Do any statistics on the parent that just gave birth...
        let gestation = GestationInfo {
            last_copied_size: f64::from(parent.phenotype.copied_size),
            // QUIRK: cOrganism registers the property as "last_exectuted_size"
            // (sic) while Genotype::HandleUnitGestation reads
            // "last_executed_size", so the genotype always adds 0.
            last_executed_size: 0.0,
            last_gestation_time: f64::from(parent.phenotype.gestation_time),
            last_metabolic_rate: parent.phenotype.last_merit(),
            last_fitness: parent.phenotype.fitness,
        };
        self.systematics
            .handle_gestation(&parent_membership, &gestation);

        // Place all of the offspring...
        let epigenetic = self.cfg.epigenetic_method;
        let inherits =
            epigenetic == EPIGENETIC_METHOD_OFFSPRING || epigenetic == EPIGENETIC_METHOD_BOTH;
        for (i, (child, &target)) in offspring.into_iter().zip(&target_cells).enumerate() {
            if target != -1 {
                if self.cfg.use_form_groups != 0 {
                    self.pop.groups.activating = Some(pending_key(i));
                }
                // pass along some state information from parent to offspring
                let mut child_org = child.org;
                if inherits {
                    match parent_hw {
                        Some(hw) => child_org.hardware.inherit_state(hw),
                        None => self.set_fatal(PARENT_HW_STOP),
                    }
                }
                let org_survived = self.activate_organism(
                    child_org,
                    child.membership,
                    target,
                    true,
                    false,
                    Some(&mut *parent),
                );
                self.note_birth(target, parent.id, parent_cell, org_survived);
                // only assign an avatar cell if the org lived through birth
                self.place_offspring_avatar(target, org_survived, parent.uid);
            } else {
                // `delete offspring_array[i]`
                self.systematics.remove_unit(&child.membership);
            }
        }
        if self.cfg.divide_method == DIVIDE_METHOD_SPLIT
            && parent_alive
            && self.cfg.reset_inputs_on_divide != 0
        {
            self.mini_trace_parent_after_divide(parent, parent_cell);
        }

        parent_alive
    }

    // ------------------------------------------------------------------
    // Offspring placement
    // ------------------------------------------------------------------

    fn age_of(&self, cell: i32, parent_cell: i32, parent: &OrgState) -> Option<i32> {
        if cell == parent_cell {
            return Some(parent.phenotype.age);
        }
        self.pop.orgs[cell as usize]
            .as_ref()
            .map(|o| o.state.phenotype.age)
    }

    fn merit_ratio_of(&self, cell: i32, parent_cell: i32, parent: &OrgState) -> Option<f64> {
        // `cOrganism::CalcMeritRatio`
        let ratio = |s: &OrgState| {
            let age = f64::from(s.phenotype.age);
            let merit = s.phenotype.merit.get_double();
            if merit > 0.0 {
                age / merit
            } else {
                age
            }
        };
        if cell == parent_cell {
            return Some(ratio(parent));
        }
        self.pop.orgs[cell as usize]
            .as_ref()
            .map(|o| ratio(&o.state))
    }

    /// Whether `cell` holds the organism being run and that is `parent`
    /// (whose state the caller holds, out of its cell).
    fn running_parent_in(&self, cell: i32, parent: &OrgState) -> bool {
        self.pop
            .running
            .as_ref()
            .is_some_and(|r| r.cell_id == cell && !r.removed && r.uid == parent.uid)
    }

    /// `FindRandEmptyCell(ctx)`
    fn find_rand_empty_cell(&mut self) -> i32 {
        let mut world_size = self.pop.cells.len();
        if self.pop.num_organisms as usize >= world_size {
            return -1;
        }
        let mut cell_idx = self.rng.get_uint(world_size as u32) as usize;
        let mut cell_id = self.pop.empty_cell_id_array[cell_idx];
        while self.is_occupied(cell_id) {
            // no need to pop this cell off the array, just move it and
            // don't check that far anymore
            world_size -= 1;
            self.pop.empty_cell_id_array.swap(cell_idx, world_size);
            // if ! empty cells at all, return error
            if world_size == 1 {
                return -1;
            }
            cell_idx = self.rng.get_uint(world_size as u32) as usize;
            cell_id = self.pop.empty_cell_id_array[cell_idx];
        }
        cell_id
    }

    /// `PositionOffspring(parent_cell, ctx, parent_ok)`: the id of the cell
    /// the offspring goes to.
    ///
    /// `parent` is the dividing organism; when it is the one being run, a
    /// population cap that culls it kills it through this state.
    pub fn position_offspring(
        &mut self,
        parent_cell: i32,
        parent: &mut OrgState,
        parent_ok: bool,
    ) -> i32 {
        let pc = parent_cell as usize;
        let birth_method = self.cfg.birth_method;
        let num_cells = self.pop.cells.len();

        // Handle Population Cap (if enabled)
        let pop_cap = self.cfg.population_cap;
        if pop_cap > 0 && self.pop.num_organisms >= pop_cap {
            let n = self.pop.live_org_list.len();
            let mut target = self.rng.get_uint(n as u32) as usize;
            let mut cell_id = self.pop.live_org_list[target];
            if cell_id == parent_cell {
                target += 1;
                if target >= n {
                    target = 0;
                }
                cell_id = self.pop.live_org_list[target];
            }
            let running_parent = self.running_parent_in(cell_id, parent);
            self.kill_organism(cell_id, running_parent.then_some(&mut *parent));
        }

        // Handle Pop Cap Eldest (if enabled)
        let pop_eldest = self.cfg.pop_cap_eldest;
        if pop_eldest > 0 && self.pop.num_organisms >= pop_eldest {
            let mut max_age = 0.0;
            let mut max_msr = 0.0;
            let mut cell_id = 0;
            for i in 0..self.pop.live_org_list.len() {
                let c = self.pop.live_org_list[i];
                if c != parent_cell {
                    let Some(org) = self.pop.orgs[c as usize].as_ref() else {
                        continue;
                    };
                    let age = f64::from(org.state.phenotype.age);
                    if age > max_age {
                        max_age = age;
                        cell_id = c;
                    } else if age == max_age {
                        let msr = self.rng.get_double();
                        if msr > max_msr {
                            max_msr = msr;
                            cell_id = c;
                        }
                    }
                }
            }
            let running_parent = self.running_parent_in(cell_id, parent);
            self.kill_organism(cell_id, running_parent.then_some(&mut *parent));
        }

        // for juvs with non-predatory parents... The C++ reads
        // `parent_cell.GetOrganism()->IsPreyFT()` here whatever MAX_PREY is,
        // a NULL dereference once a population cap culled the parent.
        if self.pop.orgs[pc].is_none() && !self.running_parent_in(parent_cell, parent) {
            self.set_fatal(
                "unsupported: a population cap culled the dividing parent (C++: \
                 cPopulation::PositionOffspring then reads IsPreyFT() of the emptied \
                 parent cell's NULL organism; the reference binary crashes)",
            );
            return parent_cell;
        }
        self.position_offspring_max_prey(parent_cell, parent);

        // The parent deme's birth count, and migration between demes (which
        // ignores the birth method).
        if let Some(cell_id) = self.position_offspring_demes(parent_cell, parent_ok) {
            return cell_id;
        }

        // This block should be changed to a switch statement with functions
        // handling the cases
        if birth_method == po::FULL_SOUP_RANDOM {
            // Look randomly within empty cells first, if requested
            if self.cfg.prefer_empty != 0 {
                let cell_id = self.find_rand_empty_cell();
                if cell_id == -1 {
                    return self.rng.get_uint(num_cells as u32) as i32;
                }
                return cell_id;
            }
            let mut out_pos = self.rng.get_uint(num_cells as u32) as i32;
            while !parent_ok && out_pos == parent_cell {
                out_pos = self.rng.get_uint(num_cells as u32) as i32;
            }
            return out_pos;
        }

        if birth_method == po::FULL_SOUP_ELDEST {
            let Some(mut out_cell) = self.pop.reaper_queue.pop_back() else {
                self.set_fatal("reaper queue is empty (C++: NULL dereference)");
                return parent_cell;
            };
            if !parent_ok && out_cell == parent_cell {
                match self.pop.reaper_queue.pop_back() {
                    Some(c) => out_cell = c,
                    None => {
                        self.set_fatal("reaper queue is empty (C++: NULL dereference)");
                        return parent_cell;
                    }
                }
                self.pop.reaper_queue.push_back(parent_cell);
            }
            return out_cell;
        }

        if birth_method == po::DEME_RANDOM {
            let deme_id = self.deme_of(parent_cell);
            return self.position_deme_random(deme_id, parent_cell, parent_ok);
        }
        if birth_method == po::PARENT_FACING {
            return self.pop.cells[pc].cell_faced().unwrap_or(parent_cell);
        }
        if birth_method == po::NEXT_CELL {
            let mut out_cell_id = parent_cell + 1;
            if out_cell_id == num_cells as i32 {
                out_cell_id = 0;
            }
            return out_cell_id;
        }
        if birth_method == po::FULL_SOUP_ENERGY_USED {
            return self.position_full_soup_energy_used(parent_cell, parent);
        }

        // All remaining methods require us to choose among mulitple local
        // positions. Construct a list of equally viable locations to place
        // the child... (`tList::Push` adds at the front.)
        let mut found_list: Vec<i32> = Vec::new();
        let conn: Vec<i32> = self.pop.cells[pc].connections.iter().collect();
        let prefer_empty = self.cfg.prefer_empty != 0;

        // First, check if there is an empty organism to work with (always
        // preferred)
        if birth_method == po::DISPERSAL && !conn.is_empty() {
            found_list = self.position_dispersal(parent_cell, parent_ok, prefer_empty);
        } else if prefer_empty {
            for &c in &conn {
                if !self.is_occupied(c) {
                    found_list.insert(0, c);
                }
            }
        }

        // If we have not found an empty organism, we must use the specified
        // function to determine how to choose among the filled organisms.
        if found_list.is_empty() {
            match birth_method {
                po::AGE => {
                    found_list.insert(0, parent_cell);
                    let mut max_age = parent.phenotype.age;
                    if !parent_ok {
                        max_age = -1;
                    }
                    for &c in &conn {
                        let Some(cur_age) = self.age_of(c, parent_cell, parent) else {
                            self.set_fatal(
                                "BIRTH_METHOD 1 with an empty neighbour (C++: NULL dereference \
                                 in cPopulation::PositionAge)",
                            );
                            return parent_cell;
                        };
                        if cur_age > max_age {
                            max_age = cur_age;
                            found_list.clear();
                            found_list.insert(0, c);
                        } else if cur_age == max_age {
                            found_list.insert(0, c);
                        }
                    }
                }
                po::MERIT => {
                    found_list.insert(0, parent_cell);
                    let mut max_ratio = self
                        .merit_ratio_of(parent_cell, parent_cell, parent)
                        .unwrap_or(0.0);
                    if !parent_ok {
                        max_ratio = -1.0;
                    }
                    for &c in &conn {
                        let Some(cur_ratio) = self.merit_ratio_of(c, parent_cell, parent) else {
                            self.set_fatal(
                                "BIRTH_METHOD 2 with an empty neighbour (C++: NULL dereference \
                                 in cPopulation::PositionMerit)",
                            );
                            return parent_cell;
                        };
                        if cur_ratio > max_ratio {
                            max_ratio = cur_ratio;
                            found_list.clear();
                            found_list.insert(0, c);
                        } else if cur_ratio == max_ratio {
                            found_list.insert(0, c);
                        }
                    }
                }
                po::RANDOM => {
                    found_list.extend(conn.iter().copied());
                    if parent_ok {
                        found_list.insert(0, parent_cell);
                    }
                }
                po::NEIGHBORHOOD_ENERGY_USED => {
                    let placed = self.position_energy_used(
                        parent_cell,
                        parent,
                        parent_ok,
                        &conn,
                        &mut found_list,
                    );
                    if !placed {
                        return parent_cell;
                    }
                }
                // POSITION_OFFSPRING_EMPTY: nothing is in list if no empty
                // cells are found...
                _ => {}
            }
        }

        // If there are no possibilities, return parent.
        if found_list.is_empty() {
            return parent_cell;
        }

        // Choose the organism randomly from those in the list, and return it.
        let choice = self.rng.get_uint(found_list.len() as u32) as usize;
        found_list[choice]
    }

    // ------------------------------------------------------------------
    // Inject
    // ------------------------------------------------------------------

    /// `InjectGenome(cell_id, src, genome, ctx, lineage_label, assign_group)`.
    pub fn inject_genome(
        &mut self,
        cell_id: i32,
        src: OrgSource,
        genome: &Genome,
        lineage_label: i32,
        assign_group: bool,
    ) -> Result<(), String> {
        if cell_id < 0 || cell_id as usize >= self.pop.cells.len() {
            return Err("InjectGenotype into nonexistent cell".to_string());
        }
        let c = cell_id as usize;
        let mut org = Organism::new(self, genome, -1, src)?;

        // Set the genotype...
        let update = self.stats.update();
        org.state
            .phenotype
            .setup_inject(&self.cfg, genome.representation().size(), update);

        // Classify this new organism
        let mut membership = GroupMembership::new();
        {
            let source = systematics_source(&org.state.src);
            let unit = UnitInfo {
                source: &source,
                genome: org.state.genome(),
                generation: org.state.phenotype.generation,
            };
            self.systematics
                .classify_new_unit(&unit, None, &mut membership);
        }

        // Coalescense Clade Setup
        org.state.cclade_id = -1;

        let genotype = self
            .systematics
            .genotype_of(&membership)
            .ok_or("injected organism has no genotype")?;
        let metrics = self
            .genome_test_metrics(genotype)
            .ok_or("injected genotype has no genome")?;
        if let Some(f) = self.fatal.borrow_mut().take() {
            return Err(f);
        }
        org.state.phenotype.merit = if self.cfg.energy_enabled {
            self.injected_energy_merit(&org.state.phenotype)
        } else {
            Merit::new(metrics.merit)
        };
        org.state.phenotype.child_copied_size = metrics.copied_size;
        org.state.phenotype.executed_size = metrics.executed_size;
        org.state.phenotype.gestation_time = metrics.gestation_time;

        // Prep the cell..
        if self.cfg.birth_method == po::FULL_SOUP_ELDEST && self.is_occupied(cell_id) {
            // Have to manually take this cell out of the reaper Queue.
            if let Some(pos) = self.pop.reaper_queue.iter().position(|&x| x == cell_id) {
                self.pop.reaper_queue.remove(pos);
            }
        }

        // Setup the child's mutation rates. Since this organism is being
        // injected and has no parent, we should always take the rate from
        // the environment.
        org.state.mut_rates.copy_from(&self.pop.cells[c].mut_rates);

        // Activate the organism in the population...
        org.state.lineage_label = lineage_label;
        self.activate_organism(org, membership, cell_id, assign_group, true, None);
        self.log_injection(cell_id)
    }

    /// `Inject(genome, src, ctx, cell_id, merit, lineage_label, neutral)`.
    pub fn inject(
        &mut self,
        genome: &Genome,
        src: OrgSource,
        cell_id: i32,
        merit: f64,
        lineage_label: i32,
        neutral: f64,
    ) -> Result<(), String> {
        self.inject_into_group(genome, src, cell_id, merit, lineage_label, neutral, None)
    }

    /// `Inject(..., inject_group, group_id, forager_type, trace)`; `group`
    /// is `Some((group_id, forager_type))` for `InjectGroup`.
    #[allow(clippy::too_many_arguments)]
    pub fn inject_into_group(
        &mut self,
        genome: &Genome,
        src: OrgSource,
        mut cell_id: i32,
        merit: f64,
        lineage_label: i32,
        neutral: f64,
        group: Option<(i32, i32)>,
    ) -> Result<(), String> {
        // If an invalid cell was given, choose a new ID for it.
        if cell_id < 0 {
            // QUIRK: the FULL_SOUP_ELDEST case pops the reaper queue and
            // falls through to `cell_id = 0`.
            if self.cfg.birth_method == po::FULL_SOUP_ELDEST {
                self.pop.reaper_queue.pop_back();
            }
            cell_id = 0;
        }

        // `m_world->IsWorldBoundary` is hard-wired to false, so only the
        // deadly-edge nudge applies. QUIRK: unlike the birth and movement
        // checks this one ignores WORLD_GEOMETRY.
        if self.cfg.deadly_boundaries == 1 {
            let wx = self.cfg.world_x;
            let dest_x = cell_id % wx;
            if dest_x == 0 {
                cell_id += 1;
            } else if dest_x == wx - 1 {
                cell_id -= 1;
            }
            let dest_y = cell_id / wx;
            if dest_y == 0 {
                cell_id += wx;
            } else if dest_y == self.cfg.world_y - 1 {
                cell_id -= wx;
            }
        }

        // if the injected org already has a group we will assign it to, do
        // not assign group id in activate organism
        self.inject_genome(cell_id, src, genome, lineage_label, group.is_none())?;
        // The germline needs this even if there's only one deme.
        self.inject_germline(cell_id);

        let c = cell_id as usize;
        let mut sched = None;
        if let Some(org) = self.pop.orgs[c].as_mut() {
            org.state.phenotype.neutral_metric = neutral;
            if merit > 0.0 {
                org.state.phenotype.merit = Merit::new(merit);
            }
            sched = Some(org.state.phenotype.merit.get_double());
            org.state.lineage_label = lineage_label;
        }
        if let Some(m) = sched {
            self.adjust_schedule(cell_id, m);
        }
        self.inject_group_and_avatar(cell_id, group);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Process step
    // ------------------------------------------------------------------

    /// `ProcessStep` / `ProcessStepSpeculative(ctx, step_size, cell_id)`.
    pub fn process_step(
        &mut self,
        step_size: f64,
        cell_id: i32,
        speculative: bool,
    ) -> Result<(), String> {
        if cell_id < 0 {
            return Ok(());
        }
        let c = cell_id as usize;
        let mut org = self.pop.orgs[c]
            .take()
            .ok_or("Unoccupied cell getting processor time!")?;
        if speculative && self.pop.spec_state[c] > 0 {
            // The step below when the organism already ran this instruction
            // (most steps of a speculative run): nothing executes, so the
            // organism is read only where a setting makes its fields matter
            // (`cur_sg` only with state grids, the merit only with demes).
            if !self.host.state_grids.is_empty() {
                self.host.cur_sg = org.state.cur_sg;
            }
            // No `hgt_step_begins`: `pop.running` stays empty on this path,
            // so nothing reads the running genome before the next step that
            // runs records its own.
            self.pop.spec_state[c] -= 1;
            let merit = if self.pop.demes.len() > 1 {
                org.state.phenotype.merit.get_double()
            } else {
                0.0
            };
            self.mini_trace_step_end(None, None);
            self.pop.orgs[c] = Some(org);
            self.pop.last_step_cell = cell_id;
            self.flush_parasite_deaths();
            self.flush_host();
            self.stats.inc_executed();
            self.pop.resource_count.update(step_size);
            self.demes_after_step(step_size, cell_id, merit, speculative);
            return Ok(());
        }
        self.pop.running = Some(Running {
            cell_id,
            org_id: org.state.id,
            uid: org.state.uid,
            ..Running::default()
        });
        self.host.cur_sg = org.state.cur_sg;
        self.hgt_step_begins(&org.state);

        let result = {
            let mut iface = PopulationInterface { w: self, cell_id };
            if !speculative {
                let r = org.single_process(&mut iface, false).map(|_| ());
                // A gradient habitat's `SetToDie` (see crate::habitat).
                iface.w.settle_habitat_to_die(&mut org.state);
                r
            } else if iface.w.pop.spec_state[c] > 0 {
                iface.w.pop.spec_state[c] -= 1;
                Ok(())
            } else {
                let first = org.single_process(&mut iface, false);
                // A gradient habitat's `SetToDie` was read by the hardware at
                // the end of the call, which died and returned false (see
                // crate::habitat).
                iface.w.settle_habitat_to_die(&mut org.state);
                match first {
                    Ok(true) => {
                        let mut spec_count = 0;
                        let mut res = Ok(());
                        while spec_count < 32 {
                            let r = org.single_process(&mut iface, true);
                            iface.w.settle_habitat_to_die(&mut org.state);
                            match r {
                                Ok(true) => spec_count += 1,
                                Ok(false) => break,
                                Err(e) => {
                                    res = Err(e);
                                    break;
                                }
                            }
                        }
                        iface.w.pop.spec_state[c] = spec_count;
                        iface.w.stats.m_spec_total += spec_count;
                        iface.w.stats.m_spec_num += 1;
                        res
                    }
                    Ok(false) => Ok(()),
                    Err(e) => Err(e),
                }
            }
        };

        // `cur_org->GetPhenotype().GetMerit()` after the step (the C++ reads
        // it before deleting a dead organism).
        let merit = org.state.phenotype.merit.get_double();
        let running = self.pop.running.take().unwrap_or_default();
        // The `UpdateQs(cur_org, false)` of a `KillOrganism` that reached the
        // organism outside a divide (see crate::mini_trace).
        let dead = running.removed.then_some(&mut *org);
        self.mini_trace_step_end(dead, running.dead_membership.as_ref());
        if running.removed {
            if running.notify_pending {
                let mut iface = PopulationInterface {
                    w: self,
                    cell_id: running.cell_id,
                };
                org.notify_death(&mut iface);
            }
            // `delete cur_org`
            self.delete_organism(org, running.dead_membership);
        } else {
            // The organism may have moved (`SwapCells`) during its step.
            self.pop.orgs[running.cell_id as usize] = Some(org);
        }
        self.pop.last_step_cell = running.cell_id;
        // Parasites the step let go of (a reset clears them) leave their
        // groups.
        self.flush_parasite_deaths();
        self.flush_host();
        result?;

        self.stats.inc_executed();
        self.pop.resource_count.update(step_size);
        // The deme work comes after the global resource update in
        // `ProcessStep` and before it in `ProcessStepSpeculative`; the two
        // touch disjoint state.
        self.demes_after_step(step_size, cell_id, merit, speculative);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Per-update processing
    // ------------------------------------------------------------------

    /// `ProcessPreUpdate()`
    pub fn process_pre_update(&mut self) {
        self.pop
            .resource_count
            .set_spatial_update(self.stats.update());
        self.demes_pre_update();
    }

    /// `ProcessPostUpdate(ctx)`
    pub fn process_post_update(&mut self) {
        self.process_post_update_with(None);
    }

    /// [`World::process_post_update`], with the organism stats gathered on
    /// the threads of `pool` (see [`World::update_organism_stats_with`]).
    pub fn process_post_update_with(&mut self, pool: Option<&rayon::ThreadPool>) {
        // `ProcessUpdateCellActions`
        for i in 0..self.pop.cells.len() {
            if self.pop.cells[i].mut_rates.test_death(&mut self.rng) {
                self.kill_organism(i as i32, None);
            }
        }
        self.stats.num_creatures = self.pop.num_organisms;
        self.update_deme_stats();
        self.update_organism_stats_with(pool);
        if self.tracks_pred_prey() {
            self.update_ft_org_stats();
        }
        if self.cfg.mating_types {
            self.update_male_female_org_stats();
        }
        self.demes_process_update();
    }

    /// `UpdateMaleFemaleOrgStats(ctx)`: per-organism sums for males and
    /// females.
    pub fn update_male_female_org_stats(&mut self) {
        let stats = &mut self.stats;
        // Clear out organism sums...
        stats.sum_male_fitness.clear();
        stats.sum_male_gestation.clear();
        stats.sum_male_merit.clear();
        stats.sum_male_creature_age.clear();
        stats.sum_male_generation.clear();

        stats.sum_female_fitness.clear();
        stats.sum_female_gestation.clear();
        stats.sum_female_merit.clear();
        stats.sum_female_creature_age.clear();
        stats.sum_female_generation.clear();

        stats.zero_mt_inst();

        for &c in &self.pop.live_org_list {
            let Some(org) = self.pop.orgs[c as usize].as_ref() else {
                continue;
            };
            let phenotype = &org.state.phenotype;
            let cur_merit = phenotype.merit.get_double();
            let cur_fitness = phenotype.fitness;
            let male = phenotype.mating_type == crate::birth_chamber::mating_type::MALE;
            let female = phenotype.mating_type == crate::birth_chamber::mating_type::FEMALE;
            if !male && !female {
                continue;
            }
            let inst_set = org.state.genome().inst_set().to_string();
            let counts = if male {
                stats.sum_male_fitness.add(cur_fitness);
                stats
                    .sum_male_gestation
                    .add(f64::from(phenotype.gestation_time));
                stats.sum_male_merit.add(cur_merit);
                stats.sum_male_creature_age.add(f64::from(phenotype.age));
                stats
                    .sum_male_generation
                    .add(f64::from(phenotype.generation));
                stats.inst_male_exe_counts_for_inst_set(&inst_set)
            } else {
                stats.sum_female_fitness.add(cur_fitness);
                stats
                    .sum_female_gestation
                    .add(f64::from(phenotype.gestation_time));
                stats.sum_female_merit.add(cur_merit);
                stats.sum_female_creature_age.add(f64::from(phenotype.age));
                stats
                    .sum_female_generation
                    .add(f64::from(phenotype.generation));
                stats.inst_female_exe_counts_for_inst_set(&inst_set)
            };
            // The C++ sizes these arrays when the instruction set is loaded
            // (cHardwareManager::LoadInstSet); the port sizes them on first
            // use, to the same length.
            if counts.len() < phenotype.last_inst_count.len() {
                counts.resize_with(phenotype.last_inst_count.len(), Default::default);
            }
            for (j, &v) in phenotype.last_inst_count.iter().enumerate() {
                counts[j].add(v);
            }
        }
    }

    /// `UpdateResStats(ctx)`
    pub fn update_res_stats(&mut self) {
        self.stats.resource_count =
            self.with_pop_resources(|rc, rng, env| rc.get_resources(rng, env).to_vec());
        self.stats.spatial_res_count =
            self.with_pop_resources(|rc, rng, env| rc.get_spatial_res(rng, env).to_vec());
        self.stats.resource_geometry = self.pop.resource_count.get_resources_geometry().to_vec();
    }

    /// `UpdateOrganismStats(ctx)`
    pub fn update_organism_stats(&mut self) {
        self.update_organism_stats_with(None);
    }

    /// [`World::update_organism_stats`], reading the organisms on the
    /// threads of `pool` when there is one (the parallel execution mode).
    /// Either way the sums take the organisms one at a time in
    /// `live_org_list` order, so the result does not depend on the pool.
    pub fn update_organism_stats_with(&mut self, pool: Option<&rayon::ThreadPool>) {
        let num_tasks = self.env.num_tasks().max(0) as usize;
        let dims = OrgStatsDims {
            num_tasks,
            num_reactions: self.env.num_reactions().max(0) as usize,
            sense_size: self.stats.sense_size.max(0) as usize,
        };
        {
            let stats = &mut self.stats;
            stats.sum_fitness.clear();
            stats.sum_gestation.clear();
            stats.sum_merit.clear();
            stats.sum_creature_age.clear();
            stats.sum_generation.clear();
            stats.sum_neutral_metric.clear();
            stats.sum_lineage_label.clear();
            stats.sum_copy_mut_rate.clear();
            stats.sum_div_mut_rate.clear();
            stats.sum_copy_size.clear();
            stats.sum_exe_size.clear();
            stats.sum_mem_size.clear();
            stats.zero_tasks();
            stats.zero_reactions();
        }

        let mut totals = OrgStatsTotals::new();
        let collect_env = self.stats.m_collect_env_test_stats;
        match pool {
            Some(pool) if !collect_env && self.pop.live_org_list.len() >= STATS_POOL_MIN => {
                self.gather_organism_stats(pool, dims, &mut totals);
            }
            _ => {
                let mut events = Vec::new();
                for i in 0..self.pop.live_org_list.len() {
                    let c = self.pop.live_org_list[i] as usize;
                    // The walk is bound by cache misses on each organism's
                    // state: start the next organism's loads now so they
                    // overlap with the work on this one.
                    if let Some(next) = self
                        .pop
                        .live_org_list
                        .get(i + 1)
                        .and_then(|&n| self.pop.orgs.get(n as usize))
                        .and_then(Option::as_deref)
                    {
                        touch_stats_fields(next);
                    }

                    if collect_env {
                        if let Some(g) = self.systematics.genotype_of(&self.pop.membership[c]) {
                            if let Some(metrics) = self.genome_test_metrics(g) {
                                for (j, &count) in
                                    metrics.task_counts.iter().enumerate().take(num_tasks)
                                {
                                    if count > 0 {
                                        self.stats.task_test_count[j] += 1;
                                    }
                                }
                            }
                        }
                    }

                    let Some(org) = self.pop.orgs[c].as_deref_mut() else {
                        continue;
                    };
                    events.clear();
                    let rec = gather_org_stats(org, dims, &mut events);
                    totals.add(&mut self.stats, &rec, &events);
                }
            }
        }
        totals.store(&mut self.stats);

        self.pop.resource_count.update_global_resources();
    }

    /// The organisms' part of [`World::update_organism_stats_with`] on the
    /// threads of `pool`: each task reads the organisms of a fixed run of
    /// cells into records (and ages them), then the sums take the records
    /// in `live_org_list` order, as the walk without a pool does.
    fn gather_organism_stats(
        &mut self,
        pool: &rayon::ThreadPool,
        dims: OrgStatsDims,
        totals: &mut OrgStatsTotals,
    ) {
        let mut live = vec![false; self.pop.orgs.len()];
        for &c in &self.pop.live_org_list {
            live[c as usize] = true;
        }
        let orgs = &mut self.pop.orgs;
        let chunks: Vec<(Vec<Option<OrgStatsRecord<'_>>>, Vec<StatsEvent>)> = pool.install(|| {
            orgs.par_chunks_mut(STATS_CHUNK)
                .zip(live.par_chunks(STATS_CHUNK))
                .map(|(orgs, live)| {
                    let mut events = Vec::new();
                    let records = orgs
                        .iter_mut()
                        .zip(live)
                        .map(|(slot, &live)| {
                            let org = slot.as_deref_mut().filter(|_| live)?;
                            Some(gather_org_stats(org, dims, &mut events))
                        })
                        .collect();
                    (records, events)
                })
                .collect()
        });
        for &c in &self.pop.live_org_list {
            let c = c as usize;
            let (records, events) = &chunks[c / STATS_CHUNK];
            if let Some(rec) = &records[c % STATS_CHUNK] {
                totals.add(&mut self.stats, rec, events);
            }
        }
    }
}

/// Fewer living organisms than this, and [`World::update_organism_stats_with`]
/// walks them on the calling thread even with a pool.
const STATS_POOL_MIN: usize = 512;

/// The cells one task of [`World::gather_organism_stats`] reads.
const STATS_CHUNK: usize = 256;

/// The sizes of the per-organism arrays [`World::update_organism_stats`]
/// reads.
#[derive(Clone, Copy)]
struct OrgStatsDims {
    num_tasks: usize,
    num_reactions: usize,
    sense_size: usize,
}

/// One organism's task, reaction or sense count as
/// [`World::update_organism_stats`] adds it to the stats.
#[derive(Clone, Copy)]
enum StatsEvent {
    CurTask(usize, f64),
    LastTask(usize, f64, i32),
    CurHostTask(usize),
    LastHostTask(usize),
    CurParasiteTask(usize),
    LastParasiteTask(usize),
    CurInternalTask(usize, f64),
    LastInternalTask(usize, f64),
    CurReaction(usize, f64),
    LastReaction(usize, i32, f64),
    Sense(usize, i32),
}

/// What [`World::update_organism_stats`] reads from one organism.
struct OrgStatsRecord<'a> {
    inst_set: &'a str,
    from_message: &'a [i32],
    merit: f64,
    fitness: f64,
    gestation_time: i32,
    age: i32,
    generation: i32,
    neutral_metric: f64,
    lineage_label: i32,
    copy_mut_prob: f64,
    div_mut: f64,
    copied_size: i32,
    executed_size: i32,
    genome_length: i32,
    parasites: i32,
    parent_true: bool,
    no_birth: bool,
    multi_thread: bool,
    modified: bool,
    mem_size: i32,
    num_threads: i32,
    /// Its task, reaction and sense counts, in the order the C++ loop
    /// body adds them, in the buffer the record was gathered into.
    events: std::ops::Range<usize>,
}

/// Reads `org` for [`World::update_organism_stats`], pushing its counts to
/// `events`, and ages it (the last statement of the C++ loop body).
fn gather_org_stats<'a>(
    org: &'a mut Organism,
    dims: OrgStatsDims,
    events: &mut Vec<StatsEvent>,
) -> OrgStatsRecord<'a> {
    let age = org.state.phenotype.age;
    org.state.phenotype.age += 1;
    let org: &'a Organism = org;
    let state = &org.state;
    let p = &state.phenotype;

    let start = events.len();
    // Test what tasks this creatures has completed.
    for j in 0..dims.num_tasks {
        if p.cur_task_count[j] > 0 {
            events.push(StatsEvent::CurTask(j, p.cur_task_quality[j]));
        }
        if p.last_task_count[j] > 0 {
            events.push(StatsEvent::LastTask(
                j,
                p.last_task_quality[j],
                p.last_task_count[j],
            ));
        }
        if p.cur_host_tasks[j] > 0 {
            events.push(StatsEvent::CurHostTask(j));
        }
        if p.last_host_tasks[j] > 0 {
            events.push(StatsEvent::LastHostTask(j));
        }
        if p.cur_para_tasks[j] > 0 {
            events.push(StatsEvent::CurParasiteTask(j));
        }
        if p.last_para_tasks[j] > 0 {
            events.push(StatsEvent::LastParasiteTask(j));
        }
        if p.cur_internal_task_count[j] > 0 {
            events.push(StatsEvent::CurInternalTask(
                j,
                p.cur_internal_task_quality[j],
            ));
        }
        if p.last_internal_task_count[j] > 0 {
            events.push(StatsEvent::LastInternalTask(
                j,
                p.last_internal_task_quality[j],
            ));
        }
    }
    // Record what add bonuses this organism garnered for different
    // reactions
    for j in 0..dims.num_reactions {
        if p.cur_reaction_count[j] > 0 {
            events.push(StatsEvent::CurReaction(j, p.cur_reaction_add_reward[j]));
        }
        if p.last_reaction_count[j] > 0 {
            events.push(StatsEvent::LastReaction(
                j,
                p.last_reaction_count[j],
                p.last_reaction_add_reward[j],
            ));
        }
    }
    // Test what resource combinations this creature has sensed
    for j in 0..dims.sense_size {
        if p.last_sense_count[j] > 0 {
            events.push(StatsEvent::Sense(j, p.last_sense_count[j]));
        }
    }

    OrgStatsRecord {
        inst_set: org.hardware.inst_set().inst_set_name(),
        from_message: &p.last_from_message_count,
        merit: p.merit.get_double(),
        fitness: p.fitness,
        gestation_time: p.gestation_time,
        age,
        generation: p.generation,
        neutral_metric: p.neutral_metric,
        lineage_label: state.lineage_label,
        copy_mut_prob: state.mut_rates.copy.mut_prob,
        div_mut: state.mut_rates.divide.divide_mut_prob / p.div_type,
        copied_size: p.copied_size,
        executed_size: p.executed_size,
        genome_length: p.genome_length,
        parasites: state.parasites.len() as i32,
        parent_true: p.parent_true,
        no_birth: p.num_divides == 0,
        multi_thread: p.is_multi_thread,
        modified: p.is_modified,
        mem_size: org.hardware.memory().size(),
        num_threads: org.hardware.num_threads(),
        events: start..events.len(),
    }
}

/// The totals [`World::update_organism_stats`] keeps outside `Stats` while
/// it walks the organisms.
struct OrgStatsTotals {
    num_breed_true: i32,
    num_parasites: i32,
    num_no_birth: i32,
    num_multi_thread: i32,
    num_single_thread: i32,
    num_threads: i32,
    num_modified: i32,
    max_merit: f64,
    max_fitness: f64,
    max_gestation_time: i32,
    max_genome_length: i32,
    min_merit: f64,
    min_fitness: f64,
    min_gestation_time: i32,
    min_genome_length: i32,
    /// The from-message counts of the last organism's instruction set,
    /// out of `stats` while organisms of that set follow one another: one
    /// map lookup per instruction set instead of two per organism. Put
    /// back by [`OrgStatsTotals::store`].
    msg_counts: Option<(String, Vec<avida_apto::Accumulator<i32>>)>,
}

impl OrgStatsTotals {
    fn new() -> Self {
        OrgStatsTotals {
            num_breed_true: 0,
            num_parasites: 0,
            num_no_birth: 0,
            num_multi_thread: 0,
            num_single_thread: 0,
            num_threads: 0,
            num_modified: 0,
            // Maximums...
            max_merit: 0.0,
            max_fitness: 0.0,
            max_gestation_time: 0,
            max_genome_length: 0,
            // Minimums...
            min_merit: f64::from(f32::MAX),
            min_fitness: f64::from(f32::MAX),
            min_gestation_time: i32::MAX,
            min_genome_length: i32::MAX,
            msg_counts: None,
        }
    }

    /// The C++ loop body for one organism, from its record.
    fn add(&mut self, stats: &mut Stats, rec: &OrgStatsRecord<'_>, events: &[StatsEvent]) {
        if self
            .msg_counts
            .as_ref()
            .is_none_or(|(name, _)| name != rec.inst_set)
        {
            if let Some((name, counts)) = self.msg_counts.take() {
                *stats.inst_from_message_exe_counts_for_inst_set(&name) = counts;
            }
            let counts =
                std::mem::take(stats.inst_from_message_exe_counts_for_inst_set(rec.inst_set));
            self.msg_counts = Some((rec.inst_set.to_string(), counts));
        }
        if let Some((_, counts)) = self.msg_counts.as_mut() {
            for (a, &v) in counts.iter_mut().zip(rec.from_message) {
                a.add(v);
            }
        }

        stats.sum_fitness.add(rec.fitness);
        stats.sum_merit.add(rec.merit);
        stats.sum_gestation.add(f64::from(rec.gestation_time));
        stats.sum_creature_age.add(f64::from(rec.age));
        stats.sum_generation.add(f64::from(rec.generation));
        stats.sum_neutral_metric.add(rec.neutral_metric);
        stats.sum_lineage_label.add(f64::from(rec.lineage_label));
        stats.sum_copy_mut_rate.push(rec.copy_mut_prob);
        stats.sum_log_copy_mut_rate.push(rec.copy_mut_prob.ln());
        stats.sum_div_mut_rate.push(rec.div_mut);
        stats.sum_log_div_mut_rate.push(rec.div_mut.ln());
        stats.sum_copy_size.add(f64::from(rec.copied_size));
        stats.sum_exe_size.add(f64::from(rec.executed_size));

        if rec.merit > self.max_merit {
            self.max_merit = rec.merit;
        }
        if rec.fitness > self.max_fitness {
            self.max_fitness = rec.fitness;
        }
        if rec.gestation_time > self.max_gestation_time {
            self.max_gestation_time = rec.gestation_time;
        }
        if rec.genome_length > self.max_genome_length {
            self.max_genome_length = rec.genome_length;
        }

        if rec.merit < self.min_merit {
            self.min_merit = rec.merit;
        }
        if rec.fitness < self.min_fitness {
            self.min_fitness = rec.fitness;
        }
        if rec.gestation_time < self.min_gestation_time {
            self.min_gestation_time = rec.gestation_time;
        }
        if rec.genome_length < self.min_genome_length {
            self.min_genome_length = rec.genome_length;
        }

        for event in &events[rec.events.clone()] {
            match *event {
                StatsEvent::CurTask(j, quality) => {
                    stats.add_cur_task(j);
                    stats.add_cur_task_quality(j, quality);
                }
                StatsEvent::LastTask(j, quality, count) => {
                    stats.add_last_task(j);
                    stats.add_last_task_quality(j, quality);
                    stats.inc_task_exe_count(j, count);
                }
                StatsEvent::CurHostTask(j) => stats.add_cur_host_task(j),
                StatsEvent::LastHostTask(j) => stats.add_last_host_task(j),
                StatsEvent::CurParasiteTask(j) => stats.add_cur_parasite_task(j),
                StatsEvent::LastParasiteTask(j) => stats.add_last_parasite_task(j),
                StatsEvent::CurInternalTask(j, quality) => {
                    stats.add_cur_internal_task(j);
                    stats.add_cur_internal_task_quality(j, quality);
                }
                StatsEvent::LastInternalTask(j, quality) => {
                    stats.add_last_internal_task(j);
                    stats.add_last_internal_task_quality(j, quality);
                }
                StatsEvent::CurReaction(j, reward) => {
                    stats.add_cur_reaction(j);
                    stats.add_cur_reaction_add_reward(j, reward);
                }
                StatsEvent::LastReaction(j, count, reward) => {
                    stats.add_last_reaction(j);
                    stats.inc_reaction_exe_count(j, count);
                    stats.add_last_reaction_add_reward(j, reward);
                }
                StatsEvent::Sense(j, count) => {
                    stats.sense_last_count[j] += 1;
                    stats.sense_last_exe_count[j] += count;
                }
            }
        }

        // Increment the counts for all qualities the organism has...
        self.num_parasites += rec.parasites;
        if rec.parent_true {
            self.num_breed_true += 1;
        }
        if rec.no_birth {
            self.num_no_birth += 1;
        }
        if rec.multi_thread {
            self.num_multi_thread += 1;
        } else {
            self.num_single_thread += 1;
        }
        if rec.modified {
            self.num_modified += 1;
        }

        // Hardware specific collections...
        stats.sum_mem_size.add(f64::from(rec.mem_size));
        self.num_threads += rec.num_threads;
    }

    /// The totals into `stats`, after the last organism.
    fn store(self, stats: &mut Stats) {
        if let Some((name, counts)) = self.msg_counts {
            *stats.inst_from_message_exe_counts_for_inst_set(&name) = counts;
        }
        stats.num_breed_true_creatures = self.num_breed_true;
        stats.num_no_birth_creatures = self.num_no_birth;
        stats.num_parasites = self.num_parasites;
        stats.num_single_thread_creatures = self.num_single_thread;
        stats.num_multi_thread_creatures = self.num_multi_thread;
        stats.m_num_threads = self.num_threads;
        stats.num_modified = self.num_modified;

        stats.max_merit = self.max_merit;
        stats.max_fitness = self.max_fitness;
        stats.max_gestation_time = self.max_gestation_time;
        stats.max_genome_length = self.max_genome_length;

        stats.min_merit = self.min_merit;
        stats.min_fitness = self.min_fitness;
        stats.min_gestation_time = self.min_gestation_time;
        stats.min_genome_length = self.min_genome_length;
    }
}

/// Reads the fields of `org` that [`World::update_organism_stats`] reads,
/// discarding the values. Only the loads matter: issued one organism
/// ahead, their cache misses overlap with the work on the current one.
#[inline]
fn touch_stats_fields(org: &Organism) {
    let s = &org.state;
    let p = &s.phenotype;
    let first = |v: &[i32]| v.first().copied().unwrap_or(0);
    let first_f = |v: &[f64]| v.first().copied().unwrap_or(0.0);
    std::hint::black_box((
        p.merit.get_double() + p.fitness + p.neutral_metric + p.div_type,
        s.mut_rates.copy.mut_prob + s.mut_rates.divide.divide_mut_prob,
        p.gestation_time ^ p.genome_length ^ p.age ^ p.generation ^ s.lineage_label,
        p.copied_size ^ p.executed_size ^ p.num_divides,
        p.parent_true ^ p.is_multi_thread ^ p.is_modified,
        s.parasites.len(),
    ));
    std::hint::black_box((
        first(&p.cur_task_count) ^ first(&p.last_task_count),
        first(&p.cur_host_tasks) ^ first(&p.last_host_tasks),
        first(&p.cur_para_tasks) ^ first(&p.last_para_tasks),
        first(&p.cur_internal_task_count) ^ first(&p.last_internal_task_count),
        first(&p.cur_reaction_count) ^ first(&p.last_reaction_count),
        first(&p.last_sense_count) ^ first(&p.last_from_message_count),
        first_f(&p.cur_task_quality) + first_f(&p.last_task_quality),
    ));
    std::hint::black_box(org.hardware.num_threads() ^ org.hardware.memory().size());
}
