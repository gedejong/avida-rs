//! `ANALYZE_REDUNDANCY_BY_INST_FAILURE` (`cAnalyze::
//! CommandAnalyzeRedundancyByInstFailure`, analyze/cAnalyze.cc at 47f13dadb):
//! each genotype's fitness as the instructions that may fail
//! (`prob_fail > 0`) fail with probabilities from `10^start` to `10^end`.
//!
//! The command runs every genotype on a copy of its instruction set,
//! registered as `<instset>:analyze_redundancy_by_inst_failure`, and leaves
//! the genotype's genome pointing at that copy. QUIRK: `cInstSet`'s copy
//! constructor and assignment drop the nop map ([`InstSet::cpp_copy`]), so
//! on the copy no instruction is a nop: labels are empty and nop modifiers
//! run as plain instructions. Most genomes then fail to reproduce and are
//! left out of the file. The copy also leaves the stack size and uops per
//! cycle uninitialised, which the experimental, GP8 and BCR hardware read;
//! those hardware types stop the command with an error.
//!
//! [`InstSet::cpp_copy`]: avida_genome::InstSet::cpp_copy

use std::io::Write;
use std::sync::Arc;

use avida_apto::fmt::fmt_g;
use avida_config::string_util as su;
use avida_genome::{InstSet, InstSetRegistry, Instruction};
use avida_hardware::test_cpu::CpuTestInfo;

use crate::analyze::Analyze;
use crate::cmd_mutation::recalc_in_place;
use crate::cmd_population::static_file;
use crate::error::{AnalyzeError, Result};
use crate::host::AnalyzeHost;
use crate::verbose::VERBOSE_ON;

/// The registered set `name`, mutable. The C++ edits it in place; here no
/// hardware holds it between test-CPU runs.
fn registered_mut<'r>(reg: &'r mut InstSetRegistry, name: &str) -> Result<&'r mut InstSet> {
    reg.get_inst_set_mut(name).ok_or_else(|| {
        AnalyzeError::fatal(format!(
            "ANALYZE_REDUNDANCY_BY_INST_FAILURE: instruction set '{name}' is unknown or still \
             shared with live hardware"
        ))
    })
}

/// `GetInst(GetName(j))` for every opcode `j` of `is`. QUIRK: an
/// instruction listed twice resolves to its first opcode each time.
fn by_name(is: &InstSet) -> Result<Vec<Instruction>> {
    (0..is.size())
        .map(|j| is.get_inst(is.name(j)).map_err(AnalyzeError::fatal))
        .collect()
}

