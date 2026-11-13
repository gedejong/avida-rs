//! Deme competition and the multi-level selection actions around it, at
//! 47f13dadb:
//!
//! * cPopulation.cc: both `CompeteDemes` overloads (by competition type,
//!   and by a vector of deme fitnesses with fitness-proportional or
//!   tournament selection), `sample_without_replacement`, `InjectClone`;
//! * PopulationActions.cc: `cActionCompeteDemes`,
//!   `cActionDemeReactionDiversity` (a `cAbstractCompeteDemes`),
//!   `cAssignRandomCellData`;
//! * PrintActions.cc: `cActionPrintEditDistance`;
//! * cStats.cc: `PrintWinningDeme` and `PrintCellData`, which read the
//!   population; `cPopulationCell::SetCellData` from an action.
//!
//! The other `cAbstractCompeteDemes` / `cAbstractMonitoringCompeteDemes`
//! fitness functions are not ported (their actions are unsupported).

use std::collections::{BTreeMap, BTreeSet};

use avida_config::string_util::{as_int, pop_word};
use avida_genome::genome_util::libcxx_random_shuffle;
use avida_genome::InstructionSequence;
use avida_hardware::organism::{transmission, OrgSource, OrgState, Organism};
use avida_systematics::unit::{GroupMembership, UnitInfo};

use crate::pop_orgs::systematics_source;
use crate::population::position_offspring as po;
use crate::stat_tools::DoubleSum;
use crate::world::World;

/// `Avida::INVALID_CONFIG`
const INVALID_CONFIG: i32 = 2;

/// `SELECTION_TYPE_PROPORTIONAL`, `SELECTION_TYPE_TOURNAMENT`
/// (avida/core/Definitions.h)
const SELECTION_TYPE_PROPORTIONAL: i32 = 0;
const SELECTION_TYPE_TOURNAMENT: i32 = 1;

/// What `cPopulation::InjectClone` reads from the original organism.
pub(crate) struct CloneOrigin {
    pub genome: avida_genome::Genome,
    pub generation: i32,
    pub phenotype: avida_phenotype::Phenotype,
    pub mut_rates: avida_environment::MutationRates,
    pub parent_group: i32,
    pub parent_teacher: bool,
    pub parent_ft: i32,
    /// The original organism, whose avatar `PlaceAvatar` reads.
    pub uid: avida_hardware::organism::OrgUid,
}

impl CloneOrigin {
    /// The data of `orig_org`.
    pub(crate) fn of(s: &avida_hardware::organism::OrgState) -> Self {
        CloneOrigin {
            genome: s.genome().clone(),
            generation: s.phenotype.generation,
            phenotype: s.phenotype.clone(),
            mut_rates: s.mut_rates,
            parent_group: s.parent_group,
            parent_teacher: s.parent_teacher,
            parent_ft: s.parent_ft,
            uid: s.uid,
        }
    }
}

/// The action names of this module (the registered spellings).
pub const COMPETE_ACTIONS: &[&str] = &[
    "CompeteDemes",
    "DemeReactionDiversity",
    "AssignRandomCellData",
    "PrintEditDistance",
];

/// State the competition actions keep between calls.
#[derive(Debug, Default)]
pub struct CompeteState {
    /// `cAssignRandomCellData::deme_to_id` (a class static): deme id ->
    /// every cell data value assigned in that deme.
    pub deme_to_id: BTreeMap<i32, BTreeSet<i32>>,
}

/// A parsed competition action. Field names follow the C++ members.
#[derive(Debug, Clone, PartialEq)]
pub enum CompeteAction {
    /// `cActionCompeteDemes`
    CompeteDemes { comp_type: i32 },
    /// `cActionDemeReactionDiversity`
    DemeReactionDiversity { uniq_only: bool },
    /// `cAssignRandomCellData`
    AssignRandomCellData { num_cells: i32 },
    /// `cActionPrintEditDistance`. `sample_size` is an `unsigned int`
    /// whose default -1 is `UINT_MAX`.
    PrintEditDistance { sample_size: u32, filename: String },
}

/// `if (largs.GetSize()) x = largs.PopWord()...`
fn next_word(args: &mut String) -> Option<String> {
    if args.is_empty() {
        None
    } else {
        Some(pop_word(args))
    }
}

