//! Port of `cBirthChamber` (avida-core/source/main/cBirthChamber.cc at
//! 47f13dadb) with `cBirthEntry` and the selection handlers
//! `cBirthGlobalHandler`, `cBirthNeighborhoodHandler`,
//! `cBirthGridLocalHandler`, `cBirthGenomeSizeHandler`,
//! `cBirthMateSelectHandler`, `cBirthMatingTypeGlobalHandler` and
//! `cBirthDemeHandler` (all in avida-core/source/main). With `NUM_DEMES > 1`
//! every deme keeps its own sub-handler, so offspring mate only within their
//! deme, and `MATING_TYPES` is ignored, as in the C++.
//!
//! The C++ handlers hand back a `cBirthEntry*` into their own storage; here
//! they return an [`EntrySlot`] that [`SelectionHandler::entry_mut`] resolves.
//! A stored entry keeps "active references" on the parent's systematics
//! groups (`Group::AddActiveReference`) until `ClearEntry`, exactly as the
//! C++ does, so a genotype whose last organism died stays alive while an
//! offspring of it waits in the chamber.
//!
//! With `ENERGY_ENABLED` the offspring's merit comes from the energy the
//! parent hands over (`ExtractParentEnergy`), stored in the entry as
//! `energy4Offspring` while it waits. The one-offspring `TWO_FOLD_COST_SEX`
//! path that keeps the second genome reads `child_array[1]` out of range in
//! C++ (undefined behaviour) and is a fatal error here.
//!
//! FMA: the reference build contracts `a * b + c * d` into
//! `fma(a, b, c * d)`; the merit mixes below do the same.

use std::collections::BTreeMap;

use avida_apto::AvidaRng;
use avida_config::string_util::{convert_double, convert_int};
use avida_config::Config;
use avida_genome::{Genome, InstructionSequence};
use avida_hardware::organism::{transmission, OrgSource, OrgState, Organism};
use avida_phenotype::Merit;
use avida_systematics::unit::{GroupMembership, UnitInfo};
use avida_systematics::Manager;

use crate::energy::{extract_parent_energy, set_child_energy, NoPhenotypeStats};
use crate::pop_orgs::systematics_source;
use crate::population::position_offspring as po;
use crate::stats::Stats;
use crate::topology::grid_neighbor;
use crate::world::World;

/// `eMatingType` (avida/core/Definitions.h).
pub mod mating_type {
    pub const JUVENILE: i32 = -1;
    pub const FEMALE: i32 = 0;
    pub const MALE: i32 = 1;
}

/// `eMatePreference` (avida/core/Definitions.h).
pub mod mate_preference {
    pub const RANDOM: i32 = 0;
    pub const HIGHEST_DISPLAY_A: i32 = 1;
    pub const HIGHEST_DISPLAY_B: i32 = 2;
    pub const HIGHEST_MERIT: i32 = 3;
    pub const LOWEST_DISPLAY_A: i32 = 4;
    pub const LOWEST_DISPLAY_B: i32 = 5;
    pub const LOWEST_MERIT: i32 = 6;
}

/// `MIN_GENOME_LENGTH` (avida/core/Definitions.h)
const MIN_GENOME_LENGTH: i32 = 8;
/// `MAX_GENOME_LENGTH` (avida/core/Definitions.h)
const MAX_GENOME_LENGTH: i32 = 2048;

/// `cBirthChamber::RegionSwap`. Returns false, leaving both genomes alone,
/// when either result would have an illegal length.
pub fn region_swap(
    genome0: &mut InstructionSequence,
    genome1: &mut InstructionSequence,
    start0: i32,
    end0: i32,
    start1: i32,
    end1: i32,
) -> bool {
    // Calculate size of sections crossing over...
    let size0 = end0 - start0;
    let size1 = end1 - start1;

    let new_size0 = genome0.size() - size0 + size1;
    let new_size1 = genome1.size() - size1 + size0;

    // Don't Crossover if offspring will be illegal!!!
    if !(MIN_GENOME_LENGTH..=MAX_GENOME_LENGTH).contains(&new_size0)
        || !(MIN_GENOME_LENGTH..=MAX_GENOME_LENGTH).contains(&new_size1)
    {
        return false;
    }

    if size0 > 0 && size1 > 0 {
        let cross0 = genome0.crop(start0, end0);
        let cross1 = genome1.crop(start1, end1);
        genome0.replace(start0, size0, &cross1);
        genome1.replace(start1, size1, &cross0);
    } else if size0 > 0 {
        // QUIRK: genome0 keeps its region; only genome1 receives a copy.
        let cross0 = genome0.crop(start0, end0);
        genome1.replace(start1, size1, &cross0);
    } else if size1 > 0 {
        let cross1 = genome1.crop(start1, end1);
        genome0.replace(start0, size0, &cross1);
    }
    true
}

/// `cBirthChamber::GenomeSwap`
pub fn genome_swap(
    genome0: &mut InstructionSequence,
    genome1: &mut InstructionSequence,
    merit0: &mut f64,
    merit1: &mut f64,
) {
    std::mem::swap(genome0, genome1);
    std::mem::swap(merit0, merit1);
}

/// The shared tail of every recombination method: mix the merits by the
/// exchanged fraction and keep the majority of each genome with its merit.
fn adjust_merits(
    genome0: &mut InstructionSequence,
    genome1: &mut InstructionSequence,
    merit0: &mut f64,
    merit1: &mut f64,
    cut_frac: f64,
    stay_frac: f64,
) {
    // FMA: `merit0 * stay_frac + merit1 * cut_frac` fuses the left product.
    let tmp_merit0 = merit0.mul_add(stay_frac, *merit1 * cut_frac);
    *merit1 = merit1.mul_add(stay_frac, *merit0 * cut_frac);
    *merit0 = tmp_merit0;

    // Majority of the genome should stay in the offspring
    if stay_frac < cut_frac {
        genome_swap(genome0, genome1, merit0, merit1);
    }
}

fn frac_pos(frac: f64, genome: &InstructionSequence) -> i32 {
    (frac * f64::from(genome.size())) as i32
}

/// `cBirthChamber::DoBasicRecombination`
pub fn do_basic_recombination(
    rng: &mut AvidaRng,
    genome0: &mut InstructionSequence,
    genome1: &mut InstructionSequence,
    merit0: &mut f64,
    merit1: &mut f64,
) {
    let mut start_frac = rng.get_double();
    let mut end_frac = rng.get_double();
    if start_frac > end_frac {
        std::mem::swap(&mut start_frac, &mut end_frac);
    }

    // calculate the proportion of the genome that will be swapped
    let cut_frac = end_frac - start_frac;
    let stay_frac = 1.0 - cut_frac;

    let start0 = frac_pos(start_frac, genome0);
    let end0 = frac_pos(end_frac, genome0);
    let start1 = frac_pos(start_frac, genome1);
    let end1 = frac_pos(end_frac, genome1);

    region_swap(genome0, genome1, start0, end0, start1, end1);
    adjust_merits(genome0, genome1, merit0, merit1, cut_frac, stay_frac);
}

/// `cBirthChamber::DoModularContRecombination` (`num_modules` is
/// `MODULE_NUM`).
pub fn do_modular_cont_recombination(
    rng: &mut AvidaRng,
    num_modules: i32,
    genome0: &mut InstructionSequence,
    genome1: &mut InstructionSequence,
    merit0: &mut f64,
    merit1: &mut f64,
) {
    let n = f64::from(num_modules);
    let start_module = (rng.get_double() * n) as i32;
    let end_module = (rng.get_double() * n) as i32;

    let mut start_frac = f64::from(start_module) / n;
    let mut end_frac = f64::from(end_module) / n;
    if start_frac > end_frac {
        std::mem::swap(&mut start_frac, &mut end_frac);
    }

    let cut_frac = end_frac - start_frac;
    let stay_frac = 1.0 - cut_frac;

    let start0 = frac_pos(start_frac, genome0);
    let end0 = frac_pos(end_frac, genome0);
    let start1 = frac_pos(start_frac, genome1);
    let end1 = frac_pos(end_frac, genome1);

    region_swap(genome0, genome1, start0, end0, start1, end1);
    adjust_merits(genome0, genome1, merit0, merit1, cut_frac, stay_frac);
}

/// `cBirthChamber::DoModularNonContRecombination`
pub fn do_modular_non_cont_recombination(
    rng: &mut AvidaRng,
    num_modules: i32,
    genome0: &mut InstructionSequence,
    genome1: &mut InstructionSequence,
    merit0: &mut f64,
    merit1: &mut f64,
) {
    let n = f64::from(num_modules);
    let mut swap_count = 0;
    for i in 0..num_modules {
        if rng.get_double() < 0.5 {
            swap_count += 1;
            let start_frac = f64::from(i) / n;
            // `((double) i+1)`: the cast binds before the addition.
            let end_frac = (f64::from(i) + 1.0) / n;
            let start0 = frac_pos(start_frac, genome0);
            let end0 = frac_pos(end_frac, genome0);
            let start1 = frac_pos(start_frac, genome1);
            let end1 = frac_pos(end_frac, genome1);
            region_swap(genome0, genome1, start0, end0, start1, end1);
        }
    }

    let cut_frac = f64::from(swap_count) / n;
    let stay_frac = 1.0 - cut_frac;
    adjust_merits(genome0, genome1, merit0, merit1, cut_frac, stay_frac);
}

