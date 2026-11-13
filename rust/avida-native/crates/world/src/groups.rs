//! Group formation and tolerance of `cPopulation`
//! (avida-core/source/main/cPopulation.cc at 47f13dadb: `JoinGroup` through
//! `GetGroupIntolerances`, `KillGroupMember`, `GetFormedGroupArray`,
//! `AttemptOffspringParentGroup`), the tolerance methods of
//! `cPopulationInterface` (`IncTolerance`, `DecTolerance`,
//! `PushToleranceInstExe`) and the population-reading group printers of
//! `cStats` (`PrintGroupTolerance`, `PrintGroupMTTolerance`).
//!
//! # Members
//! `m_group_list` holds `cOrganism*`. Here a member is a key: the id of the
//! organism's cell, or [`pending_key`] for an offspring that joined a group
//! in `ActivateOffspring` before it was placed (its target cell still holds
//! the organism it is about to replace, which may be in the same group).
//! `ActivateOrganism` re-keys it to its cell once that occupant is gone
//! ([`Groups::activating`]). An organism that changes cells
//! (`cPopulation::SwapCells`, `crate::movement`) has its entries re-keyed to
//! its new cell ([`Groups::rekey`]).
//!
//! Computations that read members' tolerances (which update caches in the
//! members' phenotypes) reach organisms that are not in a cell, the one
//! being run and an offspring being placed, through [`Held`].
//!
//! Where the C++ indexes a group cache that does not exist (undefined
//! behaviour) a fatal error is recorded instead ([`Groups::fatal`]).

use std::collections::BTreeMap;

use avida_apto::DataFile;
use avida_config::Config;
use avida_hardware::nhardware::{MATING_TYPE_FEMALE, MATING_TYPE_JUVENILE, MATING_TYPE_MALE};
use avida_hardware::organism::{OrgState, Organism};

use crate::stat_tools::DoubleSum;
use crate::world::World;

/// The member key of the `i`-th offspring of the birth being placed.
pub const fn pending_key(i: usize) -> i32 {
    -2 - i as i32
}

/// Organisms outside `pop.orgs` that group code may reach, with their
/// member keys.
pub type Held<'a, 'b> = &'a mut [(i32, &'b mut OrgState)];

/// `(update, intolerance)` caches of a group: `[0]` towards immigrants,
/// `[1]` towards offspring (`AvidaArray<pair<int,int> >(2)`).
pub type IntolCache = [(i32, i32); 2];

/// `cPopulation`'s group state.
#[derive(Debug, Default)]
pub struct Groups {
    /// `m_groups`: members per group id.
    pub groups: BTreeMap<i32, i32>,
    /// `m_group_females`
    pub females: BTreeMap<i32, i32>,
    /// `m_group_males`
    pub males: BTreeMap<i32, i32>,
    /// `m_group_list`: member keys per group id, in C++ order.
    pub list: BTreeMap<i32, Vec<i32>>,
    /// `m_group_intolerances`
    pub intolerances: BTreeMap<i32, IntolCache>,
    /// `m_group_intolerances_females`
    pub intolerances_females: BTreeMap<i32, IntolCache>,
    /// `m_group_intolerances_males`
    pub intolerances_males: BTreeMap<i32, IntolCache>,
    /// `m_group_intolerances_juvs`
    pub intolerances_juvs: BTreeMap<i32, IntolCache>,
    /// The pending key of the offspring `ActivateOrganism` is placing.
    pub activating: Option<i32>,
    /// First undefined-behaviour path reached; the run stops on it.
    pub fatal: Option<String>,
}

impl Groups {
    fn ub(&mut self, what: String) {
        if self.fatal.is_none() {
            self.fatal = Some(what);
        }
    }

    /// Replaces the member `from` by `to` in every group list.
    pub fn rekey(&mut self, from: i32, to: i32) {
        for members in self.list.values_mut() {
            for m in members.iter_mut().filter(|m| **m == from) {
                *m = to;
            }
        }
    }

    /// Renames every member key through `map` (organisms moved between
    /// cells all at once).
    pub fn remap(&mut self, map: &dyn Fn(i32) -> i32) {
        for members in self.list.values_mut() {
            for m in members.iter_mut() {
                *m = map(*m);
            }
        }
    }

    /// `m_group_list[group_id]` (the non-const `Apto::Map` accessor creates
    /// an empty entry).
    fn members(&mut self, group_id: i32) -> &mut Vec<i32> {
        self.list.entry(group_id).or_default()
    }

    /// `m_group_intolerances[group_id]` and its variants by mating type
    /// (`-1` the whole group, `0` females, `1` males, `2` juveniles).
    fn cache(&mut self, group_id: i32, which: i32) -> Option<&mut IntolCache> {
        let map = match which {
            0 => &self.intolerances_females,
            1 => &self.intolerances_males,
            2 => &self.intolerances_juvs,
            _ => &self.intolerances,
        };
        if !map.contains_key(&group_id) {
            self.ub(format!(
                "group {group_id} has no tolerance cache (C++: \
                 cPopulation::m_group_intolerances out of bounds)"
            ));
            return None;
        }
        let map = match which {
            0 => &mut self.intolerances_females,
            1 => &mut self.intolerances_males,
            2 => &mut self.intolerances_juvs,
            _ => &mut self.intolerances,
        };
        map.get_mut(&group_id)
    }

    /// Adds `delta` to `cache(group_id, which)[slot].second`.
    fn add_intolerance(&mut self, group_id: i32, which: i32, slot: usize, delta: i32) {
        if let Some(c) = self.cache(group_id, which) {
            c[slot].1 += delta;
        }
    }

    /// `GetGroupIntolerances(group_id, tol_num, mating_type)`.
    ///
    /// QUIRK: the C++ binds a reference to the whole-group slot and, for a
    /// mating type, *assigns* the mating-type slot to it before returning
    /// the whole-group slot, so the caller's `++`/`--`/read acts on the
    /// whole-group cache after it was overwritten.
    pub fn group_intolerance_slot(
        &mut self,
        group_id: i32,
        tol_num: i32,
        mating_type: i32,
    ) -> Option<&mut i32> {
        let slot = usize::try_from(tol_num).ok().filter(|&s| s < 2)?;
        let value = if (0..=2).contains(&mating_type) {
            Some(self.cache(group_id, mating_type)?[slot].1)
        } else {
            None
        };
        let main = self.cache(group_id, -1)?;
        if let Some(v) = value {
            main[slot].1 = v;
        }
        Some(&mut main[slot].1)
    }
}

