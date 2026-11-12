//! Port of `Avida::Systematics::Manager` (avida-core/source/systematics/Manager.cc)
//! and of the `Unit` methods that reach the groups
//! (avida-core/source/systematics/Unit.cc: `SelfClassify`,
//! `HandleGestation`, `~Unit`).
//!
//! The manager owns the arbiters in registration order; a group is named by
//! a [`GroupRef`] (arbiter index plus handle). `Arbiter` is a closed enum
//! of the two arbiters avida-core defines.

use std::collections::HashMap;

use avida_apto::DataFile;
use avida_genome::Genome;

use crate::apto_map::AptoMap;
use crate::clade::CladeArbiter;
use crate::genotype_arbiter::GenotypeArbiter;
use crate::group_data::{GenomeTestMetrics, SexualAncestry};
use crate::property::PropertyValue;
use crate::unit::{
    ClassificationHints, GestationInfo, GroupMembership, GroupRef, RoleClassificationHints,
    RoleIndex, UnitInfo,
};

/// `Systematics::Arbiter` subclasses.
#[derive(Clone, Debug)]
pub enum Arbiter {
    /// `GenotypeArbiter` (boxed: it carries 3203 list headers' ids)
    Genotype(Box<GenotypeArbiter>),
    /// `CladeArbiter`
    Clade(CladeArbiter),
}

impl Arbiter {
    /// Wraps a genotype arbiter.
    pub fn genotype(a: GenotypeArbiter) -> Self {
        Arbiter::Genotype(Box::new(a))
    }

    /// `Role()`.
    pub fn role(&self) -> &str {
        match self {
            Arbiter::Genotype(a) => a.role(),
            Arbiter::Clade(a) => a.role(),
        }
    }

    /// The genotype arbiter, if this is one.
    pub fn as_genotype(&self) -> Option<&GenotypeArbiter> {
        match self {
            Arbiter::Genotype(a) => Some(&**a),
            Arbiter::Clade(_) => None,
        }
    }

    /// Mutable [`Arbiter::as_genotype`].
    pub fn as_genotype_mut(&mut self) -> Option<&mut GenotypeArbiter> {
        match self {
            Arbiter::Genotype(a) => Some(&mut **a),
            Arbiter::Clade(_) => None,
        }
    }

    /// The clade arbiter, if this is one.
    pub fn as_clade(&self) -> Option<&CladeArbiter> {
        match self {
            Arbiter::Clade(a) => Some(a),
            Arbiter::Genotype(_) => None,
        }
    }

    /// Mutable [`Arbiter::as_clade`].
    pub fn as_clade_mut(&mut self) -> Option<&mut CladeArbiter> {
        match self {
            Arbiter::Clade(a) => Some(a),
            Arbiter::Genotype(_) => None,
        }
    }
}

/// Port of `Systematics::Manager`.
#[derive(Clone, Debug, Default)]
pub struct Manager {
    arbiters: Vec<Arbiter>,
}

impl Manager {
    /// An empty manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// `RegisterArbiter(arbiter)`: false when an arbiter with the same role
    /// is already registered (the C++ returns true only when re-registering
    /// the very same object, which a moved value cannot be).
    pub fn register_arbiter(&mut self, arbiter: Arbiter) -> bool {
        if self.arbiters.iter().any(|a| a.role() == arbiter.role()) {
            return false;
        }
        self.arbiters.push(arbiter);
        true
    }

    /// `ArbiterForRole(role)` as an index.
    pub fn arbiter_for_role(&self, role: &str) -> Option<RoleIndex> {
        self.arbiters.iter().position(|a| a.role() == role)
    }

    /// The arbiters in registration order.
    pub fn arbiters(&self) -> &[Arbiter] {
        &self.arbiters
    }

    /// One arbiter.
    pub fn arbiter(&self, role: RoleIndex) -> Option<&Arbiter> {
        self.arbiters.get(role)
    }

    /// One arbiter, mutably.
    pub fn arbiter_mut(&mut self, role: RoleIndex) -> Option<&mut Arbiter> {
        self.arbiters.get_mut(role)
    }

    /// `ArbiterForRole("genotype")` as a genotype arbiter.
    pub fn genotypes(&self) -> Option<&GenotypeArbiter> {
        self.arbiters
            .iter()
            .find(|a| a.role() == "genotype")
            .and_then(Arbiter::as_genotype)
    }

