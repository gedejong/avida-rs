//! `RECALCULATE` + `DETAIL` of the 1420 evolved genotypes of
//! avida-core/tests/analyze_printphenotypes (legacy heads instruction set,
//! nine logic tasks with depletable resources), compared field by field
//! with the C++ reference:
//!
//! ```text
//! LOAD detail-100000.pop
//! RECALCULATE
//! DETAIL recalc.dat id viable length copy_length exe_length merit gest_time
//!   fitness div_type mate_id executed_flags task.0..8 task_quality.0..8
//!   inst.0..25 env_input.0..2 sequence
//! ```
//!
//! (the fixture keeps every column but the sequence, which is read from the
//! `.pop` file).

mod common;

use avida_apto::fmt::fmt_double;
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu};
use common::*;

/// One DETAIL row as the C++ writes it (`ostream <<` for doubles).
fn detail_row(id: &str, info: &CpuTestInfo, flags: &str) -> Vec<String> {
    let p = info.test_phenotype(0).expect("tested");
    let mut row = vec![
        id.to_string(),
        i32::from(info.is_viable()).to_string(),
        p.genome_length.to_string(),
        p.copied_size.to_string(),
        p.executed_size.to_string(),
        fmt_double(p.merit.get_double()),
        p.gestation_time.to_string(),
        fmt_double(p.fitness),
        fmt_double(p.div_type),
        p.mate_select_id.to_string(),
        flags.to_string(),
    ];
    for i in 0..9 {
        row.push(p.last_task_count.get(i).copied().unwrap_or(0).to_string());
    }
    for i in 0..9 {
        row.push(fmt_double(
            p.last_task_quality.get(i).copied().unwrap_or(0.0),
        ));
    }
    for i in 0..26 {
        // QUIRK: cAnalyzeGenotype::GetInstExecutedCount
        // (analyze/cAnalyzeGenotype.cc:726) guards with `_inst_num > 0`, so
        // the DETAIL column `inst.0` is always 0 even when instruction 0 ran
        // (77 of these genotypes execute nop-A once). The hardware count
        // itself is right; the quirk belongs to the analyze layer.
        let n = if i == 0 {
            0
        } else {
            p.last_inst_count.get(i).copied().unwrap_or(0)
        };
        row.push(n.to_string());
    }
    for i in 0..3 {
        row.push(
            info.test_cpu_inputs()
                .get(i)
                .copied()
                .unwrap_or(0)
                .to_string(),
        );
    }
    row
}

/// `cPlasticPhenotype::SetExecutedFlags`: the executed flags of the
/// depth-0 organism's memory after the test.
fn executed_flags(info: &CpuTestInfo) -> String {
    let mem = info.test_organism(0).expect("tested").hardware.memory();
    (0..mem.size())
        .map(|i| if mem.flag_executed(i) { '+' } else { '-' })
        .collect()
}

#[test]
fn printphenotypes_recalculate_matches_reference() {
    let dir = avida_core().join("tests/analyze_printphenotypes/config");
    let mut fx = Fixture::load(&dir, &[]).expect("config loads");

    let pop = std::fs::read_to_string(dir.join("detail-100000.pop")).unwrap();
    let genotypes: Vec<(String, String)> = pop
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            (w[0].to_string(), w[w.len() - 1].to_string())
        })
        .collect();
    let expected = std::fs::read_to_string(fixtures().join("printphenotypes_recalc.dat")).unwrap();
    let expected: Vec<Vec<String>> = expected
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split_whitespace().map(str::to_string).collect())
        .collect();
    assert_eq!(genotypes.len(), 1420);
    assert_eq!(expected.len(), genotypes.len());

    let mut mismatches = Vec::new();
    for ((id, seq), want) in genotypes.iter().zip(&expected) {
        let genome = fx.genome_from_letters(seq);
        // cPhenPlastGenotype::Process: a fresh test CPU per genotype.
        let mut cpu = TestCpu::new(&mut fx.world()).unwrap();
        let mut info = CpuTestInfo::default();
        cpu.test_genome(&mut fx.world(), &mut info, &genome)
            .unwrap();
        let got = detail_row(id, &info, &executed_flags(&info));
        if &got != want {
            mismatches.push(format!(
                "{}:\n  rust: {}\n  c++ : {}",
                id,
                got.join(" "),
                want.join(" ")
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} of {} rows differ; first:\n{}",
        mismatches.len(),
        genotypes.len(),
        mismatches
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
