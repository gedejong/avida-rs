//! [`AnalyzeWorld`]: the `cWorld` of an analyze-mode run, built the way
//! `cWorld::setup` (avida-core/source/main/cWorld.cc at 47f13dadb) builds
//! it, as far as analyze mode can observe: configuration, output manager,
//! environment, instruction sets, the world RNG with the draws population
//! set-up makes, and the event file check. The population stays empty;
//! analyze mode only reaches it through the test CPU, which sees a world
//! with no organisms.
//!
//! # The population
//! `cWorld::setup` builds the whole population in analyze mode too. What
//! that leaves behind for analyze mode to observe is the world RNG after
//! the draws of topology, time slicer and gradient resource set-up, and the
//! resource set-up lines printed at `VERBOSITY > 1`. The full
//! `cPopulation` constructor and `cMigrationMatrix::Load` belong to the
//! layer above this crate; [`AnalyzeWorld::initialize_with`] runs them
//! through [`SetupHooks`], so every setting a population run accepts sets
//! up here the same way. Without hooks ([`AnalyzeWorld::initialize`]) this
//! module sets up the settings whose population set-up it reproduces
//! itself and stops with [`AnalyzeError::Unsupported`] on the others:
//! `WORLD_GEOMETRY` 7/8 (random and scale-free topologies), `NUM_DEMES > 1`,
//! deme resources, gradient resources, `DEMES_MIGRATION_METHOD 4`,
//! `ENABLE_HGT`, HGT resources.
//!
//! Event lines are checked for syntax and for a registered action name;
//! the action constructors themselves are not run.

use std::collections::BTreeMap;

use avida_apto::datafile::get_absolute_path;
use avida_apto::{diag, AvidaRng, OutputManager};
use avida_config::string_util as su;
use avida_config::{Config, Feedback, InitFile};
use avida_environment::{geometry, TaskRegistry};
use avida_genome::inst_set_registry::convert_legacy_inst_set_file;
use avida_genome::{InstCodeConfig, InstSetRegistry};
use avida_hardware::task_adapter::{Env, OutputHost, TaskLibRegistry};
use avida_hardware::test_cpu::{TestCpuHost, TestWorld, TestWorldCounts};
use avida_hardware::HardwareManager;
use avida_phenotype::PhenotypeStats;
use avida_tasks::TaskWorld;

use crate::actions::is_registered_action;
use crate::error::{AnalyzeError, Result};
use crate::host::{AnalyzeHost, TestContext, WorkerWorld};

/// `VERBOSE_NORMAL`.
const VERBOSE_NORMAL: i32 = 1;

/// The parts of `cWorld::setup` and of the `avida` front end that live in
/// the layers above this crate (the population, the migration matrix and
/// the action library), lent to [`AnalyzeWorld::initialize_with`] and
/// [`crate::run_analyze_with`]. Each method answers `None` when the hooks
/// cannot do it; the analyze world then does what it can itself or stops
/// with [`AnalyzeError::Unsupported`].
pub trait SetupHooks {
    /// `cActionLibrary::GetInstance().DescribeAll()`, which `-e` prints.
    fn describe_all_actions(&self) -> Option<String> {
        None
    }

    /// `m_mig_mat->Load(NUM_DEMES, MIGRATION_FILE, working_dir,
    /// count_parasites, count_offspring, false, feedback)`
    /// (`DEMES_MIGRATION_METHOD 4`): whether it loaded, with its messages
    /// in `feedback`.
    fn load_migration_matrix(
        &mut self,
        _config: &Config,
        _working_dir: &str,
        _feedback: &mut Feedback,
    ) -> Option<bool> {
        None
    }

    /// `m_pop = new cPopulation(this)`: build the population of `config`
    /// in environment `env`, drawing from the world RNG `rng` as the C++
    /// does. `Ok` holds the lines it printed (resource set-up at
    /// `VERBOSITY > 1`); `Err` a loud failure (an unported setting, or a
    /// set-up the C++ does not survive).
    fn setup_population(
        &mut self,
        _config: &mut Config,
        _env: &Env,
        _rng: &mut AvidaRng,
    ) -> Option<std::result::Result<Vec<String>, String>> {
        None
    }
}

/// No hooks: the analyze world sets up what it can on its own.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSetupHooks;

impl SetupHooks for NoSetupHooks {}

/// The instruction names `cStats::SetGroupAttackInstNames` collects.
const GROUP_ATTACK_INSTS: &[&str] = &[
    "attack-prey",
    "attack-ft-prey",
    "attack-prey-group",
    "attack-prey-share",
    "attack-prey-group-share",
    "attack-prey-no-share",
    "attack-prey-fake-share",
    "attack-prey-fake-group-share",
];

