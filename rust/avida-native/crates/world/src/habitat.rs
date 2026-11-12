//! The population side of the gradient habitats: `cPopulation::
//! ExecutePredatoryResource`, `ExecuteDamagingResource`,
//! `ExecuteDeadlyResource` and `InjureOrg` (avida-core/source/main/
//! cPopulation.cc at 47f13dadb), which `cGradientCount` calls while it
//! updates a predatory (`SetPredatoryResource`), damaging (`damage=`) or
//! deadly (`deadly=`) gradient resource.
//!
//! The C++ runs them in the middle of `cResourceCount::DoUpdates`, which in
//! this port borrows the resource count mutably. [`HabitatEnv`] therefore
//! decides each call from a read-only view of the cells (drawing from the
//! RNG exactly where the C++ does) and queues what it does to organisms;
//! [`World::with_pop_resources`] applies the queue as soon as the resource
//! call returns, before anything else looks at the population. Kills and
//! injuries draw no random numbers, so only their timing moves, and nothing
//! between the C++ call and the return reads the organisms.
//!
//! Stops, where the C++ cannot be followed:
//!
//! - an environment with a `path=1` resource (unless the habitat resource is
//!   a `hammer`) or, for predatory resources, a den (habitat 3 or 4): the
//!   C++ reads that resource with `GetCellResVal`, which re-enters
//!   `DoUpdates` before `m_last_updated` is advanced, updates the same
//!   gradient again and recurses until the stack overflows (verified with
//!   the reference binary);
//! - `USE_AVATARS` with an avatar in the cell: the avatar branches are not
//!   ported (without one the C++ does nothing);
//! - a damaging resource injuring the running organism from a resource read
//!   whose caller does not hand the world the organism's state (the injury
//!   changes merit, bonus and resource bins that the same instruction may
//!   go on to use). The `OrgInterface` resource reads and writes take the
//!   running organism's state, as do `MoveOrganisms` and
//!   `PushToleranceInstExe`, and injure it in place, as the C++ does; what
//!   remains are world-side reads during a step without it (the group count
//!   of `JoinGroup` and friends, HGT fragment metabolism). A deadly
//!   or predatory hit on the running organism only sets `to_die`, which
//!   nothing reads before the end of its `SingleProcess`, so it is applied
//!   when the step returns (`World::apply_running_to_die`), except with
//!   implicit reproduction on or during a speculative instruction, which
//!   stop.
//! - a damaging resource driving an organism's merit to 0: the C++ kills it
//!   in the middle of the resource update and goes on using it.
//! - a kill of an organism that is not running with `ENABLE_HGT` and
//!   `HGT_COMPETENCE_P > 0`: `KillOrganism` deposits its genome
//!   (`AddGenomeFragments`), whose `AdjustHGTResource` calls
//!   `cResourceCount::Modify`, which re-enters `DoUpdates` in the middle of
//!   the gradient update. (`ENABLE_HGT` always has an `hgt=1` resource: the
//!   C++ crashes at start-up without one.) The reference crashes there too
//!   (SIGSEGV in `cPopulation::KillOrganism`, `hab4_deadly` environment
//!   plus an `hgt=1` resource, seed 11, update 8).

use avida_apto::AvidaRng;
use avida_environment::resource::Resource;
use avida_hardware::organism::{OrgState, Organism};
use avida_resources::{ResourceCount, ResourceEnv};

use crate::population::Running;
use crate::world::{World, WorldResEnv};

/// What a habitat call does to the population, applied after the resource
/// call returns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HabitatEffect {
    /// `KillOrganism(GetCell(cell))` of an organism that is not running.
    Kill(i32),
    /// `InjureOrg(ctx, GetCell(cell), damage, false)` of an organism that is
    /// not running.
    Injure(i32, f64),
    /// `InjureOrg` of the running organism.
    InjureRunning(f64),
}

/// The habitat bookkeeping kept in [`WorldResEnv`].
#[derive(Debug, Default)]
pub struct HabitatState {
    /// Effects waiting for [`World::apply_habitat_effects`].
    pub effects: Vec<HabitatEffect>,
    /// Cells emptied by a queued kill (the C++ cell is empty at once).
    pub killed: Vec<i32>,
    /// `target_org->GetPhenotype().SetToDie()` of the running organism.
    pub running_to_die: bool,
    /// Organisms killed so far (for loops over a copy of the live list,
    /// which the C++ goes on reading after a kill).
    pub kills: u64,
}

