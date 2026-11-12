//! The world side of `cTaskContext` and of the test CPU
//! ([`avida_tasks::TaskWorld`], [`OutputHost`], [`TestCpuHost`]) for a
//! population run.
//!
//! While an organism's output is being evaluated the environment, RNG and
//! stats are lent to the evaluator, so the host cannot reach back into the
//! world. It therefore works from a snapshot of the organism's cell taken
//! by `PopulationInterface::output_services`, keeps the per-organism
//! `cPopulationInterface` members (`m_prevseen_cell_id`, `m_prev_task_cell`,
//! `m_num_task_cells`) indexed by cell, and queues the few `cStats` calls
//! tasks make; the world applies the queue when the step ends
//! (`World::flush_host`).
//!
//! Queries that need a feature this port does not have yet record a fatal
//! error instead of answering silently; the run loop stops on it.

use std::cell::RefCell;
use std::collections::BTreeMap;

use avida_apto::AvidaRng;
use avida_environment::StateGrid;
use avida_hardware::task_adapter::OutputHost;
use avida_hardware::test_cpu::TestCpuHost;
use avida_tasks::TaskWorld;

/// A `cStats` call made while the stats object was lent out.
#[derive(Debug, Clone, PartialEq)]
pub enum HostOp {
    /// `cStats::AddTag(tag, value)`
    AddTag(i32, i32),
    /// `cStats::AddStringBitsMatchedValue(name, value)`
    StringBitsMatched(String, i32),
    /// `cStats::IncPerfectMatch(amount)`
    PerfectMatch(i32),
    /// `cStats::IncPerfectMatchOrg()`
    PerfectMatchOrg,
    /// `cStats::IncResamplings()`
    IncResamplings,
    /// `cStats::IncFailedResamplings()`
    IncFailedResamplings,
    /// `cStats::AddNewTaskCount(task)`
    AddNewTaskCount(i32),
    /// `cStats::GenomeFragmentMetabolized(org, fragment)`: its size.
    HgtMetabolized(i32),
    /// `cStats::SumEnergyTestamentTo<target>().Add(amount)` from a test CPU.
    EnergyTestament(avida_hardware::org_interface::TestamentTarget, f64),
    /// What a test CPU printed to `std::cout`.
    Stdout(String),
    /// `ClearKaboomExecuted()` of the resting organism in this cell
    /// (`Task_ConsumePublicGood` picked it).
    ClearKaboom(i32),
}

/// See the module docs.
#[derive(Debug, Default)]
pub struct WorldHost {
    /// `cPopulationInterface::m_prevseen_cell_id` of each cell's occupant.
    pub prev_seen: Vec<i32>,
    /// `cPopulationInterface::m_prev_task_cell` of each cell's occupant.
    pub prev_task: Vec<i32>,
    /// `cPopulationInterface::m_num_task_cells` of each cell's occupant.
    pub num_task_cells: Vec<i32>,
    /// The cell of the organism whose output is being tested.
    pub cur_cell: i32,
    /// `GetCellData()` of that cell.
    pub cur_cell_data: i32,
    /// `GetInputs()` of that cell.
    pub cur_inputs: Vec<i32>,
    /// `GetResources(ctx)` as seen from that cell.
    pub cur_resources: Vec<f64>,
    /// `USE_AVATARS`
    pub use_avatars: bool,
    /// `GetAVCellID()` of the organism whose output is being tested.
    pub cur_av_cell: i32,
    /// `GetAVResources(ctx)` of that organism.
    pub cur_av_resources: Vec<f64>,
    /// `WORLD_X`
    pub world_x: i32,
    /// `WORLD_Y`
    pub world_y: i32,
    /// `MATCH_ALREADY_PRODUCED`
    pub match_already_produced: bool,
    /// Index of the resource named `pheromone`, if any.
    pub pheromone_id: Option<usize>,
    /// Stats calls waiting for `World::flush_host`.
    pub pending: Vec<HostOp>,
    /// `cPopulation::m_groups` (members per group), mirrored by the group
    /// code for `NumberOfOrganismsInGroup` during task evaluation.
    pub group_sizes: BTreeMap<i32, i32>,
    /// `GetSize()` of the organism's deme.
    pub cur_deme_size: i32,
    /// `GetCellEvent(i)->GetEventID()` of each cell event of the
    /// organism's deme; `None` while still uninitialised.
    pub cur_deme_event_ids: Vec<Option<i32>>,
    /// The movement predicates of the organism's deme.
    pub cur_deme_mov_preds: Vec<crate::deme_events::MovPredEvents>,
    /// `cEnvironment::m_state_grids`, copied at set-up (they never change).
    pub state_grids: Vec<StateGrid>,
    /// `m_cur_sg` of the running organism: set when its step starts and
    /// when its `HardwareReset` draws a new one (`GetStateGridID`).
    pub cur_sg: i32,
    /// First unsupported query, if any.
    pub fatal: Option<String>,
    /// First unsupported query made through a `&self` method.
    pub fatal_cell: RefCell<Option<String>>,
    /// The cells' genome fragments and the organisms' pending HGT
    /// fragments (`crate::hgt`).
    pub hgt: crate::hgt::HgtState,
    /// The running organism and its `GetForageTarget()` while a call that
    /// may run a test CPU (a divide, a fitness test) has its state at hand:
    /// the test CPU's view of the live organisms needs it.
    pub running_forage_target: Option<(avida_hardware::organism::OrgUid, i32)>,
    /// The C library's `rand()`, shared by the whole process
    /// (`Task_ConsumePublicGood`, `UpdateRandomResources`).
    pub libc_rand: avida_tasks::LibcRand,
    /// The cells around the tested organism's cell as
    /// `Task_ConsumePublicGood` sees them ([`WorldHost::snapshot_kaboom_cells`]).
    kaboom_cells: Vec<(i32, KaboomCell)>,
    /// Whether the environment has `consume-public-good`, cached with its
    /// task count.
    consume_public_good: Option<(usize, bool)>,
}

