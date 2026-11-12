//! Deme handling of `cPopulation` (avida-core/source/main/cPopulation.cc at
//! 47f13dadb) and the parts of `cDeme` (cDeme.cc) that reach the cells'
//! organisms: deme bookkeeping on birth and death, offspring migration
//! between demes, deme replication and seeding, and the per-update deme
//! statistics.
//!
//! As in C++ there is always at least one deme; with `NUM_DEMES <= 1` it
//! spans the world and most of this code is inert.
//!
//! Founder seeding (`DEMES_SEED_METHOD 1`), the founder and germline half
//! of `Inject` and `ReplaceDemeFlaggedGermline` (`DEMES_USE_GERMLINE 3`)
//! are in [`crate::deme_founders`], deme competition in
//! [`crate::deme_compete`].
//!
//! Not ported (a fatal error when reached): deme replication with
//! `DEMES_USE_GERMLINE 2`, whose C++ dereferences a NULL genotype with
//! `DEMES_SEED_METHOD 0` and stores founders with an uninitialised
//! phenotype with seed method 1 (see [`World::replace_deme`]). Deme
//! predicates are never created, so the predicate triggers never fire.

use std::collections::VecDeque;

use avida_genome::Genome;
use avida_hardware::organism::{transmission, OrgSource, OrgState};

use crate::stat_tools::DoubleSum;
use crate::topology::grid_neighbor;
use crate::world::World;

/// `eDEME_TRIGGERS` (avida/core/Definitions.h)
pub mod deme_trigger {
    pub const ALL: i32 = 0;
    pub const FULL: i32 = 1;
    pub const CORNERS: i32 = 2;
    pub const AGE: i32 = 3;
    pub const BIRTHS: i32 = 4;
    pub const MOVE_PREDATORS: i32 = 5;
    pub const GROUP_KILL: i32 = 6;
    pub const MESSAGE_PREDATORS: i32 = 7;
    pub const PREDICATE: i32 = 8;
    pub const PERFECT_REACTIONS: i32 = 9;
    pub const CONSUME_RESOURCES: i32 = 10;
    pub const UNKNOWN: i32 = 11;
}

/// The deme replication members of `cStats` (`m_deme_num_repls` ...
/// `m_var_germ_work`), filled by `DemePreReplication` and read by
/// `PrintDemeReplicationData` / `PrintDemeGermlineSequestration`
/// (deme_actions.rs). The treatable/untreatable printers are not ported.
#[derive(Debug, Clone, Default)]
pub struct DemeReplStats {
    pub deme_num_repls: i32,
    pub total_deme_num_repls: i32,
    pub deme_gestation_time: DoubleSum,
    pub deme_births: DoubleSum,
    pub deme_merit: DoubleSum,
    pub deme_generation: DoubleSum,
    pub deme_density: DoubleSum,
    pub deme_num_repls_treatable: i32,
    pub deme_gestation_time_treatable: DoubleSum,
    pub deme_births_treatable: DoubleSum,
    pub deme_merit_treatable: DoubleSum,
    pub deme_generation_treatable: DoubleSum,
    pub deme_density_treatable: DoubleSum,
    pub deme_num_repls_untreatable: i32,
    pub deme_gestation_time_untreatable: DoubleSum,
    pub deme_births_untreatable: DoubleSum,
    pub deme_merit_untreatable: DoubleSum,
    pub deme_generation_untreatable: DoubleSum,
    pub deme_density_untreatable: DoubleSum,
    pub ave_germ_size: VecDeque<f64>,
    pub ave_germ_percent: VecDeque<f64>,
    pub ave_germ_mut: VecDeque<f64>,
    pub var_germ_mut: VecDeque<f64>,
    pub ave_soma_mut: VecDeque<f64>,
    pub var_soma_mut: VecDeque<f64>,
    pub ave_germ_work: VecDeque<f64>,
    pub var_germ_work: VecDeque<f64>,
    pub ave_soma_work: VecDeque<f64>,
    pub var_soma_work: VecDeque<f64>,
}

pub(crate) fn duplication(args: &str) -> OrgSource {
    OrgSource::new(transmission::DUPLICATION, args, false)
}

impl World {
    /// `GetNumDemes()`
    pub fn num_demes(&self) -> usize {
        self.pop.demes.len()
    }

    /// The deme of a cell (`cell.GetDemeID()`).
    pub(crate) fn deme_of(&self, cell: i32) -> usize {
        self.pop.cells[cell as usize].deme_id as usize
    }

    /// The merit `AdjustSchedule(cell, merit)` hands the scheduler: scaled
    /// by the deme's merit when the deme has one.
    pub(crate) fn deme_scaled_merit(&self, cell: i32, merit: f64) -> f64 {
        let Some(deme) = usize::try_from(cell)
            .ok()
            .and_then(|c| self.pop.cells.get(c))
            .and_then(|c| self.pop.demes.get(c.deme_id as usize))
        else {
            return merit;
        };
        if deme.has_deme_merit() {
            merit * deme.current_merit.get_double()
        } else {
            merit
        }
    }

    /// `Driver().Feedback().Error(...)` for each message (printed to stdout)
    /// followed by `Driver().Abort(code)`, or a `cout` message followed by
    /// `exit(code)`: the run ends with that status.
    pub(crate) fn abort_run(&mut self, stdout_text: &str, code: i32, what: &str) {
        self.action_state.stdout.push_str(stdout_text);
        if self.action_state.exit_code.is_none() {
            self.action_state.exit_code = Some(code);
        }
        self.set_fatal(what.to_string());
    }

    /// The end of `InjectGenome`: with `LOG_INJECT` from
    /// `INJECT_LOG_START` on, a line per injected organism in
    /// `injectlog.dat`.
    pub(crate) fn log_injection(&mut self, cell_id: i32) -> Result<(), String> {
        let update = self.stats.update();
        if !self.cfg.log_inject || update < self.cfg.inject_log_start {
            return Ok(());
        }
        let c = cell_id as usize;
        let org_id = self.pop.orgs[c]
            .as_ref()
            .map(|o| o.state.id)
            .ok_or("LOG_INJECT: injected organism is not in its cell")?;
        let deme_id = self.pop.cells[c].deme_id;
        // `GetFacing()` reads the first connection, as `cPopulationCell`
        // does.
        let cell = &self.pop.cells[c];
        let faced = cell
            .cell_faced()
            .ok_or("LOG_INJECT: cell without neighbours (C++: NULL dereference)")?;
        let f = &self.pop.cells[faced as usize];
        let facing = cell.facing((f.x, f.y));
        let df = self.out.static_with_path("injectlog.dat")?;
        df.write(update, "Update");
        df.write(org_id, "Organism ID");
        df.write(deme_id, "Deme ID");
        df.write(facing, "Facing");
        df.endl();
        Ok(())
    }