/// `MATING_TYPE_*` of a phenotype as the cache index used by the
/// `TOLERANCE_VARIATIONS == 2` sub-caches.
fn mating_index(mating_type: i32) -> i32 {
    match mating_type {
        MATING_TYPE_FEMALE => 0,
        MATING_TYPE_MALE => 1,
        MATING_TYPE_JUVENILE => 2,
        _ => -1,
    }
}

/// The organism with member key `key`: one of `held`, or the one in that
/// cell.
fn member_state<'s>(
    orgs: &'s mut [Option<Box<Organism>>],
    held: &'s mut [(i32, &mut OrgState)],
    key: i32,
) -> Option<&'s mut OrgState> {
    if let Some((_, org)) = held.iter_mut().find(|(k, _)| *k == key) {
        return Some(&mut **org);
    }
    usize::try_from(key)
        .ok()
        .and_then(|c| orgs.get_mut(c))
        .and_then(|o| o.as_deref_mut())
        .map(|o| &mut o.state)
}

/// Which tolerance of a member to compute.
#[derive(Clone, Copy)]
enum Tol {
    Immigrants,
    Own,
    Others,
}

fn calc_tol(org: &mut OrgState, cfg: &Config, update: i32, which: Tol) -> i32 {
    match which {
        Tol::Immigrants => org.phenotype.calc_tolerance_immigrants(cfg, update),
        Tol::Own => org.phenotype.calc_tolerance_offspring_own(cfg, update),
        Tol::Others => org.phenotype.calc_tolerance_offspring_others(cfg, update),
    }
}

impl World {
    /// `CalcToleranceX()` of the member `key` (updating its cache), or
    /// `None` (recorded as fatal) when it is not reachable.
    fn member_tolerance(&mut self, held: Held, key: i32, which: Tol) -> Option<i32> {
        let update = self.stats.update();
        match member_state(&mut self.pop.orgs, held, key) {
            Some(org) => Some(calc_tol(org, &self.cfg, update, which)),
            None => {
                self.pop.groups.ub(format!(
                    "group member {key} is not in a cell (C++: dangling cOrganism* in \
                     cPopulation::m_group_list, or several offspring per birth with tolerance)"
                ));
                None
            }
        }
    }

    fn member_mating_type(&mut self, held: Held, key: i32) -> i32 {
        member_state(&mut self.pop.orgs, held, key)
            .map_or(MATING_TYPE_JUVENILE, |o| o.phenotype.mating_type)
    }

    /// `JoinGroup(org, group_id)`
    pub(crate) fn join_group(&mut self, held: Held, key: i32, group_id: i32) {
        let tolerance_on = self.cfg.tolerance_window > 0;
        let variations = self.cfg.tolerance_variations;
        let g = &mut self.pop.groups;
        if let std::collections::btree_map::Entry::Vacant(e) = g.groups.entry(group_id) {
            // new group
            e.insert(0);
            g.list.insert(group_id, Vec::new());
            // If tolerance is on, create the new group's tolerance cache
            if tolerance_on {
                let fresh = [(-1, -1); 2];
                g.intolerances.insert(group_id, fresh);
                if variations == 2 {
                    g.intolerances_females.insert(group_id, fresh);
                    g.intolerances_males.insert(group_id, fresh);
                    g.intolerances_juvs.insert(group_id, fresh);
                }
            }
        }
        // add to group
        *g.groups.entry(group_id).or_insert(0) += 1;
        let mating_type = self.member_mating_type(held, key);
        let g = &mut self.pop.groups;
        if mating_type == MATING_TYPE_FEMALE {
            *g.females.entry(group_id).or_insert(0) += 1;
        } else if mating_type == MATING_TYPE_MALE {
            *g.males.entry(group_id).or_insert(0) += 1;
        }
        g.members(group_id).push(key);
        let count = g.groups.get(&group_id).copied().unwrap_or(0);
        self.host.group_sizes.insert(group_id, count);

        // If tolerance is on, must add the organism's intolerance to the
        // group cache
        if tolerance_on {
            let tol_max = self.cfg.max_tolerance;
            let Some(immigrant_tol) = self.member_tolerance(held, key, Tol::Immigrants) else {
                return;
            };
            let g = &mut self.pop.groups;
            g.add_intolerance(group_id, -1, 0, tol_max - immigrant_tol);
            if variations == 2 {
                let which = mating_index(mating_type);
                if which >= 0 {
                    g.add_intolerance(group_id, which, 0, tol_max - immigrant_tol);
                }
            }
            let Some(others) = self.member_tolerance(held, key, Tol::Others) else {
                return;
            };
            self.pop
                .groups
                .add_intolerance(group_id, -1, 1, tol_max - others);
        }
    }

    /// `MakeGroup(org)`: a new group one above the highest id.
    pub(crate) fn make_group(&mut self, held: Held, key: i32) {
        if self.cfg.use_form_groups != 1 {
            return;
        }
        let highest_group = self
            .pop
            .groups
            .groups
            .keys()
            .next_back()
            .copied()
            .unwrap_or(-1);
        self.set_member_opinion(held, key, highest_group + 1);
        self.join_group(held, key, highest_group + 1);
    }

    /// The group half of `ActivateOrganism`'s `assign_group` block: the
    /// organism just placed in `target` joins a group unless it already has
    /// an opinion. Returns the birth group id (`op`).
    pub(crate) fn assign_birth_group(&mut self, target: i32) -> i32 {
        let default_group = self.cfg.default_group;
        let mut op = default_group;
        let Some(org) = usize::try_from(target)
            .ok()
            .and_then(|t| self.pop.orgs.get_mut(t))
            .and_then(|o| o.as_deref_mut())
        else {
            return op;
        };
        if org.state.has_opinion() {
            return org.state.get_opinion().0;
        }
        if default_group != -1 {
            self.set_member_opinion(&mut [], target, default_group);
            self.join_group(&mut [], target, default_group);
        } else if self.cfg.use_form_groups == 1 {
            // QUIRK: `(int) abs(GetDouble())` is always 0, after a draw.
            op = self.rng.get_double().abs() as i32;
            self.set_member_opinion(&mut [], target, op);
            self.join_group(&mut [], target, op);
        } else if self.cfg.use_form_groups == 2 {
            let num_resources = self.env.resource_lib().size();
            op = self.rng.get_int_range(0, num_resources + 1);
            self.set_member_opinion(&mut [], target, op);
            self.join_group(&mut [], target, op);
        }
        op
    }

