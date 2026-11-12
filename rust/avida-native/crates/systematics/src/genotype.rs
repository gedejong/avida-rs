//! Port of `Avida::Systematics::Genotype`
//! (avida-core/source/systematics/Genotype.cc,
//! avida-core/include/private/avida/private/systematics/Genotype.h).
//!
//! The genotype's state and its self-contained methods live here. Methods
//! that call back into the arbiter (`ClassifyNewUnit`, `RemoveUnit`,
//! `NotifyNewUnit`, `RemoveActiveReference`) are
//! [`crate::GenotypeArbiter`] methods taking the genotype's handle.

use std::collections::HashMap;

use avida_apto::{Accumulator, DataFile};
use avida_genome::genome::LegacyValue;
use avida_genome::Genome;

use crate::arena::GroupHandle;
use crate::group_data::GroupData;
use crate::property::PropertyValue;
use crate::source::{Source, TransmissionType};
use crate::sparse_list::EntryHandle;
use crate::stats::{CountTracker, DoubleSum};
use crate::unit::{GestationInfo, UnitInfo};

/// The property ids a genotype defines, in `setupPropertyMap` order
/// (Genotype.cc `Genotype::Initialize` / `setupPropertyMap`), with their
/// descriptions. The per-trigger `environment.triggers.<name>.average`
/// properties follow these.
pub const GENOTYPE_PROPERTIES: [(&str, &str); 30] = [
    ("genome", "Genome"),
    ("src_transmission_type", "Source Transmission Type"),
    ("name", "Name"),
    ("parents", "Parent IDs"),
    ("threshold", "Threshold"),
    ("update_born", "Update Born"),
    ("ave_copy_size", "Average Copied Size"),
    ("ave_exe_size", "Average Executed Size"),
    ("ave_gestation_time", "Average Gestation Time"),
    ("ave_repro_rate", "Average Repro Rate"),
    ("ave_metabolic_rate", "Average Metabolic Rate"),
    ("ave_fitness", "Average Fitness"),
    ("max_fitness", "Maximum Fitness"),
    ("recent_births", "Recent Births (during update)"),
    ("recent_deaths", "Recent Deaths (during update)"),
    ("recent_breed_true", "Recent Breed True (during update)"),
    ("recent_breed_in", "Recent Breed In (during update)"),
    ("recent_breed_out", "Recent Breed Out (during update)"),
    (
        "recent_gestation_count",
        "Recent Gestation Count (during update)",
    ),
    ("total_organisms", "Total Organisms"),
    ("last_births", "Births (during last update)"),
    ("last_deaths", "Deaths (during last update)"),
    ("last_breed_true", "Breed True (during last update)"),
    ("last_breed_in", "Breed In (during last update)"),
    ("last_breed_out", "Breed Out (during last update)"),
    (
        "last_gestation_count",
        "Gestation Count (during last update)",
    ),
    ("last_birth_cell", "Last birth cell"),
    ("last_group_id", "Last birth group"),
    ("last_forager_type", "Last birth forager type"),
    ("total_gestation_count", "Gestation Count (total)"),
];

/// One genotype: the set of units sharing source class and genome.
#[derive(Clone, Debug)]
pub struct Genotype {
    // Group
    pub(crate) id: i32,
    pub(crate) a_refs: i32,
    pub(crate) p_refs: i32,
    pub(crate) data: GroupData,

    /// `m_handle`: the entry in whichever arbiter list holds the genotype
    /// (an active size list, or the historic list).
    pub(crate) handle: Option<EntryHandle>,

    pub(crate) src: Source,
    pub(crate) genome: Genome,
    pub(crate) name: String,

    pub(crate) threshold: bool,
    pub(crate) active: bool,

    pub(crate) generation_born: i32,
    pub(crate) update_born: i32,
    pub(crate) update_deactivated: i32,
    pub(crate) depth: i32,
    pub(crate) num_organisms: i32,
    pub(crate) last_num_organisms: i32,
    pub(crate) total_organisms: i32,

    pub(crate) parents: Vec<GroupHandle>,
    pub(crate) parent_str: String,

    pub(crate) births: CountTracker,
    pub(crate) deaths: CountTracker,
    pub(crate) breed_in: CountTracker,
    pub(crate) breed_true: CountTracker,
    pub(crate) breed_out: CountTracker,
    pub(crate) gestation_count: CountTracker,