    /// The state of the organism in `cell`, wherever it is held (`None`
    /// for an empty cell and for the organism being run).
    fn org_state(&self, cell: i32) -> Option<&OrgState> {
        self.pop.orgs[cell as usize].as_ref().map(|o| &o.state)
    }

    fn org_state_mut(&mut self, cell: i32) -> Option<&mut OrgState> {
        self.pop.orgs[cell as usize].as_mut().map(|o| &mut o.state)
    }

    // ------------------------------------------------------------------
    // Birth and death bookkeeping
    // ------------------------------------------------------------------

    /// `deme_array[target_cell.GetDemeID()].IncOrgCount()` in
    /// `ActivateOrganism`.
    pub(crate) fn deme_inc_org_count(&mut self, cell: i32) {
        let d = self.deme_of(cell);
        if let Some(deme) = self.pop.demes.get_mut(d) {
            deme.cur_org_count += 1;
        }
    }

    /// `DecOrgCount()` and `OrganismDeath(cell)` in `KillOrganism`. `state`
    /// is the dying organism (`None` when the caller does not hold it).
    pub(crate) fn deme_organism_death(&mut self, cell: i32, state: Option<&OrgState>) {
        let d = self.deme_of(cell);
        if d >= self.pop.demes.len() {
            return;
        }
        self.pop.demes[d].cur_org_count -= 1;
        let track = self.cfg.demes_track_shannon_info;
        if track == 0 {
            return;
        }
        match state {
            Some(s) => {
                let react = s.phenotype.cumulative_reaction_count(&self.cfg);
                let repro = s.phenotype.num_divides - s.phenotype.num_divides_failed;
                self.pop.demes[d].update_shannon(&react, repro, track);
            }
            None => self.set_fatal(
                "unsupported: DEMES_TRACK_SHANNON_INFO for an organism killed while running \
                 without its state (C++: cDeme::OrganismDeath)",
            ),
        }
    }

    /// `cDeme::UpdateShannonAll()`
    fn deme_update_shannon_all(&mut self, d: usize) {
        let track = self.cfg.demes_track_shannon_info;
        for i in 0..self.pop.demes[d].size() {
            let cell = self.pop.demes[d].cell_id(i);
            if !self.is_occupied(cell) {
                continue;
            }
            let Some(s) = self.org_state(cell) else {
                self.set_fatal("unsupported: cDeme::UpdateShannon on the running organism");
                return;
            };
            let react = s.phenotype.cumulative_reaction_count(&self.cfg);
            let repro = s.phenotype.num_divides - s.phenotype.num_divides_failed;
            self.pop.demes[d].update_shannon(&react, repro, track);
        }
    }

    // ------------------------------------------------------------------
    // Offspring placement
    // ------------------------------------------------------------------

    /// The deme part of `PositionOffspring` that comes after the population
    /// caps: the parent deme's birth count, then `MIGRATION_RATE` and
    /// `DEMES_MIGRATION_RATE` migration. `Some(cell)` when the offspring
    /// migrates.
    pub(crate) fn position_offspring_demes(
        &mut self,
        parent_cell: i32,
        parent_ok: bool,
    ) -> Option<i32> {
        // increment the number of births in the **parent deme**.  in the
        // case of a migration, only the origin has its birth count
        // incremented.
        let parent_deme = self.deme_of(parent_cell);
        self.pop.demes[parent_deme].inc_birth_count();

        // Decide if offspring will migrate to another deme -- if migrating
        // we ignore the birth method.
        if self.cfg.migration_rate > 0.0 && self.rng.p(self.cfg.migration_rate) {
            let num_demes = self.pop.demes.len() as i32;
            if num_demes < 2 {
                self.set_fatal("MIGRATION_RATE with one deme (C++: out-of-bounds deme)");
                return Some(parent_cell);
            }
            //get another -unadjusted- deme id
            let mut rnd_deme_id = self.rng.get_int(num_demes - 1);
            //if the -unadjusted- id is above the excluded id, bump it up one
            if rnd_deme_id >= parent_deme as i32 {
                rnd_deme_id += 1;
            }
            let deme_id = rnd_deme_id as usize;
            // The rest of this is essentially POSITION_OFFSPRING_DEME_RANDOM.
            // QUIRK: it does not honor PREFER_EMPTY in the new deme.
            let deme_size = self.pop.demes[deme_id].size() as u32;
            let mut out_pos = self.rng.get_uint(deme_size) as i32;
            let mut out_cell_id = self.pop.demes[deme_id].cell_id(out_pos);
            while !parent_ok && out_cell_id == parent_cell {
                out_pos = self.rng.get_uint(deme_size) as i32;
                out_cell_id = self.pop.demes[deme_id].cell_id(out_pos);
            }
            self.pop.cells[out_cell_id as usize].migrant = true;
            return Some(out_cell_id);
        }

        if self.cfg.demes_migration_rate > 0.0 && self.rng.p(self.cfg.demes_migration_rate) {
            return Some(self.position_deme_migration(parent_cell, parent_ok));
        }
        None
    }

