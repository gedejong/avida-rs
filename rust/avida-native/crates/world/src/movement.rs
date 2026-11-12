//! Organism movement of `cPopulation` (avida-core/source/main/cPopulation.cc
//! at 47f13dadb): `MoveOrganisms`, `SwapCells` and `InjureOrg`, plus the
//! cell-side half of `cPopulationCell::Rotate(target)` used by the donation
//! instructions.
//!
//! # The mover is the running organism
//! `MoveOrganisms` is reached from `cPopulationInterface::Move` (the
//! organism steps out of its own cell, `true_cell == -1`) and from
//! `cPopulationInterface::MoveAV` (an avatar of the organism steps,
//! `true_cell` is the organism's cell). In both cases the organism that
//! moves, and the one any death or injury on the way applies to, is the one
//! `ProcessStep` took out of its cell; its state is passed in explicitly.
//!
//! # What follows an organism when it changes cells
//! The C++ moves a `cOrganism*` between two cells; its interface (cell id,
//! previously seen cell, task cells), its systematics groups, its place in
//! the live organism list and its group membership travel with the pointer.
//! The port keys those by cell, so [`World::swap_cells`] swaps every
//! per-cell record of the two occupants: `pop.orgs`, `pop.membership`,
//! `pop.live_index` (and the live list entries), the `cPopulationInterface`
//! members in [`crate::host::WorldHost`], the group member keys
//! ([`crate::groups::Groups::list`]) and `Population::running`.

use avida_hardware::organism::OrgState;
use avida_phenotype::Merit;

use crate::world::World;

/// A member key no cell or pending offspring uses (`swap_member_keys`).
const SWAP_KEY: i32 = i32::MIN;

/// The stop of `cPopulation::UpdateMerit` with a merit of 0 or below: the
/// C++ kills the organism, then reads the phenotype of the emptied cell
/// through a NULL organism pointer.
pub(crate) const ZERO_MERIT_STOP: &str = "unsupported: UpdateMerit with a merit of 0 or below \
     (C++: cPopulation::UpdateMerit kills the organism, then dereferences the emptied cell)";

impl World {
    /// `GetCellResVal(ctx, cell_id, res_id)` with the environment's resource
    /// index (the C++ indexes the global `cResourceCount` with it).
    /// `mover` is the running organism, which a damaging gradient may
    /// injure during the read (crate::habitat).
    fn move_cell_res_val(&mut self, cell_id: i32, res_id: usize, mover: &mut OrgState) -> f64 {
        if res_id >= self.pop.resource_count.size() {
            self.set_fatal(format!(
                "resource {res_id} has no global resource count (C++: \
                 cPopulation::MoveOrganisms reads out of bounds with deme resources)"
            ));
            return 0.0;
        }
        self.with_pop_resources_for(Some(mover), |rc, rng, env| {
            rc.get_cell_res_val(rng, env, cell_id, res_id)
        })
    }

    /// `cOrganism::GetVitality()` of an organism state, with the current
    /// creature age statistics. A `COLLECT_SPECIFIC_RESOURCE` bin the
    /// organism does not have (no resource bins, `USE_RESOURCE_BINS 0`)
    /// stops the run: the C++ reads past `m_cur_rbins_avail`.
    pub(crate) fn vitality_of(&self, org: &OrgState) -> f64 {
        let mean_age = self.stats.sum_creature_age.average();
        let age_stddev = self.stats.sum_creature_age.std_deviation();
        let org_age = f64::from(org.phenotype.age);
        let resource = self.cfg.collect_specific_resource;
        let mut res_level = 0.0;
        if resource >= 0 {
            let bins = &org.phenotype.cur_rbins_avail;
            match bins.get(resource as usize) {
                Some(&level) => res_level = level,
                None => self.set_fatal(format!(
                    "unsupported: cOrganism::GetVitality with COLLECT_SPECIFIC_RESOURCE \
                     {resource} and {} resource bins (C++: reads past \
                     cPhenotype::m_cur_rbins_avail)",
                    bins.len()
                )),
            }
        }
        if org_age < (mean_age - age_stddev) || org_age > (mean_age + age_stddev) {
            self.cfg.vitality_bin_extremes * res_level
        } else {
            self.cfg.vitality_bin_center * res_level
        }
    }

