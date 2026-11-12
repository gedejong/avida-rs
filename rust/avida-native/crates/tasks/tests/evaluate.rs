//! End-to-end evaluation through `TaskLib`: registration, `setup_tests`,
//! `test_output`, with a mock `TaskWorld` for the organism/world tasks.

use avida_tasks::{EnvReqs, Feedback, IoBuffer, TaskContext, TaskLib, TaskStates, TaskWorld};
use std::collections::HashMap;

/// Buffer whose `get(i)` is `vals[i]`.
fn from_get(cap: usize, vals: &[i32]) -> IoBuffer {
    let mut b = IoBuffer::new(cap);
    for &v in vals.iter().rev() {
        b.add(v);
    }
    b
}

fn add(lib: &mut TaskLib, name: &str, args: &str) -> usize {
    let mut er = EnvReqs::new();
    let mut fb = Feedback::new();
    lib.add_task(name, args, &mut er, &mut fb)
        .unwrap_or_else(|| panic!("{} {:?}", name, fb.messages))
}

fn eval(lib: &TaskLib, id: usize, input: &IoBuffer, output: &IoBuffer) -> f64 {
    let mut ctx = TaskContext::simple(input, output);
    lib.setup_tests(&mut ctx);
    ctx.set_task_entry(id);
    lib.test_output_pure(&mut ctx).expect("pure task")
}

// ---- standard logic tasks, including the logic-id path ---------------------

const A: i32 = 0x5555_5555;
const B: i32 = 0x3333_3333;
const C: i32 = 0x0f0f_0f0f;

#[test]
fn nine_logic_tasks_fire_exactly_for_their_function() {
    let mut lib = TaskLib::new();
    let names = [
        "not", "nand", "and", "orn", "or", "andn", "nor", "xor", "equ",
    ];
    let ids: Vec<usize> = names.iter().map(|n| add(&mut lib, n, "")).collect();
    // inputs[0] = A, inputs[1] = B, inputs[2] = C
    let input = from_get(3, &[A, B, C]);
    let outputs = [
        (!A, "not"),
        (!(A & B), "nand"),
        (A & B, "and"),
        (A | !B, "orn"),
        (A | B, "or"),
        (A & !B, "andn"),
        (!(A | B), "nor"),
        (A ^ B, "xor"),
        (!(A ^ B), "equ"),
    ];
    for (out_val, want) in outputs {
        let output = from_get(3, &[out_val]);
        for (n, &id) in names.iter().zip(&ids) {
            let r = eval(&lib, id, &input, &output);
            assert_eq!(
                r,
                if *n == want { 1.0 } else { 0.0 },
                "output for {} on task {}",
                want,
                n
            );
        }
    }
    // the same functions of (B, C) and (A, C) are recognised via the other
    // logic ids of each task
    let output = from_get(3, &[B ^ C]);
    assert_eq!(eval(&lib, ids[7], &input, &output), 1.0); // xor of inputs[1], inputs[2]
    let output = from_get(3, &[!(A ^ C)]);
    assert_eq!(eval(&lib, ids[8], &input, &output), 1.0);
    // an output that is no boolean function of the inputs rewards nothing
    let output = from_get(3, &[0x1234_5678]);
    for &id in &ids {
        assert_eq!(eval(&lib, id, &input, &output), 0.0);
    }
}

#[test]
fn logic_id_is_stored_on_the_context() {
    let lib = TaskLib::new();
    let input = from_get(3, &[A, B, C]);
    let output = from_get(3, &[A & B & C]);
    let mut ctx = TaskContext::simple(&input, &output);
    lib.setup_tests(&mut ctx);
    assert_eq!(ctx.get_logic_id(), 128);
    let output = from_get(3, &[0x1234_5678]);
    let mut ctx = TaskContext::simple(&input, &output);
    lib.setup_tests(&mut ctx);
    assert_eq!(ctx.get_logic_id(), -1);
}

#[test]
fn logic3_tasks_use_the_logic_id() {
    let mut lib = TaskLib::new();
    let all3 = add(&mut lib, "logic_3AH", ""); // A+B+C == 3 -> id 128
    let any = add(&mut lib, "logic_3AN", ""); // != 0 -> id 254
    let input = from_get(3, &[A, B, C]);
    let and3 = from_get(3, &[A & B & C]);
    assert_eq!(eval(&lib, all3, &input, &and3), 1.0);
    assert_eq!(eval(&lib, any, &input, &and3), 0.0);
    let or3 = from_get(3, &[A | B | C]);
    assert_eq!(eval(&lib, any, &input, &or3), 1.0);
    // logic_3AO accepts three ids of the same function class (2, 4, 16)
    let ao = add(&mut lib, "logic_3AO", "");
    let out = from_get(3, &[A & !B & !C]);
    assert_eq!(eval(&lib, ao, &input, &out), 1.0);
}