    pub(crate) copied_size: DoubleSum,
    pub(crate) exe_size: DoubleSum,
    pub(crate) gestation_time: DoubleSum,
    pub(crate) repro_rate: DoubleSum,
    pub(crate) merit: DoubleSum,
    pub(crate) fitness: DoubleSum,

    pub(crate) last_birth_cell: i32,
    pub(crate) last_group_id: i32,
    pub(crate) last_forager_type: i32,

    /// `m_task_counts`: one accumulator per environment action trigger.
    /// QUIRK: the C++ code that filled them is commented out, so they stay
    /// empty and every `environment.triggers.*.average` property is NaN.
    pub(crate) task_counts: Vec<Accumulator<i32>>,
}

/// `"%03d-no_name"`, the name before a genotype reaches threshold.
fn unnamed(size: i32) -> String {
    format!("{size:03}-no_name")
}

impl Genotype {
    /// Shared field defaults of both constructors.
    fn blank(id: i32, num_triggers: usize) -> Self {
        Genotype {
            id,
            a_refs: 0,
            p_refs: 0,
            data: GroupData::default(),
            handle: None,
            src: Source::default(),
            genome: Genome::default(),
            name: "001-no_name".to_string(),
            threshold: false,
            active: false,
            generation_born: -1,
            update_born: -1,
            update_deactivated: -1,
            depth: 0,
            num_organisms: 0,
            last_num_organisms: 0,
            total_organisms: 0,
            parents: Vec::new(),
            parent_str: String::new(),
            births: CountTracker::default(),
            deaths: CountTracker::default(),
            breed_in: CountTracker::default(),
            breed_true: CountTracker::default(),
            breed_out: CountTracker::default(),
            gestation_count: CountTracker::default(),
            copied_size: DoubleSum::new(),
            exe_size: DoubleSum::new(),
            gestation_time: DoubleSum::new(),
            repro_rate: DoubleSum::new(),
            merit: DoubleSum::new(),
            fitness: DoubleSum::new(),
            last_birth_cell: 0,
            last_group_id: -1,
            last_forager_type: -1,
            task_counts: vec![Accumulator::new(); num_triggers],
        }
    }

    /// `Genotype(mgr, id, founder, update, parents)` minus the parents'
    /// passive references, which the arbiter adds. `parents` gives each
    /// parent's handle, id and depth.
    pub(crate) fn new_founder(
        id: i32,
        founder: &UnitInfo<'_>,
        update: i32,
        parents: &[(GroupHandle, i32, i32)],
        num_triggers: usize,
    ) -> Self {
        let mut g = Self::blank(id, num_triggers);
        g.src = founder.source.clone();
        g.genome = founder.genome.clone();
        g.active = true;
        g.generation_born = founder.generation;
        g.update_born = update;
        g.num_organisms = 1;
        g.total_organisms = 1;
        // AddActiveReference()
        g.a_refs = 1;
        for (i, &(h, pid, _)) in parents.iter().enumerate() {
            g.parents.push(h);
            if i > 0 {
                g.parent_str.push(',');
            }
            g.parent_str.push_str(&pid.to_string());
        }
        if let Some(&(_, _, pdepth)) = parents.first() {
            g.depth = pdepth.wrapping_add(1);
        }
        if !g.src.external {
            g.breed_in.inc();
        }
        g.name = unnamed(g.genome.representation().size());
        g
    }

