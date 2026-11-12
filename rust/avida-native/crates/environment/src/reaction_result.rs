//! Port of `cReactionResult` (avida-core/source/main/cReactionResult.{h,cc}).
//!
//! The result is created inactive. The first call to any recording method
//! except `add_inst` "activates" it, resetting every per-resource, per-task
//! and per-reaction array and every accumulator (`ActivateReaction`). While
//! inactive, the array getters return their neutral values.

/// `cReactionResult`: everything one `TestOutput` call produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactionResult {
    resources_consumed: Vec<f64>,
    resources_produced: Vec<f64>,
    resources_detected: Vec<f64>,
    internal_resources_consumed: Vec<f64>,
    internal_resources_produced: Vec<f64>,
    tasks_done: Vec<bool>,
    tasks_quality: Vec<f64>,
    tasks_value: Vec<f64>,
    reactions_triggered: Vec<bool>,
    reaction_add_bonus: Vec<f64>,
    /// `task_plasticity`. QUIRK: never sized by the C++ (always empty).
    task_plasticity: Vec<f64>,
    energy_add: f64,
    bonus_add: f64,
    bonus_mult: f64,
    germline_add: f64,
    germline_mult: f64,
    insts_triggered: Vec<String>,
    is_random_resource: bool,
    lethal: bool,
    sterilize: bool,
    active_reaction: bool,
    used_env_resource: bool,
    deme_add_bonus: f64,
    deme_mult_bonus: f64,
    active_deme_reaction: bool,
}

impl ReactionResult {
    /// `cReactionResult(num_resources, num_tasks, num_reactions)`.
    ///
    /// QUIRK: the C++ leaves the scalar accumulators uninitialised until
    /// the first activation; here they start at their activation values.
    pub fn new(num_resources: usize, num_tasks: usize, num_reactions: usize) -> Self {
        ReactionResult {
            resources_consumed: vec![0.0; num_resources],
            resources_produced: vec![0.0; num_resources],
            resources_detected: vec![0.0; num_resources],
            internal_resources_consumed: vec![0.0; num_resources],
            internal_resources_produced: vec![0.0; num_resources],
            tasks_done: vec![false; num_tasks],
            tasks_quality: vec![0.0; num_tasks],
            tasks_value: vec![0.0; num_tasks],
            reactions_triggered: vec![false; num_reactions],
            reaction_add_bonus: vec![0.0; num_reactions],
            task_plasticity: Vec::new(),
            energy_add: 0.0,
            bonus_add: 0.0,
            bonus_mult: 1.0,
            germline_add: 0.0,
            germline_mult: 1.0,
            insts_triggered: Vec::new(),
            is_random_resource: false,
            lethal: false,
            sterilize: false,
            active_reaction: false,
            used_env_resource: true,
            deme_add_bonus: 0.0,
            deme_mult_bonus: 1.0,
            active_deme_reaction: false,
        }
    }

    /// `ActivateReaction()`: on first use, reset everything.
    fn activate_reaction(&mut self) {
        if self.active_reaction {
            return;
        }
        self.resources_consumed.fill(0.0);
        self.resources_produced.fill(0.0);
        self.resources_detected.fill(-1.0);
        self.internal_resources_consumed.fill(0.0);
        self.internal_resources_produced.fill(0.0);
        self.tasks_done.fill(false);
        self.tasks_quality.fill(0.0);
        self.tasks_value.fill(0.0);
        self.reactions_triggered.fill(false);
        self.reaction_add_bonus.fill(0.0);
        self.task_plasticity.fill(0.0);
        self.energy_add = 0.0;
        self.bonus_add = 0.0;
        self.bonus_mult = 1.0;
        self.germline_add = 0.0;
        self.germline_mult = 1.0;
        self.insts_triggered.clear();
        self.is_random_resource = false;
        self.lethal = false;
        self.sterilize = false;
        self.used_env_resource = true;
        self.deme_add_bonus = 0.0;
        self.deme_mult_bonus = 1.0;
        self.active_deme_reaction = false;
        self.active_reaction = true;
    }

    /// `GetActive()`.
    pub fn active(&self) -> bool {
        self.active_reaction
    }
    /// `GetActiveDeme()`.
    pub fn active_deme(&self) -> bool {
        self.active_deme_reaction
    }
    /// `Invalidate()`: the next recording call re-initialises everything.
    pub fn invalidate(&mut self) {
        self.active_reaction = false;
    }