#[test]
fn echo_and_arithmetic_tasks() {
    let mut lib = TaskLib::new();
    let echo = add(&mut lib, "echo", "");
    let sum = add(&mut lib, "add", "");
    let sub = add(&mut lib, "sub", "");
    let dont = add(&mut lib, "dontcare", "");
    let input = from_get(3, &[7, 5, 3]);
    assert_eq!(eval(&lib, echo, &input, &from_get(3, &[5])), 1.0);
    assert_eq!(eval(&lib, echo, &input, &from_get(3, &[6])), 0.0);
    assert_eq!(eval(&lib, sum, &input, &from_get(3, &[12])), 1.0);
    assert_eq!(eval(&lib, sub, &input, &from_get(3, &[2])), 1.0);
    assert_eq!(eval(&lib, dont, &input, &from_get(3, &[99])), 1.0);
}

#[test]
fn math_and_fib_samples_by_name() {
    let mut lib = TaskLib::new();
    let input = from_get(3, &[16, 5, 3]);
    let t = |lib: &mut TaskLib, n: &str| add(lib, n, "");
    let m1 = t(&mut lib, "math_1AF"); // sqrt
    let m2 = t(&mut lib, "math_2AN"); // X+Y
    let m3 = t(&mut lib, "math_3AH"); // X+Y+Z
    let f = t(&mut lib, "fib_6"); // 5
    assert_eq!(eval(&lib, m1, &input, &from_get(3, &[4])), 1.0);
    assert_eq!(eval(&lib, m2, &input, &from_get(3, &[21])), 1.0);
    assert_eq!(eval(&lib, m3, &input, &from_get(3, &[24])), 1.0);
    assert_eq!(eval(&lib, m3, &input, &from_get(3, &[25])), 0.0);
    assert_eq!(eval(&lib, f, &input, &from_get(3, &[5])), 1.0);
    // QUIRK: fib tasks need an input present
    assert_eq!(eval(&lib, f, &IoBuffer::new(3), &from_get(3, &[5])), 0.0);
}

#[test]
fn argument_tasks_from_real_config_strings() {
    let mut lib = TaskLib::new();
    let input = from_get(3, &[6, 7, 0]);
    let mult = add(&mut lib, "mult", "threshold=3,halflife=2");
    let div = add(&mut lib, "div", "halflife=2");
    let num = add(
        &mut lib,
        "match_number",
        "target=100,halflife=4,threshold=-1",
    );
    assert_eq!(eval(&lib, mult, &input, &from_get(3, &[42])), 1.0);
    assert_eq!(eval(&lib, mult, &input, &from_get(3, &[44])), 0.5);
    assert_eq!(eval(&lib, mult, &input, &from_get(3, &[48])), 0.0); // diff 6 > threshold 3
    assert_eq!(eval(&lib, div, &input, &from_get(3, &[1])), 1.0);
    assert_eq!(eval(&lib, num, &input, &from_get(3, &[104])), 0.5);

    let ms = add(&mut lib, "matchstr", "string=1100,binary=0");
    assert_eq!(eval(&lib, ms, &input, &from_get(3, &[12])), 1.0);
    let ms2 = add(
        &mut lib,
        "matchstr",
        "string=0000,partial=0,binary=1,pow=2.0",
    );
    assert_eq!(eval(&lib, ms2, &input, &from_get(4, &[0, 0, 0, 0])), 1.0);

    let ones = add(&mut lib, "all-ones", "length=4");
    assert_eq!(eval(&lib, ones, &input, &from_get(4, &[1, 1, 1, 0])), 0.75);
    let rr = add(&mut lib, "royal-road", "length=4,block_count=2");
    assert_eq!(eval(&lib, rr, &input, &from_get(4, &[1, 1, 1, 0])), 0.5);
    let lg = add(&mut lib, "log", "halflife=1");
    assert_eq!(
        eval(&lib, lg, &from_get(3, &[1000]), &from_get(3, &[6])),
        1.0
    );
}

