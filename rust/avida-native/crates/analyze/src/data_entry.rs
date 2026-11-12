//! The genotype data-entry table (`cAnalyzeGenotype::buildDataCommandManager`,
//! `tDataEntry`, `tDataEntryCommand`, `tDataCommandManager`): the keywords
//! `LOAD`, `DETAIL`, `FILTER` and friends use to read and write genotype
//! statistics.

use avida_config::string_util as su;
use avida_config::Feedback;

use crate::apto_map::{apto_map_key_order, apto_near_match};
use crate::error::{AnalyzeError, Result};
use crate::flex::FlexVar;
use crate::genotype::AnalyzeGenotype;
use crate::host::AnalyzeHost;

/// `FLEX_COMPARE_*` (cAnalyze.h).
pub mod flex_compare {
    pub const NONE: i32 = 0;
    pub const DIFF: i32 = 1;
    pub const MAX: i32 = 2;
    pub const MIN: i32 = 3;
    pub const DIFF2: i32 = 4;
    pub const MAX2: i32 = 5;
    pub const MIN2: i32 = 6;
}

/// The value read by an entry (`GetXxx` in `cAnalyzeGenotype`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Get {
    Name,
    Viable,
    Id,
    HwType,
    InstSet,
    Src,
    SrcArgs,
    Tag,
    Parents,
    ParentId,
    Parent2Id,
    ParentDist,
    AncestorDist,
    Lineage,
    NumCpus,
    TotalCpus,
    Length,
    CopyLength,
    ExeLength,
    Merit,
    CompMerit,
    CompMeritRatio,
    GestTime,
    Efficiency,
    EfficiencyRatio,
    Fitness,
    DivType,
    MateId,
    FitnessRatio,
    UpdateBorn,
    UpdateDead,
    Depth,
    Cells,
    GestOffsets,
    FracDead,
    FracNeg,
    FracNeut,
    FracPos,
    Complexity,
    LandFitness,
    MatingType,
    MatePreference,
    MatingDisplayA,
    MatingDisplayB,
    NumPhen,
    NumTrials,
    PhenEntropy,
    PhenMaxFitness,
    PhenMaxFitFreq,
    PhenMinFitness,
    PhenMinFreq,
    PhenAvgFitness,
    PhenLikelyFreq,
    PhenLikelyFitness,
    ProbViable,
    ParentMuts,
    TaskOrder,
    Sequence,
    Alignment,
    ExecutedFlags,
    AlignmentExecutedFlags,
    TaskList,
    Link,
    Html,
    TotalTaskCount,
    TotalTaskPerformanceCount,
    // Indexed entries (`tDataEntryOfType<.., T (int)>`).
    Task,
    TaskQuality,
    EnvInput,
    Inst,
    RTot,
    RAvail,
    ProbTask,
    RSpec,
    // `cModularityAnalysis::Initialize` (`tDataEntryProxy`).
    ModTasksDone,
    ModInstsTasks,
    ModTasksProp,
    ModAveTasksPerSite,
    ModAveSitesPerTask,
    ModSitesPerTask,
    ModSitesInvXTasks,
    ModTaskLength,
    ModAveTaskPosition,
}

/// The setter of an entry (`SetXxx`, `SetNULL` = [`Set::Null`]); `None`
/// in the table for the indexed entries, whose `Set` returns false.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Set {
    Null,
    Name,
    Viable,
    Id,
    Src,
    SrcArgs,
    Tag,
    Parents,
    ParentId,
    Parent2Id,
    ParentDist,
    AncestorDist,
    Lineage,
    NumCpus,
    TotalCpus,
    Length,
    CopyLength,
    ExeLength,
    Merit,
    GestTime,
    Fitness,
    DivType,
    MateId,
    UpdateBorn,
    UpdateDead,
    Depth,
    Cells,
    GestOffsets,
    MatingType,
    MatePreference,
    MatingDisplayA,
    MatingDisplayB,
    ParentMuts,
    TaskOrder,
    Sequence,
    Alignment,
}

/// `tDataEntry<cAnalyzeGenotype>`.
#[derive(Debug)]
pub struct DataEntry {
    name: &'static str,
    desc: &'static str,
    get: Get,
    set: Option<Set>,
    compare: i32,
    null: &'static str,
    html: &'static str,
}