    /// `cPopulation::MoveOrganisms(ctx, src_cell_id, dest_cell_id,
    /// true_cell)`. `mover` is the running organism (see the module docs).
    /// Returns whether the move happened.
    pub fn move_organisms(
        &mut self,
        src_cell_id: i32,
        dest_cell_id: i32,
        true_cell: i32,
        mover: &mut OrgState,
    ) -> bool {
        let wx = self.cfg.world_x;
        let dest_x = dest_cell_id % wx;
        let dest_y = dest_cell_id / wx;
        // The organism that dies when the move kills: the avatar's owner or
        // the mover itself (both are the running organism).
        let victim = if true_cell != -1 {
            true_cell
        } else {
            src_cell_id
        };

        // check for boundary effects on movement
        if self.cfg.deadly_boundaries == 1 && self.cfg.world_geometry == 1 {
            // Fail if we're running in the test CPU.
            if src_cell_id < 0 {
                return false;
            }
            if dest_x == 0 || dest_y == 0 || dest_x == wx - 1 || dest_y == self.cfg.world_y - 1 {
                self.kill_organism(victim, Some(mover));
                return false;
            }
        }

        let num_res = self.env.resource_lib().size().max(0) as usize;
        let has_path_env = self.env.has_path();
        let has_hammer_env = self.env.has_hammer();
        let mut has_path = false;
        let mut has_hammer = false;
        for i in 0..num_res {
            let (path, hammer) = {
                let r = self.env.resource_lib().resource(i);
                (r.path, r.hammer)
            };
            if has_path_env && path && self.move_cell_res_val(dest_cell_id, i, mover) > 0.0 {
                has_path = true;
            }
            if has_hammer_env && hammer && self.move_cell_res_val(dest_cell_id, i, mover) > 0.0 {
                has_hammer = true;
            }
        }

        if !has_path || has_hammer {
            // test for death by predatory resource or injury ... not
            // mutually exclusive
            for i in 0..num_res {
                let (predator, predator_odds, deadly, death_odds, damage, threshold) = {
                    let r = self.env.resource_lib().resource(i);
                    (
                        r.predator,
                        r.predator_odds,
                        r.deadly,
                        r.death_odds,
                        r.damage,
                        r.threshold,
                    )
                };
                if predator || deadly {
                    // get the destination cell resource levels
                    let dest_cell_resources = self.move_cell_res_val(dest_cell_id, i, mover);
                    if dest_cell_resources > 0.0 {
                        // if you step on a predatory resource, we're going to
                        // try to kill you (`||` short-circuits the draws)
                        let killed = (predator && self.rng.p(predator_odds))
                            || (deadly && self.rng.p(death_odds));
                        if killed {
                            self.kill_organism(victim, Some(mover));
                            return false;
                        }
                    }
                }
                if damage != 0.0 {
                    let dest_cell_resources = self.move_cell_res_val(dest_cell_id, i, mover);
                    if dest_cell_resources > threshold {
                        if true_cell == -1 {
                            self.set_fatal(
                                "damaging resource on an organism move (C++: \
                                 cPopulation::MoveOrganisms injures GetCell(-1))",
                            );
                            return false;
                        }
                        self.injure_running(true_cell, mover, damage, false);
                    }
                }
            }
            // if any of the resources have resistance, find the id of the
            // most resistant resource
            let mut steepest_hill = 0;
            let mut curr_resistance = 1.0;
            for i in 0..num_res {
                let (path, resistance) = {
                    let r = self.env.resource_lib().resource(i);
                    (r.path, r.resistance)
                };
                if !path
                    && resistance > curr_resistance
                    && self.move_cell_res_val(src_cell_id, i, mover) != 0.0
                {
                    curr_resistance = resistance;
                    steepest_hill = i;
                }
            }
            // apply the chance of move failing for the most resistant
            // resource in this cell, if there is one
            if num_res > 0
                && curr_resistance != 1.0
                && self.move_cell_res_val(src_cell_id, steepest_hill, mover) > 0.0
            {
                // 'resistance == # move instructions executed, on average,
                // to move one step/cell'
                let chance_move_success = 1.0 / curr_resistance;
                if !self.rng.p(chance_move_success) {
                    return false;
                }
            }

            // movement fails if there are any barrier resources in the faced
            // cell (unless the org is already on a barrier)
            let mut curr_is_barrier = false;
            for i in 0..num_res {
                let (habitat, path) = {
                    let r = self.env.resource_lib().resource(i);
                    (r.habitat, r.path)
                };
                if habitat == 2 && !path && self.move_cell_res_val(src_cell_id, i, mover) > 0.0 {
                    curr_is_barrier = true;
                    break;
                }
            }
            if !curr_is_barrier {
                for i in 0..num_res {
                    let (habitat, path, resistance) = {
                        let r = self.env.resource_lib().resource(i);
                        (r.habitat, r.path, r.resistance)
                    };
                    // fail if faced cell has this wall resource
                    if !path
                        && habitat == 2
                        && resistance != 0.0
                        && self.move_cell_res_val(dest_cell_id, i, mover) > 0.0
                    {
                        return false;
                    }
                }
            }
        }

        // effects not applied to avatars:
        if true_cell == -1 {
            let lethal = self.cfg.movement_collisions_lethal;
            // In a world one cell wide or high the mover can face, and move
            // into, its own cell: the organism it collides with is itself.
            let into_self = dest_cell_id == src_cell_id;
            if lethal != 0 && self.is_occupied(dest_cell_id) {
                if lethal == 2 {
                    return false;
                }
                let kill_source = match self.cfg.movement_collisions_selection_type {
                    1 => {
                        // binned vitality based on age
                        let src_vitality = self.vitality_of(mover);
                        let dest_vitality = if into_self {
                            src_vitality
                        } else {
                            self.pop.orgs[dest_cell_id as usize]
                                .as_ref()
                                .map_or(0.0, |o| self.vitality_of(&o.state))
                        };
                        src_vitality < self.rng.get_double_max(src_vitality + dest_vitality)
                    }
                    // 50% chance, no modifiers
                    _ => self.rng.p(0.5),
                };
                if kill_source {
                    self.kill_organism(src_cell_id, Some(mover));
                    // Killing the moving organism means that we shouldn't
                    // actually do the swap, so return
                    return false;
                }
                if into_self {
                    self.kill_organism(dest_cell_id, Some(mover));
                } else {
                    self.kill_organism(dest_cell_id, None);
                }
            }
            self.swap_cells(src_cell_id, dest_cell_id, Some(mover));

            // Find neighborhood size for facing
            let actual_neighborhood_size = if self.is_occupied(dest_cell_id) {
                self.pop.cells[dest_cell_id as usize].connections.len()
            } else if self.is_occupied(src_cell_id) {
                self.pop.cells[src_cell_id as usize].connections.len()
            } else {
                // Punt
                8
            };

            // Swap cell facings between cells, so that if movement is
            // directed, it continues to be associated with the same organism
            let from_facing = self.cell_facing(src_cell_id);
            let dest_facing = self.cell_facing(dest_cell_id);
            // Set facing in source cell
            for _ in 0..actual_neighborhood_size {
                if self.cell_facing(src_cell_id) != dest_facing {
                    self.pop.cells[src_cell_id as usize].connections.circ_next();
                } else {
                    break;
                }
            }
            // Set facing in destination cell
            for _ in 0..actual_neighborhood_size {
                if self.cell_facing(dest_cell_id) != from_facing {
                    self.pop.cells[dest_cell_id as usize]
                        .connections
                        .circ_next();
                } else {
                    break;
                }
            }
        }
        true
    }