    /// `Consume(id, num, is_env_resource)`.
    pub fn consume(&mut self, id: usize, num: f64, is_env_resource: bool) {
        self.activate_reaction();
        if is_env_resource {
            self.resources_consumed[id] += num;
        } else {
            self.used_env_resource = false;
            self.internal_resources_consumed[id] += num;
        }
    }

    /// `Produce(id, num, is_env_resource)`.
    pub fn produce(&mut self, id: usize, num: f64, is_env_resource: bool) {
        self.activate_reaction();
        if is_env_resource {
            self.resources_produced[id] += num;
        } else {
            self.used_env_resource = false;
            self.internal_resources_produced[id] += num;
        }
    }

    /// `Detect(id, num)`: adds to the (initially -1) detected amount.
    pub fn detect(&mut self, id: usize, num: f64) {
        self.activate_reaction();
        self.resources_detected[id] += num;
    }

    /// `SetRandomResource(flag)`.
    pub fn set_random_resource(&mut self, flag: bool) {
        self.activate_reaction();
        self.is_random_resource = flag;
    }

    /// `Lethal(flag)`.
    pub fn set_lethal(&mut self, flag: bool) {
        self.activate_reaction();
        self.lethal = flag;
    }

    /// `Sterilize(flag)`.
    pub fn set_sterilize(&mut self, flag: bool) {
        self.activate_reaction();
        self.sterilize = flag;
    }

    /// `MarkTask(id, quality, value)`.
    pub fn mark_task(&mut self, id: usize, quality: f64, value: f64) {
        self.activate_reaction();
        self.tasks_done[id] = true;
        self.tasks_quality[id] = quality;
        self.tasks_value[id] = value;
    }

    /// `MarkReaction(id)`.
    pub fn mark_reaction(&mut self, id: usize) {
        self.activate_reaction();
        self.reactions_triggered[id] = true;
    }

    /// `AddEnergy(value)`.
    pub fn add_energy(&mut self, value: f64) {
        self.activate_reaction();
        self.energy_add += value;
    }

    /// `AddBonus(value, id)`.
    pub fn add_bonus(&mut self, value: f64, id: usize) {
        self.activate_reaction();
        self.bonus_add += value;
        self.reaction_add_bonus[id] += value;
    }

    /// `MultBonus(value)`.
    pub fn mult_bonus(&mut self, value: f64) {
        self.activate_reaction();
        self.bonus_mult *= value;
    }

    /// `AddDemeBonus(value)`.
    pub fn add_deme_bonus(&mut self, value: f64) {
        self.activate_reaction();
        self.active_deme_reaction = true;
        self.deme_add_bonus += value;
    }

    /// `MultDemeBonus(value)`.
    pub fn mult_deme_bonus(&mut self, value: f64) {
        self.activate_reaction();
        self.active_deme_reaction = true;
        self.deme_mult_bonus *= value;
    }

    /// `AddGermline(value)`.
    pub fn add_germline(&mut self, value: f64) {
        self.activate_reaction();
        self.germline_add += value;
    }

    /// `MultGermline(value)`.
    pub fn mult_germline(&mut self, value: f64) {
        self.activate_reaction();
        self.germline_mult *= value;
    }

    /// `AddInst(inst)`. QUIRK: does not activate the result, so an
    /// instruction added to an inactive result is discarded by the next
    /// activation.
    pub fn add_inst(&mut self, inst: &str) {
        self.insts_triggered.push(inst.to_string());
    }