/// The world-level services a test organism reaches beyond its test CPU,
/// for a world whose population is empty: no groups, no cells, `cStats`
/// updates that analyze mode never reports. Services the analyze world
/// cannot answer are recorded and turned into an error after the test.
#[derive(Debug, Default)]
struct AnalyzeServices {
    unsupported: Option<String>,
    /// The C library's `rand()` state (process-wide in the C++).
    libc_rand: avida_tasks::LibcRand,
}

impl AnalyzeServices {
    fn unsupported(&mut self, what: &str) {
        if self.unsupported.is_none() {
            self.unsupported = Some(format!("unsupported: {what}"));
        }
    }
}

impl TaskWorld for AnalyzeServices {
    fn get_cell_id(&self) -> i32 {
        -1
    }
    fn get_av_cell_id(&self) -> i32 {
        -1
    }
    fn get_prev_seen_cell_id(&self) -> i32 {
        0
    }
    fn set_prev_seen_cell_id(&mut self, _id: i32) {}
    fn get_prev_task_cell_id(&self) -> i32 {
        0
    }
    fn set_prev_task_cell_id(&mut self, _id: i32) {}
    fn add_reached_task_cell(&mut self) {}
    fn get_cell_data(&self) -> i32 {
        -1
    }
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
    fn get_input_at(&self, _i: i32) -> i32 {
        0
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
    fn state_grid_name(&self) -> String {
        String::new()
    }
    fn state_grid_state_id(&self, _name: &str) -> i32 {
        -1
    }
    fn state_grid_num_states(&self) -> i32 {
        0
    }
    fn state_grid_width(&self) -> i32 {
        0
    }
    fn state_grid_height(&self) -> i32 {
        0
    }
    fn state_grid_state_at(&self, _cell: i32) -> i32 {
        0
    }
    fn deme_cell_position_x(&self, _cell_id: i32) -> i32 {
        0
    }
    fn deme_get_relative_cell_id(&self, cell_id: i32) -> i32 {
        cell_id
    }
    fn deme_num_events(&self) -> i32 {
        0
    }
    fn deme_cell_event_id(&self, _i: i32) -> i32 {
        -1
    }
    fn deme_num_movement_predicates(&self) -> i32 {
        0
    }
    fn deme_mov_predicate_event(&self, _predicate: i32, _event: i32) -> Option<(i32, bool)> {
        None
    }
    /// `cPopulation::NumberOfOrganismsInGroup`: no organisms, no groups.
    fn number_of_organisms_in_group(&mut self, _group_id: i32) -> i32 {
        0
    }
    fn world_x(&self) -> i32 {
        0
    }
    fn world_y(&self) -> i32 {
        0
    }
    fn use_avatars(&self) -> bool {
        false
    }
    fn match_already_produced(&self) -> bool {
        false
    }
    fn pheromone_resource_amount(&self) -> f64 {
        0.0
    }
    fn get_resources(&self) -> Vec<f64> {
        Vec::new()
    }
    fn get_av_resources(&self) -> Vec<f64> {
        Vec::new()
    }
    fn change_reaction_resource(&mut self, _task_id: usize, _resource_index: i32) -> bool {
        false
    }
    fn cell_kaboom_executed(&self, _cell_id: i32) -> Option<bool> {
        None
    }
    fn clear_kaboom_executed(&mut self, _cell_id: i32) {}
    /// `std::rand()` (`Task_ConsumePublicGood`, which only draws when it
    /// finds producers, and the population is empty here).
    fn std_rand(&mut self) -> i32 {
        self.libc_rand.next_value()
    }
    fn stats_add_tag(&mut self, _tag: i32, _value: i32) {}
    fn stats_add_string_bits_matched_value(&mut self, _name: &str, _value: i32) {}
    fn stats_inc_perfect_match(&mut self, _amount: i32) {}
    fn stats_inc_perfect_match_org(&mut self) {}
}

impl OutputHost for AnalyzeServices {
    /// Never asked: every organism here runs in a test CPU, whose context
    /// is in test mode, and `cEnvironment::GetTaskProbability` returns -1
    /// in test mode before reaching `cPhenPlastUtil`.
    fn task_probability(
        &mut self,
        _test: avida_hardware::task_adapter::PlasticityTest<'_>,
        _task_id: i32,
    ) -> f64 {
        self.unsupported(
            "phenotypic-plasticity task probability outside test mode in analyze mode \
             (C++: cPhenPlastUtil::GetTaskProbability; not reachable from a test CPU)",
        );
        -1.0
    }
    /// `cEnvironment::DoProcesses` only metabolizes a fragment in a
    /// population cell; a test organism's cell is -1 ("can't do this in
    /// the test cpu"), so nothing is consumed.
    fn metabolize_genome_fragment(&mut self, _rng: &mut AvidaRng) -> Option<i32> {
        None
    }
}

impl TestCpuHost for AnalyzeServices {
    fn stats_inc_resamplings(&mut self) {}
    fn stats_inc_failed_resamplings(&mut self) {}
    fn stats_add_new_task_count(&mut self, _task: i32) {}
}

/// `cStats` as `cPhenotype` updates it; analyze mode never reports it.
#[derive(Debug, Default)]
struct NullStats;

impl PhenotypeStats for NullStats {
    fn add_energy_testament_accepted(&mut self, _value: f64) {}
    fn age_task_event(&mut self, _org_id: i32, _task_id: usize, _org_age: i32) {}
    fn add_new_task_count(&mut self, _task_num: usize) {}
    fn add_other_task_counts(&mut self, _task_num: usize, _prev_tasks: i32, _cur_tasks: i32) {}
    fn add_new_reaction_count(&mut self, _reaction_num: usize) {}
    fn add_task_switch_time(&mut self, _t1: i32, _t2: i32, _time: i32) {}
}

/// How the environment was built: the inputs of `cEnvironment::Load` in
/// `cWorld::setup` and every `ENVIRONMENT` line since, with the
/// configuration each was read with. Replaying it builds an equal
/// environment for a worker thread ([`AnalyzeHost::worker_worlds`]); the
/// environment type cannot be cloned (its task library,
/// `avida_hardware::task_adapter::TaskLibRegistry`, does not implement
/// `Clone`), and every replay is checked against the live environment.
#[derive(Debug, Clone)]
struct EnvRecipe {
    config: Config,
    env_file: String,
    defs: Vec<(String, String)>,
    lines: Vec<(String, Config)>,
}

/// One worker thread's world (see [`AnalyzeHost::worker_worlds`]).
struct WorkerTestWorld<'h> {
    config: &'h Config,
    hw: &'h HardwareManager,
    group_attack: &'h BTreeMap<String, usize>,
    test_sterilize: bool,
    env: Env,
    services: AnalyzeServices,
    stats: NullStats,
}

