//! The birth methods of `cPopulation::PositionOffspring` that place by time
//! used or by dispersal (avida-core/source/main/cPopulation.cc at
//! 47f13dadb): `BIRTH_METHOD 9` (`POSITION_OFFSPRING_FULL_SOUP_ENERGY_USED`),
//! `10` (`POSITION_OFFSPRING_NEIGHBORHOOD_ENERGY_USED`, through
//! `PositionEnergyUsed`) and `11` (`POSITION_OFFSPRING_DISPERSAL`, hops of
//! `DISPERSAL_RATE`). [`crate::pop_orgs`] calls them from
//! `World::position_offspring`.
//!
//! The lists are `tList`s, whose `Push` adds at the front; the found lists
//! here are kept in that order, so `GetPos(GetUInt(size))` is an index.

use avida_hardware::organism::OrgState;

use crate::world::World;

impl World {
    /// `GetOrganism()->GetPhenotype().GetTimeUsed()` of the organism in
    /// `cell` (the dividing parent is out of its cell); `None` when the
    /// cell is empty.
    fn time_used_in(&self, cell: i32, parent_cell: i32, parent: &OrgState) -> Option<i32> {
        if cell == parent_cell {
            return Some(parent.phenotype.time_used);
        }
        self.pop.orgs[cell as usize]
            .as_ref()
            .map(|o| o.state.phenotype.time_used)
    }

    /// `POSITION_OFFSPRING_FULL_SOUP_ENERGY_USED`: a random cell among those
    /// whose organism has used the most time, an empty cell counting as
    /// `INT_MAX`.
    pub(crate) fn position_full_soup_energy_used(
        &mut self,
        parent_cell: i32,
        parent: &OrgState,
    ) -> i32 {
        let mut found_list: Vec<i32> = Vec::new();
        let mut max_time_used = 0;
        for i in 0..self.pop.cells.len() as i32 {
            let time_used = if self.is_occupied(i) {
                self.time_used_in(i, parent_cell, parent)
                    .unwrap_or(i32::MAX)
            } else {
                i32::MAX
            };
            if time_used == max_time_used {
                found_list.insert(0, i);
            } else if time_used > max_time_used {
                max_time_used = time_used;
                found_list.clear();
                found_list.insert(0, i);
            }
        }
        let choice = self.rng.get_uint(found_list.len() as u32) as usize;
        match found_list.get(choice) {
            Some(&c) => c,
            None => {
                // Every organism has a negative time used.
                self.set_fatal(
                    "BIRTH_METHOD 9 found no cell (C++: GetPos past the end of an empty list)",
                );
                parent_cell
            }
        }
    }

    /// `PositionEnergyUsed(parent_cell, found_list, parent_ok)` (the
    /// `POSITION_OFFSPRING_NEIGHBORHOOD_ENERGY_USED` case, reached only when
    /// no empty neighbour was preferred): the parent and the neighbours that
    /// used the most time. `false` (the run stops) for an empty neighbour,
    /// which the C++ dereferences.
    pub(crate) fn position_energy_used(
        &mut self,
        parent_cell: i32,
        parent: &OrgState,
        parent_ok: bool,
        conn: &[i32],
        found_list: &mut Vec<i32>,
    ) -> bool {
        found_list.insert(0, parent_cell);
        let mut max_energy_used = parent.phenotype.time_used;
        if !parent_ok {
            max_energy_used = -1;
        }
        for &c in conn {
            let Some(cur_energy_used) = self.time_used_in(c, parent_cell, parent) else {
                self.set_fatal(
                    "BIRTH_METHOD 10 with an empty neighbour (C++: NULL dereference in \
                     cPopulation::PositionEnergyUsed)",
                );
                return false;
            };
            if cur_energy_used > max_energy_used {
                max_energy_used = cur_energy_used;
                found_list.clear();
                found_list.insert(0, c);
            } else if cur_energy_used == max_energy_used {
                found_list.insert(0, c);
            }
        }
        true
    }

    /// The `POSITION_OFFSPRING_DISPERSAL` branch (parent with neighbours):
    /// `GetRandPoisson(DISPERSAL_RATE)` hops through random connections,
    /// then the empty cells of the last connection list (with
    /// `PREFER_EMPTY`), else that whole list, plus the parent when no hop
    /// was taken and the parent may be replaced.
    pub(crate) fn position_dispersal(
        &mut self,
        parent_cell: i32,
        parent_ok: bool,
        prefer_empty: bool,
    ) -> Vec<i32> {
        let mut disp_cell = parent_cell;
        // hop through connection lists based on the dispersal rate
        let hops = self.rng.get_rand_poisson(self.cfg.dispersal_rate);
        for _ in 0..hops {
            let list = &self.pop.cells[disp_cell as usize].connections;
            let pos = self.rng.get_uint(list.len() as u32) as usize;
            let Some(next) = list.get(pos) else {
                break;
            };
            disp_cell = next;
            if self.pop.cells[disp_cell as usize].connections.is_empty() {
                break;
            }
        }
        let disp_list: Vec<i32> = self.pop.cells[disp_cell as usize]
            .connections
            .iter()
            .collect();

        // if prefer empty, select an empty cell from the final connection
        // list
        let mut found_list: Vec<i32> = Vec::new();
        if prefer_empty {
            for &c in &disp_list {
                if !self.is_occupied(c) {
                    found_list.insert(0, c);
                }
            }
        }
        // if prefer empty is off, or there are no empty cells, use the whole
        // connection list as possibilities
        if found_list.is_empty() {
            found_list = disp_list;
            // if no hops were taken and ALLOW_PARENT is set, throw the parent
            // cell into the hat for possible selection
            if hops == 0 && parent_ok {
                found_list.insert(0, parent_cell);
            }
        }
        found_list
    }
}
