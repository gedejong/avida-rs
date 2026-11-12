//! PrintActions.cc printers and grid dumps outside the `STATS_OUT_FILE` table:
//! `Dump*Grid`, genotype lists, resource and instruction printers, and the
//! verbosity / echo actions (avida-core/source/actions/PrintActions.cc at 47f13dadb).
//!
//! Each action is the constructor (argument parsing, [`parse`]) and the
//! `Process` method (`run_print2_action`) of one `cAction*` class. Members
//! the C++ classes change between calls (the genotype colour chart of
//! `cActionDumpGenotypeColorGrid`, the message `cActionEcho` fills in on
//! its first call) live in [`Print2State`], keyed by an id given to each
//! parsed action.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use avida_apto::fmt::fmt_double;
use avida_config::feedback::Feedback;
use avida_config::string_util::{as_int, pop_word, trim};
use avida_genome::Genome;
use avida_hardware::organism::Organism;
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu};

use crate::actions::{Action, ParseContext};
use crate::world::{make_test_world, World};

/// The registered action names this module parses.
pub const PRINT2_ACTIONS: &[&str] = &[
    "DumpClassificationIDGrid",
    "DumpFitnessGrid",
    "DumpGenotypeColorGrid",
    "DumpPhenotypeIDGrid",
    "DumpIDGrid",
    "DumpVitalityGrid",
    "DumpTaskGrid",
    "DumpLastTaskGrid",
    "DumpHostTaskGrid",
    "DumpParasiteTaskGrid",
    "DumpHostTaskGridComma",
    "DumpParasiteTaskGridComma",
    "DumpParasiteVirulenceGrid",
    "DumpOffspringMigrationCounts",
    "DumpParasiteMigrationCounts",
    "DumpReactionGrid",
    "PrintLastReactionCountGrid",
    "DumpDonorGrid",
    "DumpReceiverGrid",
    "DumpCellDataGrid",
    "DumpSleepGrid",
    "DumpGenomeLengthGrid",
    "DumpGenotypeGrid",
    "DumpHostGenotypeList",
    "DumpParasiteGenotypeList",
    "PrintDebug",
    "Echo",
    "SetVerbose",
    "VERBOSE",
    "PrintNumDivides",
    "PrintSpatialResources",
    "PrintResourceLocData",
    "PrintResWallLocData",
    "PrintSoloTaskSnapshot",
    "PrintTaskSnapshot",
    "PrintViableTasksData",
    "PrintAveNumTasks",
    "PrintSuccessfulMates",
    "PrintGroupAttackData",
    "PrintKilledPreyFTData",
    "PrintFromMessageInstructionData",
    "PrintMaleInstructionData",
    "PrintFemaleInstructionData",
    "PrintPreyFlockingData",
    "PrintOrgGuardData",
];

/// The one-value-per-cell grid dumps (and the two genome lists), which
/// differ only in the value written for a cell and their default file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridKind {
    /// `cActionDumpCellDataGrid`
    CellData,
    /// `cActionDumpFitnessGrid`
    Fitness,
    /// `cActionDumpPhenotypeIDGrid`
    PhenotypeId,
    /// `cActionDumpIDGrid`
    Id,
    /// `cActionDumpVitalityGrid`
    Vitality,
    /// `cActionDumpSleepGrid`
    Sleep,
    /// `cActionDumpGenomeLengthGrid`
    GenomeLength,
    /// `cActionDumpTaskGrid` (test CPU)
    Task,
    /// `cActionDumpLastTaskGrid`
    LastTask,
    /// `cActionDumpHostTaskGrid`
    HostTask,
    /// `cActionDumpParasiteTaskGrid`
    ParasiteTask,
    /// `cActionDumpHostTaskGridComma`
    HostTaskComma,
    /// `cActionDumpParasiteTaskGridComma`
    ParasiteTaskComma,
    /// `cActionDumpParasiteVirulenceGrid`
    ParasiteVirulence,
    /// `cActionDumpReactionGrid`
    Reaction,
    /// `cActionDumpGenotypeGrid`
    Genotype,
    /// `cActionDumpHostGenotypeList`
    HostGenotypeList,
    /// `cActionDumpParasiteGenotypeList`
    ParasiteGenotypeList,
    /// `cActionDumpDonorGrid`
    Donor,
    /// `cActionDumpReceiverGrid`
    Receiver,
}

impl GridKind {
    /// The `filename.Set(...)` default, with `{}` for the update.
    fn default_pattern(self) -> &'static str {
        match self {
            GridKind::CellData => "grid_cell_data.{}.dat",
            GridKind::Fitness => "grid_fitness-{}.dat",
            GridKind::PhenotypeId => "grid_phenotype_id.{}.dat",
            GridKind::Id => "id_grid.{}.dat",
            GridKind::Vitality => "grid_dumps/vitality_grid.{}.dat",
            GridKind::Sleep => "grid_sleep.{}.dat",
            GridKind::GenomeLength => "grid_genome_length.{}.dat",
            GridKind::Task => "grid_task.{}.dat",
            GridKind::LastTask => "grid_last_task.{}.dat",
            GridKind::HostTask => "grid_task_hosts.{}.dat",
            GridKind::ParasiteTask => "grid_task_parasite.{}.dat",
            GridKind::HostTaskComma => "grid_task_hosts_comma.{}.dat",
            GridKind::ParasiteTaskComma => "grid_task_parasites_comma.{}.dat",
            GridKind::ParasiteVirulence => "grid_virulence.{}.dat",
            GridKind::Reaction => "grid_reactions.{}.dat",
            GridKind::Genotype => "grid_genome.{}.dat",
            GridKind::HostGenotypeList => "host_genome_list.{}.dat",
            GridKind::ParasiteGenotypeList => "parasite_genome_list.{}.dat",
            GridKind::Donor => "grid_donor.{}.dat",
            GridKind::Receiver => "grid_receiver.{}.dat",
        }
    }

    fn from_action(name: &str) -> Option<Self> {
        Some(match name {
            "DumpCellDataGrid" => GridKind::CellData,
            "DumpFitnessGrid" => GridKind::Fitness,
            "DumpPhenotypeIDGrid" => GridKind::PhenotypeId,
            "DumpIDGrid" => GridKind::Id,
            "DumpVitalityGrid" => GridKind::Vitality,
            "DumpSleepGrid" => GridKind::Sleep,
            "DumpGenomeLengthGrid" => GridKind::GenomeLength,
            "DumpTaskGrid" => GridKind::Task,
            "DumpLastTaskGrid" => GridKind::LastTask,
            "DumpHostTaskGrid" => GridKind::HostTask,
            "DumpParasiteTaskGrid" => GridKind::ParasiteTask,
            "DumpHostTaskGridComma" => GridKind::HostTaskComma,
            "DumpParasiteTaskGridComma" => GridKind::ParasiteTaskComma,
            "DumpParasiteVirulenceGrid" => GridKind::ParasiteVirulence,
            "DumpReactionGrid" => GridKind::Reaction,
            "DumpGenotypeGrid" => GridKind::Genotype,
            "DumpHostGenotypeList" => GridKind::HostGenotypeList,
            "DumpParasiteGenotypeList" => GridKind::ParasiteGenotypeList,
            "DumpDonorGrid" => GridKind::Donor,
            "DumpReceiverGrid" => GridKind::Receiver,
            _ => return None,
        })
    }
}