    /// The group block of `ActivateOffspring` for one offspring `child`
    /// (member key `key` until it is placed). Returns false when the
    /// offspring is doomed: rejected by its parent's group and by the group
    /// it tried to immigrate into.
    pub(crate) fn offspring_group(
        &mut self,
        parent: &mut OrgState,
        parent_cell: i32,
        child: &mut OrgState,
        key: i32,
    ) -> bool {
        if parent.has_opinion() {
            child.parent_group = parent.get_opinion().0;
        }
        // If tolerances are on ...
        if self.cfg.tolerance_window != 0 && self.cfg.tolerance_variations == 0 {
            return self.attempt_offspring_parent_group(
                &mut [(parent_cell, parent), (key, child)],
                parent_cell,
                key,
            );
        }
        // If not using tolerances or using immigrant only tolerance, put the
        // offspring in the parent's group.
        if self.cfg.inherit_opinion != 0 {
            if !parent.has_opinion() {
                self.pop.groups.ub(
                    "parent without an opinion (C++: GetOpinion() of an empty list in \
                     cPopulation::ActivateOffspring)"
                        .to_string(),
                );
                return true;
            }
            let group = parent.get_opinion().0;
            let held: Held = &mut [(key, child)];
            self.set_member_opinion(held, key, group);
            self.join_group(held, key, group);
        }
        true
    }

    /// The `DIVIDE_METHOD` split block at the top of `ActivateOffspring`:
    /// the parent is about to be reset to fully tolerant, so its
    /// intolerance leaves its group's cached totals.
    pub(crate) fn remove_split_parent_intolerance(&mut self, parent: &mut OrgState) {
        if !parent.has_opinion() {
            self.pop.groups.ub(
                "parent without an opinion (C++: GetOpinion() of an empty list in \
                 cPopulation::ActivateOffspring)"
                    .to_string(),
            );
            return;
        }
        let tol_max = self.cfg.max_tolerance;
        let update = self.stats.update();
        let group_id = parent.get_opinion().0;
        let org_imm_tolerance = parent
            .phenotype
            .calc_tolerance_immigrants(&self.cfg, update);
        let g = &mut self.pop.groups;
        g.add_intolerance(group_id, -1, 0, -(tol_max - org_imm_tolerance));
        if self.cfg.tolerance_variations == 2 {
            let which = mating_index(parent.phenotype.mating_type);
            if which >= 0 {
                g.add_intolerance(group_id, which, 0, -(tol_max - org_imm_tolerance));
            }
        }
        let others = parent
            .phenotype
            .calc_tolerance_offspring_others(&self.cfg, update);
        self.pop
            .groups
            .add_intolerance(group_id, -1, 1, -(tol_max - others));
    }

    /// `org->SetOpinion(opinion)` for a member.
    fn set_member_opinion(&mut self, held: Held, key: i32, opinion: i32) {
        let update = self.stats.update();
        let bsize = self.cfg.opinion_buffer_size;
        let result = match member_state(&mut self.pop.orgs, held, key) {
            Some(org) => org.set_opinion_bsize(bsize, update, opinion),
            None => Ok(()),
        };
        if let Err(e) = result {
            self.set_fatal(e);
        }
    }

    /// `LeaveGroup(org, group_id)`
    pub(crate) fn leave_group(&mut self, held: Held, key: i32, group_id: i32) {
        let mode = self.cfg.use_form_groups;
        let mating_type = self.member_mating_type(held, key);
        let g = &mut self.pop.groups;
        if let Some(count) = g.groups.get_mut(&group_id) {
            *count -= 1;
            if mating_type == MATING_TYPE_FEMALE {
                *g.females.entry(group_id).or_insert(0) -= 1;
            } else if mating_type == MATING_TYPE_MALE {
                *g.males.entry(group_id).or_insert(0) -= 1;
            }
            // If no restrictions on group ids, removes empty groups so the
            // number of total groups being tracked doesn't become excessive
            let count = *g.groups.get(&group_id).unwrap_or(&0);
            if mode == 1 && count <= 0 {
                g.groups.remove(&group_id);
                self.host.group_sizes.remove(&group_id);
            } else {
                self.host.group_sizes.insert(group_id, count);
            }
        }

        // If tolerance is on, remove the organism's intolerance from the
        // group's cache
        if self.cfg.tolerance_window > 0 {
            let tol_max = self.cfg.max_tolerance;
            let variations = self.cfg.tolerance_variations;
            let Some(immigrant_tol) = self.member_tolerance(held, key, Tol::Immigrants) else {
                return;
            };
            let g = &mut self.pop.groups;
            g.add_intolerance(group_id, -1, 0, -(tol_max - immigrant_tol));
            if variations == 2 {
                let which = mating_index(mating_type);
                if which >= 0 {
                    g.add_intolerance(group_id, which, 0, -(tol_max - immigrant_tol));
                }
            }
            let Some(others) = self.member_tolerance(held, key, Tol::Others) else {
                return;
            };
            self.pop
                .groups
                .add_intolerance(group_id, -1, 1, -(tol_max - others));
        }

        let g = &mut self.pop.groups;
        let members = g.members(group_id);
        if let Some(i) = members.iter().position(|&m| m == key) {
            members.swap_remove(i);
            // If no restrictions, removes empty groups.
            if mode == 1 && members.is_empty() {
                g.list.remove(&group_id);
            }
        }
    }

    /// `NumberOfOrganismsInGroup(group_id)`
    pub fn number_of_organisms_in_group(&self, group_id: i32) -> i32 {
        self.pop.groups.groups.get(&group_id).copied().unwrap_or(0)
    }

    /// `NumberGroupFemales(group_id)`
    pub fn number_group_females(&self, group_id: i32) -> i32 {
        let g = &self.pop.groups;
        if g.groups.contains_key(&group_id) {
            g.females.get(&group_id).copied().unwrap_or(0)
        } else {
            0
        }
    }

