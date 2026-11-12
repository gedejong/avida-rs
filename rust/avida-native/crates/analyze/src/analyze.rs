//! `cAnalyze` state, the command language (`LoadCommandList`,
//! `ProcessCommands`, `PreProcessArgs`, variables, functions) and the
//! command table (`SetupCommandDefLibrary`).

use std::io::Write;
use std::sync::Arc;

use avida_apto::AvidaRng;
use avida_config::string_util as su;
use avida_config::{Feedback, InitFile};
use avida_environment::ResourceHistory;

use crate::actions;
use crate::error::{AnalyzeError, Result};
use crate::genotype::AnalyzeGenotype;
use crate::host::AnalyzeHost;

/// `MAX_BATCHES` (cAnalyze.h).
pub const MAX_BATCHES: i32 = 50000;
/// `INITIAL_BATCHES`.
pub const INITIAL_BATCHES: i32 = 2000;
/// `NUM_BATCHES_INCREMENT`.
pub const NUM_BATCHES_INCREMENT: i32 = 2000;

/// `cGenotypeBatch`: a named list of genotypes with lineage / alignment
/// flags.
#[derive(Debug, Clone, Default)]
pub struct GenotypeBatch {
    pub list: Vec<AnalyzeGenotype>,
    pub name: String,
    pub is_lineage: bool,
    pub is_aligned: bool,
}

impl GenotypeBatch {
    fn named(i: i32) -> Self {
        GenotypeBatch {
            name: format!("Batch{i}"),
            ..Default::default()
        }
    }

    /// `SetLineage(false); SetAligned(false)`, the usual epilogue of a
    /// command that changes a batch.
    pub(crate) fn clear_flags(&mut self) {
        self.is_lineage = false;
        self.is_aligned = false;
    }
}

/// `cAnalyzeCommand` / `cAnalyzeFlowCommand`: a command word, its argument
/// string and (for `FUNCTION`, `FOREACH`, `FORRANGE`) the body up to `END`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Command {
    pub name: String,
    pub args: String,
    pub body: Vec<Command>,
}

/// What `FindAnalyzeCommandDef` resolves a command word to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Def {
    Builtin(Builtin),
    /// `cAnalyzeCommandAction` (a world action run from analyze mode).
    Action,
}

/// The ported entries of `SetupCommandDefLibrary`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Builtin {
    LoadOrganism,
    LoadSequence,
    LoadResources,
    Load,
    Filter,
    FindGenotype,
    FindOrganism,
    FindLineage,
    FindSexLineage,
    FindClade,
    FindLastCommonAncestor,
    SampleOrganisms,
    SampleGenotypes,
    KeepTop,
    TruncateLineage,
    Print,
    Trace,
    PrintTasks,
    PrintTasksQuality,
    Detail,
    DetailTimeline,
    DetailBatches,
    DetailAverage,
    DetailIndex,
    Histogram,
    PrintPhenotypes,
    Align,
    Environment,
    Set,
    ConfigGet,
    ConfigSet,
    SetBatch,
    NameBatch,
    TagBatch,
    PurgeBatch,
    Duplicate,
    Recalculate,
    Recalc,
    Rename,
    CloseFile,
    Status,
    Echo,
    Test,
    Include,
    System,
    Function,
    Foreach,
    Forrange,
    PrintDiversity,
    PrintDistances,
    MapDepth,
    Hamming,
    Levenstein,
    Species,
    Recombine,
    RecombineSample,
    Mutagenize,
    WriteClone,
    WriteInjectEvents,
    WriteInjectInitial,
    WriteCompetition,
    AnalyzeInstructions,
    AnalyzeInstPop,
    AnalyzeMutationTraceback,
    SampleOffspring,
    MapTasks,
    MapMutations,
    AnalyzeKnockouts,
    AnalyzeComplexity,
    AnalyzeFitnessTwoSites,
    AnalyzeLineageComplexity,
    AnalyzePopComplexity,
    AnalyzeMuts,
    AnalyzeNewInfo,
    MutationRevert,
    CalcFunctionalModularity,
    PrintTreeStats,
    PrintCumulativeStemminess,
    PrintGamma,
    Compete,
    Interactive,
    GetSkeletons,
    CountNewSigLineages,
    CountNovelSkeletons,
    CalcSkeletonDiversity,
    GetLargestSkeleton,
    AllComplexityBarriers,
    ChangeDebug,
    ComplexityDebug,
    AnalyzeComplexityTwoSites,
    AnalyzeComplexityDelta,
    CommunityComplexity,
    AnalyzeMateSelection,
    AverageModularity,
    PrintResourceFitnessMap,
    AnalyzeRedundancyByInstFailure,
}

