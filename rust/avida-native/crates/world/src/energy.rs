//! The population side of the energy model (`ENERGY_ENABLED`), from
//! avida-core/source/main at 47f13dadb: the energy branches of
//! `cPopulation::InjectGenome`, `ReplaceDeme` and `PrintDemeAllStats`,
//! `cDeme::GetCellEnergy` / `CalculateTotalEnergy`, and the helpers the
//! birth chamber (`cBirthChamber::StoreAsEntry`, `DoAsexBirth`,
//! `SubmitOffspring`) uses to hand energy from parent to offspring.
//!
//! `cPhenotype`'s energy arithmetic lives in `avida_phenotype`; the per
//! instruction energy cost and `Inst_Sleep` in the hardware crate.
//!
//! `FRAC_ENERGY_TRANSFER > 0`: the energy branches of
//! `cPopulationCell::InsertOrganism` (`UptakeCellEnergy`, through
//! `cDeme::GetAndClearCellEnergy` and `GiveBackCellEnergy`) and
//! `RemoveOrganism`
//! (`cDeme::GiveBackCellEnergy`), which [`World::cell_insert_energy`] and
//! [`World::cell_remove_energy`] port; their callers are the places that
//! move organisms in and out of cells (`ActivateOrganism`, `KillOrganism`,
//! `SwapCells`, `MixPopulation`).

use avida_config::Config;
use avida_hardware::organism::Organism;
use avida_phenotype::phenotype::{Phenotype, PhenotypeStats};
use avida_phenotype::Merit;

use crate::stat_tools::DoubleSum;
use crate::world::World;

/// The `cStats` energy-testament sums besides
/// `SumEnergyTestamentAcceptedByOrganisms` (which `cPhenotype` adds to).
#[derive(Debug, Clone, Default)]
pub struct EnergyStats {
    /// `SumEnergyTestamentToDemeOrganisms()`
    pub testament_to_deme_organisms: DoubleSum,
    /// `SumEnergyTestamentToFutureDeme()`
    pub testament_to_future_deme: DoubleSum,
    /// `SumEnergyTestamentToNeighborOrganisms()`
    pub testament_to_neighbor_organisms: DoubleSum,
    /// `SumEnergyTestamentAcceptedByDeme()`
    pub testament_accepted_by_deme: DoubleSum,
}

impl EnergyStats {
    /// `SumEnergyTestamentTo<target>().Add(amount)`
    pub(crate) fn add_testament(
        &mut self,
        target: avida_hardware::org_interface::TestamentTarget,
        amount: f64,
    ) {
        use avida_hardware::org_interface::TestamentTarget as T;
        match target {
            T::FutureDeme => self.testament_to_future_deme.add(amount),
            T::NeighborOrganisms => self.testament_to_neighbor_organisms.add(amount),
            T::DemeOrganisms => self.testament_to_deme_organisms.add(amount),
        }
    }
}

/// A `cStats` stand-in that drops what `cPhenotype` reports, for birth
/// chamber code run without a world (unit tests).
pub(crate) struct NoPhenotypeStats;

impl PhenotypeStats for NoPhenotypeStats {
    fn add_energy_testament_accepted(&mut self, _value: f64) {}
    fn age_task_event(&mut self, _org_id: i32, _task_id: usize, _org_age: i32) {}
    fn add_new_task_count(&mut self, _task_num: usize) {}
    fn add_other_task_counts(&mut self, _task_num: usize, _prev_tasks: i32, _cur_tasks: i32) {}
    fn add_new_reaction_count(&mut self, _reaction_num: usize) {}
    fn add_task_switch_time(&mut self, _t1: i32, _t2: i32, _time: i32) {}
}

/// `parent->GetPhenotype().ExtractParentEnergy()`. Its only failure,
/// `RefreshEnergy` with an unknown `APPLY_ENERGY_METHOD`, cannot happen
/// here: `ExtractParentEnergy` calls it only when the method is 0.
pub(crate) fn extract_parent_energy(
    ph: &mut Phenotype,
    cfg: &Config,
    stats: &mut dyn PhenotypeStats,
) -> f64 {
    ph.extract_parent_energy(cfg, stats).unwrap_or(0.0)
}

/// `child->GetPhenotype().SetEnergy(energy)` followed by
/// `ConvertEnergyToMerit(GetStoredEnergy())`, as the birth chamber sets up
/// an offspring's merit.
pub(crate) fn set_child_energy(org: &mut Organism, cfg: &Config, energy: f64) -> Merit {
    let ph = &mut org.state.phenotype;
    ph.set_energy(cfg, energy);
    Merit::new(ph.convert_energy_to_merit(cfg, ph.energy_store))
}

impl World {
    /// `phenotype.ConvertEnergyToMerit(phenotype.GetStoredEnergy())`: the
    /// merit `InjectGenome` and `LoadPopulation` give an injected organism
    /// under the energy model.
    pub(crate) fn injected_energy_merit(&self, ph: &Phenotype) -> Merit {
        Merit::new(ph.convert_energy_to_merit(&self.cfg, ph.energy_store))
    }