    /// `NumberGroupMales(group_id)`
    pub fn number_group_males(&self, group_id: i32) -> i32 {
        let g = &self.pop.groups;
        if g.groups.contains_key(&group_id) {
            g.males.get(&group_id).copied().unwrap_or(0)
        } else {
            0
        }
    }

    /// `NumberGroupJuvs(group_id)`
    pub fn number_group_juvs(&self, group_id: i32) -> i32 {
        let tot_orgs = self.number_of_organisms_in_group(group_id);
        tot_orgs - (self.number_group_males(group_id) + self.number_group_females(group_id))
    }

    /// `ChangeGroupMatingTypes(org, group_id, old_type, new_type)`.
    ///
    /// QUIRK: juveniles are `MATING_TYPE_JUVENILE` (-1), but the C++ tests
    /// the tolerance sub-caches for type 2, so juveniles never move there.
    pub(crate) fn change_group_mating_types(
        &mut self,
        held: Held,
        key: i32,
        group_id: i32,
        old_type: i32,
        new_type: i32,
    ) {
        if old_type == new_type {
            return;
        }
        let g = &mut self.pop.groups;
        if old_type == 0 {
            *g.females.entry(group_id).or_insert(0) -= 1;
        } else if old_type == 1 {
            *g.males.entry(group_id).or_insert(0) -= 1;
        }
        if new_type == 0 {
            *g.females.entry(group_id).or_insert(0) += 1;
        } else if new_type == 1 {
            *g.males.entry(group_id).or_insert(0) += 1;
        }
        if self.cfg.tolerance_window > 0 && self.cfg.tolerance_variations == 2 {
            let tol_max = self.cfg.max_tolerance;
            let Some(immigrant_tol) = self.member_tolerance(held, key, Tol::Immigrants) else {
                return;
            };
            let delta = tol_max - immigrant_tol;
            let g = &mut self.pop.groups;
            // remove from old
            if (0..=2).contains(&old_type) {
                g.add_intolerance(group_id, old_type, 0, -delta);
            }
            // add to new
            if (0..=2).contains(&new_type) {
                g.add_intolerance(group_id, new_type, 0, delta);
            }
        }
    }

    /// `CalcGroupToleranceImmigrants(group_id, mating_type)`
    pub(crate) fn calc_group_tolerance_immigrants(
        &mut self,
        held: Held,
        group_id: i32,
        mating_type: i32,
    ) -> i32 {
        let tolerance_max = self.cfg.max_tolerance;
        let variations = self.cfg.tolerance_variations;
        if group_id < 0 {
            return tolerance_max;
        }
        let g = &mut self.pop.groups;
        if g.members(group_id).is_empty() {
            return tolerance_max;
        }

        // use cache, if up to date
        let which = if variations == 2 && (0..=2).contains(&mating_type) {
            mating_type
        } else {
            -1
        };
        let Some(&mut cache) = g.cache(group_id, which) else {
            return tolerance_max;
        };
        let (tol_update, group_intol) = cache[0];
        let cur_update = self.stats.update();
        if tol_update == cur_update {
            return (tolerance_max - group_intol).max(0);
        }

        // If can't use cache, sum the total group intolerance
        let members = g.members(group_id).clone();
        let mut group_intolerance = 0;
        let mut single_member_intolerance = 0;
        for key in members {
            let mut use_org = true;
            // if using immigrant only tolerance + sex, only update the cache
            // for this current mating type
            if variations == 2 {
                let mt = self.member_mating_type(held, key);
                if (mating_type == 0 && mt != MATING_TYPE_FEMALE)
                    || (mating_type == 1 && mt != MATING_TYPE_MALE)
                    || (mating_type == 2 && mt != MATING_TYPE_JUVENILE)
                {
                    use_org = false;
                }
            }
            if use_org {
                let Some(tol) = self.member_tolerance(held, key, Tol::Immigrants) else {
                    return tolerance_max;
                };
                single_member_intolerance = tolerance_max - tol;
            }
            // QUIRK: a skipped member adds the previous member's intolerance
            // again (the C++ does not reset it).
            group_intolerance += single_member_intolerance;
        }

        // Save current update and current intolerance to group cache
        let g = &mut self.pop.groups;
        if variations != 2 || (0..=2).contains(&mating_type) {
            if let Some(c) = g.cache(group_id, which) {
                c[0] = (cur_update, group_intolerance);
            }
        }

        let group_tolerance = tolerance_max - group_intolerance;
        // return zero if totally intolerant (no negative numbers)
        group_tolerance.max(0)
    }

    /// `CalcGroupToleranceOffspring(parent)`: the group's tolerance towards
    /// offspring, the parent's own intolerance excluded.
    pub(crate) fn calc_group_tolerance_offspring(&mut self, held: Held, parent_key: i32) -> i32 {
        let tolerance_max = self.cfg.max_tolerance;
        let group_id = match member_state(&mut self.pop.orgs, held, parent_key) {
            Some(p) => p.get_opinion().0,
            None => return tolerance_max,
        };
        if group_id < 0 || self.cfg.tolerance_variations > 0 {
            return tolerance_max;
        }
        if self.pop.groups.members(group_id).is_empty() {
            return tolerance_max;
        }
        let cur_update = self.stats.update();
        let Some(parent_others) = self.member_tolerance(held, parent_key, Tol::Others) else {
            return tolerance_max;
        };
        let parent_intolerance = tolerance_max - parent_others;

        let Some(&mut cache) = self.pop.groups.cache(group_id, -1) else {
            return tolerance_max;
        };
        let mut group_intolerance = 0;
        if cache[1].0 == cur_update {
            group_intolerance = cache[1].1;
        } else {
            // Sum the total group intolerance
            let members = self.pop.groups.members(group_id).clone();
            for key in members {
                let Some(tol) = self.member_tolerance(held, key, Tol::Others) else {
                    return tolerance_max;
                };
                group_intolerance += tolerance_max - tol;
            }
            // Save current update and current intolerance to cache
            if let Some(c) = self.pop.groups.cache(group_id, -1) {
                c[1] = (cur_update, group_intolerance);
            }
        }

        // Remove the parent intolerance
        group_intolerance -= parent_intolerance;
        let group_tolerance = tolerance_max - group_intolerance;
        group_tolerance.max(0)
    }

