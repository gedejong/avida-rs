//! Port of `cWorld` (avida-core/source/main/cWorld.{h,cc} at 47f13dadb):
//! the owner of the configuration, environment, RNG, hardware manager,
//! statistics, systematics, population and event list of one run.
//!
//! The C++ world hands out pointers to its parts; here the parts are plain
//! fields so that code can borrow several of them at once. Organism-level
//! code reaches the world through [`crate::pop_interface::PopulationInterface`].
//!
//! # Unsupported settings
//! [`check_supported`] rejects, at start-up, every setting whose code path
//! is not ported; nothing is silently ignored.

use std::cell::RefCell;
use std::collections::BTreeMap;

use avida_apto::datafile::get_absolute_path;
use avida_apto::OutputManager;
use avida_apto::{Accumulator, AvidaRng};
use avida_config::config::Config;
use avida_config::feedback::Feedback as ConfigFeedback;
use avida_config::init_file::InitFile;
use avida_genome::inst_set_registry::convert_legacy_inst_set_file;
use avida_genome::{Feedback as GenomeFeedback, InstCodeConfig};
use avida_hardware::org_interface::WorldServices;
use avida_hardware::task_adapter::{Env, TaskLibRegistry};
use avida_hardware::test_cpu::{TestWorld, TestWorldCounts};
use avida_hardware::HardwareManager;
use avida_resources::ResourceEnv;
use avida_systematics::{Arbiter, GenotypeArbiter, Manager};

use crate::actions::{self, Action, ParseContext};
use crate::event_list::{parse_event_line, trigger_value, EventList};
use crate::host::{HostOp, WorldHost};
use crate::population::Population;
use crate::run_actions::ActionState;
use crate::stats::Stats;

/// The world as `cResourceCount` sees it ([`ResourceEnv`]) where the
/// population is not at hand: the deme resource counts and the set-up.
/// The population's own resource count goes through
/// [`crate::habitat::HabitatEnv`] (`World::with_pop_resources`), which
/// carries out the gradient habitats; here they stop the run.
#[derive(Debug, Default)]
pub struct WorldResEnv {
    /// `cStats::GetUpdate()`, kept in step by the world.
    pub update: i32,
    /// `cAvidaContext::HasDriver()`
    pub has_driver: bool,
    /// `Feedback().Error(...)` messages (the driver prints them).
    pub errors: Vec<String>,
    /// First request for an unported population service.
    pub unsupported: Option<String>,
    /// The gradient habitats' population effects (`crate::habitat`).
    pub habitat: crate::habitat::HabitatState,
}

impl WorldResEnv {
    fn unsupported(&mut self, what: &str) {
        if self.unsupported.is_none() {
            self.unsupported = Some(format!("unsupported: {what}"));
        }
    }

    /// Records a condition the run must stop on (the C++ crashes or the
    /// path is not ported).
    pub(crate) fn stop(&mut self, what: &str) {
        self.unsupported(what);
    }
}

impl ResourceEnv for WorldResEnv {
    fn update(&self) -> i32 {
        self.update
    }
    fn has_driver(&self) -> bool {
        self.has_driver
    }
    fn feedback_error(&mut self, msg: &str) {
        self.errors.push(msg.to_string());
    }
    fn execute_predatory_resource(
        &mut self,
        _rng: &mut AvidaRng,
        _cell_id: i32,
        _pred_odds: f64,
        _juvs_per: i32,
        _hammer: bool,
    ) {
        self.unsupported(
            "a predatory gradient updated without the population at hand (a deme resource \
             count or an unconverted call site; C++: cPopulation::ExecutePredatoryResource)",
        );
    }
    fn execute_damaging_resource(
        &mut self,
        _rng: &mut AvidaRng,
        _cell_id: i32,
        _damage: f64,
        _hammer: bool,
    ) {
        self.unsupported(
            "a damaging gradient updated without the population at hand (a deme resource \
             count or an unconverted call site; C++: cPopulation::ExecuteDamagingResource)",
        );
    }
    fn execute_deadly_resource(
        &mut self,
        _rng: &mut AvidaRng,
        _cell_id: i32,
        _odds: f64,
        _hammer: bool,
    ) {
        self.unsupported(
            "a deadly gradient updated without the population at hand (a deme resource \
             count or an unconverted call site; C++: cPopulation::ExecuteDeadlyResource)",
        );
    }
}

