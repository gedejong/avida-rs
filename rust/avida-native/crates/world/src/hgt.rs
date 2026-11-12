//! Lateral gene transfer on divide: `cPopulationInterface::GetLGTFragment`
//! (avida-core/source/main/cPopulationInterface.cc at 47f13dadb), which
//! `cHardwareBase::doLGTMutation` calls for `DIV_LGT_PROB` and its
//! siblings.
//!
//! Horizontal gene transfer (`ENABLE_HGT`): the genome fragments of
//! `cPopulationCell` (`AddGenomeFragments`, `PopGenomeFragment`,
//! `ClearFragments`), the pending fragments of `cPopulationInterface`
//! (`DoHGTMutation`, `DoHGTConjugation`, `DoHGTDonation`,
//! `ReceiveHGTDonation` and the three placements),
//! `cPopulation::AdjustHGTResource`, the `KillOrganism` deposit and the
//! `AvidianConjugation` action. The fragments live in [`HgtState`], kept in
//! [`crate::host::WorldHost`] so the task evaluator can metabolize them
//! (`cEnvironment::DoProcesses` with an `hgt=1` resource).
//!
//! Every `HGT_*` setting is handled: invalid `HGT_SOURCE`,
//! `HGT_FRAGMENT_SELECTION`, `HGT_FRAGMENT_XFORM` and
//! `HGT_CONJUGATION_METHOD` values abort when first used, as in C++, and
//! `HGT_DIFFUSION_METHOD` only matters to `DiffuseHGTGenomeFragments`
//! (`actions_pop2.rs`). Stops, where the C++ cannot be followed:
//!
//! - placing a fragment with `HGT_FRAGMENT_SELECTION` 0 or 1 (the default
//!   0 included): `FindSubstringMatch` reads its result from freed memory
//!   (see [`MATCH_PLACEMENT_STOP`]). Random placement (2) is ported, and so
//!   is everything before the placement, so runs where no fragment is ever
//!   placed (no competence or conjugation event finds one) are unaffected;
//! - fragment sizes that can never be positive (`HGT_FRAGMENT_SIZE_VARIANCE`
//!   negative, or 0 with `|HGT_FRAGMENT_SIZE_MEAN| < 1`): `RandomSplit`
//!   loops forever;
//! - a gradient habitat killing an organism inside the resource update
//!   with `HGT_COMPETENCE_P > 0`: `AdjustHGTResource` re-enters
//!   `cResourceCount::DoUpdates` there and the reference crashes (see
//!   `crate::habitat`);
//! - `ENABLE_HGT` without exactly one `hgt=1` resource: the C++ reports it
//!   through the driver before the driver exists and crashes at start-up
//!   (`crate::population`).
//! - `HGT_FRAGMENT_XFORM 1` is `assert(false)` in the C++, compiled out of
//!   the release reference, so it leaves fragments as they are (QUIRK).

use std::collections::{BTreeSet, HashMap};

use avida_apto::AvidaRng;
use avida_genome::genome_util::{self, FragmentList, SubstringMatch};
use avida_genome::{Genome, InstructionSequence};
use avida_hardware::organism::{OrgState, OrgUid};

use crate::world::World;

/// `MAX_POP_SAMPLES` of `GetLGTFragment`.
const MAX_POP_SAMPLES: usize = 10;

/// HGT bookkeeping of the cells and organisms.
#[derive(Debug, Default)]
pub struct HgtState {
    /// `cPopulationCell::m_hgt->fragments` by cell id (grown on demand).
    pub cells: Vec<FragmentList>,
    /// `cPopulationInterface::m_hgt_support->_pending` by organism.
    pub pending: HashMap<OrgUid, FragmentList>,
    /// The running organism's genome (`GetOrganism()->GetGenome()` of its
    /// cell), recorded when each step begins (`process_step` and the open
    /// slices of the stepper) with `ENABLE_HGT` set: the hardware holds the
    /// organism. Only read where the caller does not hold its state (a
    /// `KillOrganism` of its cell from elsewhere); `DoHGTMutation` reads the
    /// dividing parent it is handed.
    pub running_genome: Option<InstructionSequence>,
}