    /// Mutable [`Manager::genotypes`].
    pub fn genotypes_mut(&mut self) -> Option<&mut GenotypeArbiter> {
        self.arbiters
            .iter_mut()
            .find(|a| a.role() == "genotype")
            .and_then(Arbiter::as_genotype_mut)
    }

    /// The `"genotype"` group of a membership (`SystematicsGroup("genotype")`).
    pub fn genotype_of(&self, m: &GroupMembership) -> Option<GroupRef> {
        m.group(self.arbiter_for_role("genotype")?)
    }

    /// The dominant genotype as print actions find it
    /// (`ArbiterForRole("genotype")->Begin()->Next()`).
    pub fn dominant_genotype(&self) -> Option<GroupRef> {
        let role = self.arbiter_for_role("genotype")?;
        let handle = self.arbiters[role].as_genotype()?.dominant()?;
        Some(GroupRef { role, handle })
    }

    // ------------------------------------------------------------------
    // Classification
    // ------------------------------------------------------------------

    /// `ClassifyNewUnit(u, role_hints)`: offer the unit to every arbiter in
    /// registration order, with that role's hints, and record each group
    /// in the unit's membership (`Unit::AddClassification`).
    pub fn classify_new_unit(
        &mut self,
        u: &UnitInfo<'_>,
        role_hints: Option<&RoleClassificationHints>,
        membership: &mut GroupMembership,
    ) {
        for i in 0..self.arbiters.len() {
            let hints: Option<&ClassificationHints> =
                role_hints.and_then(|h| h.get(self.arbiters[i].role()));
            let g = match &mut self.arbiters[i] {
                Arbiter::Genotype(a) => Some(a.classify_new_unit(u, hints)),
                Arbiter::Clade(a) => a.classify_new_unit(hints),
            };
            if let Some(handle) = g {
                membership.add_classification(GroupRef { role: i, handle });
            }
        }
    }

    /// `Group::ClassifyNewUnit(u, parents)` on the group's arbiter.
    pub fn group_classify_new_unit(
        &mut self,
        g: GroupRef,
        u: &UnitInfo<'_>,
        parents: Option<&[GroupRef]>,
    ) -> Option<GroupRef> {
        let handle = match self.arbiters.get_mut(g.role)? {
            Arbiter::Genotype(a) => {
                let ps: Option<Vec<_>> = parents.map(|ps| {
                    ps.iter()
                        .filter(|p| p.role == g.role)
                        .map(|p| p.handle)
                        .collect()
                });
                a.group_classify_new_unit(g.handle, u, ps.as_deref())
            }
            Arbiter::Clade(a) => a.group_classify_new_unit(g.handle)?,
        };
        Some(GroupRef {
            role: g.role,
            handle,
        })
    }

    /// `Unit::SelfClassify(parent_groups)`: classify a newborn through its
    /// parents' groups. One parent: each of its groups classifies the unit
    /// with itself as the parent. Several parents: the parental groups are
    /// pooled per role (an `Apto::Map` keyed by role name, iterated in its
    /// bucket order: `genotype` before `clade`), and the first group of
    /// each pool classifies with the whole pool. The resulting groups are
    /// appended without the role check of `AddClassification`.
    pub fn self_classify(
        &mut self,
        u: &UnitInfo<'_>,
        parent_groups: &[&GroupMembership],
        membership: &mut GroupMembership,
    ) {
        if parent_groups.len() == 1 {
            // Handle Asexual Organisms
            for &g in parent_groups[0].groups() {
                if let Some(r) = self.group_classify_new_unit(g, u, Some(&[g])) {
                    membership.push(r);
                }
            }
            return;
        }
        // Handle Sexual Organisms (multi-parent)
        let mut pgs: AptoMap<String, Vec<GroupRef>> = AptoMap::new();
        for p in parent_groups {
            for &g in p.groups() {
                let role = self
                    .arbiters
                    .get(g.role)
                    .map(|a| a.role().to_string())
                    .unwrap_or_default();
                pgs.get_or_insert(&role).push(g);
            }
        }
        let pools: Vec<Vec<GroupRef>> = pgs.values().cloned().collect();
        for pool in pools {
            if let Some(&first) = pool.first() {
                if let Some(r) = self.group_classify_new_unit(first, u, Some(&pool)) {
                    membership.push(r);
                }
            }
        }
    }

    /// `Unit::HandleGestation()`: every group sees the unit's gestation.
    pub fn handle_gestation(&mut self, membership: &GroupMembership, info: &GestationInfo) {
        for &g in membership.groups() {
            if let Some(Arbiter::Genotype(a)) = self.arbiters.get_mut(g.role) {
                a.handle_unit_gestation(g.handle, info);
            }
        }
    }