impl DataEntry {
    /// `GetName()`.
    pub fn name(&self) -> &'static str {
        self.name
    }
    /// `GetCompareType()`.
    pub fn compare_type(&self) -> i32 {
        self.compare
    }
    /// `GetNull()`.
    pub fn null(&self) -> &'static str {
        self.null
    }
    /// `GetHtmlCellFlags()`.
    pub fn html_cell_flags(&self) -> &'static str {
        self.html
    }
}

const AC: &str = "align=center";

/// `ADD_GDATA(TYPE, KEYWORD, DESC, GET, SET, COMP, NSTR, HSTR)`: `NSTR` of
/// `0` means "0", `HSTR` of `0` means "align=center".
const fn g(
    name: &'static str,
    desc: &'static str,
    get: Get,
    set: Set,
    compare: i32,
    null: &'static str,
    html: &'static str,
) -> DataEntry {
    DataEntry {
        name,
        desc,
        get,
        set: Some(set),
        compare,
        null,
        html,
    }
}

/// An indexed entry: the description is the name (the per-index text
/// comes from the `Desc*` function), null "0", html "align=center".
const fn ix(name: &'static str, get: Get, compare: i32) -> DataEntry {
    DataEntry {
        name,
        desc: name,
        get,
        set: None,
        compare,
        null: "0",
        html: AC,
    }
}

/// A `tDataEntryProxy` of `cModularityAnalysis` (no setter, null "0",
/// html "align=center").
const fn px(name: &'static str, desc: &'static str, get: Get, compare: i32) -> DataEntry {
    DataEntry {
        name,
        desc,
        get,
        set: None,
        compare,
        null: "0",
        html: AC,
    }
}

