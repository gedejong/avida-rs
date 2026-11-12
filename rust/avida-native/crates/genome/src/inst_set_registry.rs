//! The instruction-set registry part of `cHardwareManager`
//! (avida-core/source/cpu/cHardwareManager.{h,cc}): loading every
//! `INSTSET` block from the `INSTSETS` config lines, name lookup, the
//! default set, the genome -> inst set check of `Create`, and the legacy
//! instruction-set file conversion.
//!
//! Hardware construction and the `cStats` bookkeeping done in
//! `loadInstSet` belong to the hardware and world crates.

use crate::arg_schema::{ArgContainer, ArgSchema, ArgType};
use crate::cstr::{as_double, as_int, format_f, pop, pop_word, trim};
use crate::feedback::Feedback;
use crate::genome::{Genome, DEFAULT_INST_SET_NAME};
use crate::inst_lib::InstLib;
use crate::inst_set::{InstCodeConfig, InstSet};
use std::sync::Arc;

/// `HARDWARE_TYPE_CPU_ORIGINAL` (avida/core/Definitions.h `eHARDWARE_TYPE`).
pub const HARDWARE_TYPE_CPU_ORIGINAL: i32 = 0;
/// `HARDWARE_TYPE_CPU_SMT` (not supported by `loadInstSet`).
pub const HARDWARE_TYPE_CPU_SMT: i32 = 1;
/// `HARDWARE_TYPE_CPU_TRANSSMT`
pub const HARDWARE_TYPE_CPU_TRANSSMT: i32 = 2;
/// `HARDWARE_TYPE_CPU_EXPERIMENTAL`
pub const HARDWARE_TYPE_CPU_EXPERIMENTAL: i32 = 3;
/// `HARDWARE_TYPE_CPU_GP8`
pub const HARDWARE_TYPE_CPU_GP8: i32 = 4;
/// `HARDWARE_TYPE_CPU_BCR`
pub const HARDWARE_TYPE_CPU_BCR: i32 = 5;

/// `cHardware*::GetDefaultInstFilename()` for the hardware types
/// `ConvertLegacyInstSetFile` supports.
pub fn default_inst_filename(hw_type: i32) -> Option<&'static str> {
    match hw_type {
        HARDWARE_TYPE_CPU_ORIGINAL => Some("instset-heads.cfg"),
        HARDWARE_TYPE_CPU_TRANSSMT => Some("instset-transsmt.cfg"),
        HARDWARE_TYPE_CPU_EXPERIMENTAL => Some("instset-experimental.cfg"),
        _ => None,
    }
}

/// Supplies the instruction library of a hardware type
/// (`cHardwareCPU::GetInstLib()` etc.), or `None` for an unknown or
/// unsupported type. Implemented by the hardware crate; any
/// `FnMut(i32) -> Option<Arc<InstLib>>` closure works too.
pub trait InstLibProvider {
    /// The library for `hw_type`.
    fn inst_lib(&mut self, hw_type: i32) -> Option<Arc<InstLib>>;
}

impl<F: FnMut(i32) -> Option<Arc<InstLib>>> InstLibProvider for F {
    fn inst_lib(&mut self, hw_type: i32) -> Option<Arc<InstLib>> {
        self(hw_type)
    }
}

/// `cHardwareManager`'s instruction-set storage.
#[derive(Debug)]
pub struct InstSetRegistry {
    /// Shared so hardware can hold its set (`cHardwareBase::m_inst_set`)
    /// without borrowing the registry.
    inst_sets: Vec<Arc<InstSet>>,
    /// `m_is_name_map` in insertion order (`"(default)"` -> 0 first).
    is_name_map: Vec<(String, i32)>,
}

impl Default for InstSetRegistry {
    fn default() -> Self {
        InstSetRegistry::new()
    }
}

impl InstSetRegistry {
    /// `cHardwareManager(world)`: registers `"(default)"` -> 0.
    pub fn new() -> Self {
        InstSetRegistry {
            inst_sets: Vec::new(),
            is_name_map: vec![(DEFAULT_INST_SET_NAME.to_string(), 0)],
        }
    }

    fn set_name(&mut self, name: &str, id: i32) {
        if let Some(e) = self.is_name_map.iter_mut().find(|(n, _)| n == name) {
            e.1 = id;
        } else {
            self.is_name_map.push((name.to_string(), id));
        }
    }

