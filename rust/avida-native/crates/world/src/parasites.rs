//! Parasites: the population side of `cHardwareTransSMT` host/parasite
//! runs, ported from avida-core/source at 47f13dadb:
//!
//! * main/cPopulation.cc: `ActivateParasite`, `TestForParasiteInteraction`,
//!   `InjectParasite`, and the parasite parts of `SavePopulation`,
//!   `LoadPopulation` and `UpdateOrganismStats`;
//! * main/cOrganism.cc: `ParasiteInfectHost`, `ClearParasites` and the
//!   order in which `~cOrganism` lets go of parasites;
//! * main/cParasite.{h,cc}: the parasite unit;
//! * actions/PopulationActions.cc: `cActionInjectParasite`,
//!   `cActionInjectParasiteSequence`, `cActionInjectParasitePair`,
//!   `cActionClearParasites`;
//! * actions/PrintActions.cc: `cActionDumpParasiteGenotypeGrid`.
//!
//! # Units and their lifetime
//!
//! A `cParasite` is a reference-counted `Systematics::Unit`, held by the
//! host thread that runs it and by the host's `m_parasites`; it leaves its
//! systematics groups in `~Unit()` when the last reference goes, which can
//! happen inside the hardware (`internalReset` on a divide). Here the unit
//! is an `Arc<ParasiteUnit>` whose `tag` is a [`ParasiteTag`]; the tag's
//! `Drop` queues the unit's groups on [`ParasiteState`], and
//! [`World::flush_parasite_deaths`] leaves them in queue order. The queue
//! is flushed before every systematics operation that can follow a drop
//! (classification, organism deletion) and at the end of each process
//! step and action, so the removals happen in the C++ order relative to
//! everything the systematics manager sees.
//!
//! [`World::delete_organism`] reproduces `delete organism`: the hardware
//! goes first (its threads' owners), then `m_parasites` (last to first),
//! then the organism's own unit.
//!
//! The parasite parts of `ActivateOffspring` (host-controlled virulence,
//! `VIRULENCE_SOURCE 2`, and vertical transmission, `FULL_VERTICAL_TRANS`)
//! are [`World::offspring_parasites`]; `PARASITE_USE_GENOTYPE_FILE` draws
//! from the list `LoadParasiteGenotypeList` fills.
//!
//! # Stops
//!
//! * `PARASITE_USE_GENOTYPE_FILE` with an empty list (no
//!   `LoadParasiteGenotypeList`, or an empty file): the C++ indexes past it.

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use avida_config::string_util::{as_double, as_int, pop_word};
use avida_genome::{Genome, InstructionSequence};
use avida_hardware::hardware::Hardware;
use avida_hardware::hardware_transsmt::{HardwareTransSmt, ParasiteUnit};
use avida_hardware::organism::{transmission, OrgSource, OrgState, Organism};
use avida_systematics::unit::{GroupMembership, GroupRef, UnitInfo};

use crate::pop_orgs::systematics_source;
use crate::population::position_offspring as po;
use crate::run::injected_source;
use crate::world::World;

// ---------------------------------------------------------------------------
// Units
// ---------------------------------------------------------------------------

/// The population's part of a parasite unit (`cParasite`): its genome and
/// its systematics groups.
pub struct ParasiteTag {
    /// `UnitGenome()`: hardware type, instruction set and code.
    pub genome: Genome,
    /// `SystematicsGroupMembership()`; empty until the unit is classified
    /// (after a successful infection).
    membership: Mutex<GroupMembership>,
    /// Where the groups go when the unit dies.
    deaths: Arc<DeathQueue>,
}

/// The groups of dead parasite units, in order of death. The queue is
/// checked after every time slice, and almost always empty, so `pending`
/// lets [`ParasiteState::take_deaths`] skip the lock. Units can only die
/// on the thread that runs the world, so the flag is exact there.
#[derive(Debug, Default)]
struct DeathQueue {
    /// Set after a push; cleared when the queue is taken.
    pending: AtomicBool,
    list: Mutex<Vec<GroupMembership>>,
}

impl ParasiteTag {
    /// The unit's groups.
    pub fn membership(&self) -> GroupMembership {
        self.membership
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_membership(&self, m: GroupMembership) {
        *self
            .membership
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = m;
    }
}

impl Drop for ParasiteTag {
    /// `Unit::~Unit()`: the unit leaves its groups (queued; see the module
    /// documentation).
    fn drop(&mut self) {
        let m = std::mem::take(
            self.membership
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner),
        );
        if !m.is_empty() {
            let mut list = self
                .deaths
                .list
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            list.push(m);
            self.deaths.pending.store(true, Ordering::Release);
        }
    }
}

/// The [`ParasiteTag`] of a unit made by this module.
pub fn parasite_tag(unit: &ParasiteUnit) -> Option<&ParasiteTag> {
    let tag: &(dyn Any + Send + Sync) = unit.tag.as_deref()?;
    tag.downcast_ref::<ParasiteTag>()
}