/// The constructor of the competition action `name` applied to `args`;
/// `None` for a name not in [`COMPETE_ACTIONS`].
pub fn parse(name: &str, args: &str) -> Option<CompeteAction> {
    let mut a = args.to_string();
    Some(match name {
        "CompeteDemes" => CompeteAction::CompeteDemes {
            comp_type: next_word(&mut a).map_or(1, |w| as_int(&w)),
        },
        "DemeReactionDiversity" => CompeteAction::DemeReactionDiversity {
            uniq_only: next_word(&mut a).is_some_and(|w| as_int(&w) != 0),
        },
        "AssignRandomCellData" => CompeteAction::AssignRandomCellData {
            num_cells: next_word(&mut a).map_or(0, |w| as_int(&w)),
        },
        "PrintEditDistance" => {
            let sample_size = next_word(&mut a).map_or(u32::MAX, |w| as_int(&w) as u32);
            let filename = next_word(&mut a).unwrap_or_else(|| "edit_distance.dat".to_string());
            CompeteAction::PrintEditDistance {
                sample_size,
                filename,
            }
        }
        _ => return None,
    })
}

/// Which demes `ctx.GetRandom().GetDouble(total)` picks for the next
/// generation in the type-based `CompeteDemes`. QUIRK: a draw that no
/// running total exceeds leaves its slot at 0 (the reference build's
/// `AvidaArray` value-initialises; the original `Apto::Array<int>` left
/// it indeterminate).
fn pick_demes_by_type(rng: &mut avida_apto::AvidaRng, fitness: &[f64], total: f64) -> Vec<usize> {
    let n = fitness.len();
    let mut new_demes = vec![0usize; n];
    for slot in new_demes.iter_mut() {
        let birth_choice = rng.get_double_max(total);
        let mut test_total = 0.0;
        for (test_deme, &f) in fitness.iter().enumerate() {
            test_total += f;
            if birth_choice < test_total {
                *slot = test_deme;
                break;
            }
        }
    }
    new_demes
}

/// The rank-based deme fitness of `CompeteDemes` types 4 and 6:
/// `2^-rank`, where a deme's rank is 1 plus the number of demes fitter than
/// it (computed by repeated halving).
fn rank_fitness(fitness: &[f64]) -> Vec<f64> {
    fitness
        .iter()
        .map(|&f| {
            let deme_rank = 1 + fitness.iter().filter(|&&t| f < t).count();
            let mut v = 1.0;
            for _ in 0..deme_rank {
                v /= 2.0;
            }
            v
        })
        .collect()
}

/// `sample_without_replacement(first, last, ofirst, olast, rng)`: fill
/// `out` with distinct elements of `input`, drawn by shuffling an index
/// map with libc++ `std::random_shuffle` and popping from its back. With
/// `out` at least as long as `input`, `input` is copied to its front.
fn sample_without_replacement(input: &[i32], out: &mut [i32], rng: &mut avida_apto::AvidaRng) {
    if out.len() >= input.len() {
        out[..input.len()].copy_from_slice(input);
        return;
    }
    let mut rmap: Vec<usize> = (0..input.len()).collect();
    libcxx_random_shuffle(&mut rmap, rng);
    for o in out.iter_mut() {
        let Some(k) = rmap.pop() else { break };
        *o = input[k];
    }
}

/// The fitness `cActionDemeReactionDiversity::Fitness` gives a deme whose
/// occupants have the current reaction counts `orgs` (`deme_size` cells).
fn reaction_diversity_fitness(orgs: &[&[i32]], deme_size: i32, uniq_only: bool) -> f64 {
    let mut uniq_reactions = BTreeSet::new();
    let mut performed = 0;
    for reactions in orgs {
        let mut performed_rx = false;
        for (j, &r) in reactions.iter().enumerate() {
            if r > 0 {
                uniq_reactions.insert(j);
                performed_rx = true;
            }
        }
        if performed_rx {
            performed += 1;
        }
    }
    if uniq_only {
        uniq_reactions.len() as f64 + 1.0
    } else if performed < deme_size {
        f64::from(performed) + 1.0
    } else {
        uniq_reactions.len() as f64 + f64::from(deme_size) + 1.0
    }
}

