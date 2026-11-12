//! `SET_FT_AT_BIRTH` in `cPopulation::ActivateOffspring`
//! (avida-core/source/main/cPopulation.cc at 47f13dadb): each offspring is
//! given a random prey forage target (0, 1 or 2) before it is placed, and
//! with `MAX_PREY_BT` a random live organism already using that target dies
//! when the target is full. [`crate::pop_orgs`] calls it from
//! `World::activate_offspring`, after the offspring's group is settled.
//!
//! The offspring has no population interface yet (`SetOrgInterface` runs in
//! `ActivateOrganism`), so the parts of `cOrganism::SetForageTarget` that
//! call through it dereference NULL in the C++; the port stops there.

use avida_hardware::organism::OrgState;

use crate::world::World;

impl World {
    /// The `SET_FT_AT_BIRTH` block for one offspring `child` of the running
    /// organism `parent` (of `parent_cell`). Returns `true` when the
    /// organism `MAX_PREY_BT` killed was the parent (the C++ then clears
    /// `parent_alive`).
    pub(crate) fn set_ft_at_birth(
        &mut self,
        parent: &mut OrgState,
        parent_cell: i32,
        child: &mut OrgState,
    ) -> bool {
        let mut prop_target = 2;
        if self.rng.p(0.5) {
            prop_target = 0;
            if self.rng.p(0.5) {
                prop_target = 1;
            }
        }

        let mut parent_killed = false;
        let max_prey_bt = self.cfg.max_prey_bt;
        if max_prey_bt != 0 {
            // The live organisms whose forage target is the proposed one, in
            // the order of the live list (the parent is out of its cell).
            let mut orgs: Vec<i32> = Vec::new();
            for &cell in &self.pop.live_org_list {
                let this_target = match self.pop.orgs[cell as usize].as_ref() {
                    Some(o) => o.state.forage_target,
                    None if cell == parent_cell => parent.forage_target,
                    None => continue,
                };
                if this_target == prop_target {
                    orgs.push(cell);
                }
            }
            let in_use = orgs.len() as i32;
            if in_use >= max_prey_bt {
                let pick = self.rng.get_uint_range(0, in_use as u32) as usize;
                let Some(&victim) = orgs.get(pick) else {
                    // A negative MAX_PREY_BT with no organism on the target.
                    self.set_fatal(
                        "SET_FT_AT_BIRTH with MAX_PREY_BT < 0 and no organism on the target \
                         (C++: reads past the end of the orgs array in \
                         cPopulation::ActivateOffspring)",
                    );
                    return false;
                };
                // `org->Die(ctx)`
                if self.pop.orgs[victim as usize].is_none() && victim == parent_cell {
                    self.kill_organism(parent_cell, Some(parent));
                    parent.is_dead = true;
                    parent_killed = true;
                } else {
                    self.kill_organism(victim, None);
                }
            }
        }

        // `offspring->SetForageTarget(ctx, prop_target)`. The offspring's
        // parent target is not set yet (-1), so the predator cap does not
        // apply, and its own target is still -1 (prey), so the pred/prey
        // switch counts nothing; the prey cap calls `KillRandPrey` through
        // the offspring's NULL interface.
        let max_pred = self.cfg.max_pred;
        let num_total_pred = self.pop.num_top_pred_organisms + self.pop.num_pred_organisms;
        let pred_cap = child.parent_ft <= -2 && max_pred > 0 && num_total_pred >= max_pred;
        let max_prey = self.cfg.max_prey;
        let prey_cap = max_prey > 0 && self.pop.num_prey_organisms >= max_prey;
        if pred_cap || prey_cap {
            self.set_fatal(
                "SET_FT_AT_BIRTH with a MAX_PREY or MAX_PRED cap reached (C++: calls through \
                 the offspring's NULL interface in cOrganism::SetForageTarget)",
            );
        }
        child.forage_target = prop_target;
        if child.show_ft == -1 {
            child.show_ft = child.forage_target;
        }
        // `RecordFTSet()`
        child.has_set_ft = true;
        parent_killed
    }
}