/// A parsed action of this module. Field names follow the C++ members
/// (`m_` dropped); `id` keys the state a C++ action object keeps.
#[derive(Debug, Clone, PartialEq)]
pub enum Print2Action {
    /// A [`GridKind`] dump (empty filename: the kind's default).
    Grid { kind: GridKind, filename: String },
    /// `cActionDumpClassificationIDGrid`: `filename` is a prefix.
    ClassificationIdGrid { filename: String, role: String },
    /// `cActionDumpGenotypeColorGrid`
    GenotypeColorGrid {
        id: usize,
        num_colors: i32,
        threshold: i32,
        filename: String,
    },
    /// `cActionPrintLastReactionCountGrid`
    LastReactionCountGrid { filename: String },
    /// `cActionDumpOffspringMigrationCounts` (`parasites == false`) and
    /// `cActionDumpParasiteMigrationCounts`
    MigrationCounts { parasites: bool, filename: String },
    /// `cActionPrintDebug`: does nothing.
    PrintDebug,
    /// `cActionEcho`: `args` is the action's `m_args`.
    Echo { id: usize, args: String },
    /// `cActionSetVerbose` (also registered as `VERBOSE`); upper-cased.
    SetVerbose { verbose: String },
    /// `cActionPrintNumDivides`
    NumDivides { filename: String },
    /// `cActionPrintSpatialResources`
    SpatialResources { filename: String },
    /// `cActionPrintResourceLocData`
    ResourceLocData { filename: String },
    /// `cActionPrintResWallLocData`
    ResWallLocData { filename: String },
    /// `cActionPrintSoloTaskSnapshot`
    SoloTaskSnapshot { filename: String },
    /// `cActionPrintTaskSnapshot`
    TaskSnapshot { filename: String },
    /// `cActionPrintViableTasksData`
    ViableTasksData { filename: String },
    /// `cActionPrintAveNumTasks`
    AveNumTasks { filename: String },
    /// `cActionPrintSuccessfulMates` (empty filename: `mates/mates-<update>.dat`)
    SuccessfulMates { filename: String },
    /// `cActionPrintGroupAttackData`
    GroupAttackData { filename: String, inst_set: String },
    /// `cActionPrintKilledPreyFTData`
    KilledPreyFtData { filename: String },
    /// `cActionPrintFromMessageInstructionData`
    FromMessageInstructionData { filename: String, inst_set: String },
    /// `cActionPrintMaleInstructionData` (`male`) and
    /// `cActionPrintFemaleInstructionData`
    SexInstructionData {
        male: bool,
        filename: String,
        inst_set: String,
    },
    /// `cActionPrintPreyFlockingData`
    PreyFlockingData { filename: String },
    /// `cActionPrintOrgGuardData`
    OrgGuardData { filename: String },
}

/// State the actions of this module keep between calls.
#[derive(Debug, Default)]
pub struct Print2State {
    /// A `cActionPrintFromMessageInstructionData` recorder is attached.
    record_from_message: bool,
    /// `core.population.from_message_inst_exec_counts[<inst set>]` as the
    /// recorders last received it; `None` before the first update.
    from_message_counts: Option<BTreeMap<String, Vec<i32>>>,
    /// `m_genotype_chart` of each `cActionDumpGenotypeColorGrid`.
    genotype_charts: BTreeMap<usize, Vec<i32>>,
    /// `m_args` of each `cActionEcho` once its first call filled it in.
    echo_args: BTreeMap<usize, String>,
}

/// Ids for actions whose C++ object keeps state between calls.
static NEXT_ACTION_ID: AtomicUsize = AtomicUsize::new(0);

fn next_id() -> usize {
    NEXT_ACTION_ID.fetch_add(1, Ordering::Relaxed)
}

/// `if (largs.GetSize()) x = largs.PopWord();`
struct Args(String);

impl Args {
    fn word(&mut self) -> Option<String> {
        if self.0.is_empty() {
            None
        } else {
            Some(pop_word(&mut self.0))
        }
    }
}

/// The instruction-set printers' constructor: `largs.Trim()`, the file
/// name if there is a word (`default` otherwise), the instruction set, and
/// `pattern` (with `%s` the set) for an explicitly empty name.
fn inst_set_args(
    args: &str,
    default: &str,
    pattern: &str,
    default_inst_set: &str,
) -> (String, String) {
    let mut largs = args.to_string();
    trim(&mut largs);
    let mut a = Args(largs);
    let mut filename = a.word().unwrap_or_else(|| default.to_string());
    let inst_set = a.word().unwrap_or_else(|| default_inst_set.to_string());
    if filename.is_empty() {
        filename = pattern.replace("%s", &inst_set);
    }
    (filename, inst_set)
}

/// `if (largs == "") m_filename = DEFAULT; else m_filename = largs.PopWord();`
fn default_or_word(args: &str, default: &str) -> String {
    let mut largs = args.to_string();
    if largs.is_empty() {
        default.to_string()
    } else {
        pop_word(&mut largs)
    }
}

/// The constructor of the action `name` (a [`PRINT2_ACTIONS`] spelling) applied to
/// `args`; `None` for any other name.
pub fn parse(
    name: &str,
    args: &str,
    ctx: &ParseContext<'_>,
    feedback: &mut Feedback,
) -> Option<Result<Print2Action, String>> {
    let _ = feedback;
    let mut a = Args(args.to_string());
    if let Some(kind) = GridKind::from_action(name) {
        return Some(Ok(Print2Action::Grid {
            kind,
            filename: a.word().unwrap_or_default(),
        }));
    }
    let action = match name {
        "DumpClassificationIDGrid" => Print2Action::ClassificationIdGrid {
            filename: a.word().unwrap_or_default(),
            role: a.word().unwrap_or_else(|| "genotype".to_string()),
        },
        "DumpGenotypeColorGrid" => Print2Action::GenotypeColorGrid {
            id: next_id(),
            num_colors: a.word().map_or(12, |w| as_int(&w)),
            threshold: a.word().map_or(10, |w| as_int(&w)),
            filename: a.word().unwrap_or_default(),
        },
        "PrintLastReactionCountGrid" => Print2Action::LastReactionCountGrid {
            filename: a
                .word()
                .unwrap_or_else(|| "last-reaction-count.dat".to_string()),
        },
        "DumpOffspringMigrationCounts" | "DumpParasiteMigrationCounts" => {
            Print2Action::MigrationCounts {
                parasites: name == "DumpParasiteMigrationCounts",
                filename: a.word().unwrap_or_default(),
            }
        }
        "PrintDebug" => Print2Action::PrintDebug,
        "Echo" => Print2Action::Echo {
            id: next_id(),
            args: args.to_string(),
        },
        "SetVerbose" | "VERBOSE" => Print2Action::SetVerbose {
            verbose: a.word().unwrap_or_default().to_ascii_uppercase(),
        },
        "PrintNumDivides" => Print2Action::NumDivides {
            filename: a.word().unwrap_or_default(),
        },
        "PrintSpatialResources" => Print2Action::SpatialResources {
            filename: a.word().unwrap_or_else(|| "resources.dat".to_string()),
        },
        "PrintResourceLocData" => Print2Action::ResourceLocData {
            filename: default_or_word(args, "resourceloc.dat"),
        },
        "PrintResWallLocData" => Print2Action::ResWallLocData {
            filename: default_or_word(args, "reswallloc.dat"),
        },
        "PrintSoloTaskSnapshot" => Print2Action::SoloTaskSnapshot {
            filename: default_or_word(args, "tasks-snap.dat"),
        },
        "PrintTaskSnapshot" => Print2Action::TaskSnapshot {
            filename: a.word().unwrap_or_default(),
        },
        "PrintViableTasksData" => Print2Action::ViableTasksData {
            filename: a.word().unwrap_or_else(|| "viable_tasks.dat".to_string()),
        },
        "PrintAveNumTasks" => Print2Action::AveNumTasks {
            filename: a.word().unwrap_or_else(|| "ave_num_tasks.dat".to_string()),
        },
        "PrintSuccessfulMates" => {
            let mut largs = args.to_string();
            trim(&mut largs);
            Print2Action::SuccessfulMates {
                filename: Args(largs).word().unwrap_or_default(),
            }
        }
        "PrintGroupAttackData" => {
            let (filename, inst_set) = inst_set_args(
                args,
                "group_attacks.dat",
                "group_attacks-%s.dat",
                ctx.default_inst_set,
            );
            Print2Action::GroupAttackData { filename, inst_set }
        }
        "PrintKilledPreyFTData" => {
            let mut largs = args.to_string();
            trim(&mut largs);
            let mut filename = Args(largs)
                .word()
                .unwrap_or_else(|| "killed_prey.dat".to_string());
            if filename.is_empty() {
                filename = "killed_prey.dat".to_string();
            }
            Print2Action::KilledPreyFtData { filename }
        }
        "PrintFromMessageInstructionData" => {
            let (filename, inst_set) = inst_set_args(
                args,
                "from_msg_instruction.dat",
                "from_msg_instruction-%s.dat",
                ctx.default_inst_set,
            );
            Print2Action::FromMessageInstructionData { filename, inst_set }
        }
        "PrintMaleInstructionData" | "PrintFemaleInstructionData" => {
            let male = name == "PrintMaleInstructionData";
            let (default, pattern) = if male {
                ("male_instruction.dat", "male_instruction-%s.dat")
            } else {
                ("female_instruction.dat", "female_instruction-%s.dat")
            };
            let (filename, inst_set) = inst_set_args(args, default, pattern, ctx.default_inst_set);
            Print2Action::SexInstructionData {
                male,
                filename,
                inst_set,
            }
        }
        "PrintPreyFlockingData" => Print2Action::PreyFlockingData {
            filename: a.word().unwrap_or_default(),
        },
        "PrintOrgGuardData" => Print2Action::OrgGuardData {
            filename: a.word().unwrap_or_default(),
        },
        _ => return None,
    };
    Some(Ok(action))
}