    /// `CalcGroupOddsImmigrants(group_id, mating_type)`
    pub(crate) fn calc_group_odds_immigrants(
        &mut self,
        held: Held,
        group_id: i32,
        mating_type: i32,
    ) -> f64 {
        if group_id < 0 {
            return 1.0;
        }
        let tolerance_max = self.cfg.max_tolerance;
        let group_tolerance = self.calc_group_tolerance_immigrants(held, group_id, mating_type);
        f64::from(group_tolerance) / f64::from(tolerance_max)
    }

    /// `AttemptImmigrateGroup(ctx, group_id, org)`.
    ///
    /// QUIRK: with `TOLERANCE_VARIATIONS 2` an immigrant only gets in when
    /// its own mating type is absent from the group; the odds branch is the
    /// `else` of that test and is never reached then.
    pub(crate) fn attempt_immigrate_group(&mut self, held: Held, key: i32, group_id: i32) -> bool {
        let mut immigrate = false;
        // If non-standard group, automatic success
        if group_id < 0 {
            return true;
        }

        // If there are no members of the target group, automatic successful
        // immigration
        if self.number_of_organisms_in_group(group_id) == 0 {
            immigrate = true;
        } else if self.cfg.tolerance_variations == 2 {
            let mt = self.member_mating_type(held, key);
            if (mt == MATING_TYPE_FEMALE && self.number_group_females(group_id) == 0)
                || (mt == MATING_TYPE_MALE && self.number_group_males(group_id) == 0)
                || (mt == MATING_TYPE_JUVENILE && self.number_group_juvs(group_id) == 0)
            {
                immigrate = true;
            }
        } else {
            // Calculate chances based on target group tolerance of another
            // org successfully immigrating
            let probability_immigration = self.calc_group_odds_immigrants(held, group_id, -1);
            let rand = self.rng.get_double();
            if rand <= probability_immigration {
                immigrate = true;
            }
        }

        if immigrate {
            let opinion = member_state(&mut self.pop.orgs, held, key).and_then(|org| {
                if org.has_opinion() {
                    Some(org.get_opinion().0)
                } else {
                    None
                }
            });
            if let Some(opinion) = opinion {
                self.leave_group(held, key, opinion);
            }
            self.set_member_opinion(held, key, group_id);
            self.join_group(held, key, group_id);
        }
        immigrate
    }

    /// `CalcGroupOddsOffspring(parent)`
    pub(crate) fn calc_group_odds_offspring_parent(&mut self, held: Held, parent_key: i32) -> f64 {
        let opinion =
            member_state(&mut self.pop.orgs, held, parent_key).map_or(0, |p| p.get_opinion().0);
        // If non-standard group, automatic success
        if opinion < 0 || self.cfg.tolerance_variations > 0 {
            return 1.0;
        }
        let tolerance_max = f64::from(self.cfg.max_tolerance);
        let Some(parent_tolerance) = self.member_tolerance(held, parent_key, Tol::Own) else {
            return 1.0;
        };
        let parent_group_tolerance = self.calc_group_tolerance_offspring(held, parent_key);
        let prob_parent_allows = f64::from(parent_tolerance) / tolerance_max;
        let prob_group_allows = f64::from(parent_group_tolerance) / tolerance_max;
        prob_parent_allows * prob_group_allows
    }

    /// `CalcGroupOddsOffspring(group_id)`
    pub(crate) fn calc_group_odds_offspring(&mut self, held: Held, group_id: i32) -> f64 {
        // If non-standard group, automatic success
        if group_id < 0 || self.cfg.tolerance_variations > 0 {
            return 1.0;
        }
        let tolerance_max = self.cfg.max_tolerance;
        let members = self.pop.groups.members(group_id).clone();
        let mut group_intolerance = 0;
        for key in members {
            let Some(tol) = self.member_tolerance(held, key, Tol::Others) else {
                return 1.0;
            };
            group_intolerance += tolerance_max - tol;
            if group_intolerance >= tolerance_max {
                group_intolerance = tolerance_max;
                break;
            }
        }
        let group_tolerance = tolerance_max - group_intolerance;
        f64::from(group_tolerance) / f64::from(tolerance_max)
    }

    /// `AttemptOffspringParentGroup(ctx, parent, offspring)`: whether the
    /// offspring joins its parent's group, or immigrates into another one.
    pub(crate) fn attempt_offspring_parent_group(
        &mut self,
        held: Held,
        parent_key: i32,
        offspring_key: i32,
    ) -> bool {
        let parent_group =
            member_state(&mut self.pop.orgs, held, parent_key).map_or(0, |p| p.get_opinion().0);
        let window = self.cfg.tolerance_window;
        // If joining a non-standard group, only immigrant tolerance, or only
        // immigrant + sex tolerance, automatic success
        if parent_group < 0 || self.cfg.tolerance_variations > 0 {
            self.set_member_opinion(held, offspring_key, parent_group);
            self.join_group(held, offspring_key, parent_group);
            return true;
        }

        if window < 0 {
            // If using % chance of random migration
            let prob_immigrate = -f64::from(window) / 100.0;
            let rand = self.rng.get_double();
            if rand <= prob_immigrate {
                let num_groups = self.num_resources_updated(held);
                let mut target_group;
                loop {
                    target_group = self.rng.get_uint(num_groups as u32) as i32;
                    if target_group != parent_group {
                        break;
                    }
                }
                self.set_member_opinion(held, offspring_key, target_group);
                self.join_group(held, offspring_key, target_group);
            } else {
                // Put the offspring in the parent's group.
                self.set_member_opinion(held, offspring_key, parent_group);
                self.join_group(held, offspring_key, parent_group);
            }
            return true;
        }
        if window == 0 {
            return false;
        }

        // If using standard tolerances
        let tolerance_max = f64::from(self.cfg.max_tolerance);
        // Retrieve the parent's tolerance for its offspring
        let Some(parent_tolerance) = self.member_tolerance(held, parent_key, Tol::Own) else {
            return false;
        };
        // Retrieve the parent group's tolerance for offspring
        let parent_group_tolerance = self.calc_group_tolerance_offspring(held, parent_key);

        // Offspring first passes parent vote, then must also pass group vote
        let prob_parent_allows = f64::from(parent_tolerance) / tolerance_max;
        let prob_group_allows = f64::from(parent_group_tolerance) / tolerance_max;
        let rand2 = self.rng.get_double();
        let mut rand = self.rng.get_double();

        let mut join_parent_group = false;
        if rand <= prob_parent_allows {
            // If there is nobody else in the group, the offspring gets in
            join_parent_group = true;
            // If there are others in the group, it's their turn
            if self.pop.groups.members(parent_group).len() > 1 {
                join_parent_group = rand2 <= prob_group_allows;
            }
        }

        if join_parent_group {
            self.set_member_opinion(held, offspring_key, parent_group);
            self.join_group(held, offspring_key, parent_group);
            // Let the parent know that its offspring was born into its group
            if let Some(p) = member_state(&mut self.pop.orgs, held, parent_key) {
                p.phenotype.born_parent_group = true;
            }
            return true;
        }
        // Let the parent know its offspring was not born into its group
        if let Some(p) = member_state(&mut self.pop.orgs, held, parent_key) {
            p.phenotype.born_parent_group = false;
        }

        // If the offspring is rejected by the parent group, and there are no
        // other groups, the offspring is doomed
        let num_groups = self.num_resources_updated(held);
        if num_groups <= 1 {
            return false;
        }

        // Find another group at random, which is not the parent's
        let mut target_group;
        loop {
            target_group = self.rng.get_uint(num_groups as u32) as i32;
            if target_group != parent_group {
                break;
            }
        }

        // If there are no members currently of the target group, offspring
        // has 100% chance of immigrating
        if self.pop.groups.members(target_group).is_empty() {
            self.set_member_opinion(held, offspring_key, target_group);
            self.join_group(held, offspring_key, target_group);
            return true;
        }
        let probability_born_target_group = self.calc_group_odds_immigrants(held, target_group, -1);
        rand = self.rng.get_double();
        // Calculate if the offspring successfully immigrates
        if rand <= probability_born_target_group {
            self.set_member_opinion(held, offspring_key, target_group);
            self.join_group(held, offspring_key, target_group);
            return true;
        }
        // Offspring fails to immigrate and is doomed
        false
    }

