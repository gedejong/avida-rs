//! The systematics side of structured population files (`.spop`) and
//! structured systematics group files: the parts of
//! `cPopulation::SavePopulation`, `SaveStructuredSystematicsGroup`,
//! `LoadPopulation` and `LoadStructuredSystematicsGroup`
//! (avida-core/source/main/cPopulation.cc) that do not touch organisms.
//!
//! Saving: the population scans its cells and reports each occupied cell's
//! parasites and organism to a [`PopulationSave`] (or
//! [`StructuredGroupSave`]), which groups them by systematics group id in an
//! `Apto::Map<int, sGroupInfo*>` (so rows come out in that map's bucket
//! order) and writes the rows.
//!
//! Loading: [`read_population`] parses and sorts the genotype lines
//! (`sTmpGenotype`, `Apto::QSort`), [`load_population_genotypes`] creates
//! the historic genotypes with their parent ids remapped, and
//! [`LoadedPopulation::placements`] lists the cell each organism goes to;
//! the population then builds each organism and classifies it with
//! [`genotype_hints`].

use std::collections::HashMap;

use avida_apto::fmt::fmt_fixed;
use avida_apto::DataFile;
use avida_config::init_file::InitFile;
use avida_config::string_util::{as_double, as_int, pop};

use crate::apto_map::AptoMap;
use crate::arena::GroupHandle;
use crate::genotype_arbiter::GenotypeArbiter;
use crate::manager::Manager;
use crate::source::{Source, TransmissionType};
use crate::unit::{ClassificationHints, GroupRef, RoleClassificationHints};

// ---------------------------------------------------------------------------
// Saving
// ---------------------------------------------------------------------------

/// `sOrgInfo` (cPopulation.cc).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrgSaveInfo {
    /// `cell_id`
    pub cell_id: i32,
    /// `offset`: CPU cycles used in the current gestation
    pub offset: i32,
    /// `lineage_label`
    pub lineage_label: i32,
    /// `curr_group`
    pub curr_group: i32,
    /// `curr_forage`
    pub curr_forage: i32,
    /// `birth_cell`
    pub birth_cell: i32,
    /// `avatar_cell`
    pub avatar_cell: i32,
    /// `av_bcell`
    pub av_bcell: i32,
    /// `parent_ft`
    pub parent_ft: i32,
    /// `parent_is_teacher`
    pub parent_is_teacher: i32,
    /// `parent_merit`
    pub parent_merit: f64,
}

impl OrgSaveInfo {
    /// `sOrgInfo(cell, 0, -1, -1, -1, 0, -1, -1, -1, 0, 1)`: a parasite, or
    /// an organism in a structured systematics group save (with its offset
    /// and lineage label 0).
    pub fn plain(cell_id: i32, offset: i32, lineage_label: i32) -> Self {
        OrgSaveInfo {
            cell_id,
            offset,
            lineage_label,
            curr_group: -1,
            curr_forage: -1,
            birth_cell: 0,
            avatar_cell: -1,
            av_bcell: -1,
            parent_ft: -1,
            parent_is_teacher: 0,
            parent_merit: 1.0,
        }
    }
}

/// What `SavePopulation` reads from an organism.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrganismSaveData {
    /// `GetPhenotype().GetCPUCyclesUsed()`
    pub cpu_cycles_used: i32,
    /// `GetLineageLabel()`
    pub lineage_label: i32,
    /// `HasOpinion() ? GetOpinion().first : -1`
    pub opinion_group: i32,
    /// `GetForageTarget()`
    pub forage_target: i32,
    /// `GetPhenotype().GetBirthCell()`
    pub birth_cell: i32,
    /// `GetOrgInterface().GetAVCellID()`
    pub av_cell_id: i32,
    /// `GetPhenotype().GetAVBirthCell()`
    pub av_birth_cell: i32,
    /// `GetParentFT()`
    pub parent_ft: i32,
    /// `HadParentTeacher()`
    pub had_parent_teacher: bool,
    /// `GetParentMerit()`
    pub parent_merit: f64,
}

/// The `SavePopulation` arguments plus the `USE_AVATARS` setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SaveOptions {
    /// `save_historic`
    pub save_historic: bool,
    /// `save_groupings`
    pub save_groupings: bool,
    /// `save_avatars`
    pub save_avatars: bool,
    /// `save_rebirth`
    pub save_rebirth: bool,
    /// `m_world->GetConfig().USE_AVATARS.Get()`
    pub use_avatars: bool,
}

