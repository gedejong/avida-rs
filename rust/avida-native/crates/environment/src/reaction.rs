//! Ports of `cReactionProcess` (cReactionProcess.h), `cReactionRequisite`
//! (cReactionRequisite.h), `cCellBox` (cCellBox.h),
//! `cContextReactionRequisite` (cContextReactionRequisite.h), `cReaction`
//! (cReaction.{h,cc}) and `cReactionLib` (cReactionLib.{h,cc}), all in
//! avida-core/source/main/.
//!
//! Pointers become ids: a process names its resources by resource-library
//! id, a requisite names reactions by reaction id, and a reaction names its
//! task by task-library id.

/// `nReaction::PROCTYPE_*` (nReaction.h).
pub mod proc_type {
    /// `PROCTYPE_ADD`
    pub const ADD: i32 = 0;
    /// `PROCTYPE_MULT`
    pub const MULT: i32 = 1;
    /// `PROCTYPE_POW`
    pub const POW: i32 = 2;
    /// `PROCTYPE_LIN`
    pub const LIN: i32 = 3;
    /// `PROCTYPE_ENERGY`
    pub const ENERGY: i32 = 4;
    /// `PROCTYPE_ENZYME`
    pub const ENZYME: i32 = 5;
    /// `PROCTYPE_EXP`
    pub const EXP: i32 = 6;
}

/// `avd_env_process_type` (rust/avida-rust/src/environment_helpers.rs):
/// process `type=` names, case-sensitive.
pub fn process_type_from_str(s: &str) -> Option<i32> {
    match s {
        "add" => Some(proc_type::ADD),
        "mult" => Some(proc_type::MULT),
        "pow" => Some(proc_type::POW),
        "lin" => Some(proc_type::LIN),
        "energy" => Some(proc_type::ENERGY),
        "enzyme" => Some(proc_type::ENZYME),
        "exp" => Some(proc_type::EXP),
        _ => None,
    }
}

/// `ePHENPLAST_BONUS_METHOD` (avida/core/Definitions.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhenPlastBonusMethod {
    /// `DEFAULT`
    #[default]
    Default = 0,
    /// `NO_BONUS`
    NoBonus = 1,
    /// `FRAC_BONUS`
    FracBonus = 2,
    /// `FULL_BONUS`
    FullBonus = 3,
}

impl PhenPlastBonusMethod {
    /// `avd_env_phenplast_bonus_method`: `phenplastbonus=` names,
    /// case-sensitive.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "default" => Some(Self::Default),
            "nobonus" => Some(Self::NoBonus),
            "fracbonus" => Some(Self::FracBonus),
            "fullbonus" => Some(Self::FullBonus),
            _ => None,
        }
    }
}

/// `cReactionProcess`: one way a reaction rewards an organism.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactionProcess {
    /// `resource`: resource consumed (`None` = infinite).
    pub resource: Option<usize>,
    /// `value`: efficiency.
    pub value: f64,
    /// `type`: a `proc_type` constant.
    pub proc_type: i32,
    /// `max_number`: max quantity of resource usable.
    pub max_number: f64,
    /// `min_number`: minimum quantity needed (otherwise 0).
    pub min_number: f64,
    /// `max_fraction`: max fraction of available resource usable.
    pub max_fraction: f64,
    /// `k_sub_m` (enzyme reactions).
    pub k_sub_m: f64,
    /// `product`: output resource.
    pub product: Option<usize>,
    /// `conversion`: product conversion factor.
    pub conversion: f64,
    /// `is_random_resource`
    pub is_random_resource: bool,
    /// `lethal`: lethality probability.
    pub lethal: f64,
    /// `sterilize`
    pub sterilize: bool,
    /// `deme_fraction`: fraction of the reward applied to the deme.
    pub deme_fraction: f64,
    /// `is_germline`: reward germline propensity instead of bonus.
    pub is_germline: bool,
    /// `match_string`
    pub match_string: String,
    /// `inst`: instruction triggered on success.
    pub inst: String,
    /// `depletable`: does completing consume the resource?
    pub depletable: bool,
    /// `internal`: use resource bins.
    pub internal: bool,
    /// `m_ppmethod`
    pub pp_method: PhenPlastBonusMethod,
    /// `detect`: resource measured.
    pub detect: Option<usize>,
    /// `detection_threshold`
    pub detection_threshold: f64,
    /// `detection_error`
    pub detection_error: f64,
}