    /// `m_is_name_map.GetWithDefault(name, -1)`.
    pub fn inst_set_id(&self, name: &str) -> i32 {
        self.is_name_map
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, i)| *i)
            .unwrap_or(-1)
    }

    /// `LoadInstSets(feedback)` over the `INSTSETS` config lines (already
    /// comment-stripped and whitespace-compressed by the config reader).
    ///
    /// Lines other than `INSTSET ...` / `INST ...` are ignored. Each block
    /// is loaded when the next `INSTSET` line (or the end) is reached.
    pub fn load_inst_sets(
        &mut self,
        cfg_list: &[String],
        code_cfg: InstCodeConfig,
        libs: &mut dyn InstLibProvider,
        feedback: &mut Feedback,
    ) -> bool {
        // Build up schema to process instruction set parameters
        let mut is_schema = ArgSchema::with_entry_separator(':');
        is_schema.add_required("hw_type", 0, ArgType::Int);
        is_schema.add_int("stack_size", 1, 10);
        is_schema.add_int("uops_per_cycle", 2, 20);

        let mut cur_list: Option<Vec<String>> = None;
        let mut args: Option<ArgContainer> = None;
        let mut name = String::new();

        let mut success = true;
        for line in cfg_list {
            let line_type = pop_word(&mut line.clone());
            if line_type == "INST" {
                if let Some(list) = cur_list.as_mut() {
                    list.push(line.clone());
                } else {
                    feedback.error("disassociated INST definition, no INSTSET defined");
                    success = false;
                }
            } else if line_type == "INSTSET" {
                if let (Some(list), Some(a)) = (cur_list.take(), args.take()) {
                    if !self.load_inst_set(
                        a.get_int(0),
                        &name,
                        a.get_int(1),
                        a.get_int(2),
                        &list,
                        code_cfg,
                        libs,
                        feedback,
                    ) {
                        success = false;
                    }
                }

                // Process the INSTSET line
                let mut is_def_str = line.clone();
                pop_word(&mut is_def_str); // Pop off "INSTSET"
                name = trim(&pop(&mut is_def_str, ':'));
                if name.is_empty() {
                    feedback.error("instruction set name not found");
                    success = false;
                    continue;
                }

                // Process arguments on the INSTSET line
                match ArgContainer::load(&is_def_str, &is_schema, feedback) {
                    Some(a) => args = Some(a),
                    None => {
                        success = false;
                        continue;
                    }
                }

                cur_list = Some(Vec::new());
            }
        }

        if let (Some(list), Some(a)) = (cur_list.take(), args.take()) {
            if !self.load_inst_set(
                a.get_int(0),
                &name,
                a.get_int(1),
                a.get_int(2),
                &list,
                code_cfg,
                libs,
                feedback,
            ) {
                success = false;
            }
        }

        success
    }

    /// `cHardwareManager::loadInstSet(...)` (without the `cStats` setup:
    /// after a successful load the world must call `SetInstNames` etc. for
    /// the new set; see [`InstSet::name`]).
    #[allow(clippy::too_many_arguments)]
    fn load_inst_set(
        &mut self,
        hw_type: i32,
        name: &str,
        stack_size: i32,
        uops_per_cycle: i32,
        sl: &[String],
        code_cfg: InstCodeConfig,
        libs: &mut dyn InstLibProvider,
        feedback: &mut Feedback,
    ) -> bool {
        let Some(lib) = libs.inst_lib(hw_type) else {
            feedback.error(format!(
                "unknown/unsupported hw_type specified for instset '{}'",
                name
            ));
            return false;
        };
        let mut inst_set = InstSet::new(name, hw_type, lib, stack_size, uops_per_cycle);
        if !inst_set.load_with_string_list(sl, code_cfg, feedback) {
            return false;
        }

        let inst_set_id = self.inst_sets.len() as i32;
        self.inst_sets.push(Arc::new(inst_set));
        self.set_name(name, inst_set_id);
        true
    }

    /// `IsInstSet(name)` (true for `"(default)"`).
    pub fn is_inst_set(&self, name: &str) -> bool {
        self.inst_set_id(name) != -1
    }

    /// `GetInstSet(name)`: `"(default)"` is the first set. `None` where the
    /// C++ would index out of range.
    pub fn get_inst_set(&self, name: &str) -> Option<&InstSet> {
        let id = if name == DEFAULT_INST_SET_NAME {
            0
        } else {
            self.inst_set_id(name)
        };
        if id < 0 {
            return None;
        }
        self.inst_sets.get(id as usize).map(|a| a.as_ref())
    }

    /// `GetInstSet(name)` (mutable, for `SetRedundancy` / `SetProbFail` /
    /// `ActivateNullInst`). `None` also while hardware shares the set
    /// (see [`InstSetRegistry::shared_inst_set`]): such edits belong to
    /// set-up time.
    pub fn get_inst_set_mut(&mut self, name: &str) -> Option<&mut InstSet> {
        let id = if name == DEFAULT_INST_SET_NAME {
            0
        } else {
            self.inst_set_id(name)
        };
        if id < 0 {
            return None;
        }
        self.inst_sets.get_mut(id as usize).and_then(Arc::get_mut)
    }

    /// `GetInstSet(int i)`.
    pub fn inst_set(&self, i: i32) -> &InstSet {
        &self.inst_sets[i as usize]
    }

    /// `GetDefaultInstSet()` (`None` when nothing is loaded).
    pub fn default_inst_set(&self) -> Option<&InstSet> {
        self.inst_sets.first().map(|a| a.as_ref())
    }

    /// `GetNumInstSets()`.
    pub fn num_inst_sets(&self) -> i32 {
        self.inst_sets.len() as i32
    }

    /// All loaded sets, by id.
    pub fn inst_sets(&self) -> &[Arc<InstSet>] {
        &self.inst_sets
    }

    /// `GetInstSet(int i)` as a shared handle (what hardware keeps as
    /// `m_inst_set`); `None` out of range.
    pub fn shared_inst_set(&self, i: i32) -> Option<Arc<InstSet>> {
        if i < 0 {
            return None;
        }
        self.inst_sets.get(i as usize).cloned()
    }

    /// `RegisterInstSet(name, inst_set)`: false if the name is taken.
    pub fn register_inst_set(&mut self, name: &str, inst_set: InstSet) -> bool {
        if self.is_inst_set(name) {
            return false;
        }
        let inst_set_id = self.inst_sets.len() as i32;
        self.inst_sets.push(Arc::new(inst_set));
        self.set_name(name, inst_set_id);
        true
    }

    /// The inst-set lookup and validation `cHardwareManager::Create` does
    /// before building hardware: the id and set named by the genome's
    /// `instset` property, or `None` (C++ returns NULL) if the name is
    /// unknown or the hardware types differ.
    pub fn inst_set_for_genome(&self, genome: &Genome) -> Option<(i32, &InstSet)> {
        let id = self.inst_set_id(genome.inst_set());
        if id == -1 {
            return None;
        }
        let is = self.inst_sets.get(id as usize)?;
        if is.hardware_type() != genome.hardware_type() {
            return None;
        }
        Some((id, is.as_ref()))
    }
}