    /// `GetConsumed(id)`: 0 while inactive.
    pub fn consumed(&self, id: usize) -> f64 {
        if !self.active() {
            return 0.0;
        }
        self.resources_consumed[id]
    }
    /// `GetProduced(id)`.
    pub fn produced(&self, id: usize) -> f64 {
        if !self.active() {
            return 0.0;
        }
        self.resources_produced[id]
    }
    /// `GetDetected(id)`: 0 while inactive, -1 if never detected.
    pub fn detected(&self, id: usize) -> f64 {
        if !self.active() {
            return 0.0;
        }
        self.resources_detected[id]
    }
    /// `GetInternalConsumed(id)`.
    pub fn internal_consumed(&self, id: usize) -> f64 {
        if !self.active() {
            return 0.0;
        }
        self.internal_resources_consumed[id]
    }
    /// `GetInternalProduced(id)`.
    pub fn internal_produced(&self, id: usize) -> f64 {
        if !self.active() {
            return 0.0;
        }
        self.internal_resources_produced[id]
    }
    /// `GetIsRandomResource()`.
    pub fn is_random_resource(&self) -> bool {
        self.active() && self.is_random_resource
    }
    /// `GetLethal()`.
    pub fn lethal(&self) -> bool {
        self.active() && self.lethal
    }
    /// `GetSterilize()`.
    pub fn sterilize(&self) -> bool {
        self.active() && self.sterilize
    }
    /// `ReactionTriggered(id)`.
    pub fn reaction_triggered(&self, id: usize) -> bool {
        self.active() && self.reactions_triggered[id]
    }
    /// `TaskDone(id)`.
    pub fn task_done(&self, id: usize) -> bool {
        self.active() && self.tasks_done[id]
    }
    /// `TaskQuality(id)`.
    pub fn task_quality(&self, id: usize) -> f64 {
        if !self.active() {
            return 0.0;
        }
        self.tasks_quality[id]
    }
    /// `TaskValue(id)`.
    pub fn task_value(&self, id: usize) -> f64 {
        if !self.active() {
            return 0.0;
        }
        self.tasks_value[id]
    }
    /// `GetAddEnergy()` (not gated on activity).
    pub fn add_energy_total(&self) -> f64 {
        self.energy_add
    }
    /// `GetAddBonus()`.
    pub fn add_bonus_total(&self) -> f64 {
        self.bonus_add
    }
    /// `GetReactionAddBonus(i)`.
    pub fn reaction_add_bonus(&self, i: usize) -> f64 {
        self.reaction_add_bonus[i]
    }
    /// `GetMultBonus()`.
    pub fn mult_bonus_total(&self) -> f64 {
        self.bonus_mult
    }
    /// `GetInstArray()`.
    pub fn inst_array(&self) -> &[String] {
        &self.insts_triggered
    }
    /// `UsedEnvResource()` / `IsEnvResource()`.
    pub fn used_env_resource(&self) -> bool {
        self.used_env_resource
    }
    /// `GetAddDemeBonus()`.
    pub fn add_deme_bonus_total(&self) -> f64 {
        self.deme_add_bonus
    }
    /// `GetMultDemeBonus()`.
    pub fn mult_deme_bonus_total(&self) -> f64 {
        self.deme_mult_bonus
    }
    /// `GetAddGermline()`.
    pub fn add_germline_total(&self) -> f64 {
        self.germline_add
    }
    /// `GetMultGermline()`.
    pub fn mult_germline_total(&self) -> f64 {
        self.germline_mult
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lazy_activation() {
        let mut r = ReactionResult::new(2, 3, 4);
        assert!(!r.active());
        assert_eq!(r.consumed(0), 0.0);
        assert_eq!(r.detected(0), 0.0);
        // add_inst does not activate and is wiped by the next activation.
        r.add_inst("nop-A");
        assert_eq!(r.inst_array().len(), 1);
        r.mark_reaction(2);
        assert!(r.active());
        assert!(r.inst_array().is_empty());
        assert_eq!(r.detected(1), -1.0);
        r.detect(1, 3.0);
        assert_eq!(r.detected(1), 2.0);
        r.consume(0, 1.5, true);
        r.consume(0, 1.0, true);
        r.consume(1, 2.0, false);
        assert_eq!(r.consumed(0), 2.5);
        assert_eq!(r.internal_consumed(1), 2.0);
        assert!(!r.used_env_resource());
        r.add_bonus(2.0, 3);
        r.add_bonus(1.0, 3);
        assert_eq!(r.add_bonus_total(), 3.0);
        assert_eq!(r.reaction_add_bonus(3), 3.0);
        r.mult_bonus(4.0);
        r.mult_bonus(0.5);
        assert_eq!(r.mult_bonus_total(), 2.0);
        assert!(!r.active_deme());
        r.add_deme_bonus(0.0);
        assert!(r.active_deme());
        r.mark_task(1, 0.5, 7.0);
        assert!(r.task_done(1));
        assert_eq!(r.task_quality(1), 0.5);
        assert_eq!(r.task_value(1), 7.0);
        assert!(r.reaction_triggered(2));
        assert!(!r.reaction_triggered(1));
        // invalidate: getters go neutral, next record resets
        r.invalidate();
        assert!(!r.task_done(1));
        r.add_energy(1.0);
        assert!(!r.task_done(1));
        assert_eq!(r.add_bonus_total(), 0.0);
        assert_eq!(r.add_energy_total(), 1.0);
        assert!(r.used_env_resource());
    }
}