#[test]
fn optimize_binary_function_9_from_environment_cfg() {
    let mut lib = TaskLib::new();
    let id = add(
        &mut lib,
        "optimize",
        "function=9,binary=1,varlength=3,numvars=31,basepow=1.3,maxFx=10.0,minFx=0.0",
    );
    let input = IoBuffer::new(3);
    // buffer not yet full: no reward, task value untouched
    let mut out = IoBuffer::new(93);
    out.add(0);
    let mut ctx = TaskContext::simple(&input, &out);
    ctx.set_task_entry(id);
    assert_eq!(lib.test_output_pure(&mut ctx), Some(0.0));
    assert_eq!(ctx.get_task_value(), 0.0);
    // full buffer of zeros: all variables 0, g = 1, Fx = 1, quality (10-1+.001)/(10+.001)
    let mut out = IoBuffer::new(93);
    for _ in 0..93 {
        out.add(0);
    }
    let mut ctx = TaskContext::simple(&input, &out);
    ctx.set_task_entry(id);
    let q = lib.test_output_pure(&mut ctx).unwrap();
    assert_eq!(q, (10.0 - 1.0 + 0.001) / (10.0 - 0.0 + 0.001));
    assert_eq!(ctx.get_task_value(), 1.0);
}

#[test]
fn comm_tasks_see_neighbor_inputs() {
    let mut lib = TaskLib::new();
    let echo = add(&mut lib, "comm_echo", "");
    let not = add(&mut lib, "comm_not", "");
    let n1 = from_get(3, &[10, 20]);
    let n2 = from_get(3, &[30]);
    let others = [&n1, &n2];
    let input = IoBuffer::new(3);
    for (out, id, want) in [(30, echo, 1.0), (-11, not, 1.0), (31, echo, 0.0)] {
        let output = from_get(3, &[out]);
        let mut ctx = TaskContext::simple(&input, &output);
        ctx.other_input_buffers = &others;
        ctx.set_task_entry(id);
        assert_eq!(lib.test_output_pure(&mut ctx), Some(want));
    }
}

#[test]
fn fibonacci_sequence_state_lives_in_task_states() {
    let mut lib = TaskLib::new();
    let id = add(&mut lib, "fibonacci_seq", "target=2,penalty=0.25");
    let mut states = TaskStates::new();
    let input = IoBuffer::new(3);
    let mut results = Vec::new();
    for v in [1, 2, 3, 5, 9] {
        let output = from_get(3, &[v]);
        let mut ctx = TaskContext::simple(&input, &output);
        ctx.set_task_entry(id);
        ctx.set_task_states(&mut states);
        results.push(lib.test_output_pure(&mut ctx).unwrap());
    }
    assert_eq!(results, vec![1.0, 1.0, 0.25, 0.25, 0.0]);
}

// ---- mock world for organism / population tasks -----------------------------

#[derive(Default)]
struct Mock {
    cell_id: i32,
    av_cell_id: i32,
    prev_seen: i32,
    prev_task: i32,
    reached: i32,
    cell_data: i32,
    forage_target: i32,
    gradient: f64,
    opinion: Option<i32>,
    event_killed: bool,
    lyse: bool,
    inputs: Vec<i32>,
    kaboom: bool,
    kaboom2: bool,
    produced: HashMap<i32, i32>,
    on_hand: Vec<i32>,
    tag_label: i32,
    reputation: i32,
    donations: i32,
    sg_name: String,
    sg_states: Vec<String>,
    sg_width: i32,
    sg_grid: Vec<i32>,
    events: Vec<i32>,
    mov_preds: Vec<Vec<Option<(i32, bool)>>>,
    group_sizes: HashMap<i32, i32>,
    world_x: i32,
    world_y: i32,
    avatars: bool,
    already_produced: bool,
    pheromone: f64,
    resources: Vec<f64>,
    av_resources: Vec<f64>,
    reaction_changes: Vec<(usize, i32)>,
    reaction_exists: bool,
    kaboom_cells: HashMap<i32, bool>,
    cleared: Vec<i32>,
    rand_value: i32,
    log: Vec<String>,
}