/// What `cWorld::Initialize` reported besides building the world.
#[derive(Debug, Default)]
pub struct WorldSetupReport {
    /// Text the C++ writes to stdout while setting up.
    pub stdout: String,
    /// Text the front end writes to stderr (`error: ...`, `warning: ...`).
    pub stderr: String,
}

/// C++: `cWorld`
pub struct World {
    pub cfg: Config,
    pub working_dir: String,
    pub env: Env,
    pub rng: AvidaRng,
    pub hw: HardwareManager,
    pub stats: Stats,
    pub systematics: Manager,
    pub pop: Population,
    pub host: WorldHost,
    pub res_env: WorldResEnv,
    pub out: OutputManager,
    pub events: EventList<Action>,
    /// `m_test_on_div`
    pub test_on_div: bool,
    /// `m_test_sterilize`
    pub test_sterilize: bool,
    /// `Avida2Driver::m_done`
    pub done: bool,
    /// The first unported path a run reached; the run loop stops on it.
    pub fatal: RefCell<Option<String>>,
    /// `cStats::GetGroupAttackInsts(inst_set).GetSize()` per instruction set.
    pub group_attack_counts: BTreeMap<String, usize>,
    /// The closure form of `group_attack_counts` the test CPU borrows.
    pub(crate) group_attack_fn: Box<dyn Fn(&str) -> usize>,
    /// State event actions keep between calls (see [`ActionState`]).
    pub action_state: ActionState,
    /// The organisms' avatars and the cells' avatar lists (`USE_AVATARS`).
    pub avatars: crate::avatars::Avatars,
    /// Organism messaging state (messaging.rs).
    pub messaging: crate::messaging::MessagingState,
    /// Deme competition state (deme_compete.rs).
    pub deme_compete: crate::deme_compete::CompeteState,
    /// State of the second-round action modules (actions2.rs).
    pub actions2: crate::actions2::Actions2State,
    /// The mini and micro trace queues (mini_trace.rs).
    pub mini_trace: crate::mini_trace::MiniTraceState,
}

/// Settings whose code paths are not ported. `Err` names the first one.
///
/// None is left: `ENABLE_MP` needs nothing (its only use,
/// `cPopulation::ActivateOffspring`, asks `cWorld::TestForMigration()`,
/// which is always false for the single-process `cWorld`, so every
/// offspring stays), and `NEURAL_NETWORKING` with avatars is in
/// [`crate::messaging`] and `ActivateOrganism`. Paths that are undefined in
/// the C++ stop the run where they are reached.
///
/// The port's own `EXECUTION_MODE 1` rejects the settings it cannot run
/// with ([`crate::parallel::check_parallel_supported`]).
pub fn check_supported(cfg: &Config, _env: &Env) -> Result<(), String> {
    crate::parallel::check_parallel_supported(cfg)
}

