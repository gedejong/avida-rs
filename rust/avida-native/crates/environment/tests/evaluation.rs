//! `cEnvironment::SetupInputs`, `TestOutput`, `TestRequisites` and
//! `DoProcesses` against hand-computed results, with a scripted organism.

use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_config::feedback::Feedback;
use avida_environment::{
    ContextPhenotype, EnvReqs, Environment, ReactionResult, SimpleTaskLib, TaskEnvHooks,
    TaskEvaluator, TaskRegistry,
};
use std::path::Path;

/// A scripted organism: per-task qualities, a call log, and the phenotype /
/// cell state `TestOutput` reads.
struct Org {
    quality: Vec<f64>,
    value: f64,
    cur_task: i32,
    on_divide: bool,
    has_organism: bool,
    cell_id: i32,
    stolen: Vec<i32>,
    first_reactions: Vec<i32>,
    task_prob: f64,
    fragments: Vec<i32>,
    resource_change: Option<(i32, String)>,
    log: Vec<String>,
}

impl Org {
    fn new(quality: &[f64]) -> Self {
        Org {
            quality: quality.to_vec(),
            value: 1.0,
            cur_task: -1,
            on_divide: false,
            has_organism: true,
            cell_id: 0,
            stolen: Vec::new(),
            first_reactions: Vec::new(),
            task_prob: -1.0,
            fragments: Vec::new(),
            resource_change: None,
            log: Vec::new(),
        }
    }
}

impl<T> TaskEvaluator<T> for Org {
    fn setup_tests(&mut self, _lib: &T) {
        self.log.push("setup".into());
    }
    fn set_task_entry(&mut self, _lib: &T, task_id: i32) {
        self.cur_task = task_id;
        self.log.push(format!("entry {}", task_id));
    }
    fn on_divide(&self) -> bool {
        self.on_divide
    }
    fn test_output(&mut self, _lib: &T, _rng: &mut AvidaRng) -> f64 {
        self.log.push(format!("test {}", self.cur_task));
        self.quality
            .get(self.cur_task as usize)
            .copied()
            .unwrap_or(0.0)
    }
    fn task_value(&self) -> f64 {
        self.value
    }
    fn has_organism(&self) -> bool {
        self.has_organism
    }
    fn cell_id(&self) -> i32 {
        self.cell_id
    }
    fn stolen_reaction_count(&self, reaction_id: usize) -> i32 {
        self.stolen.get(reaction_id).copied().unwrap_or(0)
    }
    fn set_first_reaction(&mut self, reaction_id: i32) {
        self.first_reactions.push(reaction_id);
    }
    fn task_probability(&mut self, _rng: &mut AvidaRng, task_id: i32) -> f64 {
        self.log.push(format!("prob {}", task_id));
        self.task_prob
    }
    fn metabolize_genome_fragment(&mut self, _rng: &mut AvidaRng) -> Option<i32> {
        self.fragments.pop()
    }
    fn take_resource_change(&mut self) -> Option<(i32, String)> {
        self.resource_change.take()
    }
}

fn env_from(lines: &[&str], config: &Config) -> Environment<SimpleTaskLib> {
    let mut env = Environment::new(config, SimpleTaskLib::new());
    let mut fb = Feedback::new();
    for l in lines {
        env.load_line(l, &mut fb, config)
            .unwrap_or_else(|e| panic!("{}: {}", l, e));
    }
    env
}

struct Run<'a> {
    task_count: &'a [i32],
    reaction_count: &'a mut [i32],
    resources: &'a [f64],
    rbins: &'a [f64],
    is_parasite: bool,
    test_mode: bool,
}

fn run<T: TaskRegistry>(
    env: &mut Environment<T>,
    org: &mut Org,
    rng: &mut AvidaRng,
    config: &Config,
    r: Run<'_>,
) -> ReactionResult {
    let mut result = ReactionResult::new(
        env.resource_lib().size() as usize,
        env.num_tasks() as usize,
        env.num_reactions() as usize,
    );
    env.test_output(
        rng,
        r.test_mode,
        config,
        &mut result,
        org,
        r.task_count,
        r.reaction_count,
        r.resources,
        r.rbins,
        r.is_parasite,
        None,
    );
    result
}