impl TaskWorld for Mock {
    fn get_cell_id(&self) -> i32 {
        self.cell_id
    }
    fn get_av_cell_id(&self) -> i32 {
        self.av_cell_id
    }
    fn get_prev_seen_cell_id(&self) -> i32 {
        self.prev_seen
    }
    fn set_prev_seen_cell_id(&mut self, id: i32) {
        self.prev_seen = id;
    }
    fn get_prev_task_cell_id(&self) -> i32 {
        self.prev_task
    }
    fn set_prev_task_cell_id(&mut self, id: i32) {
        self.prev_task = id;
    }
    fn add_reached_task_cell(&mut self) {
        self.reached += 1;
    }
    fn get_cell_data(&self) -> i32 {
        self.cell_data
    }
    fn get_forage_target(&self) -> i32 {
        self.forage_target
    }
    fn get_gradient_movement(&self) -> f64 {
        self.gradient
    }
    fn has_opinion(&self) -> bool {
        self.opinion.is_some()
    }
    fn get_opinion_first(&self) -> i32 {
        self.opinion.unwrap_or(0)
    }
    fn get_event_killed(&self) -> bool {
        self.event_killed
    }
    fn get_lyse_display(&self) -> bool {
        self.lyse
    }
    fn get_input_at(&self, i: i32) -> i32 {
        self.inputs.get(i as usize).copied().unwrap_or(0)
    }
    fn get_kaboom_executed(&self) -> bool {
        self.kaboom
    }
    fn get_kaboom_executed2(&self) -> bool {
        self.kaboom2
    }
    fn get_number_strings_produced(&self, tag: i32) -> i32 {
        self.produced.get(&tag).copied().unwrap_or(0)
    }
    fn get_number_strings_on_hand(&self, i: i32) -> i32 {
        self.on_hand.get(i as usize).copied().unwrap_or(0)
    }
    fn update_tag(&mut self, tag: i32, bits: i32) {
        self.log.push(format!("update_tag {} {}", tag, bits));
    }
    fn get_tag_label(&self) -> i32 {
        self.tag_label
    }
    fn set_lineage_label(&mut self, label: i32) {
        self.log.push(format!("lineage {}", label));
    }
    fn get_reputation(&self) -> i32 {
        self.reputation
    }
    fn set_reputation(&mut self, rep: i32) {
        self.reputation = rep;
    }
    fn get_number_of_donations(&self) -> i32 {
        self.donations
    }
    fn state_grid_name(&self) -> String {
        self.sg_name.clone()
    }
    fn state_grid_state_id(&self, name: &str) -> i32 {
        self.sg_states
            .iter()
            .position(|s| s == name)
            .map_or(-1, |p| p as i32)
    }
    fn state_grid_num_states(&self) -> i32 {
        self.sg_states.len() as i32
    }
    fn state_grid_width(&self) -> i32 {
        self.sg_width
    }
    fn state_grid_height(&self) -> i32 {
        self.sg_grid.len() as i32 / self.sg_width.max(1)
    }
    fn state_grid_state_at(&self, cell: i32) -> i32 {
        self.sg_grid[cell as usize]
    }
    fn deme_cell_position_x(&self, cell_id: i32) -> i32 {
        cell_id % self.world_x
    }
    fn deme_get_relative_cell_id(&self, cell_id: i32) -> i32 {
        cell_id % 100
    }
    fn deme_num_events(&self) -> i32 {
        self.events.len() as i32
    }
    fn deme_cell_event_id(&self, i: i32) -> i32 {
        self.events[i as usize]
    }
    fn deme_num_movement_predicates(&self) -> i32 {
        self.mov_preds.len() as i32
    }
    fn deme_mov_predicate_event(&self, p: i32, e: i32) -> Option<(i32, bool)> {
        self.mov_preds[p as usize]
            .get(e as usize)
            .copied()
            .flatten()
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
        self.avatars
    }
    fn match_already_produced(&self) -> bool {
        self.already_produced
    }
    fn pheromone_resource_amount(&self) -> f64 {
        self.pheromone
    }
    fn get_resources(&self) -> Vec<f64> {
        self.resources.clone()
    }
    fn get_av_resources(&self) -> Vec<f64> {
        self.av_resources.clone()
    }
    fn change_reaction_resource(&mut self, task_id: usize, idx: i32) -> bool {
        if self.reaction_exists {
            self.reaction_changes.push((task_id, idx));
        }
        self.reaction_exists
    }
    fn cell_kaboom_executed(&self, cell_id: i32) -> Option<bool> {
        self.kaboom_cells.get(&cell_id).copied()
    }
    fn clear_kaboom_executed(&mut self, cell_id: i32) {
        self.cleared.push(cell_id);
    }
    fn std_rand(&mut self) -> i32 {
        self.rand_value
    }
    fn stats_add_tag(&mut self, tag: i32, value: i32) {
        self.log.push(format!("add_tag {} {}", tag, value));
    }
    fn stats_add_string_bits_matched_value(&mut self, name: &str, value: i32) {
        self.log.push(format!("bits {} {}", name, value));
    }
    fn stats_inc_perfect_match(&mut self, amount: i32) {
        self.log.push(format!("perfect {}", amount));
    }
    fn stats_inc_perfect_match_org(&mut self) {
        self.log.push("perfect_org".to_string());
    }
    fn task_stdout(&mut self, text: &str) {
        self.log.push(format!("stdout {text}"));
    }
}

fn run(lib: &TaskLib, id: usize, input: &IoBuffer, output: &IoBuffer, w: &mut Mock) -> f64 {
    let mut ctx = TaskContext::simple(input, output);
    lib.setup_tests(&mut ctx);
    ctx.set_task_entry(id);
    lib.test_output(&mut ctx, w)
}

