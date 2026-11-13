//! Port of `cPopulation` (avida-core/source/main/cPopulation.{h,cc} at
//! 47f13dadb).
//!
//! Ported here: the constructor's validation and `SetupCellGrid` (cells,
//! demes, topology, time slicer, global and deme resources). Organism
//! handling lives in [`crate::pop_orgs`], deme handling in
//! [`crate::pop_demes`].
//!
//! # Unsupported settings (fail at set-up, see `ARCHITECTURE.md`)
//! * `NUM_DEMES` that does not divide `WORLD_Y` (the C++ builds the last
//!   topology past the end of the cell array).

use std::collections::VecDeque;

use avida_apto::scheduler::{build_time_slicer, PriorityScheduler};
use avida_apto::AvidaRng;
use avida_config::Config;
use avida_environment::{MutationRates, Resource, ResourceLib};
use avida_resources::{CellResource, GradientParams, ResourceCount, ResourceEnv, ResourceSetup};

use avida_hardware::Organism;
use avida_systematics::unit::GroupMembership;

use crate::birth_chamber::BirthChamber;
use crate::deme::Deme;
use crate::groups::Groups;
use crate::migration_matrix::MigrationMatrix;
use crate::pop_demes::DemeReplStats;
use crate::population_cell::PopulationCell;
use crate::stats::Stats;
use crate::topology::{build_deme_topology, ConnList};

/// `LOG_SLEEP_TIMES`: `cPopulation::sleep_log` is a single
/// `AvidaArray<pair<int,int>>` of `world_x * world_y` pairs that the C++
/// indexes per cell (`sleep_log[cellID]`), so any cell but 0 reads and
/// writes past it: in `heads_default_100u` at 10x10, seed 1, the first
/// death outside cell 0 comes in update 78 and the reference runs on with
/// heap garbage until a SIGSEGV after update 90. Cell 0 has no visible
/// effect: nothing prints the log. The run stops at the first access to
/// another cell.
pub const SLEEP_LOG_STOP: &str = "LOG_SLEEP_TIMES with an organism dying or sleeping outside \
     cell 0 (C++: cPopulation::sleep_log is one array indexed per cell; sleep_log[cellID] reads \
     past it)";

/// `ePOSITION_OFFSPRING` (avida/core/Definitions.h).
pub mod position_offspring {
    pub const RANDOM: i32 = 0;
    pub const AGE: i32 = 1;
    pub const MERIT: i32 = 2;
    pub const EMPTY: i32 = 3;
    pub const FULL_SOUP_RANDOM: i32 = 4;
    pub const FULL_SOUP_ELDEST: i32 = 5;
    pub const DEME_RANDOM: i32 = 6;
    pub const PARENT_FACING: i32 = 7;
    pub const NEXT_CELL: i32 = 8;
    pub const FULL_SOUP_ENERGY_USED: i32 = 9;
    pub const NEIGHBORHOOD_ENERGY_USED: i32 = 10;
    pub const DISPERSAL: i32 = 11;
    /// `NUM_LOCAL_POSITION_OFFSPRING`
    pub const NUM_LOCAL: i32 = FULL_SOUP_RANDOM;
}