impl World {
    /// `Process(ctx)` of a competition action.
    pub(crate) fn run_compete_action(&mut self, action: &CompeteAction) -> Result<(), String> {
        match action {
            CompeteAction::CompeteDemes { comp_type } => self.compete_demes_by_type(*comp_type),
            CompeteAction::DemeReactionDiversity { uniq_only } => {
                // `cAbstractCompeteDemes::Process`
                let mut fitness = Vec::with_capacity(self.pop.demes.len());
                for d in 0..self.pop.demes.len() {
                    let deme = &self.pop.demes[d];
                    let orgs: Vec<&[i32]> = deme
                        .cell_ids
                        .iter()
                        .filter_map(|&c| self.pop.orgs[c as usize].as_ref())
                        .map(|o| o.state.phenotype.cur_reaction_count.as_slice())
                        .collect();
                    fitness.push(reaction_diversity_fitness(&orgs, deme.size(), *uniq_only));
                }
                self.compete_demes(&fitness)
            }
            CompeteAction::AssignRandomCellData { num_cells } => {
                self.assign_random_cell_data(*num_cells);
                Ok(())
            }
            CompeteAction::PrintEditDistance {
                sample_size,
                filename,
            } => self.print_edit_distance(*sample_size, filename),
        }
    }

    /// `cPopulation::CompeteDemes(ctx, competition_type)`: pick the next
    /// generation of demes by a population-level fitness, copy winners over
    /// losers with `InjectClone`, re-inject the rest into themselves and
    /// reset every deme.
    pub fn compete_demes_by_type(&mut self, competition_type: i32) -> Result<(), String> {
        let num_demes = self.pop.demes.len();
        let mut total_fitness = 0.0;
        let mut deme_fitness = vec![0.0f64; num_demes];

        // Each deme's average of a phenotype value over its occupants.
        let deme_averages = |w: &World, f: fn(&avida_phenotype::Phenotype) -> f64| {
            w.pop
                .demes
                .iter()
                .map(|deme| {
                    let mut sum = DoubleSum::new();
                    for &cell in &deme.cell_ids {
                        if let Some(org) = w.pop.orgs[cell as usize].as_ref() {
                            sum.add(f(&org.state.phenotype));
                        }
                    }
                    sum.average()
                })
                .collect::<Vec<f64>>()
        };
        // The running total of the fitnesses, deme by deme.
        let sum = |fit: &[f64]| fit.iter().fold(0.0, |total, &f| total + f);

        match competition_type {
            // deme fitness = 1;
            0 => {
                total_fitness = num_demes as f64;
                deme_fitness.fill(1.0);
            }
            // deme fitness = number of births
            1 => {
                for (f, deme) in deme_fitness.iter_mut().zip(&self.pop.demes) {
                    *f = f64::from(deme.cur_birth_count);
                }
                total_fitness = sum(&deme_fitness);
            }
            // deme fitness = average organism fitness at the current update
            2 => {
                deme_fitness = deme_averages(self, |p| p.fitness);
                total_fitness = sum(&deme_fitness);
            }
            // deme fitness = average mutation rate at the current update
            3 => {
                deme_fitness = deme_averages(self, |p| 1.0 / p.div_type);
                total_fitness = sum(&deme_fitness);
            }
            // deme fitness = 2^(-deme fitness rank)
            4 => {
                deme_fitness = rank_fitness(&deme_averages(self, |p| p.fitness));
                total_fitness = sum(&deme_fitness);
            }
            // deme fitness = average organism life fitness at the current
            // update
            5 => {
                deme_fitness = deme_averages(self, |p| p.life_fitness);
                total_fitness = sum(&deme_fitness);
            }
            // deme fitness = 2^(-deme life fitness rank)
            6 => {
                deme_fitness = rank_fitness(&deme_averages(self, |p| p.life_fitness));
                total_fitness = sum(&deme_fitness);
            }
            // Any other type leaves every fitness and the total at 0.
            _ => {}
        }

        // Pick which demes should be in the next generation.
        let new_demes = pick_demes_by_type(&mut self.rng, &deme_fitness, total_fitness);

        // Track how many of each deme we should have.
        let mut deme_count = vec![0; num_demes];
        for &d in &new_demes {
            deme_count[d] += 1;
        }
        let mut is_init = vec![false; num_demes];

        // Copy demes until all deme counts are 1. Find the next deme to
        // copy; stop if we didn't find another deme to copy.
        while let Some(from_deme_id) = (0..num_demes).find(|&d| deme_count[d] > 1) {
            // A count above 1 leaves some deme at 0.
            let to_deme_id = (0..num_demes)
                .find(|&d| deme_count[d] == 0)
                .ok_or("CompeteDemes: no deme to copy into (C++: out-of-bounds deme)")?;

            // We now have both a from and a to deme....
            deme_count[from_deme_id] -= 1;
            deme_count[to_deme_id] += 1;

            // Do the actual copy!
            for i in 0..self.pop.demes[from_deme_id].size() {
                let from_cell_id = self.pop.demes[from_deme_id].cell_id(i);
                let to_cell_id = self.pop.demes[to_deme_id].cell_id(i);
                if self.is_occupied(from_cell_id) {
                    let src = OrgSource::new(transmission::DUPLICATION, "", false);
                    self.inject_clone(to_cell_id, from_cell_id, src)?;
                }
            }
            is_init[to_deme_id] = true;
        }

        // Now re-inject all remaining demes into themselves to reset them.
        for (deme_id, &init) in is_init.iter().enumerate() {
            if init {
                continue;
            }
            for i in 0..self.pop.demes[deme_id].size() {
                let cur_cell_id = self.pop.demes[deme_id].cell_id(i);
                let Some(org) = self.pop.orgs[cur_cell_id as usize].as_ref() else {
                    continue;
                };
                let src = org.state.src.clone();
                self.inject_clone(cur_cell_id, cur_cell_id, src)?;
            }
        }

        // Reset all deme stats to zero. QUIRK: the C++ passes the deme's
        // generation as `Reset(ctx, resetResources)`, so resources are
        // reset unless the generation is 0 (the comment there says
        // "increase deme generation by 1"; nothing does).
        let World {
            pop,
            cfg,
            rng,
            res_env,
            ..
        } = self;
        for deme in pop.demes.iter_mut() {
            let reset_resources = deme.generation != 0;
            deme.reset(cfg, rng, res_env, reset_resources, 0.0);
        }
        Ok(())
    }