#[test]
fn optimize_prints_a_quality_above_one() {
    // Fx 0 below minFx: quality (1 - 0 + .001) / (1 - .5 + .001) > 1, which
    // `Task_Optimize` reports on standard output.
    let mut lib = TaskLib::new();
    let id = add(&mut lib, "optimize", "function=1,maxFx=1,minFx=0.5");
    let input = IoBuffer::new(3);
    let mut w = Mock::default();
    let q = run(&lib, id, &input, &from_get(2, &[0, 0]), &mut w);
    assert_eq!(q, 1.001 / 0.501);
    assert_eq!(
        w.log,
        vec!["stdout \n\nquality > 1!  quality= 1.998  Fx= 0\n".to_string()]
    );
    // quality 1 or less: nothing printed
    let id = add(&mut lib, "optimize", "function=1");
    let mut w = Mock::default();
    assert_eq!(run(&lib, id, &input, &from_get(2, &[0, 0]), &mut w), 1.0);
    assert!(w.log.is_empty());
}

#[test]
fn optimize_refuses_arguments_the_reference_cannot_evaluate() {
    let mut lib = TaskLib::new();
    for args in [
        "function=20",
        "function=2,numvars=1",
        "function=1,binary=1,varlength=-1",
    ] {
        let mut er = EnvReqs::new();
        let mut fb = Feedback::new();
        assert_eq!(lib.add_task("optimize", args, &mut er, &mut fb), None);
        let errors = fb.errors();
        assert_eq!(errors.len(), 1, "{args}: {errors:?}");
        assert!(
            errors[0].starts_with("unsupported: optimize "),
            "{}",
            errors[0]
        );
    }
    assert_eq!(lib.get_size(), 0);
    // an invalid argument string still gets the generic error after the
    // load error
    let mut er = EnvReqs::new();
    let mut fb = Feedback::new();
    assert_eq!(lib.add_task("optimize", "binary=1", &mut er, &mut fb), None);
    assert!(fb
        .errors()
        .iter()
        .any(|e| e.starts_with("unknown/unprocessed task entry")));
}

#[test]
fn trait_tasks_are_not_on_the_pure_path() {
    let mut lib = TaskLib::new();
    let id = add(&mut lib, "exploded", "");
    let input = IoBuffer::new(3);
    let output = IoBuffer::new(3);
    let mut ctx = TaskContext::simple(&input, &output);
    ctx.set_task_entry(id);
    assert_eq!(lib.test_output_pure(&mut ctx), None);
    // an unset task entry is a no-op, not a panic
    let mut ctx = TaskContext::simple(&input, &output);
    assert_eq!(lib.test_output(&mut ctx, &mut Mock::default()), 0.0);
}

#[test]
fn simple_organism_state_tasks() {
    let mut lib = TaskLib::new();
    let input = IoBuffer::new(3);
    let output = IoBuffer::new(3);
    let mut w = Mock {
        gradient: 1.0,
        kaboom: true,
        event_killed: true,
        lyse: true,
        opinion: Some(4),
        ..Default::default()
    };
    let mut check = |name: &str, args: &str, w: &mut Mock, want: f64| {
        let id = add(&mut lib, name, args);
        assert_eq!(run(&lib, id, &input, &output, w), want, "{} {}", name, args);
    };
    check("move_up_gradient", "", &mut w, 1.0);
    check("move_not_up_gradient", "", &mut w, 0.0);
    check("move_neutral_gradient", "", &mut w, 0.0);
    check("move_down_gradient", "", &mut w, 0.0);
    // exact equality, not an epsilon test
    w.gradient = 1.0 - 1e-16;
    check("move_up_gradient", "", &mut w, 0.0);
    w.gradient = -1.0;
    check("move_down_gradient", "", &mut w, 1.0);
    w.gradient = 0.0;
    check("move_neutral_gradient", "", &mut w, 1.0);
    check("exploded", "", &mut w, 1.0);
    check("exploded2", "", &mut w, 0.0);
    check("produce-public-good", "", &mut w, 1.0);
    check("ai-display-cost", "", &mut w, 1.0);
    check("event_killed", "", &mut w, 1.0);
    check("opinion_is", "opinion=4", &mut w, 1.0);
    check("opinion_is", "opinion=5", &mut w, 0.0);
    // live-on-patch-id: no opinion counts as -1 (QUIRK), unlike opinion_is
    w.opinion = None;
    check("live-on-patch-id", "patch_id=-1", &mut w, 1.0);
    check("live-on-patch-id", "patch_id=0", &mut w, 0.0);
    check("opinion_is", "opinion=-1", &mut w, 0.0);
    w.opinion = Some(0);
    check("live-on-patch-id", "patch_id=0", &mut w, 1.0);
    w.cell_id = 7;
    check("collect-odd-cell", "even_or_odd=0", &mut w, 1.0);
    check("collect-odd-cell", "even_or_odd=1", &mut w, 0.0);
    w.cell_id = -1; // -1 % 2 == -1: counts as odd
    check("collect-odd-cell", "even_or_odd=0", &mut w, 1.0);
}