/// `static_cast<int>(pow(2.0, k))` (the arm64 conversion saturates, as
/// `as i32` does).
fn pow2(k: usize) -> i32 {
    2f64.powi(k as i32) as i32
}

/// A `fp << value << " "` cell list with `fp << endl` after every row, as
/// one string.
fn grid_text(world_x: i32, world_y: i32, mut cell: impl FnMut(i32) -> String) -> String {
    let mut text = String::new();
    for j in 0..world_y {
        for i in 0..world_x {
            text.push_str(&cell(j * world_x + i));
            text.push(' ');
        }
        text.push('\n');
    }
    text
}

impl World {
    /// `Process(ctx)` of an action of this module.
    pub(crate) fn run_print2_action(&mut self, action: &Print2Action) -> Result<(), String> {
        match action {
            Print2Action::Grid { kind, filename } => self.p2_dump_grid(*kind, filename),
            Print2Action::ClassificationIdGrid { filename, role } => {
                self.p2_dump_classification_id_grid(filename, role)
            }
            Print2Action::GenotypeColorGrid {
                id,
                num_colors,
                threshold,
                filename,
            } => self.p2_dump_genotype_color_grid(*id, *num_colors, *threshold, filename),
            Print2Action::LastReactionCountGrid { filename } => {
                self.p2_print_last_reaction_count_grid(filename)
            }
            Print2Action::MigrationCounts {
                parasites,
                filename,
            } => self.p2_dump_migration_counts(*parasites, filename),
            Print2Action::PrintDebug => Ok(()),
            Print2Action::Echo { id, args } => {
                self.p2_echo(*id, args);
                Ok(())
            }
            Print2Action::SetVerbose { verbose } => {
                self.p2_set_verbose(verbose);
                Ok(())
            }
            Print2Action::NumDivides { filename } => self.p2_print_num_divides(filename),
            Print2Action::SpatialResources { filename } => {
                self.p2_print_spatial_resources(filename)
            }
            Print2Action::ResourceLocData { filename } => self.p2_print_resource_loc_data(filename),
            Print2Action::ResWallLocData { filename } => self.p2_print_res_wall_loc_data(filename),
            Print2Action::SoloTaskSnapshot { filename } => {
                self.p2_print_solo_task_snapshot(filename)
            }
            Print2Action::TaskSnapshot { filename } => self.p2_print_task_snapshot(filename),
            Print2Action::ViableTasksData { filename } => self.p2_print_viable_tasks_data(filename),
            Print2Action::AveNumTasks { filename } => self.p2_print_ave_num_tasks(filename),
            Print2Action::SuccessfulMates { filename } => self.p2_print_successful_mates(filename),
            Print2Action::GroupAttackData { filename, inst_set } => self
                .stats
                .print_group_attack_data(&mut self.out, filename, inst_set),
            Print2Action::KilledPreyFtData { filename } => {
                let killed: Vec<Vec<i32>> = self
                    .pop
                    .live_org_list
                    .iter()
                    .filter_map(|&c| self.pop.orgs[c as usize].as_ref())
                    .map(|o| o.state.phenotype.cur_killed_targets.clone())
                    .collect();
                let targets = self.env.attack_prey_ft_list().to_vec();
                self.stats
                    .print_killed_prey_ft_data(&mut self.out, filename, &targets, &killed)
            }
            Print2Action::FromMessageInstructionData { filename, inst_set } => {
                self.p2_print_from_message_instruction_data(filename, inst_set)
            }
            Print2Action::SexInstructionData {
                male,
                filename,
                inst_set,
            } => {
                if *male {
                    self.stats
                        .print_male_instruction_data(&mut self.out, filename, inst_set)
                } else {
                    self.stats
                        .print_female_instruction_data(&mut self.out, filename, inst_set)
                }
            }
            Print2Action::PreyFlockingData { filename } => {
                self.p2_print_prey_flocking_data(filename)
            }
            Print2Action::OrgGuardData { filename } => self.p2_print_org_guard_data(filename),
        }
    }

    /// Called once before the first update, after the event list is loaded
    /// (where the C++ constructors attach data recorders).
    pub(crate) fn print2_note_recorders(&mut self) {
        self.actions2.print2.record_from_message = self.events.entries().iter().any(|e| {
            matches!(
                e.action,
                Action::Print2(Print2Action::FromMessageInstructionData { .. })
            )
        });
    }

    /// Called at the end of every update, after the organism statistics are
    /// gathered and before the status line.
    ///
    /// `FromMessageInstructionExecCountsProvider::UpdateReset` and
    /// `HandleOrganism` for every live organism, then `NotifyData`: per
    /// instruction set, the sums of each organism's
    /// `GetLastFromMessageInstCount()`.
    pub(crate) fn print2_end_update(&mut self) {
        if !self.actions2.print2.record_from_message {
            return;
        }
        let mut counts: BTreeMap<String, Vec<i32>> = BTreeMap::new();
        for is in self.hw.registry().inst_sets() {
            counts.insert(
                is.inst_set_name().to_string(),
                vec![0; is.size().max(0) as usize],
            );
        }
        for &cell in &self.pop.live_org_list {
            let Some(org) = self.pop.orgs[cell as usize].as_ref() else {
                continue;
            };
            let inst_set = org.state.genome().inst_set().to_string();
            // `Apto::ImplicitDefault`: an unknown name maps to an empty array.
            let sums = counts.entry(inst_set).or_default();
            for (j, &v) in org
                .state
                .phenotype
                .last_from_message_count
                .iter()
                .enumerate()
            {
                if let Some(s) = sums.get_mut(j) {
                    *s = s.wrapping_add(v);
                }
            }
        }
        self.actions2.print2.from_message_counts = Some(counts);
    }

    /// The organism in `cell` (actions run between updates, so an occupied
    /// cell always holds its organism).
    fn p2_org(&self, cell: i32) -> Option<&Organism> {
        self.pop.orgs.get(cell as usize).and_then(|o| o.as_deref())
    }

    /// `filename`, or the `{}`-pattern with the update for an empty one.
    fn p2_default_name(&self, filename: &str, pattern: &str) -> String {
        if filename.is_empty() {
            pattern.replace("{}", &self.stats.update().to_string())
        } else {
            filename.to_string()
        }
    }

    /// `Avida::Output::File::CreateWithPath(filename)` written with `text`.
    fn p2_create_and_write(&mut self, filename: &str, text: &str) -> Result<(), String> {
        let mut df = self.out.create_with_path(filename)?;
        df.ofstream()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        df.flush();
        Ok(())
    }

    /// The task bitmask of every occupied cell's genome on the test CPU
    /// (`cActionDumpTaskGrid`), `None` for empty cells.
    fn p2_test_task_sums(&mut self) -> Result<Vec<Option<i32>>, String> {
        let num_tasks = self.env.num_tasks().max(0) as usize;
        let genomes: Vec<Option<Genome>> = self
            .pop
            .orgs
            .iter()
            .map(|o| o.as_ref().map(|o| o.state.genome().clone()))
            .collect();
        let result = {
            let mut tw = make_test_world(
                &self.cfg,
                &mut self.env,
                &mut self.rng,
                &self.hw,
                &mut self.stats,
                &mut self.host,
                &*self.group_attack_fn,
                self.test_sterilize,
                &self.pop,
            );
            TestCpu::new(&mut tw).and_then(|mut cpu| {
                let mut sums = Vec::with_capacity(genomes.len());
                for genome in &genomes {
                    let Some(genome) = genome else {
                        sums.push(None);
                        continue;
                    };
                    let mut info = CpuTestInfo::default();
                    cpu.test_genome(&mut tw, &mut info, genome)?;
                    let ph = info
                        .test_phenotype(0)
                        .ok_or("DumpTaskGrid: no test phenotype")?;
                    let mut task_sum = 0i32;
                    for k in 0..num_tasks {
                        if ph.last_task_count.get(k).is_some_and(|&c| c > 0) {
                            task_sum = task_sum.wrapping_add(pow2(k));
                        }
                    }
                    sums.push(Some(task_sum));
                }
                Ok(sums)
            })
        };
        self.flush_host();
        result
    }

