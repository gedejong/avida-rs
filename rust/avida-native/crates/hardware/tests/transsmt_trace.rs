//! Byte-for-byte comparisons of `cHardwareTransSMT` (hw_type 2) test-CPU
//! traces with the C++ reference (`cbuild/work/avida -a`, analyze `TRACE`),
//! recorded as FNV-1a hashes and lengths.
//!
//! Multi-threaded execution is not covered: in a test CPU a second thread
//! can only come from `Thread-Create`, and with `MAX_CPU_THREADS` above 1
//! the reference segfaults on the next `SingleProcess` (the new thread's
//! NULL owner is dereferenced), so the fixture runs with one thread.

mod common;

use avida_hardware::test_cpu::TestCpu;
use common::*;

/// FNV-1a, 64 bit.
fn fnv1a64(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// set_random_seed's ancestors in its own config (`LOAD_ORGANISM
/// evolved-not.org`, `LOAD_ORGANISM parasite-smt.org`, `TRACE trace_out/`):
/// with `REQUIRE_SINGLE_REACTION 1` and no reaction performed, both time
/// out without offspring.
#[test]
fn transsmt_ancestor_traces_match_reference() {
    let dir = avida_core().join("tests/set_random_seed/config");
    let mut fx = Fixture::load(&dir, &[]).expect("config loads");
    let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
    for (org, hash, len) in [
        ("evolved-not.org", 0x209f_cc8b_32e1_dda3_u64, 5_942_205),
        ("parasite-smt.org", 0xc9ce_836d_1bdb_da85, 935_244),
    ] {
        let genome = fx.load_org(org);
        let (text, _) = trace(&mut fx, &mut cpu, &genome, false).unwrap();
        assert_eq!(
            (fnv1a64(text.as_bytes()), text.len()),
            (hash, len),
            "{org} trace differs from the reference"
        );
        assert!(text.ends_with("# TIMEOUT: No offspring produced.\n"));
    }
}

/// evolved-not.org with `REQUIRE_SINGLE_REACTION 0` and `MIN_COPIED_LINES`
/// / `MIN_EXE_LINES` 0: it copies itself and divides (SPLIT method).
#[test]
fn transsmt_evolved_not_divides_like_reference() {
    let dir = avida_core().join("tests/set_random_seed/config");
    let sets = [
        ("MIN_COPIED_LINES", "0.0"),
        ("MIN_EXE_LINES", "0.0"),
        ("REQUIRE_SINGLE_REACTION", "0"),
    ];
    let mut fx = Fixture::load(&dir, &sets).expect("config loads");
    let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
    let genome = fx.load_org("evolved-not.org");
    let (text, info) = trace(&mut fx, &mut cpu, &genome, false).unwrap();
    assert_eq!(
        (fnv1a64(text.as_bytes()), text.len()),
        (0xd872_abb3_0645_c557, 1_776_380),
        "trace differs from the reference"
    );
    assert!(text.contains("# Offspring Memory: "));
    assert!(info.is_viable());
}

/// Random genomes over the 48 TransSMT instructions other than `Apoptosis`
/// and `NULL` (every one is executed) plus mutants of the two ancestors
/// (28 of the 216 genomes divide), in 8 batches whose settings cover
/// the divide methods (offspring, split, birth), divide/parent/copy/inject
/// mutations, genome-size limits, virulence and thread slicing, label size
/// limits and resource collection with and without a storage cap, half with
/// random inputs (see the fixture header). Each batch is one reference run,
/// so one world RNG and one test CPU carry over from genome to genome as in
/// `cAnalyze::CommandTrace`.
#[test]
fn transsmt_random_genome_traces_match_reference() {
    let text = std::fs::read_to_string(fixtures().join("transsmt_traces.txt")).unwrap();
    struct Batch {
        random: bool,
        update: i32,
        sets: Vec<(String, String)>,
        seqs: Vec<(String, u64, usize)>,
    }
    let mut batches: Vec<Batch> = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.as_slice() {
            ["BATCH", _name, mode, update, sets @ ..] => batches.push(Batch {
                random: *mode == "random",
                update: update.parse().unwrap(),
                sets: sets
                    .iter()
                    .map(|kv| {
                        let (k, v) = kv.split_once('=').expect("KEY=VALUE");
                        (k.to_string(), v.to_string())
                    })
                    .collect(),
                seqs: Vec::new(),
            }),
            ["SEQ", seq, hash, len] => batches.last_mut().unwrap().seqs.push((
                seq.to_string(),
                u64::from_str_radix(hash, 16).unwrap(),
                len.parse().unwrap(),
            )),
            _ => panic!("bad fixture line {line:?}"),
        }
    }
    assert_eq!(batches.len(), 8);

    let dir = fixtures().join("transsmt");
    let mut failures = Vec::new();
    let mut total = 0;
    let mut divided = 0;
    for (b, batch) in batches.iter().enumerate() {
        let sets: Vec<(&str, &str)> = batch
            .sets
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let mut fx = Fixture::load(&dir, &sets).expect("config loads");
        let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
        for (i, (seq, hash, len)) in batch.seqs.iter().enumerate() {
            let genome = fx.genome_from_letters(seq);
            let (trace_text, _) =
                trace_at_update(&mut fx, &mut cpu, &genome, batch.random, batch.update).unwrap();
            total += 1;
            if trace_text.contains("# Offspring Memory: ") {
                divided += 1;
            }
            if fnv1a64(trace_text.as_bytes()) != *hash || trace_text.len() != *len {
                failures.push(format!(
                    "batch {b} seq {}: {seq} ({} vs {len} bytes)",
                    i + 1,
                    trace_text.len()
                ));
            }
        }
    }
    assert_eq!(total, 216);
    assert!(
        failures.is_empty(),
        "{} traces differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(divided, 28);
}