    /// `cDeme::GetCellEnergy(absolute_cell_id, ctx)`: the sum of the
    /// positive energy resources in the cell (the deme resources are brought
    /// up to date first).
    pub fn deme_cell_energy(&mut self, d: usize, absolute_cell_id: i32) -> f64 {
        let World {
            pop, rng, res_env, ..
        } = self;
        let deme = &mut pop.demes[d];
        let relative_cell_id = deme.relative_cell_id(absolute_cell_id);
        let mut cell_resources = deme
            .resource_count
            .get_cell_resources(relative_cell_id, rng, res_env)
            .to_vec();
        let mut total_energy = 0.0;
        // sum all energy resources (negating the copy keeps a resource
        // listed twice from counting twice)
        for &id in &deme.energy_res_ids {
            if let Some(v) = cell_resources.get_mut(id) {
                if *v > 0.0 {
                    total_energy += *v;
                    *v *= -1.0;
                }
            }
        }
        total_energy
    }

    /// `cDeme::GetAndClearCellEnergy(absolute_cell_id, ctx)` of deme `d`:
    /// the sum of the cell's positive energy resources, which are cleared.
    ///
    /// QUIRK: `ModifyCell` adds the whole (negated-energy) copy of the
    /// cell's resources, so every other resource of the cell is doubled; for
    /// a deme-global resource that is the whole deme's amount.
    fn deme_get_and_clear_cell_energy(&mut self, d: usize, absolute_cell_id: i32) -> f64 {
        let World {
            pop, rng, res_env, ..
        } = self;
        let deme = &mut pop.demes[d];
        let relative_cell_id = deme.relative_cell_id(absolute_cell_id);
        let mut cell_resources = deme
            .resource_count
            .get_cell_resources(relative_cell_id, rng, res_env)
            .to_vec();
        let mut total_energy = 0.0;
        // sum all energy resources
        for &id in &deme.energy_res_ids {
            if let Some(v) = cell_resources.get_mut(id) {
                if *v > 0.0 {
                    total_energy += *v;
                    *v *= -1.0;
                }
            }
        }
        // set energy resources to zero
        deme.resource_count
            .modify_cell(rng, res_env, &cell_resources, relative_cell_id);
        total_energy
    }

    /// `cDeme::GiveBackCellEnergy(absolute_cell_id, value, ctx)` of deme
    /// `d`: `value` spread evenly over the deme's energy resources in the
    /// cell. QUIRK: as in [`World::deme_get_and_clear_cell_energy`], the
    /// cell's resources are added to themselves on top of that (and without
    /// energy resources `value / 0` is computed and not used).
    fn deme_give_back_cell_energy(&mut self, d: usize, absolute_cell_id: i32, value: f64) {
        let World {
            pop, rng, res_env, ..
        } = self;
        let deme = &mut pop.demes[d];
        let relative_cell_id = deme.relative_cell_id(absolute_cell_id);
        let mut cell_resources = deme
            .resource_count
            .get_cell_resources(relative_cell_id, rng, res_env)
            .to_vec();
        let amount_per_resource = value / deme.energy_res_ids.len() as f64;
        // put back energy resources evenly
        for &id in &deme.energy_res_ids {
            if let Some(v) = cell_resources.get_mut(id) {
                *v += amount_per_resource;
            }
        }
        deme.resource_count
            .modify_cell(rng, res_env, &cell_resources, relative_cell_id);
    }

    /// The energy branch at the end of `cPopulationCell::InsertOrganism`
    /// for the organism just put in `cell_id`: with `FRAC_ENERGY_TRANSFER >
    /// 0` it takes up all of the cell's deme energy (`UptakeCellEnergy(1.0)`)
    /// and its merit follows its new energy. The organism is the one in the
    /// cell, or `out_of_cell` (the running organism, held by the caller).
    pub(crate) fn cell_insert_energy(&mut self, cell_id: i32, out_of_cell: Option<&mut Phenotype>) {
        if !self.cfg.energy_enabled || self.cfg.frac_energy_transfer <= 0.0 {
            return;
        }
        let d = self.pop.cells[cell_id as usize].deme_id as usize;
        // `UptakeCellEnergy(1.0, ctx)`: what is not taken up goes back
        // (nothing here, but `GiveBackCellEnergy` still runs).
        let frac_to_uptake = 1.0;
        let mut cell_energy = self.deme_get_and_clear_cell_energy(d, cell_id);
        let uptake_amount = cell_energy * frac_to_uptake;
        cell_energy -= uptake_amount;
        self.deme_give_back_cell_energy(d, cell_id, cell_energy);
        if uptake_amount == 0.0 {
            return;
        }
        let ph = match self.pop.orgs[cell_id as usize].as_mut() {
            Some(o) => &mut o.state.phenotype,
            None => match out_of_cell {
                Some(ph) => ph,
                None => {
                    self.set_fatal(
                        "unsupported: FRAC_ENERGY_TRANSFER uptake by an organism out of reach",
                    );
                    return;
                }
            },
        };
        // update energy and merit
        ph.reduce_energy(&self.cfg, -uptake_amount);
        ph.merit =
            Merit::new(ph.convert_energy_to_merit(&self.cfg, ph.energy_store * ph.execution_ratio));
    }