    /// `PositionDemeMigration(parent_cell, parent_ok)`
    fn position_deme_migration(&mut self, parent_cell: i32, parent_ok: bool) -> i32 {
        let parent_id = self.deme_of(parent_cell) as i32;
        let mut deme_id = parent_id;
        let num_demes = self.pop.demes.len() as i32;
        self.pop.demes[parent_id as usize].migrations_out += 1;

        match self.cfg.demes_migration_method {
            // Position randomly in any other deme
            0 => {
                let mut rnd_deme_id = self.rng.get_int(num_demes - 1);
                if rnd_deme_id >= deme_id {
                    rnd_deme_id += 1;
                }
                deme_id = rnd_deme_id;
            }
            // Position randomly in an adjacent deme in neighborhood
            // (assuming torus); DEMES_NUM_X simulates a grid.
            1 => {
                //get a random eight-neighbor
                let dir = self.rng.get_int(8);
                if self.cfg.demes_num_x == 0 {
                    self.abort_run(
                        "error: DEMES_NUM_X must be non-zero if DEMES_MIGRATION_METHOD 1 used.\n",
                        2,
                        "DEMES_NUM_X must be non-zero if DEMES_MIGRATION_METHOD 1 used.",
                    );
                    return parent_cell;
                }
                let x_size = self.cfg.demes_num_x;
                let y_size = self.cfg.num_demes / x_size;
                let mut x = deme_id % x_size;
                let mut y = deme_id / x_size;
                if matches!(dir, 0..=2) {
                    y -= 1;
                }
                if matches!(dir, 5..=7) {
                    y += 1;
                }
                if matches!(dir, 0 | 3 | 5) {
                    x -= 1;
                }
                if matches!(dir, 2 | 4 | 7) {
                    x += 1;
                }
                //handle boundary conditions...
                x = (x + x_size) % x_size;
                y = (y + y_size) % y_size;
                deme_id = x + x_size * y;
            }
            // Random deme adjacent in list. QUIRK: `GetInt(1)` is always 0,
            // so the offspring always goes to the previous deme.
            2 => {
                let mut rnd_deme_id = self.rng.get_int(1);
                if rnd_deme_id == 0 {
                    rnd_deme_id = -1;
                }
                deme_id = (deme_id + rnd_deme_id + num_demes) % num_demes;
            }
            // Proportional-based on a points system (hjg): the odds of a
            // deme being selected are inversely proportional to its points.
            3 => {
                let mut total_points = 0.0;
                for did in 0..num_demes {
                    if did != parent_id {
                        total_points += 1.0 / (1.0 + self.pop.demes[did as usize].points);
                    }
                }
                let rand_point = self.rng.get_double_range(0.0, total_points);
                let mut lower_point = 0.0;
                for curr_deme in 0..num_demes {
                    if curr_deme != parent_id {
                        let upper_point =
                            lower_point + (1.0 + self.pop.demes[curr_deme as usize].points);
                        if lower_point <= rand_point && rand_point < upper_point {
                            deme_id = curr_deme;
                        }
                        lower_point = upper_point;
                    }
                }
            }
            4 => {
                match self.pop.migration_matrix.probabilistic_deme_id(
                    parent_id,
                    &mut self.rng,
                    false,
                ) {
                    Ok(d) => deme_id = d,
                    Err(e) => {
                        self.set_fatal(e);
                        return parent_cell;
                    }
                }
            }
            _ => {}
        }

        let Some(deme) = usize::try_from(deme_id)
            .ok()
            .filter(|&d| d < self.pop.demes.len())
        else {
            self.set_fatal(format!(
                "migration to deme {deme_id} out of range (C++: out-of-bounds deme)"
            ));
            return parent_cell;
        };
        self.pop.demes[deme].migrations_in += 1;

        // Now return an empty cell from the chosen deme
        let mig_cell = self.position_deme_random(deme, parent_cell, parent_ok);
        self.pop.cells[mig_cell as usize].migrant = true;
        mig_cell
    }

    /// `PositionDemeRandom(deme_id, parent_cell, parent_ok)`
    pub(crate) fn position_deme_random(
        &mut self,
        deme_id: usize,
        parent_cell: i32,
        parent_ok: bool,
    ) -> i32 {
        let deme_size = self.pop.demes[deme_id].size() as u32;
        // Look randomly within empty cells first, if requested
        if self.cfg.prefer_empty != 0 {
            let num_empty_cells = self.update_empty_cell_id_array(Some(deme_id));
            if num_empty_cells > 0 {
                let out_pos = self.rng.get_uint(num_empty_cells as u32) as usize;
                return self.pop.empty_cell_id_array[out_pos];
            }
        }
        let mut out_pos = self.rng.get_uint(deme_size) as i32;
        let mut out_cell_id = self.pop.demes[deme_id].cell_id(out_pos);
        while !parent_ok && out_cell_id == parent_cell {
            out_pos = self.rng.get_uint(deme_size) as i32;
            out_cell_id = self.pop.demes[deme_id].cell_id(out_pos);
        }
        out_cell_id
    }

    /// `UpdateEmptyCellIDArray(deme_id)`: writes the empty cells of the
    /// deme (`None`: of the world) to the front of `empty_cell_id_array`
    /// and returns how many there are.
    pub(crate) fn update_empty_cell_id_array(&mut self, deme_id: Option<usize>) -> usize {
        let cells: Vec<i32> = match deme_id {
            None => (0..self.pop.cells.len() as i32).collect(),
            Some(d) => self.pop.demes[d].cell_ids.clone(),
        };
        let mut num_empty_cells = 0;
        for c in cells {
            if !self.is_occupied(c) {
                self.pop.empty_cell_id_array[num_empty_cells] = c;
                num_empty_cells += 1;
            }
        }
        num_empty_cells
    }

    // ------------------------------------------------------------------
    // Process step and update
    // ------------------------------------------------------------------

    /// The deme half of `ProcessStep` (`speculative == false`) and
    /// `ProcessStepSpeculative` after the organism in `cell_id` ran with
    /// `merit` (its merit after the step).
    pub(crate) fn demes_after_step(
        &mut self,
        step_size: f64,
        cell_id: i32,
        merit: f64,
        speculative: bool,
    ) {
        // These must be done even if there is only one deme (but the
        // speculative step skips them then).
        if speculative && self.pop.demes.len() <= 1 {
            return;
        }
        for deme in &mut self.pop.demes {
            deme.resource_count.update(step_size);
        }
        let d = self.deme_of(cell_id);
        self.pop.demes[d].inc_time_used(merit);
        self.pop.seed_limbo.speculative_step = speculative;
        self.check_implicit_deme_repro(d);
        self.pop.seed_limbo.speculative_step = false;
    }