impl HgtState {
    /// `cPopulationCell::CountGenomeFragments()`
    pub fn count(&self, cell_id: i32) -> usize {
        usize::try_from(cell_id)
            .ok()
            .and_then(|c| self.cells.get(c))
            .map_or(0, FragmentList::len)
    }

    /// `cPopulationCell::GetFragments()` (`InitHGTSupport` on first use).
    fn cell_mut(&mut self, cell: usize) -> &mut FragmentList {
        if self.cells.len() <= cell {
            self.cells.resize_with(cell + 1, FragmentList::new);
        }
        &mut self.cells[cell]
    }

    /// The `hgt=1` branch of `cEnvironment::DoProcesses`: in a population
    /// cell (`cellid != -1`) with fragments, `cell.PopGenomeFragment(ctx)`
    /// (a `GetUInt(0, size)` draw); the fragment's length.
    pub fn metabolize(&mut self, cell_id: i32, rng: &mut AvidaRng) -> Option<i32> {
        let c = usize::try_from(cell_id).ok()?;
        let frags = self.cells.get_mut(c).filter(|f| !f.is_empty())?;
        let idx = rng.get_uint_range(0, frags.len() as u32) as usize;
        frags.remove(idx).map(|f| f.size())
    }
}

/// `cGenomeUtil::RandomSplit` never ends when every draw floors to 0.
fn split_never_ends(mean: f64, variance: f64) -> bool {
    // (`!(variance >= 0)` and `!(|mean| >= 1)` in the C++: NaN counts.)
    variance.is_nan() || variance < 0.0 || (variance == 0.0 && (mean.is_nan() || mean.abs() < 1.0))
}

/// Why match placement stops the run.
///
/// `cGenomeUtil::FindSubstringMatch` ends with `delete [] m[0];
/// delete [] m[1]; return *min;`, where `min` points into one of the two
/// freed rows, so the reference returns whatever the allocator left there.
/// Traced in the reference binary (seed 101, `heads_default_100u`, 15x15,
/// `HGT_COMPETENCE_P 0.5`, 300 updates): 766 of 770 calls read the intact
/// row, 4 read pages macOS had already zeroed, giving `begin = end = 0`, so
/// the replacement became an insertion at a random site. Under
/// `MallocScribble=1`, which changes nothing but the contents of freed
/// memory, the reference's final population differs. The outcome depends on
/// the C++ heap, which the port does not have.
const MATCH_PLACEMENT_STOP: &str = "HGT_FRAGMENT_SELECTION 0 or 1 (match placement) placed a \
     fragment: C++ cGenomeUtil::FindSubstringMatch returns its result from freed memory \
     (`delete [] m[0]; delete [] m[1]; return *min;`), so the placement depends on the C++ heap";

const SPLIT_FOREVER: &str = "HGT fragment sizes that are never positive (C++: \
     cGenomeUtil::RandomSplit loops forever)";

impl World {
    /// The genome of the organism in `cell_id` (`GetOrganism()->
    /// GetGenome()`), the running one included: from `running` when the
    /// caller holds its state (the dividing parent of `DoHGTMutation`),
    /// else from the copy its step recorded.
    fn hgt_genome_in(
        &self,
        cell_id: i32,
        running: Option<&OrgState>,
    ) -> Option<InstructionSequence> {
        let c = usize::try_from(cell_id).ok()?;
        if let Some(o) = self.pop.orgs.get(c).and_then(Option::as_ref) {
            return Some(o.state.genome().representation().clone());
        }
        let in_cell = self
            .pop
            .running
            .as_ref()
            .filter(|r| r.cell_id == cell_id && !r.removed);
        if let Some(r) = in_cell {
            if let Some(state) = running.filter(|s| s.uid == r.uid) {
                return Some(state.genome().representation().clone());
            }
            if let Some(g) = self.host.hgt.running_genome.as_ref() {
                return Some(g.clone());
            }
            self.set_fatal("unsupported: HGT reached the running organism's genome before its step recorded it");
        }
        None
    }

