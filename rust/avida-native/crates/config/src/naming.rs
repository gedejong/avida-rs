//! The two-way table between the C++ setting names (`avida.cfg`) and the
//! dotted paths of the experiment file (`avida.yaml`).
//!
//! It is the one source of truth for the grouping: the loader, `-set`, the
//! converter, the front ends' settings editors and
//! `docs/reference/config-yaml.md` (rewritten from it by the test
//! `reference_page_is_current`) all read it. Renaming a group is a change of
//! its constant; renaming a key or moving a setting to another group is a
//! change of its row in [`NAMES`].
//!
//! A row's target is a setting (`group.key`), a whole section of the file
//! (`EVENT_FILE` became `events`, `ENVIRONMENT_FILE` became `environment`,
//! `INST_SET` became `instruction_sets`), or nothing: `HARDWARE_TYPE` and
//! `INST_SET_LOAD_LEGACY` only told the C++ how to read a legacy
//! instruction-set file, which the converter now does once.

use crate::config::{all_entries, EntryDef, EntryType};
use crate::port_settings::{PortEntryDef, PORT_ENTRIES};

/// The section that holds the environment (resources, reactions, grids).
pub const SECTION_ENVIRONMENT: &str = "environment";
/// The section that holds the events.
pub const SECTION_EVENTS: &str = "events";
/// The section that holds the instruction sets.
pub const SECTION_INSTRUCTION_SETS: &str = "instruction_sets";
/// The sections, in the order a written file lists them (after the groups).
pub const SECTIONS: &[&str] = &[
    SECTION_INSTRUCTION_SETS,
    SECTION_ENVIRONMENT,
    SECTION_EVENTS,
];

/// A group of settings in the experiment file.
#[derive(Debug, Clone, Copy)]
pub struct GroupInfo {
    /// The group's key in the file.
    pub key: &'static str,
    /// What the group is about (written as a comment above it).
    pub title: &'static str,
}

/// Where a C++ setting went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A setting `group.key`.
    Setting(&'static str, &'static str),
    /// A whole section of the file.
    Section(&'static str),
    /// Only the converter reads it.
    ConverterOnly,
}

/// One row of the table.
#[derive(Debug, Clone, Copy)]
pub struct NameRow {
    /// The C++ name (primary name of `cAvidaConfig.h`, or of a port setting).
    pub old: &'static str,
    /// Its place in the experiment file.
    pub target: Target,
}

/// Where the definition of a setting comes from.
#[derive(Debug, Clone, Copy)]
pub enum SettingDef {
    /// A `cAvidaConfig.h` entry.
    Cpp(&'static EntryDef),
    /// A setting of the port ([`crate::port_settings`]).
    Port(&'static PortEntryDef),
}

/// A setting of the experiment file with its definition.
#[derive(Debug, Clone, Copy)]
pub struct SettingInfo {
    /// Group key.
    pub group: &'static str,
    /// Key inside the group.
    pub key: &'static str,
    /// The C++ name.
    pub old: &'static str,
    /// The definition (type, default, description).
    pub def: SettingDef,
}

impl SettingInfo {
    /// `group.key`.
    pub fn path(&self) -> String {
        format!("{}.{}", self.group, self.key)
    }

    /// The default value text.
    pub fn default(&self) -> &'static str {
        match self.def {
            SettingDef::Cpp(e) => e.default,
            SettingDef::Port(p) => p.default,
        }
    }

    /// The description (lines separated by `\n`).
    pub fn desc(&self) -> &'static str {
        match self.def {
            SettingDef::Cpp(e) => e.desc,
            SettingDef::Port(p) => p.desc,
        }
    }

    /// The value type (every port setting is an `int`).
    pub fn ty(&self) -> EntryType {
        match self.def {
            SettingDef::Cpp(e) => e.ty,
            SettingDef::Port(_) => EntryType::Int,
        }
    }

    /// The C++ aliases of the setting.
    pub fn aliases(&self) -> &'static [&'static str] {
        match self.def {
            SettingDef::Cpp(e) => &e.names[1..],
            SettingDef::Port(_) => &[],
        }
    }
}

fn def_of(old: &str) -> Option<SettingDef> {
    all_entries()
        .find(|e| e.names[0] == old)
        .map(SettingDef::Cpp)
        .or_else(|| {
            PORT_ENTRIES
                .iter()
                .find(|p| p.name == old)
                .map(SettingDef::Port)
        })
}

/// Every setting of the experiment file in file order: groups in
/// [`GROUPS`] order, keys in table order within a group.
pub fn settings() -> Vec<SettingInfo> {
    let mut out = Vec::new();
    for g in GROUPS {
        for row in NAMES {
            if let Target::Setting(group, key) = row.target {
                if group == g.key {
                    if let Some(def) = def_of(row.old) {
                        out.push(SettingInfo {
                            group,
                            key,
                            old: row.old,
                            def,
                        });
                    }
                }
            }
        }
    }
    out
}

/// The settings of one group, in file order.
pub fn settings_of_group(group: &str) -> Vec<SettingInfo> {
    settings()
        .into_iter()
        .filter(|s| s.group == group)
        .collect()
}

/// The setting at `group.key`.
pub fn setting_by_path(path: &str) -> Option<SettingInfo> {
    let (group, key) = path.split_once('.')?;
    setting_in_group(group, key)
}

/// The setting `key` of `group`.
pub fn setting_in_group(group: &str, key: &str) -> Option<SettingInfo> {
    NAMES.iter().find_map(|row| match row.target {
        Target::Setting(g, k) if g == group && k == key => def_of(row.old).map(|def| SettingInfo {
            group: g,
            key: k,
            old: row.old,
            def,
        }),
        _ => None,
    })
}

/// The row of a C++ name (primary or alias).
pub fn row_of_old(old: &str) -> Option<&'static NameRow> {
    let primary = all_entries()
        .find(|e| e.names.contains(&old))
        .map(|e| e.names[0])
        .unwrap_or(old);
    NAMES.iter().find(|r| r.old == primary)
}

/// Where the C++ name `old` (primary or alias) went, as text: `group.key`
/// or a section name; `None` for an unknown name or one only the converter
/// reads.
pub fn new_name_of(old: &str) -> Option<String> {
    match row_of_old(old)?.target {
        Target::Setting(g, k) => Some(format!("{g}.{k}")),
        Target::Section(s) => Some(s.to_string()),
        Target::ConverterOnly => None,
    }
}

/// Whether `key` is a group of the file.
pub fn is_group(key: &str) -> bool {
    GROUPS.iter().any(|g| g.key == key)
}

/// Every top-level key of an experiment file: the groups, then the sections.
pub fn top_level_keys() -> Vec<&'static str> {
    GROUPS
        .iter()
        .map(|g| g.key)
        .chain(SECTIONS.iter().copied())
        .collect()
}

/// Every dotted path `-set` accepts: the settings, then the sections.
pub fn all_paths() -> Vec<String> {
    settings()
        .iter()
        .map(SettingInfo::path)
        .chain(SECTIONS.iter().map(|s| s.to_string()))
        .collect()
}

/// Groups of the experiment file, in the order the file lists them.
/// Renaming a group is a change of its constant below.
pub const GENERAL: &str = "general";
pub const WORLD: &str = "world";
pub const MUTATION: &str = "mutation";
pub const REPRODUCTION: &str = "reproduction";
pub const DIVIDE: &str = "divide";
pub const RECOMBINATION: &str = "recombination";
pub const MATING: &str = "mating";
pub const PARASITES: &str = "parasites";
pub const CPU: &str = "cpu";
pub const MP: &str = "mp";
pub const DEMES: &str = "demes";
pub const REVERSION: &str = "reversion";
pub const TIME: &str = "time";
pub const ALTRUISM: &str = "altruism";
pub const KABOOM: &str = "kaboom";
pub const GENEALOGY: &str = "genealogy";
pub const NETWORK: &str = "network";
pub const MESSAGING: &str = "messaging";
pub const BUY_SELL: &str = "buy_sell";
pub const HOARDING: &str = "hoarding";
pub const ANALYZE: &str = "analyze";
pub const ENERGY: &str = "energy";
pub const ENERGY_SHARING: &str = "energy_sharing";
pub const SECOND_PASS: &str = "second_pass";
pub const GENE_EXPRESSION: &str = "gene_expression";
pub const PROMOTERS: &str = "promoters";
pub const COLORS: &str = "colors";
pub const MOVEMENT: &str = "movement";
pub const SENSING: &str = "sensing";
pub const PHEROMONE: &str = "pheromone";
pub const SYNC: &str = "sync";
pub const CONSENSUS: &str = "consensus";
pub const REPUTATION: &str = "reputation";
pub const GROUP_FORMATION: &str = "groups";
pub const DEME_NETWORK: &str = "deme_network";
pub const HGT: &str = "hgt";
pub const INST_RESOURCES: &str = "inst_resources";
pub const ALARM: &str = "alarm";
pub const DIVISION_OF_LABOR: &str = "division_of_labor";
pub const EXECUTION: &str = "execution";