impl WorkerWorld for WorkerTestWorld<'_> {
    fn with_test_world(
        &mut self,
        rng: &mut AvidaRng,
        analyze_mode: bool,
        f: &mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>,
    ) -> Result<()> {
        lend_test_world(
            LentParts {
                config: self.config,
                env: &mut self.env,
                hw: self.hw,
                stats: &mut self.stats,
                services: &mut self.services,
                group_attack: self.group_attack,
                test_sterilize: self.test_sterilize,
            },
            rng,
            analyze_mode,
            f,
        )
    }

    fn inst_sets(&self) -> &InstSetRegistry {
        self.hw.registry()
    }

    fn state_digest(&self) -> String {
        format!("{:?}", self.env)
    }
}

/// The parts of a world a test CPU borrows.
struct LentParts<'a> {
    config: &'a Config,
    env: &'a mut Env,
    hw: &'a HardwareManager,
    stats: &'a mut NullStats,
    services: &'a mut AnalyzeServices,
    group_attack: &'a BTreeMap<String, usize>,
    test_sterilize: bool,
}

/// [`AnalyzeHost::with_test_world`] over `parts`: the world as the test
/// CPU sees it, then any service the analyze world could not answer as an
/// error.
fn lend_test_world(
    parts: LentParts<'_>,
    rng: &mut AvidaRng,
    analyze_mode: bool,
    f: &mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>,
) -> Result<()> {
    let group_attack = parts.group_attack;
    let count = |name: &str| group_attack.get(name).copied().unwrap_or(0);
    let mut world = TestWorld {
        config: parts.config,
        environment: parts.env,
        rng,
        hardware_manager: parts.hw,
        stats: parts.stats,
        host: &mut *parts.services,
        // cStats starts at update -1 and analyze mode never advances it.
        update: -1,
        tot_creatures: 0,
        sense_size: 0,
        group_attack_inst_count: &count,
        // No population in analyze mode.
        counts: TestWorldCounts::default(),
        live_orgs: Some(Vec::new()),
        analyze_mode,
        org_fault_reporting: false,
        verbosity: parts.config.verbosity,
        test_sterilize: parts.test_sterilize,
    };
    let r = f(&mut world);
    if let Some(what) = parts.services.unsupported.take() {
        return Err(AnalyzeError::world(what));
    }
    r
}