impl Builtin {
    pub(crate) fn is_flow(self) -> bool {
        matches!(
            self,
            Builtin::Function | Builtin::Foreach | Builtin::Forrange
        )
    }
}

/// `SetupCommandDefLibrary`, in registration order.
static LIBRARY: &[(&str, Builtin)] = &[
    ("LOAD_ORGANISM", Builtin::LoadOrganism),
    ("LOAD_SEQUENCE", Builtin::LoadSequence),
    ("LOAD_RESOURCES", Builtin::LoadResources),
    ("LOAD", Builtin::Load),
    ("FILTER", Builtin::Filter),
    ("FIND_GENOTYPE", Builtin::FindGenotype),
    ("FIND_ORGANISM", Builtin::FindOrganism),
    ("FIND_LINEAGE", Builtin::FindLineage),
    ("FIND_SEX_LINEAGE", Builtin::FindSexLineage),
    ("FIND_CLADE", Builtin::FindClade),
    ("FIND_LAST_COMMON_ANCESTOR", Builtin::FindLastCommonAncestor),
    ("SAMPLE_ORGANISMS", Builtin::SampleOrganisms),
    ("SAMPLE_GENOTYPES", Builtin::SampleGenotypes),
    ("KEEP_TOP", Builtin::KeepTop),
    ("TRUNCATELINEAGE", Builtin::TruncateLineage),
    ("TRUNCATE_LINEAGE", Builtin::TruncateLineage),
    ("SAMPLE_OFFSPRING", Builtin::SampleOffspring),
    ("PRINT", Builtin::Print),
    ("TRACE", Builtin::Trace),
    ("PRINT_TASKS", Builtin::PrintTasks),
    ("PRINT_TASKS_QUALITY", Builtin::PrintTasksQuality),
    ("DETAIL", Builtin::Detail),
    ("DETAIL_TIMELINE", Builtin::DetailTimeline),
    ("DETAIL_BATCHES", Builtin::DetailBatches),
    ("DETAIL_AVERAGE", Builtin::DetailAverage),
    ("DETAIL_INDEX", Builtin::DetailIndex),
    ("HISTOGRAM", Builtin::Histogram),
    ("PRINT_PHENOTYPES", Builtin::PrintPhenotypes),
    ("PRINT_DIVERSITY", Builtin::PrintDiversity),
    ("PRINT_DISTANCES", Builtin::PrintDistances),
    ("PRINT_TREE_STATS", Builtin::PrintTreeStats),
    (
        "PRINT_CUMULATIVE_STEMMINESS",
        Builtin::PrintCumulativeStemminess,
    ),
    ("PRINT_GAMMA", Builtin::PrintGamma),
    ("COMMUNITY_COMPLEXITY", Builtin::CommunityComplexity),
    (
        "PRINT_RESOURCE_FITNESS_MAP",
        Builtin::PrintResourceFitnessMap,
    ),
    ("MAP", Builtin::MapTasks),
    ("MAP_TASKS", Builtin::MapTasks),
    ("AVERAGE_MODULARITY", Builtin::AverageModularity),
    (
        "CALC_FUNCTIONAL_MODULARITY",
        Builtin::CalcFunctionalModularity,
    ),
    (
        "ANALYZE_REDUNDANCY_BY_INST_FAILURE",
        Builtin::AnalyzeRedundancyByInstFailure,
    ),
    ("MAP_MUTATIONS", Builtin::MapMutations),
    ("ANALYZE_COMPLEXITY", Builtin::AnalyzeComplexity),
    (
        "ANALYZE_LINEAGE_COMPLEXITY",
        Builtin::AnalyzeLineageComplexity,
    ),
    ("ANALYZE_FITNESS_TWO_SITES", Builtin::AnalyzeFitnessTwoSites),
    (
        "ANALYZE_COMPLEXITY_TWO_SITES",
        Builtin::AnalyzeComplexityTwoSites,
    ),
    ("ANALYZE_KNOCKOUTS", Builtin::AnalyzeKnockouts),
    ("GET_SKELETONS", Builtin::GetSkeletons),
    ("COUNT_NEW_SIG_LINEAGES", Builtin::CountNewSigLineages),
    ("COUNT_NOVEL_SKELETONS", Builtin::CountNovelSkeletons),
    ("CALC_SKELETON_DIVERSITY", Builtin::CalcSkeletonDiversity),
    ("GET_LARGEST_SKELETON", Builtin::GetLargestSkeleton),
    ("ALL_COMPLEXITY_BARRIERS", Builtin::AllComplexityBarriers),
    ("CHANGE_DEBUG", Builtin::ChangeDebug),
    ("COMPLEXITY_DEBUG", Builtin::ComplexityDebug),
    ("ANALYZE_POP_COMPLEXITY", Builtin::AnalyzePopComplexity),
    ("MAP_DEPTH", Builtin::MapDepth),
    ("HAMMING", Builtin::Hamming),
    ("LEVENSTEIN", Builtin::Levenstein),
    ("SPECIES", Builtin::Species),
    ("RECOMBINE", Builtin::Recombine),
    ("RECOMBINE_SAMPLE", Builtin::RecombineSample),
    ("MUTAGENIZE", Builtin::Mutagenize),
    ("ALIGN", Builtin::Align),
    ("ANALYZE_NEWINFO", Builtin::AnalyzeNewInfo),
    ("MUTATION_REVERT", Builtin::MutationRevert),
    ("WRITE_CLONE", Builtin::WriteClone),
    ("WRITE_INJECT_EVENTS", Builtin::WriteInjectEvents),
    ("WRITE_INJECT_INITIAL", Builtin::WriteInjectInitial),
    ("WRITE_COMPETITION", Builtin::WriteCompetition),
    ("ANALYZE_MUTS", Builtin::AnalyzeMuts),
    ("ANALYZE_INSTRUCTIONS", Builtin::AnalyzeInstructions),
    ("ANALYZE_INST_POP", Builtin::AnalyzeInstPop),
    (
        "ANALYZE_MUTATION_TRACEBACK",
        Builtin::AnalyzeMutationTraceback,
    ),
    ("ANALYZE_MATE_SELECTION", Builtin::AnalyzeMateSelection),
    ("ANALYZE_COMPLEXITY_DELTA", Builtin::AnalyzeComplexityDelta),
    ("ENVIRONMENT", Builtin::Environment),
    ("SET", Builtin::Set),
    ("CONFIG_GET", Builtin::ConfigGet),
    ("CONFIG_SET", Builtin::ConfigSet),
    ("SET_BATCH", Builtin::SetBatch),
    ("NAME_BATCH", Builtin::NameBatch),
    ("TAG_BATCH", Builtin::TagBatch),
    ("PURGE_BATCH", Builtin::PurgeBatch),
    ("DUPLICATE", Builtin::Duplicate),
    ("RECALCULATE", Builtin::Recalculate),
    ("RECALC", Builtin::Recalc),
    ("RENAME", Builtin::Rename),
    ("CLOSE_FILE", Builtin::CloseFile),
    ("STATUS", Builtin::Status),
    ("ECHO", Builtin::Echo),
    ("DEBUG", Builtin::Echo),
    ("TEST", Builtin::Test),
    ("INCLUDE", Builtin::Include),
    ("RUN", Builtin::Include),
    ("SYSTEM", Builtin::System),
    ("INTERACTIVE", Builtin::Interactive),
    ("FUNCTION", Builtin::Function),
    ("FOREACH", Builtin::Foreach),
    ("FORRANGE", Builtin::Forrange),
    ("COMPETE", Builtin::Compete),
];