/// A cell around the tested organism, for `Task_ConsumePublicGood`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KaboomCell {
    /// `IsOccupied()` is false.
    Empty,
    /// `GetOrganism()->GetPhenotype().GetKaboomExecuted()`
    Occupied(bool),
    /// The running organism's cell seen from a test CPU: its phenotype is
    /// not at hand.
    Running,
    /// An index outside the grid (`GetCell` out of bounds, possible only
    /// in a world one cell wide).
    OutOfGrid,
}

impl WorldHost {
    /// A host for a world of `num_cells` cells.
    pub fn new(num_cells: usize, world_x: i32, world_y: i32) -> Self {
        WorldHost {
            prev_seen: vec![-1; num_cells],
            prev_task: vec![-1; num_cells],
            num_task_cells: vec![0; num_cells],
            cur_cell: -1,
            world_x,
            world_y,
            ..WorldHost::default()
        }
    }

    /// `new cPopulationInterface(world)` for the organism entering `cell`.
    pub fn reset_interface(&mut self, cell: usize) {
        self.prev_seen[cell] = -1;
        self.prev_task[cell] = -1;
        self.num_task_cells[cell] = 0;
    }

    /// `m_world->GetEnvironment().GetStateGrid(m_cur_sg)` of the running
    /// organism.
    fn cur_state_grid(&self) -> Option<&StateGrid> {
        usize::try_from(self.cur_sg)
            .ok()
            .and_then(|sg| self.state_grids.get(sg))
    }

    fn unsupported(&mut self, what: &str) {
        if self.fatal.is_none() {
            self.fatal = Some(format!("unsupported: {what}"));
        }
    }

    /// [`WorldHost::unsupported`] from a `&self` query.
    fn unsupported_ref(&self, what: &str) {
        let mut f = self.fatal_cell.borrow_mut();
        if f.is_none() {
            *f = Some(format!("unsupported: {what}"));
        }
    }

    /// The first fatal condition, moving one recorded through a `&self`
    /// query into [`WorldHost::fatal`].
    pub fn collect_fatal(&mut self) {
        if let Some(f) = self.fatal_cell.borrow_mut().take() {
            if self.fatal.is_none() {
                self.fatal = Some(f);
            }
        }
    }

    fn cell(&self) -> Option<usize> {
        usize::try_from(self.cur_cell)
            .ok()
            .filter(|&c| c < self.prev_seen.len())
    }