    /// `Process` of the [`GridKind`] dumps.
    fn p2_dump_grid(&mut self, kind: GridKind, filename: &str) -> Result<(), String> {
        let filename = self.p2_default_name(filename, kind.default_pattern());
        let world_x = self.pop.world_x;
        let world_y = self.pop.world_y;
        let num_tasks = self.env.num_tasks().max(0) as usize;
        let update = self.stats.update();
        let text = match kind {
            GridKind::Task => {
                let sums = self.p2_test_task_sums()?;
                grid_text(world_x, world_y, |c| {
                    sums[c as usize].unwrap_or(-1).to_string()
                })
            }
            GridKind::LastTask => {
                // `update cell task_id` per cell, one per line.
                let mut text = String::new();
                for cell_num in 0..world_x * world_y {
                    let task_id = self
                        .p2_org(cell_num)
                        .map_or(-1, |o| o.state.phenotype.last_task_id);
                    text.push_str(&format!("{update} {cell_num} {task_id}\n"));
                }
                text
            }
            GridKind::HostGenotypeList | GridKind::ParasiteGenotypeList => {
                let mut text = String::new();
                for cell_num in 0..world_x * world_y {
                    let Some(o) = self.p2_org(cell_num) else {
                        continue;
                    };
                    if kind == GridKind::HostGenotypeList {
                        text.push_str(&o.state.genome().representation().as_string());
                        text.push('\n');
                    } else if let Some(p) = o.state.parasites.first() {
                        text.push_str(&p.genome.as_string());
                        text.push('\n');
                    }
                }
                text
            }
            GridKind::Vitality => {
                // `cOrganism::GetVitality()`
                let mean_age = self.stats.sum_creature_age.average();
                let age_stddev = self.stats.sum_creature_age.std_deviation();
                let resource = self.cfg.collect_specific_resource;
                let extremes = self.cfg.vitality_bin_extremes;
                let center = self.cfg.vitality_bin_center;
                if resource >= 0
                    && self
                        .pop
                        .orgs
                        .iter()
                        .flatten()
                        .any(|o| o.state.phenotype.cur_rbins_avail.len() <= resource as usize)
                {
                    return Err(format!(
                        "DumpVitalityGrid: COLLECT_SPECIFIC_RESOURCE {resource} is not a \
                         resource bin (C++: out-of-bounds GetCurRBinAvail)"
                    ));
                }
                grid_text(world_x, world_y, |c| {
                    let v = match self.p2_org(c) {
                        None => -1.0,
                        Some(o) => {
                            let ph = &o.state.phenotype;
                            let res_level = if resource >= 0 {
                                ph.cur_rbins_avail
                                    .get(resource as usize)
                                    .copied()
                                    .unwrap_or(0.0)
                            } else {
                                0.0
                            };
                            let org_age = f64::from(ph.age);
                            if org_age < mean_age - age_stddev || org_age > mean_age + age_stddev {
                                extremes * res_level
                            } else {
                                center * res_level
                            }
                        }
                    };
                    fmt_double(v)
                })
            }
            _ => grid_text(world_x, world_y, |c| {
                let cell = &self.pop.cells[c as usize];
                let org = self.p2_org(c);
                match kind {
                    GridKind::CellData => fmt_double(f64::from(cell.cell_data.contents)),
                    GridKind::Fitness => fmt_double(org.map_or(0.0, |o| o.state.phenotype.fitness)),
                    GridKind::PhenotypeId => {
                        org.map_or(-1, |o| o.state.phenotype.calc_id()).to_string()
                    }
                    GridKind::Id => org.map_or(-1, |o| o.state.id).to_string(),
                    GridKind::Sleep => {
                        fmt_double(org.map_or(0.0, |o| f64::from(u8::from(o.state.is_sleeping))))
                    }
                    GridKind::GenomeLength => org
                        .map_or(-1, |o| o.state.genome().representation().size())
                        .to_string(),
                    GridKind::HostTask | GridKind::Reaction => match org {
                        None => "-1".to_string(),
                        Some(o) => {
                            let ph = &o.state.phenotype;
                            // QUIRK: the reaction grid tests the first
                            // `GetNumTasks()` reaction counts.
                            let counts = if kind == GridKind::HostTask {
                                &ph.last_host_tasks
                            } else {
                                &ph.last_reaction_count
                            };
                            let mut task_sum = 0i32;
                            for k in 0..num_tasks {
                                if counts.get(k).is_some_and(|&n| n > 0) {
                                    task_sum = task_sum.wrapping_add(pow2(k));
                                }
                            }
                            task_sum.to_string()
                        }
                    },
                    GridKind::ParasiteTask => match org {
                        Some(o) if !o.state.parasites.is_empty() => {
                            let mut task_sum = 0i32;
                            for k in 0..num_tasks {
                                if o.state
                                    .phenotype
                                    .last_para_tasks
                                    .get(k)
                                    .is_some_and(|&n| n > 0)
                                {
                                    task_sum = task_sum.wrapping_add(pow2(k));
                                }
                            }
                            task_sum.to_string()
                        }
                        _ => "-1".to_string(),
                    },
                    GridKind::HostTaskComma | GridKind::ParasiteTaskComma => {
                        let counts = match org {
                            Some(o) if kind == GridKind::HostTaskComma => {
                                Some(&o.state.phenotype.last_host_tasks)
                            }
                            Some(o) if !o.state.parasites.is_empty() => {
                                Some(&o.state.phenotype.last_para_tasks)
                            }
                            _ => None,
                        };
                        match counts {
                            None => "-1".to_string(),
                            Some(counts) => (0..num_tasks)
                                .map(|k| counts.get(k).copied().unwrap_or(0).to_string())
                                .collect::<Vec<_>>()
                                .join(","),
                        }
                    }
                    GridKind::ParasiteVirulence => fmt_double(match org {
                        Some(o) => o.state.parasites.first().map_or(-1.0, |p| p.virulence),
                        None => -1.0,
                    }),
                    GridKind::Genotype => org.map_or_else(
                        || "-1".to_string(),
                        |o| o.state.genome().representation().as_string(),
                    ),
                    GridKind::Donor => org
                        .map_or(-1, |o| i32::from(o.state.phenotype.is_donor_last))
                        .to_string(),
                    GridKind::Receiver => org
                        .map_or(-1, |o| i32::from(o.state.phenotype.is_receiver))
                        .to_string(),
                    GridKind::Task
                    | GridKind::LastTask
                    | GridKind::HostGenotypeList
                    | GridKind::ParasiteGenotypeList
                    | GridKind::Vitality => unreachable!("handled above"),
                }
            }),
        };
        self.p2_create_and_write(&filename, &text)
    }

    /// `cActionDumpClassificationIDGrid::Process`: the id of each organism's
    /// group in `role` (-1 for none).
    fn p2_dump_classification_id_grid(&mut self, filename: &str, role: &str) -> Result<(), String> {
        let prefix = if filename.is_empty() {
            "grid_class_id"
        } else {
            filename
        };
        let filename = format!("{prefix}-{}.dat", self.stats.update());
        let role_index = self.systematics.arbiter_for_role(role);
        let text = grid_text(self.pop.world_x, self.pop.world_y, |c| {
            let id = match (self.p2_org(c), role_index) {
                (Some(_), Some(r)) => self.pop.membership[c as usize]
                    .group(r)
                    .and_then(|g| self.systematics.group_id(g))
                    .unwrap_or(-1),
                _ => -1,
            };
            id.to_string()
        });
        self.p2_create_and_write(&filename, &text)
    }

