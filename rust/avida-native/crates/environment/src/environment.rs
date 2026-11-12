//! Port of `cEnvironment` (avida-core/source/main/cEnvironment.{h,cc}):
//! state, the environment-file parser (`Load`, `LoadLine` and every
//! `Load*` it dispatches to) and the reaction/resource accessors and
//! modifiers. Reaction evaluation (`SetupInputs`, `TestOutput`, ...) is in
//! `evaluation.rs`.
//!
//! The current C++ calls small classifier helpers from rust/avida-rust
//! (`avd_env_*_var_kind`, `avd_env_process_type`, `avd_env_cellbox_validate`,
//! ...). This port follows the last pre-Rust C++
//! (`git show 47f13dadb:avida-core/source/main/cEnvironment.cc`), whose
//! inline `if (var_name == ...)` chains and checks those helpers replicate
//! exactly; they appear here as string `match`es on the same names.
//! (47f13dadb rather than ef5c6d948: the older commit predates the
//! `cellbox` requisite and the 2018 inflow/outflow fix-up change.)

use crate::arg_schema::{ArgContainer, ArgSchema};
use crate::env_reqs::EnvReqs;
use crate::mutation_rates::MutationRates;
use crate::reaction::{process_type_from_str, CellBox, PhenPlastBonusMethod, ReactionLib};
use crate::resource::{geometry, CellResource, Resource};
use crate::resource_lib::ResourceLib;
use crate::state_grid::StateGrid;
use crate::task::{TaskEnvHooks, TaskRegistry};
use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_config::feedback::{Feedback, FeedbackKind};
use avida_config::init_file::InitFile;
use avida_config::string_util::{
    as_double, as_int, is_number, is_numeric, is_substring, pop, pop_word, return_array, substring,
    trim,
};
use std::collections::BTreeSet;

/// `INPUT_SIZE_DEFAULT` (avida/core/Definitions.h).
pub const INPUT_SIZE_DEFAULT: i32 = 3;
/// `OUTPUT_SIZE_DEFAULT` (avida/core/Definitions.h).
pub const OUTPUT_SIZE_DEFAULT: i32 = 1;

/// `Avida::Environment::ActionTrigger` as registered by
/// `Environment::Manager::DefineActionTrigger` (avida-core/source/environment/Manager.cc)
/// from `cEnvironment::LoadReaction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionTrigger {
    /// Trigger id: the task name.
    pub id: String,
    /// The task description.
    pub desc: String,
    /// `tmp_order`: `m_tasklib.GetSize() - 1` at definition time.
    pub temp_ordering: i32,
}

/// `cEnvironment`. `T` is the task library (see [`TaskRegistry`]).
#[derive(Debug, Clone)]
pub struct Environment<T> {
    pub(crate) mut_rates: MutationRates,
    pub(crate) resource_lib: ResourceLib,
    pub(crate) reaction_lib: ReactionLib,
    pub(crate) tasklib: T,
    pub(crate) input_size: i32,
    pub(crate) output_size: i32,
    pub(crate) true_rand: bool,
    pub(crate) use_specific_inputs: bool,
    pub(crate) specific_inputs: Vec<i32>,
    pub(crate) mask: u32,
    pub(crate) state_grids: Vec<StateGrid>,
    pub(crate) possible_group_ids: BTreeSet<i32>,
    pub(crate) possible_target_ids: BTreeSet<i32>,
    pub(crate) possible_habitats: BTreeSet<i32>,
    pub(crate) pp_fts: Vec<i32>,
    pub(crate) hammers: bool,
    pub(crate) paths: bool,
    /// `Environment::Manager` action triggers, in definition order.
    pub(crate) action_triggers: Vec<ActionTrigger>,
}

// ---------------------------------------------------------------------------
// Parsing helpers (cEnvironment::ParseSetting / AssertInput*)
// ---------------------------------------------------------------------------

/// `ParseSetting(entry, var_name, var_value, var_type, feedback)`: split
/// `name=value`, lower-casing the name.
fn parse_setting(entry: &str, var_type: &str, fb: &mut Feedback) -> Option<(String, String)> {
    if entry.is_empty() {
        fb.error(format!("empty setting to parse in {}", var_type));
        return None;
    }
    let mut entry = entry.to_string();
    let mut var_name = pop(&mut entry, '=');
    let var_value = entry;
    if var_name.is_empty() {
        // (sic) "povided"
        fb.error(format!(
            "no variable povided to set to '{}' in '{}'",
            var_value, var_type
        ));
        return None;
    }
    if var_value.is_empty() {
        fb.error(format!("no value given for '{}' in {}", var_name, var_type));
        return None;
    }
    var_name.make_ascii_lowercase();
    Some((var_name, var_value))
}

/// `AssertInputInt`. QUIRK: `cString::IsNumeric` accepts any number
/// characters (`-+.eE` too), so "1.5" passes and is then truncated by `AsInt`.
fn assert_input_int(input: &str, name: &str, ty: &str, fb: &mut Feedback) -> bool {
    if !is_numeric(input) {
        fb.error(format!("in {}, {} set to non-integer", ty, name));
        return false;
    }
    true
}

/// `AssertInputDouble`.
fn assert_input_double(input: &str, name: &str, ty: &str, fb: &mut Feedback) -> bool {
    if !is_number(input) {
        fb.error(format!("in {}, {} set to non-number", ty, name));
        return false;
    }
    true
}

/// `AssertInputBool`: a number whose `AsInt` is 0 or 1.
fn assert_input_bool(input: &str, name: &str, ty: &str, fb: &mut Feedback) -> bool {
    if !is_number(input) {
        fb.error(format!("in {}, {} set to non-number", ty, name));
        return false;
    }
    let value = as_int(input);
    if value != 1 && value != 0 {
        fb.error(format!("in {}, {} set to non-bool", ty, name));
        return false;
    }
    true
}

/// `AssertInputValid(ptr, name, type, value)`.
fn assert_input_valid(found: bool, name: &str, ty: &str, value: &str, fb: &mut Feedback) -> bool {
    if !found {
        fb.error(format!(
            "in {}, '{}' setting of '{}' not found",
            ty, name, value
        ));
        return false;
    }
    true
}

/// The text of the most recent error in `fb` (for `Result` returns).
fn last_error(fb: &Feedback) -> String {
    fb.messages()
        .iter()
        .rev()
        .find(|(k, _)| *k == FeedbackKind::Error)
        .map(|(_, m)| m.clone())
        .unwrap_or_default()
}

/// Requisite variable names (`avd_env_requisite_var_kind`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReqVar {
    Reaction,
    NoReaction,
    MinCount,
    MaxCount,
    ReactionMinCount,
    ReactionMaxCount,
    DivideOnly,
    MinTotCount,
    MaxTotCount,
    ParasiteOnly,
    CellBox,
}

fn requisite_var_kind(name: &str) -> Option<ReqVar> {
    Some(match name {
        "reaction" => ReqVar::Reaction,
        "noreaction" => ReqVar::NoReaction,
        "min_count" => ReqVar::MinCount,
        "max_count" => ReqVar::MaxCount,
        "reaction_min_count" => ReqVar::ReactionMinCount,
        "reaction_max_count" => ReqVar::ReactionMaxCount,
        "divide_only" => ReqVar::DivideOnly,
        "min_tot_count" => ReqVar::MinTotCount,
        "max_tot_count" => ReqVar::MaxTotCount,
        "parasite_only" => ReqVar::ParasiteOnly,
        "cellbox" => ReqVar::CellBox,
        _ => return None,
    })
}