/// Which of the three variable banks `GetVariable` returns.
#[derive(Debug, Clone, Copy)]
pub(crate) enum VarSlot {
    Global(usize),
    Local(usize),
    Arg(usize),
}

/// `cAnalyze`.
pub struct Analyze {
    pub(crate) cur_batch: i32,
    pub(crate) batches: Vec<GenotypeBatch>,
    pub(crate) variables: Vec<String>,
    pub(crate) local_variables: Vec<String>,
    pub(crate) arg_variables: Vec<String>,
    pub(crate) command_list: Vec<Command>,
    /// `function_list`: name and body, first definition wins on lookup.
    pub(crate) functions: Vec<(String, Vec<Command>)>,
    /// `random`, seeded from `RANDOM_SEED`.
    pub(crate) random: AvidaRng,
    /// `m_resources` (`LOAD_RESOURCES`), shared with the test infos built
    /// from it.
    pub(crate) resources: Option<Arc<ResourceHistory>>,
    pub(crate) resource_time_spent_offset: i32,
    /// `temporary_next_id` / `temporary_next_update` (`COMPETE`).
    pub(crate) temp_next_id: i32,
    pub(crate) temp_next_update: i32,
    /// The function-local `static int sequence_count` of `LoadSequence`.
    /// QUIRK: process-global in C++; per `Analyze` here (one per process).
    pub(crate) sequence_count: i32,
    /// `m_jobqueue.m_job_seed_rng`: seeds the RNG of each analyze job.
    /// `None` when the host has no world RNG.
    pub(crate) job_seed_rng: Option<AvidaRng>,
    /// The job queue's worker count (`m_workers.GetSize()`, at least 1),
    /// fixed at construction from `MAX_CONCURRENCY` as in C++.
    pub(crate) job_workers: usize,
    pub(crate) worker_stats: crate::jobs::WorkerStats,
    pub(crate) out: Box<dyn Write>,
    pub(crate) err: Box<dyn Write>,
    /// What `cin` reads in interactive mode (`None`: standard input).
    pub(crate) input: crate::cmd_interactive::InputSource,
}