impl Default for ReactionProcess {
    /// `cReactionProcess()`.
    fn default() -> Self {
        ReactionProcess {
            resource: None,
            value: 1.0,
            proc_type: proc_type::ADD,
            max_number: 1.0,
            min_number: 0.0,
            max_fraction: 1.0,
            k_sub_m: 0.0,
            product: None,
            conversion: 1.0,
            is_random_resource: false,
            lethal: 0.0,
            sterilize: false,
            deme_fraction: 0.0,
            is_germline: false,
            match_string: String::new(),
            inst: String::new(),
            depletable: true,
            internal: false,
            pp_method: PhenPlastBonusMethod::Default,
            detect: None,
            detection_threshold: 0.0,
            detection_error: 0.0,
        }
    }
}

/// `cCellBox`: a rectangle of cells; the default box contains every cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellBox {
    /// `m_x`
    pub x: i32,
    /// `m_y`
    pub y: i32,
    /// `m_width`
    pub width: i32,
    /// `m_height`
    pub height: i32,
}

impl Default for CellBox {
    /// `cCellBox()`: all -1 (matches everything).
    fn default() -> Self {
        CellBox {
            x: -1,
            y: -1,
            width: -1,
            height: -1,
        }
    }
}

impl CellBox {
    /// `cCellBox(xx, yy, width, height)`.
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        CellBox {
            x,
            y,
            width,
            height,
        }
    }

    /// `InCellBox(cell_id, world_x, world_y)`: true for an unset box.
    pub fn in_cell_box(&self, cell_id: i32, world_x: i32, _world_y: i32) -> bool {
        if 0 > self.x || 0 > self.y || 0 >= self.width || 0 >= self.height {
            return true;
        }
        // A zero WORLD_X is a division by zero in C++; treat as outside.
        let (Some(cell_x), Some(cell_y)) =
            (cell_id.checked_rem(world_x), cell_id.checked_div(world_x))
        else {
            return false;
        };
        self.x <= cell_x
            && cell_x < self.x.wrapping_add(self.width)
            && self.y <= cell_y
            && cell_y < self.y.wrapping_add(self.height)
    }
}

/// `cReactionRequisite`: one alternative set of conditions for a reaction.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactionRequisite {
    /// `prior_reaction_list` (reaction ids that must have been done)
    pub prior_reactions: Vec<usize>,
    /// `prior_noreaction_list` (reaction ids that must not have been done)
    pub prior_noreactions: Vec<usize>,
    /// `min_task_count`
    pub min_task_count: i32,
    /// `max_task_count`
    pub max_task_count: i32,
    /// `min_reaction_count`
    pub min_reaction_count: i32,
    /// `max_reaction_count`
    pub max_reaction_count: i32,
    /// `min_tot_reaction_count`
    pub min_tot_reaction_count: i32,
    /// `max_tot_reaction_count`
    pub max_tot_reaction_count: i32,
    /// `divide_only`: 0 = IO only, 1 = divide only, 2 = always.
    pub divide_only: i32,
    /// `parasite_only`
    pub parasite_only: i32,
    /// `cell_box`
    pub cell_box: CellBox,
}

impl Default for ReactionRequisite {
    /// `cReactionRequisite()`.
    fn default() -> Self {
        ReactionRequisite {
            prior_reactions: Vec::new(),
            prior_noreactions: Vec::new(),
            min_task_count: 0,
            max_task_count: i32::MAX,
            min_reaction_count: 0,
            max_reaction_count: i32::MAX,
            min_tot_reaction_count: 0,
            max_tot_reaction_count: i32::MAX,
            divide_only: 0,
            parasite_only: 0,
            cell_box: CellBox::default(),
        }
    }
}

/// `cContextReactionRequisite`: a requisite tested against the context
/// phenotype's counts (no cell box).
#[derive(Debug, Clone, PartialEq)]
pub struct ContextReactionRequisite {
    /// `prior_reaction_list`
    pub prior_reactions: Vec<usize>,
    /// `prior_noreaction_list`
    pub prior_noreactions: Vec<usize>,
    /// `min_task_count`
    pub min_task_count: i32,
    /// `max_task_count`
    pub max_task_count: i32,
    /// `min_reaction_count`
    pub min_reaction_count: i32,
    /// `max_reaction_count`
    pub max_reaction_count: i32,
    /// `min_tot_reaction_count`
    pub min_tot_reaction_count: i32,
    /// `max_tot_reaction_count`
    pub max_tot_reaction_count: i32,
    /// `divide_only`
    pub divide_only: i32,
    /// `parasite_only` (parsed but not tested by `TestContextRequisites`)
    pub parasite_only: i32,
}