/// `cBirthChamber::DoModularShuffleRecombination`
pub fn do_modular_shuffle_recombination(
    rng: &mut AvidaRng,
    num_modules: i32,
    genome0: &mut InstructionSequence,
    genome1: &mut InstructionSequence,
    merit0: &mut f64,
    merit1: &mut f64,
) {
    let n = f64::from(num_modules);
    let mut swapped_region = vec![false; num_modules.max(0) as usize];

    let mut swap_count = 0;
    for mod0 in 0..num_modules {
        if rng.get_double() < 0.5 {
            swap_count += 1;

            // Collect start and end info for current module
            let start0_frac = f64::from(mod0) / n;
            let end0_frac = (f64::from(mod0) + 1.0) / n;
            let start0 = frac_pos(start0_frac, genome0);
            let end0 = frac_pos(end0_frac, genome0);

            // Pick module from other genome...
            let mut mod1 = rng.get_uint(num_modules as u32) as usize;
            while swapped_region[mod1] {
                mod1 = rng.get_uint(num_modules as u32) as usize;
            }
            swapped_region[mod1] = true;

            // Collect start and end info for other module
            let start1_frac = mod1 as f64 / n;
            let end1_frac = (mod1 as f64 + 1.0) / n;
            let start1 = frac_pos(start1_frac, genome1);
            let end1 = frac_pos(end1_frac, genome1);

            region_swap(genome0, genome1, start0, end0, start1, end1);
        }
    }

    let cut_frac = f64::from(swap_count) / n;
    let stay_frac = 1.0 - cut_frac;
    adjust_merits(genome0, genome1, merit0, merit1, cut_frac, stay_frac);
}

/// The recombination dispatch of `cBirthChamber::SubmitOffspring`:
/// `MODULE_NUM`, `CONT_REC_REGS` and `CORESPOND_REC_REGS` select the kernel.
#[allow(clippy::too_many_arguments)]
pub fn recombine(
    rng: &mut AvidaRng,
    num_modules: i32,
    continuous_regions: i32,
    corespond_rec_regs: bool,
    genome0: &mut InstructionSequence,
    genome1: &mut InstructionSequence,
    merit0: &mut f64,
    merit1: &mut f64,
) {
    let shuffle_regions = !corespond_rec_regs;
    if num_modules == 0 {
        do_basic_recombination(rng, genome0, genome1, merit0, merit1);
    } else if continuous_regions == 1 {
        do_modular_cont_recombination(rng, num_modules, genome0, genome1, merit0, merit1);
    } else if !shuffle_regions {
        do_modular_non_cont_recombination(rng, num_modules, genome0, genome1, merit0, merit1);
    } else {
        do_modular_shuffle_recombination(rng, num_modules, genome0, genome1, merit0, merit1);
    }
}

// ----------------------------------------------------------------------
// Birth entries
// ----------------------------------------------------------------------

/// `cBirthEntry` (avida-core/source/main/cBirthEntry.{h,cc}): an offspring
/// waiting in the chamber for a mate. `m_parent_task_count` is never set by
/// the C++ and is left out.
#[derive(Debug, Clone)]
pub struct BirthEntry {
    /// `energy4Offspring` (set by `StoreAsEntry` with `ENERGY_ENABLED`)
    pub energy4_offspring: f64,
    /// `m_mating_type`
    pub mating_type: i32,
    /// `m_mating_display_a`
    pub mating_display_a: i32,
    /// `m_mating_display_b`
    pub mating_display_b: i32,
    /// `m_mate_preference`
    pub mate_preference: i32,
    /// `m_group_id`
    pub group_id: i32,
    /// `genome`
    pub genome: Genome,
    /// `merit`
    pub merit: Merit,
    /// `timestamp` (-1 if empty)
    pub timestamp: i32,
    /// `groups` (NULL when the entry holds no active references).
    pub groups: Option<GroupMembership>,
}

impl Default for BirthEntry {
    /// `cBirthEntry::cBirthEntry()`
    fn default() -> Self {
        BirthEntry {
            energy4_offspring: 0.0,
            mating_type: mating_type::JUVENILE,
            mating_display_a: 0,
            mating_display_b: 0,
            mate_preference: mate_preference::RANDOM,
            group_id: -1,
            genome: Genome::default(),
            merit: Merit::default(),
            timestamp: -1,
            groups: None,
        }
    }
}

impl BirthEntry {
    /// `cBirthEntry::GetPhenotypeString()`
    pub fn phenotype_string(&self) -> String {
        format!(
            "{} {} {} {} {} {} {} {}",
            self.genome.representation().as_string(),
            convert_int(self.timestamp),
            convert_double(self.merit.get_double()),
            convert_int(self.mating_type),
            convert_int(self.mate_preference),
            convert_int(self.mating_display_a),
            convert_int(self.mating_display_b),
            convert_int(self.group_id),
        )
    }

    /// `cBirthEntry::GetPhenotypeStringFormat()`
    pub fn phenotype_string_format() -> &'static str {
        "genome timestamp merit mating_type mate_preference mating_display_a mating_display_b group"
    }
}

/// The parts of the world the chamber's entry bookkeeping
/// (`ValidBirthEntry`, `ValidateBirthEntry`, `StoreAsEntry`, `ClearEntry`)
/// and the handlers read: `m_world->GetConfig()`, `GetStats().GetUpdate()`,
/// `ctx.GetRandom()` and the systematics manager for the active references.
pub struct ChamberCtx<'a> {
    pub cfg: &'a Config,
    pub update: i32,
    pub rng: &'a mut AvidaRng,
    pub systematics: &'a mut Manager,
    /// `m_world->GetStats()` as `ExtractParentEnergy` uses it (`None` only
    /// where no world exists, which never stores an entry with energy).
    pub stats: Option<&'a mut Stats>,
}

impl ChamberCtx<'_> {
    /// `cBirthChamber::ValidBirthEntry(entry)`
    pub fn valid_birth_entry(&self, entry: &BirthEntry) -> bool {
        // If there is no organism in the entry, return false.
        if entry.timestamp == -1 {
            return false;
        }
        // If the max_wait_time is -1, there is no timeout, so its alive.
        let max_wait_time = self.cfg.max_birth_wait_time;
        if max_wait_time == -1 {
            return true;
        }
        // Otherwise, check if few enough updates have gone by...
        let max_update = entry.timestamp.wrapping_add(max_wait_time);
        self.update <= max_update
    }

    /// `cBirthChamber::ValidateBirthEntry(entry)`: as `ValidBirthEntry`, but
    /// an entry that timed out is cleared (its active references dropped).
    pub fn validate_birth_entry(&mut self, entry: &mut BirthEntry) -> bool {
        if entry.timestamp == -1 {
            return false;
        }
        let max_wait_time = self.cfg.max_birth_wait_time;
        if max_wait_time == -1 {
            return true;
        }
        let max_update = entry.timestamp.wrapping_add(max_wait_time);
        if self.update > max_update {
            self.clear_entry(entry);
            return false;
        }
        true
    }

    /// `cBirthChamber::StoreAsEntry(offspring, parent, entry)`.
    pub fn store_as_entry(
        &mut self,
        offspring: &Genome,
        parent: &mut Parent<'_>,
        entry: &mut BirthEntry,
    ) {
        entry.genome = offspring.clone();
        if self.cfg.energy_enabled {
            let ph = &mut parent.state.phenotype;
            entry.energy4_offspring = match self.stats.as_deref_mut() {
                Some(stats) => extract_parent_energy(ph, self.cfg, stats),
                None => extract_parent_energy(ph, self.cfg, &mut NoPhenotypeStats),
            };
            entry.merit = Merit::new(
                parent
                    .state
                    .phenotype
                    .convert_energy_to_merit(self.cfg, entry.energy4_offspring),
            );
        } else {
            entry.merit = parent.state.phenotype.merit;
        }
        entry.timestamp = self.update;
        // QUIRK: `entry.groups` is replaced without `RemoveActiveReference`
        // on the groups it held. Only an entry that timed out under
        // `ValidBirthEntry` (which, unlike `ValidateBirthEntry`, does not
        // clear it) still holds groups here; their references leak, so those
        // genotypes never die.
        entry.groups = Some(parent.membership.clone());

        //@CHC: Set all the mating type properties
        let ph = &parent.state.phenotype;
        entry.mating_type = ph.mating_type;
        entry.mating_display_a = ph.last_mating_display_a;
        entry.mating_display_b = ph.last_mating_display_b;
        entry.mate_preference = ph.mate_preference;
        entry.group_id = if parent.state.has_opinion() {
            parent.state.get_opinion().0
        } else {
            self.cfg.default_group
        };

        for &g in parent.membership.groups() {
            self.systematics.add_active_reference(g);
        }
    }

    /// `cBirthChamber::ClearEntry(entry)`
    pub fn clear_entry(&mut self, entry: &mut BirthEntry) {
        entry.timestamp = -1;
        if let Some(groups) = entry.groups.take() {
            for &g in groups.groups() {
                self.systematics.remove_active_reference(g);
            }
        }
    }
}

