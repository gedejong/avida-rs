//! Port of `cPopulationInterface`
//! (avida-core/source/main/cPopulationInterface.{h,cc} at 47f13dadb): the
//! [`OrgInterface`] of an organism living in a population cell.
//!
//! The C++ object is owned by the organism and stores `m_cell_id`; here it
//! is built on demand around `&mut World` and the organism's cell. The
//! per-organism members (`m_prevseen_cell_id`, ...) live in
//! [`crate::host::WorldHost`], indexed by cell.
//!
//! Methods whose population code is not ported record a fatal error
//! (`unsupported: cPopulationInterface::<method>`) and return a neutral
//! value; the run loop stops at the end of the step. They are the block at
//! the end of the `impl`.

use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_genome::{Genome, InstructionSequence};
use avida_hardware::hardware::Hardware;
use avida_hardware::hardware_manager::HardwareManager;
use avida_hardware::org_interface::{
    Faced, OffspringTest, OrgInterface, OrgMessage, OutputServices, SeenOrg, TestamentTarget,
    WorldServices,
};
use avida_hardware::organism::{OrgState, OrgUid, Organism};
use avida_hardware::task_adapter::{Env, PlasticityServices};
use avida_hardware::test_cpu::{self, TestWorldCounts};
use avida_phenotype::Merit;

use crate::world::{make_test_world, World};

/// See the module docs.
pub struct PopulationInterface<'a> {
    pub w: &'a mut World,
    /// `m_cell_id`
    pub cell_id: i32,
}

impl PopulationInterface<'_> {
    fn unsupported(&self, method: &str) {
        self.w
            .set_fatal(format!("unsupported: cPopulationInterface::{method}"));
    }

    fn cell(&self) -> usize {
        self.cell_id.max(0) as usize
    }

    /// `m_organism->SystematicsGroup("genotype")` (a running organism that
    /// has died keeps its membership in `pop.running`).
    fn own_genotype(&self) -> Option<avida_systematics::unit::GroupRef> {
        let membership = match self.w.pop.running.as_ref() {
            Some(r) if r.cell_id == self.cell_id && r.removed => r.dead_membership.as_ref(),
            _ => self.w.pop.membership.get(self.cell()),
        };
        membership.and_then(|m| self.w.systematics.genotype_of(m))
    }

    /// Whether the organism being run lives in `cell` (it is out of its
    /// slot while it runs).
    fn running_in(&self, cell: i32) -> bool {
        self.w
            .pop
            .running
            .as_ref()
            .is_some_and(|r| r.cell_id == cell && !r.removed)
    }

    /// `KillOrganism(GetCell(target), ctx)` asked for by the running
    /// organism `org`: `org` is the state the kill acts on when the cell
    /// holds it (a cell that is its own neighbour), so `SetToDelete()`
    /// reaches it at once.
    fn kill_cell_from(&mut self, org: &mut OrgState, target: i32) {
        let holds_org = self.running_in(target)
            && self
                .w
                .pop
                .running
                .as_ref()
                .is_some_and(|r| r.uid == org.uid);
        self.w.kill_organism(target, holds_org.then_some(org));
    }

    /// `GetCell()->GetCellFaced().GetID()`
    fn faced(&self) -> i32 {
        self.w.pop.cells[self.cell()]
            .cell_faced()
            .unwrap_or(self.cell_id)
    }

    fn res_id(&self, res_id: i32) -> Option<usize> {
        usize::try_from(res_id).ok()
    }

    /// `GetOrganism()`: the organism being run when it lives (or lived, if
    /// killed this step) in this cell, else the cell's occupant.
    fn org_uid(&self) -> OrgUid {
        match self.w.pop.running.as_ref() {
            Some(r) if r.cell_id == self.cell_id => r.uid,
            _ => self.w.org_uid_in_cell(self.cell_id).unwrap_or(OrgUid::NONE),
        }
    }

    /// `m_avatars[av_num]` when `av_num < GetNumAV()`.
    fn av(&self, av_num: i32) -> Option<crate::avatars::Avatar> {
        self.w.avatars.get(self.org_uid(), av_num).copied()
    }

    /// `m_avatars[av_num]` where the C++ only asserts it exists.
    fn av_or_fatal(&self, av_num: i32, method: &str) -> Option<crate::avatars::Avatar> {
        let a = self.av(av_num);
        if a.is_none() {
            self.w.set_fatal(format!(
                "cPopulationInterface::{method} of missing avatar {av_num} (C++: assert)"
            ));
        }
        a
    }

    /// A field of the cell data of avatar `av_num`'s cell (or faced cell);
    /// -1 without the avatar.
    fn av_cell_data(
        &self,
        av_num: i32,
        faced: bool,
        field: fn(&crate::population_cell::CellData) -> i32,
    ) -> i32 {
        let Some(a) = self.av(av_num) else {
            return -1;
        };
        let cell = if faced { a.faced_cell } else { a.cell_id };
        usize::try_from(cell)
            .ok()
            .and_then(|c| self.w.pop.cells.get(c))
            .map_or(-1, |c| field(&c.cell_data))
    }
}

impl WorldServices for PopulationInterface<'_> {
    fn config(&self) -> &Config {
        &self.w.cfg
    }
    fn environment(&self) -> &Env {
        &self.w.env
    }
    fn rng(&mut self) -> &mut AvidaRng {
        &mut self.w.rng
    }
    fn hardware_manager(&self) -> &HardwareManager {
        &self.w.hw
    }
    fn update(&self) -> i32 {
        self.w.stats.update()
    }
    fn tot_creatures(&self) -> i32 {
        self.w.stats.tot_organisms
    }
    fn sense_size(&self) -> usize {
        self.w.stats.sense_size.max(0) as usize
    }
    fn group_attack_inst_count(&self, inst_set: &str) -> usize {
        self.w.group_attack_inst_count(inst_set)
    }
    fn test_mode(&self) -> bool {
        false
    }
    fn analyze_mode(&self) -> bool {
        false
    }
    fn org_fault_reporting(&self) -> bool {
        false
    }
    fn verbosity(&self) -> i32 {
        self.w.cfg.verbosity
    }
    fn new_org_uid(&mut self) -> OrgUid {
        self.w.new_org_uid()
    }
}

/// Fewest phenotypic-plasticity summaries kept before [`prune_phenplast`]
/// looks for dead genotypes.
const PHENPLAST_PRUNE_MIN: usize = 4096;

/// Drops the `cPhenPlastSummary` of every genotype that has left the
/// systematics arena (neither active nor historic): the C++ attaches it
/// to the group and frees it with the group, and a genotype that has left
/// can no longer be asked for. The genotype being tested is kept. Sweeps
/// again when the cache has doubled.
fn prune_phenplast(w: &mut World) {
    let Some(arb) = w.systematics.genotypes() else {
        return;
    };
    let live: std::collections::HashSet<i32> = arb
        .iter_groups()
        .into_iter()
        .chain(arb.historic())
        .filter_map(|h| arb.genotype(h).map(|x| x.id()))
        .collect();
    let cur = w.host.cur_genotype;
    w.host
        .phenplast
        .retain(|id, _| live.contains(id) || Some(*id) == cur);
    w.host.phenplast_prune_at = 2 * w.host.phenplast.len();
}