/// Evaluate `env` once for `org` with zero task/reaction counts.
fn run_simple(
    env: &mut Environment<SimpleTaskLib>,
    org: &mut Org,
    config: &Config,
    resources: &[f64],
) -> (ReactionResult, Vec<i32>) {
    let mut rng = AvidaRng::new(1);
    let tasks = vec![0; env.num_tasks() as usize];
    let mut reactions = vec![0; env.num_reactions() as usize];
    let rbins = vec![0.0; resources.len()];
    let result = run(
        env,
        org,
        &mut rng,
        config,
        Run {
            task_count: &tasks,
            reaction_count: &mut reactions,
            resources,
            rbins: &rbins,
            is_parasite: false,
            test_mode: false,
        },
    );
    (result, reactions)
}

fn default_env(config: &Config) -> Environment<SimpleTaskLib> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config");
    let mut env = Environment::new(config, SimpleTaskLib::new());
    let mut fb = Feedback::new();
    env.load(
        "environment.cfg",
        dir.to_str().unwrap(),
        &mut fb,
        None,
        config,
    )
    .unwrap();
    env
}

// -- TestOutput / TestRequisites ---------------------------------------------

#[test]
fn default_environment_rewards_and_max_count() {
    let config = Config::new();
    let mut env = default_env(&config);
    // nand at full quality, equ at half quality.
    let mut q = vec![0.0; 9];
    q[1] = 1.0;
    q[8] = 0.5;
    let mut org = Org::new(&q);
    org.value = 7.0;
    let (result, reactions) = run_simple(&mut env, &mut org, &config, &[]);

    assert!(result.active());
    assert!(result.task_done(1) && result.task_done(8) && !result.task_done(0));
    assert_eq!(result.task_quality(8), 0.5);
    assert_eq!(result.task_value(1), 7.0);
    assert!(result.reaction_triggered(1) && result.reaction_triggered(8));
    // pow: 2^(value * max_number * quality) for each.
    assert_eq!(result.mult_bonus_total(), 2.0 * 2f64.powf(5.0 * 0.5));
    assert_eq!(result.add_bonus_total(), 0.0);
    assert!(result.active_deme());
    assert_eq!(reactions, vec![0, 1, 0, 0, 0, 0, 0, 0, 1]);
    assert_eq!(org.first_reactions, vec![1, 8]);
    let mut expect = vec!["setup".to_string()];
    for t in 0..9 {
        expect.push(format!("entry {}", t));
        expect.push(format!("test {}", t));
    }
    assert_eq!(org.log, expect);

    // max_count=1: once nand has been counted it is no longer tested.
    let mut org = Org::new(&q);
    let mut rng = AvidaRng::new(1);
    let mut tasks = vec![0; 9];
    tasks[1] = 1;
    let mut reactions = vec![0; 9];
    let result = run(
        &mut env,
        &mut org,
        &mut rng,
        &config,
        Run {
            task_count: &tasks,
            reaction_count: &mut reactions,
            resources: &[],
            rbins: &[],
            is_parasite: false,
            test_mode: false,
        },
    );
    assert!(!result.task_done(1));
    assert!(!org.log.contains(&"test 1".to_string()));
    assert_eq!(reactions, vec![0, 0, 0, 0, 0, 0, 0, 0, 1]);
}

#[test]
fn nothing_performed_leaves_result_inactive() {
    let config = Config::new();
    let mut env = default_env(&config);
    let mut org = Org::new(&[]);
    let (result, reactions) = run_simple(&mut env, &mut org, &config, &[]);
    assert!(!result.active());
    assert_eq!(reactions, vec![0; 9]);
}

#[test]
fn inactive_reaction_and_set_active() {
    let config = Config::new();
    let mut env = env_from(
        &[
            "REACTION A not process:value=1",
            "REACTION B nand process:value=2",
            "SET_ACTIVE reaction A false",
        ],
        &config,
    );
    assert!(!env.reaction_lib().reaction(0).active);
    let mut org = Org::new(&[1.0, 1.0]);
    let (result, _) = run_simple(&mut env, &mut org, &config, &[]);
    assert!(!result.task_done(0));
    assert_eq!(result.add_bonus_total(), 2.0);
    assert_eq!(org.log, vec!["setup", "entry 1", "test 1"]);
    env.load_line("SET_ACTIVE REACTION A", &mut Feedback::new(), &config)
        .unwrap();
    assert!(env.reaction_lib().reaction(0).active);
}