/// The organism submitting an offspring (`cOrganism* parent`), with what
/// the handlers ask of it and its interface.
pub struct Parent<'a> {
    pub state: &'a mut OrgState,
    /// `parent->SystematicsGroupMembership()`
    pub membership: &'a GroupMembership,
    /// `parent->GetOrgInterface().GetCellID()`
    pub cell_id: i32,
    /// `parent->GetOrgInterface().GetNeighborhoodCellIDs(...)`
    pub neighborhood: Vec<i32>,
    /// `parent->GetDemeID()` (the deme of its cell)
    pub deme_id: i32,
}

// ----------------------------------------------------------------------
// Selection handlers
// ----------------------------------------------------------------------

/// Where a handler keeps the entry it selected (`cBirthEntry*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntrySlot {
    /// `m_entries[i]`
    Index(usize),
    /// The handler's single entry (`cBirthGlobalHandler::m_entry`,
    /// `cBirthMateSelectHandler::m_non_ms_entry`).
    Single,
    /// `m_entries[i]` of deme `d`'s sub-handler (`cBirthDemeHandler`).
    DemeIndex(usize, usize),
    /// The single entry of deme `d`'s sub-handler (`cBirthDemeHandler`).
    DemeSingle(usize),
}

/// `cBirthSelectionHandler` and its subclasses.
#[derive(Debug)]
pub enum SelectionHandler {
    /// `cBirthGlobalHandler`
    Global { entry: BirthEntry },
    /// `cBirthNeighborhoodHandler`
    Neighborhood { entries: Vec<BirthEntry> },
    /// `cBirthGridLocalHandler`
    GridLocal {
        world_x: i32,
        world_y: i32,
        entries: Vec<BirthEntry>,
        /// `m_occupied` (`cBitArray`)
        occupied: Vec<bool>,
    },
    /// `cBirthGenomeSizeHandler`
    GenomeSize { entries: Vec<BirthEntry> },
    /// `cBirthMateSelectHandler`
    MateSelect {
        entries: Vec<BirthEntry>,
        non_ms_entry: BirthEntry,
    },
    /// `cBirthMatingTypeGlobalHandler`
    MatingTypeGlobal { entries: Vec<BirthEntry> },
    /// `cBirthDemeHandler`: `m_deme_handlers`, one sub-handler per deme. The
    /// C++ builds them all in its constructor; they are built here on a
    /// deme's first sexual birth (no RNG or other effect differs).
    Deme {
        world_x: i32,
        world_y: i32,
        handlers: Vec<Option<SelectionHandler>>,
    },
}

fn idx(cell: i32) -> usize {
    usize::try_from(cell).unwrap_or(usize::MAX)
}

impl SelectionHandler {
    /// The handler `cBirthChamber::getSelectionHandler` creates for the
    /// current configuration.
    fn create(cfg: &Config, world_x: i32, world_y: i32) -> Self {
        let birth_method = cfg.birth_method;
        let num_cells = (world_x * world_y).max(0) as usize;
        if cfg.num_demes > 1 {
            // Deme local takes priority, and manages the sub handlers
            let mut handlers = Vec::new();
            handlers.resize_with(cfg.num_demes as usize, || None);
            SelectionHandler::Deme {
                world_x,
                world_y,
                handlers,
            }
        } else if cfg.mating_types {
            // @CHC: If separate mating types are turned on, that takes
            // priority and will manage the sub handlers
            SelectionHandler::MatingTypeGlobal {
                entries: Vec::new(),
            }
        } else if birth_method < po::NUM_LOCAL || birth_method == po::PARENT_FACING {
            // ... else check if the birth method is one of the local ones...
            if cfg.legacy_grid_local_selection {
                Self::grid_local(world_x, world_y)
            } else {
                SelectionHandler::Neighborhood {
                    entries: vec![BirthEntry::default(); num_cells],
                }
            }
        } else if cfg.same_length_sex != 0 {
            // ... else check if recombination must be with organisms of the
            // same length
            SelectionHandler::GenomeSize {
                entries: Vec::new(),
            }
        } else if cfg.allow_mate_selection {
            // ... else check if we have mate selection
            SelectionHandler::MateSelect {
                entries: Vec::new(),
                non_ms_entry: BirthEntry::default(),
            }
        } else if cfg.legacy_grid_local_selection {
            // If everything failed until this point, use default global.
            Self::grid_local(world_x, world_y)
        } else {
            SelectionHandler::Global {
                entry: BirthEntry::default(),
            }
        }
    }

    /// One sub-handler built by `cBirthDemeHandler::cBirthDemeHandler`:
    /// the choice of `getSelectionHandler` without its deme and mating-type
    /// branches.
    fn create_deme_sub(cfg: &Config, world_x: i32, world_y: i32) -> Self {
        let birth_method = cfg.birth_method;
        if birth_method < po::NUM_LOCAL || birth_method == po::PARENT_FACING {
            // ... else check if the birth method is one of the local ones...
            if cfg.legacy_grid_local_selection {
                Self::grid_local(world_x, world_y)
            } else {
                SelectionHandler::Neighborhood {
                    entries: vec![BirthEntry::default(); (world_x * world_y).max(0) as usize],
                }
            }
        } else if cfg.same_length_sex != 0 {
            // ... else check if recombination must be with organisms of the
            // same length
            SelectionHandler::GenomeSize {
                entries: Vec::new(),
            }
        } else if cfg.allow_mate_selection {
            // ... else check if we have mate selection
            SelectionHandler::MateSelect {
                entries: Vec::new(),
                non_ms_entry: BirthEntry::default(),
            }
        } else {
            // If everything failed until this point, use default global.
            // QUIRK: unlike `getSelectionHandler`, the deme handler's global
            // branch ignores LEGACY_GRID_LOCAL_SELECTION.
            SelectionHandler::Global {
                entry: BirthEntry::default(),
            }
        }
    }

    fn grid_local(world_x: i32, world_y: i32) -> Self {
        let n = (world_x * world_y).max(0) as usize;
        SelectionHandler::GridLocal {
            world_x,
            world_y,
            entries: vec![BirthEntry::default(); n],
            occupied: vec![false; n],
        }
    }

    /// Resolves a slot returned by [`SelectionHandler::select_offspring`].
    pub fn entry_mut(&mut self, slot: EntrySlot) -> Option<&mut BirthEntry> {
        match (self, slot) {
            (SelectionHandler::Deme { handlers, .. }, EntrySlot::DemeIndex(d, i)) => handlers
                .get_mut(d)?
                .as_mut()?
                .entry_mut(EntrySlot::Index(i)),
            (SelectionHandler::Deme { handlers, .. }, EntrySlot::DemeSingle(d)) => {
                handlers.get_mut(d)?.as_mut()?.entry_mut(EntrySlot::Single)
            }
            (SelectionHandler::Global { entry }, EntrySlot::Single)
            | (
                SelectionHandler::MateSelect {
                    non_ms_entry: entry,
                    ..
                },
                EntrySlot::Single,
            ) => Some(entry),
            (SelectionHandler::Neighborhood { entries }, EntrySlot::Index(i))
            | (SelectionHandler::GridLocal { entries, .. }, EntrySlot::Index(i))
            | (SelectionHandler::GenomeSize { entries }, EntrySlot::Index(i))
            | (SelectionHandler::MateSelect { entries, .. }, EntrySlot::Index(i))
            | (SelectionHandler::MatingTypeGlobal { entries }, EntrySlot::Index(i)) => {
                entries.get_mut(i)
            }
            _ => None,
        }
    }