    /// `cGenomeUtil::RandomSplit(ctx, HGT_FRAGMENT_SIZE_MEAN,
    /// HGT_FRAGMENT_SIZE_VARIANCE, genome, fragments)`; `None` (the run
    /// stops) where the C++ loops forever.
    fn hgt_split(&mut self, genome: &InstructionSequence, out: &mut FragmentList) -> bool {
        let (mean, variance) = (
            self.cfg.hgt_fragment_size_mean,
            self.cfg.hgt_fragment_size_variance,
        );
        if split_never_ends(mean, variance) || genome.size() <= 0 {
            self.set_fatal(format!("unsupported: {SPLIT_FOREVER}"));
            return false;
        }
        genome_util::random_split(&mut self.rng, mean, variance, genome, out);
        true
    }

    /// `cPopulation::AdjustHGTResource(ctx, delta)`: `resource_count.
    /// Modify(ctx, m_hgt_resid, delta)`. QUIRK: `m_hgt_resid` is the index
    /// in the resource library, used as an index into the global counts.
    pub(crate) fn adjust_hgt_resource(&mut self, delta: f64) {
        let Ok(id) = usize::try_from(self.pop.hgt_resid) else {
            return;
        };
        if id >= self.pop.resource_count.size() {
            self.set_fatal(
                "unsupported: an HGT resource behind deme resources (C++: \
                 cPopulation::AdjustHGTResource writes past the global resource counts)",
            );
            return;
        }
        self.with_pop_resources(|rc, rng, env| rc.modify_one(rng, env, id, delta));
    }

    /// `cPopulationCell::AddGenomeFragments(ctx, genome)`
    fn add_genome_fragments(&mut self, cell_id: i32, genome: &InstructionSequence) {
        let Ok(c) = usize::try_from(cell_id) else {
            return;
        };
        self.adjust_hgt_resource(f64::from(genome.size()));
        let mut frags = FragmentList::new();
        if !self.hgt_split(genome, &mut frags) {
            return;
        }
        self.host.hgt.cell_mut(c).append(&mut frags);
        // pop off the front of this cell's buffer until we have <=
        // HGT_MAX_FRAGMENTS_PER_CELL (an unsigned comparison).
        let max = self.cfg.hgt_max_fragments_per_cell as u32 as usize;
        while self.host.hgt.cell_mut(c).len() > max {
            let Some(front) = self.host.hgt.cell_mut(c).pop_front() else {
                break;
            };
            self.adjust_hgt_resource(-f64::from(front.size()));
        }
    }

    /// `cPopulationCell::ClearFragments(ctx)`
    fn clear_fragments(&mut self, cell_id: i32) {
        let Ok(c) = usize::try_from(cell_id) else {
            return;
        };
        let frags = std::mem::take(self.host.hgt.cell_mut(c));
        for f in &frags {
            self.adjust_hgt_resource(-f64::from(f.size()));
        }
    }

    /// Called when an organism's step begins: keeps its genome for the HGT
    /// code that reads the running organism's cell.
    pub(crate) fn hgt_step_begins(&mut self, org: &OrgState) {
        if self.cfg.enable_hgt != 0 {
            let genome = org.genome().representation();
            match &mut self.host.hgt.running_genome {
                Some(kept) => kept.clone_from(genome),
                kept @ None => *kept = Some(genome.clone()),
            }
        }
    }

    /// The HGT part of `KillOrganism(cell)`: with natural competence
    /// possible, the dead organism's genome is split into the cell
    /// (`AddGenomeFragments`); its pending fragments die with its
    /// interface. `genome` is `None` for the running organism when the
    /// caller does not hold its state.
    pub(crate) fn hgt_organism_killed(
        &mut self,
        cell_id: i32,
        uid: OrgUid,
        genome: Option<&InstructionSequence>,
    ) {
        if self.cfg.enable_hgt == 0 {
            return;
        }
        self.host.hgt.pending.remove(&uid);
        if self.cfg.hgt_competence_p <= 0.0 {
            return;
        }
        let genome = match genome {
            Some(g) => g.clone(),
            None => match self.host.hgt.running_genome.clone() {
                Some(g) => g,
                None => {
                    self.set_fatal(
                        "unsupported: KillOrganism with HGT of an organism whose genome is not \
                         at hand",
                    );
                    return;
                }
            },
        };
        self.add_genome_fragments(cell_id, &genome);
    }