impl Analyze {
    /// `cAnalyze(world)` writing to the process's stdout / stderr.
    pub fn new(host: &mut dyn AnalyzeHost) -> Self {
        Self::with_streams(
            host,
            Box::new(std::io::stdout()),
            Box::new(std::io::stderr()),
        )
    }

    /// `cAnalyze(world)` writing its console output (`cout`) to `out` and
    /// its errors (`cerr`) to `err`.
    ///
    /// The member `m_jobqueue` is constructed here: it draws the seed of
    /// its job RNG from the world RNG (`GetInt(MaxSeed())`,
    /// cAnalyzeJobQueue.cc:46). `random` is seeded with `RANDOM_SEED`.
    pub fn with_streams(
        host: &mut dyn AnalyzeHost,
        out: Box<dyn Write>,
        err: Box<dyn Write>,
    ) -> Self {
        let job_seed_rng = host.world_rng().map(|rng| {
            let max = rng.max_seed();
            AvidaRng::new(rng.get_int(max))
        });
        let batches = (0..INITIAL_BATCHES).map(GenotypeBatch::named).collect();
        Analyze {
            cur_batch: 0,
            batches,
            variables: vec![String::new(); 26],
            local_variables: vec![String::new(); 26],
            arg_variables: vec![String::new(); 26],
            command_list: Vec::new(),
            functions: Vec::new(),
            random: AvidaRng::new(host.config().random_seed),
            resources: None,
            resource_time_spent_offset: 0,
            temp_next_id: 0,
            temp_next_update: 0,
            sequence_count: 1,
            job_seed_rng,
            job_workers: crate::jobs::worker_count(host.config().max_concurrency),
            worker_stats: Default::default(),
            out,
            err,
            input: None,
        }
    }

    // -- Accessors ---------------------------------------------------------

    /// `GetNumBatches()`.
    pub fn num_batches(&self) -> i32 {
        self.batches.len() as i32
    }

    /// The current batch id.
    pub fn current_batch(&self) -> i32 {
        self.cur_batch
    }

    /// Batch `id` (panics when out of range, like indexing).
    pub fn batch(&self, id: i32) -> &GenotypeBatch {
        &self.batches[id as usize]
    }

    /// Mutable batch `id` (panics when out of range, like indexing).
    pub fn batch_mut(&mut self, id: i32) -> &mut GenotypeBatch {
        &mut self.batches[id as usize]
    }

    /// Global variable `$a`..`$z` (`idx` 0..26).
    pub fn variable(&self, idx: usize) -> &str {
        &self.variables[idx]
    }