    /// `Genotype(mgr, id, props)` (the `.spop` load constructor) minus the
    /// parent lookup: returns the genotype and the parent ids to resolve,
    /// in `cStringList` order.
    ///
    /// QUIRK: the transmission type comes from `src`, but `external` is
    /// always set and the name stays `"001-no_name"`. Missing keys read as
    /// `""` (the C++ non-const `Apto::Map::Get` inserts them).
    pub(crate) fn from_props(
        id: i32,
        props: &HashMap<String, String>,
        num_triggers: usize,
    ) -> (Self, Vec<i32>) {
        let get = |k: &str| props.get(k).map(String::as_str).unwrap_or("");
        let mut g = Self::blank(id, num_triggers);
        g.src.transmission_type =
            Source::from_strings(get("src"), get("src_args")).transmission_type;
        g.src.external = true;
        g.src.arguments = get("src_args").to_string();
        if g.src.arguments == "(none)" {
            g.src.arguments.clear();
        }
        g.genome = Genome::from_spop_fields(get("hw_type"), get("inst_set"), get("sequence"));
        g.generation_born = if props.contains_key("gen_born") {
            avida_config::string_util::as_int(get("gen_born"))
        } else {
            -1
        };
        g.update_born = avida_config::string_util::as_int(get("update_born"));
        g.update_deactivated = if props.contains_key("update_deactivated") {
            avida_config::string_util::as_int(get("update_deactivated"))
        } else {
            -1
        };
        g.depth = avida_config::string_util::as_int(get("depth"));
        if props.contains_key("parents") {
            g.parent_str = get("parents").to_string();
        } else if props.contains_key("parent_id") {
            // Backwards compatible load
            g.parent_str = get("parent_id").to_string();
        }
        if g.parent_str == "(none)" {
            g.parent_str.clear();
        }
        // cStringList(m_parent_str, ',') then Pop().AsInt() per entry.
        let mut rest = g.parent_str.clone();
        let mut ids = Vec::new();
        while !rest.is_empty() {
            let word = avida_config::string_util::pop(&mut rest, ',');
            ids.push(avida_config::string_util::as_int(&word));
        }
        (g, ids)
    }

    /// `ID()`.
    pub fn id(&self) -> i32 {
        self.id
    }

    /// `ActiveReferenceCount()`.
    pub fn active_reference_count(&self) -> i32 {
        self.a_refs
    }

    /// `PassiveReferenceCount()`.
    pub fn passive_reference_count(&self) -> i32 {
        self.p_refs
    }

    /// `ReferenceCount()`: active plus passive.
    pub fn reference_count(&self) -> i32 {
        self.a_refs.wrapping_add(self.p_refs)
    }

    /// Attached data (`GetData<T>()`).
    pub fn data(&self) -> &GroupData {
        &self.data
    }

    /// `m_src`.
    pub fn source(&self) -> &Source {
        &self.src
    }

    /// `GroupGenome()`.
    pub fn genome(&self) -> &Genome {
        &self.genome
    }

    /// `m_name`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `IsThreshold()`.
    pub fn is_threshold(&self) -> bool {
        self.threshold
    }

    /// `IsActive()`.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// `m_generation_born`.
    pub fn generation_born(&self) -> i32 {
        self.generation_born
    }

    /// `GetUpdateBorn()`.
    pub fn update_born(&self) -> i32 {
        self.update_born
    }

    /// `m_update_deactivated`.
    pub fn update_deactivated(&self) -> i32 {
        self.update_deactivated
    }

    /// `Depth()`.
    pub fn depth(&self) -> i32 {
        self.depth
    }

    /// `NumUnits()`.
    pub fn num_units(&self) -> i32 {
        self.num_organisms
    }

    /// `m_last_num_organisms` (set by `UpdateReset`).
    pub fn last_num_units(&self) -> i32 {
        self.last_num_organisms
    }

    /// `m_total_organisms`.
    pub fn total_units(&self) -> i32 {
        self.total_organisms
    }

    /// `Parents()`.
    pub fn parents(&self) -> &[GroupHandle] {
        &self.parents
    }

    /// `m_parent_str` (the `parents` property).
    pub fn parent_str(&self) -> &str {
        &self.parent_str
    }

    /// `m_births`.
    pub fn births(&self) -> &CountTracker {
        &self.births
    }

    /// `m_deaths`.
    pub fn deaths(&self) -> &CountTracker {
        &self.deaths
    }

    /// `m_breed_in`.
    pub fn breed_in(&self) -> &CountTracker {
        &self.breed_in
    }

    /// `m_breed_true`.
    pub fn breed_true(&self) -> &CountTracker {
        &self.breed_true
    }

    /// `m_breed_out`.
    pub fn breed_out(&self) -> &CountTracker {
        &self.breed_out
    }

    /// `m_gestation_count`.
    pub fn gestation_count(&self) -> &CountTracker {
        &self.gestation_count
    }

    /// `m_copied_size` (`ave_copy_size` is its average).
    pub fn copied_size(&self) -> &DoubleSum {
        &self.copied_size
    }

    /// `m_exe_size` (`ave_exe_size`).
    pub fn exe_size(&self) -> &DoubleSum {
        &self.exe_size
    }

    /// `m_gestation_time` (`ave_gestation_time`).
    pub fn gestation_time(&self) -> &DoubleSum {
        &self.gestation_time
    }