/// Whether a test can change the environment for the tests after it: the
/// `xor-max` task repoints its reaction at another resource
/// (`cEnvironment::ChangeResource` from `Task_XorMax`).
fn tests_change_environment(env: &Env) -> bool {
    let lib = &env.tasklib().0;
    (0..lib.get_size()).any(|i| {
        lib.get_task(i)
            .is_some_and(|t| t.kind() == avida_tasks::TaskKind::XorMax)
    })
}

/// Run `f` with this thread's diagnostics (`avida_apto::diag`) collected and
/// thrown away, leaving text collected before untouched.
fn without_diagnostics<T>(f: impl FnOnce() -> T) -> T {
    let earlier = if diag::is_capturing() {
        Some(diag::take_captured())
    } else {
        diag::start_capture();
        None
    };
    let r = f();
    match earlier {
        Some(text) => {
            let _ = diag::take_captured();
            if !text.stdout.is_empty() {
                diag::stdout(format_args!("{}", text.stdout));
            }
            if !text.stderr.is_empty() {
                diag::stderr(format_args!("{}", text.stderr));
            }
        }
        None => {
            let _ = diag::stop_capture();
        }
    }
    r
}

/// The analyze-mode `cWorld`. See the module documentation.
pub struct AnalyzeWorld {
    config: Config,
    working_dir: String,
    env: Env,
    env_recipe: EnvRecipe,
    hw: HardwareManager,
    rng: AvidaRng,
    output: OutputManager,
    /// `m_ctx.GetAnalyzeMode()` of the default context.
    analyze_mode: bool,
    /// `m_test_sterilize`
    test_sterilize: bool,
    /// `cStats::GetGroupAttackInsts(inst_set).GetSize()` per set.
    group_attack: BTreeMap<String, usize>,
    services: AnalyzeServices,
    stats: NullStats,
}

impl std::fmt::Debug for AnalyzeWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalyzeWorld")
            .field("working_dir", &self.working_dir)
            .field("analyze_mode", &self.analyze_mode)
            .finish()
    }
}

/// What `cWorld::Initialize` produced.
pub struct WorldInit {
    /// `None` where the C++ returns NULL or exits during set-up; the caller
    /// prints `stdout` and `stderr` and ends with `exit_code`.
    pub world: Option<AnalyzeWorld>,
    /// With no world: -1 (`primitive.cc` returns -1 when `Initialize`
    /// returns NULL) or the code of an `exit()` inside set-up.
    pub exit_code: i32,
    /// Text the C++ writes to stdout during set-up (resource set-up lines
    /// at `VERBOSITY > 1`).
    pub stdout: String,
    /// Text for stderr: messages written to `cerr` during set-up, then the
    /// set-up feedback as `primitive.cc` prints it.
    pub stderr: String,
}

/// `cEventList::AddEventFileFormat` up to the action lookup: `Err` is the
/// line the C++ writes to `cerr` before failing.
fn check_event_line(line: &str) -> std::result::Result<(), String> {
    let mut cur_line = line.to_string();
    let cur_word = su::pop_word(&mut cur_line);
    if cur_word != "i" && cur_word != "immediate" {
        if !matches!(
            cur_word.as_str(),
            "u" | "update" | "g" | "generation" | "b" | "births" | "o" | "org_id"
        ) {
            return Err(format!("error: unrecognized event trigger '{cur_word}'"));
        }
        let mut timing_str = su::pop_word(&mut cur_line);
        let tmp = su::pop(&mut timing_str, ':');
        if !(su::is_number(&tmp) || tmp == "begin") {
            return Err(format!("error: invalid event timing '{tmp}'"));
        }
    }
    let name = su::pop_word(&mut cur_line);
    // cEventList::AddEvent: cActionLibrary::Create returns NULL.
    if !is_registered_action(&name) {
        return Err(format!("error: unable to load event '{name}'"));
    }
    Ok(())
}

impl AnalyzeWorld {
    /// `cWorld::Initialize(cfg, working_dir, new_world, feedback, defs)`
    /// without [`SetupHooks`].
    ///
    /// `Err` is a loud failure for a setting this port does not set up (see
    /// the module documentation).
    pub fn initialize(
        config: Config,
        working_dir: &str,
        defs: &[(String, String)],
    ) -> Result<WorldInit> {
        Self::initialize_with(config, working_dir, defs, &mut NoSetupHooks)
    }