/// Every group with its title (the comment above it in a written file).
pub static GROUPS: &[GroupInfo] = &[
    GroupInfo { key: GENERAL, title: "General settings" },
    GroupInfo { key: WORLD, title: "World size and topology" },
    GroupInfo { key: MUTATION, title: "Mutation rates" },
    GroupInfo { key: REPRODUCTION, title: "Birth and death" },
    GroupInfo { key: DIVIDE, title: "Divide restrictions and triggers (conditions for a successful divide)" },
    GroupInfo { key: RECOMBINATION, title: "Sexual recombination and modularity" },
    GroupInfo { key: MATING, title: "Mating types and mate choice" },
    GroupInfo { key: PARASITES, title: "Parasites" },
    GroupInfo { key: CPU, title: "Details on how the CPU works" },
    GroupInfo { key: MP, title: "Multiple, distributed populations" },
    GroupInfo { key: DEMES, title: "Demes and germlines" },
    GroupInfo { key: REVERSION, title: "Mutation reversion (most of these slow Avida down a lot and should normally be 0.0)" },
    GroupInfo { key: TIME, title: "Time slicing and merit" },
    GroupInfo { key: ALTRUISM, title: "Altruism" },
    GroupInfo { key: KABOOM, title: "Kaboom" },
    GroupInfo { key: GENEALOGY, title: "Genealogy" },
    GroupInfo { key: NETWORK, title: "Organism network communication" },
    GroupInfo { key: MESSAGING, title: "Organism message-based communication" },
    GroupInfo { key: BUY_SELL, title: "Buying and selling" },
    GroupInfo { key: HOARDING, title: "Resource hoarding" },
    GroupInfo { key: ANALYZE, title: "Analysis mode" },
    GroupInfo { key: ENERGY, title: "Energy model" },
    GroupInfo { key: ENERGY_SHARING, title: "Energy sharing" },
    GroupInfo { key: SECOND_PASS, title: "Tracking metrics known from an earlier run of the experiment" },
    GroupInfo { key: GENE_EXPRESSION, title: "Gene expression CPU" },
    GroupInfo { key: PROMOTERS, title: "Promoters" },
    GroupInfo { key: COLORS, title: "Output colours for data files printed in HTML mode (the first six for lineages, the rest for mutation tests)" },
    GroupInfo { key: MOVEMENT, title: "Movement and avatars" },
    GroupInfo { key: SENSING, title: "Sensing" },
    GroupInfo { key: PHEROMONE, title: "Pheromones" },
    GroupInfo { key: SYNC, title: "Synchronization" },
    GroupInfo { key: CONSENSUS, title: "Consensus" },
    GroupInfo { key: REPUTATION, title: "Reputation" },
    GroupInfo { key: GROUP_FORMATION, title: "Group formation and predator-prey" },
    GroupInfo { key: DEME_NETWORK, title: "Deme networks" },
    GroupInfo { key: HGT, title: "Horizontal gene transfer" },
    GroupInfo { key: INST_RESOURCES, title: "Resource-dependent instructions" },
    GroupInfo { key: ALARM, title: "Alarms" },
    GroupInfo { key: DIVISION_OF_LABOR, title: "Division of labor" },
    GroupInfo { key: EXECUTION, title: "Execution mode of the port (no C++ counterpart)" },
];