/// `sGroupInfo`.
#[derive(Clone, Debug)]
struct GroupInfo {
    group: GroupRef,
    orgs: Vec<OrgSaveInfo>,
    parasite: bool,
}

/// The `Apto::Map<int, sGroupInfo*>` both structured saves build.
#[derive(Clone, Debug, Default)]
struct GroupTable {
    index: AptoMap<i32, usize>,
    infos: Vec<GroupInfo>,
}

impl GroupTable {
    /// `map.Get(id, entry)` then push, or create the entry and `Set` it.
    fn add(&mut self, id: i32, group: GroupRef, parasite: bool, info: OrgSaveInfo) {
        if let Some(&i) = self.index.find(&id) {
            self.infos[i].orgs.push(info);
        } else {
            self.infos.push(GroupInfo {
                group,
                orgs: vec![info],
                parasite,
            });
            self.index.set(&id, self.infos.len() - 1);
        }
    }

    fn has(&self, id: i32) -> bool {
        self.index.has(&id)
    }

    /// Entries in `Values()` order.
    fn in_order(&self) -> impl Iterator<Item = &GroupInfo> {
        self.index.values().map(move |&i| &self.infos[i])
    }
}

/// `cPopulation::SavePopulation` (cPopulation.cc), minus the cell scan.
#[derive(Clone, Debug)]
pub struct PopulationSave {
    opts: SaveOptions,
    table: GroupTable,
}

impl PopulationSave {
    /// Starts a save with these options.
    pub fn new(opts: SaveOptions) -> Self {
        PopulationSave {
            opts,
            table: GroupTable::default(),
        }
    }

    /// A parasite in `cell` whose `SystematicsGroup("genotype")` is `g`.
    pub fn add_parasite(&mut self, manager: &Manager, g: GroupRef, cell: i32) {
        let Some(id) = manager.group_id(g) else {
            return;
        };
        self.table.add(id, g, true, OrgSaveInfo::plain(cell, 0, -1));
    }

    /// The organism in `cell` whose `SystematicsGroup("genotype")` is `g`.
    ///
    /// QUIRK: an organism whose genotype is already in the table records
    /// its avatar cells only when `USE_AVATARS` is set; the first organism
    /// of a genotype always records them.
    pub fn add_organism(
        &mut self,
        manager: &Manager,
        g: GroupRef,
        cell: i32,
        org: &OrganismSaveData,
    ) {
        let Some(id) = manager.group_id(g) else {
            return;
        };
        let existing = self.table.has(id);
        let (avatar_cell, av_bcell) = if !existing || self.opts.use_avatars {
            (org.av_cell_id, org.av_birth_cell)
        } else {
            (-1, -1)
        };
        let o = &self.opts;
        let base = OrgSaveInfo::plain(cell, org.cpu_cycles_used, org.lineage_label);
        let info = if !o.save_rebirth {
            match (o.save_groupings, o.save_avatars) {
                (false, false) => base,
                (true, false) => OrgSaveInfo {
                    curr_group: org.opinion_group,
                    curr_forage: org.forage_target,
                    birth_cell: org.birth_cell,
                    ..base
                },
                (true, true) => OrgSaveInfo {
                    curr_group: org.opinion_group,
                    curr_forage: org.forage_target,
                    birth_cell: org.birth_cell,
                    avatar_cell,
                    av_bcell,
                    ..base
                },
                (false, true) => OrgSaveInfo {
                    avatar_cell,
                    av_bcell,
                    ..base
                },
            }
        } else {
            OrgSaveInfo {
                curr_group: org.opinion_group,
                curr_forage: org.forage_target,
                birth_cell: org.birth_cell,
                avatar_cell,
                av_bcell,
                parent_ft: org.parent_ft,
                parent_is_teacher: i32::from(org.had_parent_teacher),
                parent_merit: org.parent_merit,
                ..base
            }
        };
        self.table.add(id, g, false, info);
    }