/// Every entry, in `dcm->Add` order; the `cModularityAnalysis` entries
/// come last (`Avida::Initialize` adds them to the finished manager).
static ENTRIES: &[DataEntry] = &[
    g("name", "Genotype Name", Get::Name, Set::Name, 0, "0", AC),
    g(
        "viable",
        "Is Viable (0/1)",
        Get::Viable,
        Set::Viable,
        5,
        "0",
        AC,
    ),
    g("id", "Genotype ID", Get::Id, Set::Id, 0, "0", AC),
    g("hw_type", "HW Type", Get::HwType, Set::Null, 0, "0", AC),
    g("inst_set", "Instset", Get::InstSet, Set::Null, 0, "0", AC),
    g(
        "src",
        "Genotype Transmission Type",
        Get::Src,
        Set::Src,
        0,
        "0",
        AC,
    ),
    g(
        "src_args",
        "Genotype Source Arguments",
        Get::SrcArgs,
        Set::SrcArgs,
        0,
        "(none)",
        AC,
    ),
    g("tag", "Genotype Tag", Get::Tag, Set::Tag, 0, "(none)", ""),
    g(
        "parents",
        "Parent String",
        Get::Parents,
        Set::Parents,
        0,
        "(none)",
        AC,
    ),
    g(
        "parent_id",
        "Parent ID",
        Get::ParentId,
        Set::ParentId,
        0,
        "0",
        AC,
    ),
    g(
        "parent2_id",
        "Second Parent ID (sexual orgs)",
        Get::Parent2Id,
        Set::Parent2Id,
        0,
        "0",
        AC,
    ),
    g(
        "parent_dist",
        "Parent Distance",
        Get::ParentDist,
        Set::ParentDist,
        0,
        "0",
        AC,
    ),
    g(
        "ancestor_dist",
        "Ancestor Distance",
        Get::AncestorDist,
        Set::AncestorDist,
        0,
        "0",
        AC,
    ),
    g(
        "lineage",
        "Unique Lineage Label",
        Get::Lineage,
        Set::Lineage,
        0,
        "0",
        AC,
    ),
    g(
        "num_cpus",
        "Number of CPUs",
        Get::NumCpus,
        Set::NumCpus,
        0,
        "0",
        AC,
    ),
    g(
        "total_cpus",
        "Total CPUs Ever",
        Get::TotalCpus,
        Set::TotalCpus,
        0,
        "0",
        AC,
    ),
    g(
        "num_units",
        "Number of CPUs",
        Get::NumCpus,
        Set::NumCpus,
        0,
        "0",
        AC,
    ),
    g(
        "total_units",
        "Total CPUs Ever",
        Get::TotalCpus,
        Set::TotalCpus,
        0,
        "0",
        AC,
    ),
    g(
        "length",
        "Genome Length",
        Get::Length,
        Set::Length,
        4,
        "0",
        AC,
    ),
    g(
        "copy_length",
        "Copied Length",
        Get::CopyLength,
        Set::CopyLength,
        0,
        "0",
        AC,
    ),
    g(
        "exe_length",
        "Executed Length",
        Get::ExeLength,
        Set::ExeLength,
        0,
        "0",
        AC,
    ),
    g("merit", "Merit", Get::Merit, Set::Merit, 5, "0", AC),
    g(
        "comp_merit",
        "Computational Merit",
        Get::CompMerit,
        Set::Null,
        5,
        "0",
        AC,
    ),
    g(
        "comp_merit_ratio",
        "Computational Merit Ratio",
        Get::CompMeritRatio,
        Set::Null,
        5,
        "0",
        AC,
    ),
    g(
        "gest_time",
        "Gestation Time",
        Get::GestTime,
        Set::GestTime,
        6,
        "Inf",
        AC,
    ),
    g(
        "efficiency",
        "Rep. Efficiency",
        Get::Efficiency,
        Set::Null,
        5,
        "0",
        AC,
    ),
    g(
        "efficiency_ratio",
        "Rep. Efficiency Ratio",
        Get::EfficiencyRatio,
        Set::Null,
        5,
        "0",
        AC,
    ),
    g("fitness", "Fitness", Get::Fitness, Set::Fitness, 5, "0", AC),
    g(
        "div_type",
        "Divide Type",
        Get::DivType,
        Set::DivType,
        0,
        "0",
        AC,
    ),
    g(
        "mate_id",
        "Mate Selection ID Number",
        Get::MateId,
        Set::MateId,
        0,
        "0",
        AC,
    ),
    g(
        "fitness_ratio",
        "Fitness Ratio",
        Get::FitnessRatio,
        Set::Null,
        5,
        "0",
        AC,
    ),
    g(
        "update_born",
        "Update Born",
        Get::UpdateBorn,
        Set::UpdateBorn,
        0,
        "0",
        AC,
    ),
    g(
        "gen_born",
        "Update Born",
        Get::UpdateBorn,
        Set::UpdateBorn,
        0,
        "0",
        AC,
    ),
    g(
        "update_dead",
        "Update Dead",
        Get::UpdateDead,
        Set::UpdateDead,
        0,
        "0",
        AC,
    ),
    g(
        "update_deactivated",
        "Update Dead",
        Get::UpdateDead,
        Set::UpdateDead,
        0,
        "0",
        AC,
    ),
    g("depth", "Tree Depth", Get::Depth, Set::Depth, 0, "0", AC),
    g("cells", "Cells", Get::Cells, Set::Cells, 0, "0", AC),
    g(
        "gest_offset",
        "Gest Offsets",
        Get::GestOffsets,
        Set::GestOffsets,
        0,
        "0",
        AC,
    ),
    g(
        "frac_dead",
        "Fraction Mutations Lethal",
        Get::FracDead,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "frac_neg",
        "Fraction Mutations Detrimental",
        Get::FracNeg,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "frac_neut",
        "Fraction Mutations Neutral",
        Get::FracNeut,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "frac_pos",
        "Fraction Mutations Beneficial",
        Get::FracPos,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "complexity",
        "Basic Complexity (beneficial muts are neutral)",
        Get::Complexity,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "land_fitness",
        "Average Lanscape Fitness",
        Get::LandFitness,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "mating_type",
        "Mating type (-1 = juvenile; 0 = female; 1 = male)",
        Get::MatingType,
        Set::MatingType,
        0,
        "0",
        AC,
    ),
    g(
        "mate_preference",
        "Mate preference",
        Get::MatePreference,
        Set::MatePreference,
        0,
        "0",
        AC,
    ),
    g(
        "mating_display_a",
        "Mating display A",
        Get::MatingDisplayA,
        Set::MatingDisplayA,
        0,
        "0",
        AC,
    ),
    g(
        "mating_display_b",
        "Mating display B",
        Get::MatingDisplayB,
        Set::MatingDisplayB,
        0,
        "0",
        AC,
    ),
    g(
        "num_phen",
        "Number of Plastic Phenotypes",
        Get::NumPhen,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "num_trials",
        "Number of Recalculation Trials",
        Get::NumTrials,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "phen_entropy",
        "Phenotpyic Entropy",
        Get::PhenEntropy,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "phen_max_fitness",
        "Phen Plast Maximum Fitness",
        Get::PhenMaxFitness,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "phen_max_fit_freq",
        "Phen Plast Maximum Fitness Frequency",
        Get::PhenMaxFitFreq,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "phen_min_fitness",
        "Phen Plast Minimum Fitness",
        Get::PhenMinFitness,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "phen_min_freq",
        "Phen Plast Minimum Fitness Frequency",
        Get::PhenMinFreq,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "phen_avg_fitness",
        "Phen Plast Wtd Avg Fitness",
        Get::PhenAvgFitness,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "phen_likely_freq",
        "Freq of Most Likely Phenotype",
        Get::PhenLikelyFreq,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "phen_likely_fitness",
        "Fitness of Most Likely Phenotype",
        Get::PhenLikelyFitness,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "prob_viable",
        "Probability Viable",
        Get::ProbViable,
        Set::Null,
        0,
        "0",
        AC,
    ),
    g(
        "parent_muts",
        "Mutations from Parent",
        Get::ParentMuts,
        Set::ParentMuts,
        0,
        "(none)",
        "",
    ),
    g(
        "task_order",
        "Task Performance Order",
        Get::TaskOrder,
        Set::TaskOrder,
        0,
        "(none)",
        "",
    ),
    g(
        "sequence",
        "Genome Sequence",
        Get::Sequence,
        Set::Sequence,
        0,
        "(N/A)",
        "",
    ),
    g(
        "alignment",
        "Aligned Sequence",
        Get::Alignment,
        Set::Alignment,
        0,
        "(N/A)",
        "",
    ),
    g(
        "executed_flags",
        "Executed Flags",
        Get::ExecutedFlags,
        Set::Null,
        0,
        "(N/A)",
        "",
    ),
    g(
        "alignment_executed_flags",
        "Alignment Executed Flags",
        Get::AlignmentExecutedFlags,
        Set::Null,
        0,
        "(N/A)",
        "",
    ),
    g(
        "task_list",
        "List of all tasks performed",
        Get::TaskList,
        Set::Null,
        0,
        "(N/A)",
        "",
    ),
    g("link", "Phenotype Map", Get::Link, Set::Null, 0, "0", AC),
    g(
        "html",
        "Genome Sequence",
        Get::Html,
        Set::Null,
        0,
        "(N/A)",
        "",
    ),
    g(
        "total_task_count",
        "# Different Tasks",
        Get::TotalTaskCount,
        Set::Null,
        1,
        "0",
        AC,
    ),
    g(
        "total_task_performance_count",
        "Total Tasks Performed",
        Get::TotalTaskPerformanceCount,
        Set::Null,
        1,
        "0",
        AC,
    ),
    ix("task", Get::Task, 5),
    ix("task_quality", Get::TaskQuality, 5),
    ix("env_input", Get::EnvInput, 0),
    ix("inst", Get::Inst, 0),
    ix("r_tot", Get::RTot, 0),
    ix("r_avail", Get::RAvail, 0),
    ix("prob_task", Get::ProbTask, 5),
    ix("r_spec", Get::RSpec, 0),
    g(
        "update",
        "Update Output",
        Get::UpdateDead,
        Set::UpdateDead,
        0,
        "0",
        AC,
    ),
    g(
        "dom_num_cpus",
        "Number of Dominant Organisms",
        Get::NumCpus,
        Set::NumCpus,
        0,
        "0",
        AC,
    ),
    g(
        "dom_depth",
        "Tree Depth of Dominant Genotype",
        Get::Depth,
        Set::Depth,
        0,
        "0",
        AC,
    ),
    g(
        "dom_id",
        "Dominant Genotype ID",
        Get::Id,
        Set::Id,
        0,
        "0",
        AC,
    ),
    g(
        "dom_sequence",
        "Dominant Genotype Sequence",
        Get::Sequence,
        Set::Null,
        0,
        "(N/A)",
        "",
    ),
    px(
        "tasks_done",
        "Number of Tasks Performed",
        Get::ModTasksDone,
        2,
    ),
    px(
        "insts_tasks",
        "Number of Instructions Involved in Tasks",
        Get::ModInstsTasks,
        1,
    ),
    px(
        "tasks_prop",
        "Proportion of Sites in Tasks",
        Get::ModTasksProp,
        4,
    ),
    px(
        "ave_tasks_per_site",
        "Average Number of Tasks Per Site",
        Get::ModAveTasksPerSite,
        6,
    ),
    px(
        "ave_sites_per_task",
        "Average Number of Sites Per Task",
        Get::ModAveSitesPerTask,
        6,
    ),
    // QUIRK: registered with GetTaskProportionFor, so it reads tasks_prop.
    px(
        "ave_prop_nonoverlap",
        "Average Proportion of the Non-overlapping Region of a Task",
        Get::ModTasksProp,
        4,
    ),
    ix("sites_per_task", Get::ModSitesPerTask, 1),
    ix("sites_inv_x_tasks", Get::ModSitesInvXTasks, 1),
    ix("task_length", Get::ModTaskLength, 4),
    ix("ave_task_position", Get::ModAveTaskPosition, 4),
];