    /// `cPopulation::CompeteDemes(calculated_fitness, ctx)`
    pub fn compete_demes(&mut self, calculated_fitness: &[f64]) -> Result<(), String> {
        // it's possible that we'll be changing the fitness values of some
        // demes, so make a copy:
        let mut fitness = calculated_fitness.to_vec();
        let num_demes = self.pop.demes.len();
        if fitness.len() != num_demes {
            return Err("CompeteDemes: one fitness per deme (C++: assert)".to_string());
        }

        // To prevent sterile demes from replicating, we're going to replace
        // the fitness of all sterile demes with 0.
        if self.cfg.demes_prevent_sterile != 0 {
            for (f, deme) in fitness.iter_mut().zip(&self.pop.demes) {
                if deme.cur_birth_count == 0 {
                    *f = 0.0;
                }
            }
        }

        // Stat-tracking:
        self.stats.compete_demes(&fitness);

        // This is to facilitate testing. Obviously we can't do competition
        // if there's only one deme, but we do want the stat-tracking.
        if fitness.len() == 1 {
            return Ok(());
        }

        // to facilitate control runs, sometimes we want to know what the
        // fitness values are, but we don't want competition to depend on
        // them.
        if self.cfg.demes_override_fitness != 0 {
            fitness.fill(1.0);
        }

        // Number of demes (at index) which should wind up in the next
        // generation.
        let mut deme_counts = vec![0u32; num_demes];
        match self.cfg.demes_competition_style {
            SELECTION_TYPE_PROPORTIONAL => {
                // Fitness-proportional selection.
                let total_fitness: f64 = fitness.iter().fold(0.0, |a, &f| a + f);
                // Sum up the fitnesses until we reach or exceed the target
                // fitness.
                for _ in 0..num_demes {
                    let mut running_sum = 0.0;
                    let target_sum = self.rng.get_double_max(total_fitness);
                    for (j, &f) in fitness.iter().enumerate() {
                        running_sum += f;
                        if running_sum >= target_sum {
                            // j'th deme will be replicated.
                            deme_counts[j] += 1;
                            break;
                        }
                    }
                }
            }
            SELECTION_TYPE_TOURNAMENT => {
                // construct a list of all possible deme ids that could
                // participate in a tournament, pruning out sterile and empty
                // demes:
                let prevent_sterile = self.cfg.demes_prevent_sterile != 0;
                let deme_ids: Vec<i32> = (0..num_demes)
                    .filter(|&i| {
                        let d = &self.pop.demes[i];
                        d.cur_org_count > 0 && (!prevent_sterile || d.cur_birth_count > 0)
                    })
                    .map(|i| i as i32)
                    .collect();

                let tournament_size = self.cfg.demes_tournament_size;
                if tournament_size > deme_ids.len() as i32 {
                    let e = "Number of demes available to participate in a tournament < the deme \
                             tournament size.";
                    self.abort_run(&format!("error: {e}\n"), INVALID_CONFIG, e);
                    return Ok(());
                }

                // Run the tournaments.
                for _ in 0..self.cfg.num_demes {
                    // Which demes are in this tournament?
                    let mut tournament = vec![0i32; tournament_size.max(0) as usize];
                    sample_without_replacement(&deme_ids, &mut tournament, &mut self.rng);

                    // If no deme actually won, meaning no one had fitness
                    // greater than 0.0, then the winner is selected at
                    // random from the tournament.
                    let pick = self.rng.get_int(tournament.len() as i32) as usize;
                    let mut winner = (
                        *tournament
                            .get(pick)
                            .ok_or("CompeteDemes: empty tournament (C++: out-of-bounds read)")?,
                        0.0f64,
                    );
                    for &j in &tournament {
                        if fitness[j as usize] > winner.1 {
                            winner = (j, fitness[j as usize]);
                        }
                    }

                    // We have a winner!  Increment his replication count.
                    deme_counts[winner.0 as usize] += 1;
                }
            }
            // `assert(false)` is compiled out: no deme is counted.
            _ => {}
        }

        // Housekeeping: re-inject demes with count of 1 back into self
        // (energy-related).
        let once: Vec<usize> = (0..num_demes).filter(|&i| deme_counts[i] == 1).collect();
        for i in once {
            self.replace_deme(i, i);
            if self.fatal_message().is_some() {
                return Ok(());
            }
        }

        // Find demes with a count greater than one, and insert them into
        // demes with a count of zero.
        // All done once we loop through the whole list of counts and don't
        // find any > 1.
        while let Some(source_id) = (0..num_demes).find(|&i| deme_counts[i] > 1) {
            deme_counts[source_id] -= 1;
            let target_id = (0..num_demes)
                .find(|&i| deme_counts[i] == 0)
                .ok_or("CompeteDemes: no deme to replace (C++: assert)")?;
            deme_counts[target_id] += 1;

            // Replace the target with a copy of the source:
            self.replace_deme(source_id, target_id);
            if self.fatal_message().is_some() {
                return Ok(());
            }
        }
        Ok(())
    }