/// `avd_env_cellbox_validate`: 0 ok, 1 bad x, 2 bad y, 3 bad width,
/// 4 bad height.
fn cellbox_validate(xx: i32, yy: i32, width: i32, height: i32, world_x: i32, world_y: i32) -> i32 {
    if xx < 0 || xx >= world_x {
        1
    } else if yy < 0 || yy >= world_y {
        2
    } else if width <= 0 || width.wrapping_add(xx) >= world_x {
        3
    } else if height <= 0 || height.wrapping_add(yy) >= world_y {
        4
    } else {
        0
    }
}

impl<T> Environment<T> {
    /// `cEnvironment(world)`: mutation rates from the config, `DEFAULT_GROUP`
    /// (unless -1) as the first possible group id.
    pub fn new(config: &Config, tasklib: T) -> Self {
        let mut mut_rates = MutationRates::default();
        mut_rates.setup(config);
        let mut possible_group_ids = BTreeSet::new();
        if config.default_group != -1 {
            possible_group_ids.insert(config.default_group);
        }
        Environment {
            mut_rates,
            resource_lib: ResourceLib::new(),
            reaction_lib: ReactionLib::new(),
            tasklib,
            input_size: INPUT_SIZE_DEFAULT,
            output_size: OUTPUT_SIZE_DEFAULT,
            true_rand: false,
            use_specific_inputs: false,
            specific_inputs: Vec::new(),
            mask: 0,
            state_grids: Vec::new(),
            possible_group_ids,
            possible_target_ids: BTreeSet::new(),
            possible_habitats: BTreeSet::new(),
            pp_fts: Vec::new(),
            hammers: false,
            paths: false,
            action_triggers: Vec::new(),
        }
    }

    // -- Accessors ---------------------------------------------------------

    /// The task library (`m_tasklib`).
    pub fn tasklib(&self) -> &T {
        &self.tasklib
    }
    /// The task library (mutable).
    pub fn tasklib_mut(&mut self) -> &mut T {
        &mut self.tasklib
    }
    /// `GetNumReactions()`.
    pub fn num_reactions(&self) -> i32 {
        self.reaction_lib.size()
    }
    /// `GetResourceLib()`.
    pub fn resource_lib(&self) -> &ResourceLib {
        &self.resource_lib
    }
    /// `GetResourceLib()` (mutable).
    pub fn resource_lib_mut(&mut self) -> &mut ResourceLib {
        &mut self.resource_lib
    }
    /// `GetReactionLib()`.
    pub fn reaction_lib(&self) -> &ReactionLib {
        &self.reaction_lib
    }
    /// `GetReactionLib()` (mutable).
    pub fn reaction_lib_mut(&mut self) -> &mut ReactionLib {
        &mut self.reaction_lib
    }
    /// `GetMutRates()`.
    pub fn mut_rates(&self) -> &MutationRates {
        &self.mut_rates
    }
    /// `GetMutRates()` (mutable).
    pub fn mut_rates_mut(&mut self) -> &mut MutationRates {
        &mut self.mut_rates
    }
    /// `GetNumStateGrids()`.
    pub fn num_state_grids(&self) -> i32 {
        self.state_grids.len() as i32
    }
    /// `GetStateGrid(sg)`.
    pub fn state_grid(&self, sg: usize) -> &StateGrid {
        &self.state_grids[sg]
    }
    /// `GetInputSize()`.
    pub fn input_size(&self) -> i32 {
        self.input_size
    }
    /// `GetOutputSize()`.
    pub fn output_size(&self) -> i32 {
        self.output_size
    }
    /// `m_true_rand`: some task requested truly random inputs.
    pub fn true_rand(&self) -> bool {
        self.true_rand
    }
    /// `SetSpecificInputs(inputs)`: use these instead of random inputs.
    pub fn set_specific_inputs(&mut self, inputs: Vec<i32>) {
        self.use_specific_inputs = true;
        self.specific_inputs = inputs;
    }
    /// `SetSpecificRandomMask(mask)`.
    pub fn set_specific_random_mask(&mut self, mask: u32) {
        self.mask = mask;
    }
    /// The `Environment::Manager` action triggers defined by reactions.
    pub fn action_triggers(&self) -> &[ActionTrigger] {
        &self.action_triggers
    }
    /// `Environment::Manager::GetActionTrigger(id)`.
    pub fn action_trigger(&self, id: &str) -> Option<&ActionTrigger> {
        self.action_triggers.iter().find(|t| t.id == id)
    }

    /// `GetReactionName(reaction_id)`.
    pub fn reaction_name(&self, reaction_id: usize) -> &str {
        self.reaction_lib.reaction(reaction_id).name()
    }

    /// `GetReactionValue(const cString& name)`: first process's value, 0 if
    /// the reaction is unknown.
    pub fn reaction_value_by_name(&self, name: &str) -> f64 {
        match self.reaction_lib.find(name) {
            Some(i) => self.reaction_lib.reaction(i).value(0),
            None => 0.0,
        }
    }

    /// `GetReactionValue(int reaction_id)` (0 for an id out of range).
    pub fn reaction_value(&self, reaction_id: i32) -> f64 {
        if reaction_id < 0 || reaction_id >= self.reaction_lib.size() {
            return 0.0;
        }
        self.reaction_lib.reaction(reaction_id as usize).value(0)
    }

    /// `SetReactionValue(ctx, name, value)`: `ALL`, `RANDOM:<n>` (n distinct
    /// reactions chosen with `Random::Choose`) or one reaction by name.
    pub fn set_reaction_value(&mut self, rng: &mut AvidaRng, name: &str, value: f64) -> bool {
        let num_reactions = self.reaction_lib.size();
        if name == "ALL" {
            for i in 0..num_reactions as usize {
                self.reaction_lib.reaction_mut(i).modify_value(value, 0);
            }
            return true;
        }
        if is_substring(name, "RANDOM:", 0) {
            let num_set = as_int(&substring(name, 7, name.len().saturating_sub(7)));
            if num_set > num_reactions {
                return false;
            }
            // A negative count aborts the C++ (vector length error).
            if num_set < 0 {
                return false;
            }
            let mut ids = vec![0i32; num_set as usize];
            rng.choose(num_reactions, &mut ids);
            for id in ids {
                self.reaction_lib
                    .reaction_mut(id as usize)
                    .modify_value(value, 0);
            }
            return true;
        }
        match self.reaction_lib.reaction_by_name_mut(name) {
            Some(r) => {
                r.modify_value(value, 0);
                true
            }
            None => false,
        }
    }

    /// `SetReactionValueMult(name, value_mult)`.
    pub fn set_reaction_value_mult(&mut self, name: &str, value_mult: f64) -> bool {
        match self.reaction_lib.reaction_by_name_mut(name) {
            Some(r) => {
                r.multiply_value(value_mult, 0);
                true
            }
            None => false,
        }
    }

    /// `SetReactionInst(name, inst_name)`.
    pub fn set_reaction_inst(&mut self, name: &str, inst_name: &str) -> bool {
        match self.reaction_lib.reaction_by_name_mut(name) {
            Some(r) => {
                r.modify_inst(inst_name, 0);
                true
            }
            None => false,
        }
    }

    /// `SetReactionMinTaskCount(name, min_count)`.
    pub fn set_reaction_min_task_count(&mut self, name: &str, v: i32) -> bool {
        match self.reaction_lib.reaction_by_name_mut(name) {
            Some(r) => r.set_min_task_count(v, 0),
            None => false,
        }
    }

