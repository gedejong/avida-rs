//! The adder ladder (`environment-adder-ladder.yaml`, task `field_arith`):
//! the hand-written organisms `ladder-*.org` for the stock heads set and the
//! modular set earn full quality on the field rungs, with no `add` or `sub`
//! in them. The wider adders (ADD3, ADD4, `environment-adder-wide.yaml`) and
//! the self-replicating ADD2 organisms of `data/wide-adder` are covered too.

mod common;

use avida_genome::{Genome, InstructionSequence};
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu};
use avida_tasks::{Feedback, FieldSpec, IoBuffer};
use common::*;
use std::path::{Path, PathBuf};

fn config_dir() -> PathBuf {
    avida_core().join("support/config")
}

const LADDER: &str = "environment-adder-ladder.yaml";
const WIDE: &str = "environment-adder-wide.yaml";

/// The stock configuration with the ladder as its environment.
fn ladder_dir(tag: &str) -> PathBuf {
    env_dir(tag, LADDER)
}

/// The stock configuration with `env_file` as its environment.
fn env_dir(tag: &str, env_file: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("ladder_hw_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for e in std::fs::read_dir(config_dir()).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_file() {
            std::fs::copy(e.path(), dir.join(e.file_name())).unwrap();
        }
    }
    // The stock file's `environment:` block becomes the name of `env_file`.
    let cfg = std::fs::read_to_string(dir.join("avida.yaml")).unwrap();
    let (head, rest) = cfg.split_once("\nenvironment:\n").unwrap();
    let (_, tail) = rest.split_once("\nevents:").unwrap();
    let cfg = format!("{head}\nenvironment: {env_file}\n\nevents:{tail}");
    std::fs::write(dir.join("avida.yaml"), cfg).unwrap();
    dir
}

fn instset_lines(file: &str) -> Vec<String> {
    section_inst_sets(&config_dir(), file)
}

/// The `INSTSET`/`INST` lines of an instruction-set file in `dir`.
fn section_inst_sets(dir: &Path, file: &str) -> Vec<String> {
    let text = avida_config::convert::section_lines(dir, file, "instruction_sets");
    text.unwrap().lines().map(str::to_string).collect()
}

fn fixture(modular: bool, tag: &str) -> Fixture {
    fixture_in(modular, tag, LADDER)
}

fn fixture_in(modular: bool, tag: &str, env_file: &str) -> Fixture {
    let dir = env_dir(tag, env_file);
    if modular {
        Fixture::load_with_instset(&dir, instset_lines("instset-heads-modular.yaml")).unwrap()
    } else {
        Fixture::load(&dir, &[]).unwrap()
    }
}

/// The instruction names of an `.org` file of the stock configuration.
fn org_names(file: &str) -> Vec<String> {
    names_at(&config_dir().join(file))
}