/// Every C++ setting (and every setting of the port) with the place it
/// has in the experiment file. One row per setting: renaming a key, or
/// moving a setting to another group, is a change of its row.
#[rustfmt::skip]
pub static NAMES: &[NameRow] = &[
    NameRow { old: "VERBOSITY", target: Target::Setting(GENERAL, "verbosity") },
    NameRow { old: "RANDOM_SEED", target: Target::Setting(GENERAL, "random_seed") },
    NameRow { old: "SPECULATIVE", target: Target::Setting(GENERAL, "speculative") },
    NameRow { old: "POPULATION_CAP", target: Target::Setting(GENERAL, "population_cap") },
    NameRow { old: "POP_CAP_ELDEST", target: Target::Setting(GENERAL, "population_cap_eldest") },
    NameRow { old: "WORLD_X", target: Target::Setting(WORLD, "x") },
    NameRow { old: "WORLD_Y", target: Target::Setting(WORLD, "y") },
    NameRow { old: "WORLD_GEOMETRY", target: Target::Setting(WORLD, "geometry") },
    NameRow { old: "SCALE_FREE_M", target: Target::Setting(WORLD, "scale_free_m") },
    NameRow { old: "SCALE_FREE_ALPHA", target: Target::Setting(WORLD, "scale_free_alpha") },
    NameRow { old: "SCALE_FREE_ZERO_APPEAL", target: Target::Setting(WORLD, "scale_free_zero_appeal") },
    NameRow { old: "DATA_DIR", target: Target::Setting(GENERAL, "data_dir") },
    NameRow { old: "EVENT_FILE", target: Target::Section("events") },
    NameRow { old: "ANALYZE_FILE", target: Target::Setting(ANALYZE, "file") },
    NameRow { old: "ENVIRONMENT_FILE", target: Target::Section("environment") },
    NameRow { old: "MIGRATION_FILE", target: Target::Setting(DEMES, "migration_file") },
    NameRow { old: "COPY_MUT_PROB", target: Target::Setting(MUTATION, "copy_prob") },
    NameRow { old: "COPY_INS_PROB", target: Target::Setting(MUTATION, "copy_ins_prob") },
    NameRow { old: "COPY_DEL_PROB", target: Target::Setting(MUTATION, "copy_del_prob") },
    NameRow { old: "COPY_UNIFORM_PROB", target: Target::Setting(MUTATION, "copy_uniform_prob") },
    NameRow { old: "COPY_SLIP_PROB", target: Target::Setting(MUTATION, "copy_slip_prob") },
    NameRow { old: "POINT_MUT_PROB", target: Target::Setting(MUTATION, "point_prob") },
    NameRow { old: "POINT_INS_PROB", target: Target::Setting(MUTATION, "point_ins_prob") },
    NameRow { old: "POINT_DEL_PROB", target: Target::Setting(MUTATION, "point_del_prob") },
    NameRow { old: "INST_POINT_MUT_PROB", target: Target::Setting(MUTATION, "inst_point_prob") },
    NameRow { old: "INST_POINT_MUT_SLOPE", target: Target::Setting(MUTATION, "inst_point_slope") },
    NameRow { old: "INST_POINT_REPAIR_COST", target: Target::Setting(MUTATION, "inst_point_repair_cost") },
    NameRow { old: "POINT_MUT_REPAIR_START", target: Target::Setting(MUTATION, "point_repair_start") },
    NameRow { old: "DIV_MUT_PROB", target: Target::Setting(MUTATION, "div_prob") },
    NameRow { old: "DIV_INS_PROB", target: Target::Setting(MUTATION, "div_ins_prob") },
    NameRow { old: "DIV_DEL_PROB", target: Target::Setting(MUTATION, "div_del_prob") },
    NameRow { old: "DIV_UNIFORM_PROB", target: Target::Setting(MUTATION, "div_uniform_prob") },
    NameRow { old: "DIV_SLIP_PROB", target: Target::Setting(MUTATION, "div_slip_prob") },
    NameRow { old: "DIV_TRANS_PROB", target: Target::Setting(MUTATION, "div_trans_prob") },
    NameRow { old: "DIV_LGT_PROB", target: Target::Setting(MUTATION, "div_lgt_prob") },
    NameRow { old: "DIVIDE_MUT_PROB", target: Target::Setting(MUTATION, "divide_prob") },
    NameRow { old: "DIVIDE_INS_PROB", target: Target::Setting(MUTATION, "divide_ins_prob") },
    NameRow { old: "DIVIDE_DEL_PROB", target: Target::Setting(MUTATION, "divide_del_prob") },
    NameRow { old: "DIVIDE_UNIFORM_PROB", target: Target::Setting(MUTATION, "divide_uniform_prob") },
    NameRow { old: "DIVIDE_SLIP_PROB", target: Target::Setting(MUTATION, "divide_slip_prob") },
    NameRow { old: "DIVIDE_TRANS_PROB", target: Target::Setting(MUTATION, "divide_trans_prob") },
    NameRow { old: "DIVIDE_LGT_PROB", target: Target::Setting(MUTATION, "divide_lgt_prob") },
    NameRow { old: "DIVIDE_POISSON_MUT_MEAN", target: Target::Setting(MUTATION, "divide_poisson_mean") },
    NameRow { old: "DIVIDE_POISSON_INS_MEAN", target: Target::Setting(MUTATION, "divide_poisson_ins_mean") },
    NameRow { old: "DIVIDE_POISSON_DEL_MEAN", target: Target::Setting(MUTATION, "divide_poisson_del_mean") },
    NameRow { old: "DIVIDE_POISSON_SLIP_MEAN", target: Target::Setting(MUTATION, "divide_poisson_slip_mean") },
    NameRow { old: "DIVIDE_POISSON_TRANS_MEAN", target: Target::Setting(MUTATION, "divide_poisson_trans_mean") },
    NameRow { old: "DIVIDE_POISSON_LGT_MEAN", target: Target::Setting(MUTATION, "divide_poisson_lgt_mean") },
    NameRow { old: "INJECT_MUT_PROB", target: Target::Setting(MUTATION, "inject_prob") },
    NameRow { old: "INJECT_INS_PROB", target: Target::Setting(MUTATION, "inject_ins_prob") },
    NameRow { old: "INJECT_DEL_PROB", target: Target::Setting(MUTATION, "inject_del_prob") },
    NameRow { old: "SLIP_FILL_MODE", target: Target::Setting(MUTATION, "slip_fill_mode") },
    NameRow { old: "SLIP_COPY_MODE", target: Target::Setting(MUTATION, "slip_copy_mode") },
    NameRow { old: "TRANS_FILL_MODE", target: Target::Setting(MUTATION, "trans_fill_mode") },
    NameRow { old: "LGT_FILL_MODE", target: Target::Setting(MUTATION, "lgt_fill_mode") },
    NameRow { old: "LGT_SOURCE_REGION", target: Target::Setting(MUTATION, "lgt_source_region") },
    NameRow { old: "PARENT_MUT_PROB", target: Target::Setting(MUTATION, "parent_prob") },
    NameRow { old: "PARENT_INS_PROB", target: Target::Setting(MUTATION, "parent_ins_prob") },
    NameRow { old: "PARENT_DEL_PROB", target: Target::Setting(MUTATION, "parent_del_prob") },
    NameRow { old: "SPECIAL_MUT_LINE", target: Target::Setting(MUTATION, "special_line") },
    NameRow { old: "META_COPY_MUT", target: Target::Setting(MUTATION, "meta_copy_prob") },
    NameRow { old: "META_STD_DEV", target: Target::Setting(MUTATION, "meta_std_dev") },
    NameRow { old: "MUT_RATE_SOURCE", target: Target::Setting(MUTATION, "rate_source") },
    NameRow { old: "DIVIDE_FAILURE_RESETS", target: Target::Setting(REPRODUCTION, "divide_failure_resets") },
    NameRow { old: "BIRTH_METHOD", target: Target::Setting(REPRODUCTION, "birth_method") },
    NameRow { old: "PREFER_EMPTY", target: Target::Setting(REPRODUCTION, "prefer_empty") },
    NameRow { old: "ALLOW_PARENT", target: Target::Setting(REPRODUCTION, "allow_parent") },
    NameRow { old: "DISPERSAL_RATE", target: Target::Setting(REPRODUCTION, "dispersal_rate") },
    NameRow { old: "DEATH_PROB", target: Target::Setting(REPRODUCTION, "death_prob") },
    NameRow { old: "DEATH_PROB_PARENT", target: Target::Setting(REPRODUCTION, "death_prob_parent") },
    NameRow { old: "DEATH_METHOD", target: Target::Setting(REPRODUCTION, "death_method") },
    NameRow { old: "AGE_LIMIT", target: Target::Setting(REPRODUCTION, "age_limit") },
    NameRow { old: "AGE_DEVIATION", target: Target::Setting(REPRODUCTION, "age_deviation") },
    NameRow { old: "JUV_PERIOD", target: Target::Setting(REPRODUCTION, "juvenile_period") },
    NameRow { old: "ALLOC_METHOD", target: Target::Setting(REPRODUCTION, "alloc_method") },
    NameRow { old: "DIVIDE_METHOD", target: Target::Setting(REPRODUCTION, "divide_method") },
    NameRow { old: "EPIGENETIC_METHOD", target: Target::Setting(REPRODUCTION, "epigenetic_method") },
    NameRow { old: "GENERATION_INC_METHOD", target: Target::Setting(REPRODUCTION, "generation_inc_method") },
    NameRow { old: "RESET_INPUTS_ON_DIVIDE", target: Target::Setting(REPRODUCTION, "reset_inputs_on_divide") },
    NameRow { old: "INHERIT_MERIT", target: Target::Setting(REPRODUCTION, "inherit_merit") },
    NameRow { old: "INHERIT_MULTITHREAD", target: Target::Setting(REPRODUCTION, "inherit_multithread") },
    NameRow { old: "OFFSPRING_SIZE_RANGE", target: Target::Setting(DIVIDE, "offspring_size_range") },
    NameRow { old: "MIN_COPIED_LINES", target: Target::Setting(DIVIDE, "min_copied_lines") },
    NameRow { old: "MIN_EXE_LINES", target: Target::Setting(DIVIDE, "min_exe_lines") },
    NameRow { old: "MIN_GENOME_SIZE", target: Target::Setting(DIVIDE, "min_genome_size") },
    NameRow { old: "MAX_GENOME_SIZE", target: Target::Setting(DIVIDE, "max_genome_size") },
    NameRow { old: "MIN_CYCLES", target: Target::Setting(DIVIDE, "min_cycles") },
    NameRow { old: "REQUIRE_ALLOCATE", target: Target::Setting(DIVIDE, "require_allocate") },
    NameRow { old: "REQUIRED_TASK", target: Target::Setting(DIVIDE, "required_task") },
    NameRow { old: "IMMUNITY_TASK", target: Target::Setting(DIVIDE, "immunity_task") },
    NameRow { old: "REQUIRED_REACTION", target: Target::Setting(DIVIDE, "required_reaction") },
    NameRow { old: "IMMUNITY_REACTION", target: Target::Setting(DIVIDE, "immunity_reaction") },
    NameRow { old: "REQUIRE_SINGLE_REACTION", target: Target::Setting(DIVIDE, "require_single_reaction") },
    NameRow { old: "MAX_UNIQUE_TASK_COUNT", target: Target::Setting(DIVIDE, "max_unique_task_count") },
    NameRow { old: "REQUIRED_BONUS", target: Target::Setting(DIVIDE, "required_bonus") },
    NameRow { old: "REQUIRE_EXACT_COPY", target: Target::Setting(DIVIDE, "require_exact_copy") },
    NameRow { old: "REQUIRED_RESOURCE", target: Target::Setting(DIVIDE, "required_resource") },
    NameRow { old: "REQUIRED_RESOURCE_LEVEL", target: Target::Setting(DIVIDE, "required_resource_level") },
    NameRow { old: "REQUIRED_PRED_HABITAT", target: Target::Setting(DIVIDE, "required_pred_habitat") },
    NameRow { old: "REQUIRED_PRED_HABITAT_VALUE", target: Target::Setting(DIVIDE, "required_pred_habitat_value") },
    NameRow { old: "REQUIRED_PREY_HABITAT", target: Target::Setting(DIVIDE, "required_prey_habitat") },
    NameRow { old: "REQUIRED_PREY_HABITAT_VALUE", target: Target::Setting(DIVIDE, "required_prey_habitat_value") },
    NameRow { old: "IMPLICIT_REPRO_BONUS", target: Target::Setting(DIVIDE, "implicit_repro_bonus") },
    NameRow { old: "IMPLICIT_REPRO_CPU_CYCLES", target: Target::Setting(DIVIDE, "implicit_repro_cpu_cycles") },
    NameRow { old: "IMPLICIT_REPRO_TIME", target: Target::Setting(DIVIDE, "implicit_repro_time") },
    NameRow { old: "IMPLICIT_REPRO_END", target: Target::Setting(DIVIDE, "implicit_repro_end") },
    NameRow { old: "IMPLICIT_REPRO_ENERGY", target: Target::Setting(DIVIDE, "implicit_repro_energy") },
    NameRow { old: "RECOMBINATION_PROB", target: Target::Setting(RECOMBINATION, "prob") },
    NameRow { old: "MAX_BIRTH_WAIT_TIME", target: Target::Setting(RECOMBINATION, "max_birth_wait_time") },
    NameRow { old: "MODULE_NUM", target: Target::Setting(RECOMBINATION, "module_num") },
    NameRow { old: "CONT_REC_REGS", target: Target::Setting(RECOMBINATION, "cont_rec_regs") },
    NameRow { old: "CORESPOND_REC_REGS", target: Target::Setting(RECOMBINATION, "correspond_rec_regs") },
    NameRow { old: "TWO_FOLD_COST_SEX", target: Target::Setting(RECOMBINATION, "two_fold_cost_sex") },
    NameRow { old: "SAME_LENGTH_SEX", target: Target::Setting(RECOMBINATION, "same_length_sex") },
    NameRow { old: "ALLOW_MATE_SELECTION", target: Target::Setting(RECOMBINATION, "allow_mate_selection") },
    NameRow { old: "MATING_TYPES", target: Target::Setting(MATING, "types") },
    NameRow { old: "LEKKING", target: Target::Setting(MATING, "lekking") },
    NameRow { old: "MAX_GLOBAL_BIRTH_CHAMBER_SIZE", target: Target::Setting(MATING, "max_global_birth_chamber_size") },
    NameRow { old: "DISABLE_GENOTYPE_CLASSIFICATION", target: Target::Setting(MATING, "disable_genotype_classification") },
    NameRow { old: "NOISY_MATE_ASSESSMENT", target: Target::Setting(MATING, "noisy_assessment") },
    NameRow { old: "MATE_ASSESSMENT_CV", target: Target::Setting(MATING, "assessment_cv") },
    NameRow { old: "FORCED_MATE_PREFERENCE", target: Target::Setting(MATING, "forced_preference") },
    NameRow { old: "MATE_IN_GROUPS", target: Target::Setting(MATING, "in_groups") },
    NameRow { old: "LOG_PARASITE_INJECTIONS", target: Target::Setting(PARASITES, "log_injections") },
    NameRow { old: "INJECT_METHOD", target: Target::Setting(PARASITES, "inject_method") },
    NameRow { old: "INFECTION_MECHANISM", target: Target::Setting(PARASITES, "infection_mechanism") },
    NameRow { old: "INJECT_QMA_EXPONENT", target: Target::Setting(PARASITES, "inject_qma_exponent") },
    NameRow { old: "INJECT_STERILIZES_HOST", target: Target::Setting(PARASITES, "inject_sterilizes_host") },
    NameRow { old: "INJECT_IS_VIRULENT", target: Target::Setting(PARASITES, "inject_is_virulent") },
    NameRow { old: "PARASITE_SKIP_REACTIONS", target: Target::Setting(PARASITES, "skip_reactions") },
    NameRow { old: "INJECT_SKIP_FIRST_TASK", target: Target::Setting(PARASITES, "inject_skip_first_task") },
    NameRow { old: "INJECT_DEFAULT_SUCCESS", target: Target::Setting(PARASITES, "inject_default_success") },
    NameRow { old: "PARASITE_VIRULENCE", target: Target::Setting(PARASITES, "virulence") },
    NameRow { old: "VIRULENCE_SOURCE", target: Target::Setting(PARASITES, "virulence_source") },
    NameRow { old: "VIRULENCE_MUT_RATE", target: Target::Setting(PARASITES, "virulence_mut_rate") },
    NameRow { old: "VIRULENCE_SD", target: Target::Setting(PARASITES, "virulence_sd") },
    NameRow { old: "PARASITE_MEM_SPACES", target: Target::Setting(PARASITES, "mem_spaces") },
    NameRow { old: "PARASITE_NO_COPY_MUT", target: Target::Setting(PARASITES, "no_copy_mut") },
    NameRow { old: "PARASITE_USE_GENOTYPE_FILE", target: Target::Setting(PARASITES, "use_genotype_file") },
    NameRow { old: "HOST_USE_GENOTYPE_FILE", target: Target::Setting(PARASITES, "host_use_genotype_file") },
    NameRow { old: "FULL_VERTICAL_TRANS", target: Target::Setting(PARASITES, "full_vertical_trans") },
    NameRow { old: "IO_EXPIRE", target: Target::Setting(CPU, "io_expire") },
    NameRow { old: "POISON_PENALTY", target: Target::Setting(CPU, "poison_penalty") },
    NameRow { old: "ENABLE_MP", target: Target::Setting(MP, "enabled") },
    NameRow { old: "MP_SCHEDULING_STYLE", target: Target::Setting(MP, "scheduling_style") },
    NameRow { old: "NUM_DEMES", target: Target::Setting(DEMES, "count") },
    NameRow { old: "DEMES_COMPETITION_STYLE", target: Target::Setting(DEMES, "competition_style") },
    NameRow { old: "DEMES_TOURNAMENT_SIZE", target: Target::Setting(DEMES, "tournament_size") },
    NameRow { old: "DEMES_OVERRIDE_FITNESS", target: Target::Setting(DEMES, "override_fitness") },
    NameRow { old: "DEMES_USE_GERMLINE", target: Target::Setting(DEMES, "use_germline") },
    NameRow { old: "DEMES_PREVENT_STERILE", target: Target::Setting(DEMES, "prevent_sterile") },
    NameRow { old: "DEMES_RESET_RESOURCES", target: Target::Setting(DEMES, "reset_resources") },
    NameRow { old: "DEMES_REPLICATE_SIZE", target: Target::Setting(DEMES, "replicate_size") },
    NameRow { old: "LOG_DEMES_REPLICATE", target: Target::Setting(DEMES, "log_replicate") },
    NameRow { old: "DEMES_REPLICATE_LOG_START", target: Target::Setting(DEMES, "replicate_log_start") },
    NameRow { old: "DEMES_PROB_ORG_TRANSFER", target: Target::Setting(DEMES, "prob_org_transfer") },
    NameRow { old: "DEMES_ORGANISM_SELECTION", target: Target::Setting(DEMES, "organism_selection") },
    NameRow { old: "DEMES_ORGANISM_PLACEMENT", target: Target::Setting(DEMES, "organism_placement") },
    NameRow { old: "DEMES_ORGANISM_FACING", target: Target::Setting(DEMES, "organism_facing") },
    NameRow { old: "DEMES_MAX_AGE", target: Target::Setting(DEMES, "max_age") },
    NameRow { old: "DEMES_MAX_BIRTHS", target: Target::Setting(DEMES, "max_births") },
    NameRow { old: "DEMES_MIM_EVENTS_KILLED_RATIO", target: Target::Setting(DEMES, "mim_events_killed_ratio") },
    NameRow { old: "DEMES_MIM_SUCCESSFUL_EVENT_PERIODS", target: Target::Setting(DEMES, "mim_successful_event_periods") },
    NameRow { old: "GERMLINE_COPY_MUT", target: Target::Setting(DEMES, "germline_copy_mut") },
    NameRow { old: "GERMLINE_INS_MUT", target: Target::Setting(DEMES, "germline_ins_mut") },
    NameRow { old: "GERMLINE_DEL_MUT", target: Target::Setting(DEMES, "germline_del_mut") },
    NameRow { old: "DEMES_REPLICATE_CPU_CYCLES", target: Target::Setting(DEMES, "replicate_cpu_cycles") },
    NameRow { old: "DEMES_REPLICATE_TIME", target: Target::Setting(DEMES, "replicate_time") },
    NameRow { old: "DEMES_REPLICATE_BIRTHS", target: Target::Setting(DEMES, "replicate_births") },
    NameRow { old: "DEMES_REPLICATE_ORGS", target: Target::Setting(DEMES, "replicate_orgs") },
    NameRow { old: "DEMES_REPLICATION_ONLY_RESETS", target: Target::Setting(DEMES, "replication_only_resets") },
    NameRow { old: "DEMES_MIGRATION_RATE", target: Target::Setting(DEMES, "migration_rate") },
    NameRow { old: "DEMES_PARASITE_MIGRATION_RATE", target: Target::Setting(DEMES, "parasite_migration_rate") },
    NameRow { old: "DEMES_MIGRATION_METHOD", target: Target::Setting(DEMES, "migration_method") },
    NameRow { old: "DEMES_PARASITE_MIGRATION_TARGET_SELECTION_METHOD", target: Target::Setting(DEMES, "parasite_migration_target_selection_method") },
    NameRow { old: "DEMES_PARASITE_MIGRATION_MEMORY_SCORE_PROTECTIVE_THRESHOLD", target: Target::Setting(DEMES, "parasite_migration_memory_score_protective_threshold") },
    NameRow { old: "DEMES_NUM_X", target: Target::Setting(DEMES, "num_x") },
    NameRow { old: "DEMES_PARTITION_INTERVAL", target: Target::Setting(DEMES, "partition_interval") },
    NameRow { old: "DEMES_SEED_METHOD", target: Target::Setting(DEMES, "seed_method") },
    NameRow { old: "DEMES_DIVIDE_METHOD", target: Target::Setting(DEMES, "divide_method") },
    NameRow { old: "DEMES_DEFAULT_GERMLINE_PROPENSITY", target: Target::Setting(DEMES, "default_germline_propensity") },
    NameRow { old: "DEMES_FOUNDER_GERMLINE_PROPENSITY", target: Target::Setting(DEMES, "founder_germline_propensity") },
    NameRow { old: "DEMES_PREFER_EMPTY", target: Target::Setting(DEMES, "prefer_empty") },
    NameRow { old: "DEMES_PROTECTION_POINTS", target: Target::Setting(DEMES, "protection_points") },
    NameRow { old: "MIGRATION_RATE", target: Target::Setting(DEMES, "uniform_migration_rate") },
    NameRow { old: "DEMES_TRACK_SHANNON_INFO", target: Target::Setting(DEMES, "track_shannon_info") },
    NameRow { old: "DEMES_MUT_ORGS_ON_REPLICATION", target: Target::Setting(DEMES, "mut_orgs_on_replication") },
    NameRow { old: "DEMES_ORGS_START_IN_GERM", target: Target::Setting(DEMES, "orgs_start_in_germ") },
    NameRow { old: "REVERT_FATAL", target: Target::Setting(REVERSION, "revert_fatal") },
    NameRow { old: "REVERT_DETRIMENTAL", target: Target::Setting(REVERSION, "revert_detrimental") },
    NameRow { old: "REVERT_NEUTRAL", target: Target::Setting(REVERSION, "revert_neutral") },
    NameRow { old: "REVERT_BENEFICIAL", target: Target::Setting(REVERSION, "revert_beneficial") },
    NameRow { old: "REVERT_TASKLOSS", target: Target::Setting(REVERSION, "revert_taskloss") },
    NameRow { old: "REVERT_EQUALS", target: Target::Setting(REVERSION, "revert_equals") },
    NameRow { old: "STERILIZE_FATAL", target: Target::Setting(REVERSION, "sterilize_fatal") },
    NameRow { old: "STERILIZE_DETRIMENTAL", target: Target::Setting(REVERSION, "sterilize_detrimental") },
    NameRow { old: "STERILIZE_NEUTRAL", target: Target::Setting(REVERSION, "sterilize_neutral") },
    NameRow { old: "STERILIZE_BENEFICIAL", target: Target::Setting(REVERSION, "sterilize_beneficial") },
    NameRow { old: "STERILIZE_TASKLOSS", target: Target::Setting(REVERSION, "sterilize_taskloss") },
    NameRow { old: "STERILIZE_UNSTABLE", target: Target::Setting(REVERSION, "sterilize_unstable") },
    NameRow { old: "NEUTRAL_MAX", target: Target::Setting(REVERSION, "neutral_max") },
    NameRow { old: "NEUTRAL_MIN", target: Target::Setting(REVERSION, "neutral_min") },
    NameRow { old: "AVE_TIME_SLICE", target: Target::Setting(TIME, "ave_time_slice") },
    NameRow { old: "SLICING_METHOD", target: Target::Setting(TIME, "slicing_method") },
    NameRow { old: "BASE_MERIT_METHOD", target: Target::Setting(TIME, "base_merit_method") },
    NameRow { old: "BASE_CONST_MERIT", target: Target::Setting(TIME, "base_const_merit") },
    NameRow { old: "MERIT_BONUS_INST", target: Target::Setting(TIME, "merit_bonus_inst") },
    NameRow { old: "MERIT_BONUS_EFFECT", target: Target::Setting(TIME, "merit_bonus_effect") },
    NameRow { old: "FITNESS_VALLEY", target: Target::Setting(TIME, "fitness_valley") },
    NameRow { old: "FITNESS_VALLEY_START", target: Target::Setting(TIME, "fitness_valley_start") },
    NameRow { old: "FITNESS_VALLEY_STOP", target: Target::Setting(TIME, "fitness_valley_stop") },
    NameRow { old: "DEFAULT_BONUS", target: Target::Setting(TIME, "default_bonus") },
    NameRow { old: "MERIT_DEFAULT_BONUS", target: Target::Setting(TIME, "merit_default_bonus") },
    NameRow { old: "MERIT_INC_APPLY_IMMEDIATE", target: Target::Setting(TIME, "merit_inc_apply_immediate") },
    NameRow { old: "TASK_REFRACTORY_PERIOD", target: Target::Setting(TIME, "task_refractory_period") },
    NameRow { old: "FITNESS_METHOD", target: Target::Setting(TIME, "fitness_method") },
    NameRow { old: "FITNESS_COEFF_1", target: Target::Setting(TIME, "fitness_coeff_1") },
    NameRow { old: "FITNESS_COEFF_2", target: Target::Setting(TIME, "fitness_coeff_2") },
    NameRow { old: "MAX_CPU_THREADS", target: Target::Setting(TIME, "max_cpu_threads") },
    NameRow { old: "THREAD_SLICING_METHOD", target: Target::Setting(TIME, "thread_slicing_method") },
    NameRow { old: "NO_CPU_CYCLE_TIME", target: Target::Setting(TIME, "no_cpu_cycle_time") },
    NameRow { old: "MAX_LABEL_EXE_SIZE", target: Target::Setting(TIME, "max_label_exe_size") },
    NameRow { old: "PRECALC_PHENOTYPE", target: Target::Setting(TIME, "precalc_phenotype") },
    NameRow { old: "GENOTYPE_PHENPLAST_CALC", target: Target::Setting(TIME, "genotype_phenplast_calc") },
    NameRow { old: "MERIT_GIVEN", target: Target::Setting(ALTRUISM, "merit_given") },
    NameRow { old: "MERIT_RECEIVED", target: Target::Setting(ALTRUISM, "merit_received") },
    NameRow { old: "MAX_DONATE_KIN_DIST", target: Target::Setting(ALTRUISM, "max_donate_kin_dist") },
    NameRow { old: "MAX_DONATE_EDIT_DIST", target: Target::Setting(ALTRUISM, "max_donate_edit_dist") },
    NameRow { old: "MIN_GB_DONATE_THRESHOLD", target: Target::Setting(ALTRUISM, "min_gb_donate_threshold") },
    NameRow { old: "DONATE_THRESH_QUANTA", target: Target::Setting(ALTRUISM, "donate_thresh_quanta") },
    NameRow { old: "MAX_DONATES", target: Target::Setting(ALTRUISM, "max_donates") },
    NameRow { old: "KABOOM_PROB", target: Target::Setting(KABOOM, "prob") },
    NameRow { old: "KABOOM_RADIUS", target: Target::Setting(KABOOM, "radius") },
    NameRow { old: "KABOOM_EFFECT", target: Target::Setting(KABOOM, "effect") },
    NameRow { old: "KABOOM_HAMMING", target: Target::Setting(KABOOM, "hamming") },
    NameRow { old: "KABOOM1_HAMMING", target: Target::Setting(KABOOM, "hamming1") },
    NameRow { old: "KABOOM2_HAMMING", target: Target::Setting(KABOOM, "hamming2") },
    NameRow { old: "KABOOM3_HAMMING", target: Target::Setting(KABOOM, "hamming3") },
    NameRow { old: "KABOOM4_HAMMING", target: Target::Setting(KABOOM, "hamming4") },
    NameRow { old: "KABOOM5_HAMMING", target: Target::Setting(KABOOM, "hamming5") },
    NameRow { old: "NO_MUT_INSTS", target: Target::Setting(KABOOM, "no_mut_insts") },
    NameRow { old: "NOISY_QS_SD", target: Target::Setting(KABOOM, "noisy_qs_sd") },
    NameRow { old: "THRESHOLD", target: Target::Setting(GENEALOGY, "threshold") },
    NameRow { old: "TEST_CPU_TIME_MOD", target: Target::Setting(GENEALOGY, "test_cpu_time_mod") },
    NameRow { old: "NET_DROP_PROB", target: Target::Setting(NETWORK, "drop_prob") },
    NameRow { old: "NET_LOG_MESSAGES", target: Target::Setting(NETWORK, "log_messages") },
    NameRow { old: "NET_LOG_RETMESSAGES", target: Target::Setting(NETWORK, "log_retmessages") },
    NameRow { old: "MESSAGE_SEND_BUFFER_SIZE", target: Target::Setting(MESSAGING, "send_buffer_size") },
    NameRow { old: "MESSAGE_RECV_BUFFER_SIZE", target: Target::Setting(MESSAGING, "recv_buffer_size") },
    NameRow { old: "MESSAGE_RECV_BUFFER_BEHAVIOR", target: Target::Setting(MESSAGING, "recv_buffer_behavior") },
    NameRow { old: "ACTIVE_MESSAGES_ENABLED", target: Target::Setting(MESSAGING, "active_messages_enabled") },
    NameRow { old: "CHECK_TASK_ON_SEND", target: Target::Setting(MESSAGING, "check_task_on_send") },
    NameRow { old: "NEURAL_NETWORKING", target: Target::Setting(MESSAGING, "neural_networking") },
    NameRow { old: "SELF_COMMUNICATION", target: Target::Setting(MESSAGING, "self_communication") },
    NameRow { old: "SAVE_RECEIVED", target: Target::Setting(BUY_SELL, "save_received") },
    NameRow { old: "USE_RESOURCE_BINS", target: Target::Setting(HOARDING, "use_resource_bins") },
    NameRow { old: "ABSORB_RESOURCE_FRACTION", target: Target::Setting(HOARDING, "absorb_resource_fraction") },
    NameRow { old: "MULTI_ABSORB_TYPE", target: Target::Setting(HOARDING, "multi_absorb_type") },
    NameRow { old: "MAX_TOTAL_STORED", target: Target::Setting(HOARDING, "max_total_stored") },
    NameRow { old: "USE_STORED_FRACTION", target: Target::Setting(HOARDING, "use_stored_fraction") },
    NameRow { old: "ENV_FRACTION_THRESHOLD", target: Target::Setting(HOARDING, "env_fraction_threshold") },
    NameRow { old: "RETURN_STORED_ON_DEATH", target: Target::Setting(HOARDING, "return_stored_on_death") },
    NameRow { old: "SPLIT_ON_DIVIDE", target: Target::Setting(HOARDING, "split_on_divide") },
    NameRow { old: "COLLECT_SPECIFIC_RESOURCE", target: Target::Setting(HOARDING, "collect_specific_resource") },
    NameRow { old: "NON_1_RESOURCE_RATIOS", target: Target::Setting(HOARDING, "non_1_resource_ratios") },
    NameRow { old: "COLLECT_AMOUNT", target: Target::Setting(HOARDING, "collect_amount") },
    NameRow { old: "RESOURCE_GIVEN_ON_INJECT", target: Target::Setting(HOARDING, "resource_given_on_inject") },
    NameRow { old: "RESOURCE_GIVEN_AT_BIRTH", target: Target::Setting(HOARDING, "resource_given_at_birth") },
    NameRow { old: "COLLECT_PROB_DIVISOR", target: Target::Setting(HOARDING, "collect_prob_divisor") },
    NameRow { old: "MAX_CONCURRENCY", target: Target::Setting(ANALYZE, "max_concurrency") },
    NameRow { old: "INJECT_RESETS_TASKS", target: Target::Setting(ANALYZE, "inject_resets_tasks") },
    NameRow { old: "ANALYZE_OPTION_1", target: Target::Setting(ANALYZE, "option_1") },
    NameRow { old: "ANALYZE_OPTION_2", target: Target::Setting(ANALYZE, "option_2") },
    NameRow { old: "ENERGY_ENABLED", target: Target::Setting(ENERGY, "enabled") },
    NameRow { old: "ENERGY_GIVEN_ON_INJECT", target: Target::Setting(ENERGY, "given_on_inject") },
    NameRow { old: "ENERGY_GIVEN_AT_BIRTH", target: Target::Setting(ENERGY, "given_at_birth") },
    NameRow { old: "FRAC_PARENT_ENERGY_GIVEN_TO_ORG_AT_BIRTH", target: Target::Setting(ENERGY, "frac_parent_energy_given_to_org_at_birth") },
    NameRow { old: "FRAC_PARENT_ENERGY_GIVEN_TO_DEME_AT_BIRTH", target: Target::Setting(ENERGY, "frac_parent_energy_given_to_deme_at_birth") },
    NameRow { old: "FRAC_ENERGY_DECAY_AT_ORG_BIRTH", target: Target::Setting(ENERGY, "frac_energy_decay_at_org_birth") },
    NameRow { old: "FRAC_ENERGY_DECAY_AT_DEME_BIRTH", target: Target::Setting(ENERGY, "frac_energy_decay_at_deme_birth") },
    NameRow { old: "NUM_CYCLES_EXC_BEFORE_0_ENERGY", target: Target::Setting(ENERGY, "num_cycles_exc_before_0_energy") },
    NameRow { old: "ENERGY_CAP", target: Target::Setting(ENERGY, "cap") },
    NameRow { old: "APPLY_ENERGY_METHOD", target: Target::Setting(ENERGY, "apply_method") },
    NameRow { old: "FIX_METABOLIC_RATE", target: Target::Setting(ENERGY, "fix_metabolic_rate") },
    NameRow { old: "FRAC_ENERGY_TRANSFER", target: Target::Setting(ENERGY, "frac_energy_transfer") },
    NameRow { old: "LOG_SLEEP_TIMES", target: Target::Setting(ENERGY, "log_sleep_times") },
    NameRow { old: "FRAC_ENERGY_RELINQUISH", target: Target::Setting(ENERGY, "frac_energy_relinquish") },
    NameRow { old: "ENERGY_PASSED_ON_DEME_REPLICATION_METHOD", target: Target::Setting(ENERGY, "passed_on_deme_replication_method") },
    NameRow { old: "INHERIT_EXE_RATE", target: Target::Setting(ENERGY, "inherit_exe_rate") },
    NameRow { old: "ATTACK_DECAY_RATE", target: Target::Setting(ENERGY, "attack_decay_rate") },
    NameRow { old: "ENERGY_THRESH_LOW", target: Target::Setting(ENERGY, "thresh_low") },
    NameRow { old: "ENERGY_THRESH_HIGH", target: Target::Setting(ENERGY, "thresh_high") },
    NameRow { old: "ENERGY_COMPARISON_EPSILON", target: Target::Setting(ENERGY, "comparison_epsilon") },
    NameRow { old: "ENERGY_REQUEST_RADIUS", target: Target::Setting(ENERGY, "request_radius") },
    NameRow { old: "ENERGY_SHARING_METHOD", target: Target::Setting(ENERGY_SHARING, "method") },
    NameRow { old: "ENERGY_SHARING_PCT", target: Target::Setting(ENERGY_SHARING, "pct") },
    NameRow { old: "ENERGY_SHARING_INCREMENT", target: Target::Setting(ENERGY_SHARING, "increment") },
    NameRow { old: "RESOURCE_SHARING_LOSS", target: Target::Setting(ENERGY_SHARING, "resource_sharing_loss") },
    NameRow { old: "ENERGY_SHARING_UPDATE_METABOLIC", target: Target::Setting(ENERGY_SHARING, "update_metabolic") },
    NameRow { old: "TRACK_CCLADES", target: Target::Setting(SECOND_PASS, "track_cclades") },
    NameRow { old: "TRACK_CCLADES_IDS", target: Target::Setting(SECOND_PASS, "track_cclades_ids") },
    NameRow { old: "MAX_PROGRAMIDS", target: Target::Setting(GENE_EXPRESSION, "max_programids") },
    NameRow { old: "MAX_PROGRAMID_AGE", target: Target::Setting(GENE_EXPRESSION, "max_programid_age") },
    NameRow { old: "IMPLICIT_GENE_EXPRESSION", target: Target::Setting(GENE_EXPRESSION, "implicit_gene_expression") },
    NameRow { old: "IMPLICIT_BG_PROMOTER_RATE", target: Target::Setting(GENE_EXPRESSION, "implicit_bg_promoter_rate") },
    NameRow { old: "IMPLICIT_TURNOVER_RATE", target: Target::Setting(GENE_EXPRESSION, "implicit_turnover_rate") },
    NameRow { old: "IMPLICIT_MAX_PROGRAMID_LENGTH", target: Target::Setting(GENE_EXPRESSION, "implicit_max_programid_length") },
    NameRow { old: "PROMOTERS_ENABLED", target: Target::Setting(PROMOTERS, "enabled") },
    NameRow { old: "PROMOTER_INST_MAX", target: Target::Setting(PROMOTERS, "inst_max") },
    NameRow { old: "PROMOTER_PROCESSIVITY", target: Target::Setting(PROMOTERS, "processivity") },
    NameRow { old: "PROMOTER_PROCESSIVITY_INST", target: Target::Setting(PROMOTERS, "processivity_inst") },
    NameRow { old: "PROMOTER_TO_REGISTER", target: Target::Setting(PROMOTERS, "to_register") },
    NameRow { old: "TERMINATION_RESETS", target: Target::Setting(PROMOTERS, "termination_resets") },
    NameRow { old: "NO_ACTIVE_PROMOTER_EFFECT", target: Target::Setting(PROMOTERS, "no_active_promoter_effect") },
    NameRow { old: "PROMOTER_CODE_SIZE", target: Target::Setting(PROMOTERS, "code_size") },
    NameRow { old: "PROMOTER_EXE_LENGTH", target: Target::Setting(PROMOTERS, "exe_length") },
    NameRow { old: "PROMOTER_EXE_THRESHOLD", target: Target::Setting(PROMOTERS, "exe_threshold") },
    NameRow { old: "INST_CODE_LENGTH", target: Target::Setting(PROMOTERS, "inst_code_length") },
    NameRow { old: "INST_CODE_DEFAULT_TYPE", target: Target::Setting(PROMOTERS, "inst_code_default_type") },
    NameRow { old: "CONSTITUTIVE_REGULATION", target: Target::Setting(PROMOTERS, "constitutive_regulation") },
    NameRow { old: "COLOR_DIFF", target: Target::Setting(COLORS, "diff") },
    NameRow { old: "COLOR_SAME", target: Target::Setting(COLORS, "same") },
    NameRow { old: "COLOR_NEG2", target: Target::Setting(COLORS, "neg2") },
    NameRow { old: "COLOR_NEG1", target: Target::Setting(COLORS, "neg1") },
    NameRow { old: "COLOR_POS1", target: Target::Setting(COLORS, "pos1") },
    NameRow { old: "COLOR_POS2", target: Target::Setting(COLORS, "pos2") },
    NameRow { old: "COLOR_MUT_POS", target: Target::Setting(COLORS, "mut_pos") },
    NameRow { old: "COLOR_MUT_NEUT", target: Target::Setting(COLORS, "mut_neut") },
    NameRow { old: "COLOR_MUT_NEG", target: Target::Setting(COLORS, "mut_neg") },
    NameRow { old: "COLOR_MUT_LETHAL", target: Target::Setting(COLORS, "mut_lethal") },
    NameRow { old: "MOVEMENT_COLLISIONS_LETHAL", target: Target::Setting(MOVEMENT, "collisions_lethal") },
    NameRow { old: "MOVEMENT_COLLISIONS_SELECTION_TYPE", target: Target::Setting(MOVEMENT, "collisions_selection_type") },
    NameRow { old: "VITALITY_BIN_EXTREMES", target: Target::Setting(MOVEMENT, "vitality_bin_extremes") },
    NameRow { old: "VITALITY_BIN_CENTER", target: Target::Setting(MOVEMENT, "vitality_bin_center") },
    NameRow { old: "DEADLY_BOUNDARIES", target: Target::Setting(MOVEMENT, "deadly_boundaries") },
    NameRow { old: "STEP_COUNTING_ERROR", target: Target::Setting(MOVEMENT, "step_counting_error") },
    NameRow { old: "USE_AVATARS", target: Target::Setting(MOVEMENT, "use_avatars") },
    NameRow { old: "AVATAR_BIRTH", target: Target::Setting(MOVEMENT, "avatar_birth") },
    NameRow { old: "AVATAR_BIRTH_FACING", target: Target::Setting(MOVEMENT, "avatar_birth_facing") },
    NameRow { old: "TRACK_BIRTH_LOCS", target: Target::Setting(MOVEMENT, "track_birth_locs") },
    NameRow { old: "LOOK_DIST", target: Target::Setting(SENSING, "look_dist") },
    NameRow { old: "LOOK_DISABLE", target: Target::Setting(SENSING, "look_disable") },
    NameRow { old: "LOOK_DISABLE_TYPE", target: Target::Setting(SENSING, "look_disable_type") },
    NameRow { old: "LOOK_DISABLE_COMBO", target: Target::Setting(SENSING, "look_disable_combo") },
    NameRow { old: "PRED_CONFUSION", target: Target::Setting(SENSING, "pred_confusion") },
    NameRow { old: "TRACK_LOOK_SETTINGS", target: Target::Setting(SENSING, "track_look_settings") },
    NameRow { old: "TRACK_LOOK_OUTPUT", target: Target::Setting(SENSING, "track_look_output") },
    NameRow { old: "USE_DISPLAY", target: Target::Setting(SENSING, "use_display") },
    NameRow { old: "USE_MIMICS", target: Target::Setting(SENSING, "use_mimics") },
    NameRow { old: "MIMIC_ODDS", target: Target::Setting(SENSING, "mimic_odds") },
    NameRow { old: "SET_FT_AT_BIRTH", target: Target::Setting(SENSING, "set_ft_at_birth") },
    NameRow { old: "PHEROMONE_ENABLED", target: Target::Setting(PHEROMONE, "enabled") },
    NameRow { old: "PHEROMONE_AMOUNT", target: Target::Setting(PHEROMONE, "amount") },
    NameRow { old: "PHEROMONE_DROP_MODE", target: Target::Setting(PHEROMONE, "drop_mode") },
    NameRow { old: "EXPLOIT_EXPLORE_PROB", target: Target::Setting(PHEROMONE, "exploit_explore_prob") },
    NameRow { old: "EXPLOIT_LOG_START", target: Target::Setting(PHEROMONE, "exploit_log_start") },
    NameRow { old: "EXPLORE_LOG_START", target: Target::Setting(PHEROMONE, "explore_log_start") },
    NameRow { old: "LOG_INJECT", target: Target::Setting(PHEROMONE, "log_inject") },
    NameRow { old: "INJECT_LOG_START", target: Target::Setting(PHEROMONE, "inject_log_start") },
    NameRow { old: "SYNC_FITNESS_WINDOW", target: Target::Setting(SYNC, "fitness_window") },
    NameRow { old: "SYNC_FLASH_LOSSRATE", target: Target::Setting(SYNC, "flash_lossrate") },
    NameRow { old: "SYNC_TEST_FLASH_ARRIVAL", target: Target::Setting(SYNC, "test_flash_arrival") },
    NameRow { old: "CONSENSUS_HOLD_TIME", target: Target::Setting(CONSENSUS, "hold_time") },
    NameRow { old: "RAW_MATERIAL_AMOUNT", target: Target::Setting(REPUTATION, "raw_material_amount") },
    NameRow { old: "AUTO_REPUTATION", target: Target::Setting(REPUTATION, "auto") },
    NameRow { old: "ALT_BENEFIT", target: Target::Setting(REPUTATION, "alt_benefit") },
    NameRow { old: "ALT_COST", target: Target::Setting(REPUTATION, "alt_cost") },
    NameRow { old: "ROTATE_ON_DONATE", target: Target::Setting(REPUTATION, "rotate_on_donate") },
    NameRow { old: "REPUTATION_REWARD", target: Target::Setting(REPUTATION, "reward") },
    NameRow { old: "DONATION_FAILURE_PERCENT", target: Target::Setting(REPUTATION, "donation_failure_percent") },
    NameRow { old: "RANDOMIZE_RAW_MATERIAL_AMOUNT", target: Target::Setting(REPUTATION, "randomize_raw_material_amount") },
    NameRow { old: "DONATION_RESTRICTIONS", target: Target::Setting(REPUTATION, "donation_restrictions") },
    NameRow { old: "INHERIT_REPUTATION", target: Target::Setting(REPUTATION, "inherit") },
    NameRow { old: "SPECIALISTS", target: Target::Setting(REPUTATION, "specialists") },
    NameRow { old: "STRING_AMOUNT_CAP", target: Target::Setting(REPUTATION, "string_amount_cap") },
    NameRow { old: "MATCH_ALREADY_PRODUCED", target: Target::Setting(REPUTATION, "match_already_produced") },
    NameRow { old: "USE_FORM_GROUPS", target: Target::Setting(GROUP_FORMATION, "use_form_groups") },
    NameRow { old: "DEFAULT_GROUP", target: Target::Setting(GROUP_FORMATION, "default_group") },
    NameRow { old: "INHERIT_OPINION", target: Target::Setting(GROUP_FORMATION, "inherit_opinion") },
    NameRow { old: "OPINION_BUFFER_SIZE", target: Target::Setting(GROUP_FORMATION, "opinion_buffer_size") },
    NameRow { old: "JOIN_GROUP_FAILURE", target: Target::Setting(GROUP_FORMATION, "join_group_failure") },
    NameRow { old: "TOLERANCE_WINDOW", target: Target::Setting(GROUP_FORMATION, "tolerance_window") },
    NameRow { old: "MAX_TOLERANCE", target: Target::Setting(GROUP_FORMATION, "max_tolerance") },
    NameRow { old: "TOLERANCE_VARIATIONS", target: Target::Setting(GROUP_FORMATION, "tolerance_variations") },
    NameRow { old: "TRACK_TOLERANCE", target: Target::Setting(GROUP_FORMATION, "track_tolerance") },
    NameRow { old: "PRED_PREY_SWITCH", target: Target::Setting(GROUP_FORMATION, "pred_prey_switch") },
    NameRow { old: "PRED_EFFICIENCY", target: Target::Setting(GROUP_FORMATION, "pred_efficiency") },
    NameRow { old: "PRED_EFFICIENCY_POISON", target: Target::Setting(GROUP_FORMATION, "pred_efficiency_poison") },
    NameRow { old: "PRED_ODDS", target: Target::Setting(GROUP_FORMATION, "pred_odds") },
    NameRow { old: "PRED_INJURY", target: Target::Setting(GROUP_FORMATION, "pred_injury") },
    NameRow { old: "MIN_PREY", target: Target::Setting(GROUP_FORMATION, "min_prey") },
    NameRow { old: "MAX_PRED", target: Target::Setting(GROUP_FORMATION, "max_pred") },
    NameRow { old: "MAX_PREY", target: Target::Setting(GROUP_FORMATION, "max_prey") },
    NameRow { old: "MAX_PREY_BT", target: Target::Setting(GROUP_FORMATION, "max_prey_bt") },
    NameRow { old: "TRACK_GROUP_ATTACK_DETAILS", target: Target::Setting(GROUP_FORMATION, "track_group_attack_details") },
    NameRow { old: "MARKING_EXPIRE_DATE", target: Target::Setting(GROUP_FORMATION, "marking_expire_date") },
    NameRow { old: "PREY_MUT_OFF", target: Target::Setting(GROUP_FORMATION, "prey_mut_off") },
    NameRow { old: "DEME_NETWORK_TYPE", target: Target::Setting(DEME_NETWORK, "type") },
    NameRow { old: "DEME_NETWORK_REQUIRES_CONNECTEDNESS", target: Target::Setting(DEME_NETWORK, "requires_connectedness") },
    NameRow { old: "DEME_NETWORK_TOPOLOGY_FITNESS", target: Target::Setting(DEME_NETWORK, "topology_fitness") },
    NameRow { old: "DEME_NETWORK_LINK_DECAY", target: Target::Setting(DEME_NETWORK, "link_decay") },
    NameRow { old: "DEME_NETWORK_REMOVE_NODE_ON_DEATH", target: Target::Setting(DEME_NETWORK, "remove_node_on_death") },
    NameRow { old: "ENABLE_HGT", target: Target::Setting(HGT, "enabled") },
    NameRow { old: "HGT_SOURCE", target: Target::Setting(HGT, "source") },
    NameRow { old: "HGT_FRAGMENT_SELECTION", target: Target::Setting(HGT, "fragment_selection") },
    NameRow { old: "HGT_FRAGMENT_SIZE_MEAN", target: Target::Setting(HGT, "fragment_size_mean") },
    NameRow { old: "HGT_FRAGMENT_SIZE_VARIANCE", target: Target::Setting(HGT, "fragment_size_variance") },
    NameRow { old: "HGT_MAX_FRAGMENTS_PER_CELL", target: Target::Setting(HGT, "max_fragments_per_cell") },
    NameRow { old: "HGT_DIFFUSION_METHOD", target: Target::Setting(HGT, "diffusion_method") },
    NameRow { old: "HGT_COMPETENCE_P", target: Target::Setting(HGT, "competence_p") },
    NameRow { old: "HGT_INSERTION_MUT_P", target: Target::Setting(HGT, "insertion_mut_p") },
    NameRow { old: "HGT_CONJUGATION_METHOD", target: Target::Setting(HGT, "conjugation_method") },
    NameRow { old: "HGT_CONJUGATION_P", target: Target::Setting(HGT, "conjugation_p") },
    NameRow { old: "HGT_FRAGMENT_XFORM", target: Target::Setting(HGT, "fragment_xform") },
    NameRow { old: "INST_RES", target: Target::Setting(INST_RESOURCES, "resource") },
    NameRow { old: "INST_RES_FLOOR", target: Target::Setting(INST_RESOURCES, "floor") },
    NameRow { old: "INST_RES_CEIL", target: Target::Setting(INST_RESOURCES, "ceil") },
    NameRow { old: "BCAST_HOPS", target: Target::Setting(ALARM, "bcast_hops") },
    NameRow { old: "ALARM_SELF", target: Target::Setting(ALARM, "self") },
    NameRow { old: "AGE_POLY_TRACKING", target: Target::Setting(DIVISION_OF_LABOR, "age_poly_tracking") },
    NameRow { old: "REACTION_THRESH", target: Target::Setting(DIVISION_OF_LABOR, "reaction_thresh") },
    NameRow { old: "TASK_SWITCH_PENALTY", target: Target::Setting(DIVISION_OF_LABOR, "task_switch_penalty") },
    NameRow { old: "TASK_SWITCH_PENALTY_TYPE", target: Target::Setting(DIVISION_OF_LABOR, "task_switch_penalty_type") },
    NameRow { old: "RES_FOR_DEME_REP", target: Target::Setting(DIVISION_OF_LABOR, "res_for_deme_rep") },
    NameRow { old: "LEARNING_COUNT", target: Target::Setting(DIVISION_OF_LABOR, "learning_count") },
    NameRow { old: "ANALYZE_MODE", target: Target::Setting(ANALYZE, "mode") },
    NameRow { old: "REPRO_METHOD", target: Target::Setting(REPRODUCTION, "repro_method") },
    NameRow { old: "LEGACY_GRID_LOCAL_SELECTION", target: Target::Setting(REPRODUCTION, "legacy_grid_local_selection") },
    NameRow { old: "HARDWARE_TYPE", target: Target::ConverterOnly },
    NameRow { old: "INST_SET", target: Target::Section("instruction_sets") },
    NameRow { old: "INST_SET_LOAD_LEGACY", target: Target::ConverterOnly },
    NameRow { old: "EXECUTION_MODE", target: Target::Setting(EXECUTION, "mode") },
    NameRow { old: "EXECUTION_THREADS", target: Target::Setting(EXECUTION, "threads") },
    NameRow { old: "DROP_EXTINCT_GENOTYPES", target: Target::Setting(GENEALOGY, "drop_extinct_genotypes") },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_setting_has_exactly_one_row() {
        let olds: Vec<&str> = NAMES.iter().map(|r| r.old).collect();
        let unique: BTreeSet<&str> = olds.iter().copied().collect();
        assert_eq!(unique.len(), olds.len(), "a C++ name appears twice");
        for e in all_entries() {
            assert!(unique.contains(e.names[0]), "{} has no row", e.names[0]);
        }
        for p in PORT_ENTRIES {
            assert!(unique.contains(p.name), "{} has no row", p.name);
        }
        assert_eq!(
            NAMES.len(),
            all_entries().count() + PORT_ENTRIES.len(),
            "a row names no setting"
        );
        for r in NAMES {
            assert!(def_of(r.old).is_some(), "{} is not a setting", r.old);
        }
    }

    #[test]
    fn paths_are_unique_and_well_formed() {
        let mut seen = BTreeSet::new();
        for s in settings() {
            assert!(seen.insert(s.path()), "{} twice", s.path());
            for part in [s.group, s.key] {
                assert!(
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_'),
                    "{} is not lower snake case",
                    s.path()
                );
            }
        }
        let groups: BTreeSet<&str> = GROUPS.iter().map(|g| g.key).collect();
        assert_eq!(groups.len(), GROUPS.len());
        for r in NAMES {
            if let Target::Setting(g, _) = r.target {
                assert!(groups.contains(g), "{} names group {g}", r.old);
            }
        }
        for g in GROUPS {
            assert!(
                !settings_of_group(g.key).is_empty(),
                "group {} is empty",
                g.key
            );
            assert!(!SECTIONS.contains(&g.key));
        }
        let n = settings().len();
        let not_settings = NAMES
            .iter()
            .filter(|r| !matches!(r.target, Target::Setting(..)))
            .count();
        assert_eq!(n + not_settings, NAMES.len());
    }

    #[test]
    fn lookups() {
        assert_eq!(
            new_name_of("COPY_MUT_PROB").as_deref(),
            Some("mutation.copy_prob")
        );
        assert_eq!(
            new_name_of("CHILD_SIZE_RANGE").as_deref(),
            Some("divide.offspring_size_range")
        );
        assert_eq!(new_name_of("EVENT_FILE").as_deref(), Some("events"));
        assert_eq!(new_name_of("INST_SET_LOAD_LEGACY"), None);
        assert_eq!(new_name_of("NOPE"), None);
        let s = setting_by_path("world.x").unwrap();
        assert_eq!(s.old, "WORLD_X");
        assert_eq!(s.default(), "60");
        assert!(setting_by_path("world").is_none());
        assert_eq!(
            setting_by_path("execution.mode").unwrap().old,
            "EXECUTION_MODE"
        );
    }
}