    /// `GetResources(ctx).GetSize()`: the global resource count after
    /// bringing the resources up to date.
    /// The running organism's state, when it is one of `held`, goes to the
    /// resource read: a damaging gradient may injure it there.
    fn num_resources_updated(&mut self, held: Held) -> usize {
        let uid = self
            .pop
            .running
            .as_ref()
            .filter(|r| !r.removed)
            .map(|r| r.uid);
        let running = uid
            .and_then(|u| held.iter_mut().find(|(_, s)| s.uid == u))
            .map(|(_, s)| &mut **s);
        self.with_pop_resources_for(running, |rc, rng, env| rc.get_resources(rng, env).len())
    }

    /// The per-member tolerance statistics behind `CalcGroupAve*` and
    /// `CalcGroupSDev*`; `mating_type` filters as `CalcGroupAveImmigrants`.
    fn group_tolerance_sum(&mut self, group_id: i32, which: Tol, mating_type: i32) -> DoubleSum {
        let mut sum = DoubleSum::new();
        let members = self.pop.groups.members(group_id).clone();
        for key in members {
            let count_org = match mating_type {
                -1 => true,
                0 => self.member_mating_type(&mut [], key) == MATING_TYPE_FEMALE,
                1 => self.member_mating_type(&mut [], key) == MATING_TYPE_MALE,
                2 => self.member_mating_type(&mut [], key) == MATING_TYPE_JUVENILE,
                _ => false,
            };
            if count_org {
                if let Some(tol) = self.member_tolerance(&mut [], key, which) {
                    sum.add(f64::from(tol));
                }
            }
        }
        sum
    }

    /// `KillGroupMember(ctx, group_id, org)`: kill a random other member.
    pub(crate) fn kill_group_member(&mut self, org: &mut OrgState, key: i32, group_id: i32) {
        let members = self.pop.groups.members(group_id).clone();
        // Check to make sure we are not killing self!
        if members.len() == 1 && members[0] == key {
            return;
        }
        if members.is_empty() {
            return;
        }
        let index = loop {
            let index = self.rng.get_uint_range(0, members.len() as u32) as usize;
            if members[index] != key {
                break index;
            }
        };
        self.kill_organism(members[index], Some(org));
    }

    /// `GetFormedGroupArray()`: the groups with members.
    pub fn formed_group_array(&self) -> Vec<i32> {
        self.pop
            .groups
            .groups
            .iter()
            .filter(|(_, &size)| size > 0)
            .map(|(&g, _)| g)
            .collect()
    }

    // ------------------------------------------------------------------
    // cPopulationInterface tolerance methods
    // ------------------------------------------------------------------

    /// `cPopulationInterface::IncTolerance(tolerance_type, ctx)`: drop the
    /// most recent dec-tolerance record; returns the new tolerance.
    pub(crate) fn inc_tolerance(
        &mut self,
        org: &mut OrgState,
        key: i32,
        tolerance_type: i32,
    ) -> i32 {
        let group_id = org.get_opinion().0;
        let variations = self.cfg.tolerance_variations;
        let update = self.stats.update();
        match tolerance_type {
            0 => {
                self.push_tolerance_inst_exe(org, key, 0);
                org.phenotype.tolerance_immigrants.pop_front();
                if org.phenotype.intolerances[0].1 != 0 {
                    org.phenotype.intolerances[0].1 -= 1;
                    self.dec_group_intolerance(group_id, 0, -1);
                    if variations == 2 {
                        let which = mating_index(org.phenotype.mating_type);
                        if which >= 0 {
                            self.dec_group_intolerance(group_id, 0, which);
                        }
                    }
                }
                org.phenotype.calc_tolerance_immigrants(&self.cfg, update)
            }
            1 => {
                self.push_tolerance_inst_exe(org, key, 1);
                org.phenotype.tolerance_offspring_own.pop_front();
                if org.phenotype.intolerances[1].1 != 0 {
                    org.phenotype.intolerances[1].1 -= 1;
                }
                org.phenotype
                    .calc_tolerance_offspring_own(&self.cfg, update)
            }
            2 => {
                self.push_tolerance_inst_exe(org, key, 2);
                org.phenotype.tolerance_offspring_others.pop_front();
                if org.phenotype.intolerances[2].1 != 0 {
                    org.phenotype.intolerances[2].1 -= 1;
                    self.dec_group_intolerance(group_id, 1, -1);
                }
                org.phenotype
                    .calc_tolerance_offspring_others(&self.cfg, update)
            }
            _ => -1,
        }
    }