    /// `SelectOffspring(ctx, offspring, parent)`: a waiting entry to mate
    /// with, or `None` after storing (or dropping) the offspring. An error
    /// where the C++ crashes.
    pub fn select_offspring(
        &mut self,
        ctx: &mut ChamberCtx<'_>,
        offspring: &Genome,
        parent: &mut Parent<'_>,
    ) -> Result<Option<EntrySlot>, String> {
        Ok(match self {
            SelectionHandler::Global { entry } => {
                if !ctx.valid_birth_entry(entry) {
                    ctx.store_as_entry(offspring, parent, entry);
                    return Ok(None);
                }
                Some(EntrySlot::Single)
            }
            SelectionHandler::Neighborhood { entries } => {
                let parent_id = idx(parent.cell_id);
                // Produce a list of all valid offspring waiting.
                // QUIRK: the C++ `valid` array has one slot per neighbour and
                // the parent's own entry is written one past its end when
                // every neighbour is valid too (a heap overflow that reads
                // back the written value); a growing list matches that.
                let mut valid: Vec<i32> = Vec::with_capacity(parent.neighborhood.len() + 1);
                for &n in &parent.neighborhood {
                    if ctx.valid_birth_entry(&entries[idx(n)]) {
                        valid.push(n);
                    }
                }
                if ctx.valid_birth_entry(&entries[parent_id]) {
                    valid.push(parent.cell_id);
                }
                // If no valid entries exist, store the current offspring
                if valid.is_empty() {
                    ctx.store_as_entry(offspring, parent, &mut entries[parent_id]);
                    return Ok(None);
                }
                // Select a random valid entry and return it
                let choice = ctx.rng.get_uint(valid.len() as u32) as usize;
                Some(EntrySlot::Index(idx(valid[choice])))
            }
            SelectionHandler::GridLocal {
                world_x,
                world_y,
                entries,
                occupied,
            } => {
                let parent_id = parent.cell_id;
                let (wx, wy) = (*world_x, *world_y);
                // hasNeighborWaiting
                let mut waiting = false;
                'search: for i in -1..=1 {
                    for j in -1..=1 {
                        if occupied[idx(grid_neighbor(parent_id, wx, wy, i, j))] {
                            waiting = true;
                            break 'search;
                        }
                    }
                }
                if !waiting {
                    ctx.store_as_entry(offspring, parent, &mut entries[idx(parent_id)]);
                    occupied[idx(parent_id)] = true;
                    return Ok(None);
                }
                // selectRandomNeighbor
                let selected = loop {
                    let test_neighbor = ctx.rng.get_uint(9) as i32;
                    let i = test_neighbor / 3 - 1;
                    let j = test_neighbor % 3 - 1;
                    let s = grid_neighbor(parent_id, wx, wy, i, j);
                    if occupied[idx(s)] {
                        break s;
                    }
                };
                occupied[idx(selected)] = false;
                Some(EntrySlot::Index(idx(selected)))
            }
            SelectionHandler::GenomeSize { entries } => {
                let offspring_length = offspring.representation().size();
                let len = idx(offspring_length);
                // If this is a new largest genome, increase the array size
                if entries.len() <= len {
                    grow_entries(entries, len + 1, "cBirthGenomeSizeHandler")?;
                }
                // Determine if we have an offspring of this length waiting
                if !ctx.valid_birth_entry(&entries[len]) {
                    ctx.store_as_entry(offspring, parent, &mut entries[len]);
                    return Ok(None);
                }
                Some(EntrySlot::Index(len))
            }
            SelectionHandler::MateSelect {
                entries,
                non_ms_entry,
            } => {
                let mate_id = parent.state.phenotype.mate_select_id;
                if mate_id >= 0 {
                    let m = idx(mate_id);
                    // If this is a new mate id, increase the array size
                    if entries.len() <= m {
                        grow_entries(entries, m + 1, "cBirthMateSelectHandler")?;
                    }
                    if !ctx.valid_birth_entry(&entries[m]) {
                        ctx.store_as_entry(offspring, parent, &mut entries[m]);
                        return Ok(None);
                    }
                    return Ok(Some(EntrySlot::Index(m)));
                }
                // Non-mate select offspring handled globally
                if !ctx.valid_birth_entry(non_ms_entry) {
                    ctx.store_as_entry(offspring, parent, non_ms_entry);
                    return Ok(None);
                }
                Some(EntrySlot::Single)
            }
            SelectionHandler::MatingTypeGlobal { entries } => {
                mating_type_select_offspring(entries, ctx, offspring, parent)
            }
            SelectionHandler::Deme {
                world_x,
                world_y,
                handlers,
            } => {
                // `m_deme_handlers[parent->GetDemeID()]->SelectOffspring(...)`
                let d = idx(parent.deme_id);
                let Some(sub) = handlers.get_mut(d) else {
                    return Err(format!(
                        "parent has deme id {} (C++: cBirthDemeHandler::SelectOffspring \
                         indexes m_deme_handlers out of range)",
                        parent.deme_id
                    ));
                };
                let sub =
                    sub.get_or_insert_with(|| Self::create_deme_sub(ctx.cfg, *world_x, *world_y));
                match sub.select_offspring(ctx, offspring, parent)? {
                    Some(EntrySlot::Index(i)) => Some(EntrySlot::DemeIndex(d, i)),
                    Some(EntrySlot::Single) => Some(EntrySlot::DemeSingle(d)),
                    nested => nested,
                }
            }
        })
    }

    /// `GetWaitingOffspringNumber(which_mating_type)`: -1 except for the
    /// mating-type handler.
    pub fn waiting_offspring_number(
        &mut self,
        ctx: &mut ChamberCtx<'_>,
        which_mating_type: i32,
    ) -> i32 {
        let SelectionHandler::MatingTypeGlobal { entries } = self else {
            return -1;
        };
        let mut num_waiting = 0;
        for e in entries.iter_mut() {
            if ctx.validate_birth_entry(e) && e.mating_type == which_mating_type {
                num_waiting += 1;
            }
        }
        num_waiting
    }

    /// The entry lines `PrintBirthChamber(filename)` writes after its header
    /// (only the mating-type handler writes a file; `None` for the others).
    pub fn birth_chamber_lines(&mut self, ctx: &mut ChamberCtx<'_>) -> Option<Vec<String>> {
        let SelectionHandler::MatingTypeGlobal { entries } = self else {
            return None;
        };
        let mut lines = Vec::new();
        for e in entries.iter_mut() {
            if ctx.validate_birth_entry(e) {
                lines.push(e.phenotype_string());
            }
        }
        Some(lines)
    }
}

/// `m_entries.Resize(new_len)` of the genome-size and mate-select handlers.
/// The C++ copies every old entry into the new array, and copying an entry
/// that never held an offspring dereferences its NULL genome representation
/// (`Genome::operator=` in the original, the `std::vector` copy constructor
/// in the reference binary): the reference crashes, so the run stops here.
fn grow_entries(
    entries: &mut Vec<BirthEntry>,
    new_len: usize,
    handler: &str,
) -> Result<(), String> {
    if entries.iter().any(|e| e.genome.hardware_type() < 0) {
        return Err(format!(
            "{handler}::SelectOffspring grows its entry array while it holds an entry that \
             never stored an offspring; the C++ copies that entry's NULL genome and crashes"
        ));
    }
    entries.resize_with(new_len, BirthEntry::default);
    Ok(())
}

/// `cBirthMatingTypeGlobalHandler::SelectOffspring`
fn mating_type_select_offspring(
    entries: &mut Vec<BirthEntry>,
    ctx: &mut ChamberCtx<'_>,
    offspring: &Genome,
    parent: &mut Parent<'_>,
) -> Option<EntrySlot> {
    let parent_sex = parent.state.phenotype.mating_type;
    let mate_choice_method = parent.state.phenotype.mate_preference;
    match parent_sex {
        // Parent has not sexually matured
        mating_type::JUVENILE => None,
        // Parent was a female
        mating_type::FEMALE => mt_select_mate(
            entries,
            ctx,
            offspring,
            parent,
            mating_type::MALE,
            mate_choice_method,
        ),
        mating_type::MALE => {
            if ctx.cfg.lekking {
                // Lekking is turned on, so if the parent is a male, force
                // the offspring into the birth chamber, and wait for a female
                mt_store_offspring(entries, ctx, offspring, parent);
                None
            } else {
                mt_select_mate(
                    entries,
                    ctx,
                    offspring,
                    parent,
                    mating_type::FEMALE,
                    mate_choice_method,
                )
            }
        }
        _ => None,
    }
}

/// `cBirthMatingTypeGlobalHandler::storeOffspring`
fn mt_store_offspring(
    entries: &mut Vec<BirthEntry>,
    ctx: &mut ChamberCtx<'_>,
    offspring: &Genome,
    parent: &mut Parent<'_>,
) {
    // First, don't bother doing ANYTHING if LEKKING is turned on and the
    // parent is a female
    if ctx.cfg.lekking && parent.state.phenotype.mating_type == mating_type::FEMALE {
        return;
    }

    // Find an empty entry; if there are none, make room for one; but if the
    // birth chamber is at the size limit already, over-write the oldest one
    let mut oldest_index: i32 = -1;
    let entry_list_size = entries.len();
    for i in 0..entry_list_size {
        if ctx.validate_birth_entry(&mut entries[i]) {
            // Current entry is valid, so keep track of the oldest one
            if oldest_index == -1 || entries[i].timestamp < entries[oldest_index as usize].timestamp
            {
                oldest_index = i as i32;
            }
        } else {
            // Current entry is empty, so let's use this one
            ctx.clear_entry(&mut entries[i]);
            ctx.store_as_entry(offspring, parent, &mut entries[i]);
            return;
        }
    }

    let mut store_index = entries.len() as i32;
    let max_buffer_size = ctx.cfg.max_global_birth_chamber_size;
    if store_index >= max_buffer_size {
        store_index = oldest_index;
    } else {
        entries.resize_with(store_index as usize + 1, BirthEntry::default);
    }
    // QUIRK: with MAX_GLOBAL_BIRTH_CHAMBER_SIZE <= 0 and an empty chamber the
    // C++ indexes entry -1 (undefined); the offspring is dropped here.
    let Ok(si) = usize::try_from(store_index) else {
        return;
    };
    ctx.clear_entry(&mut entries[si]);
    ctx.store_as_entry(offspring, parent, &mut entries[si]);
}