/// Who lives in a cell, as `cPopulationCell::IsOccupied` / `GetOrganism`
/// would answer during the resource update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Occupant {
    Empty,
    /// An organism that is not running.
    Resting,
    /// The running organism (`IsRunning()`).
    Running,
}

/// A [`ResourceEnv`] with a read-only view of the cells; see the module
/// docs.
pub struct HabitatEnv<'a> {
    pub base: &'a mut WorldResEnv,
    pub orgs: &'a [Option<Box<Organism>>],
    pub running: Option<&'a Running>,
    /// The environment's resources (`m_world->GetEnvironment().
    /// GetResourceLib()`).
    pub resources: &'a [Resource],
    /// The avatars, with `USE_AVATARS`.
    pub avatars: Option<&'a crate::avatars::Avatars>,
    /// A kill of an organism that is not running stops the run (HGT
    /// natural competence; see the module docs).
    pub hgt_kill_stops: bool,
}

impl HabitatEnv<'_> {
    fn occupant(&self, cell_id: i32) -> Occupant {
        if self.base.habitat.killed.contains(&cell_id) {
            return Occupant::Empty;
        }
        let Some(slot) = usize::try_from(cell_id).ok().and_then(|c| self.orgs.get(c)) else {
            return Occupant::Empty;
        };
        if slot.is_some() {
            return Occupant::Resting;
        }
        match self.running {
            Some(r) if r.cell_id == cell_id && !r.removed => Occupant::Running,
            _ => Occupant::Empty,
        }
    }

    /// The `if (!hammer)` scan of the path resources, and the checks this
    /// port cannot follow. `false` when the call stops the run.
    fn supported(&mut self, what: &str, cell_id: i32, hammer: bool, den_scan: bool) -> bool {
        if !hammer && self.resources.iter().any(|r| r.path) {
            self.base.stop(&format!(
                "{what} with a path resource (C++: cPopulation::{what} reads it with \
                 GetCellResVal, which re-enters cResourceCount::DoUpdates and recurses \
                 until the stack overflows)"
            ));
            return false;
        }
        if den_scan
            && self
                .resources
                .iter()
                .any(|r| r.habitat == 3 || r.habitat == 4)
        {
            self.base.stop(&format!(
                "{what} with a den resource (C++: cPopulation::{what} reads it with \
                 GetCellResVal, which re-enters cResourceCount::DoUpdates and recurses \
                 until the stack overflows)"
            ));
            return false;
        }
        if let Some(av) = self.avatars {
            if av.has_av(cell_id) {
                self.base.stop(&format!(
                    "{what} on a cell with avatars (C++: the USE_AVATARS branch of \
                     cPopulation::{what})"
                ));
            }
            // Without avatars in the cell the C++ does nothing.
            return false;
        }
        true
    }

    /// `if (!target_org->IsRunning()) KillOrganism(...); else
    /// target_org->GetPhenotype().SetToDie();`
    fn kill(&mut self, cell_id: i32, who: Occupant) {
        if who == Occupant::Resting && self.hgt_kill_stops {
            self.base.stop(
                "a gradient habitat killed an organism with ENABLE_HGT and HGT_COMPETENCE_P > 0 \
                 (C++: KillOrganism -> AddGenomeFragments -> AdjustHGTResource re-enters \
                 cResourceCount::DoUpdates in the middle of the gradient update and crashes)",
            );
            return;
        }
        let h = &mut self.base.habitat;
        match who {
            Occupant::Resting => {
                h.effects.push(HabitatEffect::Kill(cell_id));
                h.killed.push(cell_id);
            }
            Occupant::Running => h.running_to_die = true,
            Occupant::Empty => {}
        }
    }
}