impl OrgInterface for PopulationInterface<'_> {
    fn output_services(&mut self) -> OutputServices<'_> {
        let c = self.cell();
        let cell_id = self.cell_id;
        let me = self.org_uid();
        // `cEnvironment::GetTaskProbability` may test the genotype's
        // plasticity during the output test (`phenplastbonus`).
        let genotype = if self.w.env.uses_phenplast_bonus() {
            self.own_genotype()
        } else {
            None
        };
        let w = &mut *self.w;
        w.host.cur_genotype = genotype.map(|g| w.systematics.group_id(g).unwrap_or(-1));
        if genotype.is_some()
            && w.host.phenplast.len() >= w.host.phenplast_prune_at.max(PHENPLAST_PRUNE_MIN)
        {
            prune_phenplast(w);
        }
        // The genome to test, unless the summary is attached already.
        let plast_genome = genotype
            .filter(|_| {
                w.host
                    .cur_genotype
                    .is_some_and(|gid| !w.host.phenplast.contains_key(&gid))
            })
            .map(|g| Genome::from_string(&w.systematics.property(g, "genome").string_value()));
        w.host.cur_cell = cell_id;
        w.host.cur_cell_data = w.pop.cells[c].cell_data.contents;
        w.host.cur_inputs.clone_from(&w.pop.cells[c].inputs);
        w.host.cur_resources = w.pop.resource_count.get_frozen_resources(cell_id).to_vec();
        if plast_genome.is_some() {
            w.host.snapshot_plast_kaboom_cells(&w.env, &w.pop);
        }
        w.host.snapshot_kaboom_cells(&w.env, &w.pop, cell_id);
        w.host.use_avatars = w.cfg.use_avatars != 0;
        if w.host.use_avatars {
            let av_cell = w.av_cell_id(me, 0);
            w.host.cur_av_cell = av_cell;
            w.host.cur_av_resources = if av_cell >= 0 {
                w.pop.resource_count.get_frozen_resources(av_cell).to_vec()
            } else {
                Vec::new()
            };
        }
        let update = w.stats.update();
        // `cOrganism::GetDeme()`: every cell belongs to a deme.
        let deme_id = w.pop.cells[c].deme_id as usize;
        if let Some(d) = w.pop.demes.get(deme_id) {
            w.host.cur_deme_size = d.size();
            w.host.cur_deme_event_ids = d.events.event_ids();
            w.host.cur_deme_mov_preds = d.events.mov_pred_events();
        }
        // What `make_test_world` reads of the population and the stats,
        // read now: both are lent out while the test runs.
        let plasticity = plast_genome.map(|genome| {
            let live_orgs = (w.cfg.max_prey_bt != 0).then(|| {
                let running = w.pop.running.as_ref();
                w.pop
                    .live_org_list
                    .iter()
                    .filter_map(|&lc| match w.pop.orgs[lc as usize].as_ref() {
                        Some(o) => Some((o.state.uid, lc, Some(o.state.forage_target))),
                        None => running
                            .filter(|r| r.cell_id == lc)
                            .map(|r| (r.uid, lc, None)),
                    })
                    .collect()
            });
            PlasticityServices {
                hardware_manager: &w.hw,
                group_attack_inst_count: &*w.group_attack_fn,
                update,
                tot_creatures: w.stats.tot_organisms,
                sense_size: w.stats.sense_size.max(0) as usize,
                counts: TestWorldCounts {
                    num_prey: w.pop.num_prey_organisms,
                    num_pred: w.pop.num_pred_organisms,
                    num_top_pred: w.pop.num_top_pred_organisms,
                    creature_age_mean: w.stats.sum_creature_age.average(),
                    creature_age_stddev: w.stats.sum_creature_age.std_deviation(),
                },
                live_orgs,
                test_sterilize: w.test_sterilize,
                genome,
            }
        });
        let deme = w
            .pop
            .demes
            .get_mut(deme_id)
            .map(|d| d as &mut dyn avida_phenotype::phenotype::PhenotypeDeme);
        OutputServices {
            config: &w.cfg,
            environment: &mut w.env,
            rng: &mut w.rng,
            test_mode: false,
            update,
            stats: &mut w.stats,
            deme,
            host: &mut w.host,
            plasticity,
        }
    }
    fn test_sterilize(&self) -> bool {
        self.w.test_sterilize
    }
    fn genotype_test_fitness(&mut self, org: &OrgState) -> f64 {
        let c = self.cell();
        let membership = match self.w.pop.running.as_ref() {
            Some(r) if r.cell_id == self.cell_id && r.removed => r.dead_membership.clone(),
            _ => Some(self.w.pop.membership[c].clone()),
        };
        let Some(g) = membership.and_then(|m| self.w.systematics.genotype_of(&m)) else {
            return 0.0;
        };
        self.w.host.running_forage_target = Some((org.uid, org.forage_target));
        let fitness = self.w.genome_test_metrics(g).map_or(0.0, |m| m.fitness);
        self.w.host.running_forage_target = None;
        fitness
    }
    fn has_genotype(&mut self) -> bool {
        self.own_genotype().is_some()
    }
    fn phylo_distance_to(&mut self, cell: i32) -> Result<i32, String> {
        let own = self
            .own_genotype()
            .ok_or("SexualAncestry of an organism without a genotype")?;
        let other = usize::try_from(cell)
            .ok()
            .and_then(|c| self.w.pop.membership.get(c))
            .and_then(|m| self.w.systematics.genotype_of(m))
            .ok_or("SexualAncestry::GetPhyloDistance on a missing genotype (C++: NULL group)")?;
        let sa = self.w.systematics.sexual_ancestry(own);
        let tsa = self.w.systematics.sexual_ancestry(other);
        match (sa, tsa) {
            (Some(sa), Some(tsa)) => Ok(sa.phylo_distance(&tsa)),
            _ => Err("SexualAncestry of a missing genotype (C++: NULL group)".to_string()),
        }
    }
    fn test_offspring(&mut self, parent: &OrgState, genome: &Genome) -> OffspringTest {
        let w = &mut *self.w;
        w.host.running_forage_target = Some((parent.uid, parent.forage_target));
        let result = {
            let mut tw = make_test_world(
                &w.cfg,
                &mut w.env,
                &mut w.rng,
                &w.hw,
                &mut w.stats,
                &mut w.host,
                &*w.group_attack_fn,
                w.test_sterilize,
                &w.pop,
            );
            test_cpu::test_offspring(&mut tw, genome)
        };
        w.host.running_forage_target = None;
        w.flush_host();
        match result {
            Ok(t) => t,
            Err(e) => {
                w.set_fatal(e);
                OffspringTest::default()
            }
        }
    }
    fn stats_inc_resamplings(&mut self) {
        self.w.stats.num_resamplings += 1;
    }
    fn stats_inc_failed_resamplings(&mut self) {
        self.w.stats.num_failed_resamplings += 1;
    }
    fn stats_add_new_task_count(&mut self, task: i32) {
        if let Some(slot) = usize::try_from(task)
            .ok()
            .and_then(|t| self.w.stats.new_task_count.get_mut(t))
        {
            *slot += 1;
        }
    }
    fn with_organism(&mut self, uid: OrgUid, f: &mut dyn FnMut(&mut Organism)) -> bool {
        let found = self.w.pop.live_org_list.iter().copied().find(|&c| {
            self.w.pop.orgs[c as usize]
                .as_ref()
                .is_some_and(|o| o.state.uid == uid)
        });
        let Some(c) = found else {
            return false;
        };
        let Some(mut org) = self.w.pop.orgs[c as usize].take() else {
            return false;
        };
        f(&mut org);
        self.w.pop.orgs[c as usize] = Some(org);
        true
    }

    fn with_organism_in_cell(&mut self, cell_id: i32, f: &mut dyn FnMut(&mut Organism)) -> bool {
        let Some(slot) = usize::try_from(cell_id)
            .ok()
            .and_then(|c| self.w.pop.orgs.get_mut(c))
        else {
            return false;
        };
        let Some(mut org) = slot.take() else {
            return false;
        };
        f(&mut org);
        self.w.pop.orgs[cell_id as usize] = Some(org);
        true
    }

    fn live_org_ids(&self) -> Vec<i32> {
        let running = self.w.pop.running.as_ref();
        self.w
            .pop
            .live_org_list
            .iter()
            .filter_map(|&c| match self.w.pop.orgs[c as usize].as_ref() {
                Some(o) => Some(o.state.id),
                None => running.filter(|r| r.cell_id == c).map(|r| r.org_id),
            })
            .collect()
    }
    fn live_org_uids(&self) -> Vec<OrgUid> {
        let running = self.w.pop.running.as_ref();
        self.w
            .pop
            .live_org_list
            .iter()
            .filter_map(|&c| match self.w.pop.orgs[c as usize].as_ref() {
                Some(o) => Some(o.state.uid),
                None => running.filter(|r| r.cell_id == c).map(|r| r.uid),
            })
            .collect()
    }
    fn live_org_forage_targets(&self) -> Result<Vec<(OrgUid, i32, Option<i32>)>, String> {
        let running = self.w.pop.running.as_ref();
        Ok(self
            .w
            .pop
            .live_org_list
            .iter()
            .filter_map(|&c| match self.w.pop.orgs[c as usize].as_ref() {
                Some(o) => Some((o.state.uid, c, Some(o.state.forage_target))),
                None => running.filter(|r| r.cell_id == c).map(|r| (r.uid, c, None)),
            })
            .collect())
    }
    fn find_live_org(&self, id: i32) -> Option<OrgUid> {
        let running = self.w.pop.running.as_ref();
        self.w
            .pop
            .live_org_list
            .iter()
            .find_map(|&c| match self.w.pop.orgs[c as usize].as_ref() {
                Some(o) => (o.state.id == id).then_some(o.state.uid),
                None => running
                    .filter(|r| r.cell_id == c && r.org_id == id)
                    .map(|r| r.uid),
            })
    }
    fn get_cell_id(&self) -> i32 {
        self.cell_id
    }
    fn get_deme_id(&self) -> i32 {
        self.w.pop.cells[self.cell()].deme_id
    }
    fn set_cell_id(&mut self, id: i32) {
        self.cell_id = id;
    }
    fn set_deme_id(&mut self, _id: i32) {}
    fn get_cell_x_position(&self) -> i32 {
        // `GetDeme(deme_id).GetCellPosition(cell_id).first`
        let d = self.w.pop.cells[self.cell()].deme_id as usize;
        self.w.pop.demes[d].cell_position(self.cell_id).0
    }
    fn get_cell_y_position(&self) -> i32 {
        let d = self.w.pop.cells[self.cell()].deme_id as usize;
        self.w.pop.demes[d].cell_position(self.cell_id).1
    }

    // `UpdateCellDataExpired()` is a no-op: `MARKING_EXPIRE_DATE != -1` is
    // rejected at start-up.
    fn get_cell_data(&self) -> i32 {
        self.w.pop.cells[self.cell()].cell_data.contents
    }
    fn get_cell_data_org_id(&self) -> i32 {
        self.w.pop.cells[self.cell()].cell_data.org_id
    }
    fn get_cell_data_update(&self) -> i32 {
        self.w.pop.cells[self.cell()].cell_data.update
    }
    fn get_cell_data_territory(&self) -> i32 {
        self.w.pop.cells[self.cell()].cell_data.territory
    }
    fn get_cell_data_forager_type(&self) -> i32 {
        self.w.pop.cells[self.cell()].cell_data.forager
    }
    fn get_faced_cell_data(&self) -> i32 {
        self.w.pop.cells[self.faced() as usize].cell_data.contents
    }
    fn get_faced_cell_data_org_id(&self) -> i32 {
        self.w.pop.cells[self.faced() as usize].cell_data.org_id
    }
    fn get_faced_cell_data_update(&self) -> i32 {
        self.w.pop.cells[self.faced() as usize].cell_data.update
    }
    fn get_faced_cell_data_territory(&self) -> i32 {
        self.w.pop.cells[self.faced() as usize].cell_data.territory
    }

    fn get_prev_seen_cell_id(&self) -> i32 {
        self.w.host.prev_seen[self.cell()]
    }
    fn get_prev_task_cell_id(&self) -> i32 {
        self.w.host.prev_task[self.cell()]
    }
    fn get_num_task_cells_reached(&self) -> i32 {
        self.w.host.num_task_cells[self.cell()]
    }
    fn add_reached_task_cell(&mut self) {
        let c = self.cell();
        self.w.host.num_task_cells[c] += 1;
    }
    fn set_prev_seen_cell_id(&mut self, id: i32) {
        let c = self.cell();
        self.w.host.prev_seen[c] = id;
    }
    fn set_prev_task_cell_id(&mut self, id: i32) {
        let c = self.cell();
        self.w.host.prev_task[c] = id;
    }

    fn divide(&mut self, parent: &mut OrgState, offspring_genome: &Genome) -> bool {
        let cell_id = self.cell_id;
        self.w.host.running_forage_target = Some((parent.uid, parent.forage_target));
        let parent_alive = self
            .w
            .activate_offspring(offspring_genome, parent, None, cell_id);
        self.w.host.running_forage_target = None;
        parent_alive
    }
    fn divide_with_traces(
        &mut self,
        parent: &mut OrgState,
        parent_hw: &dyn Hardware,
        offspring_genome: &Genome,
        traces: &mut avida_hardware::mini_trace::LentTraces,
    ) -> bool {
        // The population works on the parent's traces through
        // `MiniTraceState::lent` while the offspring are placed.
        let cell_id = self.cell_id;
        self.w.mini_trace.lent = Some(std::mem::take(traces));
        self.w.host.running_forage_target = Some((parent.uid, parent.forage_target));
        let parent_alive =
            self.w
                .activate_offspring(offspring_genome, parent, Some(parent_hw), cell_id);
        self.w.host.running_forage_target = None;
        if let Some(t) = self.w.mini_trace.lent.take() {
            *traces = t;
        }
        parent_alive
    }
    fn with_neighbor(&mut self, f: &mut dyn FnMut(&mut Organism)) -> Faced {
        let faced = self.faced();
        let n = faced as usize;
        if let Some(mut org) = self.w.pop.orgs[n].take() {
            f(&mut org);
            self.w.pop.orgs[n] = Some(org);
            return Faced::Other;
        }
        if !self.running_in(faced) {
            return Faced::Empty;
        }
        if faced != self.cell_id {
            // Only the running organism's own interface executes
            // instructions, so the organism that is out of its cell is the
            // one asking.
            self.unsupported("GetNeighbor of the running organism from another organism");
            return Faced::Empty;
        }
        // A cell connected to itself: the running organism faces itself.
        Faced::SelfOrg
    }
    fn is_neighbor_cell_occupied(&self) -> bool {
        self.w.is_occupied(self.faced())
    }
    fn get_num_neighbors(&self) -> i32 {
        self.w.pop.cells[self.cell()].connections.len() as i32
    }
    fn get_neighborhood_cell_ids(&self) -> Vec<i32> {
        self.w.pop.cells[self.cell()].connections.iter().collect()
    }
    fn get_facing(&self) -> i32 {
        let f = &self.w.pop.cells[self.faced() as usize];
        self.w.pop.cells[self.cell()].facing((f.x, f.y))
    }
    fn get_faced_cell_id(&self) -> i32 {
        self.faced()
    }
    fn get_faced_dir(&self) -> i32 {
        let f = &self.w.pop.cells[self.faced() as usize];
        self.w.pop.cells[self.cell()].faced_dir((f.x, f.y))
    }
    fn get_neighbor_cell_contents(&self) -> i32 {
        self.w.pop.cells[self.faced() as usize].cell_data.contents
    }
    fn rotate(&mut self, direction: i32) {
        if self.w.cfg.use_avatars != 0 {
            let me = self.org_uid();
            self.w.rotate_av(me, if direction >= 0 { 1 } else { -1 }, 0);
            return;
        }
        let c = self.cell();
        if direction >= 0 {
            self.w.pop.cells[c].connections.circ_next();
        } else {
            self.w.pop.cells[c].connections.circ_prev();
        }
    }

    fn get_input_at(&mut self, input_pointer: &mut i32) -> i32 {
        let cell = &self.w.pop.cells[self.cell()];
        if cell.inputs.is_empty() {
            // Only reachable when `MixPopulation` moved the organism into a
            // cell that never had an organism born into it: the C++ takes
            // the modulus by zero and reads `m_inputs[0]` of an empty array
            // (the reference binary segfaults).
            self.w.set_fatal(
                "unsupported: an organism reads the inputs of a cell that never had any \
                 (C++: cPopulationCell::GetInputAt on an empty m_inputs, a NULL read; \
                 MixPopulation moved it there)",
            );
            return 0;
        }
        cell.input_at(input_pointer)
    }
    fn reset_inputs(&mut self) {
        let c = self.cell();
        let w = &mut *self.w;
        w.env
            .setup_inputs(&mut w.rng, &mut w.pop.cells[c].inputs, true);
    }
    fn get_inputs(&self) -> Vec<i32> {
        self.w.pop.cells[self.cell()].inputs.clone()
    }
    fn get_resources(&mut self, org: &mut OrgState) -> Vec<f64> {
        let cell_id = self.cell_id;
        self.get_cell_resources(org, cell_id)
    }
    fn get_resource_val(&mut self, org: &mut OrgState, res_id: i32) -> f64 {
        let cell_id = self.cell_id;
        self.get_cell_res_val(org, cell_id, res_id)
    }
    fn get_faced_cell_resources(&mut self, org: &mut OrgState) -> Vec<f64> {
        let faced = self.faced();
        self.get_cell_resources(org, faced)
    }
    fn get_faced_resource_val(&mut self, org: &mut OrgState, res_id: i32) -> f64 {
        let faced = self.faced();
        self.get_cell_res_val(org, faced, res_id)
    }
    /// `GetDemeResources(deme_id, ctx)`: `GetDemeCellResources` of this
    /// organism's cell.
    fn get_deme_resources(&mut self, deme_id: i32) -> Vec<f64> {
        let cell_id = self.cell_id;
        self.get_deme_cell_resources(deme_id, cell_id)
    }
    fn get_cell_resources(&mut self, org: &mut OrgState, cell_id: i32) -> Vec<f64> {
        self.w.with_pop_resources_for(Some(org), |rc, rng, env| {
            rc.get_cell_resources(cell_id, rng, env).to_vec()
        })
    }
    fn get_frozen_cell_res_val(&mut self, cell_id: i32, res_id: i32) -> f64 {
        match self.res_id(res_id) {
            Some(r) => self
                .w
                .pop
                .resource_count
                .get_frozen_cell_res_val(cell_id, r),
            None => 0.0,
        }
    }
    fn get_cell_res_val(&mut self, org: &mut OrgState, cell_id: i32, res_id: i32) -> f64 {
        let Some(r) = self.res_id(res_id) else {
            return 0.0;
        };
        self.w.with_pop_resources_for(Some(org), |rc, rng, env| {
            rc.get_cell_res_val(rng, env, cell_id, r)
        })
    }
    fn get_frozen_resources(&mut self, cell_id: i32) -> Vec<f64> {
        self.w
            .pop
            .resource_count
            .get_frozen_resources(cell_id)
            .to_vec()
    }
    fn get_cell_id_lists(&self) -> Vec<Vec<i32>> {
        self.w.pop.resource_count.get_cell_id_lists().to_vec()
    }
    fn get_curr_peak_x(&mut self, org: &mut OrgState, res_id: i32) -> i32 {
        let Some(r) = self.res_id(res_id) else {
            return 0;
        };
        self.w
            .with_pop_resources_for(Some(org), |rc, rng, env| rc.get_curr_peak_x(rng, env, r))
    }
    fn get_curr_peak_y(&mut self, org: &mut OrgState, res_id: i32) -> i32 {
        let Some(r) = self.res_id(res_id) else {
            return 0;
        };
        self.w
            .with_pop_resources_for(Some(org), |rc, rng, env| rc.get_curr_peak_y(rng, env, r))
    }
    fn get_frozen_peak_x(&mut self, res_id: i32) -> i32 {
        match self.res_id(res_id) {
            Some(r) => self.w.pop.resource_count.get_frozen_peak_x(r),
            None => 0,
        }
    }
    fn get_frozen_peak_y(&mut self, res_id: i32) -> i32 {
        match self.res_id(res_id) {
            Some(r) => self.w.pop.resource_count.get_frozen_peak_y(r),
            None => 0,
        }
    }
    fn trigger_do_updates(&mut self, org: &mut OrgState) {
        self.w
            .with_pop_resources_for(Some(org), |rc, rng, env| rc.update_resources(rng, env));
    }
    fn update_resources(&mut self, org: &mut OrgState, res_change: &[f64]) {
        let cell_id = self.cell_id;
        self.w.with_pop_resources_for(Some(org), |rc, rng, env| {
            rc.modify_cell(rng, env, res_change, cell_id);
        });
    }

    /// `UpdateDemeResources(ctx, res_change)`: `ModifyDemeResCount` of the
    /// cell's deme at the deme-relative cell.
    fn update_deme_resources(&mut self, res_change: &[f64]) {
        let c = self.cell();
        let w = &mut *self.w;
        let deme = &mut w.pop.demes[w.pop.cells[c].deme_id as usize];
        let rel = deme.relative_cell_id(self.cell_id);
        deme.resource_count
            .modify_cell(&mut w.rng, &mut w.res_env, res_change, rel);
    }

    fn die(&mut self, org: &mut OrgState) {
        let cell_id = self.cell_id;
        self.w.kill_organism(cell_id, Some(org));
    }
    fn kill_cell_id(&mut self, org: &mut OrgState, target: i32) {
        self.kill_cell_from(org, target);
    }
    fn receive_value(&mut self, org: &mut OrgState) -> i32 {
        let c = self.cell();
        let num_neighbors = self.w.pop.cells[c].connections.len();
        for _ in 0..num_neighbors {
            self.w.pop.cells[c].connections.circ_next();
            let faced = self.faced();
            // A cell connected to itself: the running organism.
            let is_self = faced == self.cell_id && self.running_in(faced);
            let neighbor = match self.w.pop.orgs[faced as usize].as_mut() {
                Some(o) => &mut o.state,
                None if is_self => &mut *org,
                None => continue,
            };
            if !neighbor.sent_active {
                continue;
            }
            // `RetrieveSentValue()`
            neighbor.sent_active = false;
            return neighbor.sent_value;
        }
        0
    }
    /// `cPopulation::UpdateMerit(ctx, cell_id, new_merit)`.
    fn update_merit(&mut self, org: &mut OrgState, new_merit: f64) -> bool {
        let cell_id = self.cell_id;
        if new_merit <= 0.0 {
            // The C++ goes on to dereference the emptied cell's organism.
            self.w.kill_organism(cell_id, Some(org));
            self.w.set_fatal(crate::movement::ZERO_MERIT_STOP);
            return true;
        }
        let phenotype = &mut org.phenotype;
        let old_merit = phenotype.merit.get_double();
        phenotype.merit = Merit::new(new_merit);
        phenotype.life_fitness = new_merit / f64::from(phenotype.gestation_time);
        if new_merit <= old_merit {
            phenotype.is_donor_cur = true;
        } else {
            phenotype.is_receiver = true;
        }
        self.w.adjust_schedule(cell_id, new_merit);
        true
    }
    fn test_on_divide(&self) -> bool {
        self.w.test_on_div
    }
    /// `cPopulationInterface::GetStateGridID(ctx)`: a random state grid of
    /// the environment. The running organism's draw is mirrored for the
    /// task host (`WorldHost::cur_sg`).
    fn get_state_grid_id(&mut self) -> i32 {
        let n = self.w.env.num_state_grids();
        let sg = self.w.rng.get_uint(n as u32) as i32;
        if self.w.pop.running.as_ref().map(|r| r.cell_id) == Some(self.cell_id) {
            self.w.host.cur_sg = sg;
        }
        sg
    }
    /// `AddLiveOrg()` / `RemoveLiveOrg()`: only the avatar and movement
    /// code call these; the population keeps the list itself.
    fn add_live_org(&mut self) {
        self.unsupported("AddLiveOrg");
    }
    fn remove_live_org(&mut self) {
        self.unsupported("RemoveLiveOrg");
    }

    fn has_opinion(&self, org: &OrgState) -> bool {
        org.opinions.as_ref().is_some_and(|o| !o.is_empty())
    }
    fn set_opinion(&mut self, opinion: i32, org: &mut OrgState) {
        let update = self.w.stats.update();
        if let Err(e) = org.set_opinion(&self.w.cfg, update, opinion) {
            self.w.set_fatal(e);
        }
    }
    fn clear_opinion(&mut self, org: &mut OrgState) {
        org.clear_opinion();
    }

    // ------------------------------------------------------------------
    // Not ported: each records a fatal error.
    // ------------------------------------------------------------------
    /// `cell.SetCellData(newData, cell.GetOrganism()->GetID())` on this
    /// cell, whose occupant is the running organism `org`.
    fn set_cell_data(&mut self, org: &mut OrgState, data: i32) {
        let cell = self.cell_id;
        let update = self.w.stats.update();
        if self.running_in(cell) {
            self.w
                .set_running_cell_data(cell, data, org.id, update, org);
            return;
        }
        // The running organism has left its cell (killed this step): the
        // C++ reads whoever occupies it now.
        let occupant = self.w.pop.orgs[self.cell()].as_ref().map(|o| o.state.id);
        match occupant {
            Some(id) => self.w.set_cell_data(cell, data, id, update),
            None => self.unsupported("SetCellData on an empty cell (C++: NULL organism)"),
        }
    }
    /// `GetLGTFragment(ctx, region, dest_genome, seq)` (crate::hgt)
    fn get_lgt_fragment(
        &mut self,
        region: i32,
        dest_genome: &Genome,
    ) -> Option<InstructionSequence> {
        let cell_id = self.cell_id;
        self.w.lgt_fragment(cell_id, region, dest_genome)
    }
    /// `UpdateRandomResources(ctx, res_change)`: `UpdateCellResources` at
    /// `rand() % (WORLD_X * WORLD_Y)`, drawn from the C library generator.
    /// The running organism's state is not passed in, so a damaging
    /// gradient that injures it here stops the run.
    fn update_random_resources(&mut self, res_change: &[f64]) {
        let size = self.w.cfg.world_x * self.w.cfg.world_y;
        if size <= 0 {
            self.unsupported("UpdateRandomResources in an empty world (C++: rand() % 0)");
            return;
        }
        let random_id = self.w.host.libc_rand.next_value() % size;
        self.w.with_pop_resources(|rc, rng, env| {
            rc.modify_cell(rng, env, res_change, random_id);
        });
    }
    /// `m_world->GetPopulation().Kaboom(cell, ctx, distance)` (kaboom.rs)
    fn kaboom(&mut self, org: &mut OrgState, distance: i32) {
        if let Err(e) = self.w.kaboom(self.cell_id, org, distance) {
            self.w.set_fatal(e);
        }
    }
    /// `m_world->GetPopulation().Kaboom(cell, ctx, distance, effect)`
    fn kaboom_effect(&mut self, org: &mut OrgState, distance: i32, effect: f64) {
        if let Err(e) = self.w.kaboom_effect(self.cell_id, org, distance, effect) {
            self.w.set_fatal(e);
        }
    }
    /// `m_world->GetPopulation().SpawnDeme(cell.GetDemeID(), ctx)`
    fn spawn_deme(&mut self, org: &mut OrgState) {
        let deme_id = self.w.pop.cells[self.cell()].deme_id;
        if let Err(e) = self.w.spawn_deme(deme_id, self.cell_id, org) {
            self.w.set_fatal(e);
        }
    }
    /// `Inst_ApplyPointMutationsGroup*` (deme_mutation.rs)
    fn point_mutate_deme_member(
        &mut self,
        org: &mut OrgState,
        hw: &mut dyn avida_hardware::Hardware,
        deme_id: i32,
        prob: f64,
        germline: Option<bool>,
    ) {
        let cell = self.cell_id;
        self.w
            .point_mutate_deme_member(cell, org, hw, deme_id, prob, germline);
    }
    /// Step output joins the event actions' stdout, which the run loop
    /// flushes before the update's status line.
    fn cout(&mut self, text: &str) {
        self.w.action_state.stdout.push_str(text);
    }
    /// `m_world->GetPopulation().ActivateParasite(host, parent, label,
    /// injected_code)`
    fn inject_parasite(
        &mut self,
        host: &mut OrgState,
        host_hw: &mut dyn avida_hardware::Hardware,
        parent: Option<std::sync::Arc<avida_hardware::hardware_transsmt::ParasiteUnit>>,
        label: &str,
        injected_code: &InstructionSequence,
    ) -> bool {
        let cell = self.cell_id;
        match self
            .w
            .activate_parasite(cell, host, host_hw, parent, label, injected_code)
        {
            Ok(b) => b,
            Err(e) => {
                self.w.set_fatal(e);
                false
            }
        }
    }
    /// Messaging: see messaging.rs.
    fn send_message(&mut self, msg: &mut OrgMessage) -> bool {
        self.w.send_message(self.cell_id, msg)
    }
    fn broadcast_message(&mut self, msg: &mut OrgMessage, depth: i32) -> bool {
        self.w.broadcast_message(self.cell_id, msg, depth)
    }
    fn bcast_alarm(&mut self, jump_label: i32, bcast_range: i32) -> bool {
        self.w.bcast_alarm(self.cell_id, jump_label, bcast_range)
    }
    fn take_self_deliveries(&mut self) -> Vec<avida_hardware::org_interface::SelfDelivery> {
        self.w.messaging.take_self_deliveries()
    }
    /// `DivideOrgTestamentAmongDeme(value)`: every organism of the deme,
    /// the caller included, gets `value / GetOrgCount()` as testament. The
    /// caller (`Inst_RelinquishEnergyToOrganismsInDeme`) dies right after
    /// and never applies its own share, so the running organism, which is
    /// out of reach here, is skipped.
    fn divide_org_testament_among_deme(&mut self, value: f64) {
        let deme_id = self.w.deme_of(self.cell_id);
        let deme = &self.w.pop.demes[deme_id];
        let share = value / f64::from(deme.cur_org_count);
        let cells = deme.cell_ids.clone();
        for cell in cells {
            if self.running_in(cell) {
                continue;
            }
            if let Some(Some(o)) = usize::try_from(cell)
                .ok()
                .and_then(|c| self.w.pop.orgs.get_mut(c))
            {
                o.state.phenotype.energy_testament(share);
            }
        }
    }
    fn send_flash(&mut self) {
        self.w.send_flash(self.cell_id);
    }
    // Reputation rotation (reputation.rs).
    fn rotate_to_greatest_reputation(&mut self, org: &mut OrgState) {
        let filter = crate::reputation::ReputationFilter::Any;
        self.w
            .rotate_to_greatest_reputation(self.cell_id, org, filter);
    }
    fn rotate_to_greatest_reputation_with_different_tag(&mut self, org: &mut OrgState, tag: i32) {
        let filter = crate::reputation::ReputationFilter::OtherTag(tag);
        self.w
            .rotate_to_greatest_reputation(self.cell_id, org, filter);
    }
    fn rotate_to_greatest_reputation_with_different_lineage(
        &mut self,
        org: &mut OrgState,
        line: i32,
    ) {
        let filter = crate::reputation::ReputationFilter::OtherLineage(line);
        self.w
            .rotate_to_greatest_reputation(self.cell_id, org, filter);
    }
    // The deme network: the first `GetNetwork()` aborts the run in the
    // reference build (see `crate::deme_events`).
    fn create_link_by_facing(&mut self, _weight: f64) {
        self.w.deme_network_abort();
    }
    fn create_link_by_xy(&mut self, _x: i32, _y: i32, _weight: f64) {
        self.w.deme_network_abort();
    }
    fn create_link_by_index(&mut self, _idx: i32, _weight: f64) {
        self.w.deme_network_abort();
    }
    fn network_broadcast(&mut self, _msg: &mut OrgMessage) -> bool {
        self.w.deme_network_abort();
        false
    }
    fn network_unicast(&mut self, _msg: &mut OrgMessage) -> bool {
        self.w.deme_network_abort();
        false
    }
    fn network_rotate(&mut self, _x: i32) -> bool {
        self.w.deme_network_abort();
        false
    }
    fn network_select(&mut self, _x: i32) -> bool {
        self.w.deme_network_abort();
        false
    }
    // Deme input and output: no code calls `cPopulationCell::SetCanInput`
    // or `SetCanOutput`, so `GetCanInput()` and `GetCanOutput()` are always
    // false.
    fn get_next_deme_input(&mut self) -> i32 {
        -1
    }
    fn do_deme_input(&mut self, _value: i32) {}
    fn do_deme_output(&mut self, _value: i32) {}
    fn do_hgt_donation(&mut self, _org: &mut OrgState) {
        self.unsupported("do_hgt_donation");
    }
    fn do_hgt_conjugation(&mut self, _org: &mut OrgState) {
        self.unsupported("do_hgt_conjugation");
    }
    /// `DoHGTMutation(ctx, offspring)` (crate::hgt)
    fn do_hgt_mutation(&mut self, org: &mut OrgState) {
        let mut offspring = std::mem::take(&mut org.offspring_genome);
        self.w.hgt_mutation(self.cell_id, org, &mut offspring);
        org.offspring_genome = offspring;
    }
    /// `ReceiveHGTDonation(fragment)`: pending until the next divide.
    fn receive_hgt_donation(&mut self, fragment: &InstructionSequence) {
        let uid = self.org_uid();
        self.w
            .host
            .hgt
            .pending
            .entry(uid)
            .or_default()
            .push_back(fragment.clone());
    }
    /// `Move(ctx, src_id, dest_id)`: `MoveOrganisms(ctx, src_id, dest_id,
    /// -1)`; a successful move puts the organism in `dest_id`
    /// (`InsertOrganism` sets the interface's cell id).
    fn move_org(&mut self, org: &mut OrgState, src_id: i32, dest_id: i32) -> bool {
        let moved = self.w.move_organisms(src_id, dest_id, -1, org);
        if let Some(r) = self.w.pop.running.as_ref() {
            self.cell_id = r.cell_id;
        }
        moved
    }
    fn join_group(&mut self, org: &mut OrgState, group_id: i32) {
        let key = self.cell_id;
        self.w.join_group(&mut [(key, org)], key, group_id);
    }
    fn make_group(&mut self, org: &mut OrgState) {
        let key = self.cell_id;
        self.w.make_group(&mut [(key, org)], key);
    }
    fn leave_group(&mut self, org: &mut OrgState, group_id: i32) {
        let key = self.cell_id;
        self.w.leave_group(&mut [(key, org)], key, group_id);
    }
    fn number_of_organisms_in_group(&mut self, group_id: i32) -> i32 {
        self.w.number_of_organisms_in_group(group_id)
    }
    fn number_group_females(&mut self, group_id: i32) -> i32 {
        self.w.number_group_females(group_id)
    }
    fn number_group_males(&mut self, group_id: i32) -> i32 {
        self.w.number_group_males(group_id)
    }
    fn number_group_juvs(&mut self, group_id: i32) -> i32 {
        self.w.number_group_juvs(group_id)
    }
    fn change_group_mating_types(
        &mut self,
        org: &mut OrgState,
        group_id: i32,
        old_type: i32,
        new_type: i32,
    ) {
        let key = self.cell_id;
        self.w
            .change_group_mating_types(&mut [(key, org)], key, group_id, old_type, new_type);
    }
    fn inc_tolerance(&mut self, org: &mut OrgState, tolerance_type: i32) -> i32 {
        let key = self.cell_id;
        self.w.inc_tolerance(org, key, tolerance_type)
    }
    fn dec_tolerance(&mut self, org: &mut OrgState, tolerance_type: i32) -> i32 {
        let key = self.cell_id;
        self.w.dec_tolerance(org, key, tolerance_type)
    }
    fn calc_group_tolerance_immigrants(
        &mut self,
        org: &mut OrgState,
        target_group_id: i32,
        mating_type: i32,
    ) -> i32 {
        let key = self.cell_id;
        self.w
            .calc_group_tolerance_immigrants(&mut [(key, org)], target_group_id, mating_type)
    }
    fn calc_group_tolerance_offspring(&mut self, parent: &mut OrgState) -> i32 {
        let key = self.cell_id;
        self.w
            .calc_group_tolerance_offspring(&mut [(key, parent)], key)
    }
    fn calc_group_odds_immigrants(
        &mut self,
        org: &mut OrgState,
        group_id: i32,
        mating_type: i32,
    ) -> f64 {
        let key = self.cell_id;
        self.w
            .calc_group_odds_immigrants(&mut [(key, org)], group_id, mating_type)
    }
    fn calc_group_odds_offspring_parent(&mut self, parent: &mut OrgState) -> f64 {
        let key = self.cell_id;
        self.w
            .calc_group_odds_offspring_parent(&mut [(key, parent)], key)
    }
    fn calc_group_odds_offspring(&mut self, org: &mut OrgState, group_id: i32) -> f64 {
        let key = self.cell_id;
        self.w
            .calc_group_odds_offspring(&mut [(key, org)], group_id)
    }
    fn attempt_immigrate_group(&mut self, group_id: i32, org: &mut OrgState) -> bool {
        let key = self.cell_id;
        self.w
            .attempt_immigrate_group(&mut [(key, org)], key, group_id)
    }
    fn push_tolerance_inst_exe(&mut self, org: &mut OrgState, tol_inst: i32) {
        let key = self.cell_id;
        self.w.push_tolerance_inst_exe(org, key, tol_inst);
    }
    /// `GetGroupIntolerances(group_id, tol_num, mating_type)` (with its
    /// QUIRK, see [`crate::groups::Groups::group_intolerance_slot`]).
    fn get_group_intolerances(&mut self, group_id: i32, tol_num: i32, mating_type: i32) -> i32 {
        self.w
            .pop
            .groups
            .group_intolerance_slot(group_id, tol_num, mating_type)
            .map_or(0, |v| *v)
    }
    fn set_group_intolerances(
        &mut self,
        group_id: i32,
        tol_num: i32,
        mating_type: i32,
        value: i32,
    ) {
        if let Some(v) = self
            .w
            .pop
            .groups
            .group_intolerance_slot(group_id, tol_num, mating_type)
        {
            *v = value;
        }
    }
    /// `cPopulationInterface::TryWriteGroupAttackBits` (`cStats::PrintGroupAttackBits`).
    fn try_write_group_attack_bits(&mut self, raw_bits: u8) {
        let w = &mut *self.w;
        if let Err(e) = w.stats.print_group_attack_bits(&mut w.out, raw_bits) {
            w.set_fatal(e);
        }
    }
    /// `cPopulationInterface::TryWriteGroupAttackString` (`cStats::PrintGroupAttackString`).
    fn try_write_group_attack_string(&mut self, s: &str) {
        let w = &mut *self.w;
        if let Err(e) = w.stats.print_group_attack_string(&mut w.out, s) {
            w.set_fatal(e);
        }
    }
    fn dec_num_prey_organisms(&mut self) {
        self.w.pop.num_prey_organisms -= 1;
    }
    fn dec_num_pred_organisms(&mut self) {
        self.w.pop.num_pred_organisms -= 1;
    }
    fn dec_num_top_pred_organisms(&mut self) {
        self.w.pop.num_top_pred_organisms -= 1;
    }
    fn inc_num_prey_organisms(&mut self) {
        self.w.pop.num_prey_organisms += 1;
    }
    fn inc_num_pred_organisms(&mut self) {
        self.w.pop.num_pred_organisms += 1;
    }
    fn inc_num_top_pred_organisms(&mut self) {
        self.w.pop.num_top_pred_organisms += 1;
    }
    /// `cPopulation::AttackFacedOrg(ctx, loser)`: `KillOrganism` of the
    /// loser's cell.
    fn attack_faced_org(&mut self, org: &mut OrgState, loser: i32) {
        self.kill_cell_from(org, loser);
    }
    /// Only writes with `TRACK_BIRTH_LOCS` (`cStats::PrintBirthLocData` of
    /// `GetLiveOrgList()[org->GetOrgIndex()]`, which is `org`;
    /// `pred_prey.rs`).
    fn try_write_birth_loc_data(&mut self, org: &OrgState) {
        self.w.try_write_birth_loc_data(org);
    }
    /// `cPopulationInterface::InjectPreyClone(ctx, gen_id)` with `gen_id`
    /// the genotype of the running organism `org` (`pred_prey.rs`).
    fn inject_prey_clone(&mut self, org: &mut OrgState) {
        let gen_id = self
            .own_genotype()
            .and_then(|g| self.w.systematics.group_id(g));
        let Some(gen_id) = gen_id else {
            self.w.set_fatal(
                "InjectPreyClone from an organism without a genotype (C++: NULL \
                 SystematicsGroup(\"genotype\"))",
            );
            return;
        };
        let cell = self.cell_id;
        if let Err(e) = self.w.inject_prey_clone(cell, org, gen_id) {
            self.w.set_fatal(e);
        }
    }
    fn kill_rand_pred(&mut self, org: &mut OrgState) {
        let cell_id = self.cell_id;
        self.w.kill_rand_pred(cell_id, org);
    }
    fn kill_rand_prey(&mut self, org: &mut OrgState) {
        let cell_id = self.cell_id;
        self.w.kill_rand_prey(cell_id, org);
    }
    /// `cPopulationInterface::TryWriteLookData`: only writes with
    /// `TRACK_LOOK_SETTINGS` (`cStats::PrintLookData`).
    fn try_write_look_data(&mut self, s: &str) {
        let w = &mut *self.w;
        if w.cfg.track_look_settings != 0 {
            if let Err(e) = w.stats.print_look_data(&mut w.out, s) {
                w.set_fatal(e);
            }
        }
    }
    /// `cPopulationInterface::TryWriteLookOutput`: only writes with
    /// `TRACK_LOOK_OUTPUT` (`cStats::PrintLookDataOutput`).
    fn try_write_look_output(&mut self, s: &str) {
        let w = &mut *self.w;
        if w.cfg.track_look_output != 0 {
            if let Err(e) = w.stats.print_look_data_output(&mut w.out, s) {
                w.set_fatal(e);
            }
        }
    }
    /// `cPopulationInterface::TryWriteLookEXOutput`: only writes with
    /// `TRACK_LOOK_OUTPUT` (`cStats::PrintLookEXDataOutput`).
    fn try_write_look_ex_output(&mut self, s: &str) {
        let w = &mut *self.w;
        if w.cfg.track_look_output != 0 {
            if let Err(e) = w.stats.print_look_ex_data_output(&mut w.out, s) {
                w.set_fatal(e);
            }
        }
    }
    fn get_formed_group_array(&mut self) -> Vec<i32> {
        self.w.formed_group_array()
    }
    // ------------------------------------------------------------------
    // Avatars (`USE_AVATARS`, see crate::avatars). An absent avatar
    // (`av_num >= GetNumAV()`) gives the C++ fallback value.
    // ------------------------------------------------------------------
    fn has_output_av(&self, av_num: i32) -> bool {
        let me = self.org_uid();
        self.av(av_num)
            .is_some_and(|a| self.w.cell_has_output_av_for(a.cell_id, me))
    }
    fn faced_has_output_av(&self, av_num: i32) -> bool {
        let me = self.org_uid();
        self.av(av_num)
            .is_some_and(|a| self.w.cell_has_output_av_for(a.faced_cell, me))
    }
    fn faced_has_av(&self, av_num: i32) -> bool {
        self.av(av_num)
            .is_some_and(|a| self.w.avatars.has_av(a.faced_cell))
    }
    fn faced_has_pred_av(&self, av_num: i32) -> bool {
        self.av(av_num)
            .is_some_and(|a| self.w.avatars.counts(a.faced_cell).0 > 0)
    }
    fn faced_has_prey_av(&self, av_num: i32) -> bool {
        self.av(av_num)
            .is_some_and(|a| self.w.avatars.counts(a.faced_cell).1 > 0)
    }
    fn add_io_av(&mut self, av_cell_id: i32, av_facing: i32, input: bool, output: bool) {
        let me = self.org_uid();
        self.w.add_io_av(me, av_cell_id, av_facing, input, output);
    }
    /// `AddPredPreyAV(ctx, av_cell_id)` reads `IsPreyFT()` of the organism
    /// `org`.
    fn add_pred_prey_av(&mut self, org: &OrgState, av_cell_id: i32) {
        let (uid, is_prey) = (org.uid, org.forage_target > -2);
        self.w.add_pred_prey_av(uid, is_prey, av_cell_id);
    }
    fn switch_pred_prey(&mut self, av_num: i32) {
        let me = self.org_uid();
        self.w.switch_pred_prey(me, av_num);
    }
    fn remove_all_av(&mut self) {
        let me = self.org_uid();
        self.w.remove_all_av(me);
    }
    fn get_av_facing(&self, av_num: i32) -> i32 {
        self.w.av_facing(self.org_uid(), av_num)
    }
    fn get_av_cell_id(&self, av_num: i32) -> i32 {
        self.w.av_cell_id(self.org_uid(), av_num)
    }
    fn get_av_faced_cell_id(&self, av_num: i32) -> i32 {
        self.w.av_faced_cell_id(self.org_uid(), av_num)
    }
    fn get_av_num_neighbors(&self, av_num: i32) -> i32 {
        self.w.av_num_neighbors(self.org_uid(), av_num)
    }
    fn get_av_faced_data(&self, av_num: i32) -> i32 {
        self.av_cell_data(av_num, true, |d| d.contents)
    }
    fn get_av_faced_data_org_id(&self, av_num: i32) -> i32 {
        self.av_cell_data(av_num, true, |d| d.org_id)
    }
    fn get_av_faced_data_update(&self, av_num: i32) -> i32 {
        self.av_cell_data(av_num, true, |d| d.update)
    }
    fn get_av_faced_data_territory(&self, av_num: i32) -> i32 {
        self.av_cell_data(av_num, true, |d| d.territory)
    }
    fn get_av_data(&self, av_num: i32) -> i32 {
        self.av_cell_data(av_num, false, |d| d.contents)
    }
    fn get_av_data_org_id(&self, av_num: i32) -> i32 {
        self.av_cell_data(av_num, false, |d| d.org_id)
    }
    fn get_av_data_update(&self, av_num: i32) -> i32 {
        self.av_cell_data(av_num, false, |d| d.update)
    }
    fn get_av_data_territory(&self, av_num: i32) -> i32 {
        self.av_cell_data(av_num, false, |d| d.territory)
    }
    fn find_av(&self, input: bool, output: bool, av_num: i32) -> i32 {
        self.w.find_av(self.org_uid(), input, output, av_num)
    }
    fn set_av_facing(&mut self, av_facing: i32, av_num: i32) {
        let me = self.org_uid();
        self.w.set_av_facing(me, av_facing, av_num);
    }
    fn set_av_cell_id(&mut self, av_cell_id: i32, av_num: i32) -> bool {
        let me = self.org_uid();
        self.w.set_av_cell_id(me, av_cell_id, av_num)
    }
    fn set_av_faced_cell_id(&mut self, av_num: i32) {
        let me = self.org_uid();
        self.w.set_av_faced_cell_id(me, av_num);
    }
    /// `SetAVCellData(new_data, org_id, av_num)`: `cPopulationCell::
    /// SetCellData` also records the opinion and forage target of the
    /// cell's occupant, which is `org` when the avatar is in the running
    /// organism's own cell.
    fn set_av_cell_data(&mut self, org: &mut OrgState, new_data: i32, org_id: i32, av_num: i32) {
        // If the avatar exists..
        let Some(a) = self.av(av_num) else {
            return;
        };
        let cell = a.cell_id;
        if usize::try_from(cell).map_or(true, |c| c >= self.w.pop.cells.len()) {
            self.unsupported("SetAVCellData outside the world (C++: out-of-bounds cell)");
            return;
        }
        let update = self.w.stats.update();
        if self.running_in(cell) {
            self.w
                .set_running_cell_data(cell, new_data, org_id, update, org);
        } else {
            self.w.set_cell_data(cell, new_data, org_id, update);
        }
    }
    fn move_av(&mut self, org: &mut OrgState, av_num: i32) -> bool {
        let me = self.org_uid();
        let true_cell = self.cell_id;
        let moved = self.w.move_av(me, true_cell, org, av_num);
        if let Some(r) = self.w.pop.running.as_ref() {
            self.cell_id = r.cell_id;
        }
        moved
    }
    fn rotate_av(&mut self, increment: i32, av_num: i32) -> bool {
        let me = self.org_uid();
        self.w.rotate_av(me, increment, av_num)
    }
    fn get_rand_faced_av(&mut self, av_num: i32) -> Option<OrgUid> {
        let faced = self.av(av_num)?.faced_cell;
        self.w.cell_rand_av(faced)
    }
    fn get_rand_faced_pred_av(&mut self, av_num: i32) -> Option<OrgUid> {
        let faced = self.av(av_num)?.faced_cell;
        self.w.avatars.first_pred(faced)
    }
    fn get_rand_faced_prey_av(&mut self, av_num: i32) -> Option<OrgUid> {
        let faced = self.av(av_num)?.faced_cell;
        self.w.avatars.first_prey(faced)
    }
    fn get_faced_avs(&mut self, av_num: i32) -> Vec<OrgUid> {
        self.av(av_num)
            .map_or_else(Vec::new, |a| self.w.avatars.cell_avs(a.faced_cell))
    }
    fn get_cell_avs(&mut self, av_cell_id: i32, av_num: i32) -> Vec<OrgUid> {
        if self.av(av_num).is_none() {
            return Vec::new();
        }
        self.w.avatars.cell_avs(av_cell_id)
    }
    fn get_faced_prey_avs(&mut self, av_num: i32) -> Vec<OrgUid> {
        self.av(av_num)
            .map_or_else(Vec::new, |a| self.w.avatars.cell_output_avs(a.faced_cell))
    }
    fn get_av_resources(&mut self, org: &mut OrgState, av_num: i32) -> Vec<f64> {
        match self.av_or_fatal(av_num, "GetAVResources") {
            Some(a) => self.get_cell_resources(org, a.cell_id),
            None => Vec::new(),
        }
    }
    fn get_av_resource_val(&mut self, org: &mut OrgState, res_id: i32, av_num: i32) -> f64 {
        match self.av_or_fatal(av_num, "GetAVResourceVal") {
            Some(a) => self.get_cell_res_val(org, a.cell_id, res_id),
            None => 0.0,
        }
    }
    fn get_av_faced_resources(&mut self, org: &mut OrgState, av_num: i32) -> Vec<f64> {
        match self.av_or_fatal(av_num, "GetAVFacedResources") {
            Some(a) => self.get_cell_resources(org, a.faced_cell),
            None => Vec::new(),
        }
    }
    fn get_av_faced_resource_val(&mut self, org: &mut OrgState, res_id: i32, av_num: i32) -> f64 {
        match self.av_or_fatal(av_num, "GetAVFacedResourceVal") {
            Some(a) => self.get_cell_res_val(org, a.faced_cell, res_id),
            None => 0.0,
        }
    }
    fn update_av_resources(&mut self, org: &mut OrgState, res_change: &[f64], av_num: i32) {
        if let Some(a) = self.av(av_num) {
            self.update_cell_resources(org, res_change, a.cell_id);
        }
    }

    fn get_av_neighborhood_cell_ids(&self, av_num: i32) -> Vec<i32> {
        match self.av_or_fatal(av_num, "GetAVNeighborhoodCellIDs") {
            Some(a) => self.w.pop.cells[a.cell_id.max(0) as usize]
                .connections
                .iter()
                .collect(),
            None => Vec::new(),
        }
    }

    // ---- what the look sensor reads of the population ----
    fn seen_org(&mut self, uid: OrgUid) -> Option<SeenOrg> {
        let cell_id = self.w.cell_of_org(uid)?;
        let o = self.w.pop.orgs[cell_id as usize].as_ref()?;
        let st = &o.state;
        let (av_cell_id, av_facing) = self.w.avatar_cell_and_facing(uid);
        Some(SeenOrg {
            id: st.id,
            uid,
            cell_id,
            av_cell_id,
            faced_dir: self.w.faced_dir_of(cell_id),
            av_facing,
            cur_bonus: st.phenotype.cur_bonus,
            opinion: st.opinions.as_ref().and_then(|l| l.last()).map(|o| o.0),
            forage_target: st.forage_target,
            show_ft: st.show_ft,
            is_dead: st.is_dead,
            is_displaying: st.display,
            display: st.org_display,
        })
    }
    fn cell_av_counts(&self, cell_id: i32) -> (i32, i32) {
        self.w.avatars.counts(cell_id)
    }
    fn cell_rand_av(&mut self, cell_id: i32) -> Option<OrgUid> {
        self.w.cell_rand_av(cell_id)
    }
    fn cell_first_pred_av(&self, cell_id: i32) -> Option<OrgUid> {
        self.w.avatars.first_pred(cell_id)
    }
    fn cell_first_prey_av(&self, cell_id: i32) -> Option<OrgUid> {
        self.w.avatars.first_prey(cell_id)
    }
    fn cell_prey_avs(&self, cell_id: i32) -> Vec<OrgUid> {
        self.w.avatars.cell_output_avs(cell_id)
    }
    fn cell_neighborhood_ids(&self, cell_id: i32) -> Vec<i32> {
        usize::try_from(cell_id)
            .ok()
            .and_then(|c| self.w.pop.cells.get(c))
            .map_or_else(Vec::new, |c| c.connections.iter().collect())
    }
    fn group_attack_inst_index(&self, inst_set: &str, inst: &str) -> Option<usize> {
        self.w
            .stats
            .group_attack_insts(inst_set)
            .iter()
            .position(|i| i == inst)
    }
    fn resource_used_bounds(&self, res_id: i32) -> (i32, i32, i32, i32) {
        let Some(r) = self.res_id(res_id) else {
            return (-1, -1, -1, -1);
        };
        let rc = &self.w.pop.resource_count;
        (
            rc.get_min_used_x(r),
            rc.get_min_used_y(r),
            rc.get_max_used_x(r),
            rc.get_max_used_y(r),
        )
    }
    /// `BeginSleep()`: the `LOG_SLEEP_TIMES` entry (see
    /// [`crate::population::SLEEP_LOG_STOP`]), then the deme's sleeping
    /// count.
    fn begin_sleep(&mut self) {
        if self.w.cfg.log_sleep_times && self.cell_id != 0 {
            self.w.set_fatal(crate::population::SLEEP_LOG_STOP);
        }
        self.deme_inc_sleeping_count();
    }
    /// `EndSleep()`
    fn end_sleep(&mut self) {
        if self.w.cfg.log_sleep_times && self.cell_id != 0 {
            self.w.set_fatal(crate::population::SLEEP_LOG_STOP);
        }
        self.deme_dec_sleeping_count();
    }

    // ------------------------------------------------------------------
    // Services the full `cHardwareCPU` instruction library needs.
    // ------------------------------------------------------------------
    fn cell_deme_id(&self, cell_id: i32) -> i32 {
        usize::try_from(cell_id)
            .ok()
            .and_then(|c| self.w.pop.cells.get(c))
            .map_or(-1, |c| c.deme_id)
    }
    fn population_cell_data(&self, cell_id: i32) -> i32 {
        usize::try_from(cell_id)
            .ok()
            .and_then(|c| self.w.pop.cells.get(c))
            .map_or(0, |c| c.cell_data.contents)
    }
    /// The occupant's id; the organism being stepped is out of its slot, so
    /// its id comes from `pop.running`.
    fn cell_organism_id(&self, cell_id: i32) -> i32 {
        let Some(c) = usize::try_from(cell_id)
            .ok()
            .filter(|&c| c < self.w.pop.orgs.len())
        else {
            return -1;
        };
        match self.w.pop.orgs[c].as_ref() {
            Some(o) => o.state.id,
            None => self
                .w
                .pop
                .running
                .as_ref()
                .filter(|r| r.cell_id == cell_id && !r.removed)
                .map_or(-1, |r| r.org_id),
        }
    }
    fn cell_organism_uid(&self, cell_id: i32) -> Option<OrgUid> {
        self.w.org_uid_in_cell(cell_id)
    }
    fn get_deme_cell_resources(&mut self, deme_id: i32, cell_id: i32) -> Vec<f64> {
        let w = &mut *self.w;
        let Some(deme) = usize::try_from(deme_id)
            .ok()
            .and_then(|d| w.pop.demes.get_mut(d))
        else {
            w.set_fatal(format!(
                "deme {deme_id} out of range (C++: out-of-bounds deme)"
            ));
            return Vec::new();
        };
        let rel = deme.relative_cell_id(cell_id);
        deme.resource_count
            .get_cell_resources(rel, &mut w.rng, &mut w.res_env)
            .to_vec()
    }
    fn deme_resource_amount(&mut self, deme_id: i32, res_index: i32) -> f64 {
        let w = &mut *self.w;
        let deme = usize::try_from(deme_id)
            .ok()
            .and_then(|d| w.pop.demes.get_mut(d));
        match (deme, usize::try_from(res_index)) {
            (Some(deme), Ok(r)) if r < deme.resource_count.size() => {
                deme.resource_count.get(&mut w.rng, &mut w.res_env, r)
            }
            _ => {
                w.set_fatal("deme resource out of range (C++: out-of-bounds index)");
                0.0
            }
        }
    }
    fn population_resource_amount(&mut self, org: &mut OrgState, res_index: i32) -> f64 {
        let w = &mut *self.w;
        match usize::try_from(res_index) {
            Ok(r) if r < w.pop.resource_count.size() => {
                w.with_pop_resources_for(Some(org), |rc, rng, env| rc.get(rng, env, r))
            }
            _ => {
                w.set_fatal("resource out of range (C++: out-of-bounds index)");
                0.0
            }
        }
    }
    /// `cDeme::AddPheromone(cell_id, value, ctx)`: QUIRK: `ModifyCell`
    /// adds, so the cell gains `value` of every resource named
    /// `pheromone` and nothing of the others.
    fn deme_add_pheromone(&mut self, deme_id: i32, cell_id: i32, value: f64) {
        let w = &mut *self.w;
        let Some(deme) = usize::try_from(deme_id)
            .ok()
            .and_then(|d| w.pop.demes.get_mut(d))
        else {
            w.set_fatal(format!(
                "deme {deme_id} out of range (C++: out-of-bounds deme)"
            ));
            return;
        };
        let rel = deme.relative_cell_id(cell_id);
        let rc = &mut deme.resource_count;
        let mut cell_resources = rc
            .get_cell_resources(rel, &mut w.rng, &mut w.res_env)
            .to_vec();
        for (i, r) in cell_resources.iter_mut().enumerate().take(rc.size()) {
            *r = if rc.get_res_name(i) == "pheromone" {
                value
            } else {
                0.0
            };
        }
        rc.modify_cell(&mut w.rng, &mut w.res_env, &cell_resources, rel);
    }
    fn deme_cell_position(&self, deme_id: i32, cell_id: i32) -> (i32, i32) {
        match usize::try_from(deme_id)
            .ok()
            .and_then(|d| self.w.pop.demes.get(d))
        {
            Some(deme) => deme.cell_position(cell_id),
            None => {
                self.unsupported("cDeme::GetCellPosition of a missing deme");
                (0, 0)
            }
        }
    }
    /// `cPopulation::UpdateCellResources(ctx, res_change, cell_id)`
    fn update_cell_resources(&mut self, org: &mut OrgState, res_change: &[f64], cell_id: i32) {
        self.w.with_pop_resources_for(Some(org), |rc, rng, env| {
            rc.modify_cell(rng, env, res_change, cell_id);
        });
    }
    /// `cStats::Move(org)`: the movement predicates of the deme cell
    /// events see the organism in its new cell.
    fn stats_move(&mut self, org: &OrgState) {
        if let Err(e) = self.w.deme_stats_move(self.cell_id, org.id) {
            self.w.set_fatal(e);
        }
    }
    /// `cPopulation::UpdateMerit(cell_id, new_merit)`; the running
    /// organism's own cell updates `org` in place.
    fn update_cell_merit(&mut self, org: &mut OrgState, cell_id: i32, new_merit: f64) -> bool {
        if self.running_in(cell_id) {
            let is_running = self
                .w
                .pop
                .running
                .as_ref()
                .is_some_and(|r| r.uid == org.uid);
            if !is_running {
                self.unsupported(
                    "cPopulation::UpdateMerit(cell) of the running organism from another \
                     organism's interface",
                );
                return false;
            }
            self.w.update_merit_of(cell_id, Some(org), new_merit);
        } else {
            self.w.update_merit_of(cell_id, None, new_merit);
        }
        true
    }
    fn deme_increase_energy_donated(&mut self, deme_id: i32, amount: f64) {
        if let Some(deme) = usize::try_from(deme_id)
            .ok()
            .and_then(|d| self.w.pop.demes.get_mut(d))
        {
            deme.total_energy_donated += amount;
        }
    }
    fn deme_increase_energy_received(&mut self, deme_id: i32, amount: f64) {
        if let Some(deme) = usize::try_from(deme_id)
            .ok()
            .and_then(|d| self.w.pop.demes.get_mut(d))
        {
            deme.total_energy_received += amount;
        }
    }
    fn turn_cell_to_face(&mut self, cell_id: i32, target_cell_id: i32) {
        self.w.turn_cell_to_face(cell_id, target_cell_id);
    }
    fn stats_inc_donate_to_donor(&mut self) {
        self.w.stats.m_donate_to_donor += 1;
    }
    fn stats_log_ret_message(&mut self, msg: &OrgMessage) {
        self.w.stats_log_ret_message(msg);
    }
    fn stats_sent_flash(&mut self, _org: &OrgState) {
        self.w.stats_sent_flash(self.cell_id);
    }
    fn deme_kill_cell_event(&mut self, org: &mut OrgState, deme_id: i32, event_id: i32) -> bool {
        let Some(d) = usize::try_from(deme_id)
            .ok()
            .filter(|&d| d < self.w.pop.demes.len())
        else {
            self.unsupported("cDeme::KillCellEvent of a missing deme");
            return false;
        };
        let running = self
            .w
            .pop
            .running
            .as_ref()
            .filter(|r| !r.removed)
            .map(|r| r.cell_id);
        match self.w.deme_kill_cell_event(d, event_id, running, org) {
            Ok(killed) => killed,
            Err(e) => {
                self.w.set_fatal(e);
                false
            }
        }
    }
    fn stats_creature_age_mean_stddev(&self) -> (f64, f64) {
        let s = &self.w.stats.sum_creature_age;
        (s.average(), s.std_deviation())
    }
    fn kill_group_member(&mut self, group_id: i32, org: &mut OrgState) {
        let key = self.cell_id;
        self.w.kill_group_member(org, key, group_id);
    }
    fn deme_add_resources_consumed(&mut self, deme_id: i32, amount: f64) {
        if let Some(deme) = usize::try_from(deme_id)
            .ok()
            .and_then(|d| self.w.pop.demes.get_mut(d))
        {
            deme.total_res_consumed += amount;
        }
    }
    /// `cDeme::ReplicateDeme()` only raises the deme's flag.
    fn deme_replicate(&mut self, deme_id: i32) {
        if let Some(deme) = usize::try_from(deme_id)
            .ok()
            .and_then(|d| self.w.pop.demes.get_mut(d))
        {
            deme.replicate_deme = true;
        }
    }
    /// `GetDeme()->IncNumSwitchingPenalties(n)`
    fn deme_inc_num_switching_penalties(&mut self, n: i32) {
        let d = self.w.pop.cells[self.cell()].deme_id as usize;
        self.w.pop.demes[d].switch_penalties += n;
    }
    /// `GetDeme()->DecSleepingCount()`
    fn deme_dec_sleeping_count(&mut self) {
        let d = self.w.pop.cells[self.cell()].deme_id as usize;
        self.w.pop.demes[d].sleeping_count -= 1;
    }
    /// `GetDeme()->IncSleepingCount()`
    fn deme_inc_sleeping_count(&mut self) {
        let d = self.w.pop.cells[self.cell()].deme_id as usize;
        self.w.pop.demes[d].sleeping_count += 1;
    }
    fn deme_record_suicide(&mut self, deme_id: i32, points: i32) {
        match usize::try_from(deme_id)
            .ok()
            .and_then(|d| self.w.pop.demes.get_mut(d))
        {
            Some(deme) => {
                deme.points += f64::from(points);
                deme.suicides += 1;
            }
            None => self.unsupported("cDeme::AddSuicide of a missing deme"),
        }
    }
    fn deme_increase_total_energy_testament(&mut self, deme_id: i32, amount: f64) {
        if let Some(deme) = usize::try_from(deme_id)
            .ok()
            .and_then(|d| self.w.pop.demes.get_mut(d))
        {
            deme.total_energy_testament += amount;
        }
    }
    fn stats_add_energy_testament(&mut self, target: TestamentTarget, amount: f64) {
        let e = &mut self.w.stats.energy;
        match target {
            TestamentTarget::FutureDeme => e.testament_to_future_deme.add(amount),
            TestamentTarget::NeighborOrganisms => e.testament_to_neighbor_organisms.add(amount),
            TestamentTarget::DemeOrganisms => e.testament_to_deme_organisms.add(amount),
        }
    }
    fn stats_inc_dont_explode(&mut self) {
        self.w.stats.inc_dont_explode();
    }
    fn stats_lyse(&mut self, percent_prob: f64, cpu_cycles: i32) {
        let stats = &mut self.w.stats;
        stats.inc_kaboom();
        stats.inc_perc_lyse(percent_prob);
        stats.inc_sum_cpus(cpu_cycles);
    }
    fn stats_inc_quorum(&mut self, org_ratio: i32) {
        let stats = &mut self.w.stats;
        stats.inc_quorum_threshold_ub(org_ratio);
        stats.inc_quorum_num();
    }
    fn stats_inc_guard_fail(&mut self) {
        self.w.stats.num_guard_fail += 1;
    }
    /// `cStats::GetNumPreyCreatures()` reads the population's count.
    fn stats_num_prey_creatures(&self) -> Result<i32, String> {
        Ok(self.w.pop.num_prey_organisms)
    }
    /// `cStats::GetNumTotalPredCreatures()`: top predators plus predators.
    fn stats_num_total_pred_creatures(&self) -> Result<i32, String> {
        Ok(self.w.pop.num_top_pred_organisms + self.w.pop.num_pred_organisms)
    }
    /// `cStats::GetNumPredCreatures()`: predators, not top predators.
    fn stats_num_pred_creatures(&self) -> Result<i32, String> {
        Ok(self.w.pop.num_pred_organisms)
    }
}