/// `cBirthMatingTypeGlobalHandler::compareBirthEntries`: true if `entry1`
/// is preferred.
fn mt_compare_birth_entries(
    ctx: &mut ChamberCtx<'_>,
    mate_choice_method: i32,
    entry1: &BirthEntry,
    entry2: &BirthEntry,
) -> bool {
    let cv = ctx.cfg.mate_assessment_cv;
    let noisy = ctx.cfg.noisy_mate_assessment;
    let (mut value1, mut value2, highest) = match mate_choice_method {
        mate_preference::HIGHEST_DISPLAY_A => (
            f64::from(entry1.mating_display_a),
            f64::from(entry2.mating_display_a),
            true,
        ),
        mate_preference::HIGHEST_DISPLAY_B => (
            f64::from(entry1.mating_display_b),
            f64::from(entry2.mating_display_b),
            true,
        ),
        mate_preference::HIGHEST_MERIT => {
            (entry1.merit.get_double(), entry2.merit.get_double(), true)
        }
        mate_preference::LOWEST_DISPLAY_A => (
            f64::from(entry1.mating_display_a),
            f64::from(entry2.mating_display_a),
            false,
        ),
        mate_preference::LOWEST_DISPLAY_B => (
            f64::from(entry1.mating_display_b),
            f64::from(entry2.mating_display_b),
            false,
        ),
        mate_preference::LOWEST_MERIT => {
            (entry1.merit.get_double(), entry2.merit.get_double(), false)
        }
        // If we're still here... just decide randomly since we need to give
        // some return value...
        _ => return ctx.rng.p(0.5),
    };
    if noisy {
        value1 += ctx.rng.get_rand_normal_params(0.0, value1 * cv);
        value2 += ctx.rng.get_rand_normal_params(0.0, value2 * cv);
    }
    if highest {
        value1 > value2
    } else {
        value1 < value2
    }
}

/// `cBirthMatingTypeGlobalHandler::selectMate`
fn mt_select_mate(
    entries: &mut Vec<BirthEntry>,
    ctx: &mut ChamberCtx<'_>,
    offspring: &Genome,
    parent: &mut Parent<'_>,
    which_mating_type: i32,
    mut mate_choice_method: i32,
) -> Option<EntrySlot> {
    // Loop through the entry list and find a mate; if none are found,
    // store the current offspring and return NULL
    let num_waiting = entries.len();
    if num_waiting == 0 {
        mt_store_offspring(entries, ctx, offspring, parent);
        return None;
    }

    let mut selected_index: Option<usize> = None;
    if ctx.cfg.forced_mate_preference != -1 {
        mate_choice_method = ctx.cfg.forced_mate_preference;
    }

    // Here, check to see if the current entry belongs to the parent's
    // group! @CHC
    let mate_in_groups = ctx.cfg.mate_in_groups;
    let groups_match = |parent: &mut Parent<'_>, e: &BirthEntry| {
        if !mate_in_groups {
            true
        } else if parent.state.has_opinion() {
            e.group_id == parent.state.get_opinion().0
        } else {
            false
        }
    };

    if mate_choice_method == mate_preference::RANDOM {
        // This is a non-choosy individual, so pick a mate randomly!
        let mut compatible_entries: Vec<usize> = Vec::with_capacity(num_waiting);
        for (i, e) in entries.iter_mut().enumerate() {
            if ctx.validate_birth_entry(e)
                && groups_match(parent, e)
                && e.mating_type == which_mating_type
            {
                compatible_entries.push(i);
            }
        }
        if !compatible_entries.is_empty() {
            let choice = ctx.rng.get_uint(compatible_entries.len() as u32) as usize;
            selected_index = Some(compatible_entries[choice]);
        }
    } else {
        // This is a choosy female, so go through all the mates and pick the
        // "best" one!
        for i in 0..num_waiting {
            if ctx.validate_birth_entry(&mut entries[i])
                && groups_match(parent, &entries[i])
                && entries[i].mating_type == which_mating_type
            {
                selected_index = match selected_index {
                    None => Some(i),
                    Some(s) => {
                        if mt_compare_birth_entries(
                            ctx,
                            mate_choice_method,
                            &entries[i],
                            &entries[s],
                        ) {
                            Some(i)
                        } else {
                            Some(s)
                        }
                    }
                };
            }
        }
    }

    match selected_index {
        None => {
            // None found: Store the current one and return NULL
            mt_store_offspring(entries, ctx, offspring, parent);
            None
        }
        Some(s) => Some(EntrySlot::Index(s)),
    }
}

// ----------------------------------------------------------------------
// The chamber
// ----------------------------------------------------------------------

/// `cBirthChamber`: one selection handler per hardware type, created on
/// first use (`getSelectionHandler`).
#[derive(Debug, Default)]
pub struct BirthChamber {
    /// `m_handler_map`
    handlers: BTreeMap<i32, SelectionHandler>,
}

impl BirthChamber {
    /// `getSelectionHandler(hw_type)`
    pub fn selection_handler(
        &mut self,
        cfg: &Config,
        world_x: i32,
        world_y: i32,
        hw_type: i32,
    ) -> &mut SelectionHandler {
        self.handlers
            .entry(hw_type)
            .or_insert_with(|| SelectionHandler::create(cfg, world_x, world_y))
    }
}

/// One offspring built by `SubmitOffspring`: the organism, its
/// `merit_array` entry and its systematics groups.
pub struct Offspring {
    pub org: Organism,
    pub merit: Merit,
    pub membership: GroupMembership,
}

impl World {
    /// `cBirthChamber::GetWaitingOffspringNumber(which_mating_type, hw_type)`
    pub fn birth_chamber_waiting_offspring_number(
        &mut self,
        which_mating_type: i32,
        hw_type: i32,
    ) -> i32 {
        let World {
            cfg,
            stats,
            rng,
            systematics,
            pop,
            ..
        } = self;
        let mut ctx = ChamberCtx {
            cfg,
            update: stats.update(),
            rng,
            systematics,
            stats: Some(stats),
        };
        pop.birth_chamber
            .selection_handler(cfg, pop.world_x, pop.world_y, hw_type)
            .waiting_offspring_number(&mut ctx, which_mating_type)
    }

    /// The entry lines of `cBirthChamber::PrintBirthChamber(filename,
    /// hw_type)`; the caller writes the time stamp, the comment
    /// [`BirthEntry::phenotype_string_format`] and a blank line before them.
    /// `None` when the handler prints nothing.
    pub fn birth_chamber_lines(&mut self, hw_type: i32) -> Option<Vec<String>> {
        let World {
            cfg,
            stats,
            rng,
            systematics,
            pop,
            ..
        } = self;
        let mut ctx = ChamberCtx {
            cfg,
            update: stats.update(),
            rng,
            systematics,
            stats: Some(stats),
        };
        pop.birth_chamber
            .selection_handler(cfg, pop.world_x, pop.world_y, hw_type)
            .birth_chamber_lines(&mut ctx)
    }

    /// `new cOrganism(m_world, ctx, genome, parent_generation,
    /// Systematics::Source(Systematics::DIVISION, ""))`
    fn new_offspring_org(
        &mut self,
        genome: &Genome,
        parent_generation: i32,
    ) -> Result<Organism, String> {
        let src = OrgSource::new(transmission::DIVISION, "", false);
        Organism::new(self, genome, parent_generation, src)
    }

    /// The organism a burst built for the offspring (parallel mode, see
    /// [`World::commit_divide`]) as `new cOrganism` would have returned it
    /// here: numbered by the world, and with the generation of a parent that
    /// has reset (`DivideReset` ran since the burst built it).
    fn adopt_prebuilt_org(&mut self, mut org: Organism, parent_generation: i32) -> Organism {
        org.state.id = self.stats.tot_organisms;
        org.state.uid = avida_hardware::org_interface::WorldServices::new_org_uid(self);
        if parent_generation >= 0 {
            let mut g = parent_generation;
            if self.cfg.generation_inc_method != avida_phenotype::phenotype::GENERATION_INC_BOTH {
                g += 1;
            }
            org.state.phenotype.generation = g;
        }
        org
    }

    /// `SetupGenotypeInfo(organism, p0grps, p1grps)` / `SelfClassify`
    fn classify_offspring(
        &mut self,
        org: &Organism,
        parents: &[&GroupMembership],
    ) -> GroupMembership {
        let mut membership = GroupMembership::new();
        let source = systematics_source(&org.state.src);
        let unit = UnitInfo {
            source: &source,
            genome: org.state.genome(),
            generation: org.state.phenotype.generation,
        };
        self.systematics
            .self_classify(&unit, parents, &mut membership);
        membership
    }