/// Result of [`convert_legacy_inst_set_file`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyConversion {
    /// Return value of `ConvertLegacyInstSetFile`.
    pub success: bool,
    /// New value for the `INST_SET` setting when the default file was
    /// substituted (`m_world->GetConfig().INST_SET.Set(filename)`).
    pub inst_set_setting: Option<String>,
}

/// `cHardwareManager::ConvertLegacyInstSetFile(filename, str_list, feedback)`:
/// read a legacy `name redundancy cost initial_cost energy_cost prob_fail
/// addl_time_cost res_cost fem_res_cost post_cost` file and append the
/// equivalent `INSTSET` / `INST` lines to `str_list`.
///
/// `hardware_type` is the `HARDWARE_TYPE` setting; `filename` the
/// `INST_SET` setting (`""` or `"-"` selects the hardware's default file).
/// QUIRK: an open failure is reported through `Feedback::append`, so it
/// does not count towards `num_errors` (as in C++).
pub fn convert_legacy_inst_set_file(
    filename: &str,
    hardware_type: i32,
    working_dir: &str,
    str_list: &mut Vec<String>,
    feedback: &mut Feedback,
) -> LegacyConversion {
    let Some(default_filename) = default_inst_filename(hardware_type) else {
        feedback.error("unknown/unsupported HARDWARE_TYPE specified");
        return LegacyConversion {
            success: false,
            inst_set_setting: None,
        };
    };

    let mut filename = filename.to_string();
    let mut inst_set_setting = None;
    if filename.is_empty() || filename == "-" {
        filename = default_filename.to_string();
        feedback.notify(format!("using default instruction set: {}", filename));
        inst_set_setting = Some(filename.clone());
    }

    // cInitFile file(filename, working_dir): the file keeps its own feedback.
    let file = avida_config::InitFile::new(&filename, working_dir, None, None);
    if !file.was_opened() {
        // QUIRK: cUserFeedback::Append copies the messages without counting
        // them as errors.
        feedback.append(file.feedback());
        return LegacyConversion {
            success: false,
            inst_set_setting,
        };
    }

    str_list.push(format!("INSTSET {}:hw_type={}", filename, hardware_type));
    for line_id in 0..file.num_lines() {
        let mut cur_line = file.get_line(line_id as i32);
        let inst_name = pop_word(&mut cur_line);
        let redundancy = as_int(&pop_word(&mut cur_line));
        let cost = as_int(&pop_word(&mut cur_line));
        let ft_cost = as_int(&pop_word(&mut cur_line));
        let energy_cost = as_int(&pop_word(&mut cur_line));
        let prob_fail = as_double(&pop_word(&mut cur_line));
        let addl_time_cost = as_int(&pop_word(&mut cur_line));
        let res_cost = as_double(&pop_word(&mut cur_line));
        let fem_res_cost = as_double(&pop_word(&mut cur_line));
        // `int post_cost = ...AsDouble();` truncates
        let post_cost = as_double(&pop_word(&mut cur_line)) as i32;

        str_list.push(format!(
            "INST {}:redundancy={}:cost={}:initial_cost={}:energy_cost={}:prob_fail={}:addl_time_cost={}:res_cost={}:fem_res_cost={}:post_cost={}",
            inst_name,
            redundancy,
            cost,
            ft_cost,
            energy_cost,
            format_f(prob_fail),
            addl_time_cost,
            format_f(res_cost),
            format_f(fem_res_cost),
            post_cost
        ));
    }

    LegacyConversion {
        success: true,
        inst_set_setting,
    }
}