    /// `SetReactionMaxTaskCount(name, max_count)`.
    pub fn set_reaction_max_task_count(&mut self, name: &str, v: i32) -> bool {
        match self.reaction_lib.reaction_by_name_mut(name) {
            Some(r) => r.set_max_task_count(v, 0),
            None => false,
        }
    }

    /// `SetReactionMinCount(name, reaction_min_count)`.
    pub fn set_reaction_min_count(&mut self, name: &str, v: i32) -> bool {
        match self.reaction_lib.reaction_by_name_mut(name) {
            Some(r) => r.set_min_reaction_count(v, 0),
            None => false,
        }
    }

    /// `SetReactionMaxCount(name, reaction_max_count)`.
    pub fn set_reaction_max_count(&mut self, name: &str, v: i32) -> bool {
        match self.reaction_lib.reaction_by_name_mut(name) {
            Some(r) => r.set_max_reaction_count(v, 0),
            None => false,
        }
    }

    /// `SetResourceInflow(name, inflow)`.
    pub fn set_resource_inflow(&mut self, name: &str, inflow: f64) -> bool {
        match self.resource_lib.get_resource_id(name) {
            Some(i) => {
                self.resource_lib.resource_mut(i).inflow = inflow;
                true
            }
            None => false,
        }
    }

    /// `SetResourceOutflow(name, outflow)`.
    pub fn set_resource_outflow(&mut self, name: &str, outflow: f64) -> bool {
        match self.resource_lib.get_resource_id(name) {
            Some(i) => {
                self.resource_lib.resource_mut(i).outflow = outflow;
                true
            }
            None => false,
        }
    }

    /// `ChangeResource(reaction, res, process_num)`: point the process at the
    /// resource named `res` (`None` if unknown, after the stderr message).
    /// The C++ dereferences a missing process; here that returns false.
    pub fn change_resource(&mut self, reaction_id: usize, res: &str, process_num: i32) -> bool {
        let rid = self.resource_lib.get_resource_id(res);
        if process_num < 0 {
            return false;
        }
        match self
            .reaction_lib
            .reaction_mut(reaction_id)
            .processes
            .get_mut(process_num as usize)
        {
            Some(p) => {
                p.resource = rid;
                true
            }
            None => false,
        }
    }

    /// `AddGroupID(id)`.
    pub fn add_group_id(&mut self, id: i32) {
        self.possible_group_ids.insert(id);
    }
    /// `IsGroupID(id)`.
    pub fn is_group_id(&self, id: i32) -> bool {
        self.possible_group_ids.contains(&id)
    }
    /// `GetGroupIDs()` (ascending, like `std::set<int>`).
    pub fn group_ids(&self) -> &BTreeSet<i32> {
        &self.possible_group_ids
    }
    /// `AddTargetID(id)`: also recomputes the attack/prey target list.
    pub fn add_target_id(&mut self, id: i32, config: &Config) {
        self.possible_target_ids.insert(id);
        self.set_attack_prey_ft_list(config);
    }
    /// `IsTargetID(id)`.
    pub fn is_target_id(&self, id: i32) -> bool {
        self.possible_target_ids.contains(&id)
    }
    /// `GetTargetIDs()`.
    pub fn target_ids(&self) -> &BTreeSet<i32> {
        &self.possible_target_ids
    }
    /// `GetAttackPreyFTList()`.
    pub fn attack_prey_ft_list(&self) -> &[i32] {
        &self.pp_fts
    }
    /// `AddHabitat(habitat)`.
    pub fn add_habitat(&mut self, habitat: i32) {
        self.possible_habitats.insert(habitat);
    }
    /// `IsHabitat(habitat)`.
    pub fn is_habitat(&self, habitat: i32) -> bool {
        self.possible_habitats.contains(&habitat)
    }
    /// `GetHabitats()`.
    pub fn habitats(&self) -> &BTreeSet<i32> {
        &self.possible_habitats
    }
    /// `HasHammer()`.
    pub fn has_hammer(&self) -> bool {
        self.hammers
    }
    /// `HasPath()`.
    pub fn has_path(&self) -> bool {
        self.paths
    }

    /// `SetAttackPreyFTList()`: the forage-target list `[-3, -2,] -1,
    /// targets >= 0...` (the predator entries only when `PRED_PREY_SWITCH`
    /// is -2 or >= 0).
    pub fn set_attack_prey_ft_list(&mut self, config: &Config) {
        let mut has_pred = false;
        let mut offset: i32 = 1;
        if config.pred_prey_switch == -2 || config.pred_prey_switch > -1 {
            has_pred = true;
            offset = 3;
        }
        let mut dec_prey = false;
        let mut dec_pred = false;
        let mut dec_tpred = false;
        let mut num_targets: i32 = 0;
        for &t in &self.possible_target_ids {
            num_targets += 1;
            if t == -1 && !dec_prey {
                offset -= 1;
                dec_prey = true;
            }
            if t == -2 && !dec_pred {
                offset -= 1;
                dec_pred = true;
            }
            if t == -3 && !dec_tpred {
                offset -= 1;
                dec_tpred = true;
            }
        }
        let raw: Vec<i32> = self.possible_target_ids.iter().copied().collect();
        let tot_targets = (num_targets + offset).max(0) as usize;
        let mut target_list = vec![0i32; tot_targets];
        let mut set = |i: i32, v: i32| {
            // Out-of-range writes are undefined behaviour in the C++.
            if i >= 0 && (i as usize) < target_list.len() {
                target_list[i as usize] = v;
            }
        };
        set(0, -1);
        if has_pred {
            set(0, -3);
            set(1, -2);
            set(2, -1);
        }
        for (i, &v) in raw.iter().enumerate() {
            if v >= 0 {
                set(i as i32 + offset, v);
            }
        }
        self.pp_fts = target_list;
    }
}

impl<T: TaskRegistry> Environment<T> {
    /// `GetNumTasks()`.
    pub fn num_tasks(&self) -> i32 {
        self.tasklib.num_tasks()
    }
    /// `UseNeighborInput()`.
    pub fn use_neighbor_input(&self) -> bool {
        self.tasklib.use_neighbor_input()
    }
    /// `UseNeighborOutput()`.
    pub fn use_neighbor_output(&self) -> bool {
        self.tasklib.use_neighbor_output()
    }

    /// `SetReactionTask(name, task)`: attach the first task-library entry
    /// named `task`.
    pub fn set_reaction_task(&mut self, name: &str, task: &str) -> bool {
        let Some(rid) = self.reaction_lib.find(name) else {
            return false;
        };
        for i in 0..self.tasklib.num_tasks() {
            if self.tasklib.task_name(i) == task {
                self.reaction_lib.reaction_mut(rid).task = Some(i);
                return true;
            }
        }
        false
    }

    // -- Loading -----------------------------------------------------------

    /// `Load(filename, working_dir, feedback, defs)`: read an environment
    /// file (through `cInitFile`, with `defs` as `$NAME` mappings) and load
    /// every line. On failure the error is also returned. Messages go to
    /// `feedback` exactly as the C++ logs them.
    pub fn load(
        &mut self,
        filename: &str,
        working_dir: &str,
        feedback: &mut Feedback,
        defs: Option<&[(String, String)]>,
        config: &Config,
    ) -> Result<(), String> {
        let infile = InitFile::new(filename, working_dir, None, defs);
        if !infile.was_opened() {
            for (kind, msg) in infile.feedback().messages() {
                match kind {
                    FeedbackKind::Error => feedback.error(msg.clone()),
                    FeedbackKind::Warning => feedback.warning(msg.clone()),
                    FeedbackKind::Notification => feedback.notify(msg.clone()),
                }
            }
            feedback.error(format!("failed to load environment '{}'", filename));
            return Err(last_error(feedback));
        }

        for line_id in 0..infile.num_lines() {
            if !self.load_line_inner(&infile.get_line(line_id as i32), feedback, config) {
                return Err(last_error(feedback));
            }
        }

        // Make sure that all pre-declared reactions have been loaded correctly.
        for r in self.reaction_lib.reactions() {
            if r.task.is_none() {
                feedback.error(format!(
                    "pre-declared reaction '{}' never defined",
                    r.name()
                ));
                return Err(last_error(feedback));
            }
        }
        Ok(())
    }

