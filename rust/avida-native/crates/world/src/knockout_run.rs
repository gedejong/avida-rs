//! Running the knockout map: the test-CPU side of `knockout.rs`, and the
//! event actions `PrintKnockoutMap` and `PrintReuseData` (port-only
//! extensions, observers; see the definitions in `knockout.rs`).
//!
//! ```text
//! u 0:1000:end PrintKnockoutMap [file=knockout_map.dat]
//! u 0:1000:end PrintReuseData   [file=reuse.dat]
//! ```
//!
//! # Observation only
//!
//! The tests run through [`avida_analyze::world::with_detached_test_world`],
//! as `PrintMODESData`'s skeletons do: a private RNG, null statistics, their
//! own `std::rand` state, no population, and a private copy of the
//! instruction sets in which the NULL instruction is active. The RNG is
//! re-seeded before every test run, so the base organism and every knockout
//! see the same inputs and the same random stream whatever the run did before.
//! The run's environment is only read (the one task that writes it, `xor-max`,
//! is refused). No result is stored on a genotype.
//!
//! # Test conditions
//!
//! The default test CPU: fixed inputs (`CpuTestInfo` does not ask for random
//! ones), the three-generation test of `cTestCPU`, and the resource levels of
//! an unexploited cell as `PrintMODESData` uses them (`RES_CONSTANT`,
//! `World::modes_levels`), so that a task paid from a limited resource whose
//! `initial` is 0 still counts as performed.
//!
//! # Cost
//!
//! One map is `length + 1` test-CPU runs, on the thread that executes the
//! event (between updates).

use std::sync::Arc;

use avida_apto::AvidaRng;
use avida_genome::Genome;
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu, RES_CONSTANT, RES_INITIAL};

use crate::knockout::{call_sites, sites_from, KnockoutMap, TestOutcome};
use crate::modes::TestLevels;
use crate::world::World;

/// The seed of every knockout test run's RNG.
pub const KNOCKOUT_TEST_SEED: i32 = 0x4b_4f_31;

/// The names this module parses.
pub const KNOCKOUT_ACTIONS: &[&str] = &["PrintKnockoutMap", "PrintReuseData"];

/// A parsed action of this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnockoutAction {
    /// `PrintKnockoutMap [string fname="knockout_map.dat"]`
    Map { filename: String },
    /// `PrintReuseData [string fname="reuse.dat"]`
    Reuse { filename: String },
}

/// The constructor of `name` (`None` when it is not an action of this
/// module).
pub fn parse(name: &str, args: &str) -> Option<Result<KnockoutAction, String>> {
    let (default, make): (&str, fn(String) -> KnockoutAction) =
        if name.eq_ignore_ascii_case("PrintKnockoutMap") {
            ("knockout_map.dat", |filename| KnockoutAction::Map {
                filename,
            })
        } else if name.eq_ignore_ascii_case("PrintReuseData") {
            ("reuse.dat", |filename| KnockoutAction::Reuse { filename })
        } else {
            return None;
        };
    let words: Vec<&str> = args.split_whitespace().collect();
    if words.len() > 1 {
        return Some(Err(format!(
            "{name} takes at most one argument, a file name; got {}",
            words.len()
        )));
    }
    Some(Ok(make(words.first().map_or(default, |w| w).to_string())))
}

/// What the actions keep between calls: the map of the last firing, so that
/// both actions firing in one update test the dominant genotype once.
#[derive(Debug, Default)]
pub struct KnockoutState {
    last: Option<(i32, i32, Arc<KnockoutMap>)>,
    /// Maps computed so far (each costs `length + 1` test runs).
    pub maps_computed: u64,
}

