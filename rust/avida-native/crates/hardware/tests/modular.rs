//! The opt-in `heads_modular` instruction set (`instset-heads-modular.cfg`):
//! six registers (nop-D to nop-F), label complements through six nops, and
//! the hand-written subroutine organism (`modular-subroutine.org`).

mod common;

use avida_genome::{Genome, InstructionSequence};
use avida_hardware::test_cpu::TestCpu;
use common::*;
use std::path::PathBuf;

fn config_dir() -> PathBuf {
    avida_core().join("support/config")
}

/// The `INSTSET`/`INST` lines of an instruction set file in the stock config.
fn instset_lines(file: &str) -> Vec<String> {
    let text = std::fs::read_to_string(config_dir().join(file)).unwrap();
    text.lines()
        .map(|l| l.split('#').next().unwrap().trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn fixture(file: &str) -> Fixture {
    Fixture::load_with_instset(&config_dir(), instset_lines(file)).expect("config loads")
}

/// A genome from instruction names.
fn genome(fx: &Fixture, names: &[&str]) -> Genome {
    let is = fx.hwm.registry().default_inst_set().unwrap();
    let mut seq = InstructionSequence::new();
    seq.resize(names.len() as i32);
    for (i, n) in names.iter().enumerate() {
        seq[i as i32] = is.get_inst(n).unwrap();
    }
    Genome::with_inst_set(is.hardware_type(), is.inst_set_name(), seq)
}

fn traced(fx: &mut Fixture, g: &Genome) -> (String, avida_hardware::test_cpu::CpuTestInfo) {
    let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
    trace(fx, &mut cpu, g, false).unwrap()
}

/// The registers printed before the `n`th instruction (1-based).
fn regs_at(trace: &str, n: usize) -> Vec<i64> {
    let line = trace
        .lines()
        .filter(|l| l.starts_with("AX:"))
        .nth(n - 1)
        .unwrap();
    line.split("  ")
        .filter_map(|f| f.split(':').nth(1))
        .filter_map(|v| v.split_whitespace().next())
        .filter_map(|v| v.parse().ok())
        .collect()
}

/// The instruction pointers of the first `n` steps.
fn ips(trace: &str, n: usize) -> Vec<i32> {
    trace
        .lines()
        .filter_map(|l| l.split(" IP:").nth(1))
        .take(n)
        .map(|s| s.split_whitespace().next().unwrap().parse().unwrap())
        .collect()
}

/// The top of the local stack printed in step `n` (1-based).
fn stack_top(trace: &str, n: usize) -> u32 {
    let line = trace
        .lines()
        .filter(|l| l.contains("Stack 0:"))
        .nth(n - 1)
        .unwrap();
    let first = line.split("Ox").nth(1).unwrap();
    u32::from_str_radix(&first[..8], 16).unwrap()
}

#[test]
fn six_registers_wrap_through_fx_to_ax() {
    let mut fx = fixture("instset-heads-modular.cfg");
    // FX = 2, then `swap ?FX?` exchanges FX with the next register, AX.
    let g = genome(
        &fx,
        &[
            "inc", "nop-F", "inc", "nop-F", "swap", "nop-F", "inc", "nop-E",
        ],
    );
    let (t, _) = traced(&mut fx, &g);
    assert_eq!(regs_at(&t, 1), vec![0, 0, 0, 0, 0, 0]);
    assert_eq!(regs_at(&t, 3), vec![0, 0, 0, 0, 0, 2]);
    assert_eq!(regs_at(&t, 4), vec![2, 0, 0, 0, 0, 0]);
    assert_eq!(regs_at(&t, 5), vec![2, 0, 0, 0, 1, 0]);
}

#[test]
fn three_registers_wrap_through_cx_to_ax_on_the_stock_set() {
    let mut fx = fixture("instset-heads.cfg");
    let g = genome(
        &fx,
        &[
            "inc", "nop-C", "inc", "nop-C", "swap", "nop-C", "inc", "nop-B",
        ],
    );
    let (t, _) = traced(&mut fx, &g);
    assert_eq!(regs_at(&t, 3), vec![0, 0, 2]);
    assert_eq!(regs_at(&t, 4), vec![2, 0, 0]);
    assert_eq!(regs_at(&t, 5), vec![2, 1, 0]);
    // Three registers print as before: no DX..FX.
    assert!(!t.contains("DX:"));
}

#[test]
fn six_registers_print_in_the_same_style() {
    let mut fx = fixture("instset-heads-modular.cfg");
    let g = genome(&fx, &["inc", "nop-F"]);
    let (t, _) = traced(&mut fx, &g);
    let line = t.lines().find(|l| l.starts_with("AX:")).unwrap();
    assert_eq!(
        line,
        "AX:0 [0x0]  BX:0 [0x0]  CX:0 [0x0]  DX:0 [0x0]  EX:0 [0x0]  FX:0 [0x0]  "
    );
}

#[test]
fn labels_complement_through_six_nops() {
    let mut fx = fixture("instset-heads-modular.cfg");
    // call C C jumps past the label D D (the complement with six nops); the
    // stock complement A A is absent. The return address 2 is on the stack
    // while the subroutine runs and gone after `return`.
    let g = genome(
        &fx,
        &[
            "inc", "nop-B", // 0, 1: marks the call site as position 2
            "call", "nop-C", "nop-C", // 2..4
            "inc", "nop-A", // 5, 6: skipped
            "nop-D", "nop-D", // 7, 8: the label D D
            "inc", "nop-E",  // 9, 10: the subroutine body
            "return", // 11
        ],
    );
    let (t, _) = traced(&mut fx, &g);
    assert_eq!(ips(&t, 5), vec![0, 2, 9, 11, 3]);
    assert_eq!(stack_top(&t, 2), 0, "before the call");
    assert_eq!(stack_top(&t, 3), 2, "the return address sits on the stack");
    assert_eq!(stack_top(&t, 5), 0, "return pops it");
    assert_eq!(regs_at(&t, 4), vec![0, 1, 0, 0, 1, 0]);
}

/// The names of the tasks the organism did, by reaction order of the stock
/// environment: not, nand, and, orn, or, andn, nor, xor, equ.
const TASKS: [&str; 9] = [
    "not", "nand", "and", "orn", "or", "andn", "nor", "xor", "equ",
];

#[test]
fn hand_written_organism_replicates_true_and_does_three_tasks() {
    let mut fx = fixture("instset-heads-modular.cfg");
    // The organism file names its set; load it from the config directory.
    let g = fx.load_org("modular-subroutine.org");
    assert_eq!(g.representation().size(), 57);
    let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
    let (t, info) = trace(&mut fx, &mut cpu, &g, false).unwrap();
    assert!(info.is_viable(), "the organism replicates");
    let org = info.test_organism(0).unwrap();
    assert_eq!(
        org.state.offspring_genome.representation().as_string(),
        g.representation().as_string(),
        "the offspring is identical"
    );
    let p = info.test_phenotype(0).unwrap();
    let done: Vec<&str> = TASKS
        .iter()
        .zip(p.last_task_count.iter())
        .filter(|(_, &c)| c > 0)
        .map(|(n, _)| *n)
        .collect();
    assert_eq!(done, vec!["not", "nand", "and"]);
    // The shared subroutine is entered from three call sites.
    assert!(t.matches("(call)").count() >= 3);
    assert!(t.contains("(return)"));
}

#[test]
fn the_new_instructions_run_in_the_parallel_bursts() {
    use avida_hardware::parallel::cpu_parallel_ok;
    let fx = fixture("instset-heads-modular.cfg");
    let is = fx.hwm.registry().default_inst_set().unwrap();
    assert_eq!((is.size(), is.num_nops()), (31, 6));
    for name in ["nop-D", "nop-E", "nop-F", "call", "return", "push", "pop"] {
        let inst = is.get_inst(name).unwrap();
        assert!(
            cpu_parallel_ok(is.lib_function_index(inst)),
            "{name} is organism-local"
        );
    }
}