impl ResourceEnv for HabitatEnv<'_> {
    fn update(&self) -> i32 {
        self.base.update
    }
    fn has_driver(&self) -> bool {
        self.base.has_driver
    }
    fn feedback_error(&mut self, msg: &str) {
        self.base.errors.push(msg.to_string());
    }

    /// `cPopulation::ExecutePredatoryResource`. Without dens (which stop),
    /// `cell_has_den` is false, so any occupant is a target.
    fn execute_predatory_resource(
        &mut self,
        rng: &mut AvidaRng,
        cell_id: i32,
        pred_odds: f64,
        _juvs_per: i32,
        hammer: bool,
    ) {
        if !self.supported("ExecutePredatoryResource", cell_id, hammer, true) {
            return;
        }
        let who = self.occupant(cell_id);
        if who != Occupant::Empty {
            // `GetTimeUsed() < juv_age || !cell_has_den`, then
            // `ctx.GetRandom().P(pred_odds) && !target_org->IsDead()`: an
            // organism still in its cell is not dead.
            if rng.p(pred_odds) {
                self.kill(cell_id, who);
            }
        }
    }

    /// `cPopulation::ExecuteDamagingResource`
    fn execute_damaging_resource(
        &mut self,
        _rng: &mut AvidaRng,
        cell_id: i32,
        damage: f64,
        hammer: bool,
    ) {
        if !self.supported("ExecuteDamagingResource", cell_id, hammer, false) {
            return;
        }
        match self.occupant(cell_id) {
            Occupant::Empty => {}
            Occupant::Resting => {
                // `InjureOrg(ctx, GetCell(cell_id), damage, false)`
                if damage != 0.0 {
                    self.base
                        .habitat
                        .effects
                        .push(HabitatEffect::Injure(cell_id, damage));
                }
            }
            Occupant::Running => {
                if damage != 0.0 {
                    self.base
                        .habitat
                        .effects
                        .push(HabitatEffect::InjureRunning(damage));
                }
            }
        }
    }

    /// `cPopulation::ExecuteDeadlyResource`
    fn execute_deadly_resource(
        &mut self,
        rng: &mut AvidaRng,
        cell_id: i32,
        odds: f64,
        hammer: bool,
    ) {
        if !self.supported("ExecuteDeadlyResource", cell_id, hammer, false) {
            return;
        }
        let who = self.occupant(cell_id);
        if who != Occupant::Empty && rng.p(odds) {
            self.kill(cell_id, who);
        }
    }
}

impl World {
    /// Runs `f` on the population's resource count with a [`HabitatEnv`],
    /// then applies the habitat effects it queued.
    pub(crate) fn with_pop_resources<R>(
        &mut self,
        f: impl FnOnce(&mut ResourceCount, &mut AvidaRng, &mut dyn ResourceEnv) -> R,
    ) -> R {
        self.with_pop_resources_for(None, f)
    }

    /// [`World::with_pop_resources`] from code that holds the running
    /// organism's state, which a damaging gradient may injure.
    pub(crate) fn with_pop_resources_for<R>(
        &mut self,
        running: Option<&mut OrgState>,
        f: impl FnOnce(&mut ResourceCount, &mut AvidaRng, &mut dyn ResourceEnv) -> R,
    ) -> R {
        let r = {
            let World {
                pop,
                rng,
                res_env,
                env,
                avatars,
                cfg,
                ..
            } = self;
            let mut henv = HabitatEnv {
                base: res_env,
                orgs: &pop.orgs,
                running: pop.running.as_ref(),
                resources: env.resource_lib().resources(),
                avatars: (cfg.use_avatars != 0).then_some(&*avatars),
                hgt_kill_stops: cfg.enable_hgt != 0 && cfg.hgt_competence_p > 0.0,
            };
            f(&mut pop.resource_count, rng, &mut henv)
        };
        self.apply_habitat_effects(running);
        r
    }

    /// Applies the kills and injuries queued by a [`HabitatEnv`], in the
    /// order the C++ made them.
    fn apply_habitat_effects(&mut self, mut running: Option<&mut OrgState>) {
        if self.res_env.habitat.effects.is_empty() {
            self.res_env.habitat.killed.clear();
            return;
        }
        let effects = std::mem::take(&mut self.res_env.habitat.effects);
        self.res_env.habitat.killed.clear();
        for e in effects {
            match e {
                HabitatEffect::Kill(cell_id) => {
                    self.res_env.habitat.kills += 1;
                    self.kill_organism(cell_id, None);
                }
                HabitatEffect::Injure(cell_id, injury) => self.injure_resting(cell_id, injury),
                HabitatEffect::InjureRunning(injury) => {
                    self.injure_running_from_habitat(running.as_deref_mut(), injury);
                }
            }
        }
    }