/// The argument list of the `resource_count.Setup(...)` call in
/// `cPopulation::SetupCellGrid`, built from an environment resource.
/// `decay` is `1.0 - res->GetOutflow()`.
pub fn resource_setup_from(res: &Resource, verbosity: i32) -> ResourceSetup {
    ResourceSetup {
        name: res.name().to_string(),
        initial: res.initial,
        inflow: res.inflow,
        decay: 1.0 - res.outflow,
        geometry: res.geometry,
        xdiffuse: res.xdiffuse,
        xgravity: res.xgravity,
        ydiffuse: res.ydiffuse,
        ygravity: res.ygravity,
        inflow_x1: res.inflow_x1,
        inflow_x2: res.inflow_x2,
        inflow_y1: res.inflow_y1,
        inflow_y2: res.inflow_y2,
        outflow_x1: res.outflow_x1,
        outflow_x2: res.outflow_x2,
        outflow_y1: res.outflow_y1,
        outflow_y2: res.outflow_y2,
        cell_list: res
            .cell_list
            .iter()
            .map(|c| CellResource {
                cell_id: c.cell_id,
                initial: c.initial,
                inflow: c.inflow,
                outflow: c.outflow,
            })
            .collect(),
        cell_id_list: res.cell_id_list.clone(),
        verbosity,
        peaks: res.peaks,
        min_height: res.min_height,
        min_radius: res.min_radius,
        radius_range: res.radius_range,
        ah: res.ah,
        ar: res.ar,
        acx: res.acx,
        acy: res.acy,
        hstepscale: res.hstepscale,
        rstepscale: res.rstepscale,
        cstepscalex: res.cstepscalex,
        cstepscaley: res.cstepscaley,
        hstep: res.hstep,
        rstep: res.rstep,
        cstepx: res.cstepx,
        cstepy: res.cstepy,
        update_dynamic: res.update_dynamic,
        gradient: GradientParams {
            peakx: res.peakx,
            peaky: res.peaky,
            height: res.height,
            spread: res.spread,
            plateau: res.plateau,
            decay: res.decay,
            max_x: res.max_x,
            min_x: res.min_x,
            max_y: res.max_y,
            min_y: res.min_y,
            move_a_scaler: res.move_a_scaler,
            updatestep: res.updatestep,
            halo: res.halo,
            halo_inner_radius: res.halo_inner_radius,
            halo_width: res.halo_width,
            halo_anchor_x: res.halo_anchor_x,
            halo_anchor_y: res.halo_anchor_y,
            move_speed: res.move_speed,
            move_resistance: res.move_resistance,
            plateau_inflow: res.plateau_inflow,
            plateau_outflow: res.plateau_outflow,
            cone_inflow: res.cone_inflow,
            cone_outflow: res.cone_outflow,
            gradient_inflow: res.gradient_inflow,
            is_plateau_common: res.is_plateau_common,
            floor: res.floor,
            habitat: res.habitat,
            min_size: res.min_size,
            max_size: res.max_size,
            config: res.config,
            count: res.count,
            resistance: res.resistance,
            damage: res.damage,
            death_odds: res.death_odds,
            path: i32::from(res.path),
            hammer: i32::from(res.hammer),
            init_plat: res.init_plat,
            threshold: res.threshold,
            refuge: res.refuge,
        },
        is_gradient: res.isgradient,
    }
}