    /// `CheckImplicitDemeRepro(deme, ctx)`
    fn check_implicit_deme_repro(&mut self, d: usize) {
        if self.pop.demes.len() <= 1 {
            return;
        }
        let cfg = &self.cfg;
        let deme = &self.pop.demes[d];
        // The C++ `if / else if` chain calls `ReplicateDeme` from whichever
        // branch matches first; the conditions have no side effects.
        let replicate = (cfg.demes_replicate_cpu_cycles != 0.0
            && f64::from(deme.time_used) >= cfg.demes_replicate_cpu_cycles)
            || (cfg.demes_replicate_time != 0.0
                && deme.cur_normalized_time_used >= cfg.demes_replicate_time)
            || (cfg.demes_replicate_births != 0
                && deme.cur_birth_count >= cfg.demes_replicate_births)
            || (cfg.demes_replicate_orgs != 0 && deme.cur_org_count >= cfg.demes_replicate_orgs);
        if replicate {
            self.replicate_deme(d);
        }
    }

    /// The deme half of `ProcessPreUpdate()`.
    pub(crate) fn demes_pre_update(&mut self) {
        let update = self.stats.update();
        for deme in &mut self.pop.demes {
            deme.process_pre_update(update);
        }
    }

    /// The deme loop at the end of `ProcessPostUpdate(ctx)`.
    pub(crate) fn demes_process_update(&mut self) {
        for d in 0..self.pop.demes.len() {
            if let Err(e) = self.deme_process_update(d) {
                self.set_fatal(e);
            }
            if self.fatal_message().is_some() || self.action_state.exit_code.is_some() {
                return;
            }
        }
    }

    /// `UpdateDemeStats(ctx)`
    pub(crate) fn update_deme_stats(&mut self) {
        // These must be updated, even if there is only one deme
        for deme in &mut self.pop.demes {
            deme.resource_count
                .get_resources(&mut self.rng, &mut self.res_env);
        }
        // bail early to save time if there are no demes
        if self.pop.demes.len() == 1 {
            return;
        }
        let stats = &mut self.stats;
        stats.sum_deme_age.clear();
        stats.sum_deme_birth_count.clear();
        stats.sum_deme_org_count.clear();
        stats.sum_deme_generation.clear();
        stats.sum_deme_gestation_time.clear();
        stats.sum_deme_normalized_time_used.clear();
        stats.sum_deme_merit.clear();
        stats.sum_deme_generations_per_lifetime.clear();
        stats.clear_num_occupied_demes();
        stats.sum_deme_events_killed.clear();
        stats.sum_deme_events_kill_attempts.clear();
        // QUIRK: SumDemeLastBirthCount and SumDemeLastOrgCount are added to
        // below but never cleared.
        for deme in &self.pop.demes {
            if deme.is_empty() {
                continue;
            }
            stats.inc_num_occupied_demes();
            stats.sum_deme_age.add(deme.age);
            stats.sum_deme_birth_count.add(deme.cur_birth_count);
            stats.sum_deme_org_count.add(deme.cur_org_count);
            stats.sum_deme_generation.add(deme.generation);
            stats.sum_deme_last_birth_count.add(deme.last_birth_count);
            stats.sum_deme_last_org_count.add(deme.last_org_count);
            stats.sum_deme_gestation_time.add(deme.gestation_time);
            stats
                .sum_deme_normalized_time_used
                .add(deme.last_normalized_time_used);
            stats.sum_deme_merit.add(deme.current_merit.get_double());
            stats
                .sum_deme_generations_per_lifetime
                .add(deme.generations_per_lifetime);
            stats.sum_deme_events_killed.add(deme.events_killed as i32);
            stats
                .sum_deme_events_kill_attempts
                .add(deme.event_kill_attempts as i32);
        }
    }

    // ------------------------------------------------------------------
    // Replication
    // ------------------------------------------------------------------

    /// `ReplicateDemes(rep_trigger, ctx)`
    pub fn replicate_demes(&mut self, rep_trigger: i32) {
        use deme_trigger as t;
        let num_demes = self.pop.demes.len();
        for deme_id in 0..num_demes {
            let deme = &self.pop.demes[deme_id];
            let cfg = &self.cfg;
            // Test this deme to determine if it should be replicated.
            let skip = match rep_trigger {
                // Replicate all non-empty demes.
                t::ALL => deme.is_empty(),
                // Replicate all full demes.
                t::FULL => !deme.is_full(),
                // Replicate all demes with the corners filled in.
                t::CORNERS => {
                    let id1 = deme.cell_id(0);
                    let id2 = deme.cell_id(deme.size() - 1);
                    !self.is_occupied(id1) || !self.is_occupied(id2)
                }
                // Replicate old demes.
                t::AGE => deme.age < cfg.demes_max_age,
                // Replicate demes that have had a certain number of births.
                t::BIRTHS => deme.cur_birth_count < cfg.demes_max_births,
                // No action creates a message predicate, so that list is
                // always empty.
                t::MESSAGE_PREDATORS => true,
                t::MOVE_PREDATORS => !deme.events.mov_pred_satisfied_previously(),
                t::PREDICATE => !deme.events.deme_pred_satisfied_previously(),
                t::GROUP_KILL => {
                    let kill_ratio = if deme.slot_flow_rate() == 0 {
                        1.0
                    } else {
                        f64::from(deme.events_killed_this_slot) / f64::from(deme.slot_flow_rate())
                    };
                    let current_slot_successful =
                        i64::from(kill_ratio >= cfg.demes_mim_events_killed_ratio);
                    i64::from(deme.consecutive_successful_event_periods) + current_slot_successful
                        < i64::from(cfg.demes_mim_successful_event_periods)
                }
                // loop through each reaction. Make sure each has been
                // performed X times.
                t::PERFECT_REACTIONS => {
                    deme.min_num_times_reaction_performed() < cfg.reaction_thresh
                }
                t::CONSUME_RESOURCES => deme.total_res_consumed < f64::from(cfg.res_for_deme_rep),
                _ => {
                    // `assert(false)` is compiled out of the reference build,
                    // which goes on to replicate the deme.
                    eprintln!(
                        "ERROR: Invalid replication trigger {rep_trigger} in \
                         cPopulation::ReplicateDemes()"
                    );
                    false
                }
            };
            if skip {
                continue;
            }
            self.replicate_deme(deme_id);
            if self.fatal_message().is_some() {
                return;
            }
        }
    }