    /// `cActionDumpGenotypeColorGrid::Process`: keeps a colour per abundant
    /// genotype across calls (`m_genotype_chart`).
    fn p2_dump_genotype_color_grid(
        &mut self,
        id: usize,
        num_colors: i32,
        threshold: i32,
        filename: &str,
    ) -> Result<(), String> {
        let n = num_colors.max(0) as usize;
        let order: Vec<i32> = match self.systematics.arbiter_for_role("genotype") {
            Some(role) => self
                .systematics
                .iter_groups(role)
                .into_iter()
                .filter_map(|g| self.systematics.group_id(g))
                .collect(),
            None => Vec::new(),
        };
        let mut chart = self
            .actions2
            .print2
            .genotype_charts
            .remove(&id)
            .unwrap_or_else(|| vec![0; n]);
        // Update current entries in the color chart (`FindPos(gid) < 0`:
        // not among the first `num_colors` genotypes).
        for slot in chart.iter_mut() {
            if *slot != 0 && !order.iter().take(n).any(|&g| g == *slot) {
                *slot = 0;
            }
        }
        // Add new entries where possible
        for &gid in order.iter().take(threshold.max(0) as usize) {
            if !chart.contains(&gid) {
                if let Some(slot) = chart.iter_mut().find(|s| **s == 0) {
                    *slot = gid;
                }
            }
        }
        let filename = self.p2_default_name(filename, "grid_genotype_color-{}.dat");
        let genotype_role = self.systematics.arbiter_for_role("genotype");
        let text = grid_text(self.pop.world_x, self.pop.world_y, |c| {
            let bg = match (self.p2_org(c), genotype_role) {
                (Some(_), Some(r)) => self.pop.membership[c as usize].group(r),
                _ => None,
            };
            match bg {
                None => "-1".to_string(),
                Some(g) => {
                    let gid = self.systematics.group_id(g).unwrap_or(-1);
                    let mut color = chart.iter().position(|&x| x == gid).unwrap_or(n);
                    if color == n && self.systematics.property(g, "threshold").bool_value() {
                        color += 1;
                    }
                    color.to_string()
                }
            }
        });
        self.actions2.print2.genotype_charts.insert(id, chart);
        self.p2_create_and_write(&filename, &text)
    }

    /// `cActionPrintLastReactionCountGrid::Process`. As with
    /// `PrintCurrReactionCountGrid`, the header state stands in for the
    /// action's `first_time` flag.
    fn p2_print_last_reaction_count_grid(&mut self, filename: &str) -> Result<(), String> {
        const UNOCCUPIED: i32 = -1;
        let update = self.stats.update();
        let names: Vec<String> = (0..self.env.num_reactions().max(0) as usize)
            .map(|r| self.env.reaction_name(r).to_string())
            .collect();
        let mut lines = Vec::with_capacity(names.len());
        for (react, name) in names.iter().enumerate() {
            let mut line = format!("{update} {name}");
            for slot in &self.pop.orgs {
                line.push(' ');
                match slot {
                    None => line.push_str(&UNOCCUPIED.to_string()),
                    Some(org) => line.push_str(
                        &org.state
                            .phenotype
                            .last_reaction_count
                            .get(react)
                            .copied()
                            .unwrap_or(0)
                            .to_string(),
                    ),
                }
            }
            lines.push(line);
        }
        let df = self.out.static_with_path(filename)?;
        df.write_comment(
            "First column is update, second column is reaction name, subsequent columns are individual cells",
        );
        df.write_comment("-1 for a reaction count indicates the cell is not occupied.");
        df.flush_comments();
        for line in lines {
            df.ofstream()
                .write_all(line.as_bytes())
                .map_err(|e| e.to_string())?;
            df.endl();
            df.flush();
        }
        Ok(())
    }

    /// `cActionDumpOffspringMigrationCounts::Process` and
    /// `cActionDumpParasiteMigrationCounts::Process`: the migration-matrix
    /// counts as comma-separated rows, then the counts are reset.
    fn p2_dump_migration_counts(&mut self, parasites: bool, filename: &str) -> Result<(), String> {
        let pattern = if parasites {
            "counts_parasite_migration.{}.dat"
        } else {
            "counts_offspring_migration.{}.dat"
        };
        let filename = self.p2_default_name(filename, pattern);
        let num_demes = self.pop.demes.len();
        let mm = &self.pop.migration_matrix;
        let mut text = String::new();
        for row in 0..num_demes {
            for col in 0..num_demes {
                let v = if parasites {
                    mm.parasite_count_at(row, col)
                } else {
                    mm.offspring_count_at(row, col)
                }
                .ok_or(
                    "DumpMigrationCounts: no migration matrix (C++: out-of-bounds array read)",
                )?;
                text.push_str(&v.to_string());
                if col + 1 < num_demes {
                    text.push(',');
                }
            }
            text.push('\n');
        }
        self.p2_create_and_write(&filename, &text)?;
        if parasites {
            self.pop.migration_matrix.reset_parasite_counts();
        } else {
            self.pop.migration_matrix.reset_offspring_counts();
        }
        Ok(())
    }

    /// `cActionEcho::Process`: `Driver().Feedback().Notify(m_args)`. With
    /// no arguments the first call fills `m_args` in, and later calls echo
    /// that same text.
    fn p2_echo(&mut self, id: usize, args: &str) {
        let text = if args.is_empty() {
            let update = self.stats.update();
            let ave_gen = self.stats.sum_generation.average();
            self.actions2
                .print2
                .echo_args
                .entry(id)
                .or_insert_with(|| {
                    // QUIRK: `m_args.Set("Echo : Update = %f\t AveGeneration = %f",
                    // GetUpdate(), ...)` passes the int update to `%f`; on the
                    // reference platform (arm64, variadic arguments in 8-byte
                    // stack slots) that slot reads back as a denormal, printed
                    // as 0.000000.
                    let _ = update;
                    format!("Echo : Update = 0.000000\t AveGeneration = {ave_gen:.6}")
                })
                .clone()
        } else {
            args.to_string()
        };
        // `StdIOFeedback::Notify(fmt)`: `printf(fmt, args)` then a newline.
        self.action_state.stdout.push_str(&text);
        self.action_state.stdout.push('\n');
    }

    /// `cActionSetVerbose::Process`
    fn p2_set_verbose(&mut self, verbose: &str) {
        const VERBOSE_SILENT: i32 = 0;
        const VERBOSE_NORMAL: i32 = 1;
        const VERBOSE_ON: i32 = 2;
        const VERBOSE_DETAILS: i32 = 3;
        let current = self.cfg.verbosity;
        // If no arguments are given, assume a basic toggle.
        let new = if verbose.is_empty() && current <= VERBOSE_NORMAL {
            VERBOSE_ON
        } else if verbose.is_empty() && current >= VERBOSE_ON {
            VERBOSE_NORMAL
        } else {
            match verbose {
                "SILENT" => VERBOSE_SILENT,
                "ON" => VERBOSE_ON,
                "DETAILS" | "HIGH" => VERBOSE_DETAILS,
                _ => VERBOSE_NORMAL,
            }
        };
        self.cfg.verbosity = new;
        // Print out new verbose level (nothing for silent!)
        let out = &mut self.action_state.stdout;
        match new {
            VERBOSE_NORMAL => out.push_str("Verbose NORMAL: Using standard log messages...\n"),
            VERBOSE_ON => out.push_str("Verbose ON: Using verbose log messages...\n"),
            VERBOSE_DETAILS => out.push_str("Verbose DETAILS: Using detailed log messages...\n"),
            _ => {}
        }
    }

    /// `cActionPrintNumDivides::Process`
    fn p2_print_num_divides(&mut self, filename: &str) -> Result<(), String> {
        // `filename.Set("divides.dat", update)`: no format field.
        let filename = if filename.is_empty() {
            "divides.dat"
        } else {
            filename
        };
        let mut text = String::from("# org_id,age,num_divides\n");
        for &cell in &self.pop.live_org_list {
            let Some(o) = self.p2_org(cell) else {
                continue;
            };
            let ph = &o.state.phenotype;
            text.push_str(&format!("{},{},{}\n", o.state.id, ph.age, ph.num_divides));
        }
        self.p2_create_and_write(filename, &text)
    }

    /// `cActionPrintSpatialResources::Process`: one row per spatial
    /// resource, the abundance in every cell. The header state stands in
    /// for `m_first_run`.
    fn p2_print_spatial_resources(&mut self, filename: &str) -> Result<(), String> {
        self.update_res_stats();
        let update = self.stats.update();
        let world_size = self.pop.cells.len();
        let mut rows = Vec::new();
        for res_id in 0..self.stats.resource_count.len() {
            if !self.pop.resource_count.is_spatial_resource(res_id) {
                continue;
            }
            let name = self
                .stats
                .resource_names
                .get(res_id)
                .cloned()
                .unwrap_or_default();
            let counts = self
                .stats
                .spatial_res_count
                .get(res_id)
                .ok_or("PrintSpatialResources: no spatial counts (C++: out-of-bounds read)")?;
            let mut raw = String::new();
            for cell_ndx in 0..world_size {
                let v = counts.get(cell_ndx).copied().ok_or(
                    "PrintSpatialResources: short spatial counts (C++: out-of-bounds read)",
                )?;
                raw.push_str(&fmt_double(v));
                if cell_ndx + 1 < world_size {
                    raw.push(' ');
                }
            }
            rows.push((name, raw));
        }
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida Spatial Resource Information");
        df.write_comment("Column 1 is the update");
        df.write_comment("Column 2 is the name of the resource");
        df.write_comment("The remaining columns are abundance of the named resource per cell");
        df.flush_comments();
        for (name, raw) in rows {
            df.write_anonymous(update);
            df.write_anonymous(name.as_str());
            df.ofstream()
                .write_all(raw.as_bytes())
                .map_err(|e| e.to_string())?;
            df.endl();
        }
        Ok(())
    }