/// C++: `cPopulation`
pub struct Population {
    pub world_x: i32,
    pub world_y: i32,
    /// `cell_array`
    pub cells: Vec<PopulationCell>,
    /// `cPopulationCell::m_spec_state` of each cell (the speculative
    /// instructions its occupant has run ahead), apart from `cells`: every
    /// time slice reads it, and a dense array stays in cache.
    pub spec_state: Vec<i32>,
    /// `empty_cell_id_array`
    pub empty_cell_id_array: Vec<i32>,
    /// `reaper_queue` (`tList<cPopulationCell>`; `Push` adds at the front).
    pub reaper_queue: VecDeque<i32>,
    /// `m_scheduler`
    pub scheduler: Box<dyn PriorityScheduler>,
    /// The priority each cell last got from `AdjustSchedule` (what
    /// `scheduler` weighs it by). Only the parallel execution mode reads
    /// it (see [`crate::parallel`]); not in the C++.
    pub sched_priority: Vec<f64>,
    /// The cell the organism of the last `ProcessStep` ended in (it may
    /// have moved). Only the parallel execution mode reads it.
    pub last_step_cell: i32,
    /// `resource_count`
    pub resource_count: ResourceCount,
    /// `m_hgt_resid`
    pub hgt_resid: i32,
    pub num_organisms: i32,
    pub num_prey_organisms: i32,
    pub num_pred_organisms: i32,
    pub num_top_pred_organisms: i32,
    /// `sync_events`
    pub sync_events: bool,
    /// The occupant of each cell (`cPopulationCell::m_organism`), `None`
    /// for an empty cell and for the cell of the organism being run.
    pub orgs: Vec<Option<Box<Organism>>>,
    /// The systematics groups of each cell's occupant
    /// (`Systematics::Unit::m_groups`).
    pub membership: Vec<GroupMembership>,
    /// `live_org_list`, as cell ids.
    pub live_org_list: Vec<i32>,
    /// `cOrganism::m_org_list_index` of each cell's occupant.
    pub live_index: Vec<i32>,
    /// The organism `ProcessStep` is running, taken out of `orgs`.
    pub running: Option<Running>,
    /// `birth_chamber`
    pub birth_chamber: BirthChamber,
    /// Group formation and tolerance state (`m_groups`, `m_group_list`, ...).
    pub groups: Groups,
    /// `deme_array`
    pub demes: Vec<Deme>,
    /// The deme replication members of `cStats` (see [`DemeReplStats`]).
    pub deme_repl_stats: DemeReplStats,
    /// Organisms founder-based deme seeding has killed but not yet deleted
    /// (see [`crate::deme_founders::SeedLimbo`]).
    pub seed_limbo: crate::deme_founders::SeedLimbo,
    /// `cWorld::m_mig_mat`
    pub migration_matrix: MigrationMatrix,
    /// Parasite bookkeeping (see [`crate::parasites`]).
    pub parasites: crate::parasites::ParasiteState,
    /// Births recorded for a front end (see [`crate::births`]; not in the
    /// C++).
    pub births: crate::births::BirthLog,
    /// The last [`avida_hardware::organism::OrgUid`] handed out (organisms
    /// built in this world are numbered from 1; not in the C++, where the
    /// `cOrganism*` is the identity).
    pub last_org_uid: u64,
    /// Whether [`World::delete_organism`](crate::world::World::delete_organism)
    /// parks the memory of the organisms it deletes in `graveyard` instead
    /// of freeing it (the parallel mode, which frees it on its worker
    /// threads).
    pub defer_drops: bool,
    /// Hardware and state of deleted organisms, not yet freed.
    pub graveyard: Vec<(
        Box<dyn avida_hardware::hardware::Hardware>,
        avida_hardware::organism::OrgState,
    )>,
    /// The offspring's organism a burst of the parallel mode built for the
    /// divide being committed ([`World::commit_divide`]); the birth chamber
    /// takes it where the exact path builds one. Always `None` in the exact
    /// mode.
    pub prebuilt_offspring: Option<Box<avida_hardware::organism::Organism>>,
}

/// The organism a process step has taken out of its cell.
#[derive(Debug, Default)]
pub struct Running {
    pub cell_id: i32,
    /// `cOrganism::GetID()` of the organism being stepped.
    pub org_id: i32,
    /// The organism being stepped (its `cOrganism*`).
    pub uid: avida_hardware::organism::OrgUid,
    /// `KillOrganism` must still call `NotifyDeath` on it when the step ends.
    pub notify_pending: bool,
    /// `KillOrganism` reached it (`cPhenotype::SetToDelete`); its cell no
    /// longer belongs to it.
    pub removed: bool,
    /// Its systematics groups once removed; the unit leaves them when the
    /// step ends (`delete cur_org`).
    pub dead_membership: Option<GroupMembership>,
}

/// What set-up printed or reported besides building the population.
#[derive(Debug, Default)]
pub struct SetupReport {
    /// Lines `cResourceCount::Setup` writes to stdout at verbose levels.
    pub stdout: Vec<String>,
    /// `Feedback().Warning(...)` messages.
    pub warnings: Vec<String>,
}