    /// `ReplicateDeme(source_deme, ctx)`
    pub fn replicate_deme(&mut self, source: usize) {
        // Doesn't make sense to try and replicate a deme that *has no
        // organisms*.
        if self.pop.demes[source].is_empty() {
            return;
        }
        self.deme_update_shannon_all(source);

        // Prevent sterile demes from replicating.
        if self.cfg.demes_prevent_sterile != 0 && self.pop.demes[source].cur_birth_count == 0 {
            // assumes that all group level tasks cannot be solved by a
            // single organism
            self.deme_kill_all(source);
            return;
        }

        // Update stats calculate how many different reactions the deme
        // performed.
        let deme = &self.pop.demes[source];
        let deme_performed_rx = deme.cur_reaction_count.iter().filter(|&&c| c > 0).count() as f64;
        // calculate how many penalties were accrued by the orgs on average
        let mut switch_penalties = f64::from(deme.switch_penalties);
        let num_orgs_perf_reaction = f64::from(deme.num_active);
        let shannon_div = deme.shannon_mutual_information();
        let per_reproductives = deme.percent_reproductives();
        if switch_penalties > 0.0 {
            switch_penalties /= f64::from(deme.injected_count + deme.cur_birth_count);
        }
        self.stats.inc_deme_reaction_diversity_replication_data(
            deme_performed_rx,
            switch_penalties,
            shannon_div,
            num_orgs_perf_reaction,
            per_reproductives,
        );

        //Option to bridge between kin and group selection.
        if self.cfg.demes_replication_only_resets != 0 {
            //Reset deme (resources and births, among other things)
            let reset = self.cfg.demes_reset_resources == 0;
            self.deme_divide_reset(source, source, reset, 0.0);
            //Reset all organisms in deme, by re-injecting them?
            if self.cfg.demes_replication_only_resets == 2 {
                for i in 0..self.pop.demes[source].size() {
                    let cellid = self.pop.demes[source].cell_id(i);
                    if !self.is_occupied(cellid) {
                        continue;
                    }
                    let Some(s) = self.org_state(cellid) else {
                        self.set_fatal("unsupported: re-injecting the running organism");
                        return;
                    };
                    let lineage = s.lineage_label;
                    let genome = s.genome().clone();
                    if let Err(e) =
                        self.inject_genome(cellid, duplication(""), &genome, lineage, true)
                    {
                        self.set_fatal(e);
                        return;
                    }
                }
            }
            return;
        }

        // Pick a target deme to replicate to, making sure that we don't try
        // to replicate over ourself, i.e. DEMES_REPLACE_PARENT 0
        let source_id = source as i32;
        let num_demes = self.pop.demes.len() as i32;
        let partition_interval = self.cfg.demes_partition_interval;
        let begin = if partition_interval != 0 {
            (source_id / partition_interval) * partition_interval
        } else {
            0
        };
        let end = if partition_interval != 0 {
            begin + partition_interval
        } else {
            num_demes
        };
        let partition_size = end - begin;
        if partition_size < 2 {
            let text = format!(
                "error: Partitions must have at least two demes.\n\
                 error: Partition interval is {partition_interval}.\n\
                 error: There are {partition_size} demes in partition.\n\
                 error: Partition begins at {begin} and ends at {end}.\n"
            );
            self.abort_run(&text, 2, "Partitions must have at least two demes.");
            return;
        }
        if end > num_demes {
            self.set_fatal("deme partition extends past the last deme (C++: out-of-bounds deme)");
            return;
        }

        let mut target_id = -1;
        if self.cfg.demes_prefer_empty != 0 {
            // @JEB -- use empty_cell_id_array to hold empty demes so we
            // don't have to allocate a list
            let mut num_empty = 0;
            for i in begin..end {
                if self.pop.demes[i as usize].is_empty() {
                    self.pop.empty_cell_id_array[num_empty] = i;
                    num_empty += 1;
                }
            }
            if num_empty > 0 {
                let pick = self.rng.get_uint(num_empty as u32) as usize;
                target_id = self.pop.empty_cell_id_array[pick];
            }
        }
        // if we haven't found one yet, choose a random one
        if target_id == -1 {
            target_id = source_id;
            while target_id == source_id {
                let target_pos = self.rng.get_uint(partition_size as u32) as i32;
                target_id = begin + target_pos;
            }
        }

        // Write some logging information if LOG_DEMES_REPLICATE is set.
        if self.cfg.log_demes_replicate && self.stats.update() >= self.cfg.demes_replicate_log_start
        {
            let line = format!("{},{},{}", self.stats.update(), source_id, target_id);
            match self.out.static_with_path("deme_replication.dat") {
                Ok(df) => df.write_raw(&line),
                Err(e) => {
                    self.set_fatal(e);
                    return;
                }
            }
        }

        if self.cfg.demes_use_germline == 3 {
            if let Err(e) = self.replace_deme_flagged_germline(source, target_id as usize) {
                self.set_fatal(e);
            }
        } else {
            self.replace_deme(source, target_id as usize);
        }
    }

    /// `target.DivideReset(ctx, parent, reset_resources, deme_energy)`
    /// (`deme_energy` is 0 with the energy model off).
    pub(crate) fn deme_divide_reset(
        &mut self,
        target: usize,
        parent: usize,
        reset_resources: bool,
        deme_energy: f64,
    ) {
        let snapshot = self.pop.demes[parent].divide_parent();
        let World {
            pop,
            cfg,
            rng,
            res_env,
            ..
        } = self;
        pop.demes[target].divide_reset(&snapshot, cfg, rng, res_env, reset_resources, deme_energy);
    }

