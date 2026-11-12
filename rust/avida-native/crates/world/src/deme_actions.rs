//! Deme event actions: the constructor (argument parsing) and `Process`
//! halves of the deme-related `cAction*` classes at 47f13dadb:
//!
//! * PopulationActions.cc: `cActionInjectDemes`, `cActionReplicateDemes`,
//!   `cActionReplicateDeme`, `cActionSetMigrationRate`;
//! * EnvironmentActions.cc: `cActionSetMigrationMatrix`,
//!   `cActionMergeResourceAcrossDemes`;
//! * PrintActions.cc: `cActionPrintMigrationMatrix`,
//!   `cActionPrintDemeResources` (registered as `PrintDemeResourceStats`),
//!   `cActionPrintDemeGlobalResources`, with the cPopulation.cc
//!   `PrintDemeResource` / `PrintDemeGlobalResources` they call;
//! * PopulationActions.cc: `cActionUpdateDemeParasiteMemoryScores`,
//!   `cActionKillDemesHighestParasiteLoad`,
//!   `cActionReplicateDemesHighestBirthCount`,
//!   `cActionReplicateDemesHighestFecundity`;
//! * SaveLoadActions.cc: `cActionSaveGermlines`, `cActionLoadGermlines`,
//!   `cActionLoadBirthCounts`, `cActionLoadParasiteMemoryScores`;
//! * DriverActions.cc: `cActionExitDemeReplications`;
//! * EnvironmentActions.cc: `cActionSetEnvironmentInputs`, which is not a
//!   deme action but is what the deme task-switching test sets up with;
//! * the cStats.cc deme replication printers (`PrintDemeReplicationData`,
//!   `PrintDemeGermlineSequestration`), which read [`DemeReplStats`].
//!
//! [`DemeReplStats`]: crate::pop_demes::DemeReplStats

use avida_config::feedback::Feedback;
use avida_config::string_util::{as_double, as_int, pop_word};

use avida_apto::fmt::{fmt_g, fmt_g_showpoint};
use avida_config::init_file::InitFile;
use avida_genome::genome::{setup_property_map, PropertyMap};
use avida_genome::{ArgContainer, ArgSchema, Genome, InstructionSequence};

use crate::driver::verbosity;
use crate::pop_demes::deme_trigger;
use crate::world::World;

/// Deme action names (the registered spellings).
pub const DEME_ACTIONS: &[&str] = &[
    "InjectDemes",
    "ReplicateDemes",
    "ReplicateDeme",
    "SetMigrationRate",
    "SetMigrationMatrix",
    "PrintMigrationMatrix",
    "MergeResourceAcrossDemes",
    "PrintDemeResourceStats",
    "PrintDemeGlobalResources",
    "ExitDemeReplications",
    "SetEnvironmentInputs",
    "UpdateDemeParasiteMemoryScores",
    "KillDemesHighestParasiteLoad",
    "ReplicateDemesHighestBirthCount",
    "ReplicateDemesHighestFecundity",
    "SaveGermlines",
    "LoadGermlines",
    "LoadBirthCounts",
    "LoadParasiteMemoryScores",
];