    /// The source (`DoHGTConjugation`) or target (`DoHGTDonation`) cell of
    /// a conjugation from `cell_id`, per `HGT_CONJUGATION_METHOD`; `None`
    /// when there is nothing to do.
    fn hgt_partner(&mut self, cell_id: i32) -> Option<i32> {
        match self.cfg.hgt_conjugation_method {
            // selected at random from neighborhood
            0 => {
                // `GetOccupiedNeighboringCells(set, 1)`: a set of cell
                // pointers into the cell array, so in cell id order.
                let mut cell_set = BTreeSet::new();
                self.neighboring_cells(cell_id, 1, &mut cell_set);
                let occupied: Vec<i32> = cell_set
                    .into_iter()
                    .filter(|&c| self.is_occupied(c))
                    .collect();
                if occupied.is_empty() {
                    // nothing to do here, there are no neighbors
                    return None;
                }
                let k = self.rng.get_int(occupied.len() as i32) as usize;
                Some(occupied[k])
            }
            // faced individual
            1 => {
                let Some(faced) = self.pop.cells[cell_id as usize].cell_faced() else {
                    self.set_fatal(
                        "HGT conjugation from a cell without neighbours (C++: NULL dereference)",
                    );
                    return None;
                };
                // nothing to do if we're facing an empty cell.
                self.is_occupied(faced).then_some(faced)
            }
            _ => {
                self.abort_run(
                    "error: HGT_CONJUGATION_METHOD is set to an invalid value.\n",
                    2,
                    "HGT_CONJUGATION_METHOD is set to an invalid value.",
                );
                None
            }
        }
    }

    /// A random fragment of a `RandomSplit` of `genome`
    /// (`fragments[GetInt(fragments.size())]`).
    fn hgt_random_fragment(&mut self, genome: &InstructionSequence) -> Option<InstructionSequence> {
        let mut frags = FragmentList::new();
        if !self.hgt_split(genome, &mut frags) {
            return None;
        }
        let idx = self.rng.get_int(frags.len() as i32) as usize;
        frags.swap_remove_back(idx)
    }

    /// `cPopulationInterface::DoHGTConjugation(ctx)` of the running
    /// organism `me` in `cell_id`: a fragment of a neighbour's genome
    /// becomes pending. The neighbour is `me` itself where the cell is its
    /// own neighbour (a 1x1 torus).
    fn hgt_conjugation(&mut self, cell_id: i32, me: &OrgState) {
        let Some(source) = self.hgt_partner(cell_id) else {
            return;
        };
        let uid = me.uid;
        let Some(genome) = self.hgt_genome_in(source, Some(me)) else {
            return;
        };
        if let Some(frag) = self.hgt_random_fragment(&genome) {
            // `ReceiveHGTDonation(fragment)`
            self.host
                .hgt
                .pending
                .entry(uid)
                .or_default()
                .push_back(frag);
        }
    }

    /// `cPopulationInterface::DoHGTDonation(ctx)` of the organism in
    /// `cell_id`: a fragment of its genome becomes pending in the target.
    fn hgt_donation(&mut self, cell_id: i32) {
        let Some(target) = self.hgt_partner(cell_id) else {
            return;
        };
        let Some(genome) = self.hgt_genome_in(cell_id, None) else {
            return;
        };
        let Some(frag) = self.hgt_random_fragment(&genome) else {
            return;
        };
        let Some(target_uid) = self.org_uid_in_cell(target) else {
            return;
        };
        // `target->GetOrganism()->GetOrgInterface().ReceiveHGTDonation(...)`
        self.host
            .hgt
            .pending
            .entry(target_uid)
            .or_default()
            .push_back(frag);
    }

    /// `cActionAvidianConjugation::Process`: each organism, in cell order,
    /// donates with probability `donation_p`.
    pub(crate) fn avidian_conjugation(&mut self, donation_p: f64) {
        for i in 0..self.pop.cells.len() {
            if self.pop.orgs[i].is_some() && donation_p > 0.0 && self.rng.p(donation_p) {
                self.hgt_donation(i as i32);
                if self.fatal.borrow().is_some() {
                    return;
                }
            }
        }
    }