fn names_at(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .map(|l| l.split('#').next().unwrap().trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn genome(fx: &Fixture, names: &[String]) -> Genome {
    let is = fx.hwm.registry().default_inst_set().unwrap();
    let mut seq = InstructionSequence::new();
    seq.resize(names.len() as i32);
    for (i, n) in names.iter().enumerate() {
        seq[i as i32] = is.get_inst(n).unwrap_or_else(|_| panic!("instruction {n}"));
    }
    Genome::with_inst_set(is.hardware_type(), is.inst_set_name(), seq)
}

struct Run {
    input: IoBuffer,
    output: IoBuffer,
    task_counts: Vec<i32>,
    task_quality: Vec<f64>,
}

fn run(fx: &mut Fixture, g: &Genome, inputs: [u32; 3]) -> Run {
    let mut info = CpuTestInfo::default();
    info.use_manual_inputs(inputs.iter().map(|&v| v as i32).collect());
    info.set_resource_options(0, None, -1, 0);
    let mut world = fx.world();
    let mut cpu = TestCpu::new(&mut world).unwrap();
    cpu.test_genome(&mut world, &mut info, g).unwrap();
    let org = info.test_organism(0).expect("the organism ran");
    Run {
        input: org.state.input_buf.clone(),
        output: org.state.output_buf.clone(),
        task_counts: org.state.phenotype.cur_task_count.clone(),
        task_quality: org.state.phenotype.cur_task_quality.clone(),
    }
}

/// Inputs as the environment makes them: a fixed top byte, random below.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
    fn inputs(&mut self) -> [u32; 3] {
        [
            (0x0f << 24) | (self.next() & 0xff_ffff),
            (0x33 << 24) | (self.next() & 0xff_ffff),
            (0x55 << 24) | (self.next() & 0xff_ffff),
        ]
    }
}

/// (file, modular, field_arith arguments, task id in the environment file,
/// genome length, genome length without the no-op modifiers, environment file)
const SOLUTIONS: &[(&str, bool, &str, usize, usize, usize, &str)] = &[
    (
        "ladder-halfadd-planes-heads.org",
        false,
        "fn=halfadd,mode=planes",
        4,
        32,
        23,
        LADDER,
    ),
    (
        "ladder-halfadd-heads.org",
        false,
        "fn=halfadd,mode=packed",
        5,
        83,
        57,
        LADDER,
    ),
    (
        "ladder-add2-planes-heads.org",
        false,
        "fn=add,bits=2,mode=planes",
        6,
        90,
        66,
        LADDER,
    ),
    (
        "ladder-add2-heads.org",
        false,
        "fn=add,bits=2,mode=packed",
        7,
        113,
        77,
        LADDER,
    ),
    (
        "ladder-halfadd-planes-modular.org",
        true,
        "fn=halfadd,mode=planes",
        4,
        49,
        30,
        LADDER,
    ),
    (
        "ladder-halfadd-modular.org",
        true,
        "fn=halfadd,mode=packed",
        5,
        82,
        50,
        LADDER,
    ),
    (
        "ladder-add2-planes-modular.org",
        true,
        "fn=add,bits=2,mode=planes",
        6,
        78,
        49,
        LADDER,
    ),
    (
        "ladder-add2-modular.org",
        true,
        "fn=add,bits=2,mode=packed",
        7,
        89,
        53,
        LADDER,
    ),
    // The wider adders, with the default stride (bits + 1) and no partial credit.
    (
        "ladder-add2-heads.org",
        false,
        "fn=add,bits=2,mode=packed,thresh=1",
        0,
        113,
        77,
        WIDE,
    ),
    (
        "ladder-add2-modular.org",
        true,
        "fn=add,bits=2,mode=packed,thresh=1",
        0,
        89,
        53,
        WIDE,
    ),
    (
        "ladder-add3-heads.org",
        false,
        "fn=add,bits=3,mode=packed,thresh=1",
        1,
        143,
        97,
        WIDE,
    ),
    (
        "ladder-add3-modular.org",
        true,
        "fn=add,bits=3,mode=packed,thresh=1",
        1,
        96,
        56,
        WIDE,
    ),
    (
        "ladder-add4-heads.org",
        false,
        "fn=add,bits=4,mode=packed,thresh=1",
        2,
        173,
        117,
        WIDE,
    ),
    (
        "ladder-add4-modular.org",
        true,
        "fn=add,bits=4,mode=packed,thresh=1",
        2,
        103,
        59,
        WIDE,
    ),
];

fn spec(args: &str) -> FieldSpec {
    let mut fb = Feedback::new();
    FieldSpec::load(args, &mut fb).expect("arguments").0
}

#[test]
fn the_solutions_earn_full_quality_on_random_inputs_in_both_sets() {
    let mut stock = fixture(false, "all_stock");
    let mut modular = fixture(true, "all_modular");
    let mut stock_wide = fixture_in(false, "all_stock_wide", WIDE);
    let mut modular_wide = fixture_in(true, "all_modular_wide", WIDE);
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    for &(file, is_modular, args, task, len, plain, env_file) in SOLUTIONS {
        let names = org_names(file);
        assert_eq!(names.len(), len, "{file}: genome length");
        let non_nop = names.iter().filter(|n| !n.starts_with("nop-")).count();
        assert_eq!(non_nop, plain, "{file}: instructions without modifiers");
        assert!(
            !names.iter().any(|n| n == "add" || n == "sub"),
            "{file} uses add or sub"
        );
        let fx = match (is_modular, env_file == WIDE) {
            (true, false) => &mut modular,
            (false, false) => &mut stock,
            (true, true) => &mut modular_wide,
            (false, true) => &mut stock_wide,
        };
        let g = genome(fx, &names);
        let spec = spec(args);
        let mut lowest: f64 = 1.0;
        for round in 0..30 {
            let inputs = rng.inputs();
            let r = run(fx, &g, inputs);
            // The output test of the last IO ran before that IO read its
            // own input: take that read off.
            let mut input = r.input.clone();
            input.pop();
            assert_eq!(
                spec.quality(&input, &r.output),
                1.0,
                "{file} round {round} inputs {inputs:x?}"
            );
            // The ladder's own task fires too. With `requisite:max_count=1`
            // the reaction is paid at the FIRST output that reaches the
            // threshold: a planes organism's first plane is already judged
            // against the older output as its second plane, so the quality
            // recorded may be below 1.
            assert!(
                r.task_counts[task] >= 1,
                "{file}: ladder task {task} not done"
            );
            let q = r.task_quality[task];
            assert!(
                (0.75..=1.0).contains(&q),
                "{file}: quality {q} of task {task}"
            );
            lowest = lowest.min(q);
        }
        eprintln!("{file}: ladder quality at the first firing, lowest of 30: {lowest}");
    }
}

#[test]
fn a_solution_for_a_lower_rung_does_not_earn_the_rung_above() {
    // The half adder does not do two-bit fields: the sum bit is right, the
    // carries are not.
    let mut fx = fixture(false, "lower");
    let g = genome(&fx, &org_names("ladder-halfadd-heads.org"));
    let mut rng = Rng(77);
    let add2 = spec("fn=add,bits=2");
    for _ in 0..10 {
        let r = run(&mut fx, &g, rng.inputs());
        let mut input = r.input.clone();
        input.pop();
        assert!(add2.quality(&input, &r.output) < 1.0);
    }
}

#[test]
fn the_ladder_passes_the_parallel_mode_gate_and_sizes_the_output_buffer() {
    // Every task of the ladder is organism-local and every resource a plain
    // per-cell grid: the output tests run in the bursts.
    let fx = fixture(false, "gate");
    assert!(avida_hardware::local_output::output_is_local(
        &fx.cfg, &fx.env, true
    ));
    // ADD2_PLANES asks for three outputs; nothing else asks for more than
    // the default of one.
    assert_eq!(fx.env.output_size(), 3);
    assert_eq!(fx.env.input_size(), 3);
}

#[test]
fn quality_scales_what_a_reaction_consumes_and_so_the_bonus() {
    // Infinite resource: a process consumes `max` (1 here) times the task
    // quality; the bonus is that times `value` (2 here), added for `add`,
    // multiplied in for `mult`, and the exponent of 2 for `pow`.
    for (kind, bonus_of) in [
        ("add", (|q: f64| 1.0 + 2.0 * q) as fn(f64) -> f64),
        ("mult", |q| 2.0 * q),
        ("pow", |q| 2f64.powf(2.0 * q)),
    ] {
        let dir = ladder_dir(&format!("q_{kind}"));
        avida_config::convert::replace_section(
            &dir,
            "avida.yaml",
            "environment",
            &format!(
                "REACTION R field_arith:fn=halfadd:bits=1:mode=planes:thresh=0.1 \
                 process:value=2.0:type={kind} requisite:max_count=1\n"
            ),
        )
        .unwrap();
        let mut fx = Fixture::load(&dir, &[]).unwrap();
        let g = genome(&fx, &org_names("ladder-halfadd-planes-heads.org"));
        let r = run(&mut fx, &g, [0x0f13_149f, 0x3308_e53e, 0x5562_41eb]);
        let q = r.task_quality[0];
        assert!(q > 0.1 && q < 1.0, "a partial first output: {q}");
        let mut info = CpuTestInfo::default();
        info.use_manual_inputs(vec![0x0f13_149f, 0x3308_e53e, 0x5562_41eb]);
        let mut world = fx.world();
        let mut cpu = TestCpu::new(&mut world).unwrap();
        cpu.test_genome(&mut world, &mut info, &g).unwrap();
        let bonus = info.test_phenotype(0).unwrap().cur_bonus;
        assert!(
            (bonus - bonus_of(q)).abs() < 1e-12,
            "{kind}: bonus {bonus} for quality {q}, wanted {}",
            bonus_of(q)
        );
    }
}

/// Packed quality of `file`'s organism against `args`, over `n` random
/// input triples; the highest of them.
fn best_quality(modular: bool, tag: &str, file: &str, args: &str, n: usize) -> f64 {
    let mut fx = fixture_in(modular, tag, WIDE);
    let g = genome(&fx, &org_names(file));
    let spec = spec(args);
    let mut rng = Rng(0x0bad_5eed_0000_0042);
    let mut best: f64 = 0.0;
    for _ in 0..n {
        let r = run(&mut fx, &g, rng.inputs());
        let mut input = r.input.clone();
        input.pop();
        best = best.max(spec.quality(&input, &r.output));
    }
    best
}

#[test]
fn a_narrower_adder_does_not_earn_the_wider_rung() {
    // The two-bit adder masks and adds in the wrong fields for three-bit
    // operands, and the three-bit one for four-bit operands: no input gets
    // full quality (it is below 1 on every one of them).
    for (modular, set) in [(false, "stock"), (true, "modular")] {
        let kind = if modular { "modular" } else { "heads" };
        let q = best_quality(
            modular,
            &format!("narrow2_{set}"),
            &format!("ladder-add2-{kind}.org"),
            "fn=add,bits=3,mode=packed",
            20,
        );
        assert!(q < 1.0, "{set}: ADD2 organism on bits=3: {q}");
        let q = best_quality(
            modular,
            &format!("narrow3_{set}"),
            &format!("ladder-add3-{kind}.org"),
            "fn=add,bits=4,mode=packed",
            20,
        );
        assert!(q < 1.0, "{set}: ADD3 organism on bits=4: {q}");
    }
}

#[test]
fn the_wide_environment_has_three_packed_reactions_on_their_own_resources() {
    let fx = fixture_in(false, "wide_env", WIDE);
    assert_eq!(fx.env.output_size(), 1, "packed tasks need one output");
    // The environment lines the experiment file renders from the section.
    let text = fx.cfg.experiment.environment.join("\n");
    for (n, value) in [(2, 7), (3, 8), (4, 9)] {
        let line = text
            .lines()
            .find(|l| l.starts_with(&format!("REACTION ADD{n} ")))
            .unwrap_or_else(|| panic!("no ADD{n} in\n{text}"));
        assert!(line.contains(&format!(
            "field_arith:fn=add:bits={n}:mode=packed:thresh=1 "
        )));
        assert!(line.contains(&format!("resource=resADD{n}:value={value}.0:type=pow")));
        assert!(line.contains("requisite:max_count=1"));
    }
    assert_eq!(
        text.lines().filter(|l| l.starts_with("RESOURCE ")).count(),
        3
    );
    assert_eq!(
        text.lines().filter(|l| l.starts_with("REACTION ")).count(),
        3
    );
}

/// The self-replicating ADD2 organisms in `tests/data/wide-adder`, with the
/// instruction sets they need (no `add`, no `sub`).
fn wide_adder_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/wide-adder")
}