    /// Copies out the kaboom flags of the organisms in the cells
    /// `Task_ConsumePublicGood` visits around `center` (a test organism's
    /// cell is -1, whose neighbourhood wraps to the grid's corners as in
    /// the C++), when the environment has that task. The running organism
    /// is out of `pop.orgs`: from its own output `OrgTaskWorld` answers for
    /// its cell; from a test CPU its cell is [`KaboomCell::Running`].
    pub fn snapshot_kaboom_cells(
        &mut self,
        env: &avida_hardware::task_adapter::Env,
        pop: &crate::population::Population,
        center: i32,
    ) {
        self.kaboom_cells.clear();
        let tasks = env.tasklib().0.tasks();
        let wanted = match self.consume_public_good {
            Some((n, wanted)) if n == tasks.len() => wanted,
            _ => {
                let wanted = tasks.iter().any(|t| t.name() == "consume-public-good");
                self.consume_public_good = Some((tasks.len(), wanted));
                wanted
            }
        };
        if !wanted || self.world_x <= 0 {
            return;
        }
        let (wx, wy) = (self.world_x, self.world_y);
        let cx = center % wx;
        let cy = (center - cx) / wx;
        let wrap = |v: i32, size: i32| {
            if v < 0 {
                size + v
            } else if v >= size {
                v - size
            } else {
                v
            }
        };
        let running = pop
            .running
            .as_ref()
            .filter(|r| !r.removed)
            .map(|r| r.cell_id);
        for i in (cx - 1)..=(cx + 1) {
            for j in (cy - 1)..=(cy + 1) {
                let (x, y) = (wrap(i, wx), wrap(j, wy));
                let cell = y * wx + x;
                let state = if cell < 0 || cell as usize >= pop.orgs.len() {
                    KaboomCell::OutOfGrid
                } else if Some(cell) == running && center != cell {
                    KaboomCell::Running
                } else {
                    match &pop.orgs[cell as usize] {
                        Some(o) => KaboomCell::Occupied(o.state.phenotype.kaboom_executed),
                        None => KaboomCell::Empty,
                    }
                };
                if !self.kaboom_cells.iter().any(|&(c, _)| c == cell) {
                    self.kaboom_cells.push((cell, state));
                }
            }
        }
    }
}

impl TaskWorld for WorldHost {
    fn get_cell_id(&self) -> i32 {
        self.cur_cell
    }
    fn get_av_cell_id(&self) -> i32 {
        self.cur_av_cell
    }
    fn get_prev_seen_cell_id(&self) -> i32 {
        self.cell().map_or(-1, |c| self.prev_seen[c])
    }
    fn set_prev_seen_cell_id(&mut self, id: i32) {
        if let Some(c) = self.cell() {
            self.prev_seen[c] = id;
        }
    }
    fn get_prev_task_cell_id(&self) -> i32 {
        self.cell().map_or(-1, |c| self.prev_task[c])
    }
    fn set_prev_task_cell_id(&mut self, id: i32) {
        if let Some(c) = self.cell() {
            self.prev_task[c] = id;
        }
    }
    fn add_reached_task_cell(&mut self) {
        if let Some(c) = self.cell() {
            self.num_task_cells[c] += 1;
        }
    }
    fn get_cell_data(&self) -> i32 {
        self.cur_cell_data
    }

    // The organism fields below are answered by `OrgTaskWorld` from its
    // snapshot and never reach the host.
    fn get_forage_target(&self) -> i32 {
        -1
    }
    fn get_gradient_movement(&self) -> f64 {
        0.0
    }
    fn has_opinion(&self) -> bool {
        false
    }
    fn get_opinion_first(&self) -> i32 {
        0
    }
    fn get_event_killed(&self) -> bool {
        false
    }
    fn get_lyse_display(&self) -> bool {
        false
    }
    fn get_input_at(&self, i: i32) -> i32 {
        // `cOrganism::GetInputAt(int i)`: the cell advances a copy.
        if self.cur_inputs.is_empty() {
            return 0;
        }
        let n = self.cur_inputs.len() as i32;
        self.cur_inputs[i.rem_euclid(n) as usize]
    }
    fn get_kaboom_executed(&self) -> bool {
        false
    }
    fn get_kaboom_executed2(&self) -> bool {
        false
    }
    fn get_number_strings_produced(&self, _tag: i32) -> i32 {
        0
    }
    fn get_number_strings_on_hand(&self, _i: i32) -> i32 {
        0
    }
    fn update_tag(&mut self, _tag: i32, _bits: i32) {}
    fn get_tag_label(&self) -> i32 {
        -1
    }
    fn set_lineage_label(&mut self, _label: i32) {}
    fn get_reputation(&self) -> i32 {
        0
    }
    fn set_reputation(&mut self, _rep: i32) {}
    fn get_number_of_donations(&self) -> i32 {
        0
    }

