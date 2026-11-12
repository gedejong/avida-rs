//! Port of `cPopulationInterface::RotateToGreatestReputation`,
//! `RotateToGreatestReputationWithDifferentTag` and
//! `RotateToGreatestReputationWithDifferentLineage`
//! (avida-core/source/main/cPopulationInterface.cc at 47f13dadb), which the
//! heads CPU's `rotate-to-rep*` instructions reach.
//!
//! All three turn the cell's connection list itself (`CircNext`), also
//! under `USE_AVATARS`, where `Rotate` would turn the avatar instead.

use avida_hardware::organism::OrgState;

use crate::world::World;

/// The neighbours a `RotateToGreatestReputation*` call may turn to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReputationFilter {
    /// `RotateToGreatestReputation()`
    Any,
    /// `...WithDifferentTag(tag)`: `GetTagLabel() != tag`
    OtherTag(i32),
    /// `...WithDifferentLineage(line)`: `GetLineageLabel() != line`
    OtherLineage(i32),
}

/// `cOrganism::GetReputation()`: the newest opinion, 0 without one.
fn reputation(state: &mut OrgState) -> i32 {
    if state.has_opinion() {
        state.get_opinion().0
    } else {
        0
    }
}

impl World {
    /// Runs `f` on the organism in `cell`: `org` for `own_cell` (the
    /// organism being run is not in the population while it runs).
    fn with_cell_state<T>(
        &mut self,
        cell: i32,
        own_cell: i32,
        org: &mut OrgState,
        f: impl FnOnce(&mut OrgState) -> T,
    ) -> Option<T> {
        if cell == own_cell {
            return self.is_occupied(cell).then(|| f(org));
        }
        let c = usize::try_from(cell).ok()?;
        self.pop.orgs.get_mut(c)?.as_mut().map(|o| f(&mut o.state))
    }

    /// Faces the neighbour with the highest reputation (at least -1) among
    /// those `filter` admits, a random one of them on a tie (one
    /// `GetUInt(0, n)` draw on the world generator); stays put without one.
    /// The first pass turns the list once around, the second turns it to
    /// the first cell holding the chosen organism.
    pub fn rotate_to_greatest_reputation(
        &mut self,
        cell: i32,
        org: &mut OrgState,
        filter: ReputationFilter,
    ) {
        let Ok(c) = usize::try_from(cell) else {
            return;
        };
        let size = self.pop.cells[c].connections.len();
        let mut high_rep = -1;
        let mut high_rep_orgs: Vec<i32> = Vec::new();

        // loop to find the max reputation
        for _ in 0..size {
            let faced = self.pop.cells[c].connections.first().unwrap_or(cell);
            let candidate = self.with_cell_state(faced, cell, org, |s| {
                let admitted = match filter {
                    ReputationFilter::Any => true,
                    ReputationFilter::OtherTag(tag) => s.tag.0 != tag,
                    ReputationFilter::OtherLineage(line) => s.lineage_label != line,
                };
                // `GetReputation()` is only asked once the label test passed.
                admitted.then(|| (s.id, reputation(s)))
            });
            if let Some(Some((id, rep))) = candidate {
                if rep >= high_rep {
                    if rep > high_rep {
                        high_rep = rep;
                        high_rep_orgs.clear();
                    }
                    high_rep_orgs.push(id);
                }
            }
            // check the next neighbor
            self.pop.cells[c].connections.circ_next();
        }

        // Pick an organism to donate to
        if high_rep_orgs.is_empty() {
            return;
        }
        let rand_num = self.rng.get_uint_range(0, high_rep_orgs.len() as u32) as usize;
        let high_org_id = high_rep_orgs[rand_num];
        for _ in 0..size {
            let faced = self.pop.cells[c].connections.first().unwrap_or(cell);
            if self.with_cell_state(faced, cell, org, |s| s.id) == Some(high_org_id) {
                break;
            }
            self.pop.cells[c].connections.circ_next();
        }
    }
}