fn wide_adder_instset(file: &str) -> Vec<String> {
    section_inst_sets(&wide_adder_dir(), file)
}

#[test]
fn the_wide_adder_replicators_replicate_and_do_add2() {
    for (file, set_file, expected_len) in [
        ("replicator-add2-heads.org", "instset-heads.yaml", 125usize),
        (
            "replicator-add2-modular.org",
            "instset-heads-modular.yaml",
            102,
        ),
    ] {
        let lines = wide_adder_instset(set_file);
        assert!(
            !lines
                .iter()
                .any(|l| l.starts_with("INST add") || l.starts_with("INST sub")),
            "{set_file} has add or sub"
        );
        let dir = env_dir(&format!("rep_{file}"), WIDE);
        let mut fx = Fixture::load_with_instset(&dir, lines).unwrap();
        let names = names_at(&wide_adder_dir().join(file));
        assert_eq!(names.len(), expected_len, "{file}: genome length");
        assert!(!names.iter().any(|n| n == "add" || n == "sub"));
        let g = genome(&fx, &names);
        let mut rng = Rng(0xfeed_0000_0000_0007);
        for round in 0..5 {
            let inputs = rng.inputs();
            let mut info = CpuTestInfo::default();
            info.use_manual_inputs(inputs.iter().map(|&v| v as i32).collect());
            info.set_resource_options(0, None, -1, 0);
            let mut world = fx.world();
            let mut cpu = TestCpu::new(&mut world).unwrap();
            cpu.test_genome(&mut world, &mut info, &g).unwrap();
            assert!(info.is_viable(), "{file} round {round}: viable");
            let org = info.test_organism(0).unwrap();
            assert_eq!(
                org.state.offspring_genome.representation().as_string(),
                g.representation().as_string(),
                "{file} round {round}: the offspring is identical"
            );
            let p = info.test_phenotype(0).unwrap();
            // ADD2 (task 0 of the wide environment) is done, ADD3 and ADD4 are not.
            assert!(p.last_task_count[0] >= 1, "{file} round {round}: ADD2");
            assert_eq!(p.last_task_count[1], 0, "{file}: ADD3");
            assert_eq!(p.last_task_count[2], 0, "{file}: ADD4");
        }
    }
}