#[test]
fn consume_target_family_and_move_ft() {
    let mut lib = TaskLib::new();
    let input = from_get(3, &[A, B, C]);
    let and_out = from_get(3, &[A & B]);
    let mut w = Mock {
        forage_target: 2,
        cell_id: 9,
        prev_seen: 9,
        ..Default::default()
    };
    let mut run_named = |name: &str, args: &str, w: &mut Mock, out: &IoBuffer| {
        let id = add(&mut lib, name, args);
        run(&lib, id, &input, out, w)
    };
    assert_eq!(
        run_named("eat-target", "target_id=2", &mut w, &and_out),
        1.0
    );
    assert_eq!(
        run_named("eat-target", "target_id=3", &mut w, &and_out),
        0.0
    );
    assert_eq!(
        run_named("eat-target-and", "target_id=2", &mut w, &and_out),
        1.0
    );
    assert_eq!(
        run_named("eat-target-and", "target_id=1", &mut w, &and_out),
        0.0
    );
    assert_eq!(
        run_named("eat-target-xor", "target_id=2", &mut w, &and_out),
        0.0
    );
    let echo_out = from_get(3, &[A]);
    assert_eq!(
        run_named("eat-target-echo", "target_id=2", &mut w, &echo_out),
        1.0
    );
    // move-ft: no move, then a move that sets the previous cell
    assert_eq!(run_named("move-ft", "target_id=2", &mut w, &and_out), 0.0);
    w.cell_id = 10;
    assert_eq!(run_named("move-ft", "target_id=1", &mut w, &and_out), 0.0);
    assert_eq!(w.prev_seen, 9);
    assert_eq!(run_named("move-ft", "target_id=2", &mut w, &and_out), 1.0);
    assert_eq!(w.prev_seen, 10);
    // with avatars the avatar cell is used
    w.avatars = true;
    w.av_cell_id = 11;
    assert_eq!(run_named("move-ft", "target_id=2", &mut w, &and_out), 1.0);
    assert_eq!(w.prev_seen, 11);
}

#[test]
fn movement_tasks() {
    let mut lib = TaskLib::new();
    let input = IoBuffer::new(3);
    let output = IoBuffer::new(3);
    let mut w = Mock {
        world_x: 10,
        cell_id: 19,
        prev_seen: 5,
        cell_data: 3,
        prev_task: 7,
        events: vec![1, 3],
        ..Default::default()
    };
    let mut r = |name: &str, w: &mut Mock| {
        let id = add(&mut lib, name, "");
        run(&lib, id, &input, &output, w)
    };
    assert_eq!(r("move_to_right_side", &mut w), 1.0); // x = 9 = world_x - 1
    assert_eq!(r("move_to_left_side", &mut w), 0.0);
    assert_eq!(r("move", &mut w), 1.0);
    assert_eq!(w.prev_seen, 19);
    assert_eq!(r("move", &mut w), 0.0);
    // movetotarget: cell_data 3 > 1, current (19) != prev (7)
    assert_eq!(r("movetotarget", &mut w), 1.0);
    assert_eq!((w.reached, w.prev_task), (1, 19));
    assert_eq!(r("movetotarget", &mut w), 0.0); // same target again
    w.cell_data = 1;
    w.prev_task = 7;
    assert_eq!(r("movetotarget", &mut w), 0.0); // cell_data == 1 is not a target
    w.cell_data = 3;
    assert_eq!(r("move_to_event", &mut w), 1.0);
    w.cell_data = 4;
    assert_eq!(r("move_to_event", &mut w), 0.0);
    w.cell_id = -1;
    w.cell_data = 3;
    assert_eq!(r("move_to_event", &mut w), 0.0);
    w.cell_id = 19;
    // movement events (the five ported-from-C++ evaluators of this family)
    w.mov_preds = vec![
        vec![Some((8, true)), None],
        vec![Some((9, false)), Some((3, true))],
    ];
    w.cell_data = 3;
    w.prev_task = 50;
    assert_eq!(r("movetoevent", &mut w), 0.0); // only event 0 is consulted
    assert_eq!(r("movebetweenevent", &mut w), 1.0); // predicate 1, event 1, active
    assert_eq!(w.prev_task, 3);
    assert_eq!(r("movebetweenevent", &mut w), 0.0); // same target twice in a row
    w.mov_preds = vec![vec![Some((3, true))]];
    w.prev_task = 50;
    assert_eq!(r("movetoevent", &mut w), 1.0);
    // inactive event does not count for movebetweenevent
    w.mov_preds = vec![vec![Some((3, false)), Some((4, true))]];
    w.prev_task = 50;
    assert_eq!(r("movebetweenevent", &mut w), 0.0);
}

