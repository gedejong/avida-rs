//! The deme loop of `cHardwareCPU::Inst_ApplyPointMutationsGroupRandom`
//! and `Inst_ApplyPointMutationsGroupGS`
//! (avida-core/source/cpu/cHardwareCPU.cc at 47f13dadb), which point-mutate
//! the hardware of another member of the organism's deme (or its own).

use avida_hardware::organism::OrgState;
use avida_hardware::Hardware;

use crate::pop_interface::PopulationInterface;
use crate::world::World;

impl World {
    /// From `GetInt(0, deme_size)` on, the first occupied cell of deme
    /// `deme_id` whose organism's `IsGermline()` equals `germline` (any
    /// organism without it) gets `PointMutate(ctx, prob)` on its hardware
    /// and the count added by `IncPointMutations`. The running organism
    /// (`org` / `hw`, in `own_cell`) is a member like any other.
    pub fn point_mutate_deme_member(
        &mut self,
        own_cell: i32,
        org: &mut OrgState,
        hw: &mut dyn Hardware,
        deme_id: i32,
        prob: f64,
        germline: Option<bool>,
    ) {
        let Some(deme) = usize::try_from(deme_id)
            .ok()
            .and_then(|d| self.pop.demes.get(d))
        else {
            return;
        };
        let deme_size = deme.size();
        let start_pos = self.rng.get_int_range(0, deme_size);

        for i in 0..deme_size {
            let pos = (i + start_pos) % deme_size;
            let cell = self.pop.demes[deme_id as usize].cell_id(pos);
            if !self.is_occupied(cell) {
                continue;
            }
            if cell == own_cell {
                if germline.is_some_and(|gs| gs != org.phenotype.is_germ_cell) {
                    continue;
                }
                let num_mut = {
                    let mut iface = PopulationInterface {
                        w: self,
                        cell_id: cell,
                    };
                    hw.point_mutate(org, &mut iface, prob)
                };
                org.num_point_mut += num_mut;
                return;
            }
            let Some(mut sl) = self.pop.orgs[cell as usize].take() else {
                continue;
            };
            if germline.is_some_and(|gs| gs != sl.state.phenotype.is_germ_cell) {
                self.pop.orgs[cell as usize] = Some(sl);
                continue;
            }
            let num_mut = {
                let mut iface = PopulationInterface {
                    w: self,
                    cell_id: cell,
                };
                sl.hardware.point_mutate(&sl.state, &mut iface, prob)
            };
            sl.state.num_point_mut += num_mut;
            self.pop.orgs[cell as usize] = Some(sl);
            return;
        }
    }
}