    /// `cPopulation::InjectClone(cell_id, orig_org, src)` with the original
    /// organism the occupant of `orig_cell` (which may be `cell_id`).
    pub fn inject_clone(
        &mut self,
        cell_id: i32,
        orig_cell: i32,
        src: OrgSource,
    ) -> Result<(), String> {
        if cell_id < 0 || cell_id as usize >= self.pop.cells.len() {
            return Err("InjectClone into nonexistent cell (C++: assert)".to_string());
        }
        if self.cfg.use_avatars != 0 && orig_cell == cell_id && self.is_occupied(cell_id) {
            return Err(
                "unsupported: InjectClone over its own original with USE_AVATARS \
                        (C++: cPopulation::PlaceAvatar reads the original organism after \
                        ActivateOrganism has deleted it; the reference binary crashes \
                        with SIGSEGV in ResetDemes)"
                    .to_string(),
            );
        }
        let orig = self.pop.orgs[orig_cell as usize]
            .as_ref()
            .ok_or("InjectClone of an empty cell (C++: NULL dereference)")?;
        let origin = CloneOrigin::of(&orig.state);
        self.inject_clone_from(cell_id, origin, src)
    }

    /// The rest of `cPopulation::InjectClone(cell_id, orig_org, src)` once
    /// the original's data is in hand (the original may be the organism
    /// being run, which is not in its cell).
    pub(crate) fn inject_clone_from(
        &mut self,
        cell_id: i32,
        origin: CloneOrigin,
        src: OrgSource,
    ) -> Result<(), String> {
        let c = cell_id as usize;
        let CloneOrigin {
            genome,
            generation: orig_generation,
            phenotype: orig_phenotype,
            mut_rates: orig_mut_rates,
            parent_group: orig_parent_group,
            parent_teacher: orig_parent_teacher,
            parent_ft: orig_parent_ft,
            uid: orig_uid,
        } = origin;

        let mut org = Organism::new(self, &genome, orig_generation, src)?;

        // Classify the new organism
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

        // Setup the phenotype...
        let update = self.stats.update();
        org.state
            .phenotype
            .setup_clone(&self.cfg, &orig_phenotype, update, &mut self.rng);

        // Prep the cell..
        if self.cfg.birth_method == po::FULL_SOUP_ELDEST && self.is_occupied(cell_id) {
            // Have to manually take this cell out of the reaper Queue.
            if let Some(pos) = self.pop.reaper_queue.iter().position(|&x| x == cell_id) {
                self.pop.reaper_queue.remove(pos);
            }
        }

        // Setup the mutation rate based on the population cell...
        if self.cfg.mut_rate_source == 1 {
            // Update the mutation rates of each child from the environment....
            org.state.mut_rates.copy_from(&self.pop.cells[c].mut_rates);
        } else {
            // Update the mutation rates of each child from its parent.
            org.state.mut_rates.copy_from(&orig_mut_rates);
        }

        // since this is a clone, we want some of the parent data from the
        // genome source (the source is not the parent, the source's parent
        // is)
        if self.cfg.use_form_groups != 0 {
            org.state.parent_group = orig_parent_group;
        }
        if orig_parent_teacher {
            org.state.parent_teacher = true;
        }
        org.state.parent_ft = orig_parent_ft;

        // Activate the organism in the population...
        let org_survived = self.activate_organism(org, membership, cell_id, true, true, None);
        // only assign an avatar cell if the org lived through birth
        self.place_offspring_avatar(cell_id, org_survived, orig_uid);
        if let Some(f) = self.fatal.borrow_mut().take() {
            return Err(f);
        }
        Ok(())
    }

