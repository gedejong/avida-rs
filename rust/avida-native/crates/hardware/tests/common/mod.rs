//! Shared set-up for the reference comparisons: load a consistency test's
//! config directory the way `cWorld::setup` does in analyze mode (config,
//! environment, instruction sets) and run genomes through the test CPU.

#![allow(dead_code)]

use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_config::feedback::Feedback as ConfigFeedback;
use avida_genome::inst_set_registry::convert_legacy_inst_set_file;
use avida_genome::{Feedback as GenomeFeedback, Genome, InstCodeConfig, InstructionSequence};
use avida_hardware::task_adapter::{Env, OutputHost, TaskLibRegistry};
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu, TestCpuHost, TestWorld, TestWorldCounts};
use avida_hardware::tracer::{HardwareTracerPtr, StringTracer};
use avida_hardware::HardwareManager;
use avida_phenotype::PhenotypeStats;
use avida_tasks::TaskWorld;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn avida_core() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core")
}

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// World-level services of an analyze-mode world with an empty population.
#[derive(Default)]
pub struct NullHost;

impl TaskWorld for NullHost {
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
    fn std_rand(&mut self) -> i32 {
        0
    }
    fn stats_add_tag(&mut self, _tag: i32, _value: i32) {}
    fn stats_add_string_bits_matched_value(&mut self, _name: &str, _value: i32) {}
    fn stats_inc_perfect_match(&mut self, _amount: i32) {}
    fn stats_inc_perfect_match_org(&mut self) {}
}

impl OutputHost for NullHost {
    fn task_probability(&mut self, _rng: &mut AvidaRng, _task_id: i32) -> f64 {
        0.0
    }
    fn metabolize_genome_fragment(&mut self, _rng: &mut AvidaRng) -> Option<i32> {
        None
    }
}

impl TestCpuHost for NullHost {
    fn stats_inc_resamplings(&mut self) {}
    fn stats_inc_failed_resamplings(&mut self) {}
    fn stats_add_new_task_count(&mut self, _task: i32) {}
}

#[derive(Default)]
pub struct NullStats;

impl PhenotypeStats for NullStats {
    fn add_energy_testament_accepted(&mut self, _value: f64) {}
    fn age_task_event(&mut self, _org_id: i32, _task_id: usize, _org_age: i32) {}
    fn add_new_task_count(&mut self, _task_num: usize) {}
    fn add_other_task_counts(&mut self, _task_num: usize, _prev_tasks: i32, _cur_tasks: i32) {}
    fn add_new_reaction_count(&mut self, _reaction_num: usize) {}
    fn add_task_switch_time(&mut self, _t1: i32, _t2: i32, _time: i32) {}
}

fn no_group_attack(_: &str) -> usize {
    0
}

/// An analyze-mode world: config, environment, instruction sets, RNG.
pub struct Fixture {
    pub dir: PathBuf,
    pub cfg: Config,
    pub env: Env,
    pub hwm: HardwareManager,
    pub rng: AvidaRng,
    host: NullHost,
    stats: NullStats,
}

impl Fixture {
    /// `cWorld::setup` for `dir` (avida.cfg, `sets` applied on top).
    pub fn load(dir: &Path, sets: &[(&str, &str)]) -> Result<Fixture, String> {
        Fixture::load_inner(dir, sets, None)
    }

    /// As [`Fixture::load`], with the `INSTSET`/`INST` lines of avida.cfg
    /// replaced by `instset`.
    pub fn load_with_instset(dir: &Path, instset: Vec<String>) -> Result<Fixture, String> {
        Fixture::load_inner(dir, &[], Some(instset))
    }

    fn load_inner(
        dir: &Path,
        sets: &[(&str, &str)],
        instset: Option<Vec<String>>,
    ) -> Result<Fixture, String> {
        let wdir = dir.to_str().ok_or("non-UTF-8 path")?;
        let mut cfg = Config::new();
        let mut fb = ConfigFeedback::new();
        if !cfg.load("avida.cfg", wdir, Some(&mut fb), None, false) {
            return Err(format!("avida.cfg in {}", dir.display()));
        }
        let mut s: Vec<(String, String)> = sets
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        cfg.set_from_map(&mut s);

        let mut env = Env::new(&cfg, TaskLibRegistry::new());
        let mut efb = ConfigFeedback::new();
        let env_file = cfg.environment_file.clone();
        env.load(&env_file, wdir, &mut efb, None, &cfg)?;
        if efb.num_errors() > 0 {
            return Err(efb.render());
        }

        let replaced = instset.is_some();
        let mut list: Vec<String> = instset.unwrap_or_else(|| cfg.instsets().to_vec());
        let mut gfb = GenomeFeedback::new();
        if cfg.inst_set_load_legacy != 0 && !replaced {
            let conv = convert_legacy_inst_set_file(
                &cfg.inst_set,
                cfg.hardware_type,
                wdir,
                &mut list,
                &mut gfb,
            );
            if !conv.success {
                return Err(format!("legacy inst set: {:?}", gfb));
            }
        }
        let mut hwm = HardwareManager::new();
        let code_cfg = InstCodeConfig {
            inst_code_length: cfg.inst_code_length,
            inst_code_default_type: cfg.inst_code_default_type,
        };
        if !hwm.load_inst_sets(&list, code_cfg, &mut gfb)? {
            return Err(format!("inst sets: {:?}", gfb));
        }
        // The world RNG as the first analyze command sees it: seeded by
        // cWorld::setup, then one draw by cPopulation::BuildTimeSlicer
        // (probabilistic slicers seed their own RNG from it) and one by the
        // cAnalyzeJobQueue constructor (`m_job_seed_rng`, cAnalyzeJobQueue.cc:46).
        // The geometries of these tests draw nothing in SetupCellGrid.
        let mut rng = AvidaRng::new(cfg.random_seed);
        let cells = (cfg.world_x.max(0) as usize) * (cfg.world_y.max(0) as usize);
        avida_apto::scheduler::build_time_slicer(cfg.slicing_method, cells, &mut rng)?;
        let max_seed = rng.max_seed();
        rng.get_int(max_seed);
        Ok(Fixture {
            dir: dir.to_path_buf(),
            cfg,
            env,
            hwm,
            rng,
            host: NullHost,
            stats: NullStats,
        })
    }