    /// `LoadLine(line, feedback)`: one environment-file line.
    pub fn load_line(
        &mut self,
        line: &str,
        feedback: &mut Feedback,
        config: &Config,
    ) -> Result<(), String> {
        if self.load_line_inner(line, feedback, config) {
            Ok(())
        } else {
            Err(last_error(feedback))
        }
    }

    fn load_line_inner(&mut self, line: &str, fb: &mut Feedback, config: &Config) -> bool {
        let mut line = line.to_string();
        let mut ty = pop_word(&mut line);
        ty.make_ascii_uppercase();

        let load_ok = match ty.as_str() {
            "RESOURCE" => self.load_resource(line, fb, config),
            "REACTION" => self.load_reaction(line, fb, config),
            "SET_ACTIVE" => self.load_set_active(line, fb),
            "CELL" => self.load_cell(line, fb),
            "GRID" => self.load_state_grid(line, fb),
            "GRADIENT_RESOURCE" => self.load_gradient_resource(line, fb),
            _ => {
                fb.error(format!("unknown environment keyword '{}'", ty));
                return false;
            }
        };

        if !load_ok {
            fb.error(format!("failed in loading '{}'", ty));
            return false;
        }
        true
    }

    /// `LoadReactionProcess(reaction, desc, feedback)`. The process is
    /// attached before parsing, so a failure leaves it partially set (as in
    /// C++).
    fn load_reaction_process(&mut self, rid: usize, mut desc: String, fb: &mut Feedback) -> bool {
        let rname = self.reaction_lib.reaction(rid).name().to_string();
        let pidx = self.reaction_lib.reaction_mut(rid).add_process();
        let var_type = format!("reaction '{}' process", rname);

        while !desc.is_empty() {
            let var_entry = pop(&mut desc, ':');
            let Some((var_name, var_value)) = parse_setting(&var_entry, &var_type, fb) else {
                return false;
            };
            let resource_lib = &self.resource_lib;
            let p = &mut self.reaction_lib.reaction_mut(rid).processes[pidx];
            let vt = var_type.as_str();
            let vv = var_value.as_str();

            match var_name.as_str() {
                "resource" => {
                    let r = resource_lib.get_resource_id(vv);
                    if !assert_input_valid(r.is_some(), "resource", vt, vv, fb) {
                        return false;
                    }
                    p.resource = r;
                }
                "value" => {
                    if !assert_input_double(vv, "value", vt, fb) {
                        return false;
                    }
                    p.value = as_double(vv);
                }
                "type" => match process_type_from_str(vv) {
                    Some(t) => p.proc_type = t,
                    None => {
                        fb.error(format!(
                            "unknown reaction process type '{}' found in '{}'",
                            vv, rname
                        ));
                        return false;
                    }
                },
                "max" => {
                    if !assert_input_double(vv, "max", vt, fb) {
                        return false;
                    }
                    p.max_number = as_double(vv);
                }
                "min" => {
                    if !assert_input_double(vv, "min", vt, fb) {
                        return false;
                    }
                    p.min_number = as_double(vv);
                }
                "frac" => {
                    if !assert_input_double(vv, "frac", vt, fb) {
                        return false;
                    }
                    let mut in_frac = as_double(vv);
                    if in_frac > 1.0 {
                        in_frac = 1.0;
                    }
                    p.max_fraction = in_frac;
                }
                "ksubm" => {
                    if !assert_input_double(vv, "ksubm", vt, fb) {
                        return false;
                    }
                    p.k_sub_m = as_double(vv);
                }
                "product" => {
                    let r = resource_lib.get_resource_id(vv);
                    if !assert_input_valid(r.is_some(), "product", vt, vv, fb) {
                        return false;
                    }
                    p.product = r;
                }
                "conversion" => {
                    if !assert_input_double(vv, "conversion", vt, fb) {
                        return false;
                    }
                    p.conversion = as_double(vv);
                }
                "inst" => p.inst = var_value.clone(),
                "random" => {
                    if !assert_input_bool(vv, "random", vt, fb) {
                        return false;
                    }
                    p.is_random_resource = as_int(vv) != 0;
                }
                "lethal" => {
                    if !assert_input_double(vv, "lethal", vt, fb) {
                        return false;
                    }
                    p.lethal = as_double(vv);
                }
                "sterilize" => {
                    if !assert_input_bool(vv, "sterilize", vt, fb) {
                        return false;
                    }
                    p.sterilize = as_int(vv) != 0;
                }
                "deme" => {
                    if !assert_input_double(vv, "demefrac", vt, fb) {
                        return false;
                    }
                    // SetDemeFraction asserts 0 <= v <= 1 (compiled out).
                    p.deme_fraction = as_double(vv);
                }
                "germline" => {
                    if !assert_input_bool(vv, "germline", vt, fb) {
                        return false;
                    }
                    p.is_germline = as_int(vv) != 0;
                }
                "detect" => {
                    let r = resource_lib.get_resource_id(vv);
                    // QUIRK: the C++ reports a bad detect resource as "product".
                    if !assert_input_valid(r.is_some(), "product", vt, vv, fb) {
                        return false;
                    }
                    p.detect = r;
                }
                "threshold" => {
                    if !assert_input_double(vv, "threshold", vt, fb) {
                        return false;
                    }
                    p.detection_threshold = as_double(vv);
                }
                "detectionerror" => {
                    if !assert_input_double(vv, "detectionerror", vt, fb) {
                        return false;
                    }
                    p.detection_error = as_double(vv);
                }
                "string" => p.match_string = var_value.clone(),
                "depletable" => {
                    if !assert_input_bool(vv, "depletable", vt, fb) {
                        return false;
                    }
                    p.depletable = as_int(vv) != 0;
                }
                "phenplastbonus" => match PhenPlastBonusMethod::from_str_opt(vv) {
                    Some(m) => p.pp_method = m,
                    None => {
                        fb.error(format!(
                            "invalid setting for phenplastbonus in reaction '{}'",
                            rname
                        ));
                        return false;
                    }
                },
                "internal" => {
                    if !assert_input_bool(vv, "internal", vt, fb) {
                        return false;
                    }
                    p.internal = as_int(vv) != 0;
                }
                _ => {
                    fb.error(format!(
                        "unknown process variable '{}' in reaction '{}'",
                        var_name, rname
                    ));
                    return false;
                }
            }
        }
        true
    }