    /// `cAssignRandomCellData::Process`
    pub(crate) fn assign_random_cell_data(&mut self, num_cells: i32) {
        let update = self.stats.update();
        for d in 0..self.pop.demes.len() {
            let deme_id = self.pop.demes[d].id;
            let size = self.pop.demes[d].size();
            // `zero_cell_data(deme)`
            self.deme_compete
                .deme_to_id
                .entry(deme_id)
                .or_default()
                .clear();
            for j in 0..size {
                let cell = self.pop.demes[d].cell_id(j);
                self.set_cell_data(cell, 0, -1, update);
            }
            if num_cells == 0 || num_cells >= size {
                // Assign random data to each cell:
                for j in 0..size {
                    let data = self.rng.get_int(i32::MAX);
                    let cell = self.pop.demes[d].cell_id(j);
                    self.set_cell_data(cell, data, -1, update);
                    // Save that data by deme in the map:
                    self.deme_compete
                        .deme_to_id
                        .entry(deme_id)
                        .or_default()
                        .insert(data);
                }
            } else {
                // Assign random data to exactly num_cells cells, with
                // replacement:
                for _ in 0..num_cells {
                    let pos = self.rng.get_int(size);
                    let data = self.rng.get_int(i32::MAX);
                    let cell = self.pop.demes[d].cell_id(pos);
                    self.set_cell_data(cell, data, -1, update);
                    self.deme_compete
                        .deme_to_id
                        .entry(deme_id)
                        .or_default()
                        .insert(data);
                }
            }
        }
    }

    /// `cPopulationCell::SetCellData(data, org_id)` from outside an
    /// organism's step: the occupant's opinion (if it has one) becomes the
    /// territory and its forage target the forager.
    pub(crate) fn set_cell_data(&mut self, cell: i32, data: i32, org_id: i32, update: i32) {
        let c = cell as usize;
        let occupant = self.pop.orgs[c].as_mut().map(|o| {
            let opinion = if o.state.has_opinion() {
                Some(o.state.get_opinion().0)
            } else {
                None
            };
            (opinion, o.state.forage_target)
        });
        self.pop.cells[c].set_cell_data(data, org_id, update, occupant);
    }

    /// [`World::set_cell_data`] on the cell of the organism being run,
    /// which is out of its cell (`Population::process_step`): `org` is its
    /// state, as the hardware holds it. For `SetCellData`, `SetAVCellData`
    /// and `KillCellEvent` on the running organism's own cell once the
    /// `OrgInterface` passes the organism to them.
    pub fn set_running_cell_data(
        &mut self,
        cell: i32,
        data: i32,
        org_id: i32,
        update: i32,
        org: &mut OrgState,
    ) {
        let opinion = if org.has_opinion() {
            Some(org.get_opinion().0)
        } else {
            None
        };
        let occupant = Some((opinion, org.forage_target));
        self.pop.cells[cell as usize].set_cell_data(data, org_id, update, occupant);
    }