    /// `m_repro_rate` (`ave_repro_rate`).
    pub fn repro_rate(&self) -> &DoubleSum {
        &self.repro_rate
    }

    /// `m_merit` (`ave_metabolic_rate`).
    pub fn merit(&self) -> &DoubleSum {
        &self.merit
    }

    /// `m_fitness` (`ave_fitness`, `max_fitness`).
    pub fn fitness(&self) -> &DoubleSum {
        &self.fitness
    }

    /// `m_last_birth_cell`.
    pub fn last_birth_cell(&self) -> i32 {
        self.last_birth_cell
    }

    /// `m_last_group_id`.
    pub fn last_group_id(&self) -> i32 {
        self.last_group_id
    }

    /// `m_last_forager_type`.
    pub fn last_forager_type(&self) -> i32 {
        self.last_forager_type
    }

    /// `SetLastBirthCell(birth_cell)`.
    pub fn set_last_birth_cell(&mut self, birth_cell: i32) {
        self.last_birth_cell = birth_cell;
    }

    /// `SetLastGroupID(group_id)`.
    pub fn set_last_group_id(&mut self, group_id: i32) {
        self.last_group_id = group_id;
    }

    /// `SetLastForagerType(forager_type)`.
    pub fn set_last_forager_type(&mut self, forager_type: i32) {
        self.last_forager_type = forager_type;
    }

    /// `HandleUnitGestation(u)`.
    pub fn handle_unit_gestation(&mut self, u: &GestationInfo) {
        self.gestation_count.inc();
        self.copied_size.add1(u.last_copied_size);
        self.exe_size.add1(u.last_executed_size);
        let last_gestation_time = u.last_gestation_time;
        self.gestation_time.add1(last_gestation_time);
        self.repro_rate.add1(1.0 / last_gestation_time);
        self.merit.add1(u.last_metabolic_rate);
        self.fitness.add1(u.last_fitness);
    }

    /// `Matches(u)`: same transmission class (parasites also need equal
    /// inject labels), then equal genomes.
    pub fn matches(&self, u: &UnitInfo<'_>) -> bool {
        use TransmissionType::*;
        let ut = u.source.transmission_type;
        match self.src.transmission_type {
            Division | Duplication => match ut {
                Division | Duplication => {}
                _ => return false,
            },
            Vertical | Horizontal => match ut {
                Vertical | Horizontal => {
                    if self.src.arguments != u.source.arguments {
                        return false;
                    }
                }
                _ => return false,
            },
            Unknown => return false,
        }
        self.genome == *u.genome
    }

    /// The bookkeeping half of `NotifyNewUnit(u)` (before
    /// `AdjustGenotype`): reactivates, counts a breed-in for internal
    /// division and parasite births, and adds the unit.
    pub(crate) fn notify_new_unit_counts(&mut self, u: &UnitInfo<'_>) {
        self.active = true;
        if !u.source.external {
            match u.source.transmission_type {
                TransmissionType::Division
                | TransmissionType::Horizontal
                | TransmissionType::Vertical => self.breed_in.inc(),
                _ => {}
            }
        }
        self.total_organisms = self.total_organisms.wrapping_add(1);
        self.num_organisms = self.num_organisms.wrapping_add(1);
    }

    /// `UpdateReset()`.
    pub(crate) fn update_reset(&mut self) {
        self.last_num_organisms = self.num_organisms;
        self.births.next();
        self.deaths.next();
        self.breed_out.next();
        self.breed_true.next();
        self.breed_in.next();
        self.gestation_count.next();
    }

    /// `Deactivate(update)`.
    pub(crate) fn deactivate(&mut self, update: i32) {
        self.active = false;
        self.update_deactivated = update;
    }

    /// `Reactivate()`.
    pub(crate) fn reactivate(&mut self) {
        self.active = true;
        self.update_deactivated = -1;
    }

    /// `Properties().Has(name)`, given the arbiter's trigger property ids.
    pub fn has_property(&self, name: &str, trigger_average_ids: &[String]) -> bool {
        GENOTYPE_PROPERTIES.iter().any(|(n, _)| *n == name)
            || trigger_average_ids.iter().any(|t| t == name)
    }