    /// `ReplaceDeme(source_deme, target_deme, ctx)`
    pub fn replace_deme(&mut self, source: usize, target: usize) {
        // Stats tracking; pre-replication hook.
        self.deme_pre_replication(source);

        // used to pass energy to offspring demes (zero if energy model is
        // not enabled)
        let (parent_deme_energy, offspring_deme_energy) = self.deme_replication_energy(source);

        let (source_reset, target_reset) = match self.cfg.demes_reset_resources {
            // reset resource in both demes
            0 => (true, true),
            // reset resource only in target deme
            1 => (false, true),
            // do not reset either deme resource
            2 => (false, false),
            v => {
                self.abort_run(
                    &format!("Undefined value {v} for DEMES_RESET_RESOURCES\n"),
                    1,
                    "Undefined value for DEMES_RESET_RESOURCES",
                );
                return;
            }
        };

        // Reset both demes, in case they have any cleanup work to do. Must
        // reset target first for stats to be correctly updated!
        let mut target_successfully_seeded = true;
        self.deme_divide_reset(target, source, target_reset, offspring_deme_energy);
        self.deme_divide_reset(source, source, source_reset, parent_deme_energy);

        // Are we using germlines?  If so, we need to mutate the germline to
        // get the genome that we're going to seed the target with.
        match self.cfg.demes_use_germline {
            1 => {
                if let Err(e) = self.replicate_germline(source, target) {
                    self.set_fatal(e);
                    return;
                }
            }
            2 => {
                // With DEMES_SEED_METHOD 0 nothing sets the germline
                // genotype, so `Group(0)` is NULL and the reference binary
                // crashes here; with seed method 1, `SeedDeme(deme, bg)`
                // adds founders with a default-constructed `cPhenotype`
                // whose lifetime fields (the generation `DivideReset`
                // averages) are uninitialised, and longer runs crash in
                // `GenotypeArbiter::PerformUpdate`.
                self.set_fatal(
                    "unsupported: deme replication with DEMES_USE_GERMLINE 2 (C++: \
                     cPopulation::ReplaceDeme reads a NULL germline genotype or founders with \
                     uninitialised phenotypes)",
                );
                return;
            }
            _ => {
                // Not using germlines; things are much simpler. Seed the
                // target from the source.
                match self.seed_deme_from(source, target) {
                    Ok(seeded) => target_successfully_seeded = seeded,
                    Err(e) => {
                        self.set_fatal(e);
                        return;
                    }
                }
                if self.fatal_message().is_some() {
                    return;
                }
            }
        }

        // split energy from parent deme evenly among orgs in child deme
        self.distribute_deme_energy(source, target, parent_deme_energy, offspring_deme_energy);

        // The source's merit must be transferred to the target, and then the
        // source has to rotate its heritable merit to its current merit.
        if target_successfully_seeded {
            let heritable = self.pop.demes[source].next_merit;
            self.pop.demes[target].update_deme_merit_from(heritable);
        }
        self.pop.demes[source].update_deme_merit();
        self.pop.demes[source].clear_shannon_information_stats();
        self.pop.demes[target].clear_shannon_information_stats();

        // do our post-replication stats tracking.
        let genotype_ids = self.deme_genotype_ids(target);
        self.stats
            .deme_post_replication(target as i32, genotype_ids);
    }

    /// The `DEMES_USE_GERMLINE == 1` branch of `ReplaceDeme`.
    fn replicate_germline(&mut self, source: usize, target: usize) -> Result<(), String> {
        // @JEB Original germlines
        let mut next_germ = self.pop.demes[source]
            .germline
            .last()
            .cloned()
            .ok_or("empty deme germline (C++: out-of-bounds cGermline::GetLatest)")?;
        self.mutate_germline(&mut next_germ, None)?;

        // Replace the target deme's germline with the source deme's, and
        // add the newly-mutated germ to ONLY the target's germline.  The
        // source remains unchanged.
        let source_germline = self.pop.demes[source].germline.clone();
        self.pop.demes[target].replace_germline(&source_germline);
        self.pop.demes[target].germline.push(next_germ);

        // Germline stats tracking.
        self.stats
            .germline_replication(source_germline.len() as i32);

        // All done with the germline manipulation; seed each deme.
        let latest = self.pop.demes[source]
            .germline
            .last()
            .cloned()
            .ok_or("empty deme germline")?;
        self.seed_deme_genome(source, &latest, duplication("germline"))?;
        // MJM - source and target deme could be the same! Seeding the same
        // deme twice probably shouldn't happen.
        if source != target {
            let latest = self.pop.demes[target]
                .germline
                .last()
                .cloned()
                .ok_or("empty deme germline")?;
            self.seed_deme_genome(target, &latest, duplication("germline"))?;
        }
        Ok(())
    }

    /// `SeedDeme(deme, genome, src, ctx)`
    pub fn seed_deme_genome(
        &mut self,
        d: usize,
        genome: &Genome,
        src: OrgSource,
    ) -> Result<(), String> {
        // Kill all the organisms in the deme.
        self.deme_kill_all(d);
        // Create the specified number of organisms in the deme.
        for i in 0..self.cfg.demes_replicate_size {
            let cellid = self.deme_select_injection_cell(d, i)?;
            self.inject_genome(cellid, src.clone(), genome, 0, true)?;
            self.deme_post_injection(d, cellid);
        }
        Ok(())
    }