    /// `cStats::PrintCellData(filename)`: every cell's deme and data.
    pub(crate) fn print_cell_data(&mut self, filename: &str) -> Result<(), String> {
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Cell data per udpate.");
        df.write_time_stamp();
        for cell in &self.pop.cells {
            df.write(update, "Update [update]");
            df.write(cell.cell_id, "Global cell ID [globalid]");
            df.write(cell.deme_id, "Deme ID for cell [demeid]");
            df.write(cell.cell_data.contents, "Cell data [data]");
            df.endl();
        }
        Ok(())
    }

    /// `cActionPrintEditDistance::Process`: the mean edit distance between
    /// random pairs of organisms within each deme and across the
    /// population.
    pub(crate) fn print_edit_distance(
        &mut self,
        sample_size: u32,
        filename: &str,
    ) -> Result<(), String> {
        // within deme edit distance:
        let mut within_deme_ed = DoubleSum::new();
        for d in 0..self.pop.demes.len() {
            let cells: Vec<i32> = self.pop.demes[d]
                .cell_ids
                .iter()
                .copied()
                .filter(|&c| self.pop.orgs[c as usize].is_some())
                .collect();
            let ed = self.average_edit_distance(cells, sample_size);
            within_deme_ed.add(ed);
        }

        // among deme edit distance:
        let cells: Vec<i32> = (0..self.pop.cells.len() as i32)
            .filter(|&c| self.pop.orgs[c as usize].is_some())
            .collect();
        let among_deme_ed = self.average_edit_distance(cells, sample_size);

        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write(update, "Update [update]");
        df.write(within_deme_ed.average(), "Mean deme edit distance [deme]");
        df.write(among_deme_ed, "Mean population edit distance [population]");
        df.endl();
        Ok(())
    }

    /// `average_edit_distance(organisms, ctx)` over the occupants of
    /// `cells`.
    fn average_edit_distance(&mut self, mut cells: Vec<i32>, sample_size: u32) -> f64 {
        libcxx_random_shuffle(&mut cells, &mut self.rng);
        if cells.len() % 2 == 1 {
            cells.pop();
        }
        let mut sample_pairs = (cells.len() / 2) as u32;
        if sample_size > 0 {
            sample_pairs = sample_size.min(sample_pairs);
        }
        let mut edit_distance = DoubleSum::new();
        let seq = |w: &World, c: i32| -> InstructionSequence {
            w.pop.orgs[c as usize]
                .as_ref()
                .map(|o| o.state.genome().representation().clone())
                .unwrap_or_default()
        };
        for _ in 0..sample_pairs {
            let (Some(a), Some(b)) = (cells.pop(), cells.pop()) else {
                break;
            };
            let (a_seq, b_seq) = (seq(self, a), seq(self, b));
            edit_distance.add(f64::from(InstructionSequence::find_edit_distance(
                &a_seq, &b_seq,
            )));
        }
        edit_distance.average()
    }