    /// `cPopulationCell::GetFacing()` of a cell.
    pub(crate) fn cell_facing(&self, cell_id: i32) -> i32 {
        let cell = &self.pop.cells[cell_id as usize];
        match cell.cell_faced() {
            Some(f) => {
                let faced = &self.pop.cells[f as usize];
                cell.facing((faced.x, faced.y))
            }
            None => 0,
        }
    }

    /// `cPopulation::SwapCells(cell_id1, cell_id2, ctx)`. `running` is the
    /// running organism's state when one of the cells holds it (the mover).
    pub fn swap_cells(&mut self, cell_id1: i32, cell_id2: i32, mut running: Option<&mut OrgState>) {
        // Sanity checks: Don't process if the cells are the same
        if cell_id1 == cell_id2 {
            return;
        }
        let (c1, c2) = (cell_id1 as usize, cell_id2 as usize);
        let occ1 = self.is_occupied(cell_id1);
        let occ2 = self.is_occupied(cell_id2);
        // The organism in a cell: the cell's own, or the running one (out
        // of its cell while it runs).
        let merit_in = |w: &World, c: usize, running: Option<&OrgState>| -> f64 {
            match w.pop.orgs[c].as_ref() {
                Some(o) => o.state.phenotype.merit.get_double(),
                None => running.map_or(0.0, |r| r.phenotype.merit.get_double()),
            }
        };

        // `cell1.RemoveOrganism(ctx)`, `cell2.RemoveOrganism(ctx)`
        for (occ, cell) in [(occ1, cell_id1), (occ2, cell_id2)] {
            if occ {
                let energy = match self.pop.orgs[cell as usize].as_ref() {
                    Some(o) => Some(o.state.phenotype.energy_store),
                    None => running.as_deref().map(|r| r.phenotype.energy_store),
                };
                match energy {
                    Some(e) => self.cell_remove_energy(cell, e),
                    None if self.cfg.energy_enabled && self.cfg.frac_energy_transfer > 0.0 => {
                        self.set_fatal(
                            "unsupported: FRAC_ENERGY_TRANSFER when SwapCells moves the running \
                             organism without its state",
                        );
                    }
                    None => {}
                }
            }
        }

        // Everything that follows the organism.
        self.pop.orgs.swap(c1, c2);
        self.pop.membership.swap(c1, c2);
        self.pop.live_index.swap(c1, c2);
        for &c in &[c1, c2] {
            if let Ok(idx) = usize::try_from(self.pop.live_index[c]) {
                self.pop.live_org_list[idx] = c as i32;
            }
        }
        self.host.prev_seen.swap(c1, c2);
        self.host.prev_task.swap(c1, c2);
        self.host.num_task_cells.swap(c1, c2);
        self.swap_member_keys(cell_id1, cell_id2);
        if let Some(r) = self.pop.running.as_mut() {
            if !r.removed {
                if r.cell_id == cell_id1 {
                    r.cell_id = cell_id2;
                } else if r.cell_id == cell_id2 {
                    r.cell_id = cell_id1;
                }
            }
        }

        // `cell1.InsertOrganism(org2)`, `cell2.InsertOrganism(org1)`, each
        // followed by `AdjustSchedule` with the merit the insertion left.
        let mut merit2 = 0.0;
        if occ2 {
            self.stats.m_spec_waste += self.pop.spec_state[c1];
            self.pop.spec_state[c1] = 0;
            if self.host.prev_seen[c1] == -1 {
                self.host.prev_seen[c1] = cell_id1;
            }
            self.cell_insert_energy(cell_id1, running.as_deref_mut().map(|r| &mut r.phenotype));
            merit2 = merit_in(self, c1, running.as_deref());
        }
        self.adjust_schedule(cell_id1, merit2);
        let mut merit1 = 0.0;
        if occ1 {
            self.stats.m_spec_waste += self.pop.spec_state[c2];
            self.pop.spec_state[c2] = 0;
            if self.host.prev_seen[c2] == -1 {
                self.host.prev_seen[c2] = cell_id2;
            }
            self.cell_insert_energy(cell_id2, running.as_deref_mut().map(|r| &mut r.phenotype));
            self.pop.cells[c2].visits += 1; // Increment visit count
            merit1 = merit_in(self, c2, running.as_deref());
        }
        self.adjust_schedule(cell_id2, merit1);

        // LHZ: Take organism inputs from the PopulationCell along with the
        // organisms
        let mut inputs1 = std::mem::take(&mut self.pop.cells[c1].inputs);
        let mut inputs2 = std::mem::take(&mut self.pop.cells[c2].inputs);
        self.env.swap_inputs(&mut inputs1, &mut inputs2);
        self.pop.cells[c1].inputs = inputs1;
        self.pop.cells[c2].inputs = inputs2;
    }