    /// Writes the file: header, one row per current genotype in map order,
    /// then (with `save_historic`) the genotype arbiter's historic rows.
    pub fn write(&self, manager: &Manager, df: &mut DataFile) -> bool {
        df.set_file_type("genotype_data");
        df.write_comment("Structured Population Save");
        df.write_time_stamp();
        let o = &self.opts;
        for info in self.table.in_order() {
            manager.group_legacy_save(info.group, df);
            let cells = &info.orgs;
            let join = |f: &dyn Fn(&OrgSaveInfo) -> String, all: bool| -> String {
                let mut s = f(&cells[0]);
                if all {
                    for c in &cells[1..] {
                        s.push(',');
                        s.push_str(&f(c));
                    }
                }
                s
            };
            let groupings = o.save_rebirth || o.save_groupings;
            let avatars = o.save_rebirth || o.save_avatars;
            let cellstr = join(&|c| c.cell_id.to_string(), true);
            let offsetstr = join(&|c| c.offset.to_string(), true);
            let lineagestr = join(&|c| c.lineage_label.to_string(), true);
            let groupstr = join(&|c| c.curr_group.to_string(), groupings);
            let foragestr = join(&|c| c.curr_forage.to_string(), groupings);
            let birthstr = join(&|c| c.birth_cell.to_string(), groupings);
            let avatarstr = join(&|c| c.avatar_cell.to_string(), avatars);
            let avatarbstr = join(&|c| c.av_bcell.to_string(), avatars);
            let pforagestr = join(&|c| c.parent_ft.to_string(), o.save_rebirth);
            let pteachstr = join(&|c| c.parent_is_teacher.to_string(), o.save_rebirth);
            let pmeritstr = join(&|c| fmt_fixed(c.parent_merit, 6), o.save_rebirth);

            df.write_with_format(cellstr, "Occupied Cell IDs", "cells");
            let offsets = if info.parasite {
                String::new()
            } else {
                offsetstr
            };
            df.write_with_format(offsets, "Gestation (CPU) Cycle Offsets", "gest_offset");
            df.write_with_format(lineagestr, "Lineage Label", "lineage");
            if !o.save_rebirth {
                if o.save_groupings {
                    df.write_with_format(groupstr, "Current Group IDs", "group_id");
                    df.write_with_format(foragestr, "Current Forager Types", "forager_type");
                    df.write_with_format(birthstr, "Birth Cells", "birth_cell");
                }
                if o.save_avatars {
                    df.write_with_format(avatarstr, "Current Avatar Cell Locations", "avatar_cell");
                    df.write_with_format(avatarbstr, "Avatar Birth Cell", "av_bcell");
                }
            } else {
                df.write_with_format(groupstr, "Current Group IDs", "group_id");
                df.write_with_format(foragestr, "Current Forager Types", "forager_type");
                df.write_with_format(birthstr, "Birth Cells", "birth_cell");
                df.write_with_format(avatarstr, "Current Avatar Cell Locations", "avatar_cell");
                df.write_with_format(avatarbstr, "Avatar Birth Cell", "av_bcell");
                df.write_with_format(pforagestr, "Parent forager type", "parent_ft");
                df.write_with_format(pteachstr, "Was Parent a Teacher", "parent_is_teach");
                df.write_with_format(pmeritstr, "Parent Merit", "parent_merit");
            }
            df.endl();
        }
        if o.save_historic {
            if let Some(role) = manager.arbiter_for_role("genotype") {
                manager.legacy_save(role, df);
            }
        }
        true
    }
}

/// `cPopulation::SaveStructuredSystematicsGroup(role, filename)`, minus
/// the cell scan: one row per group of the role (its `LegacySave` columns
/// and the occupied cells).
#[derive(Clone, Debug, Default)]
pub struct StructuredGroupSave {
    table: GroupTable,
}

impl StructuredGroupSave {
    /// An empty save.
    pub fn new() -> Self {
        Self::default()
    }

    /// A parasite in `cell` whose `SystematicsGroup(role)` is `g`.
    pub fn add_parasite(&mut self, manager: &Manager, g: GroupRef, cell: i32) {
        if let Some(id) = manager.group_id(g) {
            self.table.add(id, g, true, OrgSaveInfo::plain(cell, 0, -1));
        }
    }

    /// The organism in `cell` whose `SystematicsGroup(role)` is `g`;
    /// `offset` is its `GetCPUCyclesUsed()` (recorded but not written).
    pub fn add_organism(&mut self, manager: &Manager, g: GroupRef, cell: i32, offset: i32) {
        if let Some(id) = manager.group_id(g) {
            self.table
                .add(id, g, false, OrgSaveInfo::plain(cell, offset, 0));
        }
    }

