//! Byte-for-byte comparisons of test-CPU traces with the C++ reference
//! (`cbuild/work/avida -a`, analyze `TRACE`), stored under
//! `tests/fixtures`.

mod common;

use avida_hardware::test_cpu::TestCpu;
use common::*;

/// heads_default_100u: `LOAD_ORGANISM default-classic.org` + `TRACE`.
#[test]
fn heads_default_ancestor_trace_matches_reference() {
    let dir = avida_core().join("tests/heads_default_100u/config");
    let mut fx = Fixture::load(&dir, &[]).expect("config loads");
    let genome = fx.load_org("default-classic.org");
    let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
    let (text, info) = trace(&mut fx, &mut cpu, &genome, false).unwrap();
    let expected =
        std::fs::read_to_string(fixtures().join("heads_default_ancestor.trace")).unwrap();
    if let Some(d) = first_difference(&text, &expected) {
        panic!("trace differs at {}", d);
    }
    assert_eq!(text, expected);
    assert!(info.is_viable());
    let p = info.test_phenotype(0).unwrap();
    assert_eq!(p.gestation_time, 389);
    assert_eq!(p.copied_size, 100);
    assert_eq!(p.executed_size, 97);
}

/// FNV-1a, 64 bit.
fn fnv1a64(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Random genomes over every ported instruction that the reference binary
/// survives in a test CPU (see the fixture header): 180 genomes in 12
/// batches of 62-instruction sets, half with fixed and half with random
/// inputs, each trace compared by hash and length with the C++ `TRACE`.
/// Each batch was one reference run, so one world RNG and one test CPU
/// carry over from genome to genome as in `cAnalyze::CommandTrace`.
#[test]
fn random_genome_traces_match_reference() {
    let dir = avida_core().join("tests/heads_default_100u/config");
    check_trace_fixture(&dir, "random_genome_traces.txt", -1, 12, 180);
}

/// The environment-dependent families (resources and sensing, movement,
/// donation and energy sharing, messaging, energy levels, groups, kaboom
/// and promoters) against the C++ `TRACE` at update 0 in the `res_env`
/// fixture config, whose resources, reactions and state grid give the
/// sensing and collection instructions something to read: 480 genomes in
/// 16 batches (see the fixture header for the instructions left out).
#[test]
fn random_genome_traces_with_resources_match_reference() {
    let dir = fixtures().join("res_env");
    check_trace_fixture(&dir, "random_genome_traces_res.txt", 0, 16, 480);
}

/// Trace every `SEQ` of the fixture file `name` with its batch's
/// instruction set in the config `dir` at `update`, and compare each
/// trace's FNV-1a hash and length with the recorded C++ values.
fn check_trace_fixture(
    dir: &std::path::Path,
    name: &str,
    update: i32,
    num_batches: usize,
    num_traces: usize,
) {
    let text = std::fs::read_to_string(fixtures().join(name)).unwrap();
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

    let mut failures = Vec::new();
    let mut total = 0;
    for (b, batch) in batches.iter().enumerate() {
        let mut instset = vec!["INSTSET diff:hw_type=0".to_string()];
        instset.extend(batch.insts.iter().map(|n| format!("INST {n}")));
        let mut fx = Fixture::load_with_instset(dir, instset).expect("config loads");
        let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
        for (i, (seq, hash, len)) in batch.seqs.iter().enumerate() {
            let genome = fx.genome_from_letters(seq);
            let (trace_text, _) =
                trace_at_update(&mut fx, &mut cpu, &genome, batch.random, update).unwrap();
            total += 1;
            if fnv1a64(trace_text.as_bytes()) != *hash || trace_text.len() != *len {
                failures.push(format!(
                    "batch {b} seq {}: {seq} ({} vs {len} bytes)",
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

/// Developer aid (ignored): trace genomes in the config directory
/// `HW_TRACE_DIR` the way analyze `TRACE` does, for diffing against the
/// reference binary. With `HW_TRACE_SEQ` (letters) the single trace goes to
/// the file `HW_TRACE_OUT`; with `HW_TRACE_SEQ_FILE` (one sequence per line,
/// as a run of `LOAD_SEQUENCE`s followed by one `TRACE`) the traces go to
/// `HW_TRACE_OUT/org-Seq<n>.trace`, sharing one test CPU and the world RNG
/// as `cAnalyze::CommandTrace` does. `HW_TRACE_RANDOM` asks for random
/// inputs.
#[test]
#[ignore]
fn dump_trace() {
    let (Ok(dir), Ok(out)) = (std::env::var("HW_TRACE_DIR"), std::env::var("HW_TRACE_OUT")) else {
        return;
    };
    let random = std::env::var("HW_TRACE_RANDOM").is_ok();
    let update: i32 = std::env::var("HW_TRACE_UPDATE")
        .ok()
        .and_then(|u| u.parse().ok())
        .unwrap_or(-1);
    let mut fx = Fixture::load(std::path::Path::new(&dir), &[]).expect("config loads");
    let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
    if let Ok(seq) = std::env::var("HW_TRACE_SEQ") {
        let genome = fx.genome_from_letters(&seq);
        let (text, _) = trace_at_update(&mut fx, &mut cpu, &genome, random, update).unwrap();
        std::fs::write(out, text).unwrap();
        return;
    }
    let Ok(file) = std::env::var("HW_TRACE_SEQ_FILE") else {
        return;
    };
    let seqs = std::fs::read_to_string(file).unwrap();
    let out = std::path::Path::new(&out);
    std::fs::create_dir_all(out).unwrap();
    for (n, seq) in seqs.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let genome = fx.genome_from_letters(seq.trim());
        let text = match trace_at_update(&mut fx, &mut cpu, &genome, random, update) {
            Ok((text, _)) => text,
            Err(e) => format!("ERROR: {}\n", e),
        };
        std::fs::write(out.join(format!("org-Seq{}.trace", n + 1)), text).unwrap();
    }
}