impl Population {
    /// `cPopulation::cPopulation` followed by `SetupCellGrid`.
    ///
    /// `mut_rates` is `environment.GetMutRates()`, `resource_lib` is
    /// `environment.GetResourceLib()`, `num_tasks` and `num_reactions` the
    /// environment's counts (sizing each deme's task arrays). Returns `Err`
    /// where C++ aborts with `INVALID_CONFIG`, trips an assertion, or would
    /// enter a path that is not ported.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cfg: &mut Config,
        mut_rates: &MutationRates,
        resource_lib: &ResourceLib,
        stats: &mut Stats,
        rng: &mut AvidaRng,
        res_env: &mut dyn ResourceEnv,
        num_tasks: usize,
        num_reactions: usize,
    ) -> Result<(Population, SetupReport), String> {
        // Validate settings
        if cfg.energy_cap == -1.0 {
            cfg.energy_cap = f64::MAX;
        }
        let cfg = &*cfg;

        let world_x = cfg.world_x;
        let world_y = cfg.world_y;
        let num_cells = world_x * world_y;
        let geometry = cfg.world_geometry;

        let mut num_demes = cfg.num_demes;
        if num_demes <= 0 {
            num_demes = 1; // One population == one deme.
        }
        if world_x <= 0 || world_y <= 0 {
            return Err(format!(
                "invalid world size {world_x} x {world_y} (C++: undefined behaviour)"
            ));
        }
        if world_y % num_demes != 0 {
            return Err(format!(
                "unsupported: NUM_DEMES={num_demes} does not divide WORLD_Y={world_y} (C++: \
                 topology built past the end of the cell array)"
            ));
        }

        // Allocate and set up the cells; nothing here depends on topology.
        let mut cells = vec![PopulationCell::default(); num_cells as usize];
        let empty_cell_id_array: Vec<i32> = (0..num_cells).collect();
        let fill_reaper_queue = cfg.birth_method == position_offspring::FULL_SOUP_ELDEST;
        let mut reaper_queue = VecDeque::new();
        for i in 0..num_cells {
            cells[i as usize].setup(i, mut_rates, i % world_x, i / world_x);
            if fill_reaper_queue {
                reaper_queue.push_front(i);
            }
        }

        // What are the sizes of the demes that we're creating?
        let deme_size_x = world_x;
        let deme_size_y = world_y / num_demes;
        let deme_size = deme_size_x * deme_size_y;
        if cfg.demes_replicate_size > deme_size {
            return Err("DEMES_REPLICATE_SIZE exceeds the deme size (C++: assertion)".into());
        }

        // Setup the deme structures.
        let resources = resource_lib.resources();
        let num_deme_res = resources.iter().filter(|r| r.deme_resource).count();
        let mut demes = Vec::with_capacity(num_demes as usize);
        for deme_id in 0..num_demes {
            let deme_cells: Vec<i32> = (0..deme_size).map(|o| deme_id * deme_size + o).collect();
            for &c in &deme_cells {
                cells[c as usize].deme_id = deme_id;
            }
            demes.push(Deme::new(
                deme_id,
                deme_cells,
                deme_size_x,
                num_tasks,
                num_reactions,
                num_deme_res,
            ));
        }

        // Topology: the builders work on the connection lists alone.
        let mut conns: Vec<ConnList> = cells
            .iter_mut()
            .map(|c| std::mem::take(&mut c.connections))
            .collect();
        let mut i = 0;
        while i < num_cells {
            let range = i as usize..(i + deme_size) as usize;
            build_deme_topology(
                geometry,
                &mut conns[range],
                i,
                deme_size_x,
                deme_size_y,
                (
                    cfg.scale_free_m,
                    cfg.scale_free_alpha,
                    cfg.scale_free_zero_appeal,
                ),
                rng,
            )?;
            i += deme_size;
        }
        for (cell, conn) in cells.iter_mut().zip(conns) {
            cell.connections = conn;
        }

        // BuildTimeSlicer
        // The C++ default branch prints "error: requested time slicer not
        // found." to cout and calls `GetDriver().Abort(INVALID_CONFIG)`,
        // but the driver is only attached after `cWorld::Initialize`, so the
        // reference dies of a NULL dereference (SIGSEGV), which this port
        // cannot reproduce.
        let scheduler =
            build_time_slicer(cfg.slicing_method, num_cells as usize, rng).map_err(|e| {
                format!(
                    "unsupported: SLICING_METHOD {} (C++: prints '{e}' and then \
                     dereferences the NULL driver in Abort)",
                    cfg.slicing_method
                )
            })?;

        // Setup the resources...
        let mut report = SetupReport::default();
        let mut resource_count = ResourceCount::new(resources.len() - num_deme_res);
        resource_count.resize_spatial_grids(world_x, world_y);
        for deme in &mut demes {
            deme.resource_count
                .resize_spatial_grids(deme_size_x, deme_size_y);
        }

        let mut hgt_resid = -1;
        let mut global_res_index = 0;
        let mut deme_res_index = 0;
        for (i, res) in resources.iter().enumerate() {
            // check to see if this is the hgt resource:
            if res.hgt_metabolize {
                if hgt_resid != -1 {
                    // Reachable only with ENABLE_HGT set (cEnvironment
                    // rejects `hgt=1` otherwise).
                    return Err(
                        "unsupported: two hgt=1 resources (C++: reports 'Only one HGT \
                         resource is currently supported.' through the NULL driver in \
                         cPopulation::SetupCellGrid and crashes)"
                            .into(),
                    );
                }
                hgt_resid = i as i32;
            }
            if !res.deme_resource {
                let setup = resource_setup_from(res, cfg.verbosity);
                let lines = resource_count.setup(global_res_index, &setup, rng, res_env)?;
                report.stdout.extend(lines);
                // `cStats::SetResourceName`
                if let Some(slot) = stats.resource_names.get_mut(global_res_index) {
                    *slot = res.name().to_string();
                }
                global_res_index += 1;
            } else {
                // `cDeme::SetupDemeRes` for every deme. QUIRK: it passes
                // `GetMaxX(), GetMaxY(), GetMinX(), GetMinY()` where
                // `cResourceCount::Setup` takes max_x, min_x, max_y, min_y.
                let mut setup = resource_setup_from(res, cfg.verbosity);
                setup.gradient.min_x = res.max_y;
                setup.gradient.max_y = res.min_x;
                for deme in &mut demes {
                    let lines = deme
                        .resource_count
                        .setup(deme_res_index, &setup, rng, res_env)?;
                    report.stdout.extend(lines);
                    if res.energy_resource {
                        deme.energy_res_ids.push(deme_res_index);
                    }
                }
                deme_res_index += 1;
            }
        }

        // if HGT is on, make sure there's a resource for it:
        if cfg.enable_hgt != 0 && hgt_resid == -1 {
            return Err(
                "unsupported: ENABLE_HGT without an hgt=1 resource (C++: warns 'HGT is \
                 enabled, but no HGT resource is defined' through the NULL driver in \
                 cPopulation::SetupCellGrid and crashes)"
                    .into(),
            );
        }

        Ok((
            Population {
                world_x,
                world_y,
                cells,
                spec_state: vec![0; num_cells as usize],
                empty_cell_id_array,
                reaper_queue,
                scheduler,
                sched_priority: vec![0.0; num_cells as usize],
                last_step_cell: -1,
                resource_count,
                hgt_resid,
                num_organisms: 0,
                num_prey_organisms: 0,
                num_pred_organisms: 0,
                num_top_pred_organisms: 0,
                sync_events: false,
                orgs: (0..num_cells).map(|_| None).collect(),
                membership: vec![GroupMembership::new(); num_cells as usize],
                live_org_list: Vec::new(),
                live_index: vec![-1; num_cells as usize],
                running: None,
                birth_chamber: BirthChamber::default(),
                groups: Groups::default(),
                demes,
                deme_repl_stats: DemeReplStats::default(),
                seed_limbo: crate::deme_founders::SeedLimbo::default(),
                migration_matrix: MigrationMatrix::new(),
                parasites: crate::parasites::ParasiteState::default(),
                births: crate::births::BirthLog::default(),
                last_org_uid: 0,
                defer_drops: false,
                graveyard: Vec::new(),
                prebuilt_offspring: None,
            },
            report,
        ))
    }

    /// `GetSize()`: the number of cells.
    pub fn size(&self) -> i32 {
        self.cells.len() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use avida_resources::RecordingEnv;

    fn build(cfg: &mut Config, lib: &ResourceLib) -> Result<(Population, SetupReport), String> {
        let mut stats = Stats::new(vec![], vec![], lib.size() as usize, 0.0);
        let mut rng = AvidaRng::new(101);
        let mut env = RecordingEnv::default();
        Population::new(
            cfg,
            &MutationRates::default(),
            lib,
            &mut stats,
            &mut rng,
            &mut env,
            0,
            0,
        )
    }

    #[test]
    fn default_world_is_a_torus_of_3600_cells() {
        let mut cfg = Config::default();
        let (pop, report) = build(&mut cfg, &ResourceLib::new()).unwrap();
        assert_eq!(pop.size(), 3600);
        assert_eq!(pop.empty_cell_id_array.len(), 3600);
        assert_eq!(pop.empty_cell_id_array[3599], 3599);
        assert!(pop.reaper_queue.is_empty());
        assert!(report.stdout.is_empty() && report.warnings.is_empty());
        assert_eq!(cfg.energy_cap, f64::MAX);
        let c = &pop.cells[61];
        assert_eq!((c.x, c.y, c.cell_id, c.deme_id), (1, 1, 61, 0));
        assert_eq!(c.connections.len(), 8);
        // Every neighbour relation is mutual on a torus.
        for cell in &pop.cells {
            for n in cell.connections.iter() {
                assert!(pop.cells[n as usize].connections.contains(cell.cell_id));
            }
        }
    }

    #[test]
    fn eldest_birth_method_fills_the_reaper_queue_front_first() {
        let mut cfg = Config::default();
        cfg.world_x = 3;
        cfg.world_y = 2;
        cfg.birth_method = position_offspring::FULL_SOUP_ELDEST;
        let (pop, _) = build(&mut cfg, &ResourceLib::new()).unwrap();
        assert_eq!(
            pop.reaper_queue.iter().copied().collect::<Vec<_>>(),
            vec![5, 4, 3, 2, 1, 0]
        );
    }

    #[test]
    fn global_resources_are_set_up_and_named() {
        let mut cfg = Config::default();
        cfg.world_x = 4;
        cfg.world_y = 4;
        let mut lib = ResourceLib::new();
        let id = lib.add_resource("resA").unwrap();
        lib.resource_mut(id).initial = 100.0;
        lib.resource_mut(id).inflow = 1.0;
        lib.resource_mut(id).outflow = 0.01;
        let mut stats = Stats::new(vec![], vec![], 1, 0.0);
        let mut rng = AvidaRng::new(1);
        let mut env = RecordingEnv::default();
        let (pop, _) = Population::new(
            &mut cfg,
            &MutationRates::default(),
            &lib,
            &mut stats,
            &mut rng,
            &mut env,
            0,
            0,
        )
        .unwrap();
        assert_eq!(stats.resource_names, vec!["resA".to_string()]);
        assert_eq!(pop.resource_count.get_initial_resource_value(0), 100.0);
        assert_eq!(pop.hgt_resid, -1);
    }

    #[test]
    fn demes_split_the_world_into_rows() {
        let mut cfg = Config::default();
        cfg.world_x = 3;
        cfg.world_y = 4;
        cfg.num_demes = 2;
        let (pop, _) = build(&mut cfg, &ResourceLib::new()).unwrap();
        assert_eq!(pop.demes.len(), 2);
        assert_eq!(pop.demes[1].cell_ids, vec![6, 7, 8, 9, 10, 11]);
        assert_eq!((pop.demes[1].width, pop.demes[1].height()), (3, 2));
        assert_eq!(pop.cells[7].deme_id, 1);
        // Each deme is its own torus.
        assert!(pop.cells[6].connections.contains(11));
        assert!(!pop.cells[6].connections.contains(5));
    }

    #[test]
    fn unported_paths_fail_loudly() {
        let mut cfg = Config::default();
        cfg.num_demes = 7;
        assert!(build(&mut cfg, &ResourceLib::new())
            .err()
            .unwrap()
            .contains("NUM_DEMES"));
        let mut cfg = Config::default();
        cfg.slicing_method = 99;
        assert!(build(&mut cfg, &ResourceLib::new()).is_err());
    }
}