impl World {
    /// Whether the knockout tests may run in this world.
    fn knockout_check(&self) -> Result<(), String> {
        let lib = &self.env.tasklib().0;
        if (0..lib.get_size()).any(|i| {
            lib.get_task(i)
                .is_some_and(|t| t.kind() == avida_tasks::TaskKind::XorMax)
        }) {
            return Err(
                "unsupported: the knockout map with the xor-max task (the test CPU would repoint \
                 the run's reaction; C++: cEnvironment::ChangeResource from Task_XorMax)"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// The knockout map of `genome` (a tracker genotype's, `genotype_id`, or
    /// -1), tested as described in the module documentation. Reads the world;
    /// changes nothing the run depends on.
    pub fn knockout_map_of(
        &mut self,
        genome: &Genome,
        genotype_id: i32,
    ) -> Result<KnockoutMap, String> {
        self.knockout_check()?;
        let levels = self.modes_levels();
        let hw = self.modes_private_hw();
        let (null, names, nop_mods, num_regs, has_call, hw_type) = {
            let (_, set) = hw
                .registry()
                .inst_set_for_genome(genome)
                .ok_or("the genome's instruction set is not loaded")?;
            let null =
                crate::modes::null_of(set).ok_or("the instruction set has no NULL instruction")?;
            let len = genome.representation().size().max(0);
            let mut names = Vec::new();
            let mut nop_mods = Vec::new();
            for i in 0..len {
                let inst = genome.representation().get(i);
                names.push(set.name_of(inst).to_string());
                nop_mods.push(set.is_nop(inst).then(|| set.nop_mod(inst)));
            }
            (
                null,
                names,
                nop_mods,
                avida_hardware::hardware_cpu::reg_count(set) as i32,
                set.inst_in_set("call"),
                set.hardware_type(),
            )
        };
        let tasks: Vec<String> = self
            .env
            .tasklib()
            .0
            .tasks()
            .iter()
            .map(|t| t.name().to_string())
            .collect();
        let update = self.stats.update();
        let len = names.len();
        let World {
            cfg,
            env,
            group_attack_counts,
            test_sterilize,
            ..
        } = self;
        let mut rng = AvidaRng::new(KNOCKOUT_TEST_SEED);
        let mut test_genome = genome.clone();
        let mut base: Option<TestOutcome> = None;
        let mut sites = Vec::new();
        avida_analyze::world::with_detached_test_world(
            cfg,
            env,
            &hw,
            group_attack_counts,
            *test_sterilize,
            &mut rng,
            &mut |tw| {
                let mut run = |site: Option<usize>| -> Result<TestOutcome, String> {
                    let original = site.map(|i| {
                        let old = genome.representation().get(i as i32);
                        test_genome.representation_mut().set(i as i32, null);
                        (i, old)
                    });
                    // The same random stream for every test run.
                    *tw.rng = AvidaRng::new(KNOCKOUT_TEST_SEED);
                    let r = TestCpu::new(tw).and_then(|mut cpu| {
                        let mut info = CpuTestInfo::default();
                        match &levels {
                            TestLevels::Constant(h) => {
                                info.set_resource_options(RES_CONSTANT, Some(Arc::clone(h)), 0, 0)
                            }
                            TestLevels::Initial => {
                                info.set_resource_options(RES_INITIAL, None, 0, 0)
                            }
                        }
                        cpu.test_genome(tw, &mut info, &test_genome)?;
                        Ok(TestOutcome {
                            fitness: info.genotype_fitness(),
                            task_counts: info
                                .test_phenotype(0)
                                .map(|p| p.last_task_count.clone())
                                .unwrap_or_default(),
                            inputs: info.test_cpu_inputs().to_vec(),
                        })
                    });
                    if let Some((i, old)) = original {
                        test_genome.representation_mut().set(i as i32, old);
                    }
                    r
                };
                let b = run(None)?;
                if b.viable() {
                    sites = sites_from(&b, len, &mut |i| run(Some(i)))?;
                }
                base = Some(b);
                Ok(())
            },
        )?;
        let base = base.ok_or("the test CPU did not run")?;
        self.actions2.knockout.maps_computed += 1;
        let mut symbols = String::new();
        for i in 0..len {
            genome
                .representation()
                .get(i as i32)
                .push_symbol(&mut symbols);
        }
        let performed = (0..base.task_counts.len())
            .filter(|&t| base.task_counts[t] > 0)
            .collect();
        let calls = (has_call && hw_type == 0).then(|| call_sites(&names, &nop_mods, num_regs));
        Ok(KnockoutMap {
            genotype_id,
            update,
            inst_names: names,
            symbols,
            tasks,
            base_viable: base.viable(),
            base_fitness: base.fitness,
            performed,
            inputs: base.inputs,
            sites,
            calls,
        })
    }

    /// The genome of the organism in `cell` and its genotype id.
    pub fn organism_genome(&self, cell: i32) -> Option<(Genome, i32)> {
        let c = usize::try_from(cell).ok()?;
        let org = self.pop.orgs.get(c)?.as_deref()?;
        let id = self
            .systematics
            .arbiter_for_role("genotype")
            .and_then(|r| self.pop.membership.get(c)?.group(r))
            .and_then(|g| self.systematics.group_id(g))
            .unwrap_or(-1);
        Some((org.state.genome().clone(), id))
    }

    /// The map of the genome of organism `org_id` in `cell`, for the front
    /// ends: an error when the organism is no longer there.
    pub fn knockout_map_of_cell(&mut self, cell: i32, org_id: i32) -> Result<KnockoutMap, String> {
        let here = usize::try_from(cell)
            .ok()
            .and_then(|c| self.pop.orgs.get(c))
            .and_then(|o| o.as_deref())
            .map(|o| o.state.id);
        if here != Some(org_id) {
            return Err(format!("organism #{org_id} is no longer in cell {cell}"));
        }
        let (genome, id) = self
            .organism_genome(cell)
            .ok_or_else(|| format!("no organism in cell {cell}"))?;
        self.knockout_map_of(&genome, id)
    }

    /// The map of the dominant genotype at this update, computed once per
    /// update and genotype.
    fn dominant_knockout_map(&mut self) -> Result<Option<Arc<KnockoutMap>>, String> {
        let arb = self
            .systematics
            .genotypes()
            .ok_or("the knockout actions need the genotype tracker (no genotype arbiter)")?;
        if arb.disable_class() {
            return Err("the knockout actions need genotype classification \
                 (DISABLE_GENOTYPE_CLASSIFICATION 0)"
                .to_string());
        }
        let Some(g) = arb.dominant().and_then(|h| arb.genotype(h)) else {
            return Ok(None);
        };
        let (id, genome) = (g.id(), g.genome().clone());
        let update = self.stats.update();
        if let Some((u, i, m)) = &self.actions2.knockout.last {
            if *u == update && *i == id {
                return Ok(Some(Arc::clone(m)));
            }
        }
        let map = Arc::new(self.knockout_map_of(&genome, id)?);
        self.actions2.knockout.last = Some((update, id, Arc::clone(&map)));
        Ok(Some(map))
    }

    /// `PrintKnockoutMap` and `PrintReuseData`, once per event firing.
    pub(crate) fn run_knockout_action(&mut self, a: &KnockoutAction) -> Result<(), String> {
        let Some(map) = self.dominant_knockout_map()? else {
            return Ok(());
        };
        match a {
            KnockoutAction::Map { filename } => {
                let df = self.out.static_with_path(filename)?;
                df.write_raw("");
                for l in map.file_lines() {
                    df.write_raw(&l);
                }
                df.flush();
            }
            KnockoutAction::Reuse { filename } => {
                let m = map.measures();
                let df = self.out.static_with_path(filename)?;
                df.write_comment("Code reuse of the dominant genotype from single-site knockouts.");
                df.write_comment(
                    "An extension of this port (the C++ program has no PrintReuseData); an \
                     observer, it does not change the run. Definitions: ARCHITECTURE.md, \
                     'Knockout map and reuse measures'.",
                );
                df.write_comment(
                    "A knockout replaces one site by the NULL instruction and runs the genome \
                     in the test CPU. A task is lost when the base performs it and the \
                     knockout does not; lethal sites (no replication) are left out of the task \
                     sets.",
                );
                df.write_comment(
                    "-1 means not defined: shared sites and reuse index need two performed \
                     tasks (the index also one defined pair); call columns need `call` in the \
                     instruction set.",
                );
                df.write(map.update, "Update");
                df.write(map.genotype_id, "Genotype id");
                df.write(map.symbols.chars().count() as i32, "Genome length");
                df.write(m.tasks_performed as i32, "Tasks performed");
                df.write(
                    m.shared_sites.map_or(-1, |n| n as i32),
                    "Shared sites (knockout loses two or more tasks)",
                );
                df.write(
                    m.reuse_index.unwrap_or(-1.0),
                    "Reuse index (mean pairwise Jaccard overlap of the task site sets)",
                );
                df.write(m.calls.map_or(-1, |c| c.call_sites as i32), "Call sites");
                df.write(
                    m.calls.map_or(-1, |c| c.non_neutral as i32),
                    "Non-neutral call sites",
                );
                df.write(
                    m.calls.map_or(-1, |c| c.distinct_targets as i32),
                    "Distinct call targets",
                );
                df.endl();
            }
        }
        Ok(())
    }
}