    /// `cPopulationInterface::DoHGTMutation(ctx, offspring)` of the
    /// organism `parent` dividing in `cell_id`.
    pub(crate) fn hgt_mutation(&mut self, cell_id: i32, parent: &OrgState, offspring: &mut Genome) {
        let uid = parent.uid;
        // first, gather up all the fragments that we're going to be
        // inserting into this offspring: these come from a per-replication
        // conjugational event:
        let conjugation_p = self.cfg.hgt_conjugation_p;
        if conjugation_p > 0.0 && self.rng.p(conjugation_p) {
            self.hgt_conjugation(cell_id, parent);
        }
        if self.fatal.borrow().is_some() {
            return;
        }

        // the pending list includes both the fragments selected above and
        // those from population-level conjugational events.
        let mut fragments = self.host.hgt.pending.remove(&uid).unwrap_or_default();

        // these come from "natural" competence (ie, eating the dead):
        let competence_p = self.cfg.hgt_competence_p;
        if competence_p > 0.0 && self.rng.p(competence_p) {
            match self.cfg.hgt_source {
                // source is other genomes, nothing to do here (default)
                0 => {}
                // source is the parent (a control)
                1 => {
                    self.clear_fragments(cell_id);
                    let Some(genome) = self.hgt_genome_in(cell_id, Some(parent)) else {
                        return;
                    };
                    self.add_genome_fragments(cell_id, &genome);
                }
                _ => {
                    self.abort_run(
                        "error: HGT_SOURCE is set to an invalid value.\n",
                        2,
                        "HGT_SOURCE is set to an invalid value.",
                    );
                    return;
                }
            }
            if self.fatal.borrow().is_some() {
                return;
            }
            // do we have any fragments available?
            let n = self.host.hgt.count(cell_id);
            if n > 0 {
                // add a randomly-selected fragment to the list of fragments
                // to be HGT'd, remove it from the cell, and adjust the level
                // of HGT resource.
                let idx = self.rng.get_int(n as i32) as usize;
                if let Some(selected) = self.host.hgt.cell_mut(cell_id as usize).remove(idx) {
                    let size = selected.size();
                    fragments.push_back(selected);
                    self.adjust_hgt_resource(-f64::from(size));
                }
            }
        }

        // now, for each fragment being HGT'd, figure out where to put it:
        let selection = self.cfg.hgt_fragment_selection;
        let xform = self.cfg.hgt_fragment_xform;
        let insertion_p = self.cfg.hgt_insertion_mut_p;
        for frag in fragments.iter_mut() {
            let location = match selection {
                // match placement (0), and match placement with redundant
                // instruction trimming (1): `FindUnbiasedCircularMatch`.
                0 | 1 => {
                    self.set_fatal(MATCH_PLACEMENT_STOP);
                    return;
                }
                // random placement
                2 => {
                    let size = offspring.representation().size();
                    let mut location = SubstringMatch::default();
                    location.begin = self.rng.get_uint(size as u32) as i32;
                    location.end = location
                        .begin
                        .wrapping_add(self.rng.get_uint((frag.size() * 2) as u32) as i32);
                    location.size = size as usize;
                    location.resize(size as usize);
                    location
                }
                _ => {
                    self.abort_run(
                        "error: HGT_FRAGMENT_SELECTION is set to an invalid value.\n",
                        2,
                        "HGT_FRAGMENT_SELECTION is set to an invalid value.",
                    );
                    return;
                }
            };

            // transformations of the fragment, as controls.
            match xform {
                // no transformation.
                0 => {}
                // QUIRK: random shuffle is `assert(false)`, compiled out of
                // the release build: nothing happens.
                1 => {}
                // replace the instructions in the fragment with random
                // instructions.
                2 => {
                    let Some(inst_set) = self.hw.registry().get_inst_set(offspring.inst_set())
                    else {
                        self.set_fatal(
                            "HGT_FRAGMENT_XFORM 2: the offspring's instruction set is not loaded",
                        );
                        return;
                    };
                    for j in 0..frag.size() {
                        frag.set(j, inst_set.random_inst(&mut self.rng));
                    }
                }
                _ => {
                    self.abort_run(
                        "error: HGT_FRAGMENT_XFORM is set to an invalid value.\n",
                        2,
                        "HGT_FRAGMENT_XFORM is set to an invalid value.",
                    );
                    return;
                }
            }

            // do the mutation: an insertion just after the final location,
            // or a replacement of [begin, end) respecting circularity.
            let seq = offspring.representation_mut();
            if self.rng.p(insertion_p) {
                seq.insert_seq(location.end, frag);
            } else {
                seq.replace_circular(frag, location.begin, location.end);
            }

            // stats tracking:
            self.stats.genome_fragment_inserted(frag.size());
        }
    }
}