    /// The analyze-mode world a test CPU sees (`U:-1`).
    pub fn world(&mut self) -> TestWorld<'_> {
        TestWorld {
            config: &self.cfg,
            environment: &mut self.env,
            rng: &mut self.rng,
            hardware_manager: &self.hwm,
            stats: &mut self.stats,
            host: &mut self.host,
            update: -1,
            tot_creatures: 0,
            sense_size: 0,
            group_attack_inst_count: &no_group_attack,
            counts: TestWorldCounts::default(),
            live_orgs: Some(Vec::new()),
            analyze_mode: true,
            org_fault_reporting: false,
            verbosity: 1,
            test_sterilize: false,
        }
    }

    /// A genome of the default instruction set from its letter string.
    pub fn genome_from_letters(&self, letters: &str) -> Genome {
        let is = self
            .hwm
            .registry()
            .default_inst_set()
            .expect("an instruction set is loaded");
        Genome::with_inst_set(
            is.hardware_type(),
            is.inst_set_name(),
            InstructionSequence::from_string(letters),
        )
    }

    /// `Util::LoadGenomeDetailFile` on a file of the config directory.
    pub fn load_org(&self, name: &str) -> Genome {
        let mut fb = GenomeFeedback::new();
        avida_genome::load_genome_detail_file(
            name,
            self.dir.to_str().unwrap(),
            self.hwm.registry(),
            &mut fb,
            "",
        )
        .expect("known instructions")
        .expect("organism loads")
    }
}

/// `TRACE` of one genome with a fresh test CPU: the trace text and the
/// finished test info.
pub fn trace(
    fx: &mut Fixture,
    cpu: &mut TestCpu,
    genome: &Genome,
    random_inputs: bool,
) -> Result<(String, CpuTestInfo), String> {
    trace_at_update(fx, cpu, genome, random_inputs, -1)
}

/// `TRACE <dir> 0 <update> <random>`: as [`trace`] with the initial
/// resource levels looked up exactly at `update`.
pub fn trace_at_update(
    fx: &mut Fixture,
    cpu: &mut TestCpu,
    genome: &Genome,
    random_inputs: bool,
    update: i32,
) -> Result<(String, CpuTestInfo), String> {
    let tracer = Arc::new(Mutex::new(StringTracer::default()));
    let ptr: HardwareTracerPtr = tracer.clone();
    let mut info = CpuTestInfo::default();
    info.set_trace_execution(Some(ptr));
    info.use_random_inputs(random_inputs);
    // `SetResourceOptions(0, m_resources, update,
    // m_resource_time_spent_offset)`: RES_INITIAL, the environment's
    // initial levels looked up exactly at `update` (all 0 unless 0).
    info.set_resource_options(0, None, update, 0);
    let mut world = fx.world();
    cpu.test_genome(&mut world, &mut info, genome)?;
    info.set_trace_execution(None);
    let text = tracer.lock().map(|t| t.text.clone()).unwrap_or_default();
    Ok((text, info))
}

/// The first differing line of two texts, for assertion messages.
pub fn first_difference(a: &str, b: &str) -> Option<String> {
    let al: Vec<&str> = a.lines().collect();
    let bl: Vec<&str> = b.lines().collect();
    for i in 0..al.len().max(bl.len()) {
        let x = al.get(i).copied().unwrap_or("<missing>");
        let y = bl.get(i).copied().unwrap_or("<missing>");
        if x != y {
            let from = i.saturating_sub(16);
            let ctx: Vec<String> = (from..i)
                .map(|k| format!("   {}", al.get(k).copied().unwrap_or("")))
                .collect();
            return Some(format!(
                "line {}:\n{}\n - rust: {}\n - c++ : {}",
                i + 1,
                ctx.join("\n"),
                x,
                y
            ));
        }
    }
    None
}