    /// `cPopulationInterface::DecTolerance(tolerance_type, ctx)`: record a
    /// dec-tolerance for the current update; returns the new tolerance.
    pub(crate) fn dec_tolerance(
        &mut self,
        org: &mut OrgState,
        key: i32,
        tolerance_type: i32,
    ) -> i32 {
        let cur_update = self.stats.update();
        let tolerance_max = self.cfg.max_tolerance;
        let variations = self.cfg.tolerance_variations;
        let group_id = org.get_opinion().0;
        let (inst, slot) = match tolerance_type {
            0 => (3, 0),
            1 => (4, 1),
            2 => (5, 2),
            _ => return -1,
        };
        self.push_tolerance_inst_exe(org, key, inst);
        // Update tolerance list by inserting new record (at the front)
        let ph = &mut org.phenotype;
        let list = match slot {
            0 => &mut ph.tolerance_immigrants,
            1 => &mut ph.tolerance_offspring_own,
            _ => &mut ph.tolerance_offspring_others,
        };
        list.push_front(cur_update);
        if list.len() as i32 > tolerance_max {
            list.pop_back();
        }
        // If not at min tolerance, increase the intolerance cache
        if ph.intolerances[slot].1 != tolerance_max {
            ph.intolerances[slot].1 += 1;
            match slot {
                0 => {
                    self.inc_group_intolerance(group_id, 0, -1);
                    if variations == 2 {
                        let which = mating_index(org.phenotype.mating_type);
                        if which >= 0 {
                            self.inc_group_intolerance(group_id, 0, which);
                        }
                    }
                }
                2 => self.inc_group_intolerance(group_id, 1, -1),
                _ => {}
            }
        }
        match slot {
            0 => org
                .phenotype
                .calc_tolerance_immigrants(&self.cfg, cur_update),
            1 => org
                .phenotype
                .calc_tolerance_offspring_own(&self.cfg, cur_update),
            _ => org
                .phenotype
                .calc_tolerance_offspring_others(&self.cfg, cur_update),
        }
    }

    /// `GetGroupIntolerances(group_id, tol_num, mating_type)--`
    fn dec_group_intolerance(&mut self, group_id: i32, tol_num: i32, mating_type: i32) {
        if let Some(v) = self
            .pop
            .groups
            .group_intolerance_slot(group_id, tol_num, mating_type)
        {
            *v -= 1;
        }
    }

    /// `GetGroupIntolerances(group_id, tol_num, mating_type)++`
    fn inc_group_intolerance(&mut self, group_id: i32, tol_num: i32, mating_type: i32) {
        if let Some(v) = self
            .pop
            .groups
            .group_intolerance_slot(group_id, tol_num, mating_type)
        {
            *v += 1;
        }
    }

    /// `cPopulationInterface::PushToleranceInstExe(tol_inst, ctx)`: count a
    /// tolerance instruction and, with `TRACK_TOLERANCE`, record the
    /// circumstances of the executing organism `org` (in cell `key`).
    pub(crate) fn push_tolerance_inst_exe(&mut self, org: &mut OrgState, key: i32, tol_inst: i32) {
        let tol_inst_idx = tol_inst.max(0) as usize;
        if self.cfg.track_tolerance == 0 {
            self.stats.push_tolerance_inst_exe(tol_inst_idx);
            return;
        }

        let group_id = org.get_opinion().0;
        let group_size = self.number_of_organisms_in_group(group_id);
        let resource_level = {
            let res_count = self.with_pop_resources_for(Some(&mut *org), |rc, rng, env| {
                rc.get_cell_resources(key, rng, env).to_vec()
            });
            match usize::try_from(group_id)
                .ok()
                .and_then(|g| res_count.get(g))
            {
                Some(&v) => v,
                None => {
                    self.set_fatal(format!(
                        "PushToleranceInstExe: group {group_id} is not a resource index \
                         (C++: out-of-bounds read)"
                    ));
                    0.0
                }
            }
        };
        let tol_max = self.cfg.max_tolerance;
        let update = self.stats.update();

        let held: Held = &mut [(key, org)];
        let immigrant_odds = self.calc_group_odds_immigrants(held, group_id, -1);
        let tol_immi = self
            .member_tolerance(held, key, Tol::Immigrants)
            .unwrap_or(0);
        let (offspring_own_odds, offspring_others_odds, tol_own, tol_others) =
            if self.cfg.tolerance_variations > 0 {
                (1.0, 1.0, tol_max, tol_max)
            } else {
                let own_odds = self.calc_group_odds_offspring_parent(held, key);
                let others_odds = self.calc_group_odds_offspring(held, group_id);
                let org = &mut *held[0].1;
                let tol_own = org
                    .phenotype
                    .calc_tolerance_offspring_own(&self.cfg, update);
                let tol_others = org
                    .phenotype
                    .calc_tolerance_offspring_others(&self.cfg, update);
                (own_odds, others_odds, tol_own, tol_others)
            };

        let odds_immi = immigrant_odds * 100.0;
        let odds_own = offspring_own_odds * 100.0;
        let odds_others = offspring_others_odds * 100.0;

        self.stats.push_tolerance_inst_exe_circumstances(
            tol_inst_idx,
            group_id,
            group_size,
            resource_level,
            odds_immi,
            odds_own,
            odds_others,
            tol_immi,
            tol_own,
            tol_others,
            tol_max,
        );
    }

    // ------------------------------------------------------------------
    // Printers
    // ------------------------------------------------------------------