impl Default for ContextReactionRequisite {
    /// `cContextReactionRequisite()`.
    fn default() -> Self {
        ContextReactionRequisite {
            prior_reactions: Vec::new(),
            prior_noreactions: Vec::new(),
            min_task_count: 0,
            max_task_count: i32::MAX,
            min_reaction_count: 0,
            max_reaction_count: i32::MAX,
            min_tot_reaction_count: 0,
            max_tot_reaction_count: i32::MAX,
            divide_only: 0,
            parasite_only: 0,
        }
    }
}

/// `cReaction`: a named reward triggered by a task.
#[derive(Debug, Clone, PartialEq)]
pub struct Reaction {
    name: String,
    id: i32,
    /// `task`: the triggering task's id in the task library (`None` while
    /// only pre-declared).
    pub task: Option<i32>,
    /// `process_list`
    pub processes: Vec<ReactionProcess>,
    /// `requisite_list`
    pub requisites: Vec<ReactionRequisite>,
    /// `context_requisite_list`
    pub context_requisites: Vec<ContextReactionRequisite>,
    /// `active`
    pub active: bool,
    /// `internal`
    pub internal: bool,
}

impl Reaction {
    /// `cReaction(name, id)`.
    pub fn new(name: &str, id: i32) -> Self {
        Reaction {
            name: name.to_string(),
            id,
            task: None,
            processes: Vec::new(),
            requisites: Vec::new(),
            context_requisites: Vec::new(),
            active: true,
            internal: false,
        }
    }

    /// `GetName()`.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// `GetID()`.
    pub fn id(&self) -> i32 {
        self.id
    }

    /// `AddProcess()`: index of the new default process.
    pub fn add_process(&mut self) -> usize {
        self.processes.push(ReactionProcess::default());
        self.processes.len() - 1
    }
    /// `AddRequisite()`.
    pub fn add_requisite(&mut self) -> usize {
        self.requisites.push(ReactionRequisite::default());
        self.requisites.len() - 1
    }
    /// `AddContextRequisite()`.
    pub fn add_context_requisite(&mut self) -> usize {
        self.context_requisites
            .push(ContextReactionRequisite::default());
        self.context_requisites.len() - 1
    }

    /// `GetProcess(process)`.
    pub fn process(&self, process_num: i32) -> Option<&ReactionProcess> {
        if process_num < 0 {
            return None;
        }
        self.processes.get(process_num as usize)
    }

    fn process_mut(&mut self, n: i32) -> Option<&mut ReactionProcess> {
        if n < 0 {
            return None;
        }
        self.processes.get_mut(n as usize)
    }

    fn requisite_mut(&mut self, n: i32) -> Option<&mut ReactionRequisite> {
        if n < 0 {
            return None;
        }
        self.requisites.get_mut(n as usize)
    }

    /// `ModifyValue(new_value, process_num)`.
    pub fn modify_value(&mut self, new_value: f64, process_num: i32) -> bool {
        match self.process_mut(process_num) {
            Some(p) => {
                p.value = new_value;
                true
            }
            None => false,
        }
    }

    /// `MultiplyValue(value_mult, process_num)`.
    pub fn multiply_value(&mut self, value_mult: f64, process_num: i32) -> bool {
        match self.process_mut(process_num) {
            Some(p) => {
                p.value *= value_mult;
                true
            }
            None => false,
        }
    }

    /// `ModifyInst(inst, process_num)`.
    pub fn modify_inst(&mut self, inst: &str, process_num: i32) -> bool {
        match self.process_mut(process_num) {
            Some(p) => {
                p.inst = inst.to_string();
                true
            }
            None => false,
        }
    }

    /// `SetMinTaskCount(min_count, requisite_num)`.
    pub fn set_min_task_count(&mut self, v: i32, requisite_num: i32) -> bool {
        match self.requisite_mut(requisite_num) {
            Some(r) => {
                r.min_task_count = v;
                true
            }
            None => false,
        }
    }

    /// `SetMaxTaskCount(max_count, requisite_num)`.
    pub fn set_max_task_count(&mut self, v: i32, requisite_num: i32) -> bool {
        match self.requisite_mut(requisite_num) {
            Some(r) => {
                r.max_task_count = v;
                true
            }
            None => false,
        }
    }