    /// Writes the file.
    pub fn write(&self, manager: &Manager, df: &mut DataFile) -> bool {
        df.set_file_type("systematics_data");
        df.write_comment("Structured Systematics Group Save");
        df.write_time_stamp();
        for info in self.table.in_order() {
            manager.group_legacy_save(info.group, df);
            let cellstr: Vec<String> = info.orgs.iter().map(|c| c.cell_id.to_string()).collect();
            df.write_with_format(cellstr.join(","), "Occupied Cell IDs", "cells");
            df.endl();
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// The `LoadPopulation` arguments plus the `USE_AVATARS` setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadOptions {
    /// `load_groups`
    pub load_groups: bool,
    /// `load_birth_cells`
    pub load_birth_cells: bool,
    /// `load_avatars`
    pub load_avatars: bool,
    /// `load_rebirth`
    pub load_rebirth: bool,
    /// `load_parent_dat`
    pub load_parent_dat: bool,
    /// `m_world->GetConfig().USE_AVATARS.Get()`
    pub use_avatars: bool,
}

/// `sTmpGenotype`: one genotype line of a population file.
#[derive(Clone, Debug)]
pub struct LoadedGenotype {
    /// `id_num`: the id in the file
    pub id_num: i32,
    /// `props`: the line as a dictionary (`parents` is rewritten by
    /// [`load_population_genotypes`])
    pub props: HashMap<String, String>,
    /// `num_cpus`: `num_units`, or the older `num_cpus`
    pub num_cpus: i32,
    /// `cells`
    pub cells: Vec<i32>,
    /// `offsets` (`gest_offset`)
    pub offsets: Vec<i32>,
    /// `lineage_labels`
    pub lineage_labels: Vec<i32>,
    /// `group_ids`
    pub group_ids: Vec<i32>,
    /// `forager_types`
    pub forager_types: Vec<i32>,
    /// `birth_cells`
    pub birth_cells: Vec<i32>,
    /// `avatar_cells`
    pub avatar_cells: Vec<i32>,
    /// `parent_merit`
    pub parent_merit: Vec<f64>,
    /// `parent_teacher`
    pub parent_teacher: Vec<bool>,
    /// `parent_ft`
    pub parent_ft: Vec<i32>,
    /// `source`: `Source(props["src"], filename)`. QUIRK: the arguments are
    /// the file name, and `external` comes from the file (`div:int` loads
    /// as internal).
    pub source: Source,
    /// `bg`: the genotype created by [`load_population_genotypes`]
    pub group: Option<GroupHandle>,
}

impl LoadedGenotype {
    /// Whether the line is a parasite genotype (vertical or horizontal).
    pub fn is_parasite(&self) -> bool {
        self.source.transmission_type.is_parasite()
    }

    /// `props->Get(key)` (missing keys read as `""`).
    pub fn prop(&self, key: &str) -> &str {
        self.props.get(key).map(String::as_str).unwrap_or("")
    }
}

/// The genotypes of a population file and whether it has cell ids.
#[derive(Clone, Debug, Default)]
pub struct LoadedPopulation {
    /// Sorted: non-parasites before parasites, each by descending file id.
    pub genotypes: Vec<LoadedGenotype>,
    /// `structured`: some line had a `cells` column (sticky from then on).
    pub structured: bool,
}

/// One organism to create: `genotypes[genotype]`, unit `cell_i`, in
/// `cell_id` with `lineage_label`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// Index into [`LoadedPopulation::genotypes`].
    pub genotype: usize,
    /// `cell_i`
    pub cell_i: usize,
    /// The target cell (offset applied).
    pub cell_id: i32,
    /// `lineage_labels[cell_i] + lineage_offset`, or 0.
    pub lineage_label: i32,
}

/// `while (str.GetSize()) list.Push(str.Pop(',').AsInt())`.
fn int_list(s: &str) -> Vec<i32> {
    let mut s = s.to_string();
    let mut out = Vec::new();
    while !s.is_empty() {
        out.push(as_int(&pop(&mut s, ',')));
    }
    out
}

fn double_list(s: &str) -> Vec<f64> {
    let mut s = s.to_string();
    let mut out = Vec::new();
    while !s.is_empty() {
        out.push(as_double(&pop(&mut s, ',')));
    }
    out
}

/// The first loop of `cPopulation::LoadPopulation`: parse every line into
/// a [`LoadedGenotype`], then `Apto::QSort` them. (`Apto::QSort` always
/// falls through to its insertion sort, which is stable; the order is
/// non-parasites first, by descending id.)
pub fn read_population(input: &InitFile, filename: &str, opts: &LoadOptions) -> LoadedPopulation {
    let mut genotypes = Vec::with_capacity(input.num_lines());
    let mut structured = false;
    for line_id in 0..input.num_lines() {
        let props = input.get_line_as_dict(line_id as i32);
        let get = |k: &str| props.get(k).map(String::as_str).unwrap_or("");
        let has = |k: &str| props.contains_key(k);
        let id_num = as_int(get("id"));
        let num_cpus = if has("num_units") {
            as_int(get("num_units"))
        } else {
            as_int(get("num_cpus"))
        };
        let mut tmp = LoadedGenotype {
            id_num,
            num_cpus,
            cells: Vec::new(),
            offsets: Vec::new(),
            lineage_labels: Vec::new(),
            group_ids: Vec::new(),
            forager_types: Vec::new(),
            birth_cells: Vec::new(),
            avatar_cells: Vec::new(),
            parent_merit: Vec::new(),
            parent_teacher: Vec::new(),
            parent_ft: Vec::new(),
            source: Source::new(TransmissionType::Unknown, "", true),
            group: None,
            props: HashMap::new(),
        };

        let cellstr = get("cells");
        if structured || !cellstr.is_empty() {
            structured = true;
            tmp.cells = int_list(cellstr);
        }

        tmp.source = Source::from_strings(get("src"), filename);

        if !opts.load_rebirth {
            tmp.offsets = int_list(get("gest_offset"));
        }
        tmp.lineage_labels = int_list(get("lineage"));

        let parent_dat = |tmp: &mut LoadedGenotype| {
            if has("parent_is_teach") {
                tmp.parent_teacher = int_list(get("parent_is_teach"))
                    .into_iter()
                    .map(|v| v != 0)
                    .collect();
            }
            if has("parent_ft") {
                tmp.parent_ft = int_list(get("parent_ft"));
            }
            if has("parent_merit") {
                tmp.parent_merit = double_list(get("parent_merit"));
            }
        };

        if opts.load_rebirth {
            if has("birth_cell") {
                tmp.birth_cells = int_list(get("birth_cell"));
            }
            if has("av_bcell") && opts.use_avatars {
                tmp.avatar_cells = int_list(get("av_bcell"));
            }
            parent_dat(&mut tmp);
        } else {
            if opts.load_groups {
                if has("group_id") {
                    tmp.group_ids = int_list(get("group_id"));
                }
                if has("forager_type") {
                    tmp.forager_types = int_list(get("forager_type"));
                }
            }
            if opts.load_birth_cells {
                if has("birth_cell") {
                    tmp.birth_cells = int_list(get("birth_cell"));
                }
                if has("av_bcell") && opts.use_avatars {
                    tmp.avatar_cells = int_list(get("av_bcell"));
                }
            } else if opts.load_avatars && has("avatar_cell") {
                tmp.avatar_cells = int_list(get("avatar_cell"));
            }
            if opts.load_parent_dat {
                parent_dat(&mut tmp);
            }
        }
        if opts.use_avatars && tmp.avatar_cells.is_empty() {
            tmp.avatar_cells = int_list(get("avatar_cell"));
        }
        tmp.props = props;
        genotypes.push(tmp);
    }

    genotypes.sort_by(|a, b| {
        a.is_parasite()
            .cmp(&b.is_parasite())
            .then(b.id_num.cmp(&a.id_num))
    });
    LoadedPopulation {
        genotypes,
        structured,
    }
}

/// The second loop of `cPopulation::LoadPopulation`: from the last line to
/// the first, rewrite each line's `parents` to the new ids of the parents
/// already loaded (parents not in the file, or only earlier in the sorted
/// order, are dropped) and `LegacyLoad` it into the genotype arbiter.
///
/// QUIRK: only the `parents` column is read here, so the older `parent_id`
/// column is overwritten by an empty `parents` and ignored.
pub fn load_population_genotypes(
    arbiter: &mut GenotypeArbiter,
    pop_: &mut LoadedPopulation,
) -> Result<(), String> {
    let n = pop_.genotypes.len();
    for i in (0..n).rev() {
        let mut nparentstr = String::new();
        let mut pcount = 0;
        let mut lparentstr = pop_.genotypes[i].prop("parents").to_string();
        if lparentstr == "(none)" {
            lparentstr.clear();
        }
        for opid in int_list(&lparentstr) {
            let mut npid = -1;
            for j in i..n {
                if pop_.genotypes[j].id_num == opid {
                    // j == i (a line naming itself) is a NULL dereference in
                    // C++; it counts as not found here.
                    npid = pop_.genotypes[j]
                        .group
                        .and_then(|g| arbiter.genotype(g))
                        .map_or(-1, |x| x.id());
                    break;
                }
            }
            if npid != -1 {
                if pcount != 0 {
                    nparentstr.push(',');
                }
                nparentstr.push_str(&npid.to_string());
                pcount += 1;
            }
        }
        let tmp = &mut pop_.genotypes[i];
        tmp.props.insert("parents".to_string(), nparentstr);
        tmp.group = Some(arbiter.legacy_load(&tmp.props)?);
    }
    Ok(())
}

impl LoadedPopulation {
    /// The organisms of the third loop of `LoadPopulation`, in creation
    /// order: genotype by genotype, unit by unit. Cells come from `cells`
    /// (or `birth_cells` when loading birth cells or rebirth data) in a
    /// structured file, else they are numbered consecutively.
    pub fn placements(
        &self,
        cellid_offset: i32,
        lineage_offset: i32,
        opts: &LoadOptions,
    ) -> Result<Vec<Placement>, String> {
        let mut out = Vec::new();
        let mut u_cell_id: i32 = 0;
        for (gi, tmp) in self.genotypes.iter().enumerate() {
            for cell_i in 0..tmp.num_cpus.max(0) as usize {
                let cell_id = if self.structured {
                    let list = if !opts.load_birth_cells && !opts.load_rebirth {
                        &tmp.cells
                    } else {
                        &tmp.birth_cells
                    };
                    let c = list.get(cell_i).ok_or_else(|| {
                        format!(
                            "genotype {} lists fewer cells than its {} units",
                            tmp.id_num, tmp.num_cpus
                        )
                    })?;
                    c.wrapping_add(cellid_offset)
                } else {
                    let c = u_cell_id.wrapping_add(cellid_offset);
                    u_cell_id += 1;
                    c
                };
                let lineage_label = if tmp.lineage_labels.is_empty() {
                    0
                } else {
                    tmp.lineage_labels
                        .get(cell_i)
                        .copied()
                        .unwrap_or(0)
                        .wrapping_add(lineage_offset)
                };
                out.push(Placement {
                    genotype: gi,
                    cell_i,
                    cell_id,
                    lineage_label,
                });
            }
        }
        Ok(out)
    }
}

/// `hints["genotype"]["id"] = Apto::FormatStr("%d", id)`.
pub fn genotype_hints(id: i32) -> RoleClassificationHints {
    let mut h = ClassificationHints::new();
    h.insert("id".to_string(), id.to_string());
    let mut r = RoleClassificationHints::new();
    r.insert("genotype".to_string(), h);
    r
}

/// One line of a structured systematics group file, as
/// `LoadStructuredSystematicsGroup` uses it: the dictionary (for
/// `Arbiter::LegacyLoad`) and the cells whose organisms then join the
/// group (`grp->ClassifyNewUnit(unit, NULL)` for each occupied cell).
#[derive(Clone, Debug)]
pub struct StructuredGroupLine {
    /// The line as a dictionary.
    pub props: HashMap<String, String>,
    /// `cells`, in file order.
    pub cells: Vec<i32>,
}

/// The lines of a structured systematics group file.
pub fn read_structured_group(input: &InitFile) -> Vec<StructuredGroupLine> {
    (0..input.num_lines())
        .map(|i| {
            let props = input.get_line_as_dict(i as i32);
            let cells = int_list(props.get("cells").map(String::as_str).unwrap_or(""));
            StructuredGroupLine { props, cells }
        })
        .collect()
}