    /// Swaps the group member keys of two cells' occupants.
    fn swap_member_keys(&mut self, a: i32, b: i32) {
        let g = &mut self.pop.groups;
        g.rekey(a, SWAP_KEY);
        g.rekey(b, a);
        g.rekey(SWAP_KEY, b);
    }

    /// `cPopulation::InjureOrg(ctx, cell, injury, ding_reacs)` on the
    /// running organism living in `cell_id`.
    pub(crate) fn injure_running(
        &mut self,
        cell_id: i32,
        target: &mut OrgState,
        injury: f64,
        ding_reacs: bool,
    ) {
        if injury == 0.0 {
            return;
        }
        if self.cfg.merit_inc_apply_immediate {
            let target_merit = target.phenotype.merit.get_double();
            // FMA (fmsub in the reference): `target_merit -= target_merit * injury`
            let target_merit = (-target_merit).mul_add(injury, target_merit);
            self.update_merit_of(cell_id, Some(target), target_merit);
        }
        injure_state(target, injury, ding_reacs, self.cfg.use_resource_bins);
    }

    /// `cOrganism::UpdateMerit(ctx, new_merit)` →
    /// `cPopulation::UpdateMerit(ctx, cell_id, new_merit)` for the organism
    /// in `cell_id` (`state` when it is the running one). A merit of 0 or
    /// below kills the organism and stops the run: the C++ goes on to read
    /// the phenotype of the cell it has just emptied.
    pub(crate) fn update_merit_of(
        &mut self,
        cell_id: i32,
        state: Option<&mut OrgState>,
        new_merit: f64,
    ) {
        if new_merit <= 0.0 {
            self.kill_organism(cell_id, state);
            self.set_fatal(ZERO_MERIT_STOP);
            return;
        }
        let apply = |phenotype: &mut avida_phenotype::Phenotype| {
            let old_merit = phenotype.merit.get_double();
            phenotype.merit = Merit::new(new_merit);
            phenotype.life_fitness = new_merit / f64::from(phenotype.gestation_time);
            if new_merit <= old_merit {
                phenotype.is_donor_cur = true;
            } else {
                phenotype.is_receiver = true;
            }
        };
        match state {
            Some(s) => apply(&mut s.phenotype),
            None => match self.pop.orgs[cell_id as usize].as_mut() {
                Some(o) => apply(&mut o.state.phenotype),
                None => return,
            },
        }
        self.adjust_schedule(cell_id, new_merit);
    }