    /// Flush the console streams (the C++ flushes `cout` at every `endl`).
    pub fn flush(&mut self) -> Result<()> {
        self.out.flush()?;
        self.err.flush()?;
        Ok(())
    }

    // -- Helpers -----------------------------------------------------------

    pub(crate) fn verbosity(host: &dyn AnalyzeHost) -> i32 {
        host.config().verbosity
    }

    /// Index of batch `id`; the C++ indexes the batch array unchecked.
    pub(crate) fn batch_index(&self, id: i32) -> Result<usize> {
        if id < 0 || id >= self.num_batches() {
            return Err(AnalyzeError::fatal(format!(
                "batch {id} out of range (0..{}); the C++ indexes past the batch array",
                self.num_batches()
            )));
        }
        Ok(id as usize)
    }

    /// The current batch, checked.
    pub(crate) fn cur(&mut self) -> Result<&mut GenotypeBatch> {
        let i = self.batch_index(self.cur_batch)?;
        Ok(&mut self.batches[i])
    }

    /// Print `msg` (plus newline) to the error stream and stop as
    /// `exit(1)` does (`exit_on_error` is always true).
    pub(crate) fn exit_with(&mut self, msg: &str) -> AnalyzeError {
        let _ = writeln!(self.err, "{msg}");
        let _ = self.err.flush();
        AnalyzeError::Exit(1)
    }

    /// Print every feedback message the way analyze mode does:
    /// `error: ` / `warning: ` prefixes, notifications bare.
    pub(crate) fn print_feedback(&mut self, fb: &Feedback) -> Result<()> {
        for i in 0..fb.num_messages() {
            writeln!(self.err, "{}", fb.format_message(i))?;
        }
        Ok(())
    }

    /// `GetVariable(var)`.
    pub(crate) fn get_variable(&mut self, var: &str) -> Result<VarSlot> {
        let b = var.as_bytes();
        if b.len() != 1 || !(b[0].is_ascii_alphabetic() || b[0].is_ascii_digit()) {
            return Err(self.exit_with(&format!("Error: Illegal variable {var} being used.")));
        }
        let c = b[0];
        Ok(if c.is_ascii_lowercase() {
            VarSlot::Global((c - b'a') as usize)
        } else if c.is_ascii_uppercase() {
            VarSlot::Local((c - b'A') as usize)
        } else {
            VarSlot::Arg((c - b'0') as usize)
        })
    }

    pub(crate) fn var(&self, slot: VarSlot) -> &str {
        match slot {
            VarSlot::Global(i) => &self.variables[i],
            VarSlot::Local(i) => &self.local_variables[i],
            VarSlot::Arg(i) => &self.arg_variables[i],
        }
    }

    pub(crate) fn set_var(&mut self, slot: VarSlot, value: String) {
        match slot {
            VarSlot::Global(i) => self.variables[i] = value,
            VarSlot::Local(i) => self.local_variables[i] = value,
            VarSlot::Arg(i) => self.arg_variables[i] = value,
        }
    }

    // -- Command language ------------------------------------------------

    /// `FindAnalyzeCommandDef(name)`: library commands match
    /// case-insensitively; otherwise a registered action (also
    /// case-insensitive, `cActionLibrary::Supports`).
    pub(crate) fn find_def(&self, host: &dyn AnalyzeHost, name: &str) -> Option<Def> {
        let upper = name.to_ascii_uppercase();
        if let Some((_, b)) = LIBRARY.iter().find(|(n, _)| *n == upper) {
            return Some(Def::Builtin(*b));
        }
        if actions::is_registered_action(name) || host.has_action(name) {
            return Some(Def::Action);
        }
        None
    }

    /// `RunFile(filename)`: load and run an analyze script, or create a
    /// default one (relative to the process directory, as `ofstream` does)
    /// when it cannot be opened.
    pub fn run_file(&mut self, host: &mut dyn AnalyzeHost, filename: &str) -> Result<()> {
        let saved_analyze = host.analyze_mode();
        host.set_analyze_mode(true);

        let file = InitFile::new(filename, host.working_dir(), None, None);
        if !file.was_opened() {
            let fb = file.feedback().clone();
            self.print_feedback(&fb)?;
            writeln!(self.err, "warning: creating default file: '{filename}'")?;
            if let Ok(mut fp) = std::fs::File::create(filename) {
                let _ = fp.write_all(DEFAULT_ANALYZE_FILE.as_bytes());
            }
        } else {
            let lines = file_lines(&file);
            let mut list = Vec::new();
            self.load_command_list(host, &lines, 0, &mut list);
            self.command_list = list;
            let mut list = std::mem::take(&mut self.command_list);
            let r = self.process_commands(host, &mut list);
            self.command_list = list;
            r?;
        }

        if !saved_analyze {
            host.set_analyze_mode(false);
        }
        Ok(())
    }