#[test]
fn resource_limited_nine_resource_reactions() {
    // support/config/misc/environment-9resource.cfg: resNOT inflow=100
    // outflow=0.01; NOT process resource=resNOT value=1.0 frac=0.0025 max=25.
    let config = Config::new();
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config/misc");
    let mut env = Environment::new(&config, SimpleTaskLib::new());
    env.load(
        "environment-9resource.cfg",
        dir.to_str().unwrap(),
        &mut Feedback::new(),
        None,
        &config,
    )
    .unwrap();

    let mut q = vec![0.0; 9];
    q[0] = 1.0; // not
    q[1] = 1.0; // nand
    q[2] = 0.5; // and
    let mut org = Org::new(&q);
    let mut res = vec![0.0; 9];
    res[0] = 1000.0;
    res[1] = 0.0;
    res[2] = 20000.0;
    let (result, reactions) = run_simple(&mut env, &mut org, &config, &res);

    // NOT: 1000 * 0.0025 = 2.5 (below max 25), bonus 2.5 * 1.0.
    assert_eq!(result.consumed(0), 2.5);
    assert_eq!(result.reaction_add_bonus(0), 2.5);
    // NAND: its resource is empty; the task counts, the reaction does not.
    assert!(result.task_done(1));
    assert!(!result.reaction_triggered(1));
    assert_eq!(result.consumed(1), 0.0);
    // AND: 20000 * 0.0025 = 50, capped at 25, times quality 0.5 = 12.5;
    // bonus 12.5 * 2.0.
    assert_eq!(result.consumed(2), 12.5);
    assert_eq!(result.reaction_add_bonus(2), 25.0);
    assert_eq!(result.add_bonus_total(), 27.5);
    assert_eq!(result.mult_bonus_total(), 1.0);
    assert!(result.used_env_resource());
    assert_eq!(reactions, vec![1, 0, 1, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn shared_resource_and_min_consumption() {
    let config = Config::new();
    let mut env = env_from(
        &[
            "RESOURCE r",
            "REACTION A not process:resource=r:frac=0.5:max=100",
            "REACTION B nand process:resource=r:frac=0.5:max=100",
            "REACTION C and process:resource=r:frac=0.5:max=100:min=3",
        ],
        &config,
    );
    let mut org = Org::new(&[1.0, 1.0, 1.0]);
    let (result, reactions) = run_simple(&mut env, &mut org, &config, &[10.0]);
    // A: 10 * 0.5 = 5; B: (10 - 5) * 0.5 = 2.5; C: (10 - 7.5) * 0.5 =
    // 1.25 < min 3, so nothing.
    assert_eq!(result.consumed(0), 7.5);
    assert_eq!(result.reaction_add_bonus(0), 5.0);
    assert_eq!(result.reaction_add_bonus(1), 2.5);
    assert!(!result.reaction_triggered(2));
    assert_eq!(reactions, vec![1, 1, 0]);

    // depletable=0: the bonus is given, nothing is consumed.
    let mut env = env_from(
        &[
            "RESOURCE r",
            "REACTION A not process:resource=r:max=4:depletable=0",
        ],
        &config,
    );
    let mut org = Org::new(&[1.0]);
    let (result, _) = run_simple(&mut env, &mut org, &config, &[10.0]);
    assert_eq!(result.consumed(0), 0.0);
    assert_eq!(result.add_bonus_total(), 4.0);
}

#[test]
fn process_types() {
    let config = Config::new();
    let single = |line: &str, quality: f64, task_count: i32, res: &[f64]| {
        let mut lines = vec!["RESOURCE r"];
        lines.push(line);
        let mut env = env_from(&lines, &config);
        let mut org = Org::new(&[quality]);
        let mut rng = AvidaRng::new(3);
        let mut reactions = vec![0];
        run(
            &mut env,
            &mut org,
            &mut rng,
            &config,
            Run {
                task_count: &[task_count],
                reaction_count: &mut reactions,
                resources: res,
                rbins: &[0.0],
                is_parasite: false,
                test_mode: false,
            },
        )
    };

    let r = single("REACTION X not process:value=2:type=add", 1.0, 0, &[0.0]);
    assert_eq!(r.add_bonus_total(), 2.0);
    assert!(r.active_deme(), "ADD always calls AddDemeBonus");

    let r = single("REACTION X not process:value=3:type=mult", 1.0, 0, &[0.0]);
    assert_eq!(r.mult_bonus_total(), 3.0);
    assert!(!r.active_deme(), "MULT without deme fraction");

    let r = single("REACTION X not process:value=2:type=pow", 0.5, 0, &[0.0]);
    assert_eq!(r.mult_bonus_total(), 2.0);
    assert_eq!(r.mult_deme_bonus_total(), 1.0);

    let r = single("REACTION X not process:value=2:type=lin", 1.0, 3, &[0.0]);
    assert_eq!(r.add_bonus_total(), 6.0);

    let r = single("REACTION X not process:value=5:type=energy", 1.0, 0, &[0.0]);
    assert_eq!(r.add_energy_total(), 5.0);
    assert_eq!(r.add_bonus_total(), 0.0);

    // enzyme: consumes min(3 * frac 1, max 1) = 1; reward 2 * 3 / (3 + 1).
    let r = single(
        "REACTION X not process:resource=r:value=2:type=enzyme:ksubm=1",
        1.0,
        0,
        &[3.0],
    );
    assert_eq!(r.consumed(0), 1.0);
    assert_eq!(r.add_bonus_total(), 1.5);

    // exp: value * (1/decay) * (exp((n-1)*decay) - exp(n*decay)).
    let r = single(
        "REACTION X not process:value=3:type=exp:frac=0.5",
        1.0,
        2,
        &[0.0],
    );
    let expect = 3.0 * (1.0 / 0.5) * ((1.0f64 * 0.5).exp() - (2.0f64 * 0.5).exp());
    assert_eq!(r.add_bonus_total(), expect);

    // deme fraction: 4 * 0.25 to the deme, (1 - 0.25) * 4 to the organism.
    let r = single(
        "REACTION X not process:value=4:type=add:deme=0.25",
        1.0,
        0,
        &[0.0],
    );
    assert_eq!(r.add_deme_bonus_total(), 1.0);
    assert_eq!(r.add_bonus_total(), 3.0);
    let r = single(
        "REACTION X not process:value=4:type=mult:deme=0.5",
        1.0,
        0,
        &[0.0],
    );
    assert_eq!(r.mult_deme_bonus_total(), 2.0);
    assert_eq!(r.mult_bonus_total(), 2.0);

    // germline processes change germline propensity, not bonus.
    let r = single(
        "REACTION X not process:value=3:type=add:germline=1",
        1.0,
        0,
        &[0.0],
    );
    assert_eq!(r.add_germline_total(), 3.0);
    assert_eq!(r.add_bonus_total(), 0.0);
    let r = single(
        "REACTION X not process:value=3:type=mult:germline=1",
        1.0,
        0,
        &[0.0],
    );
    assert_eq!(r.mult_germline_total(), 3.0);
    let r = single(
        "REACTION X not process:value=2:type=pow:germline=1",
        1.0,
        0,
        &[0.0],
    );
    assert_eq!(r.mult_germline_total(), 4.0);
    assert_eq!(r.mult_bonus_total(), 1.0);
    // ENERGY has no germline form: nothing changes but the reaction counts.
    let r = single(
        "REACTION X not process:value=2:type=energy:germline=1",
        1.0,
        0,
        &[0.0],
    );
    assert_eq!(r.add_energy_total(), 0.0);
    assert!(r.reaction_triggered(0));
}

#[test]
fn products_detection_instructions_and_flags() {
    let config = Config::new();
    let mut env = env_from(
        &[
            "RESOURCE a b c",
            "REACTION X not process:resource=a:frac=1:max=10:product=b:conversion=2:\
             detect=c:threshold=0.5:detectionerror=0.1:inst=nop-A:random=1:sterilize=1:lethal=0.5",
        ],
        &config,
    );
    let mut org = Org::new(&[1.0]);
    let mut rng = AvidaRng::new(42);
    let mut shadow = AvidaRng::new(42);
    let mut reactions = vec![0];
    let res = [4.0, 0.0, 8.0];
    let result = run(
        &mut env,
        &mut org,
        &mut rng,
        &config,
        Run {
            task_count: &[0],
            reaction_count: &mut reactions,
            resources: &res,
            rbins: &[0.0; 3],
            is_parasite: false,
            test_mode: false,
        },
    );
    assert_eq!(result.consumed(0), 4.0);
    assert_eq!(result.produced(1), 8.0);
    // Detection draws GetRandNormal(8, 0.1 * 8), then the lethal draw.
    let est = shadow.get_rand_normal_params(8.0, 0.1 * 8.0);
    let detected = if est < 0.5 { 0.0 } else { est };
    // Detected amounts start at -1 once the result is activated.
    assert_eq!(result.detected(2), -1.0 + detected);
    let x = shadow.get_double();
    assert_eq!(result.lethal(), x < 0.5);
    assert_eq!(rng.get_uint(1000), shadow.get_uint(1000));
    assert!(result.sterilize());
    assert!(result.is_random_resource());
    assert_eq!(result.inst_array(), &["nop-A".to_string()]);

    // lethal=1 kills without a random draw.
    let mut env = env_from(&["REACTION X not process:lethal=1"], &config);
    let mut org = Org::new(&[1.0]);
    let mut rng = AvidaRng::new(42);
    let mut reactions = vec![0];
    let result = run(
        &mut env,
        &mut org,
        &mut rng,
        &config,
        Run {
            task_count: &[0],
            reaction_count: &mut reactions,
            resources: &[],
            rbins: &[],
            is_parasite: false,
            test_mode: false,
        },
    );
    assert!(result.lethal());
    assert_eq!(rng.get_uint(1000), AvidaRng::new(42).get_uint(1000));
}

#[test]
fn resource_bins() {
    let mut config = Config::new();
    config.use_resource_bins = true;
    config.use_stored_fraction = 0.5;
    config.env_fraction_threshold = 1.0;
    let mut env = env_from(
        &[
            "RESOURCE r",
            "REACTION X not process:resource=r:frac=0.1:max=100",
        ],
        &config,
    );
    let run_bins = |env: &mut Environment<SimpleTaskLib>, res: f64, bin: f64| {
        let mut org = Org::new(&[1.0]);
        let mut rng = AvidaRng::new(1);
        let mut reactions = vec![0];
        run(
            env,
            &mut org,
            &mut rng,
            &config,
            Run {
                task_count: &[0],
                reaction_count: &mut reactions,
                resources: &[res],
                rbins: &[bin],
                is_parasite: false,
                test_mode: false,
            },
        )
    };
    // Environment offers 10 * 0.1 = 1; the bin's usable 0.5 * 4 = 2 beats it.
    let r = run_bins(&mut env, 10.0, 4.0);
    assert_eq!(r.consumed(0), 0.0);
    assert_eq!(r.internal_consumed(0), 2.0);
    assert!(!r.used_env_resource());
    assert_eq!(r.add_bonus_total(), 2.0);
    // A bin worth less than the environment's offer is ignored.
    let r = run_bins(&mut env, 100.0, 4.0);
    assert_eq!(r.consumed(0), 10.0);
    assert_eq!(r.internal_consumed(0), 0.0);
    assert!(r.used_env_resource());

    // internal=1 always draws the whole bin (capped by max), and products
    // go to internal storage.
    let mut env = env_from(
        &[
            "RESOURCE r p",
            "REACTION X not process:resource=r:max=3:internal=1:product=p:conversion=1",
        ],
        &config,
    );
    let mut org = Org::new(&[1.0]);
    let mut rng = AvidaRng::new(1);
    let mut reactions = vec![0];
    let r = run(
        &mut env,
        &mut org,
        &mut rng,
        &config,
        Run {
            task_count: &[0],
            reaction_count: &mut reactions,
            resources: &[50.0, 0.0],
            rbins: &[4.0, 0.0],
            is_parasite: false,
            test_mode: false,
        },
    );
    assert_eq!(r.internal_consumed(0), 3.0);
    assert_eq!(r.internal_produced(1), 3.0);
    assert_eq!(r.produced(1), 0.0);
}

#[test]
fn requisites() {
    let mut config = Config::new();
    config.world_x = 10;
    config.world_y = 10;
    // The default (1) would skip all processing for parasites.
    config.parasite_skip_reactions = 0;
    let mut env = env_from(
        &[
            "REACTION A not process:value=1",
            "REACTION B nand process:value=1 requisite:reaction=A",
            "REACTION C and process:value=1 requisite:noreaction=A",
            "REACTION D orn process:value=1 requisite:divide_only=1",
            "REACTION E or process:value=1 requisite:parasite_only=1",
            "REACTION F andn process:value=1 requisite:cellbox=2,2,3,3",
            "REACTION G nor process:value=1 requisite:min_tot_count=2",
            // Alternatives: either requisite may be met.
            "REACTION H xor process:value=1 requisite:reaction=A requisite:reaction=B",
        ],
        &config,
    );
    let all = vec![1.0; 8];
    let eval = |env: &mut Environment<SimpleTaskLib>,
                org: &mut Org,
                reactions: &mut [i32],
                is_parasite: bool| {
        let mut rng = AvidaRng::new(1);
        run(
            env,
            org,
            &mut rng,
            &config,
            Run {
                task_count: &[0; 8],
                reaction_count: reactions,
                resources: &[],
                rbins: &[],
                is_parasite,
                test_mode: false,
            },
        )
    };
    let done = |r: &ReactionResult| (0..8).map(|i| r.reaction_triggered(i)).collect::<Vec<_>>();

    // Fresh organism in cell 0 (outside the box). Reaction counts update
    // during the pass, so A satisfies B and H (and blocks C), and A + B
    // make the total of 2 G needs.
    let mut org = Org::new(&all);
    let mut rc = vec![0; 8];
    let r = eval(&mut env, &mut org, &mut rc, false);
    assert_eq!(
        done(&r),
        vec![true, true, false, false, false, false, true, true]
    );
    assert_eq!(rc, vec![1, 1, 0, 0, 0, 0, 1, 1]);

    // A not performed now but stolen from another organism (enough for B
    // and H, not a reaction for C's noreaction), a parasite, in the box
    // (cell 33 = (3, 3)).
    let mut org = Org::new(&[0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
    org.cell_id = 33;
    org.stolen = vec![1, 0, 0, 0, 0, 0, 0, 0];
    let mut rc = vec![0; 8];
    let r = eval(&mut env, &mut org, &mut rc, true);
    assert_eq!(
        done(&r),
        vec![false, true, true, false, true, true, true, true]
    );

    // On divide only D (and nothing without requisites) is tested.
    let mut org = Org::new(&all);
    org.on_divide = true;
    let mut rc = vec![0; 8];
    let r = eval(&mut env, &mut org, &mut rc, false);
    assert_eq!(
        done(&r),
        vec![false, false, false, true, false, false, false, false]
    );

    // QUIRK: without an organism (deme evaluation) the cell-box check fails
    // every requisite; only reactions without requisites run.
    let mut org = Org::new(&all);
    org.has_organism = false;
    let mut rc = vec![1, 1, 1, 1, 1, 1, 1, 1];
    let r = eval(&mut env, &mut org, &mut rc, false);
    assert_eq!(
        done(&r),
        vec![true, false, false, false, false, false, false, false]
    );
}

#[test]
fn parasite_skip_reactions() {
    let mut config = Config::new();
    config.parasite_skip_reactions = 1;
    let mut env = env_from(
        &["REACTION A not process:value=1 requisite:max_count=1"],
        &config,
    );
    let mut org = Org::new(&[1.0]);
    let mut rng = AvidaRng::new(1);
    let mut rc = vec![0];
    // Requisites fail (task count 5), but parasites skipping reactions are
    // still tested and marked; no processes run.
    let r = run(
        &mut env,
        &mut org,
        &mut rng,
        &config,
        Run {
            task_count: &[5],
            reaction_count: &mut rc,
            resources: &[],
            rbins: &[],
            is_parasite: true,
            test_mode: false,
        },
    );
    assert!(r.task_done(0));
    assert!(!r.reaction_triggered(0));
    assert_eq!(rc, vec![0]);
}

#[test]
fn phenotypic_plasticity_bonus_methods() {
    let config = Config::new();
    let eval = |line: &str, quality: f64, prob: f64, test_mode: bool| {
        let mut env = env_from(&[line], &config);
        let mut org = Org::new(&[quality]);
        org.task_prob = prob;
        let mut rng = AvidaRng::new(1);
        let mut rc = vec![0];
        let r = run(
            &mut env,
            &mut org,
            &mut rng,
            &config,
            Run {
                task_count: &[0],
                reaction_count: &mut rc,
                resources: &[],
                rbins: &[],
                is_parasite: false,
                test_mode,
            },
        );
        (r, org.log)
    };
    // fullbonus: undemonstrated task forced, quality 1.
    let (r, log) = eval(
        "REACTION A not process:value=3:phenplastbonus=fullbonus",
        0.0,
        0.25,
        false,
    );
    assert!(r.task_done(0));
    assert_eq!(r.add_bonus_total(), 3.0);
    assert_eq!(log.last().unwrap(), "prob 0");
    // fracbonus: scaled by the task probability.
    let (r, _) = eval(
        "REACTION A not process:value=4:phenplastbonus=fracbonus",
        0.0,
        0.25,
        false,
    );
    assert_eq!(r.add_bonus_total(), 1.0);
    // Probability 0: nothing to force.
    let (r, _) = eval(
        "REACTION A not process:value=4:phenplastbonus=fracbonus",
        0.0,
        0.0,
        false,
    );
    assert!(!r.active());
    // nobonus: zero unless the task is certain, but the reaction counts.
    let (r, _) = eval(
        "REACTION A not process:value=4:phenplastbonus=nobonus",
        1.0,
        0.5,
        false,
    );
    assert_eq!(r.add_bonus_total(), 0.0);
    assert!(r.reaction_triggered(0));
    let (r, _) = eval(
        "REACTION A not process:value=4:phenplastbonus=nobonus",
        1.0,
        1.0,
        false,
    );
    assert_eq!(r.add_bonus_total(), 4.0);
    // Test mode never asks for the probability.
    let (r, log) = eval(
        "REACTION A not process:value=4:phenplastbonus=fullbonus",
        0.0,
        1.0,
        true,
    );
    assert!(!r.active());
    assert!(!log.iter().any(|l| l.starts_with("prob")));
}

#[test]
fn hgt_metabolism() {
    let mut config = Config::new();
    config.enable_hgt = 1;
    let mut env = env_from(
        &[
            "RESOURCE frag:hgt=1:inflowx=-99",
            "REACTION A not process:resource=frag:value=0.5",
        ],
        &config,
    );
    assert!(env.resource_lib().resource(0).hgt_metabolize);
    let mut org = Org::new(&[0.5]);
    org.fragments = vec![40];
    let (r, rc) = run_simple(&mut env, &mut org, &config, &[0.0]);
    // Consumed is the fragment length (times quality for the bonus).
    assert_eq!(r.consumed(0), 40.0);
    assert_eq!(r.add_bonus_total(), 0.5 * 40.0 * 0.5);
    assert_eq!(rc, vec![1]);
    // No fragment: the reaction is not performed.
    let mut org = Org::new(&[1.0]);
    let (r, rc) = run_simple(&mut env, &mut org, &config, &[0.0]);
    assert!(r.task_done(0) && !r.reaction_triggered(0));
    assert_eq!(rc, vec![0]);
}

#[test]
fn task_may_redirect_process_resource() {
    let config = Config::new();
    let mut env = env_from(
        &["RESOURCE r1 r2", "REACTION X xor process:resource=r1:max=5"],
        &config,
    );
    let mut org = Org::new(&[1.0]);
    org.resource_change = Some((0, "r2".to_string()));
    let (r, _) = run_simple(&mut env, &mut org, &config, &[100.0, 100.0]);
    assert_eq!(
        env.reaction_lib().reaction(0).processes[0].resource,
        Some(1)
    );
    assert_eq!(r.consumed(0), 0.0);
    assert_eq!(r.consumed(1), 5.0);
}

#[test]
fn context_requisites() {
    let config = Config::new();
    let mut env = env_from(
        &[
            "REACTION A not process:value=1",
            "REACTION B nand process:value=1 context_requisite:reaction=A",
        ],
        &config,
    );
    let eval = |env: &mut Environment<SimpleTaskLib>, cp: &mut ContextPhenotype| {
        let mut org = Org::new(&[1.0, 1.0]);
        let mut rng = AvidaRng::new(1);
        let mut result = ReactionResult::new(0, 2, 2);
        let mut rc = vec![0, 0];
        env.test_output(
            &mut rng,
            false,
            &config,
            &mut result,
            &mut org,
            &[0, 0],
            &mut rc,
            &[],
            &[],
            false,
            Some(cp),
        );
        result
    };
    let mut cp = ContextPhenotype::new();
    let r = eval(&mut env, &mut cp);
    assert!(r.reaction_triggered(0));
    assert!(!r.reaction_triggered(1));
    assert_eq!(cp.number_tasks, 2);
    assert_eq!(cp.reaction_counts(), &[0, 0]);

    // Context counts, not the organism's, satisfy the context requisite.
    cp.cur_reaction_count = vec![1, 0];
    let r = eval(&mut env, &mut cp);
    assert!(r.reaction_triggered(1));
}

// -- SetupInputs ------------------------------------------------------------

/// A registry that requests extra (and optionally truly random) inputs, as
/// tasks like `Load_*` do through `cEnvReqs`.
struct InputTasks {
    lib: SimpleTaskLib,
    min_inputs: i32,
    true_rand: bool,
}

impl TaskRegistry for InputTasks {
    fn add_task(
        &mut self,
        name: &str,
        info: &str,
        envreqs: &mut EnvReqs,
        feedback: &mut Feedback,
        env: &mut TaskEnvHooks<'_>,
    ) -> Option<i32> {
        envreqs.set_min_inputs(self.min_inputs);
        envreqs.set_true_rand_inputs(self.true_rand);
        self.lib.add_task(name, info, envreqs, feedback, env)
    }
    fn num_tasks(&self) -> i32 {
        self.lib.num_tasks()
    }
    fn task_name(&self, id: i32) -> &str {
        self.lib.task_name(id)
    }
    fn task_desc(&self, id: i32) -> &str {
        self.lib.task_desc(id)
    }
}

fn input_env(min_inputs: i32, true_rand: bool) -> Environment<InputTasks> {
    let config = Config::new();
    let mut env = Environment::new(
        &config,
        InputTasks {
            lib: SimpleTaskLib::new(),
            min_inputs,
            true_rand,
        },
    );
    env.load_line(
        "REACTION A not process:value=1",
        &mut Feedback::new(),
        &config,
    )
    .unwrap();
    env
}

#[test]
fn setup_inputs_fixed_patterns() {
    let env = input_env(7, false);
    assert_eq!(env.input_size(), 7);
    let mut rng = AvidaRng::new(100);
    let mut inputs = Vec::new();
    env.setup_inputs(&mut rng, &mut inputs, false);
    let base = [0x0f13149fu32 as i32, 0x3308e53e, 0x556241eb];
    let expect: Vec<i32> = (0..7)
        .map(|i| base[i % 3].wrapping_shl((i / 3) as u32))
        .collect();
    assert_eq!(inputs, expect);
    // 0x556241eb << 1 overflows into the sign bit, as the C++ int shift does.
    assert_eq!(inputs[5], 0xaac483d6u32 as i32);
    assert_eq!(inputs[6], 0x3c4c527c);
    // No random numbers are drawn.
    assert_eq!(rng.get_uint(1 << 20), AvidaRng::new(100).get_uint(1 << 20));
}

#[test]
fn setup_inputs_random_bit_patterns() {
    let env = input_env(5, false);
    for seed in [1, 7, 12345] {
        let mut rng = AvidaRng::new(seed);
        let mut shadow = AvidaRng::new(seed);
        let mut inputs = vec![9; 2];
        env.setup_inputs(&mut rng, &mut inputs, true);
        assert_eq!(inputs.len(), 5);
        // Top bytes 00001111, 00110011, 01010101 over 24 random bits.
        assert_eq!(inputs[0] >> 24, 0x0F);
        assert_eq!(inputs[1] >> 24, 0x33);
        assert_eq!(inputs[2] >> 24, 0x55);
        for (i, top) in [15u32, 51, 85].iter().enumerate() {
            assert_eq!(inputs[i], ((top << 24) + shadow.get_uint(1 << 24)) as i32);
        }
        for v in &inputs[3..] {
            assert_eq!(*v >> 24, 0);
            assert_eq!(*v, shadow.get_uint(1 << 24) as i32);
        }
    }
}

#[test]
fn setup_inputs_true_random_and_specific() {
    let env = input_env(4, true);
    assert!(env.true_rand());
    let mut rng = AvidaRng::new(5);
    let mut shadow = AvidaRng::new(5);
    let mut inputs = Vec::new();
    env.setup_inputs(&mut rng, &mut inputs, true);
    let expect: Vec<i32> = (0..4).map(|_| shadow.get_uint(1u32 << 31) as i32).collect();
    assert_eq!(inputs, expect);
    // Not random: the fixed patterns even with true-random tasks.
    env.setup_inputs(&mut rng, &mut inputs, false);
    assert_eq!(inputs[0], 0x0f13149f);

    // Specific inputs win; the mask re-randomises its bits.
    let mut env = input_env(3, false);
    env.set_specific_inputs(vec![0x7f00_00ff, 0x1234_5678, -1]);
    let mut rng = AvidaRng::new(9);
    env.setup_inputs(&mut rng, &mut inputs, true);
    assert_eq!(inputs, vec![0x7f00_00ff, 0x1234_5678, -1]);
    env.set_specific_random_mask(0x0000_ff00);
    let mut rng = AvidaRng::new(9);
    let mut shadow = AvidaRng::new(9);
    env.setup_inputs(&mut rng, &mut inputs, false);
    let spec = [0x7f00_00ffu32, 0x1234_5678, 0xffff_ffff];
    for i in 0..3 {
        let r = shadow.get_uint(1 << 24);
        assert_eq!(inputs[i] as u32, (spec[i] & !0xff00) | (0xff00 & r));
    }
}

#[test]
fn swap_inputs() {
    let env = input_env(3, false);
    let mut a = vec![1, 2, 3];
    let mut b = vec![4];
    env.swap_inputs(&mut a, &mut b);
    assert_eq!(a, vec![4]);
    assert_eq!(b, vec![1, 2, 3]);
}