impl World {
    /// `cWorld::Initialize(cfg, working_dir, new_world, feedback, defs)`.
    ///
    /// `Err` carries the text for stderr when the C++ returns NULL (the
    /// front end then exits with -1) or when a setting is not supported.
    pub fn new(
        mut cfg: Config,
        working_dir: &str,
        defs: &[(String, String)],
    ) -> Result<(World, WorldSetupReport), String> {
        let mut report = WorldSetupReport::default();
        let mut success = true;

        // Setup Random Number Generator
        let mut rng = AvidaRng::new(cfg.random_seed);

        // Output Manager
        let opath = get_absolute_path(&cfg.data_dir, working_dir);
        let out = OutputManager::with_working_dir(&opath, working_dir);

        // Initialize the default environment...
        let mut fb = ConfigFeedback::new();
        let mut env = Env::new(&cfg, TaskLibRegistry::new());
        let env_file = cfg.environment_file.clone();
        if env
            .load(&env_file, working_dir, &mut fb, Some(defs), &cfg)
            .is_err()
        {
            success = false;
        }

        // `cMigrationMatrix::Load` when DEMES_MIGRATION_METHOD is 4.
        let mut migration_matrix = crate::migration_matrix::MigrationMatrix::new();
        if cfg.demes_migration_method == 4 {
            let count_parasites = cfg.demes_parasite_migration_rate > 0.0;
            let count_offspring = cfg.demes_migration_rate > 0.0;
            if !migration_matrix.load(
                cfg.num_demes,
                &cfg.migration_file,
                working_dir,
                count_parasites,
                count_offspring,
                false,
                &mut fb,
            ) {
                success = false;
            }
        }

        // Systematics
        let trigger_names: Vec<String> =
            env.action_triggers().iter().map(|t| t.id.clone()).collect();
        let mut systematics = Manager::new();
        systematics.register_arbiter(Arbiter::genotype(GenotypeArbiter::new(
            "genotype",
            cfg.threshold,
            cfg.disable_genotype_classification,
            &trigger_names,
        )));

        // Setup Stats Object
        let task_names: Vec<String> = env
            .tasklib()
            .0
            .tasks()
            .iter()
            .map(|t| t.desc().to_string())
            .collect();
        let reaction_names: Vec<String> = (0..env.num_reactions().max(0) as usize)
            .map(|i| env.reaction_name(i).to_string())
            .collect();
        let num_resources = env.resource_lib().size().max(0) as usize;
        let mut stats = Stats::new(
            task_names,
            reaction_names,
            num_resources,
            f64::from(cfg.res_for_deme_rep),
        );

        // Initialize the hardware manager, loading all of the instruction sets
        let mut gfb = GenomeFeedback::new();
        let mut list: Vec<String> = cfg.instsets().to_vec();
        if cfg.inst_set_load_legacy != 0
            && !convert_legacy_inst_set_file(
                &cfg.inst_set,
                cfg.hardware_type,
                working_dir,
                &mut list,
                &mut gfb,
            )
            .success
        {
            success = false;
        }
        let mut hw = HardwareManager::new();
        let code_cfg = InstCodeConfig {
            inst_code_length: cfg.inst_code_length,
            inst_code_default_type: cfg.inst_code_default_type,
        };
        match hw.load_inst_sets(&list, code_cfg, &mut gfb) {
            Ok(true) => {}
            Ok(false) => success = false,
            // An unported instruction or hardware type: unless the C++ stops
            // earlier at its event file.
            Err(e) if success && e.starts_with("unsupported") => {
                let default_inst_set = hw
                    .registry()
                    .shared_inst_set(0)
                    .map(|is| is.inst_set_name().to_string())
                    .unwrap_or_default();
                let feedback = format!("{}{}", fb.render(), gfb.render());
                return Err(actions::prefer_event_file_failure(
                    e,
                    &cfg.event_file,
                    defs,
                    working_dir,
                    &default_inst_set,
                    &feedback,
                ));
            }
            Err(e) => return Err(e),
        }
        report.stderr.push_str(&fb.render());
        report.stderr.push_str(&gfb.render());
        if hw.registry().num_inst_sets() == 0 {
            report
                .stderr
                .push_str("error: no instruction sets defined\n");
            if cfg.inst_set_load_legacy == 0 && !cfg.inst_set.is_empty() && cfg.inst_set != "-" {
                report.stderr.push_str(
                    "It looks like you are attempting to load a legacy format instruction set file.  Try setting INST_SET_LOAD_LEGACY to 1.\n",
                );
            }
            success = false;
        }

        // If there were errors loading at this point, it is perilous to try
        // to go further (pop depends on an instruction set)
        if !success {
            return Err(report.stderr);
        }
        // A run the C++ cannot start for its event file fails that way here
        // too, before any unported setting matters.
        let default_inst_set = hw
            .registry()
            .shared_inst_set(0)
            .map(|is| is.inst_set_name().to_string())
            .unwrap_or_default();
        if let Err(e) = hw.check_ported().and_then(|()| check_supported(&cfg, &env)) {
            return Err(actions::prefer_event_file_failure(
                e,
                &cfg.event_file,
                defs,
                working_dir,
                &default_inst_set,
                &report.stderr,
            ));
        }
        let mut group_attack_counts = BTreeMap::new();
        for i in 0..hw.registry().num_inst_sets() {
            let Some(is) = hw.registry().shared_inst_set(i) else {
                continue;
            };
            let name = is.inst_set_name();
            let names: Vec<String> = (0..is.size()).map(|j| is.name(j).to_string()).collect();
            stats.set_inst_names(name, names);
            // `cHardwareManager::loadInstSet`: the attack instructions of the
            // set, whose count sizes each organism's group attack arrays
            // (`cOrganism` constructor, `SetGroupAttackInstSetSize`).
            stats.set_group_attack_inst_names(name);
            // The forager-class execution counts are sized here too, so the
            // prey / predator printers list every instruction.
            let size = is.size() as usize;
            type Counts = for<'a> fn(&'a mut Stats, &str) -> &'a mut Vec<Accumulator<i32>>;
            let ft_counts: [Counts; 9] = [
                Stats::inst_prey_exe_counts_for_inst_set,
                Stats::inst_pred_exe_counts_for_inst_set,
                Stats::inst_top_pred_exe_counts_for_inst_set,
                Stats::inst_prey_from_sensor_exe_counts_for_inst_set,
                Stats::inst_pred_from_sensor_exe_counts_for_inst_set,
                Stats::inst_top_pred_from_sensor_exe_counts_for_inst_set,
                Stats::inst_from_message_exe_counts_for_inst_set,
                Stats::inst_male_exe_counts_for_inst_set,
                Stats::inst_female_exe_counts_for_inst_set,
            ];
            for counts in ft_counts {
                counts(&mut stats, name).resize_with(size, Default::default);
            }
            stats.exec_counts_for_group_attack_inst_set(name).clear();
            let attack_insts = stats.group_attack_insts(name).to_vec();
            for inst in &attack_insts {
                *stats.exec_counts_for_group_attack_inst(name, inst) =
                    vec![Accumulator::default(); 20];
            }
            group_attack_counts.insert(name.to_string(), attack_insts.len());
        }
        let attack_counts = group_attack_counts.clone();
        let group_attack_fn: Box<dyn Fn(&str) -> usize> =
            Box::new(move |inst_set| attack_counts.get(inst_set).copied().unwrap_or(0));