#[test]
fn group_tasks() {
    let mut lib = TaskLib::new();
    let input = IoBuffer::new(3);
    let output = IoBuffer::new(3);
    let mut w = Mock::default();
    w.group_sizes.insert(0, 3);
    w.group_sizes.insert(4, 6);
    let fg = add(&mut lib, "form-group", "group_size=5");
    // no opinion: group 0 of size 3; (5-3)^2/25 = 0.16 -> 0.84
    assert_eq!(
        run(&lib, fg, &input, &output, &mut w),
        1.0 - (2.0 * 2.0) / 25.0
    );
    w.opinion = Some(4); // size 6: (5-6)^2/25
    assert_eq!(run(&lib, fg, &input, &output, &mut w), 1.0 - 1.0 / 25.0);
    // QUIRK: group_size 0 gives NaN (0/0)
    let fg0 = add(&mut lib, "form-group", "group_size=0");
    w.group_sizes.insert(4, 0);
    assert!(run(&lib, fg0, &input, &output, &mut w).is_nan());
    let fgid = add(&mut lib, "form-group-id", "group_size=5,group_id=4");
    w.group_sizes.insert(4, 3);
    assert_eq!(run(&lib, fgid, &input, &output, &mut w), 1.0); // under max size
    w.group_sizes.insert(4, 7);
    assert_eq!(run(&lib, fgid, &input, &output, &mut w), 1.0 - 4.0 / 25.0);
    w.opinion = Some(5);
    assert_eq!(run(&lib, fgid, &input, &output, &mut w), 0.0);
}

#[test]
fn resource_dependent_and_xor_max() {
    let mut lib = TaskLib::new();
    let input = from_get(3, &[A, B, C]);
    let mut w = Mock {
        pheromone: 50.0,
        ..Default::default()
    };
    let nand = add(&mut lib, "nand-resourceDependent", "");
    let nor = add(&mut lib, "nor-resourceDependent", "");
    let nand_out = from_get(3, &[!(A & B)]);
    let nor_out = from_get(3, &[!(A | B)]);
    assert_eq!(run(&lib, nand, &input, &nand_out, &mut w), 1.0);
    assert_eq!(run(&lib, nor, &input, &nor_out, &mut w), 0.0); // needs > 100
    w.pheromone = 150.0;
    assert_eq!(run(&lib, nand, &input, &nand_out, &mut w), 0.0);
    assert_eq!(run(&lib, nor, &input, &nor_out, &mut w), 1.0);
    assert_eq!(run(&lib, nor, &input, &nand_out, &mut w), 0.0);

    // xor-max: picks the richest resource, then evaluates xor
    let xm = add(&mut lib, "xor-max", "");
    let xor_out = from_get(3, &[A ^ B]);
    w.resources = vec![1.0, 9.0, 4.0];
    w.reaction_exists = true;
    assert_eq!(run(&lib, xm, &input, &xor_out, &mut w), 1.0);
    assert_eq!(w.reaction_changes, vec![(xm, 1)]);
    // avatars use the avatar resources
    w.avatars = true;
    w.av_resources = vec![0.0, 0.0, 2.0];
    assert_eq!(run(&lib, xm, &input, &xor_out, &mut w), 1.0);
    assert_eq!(w.reaction_changes.last(), Some(&(xm, 2)));
    // no reaction: false -> 0
    w.reaction_exists = false;
    assert_eq!(run(&lib, xm, &input, &xor_out, &mut w), 0.0);
}

#[test]
fn consume_public_good_toroidal_neighbourhood() {
    let mut lib = TaskLib::new();
    let input = IoBuffer::new(3);
    let output = IoBuffer::new(3);
    let id = add(&mut lib, "consume-public-good", "");
    // 5x5 world, organism in cell 0 (corner): neighbours wrap around
    let mut w = Mock {
        world_x: 5,
        world_y: 5,
        cell_id: 0,
        ..Default::default()
    };
    // two producers: not enough
    w.kaboom_cells.insert(1, true);
    w.kaboom_cells.insert(5, true);
    assert_eq!(run(&lib, id, &input, &output, &mut w), 0.0);
    assert!(w.cleared.is_empty());
    // third producer via wrap-around (cell 24 = (4,4)); occupied non-producers ignored
    w.kaboom_cells.insert(24, true);
    w.kaboom_cells.insert(6, false);
    w.rand_value = 7; // 7 % 3 producers = index 1 in cell-id order -> cell 5
    assert_eq!(run(&lib, id, &input, &output, &mut w), 1.0);
    assert_eq!(w.cleared, vec![5]);
}