    /// `cWorld::Initialize(cfg, working_dir, new_world, feedback, defs)`,
    /// with the migration matrix and the population set up by `hooks`.
    pub fn initialize_with(
        mut config: Config,
        working_dir: &str,
        defs: &[(String, String)],
        hooks: &mut dyn SetupHooks,
    ) -> Result<WorldInit> {
        let mut stdout = String::new();
        let mut direct_err = String::new();
        let mut fb = Feedback::new();
        let mut success = true;

        // Setup Random Number Generator
        let mut rng = AvidaRng::new(config.random_seed);

        // Output Manager
        let output = OutputManager::new(&get_absolute_path(&config.data_dir, working_dir));

        // Initialize the default environment...
        let mut env = Env::new(&config, TaskLibRegistry::new());
        let env_file = config.environment_file.clone();
        let env_recipe = EnvRecipe {
            config: config.clone(),
            env_file: env_file.clone(),
            defs: defs.to_vec(),
            lines: Vec::new(),
        };
        if env
            .load(&env_file, working_dir, &mut fb, Some(defs), &config)
            .is_err()
        {
            success = false;
        }

        if config.demes_migration_method == 4 {
            match hooks.load_migration_matrix(&config, working_dir, &mut fb) {
                Some(true) => {}
                Some(false) => success = false,
                None => {
                    return Err(AnalyzeError::unsupported(
                        "DEMES_MIGRATION_METHOD 4",
                        "cMigrationMatrix::Load (cWorld::setup)",
                    ))
                }
            }
        }

        // Initialize the hardware manager, loading all of the instruction sets
        let mut list: Vec<String> = config.instsets().to_vec();
        if config.inst_set_load_legacy != 0 {
            let conv = convert_legacy_inst_set_file(
                &config.inst_set,
                config.hardware_type,
                working_dir,
                &mut list,
                &mut fb,
            );
            if let Some(name) = conv.inst_set_setting {
                config.inst_set = name;
            }
            if !conv.success {
                success = false;
            }
        }
        let mut hw = HardwareManager::new();
        let code_cfg = InstCodeConfig {
            inst_code_length: config.inst_code_length,
            inst_code_default_type: config.inst_code_default_type,
        };
        if !hw
            .load_inst_sets(&list, code_cfg, &mut fb)
            .map_err(AnalyzeError::world)?
        {
            success = false;
        }
        if hw.registry().num_inst_sets() == 0 {
            fb.error("no instruction sets defined");
            if config.inst_set_load_legacy == 0
                && !config.inst_set.is_empty()
                && config.inst_set != "-"
            {
                fb.notify("It looks like you are attempting to load a legacy format instruction set file.  Try setting INST_SET_LOAD_LEGACY to 1.");
            }
            success = false;
        }
        if !success {
            return Ok(WorldInit {
                world: None,
                exit_code: -1,
                stdout,
                stderr: direct_err + &fb.render(),
            });
        }

        let test_sterilize = config.sterilize_fatal > 0.0
            || config.sterilize_detrimental > 0.0
            || config.sterilize_neutral > 0.0
            || config.sterilize_beneficial > 0.0
            || config.sterilize_taskloss > 0.0;

        let mut group_attack = BTreeMap::new();
        for is in hw.registry().inst_sets() {
            let n = (0..is.size())
                .filter(|&i| GROUP_ATTACK_INSTS.contains(&is.name(i)))
                .count();
            group_attack.insert(is.inst_set_name().to_string(), n);
        }

        // cPopulation(world)
        if config.energy_cap == -1.0 {
            config.energy_cap = f64::MAX;
        }
        if let Some(msg) = Self::setup_population(&mut config, &env, &mut rng, &mut stdout, hooks)?
        {
            // `cerr << ...; exit(2)` inside cResourceCount::Setup: the
            // set-up feedback is never printed.
            return Ok(WorldInit {
                world: None,
                exit_code: 2,
                stdout,
                stderr: direct_err + &msg + "\n",
            });
        }

        // Setup Event List
        let event_file = InitFile::new(&config.event_file, working_dir, None, Some(defs));
        let mut events_ok = event_file.was_opened();
        if events_ok {
            for line_id in 0..event_file.num_lines() {
                if let Err(line) = check_event_line(&event_file.get_line(line_id as i32)) {
                    direct_err.push_str(&line);
                    direct_err.push('\n');
                    events_ok = false;
                    break;
                }
            }
        }
        if !events_ok {
            fb.error("unable to load event file");
            return Ok(WorldInit {
                world: None,
                exit_code: -1,
                stdout,
                stderr: direct_err + &fb.render(),
            });
        }

        let world = AnalyzeWorld {
            config,
            working_dir: working_dir.to_string(),
            env,
            env_recipe,
            hw,
            rng,
            output,
            analyze_mode: false,
            test_sterilize,
            group_attack,
            services: AnalyzeServices::default(),
            stats: NullStats,
        };
        Ok(WorldInit {
            world: Some(world),
            exit_code: 0,
            stdout,
            stderr: direct_err + &fb.render(),
        })
    }