    /// `LoadReactionRequisite(reaction, desc, feedback)`.
    fn load_reaction_requisite(
        &mut self,
        rid: usize,
        mut desc: String,
        fb: &mut Feedback,
        config: &Config,
    ) -> bool {
        let rname = self.reaction_lib.reaction(rid).name().to_string();
        let qidx = self.reaction_lib.reaction_mut(rid).add_requisite();
        let var_type = format!("reaction '{}' requisite", rname);

        while !desc.is_empty() {
            let var_entry = pop(&mut desc, ':');
            let Some((var_name, var_value)) = parse_setting(&var_entry, &var_type, fb) else {
                return false;
            };
            let vt = var_type.as_str();
            let vv = var_value.as_str();
            let kind = requisite_var_kind(&var_name);
            // Reaction lookups happen before the requisite is borrowed.
            let looked_up = match kind {
                Some(ReqVar::Reaction) | Some(ReqVar::NoReaction) => self.reaction_lib.find(vv),
                _ => None,
            };
            let q = &mut self.reaction_lib.reaction_mut(rid).requisites[qidx];

            match kind {
                Some(ReqVar::Reaction) => {
                    if !assert_input_valid(looked_up.is_some(), "reaction", vt, vv, fb) {
                        return false;
                    }
                    q.prior_reactions.extend(looked_up);
                }
                Some(ReqVar::NoReaction) => {
                    if !assert_input_valid(looked_up.is_some(), "noreaction", vt, vv, fb) {
                        return false;
                    }
                    q.prior_noreactions.extend(looked_up);
                }
                Some(ReqVar::MinCount) => {
                    if !assert_input_int(vv, "min_count", vt, fb) {
                        return false;
                    }
                    q.min_task_count = as_int(vv);
                }
                Some(ReqVar::MaxCount) => {
                    if !assert_input_int(vv, "max_count", vt, fb) {
                        return false;
                    }
                    q.max_task_count = as_int(vv);
                }
                Some(ReqVar::ReactionMinCount) => {
                    if !assert_input_int(vv, "reaction_min_count", vt, fb) {
                        return false;
                    }
                    q.min_reaction_count = as_int(vv);
                }
                Some(ReqVar::ReactionMaxCount) => {
                    if !assert_input_int(vv, "reaction_max_count", vt, fb) {
                        return false;
                    }
                    q.max_reaction_count = as_int(vv);
                }
                Some(ReqVar::DivideOnly) => {
                    if !assert_input_int(vv, "divide_only", vt, fb) {
                        return false;
                    }
                    q.divide_only = as_int(vv);
                }
                Some(ReqVar::MinTotCount) => {
                    if !assert_input_int(vv, "min_tot_count", vt, fb) {
                        return false;
                    }
                    q.min_tot_reaction_count = as_int(vv);
                }
                Some(ReqVar::MaxTotCount) => {
                    if !assert_input_int(vv, "max_tot_count", vt, fb) {
                        return false;
                    }
                    q.max_tot_reaction_count = as_int(vv);
                }
                Some(ReqVar::ParasiteOnly) => {
                    if !assert_input_int(vv, "parasite_only", vt, fb) {
                        return false;
                    }
                    q.parasite_only = as_int(vv);
                }
                Some(ReqVar::CellBox) => {
                    let mut v = var_value.clone();
                    let xx = if !v.is_empty() {
                        as_int(&pop(&mut v, ','))
                    } else {
                        -1
                    };
                    let yy = if !v.is_empty() {
                        as_int(&pop(&mut v, ','))
                    } else {
                        -1
                    };
                    let width = if !v.is_empty() {
                        as_int(&pop(&mut v, ','))
                    } else {
                        1
                    };
                    let height = if !v.is_empty() { as_int(&v) } else { 1 };

                    match cellbox_validate(xx, yy, width, height, config.world_x, config.world_y) {
                        1 => {
                            fb.error(
                                "cellbox requisite requires 0 >= < xx < WORLD_X for first argument",
                            );
                            return false;
                        }
                        2 => {
                            fb.error(
                                "cellbox requisite requires 0 >= < yy < WORLD_Y for 2nd argument",
                            );
                            return false;
                        }
                        3 => {
                            fb.error("cellbox requisite requires 0 < width+xx < WORLD_X for 3rd argument");
                            return false;
                        }
                        4 => {
                            fb.error("cellbox requisite requires 0 < height+yy < WORLD_Y for 4th argument");
                            return false;
                        }
                        _ => {}
                    }
                    q.cell_box = CellBox::new(xx, yy, width, height);
                }
                None => {
                    fb.error(format!(
                        "unknown requisite variable '{}' in reaction '{}'",
                        var_name, rname
                    ));
                    return false;
                }
            }
        }
        true
    }

    /// `LoadContextReactionRequisite(reaction, desc, feedback)`: as
    /// `LoadReactionRequisite` without `cellbox`.
    fn load_context_reaction_requisite(
        &mut self,
        rid: usize,
        mut desc: String,
        fb: &mut Feedback,
    ) -> bool {
        let rname = self.reaction_lib.reaction(rid).name().to_string();
        let qidx = self.reaction_lib.reaction_mut(rid).add_context_requisite();
        let var_type = format!("reaction '{}' requisite", rname);

        while !desc.is_empty() {
            let var_entry = pop(&mut desc, ':');
            let Some((var_name, var_value)) = parse_setting(&var_entry, &var_type, fb) else {
                return false;
            };
            let vt = var_type.as_str();
            let vv = var_value.as_str();
            let kind = requisite_var_kind(&var_name);
            let looked_up = match kind {
                Some(ReqVar::Reaction) | Some(ReqVar::NoReaction) => self.reaction_lib.find(vv),
                _ => None,
            };
            let q = &mut self.reaction_lib.reaction_mut(rid).context_requisites[qidx];

            match kind {
                Some(ReqVar::Reaction) => {
                    if !assert_input_valid(looked_up.is_some(), "reaction", vt, vv, fb) {
                        return false;
                    }
                    q.prior_reactions.extend(looked_up);
                }
                Some(ReqVar::NoReaction) => {
                    if !assert_input_valid(looked_up.is_some(), "noreaction", vt, vv, fb) {
                        return false;
                    }
                    q.prior_noreactions.extend(looked_up);
                }
                Some(ReqVar::MinCount) => {
                    if !assert_input_int(vv, "min_count", vt, fb) {
                        return false;
                    }
                    q.min_task_count = as_int(vv);
                }
                Some(ReqVar::MaxCount) => {
                    if !assert_input_int(vv, "max_count", vt, fb) {
                        return false;
                    }
                    q.max_task_count = as_int(vv);
                }
                Some(ReqVar::ReactionMinCount) => {
                    if !assert_input_int(vv, "reaction_min_count", vt, fb) {
                        return false;
                    }
                    q.min_reaction_count = as_int(vv);
                }
                Some(ReqVar::ReactionMaxCount) => {
                    if !assert_input_int(vv, "reaction_max_count", vt, fb) {
                        return false;
                    }
                    q.max_reaction_count = as_int(vv);
                }
                Some(ReqVar::DivideOnly) => {
                    if !assert_input_int(vv, "divide_only", vt, fb) {
                        return false;
                    }
                    q.divide_only = as_int(vv);
                }
                Some(ReqVar::MinTotCount) => {
                    if !assert_input_int(vv, "min_tot_count", vt, fb) {
                        return false;
                    }
                    q.min_tot_reaction_count = as_int(vv);
                }
                Some(ReqVar::MaxTotCount) => {
                    if !assert_input_int(vv, "max_tot_count", vt, fb) {
                        return false;
                    }
                    q.max_tot_reaction_count = as_int(vv);
                }
                Some(ReqVar::ParasiteOnly) => {
                    if !assert_input_int(vv, "parasite_only", vt, fb) {
                        return false;
                    }
                    q.parasite_only = as_int(vv);
                }
                Some(ReqVar::CellBox) | None => {
                    fb.error(format!(
                        "unknown requisite variable '{}' in reaction '{}'",
                        var_name, rname
                    ));
                    return false;
                }
            }
        }
        true
    }