        let test_on_div = cfg.revert_fatal > 0.0
            || cfg.revert_detrimental > 0.0
            || cfg.revert_neutral > 0.0
            || cfg.revert_beneficial > 0.0
            || cfg.revert_taskloss > 0.0
            || cfg.revert_equals > 0.0
            || cfg.sterilize_unstable > 0;
        let test_sterilize = cfg.sterilize_fatal > 0.0
            || cfg.sterilize_detrimental > 0.0
            || cfg.sterilize_neutral > 0.0
            || cfg.sterilize_beneficial > 0.0
            || cfg.sterilize_taskloss > 0.0;

        let mut res_env = WorldResEnv {
            update: stats.update(),
            ..WorldResEnv::default()
        };
        // Not deferred to the event file: the C++ population set-up has
        // failure paths of its own that come before it.
        let (mut pop, pop_report) = Population::new(
            &mut cfg,
            env.mut_rates(),
            env.resource_lib(),
            &mut stats,
            &mut rng,
            &mut res_env,
            env.num_tasks().max(0) as usize,
            env.num_reactions().max(0) as usize,
        )?;
        pop.migration_matrix = migration_matrix;
        for line in &pop_report.stdout {
            report.stdout.push_str(line);
            report.stdout.push('\n');
        }
        for w in &pop_report.warnings {
            report.stderr.push_str(&format!("warning: {w}\n"));
        }

        let mut host = WorldHost::new(pop.cells.len(), cfg.world_x, cfg.world_y);
        host.match_already_produced = cfg.match_already_produced != 0;
        host.pheromone_id = env.resource_lib().find("pheromone");
        host.state_grids = (0..env.num_state_grids().max(0) as usize)
            .map(|sg| env.state_grid(sg).clone())
            .collect();

        // Setup Event List
        let mut events = EventList::new();
        let event_file = InitFile::with_mappings(&cfg.event_file, defs, working_dir);
        let mut events_ok = event_file.was_opened();
        // `cEventList` reports a bad line with `cerr`, before the driver
        // prints the accumulated feedback.
        let mut event_cerr = String::new();
        if events_ok {
            let ctx = ParseContext {
                default_inst_set: &default_inst_set,
            };
            let mut afb = ConfigFeedback::new();
            for line_id in 0..event_file.num_lines() {
                let line = event_file.get_line(line_id as i32);
                let spec = match parse_event_line(&line) {
                    Ok(spec) => spec,
                    Err(msg) => {
                        event_cerr.push_str(&msg);
                        event_cerr.push('\n');
                        events_ok = false;
                        break;
                    }
                };
                if !actions::is_cpp_action(&spec.name) {
                    // `cEventList::AddEvent`: the library cannot create it.
                    event_cerr.push_str(&format!("error: unable to load event '{}'\n", spec.name));
                    events_ok = false;
                    break;
                }
                match actions::parse(&spec.name, &spec.args, &ctx, &mut afb) {
                    Ok(action) => {
                        let update = stats.update();
                        let ave_gen = stats.sum_generation.average();
                        let tot = stats.tot_organisms;
                        events.add_event(spec, action, |t| trigger_value(t, update, ave_gen, tot));
                    }
                    // `cerr << ...; exit(1);` in the constructor: nothing
                    // else is printed.
                    Err(msg) if msg.starts_with(actions::CERR_EXIT_MARK) => return Err(msg),
                    Err(msg) => {
                        report.stderr.push_str(&format!("error: {msg}\n"));
                        events_ok = false;
                        break;
                    }
                }
            }
            report.stderr.push_str(&afb.render());
        }
        if !events_ok {
            report.stderr.push_str("error: unable to load event file\n");
            return Err(event_cerr + &report.stderr);
        }