    /// `cBirthChamber::SubmitOffspring(ctx, offspring, parent, child_array,
    /// merit_array)`. The parent lives in `parent_cell` and belongs to
    /// `parent_membership`. An empty list means the offspring was stored in
    /// (or, for a juvenile under `MATING_TYPES`, dropped by) the chamber.
    pub fn submit_offspring(
        &mut self,
        offspring: &Genome,
        parent: &mut OrgState,
        parent_cell: i32,
        parent_membership: &GroupMembership,
    ) -> Result<Vec<Offspring>, String> {
        let parent_generation = parent.phenotype.generation;

        if !parent.phenotype.divide_sex {
            // `DoAsexBirth`: this is asexual who doesn't need to wait in the
            // birth chamber; just build the child and return.
            let mut org = match self.pop.prebuilt_offspring.take() {
                Some(built) => self.adopt_prebuilt_org(*built, parent_generation),
                None => self.new_offspring_org(offspring, parent_generation)?,
            };
            let merit = if self.cfg.energy_enabled {
                // calculate energy to be given to child
                let child_energy =
                    extract_parent_energy(&mut parent.phenotype, &self.cfg, &mut self.stats);
                // set child energy & merit
                let merit = set_child_energy(&mut org, &self.cfg, child_energy);
                if merit.get_double() <= 0.0 {
                    // do not allow zero merit: the child is deleted before
                    // it is classified.
                    return Ok(Vec::new());
                }
                merit
            } else if self.cfg.inherit_merit != 0 {
                parent.phenotype.merit
            } else {
                Merit::new(f64::from(parent.phenotype.calc_size_merit(&self.cfg)))
            };
            let membership = self.classify_offspring(&org, &[parent_membership]);
            return Ok(vec![Offspring {
                org,
                merit,
                membership,
            }]);
        }

        // If we make it this far, this must be a sexual or a "waiting"
        // asexual organism (which is the same as sexual with 0
        // recombination points). Find a waiting entry (locally or globally).
        let hw_type = offspring.hardware_type();
        let neighborhood: Vec<i32> = self.pop.cells[idx(parent_cell)]
            .connections
            .iter()
            .collect();
        let deme_id = self.pop.cells[idx(parent_cell)].deme_id;
        let slot = {
            let World {
                cfg,
                stats,
                rng,
                systematics,
                pop,
                ..
            } = self;
            let mut ctx = ChamberCtx {
                cfg,
                update: stats.update(),
                rng,
                systematics,
                stats: Some(stats),
            };
            let mut p = Parent {
                state: parent,
                membership: parent_membership,
                cell_id: parent_cell,
                neighborhood,
                deme_id,
            };
            pop.birth_chamber
                .selection_handler(cfg, pop.world_x, pop.world_y, hw_type)
                .select_offspring(&mut ctx, offspring, &mut p)?
        };

        // If we couldn't find a waiting entry, this one was saved -- stop
        // here!
        let Some(slot) = slot else {
            return Ok(Vec::new());
        };
        let (old_genome, old_merit, old_groups, old_energy) = {
            let (cfg, wx, wy) = (&self.cfg, self.pop.world_x, self.pop.world_y);
            let entry = self
                .pop
                .birth_chamber
                .selection_handler(cfg, wx, wy, hw_type)
                .entry_mut(slot)
                .ok_or("birth chamber returned a missing entry")?;
            (
                entry.genome.clone(),
                entry.merit,
                // A selected entry always holds its groups (a NULL pointer
                // would be skipped by `SetupGenotypeInfo`).
                entry.groups.clone(),
                entry.energy4_offspring,
            )
        };
        let old_groups_ref: Vec<&GroupMembership> = old_groups.iter().collect();

        // If we are NOT recombining, handle that here.
        let children = if parent.phenotype.cross_num == 0
            || self.rng.get_double() > self.cfg.recombination_prob
        {
            // `DoPairAsexBirth`: build both child organisms... QUIRK: with
            // ENERGY_ENABLED neither child is given energy here (their
            // stores stay 0); the merits are the stored entry's and the
            // parent's current one.
            let org0 = self.new_offspring_org(&old_genome, parent_generation)?;
            let org1 = self.new_offspring_org(offspring, parent_generation)?;
            let merit1 = parent.phenotype.merit;
            // Setup the genotypes for both children...
            let m0 = self.classify_offspring(&org0, &old_groups_ref);
            let m1 = self.classify_offspring(&org1, &[parent_membership]);
            vec![
                Offspring {
                    org: org0,
                    merit: old_merit,
                    membership: m0,
                },
                Offspring {
                    org: org1,
                    merit: merit1,
                    membership: m1,
                },
            ]
        } else {
            // If we made it this far, RECOMBINATION will happen!
            let mut genome0 = old_genome;
            let mut genome1 = offspring.clone();
            let energy_enabled = self.cfg.energy_enabled;
            // `meritOrEnergy0` / `meritOrEnergy1`
            let (mut merit0, mut merit1) = if energy_enabled {
                let energy1 =
                    extract_parent_energy(&mut parent.phenotype, &self.cfg, &mut self.stats);
                (old_energy, energy1)
            } else {
                (old_merit.get_double(), parent.phenotype.merit.get_double())
            };
            // Check the modular recombination settings: MODULE_NUM (0 =
            // non-modular), CONT_REC_REGS and CORESPOND_REC_REGS.
            recombine(
                &mut self.rng,
                self.cfg.module_num,
                self.cfg.cont_rec_regs,
                self.cfg.corespond_rec_regs != 0,
                genome0.representation_mut(),
                genome1.representation_mut(),
                &mut merit0,
                &mut merit1,
            );

            let mut parents01: Vec<&GroupMembership> = old_groups_ref.clone();
            parents01.push(parent_membership);
            let mut parents10: Vec<&GroupMembership> = vec![parent_membership];
            parents10.extend(old_groups_ref.iter().copied());

            // Should there be a 2-fold cost to sex?
            if self.cfg.two_fold_cost_sex == 0 {
                // Build the two organisms.
                let mut org0 = self.new_offspring_org(&genome0, parent_generation)?;
                let mut org1 = self.new_offspring_org(&genome1, parent_generation)?;
                if energy_enabled {
                    merit0 = set_child_energy(&mut org0, &self.cfg, merit0).get_double();
                    merit1 = set_child_energy(&mut org1, &self.cfg, merit1).get_double();
                }
                let m0 = self.classify_offspring(&org0, &parents01);
                let m1 = self.classify_offspring(&org1, &parents10);
                vec![
                    Offspring {
                        org: org0,
                        merit: Merit::new(merit0),
                        membership: m0,
                    },
                    Offspring {
                        org: org1,
                        merit: Merit::new(merit1),
                        membership: m1,
                    },
                ]
            } else if self.rng.get_double() < 0.5 {
                // Build only one organism
                let mut org = self.new_offspring_org(&genome0, parent_generation)?;
                if energy_enabled {
                    merit0 = set_child_energy(&mut org, &self.cfg, merit0).get_double();
                }
                let m = self.classify_offspring(&org, &parents01);
                vec![Offspring {
                    org,
                    merit: Merit::new(merit0),
                    membership: m,
                }]
            } else {
                if energy_enabled {
                    // The C++ sets the energy of `child_array[0]` and then
                    // reads `child_array[1]` of the one-element array.
                    return Err(
                        "unsupported: TWO_FOLD_COST_SEX with ENERGY_ENABLED keeping the \
                         second genome (C++: cBirthChamber::SubmitOffspring reads \
                         child_array[1] out of range)"
                            .to_string(),
                    );
                }
                let org = self.new_offspring_org(&genome1, parent_generation)?;
                let m = self.classify_offspring(&org, &parents10);
                vec![Offspring {
                    org,
                    merit: Merit::new(merit1),
                    membership: m,
                }]
            }
        };

        // `ClearEntry(*old_entry)`
        let World {
            cfg,
            stats,
            rng,
            systematics,
            pop,
            ..
        } = self;
        let mut ctx = ChamberCtx {
            cfg,
            update: stats.update(),
            rng,
            systematics,
            stats: Some(stats),
        };
        if let Some(entry) = pop
            .birth_chamber
            .selection_handler(cfg, pop.world_x, pop.world_y, hw_type)
            .entry_mut(slot)
        {
            ctx.clear_entry(entry);
        }
        Ok(children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(s: &str) -> InstructionSequence {
        InstructionSequence::from_string(s)
    }

    #[test]
    fn region_swap_exchanges_equal_and_unequal_regions() {
        let mut a = seq("aaaaaaaaaa");
        let mut b = seq("bbbbbbbbbbbb");
        assert!(region_swap(&mut a, &mut b, 2, 4, 3, 8));
        assert_eq!(a.as_string(), "aabbbbbaaaaaa");
        assert_eq!(b.as_string(), "bbbaabbbb");
    }

    #[test]
    fn region_swap_refuses_illegal_lengths() {
        let mut a = seq("aaaaaaaa");
        let mut b = seq("bbbbbbbbbbbb");
        // a would shrink to 6 < MIN_GENOME_LENGTH.
        assert!(!region_swap(&mut a, &mut b, 0, 3, 0, 1));
        assert_eq!(a.as_string(), "aaaaaaaa");
        assert_eq!(b.as_string(), "bbbbbbbbbbbb");
    }

    #[test]
    fn region_swap_with_one_empty_region_copies_one_way() {
        let mut a = seq("aaaaaaaaaa");
        let mut b = seq("bbbbbbbbbb");
        assert!(region_swap(&mut a, &mut b, 2, 4, 5, 5));
        assert_eq!(a.as_string(), "aaaaaaaaaa");
        assert_eq!(b.as_string(), "bbbbbaabbbbb");
    }

    #[test]
    fn basic_recombination_conserves_merit_and_material() {
        let mut rng = AvidaRng::new(7);
        let mut a = seq(&"a".repeat(100));
        let mut b = seq(&"b".repeat(100));
        let (mut m0, mut m1) = (10.0, 30.0);
        do_basic_recombination(&mut rng, &mut a, &mut b, &mut m0, &mut m1);
        assert!((m0 + m1 - 40.0).abs() < 1e-9);
        let count = |s: &InstructionSequence, c: char| s.as_string().matches(c).count();
        assert_eq!(count(&a, 'a') + count(&b, 'a'), 100);
        assert_eq!(a.size() + b.size(), 200);
        // Two draws were consumed.
        let mut fresh = AvidaRng::new(7);
        fresh.get_double();
        fresh.get_double();
        assert_eq!(rng.get_double(), fresh.get_double());
    }

    #[test]
    fn modular_kernels_swap_whole_modules() {
        let mut rng = AvidaRng::new(3);
        let mut a = seq(&"a".repeat(40));
        let mut b = seq(&"b".repeat(40));
        let (mut m0, mut m1) = (1.0, 3.0);
        do_modular_non_cont_recombination(&mut rng, 4, &mut a, &mut b, &mut m0, &mut m1);
        for chunk in a.as_string().as_bytes().chunks(10) {
            assert!(chunk.iter().all(|c| *c == chunk[0]));
        }
        assert!((m0 + m1 - 4.0).abs() < 1e-12);

        let mut a = seq(&"a".repeat(40));
        let mut b = seq(&"b".repeat(40));
        do_modular_shuffle_recombination(&mut rng, 4, &mut a, &mut b, &mut m0, &mut m1);
        assert_eq!(a.size(), 40);
        recombine(&mut rng, 4, 1, true, &mut a, &mut b, &mut m0, &mut m1);
        assert_eq!(a.size() + b.size(), 80);
    }

    // ------------------------------------------------------------------
    // Entries, handlers and SubmitOffspring on a world built from the
    // `sex` consistency test's configuration (copied to a temp dir).
    // ------------------------------------------------------------------

    use std::path::Path;

    fn sex_world(tag: &str, tweak: impl FnOnce(&mut Config)) -> World {
        let src =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/tests/sex/config");
        let dir = std::env::temp_dir().join(format!("avida-bc-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for e in std::fs::read_dir(&src).unwrap() {
            let e = e.unwrap();
            std::fs::copy(e.path(), dir.join(e.file_name())).unwrap();
        }
        let wd = dir.to_string_lossy().into_owned();
        let mut cfg = Config::default();
        assert!(cfg.load("avida.cfg", &wd, None, None, false));
        tweak(&mut cfg);
        let mut w = World::new(cfg, &wd, &[]).unwrap().0;
        // Before the first update the update is -1, which an entry's
        // timestamp cannot be told apart from "empty".
        w.stats.set_current_update(0);
        w
    }

    fn genome(w: &World, s: &str) -> Genome {
        let is = w.hw.registry().shared_inst_set(0).unwrap();
        Genome::with_inst_set(0, is.inst_set_name(), InstructionSequence::from_string(s))
    }

    /// Injects `s` into `cell` and takes the organism out, as `ProcessStep`
    /// does, with its groups.
    fn parent(w: &mut World, s: &str, cell: i32) -> (Organism, GroupMembership) {
        let g = genome(w, s);
        w.inject(
            &g,
            OrgSource::new(transmission::DIVISION, "", false),
            cell,
            10.0,
            0,
            0.0,
        )
        .unwrap();
        let org = w.pop.orgs[cell as usize].take().unwrap();
        let m = w.pop.membership[cell as usize].clone();
        (*org, m)
    }

    fn global_entry(w: &mut World) -> BirthEntry {
        let (wx, wy) = (w.pop.world_x, w.pop.world_y);
        match w.pop.birth_chamber.selection_handler(&w.cfg, wx, wy, 0) {
            SelectionHandler::Global { entry } => entry.clone(),
            h => panic!("expected the global handler, got {h:?}"),
        }
    }

    #[test]
    fn handler_follows_the_configuration() {
        let mut cfg = Config::default();
        let h = |cfg: &Config| SelectionHandler::create(cfg, 4, 3);
        cfg.birth_method = po::AGE;
        assert!(
            matches!(h(&cfg), SelectionHandler::Neighborhood { entries } if entries.len() == 12)
        );
        cfg.legacy_grid_local_selection = true;
        assert!(matches!(h(&cfg), SelectionHandler::GridLocal { .. }));
        cfg.birth_method = po::FULL_SOUP_RANDOM;
        assert!(matches!(h(&cfg), SelectionHandler::GridLocal { .. }));
        cfg.legacy_grid_local_selection = false;
        assert!(matches!(h(&cfg), SelectionHandler::Global { .. }));
        cfg.allow_mate_selection = true;
        assert!(matches!(h(&cfg), SelectionHandler::MateSelect { .. }));
        cfg.same_length_sex = 1;
        assert!(matches!(h(&cfg), SelectionHandler::GenomeSize { .. }));
        cfg.mating_types = true;
        assert!(matches!(h(&cfg), SelectionHandler::MatingTypeGlobal { .. }));
        // Demes take priority over mating types; the sub-handlers ignore
        // LEGACY_GRID_LOCAL_SELECTION for global birth methods.
        cfg.num_demes = 3;
        assert!(matches!(h(&cfg), SelectionHandler::Deme { handlers, .. } if handlers.len() == 3));
        let sub = |cfg: &Config| SelectionHandler::create_deme_sub(cfg, 4, 3);
        assert!(matches!(sub(&cfg), SelectionHandler::GenomeSize { .. }));
        cfg.same_length_sex = 0;
        assert!(matches!(sub(&cfg), SelectionHandler::MateSelect { .. }));
        cfg.allow_mate_selection = false;
        cfg.legacy_grid_local_selection = true;
        assert!(matches!(sub(&cfg), SelectionHandler::Global { .. }));
        cfg.birth_method = po::PARENT_FACING;
        assert!(matches!(sub(&cfg), SelectionHandler::GridLocal { .. }));
        cfg.legacy_grid_local_selection = false;
        assert!(matches!(sub(&cfg), SelectionHandler::Neighborhood { .. }));
    }

    #[test]
    fn deme_handler_mates_only_within_a_deme() {
        let mut w = sex_world("deme", |cfg| {
            cfg.num_demes = 2;
            cfg.mating_types = true;
        });
        let last = w.pop.world_x * w.pop.world_y - 1;
        assert_eq!(w.pop.cells[last as usize].deme_id, 1);
        let (mut p, m) = parent(&mut w, &"a".repeat(50), 0);
        p.state.phenotype.divide_sex = true;
        p.state.phenotype.cross_num = 0;
        let g = |w: &World, c: &str| genome(w, &c.repeat(40));
        // Mating types are ignored: a juvenile's offspring waits.
        assert!(w
            .submit_offspring(&g(&w, "o"), &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        // A parent in deme 1 does not see deme 0's entry.
        assert!(w
            .submit_offspring(&g(&w, "p"), &mut p.state, last, &m)
            .unwrap()
            .is_empty());
        let kids = w
            .submit_offspring(&g(&w, "q"), &mut p.state, 1, &m)
            .unwrap();
        assert_eq!(
            kids[0].org.state.genome().representation().as_string(),
            "o".repeat(40)
        );
        let kids2 = w
            .submit_offspring(&g(&w, "r"), &mut p.state, last - 1, &m)
            .unwrap();
        assert_eq!(
            kids2[0].org.state.genome().representation().as_string(),
            "p".repeat(40)
        );
        assert_eq!(w.birth_chamber_waiting_offspring_number(0, 0), -1);
        assert_eq!(w.birth_chamber_lines(0), None);
        for k in kids.iter().chain(&kids2) {
            w.systematics.remove_unit(&k.membership);
        }
    }

    #[test]
    fn entries_time_out_after_max_birth_wait_time() {
        let mut cfg = Config::default();
        let mut rng = AvidaRng::new(1);
        let mut systematics = Manager::new();
        let mut entry = BirthEntry {
            timestamp: 5,
            ..BirthEntry::default()
        };
        let ctx = ChamberCtx {
            cfg: &cfg,
            update: 9,
            rng: &mut rng,
            systematics: &mut systematics,
            stats: None,
        };
        assert!(ctx.valid_birth_entry(&entry));
        assert!(!ctx.valid_birth_entry(&BirthEntry::default()));
        cfg.max_birth_wait_time = 3;
        let mut ctx = ChamberCtx {
            cfg: &cfg,
            update: 8,
            rng: &mut rng,
            systematics: &mut systematics,
            stats: None,
        };
        assert!(ctx.validate_birth_entry(&mut entry));
        ctx.update = 9;
        assert!(!ctx.valid_birth_entry(&entry));
        assert_eq!(entry.timestamp, 5);
        assert!(!ctx.validate_birth_entry(&mut entry));
        assert_eq!(entry.timestamp, -1);
    }

    #[test]
    fn phenotype_string_lists_the_entry_fields() {
        let e = BirthEntry {
            genome: Genome::with_inst_set(0, "x", InstructionSequence::from_string("abc")),
            timestamp: 7,
            merit: Merit::new(2.5),
            ..BirthEntry::default()
        };
        assert_eq!(e.phenotype_string(), "abc 7 2.500000 -1 0 0 0 -1");
        assert_eq!(BirthEntry::phenotype_string_format().split(' ').count(), 8);
    }

    #[test]
    fn global_handler_stores_then_recombines_two_offspring() {
        let mut w = sex_world("global", |_| {});
        let (mut p, m) = parent(&mut w, &"a".repeat(50), 0);
        p.state.phenotype.divide_sex = true;
        p.state.phenotype.cross_num = 1;

        // The first sexual offspring waits in the chamber.
        let g1 = genome(&w, &"a".repeat(50));
        let kids = w.submit_offspring(&g1, &mut p.state, 0, &m).unwrap();
        assert!(kids.is_empty());
        let e = global_entry(&mut w);
        assert_eq!(e.timestamp, w.stats.update());
        assert_eq!(e.groups.as_ref(), Some(&m));
        assert_eq!(e.genome.representation().as_string(), "a".repeat(50));

        // The second one mates with it; the entry is cleared.
        let g2 = genome(&w, &"b".repeat(60));
        let kids = w.submit_offspring(&g2, &mut p.state, 0, &m).unwrap();
        assert_eq!(kids.len(), 2);
        let total: i32 = kids
            .iter()
            .map(|k| k.org.state.genome().representation().size())
            .sum();
        assert_eq!(total, 110);
        let merit_sum: f64 = kids.iter().map(|k| k.merit.get_double()).sum();
        assert!((merit_sum - 2.0 * p.state.phenotype.merit.get_double()).abs() < 1e-9);
        // Both offspring are built before either is born (same id), but
        // they are two organisms.
        assert_eq!(kids[0].org.state.id, kids[1].org.state.id);
        assert_ne!(kids[0].org.state.uid, kids[1].org.state.uid);
        assert_ne!(
            kids[0].org.state.uid,
            avida_hardware::organism::OrgUid::NONE
        );
        let e = global_entry(&mut w);
        assert_eq!(e.timestamp, -1);
        assert!(e.groups.is_none());
        for k in &kids {
            w.systematics.remove_unit(&k.membership);
        }
    }

    #[test]
    fn without_crossover_the_pair_is_born_unchanged() {
        let mut w = sex_world("pair", |_| {});
        let (mut p, m) = parent(&mut w, &"a".repeat(50), 0);
        p.state.phenotype.divide_sex = true;
        p.state.phenotype.cross_num = 0;
        let g1 = genome(&w, &"c".repeat(40));
        assert!(w
            .submit_offspring(&g1, &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        let g2 = genome(&w, &"d".repeat(45));
        let kids = w.submit_offspring(&g2, &mut p.state, 0, &m).unwrap();
        let seqs: Vec<String> = kids
            .iter()
            .map(|k| k.org.state.genome().representation().as_string())
            .collect();
        assert_eq!(seqs, vec!["c".repeat(40), "d".repeat(45)]);
        assert_eq!(
            kids[0].merit.get_double(),
            p.state.phenotype.merit.get_double()
        );
    }

    #[test]
    fn mating_types_pair_females_with_males() {
        let mut w = sex_world("mt", |cfg| cfg.mating_types = true);
        let (mut p, m) = parent(&mut w, &"a".repeat(50), 0);
        p.state.phenotype.divide_sex = true;
        p.state.phenotype.cross_num = 1;
        let g = genome(&w, &"a".repeat(50));

        // Juveniles' offspring are dropped.
        assert!(w
            .submit_offspring(&g, &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        assert_eq!(
            w.birth_chamber_waiting_offspring_number(mating_type::JUVENILE, 0),
            0
        );
        assert_eq!(w.birth_chamber_lines(0), Some(Vec::new()));

        // Two females wait; a male mates with one of them.
        p.state.phenotype.mating_type = mating_type::FEMALE;
        assert!(w
            .submit_offspring(&g, &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        assert!(w
            .submit_offspring(&g, &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        assert_eq!(
            w.birth_chamber_waiting_offspring_number(mating_type::FEMALE, 0),
            2
        );
        p.state.phenotype.mating_type = mating_type::MALE;
        let kids = w.submit_offspring(&g, &mut p.state, 0, &m).unwrap();
        assert_eq!(kids.len(), 2);
        assert_eq!(
            w.birth_chamber_waiting_offspring_number(mating_type::FEMALE, 0),
            1
        );
        assert_eq!(
            w.birth_chamber_waiting_offspring_number(mating_type::MALE, 0),
            0
        );
        assert_eq!(w.birth_chamber_lines(0).map(|l| l.len()), Some(1));

        // With lekking, a male's offspring waits and a female's is dropped
        // when no male is waiting.
        w.cfg.lekking = true;
        assert!(w
            .submit_offspring(&g, &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        assert_eq!(
            w.birth_chamber_waiting_offspring_number(mating_type::MALE, 0),
            1
        );
        for k in &kids {
            w.systematics.remove_unit(&k.membership);
        }
    }

    #[test]
    fn choosy_females_pick_the_highest_display() {
        let mut w = sex_world("choosy", |cfg| cfg.mating_types = true);
        let (mut p, m) = parent(&mut w, &"a".repeat(50), 0);
        p.state.phenotype.divide_sex = true;
        p.state.phenotype.cross_num = 0;
        p.state.phenotype.mating_type = mating_type::MALE;
        for (len, display) in [(41, 3), (42, 9), (43, 5)] {
            p.state.phenotype.last_mating_display_a = display;
            let g = genome(&w, &"e".repeat(len));
            assert!(w
                .submit_offspring(&g, &mut p.state, 0, &m)
                .unwrap()
                .is_empty());
        }
        p.state.phenotype.mating_type = mating_type::FEMALE;
        p.state.phenotype.mate_preference = mate_preference::HIGHEST_DISPLAY_A;
        let g = genome(&w, &"f".repeat(50));
        let kids = w.submit_offspring(&g, &mut p.state, 0, &m).unwrap();
        assert_eq!(kids[0].org.state.genome().representation().size(), 42);
    }

    #[test]
    fn genome_size_handler_mates_equal_lengths_only() {
        let mut w = sex_world("size", |cfg| cfg.same_length_sex = 1);
        let (mut p, m) = parent(&mut w, &"a".repeat(50), 0);
        p.state.phenotype.divide_sex = true;
        p.state.phenotype.cross_num = 0;
        let g40 = genome(&w, &"g".repeat(40));
        let g41 = genome(&w, &"h".repeat(41));
        // The longest genome comes first: the C++ array never grows again.
        assert!(w
            .submit_offspring(&g41, &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        assert!(w
            .submit_offspring(&g40, &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        let kids = w.submit_offspring(&g40, &mut p.state, 0, &m).unwrap();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].org.state.genome().representation().size(), 40);
        // A longer genome grows the array over never-used entries, which
        // crashes the C++.
        let g42 = genome(&w, &"s".repeat(42));
        let err = w.submit_offspring(&g42, &mut p.state, 0, &m).err().unwrap();
        assert!(err.contains("cBirthGenomeSizeHandler"), "{err}");
        for k in &kids {
            w.systematics.remove_unit(&k.membership);
        }
    }

    #[test]
    fn mate_select_handler_pairs_equal_mate_ids() {
        let mut w = sex_world("select", |cfg| cfg.allow_mate_selection = true);
        let (mut p, m) = parent(&mut w, &"a".repeat(50), 0);
        p.state.phenotype.divide_sex = true;
        p.state.phenotype.cross_num = 0;
        let g = |w: &World, c: &str| genome(w, &c.repeat(40));
        p.state.phenotype.mate_select_id = 3;
        assert!(w
            .submit_offspring(&g(&w, "i"), &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        p.state.phenotype.mate_select_id = -1;
        assert!(w
            .submit_offspring(&g(&w, "j"), &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        p.state.phenotype.mate_select_id = 3;
        let kids = w
            .submit_offspring(&g(&w, "k"), &mut p.state, 0, &m)
            .unwrap();
        assert_eq!(
            kids[0].org.state.genome().representation().as_string(),
            "i".repeat(40)
        );
        p.state.phenotype.mate_select_id = -1;
        let kids = w
            .submit_offspring(&g(&w, "l"), &mut p.state, 0, &m)
            .unwrap();
        assert_eq!(
            kids[0].org.state.genome().representation().as_string(),
            "j".repeat(40)
        );
    }

    #[test]
    fn grid_local_handler_mates_with_a_waiting_neighbour() {
        let mut w = sex_world("grid", |cfg| cfg.legacy_grid_local_selection = true);
        let (mut p, m) = parent(&mut w, &"a".repeat(50), 0);
        p.state.phenotype.divide_sex = true;
        p.state.phenotype.cross_num = 0;
        let g1 = genome(&w, &"m".repeat(40));
        // Stored at the parent's cell 0; a parent in the far corner (a
        // torus neighbour of 0) finds it, one in the middle does not.
        assert!(w
            .submit_offspring(&g1, &mut p.state, 0, &m)
            .unwrap()
            .is_empty());
        let mid = w.pop.world_x * (w.pop.world_y / 2) + w.pop.world_x / 2;
        let g2 = genome(&w, &"n".repeat(40));
        assert!(w
            .submit_offspring(&g2, &mut p.state, mid, &m)
            .unwrap()
            .is_empty());
        let corner = w.pop.world_x * w.pop.world_y - 1;
        let kids = w.submit_offspring(&g2, &mut p.state, corner, &m).unwrap();
        assert_eq!(
            kids[0].org.state.genome().representation().as_string(),
            "m".repeat(40)
        );
    }

    #[test]
    fn non_mating_handlers_report_no_waiting_count() {
        let mut w = sex_world("count", |_| {});
        assert_eq!(w.birth_chamber_waiting_offspring_number(0, 0), -1);
        assert_eq!(w.birth_chamber_lines(0), None);
    }
}