/// Population-wide parasite bookkeeping.
#[derive(Debug, Default)]
pub struct ParasiteState {
    /// Groups of parasite units that died and have not yet left them, in
    /// order of death.
    deaths: Arc<DeathQueue>,
}

impl ParasiteState {
    /// `new cParasite(world, genome, parent_generation, Source(HORIZONTAL,
    /// label))` with its virulence set.
    pub fn new_unit(&self, genome: Genome, label: &str, virulence: f64) -> Arc<ParasiteUnit> {
        Arc::new(ParasiteUnit {
            genome: genome.representation().clone(),
            source: OrgSource::new(transmission::HORIZONTAL, label, false),
            virulence,
            tag: Some(Box::new(ParasiteTag {
                genome,
                membership: Mutex::new(GroupMembership::new()),
                deaths: Arc::clone(&self.deaths),
            })),
        })
    }

    /// Whether a unit died since the last [`ParasiteState::take_deaths`]:
    /// a plain load, cheap enough for every time slice.
    #[inline]
    fn has_deaths(&self) -> bool {
        self.deaths.pending.load(Ordering::Relaxed)
    }

    fn take_deaths(&self) -> Vec<GroupMembership> {
        if !self.deaths.pending.swap(false, Ordering::Acquire) {
            return Vec::new();
        }
        std::mem::take(
            &mut *self
                .deaths
                .list
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }
}

/// `cOrganism::ParasiteInfectHost(parasite)`: the hardware takes the
/// parasite (only `cHardwareTransSMT` can; the other CPUs answer false),
/// then the organism records it.
pub fn organism_parasite_infect_host(
    cfg: &avida_config::config::Config,
    state: &mut OrgState,
    hw: &mut dyn Hardware,
    unit: Arc<ParasiteUnit>,
) -> bool {
    let Some(smt) = hw.as_any_mut().downcast_mut::<HardwareTransSmt>() else {
        return false;
    };
    if !smt.parasite_infect_host(cfg, Arc::clone(&unit)) {
        return false;
    }
    state.parasites.push(unit);
    true
}

/// `Apto::Max(Apto::Min(v, 1.0), 0.0)`
fn clamp_virulence(v: f64) -> f64 {
    let v = if v < 1.0 { v } else { 1.0 };
    if v > 0.0 {
        v
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Parasite action names (the registered spellings).
pub const PARASITE_ACTIONS: &[&str] = &[
    "InjectParasite",
    "InjectParasiteSequence",
    "InjectParasitePair",
    "ClearParasites",
    "DumpParasiteGenotypeGrid",
];

/// A parsed parasite action. Field names follow the C++ members (`m_`
/// dropped).
#[derive(Debug, Clone, PartialEq)]
pub enum ParasiteAction {
    /// `cActionInjectParasite`
    InjectParasite {
        filename: String,
        label: String,
        cell_start: i32,
        cell_end: i32,
        cell_stride: i32,
        only_if_parasites_extinct: i32,
    },
    /// `cActionInjectParasiteSequence`
    InjectParasiteSequence {
        sequence: String,
        label: String,
        cell_start: i32,
        cell_end: i32,
    },
    /// `cActionInjectParasitePair`
    InjectParasitePair {
        filename_genome: String,
        filename_parasite: String,
        label: String,
        cell_start: i32,
        cell_end: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionClearParasites`
    ClearParasites { cell_start: i32, cell_end: i32 },
    /// `cActionDumpParasiteGenotypeGrid`
    DumpParasiteGenotypeGrid { filename: String },
}

/// `cString largs(args)` with the constructors' `PopWord` idioms.
struct Largs(String);

impl Largs {
    /// `largs.PopWord()`
    fn word(&mut self) -> String {
        pop_word(&mut self.0)
    }
    /// `if (largs.GetSize()) x = largs.PopWord().AsInt();`
    fn int(&mut self, def: i32) -> i32 {
        if self.0.is_empty() {
            def
        } else {
            as_int(&self.word())
        }
    }
    /// `if (largs.GetSize()) x = largs.PopWord().AsDouble();`
    fn double(&mut self, def: f64) -> f64 {
        if self.0.is_empty() {
            def
        } else {
            as_double(&self.word())
        }
    }
}

/// The constructor of the parasite action `name` (a [`PARASITE_ACTIONS`]
/// spelling); `None` for any other name. None of them reports feedback.
pub fn parse(name: &str, args: &str) -> Option<ParasiteAction> {
    let mut a = Largs(args.to_string());
    let end = |start: i32, end: i32| if end == -1 { start + 1 } else { end };
    Some(match name {
        "InjectParasite" => {
            let filename = a.word();
            let label = a.word();
            let cell_start = a.int(0);
            let cell_end = a.int(-1);
            let cell_stride = a.int(1);
            let only_if_parasites_extinct = a.int(0);
            ParasiteAction::InjectParasite {
                filename,
                label,
                cell_start,
                cell_end: end(cell_start, cell_end),
                cell_stride,
                only_if_parasites_extinct,
            }
        }
        "InjectParasiteSequence" => {
            let sequence = a.word();
            let label = a.word();
            let cell_start = a.int(0);
            let cell_end = a.int(-1);
            ParasiteAction::InjectParasiteSequence {
                sequence,
                label,
                cell_start,
                cell_end: end(cell_start, cell_end),
            }
        }
        "InjectParasitePair" => {
            let filename_genome = a.word();
            let filename_parasite = a.word();
            let label = a.word();
            let cell_start = a.int(0);
            let cell_end = a.int(-1);
            let merit = a.double(-1.0);
            let lineage_label = a.int(0);
            let neutral_metric = a.double(0.0);
            ParasiteAction::InjectParasitePair {
                filename_genome,
                filename_parasite,
                label,
                cell_start,
                cell_end: end(cell_start, cell_end),
                merit,
                lineage_label,
                neutral_metric,
            }
        }
        "ClearParasites" => {
            let cell_start = a.int(0);
            let cell_end = a.int(-1);
            ParasiteAction::ClearParasites {
                cell_start,
                cell_end: end(cell_start, cell_end),
            }
        }
        "DumpParasiteGenotypeGrid" => ParasiteAction::DumpParasiteGenotypeGrid {
            filename: if a.0.is_empty() {
                String::new()
            } else {
                a.word()
            },
        },
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Population
// ---------------------------------------------------------------------------

impl World {
    /// Lets the parasite units that died since the last flush leave their
    /// systematics groups, in order of death.
    #[inline]
    pub fn flush_parasite_deaths(&mut self) {
        if self.pop.parasites.has_deaths() {
            self.flush_parasite_deaths_queued();
        }
    }

    #[cold]
    #[inline(never)]
    fn flush_parasite_deaths_queued(&mut self) {
        for m in self.pop.parasites.take_deaths() {
            self.systematics.remove_unit(&m);
        }
    }

    /// `delete organism` for an organism out of its cell: the hardware
    /// (thread owners), then `m_parasites` last to first, then the
    /// organism's own unit (`membership`, `None` if it has none).
    pub fn delete_organism(&mut self, org: Box<Organism>, membership: Option<GroupMembership>) {
        let Organism {
            mut state,
            hardware,
        } = *org;
        drop(hardware);
        state.clear_parasites();
        drop(state);
        self.flush_parasite_deaths();
        if let Some(m) = membership {
            self.systematics.remove_unit(&m);
        }
    }

    /// `GetNumParasites()` of the organism in `cell` (0 for an empty cell
    /// and for the organism being run).
    pub fn num_parasites_in_cell(&self, cell: i32) -> i32 {
        usize::try_from(cell)
            .ok()
            .and_then(|c| self.pop.orgs.get(c))
            .and_then(|o| o.as_ref())
            .map_or(0, |o| o.state.num_parasites())
    }

    /// `SystematicsGroup("genotype")` of a parasite unit.
    pub fn parasite_genotype(&self, unit: &ParasiteUnit) -> Option<GroupRef> {
        let tag = parasite_tag(unit)?;
        self.systematics.genotype_of(&tag.membership())
    }

    /// `TestForParasiteInteraction(infected_host, target_host)`, given the
    /// injecting host's `GetLastParasiteTaskCount()` and the target's
    /// `GetLastHostTaskCount()`.
    fn test_for_parasite_interaction(
        &mut self,
        parasite_tasks: &[i32],
        host_tasks: &[i32],
    ) -> bool {
        let cfg = &self.cfg;
        let start = usize::from(cfg.inject_skip_first_task != 0);
        let n = host_tasks.len();
        let para = |i: usize| parasite_tasks.get(i).copied().unwrap_or(0);
        let range = start..n;
        // default to failing the interaction
        let interaction_fails = match cfg.infection_mechanism {
            0 => false,
            // 1: Parasite must match at least 1 task the host does (Overlap)
            1 => !range.clone().any(|i| host_tasks[i] > 0 && para(i) > 0),
            // 2: Parasite must perform at least one task the host does not
            // (Inverse Overlap)
            2 => !range.clone().any(|i| host_tasks[i] == 0 && para(i) > 0),
            // 3: Parasite tasks must match host tasks exactly (Matching
            // Alleles)
            3 => range.clone().any(|i| {
                (host_tasks[i] == 0 && para(i) > 0) || (host_tasks[i] > 0 && para(i) == 0)
            }),
            // 4: Parasite tasks must overcome hosts (GFG)
            4 => {
                let host_overcomes = range.clone().any(|i| host_tasks[i] > 0 && para(i) == 0);
                let parasite_overcomes = range.clone().any(|i| para(i) > 0 && host_tasks[i] == 0);
                host_overcomes || !parasite_overcomes
            }
            // 5: Quantitative Matching Allele
            5 => {
                let num_overlap = range
                    .clone()
                    .filter(|&i| {
                        (host_tasks[i] > 0 && para(i) > 0) || (host_tasks[i] == 0 && para(i) == 0)
                    })
                    .count();
                let prop_overlap = num_overlap as f64 / (n as f64 - start as f64);
                let prob_success = prop_overlap.powf(cfg.inject_qma_exponent);
                self.rng.get_double() > prob_success
            }
            // 6: Parasite must perform at least one task
            6 => !range.clone().any(|i| para(i) > 0),
            _ => true,
        };
        if interaction_fails {
            let prob_success = self.cfg.inject_default_success;
            if self.rng.get_double() > prob_success {
                return false;
            }
        }
        true
    }

    /// The target selection of `ActivateParasite`: the cell whose organism
    /// the parasite tries, `None` when the chosen cell is empty.
    fn parasite_target_cell(&mut self, host_cell: i32) -> Result<Option<i32>, String> {
        let hc = host_cell as usize;
        let occupied = |w: &World, c: i32| w.is_occupied(c).then_some(c);
        // If there's any migration turned on ... try this first
        if self.cfg.num_demes > 0
            && self.cfg.demes_parasite_migration_rate > 0.0
            && self.cfg.demes_migration_method == 4
            && self.rng.p(self.cfg.demes_parasite_migration_rate)
        {
            let from = self.pop.cells[hc].deme_id;
            let d = self
                .pop
                .migration_matrix
                .probabilistic_deme_id(from, &mut self.rng, true)?;
            let d = usize::try_from(d)
                .ok()
                .filter(|&d| d < self.pop.demes.len())
                .ok_or("ActivateParasite: migration to a deme that does not exist")?;
            let infection_mode = self.cfg.demes_parasite_migration_target_selection_method;
            let immune_thresh = self
                .cfg
                .demes_parasite_migration_memory_score_protective_threshold;
            if immune_thresh != 0.0 && self.pop.demes[d].parasite_memory_score >= immune_thresh {
                return Ok(None);
            }
            return match infection_mode {
                // Implementation #1 - Picks randomly of ALL cells in to-deme
                // and then finds if the one it chose was occupied
                0 => {
                    let pos = self.rng.get_int(self.pop.demes[d].size());
                    let c = self.pop.demes[d].cell_id(pos);
                    Ok(occupied(self, c))
                }
                // Implementation #2 - Picks randomly, guaranteeing an
                // infection
                1 => {
                    let num_occupied_cells = self.pop.demes[d].cur_org_count;
                    let mut which_cell = self.rng.get_int(num_occupied_cells);
                    for i in 0..self.pop.demes[d].size() {
                        let c = self.pop.demes[d].cell_id(i);
                        if self.is_occupied(c) {
                            if which_cell == 0 {
                                return Ok(Some(c));
                            }
                            which_cell -= 1;
                        }
                    }
                    Ok(None)
                }
                _ => {
                    self.action_state.stdout.push_str(&format!(
                        "bad demes parasite migration infection mode {infection_mode}\n"
                    ));
                    Err(format!(
                        "C++ abort: bad demes parasite migration infection mode {infection_mode}"
                    ))
                }
            };
        }
        // Else there was no migration ... Resort to the default BIRTH_METHOD
        if self.cfg.birth_method == po::FULL_SOUP_RANDOM {
            let c = self.rng.get_uint(self.pop.cells.len() as u32) as i32;
            Ok(occupied(self, c))
        } else if self.cfg.birth_method == po::DEME_RANDOM {
            let d = self.pop.cells[hc].deme_id as usize;
            let deme = self
                .pop
                .demes
                .get(d)
                .ok_or("ActivateParasite: host cell without a deme")?;
            let pos = self.rng.get_int(deme.size());
            let c = self.pop.demes[d].cell_id(pos);
            Ok(occupied(self, c))
        } else {
            let n = self.pop.cells[hc].connections.len() as u32;
            let pos = self.rng.get_uint(n) as usize;
            let c = self.pop.cells[hc].connections.get(pos).ok_or(
                "C++ segfault: ActivateParasite in a cell without neighbours (ConnectionList().GetPos)",
            )?;
            Ok(occupied(self, c))
        }
    }

    /// `ActivateParasite(host, parent, label, injected_code)`: the host in
    /// `host_cell` (state `host`, hardware `host_hw`) injects code; `parent`
    /// is the injecting thread's owner, `None` for the host itself.
    pub fn activate_parasite(
        &mut self,
        host_cell: i32,
        host: &mut OrgState,
        host_hw: &mut dyn Hardware,
        parent: Option<Arc<ParasiteUnit>>,
        label: &str,
        injected_code: &InstructionSequence,
    ) -> Result<bool, String> {
        self.flush_parasite_deaths();

        // Quick check for empty parasites
        if injected_code.size() == 0 {
            return Ok(false);
        }

        // Select a target organism
        let Some(target_cell) = self.parasite_target_cell(host_cell)? else {
            return Ok(false);
        };
        let tc = target_cell as usize;
        let target_is_host = target_cell == host_cell;

        // The parent unit: a parasite, or the host organism itself.
        let parent_tag = match &parent {
            Some(p) => Some(
                parasite_tag(p).ok_or("ActivateParasite: parent unit without population data")?,
            ),
            None => None,
        };
        let parent_genome = match parent_tag {
            Some(t) => t.genome.clone(),
            None => host.genome().clone(),
        };
        let parent_horizontal = match &parent {
            Some(p) => p.source.transmission_type == transmission::HORIZONTAL,
            None => host.src.transmission_type == transmission::HORIZONTAL,
        };

        // Pre-check target hardware
        let (hw_type, inst_set_name, num_threads, host_tasks) = if target_is_host {
            (
                host_hw.hw_type(),
                host_hw.inst_set().inst_set_name().to_string(),
                host_hw.num_threads(),
                host.phenotype.last_host_tasks.clone(),
            )
        } else {
            let o = self.pop.orgs[tc]
                .as_ref()
                .ok_or("ActivateParasite: target cell has no organism")?;
            (
                o.hardware.hw_type(),
                o.hardware.inst_set().inst_set_name().to_string(),
                o.hardware.num_threads(),
                o.state.phenotype.last_host_tasks.clone(),
            )
        };
        if hw_type != parent_genome.hardware_type()
            || inst_set_name != parent_genome.inst_set()
            || num_threads == self.cfg.max_cpu_threads
        {
            return Ok(false);
        }

        // Handle host specific injection
        let parasite_tasks = host.phenotype.last_para_tasks.clone();
        let parasite_interaction = self.test_for_parasite_interaction(&parasite_tasks, &host_tasks);
        if self.cfg.log_parasite_injections {
            let line = format!(
                "{},{},{},{}",
                self.stats.update(),
                self.pop.cells[tc].deme_id,
                target_cell,
                i32::from(parasite_interaction)
            );
            self.out
                .static_with_path("parasite_injection.dat")?
                .write_raw(&line);
        }
        if !parasite_interaction {
            return Ok(false);
        }

        // Attempt actual parasite injection
        // (`PARASITE_USE_GENOTYPE_FILE`: a random entry of the list
        // `LoadParasiteGenotypeList` filled, in place of the injected code.)
        let code = if self.cfg.parasite_use_genotype_file != 0 {
            let list = &self.actions2.env.parasite_genotype_list;
            if list.is_empty() {
                return Err(
                    "unsupported: PARASITE_USE_GENOTYPE_FILE with an empty parasite \
                            genotype list (C++: indexes past cPopulation::parasite_genotype_list)"
                        .into(),
                );
            }
            let k = self.rng.get_int(list.len() as i32) as usize;
            InstructionSequence::from_string(&self.actions2.env.parasite_genotype_list[k])
        } else {
            injected_code.clone()
        };
        let mg = Genome::with_inst_set(
            parent_genome.hardware_type(),
            parent_genome.inst_set(),
            code,
        );

        // Handle potential virulence evolution if this parasite is coming
        // from a parasite and virulence is inherited from the parent
        let virulence = if parent_horizontal && self.cfg.virulence_source == 1 {
            let old_vir = parent.as_ref().map(|p| p.virulence).ok_or(
                "C++ undefined behaviour: VIRULENCE_SOURCE 1 casts a HORIZONTAL organism to cParasite",
            )?;
            let mut new_vir = old_vir;
            if self.rng.get_double() < self.cfg.virulence_mut_rate {
                let vir_sd = self.cfg.virulence_sd;
                new_vir = self.rng.get_rand_normal_params(old_vir, vir_sd * vir_sd);
            }
            clamp_virulence(new_vir)
        } else {
            self.cfg.parasite_virulence
        };
        let parasite = self.pop.parasites.new_unit(mg, label, virulence);

        let infected = if target_is_host {
            organism_parasite_infect_host(&self.cfg, host, host_hw, Arc::clone(&parasite))
        } else {
            let o = self.pop.orgs[tc]
                .as_deref_mut()
                .ok_or("ActivateParasite: target cell has no organism")?;
            organism_parasite_infect_host(
                &self.cfg,
                &mut o.state,
                o.hardware.as_mut(),
                Arc::clone(&parasite),
            )
        };
        if !infected {
            return Ok(false);
        }

        // If parasite was successfully injected, update the phenotype for
        // the parasite in new organism
        if !target_is_host {
            if let Some(o) = self.pop.orgs[tc].as_deref_mut() {
                let dst = &mut o.state.phenotype.last_para_tasks;
                for (i, &v) in parasite_tasks.iter().enumerate() {
                    if let Some(x) = dst.get_mut(i) {
                        *x = v;
                    }
                }
            }
        }

        // Classify the parasite
        let parent_membership = match parent_tag {
            Some(t) => t.membership(),
            None => self.host_membership(host_cell),
        };
        let tag = parasite_tag(&parasite).ok_or("parasite unit without population data")?;
        let source = systematics_source(&parasite.source);
        let unit = UnitInfo {
            source: &source,
            genome: &tag.genome,
            // `cParasite::Properties()` is empty.
            generation: 0,
        };
        let mut membership = GroupMembership::new();
        self.systematics
            .self_classify(&unit, &[&parent_membership], &mut membership);
        tag.set_membership(membership);

        // Handle post injection actions
        if self.cfg.inject_sterilizes_host != 0 {
            if target_is_host {
                host.phenotype.sterilize();
            } else if let Some(o) = self.pop.orgs[tc].as_deref_mut() {
                o.state.phenotype.sterilize();
            }
        }
        Ok(true)
    }

    /// The systematics groups of the organism in `cell`, also while it is
    /// being run (after `KillOrganism` the running organism's groups wait
    /// in `Running::dead_membership` until it is deleted).
    fn host_membership(&self, cell: i32) -> GroupMembership {
        if let Some(r) = self.pop.running.as_ref() {
            if r.cell_id == cell && r.removed {
                return r.dead_membership.clone().unwrap_or_default();
            }
        }
        self.pop.membership[cell as usize].clone()
    }

    /// `cPopulation::InjectParasite(label, injected_code, cell_id)`: a new
    /// parasite with the configured virulence infects the cell's organism
    /// and is classified without parents.
    pub fn inject_parasite_into_cell(
        &mut self,
        label: &str,
        injected_code: &InstructionSequence,
        cell_id: i32,
    ) {
        self.flush_parasite_deaths();
        let Some(target) = usize::try_from(cell_id)
            .ok()
            .and_then(|c| self.pop.orgs.get_mut(c))
            .and_then(|o| o.as_deref_mut())
        else {
            return;
        };
        let mg = Genome::with_inst_set(
            target.hardware.hw_type(),
            target.hardware.inst_set().inst_set_name(),
            injected_code.clone(),
        );
        // default to configured parasite virulence
        let parasite = self
            .pop
            .parasites
            .new_unit(mg, label, self.cfg.parasite_virulence);
        if !organism_parasite_infect_host(
            &self.cfg,
            &mut target.state,
            target.hardware.as_mut(),
            Arc::clone(&parasite),
        ) {
            return;
        }
        let Some(tag) = parasite_tag(&parasite) else {
            return;
        };
        let source = systematics_source(&parasite.source);
        let unit = UnitInfo {
            source: &source,
            genome: &tag.genome,
            generation: 0,
        };
        let mut membership = GroupMembership::new();
        self.systematics
            .classify_new_unit(&unit, None, &mut membership);
        tag.set_membership(membership);
    }

    /// The parasite part of `ActivateOffspring` for one offspring, after its
    /// reputation: host-controlled virulence (`VIRULENCE_SOURCE 2`: the
    /// donation probability is inherited and may mutate), then full vertical
    /// transmission (`FULL_VERTICAL_TRANS`: with that probability every
    /// parasite of the parent infects the offspring as a new unit labelled
    /// "Test", classified without parents).
    pub(crate) fn offspring_parasites(&mut self, parent: &OrgState, child: &mut Organism) {
        if self.cfg.virulence_source == 2 {
            let old_vir = parent.para_donate;
            let mut new_vir = old_vir;
            if self.rng.get_double() < self.cfg.virulence_mut_rate {
                let vir_sd = self.cfg.virulence_sd;
                new_vir = self.rng.get_rand_normal_params(old_vir, vir_sd * vir_sd);
            }
            child.state.para_donate = clamp_virulence(new_vir);
        }

        let p = self.cfg.full_vertical_trans;
        if p == 0.0 || !self.rng.p(p) {
            return;
        }
        // `AvidaArray<Systematics::UnitPtr> parasites_to_inject =
        // parent_organism->GetParasites();` (a copy)
        for parent_parasite in parent.parasites.clone() {
            self.flush_parasite_deaths();
            let Some(parent_tag) = parasite_tag(&parent_parasite) else {
                self.set_fatal("vertical transmission: parasite unit without population data");
                return;
            };
            let mg = parent_tag.genome.clone();
            let virulence = match self.cfg.virulence_source {
                // Virulence controlled by host
                2 => child.state.para_donate,
                // Virulence inherited from parent parasite
                1 => {
                    let old_vir = parent_parasite.virulence;
                    let mut new_vir = old_vir;
                    if self.rng.get_double() < self.cfg.virulence_mut_rate {
                        let vir_sd = self.cfg.virulence_sd;
                        new_vir = self.rng.get_rand_normal_params(old_vir, vir_sd * vir_sd);
                    }
                    clamp_virulence(new_vir)
                }
                // get default virulence
                _ => self.cfg.parasite_virulence,
            };
            let parasite = self.pop.parasites.new_unit(mg, "Test", virulence);
            if !organism_parasite_infect_host(
                &self.cfg,
                &mut child.state,
                child.hardware.as_mut(),
                Arc::clone(&parasite),
            ) {
                continue;
            }
            // `Systematics::Manager::Of(...)->ClassifyNewUnit(parasite)`
            let Some(tag) = parasite_tag(&parasite) else {
                continue;
            };
            let source = systematics_source(&parasite.source);
            let unit = UnitInfo {
                source: &source,
                genome: &tag.genome,
                generation: 0,
            };
            let mut membership = GroupMembership::new();
            self.systematics
                .classify_new_unit(&unit, None, &mut membership);
            tag.set_membership(membership);
        }
    }

    /// `0 <= start < end <= size`
    fn parasite_cell_range_ok(&self, start: i32, end: i32) -> bool {
        start >= 0 && end <= self.pop.cells.len() as i32 && start < end
    }

    /// `cAction::Process(ctx)` of a parasite action.
    /// (`World::run_action` flushes parasite deaths afterwards.)
    pub fn run_parasite_action(&mut self, action: &ParasiteAction) -> Result<(), String> {
        match action {
            ParasiteAction::InjectParasite {
                filename,
                label,
                cell_start,
                cell_end,
                cell_stride,
                only_if_parasites_extinct,
            } => {
                if *only_if_parasites_extinct != 0 && self.stats.num_parasites != 0 {
                    return Ok(());
                }
                if !self.parasite_cell_range_ok(*cell_start, *cell_end) || *cell_stride <= 0 {
                    self.action_state
                        .stdout
                        .push_str("warning: InjectParasite has invalid range!\n");
                    return Ok(());
                }
                let Some(genome) = self.load_genome(filename, "", false)? else {
                    return Ok(());
                };
                let seq = genome.representation().clone();
                let mut i = *cell_start;
                while i < *cell_end {
                    self.inject_parasite_into_cell(label, &seq, i);
                    i += *cell_stride;
                }
                self.pop.sync_events = true;
                Ok(())
            }
            ParasiteAction::InjectParasiteSequence {
                sequence,
                label,
                cell_start,
                cell_end,
            } => {
                if !self.parasite_cell_range_ok(*cell_start, *cell_end) {
                    // QUIRK: the message names InjectParasite.
                    self.action_state
                        .stdout
                        .push_str("warning: InjectParasite has invalid range!\n");
                    return Ok(());
                }
                let seq = InstructionSequence::from_string(sequence);
                for i in *cell_start..*cell_end {
                    self.inject_parasite_into_cell(label, &seq, i);
                }
                self.pop.sync_events = true;
                Ok(())
            }
            ParasiteAction::InjectParasitePair {
                filename_genome,
                filename_parasite,
                label,
                cell_start,
                cell_end,
                merit,
                lineage_label,
                neutral_metric,
            } => {
                if !self.parasite_cell_range_ok(*cell_start, *cell_end) {
                    self.action_state
                        .stdout
                        .push_str("warning: InjectParasitePair has invalid range!\n");
                    return Ok(());
                }
                let genome = self.load_genome(filename_genome, "", false)?;
                let parasite = self.load_genome(filename_parasite, "", false)?;
                let (Some(genome), Some(parasite)) = (genome, parasite) else {
                    return Ok(());
                };
                let seq = parasite.representation().clone();
                for i in *cell_start..*cell_end {
                    self.inject(
                        &genome,
                        injected_source(),
                        i,
                        *merit,
                        *lineage_label,
                        *neutral_metric,
                    )?;
                    self.inject_parasite_into_cell(label, &seq, i);
                }
                self.pop.sync_events = true;
                Ok(())
            }
            ParasiteAction::ClearParasites {
                cell_start,
                cell_end,
            } => {
                if !self.parasite_cell_range_ok(*cell_start, *cell_end) {
                    self.action_state
                        .stdout
                        .push_str("warning: ClearParasites has invalid range!\n");
                    return Ok(());
                }
                // QUIRK: only `m_parasites` is cleared; the parasite threads
                // keep running and keep their units (and groups) alive
                // until the host's hardware resets.
                for i in *cell_start..*cell_end {
                    if let Some(o) = self.pop.orgs[i as usize].as_deref_mut() {
                        o.state.clear_parasites();
                    }
                }
                self.pop.sync_events = true;
                Ok(())
            }
            ParasiteAction::DumpParasiteGenotypeGrid { filename } => {
                self.dump_parasite_genotype_grid(filename)
            }
        }
    }

    /// `cActionDumpParasiteGenotypeGrid::Process`: per cell the code of its
    /// organism's first parasite, `0` for an uninfected organism and `-1`
    /// for an empty cell.
    fn dump_parasite_genotype_grid(&mut self, filename: &str) -> Result<(), String> {
        let filename = if filename.is_empty() {
            format!("grid_genome_parasite.{}.dat", self.stats.update())
        } else {
            filename.to_string()
        };
        let mut df = self.out.create_with_path(&filename)?;
        let world_x = self.pop.world_x;
        let world_y = self.pop.world_y;
        let mut text = String::new();
        for j in 0..world_y {
            for i in 0..world_x {
                let cell_num = (j * world_x + i) as usize;
                let genome_seq = match self.pop.orgs[cell_num].as_ref() {
                    Some(o) => match o.state.parasites.first() {
                        Some(p) => p.genome.as_string(),
                        None => "0".to_string(),
                    },
                    None => "-1".to_string(),
                };
                text.push_str(&genome_seq);
                text.push(' ');
            }
            text.push('\n');
        }
        df.ofstream()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        df.flush();
        Ok(())
    }

    /// The parasite rows `SavePopulation` writes for the organism in `cell`
    /// before the organism's own row.
    pub(crate) fn save_parasites(
        &self,
        save: &mut avida_systematics::spop::PopulationSave,
        cell: usize,
    ) {
        let Some(o) = self.pop.orgs.get(cell).and_then(|o| o.as_ref()) else {
            return;
        };
        for p in &o.state.parasites {
            if let Some(pg) = self.parasite_genotype(p) {
                save.add_parasite(&self.systematics, pg, cell as i32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_matches_the_constructors() {
        assert_eq!(
            parse("InjectParasite", "parasite-smt.org ABB 0 2500 2 1"),
            Some(ParasiteAction::InjectParasite {
                filename: "parasite-smt.org".into(),
                label: "ABB".into(),
                cell_start: 0,
                cell_end: 2500,
                cell_stride: 2,
                only_if_parasites_extinct: 1,
            })
        );
        assert_eq!(
            parse("InjectParasite", "p.org ABB 5"),
            Some(ParasiteAction::InjectParasite {
                filename: "p.org".into(),
                label: "ABB".into(),
                cell_start: 5,
                cell_end: 6,
                cell_stride: 1,
                only_if_parasites_extinct: 0,
            })
        );
        assert_eq!(
            parse("ClearParasites", ""),
            Some(ParasiteAction::ClearParasites {
                cell_start: 0,
                cell_end: 1
            })
        );
        assert_eq!(
            parse("DumpParasiteGenotypeGrid", ""),
            Some(ParasiteAction::DumpParasiteGenotypeGrid {
                filename: String::new()
            })
        );
        assert_eq!(
            parse("InjectParasitePair", "a.org b.org AB 3 -1 2.5"),
            Some(ParasiteAction::InjectParasitePair {
                filename_genome: "a.org".into(),
                filename_parasite: "b.org".into(),
                label: "AB".into(),
                cell_start: 3,
                cell_end: 4,
                merit: 2.5,
                lineage_label: 0,
                neutral_metric: 0.0,
            })
        );
        assert_eq!(parse("Exit", ""), None);
    }

    #[test]
    fn dead_units_queue_their_groups_in_order() {
        use avida_systematics::{Arbiter, GenotypeArbiter, Manager, Source, TransmissionType};
        let mut manager = Manager::new();
        manager.register_arbiter(Arbiter::genotype(GenotypeArbiter::new(
            "genotype",
            3,
            false,
            &[],
        )));
        let source = Source::new(TransmissionType::Horizontal, "AB", false);
        let mut g = |code: &str| {
            let genome =
                Genome::with_inst_set(2, "transsmt", InstructionSequence::from_string(code));
            let unit = UnitInfo {
                source: &source,
                genome: &genome,
                generation: 0,
            };
            let mut m = GroupMembership::new();
            manager.classify_new_unit(&unit, None, &mut m);
            assert!(!m.is_empty());
            m
        };
        let (g1, g2) = (g("abc"), g("abd"));
        assert_ne!(g1, g2);
        let state = ParasiteState::default();
        let a = state.new_unit(Genome::default(), "AB", 0.5);
        let b = state.new_unit(Genome::default(), "AB", 0.5);
        let unclassified = state.new_unit(Genome::default(), "AB", 0.5);
        parasite_tag(&a).unwrap().set_membership(g1.clone());
        parasite_tag(&b).unwrap().set_membership(g2.clone());
        let a2 = Arc::clone(&a);
        drop(unclassified);
        drop(a);
        assert!(state.take_deaths().is_empty(), "a is still referenced");
        drop(b);
        drop(a2);
        assert_eq!(state.take_deaths(), vec![g2, g1]);
    }

    #[test]
    fn virulence_is_clamped_like_apto() {
        assert_eq!(clamp_virulence(1.5), 1.0);
        assert_eq!(clamp_virulence(-0.5), 0.0);
        assert_eq!(clamp_virulence(0.25), 0.25);
    }
}