        let world = World {
            cfg,
            working_dir: working_dir.to_string(),
            env,
            rng,
            hw,
            stats,
            systematics,
            pop,
            host,
            res_env,
            out,
            events,
            test_on_div,
            test_sterilize,
            done: false,
            fatal: RefCell::new(None),
            group_attack_counts,
            group_attack_fn,
            action_state: ActionState::default(),
            avatars: crate::avatars::Avatars::default(),
            messaging: crate::messaging::MessagingState::default(),
            deme_compete: crate::deme_compete::CompeteState::default(),
            actions2: crate::actions2::Actions2State::default(),
            mini_trace: crate::mini_trace::MiniTraceState::default(),
        };
        Ok((world, report))
    }

    /// Records the first unported path the run reached.
    pub fn set_fatal(&self, what: impl Into<String>) {
        let mut f = self.fatal.borrow_mut();
        if f.is_none() {
            *f = Some(what.into());
        }
    }

    /// The first fatal condition recorded anywhere in the world.
    pub fn take_fatal(&mut self) -> Option<String> {
        // The run loop asks after every time slice, and almost always
        // nothing is pending: reading the slots is cheaper than taking them.
        if self.fatal.get_mut().is_none()
            && self.host.fatal.is_none()
            && self.host.fatal_cell.get_mut().is_none()
            && self.pop.groups.fatal.is_none()
            && self.res_env.unsupported.is_none()
        {
            return None;
        }
        if let Some(f) = self.fatal.borrow_mut().take() {
            return Some(f);
        }
        self.host.collect_fatal();
        if let Some(f) = self.host.fatal.take() {
            return Some(f);
        }
        if let Some(f) = self.pop.groups.fatal.take() {
            return Some(f);
        }
        self.res_env.unsupported.take()
    }

    /// The pending fatal condition, if any, without clearing it (for front
    /// ends; the run loop uses [`World::take_fatal`]).
    pub fn fatal_message(&self) -> Option<String> {
        self.fatal
            .borrow()
            .clone()
            .or_else(|| self.host.fatal.clone())
            .or_else(|| self.host.fatal_cell.borrow().clone())
            .or_else(|| self.pop.groups.fatal.clone())
            .or_else(|| self.res_env.unsupported.clone())
    }

    /// Applies the `cStats` calls (and the changes to resting organisms)
    /// tasks queued while the stats were lent out.
    pub fn flush_host(&mut self) {
        if self.host.pending.is_empty() {
            return;
        }
        for op in std::mem::take(&mut self.host.pending) {
            match op {
                HostOp::AddTag(tag, value) => self.stats.add_tag(tag, value),
                HostOp::StringBitsMatched(name, value) => {
                    self.stats.add_string_bits_matched_value(&name, value);
                }
                HostOp::PerfectMatch(amount) => self.stats.inc_perfect_match(amount),
                HostOp::PerfectMatchOrg => self.stats.inc_perfect_match_org(),
                HostOp::IncResamplings => self.stats.num_resamplings += 1,
                HostOp::IncFailedResamplings => self.stats.num_failed_resamplings += 1,
                HostOp::HgtMetabolized(size) => self.stats.genome_fragment_metabolized(size),
                HostOp::EnergyTestament(target, amount) => {
                    self.stats.energy.add_testament(target, amount);
                }
                HostOp::Stdout(text) => self.action_state.stdout.push_str(&text),
                HostOp::AddNewTaskCount(task) => {
                    if let Some(slot) = usize::try_from(task)
                        .ok()
                        .and_then(|t| self.stats.new_task_count.get_mut(t))
                    {
                        *slot += 1;
                    }
                }
                HostOp::KillCell(cell, uid) => self.kill_for_test_cpu(cell, uid),
                HostOp::ClearKaboom(cell) => {
                    if let Some(o) = usize::try_from(cell)
                        .ok()
                        .and_then(|c| self.pop.orgs.get_mut(c))
                        .and_then(Option::as_mut)
                    {
                        o.state.phenotype.kaboom_executed = false;
                    }
                }
            }
        }
    }

    /// The kill a test CPU asked for (`HostOp::KillCell`), carried out when
    /// the test has ended. The C++ makes it in the middle of the test; that
    /// order only matters if the kill draws random numbers, which this
    /// checks.
    fn kill_for_test_cpu(&mut self, cell: i32, uid: avida_hardware::organism::OrgUid) {
        let still_there = usize::try_from(cell)
            .ok()
            .and_then(|c| self.pop.orgs.get(c))
            .and_then(Option::as_ref)
            .is_some_and(|o| o.state.uid == uid);
        if !still_there {
            self.set_fatal(
                "internal: the organism a test CPU's MAX_PREY_BT kill chose is not in its cell",
            );
            return;
        }
        let before = self.rng.clone();
        self.kill_organism(cell, None);
        if self.rng != before {
            self.set_fatal(
                "unsupported: a MAX_PREY_BT kill from a test CPU that draws random numbers (C++: \
                 the kill happens in the middle of the test, before its later draws)",
            );
        }
    }

    /// `CalculateUpdateSize()`
    pub fn calculate_update_size(&self) -> i32 {
        self.cfg.ave_time_slice * self.pop.num_organisms
    }
}