    /// `SeedDeme(source_deme, target_deme, ctx)`: returns whether the target
    /// deme was successfully seeded.
    fn seed_deme_from(&mut self, source: usize, target: usize) -> Result<bool, String> {
        let prob_transfer = self.cfg.demes_prob_org_transfer;
        if prob_transfer != 0.0 {
            // Probabilistic organism replication -- aka "fruiting body."
            // Kill all the organisms in the target deme.
            self.deme_kill_all(target);
            let mut j = 0;
            for i in 0..self.pop.demes[source].size() {
                let source_cellid = self.pop.demes[source].cell_id(i);
                // Does this organism stay with the source or move to the
                // target?
                if self.is_occupied(source_cellid) && self.rng.p(prob_transfer) {
                    let s = self
                        .org_state(source_cellid)
                        .ok_or("unsupported: transferring the running organism")?;
                    let genome = s.genome().clone();
                    let lineage = s.lineage_label;
                    // Remove this organism from the source.
                    self.kill_organism(source_cellid, None);
                    // And inject it into target deme.
                    let target_cellid = self.deme_select_injection_cell(target, j)?;
                    j += 1;
                    self.inject_genome(target_cellid, duplication(""), &genome, lineage, true)?;
                    self.deme_post_injection(target, target_cellid);
                }
            }
            return Ok(true);
        }

        if self.cfg.demes_seed_method != 0 {
            return self.seed_deme_founders(source, target);
        }

        // Store up a list of the genotypes from the source that we need to
        // copy to the target. Then clear both the source and target demes,
        // and finally inject organisms from the saved genotypes into both.
        let replicate_size = self.cfg.demes_replicate_size;
        let mut xfer: Vec<(Genome, i32)> = Vec::new();
        match self.cfg.demes_organism_selection {
            // Random w/ replacement (meaning, we don't prevent the same
            // genotype from being selected more than once).
            0 => {
                let size = self.pop.demes[source].size() as u32;
                while (xfer.len() as i32) < replicate_size {
                    let pos = self.rng.get_uint(size) as i32;
                    let cellid = self.pop.demes[source].cell_id(pos);
                    if self.is_occupied(cellid) {
                        let s = self
                            .org_state(cellid)
                            .ok_or("unsupported: seeding from the running organism")?;
                        xfer.push((s.genome().clone(), s.lineage_label));
                    }
                }
            }
            // Sequential selection, from the beginning.
            1 => {
                for i in 0..replicate_size {
                    let cellid = self.pop.demes[source].cell_id(i);
                    if self.is_occupied(cellid) {
                        let s = self
                            .org_state(cellid)
                            .ok_or("unsupported: seeding from the running organism")?;
                        xfer.push((s.genome().clone(), s.lineage_label));
                    }
                }
            }
            v => {
                self.abort_run(
                    &format!("Undefined value ({v}) for DEMES_ORGANISM_SELECTION.\n"),
                    1,
                    "Undefined value for DEMES_ORGANISM_SELECTION",
                );
                return Ok(true);
            }
        }

        // Clear the demes.
        self.deme_update_stats(source);
        self.deme_kill_all(source);
        self.deme_update_stats(target);
        self.deme_kill_all(target);

        // And now populate the source and target.
        for (j, (genome, lineage)) in xfer.iter().enumerate() {
            let j = j as i32;
            let cellid = self.deme_select_injection_cell(source, j)?;
            self.inject_genome(cellid, duplication(""), genome, *lineage, true)?;
            self.deme_post_injection(source, cellid);
            if source != target {
                let cellid = self.deme_select_injection_cell(target, j)?;
                self.inject_genome(cellid, duplication(""), genome, *lineage, true)?;
                self.deme_post_injection(target, cellid);
            }
        }
        Ok(true)
    }

    /// `DemeSelectInjectionCell(deme, sequence)`
    pub(crate) fn deme_select_injection_cell(
        &mut self,
        d: usize,
        sequence: i32,
    ) -> Result<i32, String> {
        let deme = &self.pop.demes[d];
        let size = deme.size();
        if sequence >= size {
            return Err("deme injection sequence exceeds the deme size (C++: assertion)".into());
        }
        Ok(match self.cfg.demes_organism_placement {
            // Array-middle.
            0 => deme.cell_id((size / 2 + sequence) % size),
            // Sequential placement, start in the center of the deme.
            1 => deme.cell_id_xy(deme.width / 2, deme.height() / 2),
            // Random placement.
            2 => {
                let mut cellid = deme.cell_id(self.rng.get_int_range(0, size));
                while self.is_occupied(cellid) {
                    cellid = self.pop.demes[d].cell_id(self.rng.get_int_range(0, size));
                }
                cellid
            }
            // Sequential.
            3 => deme.cell_id(sequence),
            v => {
                return Err(format!(
                    "DEMES_ORGANISM_PLACEMENT {v} (C++: assertion, then cell -1)"
                ))
            }
        })
    }

    /// `DemePostInjection(deme, cell)`
    pub(crate) fn deme_post_injection(&mut self, d: usize, cellid: i32) {
        self.pop.demes[d].injected_count += 1;
        let c = cellid as usize;
        match self.cfg.demes_organism_facing {
            // Spin cell to face NW.
            1 => {
                let deme = &self.pop.demes[d];
                let first = deme.cell_id(0);
                let target =
                    grid_neighbor(cellid - first, deme.width, deme.height(), -1, -1) + first;
                if !self.pop.cells[c].rotate(target) {
                    self.set_fatal(format!(
                        "cell {cellid} is not connected to cell {target} (C++: \
                         cPopulationCell::Rotate loops forever)"
                    ));
                }
            }
            // Spin cell to face randomly.
            2 => {
                let n = self.pop.cells[c].connections.len() as i32;
                let rotate_count = self.rng.get_int_range(0, n);
                for _ in 0..rotate_count {
                    self.pop.cells[c].connections.circ_next();
                }
            }
            // 0: Unchanged (anything else trips an assertion in C++).
            _ => {}
        }
    }

    /// `cDeme::KillAll(ctx)`
    pub fn deme_kill_all(&mut self, d: usize) {
        self.pop.demes[d].last_org_count = self.pop.demes[d].cur_org_count;
        for i in 0..self.pop.demes[d].size() {
            let cell = self.pop.demes[d].cell_id(i);
            if self.is_occupied(cell) {
                self.kill_organism(cell, None);
            }
        }
        // HACK: organism are killed after DivideReset is called. need clear
        // a deme before it is reset.
        self.pop.demes[d].sleeping_count = 0;
    }

    /// `cDeme::UpdateStats()`: what tasks the deme's organisms were doing.
    pub(crate) fn deme_update_stats(&mut self, d: usize) {
        let cells = self.pop.demes[d].cell_ids.clone();
        let num_tasks = self.pop.demes[d].cur_org_task_count.len();
        let num_reactions = self.pop.demes[d].cur_org_reaction_count.len();
        let mut task_count = vec![0; num_tasks];
        let mut task_exe_count = vec![0; num_tasks];
        let mut reaction_count = vec![0; num_reactions];
        for &cell in &cells {
            let Some(s) = self.org_state(cell) else {
                continue;
            };
            let ph = &s.phenotype;
            for j in 0..num_tasks {
                let c = ph.last_task_count.get(j).copied().unwrap_or(0);
                task_count[j] += i32::from(c > 0);
                task_exe_count[j] += c;
            }
            for (j, r) in reaction_count.iter_mut().enumerate() {
                *r += ph.last_reaction_count.get(j).copied().unwrap_or(0);
            }
        }
        // The C++ assigns inside the cell loop, so an empty deme keeps the
        // old counts.
        if !cells.is_empty() {
            let deme = &mut self.pop.demes[d];
            deme.cur_org_task_count = task_count;
            deme.cur_org_task_exe_count = task_exe_count;
            deme.cur_org_reaction_count = reaction_count;
        }
    }

