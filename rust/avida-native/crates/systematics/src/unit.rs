//! What the systematics layer needs to know about a unit, and the unit's
//! group membership (avida-core/include/public/avida/systematics/Unit.h,
//! Types.h; avida-core/source/systematics/Unit.cc).
//!
//! In C++ a `Systematics::Unit` (organism, parasite, deme placeholder) is a
//! reference-counted object the groups point back to. Here units belong to
//! the upper layers: classification takes the unit's data ([`UnitInfo`]),
//! and the unit keeps its [`GroupMembership`]. The `Unit` methods that only
//! touch the membership (`SystematicsGroup`, `AddClassification`,
//! `Reclassify`) are [`GroupMembership`] methods; those that reach the groups
//! (`SelfClassify`, `HandleGestation`, `~Unit`) are on
//! [`crate::Manager`].

use std::collections::BTreeMap;

use avida_genome::Genome;

use crate::arena::GroupHandle;
use crate::source::Source;

/// `Systematics::ClassificationHints` (`Apto::Map<String, String>`): only
/// looked up by key, so any map type will do.
pub type ClassificationHints = BTreeMap<String, String>;

/// `Systematics::RoleClassificationHints`: hints per role name.
pub type RoleClassificationHints = BTreeMap<String, ClassificationHints>;

/// Index of an arbiter in its [`crate::Manager`] (registration order); the
/// Rust form of `Systematics::RoleID` inside a membership.
pub type RoleIndex = usize;

/// The unit data classification reads: `UnitSource()`, `UnitGenome()` and
/// `Properties().Get("generation").IntValue()` (used when a unit founds a
/// genotype).
#[derive(Clone, Copy, Debug)]
pub struct UnitInfo<'a> {
    /// `UnitSource()`
    pub source: &'a Source,
    /// `UnitGenome()`
    pub genome: &'a Genome,
    /// `Properties().Get("generation").IntValue()`
    pub generation: i32,
}

/// The unit properties `Genotype::HandleUnitGestation` reads, each as
/// `Property::DoubleValue()` (`last_copied_size`, `last_executed_size`,
/// `last_gestation_time`, `last_metabolic_rate`, `last_fitness`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GestationInfo {
    /// `last_copied_size`
    pub last_copied_size: f64,
    /// `last_executed_size`
    pub last_executed_size: f64,
    /// `last_gestation_time`
    pub last_gestation_time: f64,
    /// `last_metabolic_rate`
    pub last_metabolic_rate: f64,
    /// `last_fitness`
    pub last_fitness: f64,
}

/// A group of some role: the Rust `Systematics::GroupPtr`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GroupRef {
    /// The arbiter (role) the group belongs to.
    pub role: RoleIndex,
    /// The group inside that arbiter.
    pub handle: GroupHandle,
}

/// `Systematics::GroupMembership` (`Apto::Array<GroupPtr>`): the groups a
/// unit is classified into, in classification order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupMembership {
    groups: Vec<GroupRef>,
}

impl GroupMembership {
    /// An empty membership (`new GroupMembership`).
    pub fn new() -> Self {
        Self::default()
    }

    /// The groups, in order.
    pub fn groups(&self) -> &[GroupRef] {
        &self.groups
    }

    /// `GetSize()`.
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// `GetSize() == 0`.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// `Unit::SystematicsGroup(role)`: the first group of that role.
    pub fn group(&self, role: RoleIndex) -> Option<GroupRef> {
        self.groups.iter().copied().find(|g| g.role == role)
    }

    /// `Unit::AddClassification(g)`: false (and no change) when a group of
    /// the same role is already present.
    pub fn add_classification(&mut self, g: GroupRef) -> bool {
        if self.groups.iter().any(|x| x.role == g.role) {
            return false;
        }
        self.groups.push(g);
        true
    }

    /// `Unit::Reclassify(g)`: replace the group of the same role, or append.
    /// QUIRK: like the C++, the replaced group is not told the unit left.
    pub fn reclassify(&mut self, g: GroupRef) {
        if let Some(x) = self.groups.iter_mut().find(|x| x.role == g.role) {
            *x = g;
            return;
        }
        self.groups.push(g);
    }

    /// `m_groups->Push(group)` without the role check, as `SelfClassify`
    /// does.
    pub(crate) fn push(&mut self, g: GroupRef) {
        self.groups.push(g);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::Arena;

    #[test]
    fn membership_ops() {
        let mut a: Arena<()> = Arena::default();
        let h1 = a.insert(());
        let h2 = a.insert(());
        let mut m = GroupMembership::new();
        assert!(m.add_classification(GroupRef {
            role: 0,
            handle: h1
        }));
        assert!(!m.add_classification(GroupRef {
            role: 0,
            handle: h2
        }));
        assert!(m.add_classification(GroupRef {
            role: 1,
            handle: h2
        }));
        assert_eq!(m.group(0).map(|g| g.handle), Some(h1));
        m.reclassify(GroupRef {
            role: 0,
            handle: h2,
        });
        assert_eq!(m.group(0).map(|g| g.handle), Some(h2));
        assert_eq!(m.len(), 2);
    }
}