/// A parsed deme action. Field names follow the C++ members (`m_` dropped).
#[derive(Debug, Clone, PartialEq)]
pub enum DemeAction {
    /// `cActionInjectDemes`
    InjectDemes {
        filename: String,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionReplicateDemes`: one of the [`deme_trigger`] values.
    ReplicateDemes { rep_trigger: i32 },
    /// `cActionReplicateDeme`
    ReplicateDeme { id: i32 },
    /// `cActionSetMigrationRate`
    SetMigrationRate { rate: f64 },
    /// `cActionSetMigrationMatrix`
    SetMigrationMatrix { fname: String },
    /// `cActionPrintMigrationMatrix`
    PrintMigrationMatrix { filename: String },
    /// `cActionMergeResourceAcrossDemes`
    MergeResourceAcrossDemes {
        deme_res_name: String,
        global_res_name: String,
    },
    /// `cActionPrintDemeResources`
    PrintDemeResourceStats,
    /// `cActionPrintDemeGlobalResources`
    PrintDemeGlobalResources,
    /// `cActionExitDemeReplications`
    ExitDemeReplications { deme_rep: i32 },
    /// `cActionSetEnvironmentInputs`
    SetEnvironmentInputs { inputs: Vec<i32> },
    /// `cActionUpdateDemeParasiteMemoryScores`
    UpdateDemeParasiteMemoryScores { decay_rate: f64, verbose: bool },
    /// `cActionKillDemesHighestParasiteLoad`
    KillDemesHighestParasiteLoad { killprob: f64, killmax: i32 },
    /// `cActionReplicateDemesHighestBirthCount` (`by_fecundity == false`)
    /// and `cActionReplicateDemesHighestFecundity`
    ReplicateDemesHighest {
        replprob: f64,
        replmax: i32,
        by_fecundity: bool,
    },
    /// `cActionSaveGermlines`
    SaveGermlines {
        filename: String,
        birthcounts: bool,
        parasitememoryscores: bool,
    },
    /// `cActionLoadGermlines`, `cActionLoadBirthCounts`,
    /// `cActionLoadParasiteMemoryScores`
    LoadDemeData {
        what: DemeDataKind,
        filename: String,
        verbose: bool,
    },
}

/// What a `Load*` deme action reads from a `.sgerm` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemeDataKind {
    /// `LoadGermlines`
    Germlines,
    /// `LoadBirthCounts`
    BirthCounts,
    /// `LoadParasiteMemoryScores`
    ParasiteMemoryScores,
}

/// `if (largs.GetSize()) x = largs.PopWord()...`
struct Args(String);

impl Args {
    fn word(&mut self) -> Option<String> {
        if self.0.is_empty() {
            None
        } else {
            Some(pop_word(&mut self.0))
        }
    }
    fn int(&mut self, def: i32) -> i32 {
        self.word().map_or(def, |w| as_int(&w))
    }
    fn double(&mut self, def: f64) -> f64 {
        self.word().map_or(def, |w| as_double(&w))
    }
    fn string(&mut self, def: &str) -> String {
        self.word().unwrap_or_else(|| def.to_string())
    }
}

/// `cActionReplicateDemes` trigger names.
fn replication_trigger(name: &str) -> Option<i32> {
    use deme_trigger::*;
    Some(match name {
        "all" => ALL,
        "full_deme" => FULL,
        "corners" => CORNERS,
        "deme-age" => AGE,
        "birth-count" => BIRTHS,
        "sat-mov-pred" => MOVE_PREDATORS,
        "events-killed" => GROUP_KILL,
        "sat-msg-pred" => MESSAGE_PREDATORS,
        "sat-deme-predicate" => PREDICATE,
        "perf-reactions" => PERFECT_REACTIONS,
        "consume-res" => CONSUME_RESOURCES,
        _ => return None,
    })
}

/// The constructor of the deme action `name` (a [`DEME_ACTIONS`] spelling)
/// applied to `args`; `None` for any other name. `Err` for a constructor
/// that ends the C++ run.
pub fn parse(
    name: &str,
    args: &str,
    feedback: &mut Feedback,
) -> Option<Result<DemeAction, String>> {
    let mut a = Args(args.to_string());
    Some(Ok(match name {
        "InjectDemes" => DemeAction::InjectDemes {
            filename: a.string(""),
            merit: a.double(-1.0),
            lineage_label: a.int(0),
            neutral_metric: a.double(0.0),
        },
        "ReplicateDemes" => {
            let in_trigger = a.string("full_deme");
            let rep_trigger = replication_trigger(&in_trigger).unwrap_or_else(|| {
                feedback.error(format!(
                    "Unknown replication trigger '{in_trigger}' in ReplicatDemes action."
                ));
                deme_trigger::UNKNOWN
            });
            DemeAction::ReplicateDemes { rep_trigger }
        }
        "ReplicateDeme" => DemeAction::ReplicateDeme { id: a.int(0) },
        "SetMigrationRate" => DemeAction::SetMigrationRate {
            rate: a.double(0.0),
        },
        "SetMigrationMatrix" => DemeAction::SetMigrationMatrix {
            fname: a.string(""),
        },
        "PrintMigrationMatrix" => DemeAction::PrintMigrationMatrix {
            filename: a.string(""),
        },
        "MergeResourceAcrossDemes" => DemeAction::MergeResourceAcrossDemes {
            deme_res_name: a.string(""),
            global_res_name: a.string(""),
        },
        "PrintDemeResourceStats" => DemeAction::PrintDemeResourceStats,
        "PrintDemeGlobalResources" => DemeAction::PrintDemeGlobalResources,
        "ExitDemeReplications" => match a.word() {
            Some(w) => DemeAction::ExitDemeReplications {
                deme_rep: as_int(&w),
            },
            None => {
                return Some(Err(
                    "unsupported: ExitDemeReplications without a count (C++: \
                     Feedback().Error and Abort through the driver, which is not \
                     attached while the event file loads)"
                        .to_string(),
                ))
            }
        },
        "SetEnvironmentInputs" => {
            let mut inputs = Vec::new();
            for _ in 0..3 {
                if let Some(w) = a.word() {
                    inputs.push(as_int(&w));
                }
            }
            // The C++ prints to `cerr` and calls `exit(1)` on either check.
            if inputs.len() != 3 {
                return Some(Err("unsupported: SetEnvironmentInputs without exactly 3 \
                     inputs (C++: exit(1) while the event file loads)"
                    .to_string()));
            }
            if inputs[0] >> 24 != 15 || inputs[1] >> 24 != 51 || inputs[2] >> 24 != 85 {
                return Some(Err(format!(
                    "unsupported: SetEnvironmentInputs inputs {} {} {} that do not begin \
                     0F, 33, 55 (C++: exit(1) while the event file loads)",
                    inputs[0], inputs[1], inputs[2]
                )));
            }
            DemeAction::SetEnvironmentInputs { inputs }
        }
        // Both words are popped unconditionally (an empty word reads as 0).
        "UpdateDemeParasiteMemoryScores" => DemeAction::UpdateDemeParasiteMemoryScores {
            decay_rate: as_double(&pop_word(&mut a.0)),
            verbose: as_int(&pop_word(&mut a.0)) != 0,
        },
        "KillDemesHighestParasiteLoad" => DemeAction::KillDemesHighestParasiteLoad {
            killprob: a.double(0.01),
            killmax: a.int(i32::MAX),
        },
        "ReplicateDemesHighestBirthCount" | "ReplicateDemesHighestFecundity" => {
            DemeAction::ReplicateDemesHighest {
                replprob: a.double(0.01),
                replmax: a.int(i32::MAX),
                by_fecundity: name == "ReplicateDemesHighestFecundity",
            }
        }
        "SaveGermlines" => {
            let mut schema = ArgSchema::new(':', '=', false);
            schema.add_string("filename", 0, "detailgermlines");
            schema.add_int_range_default("birthcounts", 0, 0, 1, 0);
            schema.add_int_range_default("parasitememoryscores", 1, 0, 1, 0);
            // A failed load keeps the member defaults.
            match ArgContainer::load(args, &schema, feedback) {
                Some(c) => DemeAction::SaveGermlines {
                    filename: c.get_string(0).to_string(),
                    birthcounts: c.get_int(0) != 0,
                    parasitememoryscores: c.get_int(1) != 0,
                },
                None => DemeAction::SaveGermlines {
                    filename: String::new(),
                    birthcounts: false,
                    parasitememoryscores: false,
                },
            }
        }
        "LoadGermlines" | "LoadBirthCounts" | "LoadParasiteMemoryScores" => {
            DemeAction::LoadDemeData {
                what: match name {
                    "LoadGermlines" => DemeDataKind::Germlines,
                    "LoadBirthCounts" => DemeDataKind::BirthCounts,
                    _ => DemeDataKind::ParasiteMemoryScores,
                },
                filename: a.string(""),
                verbose: a.int(0) != 0,
            }
        }
        _ => return None,
    }))
}

impl World {
    /// `Process(ctx)` of a deme action.
    pub(crate) fn run_deme_action(&mut self, action: &DemeAction) -> Result<(), String> {
        match action {
            DemeAction::InjectDemes {
                filename,
                merit,
                lineage_label,
                neutral_metric,
            } => self.inject_demes(filename, *merit, *lineage_label, *neutral_metric),
            DemeAction::ReplicateDemes { rep_trigger } => {
                self.replicate_demes(*rep_trigger);
                Ok(())
            }
            DemeAction::ReplicateDeme { id } => {
                // `GetDeme(m_id)`: an unchecked array index in C++.
                let d = usize::try_from(*id)
                    .ok()
                    .filter(|&d| d < self.pop.demes.len())
                    .ok_or_else(|| {
                        format!("ReplicateDeme: deme {id} out of range (C++: out-of-bounds deme)")
                    })?;
                self.replicate_deme(d);
                Ok(())
            }
            DemeAction::SetMigrationRate { rate } => {
                self.cfg.migration_rate = *rate;
                Ok(())
            }
            DemeAction::SetMigrationMatrix { fname } => {
                let count_parasites = self.cfg.demes_parasite_migration_rate > 0.0;
                let count_offspring = self.cfg.demes_migration_rate > 0.0;
                let num_demes = self.pop.demes.len() as i32;
                // The feedback is discarded and the result only `assert`ed,
                // which the -DNDEBUG reference build compiles out.
                let mut feedback = Feedback::new();
                let _ = self.pop.migration_matrix.load(
                    num_demes,
                    fname,
                    &self.working_dir,
                    count_parasites,
                    count_offspring,
                    true,
                    &mut feedback,
                );
                Ok(())
            }
            DemeAction::PrintMigrationMatrix { filename } => {
                let filename = if filename.is_empty() {
                    format!("migration_matrix.{}.dat", self.stats.update())
                } else {
                    filename.clone()
                };
                let mut df = self.out.create_with_path(&filename)?;
                let text = self.pop.migration_matrix.print();
                df.ofstream()
                    .write_all(text.as_bytes())
                    .map_err(|e| format!("{filename}: {e}"))?;
                Ok(())
            }
            DemeAction::MergeResourceAcrossDemes {
                deme_res_name,
                global_res_name,
            } => {
                self.merge_resource_across_demes(deme_res_name, global_res_name);
                Ok(())
            }
            DemeAction::PrintDemeResourceStats => self.print_deme_resource(),
            DemeAction::PrintDemeGlobalResources => self.print_deme_global_resources(),
            DemeAction::ExitDemeReplications { deme_rep } => {
                // `GetNumDemeReplications()` is the running total.
                if self.pop.deme_repl_stats.total_deme_num_repls >= *deme_rep {
                    // `GetDriver().Finish()`
                    self.done = true;
                }
                Ok(())
            }
            DemeAction::SetEnvironmentInputs { inputs } => {
                self.set_environment_inputs(inputs);
                Ok(())
            }
            DemeAction::UpdateDemeParasiteMemoryScores {
                decay_rate,
                verbose,
            } => {
                let mut line = String::new();
                for d in 0..self.pop.demes.len() {
                    let load = self.deme_parasite_load(d);
                    let deme = &mut self.pop.demes[d];
                    deme.update_parasite_memory_score(load, *decay_rate);
                    let score = deme.parasite_memory_score;
                    if *verbose {
                        line.push_str(&self.cout_double(score));
                        line.push(' ');
                    }
                }
                if *verbose {
                    line.push('\n');
                    self.action_state.stdout.push_str(&line);
                }
                Ok(())
            }
            DemeAction::KillDemesHighestParasiteLoad { killprob, killmax } => {
                for (begin, end) in self.deme_partitions("KillDemesHighestParasiteLoad")? {
                    self.kill_partition_highest_parasite_load(begin, end, *killprob, *killmax)?;
                }
                Ok(())
            }
            DemeAction::ReplicateDemesHighest {
                replprob,
                replmax,
                by_fecundity,
            } => {
                let name = if *by_fecundity {
                    "ReplicateDemesHighestFecundity"
                } else {
                    "ReplicateDemesHighestBirthCount"
                };
                for (begin, end) in self.deme_partitions(name)? {
                    self.replicate_partition_highest(
                        begin,
                        end,
                        *replprob,
                        *replmax,
                        *by_fecundity,
                    )?;
                    if self.fatal_message().is_some() {
                        break;
                    }
                }
                Ok(())
            }
            DemeAction::SaveGermlines {
                filename,
                birthcounts,
                parasitememoryscores,
            } => self.save_germlines(filename, *birthcounts, *parasitememoryscores),
            DemeAction::LoadDemeData {
                what,
                filename,
                verbose,
            } => self.load_deme_data(*what, filename, *verbose),
        }
    }

    /// The `[begin, end)` deme ranges `Process` hands `ProcessPartition`:
    /// one per `DEMES_PARTITION_INTERVAL` demes, or all demes.
    fn deme_partitions(&self, action: &str) -> Result<Vec<(i32, i32)>, String> {
        let part_size = self.cfg.demes_partition_interval;
        let num_demes = self.pop.demes.len() as i32;
        if part_size < 0 {
            return Err(format!(
                "unsupported: {action} with DEMES_PARTITION_INTERVAL {part_size} \
                 (C++: endless loop)"
            ));
        }
        if part_size == 0 {
            return Ok(vec![(0, num_demes)]);
        }
        let mut parts = Vec::new();
        let mut i = 0;
        while i < num_demes {
            parts.push((i, (i + part_size).min(num_demes)));
            i += part_size;
        }
        Ok(parts)
    }

    /// `ProcessPartition(begin, end)` of `cActionReplicateDemesHighestBirthCount`
    /// and `cActionReplicateDemesHighestFecundity`: replicate the
    /// `min(Binomial(demes, replprob), replmax)` demes with the most births
    /// (or the most organisms, ties broken by births).
    fn replicate_partition_highest(
        &mut self,
        begin: i32,
        end: i32,
        replprob: f64,
        replmax: i32,
        by_fecundity: bool,
    ) -> Result<(), String> {
        let num_demes = end - begin;
        let mut deme_indices: Vec<i32> = (begin..end).collect();
        let binomial_draw = self.rng.get_rand_binomial(f64::from(num_demes), replprob) as i32;
        let repl_quota = binomial_draw.min(replmax);
        if repl_quota == 0 {
            return Ok(());
        }
        if repl_quota != binomial_draw {
            let line = format!(
                "warning: capped repl quota at {repl_quota} from {binomial_draw} binomial \
                 sample with {num_demes} eligible and repl prob {}\n",
                self.cout_double(replprob)
            );
            self.action_state.stdout.push_str(&line);
        }
        let Ok(quota) = usize::try_from(repl_quota) else {
            return Err(format!(
                "unsupported: deme replication quota {repl_quota} (C++: std::partial_sort \
                 past the range)"
            ));
        };
        // Births are compared as doubles; fecundity is the pair
        // (organisms, births) compared lexicographically.
        let keys: Vec<(i32, f64)> = deme_indices
            .iter()
            .map(|&d| {
                let deme = &self.pop.demes[d as usize];
                if by_fecundity {
                    (deme.cur_org_count, f64::from(deme.cur_birth_count))
                } else {
                    (0, f64::from(deme.cur_birth_count))
                }
            })
            .collect();
        libcxx_partial_sort(&mut deme_indices, quota, |&d1, &d2| {
            let (a, b) = (keys[(d1 - begin) as usize], keys[(d2 - begin) as usize]);
            a.0 > b.0 || (a.0 == b.0 && a.1 > b.1)
        });
        for &d in &deme_indices[..quota] {
            self.replicate_deme(d as usize);
            if self.fatal_message().is_some() {
                break;
            }
        }
        Ok(())
    }

    /// `cActionSaveGermlines::Process`: the latest germline genome of every
    /// deme that has one, to `<filename>-<update>.sgerm`.
    fn save_germlines(
        &mut self,
        filename: &str,
        birthcounts: bool,
        parasitememoryscores: bool,
    ) -> Result<(), String> {
        let update = self.stats.update();
        let mut df = self
            .out
            .create_with_path(&format!("{filename}-{update}.sgerm"))?;
        df.set_file_type("germline_data");
        df.write_comment("Mode 1 Germline Save");
        df.write_time_stamp();
        for (i, deme) in self.pop.demes.iter().enumerate() {
            let Some(genome) = deme.germline.last() else {
                continue;
            };
            df.write_with_format(i as i32, "Deme ID", "deme_id");
            for (value, descr, name) in genome.legacy_save_columns() {
                match value {
                    avida_genome::genome::LegacyValue::Int(v) => {
                        df.write_with_format(v, descr, name)
                    }
                    avida_genome::genome::LegacyValue::Str(v) => {
                        df.write_with_format(v, descr, name)
                    }
                }
            }
            if birthcounts {
                df.write_with_format(deme.cur_birth_count, "Deme Birth Count", "deme_birth_count");
            }
            if parasitememoryscores {
                df.write_with_format(
                    deme.parasite_memory_score,
                    "Deme Parasite Memory Score",
                    "deme_parasite_memory_score",
                );
            }
            df.endl();
        }
        Ok(())
    }

    /// `Process` of `cActionLoadGermlines`, `cActionLoadBirthCounts` and
    /// `cActionLoadParasiteMemoryScores`: one `.sgerm` line per deme.
    fn load_deme_data(
        &mut self,
        what: DemeDataKind,
        filename: &str,
        verbose: bool,
    ) -> Result<(), String> {
        let action = match what {
            DemeDataKind::Germlines => "LoadGermlines",
            DemeDataKind::BirthCounts => "LoadBirthCounts",
            DemeDataKind::ParasiteMemoryScores => "LoadParasiteMemoryScores",
        };
        let input = InitFile::new(filename, &self.working_dir, None, None);
        // `ctx.Driver().Feedback()` prints with `printf`; the
        // `assert(WasOpened())` is compiled out, leaving zero lines.
        let mut out = String::new();
        for line in input.feedback().render().lines() {
            out.push_str(line);
            out.push('\n');
        }
        if verbose {
            out.push_str(&format!(
                "{action} input file {filename} has {} lines.\n",
                input.num_lines()
            ));
        }
        let mut result = Ok(());
        for line_id in 0..input.num_lines() {
            let props = input.get_line_as_dict(line_id as i32);
            let field = |k: &str| props.get(k).map_or("", String::as_str).to_string();
            // `Apto::StrAs(file_props->Get("deme_id"))`
            let deme_id = as_int(&field("deme_id"));
            let Some(d) = usize::try_from(deme_id)
                .ok()
                .filter(|&d| d < self.pop.demes.len())
            else {
                result = Err(format!(
                    "{action}: deme {deme_id} out of range (C++: out-of-bounds deme)"
                ));
                break;
            };
            match what {
                DemeDataKind::Germlines => {
                    let sequence = field("sequence");
                    if verbose {
                        out.push_str(&format!(
                            "LoadGermlines adding sequence {sequence} to deme {deme_id} \
                             germline...\n"
                        ));
                    }
                    let is = self
                        .hw
                        .registry()
                        .shared_inst_set(0)
                        .ok_or("LoadGermlines: no instruction set")?;
                    let mut props = PropertyMap::new();
                    setup_property_map(&mut props, is.inst_set_name());
                    if verbose {
                        out.push_str(&format!(
                            "LoadGermlines hardware type is {} and instruction set is {}.\n",
                            is.hardware_type(),
                            is.inst_set_name()
                        ));
                    }
                    let genome = Genome::new(
                        is.hardware_type(),
                        &props,
                        InstructionSequence::from_string(&sequence),
                    );
                    if verbose {
                        out.push_str(&format!(
                            "Loaded Genome string representation is `{}`.\n",
                            genome.as_string()
                        ));
                    }
                    let germline = &mut self.pop.demes[d].germline;
                    if verbose {
                        out.push_str(&format!(
                            "LoadGermlines deme {deme_id} germline size before add was {}.\n",
                            germline.len()
                        ));
                    }
                    germline.push(genome);
                    if verbose {
                        out.push_str(&format!(
                            "LoadGermlines deme {deme_id} germline size after add is {}.\n",
                            germline.len()
                        ));
                    }
                }
                DemeDataKind::BirthCounts => {
                    let count = as_int(&field("deme_birth_count"));
                    if verbose {
                        out.push_str(&format!(
                            "LoadBirthCounts deme {deme_id} has {count} births.\n"
                        ));
                    }
                    self.pop.demes[d].cur_birth_count = count;
                    out.push_str(&format!(
                        "LoadBirthCounts set deme {deme_id} birth counts to {count}.\n"
                    ));
                }
                DemeDataKind::ParasiteMemoryScores => {
                    let score = as_double(&field("deme_parasite_memory_score"));
                    if verbose {
                        out.push_str(&format!(
                            "LoadParasiteMemoryScores deme {deme_id} has {score:.6} score.\n"
                        ));
                    }
                    self.pop.demes[d].parasite_memory_score = score;
                    out.push_str(&format!(
                        "LoadParasiteMemoryScores set deme {deme_id} score to {score:.6}.\n"
                    ));
                }
            }
        }
        self.action_state.stdout.push_str(&out);
        result
    }

    /// `std::cout << v` from an action. `Avida2Driver::Run` sets
    /// `ios::showpoint` and a sticky `setprecision` while it prints each
    /// status line, so once one has been printed doubles come out as
    /// `%#.7g`, or `%#.4g` when the debug columns ran last; before that, as
    /// the default `%g`. Events run before the update is incremented, so
    /// the first status line ("UD: 0") precedes the events of update 0.
    pub(crate) fn cout_double(&self, v: f64) -> String {
        if self.stats.update() >= 0 && self.cfg.verbosity > verbosity::SILENT {
            let precision = if self.cfg.verbosity >= verbosity::DEBUG {
                4
            } else {
                7
            };
            fmt_g_showpoint(v, precision)
        } else {
            fmt_g(v, 6)
        }
    }

    /// `cDeme::GetParasiteLoad()`: parasites per organism.
    fn deme_parasite_load(&self, d: usize) -> f64 {
        let deme = &self.pop.demes[d];
        if deme.cur_org_count == 0 {
            return 0.0;
        }
        // `GetNumParasites()`
        let count: i32 = (0..deme.size())
            .map(|pos| self.num_parasites_in_cell(deme.cell_id(pos)))
            .sum();
        f64::from(count) / f64::from(deme.cur_org_count)
    }

    /// `cActionKillDemesHighestParasiteLoad::ProcessPartition(begin, end)`:
    /// kill every organism in the `min(killmax, Binomial(eligible,
    /// killprob))` demes with the highest parasite load, where `eligible`
    /// counts the non-empty demes of the partition.
    fn kill_partition_highest_parasite_load(
        &mut self,
        begin: i32,
        end: i32,
        killprob: f64,
        killmax: i32,
    ) -> Result<(), String> {
        let mut deme_indices: Vec<i32> = (begin..end).collect();
        let num_eligible = deme_indices
            .iter()
            .filter(|&&d| !self.pop.demes[d as usize].is_empty())
            .count() as i32;
        let binomial_draw =
            self.rng
                .get_rand_binomial(f64::from(num_eligible), killprob) as i32;
        let kill_quota = killmax.min(binomial_draw);
        if kill_quota != binomial_draw {
            let line = format!(
                "warning: capped kill quota at {kill_quota} from {binomial_draw} binomial \
                 sample with {num_eligible} eligible and kill prob {}\n",
                self.cout_double(killprob)
            );
            self.action_state.stdout.push_str(&line);
        }
        let Ok(quota) = usize::try_from(kill_quota) else {
            return Err(format!(
                "unsupported: KillDemesHighestParasiteLoad with kill quota {kill_quota} \
                 (C++: std::partial_sort past the range)"
            ));
        };
        let parasite_loads: Vec<f64> = deme_indices
            .iter()
            .map(|&d| self.deme_parasite_load(d as usize))
            .collect();
        // QUIRK: `std::partial_sort` is not stable, so which of several
        // equally loaded demes die follows libc++'s heap selection.
        libcxx_partial_sort(&mut deme_indices, quota, |&d1, &d2| {
            parasite_loads[(d1 - begin) as usize] > parasite_loads[(d2 - begin) as usize]
        });
        for &d in &deme_indices[..quota] {
            self.deme_kill_all(d as usize);
        }
        Ok(())
    }

    /// `cActionSetEnvironmentInputs::Process`: `SetSpecificInputs` then
    /// `cPopulation::ResetInputs(ctx)`.
    fn set_environment_inputs(&mut self, inputs: &[i32]) {
        self.env.set_specific_inputs(inputs.to_vec());
        for c in 0..self.pop.cells.len() {
            // `cell.ResetInputs(ctx)`: specific inputs draw no random
            // numbers.
            self.env
                .setup_inputs(&mut self.rng, &mut self.pop.cells[c].inputs, true);
            // `GetOrganism()->ResetInput()`
            if let Some(org) = self.pop.orgs[c].as_mut() {
                org.state.input_pointer = 0;
                org.state.input_buf.clear();
            }
        }
        // Events run between updates, so no organism is held outside
        // `pop.orgs` here. `cDeme::ResetInputs` / `ResetInput` reset
        // the deme-level inputs, which only the deme I/O instructions read;
        // `Deme` does not model them (those instructions are not ported).
    }

    /// `cStats::PrintDemeReplicationData(filename)`: the replications since
    /// the previous call, whose statistics it then clears.
    pub(crate) fn print_deme_replication_data(&mut self, filename: &str) -> Result<(), String> {
        let update = self.stats.update();
        let r = &mut self.pop.deme_repl_stats;
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida deme replication data");
        df.write_time_stamp();
        df.write(update, "Update [update]");
        df.write(r.deme_num_repls, "Number of deme replications [numrepl]");
        df.write(
            r.deme_gestation_time.average(),
            "Mean deme gestation time [gesttime]",
        );
        df.write(
            r.deme_births.average(),
            "Mean number of births within replicated demes [numbirths]",
        );
        df.write(
            r.deme_merit.average(),
            "Mean heritable merit of replicated demes [merit]",
        );
        df.write(
            r.deme_generation.average(),
            "Mean generation of replicated demes [generation]",
        );
        df.write(
            r.deme_density.average(),
            "Mean density of replicated demes [density]",
        );
        df.endl();
        r.deme_num_repls = 0;
        r.deme_gestation_time.clear();
        r.deme_births.clear();
        r.deme_merit.clear();
        r.deme_generation.clear();
        r.deme_density.clear();
        Ok(())
    }

    /// `cStats::PrintDemeGermlineSequestration(filename)`: means over the
    /// last 100 deme replications that had a germline.
    pub(crate) fn print_deme_germline_sequestration(
        &mut self,
        filename: &str,
    ) -> Result<(), String> {
        let update = self.stats.update();
        let r = &mut self.pop.deme_repl_stats;
        let columns: [(&mut std::collections::VecDeque<f64>, &str); 10] = [
            (
                &mut r.ave_germ_size,
                "Mean absolute germ size [m_ave_germ_size]",
            ),
            (
                &mut r.ave_germ_percent,
                "Mean percent of germ size [m_ave_germ_percent]",
            ),
            (
                &mut r.ave_germ_mut,
                "Mean number of mutations to germline [m_ave_germ_mut]",
            ),
            (
                &mut r.var_germ_mut,
                "Mean variance of mutations to germline [m_var_germ_mut]",
            ),
            (
                &mut r.ave_soma_mut,
                "Mean number of mutations to soma [m_ave_soma_mut]",
            ),
            (
                &mut r.var_soma_mut,
                "Mean variance of mutations to soma [m_var_soma_mut]",
            ),
            (&mut r.ave_germ_work, "Mean germ workload [m_ave_germ_work]"),
            (
                &mut r.var_germ_work,
                "Mean variance of germ workload [m_var_germ_work]",
            ),
            (&mut r.ave_soma_work, "Mean soma workload [m_ave_soma_work]"),
            (
                &mut r.var_soma_work,
                "Mean variance of soma workload [m_var_soma_work]",
            ),
        ];
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida deme germline sequestration data");
        df.write_time_stamp();
        df.write(update, "Update [update]");
        let mut values = Vec::with_capacity(columns.len());
        for (q, descr) in columns {
            while q.len() > 100 {
                q.pop_front();
            }
            values.push((q.iter().fold(0.0, |a, b| a + b), q.len(), descr));
        }
        // `if (m_ave_germ_mut.empty())` decides for every column.
        let empty = values[2].1 == 0;
        for (sum, n, descr) in values {
            // `std::accumulate(...) / size()`
            df.write(if empty { 0.0 } else { sum / n as f64 }, descr);
        }
        df.endl();
        Ok(())
    }

    /// `cActionInjectDemes::Process`
    fn inject_demes(
        &mut self,
        filename: &str,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    ) -> Result<(), String> {
        if filename.is_empty() {
            eprintln!("error: no organism file specified");
            return Ok(());
        }
        // Errors are printed, not fatal: the action returns when no genome
        // was loaded.
        let Some(genome) = self.load_genome(filename, "", false)? else {
            return Ok(());
        };
        // With ENERGY_ENABLED the first deme is skipped ("first org has
        // already been injected"); otherwise every deme's cell 0 is
        // (re)injected.
        let first = usize::from(self.cfg.energy_enabled);
        for d in first..self.pop.demes.len() {
            let cell_id = self.pop.demes[d].cell_id(0);
            self.inject(
                &genome,
                crate::run::injected_source(),
                cell_id,
                merit,
                lineage_label,
                neutral_metric,
            )?;
            self.pop.demes[d].injected_count += 1;
        }
        Ok(())
    }

    /// `cActionMergeResourceAcrossDemes::Process`: move a deme resource
    /// (its level, inflow and outflow) into a global resource.
    fn merge_resource_across_demes(&mut self, deme_res_name: &str, global_res_name: &str) {
        let lib = self.env.resource_lib();
        let (Some(deme_res), Some(global_res)) = (
            lib.resource_by_name(deme_res_name),
            lib.resource_by_name(global_res_name),
        ) else {
            return;
        };
        let deme_index = deme_res.index() as usize;
        let global_index = global_res.index() as usize;
        // The C++ reads this through the held `cResource*` after the inflow
        // changes below, which do not touch it.
        let outflow = deme_res.outflow;

        // Sum the resource across demes.
        let mut res_total = 0.0;
        for deme in &mut self.pop.demes {
            res_total += deme
                .resource_count
                .get(&mut self.rng, &mut self.res_env, deme_index);
        }

        // `SetResource(ctx, global_res_name, res_total)`
        self.with_pop_resources(|rc, rng, env| rc.set(rng, env, global_index, res_total));

        // `SetDemeResource(ctx, deme_res_name, 0.0)`
        for deme in &mut self.pop.demes {
            deme.resource_count
                .set(&mut self.rng, &mut self.res_env, deme_index, 0.0);
        }

        // Find the total inflow across demes. `GetInflow` of an unknown
        // name prints its error and yields -1.
        let mut inflow_total = 0.0;
        for deme in &self.pop.demes {
            inflow_total += deme
                .resource_count
                .get_inflow(deme_res_name)
                .unwrap_or_else(|e| {
                    eprintln!("{e}");
                    -1.0
                });
        }

        // `SetResourceInflow(global_res_name, inflow_total)`
        self.env.set_resource_inflow(global_res_name, inflow_total);
        report(
            self.pop
                .resource_count
                .set_inflow(global_res_name, inflow_total),
        );

        // `SetDemeResourceInflow(deme_res_name, 0.0)`
        self.env.set_resource_inflow(deme_res_name, 0.0);
        for deme in &mut self.pop.demes {
            report(deme.resource_count.set_inflow(deme_res_name, 0.0));
        }

        // `SetResourceOutflow(global_res_name, outflow)`: outflow is a
        // fraction, so it is not summed.
        self.env.set_resource_outflow(global_res_name, outflow);
        report(
            self.pop
                .resource_count
                .set_decay(global_res_name, 1.0 - outflow),
        );

        // `SetDemeResourceOutflow(deme_res_name, 0.0)`
        self.env.set_resource_outflow(deme_res_name, 0.0);
        for deme in &mut self.pop.demes {
            report(deme.resource_count.set_decay(deme_res_name, 1.0));
        }
    }

    /// `cPopulation::PrintDemeResource(ctx)`
    pub(crate) fn print_deme_resource(&mut self) -> Result<(), String> {
        let update = self.stats.update();
        let mut values = Vec::new();
        let mut spatial = Vec::new();
        for (deme_id, deme) in self.pop.demes.iter_mut().enumerate() {
            // `UpdateDemeRes(ctx)`
            deme.resource_count
                .get_resources(&mut self.rng, &mut self.res_env);
            for j in 0..deme.resource_count.size() {
                let v = deme.resource_count.get(&mut self.rng, &mut self.res_env, j);
                let res = &deme.resource_count;
                values.push((
                    v,
                    format!("Deme {deme_id} Resource {}", res.get_res_name(j)),
                ));
                if res.is_spatial_resource(j) {
                    // `PrintDemeSpatialResData(res, j, deme_id, ctx)`
                    let sp_res = res.get_spatial_resource(j).base();
                    let amounts: Vec<f64> =
                        (0..sp_res.size()).map(|k| sp_res.get_amount(k)).collect();
                    spatial.push((res.get_res_name(j).to_string(), deme_id, amounts));
                }
            }
        }
        for (res_name, deme_id, amounts) in spatial {
            self.print_deme_spatial_res_data(&res_name, deme_id, &amounts)?;
        }
        let df = self.out.static_with_path("deme_resources.dat")?;
        df.write_comment("Avida deme resource data");
        df.write_time_stamp();
        df.write(update, "update");
        for (v, descr) in values {
            df.write(v, &descr);
        }
        df.endl();
        Ok(())
    }

    /// `cPopulation::PrintDemeGlobalResources(ctx)`: one Matlab array per
    /// call, a row `<deme id> <res 0> ... <res n>` per deme (spatial
    /// resources leave their column unwritten).
    fn print_deme_global_resources(&mut self) -> Result<(), String> {
        let update = self.stats.update();
        let mut rows = Vec::new();
        for deme in &mut self.pop.demes {
            deme.resource_count
                .get_resources(&mut self.rng, &mut self.res_env);
            let num_res = deme.resource_count.size();
            let mut row = Vec::with_capacity(num_res);
            for r in 0..num_res {
                if !deme.resource_count.is_spatial_resource(r) {
                    let v = deme.resource_count.get(&mut self.rng, &mut self.res_env, r);
                    row.push((v, r as i32 + 1));
                }
            }
            rows.push((num_res as i32, row));
        }
        let df = self.out.static_with_path("deme_global_resources.dat")?;
        df.write_comment("Avida deme resource data");
        df.write_time_stamp();
        df.write_raw(&format!("deme_global_resources_{update:07} = [ ..."));
        for (deme_id, (num_res, row)) in rows.into_iter().enumerate() {
            df.write_block_element(deme_id as i32, 0, num_res + 1);
            for (v, element) in row {
                df.write_block_element(v, element, num_res + 1);
            }
        }
        df.write_raw("];");
        df.endl();
        Ok(())
    }
}

/// libc++ `std::partial_sort(first, first + middle, last, comp)` on a range
/// of an arithmetic type (libc++ 19+, as in the macOS SDK the reference was
/// built with): `__make_heap` on the first `middle` elements, a
/// `__sift_down` for every later element that compares before the heap
/// top, then `__sort_heap` (Floyd's `__pop_heap`). Equal elements end up
/// where libc++ puts them.
fn libcxx_partial_sort<T: Copy>(v: &mut [T], middle: usize, comp: impl Fn(&T, &T) -> bool) {
    let middle = middle.min(v.len());
    if middle == 0 {
        return;
    }
    make_heap(&mut v[..middle], &comp);
    for i in middle..v.len() {
        if comp(&v[i], &v[0]) {
            v.swap(i, 0);
            sift_down(v, &comp, middle, 0, false);
        }
    }
    // `__sort_heap`
    let mut n = middle;
    while n > 1 {
        pop_heap(&mut v[..n], &comp);
        n -= 1;
    }
}

/// libc++ `__make_heap` for arithmetic value types: sift down assuming
/// both children over an odd length, then sift up the last element.
fn make_heap<T: Copy>(v: &mut [T], comp: &impl Fn(&T, &T) -> bool) {
    let n = v.len();
    if n <= 1 {
        return;
    }
    let sift_down_n = if n & 1 == 1 { n } else { n - 1 };
    // `(__sift_down_n - 2) / 2` truncates -1/2 to 0 when it is 1.
    let mut start = sift_down_n.saturating_sub(2) / 2;
    loop {
        sift_down(v, comp, sift_down_n, start, true);
        if start == 0 {
            break;
        }
        start -= 1;
    }
    sift_up(v, comp, n, n);
}

/// libc++ `__sift_down<_, assume_both_children>(first, comp, len, start)`.
fn sift_down<T: Copy>(
    v: &mut [T],
    comp: &impl Fn(&T, &T) -> bool,
    len: usize,
    mut start: usize,
    assume_both_children: bool,
) {
    let larger_child = |v: &[T], child: usize| {
        if assume_both_children {
            child + usize::from(comp(&v[child], &v[child + 1]))
        } else if child + 1 < len && comp(&v[child], &v[child + 1]) {
            child + 1
        } else {
            child
        }
    };
    if len < 2 || (len - 2) / 2 < start {
        return;
    }
    let mut child = larger_child(v, 2 * start + 1);
    if comp(&v[child], &v[start]) {
        return;
    }
    let top = v[start];
    loop {
        v[start] = v[child];
        start = child;
        if (len - 2) / 2 < child {
            break;
        }
        child = larger_child(v, 2 * child + 1);
        if comp(&v[child], &top) {
            break;
        }
    }
    v[start] = top;
}

/// libc++ `__sift_up(first, first + last, comp, len)`.
fn sift_up<T: Copy>(v: &mut [T], comp: &impl Fn(&T, &T) -> bool, last: usize, len: usize) {
    if len <= 1 {
        return;
    }
    let mut len = (len - 2) / 2;
    let mut ptr = len;
    let mut last = last - 1;
    if comp(&v[ptr], &v[last]) {
        let t = v[last];
        loop {
            v[last] = v[ptr];
            last = ptr;
            if len == 0 {
                break;
            }
            len = (len - 1) / 2;
            ptr = len;
            if !comp(&v[ptr], &t) {
                break;
            }
        }
        v[last] = t;
    }
}

/// libc++ `__pop_heap(first, last, comp, len)`: Floyd's sift-down of the
/// hole left by the top, then a sift-up of the element moved into it.
fn pop_heap<T: Copy>(v: &mut [T], comp: &impl Fn(&T, &T) -> bool) {
    let len = v.len();
    if len < 2 {
        return;
    }
    let top = v[0];
    // `__floyd_sift_down` (`__child_i` is always `__first + __child`).
    let mut hole = 0;
    let mut child = 0;
    loop {
        child = 2 * child + 1;
        if child + 1 < len && comp(&v[child], &v[child + 1]) {
            child += 1;
        }
        v[hole] = v[child];
        hole = child;
        if child > (len - 2) / 2 {
            break;
        }
    }
    let last = len - 1;
    if hole == last {
        v[hole] = top;
    } else {
        v[hole] = v[last];
        v[last] = top;
        sift_up(v, comp, hole + 1, hole + 1);
    }
}

/// The resource-count name setters print their error to `cerr` and do
/// nothing for an unknown name.
fn report(r: Result<(), String>) {
    if let Err(e) = r {
        eprintln!("{e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, args: &str) -> (Option<DemeAction>, String) {
        let mut fb = Feedback::new();
        let a = parse(name, args, &mut fb).and_then(Result::ok);
        (a, fb.render())
    }

    #[test]
    fn parses_defaults_and_arguments() {
        assert_eq!(
            p("InjectDemes", "a.org").0,
            Some(DemeAction::InjectDemes {
                filename: "a.org".into(),
                merit: -1.0,
                lineage_label: 0,
                neutral_metric: 0.0,
            })
        );
        assert_eq!(
            p("ReplicateDemes", "").0,
            Some(DemeAction::ReplicateDemes {
                rep_trigger: deme_trigger::FULL
            })
        );
        assert_eq!(
            p("ReplicateDemes", "deme-age").0,
            Some(DemeAction::ReplicateDemes {
                rep_trigger: deme_trigger::AGE
            })
        );
        assert_eq!(
            p("SetMigrationRate", "0.01").0,
            Some(DemeAction::SetMigrationRate { rate: 0.01 })
        );
        assert_eq!(
            p("MergeResourceAcrossDemes", "dA gA").0,
            Some(DemeAction::MergeResourceAcrossDemes {
                deme_res_name: "dA".into(),
                global_res_name: "gA".into(),
            })
        );
        assert_eq!(p("Exit", "").0, None);
        assert_eq!(
            p("SetEnvironmentInputs", "0x0f13149f 0x3308e53e 0x556241eb").0,
            Some(DemeAction::SetEnvironmentInputs {
                inputs: vec![0x0f13149f, 0x3308e53e, 0x556241eb]
            })
        );
    }

    #[test]
    fn libcxx_partial_sort_selects_and_orders_the_front() {
        let mut v = vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5];
        libcxx_partial_sort(&mut v, 4, |a, b| a > b);
        assert_eq!(&v[..4], &[9, 6, 5, 5]);
        let mut rest = v[4..].to_vec();
        rest.sort_unstable();
        assert_eq!(rest, vec![1, 1, 2, 3, 3, 4, 5]);
        for n in 0..12 {
            let base: Vec<i32> = (0..n).map(|i| (i * 7919) % 13).collect();
            for k in 0..=n as usize {
                let mut v = base.clone();
                libcxx_partial_sort(&mut v, k, |a, b| a < b);
                let mut sorted = base.clone();
                sorted.sort_unstable();
                assert_eq!(&v[..k], &sorted[..k], "n {n} k {k}");
            }
        }
    }

    #[test]
    fn libcxx_partial_sort_keeps_the_first_of_equal_maxima_for_one() {
        // With middle == 1 only strictly greater elements replace the top.
        let loads = [0.5, 1.0, 1.0, 0.0];
        let mut d = vec![0usize, 1, 2, 3];
        libcxx_partial_sort(&mut d, 1, |&a, &b| loads[a] > loads[b]);
        assert_eq!(d[0], 1);
    }

    #[test]
    fn aborting_constructors_are_errors() {
        let mut fb = Feedback::new();
        for (name, args) in [
            ("ExitDemeReplications", ""),
            ("SetEnvironmentInputs", "1 2"),
            ("SetEnvironmentInputs", "1 2 3"),
        ] {
            assert!(
                matches!(parse(name, args, &mut fb), Some(Err(_))),
                "{name} {args}"
            );
        }
    }

    #[test]
    fn unknown_trigger_reports_and_keeps_the_action() {
        let (a, fb) = p("ReplicateDemes", "bogus");
        assert_eq!(
            a,
            Some(DemeAction::ReplicateDemes {
                rep_trigger: deme_trigger::UNKNOWN
            })
        );
        assert!(fb.contains("Unknown replication trigger 'bogus' in ReplicatDemes action."));
    }
}