impl World {
    /// The genome of the organism in `cell_id` for `GetLGTFragment`. The
    /// running organism (the one dividing) is out of `pop.orgs`; its genome
    /// is `running_genome` (`m_organism->GetGenome()`).
    fn lgt_genome_in<'a>(&'a self, cell_id: i32, running_genome: &'a Genome) -> Option<&'a Genome> {
        let c = usize::try_from(cell_id).ok()?;
        if let Some(o) = self.pop.orgs.get(c).and_then(Option::as_ref) {
            return Some(o.state.genome());
        }
        self.pop
            .running
            .as_ref()
            .filter(|r| r.cell_id == cell_id && !r.removed)
            .map(|_| running_genome)
    }

    /// `cPopulationInterface::GetLGTFragment(ctx, region, dest_genome,
    /// seq)` of the organism in `cell_id`: `None` where the C++ returns
    /// false.
    pub(crate) fn lgt_fragment(
        &mut self,
        cell_id: i32,
        region: i32,
        dest_genome: &Genome,
    ) -> Option<InstructionSequence> {
        let dest_hw = dest_genome.hardware_type();
        let src_seq: InstructionSequence = match region {
            // Local Neighborhood
            0 => {
                let c = usize::try_from(cell_id).ok()?;
                // `GetOccupiedNeighboringCells`: the connections in list order.
                let mut occupied: Vec<i32> = self.pop.cells[c]
                    .connections
                    .iter()
                    .filter(|&n| self.lgt_genome_in(n, dest_genome).is_some())
                    .collect();
                let mut num_cells = occupied.len();
                let mut i = 0;
                while i < num_cells {
                    let hw = self
                        .lgt_genome_in(occupied[i], dest_genome)
                        .map_or(-1, Genome::hardware_type);
                    if hw != dest_hw {
                        // Organism type mis-match, remove from consideration.
                        num_cells -= 1;
                        occupied[i] = occupied[num_cells];
                    } else {
                        i += 1;
                    }
                }
                if num_cells == 0 {
                    return None;
                }
                let cell_idx = self.rng.get_int(num_cells as i32) as usize;
                self.lgt_genome_in(occupied[cell_idx], dest_genome)?
                    .representation()
                    .clone()
            }
            // Entire Population
            1 => {
                let n = self.pop.live_org_list.len() as i32;
                let mut found = None;
                for _ in 0..MAX_POP_SAMPLES {
                    let org_idx = self.rng.get_int(n) as usize;
                    let cell = self.pop.live_org_list[org_idx];
                    let genome = self.lgt_genome_in(cell, dest_genome)?;
                    // QUIRK: takes the first sampled organism whose hardware
                    // type differs from the destination's (`!=`), so with a
                    // single hardware type it never finds one.
                    if genome.hardware_type() != dest_hw {
                        found = Some(genome.representation().clone());
                        break;
                    }
                }
                found?
            }
            _ => return None,
        };

        // Select random start and end point
        let mut from = self.rng.get_int(src_seq.size());
        let mut to = self.rng.get_int(src_seq.size());
        // Order from and to indices
        if from > to {
            std::mem::swap(&mut from, &mut to);
        }
        if to == from {
            // zero size treated as transfer of the whole genome
            Some(src_seq)
        } else {
            Some(src_seq.crop(from, to))
        }
    }
}