    /// `new cPopulation(this)`: through `hooks` when they can, else as far
    /// as [`AnalyzeWorld::setup_cell_grid`] reproduces it. `Some(msg)` is
    /// the `exit(2)` of an unknown resource geometry, which is always
    /// handled here: the population set-up of the hooks reports it without
    /// the lines printed before it.
    fn setup_population(
        config: &mut Config,
        env: &Env,
        rng: &mut AvidaRng,
        stdout: &mut String,
        hooks: &mut dyn SetupHooks,
    ) -> Result<Option<String>> {
        let unknown_geometry = env.resource_lib().resources().iter().any(|r| {
            !matches!(
                r.geometry,
                geometry::GLOBAL | geometry::GRID | geometry::TORUS | geometry::PARTIAL
            )
        });
        if !unknown_geometry {
            if let Some(r) = hooks.setup_population(config, env, rng) {
                let lines = r.map_err(AnalyzeError::world)?;
                for line in lines {
                    stdout.push_str(&line);
                    stdout.push('\n');
                }
                return Ok(None);
            }
        }
        Self::setup_cell_grid(config, env, rng, stdout)
    }

    /// The observable part of `cPopulation::SetupCellGrid`: the topology
    /// builders that draw from the world RNG (refused), `BuildTimeSlicer`
    /// (which may seed its own RNG from the world's), and the resource
    /// set-up messages. `Some(msg)` is the `exit(2)` of an unknown resource
    /// geometry.
    fn setup_cell_grid(
        config: &Config,
        env: &Env,
        rng: &mut AvidaRng,
        stdout: &mut String,
    ) -> Result<Option<String>> {
        let num_demes = config.num_demes.max(1);
        if num_demes > 1 {
            return Err(AnalyzeError::unsupported(
                format!("NUM_DEMES {num_demes}"),
                "cDeme::Setup (cPopulation::SetupCellGrid)",
            ));
        }
        match config.world_geometry {
            geometry::GRID
            | geometry::TORUS
            | geometry::CLIQUE
            | geometry::HEX
            | geometry::LATTICE => {}
            geometry::RANDOM_CONNECTED => {
                return Err(AnalyzeError::unsupported(
                    "WORLD_GEOMETRY 7",
                    "build_random_connected_network (cPopulation::SetupCellGrid)",
                ))
            }
            geometry::SCALE_FREE => {
                return Err(AnalyzeError::unsupported(
                    "WORLD_GEOMETRY 8",
                    "build_scale_free (cPopulation::SetupCellGrid)",
                ))
            }
            g => {
                return Err(AnalyzeError::fatal(format!(
                    "WORLD_GEOMETRY {g}: cPopulation::SetupCellGrid asserts"
                )))
            }
        }
        let cells = (config.world_x.max(0) as usize) * (config.world_y.max(0) as usize);
        avida_apto::scheduler::build_time_slicer(config.slicing_method, cells, rng)
            .map_err(AnalyzeError::world)?;

        // Setup the resources...
        let resources = env.resource_lib().resources();
        if resources.iter().any(|r| r.deme_resource) {
            return Err(AnalyzeError::unsupported(
                "deme resources",
                "cDeme::SetupDemeRes (cPopulation::SetupCellGrid)",
            ));
        }
        for res in resources {
            if res.hgt_metabolize {
                return Err(AnalyzeError::unsupported(
                    "HGT resources",
                    "cPopulation::SetupCellGrid (m_hgt_resid)",
                ));
            }
            // cResourceCount::Setup
            let geo_name = match res.geometry {
                geometry::GLOBAL => "GLOBAL",
                geometry::GRID => "GRID",
                geometry::TORUS => "TORUS",
                geometry::PARTIAL => "PARTIAL",
                g => {
                    return Ok(Some(format!(
                        "[cResourceCount::Setup] Unknown resource geometry {g}.  Exiting."
                    )))
                }
            };
            if res.isgradient && (res.geometry == geometry::GRID || res.geometry == geometry::TORUS)
            {
                return Err(AnalyzeError::unsupported(
                    format!("gradient resource '{}'", res.name()),
                    "cGradientCount (cResourceCount::Setup draws from the world RNG)",
                ));
            }
            if config.verbosity > VERBOSE_NORMAL {
                let decay = 1.0 - res.outflow;
                stdout.push_str(&format!(
                    "Setting up resource {}({}) with initial quantity={}, decay={}, inflow={}\n",
                    res.name(),
                    geo_name,
                    avida_apto::fmt_g(res.initial, 6),
                    avida_apto::fmt_g(decay, 6),
                    avida_apto::fmt_g(res.inflow, 6)
                ));
                if res.geometry == geometry::GRID || res.geometry == geometry::TORUS {
                    stdout.push_str(&format!(
                        "  Inflow rectangle ({},{}) to ({},{})\n",
                        res.inflow_x1, res.inflow_y1, res.inflow_x2, res.inflow_y2
                    ));
                    stdout.push_str(&format!(
                        "  Outflow rectangle ({},{}) to ({},{})\n",
                        res.outflow_x1, res.outflow_y1, res.outflow_x2, res.outflow_y2
                    ));
                    stdout.push_str(&format!(
                        "  xdiffuse={}, xgravity={}, ydiffuse={}, ygravity={}\n",
                        avida_apto::fmt_g(res.xdiffuse, 6),
                        avida_apto::fmt_g(res.xgravity, 6),
                        avida_apto::fmt_g(res.ydiffuse, 6),
                        avida_apto::fmt_g(res.ygravity, 6)
                    ));
                }
            }
        }
        if config.enable_hgt != 0 {
            return Err(AnalyzeError::unsupported(
                "ENABLE_HGT",
                "cPopulation::SetupCellGrid (HGT resource check)",
            ));
        }
        Ok(None)
    }