/// What a test CPU borrows from the world, from disjoint fields.
#[allow(clippy::too_many_arguments)]
pub(crate) fn make_test_world<'a>(
    cfg: &'a Config,
    env: &'a mut Env,
    rng: &'a mut AvidaRng,
    hw: &'a HardwareManager,
    stats: &'a mut Stats,
    host: &'a mut WorldHost,
    group_attack_fn: &'a dyn Fn(&str) -> usize,
    test_sterilize: bool,
    pop: &Population,
) -> TestWorld<'a> {
    let update = stats.update();
    let tot_creatures = stats.tot_organisms;
    let sense_size = stats.sense_size.max(0) as usize;
    let counts = TestWorldCounts {
        num_prey: pop.num_prey_organisms,
        num_pred: pop.num_pred_organisms,
        num_top_pred: pop.num_top_pred_organisms,
        creature_age_mean: stats.sum_creature_age.average(),
        creature_age_stddev: stats.sum_creature_age.std_deviation(),
    };
    // A test organism's cell is -1 (no HGT metabolism, see crate::hgt).
    host.cur_cell = -1;
    host.snapshot_kaboom_cells(env, pop, -1);
    // Only `Inst_SetRandPFTOnce` under MAX_PREY_BT reads the population.
    // The running organism is out of its cell; its forage target is known
    // only while a call that has its state at hand runs the test.
    let live_orgs = (cfg.max_prey_bt != 0).then(|| {
        let running = pop.running.as_ref();
        let running_ft = |uid| {
            host.running_forage_target
                .filter(|&(u, _)| u == uid)
                .map(|(_, ft)| ft)
        };
        pop.live_org_list
            .iter()
            .filter_map(|&c| match pop.orgs[c as usize].as_ref() {
                Some(o) => Some((o.state.uid, c, Some(o.state.forage_target))),
                None => running
                    .filter(|r| r.cell_id == c)
                    .map(|r| (r.uid, c, running_ft(r.uid))),
            })
            .collect()
    });
    TestWorld {
        config: cfg,
        environment: env,
        rng,
        hardware_manager: hw,
        stats,
        host,
        update,
        tot_creatures,
        sense_size,
        group_attack_inst_count: group_attack_fn,
        counts,
        live_orgs,
        analyze_mode: false,
        org_fault_reporting: false,
        verbosity: cfg.verbosity,
        test_sterilize,
    }
}

impl WorldServices for World {
    fn config(&self) -> &Config {
        &self.cfg
    }
    fn environment(&self) -> &Env {
        &self.env
    }
    fn rng(&mut self) -> &mut AvidaRng {
        &mut self.rng
    }
    fn hardware_manager(&self) -> &HardwareManager {
        &self.hw
    }
    fn update(&self) -> i32 {
        self.stats.update()
    }
    fn tot_creatures(&self) -> i32 {
        self.stats.tot_organisms
    }
    fn sense_size(&self) -> usize {
        self.stats.sense_size.max(0) as usize
    }
    fn group_attack_inst_count(&self, inst_set: &str) -> usize {
        self.group_attack_counts.get(inst_set).copied().unwrap_or(0)
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
        self.cfg.verbosity
    }
    fn new_org_uid(&mut self) -> avida_hardware::organism::OrgUid {
        self.pop.last_org_uid += 1;
        avida_hardware::organism::OrgUid(self.pop.last_org_uid)
    }
}