    /// `InjureOrg(ctx, cell, injury, false)` of the running organism.
    /// `state` is what the caller holds: the running organism's state for a
    /// read made through its `OrgInterface` or from `MoveOrganisms`.
    fn injure_running_from_habitat(&mut self, state: Option<&mut OrgState>, injury: f64) {
        let Some((cell_id, uid)) = self
            .pop
            .running
            .as_ref()
            .filter(|r| !r.removed)
            .map(|r| (r.cell_id, r.uid))
        else {
            return;
        };
        let Some(state) = state.filter(|s| s.uid == uid) else {
            self.set_fatal(
                "unsupported: a damaging gradient injures the running organism during a \
                 resource read made without its state at hand (C++: cPopulation::InjureOrg \
                 in the middle of the instruction)",
            );
            return;
        };
        self.injure_running(cell_id, state, injury, false);
        if self.pop.running.as_ref().is_some_and(|r| r.removed) {
            self.set_fatal(
                "unsupported: a damaging gradient drove the running organism's merit to 0 \
                 (C++: KillOrganism in the middle of the resource update, which goes on \
                 with the cell emptied)",
            );
        }
    }

    /// `cPopulation::InjureOrg(ctx, cell, injury, false)` of the organism
    /// resting in `cell_id`.
    fn injure_resting(&mut self, cell_id: i32, injury: f64) {
        let Some(c) = usize::try_from(cell_id).ok() else {
            return;
        };
        if self.pop.orgs.get(c).and_then(Option::as_ref).is_none() {
            return;
        }
        if self.cfg.merit_inc_apply_immediate {
            let merit = self.pop.orgs[c]
                .as_ref()
                .map_or(0.0, |o| o.state.phenotype.merit.get_double());
            // FMA (fmsub in the reference): `target_merit -= target_merit * injury`
            let target_merit = (-merit).mul_add(injury, merit);
            self.update_merit_of(cell_id, None, target_merit);
        }
        // `UpdateMerit` kills an organism whose merit drops to 0; the C++
        // then goes on with the deleted organism.
        let use_bins = self.cfg.use_resource_bins;
        if let Some(o) = self.pop.orgs[c].as_mut() {
            crate::movement::injure_state(&mut o.state, injury, false, use_bins);
        } else {
            self.set_fatal(
                "unsupported: a damaging gradient drove an organism's merit to 0 (C++: \
                 cPopulation::InjureOrg goes on with the organism UpdateMerit deleted)",
            );
        }
    }