    /// `Properties().Get(name)`: the value as the property map would
    /// return it ([`PropertyValue::Null`] for undefined ids, e.g.
    /// `"fitness"`). `trigger_average_ids` are the arbiter's
    /// `environment.triggers.<name>.average` ids.
    pub fn property(&self, name: &str, trigger_average_ids: &[String]) -> PropertyValue {
        use PropertyValue::*;
        match name {
            "genome" => Str(self.genome.as_string()),
            "src_transmission_type" => IntText(self.src.transmission_type.as_int().to_string()),
            "name" => Str(self.name.clone()),
            "parents" => Str(self.parent_str.clone()),
            "threshold" => Bool(self.threshold),
            "update_born" => Int(self.update_born),
            "ave_copy_size" => Double(self.copied_size.average()),
            "ave_exe_size" => Double(self.exe_size.average()),
            "ave_gestation_time" => Double(self.gestation_time.average()),
            "ave_repro_rate" => Double(self.repro_rate.average()),
            "ave_metabolic_rate" => Double(self.merit.average()),
            "ave_fitness" => Double(self.fitness.average()),
            "max_fitness" => Double(self.fitness.max()),
            "recent_births" => Int(self.births.cur()),
            "recent_deaths" => Int(self.deaths.cur()),
            "recent_breed_true" => Int(self.breed_true.cur()),
            "recent_breed_in" => Int(self.breed_in.cur()),
            "recent_breed_out" => Int(self.breed_out.cur()),
            "recent_gestation_count" => Int(self.gestation_count.cur()),
            "total_organisms" => Int(self.total_organisms),
            "last_births" => Int(self.births.last()),
            "last_deaths" => Int(self.deaths.last()),
            "last_breed_true" => Int(self.breed_true.last()),
            "last_breed_in" => Int(self.breed_in.last()),
            "last_breed_out" => Int(self.breed_out.last()),
            "last_gestation_count" => Int(self.gestation_count.last()),
            "last_birth_cell" => Int(self.last_birth_cell),
            "last_group_id" => Int(self.last_group_id),
            "last_forager_type" => Int(self.last_forager_type),
            "total_gestation_count" => Int(self.gestation_count.total()),
            _ => match trigger_average_ids.iter().position(|t| t == name) {
                Some(i) => Double(self.task_counts.get(i).map_or(f64::NAN, Accumulator::mean)),
                None => Null,
            },
        }
    }

    /// `LegacySave(df)`: one `.spop` row's genotype columns (the caller
    /// appends further columns and ends the row). `parent_ids` are the
    /// current ids of [`Genotype::parents`].
    pub(crate) fn legacy_save(&self, df: &mut DataFile, parent_ids: &[i32]) {
        df.write_with_format(self.id, "ID", "id");
        df.write_with_format(self.src.as_string(), "Source", "src");
        let args = if self.src.arguments.is_empty() {
            "(none)"
        } else {
            self.src.arguments.as_str()
        };
        df.write_with_format(args, "Source Args", "src_args");
        let mut s = String::new();
        if let Some(first) = parent_ids.first() {
            s.push_str(&first.to_string());
            for p in &parent_ids[1..] {
                s.push(',');
                s.push_str(&p.to_string());
            }
        }
        let parents = if s.is_empty() {
            "(none)".to_string()
        } else {
            s
        };
        df.write_with_format(parents, "Parent ID(s)", "parents");
        df.write_with_format(
            self.num_organisms,
            "Number of currently living organisms",
            "num_units",
        );
        df.write_with_format(
            self.total_organisms,
            "Total number of organisms that ever existed",
            "total_units",
        );
        df.write_with_format(
            self.genome.representation().size(),
            "Genome Length",
            "length",
        );
        df.write_with_format(self.merit.average(), "Average Merit", "merit");
        df.write_with_format(
            self.gestation_time.average(),
            "Average Gestation Time",
            "gest_time",
        );
        df.write_with_format(self.fitness.average(), "Average Fitness", "fitness");
        df.write_with_format(self.generation_born, "Generation Born", "gen_born");
        df.write_with_format(self.update_born, "Update Born", "update_born");
        df.write_with_format(
            self.update_deactivated,
            "Update Deactivated",
            "update_deactivated",
        );
        df.write_with_format(self.depth, "Phylogenetic Depth", "depth");
        for (value, descr, name) in self.genome.legacy_save_columns() {
            match value {
                LegacyValue::Int(i) => df.write_with_format(i, descr, name),
                LegacyValue::Str(s) => df.write_with_format(s, descr, name),
            }
        }
    }
}