    // `ctx.GetOrganism()->GetStateGrid()`: the environment's state grid
    // `m_cur_sg` of the organism whose output is being tested. Without
    // state grids the C++ indexes an empty array (only reached by a
    // `sg_path_traversal` task, which then never matches).
    fn state_grid_name(&self) -> String {
        self.cur_state_grid()
            .map(|s| s.name().to_string())
            .unwrap_or_default()
    }
    fn state_grid_state_id(&self, name: &str) -> i32 {
        self.cur_state_grid().map_or(-1, |s| s.state_id(name))
    }
    fn state_grid_num_states(&self) -> i32 {
        self.cur_state_grid().map_or(0, StateGrid::num_states)
    }
    fn state_grid_width(&self) -> i32 {
        self.cur_state_grid().map_or(0, StateGrid::width)
    }
    fn state_grid_height(&self) -> i32 {
        self.cur_state_grid().map_or(0, StateGrid::height)
    }
    fn state_grid_state_at(&self, cell: i32) -> i32 {
        self.cur_state_grid().map_or(0, |s| s.state_at_id(cell))
    }

    // The organism's deme, from the snapshot `output_services` takes. Every
    // deme is WORLD_X cells wide.
    fn deme_cell_position_x(&self, cell_id: i32) -> i32 {
        if self.world_x > 0 {
            cell_id % self.world_x
        } else {
            0
        }
    }
    fn deme_get_relative_cell_id(&self, cell_id: i32) -> i32 {
        if self.cur_deme_size > 0 {
            cell_id % self.cur_deme_size
        } else {
            cell_id
        }
    }
    fn deme_num_events(&self) -> i32 {
        self.cur_deme_event_ids.len() as i32
    }
    fn deme_cell_event_id(&self, i: i32) -> i32 {
        let id = usize::try_from(i)
            .ok()
            .and_then(|i| self.cur_deme_event_ids.get(i).copied());
        match id {
            Some(Some(id)) => id,
            _ => {
                self.unsupported_ref(
                    "a task reads the id of a deme cell event that never started (C++: \
                     cDemeCellEvent::m_eventID is uninitialised)",
                );
                -1
            }
        }
    }
    fn deme_num_movement_predicates(&self) -> i32 {
        self.cur_deme_mov_preds.len() as i32
    }
    fn deme_mov_predicate_event(&self, predicate: i32, event: i32) -> Option<(i32, bool)> {
        let ev = usize::try_from(predicate)
            .ok()
            .and_then(|p| self.cur_deme_mov_preds.get(p))
            .zip(usize::try_from(event).ok())
            .and_then(|(evs, e)| evs.get(e).copied().flatten());
        match ev {
            Some((Some(id), active)) => Some((id, active)),
            Some((None, _)) => {
                // Conservative: Task_MoveBetweenMovementEvent would only
                // read the id of an active event, but the task cannot tell
                // the host which it is.
                self.unsupported_ref(
                    "a task reads the id of a deme cell event that never started (C++: \
                     cDemeCellEvent::m_eventID is uninitialised)",
                );
                None
            }
            None => {
                self.unsupported_ref(
                    "a movement task reads event 1 of an EventMovedBetweenTargets predicate \
                     with one event (C++: out-of-bounds m_events[1])",
                );
                None
            }
        }
    }

