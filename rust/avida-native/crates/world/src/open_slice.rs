//! A speculative time slice cut between its instructions, for debuggers
//! (not in the C++).
//!
//! `ProcessStepSpeculative` runs an organism's instruction and, when that
//! returns true, up to 32 more with `SingleProcess(ctx, true)`, all in one
//! call. Stepping a debugger one time slice at a time therefore steps up to
//! 33 instructions at once. A debugger cannot run the organism without
//! speculation instead: speculative execution stops before instructions
//! that draw random numbers or touch other organisms (those that STALL),
//! so the order of everything the run computes depends on it.
//!
//! [`World::open_slice`] runs the first instruction of
//! [`World::process_step`]`(step_size, cell_id, true)` and stops;
//! [`World::open_slice_next`] runs one more instruction of the burst, until
//! the burst ends and the slice closes with the rest of `process_step`.
//! The calls on the world are those `process_step` makes, in the same
//! order, so a run computes the same whether its slices are cut or not
//! (`crates/avida/tests/stepper.rs` compares the output files).
//!
//! Between two instructions the organism sits in its cell as it does
//! between time slices (`process_step` holds it outside the population),
//! so captures and probes see it, and `Population::running` is clear, so
//! nothing that may run meanwhile mistakes it for the organism in the
//! middle of a step. A slice is only cut while that is so: once a step has
//! killed the organism, the rest of its burst runs at once.

use avida_hardware::organism::Organism;

use crate::pop_interface::PopulationInterface;
use crate::population::Running;
use crate::world::World;

/// Speculative instructions a burst may run after the slice's real one
/// (`ProcessStepSpeculative`'s `spec_count < 32`).
const MAX_SPECULATIVE: i32 = 32;

/// A time slice paused between two of its organism's instructions.
#[derive(Clone, Debug)]
pub struct OpenSlice {
    /// The scheduled cell (`process_step`'s `cell_id`).
    cell_id: i32,
    step_size: f64,
    /// The organism (`GetID()` and the organism itself) and the cell it
    /// lives in now (a step may move it).
    org_id: i32,
    org_uid: avida_hardware::organism::OrgUid,
    org_cell: i32,
    /// The cell id of the organism's interface (`m_cell_id`), carried from
    /// one instruction to the next as `process_step`'s one interface does.
    iface_cell: i32,
    /// Speculative instructions run so far.
    spec_count: i32,
}

impl OpenSlice {
    /// The cell the organism of the slice lives in.
    pub fn cell(&self) -> i32 {
        self.org_cell
    }

    /// Its `cOrganism::GetID()`.
    pub fn org_id(&self) -> i32 {
        self.org_id
    }

    /// The scheduled cell the slice was given to.
    pub fn scheduled_cell(&self) -> i32 {
        self.cell_id
    }

    /// Instructions the slice has run (the real one and the speculative
    /// ones).
    pub fn instructions(&self) -> i32 {
        1 + self.spec_count
    }
}

impl World {
    /// `process_step(step_size, cell_id, true)` up to the end of its first
    /// instruction: `Some` when a speculative burst follows, which
    /// [`World::open_slice_next`] then runs one instruction at a time;
    /// `None` when the whole step is done (no burst, or a slice that only
    /// used up the cell's speculative credit).
    pub fn open_slice(
        &mut self,
        step_size: f64,
        cell_id: i32,
    ) -> Result<Option<OpenSlice>, String> {
        if cell_id < 0 || self.pop.spec_state[cell_id as usize] > 0 {
            return self.process_step(step_size, cell_id, true).map(|()| None);
        }
        let c = cell_id as usize;
        let mut org = self.pop.orgs[c]
            .take()
            .ok_or("Unoccupied cell getting processor time!")?;
        let org_id = org.state.id;
        let org_uid = org.state.uid;
        self.pop.running = Some(Running {
            cell_id,
            org_id,
            uid: org_uid,
            ..Running::default()
        });
        self.host.cur_sg = org.state.cur_sg;
        self.hgt_step_begins(&org.state);
        let mut slice = OpenSlice {
            cell_id,
            step_size,
            org_id,
            org_uid,
            org_cell: cell_id,
            iface_cell: cell_id,
            spec_count: 0,
        };
        let first = {
            let mut iface = PopulationInterface { w: self, cell_id };
            let r = org.single_process(&mut iface, false);
            slice.iface_cell = iface.cell_id;
            r
        };
        // A gradient habitat's `SetToDie` (as in `process_step`).
        self.settle_habitat_to_die(&mut org.state);
        match first {
            Ok(true) => self.burst_goes_on(org, slice),
            Ok(false) => self.end_slice(org, slice, false, Ok(())),
            Err(e) => self.end_slice(org, slice, false, Err(e)),
        }
    }