#[test]
fn sort_inputs_through_the_library() {
    let mut lib = TaskLib::new();
    let id = add(
        &mut lib,
        "sort_inputs",
        "size=5,direction=1,contiguous=1,halflife=5.0",
    );
    let mut w = Mock {
        inputs: vec![5, 3, 1, 4, 2],
        ..Default::default()
    };
    let input = IoBuffer::new(3);
    // get(i) is the i-th output; perfectly ascending
    let out = from_get(10, &[1, 2, 3, 4, 5]);
    assert_eq!(run(&lib, id, &input, &out, &mut w), 1.0);
    // reversed ascending request: maximal disorder -> 0
    let out = from_get(10, &[5, 4, 3, 2, 1]);
    assert_eq!(run(&lib, id, &input, &out, &mut w), 0.0);
    // fewer than half stored
    let out = from_get(10, &[1, 2]);
    assert_eq!(run(&lib, id, &input, &out, &mut w), 0.0);
    // descending request
    let did = add(
        &mut lib,
        "sort_inputs",
        "size=5,direction=-1,contiguous=1,halflife=5.0",
    );
    let out = from_get(10, &[5, 4, 3, 2, 1]);
    assert_eq!(run(&lib, did, &input, &out, &mut w), 1.0);
    // invalid values around a sorted span
    let mut w3 = Mock {
        inputs: vec![10, 20, 30],
        ..Default::default()
    };
    let id3 = add(&mut lib, "sort_inputs", "size=3,halflife=5.0");
    let out = from_get(10, &[99, 10, 20, 30, 99]);
    assert_eq!(run(&lib, id3, &input, &out, &mut w3), 1.0);
    // QUIRK: scattered mode starts at the first output that is NOT an input,
    // so a fully valid output scores 0
    let sid = add(&mut lib, "sort_inputs", "size=5,contiguous=0,halflife=5.0");
    let out = from_get(10, &[1, 2, 3, 4, 5]);
    assert_eq!(run(&lib, sid, &input, &out, &mut w), 0.0);
}

#[test]
fn sg_path_traversal_and_perfect_strings() {
    let mut lib = TaskLib::new();
    let id = add(
        &mut lib,
        "sg_path_traversal",
        "pathlen=4,sgname=grid,poison=P",
    );
    // states: 0 = P (poison), 1 = food; grid of 6 cells
    let mut w = Mock {
        sg_name: "grid".to_string(),
        sg_states: vec!["P".to_string(), "F".to_string()],
        sg_width: 3,
        sg_grid: vec![1, 0, 1, 1, 0, 1],
        ..Default::default()
    };
    let input = IoBuffer::new(3);
    let output = IoBuffer::new(3);
    // ext_mem: 3 header words + 2 per-state counters, then the visit history.
    // header offset = 3 + num_states = 5; ext_mem[3 + state(0)] = poison touches.
    let ext = [0, 0, 0, 1, 0, 0, 2, 0, 0, 3, 3, 5];
    let mut ctx = TaskContext::simple(&input, &output);
    ctx.ext_mem = &ext;
    ctx.set_task_entry(id);
    // history {0,2,0,0,3,3,5} -> unique cells 0,2,3,5: states 1,1,1,1 are non-poison -> 4,
    // minus 1 poison touch -> 3 -> 3/4. (0 is state 1; cell 1,4 are poison)
    assert_eq!(lib.test_output(&mut ctx, &mut w), 0.75);
    w.sg_name = "other".to_string();
    assert_eq!(lib.test_output(&mut ctx, &mut w), 0.0);

    // perfect_strings counts matchprodstr tasks (min 2, extra 3)
    let mut lib = TaskLib::new();
    add(&mut lib, "matchprodstr", "string=0000,tag=0");
    add(&mut lib, "matchprodstr", "string=1111,tag=1");
    let ps = add(&mut lib, "perfect_strings", "");
    let mut w = Mock {
        on_hand: vec![2, 5],
        ..Default::default()
    };
    assert_eq!(run(&lib, ps, &input, &output, &mut w), 2.0 + 3.0);
    assert_eq!(
        w.log,
        vec!["perfect 2".to_string(), "perfect_org".to_string()]
    );
}