/// All entries in registration order.
pub fn entries() -> &'static [DataEntry] {
    ENTRIES
}

fn find_entry(name: &str) -> Option<&'static DataEntry> {
    ENTRIES.iter().find(|e| e.name == name)
}

/// `tDataEntryCommand<cAnalyzeGenotype>`: an entry with its index
/// (`name.idx`) and `:`-separated arguments.
#[derive(Debug, Clone)]
pub struct DataEntryCommand {
    entry: &'static DataEntry,
    /// `m_idx`: a string `cFlexVar`, read with `As<int>()`.
    idx: String,
    args: Vec<String>,
}

impl DataEntryCommand {
    fn new(entry: &'static DataEntry, idx: &str, args: &str) -> Self {
        DataEntryCommand {
            entry,
            idx: idx.to_string(),
            args: su::split_list(args, ':'),
        }
    }

    /// `GetName()` (without the index).
    pub fn name(&self) -> &'static str {
        self.entry.name
    }
    /// `GetCompareType()`.
    pub fn compare_type(&self) -> i32 {
        self.entry.compare
    }
    /// `GetNull()`.
    pub fn null(&self) -> &'static str {
        self.entry.null
    }
    /// `GetHtmlCellFlags()`.
    pub fn html_cell_flags(&self) -> &'static str {
        self.entry.html
    }
    /// `HasArg(arg)`.
    pub fn has_arg(&self, arg: &str) -> bool {
        self.args.iter().any(|a| a == arg)
    }

    fn idx_int(&self) -> i32 {
        su::as_int(&self.idx)
    }

    /// `GetDesc(target)`: the entry description, or for indexed entries
    /// the `Desc*` text of the index. `target` is `None` where the C++
    /// passes the first genotype of an empty batch (NULL); the descriptions
    /// that read genotype or world state through it then crash in C++.
    pub fn desc(&self, target: Option<&AnalyzeGenotype>, host: &dyn AnalyzeHost) -> Result<String> {
        let id = self.idx_int();
        let null_target = || {
            AnalyzeError::fatal(format!(
                "description of '{}' through a NULL genotype (empty batch)",
                self.entry.name
            ))
        };
        match self.entry.get {
            Get::Task | Get::TaskQuality => {
                target.ok_or_else(null_target)?;
                desc_task(host, id)
            }
            Get::ProbTask => {
                target.ok_or_else(null_target)?;
                if id > host.num_tasks() {
                    return Ok(String::new());
                }
                Ok(format!("{} (Probability)", desc_task(host, id)?))
            }
            Get::EnvInput => Ok(format!("task.{id}")),
            Get::Inst => {
                let t = target.ok_or_else(null_target)?;
                desc_inst_exe(t, host, id)
            }
            Get::RTot => Ok(format!("Resource {id} Total")),
            Get::RAvail => Ok(format!("Resource {id} Available")),
            Get::RSpec => Ok(format!("# times specification {id} used")),
            Get::ModSitesPerTask => Ok(format!("Number of Sites Per Task {id}")),
            Get::ModSitesInvXTasks => Ok(format!("Number of Sites Involved in {id} Tasks")),
            Get::ModTaskLength => Ok(format!("Task {id} Length")),
            Get::ModAveTaskPosition => Ok(format!("Task {id} Position")),
            _ => Ok(self.entry.desc.to_string()),
        }
    }

    /// `SetValue(target, value)`: returns whether the entry has a setter.
    pub fn set_value(&self, t: &mut AnalyzeGenotype, value: &str) -> bool {
        let Some(set) = self.entry.set else {
            return false;
        };
        let int = || su::as_int(value);
        let dbl = || su::as_double(value);
        match set {
            Set::Null => {}
            Set::Name => t.name = value.to_string(),
            Set::Viable => t.viable = su::as_int(value) != 0,
            Set::Id => t.id_num = int(),
            Set::Src => t.set_source(int()),
            Set::SrcArgs => t.src_args = value.to_string(),
            Set::Tag => t.tag = value.to_string(),
            Set::Parents => t.set_parents(value),
            Set::ParentId => t.set_parent_id(int()),
            Set::Parent2Id => t.set_parent2_id(int()),
            Set::ParentDist => t.parent_dist = int(),
            Set::AncestorDist => t.ancestor_dist = int(),
            Set::Lineage => t.lineage_label = int(),
            Set::NumCpus => t.num_cpus = int(),
            Set::TotalCpus => t.total_cpus = int(),
            Set::Length => t.length = int(),
            Set::CopyLength => t.copy_length = int(),
            Set::ExeLength => t.exe_length = int(),
            Set::Merit => t.merit = dbl(),
            Set::GestTime => t.gest_time = int(),
            Set::Fitness => t.fitness = dbl(),
            Set::DivType => t.div_type = dbl(),
            Set::MateId => t.mate_id = int(),
            Set::UpdateBorn => t.update_born = int(),
            Set::UpdateDead => t.update_dead = int(),
            Set::Depth => t.depth = int(),
            Set::Cells => t.cells = value.to_string(),
            Set::GestOffsets => t.gest_offsets = value.to_string(),
            Set::MatingType => t.mating_type = int(),
            Set::MatePreference => t.mate_preference = int(),
            Set::MatingDisplayA => t.mating_display_a = int(),
            Set::MatingDisplayB => t.mating_display_b = int(),
            Set::ParentMuts => t.parent_muts = value.to_string(),
            Set::TaskOrder => t.task_order = value.to_string(),
            Set::Sequence => t.set_sequence(value),
            Set::Alignment => t.aligned_sequence = value.to_string(),
        }
        true
    }

    /// `GetValue(target)`. Entries backed by the landscape or the
    /// phenotypic-plasticity summary compute it through `host` on first use
    /// and cache it in the genotype, as the C++ `CheckLand` /
    /// `CheckPhenPlast` do.
    pub fn value(&self, t: &mut AnalyzeGenotype, host: &mut dyn AnalyzeHost) -> Result<FlexVar> {
        use FlexVar::{Double as D, Int as I, Str as S};
        let id = self.idx_int();
        Ok(match self.entry.get {
            Get::Name => S(t.name.clone()),
            Get::Viable => I(t.viable as i32),
            Get::Id => I(t.id_num),
            Get::HwType => I(t.genome.hardware_type()),
            Get::InstSet => S(t.inst_set()),
            Get::Src => I(t.src),
            Get::SrcArgs => S(t.src_args.clone()),
            Get::Tag => S(t.tag.clone()),
            Get::Parents => S(t.parent_str.clone()),
            Get::ParentId => I(t.parent_id),
            Get::Parent2Id => I(t.parent2_id),
            Get::ParentDist => I(t.parent_dist),
            Get::AncestorDist => I(t.ancestor_dist),
            Get::Lineage => I(t.lineage_label),
            Get::NumCpus => I(t.num_cpus),
            Get::TotalCpus => I(t.total_cpus),
            Get::Length => I(t.length),
            Get::CopyLength => I(t.copy_length),
            Get::ExeLength => I(t.exe_length),
            Get::Merit => D(t.merit),
            Get::CompMerit => D(t.comp_merit()),
            Get::CompMeritRatio => D(t.comp_merit_ratio),
            Get::GestTime => I(t.gest_time),
            Get::Efficiency => D(t.efficiency()),
            Get::EfficiencyRatio => D(t.efficiency_ratio),
            Get::Fitness => D(t.fitness),
            Get::DivType => D(t.div_type),
            Get::MateId => I(t.mate_id),
            Get::FitnessRatio => D(t.fitness_ratio),
            Get::UpdateBorn => I(t.update_born),
            Get::UpdateDead => I(t.update_dead),
            Get::Depth => I(t.depth),
            Get::Cells => S(t.cells.clone()),
            Get::GestOffsets => S(t.gest_offsets.clone()),
            Get::FracDead => D(t.landscape(host)?.prob_dead()),
            Get::FracNeg => D(t.landscape(host)?.prob_neg()),
            Get::FracNeut => D(t.landscape(host)?.prob_neut()),
            Get::FracPos => D(t.landscape(host)?.prob_pos()),
            Get::Complexity => D(t.landscape(host)?.complexity()),
            Get::LandFitness => D(t.landscape(host)?.ave_fitness()),
            Get::MatingType => I(t.mating_type),
            Get::MatePreference => I(t.mate_preference),
            Get::MatingDisplayA => I(t.mating_display_a),
            Get::MatingDisplayB => I(t.mating_display_b),
            Get::NumPhen => I(t.phen_plast_stats(host)?.num_phenotypes),
            Get::NumTrials => I(t.phen_plast_stats(host)?.recalculate_trials),
            Get::PhenEntropy => D(t.phen_plast_stats(host)?.phenotypic_entropy),
            Get::PhenMaxFitness => D(t.phen_plast_stats(host)?.max_fitness),
            // QUIRK: GetMaximumFitnessFrequency returns m_min_fit_frequency.
            Get::PhenMaxFitFreq => D(t.phen_plast_stats(host)?.min_fit_frequency),
            Get::PhenMinFitness => D(t.phen_plast_stats(host)?.min_fitness),
            Get::PhenMinFreq => D(t.phen_plast_stats(host)?.min_fit_frequency),
            Get::PhenAvgFitness => D(t.phen_plast_stats(host)?.avg_fitness),
            Get::PhenLikelyFreq => D(t.phen_plast_stats(host)?.likely_frequency),
            Get::PhenLikelyFitness => D(t.phen_plast_stats(host)?.likely_fitness),
            Get::ProbViable => D(t.phen_plast_stats(host)?.viable_probability),
            Get::ParentMuts => S(t.parent_muts.clone()),
            Get::TaskOrder => S(t.task_order.clone()),
            Get::Sequence => S(t.sequence()),
            Get::Alignment => S(t.aligned_sequence.clone()),
            Get::ExecutedFlags => S(t.executed_flags.clone()),
            Get::AlignmentExecutedFlags => S(t.alignment_executed_flags()?),
            Get::TaskList => S(t.task_list()),
            Get::Link => S(t.map_link()),
            Get::Html => S(t.html_sequence()),
            Get::TotalTaskCount => I(t.total_task_count()),
            Get::TotalTaskPerformanceCount => I(t.total_task_performance_count()),
            Get::Task => {
                let c = t.task_count(id)?;
                if id < t.num_tasks() && self.has_arg("binary") {
                    I((c > 0) as i32)
                } else {
                    I(c)
                }
            }
            Get::TaskQuality => D(t.task_quality(id)?),
            Get::EnvInput => I(t.env_input(id)?),
            Get::Inst => I(t.inst_executed_count(id)),
            Get::RTot => D(t.rbin_total(id)?),
            Get::RAvail => D(t.rbin_avail(id)?),
            Get::ProbTask => D(t.task_probability(host, id)?),
            Get::RSpec => I(t.r_spec(id)),
            Get::ModTasksDone => I(t.data.modularity().map_or(0, |d| d.tasks_done)),
            Get::ModInstsTasks => I(t.data.modularity().map_or(0, |d| d.insts_tasks)),
            Get::ModTasksProp => D(t.data.modularity().map_or(0.0, |d| d.tasks_prop)),
            Get::ModAveTasksPerSite => D(t.data.modularity().map_or(0.0, |d| d.ave_tasks_per_site)),
            Get::ModAveSitesPerTask => D(t.data.modularity().map_or(0.0, |d| d.ave_sites_per_task)),
            // QUIRK: every indexed getter bounds the index by the size of
            // sites_per_task (the task count), so the last of the
            // num_tasks + 1 sites_inv_x_tasks counts is unreachable.
            Get::ModSitesPerTask => I(modularity_at(t, id, |d| &d.sites_per_task).unwrap_or(0)),
            Get::ModSitesInvXTasks => {
                I(modularity_at(t, id, |d| &d.sites_inv_x_tasks).unwrap_or(0))
            }
            Get::ModTaskLength => I(modularity_at(t, id, |d| &d.task_length).unwrap_or(0)),
            Get::ModAveTaskPosition => {
                D(modularity_at(t, id, |d| &d.ave_task_position).unwrap_or(0.0))
            }
        })
    }
}