    /// `while (neighbor->GetNeighbor() != m_organism) neighbor->Rotate(ctx,
    /// 1)`: turn `cell_id` until it faces `target_cell_id`. The C++ loops
    /// forever when the cells are not connected; that is a fatal error here.
    pub fn turn_cell_to_face(&mut self, cell_id: i32, target_cell_id: i32) {
        let Some(cell) = usize::try_from(cell_id)
            .ok()
            .and_then(|c| self.pop.cells.get_mut(c))
        else {
            return;
        };
        if !cell.connections.contains(target_cell_id) {
            self.set_fatal(format!(
                "cell {cell_id} is not connected to cell {target_cell_id} (C++: \
                 cPopulationInterface rotation loops forever)"
            ));
            return;
        }
        while cell.cell_faced() != Some(target_cell_id) {
            cell.connections.circ_next();
        }
    }
}

/// The phenotype half of `cPopulation::InjureOrg`.
pub(crate) fn injure_state(
    target: &mut OrgState,
    injury: f64,
    ding_reacs: bool,
    use_resource_bins: bool,
) {
    if ding_reacs {
        let target_reactions = target.phenotype.last_reaction_count.clone();
        for (i, r) in target_reactions.iter().enumerate() {
            let v = *r - (f64::from(*r) * injury) as i32;
            if let Some(slot) = target.phenotype.cur_reaction_count.get_mut(i) {
                *slot = v;
            }
        }
    }
    let target_bonus = target.phenotype.cur_bonus;
    // FMA (fmsub in the reference): `target_bonus - (target_bonus * injury)`
    target.phenotype.cur_bonus = (-target_bonus).mul_add(injury, target_bonus);
    if use_resource_bins {
        let target_bins = target.rbins().to_vec();
        for (i, b) in target_bins.iter().enumerate() {
            target.add_to_rbin(i as i32, -(b * injury));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SWAP_KEY;
    use crate::groups::Groups;

    #[test]
    fn member_keys_swap_between_two_cells() {
        let mut g = Groups::default();
        g.list.insert(0, vec![3, 7, 9]);
        g.list.insert(1, vec![7]);
        g.rekey(3, SWAP_KEY);
        g.rekey(7, 3);
        g.rekey(SWAP_KEY, 7);
        assert_eq!(g.list[&0], vec![7, 3, 9]);
        assert_eq!(g.list[&1], vec![3]);
    }
}