    /// `cActionPrintGroupTolerance` / `cActionPrintGroupMTTolerance`:
    /// `UpdateResStats(ctx)`, then `cStats::PrintGroupTolerance` or
    /// `PrintGroupMTTolerance`.
    pub(crate) fn print_group_tolerance(
        &mut self,
        filename: &str,
        by_mating_type: bool,
    ) -> Result<(), String> {
        self.update_res_stats();
        let groups: Vec<(i32, i32)> = self
            .pop
            .groups
            .groups
            .iter()
            .map(|(&g, &n)| (g, n))
            .collect();
        let tolerance_on = self.cfg.tolerance_window > 0;
        let update = self.stats.update();

        // Gather each row first: the calculations borrow the world, the
        // data file borrows the output manager.
        let mut rows: Vec<Vec<(Value, &'static str)>> = Vec::new();
        for (i, size) in groups {
            let cur_size = f64::from(size);
            let Some(&res) = usize::try_from(i)
                .ok()
                .and_then(|r| self.stats.resource_count.get(r))
            else {
                return Err(format!(
                    "{filename}: group {i} is not a resource index (C++: out-of-bounds read of \
                     cStats::resource_count)"
                ));
            };
            let mut row: Vec<(Value, &'static str)> = Vec::new();
            if by_mating_type {
                row.push((Value::I(update), "Update"));
                row.push((Value::I(i), "group id"));
                row.push((Value::F(cur_size), "group size"));
                row.push((
                    Value::I(self.number_group_females(i)),
                    "number group females",
                ));
                row.push((Value::I(self.number_group_males(i)), "number group males"));
                row.push((Value::I(self.number_group_juvs(i)), "number group juvs"));
                if tolerance_on {
                    for (mt, names) in [
                        (
                            0,
                            [
                                "immigrant female odds",
                                "ave female-female tolerance",
                                "sd female-female tolerance",
                            ],
                        ),
                        (
                            1,
                            [
                                "immigrant male odds",
                                "ave male-male tolerance",
                                "sd male-male tolerance",
                            ],
                        ),
                        (
                            2,
                            [
                                "immigrant juv odds",
                                "ave juv-juv tolerance",
                                "sd juv-juv tolerance",
                            ],
                        ),
                    ] {
                        let odds = self.calc_group_odds_immigrants(&mut [], i, mt);
                        row.push((Value::F(odds), names[0]));
                        let ave = self.group_tolerance_sum(i, Tol::Immigrants, mt).average();
                        row.push((Value::F(ave), names[1]));
                        let sd = self
                            .group_tolerance_sum(i, Tol::Immigrants, mt)
                            .std_deviation();
                        row.push((Value::F(sd), names[2]));
                    }
                }
                row.push((Value::F(res), "group resource available"));
                row.push((
                    Value::F(res / cur_size),
                    "per capita group resource available",
                ));
            } else {
                row.push((Value::I(update), "Update [update]"));
                row.push((Value::I(i), "group id [groupid]"));
                row.push((Value::F(cur_size), "size of groups [grsize]"));
                row.push((Value::F(res), "group resource available [grfood]"));
                row.push((
                    Value::F(res / cur_size),
                    "per capita group resource available [grfoodper]",
                ));
                if tolerance_on {
                    let v = self.calc_group_odds_immigrants(&mut [], i, -1);
                    row.push((
                        Value::F(v),
                        "odds for immigrants coming into group [oddsimmigrants]",
                    ));
                    let v = self.group_tolerance_sum(i, Tol::Immigrants, -1).average();
                    row.push((
                        Value::F(v),
                        "average intra-group tolerance to immigrants [aveimmigrants]",
                    ));
                    let v = self
                        .group_tolerance_sum(i, Tol::Immigrants, -1)
                        .std_deviation();
                    row.push((
                        Value::F(v),
                        "standard deviation for group tolerance to immigrants [sdevimmigrants]",
                    ));
                    let v = self.calc_group_odds_offspring(&mut [], i);
                    row.push((
                        Value::F(v),
                        "odds for offspring being accepted by group [oddsoffspring]",
                    ));
                    let v = self.group_tolerance_sum(i, Tol::Others, -1).average();
                    row.push((
                        Value::F(v),
                        "average intra-group tolerance to other offspring being born into group \
                         [aveothers]",
                    ));
                    let v = self.group_tolerance_sum(i, Tol::Others, -1).std_deviation();
                    row.push((
                        Value::F(v),
                        "standard deviation for group tolerance to other offspring being born \
                         into the group [sdevothers]",
                    ));
                    let v = self.group_tolerance_sum(i, Tol::Own, -1).average();
                    row.push((
                        Value::F(v),
                        "average intra-group tolerance to individual's own offspring [aveown]",
                    ));
                    let v = self.group_tolerance_sum(i, Tol::Own, -1).std_deviation();
                    row.push((
                        Value::F(v),
                        "standard deviation for tolerance to own offspring [sdevown]",
                    ));
                }
            }
            rows.push(row);
        }

        let df = self.out.static_with_path(filename)?;
        df.write_comment(if by_mating_type {
            "Group level tolerance data by mating type."
        } else {
            "Group level tolerance data."
        });
        df.write_time_stamp();
        for row in rows {
            for (v, descr) in row {
                v.write_to(df, descr);
            }
            df.endl();
        }
        Ok(())
    }
}

/// A data-file value gathered before the file is borrowed.
#[derive(Clone, Copy)]
enum Value {
    I(i32),
    F(f64),
}

impl Value {
    fn write_to(self, df: &mut DataFile, descr: &str) {
        match self {
            Value::I(v) => df.write(v, descr),
            Value::F(v) => df.write(v, descr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rekey_replaces_pending_members() {
        let mut g = Groups::default();
        g.list.insert(0, vec![5, pending_key(0), 7]);
        g.list.insert(1, vec![pending_key(1)]);
        g.rekey(pending_key(0), 9);
        assert_eq!(g.list[&0], vec![5, 9, 7]);
        assert_eq!(g.list[&1], vec![pending_key(1)]);
    }

    #[test]
    fn group_intolerance_slot_copies_the_mating_type_cache_first() {
        let mut g = Groups::default();
        g.intolerances.insert(3, [(1, 4), (1, 6)]);
        g.intolerances_females.insert(3, [(1, 9), (1, 2)]);
        // whole group: no copy
        *g.group_intolerance_slot(3, 0, -1).unwrap() += 1;
        assert_eq!(g.intolerances[&3][0].1, 5);
        // QUIRK: females' value lands in the whole-group slot, then `--`
        *g.group_intolerance_slot(3, 0, 0).unwrap() -= 1;
        assert_eq!(g.intolerances[&3][0].1, 8);
        assert_eq!(g.intolerances_females[&3][0].1, 9);
        // a missing cache is recorded as undefined behaviour
        assert!(g.group_intolerance_slot(4, 0, -1).is_none());
        assert!(g.fatal.is_some());
    }

    #[test]
    fn mating_index_maps_juveniles_to_two() {
        assert_eq!(mating_index(MATING_TYPE_FEMALE), 0);
        assert_eq!(mating_index(MATING_TYPE_MALE), 1);
        assert_eq!(mating_index(MATING_TYPE_JUVENILE), 2);
    }
}