    /// `GetRandom().Seed()`.
    pub fn random_seed(&self) -> i32 {
        self.rng.seed()
    }

    /// `Avida::Output::Manager::Of(world)->OutputPath()`.
    pub fn output_path(&self) -> &str {
        self.output.output_path()
    }

    /// The instruction set registry of the hardware manager.
    pub fn hardware_manager(&self) -> &HardwareManager {
        &self.hw
    }

    /// An environment built the way this world's was ([`EnvRecipe`]), or
    /// `None` when it does not come out equal to the live one (`live` is
    /// the live one's `Debug` text). The live environment may already have
    /// computed its initial resource levels (a cache filled by the first
    /// test CPU), so a mismatch is retried with the copy's computed too.
    fn rebuild_env(&self, live: &str) -> Option<Env> {
        let r = &self.env_recipe;
        let env = without_diagnostics(|| {
            let mut env = Env::new(&r.config, TaskLibRegistry::new());
            let mut fb = Feedback::new();
            env.load(
                &r.env_file,
                &self.working_dir,
                &mut fb,
                Some(&r.defs),
                &r.config,
            )
            .ok()?;
            for (line, config) in &r.lines {
                let _ = env.load_line(line, &mut Feedback::new(), config);
            }
            Some(env)
        })?;
        if format!("{env:?}") == live {
            return Some(env);
        }
        let _ = env.resource_lib().initial_resource_levels();
        (format!("{env:?}") == live).then_some(env)
    }
}

impl AnalyzeHost for AnalyzeWorld {
    fn config(&self) -> &Config {
        &self.config
    }

    fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    fn working_dir(&self) -> &str {
        &self.working_dir
    }

    fn output(&mut self) -> &mut OutputManager {
        &mut self.output
    }

    fn inst_sets(&self) -> &InstSetRegistry {
        self.hw.registry()
    }

    fn inst_sets_mut(&mut self) -> Result<&mut InstSetRegistry> {
        Ok(self.hw.registry_mut())
    }

    fn num_tasks(&self) -> i32 {
        self.env.num_tasks()
    }

    fn task_desc(&self, task_id: i32) -> Result<String> {
        if task_id < 0 || task_id >= self.env.num_tasks() {
            return Err(AnalyzeError::fatal(format!(
                "task {task_id} out of range (0..{}); the C++ indexes past the task array",
                self.env.num_tasks()
            )));
        }
        Ok(self.env.tasklib().task_desc(task_id).to_string())
    }