impl Analyze {
    /// `ANALYZE_REDUNDANCY_BY_INST_FAILURE [file [replicates [log10_start
    /// [log10_end [log10_step]]]]]`.
    pub(crate) fn command_analyze_redundancy_by_inst_failure(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(
            self.out,
            "Analyzing redundancy by changing instruction failure probability..."
        )?;
        let mut filename = "analyze_redundancy_by_inst_failure.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let mut replicates = 1000;
        if !cur_string.is_empty() {
            replicates = su::as_int(&su::pop_word(&mut cur_string));
        }
        let mut log10_start_pr_fail = -4.0;
        if !cur_string.is_empty() {
            log10_start_pr_fail = su::as_double(&su::pop_word(&mut cur_string));
        }
        let mut log10_end_pr_fail = 0.0;
        if !cur_string.is_empty() {
            log10_end_pr_fail = su::as_double(&su::pop_word(&mut cur_string));
        }
        if log10_end_pr_fail > 0.0 {
            // QUIRK: the end is not actually set to 0.
            writeln!(
                self.err,
                "warning: ANALYZE_REDUNDANCY_BY_INST_FAILURE: End log value greater than 0 set to 0."
            )?;
        }
        let mut log10_step_size_pr_fail = 0.1;
        if !cur_string.is_empty() {
            log10_step_size_pr_fail = su::as_double(&su::pop_word(&mut cur_string));
        }

        {
            let df = static_file(host, &filename)?;
            df.write_comment(
                "Redundancy calculated by changing the probability of instruction failure",
            );
            df.write_comment(&format!(
                "{replicates} replicates at each chance of instruction failure"
            ));
            df.write_time_stamp();
        }

        let bi = self.batch_index(self.cur_batch)?;
        for gi in 0..self.batches[bi].list.len() {
            if Self::verbosity(host) >= VERBOSE_ON {
                writeln!(
                    self.out,
                    "  Determining redundancy by instruction failure for {}",
                    self.batches[bi].list[gi].name
                )?;
            }

            let orig_name = self.batches[bi].list[gi].genome.inst_set().to_string();
            let reg = host.inst_sets_mut()?;
            let original: Arc<InstSet> = reg
                .shared_inst_set(reg.inst_set_id(&orig_name))
                .ok_or_else(|| {
                    AnalyzeError::fatal(format!(
                        "ANALYZE_REDUNDANCY_BY_INST_FAILURE: unknown instruction set \
                             '{orig_name}' (C++: GetInstSet indexes the set array with -1)"
                    ))
                })?;
            if matches!(original.hardware_type(), 3..=5) {
                return Err(AnalyzeError::fatal(format!(
                    "ANALYZE_REDUNDANCY_BY_INST_FAILURE on hardware type {} (C++: the copy of \
                     the instruction set leaves the stack size and uops per cycle \
                     uninitialised, and this hardware reads them)",
                    original.hardware_type()
                )));
            }
            let isname = format!("{orig_name}:analyze_redundancy_by_inst_failure");
            // `RegisterInstSet` fails when the name is taken; the
            // registered set is then reused.
            reg.register_inst_set(&isname, original.cpp_copy());

            // Count the instructions that may fail, and let none fail.
            let modify = registered_mut(reg, &isname)?;
            let mut num_pr_fail_insts = 0;
            for inst in by_name(modify)? {
                if original.prob_fail(inst) > 0.0 {
                    num_pr_fail_insts += 1;
                }
                modify.set_prob_fail(inst, 0.0);
            }
            self.batches[bi].list[gi]
                .genome
                .set_property("instset", &isname);

            if num_pr_fail_insts == 0 {
                return Err(self.exit_with(
                    "ANALYZE_REDUNDANCY_BY_INST_FAILURE: No instructions have a chance of failure in default instruction set.",
                ));
            }

            // `genotype->Recalculate(m_ctx)`: the baseline fitness.
            let recalc = |host: &mut dyn AnalyzeHost, this: &mut Analyze| -> Result<f64> {
                let g = &mut this.batches[bi].list[gi];
                recalc_in_place(host, g, &mut CpuTestInfo::default(), 1)?;
                Ok(g.fitness)
            };
            let baseline_fitness = recalc(host, self)?;
            if baseline_fitness <= 0.0 {
                continue;
            }

            let (name, id) = {
                let g = &self.batches[bi].list[gi];
                (g.name.clone(), g.id_num)
            };
            {
                let df = static_file(host, &filename)?;
                df.write(&name, "genotype name");
                df.write(id, "genotype id");
                df.write(baseline_fitness, "fitness");
            }

            let mut log10_fc = log10_start_pr_fail;
            while log10_fc <= log10_end_pr_fail {
                let fc = (log10_fc * 10.0f64.ln()).exp();

                // `*modify_inst_set = original_inst_set`, then the
                // instructions that may fail fail with probability `fc`.
                let reg = host.inst_sets_mut()?;
                let assigned = registered_mut(reg, &isname)?.cpp_assigned(&original);
                let modify = registered_mut(reg, &isname)?;
                *modify = assigned;
                for inst in by_name(modify)? {
                    if original.prob_fail(inst) > 0.0 {
                        modify.set_prob_fail(inst, fc);
                    }
                }

                let mut avg_fitness = 0.0;
                for _ in 0..replicates {
                    avg_fitness += recalc(host, self)?;
                }

                let df = static_file(host, &filename)?;
                df.write(
                    avg_fitness / f64::from(replicates),
                    &format!("Avg fitness when inst prob fail {}", fmt_g(fc, 3)),
                );
                log10_fc += log10_step_size_pr_fail;
            }
            static_file(host, &filename)?.endl();
        }
        Ok(())
    }
}
