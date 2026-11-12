//! `PRECALC_PHENOTYPE` (avida-core/source/main/cPopulation.cc at 47f13dadb):
//! run an organism's memory on a test CPU, with the inputs its cell will
//! hand it, and take merit, gestation time and (for a parent) instruction
//! counts from the test phenotype.
//!
//! Two places do this: `cPopulation::ActivateOrganism`, for a newly placed
//! organism before it is scheduled, and `cPopulation::ActivateOffspring`
//! when `RESET_INPUTS_ON_DIVIDE > 0`, for the dividing parent once its cell
//! has fresh inputs. The parent is the running organism there, so its
//! memory comes from the hardware that lends it to the divide
//! (`OrgInterface::divide_with_traces`).

use avida_genome::{Genome, InstructionSequence};
use avida_hardware::hardware::Hardware;
use avida_hardware::organism::OrgState;
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu};
use avida_phenotype::Merit;

use crate::world::{make_test_world, World};

/// What the callers take from the test phenotype.
struct TestedPhenotype {
    merit: Merit,
    gestation_time: i32,
    last_inst_count: Vec<i32>,
}

impl World {
    /// `TestGenome(ctx, test_info, mg)` on a fresh test CPU with
    /// `test_info.UseManualInputs(inputs)`, where `mg` is `genome`'s hardware
    /// type and properties around the sequence `memory`.
    fn precalc_test(
        &mut self,
        genome: &Genome,
        memory: InstructionSequence,
        inputs: Vec<i32>,
    ) -> Result<TestedPhenotype, String> {
        let mg = Genome::new(genome.hardware_type(), &genome.properties(), memory);
        let World {
            cfg,
            env,
            rng,
            hw,
            stats,
            host,
            group_attack_fn,
            test_sterilize,
            pop,
            ..
        } = self;
        let mut tw = make_test_world(
            cfg,
            env,
            rng,
            hw,
            stats,
            host,
            &**group_attack_fn,
            *test_sterilize,
            pop,
        );
        let mut test_info = CpuTestInfo::default();
        let mut test_cpu = TestCpu::new(&mut tw)?;
        // Test using what the environment will be
        test_info.use_manual_inputs(inputs);
        // Use the true genome
        test_cpu.test_genome(&mut tw, &mut test_info, &mg)?;
        match test_info.test_phenotype(0) {
            Some(ph) => Ok(TestedPhenotype {
                merit: ph.merit,
                gestation_time: ph.gestation_time,
                last_inst_count: ph.last_inst_count.clone(),
            }),
            None => Err("PRECALC_PHENOTYPE: test CPU without a phenotype".to_string()),
        }
    }

    /// The `PRECALC_PHENOTYPE` block of `ActivateOrganism` for the organism
    /// just placed in cell `t` (its inputs already set up).
    pub(crate) fn precalc_phenotype(&mut self, t: usize) -> Result<(), String> {
        let pc_phenotype = self.cfg.precalc_phenotype;
        if pc_phenotype == 0 {
            return Ok(());
        }
        let Some(org) = self.pop.orgs[t].as_ref() else {
            return Ok(());
        };
        let genome = org.state.genome().clone();
        let memory = org.hardware.memory().to_sequence();
        let inputs = self.pop.cells[t].inputs.clone();
        let tested = self.precalc_test(&genome, memory, inputs)?;

        let Some(org) = self.pop.orgs[t].as_mut() else {
            return Ok(());
        };
        let ph = &mut org.state.phenotype;
        if pc_phenotype & 1 != 0 {
            ph.merit = tested.merit;
        }
        if pc_phenotype & 2 != 0 {
            ph.gestation_time = tested.gestation_time;
        }
        ph.fitness = ph.merit.calc_fitness(ph.gestation_time);
        Ok(())
    }

    /// The `PRECALC_PHENOTYPE` block of `ActivateOffspring` (with
    /// `RESET_INPUTS_ON_DIVIDE > 0`, after the parent's cell got new
    /// inputs): the dividing `parent`, whose hardware is `parent_hw`, in
    /// cell `pc`.
    pub(crate) fn precalc_parent_phenotype(
        &mut self,
        parent: &mut OrgState,
        parent_hw: &dyn Hardware,
        pc: usize,
    ) -> Result<(), String> {
        let pc_phenotype = self.cfg.precalc_phenotype;
        if pc_phenotype == 0 {
            return Ok(());
        }
        let inputs = self.pop.cells[pc].inputs.clone();
        let tested =
            self.precalc_test(parent.genome(), parent_hw.memory().to_sequence(), inputs)?;
        let ph = &mut parent.phenotype;
        if pc_phenotype & 1 != 0 {
            // If we must update the merit
            ph.merit = tested.merit;
        }
        if pc_phenotype & 2 != 0 {
            // If we must update the gestation time
            ph.gestation_time = tested.gestation_time;
        }
        if pc_phenotype & 4 != 0 {
            // If we must update the last instruction counts
            ph.test_cpu_inst_count = tested.last_inst_count;
        }
        // Update fitness
        ph.fitness = ph.merit.calc_fitness(ph.gestation_time);
        Ok(())
    }
}