    fn input_size(&self) -> i32 {
        self.env.input_size()
    }

    fn num_state_grids(&self) -> i32 {
        self.env.num_state_grids()
    }

    fn world_rng(&mut self) -> Option<&mut AvidaRng> {
        Some(&mut self.rng)
    }

    fn analyze_mode(&self) -> bool {
        self.analyze_mode
    }

    fn set_analyze_mode(&mut self, on: bool) {
        self.analyze_mode = on;
    }

    fn load_environment_line(&mut self, line: &str, feedback: &mut Feedback) -> Result<()> {
        // The C++ ignores the return value; the messages are in `feedback`.
        let _ = self.env.load_line(line, feedback, &self.config);
        self.env_recipe
            .lines
            .push((line.to_string(), self.config.clone()));
        Ok(())
    }

    fn activate_null_inst(&mut self, inst_set: &str) -> Result<avida_genome::Instruction> {
        let is = self
            .hw
            .registry_mut()
            .get_inst_set_mut(inst_set)
            .ok_or_else(|| {
                AnalyzeError::fatal(format!(
                    "ActivateNullInst: instruction set '{inst_set}' is unknown or still shared with live hardware"
                ))
            })?;
        Ok(is.activate_null_inst())
    }

    fn with_test_world(
        &mut self,
        ctx: TestContext<'_>,
        f: &mut dyn FnMut(&mut TestWorld<'_>) -> Result<()>,
    ) -> Result<()> {
        let (rng, analyze_mode) = match ctx {
            TestContext::Default => (&mut self.rng, self.analyze_mode),
            TestContext::Job(rng) => (rng, false),
        };
        lend_test_world(
            LentParts {
                config: &self.config,
                env: &mut self.env,
                hw: &self.hw,
                stats: &mut self.stats,
                services: &mut self.services,
                group_attack: &self.group_attack,
                test_sterilize: self.test_sterilize,
            },
            rng,
            analyze_mode,
            f,
        )
    }

    /// The environment is the only state a test changes that later tests
    /// read (the services record nothing, the statistics are discarded), so
    /// each worker gets a rebuilt environment checked equal to the live one
    /// and fresh services. Refused when the environment has a task that
    /// changes it during a test (`xor-max`), or when a rebuilt copy is not
    /// equal to it.
    fn worker_worlds(&self, n: usize) -> Option<Vec<Box<dyn WorkerWorld + '_>>> {
        if tests_change_environment(&self.env) {
            return None;
        }
        let live = format!("{:?}", self.env);
        let mut worlds: Vec<Box<dyn WorkerWorld + '_>> = Vec::with_capacity(n);
        for _ in 0..n {
            worlds.push(Box::new(WorkerTestWorld {
                config: &self.config,
                hw: &self.hw,
                group_attack: &self.group_attack,
                test_sterilize: self.test_sterilize,
                env: self.rebuild_env(&live)?,
                services: AnalyzeServices::default(),
                stats: NullStats,
            }));
        }
        Some(worlds)
    }

    /// A test changes the environment in one way that later tests do not
    /// see: the first test CPU computes its initial resource levels (after
    /// which `RESOURCE` lines fail). Worker environments that differ from
    /// the live one only in that are adopted by computing the live one's;
    /// any other difference refuses.
    fn adopt_worker_states(&mut self, digests: &[String]) -> bool {
        let live = format!("{:?}", self.env);
        if digests.iter().all(|d| *d == live) {
            return true;
        }
        // Compute the cache on a copy first, so a refusal changes nothing.
        let Some(copy) = self.rebuild_env(&live) else {
            return false;
        };
        let _ = copy.resource_lib().initial_resource_levels();
        let computed = format!("{copy:?}");
        if !digests.iter().all(|d| *d == live || *d == computed) {
            return false;
        }
        let _ = self.env.resource_lib().initial_resource_levels();
        format!("{:?}", self.env) == computed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_lines() {
        assert!(check_event_line("u begin Exit").is_ok());
        assert!(check_event_line("u 100:100 PrintAverageData").is_ok());
        assert!(check_event_line("i Inject default-heads.org").is_ok());
        assert_eq!(
            check_event_line("x 1 Exit"),
            Err("error: unrecognized event trigger 'x'".to_string())
        );
        assert_eq!(
            check_event_line("u never Exit"),
            Err("error: invalid event timing 'never'".to_string())
        );
        assert_eq!(
            check_event_line("u 1 NoSuchAction"),
            Err("error: unable to load event 'NoSuchAction'".to_string())
        );
    }
}