    /// A deadly or predatory gradient hit the running organism during its
    /// step (`SetToDie`); the C++ kills it at the end of `SingleProcess`.
    /// True when the organism died here.
    pub(crate) fn apply_running_to_die(&mut self, org: &mut OrgState) -> bool {
        if !std::mem::take(&mut self.res_env.habitat.running_to_die) {
            return false;
        }
        // The organism may have moved during its step.
        let Some(cell_id) = self
            .pop
            .running
            .as_ref()
            .filter(|r| !r.removed)
            .map(|r| r.cell_id)
        else {
            return false;
        };
        let c = &self.cfg;
        if c.implicit_repro_time != 0
            || c.implicit_repro_cpu_cycles != 0
            || c.implicit_repro_bonus != 0
            || c.implicit_repro_end != 0
            || c.implicit_repro_energy != 0.0
        {
            // The C++ dies before `CheckImplicitRepro`, which then skips the
            // dead organism; the step has already run it here.
            self.set_fatal(
                "unsupported: a deadly or predatory gradient killed the running organism with implicit \
                 reproduction on (C++: Die before CheckImplicitRepro in SingleProcess)",
            );
            return false;
        }
        org.phenotype.to_die = true;
        let mut iface = crate::pop_interface::PopulationInterface { w: self, cell_id };
        org.die(&mut iface);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resources(path: bool, den: bool) -> Vec<Resource> {
        let mut food = Resource::new("food", 0);
        food.path = path;
        let mut bad = Resource::new("bad", 1);
        if den {
            bad.habitat = 4;
        }
        vec![food, bad]
    }

    fn running(cell_id: i32) -> Running {
        Running {
            cell_id,
            ..Running::default()
        }
    }

    /// Runs `f` on a [`HabitatEnv`] over four empty cells with the running
    /// organism in cell 2; returns the state and how many numbers were drawn.
    fn with_env(
        res: &[Resource],
        f: impl FnOnce(&mut HabitatEnv, &mut AvidaRng),
    ) -> (WorldResEnv, usize) {
        let mut base = WorldResEnv::default();
        let orgs: Vec<Option<Box<Organism>>> = (0..4).map(|_| None).collect();
        let r = running(2);
        let mut rng = AvidaRng::new(17);
        {
            let mut env = HabitatEnv {
                base: &mut base,
                orgs: &orgs,
                running: Some(&r),
                resources: res,
                avatars: None,
                hgt_kill_stops: false,
            };
            f(&mut env, &mut rng);
        }
        let mut fresh = AvidaRng::new(17);
        let mut draws = 0;
        while fresh.get_double() != rng.clone().get_double() {
            draws += 1;
            assert!(draws < 10, "drew too many numbers");
        }
        (base, draws)
    }

    #[test]
    fn deadly_draws_only_for_an_occupied_cell_and_marks_the_running_organism() {
        let res = resources(false, false);
        let (base, draws) = with_env(&res, |env, rng| {
            env.execute_deadly_resource(rng, 1, 1.0, false);
        });
        assert_eq!(draws, 0);
        assert!(!base.habitat.running_to_die);

        let (base, draws) = with_env(&res, |env, rng| {
            env.execute_deadly_resource(rng, 2, 1.0, false);
        });
        assert_eq!(draws, 1);
        assert!(base.habitat.running_to_die);
        assert!(base.habitat.effects.is_empty());
        assert!(base.unsupported.is_none());
    }

    #[test]
    fn predatory_hits_the_running_organism_without_dens() {
        let res = resources(false, false);
        let (base, draws) = with_env(&res, |env, rng| {
            env.execute_predatory_resource(rng, 2, 1.0, 0, false);
        });
        assert_eq!(draws, 1);
        assert!(base.habitat.running_to_die);
    }

    #[test]
    fn damaging_queues_the_running_injury() {
        let res = resources(false, false);
        let (base, draws) = with_env(&res, |env, rng| {
            env.execute_damaging_resource(rng, 2, 0.25, false);
            env.execute_damaging_resource(rng, 3, 0.25, false);
        });
        assert_eq!(draws, 0);
        assert_eq!(
            base.habitat.effects,
            vec![HabitatEffect::InjureRunning(0.25)]
        );
    }

    #[test]
    fn paths_and_dens_stop_unless_hammered() {
        let res = resources(true, false);
        let (base, draws) = with_env(&res, |env, rng| {
            env.execute_deadly_resource(rng, 2, 1.0, false);
        });
        assert_eq!(draws, 0);
        assert!(base
            .unsupported
            .as_deref()
            .is_some_and(|m| m.contains("path resource")));

        let (base, draws) = with_env(&res, |env, rng| {
            env.execute_deadly_resource(rng, 2, 1.0, true);
        });
        assert_eq!(draws, 1);
        assert!(base.unsupported.is_none());

        let res = resources(false, true);
        let (base, _) = with_env(&res, |env, rng| {
            env.execute_predatory_resource(rng, 2, 1.0, 0, true);
        });
        assert!(base
            .unsupported
            .as_deref()
            .is_some_and(|m| m.contains("den resource")));
        // Deadly and damaging resources do not look for dens.
        let (base, draws) = with_env(&res, |env, rng| {
            env.execute_deadly_resource(rng, 2, 1.0, false);
        });
        assert_eq!(draws, 1);
        assert!(base.unsupported.is_none());
    }

    #[test]
    fn avatars_without_one_in_the_cell_do_nothing() {
        let res = resources(false, false);
        let mut base = WorldResEnv::default();
        let orgs: Vec<Option<Box<Organism>>> = (0..4).map(|_| None).collect();
        let r = running(2);
        let avatars = crate::avatars::Avatars::default();
        let mut rng = AvidaRng::new(17);
        let before = rng.clone().get_double();
        let mut env = HabitatEnv {
            base: &mut base,
            orgs: &orgs,
            running: Some(&r),
            resources: &res,
            avatars: Some(&avatars),
            hgt_kill_stops: false,
        };
        env.execute_deadly_resource(&mut rng, 2, 1.0, false);
        env.execute_damaging_resource(&mut rng, 2, 1.0, false);
        assert_eq!(rng.get_double(), before);
        assert!(!base.habitat.running_to_die);
        assert!(base.habitat.effects.is_empty());
        assert!(base.unsupported.is_none());
    }
}