    /// `AddResource` for the loaders; the C++ dereferences the `NULL` it
    /// returns once initial levels exist, here that is an error.
    fn add_resource_checked(&mut self, name: &str, fb: &mut Feedback) -> Option<usize> {
        let r = self.resource_lib.add_resource(name);
        if r.is_none() {
            fb.error(format!(
                "cannot add resource '{}' after initial resource levels were computed",
                name
            ));
        }
        r
    }

    /// `LoadResource(desc, feedback)`: `RESOURCE name[:var=value...] ...`.
    fn load_resource(&mut self, mut desc: String, fb: &mut Feedback, config: &Config) -> bool {
        if desc.is_empty() {
            fb.warning("resource line with no resources listed");
            return false;
        }

        while !desc.is_empty() {
            let mut cur_resource = pop_word(&mut desc);
            let name = pop(&mut cur_resource, ':');

            // An existing resource (e.g. created by CELL) is modified in place.
            let rid = if !self.resource_lib.does_resource_exist(&name) {
                match self.add_resource_checked(&name, fb) {
                    Some(r) => r,
                    None => return false,
                }
            } else {
                match self.resource_lib.find(&name) {
                    Some(r) => r,
                    None => return false,
                }
            };
            let var_type = format!("resource '{}'", name);

            while !cur_resource.is_empty() {
                let var_entry = pop(&mut cur_resource, ':');
                let Some((var_name, var_value)) = parse_setting(&var_entry, &var_type, fb) else {
                    return false;
                };
                let vt = var_type.as_str();
                let vv = var_value.as_str();
                let r = self.resource_lib.resource_mut(rid);

                macro_rules! dbl {
                    ($label:expr, $field:ident) => {{
                        if !assert_input_double(vv, $label, vt, fb) {
                            return false;
                        }
                        r.$field = as_double(vv);
                    }};
                }
                macro_rules! int {
                    ($label:expr, $field:ident) => {{
                        if !assert_input_int(vv, $label, vt, fb) {
                            return false;
                        }
                        r.$field = as_int(vv);
                    }};
                }

                match var_name.as_str() {
                    "inflow" => dbl!("inflow", inflow),
                    "outflow" => dbl!("outflow", outflow),
                    "initial" => dbl!("initial", initial),
                    "geometry" => {
                        if !r.set_geometry(vv) {
                            fb.error(format!("in {}, {} unknown geometry", vt, vv));
                            return false;
                        }
                    }
                    "cells" => {
                        let mut v = var_value.clone();
                        let cell_list = return_array(&mut v);
                        r.set_cell_id_list(&cell_list);
                    }
                    "inflowx1" | "inflowx" => int!("inflowX1", inflow_x1),
                    "inflowx2" => int!("inflowX2", inflow_x2),
                    "inflowy1" | "inflowy" => int!("inflowY1", inflow_y1),
                    "inflowy2" => int!("inflowY2", inflow_y2),
                    "outflowx1" | "outflowx" => int!("outflowX1", outflow_x1),
                    "outflowx2" => int!("outflowX2", outflow_x2),
                    "outflowy1" | "outflowy" => int!("outflowY1", outflow_y1),
                    "outflowy2" => int!("outflowY2", outflow_y2),
                    "xdiffuse" => dbl!("xdiffuse", xdiffuse),
                    "xgravity" => dbl!("xgravity", xgravity),
                    "ydiffuse" => dbl!("ydiffuse", ydiffuse),
                    "ygravity" => dbl!("ygravity", ygravity),
                    "deme" => {
                        if !r.set_deme_resource(vv) {
                            fb.error(format!("in {}, {} must be true or false", vt, vv));
                            return false;
                        }
                    }
                    "collectable" => {
                        if !assert_input_bool(vv, "collectable", vt, fb) {
                            return false;
                        }
                        r.collectable = as_int(vv) != 0;
                    }
                    "energy" => {
                        if !r.set_energy_resource(vv) {
                            fb.error(format!("in {}, {} must be true or false", vt, vv));
                            return false;
                        } else if !config.energy_enabled {
                            fb.error("energy resources can not be used without the energy model");
                            return false;
                        }
                    }
                    "hgt" => {
                        // Genome fragments present in cells, metabolised as a resource.
                        if !assert_input_bool(vv, "hgt", vt, fb) {
                            return false;
                        }
                        r.hgt_metabolize = as_int(vv) != 0;
                    }
                    _ => {
                        fb.error(format!(
                            "unknown variable '{}' in resource '{}'",
                            var_name, name
                        ));
                        return false;
                    }
                }
            }

            // Now that all geometry, etc. information is known, give the
            // resource an index within its own type.
            self.resource_lib.set_resource_index(rid);

            let r = self.resource_lib.resource_mut(rid);
            // Prevent misconfiguration of HGT (see the inflow_x1 QUIRK on
            // `Resource::new`).
            if r.hgt_metabolize
                && (r.geometry != geometry::GLOBAL
                    || r.initial > 0.0
                    || r.inflow > 0.0
                    || r.outflow > 0.0
                    || r.inflow_x1 != Resource::NONE
                    || r.inflow_x2 != Resource::NONE
                    || r.inflow_y1 != Resource::NONE
                    || r.inflow_y2 != Resource::NONE
                    || r.xdiffuse != 1.0
                    || r.xgravity != 0.0
                    || r.ydiffuse != 1.0
                    || r.ygravity != 0.0
                    || r.deme_resource)
            {
                fb.error(format!("misconfigured HGT resource: {}", name));
                return false;
            }
            if r.hgt_metabolize && config.enable_hgt == 0 {
                fb.error("resource configured to use HGT, but HGT not enabled");
                return false;
            }

            // Valid X/Y1 but no X/Y2: the user means a single point.
            if r.inflow_x1 >= 0 && r.inflow_x2 == Resource::NONE {
                r.inflow_x2 = r.inflow_x1;
            }
            if r.inflow_y1 >= 0 && r.inflow_y2 == Resource::NONE {
                r.inflow_y2 = r.inflow_y1;
            }
            // QUIRK: the outflow checks use `> 0`, not `>= 0`.
            if r.outflow_x1 > 0 && r.outflow_x2 == Resource::NONE {
                r.outflow_x2 = r.outflow_x1;
            }
            if r.outflow_y1 > 0 && r.outflow_y2 == Resource::NONE {
                r.outflow_y2 = r.outflow_y1;
            }
        }
        true
    }