    /// `SetMinReactionCount(reaction_min_count, requisite_num)`.
    pub fn set_min_reaction_count(&mut self, v: i32, requisite_num: i32) -> bool {
        match self.requisite_mut(requisite_num) {
            Some(r) => {
                r.min_reaction_count = v;
                true
            }
            None => false,
        }
    }

    /// `SetMaxReactionCount(reaction_max_count, requisite_num)`.
    pub fn set_max_reaction_count(&mut self, v: i32, requisite_num: i32) -> bool {
        match self.requisite_mut(requisite_num) {
            Some(r) => {
                r.max_reaction_count = v;
                true
            }
            None => false,
        }
    }

    /// `GetValue(process_num)`. QUIRK: the C++ returns `false` (0.0) for a
    /// bad process number.
    pub fn value(&self, process_num: i32) -> f64 {
        self.process(process_num).map(|p| p.value).unwrap_or(0.0)
    }
}

/// `cReactionLib`: reactions in definition order (id = position).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReactionLib {
    reactions: Vec<Reaction>,
}

impl ReactionLib {
    /// `cReactionLib()`.
    pub fn new() -> Self {
        Self::default()
    }
    /// `GetSize()`.
    pub fn size(&self) -> i32 {
        self.reactions.len() as i32
    }
    /// All reactions, indexed by id.
    pub fn reactions(&self) -> &[Reaction] {
        &self.reactions
    }
    /// `AddReaction(name)`: the existing reaction's id if the name is known,
    /// otherwise the id of a new reaction.
    pub fn add_reaction(&mut self, name: &str) -> usize {
        if let Some(i) = self.find(name) {
            return i;
        }
        let id = self.reactions.len();
        self.reactions.push(Reaction::new(name, id as i32));
        id
    }
    /// `GetReaction(const cString&)`: id of the first reaction named `name`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.reactions.iter().position(|r| r.name() == name)
    }
    /// `GetReaction(int id)`.
    pub fn reaction(&self, id: usize) -> &Reaction {
        &self.reactions[id]
    }
    /// `GetReaction(int id)` (mutable).
    pub fn reaction_mut(&mut self, id: usize) -> &mut Reaction {
        &mut self.reactions[id]
    }
    /// `GetReaction(name)` (mutable).
    pub fn reaction_by_name_mut(&mut self, name: &str) -> Option<&mut Reaction> {
        let i = self.find(name)?;
        Some(&mut self.reactions[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let p = ReactionProcess::default();
        assert_eq!(p.value, 1.0);
        assert_eq!(p.proc_type, proc_type::ADD);
        assert_eq!(p.max_number, 1.0);
        assert_eq!(p.max_fraction, 1.0);
        assert_eq!(p.conversion, 1.0);
        assert!(p.depletable);
        let r = ReactionRequisite::default();
        assert_eq!(r.max_task_count, i32::MAX);
        assert_eq!(r.cell_box, CellBox::default());
    }

    #[test]
    fn cell_box() {
        assert!(CellBox::default().in_cell_box(1234, 10, 10));
        let b = CellBox::new(2, 1, 3, 2);
        // cell 12 = (2,1), 24 = (4,2), 15 = (5,1), 31 = (1,3)
        assert!(b.in_cell_box(12, 10, 10));
        assert!(b.in_cell_box(24, 10, 10));
        assert!(!b.in_cell_box(15, 10, 10));
        assert!(!b.in_cell_box(31, 10, 10));
    }

    #[test]
    fn lib_and_modifiers() {
        let mut lib = ReactionLib::new();
        assert_eq!(lib.add_reaction("A"), 0);
        assert_eq!(lib.add_reaction("B"), 1);
        assert_eq!(lib.add_reaction("A"), 0);
        assert_eq!(lib.size(), 2);
        let r = lib.reaction_mut(1);
        assert_eq!(r.value(0), 0.0);
        assert!(!r.modify_value(3.0, 0));
        r.add_process();
        assert!(r.modify_value(3.0, 0));
        assert!(r.multiply_value(2.0, 0));
        assert_eq!(r.value(0), 6.0);
        assert!(!r.modify_value(3.0, -1));
        assert!(r.modify_inst("nop-A", 0));
        assert!(!r.set_max_task_count(1, 0));
        r.add_requisite();
        assert!(r.set_max_task_count(1, 0));
        assert_eq!(r.requisites[0].max_task_count, 1);
        assert_eq!(lib.find("B"), Some(1));
        assert_eq!(lib.find("C"), None);
    }
}