    /// Parse script lines (as `cInitFile` delivers them) into commands, the
    /// way `RunFile` does, without running them.
    pub fn parse_commands(&self, host: &dyn AnalyzeHost, lines: &[String]) -> Vec<Command> {
        let mut list = Vec::new();
        self.load_command_list(host, lines, 0, &mut list);
        list
    }

    /// `LoadCommandList(init_file, clist, start_at)`: returns the index of
    /// the `END` line that closed this level (or the line count).
    pub(crate) fn load_command_list(
        &self,
        host: &dyn AnalyzeHost,
        lines: &[String],
        start_at: usize,
        clist: &mut Vec<Command>,
    ) -> usize {
        let mut i = start_at;
        while i < lines.len() {
            let mut cur_string = lines[i].clone();
            let command = su::pop_word(&mut cur_string);
            let def = self.find_def(host, &command);
            if command == "END" {
                return i;
            }
            let mut body = Vec::new();
            if let Some(Def::Builtin(b)) = def {
                if b.is_flow() {
                    i = self.load_command_list(host, lines, i + 1, &mut body);
                }
            }
            clist.push(Command {
                name: command,
                args: cur_string,
                body,
            });
            i += 1;
        }
        lines.len()
    }

    /// `PreProcessArgs(args)`: substitute `$x` variables and `$$`.
    pub(crate) fn pre_process_args(&self, args: &mut String) -> Result<()> {
        let mut search_start: i32 = 0;
        loop {
            let pos = su::find_char(args, '$', search_start);
            if pos == -1 {
                return Ok(());
            }
            let p = pos as usize;
            let varlet = args.as_bytes().get(p + 1).copied().unwrap_or(0);
            let varsize = if varlet == b'$' {
                su::clip(args, p + 1, 1);
                1
            } else if varlet.is_ascii_alphanumeric() {
                let value = if varlet.is_ascii_lowercase() {
                    &self.variables[(varlet - b'a') as usize]
                } else if varlet.is_ascii_uppercase() {
                    &self.local_variables[(varlet - b'A') as usize]
                } else {
                    &self.arg_variables[(varlet - b'0') as usize]
                };
                let varname = format!("${}", varlet as char);
                su::replace(args, &varname, value, pos);
                value.len()
            } else {
                // The C++ leaves search_start at the '$' and never ends.
                return Err(AnalyzeError::fatal(format!(
                    "'$' followed by {:?} in arguments '{args}': cAnalyze::PreProcessArgs loops forever",
                    varlet as char
                )));
            };
            search_start = pos + varsize as i32;
        }
    }

    /// `ProcessCommands(clist)`: run each command after variable
    /// substitution: library command, else world action (its feedback is
    /// printed and errors exit), else user function, else
    /// "Unknown analysis keyword" and exit.
    pub fn process_commands(
        &mut self,
        host: &mut dyn AnalyzeHost,
        clist: &mut [Command],
    ) -> Result<()> {
        for cmd in clist.iter_mut() {
            let command = cmd.name.clone();
            let mut args = cmd.args.clone();
            self.pre_process_args(&mut args)?;

            match self.find_def(host, &command) {
                Some(Def::Builtin(b)) => self.run_builtin(host, b, args, &mut cmd.body)?,
                Some(Def::Action) => {
                    // cAnalyzeCommandAction::Run: create the action, then
                    // Process(m_world->GetDefaultContext()).
                    let mut fb = Feedback::new();
                    if let Some(native) = actions::native_action(&command) {
                        self.run_native_action(host, native, &args, &mut fb)?;
                    } else {
                        host.run_action(&command, &args, &mut fb, &mut *self.out)?;
                    }
                    for i in 0..fb.num_messages() {
                        writeln!(self.err, "{}", fb.format_message(i))?;
                        if fb.num_errors() > 0 {
                            let _ = self.err.flush();
                            return Err(AnalyzeError::Exit(1));
                        }
                    }
                }
                None => {
                    if !self.function_run(host, &command, args)? {
                        return Err(self
                            .exit_with(&format!("error: Unknown analysis keyword '{command}'.")));
                    }
                }
            }
        }
        Ok(())
    }