/// The `idx` entry of a `cModularityData` array, for `0 <= idx <
/// sites_per_task.GetSize()`.
fn modularity_at<T: Copy>(
    t: &AnalyzeGenotype,
    idx: i32,
    field: impl Fn(&crate::genotype::ModularityData) -> &Vec<T>,
) -> Option<T> {
    let d = t.data.modularity()?;
    if idx < 0 || idx as usize >= d.sites_per_task.len() {
        return None;
    }
    field(&d).get(idx as usize).copied()
}

/// `cAnalyzeGenotype::DescTask(task_id)`.
/// QUIRK: the bound is `task_id > num_tasks`, so `task_id == num_tasks`
/// reaches the task array (the host reports that as fatal).
fn desc_task(host: &dyn AnalyzeHost, id: i32) -> Result<String> {
    if id > host.num_tasks() {
        return Ok(String::new());
    }
    host.task_desc(id)
}

/// `cAnalyzeGenotype::DescInstExe(inst_id)`.
fn desc_inst_exe(t: &AnalyzeGenotype, host: &dyn AnalyzeHost, id: i32) -> Result<String> {
    if id > t.inst_executed_counts.len() as i32 || id < 0 {
        return Ok(String::new());
    }
    let is = host
        .inst_sets()
        .get_inst_set(t.genome.inst_set())
        .ok_or_else(|| {
            AnalyzeError::fatal(format!(
                "DescInstExe: unknown instruction set '{}'",
                t.genome.inst_set()
            ))
        })?;
    if id >= is.size() {
        return Err(AnalyzeError::fatal(format!(
            "DescInstExe: instruction {id} beyond instruction set '{}' (C++ reads past the array)",
            is.inst_set_name()
        )));
    }
    Ok(format!("# Times {} Executed", is.name(id)))
}