    /// `Unit::~Unit()`: the unit leaves every group it belongs to.
    pub fn remove_unit(&mut self, membership: &GroupMembership) {
        for &g in membership.groups() {
            self.group_remove_unit(g);
        }
    }

    /// `Group::RemoveUnit()`.
    pub fn group_remove_unit(&mut self, g: GroupRef) {
        match self.arbiters.get_mut(g.role) {
            Some(Arbiter::Genotype(a)) => a.remove_unit(g.handle),
            Some(Arbiter::Clade(a)) => a.remove_unit(g.handle),
            None => {}
        }
    }

    /// `Group::AddActiveReference()` (birth chamber entries).
    pub fn add_active_reference(&mut self, g: GroupRef) {
        match self.arbiters.get_mut(g.role) {
            Some(Arbiter::Genotype(a)) => a.add_active_reference(g.handle),
            Some(Arbiter::Clade(a)) => a.add_active_reference(g.handle),
            None => {}
        }
    }

    /// `Group::RemoveActiveReference()`.
    pub fn remove_active_reference(&mut self, g: GroupRef) {
        match self.arbiters.get_mut(g.role) {
            Some(Arbiter::Genotype(a)) => a.remove_active_reference(g.handle),
            Some(Arbiter::Clade(a)) => a.remove_active_reference(g.handle),
            None => {}
        }
    }

    /// `Group::AddPassiveReference()` (`cDeme` germline genotypes).
    pub fn add_passive_reference(&mut self, g: GroupRef) {
        match self.arbiters.get_mut(g.role) {
            Some(Arbiter::Genotype(a)) => a.add_passive_reference(g.handle),
            Some(Arbiter::Clade(a)) => a.add_passive_reference(g.handle),
            None => {}
        }
    }

    /// `Group::RemovePassiveReference()`.
    pub fn remove_passive_reference(&mut self, g: GroupRef) {
        match self.arbiters.get_mut(g.role) {
            Some(Arbiter::Genotype(a)) => a.remove_passive_reference(g.handle),
            Some(Arbiter::Clade(a)) => a.remove_passive_reference(g.handle),
            None => {}
        }
    }

    // ------------------------------------------------------------------
    // Updates and provided data
    // ------------------------------------------------------------------

    /// `PerformUpdate(ctx, current_update)` of every arbiter, in order.
    pub fn perform_update(&mut self, current_update: i32) {
        for a in &mut self.arbiters {
            match a {
                Arbiter::Genotype(a) => a.perform_update(current_update),
                Arbiter::Clade(a) => a.perform_update(current_update),
            }
        }
    }

    /// `UpdateProvidedValues(current_update)` of every arbiter.
    pub fn update_provided_values(&mut self, current_update: i32) {
        for a in &mut self.arbiters {
            match a {
                Arbiter::Genotype(a) => a.update_provided_values(current_update),
                Arbiter::Clade(a) => a.update_provided_values(current_update),
            }
        }
    }

    /// The provided value with this id from whichever arbiter provides it.
    /// `cStats` reads `systematics.genotype.current` and
    /// `systematics.genotype.current_threshold`.
    pub fn provided_value(&self, data_id: &str) -> Option<PropertyValue> {
        self.arbiters.iter().find_map(|a| match a {
            Arbiter::Genotype(a) => a.provided_value(data_id),
            Arbiter::Clade(a) => a.provided_value(data_id),
        })
    }

    // ------------------------------------------------------------------
    // Group queries
    // ------------------------------------------------------------------

    /// `Arbiter::Group(id)` on one arbiter.
    pub fn group(&self, role: RoleIndex, id: i32) -> Option<GroupRef> {
        let handle = match self.arbiters.get(role)? {
            Arbiter::Genotype(a) => a.group(id),
            Arbiter::Clade(a) => a.group(id),
        }?;
        Some(GroupRef { role, handle })
    }

    /// `Begin()` / `Next()` over one arbiter's groups.
    pub fn iter_groups(&self, role: RoleIndex) -> Vec<GroupRef> {
        let hs = match self.arbiters.get(role) {
            Some(Arbiter::Genotype(a)) => a.iter_groups(),
            Some(Arbiter::Clade(a)) => a.iter_groups(),
            None => Vec::new(),
        };
        hs.into_iter()
            .map(|handle| GroupRef { role, handle })
            .collect()
    }