    /// Run one command as if it were a script line `name args`.
    pub fn run_command(
        &mut self,
        host: &mut dyn AnalyzeHost,
        name: &str,
        args: &str,
    ) -> Result<()> {
        let mut list = vec![Command {
            name: name.to_string(),
            args: args.to_string(),
            body: Vec::new(),
        }];
        self.process_commands(host, &mut list)
    }

    pub(crate) fn run_builtin(
        &mut self,
        host: &mut dyn AnalyzeHost,
        b: Builtin,
        args: String,
        body: &mut Vec<Command>,
    ) -> Result<()> {
        match b {
            Builtin::LoadOrganism => self.load_organism(host, args),
            Builtin::LoadSequence => self.load_sequence(host, args),
            Builtin::LoadResources => self.load_resources(host, args),
            Builtin::Load => self.load_file(host, args),
            Builtin::Filter => self.command_filter(host, args),
            Builtin::FindGenotype => self.find_genotype(host, args),
            Builtin::FindOrganism => self.find_organism(host, args),
            Builtin::FindLineage => self.find_lineage(host, args),
            Builtin::FindSexLineage => self.find_sex_lineage(host, args),
            Builtin::FindClade => self.find_clade(host, args),
            Builtin::FindLastCommonAncestor => self.find_last_common_ancestor(host, args),
            Builtin::SampleOrganisms => self.sample_organisms(host, args),
            Builtin::SampleGenotypes => self.sample_genotypes(host, args),
            Builtin::KeepTop => self.keep_top_genotypes(args),
            Builtin::TruncateLineage => self.truncate_lineage(host, args),
            Builtin::Print => self.command_print(host, args),
            Builtin::Trace => self.command_trace(host, args),
            Builtin::PrintTasks => self.command_print_tasks(host, args, false),
            Builtin::PrintTasksQuality => self.command_print_tasks(host, args, true),
            Builtin::Detail => self.command_detail(host, args),
            Builtin::DetailTimeline => self.command_detail_timeline(host, args),
            Builtin::DetailBatches => self.command_detail_batches(host, args),
            Builtin::DetailAverage => self.command_detail_average(host, args),
            Builtin::DetailIndex => self.command_detail_index(host, args),
            Builtin::Histogram => self.command_histogram(host, args),
            Builtin::PrintPhenotypes => self.command_print_phenotypes(host, args),
            Builtin::Align => self.command_align(host),
            Builtin::Environment => self.environment_setup(host, args),
            Builtin::Set => self.var_set(host, args),
            Builtin::ConfigGet => self.config_get(host, args),
            Builtin::ConfigSet => self.config_set(host, args),
            Builtin::SetBatch => self.batch_set(host, args),
            Builtin::NameBatch => self.batch_name(host, args),
            Builtin::TagBatch => self.batch_tag(host, args),
            Builtin::PurgeBatch => self.batch_purge(host, args),
            Builtin::Duplicate => self.batch_duplicate(host, args),
            Builtin::Recalculate => self.batch_recalculate(host, args),
            Builtin::Recalc => self.batch_recalculate_with_args(host, args),
            Builtin::Rename => self.batch_rename(host, args),
            Builtin::CloseFile => self.close_file(host, args),
            Builtin::Status => self.print_status(),
            Builtin::Echo => self.print_debug(args),
            Builtin::Test => self.print_test_info(),
            Builtin::Include => self.include_file(host, args),
            Builtin::System => self.command_system(args),
            Builtin::Function => self.function_create(host, args, body),
            Builtin::Foreach => self.command_foreach(host, args, body),
            Builtin::Forrange => self.command_forrange(host, args, body),
            Builtin::PrintDiversity => self.command_print_diversity(host, args),
            Builtin::PrintDistances => self.command_print_distances(host, args),
            Builtin::MapDepth => self.command_map_depth(host, args),
            Builtin::Hamming => self.command_pair_distance(host, args, false),
            Builtin::Levenstein => self.command_pair_distance(host, args, true),
            Builtin::Species => self.command_species(host, args),
            Builtin::Recombine => self.command_recombine(host, args),
            Builtin::RecombineSample => self.command_recombine_sample(host, args),
            Builtin::Mutagenize => self.command_mutagenize(host),
            Builtin::WriteClone => self.write_clone(host, args),
            Builtin::WriteInjectEvents => self.write_inject(host, args, false),
            Builtin::WriteInjectInitial => self.write_inject(host, args, true),
            Builtin::WriteCompetition => self.write_competition(host, args),
            Builtin::AnalyzeInstructions => self.analyze_instructions(host, args),
            Builtin::AnalyzeInstPop => self.analyze_inst_pop(host, args),
            Builtin::AnalyzeMutationTraceback => self.analyze_mutation_traceback(host, args),
            Builtin::SampleOffspring => self.sample_offspring(host, args),
            Builtin::MapTasks => self.command_map_tasks(host, args),
            Builtin::MapMutations => self.command_map_mutations(host, args),
            Builtin::AnalyzeKnockouts => self.analyze_knockouts(host, args),
            Builtin::AnalyzeComplexity => self.analyze_complexity(host, args),
            Builtin::AnalyzeFitnessTwoSites => self.analyze_fitness_two_sites(host, args),
            Builtin::AnalyzeLineageComplexity => self.analyze_lineage_complexity(host, args),
            Builtin::AnalyzePopComplexity => self.analyze_pop_complexity(host, args),
            Builtin::AnalyzeMuts => self.analyze_muts(host, args),
            Builtin::AnalyzeNewInfo => self.analyze_new_info(host, args),
            Builtin::MutationRevert => self.mutation_revert(host, args),
            Builtin::CalcFunctionalModularity => {
                self.command_calc_functional_modularity(host, args)
            }
            Builtin::PrintTreeStats => self.command_print_stemminess(host, args, true),
            Builtin::PrintCumulativeStemminess => self.command_print_stemminess(host, args, false),
            Builtin::PrintGamma => self.command_print_gamma(host, args),
            Builtin::Compete => self.batch_compete(host, args),
            Builtin::Interactive => self.run_interactive(host),
            Builtin::GetSkeletons => self.get_skeletons_batch(host, args),
            Builtin::CountNewSigLineages => self.count_new_significant_lineages(host, args),
            Builtin::CountNovelSkeletons => self.count_novel_skeletons(host, args),
            Builtin::CalcSkeletonDiversity => self.shannon_diversity_skeletons(host, args),
            Builtin::GetLargestSkeleton => self.get_largest_skeleton(host, args),
            Builtin::AllComplexityBarriers => self.all_complexity_barriers(host, args),
            Builtin::ChangeDebug => self.change_debugging(host, args),
            Builtin::ComplexityDebug => self.complexity_debugging(host, args),
            Builtin::AnalyzeComplexityTwoSites => self.analyze_complexity_two_sites(host, args),
            Builtin::AnalyzeComplexityDelta => self.analyze_complexity_delta(host, args),
            Builtin::CommunityComplexity => self.analyze_community_complexity(host, args),
            Builtin::AnalyzeMateSelection => self.analyze_mate_selection(host, args),
            Builtin::AverageModularity => self.command_average_modularity(host, args),
            Builtin::PrintResourceFitnessMap => self.command_print_resource_fitness_map(host, args),
            Builtin::AnalyzeRedundancyByInstFailure => {
                self.command_analyze_redundancy_by_inst_failure(host, args)
            }
        }
    }
}

/// The lines of an opened `cInitFile`.
pub(crate) fn file_lines(file: &InitFile) -> Vec<String> {
    (0..file.num_lines() as i32)
        .map(|i| file.get_line(i))
        .collect()
}

/// What `RunFile` writes when the analyze file is missing.
const DEFAULT_ANALYZE_FILE: &str = "\
################################################################################################
# This file is used to setup avida when it is in analysis-only mode, which can be triggered by
# running \"avida -a\".
#
# Please see the documentation in documentation/analyze.html for information on how to use
# analyze mode.
################################################################################################

";