    /// The next speculative instruction of an open slice: `Some` while the
    /// burst goes on, `None` once the slice has closed.
    pub fn open_slice_next(&mut self, mut slice: OpenSlice) -> Result<Option<OpenSlice>, String> {
        let oc = usize::try_from(slice.org_cell).map_err(|_| "an open time slice lost its cell")?;
        match self.pop.orgs.get(oc).and_then(|o| o.as_deref()) {
            Some(o) if o.state.uid == slice.org_uid => {}
            _ => {
                return Err(format!(
                    "the organism of an open time slice (id {}) left cell {}",
                    slice.org_id, slice.org_cell
                ))
            }
        }
        let Some(mut org) = self.pop.orgs[oc].take() else {
            return Err("an open time slice lost its organism".to_string());
        };
        self.pop.running = Some(Running {
            cell_id: slice.org_cell,
            org_id: slice.org_id,
            uid: slice.org_uid,
            ..Running::default()
        });
        self.host.cur_sg = org.state.cur_sg;
        self.hgt_step_begins(&org.state);
        match self.speculative_instruction(&mut org, &mut slice) {
            Ok(true) => {
                slice.spec_count += 1;
                self.burst_goes_on(org, slice)
            }
            Ok(false) => self.end_slice(org, slice, true, Ok(())),
            Err(e) => self.end_slice(org, slice, true, Err(e)),
        }
    }

    /// Runs an open slice to its end.
    pub fn close_slice(&mut self, mut slice: OpenSlice) -> Result<(), String> {
        while let Some(s) = self.open_slice_next(slice)? {
            slice = s;
        }
        Ok(())
    }

    /// `org.single_process(true)` through the slice's interface.
    fn speculative_instruction(
        &mut self,
        org: &mut Organism,
        slice: &mut OpenSlice,
    ) -> Result<bool, String> {
        let mut iface = PopulationInterface {
            w: self,
            cell_id: slice.iface_cell,
        };
        let r = org.single_process(&mut iface, true);
        slice.iface_cell = iface.cell_id;
        // A gradient habitat's `SetToDie` (as in `process_step`).
        self.settle_habitat_to_die(&mut org.state);
        r
    }

    /// After an instruction that lets the burst go on: pauses with the
    /// organism back in its cell, or, when it cannot go back, runs the rest
    /// of the burst now.
    fn burst_goes_on(
        &mut self,
        mut org: Box<Organism>,
        mut slice: OpenSlice,
    ) -> Result<Option<OpenSlice>, String> {
        loop {
            if slice.spec_count >= MAX_SPECULATIVE {
                return self.end_slice(org, slice, true, Ok(()));
            }
            if let Some(cell) = self.parking_cell() {
                self.pop.running = None;
                slice.org_cell = cell;
                self.pop.orgs[cell as usize] = Some(org);
                return Ok(Some(slice));
            }
            match self.speculative_instruction(&mut org, &mut slice) {
                Ok(true) => slice.spec_count += 1,
                Ok(false) => return self.end_slice(org, slice, true, Ok(())),
                Err(e) => return self.end_slice(org, slice, true, Err(e)),
            }
        }
    }

    /// The cell the running organism can wait in between two instructions:
    /// its own, while it is alive there and the cell holds no one else.
    fn parking_cell(&self) -> Option<i32> {
        let r = self.pop.running.as_ref()?;
        if r.removed || r.notify_pending || r.dead_membership.is_some() {
            return None;
        }
        let c = usize::try_from(r.cell_id).ok()?;
        matches!(self.pop.orgs.get(c), Some(None)).then_some(r.cell_id)
    }

    /// The end of `process_step`: the burst's bookkeeping (when there was
    /// a burst), then everything after the organism's instructions.
    fn end_slice(
        &mut self,
        mut org: Box<Organism>,
        slice: OpenSlice,
        burst: bool,
        result: Result<(), String>,
    ) -> Result<Option<OpenSlice>, String> {
        if burst {
            self.pop.spec_state[slice.cell_id as usize] = slice.spec_count;
            self.stats.m_spec_total += slice.spec_count;
            self.stats.m_spec_num += 1;
        }

        // From here on as in `process_step`.
        let merit = org.state.phenotype.merit.get_double();
        let running = self.pop.running.take().unwrap_or_default();
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
            self.delete_organism(org, running.dead_membership);
        } else {
            self.pop.orgs[running.cell_id as usize] = Some(org);
        }
        self.flush_parasite_deaths();
        self.flush_host();
        result?;

        self.stats.inc_executed();
        self.pop.resource_count.update(slice.step_size);
        self.demes_after_step(slice.step_size, slice.cell_id, merit, true);
        Ok(None)
    }
}