    /// `LoadCell(desc, feedback)`: `CELL name:cell_list[:initial=..:inflow=..:outflow=..] ...`.
    ///
    /// QUIRK: unlike `LoadResource`, this never assigns the resource an
    /// index.
    fn load_cell(&mut self, mut desc: String, fb: &mut Feedback) -> bool {
        if desc.is_empty() {
            fb.warning("CELL line with no resources listed");
            return false;
        }

        while !desc.is_empty() {
            let mut cur_resource = pop_word(&mut desc);
            let name = pop(&mut cur_resource, ':');

            let rid = if !self.resource_lib.does_resource_exist(&name) {
                let Some(rid) = self.add_resource_checked(&name, fb) else {
                    return false;
                };
                let r = self.resource_lib.resource_mut(rid);
                r.initial = 0.0;
                r.inflow = 0.0;
                r.outflow = 0.0;
                r.set_geometry("GRID");
                r.inflow_x1 = Resource::NONE;
                r.inflow_x2 = Resource::NONE;
                r.inflow_y1 = Resource::NONE;
                r.inflow_y2 = Resource::NONE;
                r.outflow_x1 = Resource::NONE;
                r.outflow_x2 = Resource::NONE;
                r.outflow_y1 = Resource::NONE;
                r.outflow_y2 = Resource::NONE;
                r.xdiffuse = 0.0;
                r.xgravity = 0.0;
                r.ydiffuse = 0.0;
                r.ygravity = 0.0;
                r.set_deme_resource("false");
                rid
            } else {
                match self.resource_lib.find(&name) {
                    Some(r) => r,
                    None => return false,
                }
            };

            let mut cell_list_str = pop(&mut cur_resource, ':');
            let cell_list = return_array(&mut cell_list_str);
            let mut tmp_initial = 0.0;
            let mut tmp_inflow = 0.0;
            let mut tmp_outflow = 0.0;
            let var_type = format!("resource '{}'", name);
            while !cur_resource.is_empty() {
                let var_entry = pop(&mut cur_resource, ':');
                let Some((var_name, var_value)) = parse_setting(&var_entry, &var_type, fb) else {
                    return false;
                };
                let vv = var_value.as_str();
                match var_name.as_str() {
                    "inflow" => {
                        if !assert_input_double(vv, "inflow", &var_type, fb) {
                            return false;
                        }
                        tmp_inflow = as_double(vv);
                    }
                    "outflow" => {
                        if !assert_input_double(vv, "outflow", &var_type, fb) {
                            return false;
                        }
                        tmp_outflow = as_double(vv);
                    }
                    "initial" => {
                        if !assert_input_double(vv, "initial", &var_type, fb) {
                            return false;
                        }
                        tmp_initial = as_double(vv);
                    }
                    _ => {
                        fb.error(format!(
                            "unknown variable '{}' in resource '{}'",
                            var_name, name
                        ));
                        return false;
                    }
                }
            }
            let r = self.resource_lib.resource_mut(rid);
            for &cell in &cell_list {
                match r.cell_resource_mut(cell) {
                    Some(c) => {
                        Resource::update_cell_resource(c, tmp_initial, tmp_inflow, tmp_outflow)
                    }
                    None => r.add_cell_resource(CellResource::new(
                        cell,
                        tmp_initial,
                        tmp_inflow,
                        tmp_outflow,
                    )),
                }
            }
        }
        true
    }

    /// `LoadReaction(desc, feedback)`:
    /// `REACTION name trigger[:args] [process:...] [requisite:...] [context_requisite:...]`.
    fn load_reaction(&mut self, mut desc: String, fb: &mut Feedback, config: &Config) -> bool {
        if desc.is_empty() {
            fb.error("each reaction must include a name and trigger");
            return false;
        }

        let name = pop_word(&mut desc);
        let rid = self.reaction_lib.add_reaction(&name);

        // Only a name: a pre-declaration.
        if desc.is_empty() {
            return true;
        }

        if self.reaction_lib.reaction(rid).task.is_some() {
            fb.warning(format!("re-defining reaction '{}'", name));
        }

        let mut trigger_info = pop_word(&mut desc);
        let trigger = pop(&mut trigger_info, ':');

        // Load the task trigger.
        let mut envreqs = EnvReqs::new();
        let mut hooks = TaskEnvHooks::new(
            &self.resource_lib,
            &mut self.possible_group_ids,
            &mut self.possible_target_ids,
        );
        let task = self
            .tasklib
            .add_task(&trigger, &trigger_info, &mut envreqs, fb, &mut hooks);
        if hooks.target_added() {
            self.set_attack_prey_ft_list(config);
        }
        let Some(task_id) = task else {
            return false;
        };
        self.reaction_lib.reaction_mut(rid).task = Some(task_id);

        // Environment::Manager::DefineActionTrigger(name, desc, product,
        // tasklib size - 1): the first definition of a trigger id wins.
        // QUIRK: the ordering is the last task's id, even when AddTask
        // returned an existing entry.
        let trig_name = self.tasklib.task_name(task_id).to_string();
        if !self.action_triggers.iter().any(|t| t.id == trig_name) {
            let trig_desc = self.tasklib.task_desc(task_id).to_string();
            let order = self.tasklib.num_tasks() - 1;
            self.action_triggers.push(ActionTrigger {
                id: trig_name,
                desc: trig_desc,
                temp_ordering: order,
            });
        }

        while !desc.is_empty() {
            let mut desc_entry = pop_word(&mut desc);
            let mut entry_type = pop(&mut desc_entry, ':');
            entry_type.make_ascii_lowercase();

            match entry_type.as_str() {
                "process" => {
                    if !self.load_reaction_process(rid, desc_entry, fb) {
                        fb.error("failed in loading reaction-process...");
                        return false;
                    }
                }
                "requisite" => {
                    if !self.load_reaction_requisite(rid, desc_entry, fb, config) {
                        fb.error("failed in loading reaction-requisite...");
                        return false;
                    }
                }
                "context_requisite" => {
                    if !self.load_context_reaction_requisite(rid, desc_entry, fb) {
                        fb.error("failed in loading reaction-requisite...");
                        return false;
                    }
                }
                _ => {
                    fb.error(format!(
                        "unknown entry type '{}' in reaction '{}'",
                        entry_type, name
                    ));
                    return false;
                }
            }
        }

        // Process the environment requirements of this task.
        if envreqs.min_inputs > self.input_size {
            self.input_size = envreqs.min_inputs;
        }
        if envreqs.min_outputs > self.output_size {
            self.output_size = envreqs.min_outputs;
        }
        if envreqs.true_rand_inputs {
            self.true_rand = true;
        }
        true
    }