/// `tDataCommandManager::GetDataCommand(cmd, &error)`: `cmd` is
/// `name[.idx][:arg[:arg..]]`. The error is the text the C++ puts in
/// `error_str` (including its "date entry" typo).
pub fn get_data_command(cmd: &str) -> std::result::Result<DataEntryCommand, String> {
    let mut arg_list = cmd.to_string();
    let mut idx = su::pop(&mut arg_list, ':');
    let entry_name = su::pop(&mut idx, '.');
    if let Some(e) = find_entry(&entry_name) {
        return Ok(DataEntryCommand::new(e, &idx, &arg_list));
    }
    let names: Vec<&str> = ENTRIES.iter().map(|e| e.name).collect();
    let nm = apto_near_match(&entry_name, &names);
    if !nm.is_empty() {
        Err(format!(
            "data entry '{entry_name}' not found, best match is '{nm}'"
        ))
    } else {
        Err(format!("date entry '{entry_name}' not found"))
    }
}

/// `tDataCommandManager::LoadCommandList(arg_list, output_list, feedback)`.
///
/// With no arguments every entry is listed in `Apto::Map` order with an
/// empty index (so indexed entries report index 0). Unknown names are
/// skipped; their errors go to `feedback` when given.
pub fn load_command_list(
    args: &[String],
    feedback: Option<&mut Feedback>,
) -> Vec<DataEntryCommand> {
    if args.is_empty() {
        let names: Vec<&str> = ENTRIES.iter().map(|e| e.name).collect();
        return apto_map_key_order(&names)
            .into_iter()
            .map(|n| DataEntryCommand::new(find_entry(n).expect("listed"), "", ""))
            .collect();
    }
    let mut out = Vec::new();
    let mut fb = feedback;
    for a in args {
        match get_data_command(a) {
            Ok(c) => out.push(c),
            Err(e) => {
                if let Some(f) = fb.as_deref_mut() {
                    f.error(e);
                }
            }
        }
    }
    out
}