    /// `cActionPrintResourceLocData::Process`: `TriggerDoUpdates`, then
    /// `cStats::PrintResourceLocData`: the cell at the peak of each
    /// gradient resource.
    fn p2_print_resource_loc_data(&mut self, filename: &str) -> Result<(), String> {
        self.with_pop_resources(|rc, rng, env| rc.update_resources(rng, env));
        let world_x = self.cfg.world_x;
        let gradients: Vec<usize> = self
            .env
            .resource_lib()
            .resources()
            .iter()
            .enumerate()
            .filter(|(_, r)| r.isgradient)
            .map(|(i, _)| i)
            .collect();
        let mut cells = Vec::with_capacity(gradients.len());
        for i in gradients {
            let x = self.with_pop_resources(|rc, rng, env| rc.get_curr_peak_x(rng, env, i));
            let y = self.with_pop_resources(|rc, rng, env| rc.get_curr_peak_y(rng, env, i));
            cells.push(x.wrapping_add(y.wrapping_mul(world_x)));
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida resource location data");
        df.write_time_stamp();
        df.write_comment(
            "First column gives the current update, all further columns give the cell id",
        );
        df.write_comment("for center of gradient resources at that update.");
        df.write(update, "Update");
        for c in cells {
            df.write(c, "CellID");
        }
        df.endl();
        Ok(())
    }

    /// `cActionPrintResWallLocData::Process`: `TriggerDoUpdates`, then
    /// `cStats::PrintResWallLocData`: the filled cells of each wall.
    fn p2_print_res_wall_loc_data(&mut self, filename: &str) -> Result<(), String> {
        self.with_pop_resources(|rc, rng, env| rc.update_resources(rng, env));
        let mut text = format!("{} ", self.stats.update());
        let walls: Vec<usize> = self
            .env
            .resource_lib()
            .resources()
            .iter()
            .enumerate()
            .filter(|(_, r)| r.isgradient && r.habitat == 2)
            .map(|(i, _)| i)
            .collect();
        for i in walls {
            let cells = self
                .pop
                .resource_count
                .get_wall_cells(i)
                .ok_or("PrintResWallLocData: no wall cells (C++: NULL dereference)")?;
            if let Some((last, rest)) = cells.split_last() {
                for c in rest {
                    text.push_str(&format!("{c},"));
                }
                text.push_str(&format!("{last} "));
            }
        }
        text.push('\n');
        let df = self.out.static_with_path(filename)?;
        if !df.header_done() {
            df.write_comment("Avida wall resource filled cells data");
            df.write_time_stamp();
            df.write_comment(
                "First column gives the current update, all further columns give filled cell ids for each wall res",
            );
            df.endl();
        }
        df.ofstream()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        df.flush();
        Ok(())
    }

    /// `cActionPrintSoloTaskSnapshot::Process` (`cStats::PrintSoloTaskSnapshot`):
    /// each live organism tested on a fresh test CPU once per resource,
    /// with only that resource available.
    fn p2_print_solo_task_snapshot(&mut self, filename: &str) -> Result<(), String> {
        let num_tasks = self.env.num_tasks().max(0) as usize;
        let num_res = self.env.resource_lib().size().max(0);
        let genomes: Vec<Genome> = self
            .pop
            .live_org_list
            .iter()
            .filter_map(|&c| self.pop.orgs[c as usize].as_ref())
            .map(|o| o.state.genome().clone())
            .collect();
        let mut reac_list = vec![0i32; num_tasks];
        let mut total_reacs = vec![0i32; num_tasks];
        let mut task_list = vec![0i32; num_tasks];
        let mut totals_list = vec![0i32; num_tasks];
        let result = {
            let mut tw = make_test_world(
                &self.cfg,
                &mut self.env,
                &mut self.rng,
                &self.hw,
                &mut self.stats,
                &mut self.host,
                &*self.group_attack_fn,
                self.test_sterilize,
                &self.pop,
            );
            (|| -> Result<(), String> {
                for genome in &genomes {
                    for k in 0..num_res {
                        let mut info = CpuTestInfo::default();
                        let mut cpu = TestCpu::new(&mut tw)?;
                        cpu.set_solo_res(k, 1.0);
                        cpu.test_genome(&mut tw, &mut info, genome)?;
                        let ph = info
                            .test_phenotype(0)
                            .ok_or("PrintSoloTaskSnapshot: no test phenotype")?;
                        for j in 0..num_tasks {
                            let task = *ph.last_task_count.get(j).ok_or(
                                "PrintSoloTaskSnapshot: short task counts (C++: out-of-bounds read)",
                            )?;
                            // QUIRK: the reaction counts are indexed by task.
                            let reac = *ph.last_reaction_count.get(j).ok_or(
                                "PrintSoloTaskSnapshot: fewer reactions than tasks (C++: out-of-bounds read)",
                            )?;
                            task_list[j] += i32::from(task != 0);
                            totals_list[j] = totals_list[j].wrapping_add(task);
                            reac_list[j] += i32::from(reac != 0);
                            total_reacs[j] = total_reacs[j].wrapping_add(reac);
                        }
                    }
                }
                Ok(())
            })()
        };
        self.flush_host();
        result?;
        let update = self.stats.update();
        let generation = self.stats.sum_generation.average();
        let pop_size = genomes.len() as i32;
        let task_names = self.stats.task_names.clone();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Pop potential tasks (test cpu).");
        df.write_time_stamp();
        df.write_comment("First set of columns gives number of organisms that can perform that particular reaction at least once. ");
        df.write_comment(
            "Second set of columns gives total number of times that reaction can be performed. ",
        );
        df.write_comment("Third set of columns gives number of organisms that can perform that particular task at least once. ");
        df.write_comment(
            "Fourth set of columns gives total number of times that task can be performed. ",
        );
        df.write_comment("Orgs are tested for each resource in the environment with res level of 1 for the current test resource and levels of 0 for other resources.");
        df.write(update, "Update");
        df.write(generation, "Generation");
        df.write(pop_size, "PopSize");
        let name = |j: usize| task_names.get(j).map_or("", String::as_str);
        for (j, v) in reac_list.iter().enumerate() {
            df.write(*v, name(j));
        }
        for (j, v) in total_reacs.iter().enumerate() {
            df.write(*v, &format!("{}_reac_totals", name(j)));
        }
        for (j, v) in task_list.iter().enumerate() {
            df.write(*v, name(j));
        }
        for (j, v) in totals_list.iter().enumerate() {
            df.write(*v, &format!("{}_task_totals", name(j)));
        }
        df.write(generation, "Generation");
        df.endl();
        Ok(())
    }

    /// `cActionPrintTaskSnapshot::Process`: per occupied cell, the tasks
    /// done now, on the test CPU and in the last gestation.
    fn p2_print_task_snapshot(&mut self, filename: &str) -> Result<(), String> {
        // QUIRK: the default name `tasks_<update>.dat` is computed but the
        // file is created with the member `m_filename`, which is empty then.
        if filename.is_empty() {
            return Err(
                "unsupported: PrintTaskSnapshot without a file name (C++: CreateWithPath(\"\") on the data directory)"
                    .to_string(),
            );
        }
        let num_tasks = self.env.num_tasks().max(0) as usize;
        let genotype_role = self.systematics.arbiter_for_role("genotype");
        let mut subjects = Vec::new();
        for i in 0..self.pop.cells.len() as i32 {
            let Some(o) = self.p2_org(i) else {
                continue;
            };
            let genotype_id = genotype_role
                .and_then(|r| self.pop.membership[i as usize].group(r))
                .and_then(|g| self.systematics.group_id(g))
                .ok_or("PrintTaskSnapshot: organism without a genotype (C++: NULL dereference)")?;
            subjects.push((
                i,
                o.state.genome().clone(),
                o.state.phenotype.cur_task_count.clone(),
                o.state.phenotype.last_task_count.clone(),
                genotype_id,
            ));
        }
        let tested = {
            let mut tw = make_test_world(
                &self.cfg,
                &mut self.env,
                &mut self.rng,
                &self.hw,
                &mut self.stats,
                &mut self.host,
                &*self.group_attack_fn,
                self.test_sterilize,
                &self.pop,
            );
            TestCpu::new(&mut tw).and_then(|mut cpu| {
                let mut out = Vec::with_capacity(subjects.len());
                for (_, genome, ..) in &subjects {
                    let mut info = CpuTestInfo::default();
                    cpu.test_genome(&mut tw, &mut info, genome)?;
                    let last = info
                        .test_phenotype(0)
                        .ok_or("PrintTaskSnapshot: no test phenotype")?
                        .last_task_count
                        .clone();
                    out.push((last, info.colony_fitness()));
                }
                Ok(out)
            })
        };
        self.flush_host();
        let tested = tested?;
        let mut df = self.out.create_with_path(filename)?;
        for ((cell, _, cur, last, genotype_id), (test_last, colony_fitness)) in
            subjects.iter().zip(tested)
        {
            let mut sum_tasks_all = 0;
            let mut divide_sum_tasks_all = 0;
            let mut parent_sum_tasks_all = 0;
            for j in 0..num_tasks {
                let get = |v: &Vec<i32>| {
                    v.get(j)
                        .copied()
                        .ok_or("PrintTaskSnapshot: short task counts (C++: out-of-bounds read)")
                };
                // Only one bonus per task, so no task counts as rewarded.
                sum_tasks_all += i32::from(get(cur)? != 0);
                divide_sum_tasks_all += i32::from(get(&test_last)? != 0);
                parent_sum_tasks_all += i32::from(get(last)? != 0);
            }
            df.write(*cell, "Cell Number");
            df.write(0, "Number of Tasks Rewarded");
            df.write(sum_tasks_all, "Total Number of Tasks Done");
            df.write(0, "Number of Rewarded Tasks on Divide");
            df.write(divide_sum_tasks_all, "Number of Total Tasks on Divide");
            df.write(0, "Parent Number of Tasks Rewared");
            df.write(parent_sum_tasks_all, "Parent Total Number of Tasks Done");
            df.write(colony_fitness, "Genotype Fitness");
            df.write(*genotype_id, "Genotype ID");
            df.endl();
        }
        Ok(())
    }

    /// `cActionPrintViableTasksData::Process`: per task, how many organisms
    /// with a positive test fitness do it in their current gestation.
    fn p2_print_viable_tasks_data(&mut self, filename: &str) -> Result<(), String> {
        let num_tasks = self.env.num_tasks().max(0) as usize;
        let mut tasks = vec![0i32; num_tasks];
        for i in 0..self.pop.cells.len() {
            if self.pop.orgs[i].is_none() {
                continue;
            }
            let g = self
                .systematics
                .genotype_of(&self.pop.membership[i])
                .ok_or(
                    "PrintViableTasksData: organism without a genotype (C++: NULL dereference)",
                )?;
            let fitness = self.genome_test_metrics(g).map_or(0.0, |m| m.fitness);
            if fitness > 0.0 {
                let Some(o) = self.p2_org(i as i32) else {
                    continue;
                };
                for (j, t) in tasks.iter_mut().enumerate() {
                    if o.state
                        .phenotype
                        .cur_task_count
                        .get(j)
                        .is_some_and(|&c| c > 0)
                    {
                        *t += 1;
                    }
                }
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida viable tasks data");
        df.write_time_stamp();
        df.write_comment("First column gives the current update, next columns give the number");
        df.write_comment(
            "of organisms that have the particular task as a component of their merit",
        );
        df.write(update, "Update");
        // QUIRK: `WriteAnonymous` goes to the file at once, ahead of the
        // header and the update on the first call.
        for t in tasks {
            df.write_anonymous(t);
        }
        df.endl();
        Ok(())
    }

    /// `cActionPrintAveNumTasks::Process`: the average number of tasks done
    /// in the last gestation by the organisms that did any.
    fn p2_print_ave_num_tasks(&mut self, filename: &str) -> Result<(), String> {
        let num_tasks = self.env.num_tasks().max(0) as usize;
        let mut ave_tot_tasks = 0i32;
        let mut num_task_orgs = 0i32;
        for slot in &self.pop.orgs {
            let Some(o) = slot.as_ref() else {
                continue;
            };
            let sum_tasks = (0..num_tasks)
                .filter(|&j| {
                    o.state
                        .phenotype
                        .last_task_count
                        .get(j)
                        .is_some_and(|&c| c != 0)
                })
                .count() as i32;
            if sum_tasks > 0 {
                ave_tot_tasks += sum_tasks;
                num_task_orgs += 1;
            }
        }
        let pop_ave = if num_task_orgs > 0 {
            f64::from(ave_tot_tasks) / f64::from(num_task_orgs)
        } else {
            -1.0
        };
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida num tasks data");
        df.write_time_stamp();
        df.write_comment(
            "First column gives the current update, 2nd column gives the average number of tasks performed",
        );
        df.write_comment(
            "by each organism in the current population that performs at least one task ",
        );
        df.write(update, "Update");
        df.write(
            pop_ave,
            "Ave num tasks done by single org that is doing at least one task",
        );
        df.endl();
        Ok(())
    }

    /// `cActionPrintSuccessfulMates::Process` (`cStats::PrintSuccessfulMates`).
    /// `cBirthChamber` never records a successful mate (the
    /// `RecordSuccessfulMate` call is commented out), so the file holds
    /// only its header.
    fn p2_print_successful_mates(&mut self, filename: &str) -> Result<(), String> {
        let filename = self.p2_default_name(filename, "mates/mates-{}.dat");
        if self.stats.m_num_successful_mates != 0 {
            return Err(
                "unsupported: PrintSuccessfulMates with recorded mates (C++: cStats::m_successful_mates)"
                    .to_string(),
            );
        }
        let mut df = self.out.create_with_path(&filename)?;
        df.write_time_stamp();
        df.write_comment("First half of each line gives information about the 'chosen' mate");
        df.write_comment("Second half of each line gives information about the 'chooser'");
        df.write_comment("genome timestamp merit mating_type mate_preference mating_display_a mating_display_b group");
        df.endl();
        Ok(())
    }

    /// `cActionPrintFromMessageInstructionData::Process`
    fn p2_print_from_message_instruction_data(
        &mut self,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        let is = self.hw.registry().get_inst_set(inst_set).ok_or_else(|| {
            format!("PrintFromMessageInstructionData: unknown instruction set '{inst_set}'")
        })?;
        let names: Vec<String> = (0..is.size()).map(|i| is.name(i).to_string()).collect();
        let update = self.stats.update();
        let data = self
            .actions2
            .print2
            .from_message_counts
            .as_ref()
            .map(|m| m.get(inst_set).cloned().unwrap_or_default());
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida from message instruction execution data");
        df.write_time_stamp();
        df.write(update, "Update");
        if let Some(counts) = data {
            for (i, v) in counts.iter().enumerate() {
                df.write(*v, names.get(i).map_or("", String::as_str));
            }
        }
        df.endl();
        Ok(())
    }

    /// `cActionPrintPreyFlockingData::Process`: for each prey, how many
    /// neighbouring cells hold prey (avatars: prey avatars).
    fn p2_print_prey_flocking_data(&mut self, filename: &str) -> Result<(), String> {
        let filename = self.p2_default_name(filename, "grid_dumps/prey_flocking.{}.dat");
        let use_av = self.cfg.use_avatars != 0;
        let mut text = if use_av {
            String::from(
                "# org_id,org_av_cellx,org_av_celly,num_neighbor_cells_with_prey,num_prey_this_cell\n",
            )
        } else {
            String::from("# org_id,org_cellx,org_celly,num_prey_neighbors,num_prey_this_cell\n")
        };
        let worldx = self.cfg.world_x;
        for &loc in &self.pop.live_org_list {
            let Some(o) = self.p2_org(loc) else {
                continue;
            };
            if o.state.forage_target <= -2 {
                continue;
            }
            let (id, uid) = (o.state.id, o.state.uid);
            let mut num_neighbors = 0;
            if !use_av {
                for n in self.pop.cells[loc as usize].connections.iter() {
                    if self.p2_org(n).is_some_and(|x| x.state.forage_target > -2) {
                        num_neighbors += 1;
                    }
                }
                text.push_str(&format!(
                    "{id},{},{},{num_neighbors},1\n",
                    loc % worldx,
                    loc / worldx
                ));
            } else {
                let avloc = self.av_cell_id(uid, 0);
                let num_prey_this_cell = self.avatars.counts(avloc).1;
                for n in self.pop.cells[avloc.max(0) as usize].connections.iter() {
                    if self.avatars.counts(n).1 > 0 {
                        num_neighbors += 1;
                    }
                }
                text.push_str(&format!(
                    "{id},{},{},{num_neighbors},{num_prey_this_cell}\n",
                    avloc % worldx,
                    avloc / worldx
                ));
            }
        }
        self.p2_create_and_write(&filename, &text)
    }

    /// `cActionPrintOrgGuardData::Process`: organism locations with their
    /// guarding and den data.
    fn p2_print_org_guard_data(&mut self, filename: &str) -> Result<(), String> {
        let filename = self.p2_default_name(filename, "grid_dumps/org_loc_guard.{}.dat");
        let use_av = self.cfg.use_avatars != 0;
        let mut text = if use_av {
            String::from("# org_id,org_cellx,org_celly,org_forage_target,org_group_id,org_facing,av_cellx,av_celly,av_facing,is_guard,num_guard_inst,on_den,r_bins_total,time_used,num_deposits,amount_deposited_total\n")
        } else {
            String::from("# org_id,org_cellx,org_celly,org_forage_target,org_group_id,org_facing,is_guard,num_guard_inst,on_den,r_bins_total,time_used,num_deposits,amount_deposited_total\n")
        };
        let worldx = self.cfg.world_x;
        let habitats: Vec<i32> = self
            .env
            .resource_lib()
            .resources()
            .iter()
            .map(|r| r.habitat)
            .collect();
        let live = self.pop.live_org_list.clone();
        for loc in live {
            let faced_dir = self.faced_dir_of(loc);
            let Some(o) = self.p2_org(loc) else {
                continue;
            };
            let (id, uid) = (o.state.id, o.state.uid);
            let ft = o.state.forage_target;
            let opinion = o
                .state
                .opinions
                .as_ref()
                .and_then(|v| v.last())
                .map_or(-1, |op| op.0);
            let is_guard = i32::from(o.state.guard);
            let num_guard = o.state.num_guard;
            let ph = &o.state.phenotype;
            // `GetRBinsTotal()` starts from +0 (`Sum` for f64 starts from
            // -0, which prints as `-0` for an organism without bins).
            let rbins_total = ph.cur_rbins_avail.iter().fold(0.0, |t, &b| t + b);
            let time_used = ph.time_used;
            let num_deposits = o.state.num_deposits;
            let amount_deposited = o.state.amount_deposited;
            let mut line = format!(
                "{id},{},{},{ft},{opinion},{faced_dir}",
                loc % worldx,
                loc / worldx
            );
            let mut res_cell = loc;
            if use_av {
                let (avloc, avfaced_dir) = self.avatar_cell_and_facing(uid);
                line.push_str(&format!(
                    ",{},{},{avfaced_dir}",
                    avloc % worldx,
                    avloc / worldx
                ));
                // `GetCellResources(loc)` is read too, then replaced.
                self.with_pop_resources(|rc, rng, env| {
                    rc.get_cell_resources(loc, rng, env);
                });
                res_cell = avloc;
            }
            line.push_str(&format!(",{is_guard},{num_guard}"));
            // Find out if the organism is in a den:
            let kills = self.res_env.habitat.kills;
            let res_count = self.with_pop_resources(|rc, rng, env| {
                rc.get_cell_resources(res_cell, rng, env).to_vec()
            });
            if self.res_env.habitat.kills != kills {
                return Err(
                    "PrintOrgGuardData: a gradient habitat killed an organism while \
                            the live organisms were listed (C++: reads the deleted organism)"
                        .to_string(),
                );
            }
            let on_den = res_count
                .iter()
                .zip(&habitats)
                .any(|(&amount, &hab)| (hab == 3 || hab == 4) && amount > 0.0);
            line.push_str(&format!(
                ",{},{},{time_used},{num_deposits},{}\n",
                i32::from(on_den),
                fmt_double(rbins_total),
                fmt_double(amount_deposited)
            ));
            text.push_str(&line);
        }
        self.p2_create_and_write(&filename, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTX: ParseContext<'static> = ParseContext {
        default_inst_set: "heads_default",
    };

    fn p(name: &str, args: &str) -> Print2Action {
        let mut fb = Feedback::new();
        parse(name, args, &CTX, &mut fb).unwrap().unwrap()
    }

    #[test]
    fn grid_dumps_take_an_optional_file_name() {
        assert_eq!(
            p("DumpFitnessGrid", ""),
            Print2Action::Grid {
                kind: GridKind::Fitness,
                filename: String::new()
            }
        );
        assert_eq!(
            p("DumpTaskGrid", "t.dat extra"),
            Print2Action::Grid {
                kind: GridKind::Task,
                filename: "t.dat".into()
            }
        );
        assert_eq!(
            p("DumpClassificationIDGrid", ""),
            Print2Action::ClassificationIdGrid {
                filename: String::new(),
                role: "genotype".into()
            }
        );
        match p("DumpGenotypeColorGrid", "5 3 c.dat") {
            Print2Action::GenotypeColorGrid {
                num_colors,
                threshold,
                filename,
                ..
            } => assert_eq!((num_colors, threshold, filename.as_str()), (5, 3, "c.dat")),
            other => panic!("{other:?}"),
        }
        assert!(parse("NoSuchAction", "", &CTX, &mut Feedback::new()).is_none());
    }

    #[test]
    fn default_file_names() {
        assert_eq!(
            p("PrintLastReactionCountGrid", ""),
            Print2Action::LastReactionCountGrid {
                filename: "last-reaction-count.dat".into()
            }
        );
        assert_eq!(
            p("PrintSpatialResources", ""),
            Print2Action::SpatialResources {
                filename: "resources.dat".into()
            }
        );
        assert_eq!(
            p("PrintResourceLocData", ""),
            Print2Action::ResourceLocData {
                filename: "resourceloc.dat".into()
            }
        );
        assert_eq!(
            p("PrintAveNumTasks", ""),
            Print2Action::AveNumTasks {
                filename: "ave_num_tasks.dat".into()
            }
        );
        assert_eq!(
            p("PrintKilledPreyFTData", "  "),
            Print2Action::KilledPreyFtData {
                filename: "killed_prey.dat".into()
            }
        );
    }

    #[test]
    fn instruction_set_printers() {
        assert_eq!(
            p("PrintFromMessageInstructionData", ""),
            Print2Action::FromMessageInstructionData {
                filename: "from_msg_instruction.dat".into(),
                inst_set: "heads_default".into()
            }
        );
        assert_eq!(
            p("PrintMaleInstructionData", "m.dat other"),
            Print2Action::SexInstructionData {
                male: true,
                filename: "m.dat".into(),
                inst_set: "other".into()
            }
        );
        assert_eq!(
            p("PrintGroupAttackData", ""),
            Print2Action::GroupAttackData {
                filename: "group_attacks.dat".into(),
                inst_set: "heads_default".into()
            }
        );
    }

    #[test]
    fn verbosity_and_echo() {
        assert_eq!(
            p("VERBOSE", "details"),
            Print2Action::SetVerbose {
                verbose: "DETAILS".into()
            }
        );
        assert_eq!(
            p("SetVerbose", ""),
            Print2Action::SetVerbose {
                verbose: String::new()
            }
        );
        match p("Echo", "hello world") {
            Print2Action::Echo { args, .. } => assert_eq!(args, "hello world"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_listed_name_parses() {
        for n in PRINT2_ACTIONS {
            assert!(parse(n, "", &CTX, &mut Feedback::new()).is_some(), "{n}");
            assert!(crate::action_library::is_registered(n), "{n}");
        }
    }

    #[test]
    fn pow2_saturates_like_the_reference() {
        assert_eq!(pow2(0), 1);
        assert_eq!(pow2(30), 1 << 30);
        assert_eq!(pow2(31), i32::MAX);
    }
}