    /// `cDeme::GetGenotypeIDs()`
    pub(crate) fn deme_genotype_ids(&self, d: usize) -> Vec<i32> {
        let mut ids = Vec::new();
        for &cell in &self.pop.demes[d].cell_ids {
            if !self.is_occupied(cell) {
                continue;
            }
            let id = self
                .systematics
                .genotype_of(&self.pop.membership[cell as usize])
                .and_then(|g| self.systematics.group_id(g))
                .unwrap_or(-1);
            ids.push(id);
        }
        ids
    }

    /// `cStats::DemePreReplication(source_deme, target_deme)`
    pub(crate) fn deme_pre_replication(&mut self, source: usize) {
        let deme = &self.pop.demes[source];
        let age = f64::from(deme.age);
        let births = f64::from(deme.cur_birth_count);
        let merit = deme.next_merit.get_double();
        let generation = f64::from(deme.generation);
        let density = deme.density();
        let treatable = deme.treatable;
        let r = &mut self.pop.deme_repl_stats;
        r.deme_num_repls += 1;
        r.total_deme_num_repls += 1;
        r.deme_gestation_time.add(age);
        r.deme_births.add(births);
        r.deme_merit.add(merit);
        r.deme_generation.add(generation);
        r.deme_density.add(density);
        if treatable {
            r.deme_num_repls_treatable += 1;
            r.deme_gestation_time_treatable.add(age);
            r.deme_births_treatable.add(births);
            r.deme_merit_treatable.add(merit);
            r.deme_generation_treatable.add(generation);
            r.deme_density_treatable.add(density);
        } else {
            r.deme_num_repls_untreatable += 1;
            r.deme_gestation_time_untreatable.add(age);
            r.deme_births_untreatable.add(births);
            r.deme_merit_untreatable.add(merit);
            r.deme_generation_untreatable.add(generation);
            r.deme_density_untreatable.add(density);
        }

        // Track the number of mutations that have occured to the germline
        // as the result of damage resulting from performing metabolic work.
        // (`GetGermlineNumPercent().first` is a count, so this always runs.)
        let (count, percent) = self.germline_num_percent(source);
        let r = &mut self.pop.deme_repl_stats;
        r.ave_germ_size.push_back(count);
        r.ave_germ_percent.push_back(percent);
        let (a, v) = self.ave_var_mut(source, true);
        self.pop.deme_repl_stats.ave_germ_mut.push_back(a);
        self.pop.deme_repl_stats.var_germ_mut.push_back(v);
        let (a, v) = self.ave_var_mut(source, false);
        self.pop.deme_repl_stats.ave_soma_mut.push_back(a);
        self.pop.deme_repl_stats.var_soma_mut.push_back(v);
        let (a, v) = self.ave_var_work_load(source, true);
        self.pop.deme_repl_stats.ave_germ_work.push_back(a);
        self.pop.deme_repl_stats.var_germ_work.push_back(v);
        let (a, v) = self.ave_var_work_load(source, false);
        self.pop.deme_repl_stats.ave_soma_work.push_back(a);
        self.pop.deme_repl_stats.var_soma_work.push_back(v);
    }

    /// `cDeme::GetGermlineNumPercent()`
    fn germline_num_percent(&self, d: usize) -> (f64, f64) {
        let mut count = 0.0;
        let mut total_count = 0.0;
        for &cell in &self.pop.demes[d].cell_ids {
            if let Some(s) = self.org_state(cell) {
                if s.phenotype.is_germ_cell {
                    count += 1.0;
                }
                total_count += 1.0;
            }
        }
        (count, (count / total_count) * 100.0)
    }

    /// `cDeme::GetAveVarGermMut()` (`germ`) and `GetAveVarSomaMut()`.
    /// Both first make the organism in the deme's first cell join the
    /// germline unless `DEMES_ORGANISM_SELECTION` is 7 or 8.
    fn ave_var_mut(&mut self, d: usize, germ: bool) -> (f64, f64) {
        let selection = self.cfg.demes_organism_selection;
        if selection != 7 && selection != 8 {
            let first = self.pop.demes[d].cell_id(0);
            if let Some(s) = self.org_state_mut(first) {
                s.phenotype.is_germ_cell = true;
            }
        }
        let mut mut_count = DoubleSum::new();
        for &cell in &self.pop.demes[d].cell_ids {
            if let Some(s) = self.org_state(cell) {
                if s.phenotype.is_germ_cell == germ {
                    mut_count.add(f64::from(s.num_point_mut));
                }
            }
        }
        (mut_count.average(), mut_count.variance())
    }

    /// `cDeme::GetAveVarGermWorkLoad()` (`germ`) and
    /// `GetAveVarSomaWorkLoad()`.
    fn ave_var_work_load(&self, d: usize, germ: bool) -> (f64, f64) {
        let slope = self.cfg.inst_point_mut_slope;
        let mut work_load = DoubleSum::new();
        for &cell in &self.pop.demes[d].cell_ids {
            let Some(s) = self.org_state(cell) else {
                continue;
            };
            if s.phenotype.is_germ_cell != germ {
                continue;
            }
            let curr_react = s.phenotype.cumulative_reaction_count(&self.cfg);
            let mut cur_org_w = 0.0;
            for (j, &c) in curr_react.iter().enumerate() {
                let mut weight = 1.0;
                // weight each reaction according to.
                if slope > 0.0 {
                    weight = slope * j as f64;
                }
                // The first task never has any work associated with it.
                if j > 0 {
                    cur_org_w += weight * f64::from(c);
                }
            }
            work_load.add(cur_org_w);
        }
        (work_load.average(), work_load.variance())
    }
}
