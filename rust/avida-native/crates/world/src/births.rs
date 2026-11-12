//! A log of births for front ends that follow an organism's offspring (not
//! in the C++).
//!
//! `ActivateOffspring` places each offspring with `ActivateOrganism`; the
//! one line [`World::activate_offspring`] adds after that call records who
//! was born where, from which parent, while recording is on. Recording only
//! appends to a list that nothing in the simulation reads, so it cannot
//! change what a run computes; it is off unless a front end turns it on.

use crate::world::World;

/// An offspring placed in the population.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Birth {
    /// `cOrganism::GetID()` of the offspring.
    pub org_id: i32,
    /// The cell it was placed in.
    pub cell: i32,
    /// The parent's id.
    pub parent_id: i32,
    /// The cell the parent lived in when it divided.
    pub parent_cell: i32,
    /// `cStats::GetUpdate()` at the birth.
    pub update: i32,
}

/// The births recorded since the last [`World::take_births`].
#[derive(Debug, Default)]
pub struct BirthLog {
    on: bool,
    births: Vec<Birth>,
}

impl World {
    /// Turns recording of births on or off (off discards what was recorded).
    pub fn record_births(&mut self, on: bool) {
        self.pop.births.on = on;
        if !on {
            self.pop.births.births.clear();
        }
    }

    /// The births recorded since the last call, oldest first.
    pub fn take_births(&mut self) -> Vec<Birth> {
        std::mem::take(&mut self.pop.births.births)
    }

    /// Records the offspring `ActivateOrganism` just placed in `cell`, if it
    /// lived through being placed (deadly boundaries, the tolerance doom
    /// cell and birth methods 12 and 13 kill it at once; those births are
    /// not recorded).
    pub(crate) fn note_birth(
        &mut self,
        cell: i32,
        parent_id: i32,
        parent_cell: i32,
        survived: bool,
    ) {
        if !self.pop.births.on || !survived {
            return;
        }
        let Some(org) = usize::try_from(cell)
            .ok()
            .and_then(|c| self.pop.orgs.get(c))
            .and_then(|o| o.as_deref())
        else {
            return;
        };
        let birth = Birth {
            org_id: org.state.id,
            cell,
            parent_id,
            parent_cell,
            update: self.stats.update(),
        };
        self.pop.births.births.push(birth);
    }
}