    /// `ID()`.
    pub fn group_id(&self, g: GroupRef) -> Option<i32> {
        match self.arbiters.get(g.role)? {
            Arbiter::Genotype(a) => a.genotype(g.handle).map(|x| x.id()),
            Arbiter::Clade(a) => a.clade(g.handle).map(|x| x.id()),
        }
    }

    /// `NumUnits()`.
    pub fn num_units(&self, g: GroupRef) -> Option<i32> {
        match self.arbiters.get(g.role)? {
            Arbiter::Genotype(a) => a.genotype(g.handle).map(|x| x.num_units()),
            Arbiter::Clade(a) => a.clade(g.handle).map(|x| x.num_units()),
        }
    }

    /// `Depth()`.
    pub fn depth(&self, g: GroupRef) -> Option<i32> {
        match self.arbiters.get(g.role)? {
            Arbiter::Genotype(a) => a.genotype(g.handle).map(|x| x.depth()),
            Arbiter::Clade(a) => a.clade(g.handle).map(|x| x.depth()),
        }
    }

    /// `Role()` of a group.
    pub fn role_of(&self, g: GroupRef) -> Option<&str> {
        self.arbiters.get(g.role).map(Arbiter::role)
    }

    /// `Properties().Has(name)`.
    pub fn has_property(&self, g: GroupRef, name: &str) -> bool {
        match self.arbiters.get(g.role) {
            Some(Arbiter::Genotype(a)) => a.has_property(g.handle, name),
            Some(Arbiter::Clade(a)) => a.clade(g.handle).is_some_and(|c| c.has_property(name)),
            None => false,
        }
    }

    /// `Properties().Get(name)`.
    pub fn property(&self, g: GroupRef, name: &str) -> PropertyValue {
        match self.arbiters.get(g.role) {
            Some(Arbiter::Genotype(a)) => a.property(g.handle, name),
            Some(Arbiter::Clade(a)) => a
                .clade(g.handle)
                .map_or(PropertyValue::Null, |c| c.property(name)),
            None => PropertyValue::Null,
        }
    }

    /// `GetData<SexualAncestry>()`, created on demand. A clade has no
    /// `parents` property, so its record has no ancestors (it is not
    /// stored).
    pub fn sexual_ancestry(&mut self, g: GroupRef) -> Option<SexualAncestry> {
        match self.arbiters.get_mut(g.role)? {
            Arbiter::Genotype(a) => a.sexual_ancestry(g.handle),
            Arbiter::Clade(a) => a
                .clade(g.handle)
                .map(|c| SexualAncestry::without_parents(c.id())),
        }
    }

    /// `GenomeTestMetrics::GetMetrics(world, ctx, g)`; `None` for groups
    /// without a `genome` property (the C++ returns NULL).
    pub fn genome_test_metrics(
        &mut self,
        g: GroupRef,
        test: impl FnOnce(&Genome) -> GenomeTestMetrics,
    ) -> Option<&GenomeTestMetrics> {
        match self.arbiters.get_mut(g.role)? {
            Arbiter::Genotype(a) => a.genome_test_metrics(g.handle, test),
            Arbiter::Clade(_) => None,
        }
    }

    // ------------------------------------------------------------------
    // Legacy save / load
    // ------------------------------------------------------------------

    /// `Group::LegacySave(df)`: the group's columns of a structured save row.
    pub fn group_legacy_save(&self, g: GroupRef, df: &mut DataFile) -> bool {
        match self.arbiters.get(g.role) {
            Some(Arbiter::Genotype(a)) => a.save_genotype(g.handle, df),
            Some(Arbiter::Clade(a)) => a.save_clade(g.handle, df),
            None => false,
        }
    }

    /// `Arbiter::LegacySave(df)` (the historic genotypes).
    pub fn legacy_save(&self, role: RoleIndex, df: &mut DataFile) -> bool {
        match self.arbiters.get(role) {
            Some(Arbiter::Genotype(a)) => a.legacy_save(df),
            Some(Arbiter::Clade(a)) => a.legacy_save(df),
            None => false,
        }
    }

    /// `Arbiter::LegacyLoad(props)`.
    pub fn legacy_load(
        &mut self,
        role: RoleIndex,
        props: &HashMap<String, String>,
    ) -> Result<Option<GroupRef>, String> {
        let handle = match self.arbiters.get_mut(role) {
            Some(Arbiter::Genotype(a)) => Some(a.legacy_load(props)?),
            Some(Arbiter::Clade(a)) => a.legacy_load(props),
            None => None,
        };
        Ok(handle.map(|handle| GroupRef { role, handle }))
    }
}