    /// `cStats::PrintWinningDeme(filename)`: the genotypes of the deme with
    /// the highest fitness of the last competition (the first such deme).
    pub(crate) fn print_winning_deme(&mut self, filename: &str) -> Result<(), String> {
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Genotype IDs of the constituent organisms within each deme.");
        df.write_time_stamp();
        df.write_column_desc("Update [update]", "");
        df.write_column_desc("Deme id [demeid]", "");
        df.write_column_desc("Deme fitness [fitness]", "");
        df.write_column_desc("Number of unique genomes in deme [uniq]", "");
        df.write_column_desc("Genome ID [genomeids]", "");
        df.flush_comments();

        let mut max_element = (0usize, 0.0f64);
        let mut found_max = false;
        for (i, &f) in self.stats.m_deme_fitness.iter().enumerate() {
            if f > max_element.1 {
                max_element = (i, f);
                found_max = true;
            }
        }
        if !found_max {
            return Ok(());
        }

        let genotypes = self.deme_genotype_ids(max_element.0);
        let uniq: BTreeSet<i32> = genotypes.iter().copied().collect();
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_anonymous(update);
        df.write_anonymous(max_element.0 as i32);
        df.write_anonymous(max_element.1);
        df.write_anonymous(uniq.len() as i32);
        for g in genotypes {
            df.write_anonymous(g);
        }
        df.endl();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use avida_apto::AvidaRng;

    #[test]
    fn parses_defaults_and_arguments() {
        assert_eq!(
            parse("CompeteDemes", ""),
            Some(CompeteAction::CompeteDemes { comp_type: 1 })
        );
        assert_eq!(
            parse("DemeReactionDiversity", "1"),
            Some(CompeteAction::DemeReactionDiversity { uniq_only: true })
        );
        assert_eq!(
            parse("AssignRandomCellData", ""),
            Some(CompeteAction::AssignRandomCellData { num_cells: 0 })
        );
        assert_eq!(
            parse("PrintEditDistance", ""),
            Some(CompeteAction::PrintEditDistance {
                sample_size: u32::MAX,
                filename: "edit_distance.dat".to_string()
            })
        );
        assert_eq!(
            parse("PrintEditDistance", "-2 ed.dat"),
            Some(CompeteAction::PrintEditDistance {
                sample_size: u32::MAX - 1,
                filename: "ed.dat".to_string()
            })
        );
        assert_eq!(parse("Exit", ""), None);
    }

    #[test]
    fn reaction_diversity_rewards_unique_reactions() {
        let a = [1, 0, 2];
        let b = [0, 0, 0];
        let c = [0, 3, 0];
        // Unique reactions only.
        assert_eq!(reaction_diversity_fitness(&[&a, &b, &c], 4, true), 4.0);
        // Not every cell performed: performed + 1.
        assert_eq!(reaction_diversity_fitness(&[&a, &b, &c], 4, false), 3.0);
        // Every cell performed: unique + size + 1.
        assert_eq!(reaction_diversity_fitness(&[&a, &c], 2, false), 6.0);
    }

    #[test]
    fn sampling_copies_short_inputs_and_draws_distinct_otherwise() {
        let mut rng = AvidaRng::new(1);
        let mut out = vec![0; 4];
        sample_without_replacement(&[7, 8], &mut out, &mut rng);
        assert_eq!(out, vec![7, 8, 0, 0]);
        let input: Vec<i32> = (10..20).collect();
        let mut out = vec![0; 5];
        sample_without_replacement(&input, &mut out, &mut rng);
        let set: BTreeSet<i32> = out.iter().copied().collect();
        assert_eq!(set.len(), 5);
        assert!(out.iter().all(|v| input.contains(v)));
    }

    #[test]
    fn type_based_picks_fall_back_to_deme_zero() {
        let mut rng = AvidaRng::new(3);
        assert_eq!(pick_demes_by_type(&mut rng, &[0.0, 0.0], 0.0), vec![0, 0]);
        let picks = pick_demes_by_type(&mut rng, &[0.0, 1.0], 1.0);
        assert_eq!(picks, vec![1, 1]);
    }

    /// `SetCellData` on the running organism's cell reads the opinion and
    /// forage target from the state the caller passes.
    #[test]
    fn running_cell_data_reads_the_running_organism() {
        use avida_config::Config;
        use avida_genome::Genome;
        use std::path::Path;

        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../avida-core/tests/demes_germline/config");
        let dir = std::env::temp_dir().join(format!("avida-cell-data-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for e in std::fs::read_dir(&src).unwrap() {
            let e = e.unwrap();
            std::fs::copy(e.path(), dir.join(e.file_name())).unwrap();
        }
        let wd = dir.to_string_lossy().into_owned();
        let mut cfg = Config::default();
        assert!(cfg.load("avida.cfg", &wd, None, None, false));
        cfg.world_y = 20;
        cfg.num_demes = 10;
        let mut w = World::new(cfg, &wd, &[]).unwrap().0;
        w.stats.set_current_update(0);
        let is = w.hw.registry().shared_inst_set(0).unwrap();
        let g = Genome::with_inst_set(
            0,
            is.inst_set_name(),
            InstructionSequence::from_string("rucavutycl"),
        );
        let src = OrgSource::new(transmission::DUPLICATION, "", false);
        w.inject(&g, src, 5, -1.0, 0, 0.0).unwrap();
        // `ProcessStep` takes the organism out of its cell.
        let mut org = w.pop.orgs[5].take().unwrap();

        org.state.forage_target = 3;
        w.set_running_cell_data(5, 7, org.state.id, 4, &mut org.state);
        let d = &w.pop.cells[5].cell_data;
        assert_eq!((d.contents, d.org_id, d.update), (7, org.state.id, 4));
        assert_eq!((d.territory, d.forager), (-1, 3));

        org.state.set_opinion(&w.cfg, 4, 42).unwrap();
        w.set_running_cell_data(5, 8, org.state.id, 5, &mut org.state);
        let d = &w.pop.cells[5].cell_data;
        assert_eq!((d.contents, d.territory, d.forager), (8, 42, 3));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
