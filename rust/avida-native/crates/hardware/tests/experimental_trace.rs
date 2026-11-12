//! Byte-for-byte comparisons of `cHardwareExperimental` (hw_type 3)
//! test-CPU traces with the C++ reference (`cbuild/work/avida -a`, analyze
//! `TRACE` at update 0), recorded as FNV-1a hashes and lengths in
//! `tests/fixtures/exp_env/traces_<config>.txt` (see each file's header).
//!
//! The configs are the `res_env` fixture's (global resources, reactions, a
//! state grid) with the experimental-hardware settings of each file. They
//! hold no gradient resources: `cGradientCount` draws from the world RNG at
//! setup, which the analyze-mode fixture world does not reproduce (and the
//! analyze crate rejects).

mod common;

use avida_hardware::test_cpu::TestCpu;
use common::*;

/// FNV-1a, 64 bit.
fn fnv1a64(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// `MULTI_ABSORB_TYPE 1`: random genomes over every experimental
/// instruction but `die`, `deposit-resource` and `nop-deposit-resource`
/// (5 batches each with fixed inputs, random inputs, and nop-A..nop-P),
/// hand-written genomes for the two deposits, and the ancestors of the
/// lookAhead_v1, avatars-pred_look and avatars tests.
#[test]
fn experimental_traces_match_reference() {
    check_exp_fixture("abs1", 18, 240);
}

/// `MULTI_ABSORB_TYPE 2` with `USE_AVATARS 2`, leaving out the
/// instructions that reach `cOrganism::doAVOutput` (output, divide and
/// task-checked messaging).
#[test]
fn experimental_avatar_traces_match_reference() {
    check_exp_fixture("abs2av", 6, 81);
}

/// Forage targets 0, 1 and 3 with `PRED_PREY_SWITCH 1` (fixed inputs, plus
/// the deposits and the test ancestors) and with `PRED_PREY_SWITCH -2`,
/// `JUV_PERIOD 3` (random inputs).
#[test]
fn experimental_forage_target_traces_match_reference() {
    check_exp_fixture("ft1", 8, 90);
    check_exp_fixture("ft2", 6, 81);
}

/// Genomes of the pp-evolved test with its instruction set (instruction
/// costs, four threads under `THREAD_SLICING_METHOD 1`) and settings, as
/// evolved (none reproduces: a reaction is required) and with reproduction
/// unconstrained (8 of 30 divide, with `DIVIDE_MUT_PROB 0.25`).
#[test]
fn experimental_pp_evolved_traces_match_reference() {
    check_exp_fixture("pp", 2, 40);
    check_exp_fixture("pp_div", 2, 30);
}

/// `abs(INT_MIN)` in the experimental instructions that fold a register
/// with it (forage targets, look-around, rotate-x, the deposits,
/// get-res-stored), in the `ft1` config.
#[test]
fn experimental_int_min_traces_match_reference() {
    check_exp_traces("traces_intmin.txt", "ft1", 1, 9);
}

/// [`check_exp_traces`] on `exp_env/traces_<config>.txt`.
fn check_exp_fixture(config: &str, num_batches: usize, num_traces: usize) {
    check_exp_traces(
        &format!("traces_{config}.txt"),
        config,
        num_batches,
        num_traces,
    );
}

/// Trace every `SEQ` of `exp_env/<file>` with its batch's instruction set
/// (hw_type 3) in the config `exp_env/<config>` at update 0, and compare
/// each trace's FNV-1a hash and length with the recorded C++ values. Each
/// batch starts from a fresh world, as each was one reference run.
fn check_exp_traces(file: &str, config: &str, num_batches: usize, num_traces: usize) {
    let base = fixtures().join("exp_env");
    let text = std::fs::read_to_string(base.join(file)).unwrap();
    struct Batch {
        random: bool,
        insts: Vec<String>,
        seqs: Vec<(String, u64, usize)>,
    }
    let mut batches: Vec<Batch> = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["BATCH", mode] => batches.push(Batch {
                random: *mode == "random",
                insts: Vec::new(),
                seqs: Vec::new(),
            }),
            ["INST", name] => batches.last_mut().unwrap().insts.push(name.to_string()),
            ["SEQ", seq, hash, len] => batches.last_mut().unwrap().seqs.push((
                seq.to_string(),
                u64::from_str_radix(hash, 16).unwrap(),
                len.parse().unwrap(),
            )),
            _ => panic!("bad fixture line {line:?}"),
        }
    }
    assert_eq!(batches.len(), num_batches);

    let dir = base.join(config);
    let mut failures = Vec::new();
    let mut total = 0;
    for (b, batch) in batches.iter().enumerate() {
        let mut instset = vec!["INSTSET diff:hw_type=3".to_string()];
        instset.extend(batch.insts.iter().map(|n| format!("INST {n}")));
        let mut fx = Fixture::load_with_instset(&dir, instset).expect("config loads");
        let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
        for (i, (seq, hash, len)) in batch.seqs.iter().enumerate() {
            let genome = fx.genome_from_letters(seq);
            let trace_text = match trace_at_update(&mut fx, &mut cpu, &genome, batch.random, 0) {
                Ok((t, _)) => t,
                Err(e) => format!("ERROR: {e}"),
            };
            total += 1;
            if fnv1a64(trace_text.as_bytes()) != *hash || trace_text.len() != *len {
                failures.push(format!(
                    "{config} batch {b} seq {}: {seq} ({} vs {len} bytes)",
                    i + 1,
                    trace_text.len()
                ));
            }
        }
    }
    assert_eq!(total, num_traces);
    assert!(
        failures.is_empty(),
        "{} traces differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
