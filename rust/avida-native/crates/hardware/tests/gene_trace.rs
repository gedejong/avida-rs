//! Byte-for-byte comparisons of `cHardwareGP8` (hw_type 4) and
//! `cHardwareBCR` (hw_type 5) test-CPU traces with the C++ reference
//! (`cbuild/work/avida -a`, analyze `LOAD_SEQUENCE`s + `TRACE`), recorded as
//! FNV-1a hashes and lengths in `tests/fixtures/gene_env/traces_<type>.txt`
//! (see each file's header).
//!
//! Each `BATCH` line names its config directory (relative to
//! `tests/fixtures`), input mode and trace update; the instruction set is
//! the batch's `INST` list. The configs are the experimental-hardware
//! fixtures (`exp_env/*`) plus five variants under `gene_env/`: copy and
//! divide mutations (`mut1`), `DIVIDE_METHOD 2` with failure resets and
//! `MIN_CYCLES` (`div2`), six threads (`thr`), `DIVIDE_METHOD 0` with slip
//! copying, resource bins and `LOOK_DISABLE 7` (`misc1`), and forage targets
//! with `LOOK_DISABLE 5`, three threads and copy insertions/deletions
//! (`misc2`).
//!
//! The GP8 trace shows no hardware status (`cOrganism::PrintStatus` skips
//! `PrintStatus` for `HARDWARE_TYPE_CPU_GP8`), so its traces compare the
//! phenotype, inputs, outputs and step count of every step; the BCR trace
//! shows the registers, heads, stacks, memory spaces and genes too.

mod common;

use avida_hardware::test_cpu::TestCpu;
use common::*;

/// FNV-1a, 64 bit.
fn fnv1a64(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[test]
fn gp8_traces_match_reference() {
    check_gene_fixture("gp8", 4, 35, 186);
}

#[test]
fn bcr_traces_match_reference() {
    check_gene_fixture("bcr", 5, 48, 173);
}

/// Trace every `SEQ` of `gene_env/traces_<name>.txt` with its batch's
/// config, instruction set (as `INSTSET diff:hw_type=<hw_type>`), input mode
/// and update, and compare each trace's FNV-1a hash and length with the
/// recorded C++ values. Each batch starts from a fresh world, as each was
/// one reference run.
fn check_gene_fixture(name: &str, hw_type: i32, num_batches: usize, num_traces: usize) {
    let text = std::fs::read_to_string(
        fixtures()
            .join("gene_env")
            .join(format!("traces_{name}.txt")),
    )
    .unwrap();
    struct Batch {
        config: String,
        random: bool,
        update: i32,
        insts: Vec<String>,
        seqs: Vec<(String, u64, usize)>,
    }
    let mut batches: Vec<Batch> = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["BATCH", config, mode, update] => batches.push(Batch {
                config: config.to_string(),
                random: *mode == "random",
                update: update.parse().unwrap(),
                insts: Vec::new(),
                seqs: Vec::new(),
            }),
            ["INST", inst] => batches.last_mut().unwrap().insts.push(inst.to_string()),
            ["SEQ", seq, hash, len] => batches.last_mut().unwrap().seqs.push((
                seq.to_string(),
                u64::from_str_radix(hash, 16).unwrap(),
                len.parse().unwrap(),
            )),
            _ => panic!("bad fixture line {line:?}"),
        }
    }
    assert_eq!(batches.len(), num_batches);

    let mut failures = Vec::new();
    let mut total = 0;
    for (b, batch) in batches.iter().enumerate() {
        let mut instset = vec![format!("INSTSET diff:hw_type={hw_type}")];
        instset.extend(batch.insts.iter().map(|n| format!("INST {n}")));
        let dir = fixtures().join(&batch.config);
        let mut fx = Fixture::load_with_instset(&dir, instset).expect("config loads");
        let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
        for (i, (seq, hash, len)) in batch.seqs.iter().enumerate() {
            let genome = fx.genome_from_letters(seq);
            let trace_text =
                match trace_at_update(&mut fx, &mut cpu, &genome, batch.random, batch.update) {
                    Ok((t, _)) => t,
                    Err(e) => format!("ERROR: {e}"),
                };
            total += 1;
            if fnv1a64(trace_text.as_bytes()) != *hash || trace_text.len() != *len {
                failures.push(format!(
                    "{name} batch {b} ({}) seq {}: {seq} ({} vs {len} bytes)",
                    batch.config,
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