    /// `LoadGradientResource(desc, feedback)`:
    /// `GRADIENT_RESOURCE name[:var=value...] ...`.
    ///
    /// QUIRK: like `LoadCell`, this never assigns the resource an index.
    fn load_gradient_resource(&mut self, mut desc: String, fb: &mut Feedback) -> bool {
        if desc.is_empty() {
            fb.error("gradient resource line with no resources listed");
            return false;
        }

        while !desc.is_empty() {
            let mut cur_resource = pop_word(&mut desc);
            let name = pop(&mut cur_resource, ':');

            let rid = if !self.resource_lib.does_resource_exist(&name) {
                match self.add_resource_checked(&name, fb) {
                    Some(r) => r,
                    None => return false,
                }
            } else {
                match self.resource_lib.find(&name) {
                    Some(r) => r,
                    None => return false,
                }
            };

            {
                let r = self.resource_lib.resource_mut(rid);
                r.set_geometry("grid");
                r.initial = 0.0;
                r.outflow = 1.0;
                r.xdiffuse = 0.0;
                r.ydiffuse = 0.0;
                r.xgravity = 0.0;
                r.ygravity = 0.0;
                r.isgradient = true;
            }

            let var_type = format!("gradient resource '{}'", name);
            while !cur_resource.is_empty() {
                let var_entry = pop(&mut cur_resource, ':');
                let Some((var_name, var_value)) = parse_setting(&var_entry, &var_type, fb) else {
                    return false;
                };
                let vt = var_type.as_str();
                let vv = var_value.as_str();
                let r = self.resource_lib.resource_mut(rid);

                macro_rules! dbl {
                    ($label:expr, $field:ident) => {{
                        if !assert_input_double(vv, $label, vt, fb) {
                            return false;
                        }
                        r.$field = as_double(vv);
                    }};
                }
                macro_rules! int {
                    ($label:expr, $field:ident) => {{
                        if !assert_input_int(vv, $label, vt, fb) {
                            return false;
                        }
                        r.$field = as_int(vv);
                    }};
                }

                match var_name.as_str() {
                    "peakx" => int!("peakx", peakx),
                    "peaky" => int!("peaky", peaky),
                    "height" => int!("height", height),
                    "spread" => int!("spread", spread),
                    "plateau" => dbl!("plateau", plateau),
                    "decay" => int!("decay", decay),
                    "max_x" => int!("max_x", max_x),
                    "max_y" => int!("max_y", max_y),
                    "min_x" => int!("min_x", min_x),
                    "min_y" => int!("min_y", min_y),
                    "move_a_scaler" => dbl!("move_a_scaler", move_a_scaler),
                    "updatestep" => int!("updatestep", updatestep),
                    "halo" => int!("halo", halo),
                    "halo_inner_radius" => int!("halo_inner_radius", halo_inner_radius),
                    // (sic) "halo_halo_anchor_*"
                    "halo_anchor_x" => int!("halo_halo_anchor_x", halo_anchor_x),
                    "halo_anchor_y" => int!("halo_halo_anchor_y", halo_anchor_y),
                    "move_speed" => int!("move_speed", move_speed),
                    "move_resistance" => int!("move_resistance", move_resistance),
                    "halo_width" => int!("halo_width", halo_width),
                    "plateau_inflow" => dbl!("plateau_inflow", plateau_inflow),
                    "plateau_outflow" => dbl!("plateau_outflow", plateau_outflow),
                    "cone_inflow" => dbl!("cone_inflow", cone_inflow),
                    "cone_outflow" => dbl!("cone_outflow", cone_outflow),
                    "gradient_inflow" => dbl!("gradient_inflow", gradient_inflow),
                    "initial" => dbl!("initial", init_plat),
                    "common" => int!("common", is_plateau_common),
                    "floor" => dbl!("floor", floor),
                    "habitat" => {
                        int!("habitat", habitat);
                        self.possible_habitats.insert(as_int(vv));
                    }
                    "min_size" => int!("min_size", min_size),
                    "max_size" => int!("max_size", max_size),
                    "config" => int!("config", config),
                    "count" => int!("count", count),
                    "resistance" => dbl!("resistance", resistance),
                    "damage" => dbl!("damage", damage),
                    "deadly" => {
                        if !assert_input_double(vv, "deadly", vt, fb) {
                            return false;
                        }
                        r.set_deadly(as_double(vv));
                    }
                    "path" => {
                        if !assert_input_int(vv, "path", vt, fb) {
                            return false;
                        }
                        r.path = as_int(vv) != 0;
                        self.paths = true;
                    }
                    "hammer" => {
                        if !assert_input_int(vv, "hammer", vt, fb) {
                            return false;
                        }
                        r.hammer = as_int(vv) != 0;
                        self.hammers = true;
                    }
                    "threshold" => dbl!("threshold", threshold),
                    "refuge" => int!("refuge", refuge),
                    _ => {
                        fb.error(format!(
                            "unknown variable '{}' in gradient resource '{}'",
                            var_name, name
                        ));
                        return false;
                    }
                }
            }
        }
        true
    }

    /// `LoadStateGrid(desc, feedback)`:
    /// `GRID name:width=..:height=..:initx=..:inity=..:initfacing=..:states=..:grid=..`.
    fn load_state_grid(&mut self, mut desc: String, fb: &mut Feedback) -> bool {
        let name = pop(&mut desc, ':');

        let mut schema = ArgSchema::new(':', '=');
        schema.add_int_range("width", 0, 0, i32::MAX);
        schema.add_int_range("height", 1, 0, i32::MAX);
        schema.add_int_range("initx", 2, 0, i32::MAX);
        schema.add_int_range("inity", 3, 0, i32::MAX);
        schema.add_int_range("initfacing", 4, 0, 7);
        schema.add_string("states", 0);
        schema.add_string("grid", 1);

        let Some(args) = ArgContainer::load(&desc, &schema, fb) else {
            return false;
        };

        let width = args.int(0);
        let height = args.int(1);
        let initx = args.int(2);
        let inity = args.int(3);
        let initfacing = args.int(4);

        if initx >= width || inity >= height {
            fb.error("initx and inity must not exceed (width - 1) and (height - 1)");
            return false;
        }

        // Load the states.
        let mut states: Vec<String> = Vec::new();
        let mut state_sense: Vec<i32> = Vec::new();
        let mut statestr = args.string(0).to_string();
        trim(&mut statestr);
        while !statestr.is_empty() {
            let mut statesensestr = pop(&mut statestr, ',');
            let mut statename = pop(&mut statesensestr, '=');
            trim(&mut statename);

            if states.contains(&statename) {
                fb.error(format!(
                    "duplicate state identifier for state grid {}",
                    name
                ));
                return false;
            }
            states.push(statename);

            // Sense value defaults to the order in which states are loaded.
            let mut state_sense_value = states.len() as i32;
            if !statesensestr.is_empty() {
                state_sense_value = as_int(&statesensestr);
            }
            state_sense.push(state_sense_value);
        }
        if states.is_empty() {
            fb.error(format!("no states defined for state grid {}", name));
            return false;
        }

        // Load the state grid itself. The C++ allocates `width * height`
        // cells up front (an `int` product; a negative or overflowing size is
        // undefined there, and out-of-range values only log an error in
        // `ArgContainer`). Here the size is computed exactly and cells are
        // collected as parsed, so a bogus size cannot force a huge
        // allocation; the outcome for a valid grid is the same.
        let size = (width as i64 * height as i64).max(0) as u64;
        let mut lgrid: Vec<i32> = Vec::new();
        let mut gridstr = args.string(1).to_string();
        while !gridstr.is_empty() && (lgrid.len() as u64) < size {
            let mut statename = pop(&mut gridstr, ',');
            trim(&mut statename);
            match states.iter().position(|s| *s == statename) {
                Some(i) => lgrid.push(i as i32),
                None => {
                    let c = lgrid.len() as i32;
                    fb.error(format!(
                        "state identifier undefined for cell ({}, {}) in state grid {}",
                        c / width,
                        c % width,
                        name
                    ));
                    return false;
                }
            }
        }
        if lgrid.len() as u64 != size || !gridstr.is_empty() {
            fb.error(format!(
                "grid definition size mismatch for state grid {}",
                name
            ));
            return false;
        }

        // Invert row ordering: the highest indexed row comes first. (With
        // both dimensions negative the C++ loops do not run either.)
        let mut grid = vec![0i32; lgrid.len()];
        if width > 0 && height > 0 {
            let (w, h) = (width as usize, height as usize);
            for y in 0..h {
                let off = y * w;
                let loff = (h - y - 1) * w;
                grid[off..off + w].copy_from_slice(&lgrid[loff..loff + w]);
            }
        }

        self.state_grids.push(StateGrid::new(
            &name,
            width,
            height,
            initx,
            inity,
            initfacing,
            states,
            state_sense,
            grid,
        ));
        true
    }

    /// `LoadSetActive(desc, feedback)`: `SET_ACTIVE REACTION name [0|false]`.
    fn load_set_active(&mut self, mut desc: String, fb: &mut Feedback) -> bool {
        let mut item_type = pop_word(&mut desc);
        item_type.make_ascii_uppercase();
        let item_name = pop_word(&mut desc);
        let mut item_active = pop_word(&mut desc);
        item_active.make_ascii_uppercase();

        let new_active = !(item_active == "0" || item_active == "FALSE");

        if item_type == "REACTION" {
            match self.reaction_lib.reaction_by_name_mut(&item_name) {
                Some(r) => r.active = new_active,
                None => {
                    fb.error(format!("unknown REACTION: '{}'", item_name));
                    return false;
                }
            }
        } else if item_type.is_empty() {
            fb.notify("format: SET_ACTIVE <type> <name> <new_status=true>");
        } else {
            fb.error(format!("cannot deactivate items of type {}", item_type));
            return false;
        }
        true
    }
}