    /// The energy branch of `cPopulationCell::RemoveOrganism` for an
    /// organism with `stored_energy` leaving `cell_id`: unless
    /// `FRAC_ENERGY_DECAY_AT_DEME_BIRTH` is 1 (marked "hack" in the C++),
    /// `FRAC_ENERGY_TRANSFER` of it goes back to the cell.
    pub(crate) fn cell_remove_energy(&mut self, cell_id: i32, stored_energy: f64) {
        if !self.cfg.energy_enabled
            || self.cfg.frac_energy_transfer <= 0.0
            || self.cfg.frac_energy_decay_at_deme_birth == 1.0
        {
            return;
        }
        let d = self.pop.cells[cell_id as usize].deme_id as usize;
        let value = stored_energy * self.cfg.frac_energy_transfer;
        self.deme_give_back_cell_energy(d, cell_id, value);
    }

    /// `cDeme::CalculateTotalEnergy(ctx)`: the stored energy of the deme's
    /// organisms plus `FRAC_ENERGY_TRANSFER` of the energy in its empty
    /// cells.
    pub fn deme_calculate_total_energy(&mut self, d: usize) -> f64 {
        let mut energy_sum = 0.0;
        for i in 0..self.pop.demes[d].size() {
            let cellid = self.pop.demes[d].cell_id(i);
            if self.is_occupied(cellid) {
                match self.pop.orgs[cellid as usize].as_ref() {
                    Some(o) => energy_sum += o.state.phenotype.energy_store,
                    None => self.set_fatal(
                        "unsupported: cDeme::CalculateTotalEnergy over the running organism",
                    ),
                }
            } else {
                let energy_in_cell = self.deme_cell_energy(d, cellid);
                energy_sum += energy_in_cell * self.cfg.frac_energy_transfer;
            }
        }
        energy_sum
    }

    /// The energy prologue of `ReplaceDeme`: the energy the source deme has
    /// left plus its testament, after decay, split into the part that stays
    /// (`parent_deme_energy`) and the part the target receives
    /// (`offspring_deme_energy`). Both are 0 without the energy model.
    pub(crate) fn deme_replication_energy(&mut self, source: usize) -> (f64, f64) {
        if !self.cfg.energy_enabled {
            return (0.0, 0.0);
        }
        let energy_remaining_in_source_deme = self.deme_calculate_total_energy(source);
        let deme = &mut self.pop.demes[source];
        deme.energy_remaining_in_deme_at_replication = energy_remaining_in_source_deme;
        let source_deme_energy = energy_remaining_in_source_deme + deme.total_energy_testament;
        self.stats
            .energy
            .testament_accepted_by_deme
            .add(deme.total_energy_testament);
        let deme_energy_decay = 1.0 - self.cfg.frac_energy_decay_at_deme_birth;
        let frac = self.cfg.frac_parent_energy_given_to_deme_at_birth;
        let parent_deme_energy = source_deme_energy * deme_energy_decay * (1.0 - frac);
        let offspring_deme_energy = source_deme_energy * deme_energy_decay * frac;
        (parent_deme_energy, offspring_deme_energy)
    }

    /// The end of `ReplaceDeme` under `ENERGY_PASSED_ON_DEME_REPLICATION_METHOD
    /// 0`: split each deme's energy evenly between its (newly seeded)
    /// organisms. QUIRK: the new merits do not reach the scheduler (no
    /// `AdjustSchedule`).
    pub(crate) fn distribute_deme_energy(
        &mut self,
        source: usize,
        target: usize,
        parent_deme_energy: f64,
        offspring_deme_energy: f64,
    ) {
        if !self.cfg.energy_enabled || self.cfg.energy_passed_on_deme_replication_method != 0 {
            return;
        }
        if offspring_deme_energy > 0.0 {
            // split deme energy evenly between organisms in target deme
            self.inject_deme_energy(target, offspring_deme_energy);
        }
        if parent_deme_energy > 0.0 {
            // split deme energy evenly between organisms in source deme
            self.inject_deme_energy(source, parent_deme_energy);
        }
    }

    fn inject_deme_energy(&mut self, d: usize, deme_energy: f64) {
        let org_count = f64::from(self.pop.demes[d].cur_org_count);
        let mut total_energy_injected_into_organisms = 0.0;
        for i in 0..self.pop.demes[d].size() {
            let cellid = self.pop.demes[d].cell_id(i);
            if !self.is_occupied(cellid) {
                continue;
            }
            let Some(org) = self.pop.orgs[cellid as usize].as_mut() else {
                self.set_fatal("unsupported: deme energy for the running organism");
                return;
            };
            let ph = &mut org.state.phenotype;
            ph.set_energy(&self.cfg, ph.energy_store + deme_energy / org_count);
            ph.merit = Merit::new(
                ph.convert_energy_to_merit(&self.cfg, ph.energy_store * ph.execution_ratio),
            );
            total_energy_injected_into_organisms += ph.energy_store;
        }
        self.pop.demes[d].energy_injected_into_organisms = total_energy_injected_into_organisms;
    }
}