/// `LoadCommandList(cStringList(cur_string), ...)`: the C++ converts the
/// remaining argument string with `cStringList(str, ' ')`, which splits on
/// every single space (empty words become unknown entries).
pub fn load_command_list_str(args: &str, feedback: Option<&mut Feedback>) -> Vec<DataEntryCommand> {
    load_command_list(&su::split_list(args, ' '), feedback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_size_and_unique_names() {
        // 83 from cAnalyzeGenotype, 10 from cModularityAnalysis::Initialize.
        assert_eq!(ENTRIES.len(), 93);
        assert_eq!(ENTRIES[83].name, "tasks_done");
        for (i, e) in ENTRIES.iter().enumerate() {
            assert!(ENTRIES[..i].iter().all(|o| o.name != e.name), "{}", e.name);
        }
    }

    #[test]
    fn parse_data_command() {
        let c = get_data_command("task.3:binary").unwrap();
        assert_eq!(c.name(), "task");
        assert_eq!(c.idx_int(), 3);
        assert!(c.has_arg("binary"));
        // The reference binary (LOAD of a file whose #format holds the
        // misspelling) names 'num_cpus' for every one of these, run after
        // run: the quirky EditDistance, not a real Levenshtein distance.
        for w in [
            "fitnes", "lenght", "meritt", "gest_tim", "xyz", "num_cpu", "sequense",
        ] {
            assert_eq!(
                get_data_command(w).unwrap_err(),
                format!("data entry '{w}' not found, best match is 'num_cpus'")
            );
        }
    }
}