    fn number_of_organisms_in_group(&mut self, group_id: i32) -> i32 {
        self.group_sizes.get(&group_id).copied().unwrap_or(0)
    }
    fn world_x(&self) -> i32 {
        self.world_x
    }
    fn world_y(&self) -> i32 {
        self.world_y
    }
    fn use_avatars(&self) -> bool {
        self.use_avatars
    }
    fn match_already_produced(&self) -> bool {
        self.match_already_produced
    }
    fn pheromone_resource_amount(&self) -> f64 {
        self.pheromone_id
            .and_then(|id| self.cur_resources.get(id).copied())
            .unwrap_or(0.0)
    }
    fn get_resources(&self) -> Vec<f64> {
        self.cur_resources.clone()
    }
    fn get_av_resources(&self) -> Vec<f64> {
        self.cur_av_resources.clone()
    }
    fn change_reaction_resource(&mut self, _task_id: usize, _resource_index: i32) -> bool {
        // Deferred by `OrgTaskWorld`; never reaches the host.
        false
    }
    /// From [`WorldHost::snapshot_kaboom_cells`].
    fn cell_kaboom_executed(&self, cell_id: i32) -> Option<bool> {
        match self.kaboom_cells.iter().find(|&&(c, _)| c == cell_id) {
            Some((_, KaboomCell::Empty)) => None,
            Some((_, KaboomCell::Occupied(flag))) => Some(*flag),
            Some((_, KaboomCell::Running)) => {
                self.unsupported_ref(
                    "consume-public-good in a test CPU next to the running organism \
                     (C++: reads the live organism's kaboom flag)",
                );
                None
            }
            Some((_, KaboomCell::OutOfGrid)) => {
                self.unsupported_ref(
                    "consume-public-good reading a cell outside the grid \
                     (C++: cPopulation::GetCell out of bounds)",
                );
                None
            }
            None => {
                self.unsupported_ref(
                    "consume-public-good reading a cell missing from the snapshot",
                );
                None
            }
        }
    }
    /// Applied to the organism by `World::flush_host`, after the step.
    fn clear_kaboom_executed(&mut self, cell_id: i32) {
        if let Some(entry) = self.kaboom_cells.iter_mut().find(|(c, _)| *c == cell_id) {
            entry.1 = KaboomCell::Occupied(false);
        }
        self.pending.push(HostOp::ClearKaboom(cell_id));
    }
    fn std_rand(&mut self) -> i32 {
        self.libc_rand.next_value()
    }
    fn unreproducible_producer_choice(&mut self, producers: usize) {
        self.unsupported(&format!(
            "consume-public-good choosing among {producers} producers (C++: \
             Task_ConsumePublicGood indexes a std::set<cOrganism*>, ordered by heap address)"
        ));
    }
    fn stats_add_tag(&mut self, tag: i32, value: i32) {
        self.pending.push(HostOp::AddTag(tag, value));
    }
    fn stats_add_string_bits_matched_value(&mut self, name: &str, value: i32) {
        self.pending
            .push(HostOp::StringBitsMatched(name.to_string(), value));
    }
    fn stats_inc_perfect_match(&mut self, amount: i32) {
        self.pending.push(HostOp::PerfectMatch(amount));
    }
    fn stats_inc_perfect_match_org(&mut self) {
        self.pending.push(HostOp::PerfectMatchOrg);
    }
}

impl OutputHost for WorldHost {
    fn task_probability(&mut self, _rng: &mut AvidaRng, _task_id: i32) -> f64 {
        // Reached only when a process of the reaction sets `phenplastbonus`
        // and the organism is not in a test CPU.
        self.unsupported(
            "phenplastbonus on a reaction process in a population run (C++: \
             cEnvironment::GetTaskProbability runs cPhenPlastUtil::TestPlasticity, a batch of \
             test CPUs on the organism's genotype drawing from the world RNG, in the middle of \
             the organism's output test, and caches the summary on the genotype)",
        );
        0.0
    }
    /// `cell.PopGenomeFragment(ctx)` of the organism's cell (-1, so
    /// nothing, in the test CPU: `make_test_world` clears `cur_cell`).
    fn metabolize_genome_fragment(&mut self, rng: &mut AvidaRng) -> Option<i32> {
        let len = self.hgt.metabolize(self.cur_cell, rng)?;
        self.pending.push(HostOp::HgtMetabolized(len));
        Some(len)
    }
}

impl TestCpuHost for WorldHost {
    fn stats_inc_resamplings(&mut self) {
        self.pending.push(HostOp::IncResamplings);
    }
    fn stats_inc_failed_resamplings(&mut self) {
        self.pending.push(HostOp::IncFailedResamplings);
    }
    fn stats_add_new_task_count(&mut self, task: i32) {
        self.pending.push(HostOp::AddNewTaskCount(task));
    }
    fn stats_add_energy_testament(
        &mut self,
        target: avida_hardware::org_interface::TestamentTarget,
        amount: f64,
    ) {
        self.pending.push(HostOp::EnergyTestament(target, amount));
    }
    fn cout(&mut self, text: &str) {
        self.pending.push(HostOp::Stdout(text.to_string()));
    }
}
