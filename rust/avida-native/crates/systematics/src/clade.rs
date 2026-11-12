//! Port of `Avida::Systematics::Clade` and `CladeArbiter`
//! (avida-core/source/systematics/Clade.cc, CladeArbiter.cc; headers in
//! avida-core/include/private/avida/private/systematics/).
//!
//! Clades are named groups that units join by hint (`name`). The arbiter
//! keeps them in an `Apto::Map<Apto::String, CladePtr>`, whose bucket order
//! is the iteration order ([`crate::apto_map`]). `cWorld` registers only the
//! genotype arbiter, so no consistency test exercises this code.

use std::collections::HashMap;

use avida_apto::{Accumulator, DataFile};
use avida_config::string_util::as_int;

use crate::apto_map::AptoMap;
use crate::arena::{Arena, GroupHandle};
use crate::group_data::GroupData;
use crate::property::PropertyValue;
use crate::unit::ClassificationHints;

/// The clade property ids and descriptions (`Clade::Initialize`).
pub const CLADE_PROPERTIES: [(&str, &str); 2] =
    [("name", "Name"), ("total_organisms", "Total Organisms")];

/// The provided data names (after `systematics.<role>.`) and descriptions
/// (`CladeArbiter::setupProvidedData`).
pub const CLADE_PROVIDED: [(&str, &str); 5] = [
    ("total", "Total Number of Clades"),
    ("current", "Number of Current Clades"),
    ("ave_abundance", "Average Abundance"),
    ("stderr_abundance", "Abundance Standard Error"),
    ("var_abundance", "Abundance Variance"),
];

/// Port of `Systematics::Clade`.
#[derive(Clone, Debug)]
pub struct Clade {
    id: i32,
    a_refs: i32,
    p_refs: i32,
    data: GroupData,
    name: String,
    num_organisms: i32,
    last_num_organisms: i32,
    total_organisms: i32,
}

impl Clade {
    /// `Clade(mgr, id, name, create_empty)`: a clade created for a unit
    /// starts with that unit and its active reference.
    fn new(id: i32, name: &str, create_empty: bool) -> Self {
        let n = if create_empty { 0 } else { 1 };
        Clade {
            id,
            a_refs: n,
            p_refs: 0,
            data: GroupData::default(),
            name: name.to_string(),
            num_organisms: n,
            last_num_organisms: 0,
            total_organisms: n,
        }
    }

    /// `ID()`.
    pub fn id(&self) -> i32 {
        self.id
    }

    /// `Name()`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `NumUnits()`.
    pub fn num_units(&self) -> i32 {
        self.num_organisms
    }

    /// `m_last_num_organisms`.
    pub fn last_num_units(&self) -> i32 {
        self.last_num_organisms
    }

    /// `m_total_organisms`.
    pub fn total_units(&self) -> i32 {
        self.total_organisms
    }

    /// `Depth()`: always 0.
    pub fn depth(&self) -> i32 {
        0
    }

    /// `ActiveReferenceCount()`.
    pub fn active_reference_count(&self) -> i32 {
        self.a_refs
    }

    /// `PassiveReferenceCount()`.
    pub fn passive_reference_count(&self) -> i32 {
        self.p_refs
    }

    /// Attached data.
    pub fn data(&self) -> &GroupData {
        &self.data
    }

    /// `Properties().Has(name)`.
    pub fn has_property(&self, name: &str) -> bool {
        CLADE_PROPERTIES.iter().any(|(n, _)| *n == name)
    }

    /// `Properties().Get(name)`.
    pub fn property(&self, name: &str) -> PropertyValue {
        match name {
            "name" => PropertyValue::Str(self.name.clone()),
            "total_organisms" => PropertyValue::Int(self.total_organisms),
            _ => PropertyValue::Null,
        }
    }

    /// `LegacySave(df)`: just the name.
    fn legacy_save(&self, df: &mut DataFile) {
        df.write_with_format(self.name.as_str(), "Clade Name", "name");
    }
}

/// Port of `Systematics::CladeArbiter`.
#[derive(Clone, Debug)]
pub struct CladeArbiter {
    role: String,
    /// `m_clades`. A `None` value is the `CladePtr(NULL)` that
    /// `GroupWithName` inserts for an unknown name.
    clades: AptoMap<String, Option<GroupHandle>>,
    arena: Arena<Clade>,
    next_id: i32,
    cur_update: i32,
    /// QUIRK: uninitialised in C++; 0 here.
    tot_clades: i32,
    ave_abundance: f64,
    stderr_abundance: f64,
    var_abundance: f64,
}

impl CladeArbiter {
    /// `CladeArbiter(world, role)`.
    ///
    /// QUIRK: the C++ registers the provided values with the data manager
    /// under the bare names (`"total"`, ...) but stores them under
    /// `systematics.<role>.<name>`; [`CladeArbiter::provided_value`] uses
    /// the full ids.
    pub fn new(role: &str) -> Self {
        CladeArbiter {
            role: role.to_string(),
            clades: AptoMap::new(),
            arena: Arena::default(),
            next_id: 1,
            cur_update: -1,
            tot_clades: 0,
            ave_abundance: 0.0,
            stderr_abundance: 0.0,
            var_abundance: 0.0,
        }
    }

    /// `Role()`.
    pub fn role(&self) -> &str {
        &self.role
    }

    /// `m_cur_update`.
    pub fn current_update(&self) -> i32 {
        self.cur_update
    }

    /// `m_tot_clades`.
    pub fn total_clades(&self) -> i32 {
        self.tot_clades
    }

    /// The clade behind a handle.
    pub fn clade(&self, g: GroupHandle) -> Option<&Clade> {
        self.arena.get(g)
    }

    /// `ClassifyNewUnit(u, hints)`: `name` joins or founds the named
    /// clade; `id` returns that clade without adding the unit (QUIRK: no
    /// unit count or reference is taken, yet the unit's death will release
    /// one); no hint gives no clade.
    pub fn classify_new_unit(
        &mut self,
        hints: Option<&ClassificationHints>,
    ) -> Option<GroupHandle> {
        let hints = hints?;
        if let Some(group_name) = hints.get("name") {
            let key = group_name.clone();
            let mut grp = self.clades.find(&key).copied().flatten();
            if let Some(g) = grp {
                self.notify_new_unit(g);
            }
            if grp.is_none() {
                let g = self
                    .arena
                    .insert(Clade::new(self.next_id, group_name, false));
                self.next_id = self.next_id.wrapping_add(1);
                self.clades.set(&key, Some(g));
                self.tot_clades = self.tot_clades.wrapping_add(1);
                grp = Some(g);
            }
            return grp;
        }
        if let Some(id) = hints.get("id") {
            return self.group(as_int(id));
        }
        None
    }

    /// `Clade::ClassifyNewUnit(u, parents)`: the unit always joins its
    /// parent's clade.
    pub fn group_classify_new_unit(&mut self, g: GroupHandle) -> Option<GroupHandle> {
        self.arena.get(g)?;
        self.notify_new_unit(g);
        Some(g)
    }

    /// `Clade::NotifyNewUnit(u)` (same body as `Clade::ClassifyNewUnit`).
    fn notify_new_unit(&mut self, g: GroupHandle) {
        let Some(c) = self.arena.get_mut(g) else {
            return;
        };
        c.total_organisms = c.total_organisms.wrapping_add(1);
        c.num_organisms = c.num_organisms.wrapping_add(1);
        let n = c.num_organisms;
        self.adjust_clade(g, n.wrapping_sub(1), n);
        if let Some(c) = self.arena.get_mut(g) {
            c.a_refs = c.a_refs.wrapping_add(1);
        }
    }

    /// `Clade::RemoveUnit()`.
    pub fn remove_unit(&mut self, g: GroupHandle) {
        let Some(c) = self.arena.get_mut(g) else {
            return;
        };
        c.a_refs = c.a_refs.wrapping_sub(1);
        c.num_organisms = c.num_organisms.wrapping_sub(1);
        let n = c.num_organisms;
        self.adjust_clade(g, n.wrapping_add(1), n);
    }

    /// `Group::AddActiveReference()`.
    pub fn add_active_reference(&mut self, g: GroupHandle) {
        if let Some(c) = self.arena.get_mut(g) {
            c.a_refs = c.a_refs.wrapping_add(1);
        }
    }

    /// `Clade::RemoveActiveReference()`.
    pub fn remove_active_reference(&mut self, g: GroupHandle) {
        let Some(c) = self.arena.get_mut(g) else {
            return;
        };
        c.a_refs = c.a_refs.wrapping_sub(1);
        if c.a_refs == 0 {
            let n = c.num_organisms;
            self.adjust_clade(g, n, 0);
        }
    }

    /// `Group::AddPassiveReference()`.
    pub fn add_passive_reference(&mut self, g: GroupHandle) {
        if let Some(c) = self.arena.get_mut(g) {
            c.p_refs = c.p_refs.wrapping_add(1);
        }
    }

    /// `Group::RemovePassiveReference()`.
    pub fn remove_passive_reference(&mut self, g: GroupHandle) {
        if let Some(c) = self.arena.get_mut(g) {
            c.p_refs = c.p_refs.wrapping_sub(1);
        }
    }

    /// `AdjustClade(clade, old_size, new_size)`.
    fn adjust_clade(&mut self, g: GroupHandle, _old_size: i32, new_size: i32) {
        if new_size == 0 && self.arena.get(g).is_some_and(|c| c.a_refs == 0) {
            self.remove_clade(g);
        }
    }

    /// `removeClade(clade)`: drop it from the map unless referenced. The
    /// clade leaves the arena once it is unreferenced and no longer in the
    /// map (`HashBTree::Remove` can slip and drop a different entry; see
    /// [`crate::apto_map`]).
    fn remove_clade(&mut self, g: GroupHandle) {
        let Some(c) = self.arena.get(g) else {
            return;
        };
        if c.a_refs != 0 || c.p_refs != 0 {
            return;
        }
        let name = c.name.clone();
        self.clades.remove(&name);
        if !self.clades.values().any(|v| *v == Some(g)) {
            self.arena.remove(g);
        }
    }

    /// `Group(id)`: map value order.
    pub fn group(&self, id: i32) -> Option<GroupHandle> {
        self.clades
            .values()
            .flatten()
            .copied()
            .find(|&g| self.arena.get(g).is_some_and(|c| c.id == id))
    }

    /// `GroupWithName(name)`. QUIRK: an unknown name inserts a NULL entry
    /// (non-const `GetWithDefault`), which counts in `current`; the C++
    /// would crash on it when iterating, here it is skipped.
    pub fn group_with_name(&mut self, name: &str) -> Option<GroupHandle> {
        *self.clades.get_with_default(&name.to_string(), None)
    }

    /// `Begin()` / `CladeIterator`: the clades in map value order.
    pub fn iter_groups(&self) -> Vec<GroupHandle> {
        self.clades.values().flatten().copied().collect()
    }

    /// `PerformUpdate(ctx, current_update)`.
    pub fn perform_update(&mut self, current_update: i32) {
        self.cur_update = current_update.wrapping_add(1);
        for g in self.iter_groups() {
            if let Some(c) = self.arena.get_mut(g) {
                c.last_num_organisms = c.num_organisms;
            }
        }
    }

    /// `UpdateProvidedValues(current_update)`.
    pub fn update_provided_values(&mut self, _current_update: i32) {
        let mut sum_abundance: Accumulator<f64> = Accumulator::new();
        for g in self.iter_groups() {
            if let Some(c) = self.arena.get(g) {
                sum_abundance.add(f64::from(c.num_organisms));
            }
        }
        self.ave_abundance = sum_abundance.mean();
        self.stderr_abundance = sum_abundance.std_error();
        self.var_abundance = sum_abundance.variance();
    }

    /// `Provides()`.
    pub fn provided_ids(&self) -> Vec<String> {
        CLADE_PROVIDED
            .iter()
            .map(|(n, _)| format!("systematics.{}.{}", self.role, n))
            .collect()
    }

    /// `GetProvidedValue(data_id)`. QUIRK: the C++ binds `current` to a
    /// temporary copy of `m_clades.GetSize()` (a dangling reference); the
    /// live size is returned here.
    pub fn provided_value(&self, data_id: &str) -> Option<PropertyValue> {
        let prefix = format!("systematics.{}.", self.role);
        Some(match data_id.strip_prefix(&prefix)? {
            "total" => PropertyValue::Int(self.tot_clades),
            "current" => PropertyValue::Int(self.clades.len()),
            "ave_abundance" => PropertyValue::Double(self.ave_abundance),
            "stderr_abundance" => PropertyValue::Double(self.stderr_abundance),
            "var_abundance" => PropertyValue::Double(self.var_abundance),
            _ => return None,
        })
    }

    /// `DescribeProvidedValue(data_id)`.
    pub fn describe_provided_value(&self, data_id: &str) -> Option<&'static str> {
        let prefix = format!("systematics.{}.", self.role);
        let name = data_id.strip_prefix(&prefix)?;
        CLADE_PROVIDED
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, d)| *d)
    }

    /// `Clade::LegacySave(df)` for one clade (the row is not ended).
    pub fn save_clade(&self, g: GroupHandle, df: &mut DataFile) -> bool {
        match self.arena.get(g) {
            Some(c) => {
                c.legacy_save(df);
                true
            }
            None => false,
        }
    }

    /// `LegacySave(df)`: the base `Arbiter::LegacySave`, which writes
    /// nothing and returns false.
    pub fn legacy_save(&self, _df: &mut DataFile) -> bool {
        false
    }

    /// `LegacyLoad(props)`: the clade named by `name` (created empty when
    /// new), or `None` without a name.
    pub fn legacy_load(&mut self, props: &HashMap<String, String>) -> Option<GroupHandle> {
        let group_name = props.get("name").cloned().unwrap_or_default();
        if group_name.is_empty() {
            return None;
        }
        let mut grp = *self.clades.get_with_default(&group_name, None);
        if grp.is_none() {
            let g = self
                .arena
                .insert(Clade::new(self.next_id, &group_name, true));
            self.next_id = self.next_id.wrapping_add(1);
            self.clades.set(&group_name, Some(g));
            self.tot_clades = self.tot_clades.wrapping_add(1);
            grp = Some(g);
        }
        grp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_hint(n: &str) -> ClassificationHints {
        let mut h = ClassificationHints::new();
        h.insert("name".into(), n.into());
        h
    }

    #[test]
    fn clades_by_name() {
        let mut a = CladeArbiter::new("clade");
        assert_eq!(a.classify_new_unit(None), None);
        let x = a.classify_new_unit(Some(&name_hint("alpha"))).unwrap();
        let y = a.classify_new_unit(Some(&name_hint("beta"))).unwrap();
        assert_eq!(a.classify_new_unit(Some(&name_hint("alpha"))), Some(x));
        assert_eq!(a.clade(x).unwrap().num_units(), 2);
        assert_eq!(a.clade(x).unwrap().active_reference_count(), 2);
        assert_eq!((a.clade(x).unwrap().id(), a.clade(y).unwrap().id()), (1, 2));
        assert_eq!(a.total_clades(), 2);
        // Child of a clade member always joins.
        assert_eq!(a.group_classify_new_unit(y), Some(y));
        assert_eq!(a.clade(y).unwrap().num_units(), 2);

        // "id" hint: the group, without counting the unit.
        let mut idh = ClassificationHints::new();
        idh.insert("id".into(), "2".into());
        assert_eq!(a.classify_new_unit(Some(&idh)), Some(y));
        assert_eq!(a.clade(y).unwrap().num_units(), 2);

        a.update_provided_values(0);
        assert_eq!(
            a.provided_value("systematics.clade.ave_abundance"),
            Some(PropertyValue::Double(2.0))
        );
        assert_eq!(
            a.provided_value("systematics.clade.current"),
            Some(PropertyValue::Int(2))
        );

        a.remove_unit(y);
        a.remove_unit(y);
        assert!(a.clade(y).is_none());
        assert_eq!(a.iter_groups(), vec![x]);
        assert_eq!(a.group(2), None);
        assert_eq!(
            a.clade(x).unwrap().property("name"),
            PropertyValue::Str("alpha".into())
        );
    }

    #[test]
    fn legacy_load_creates_empty_clades() {
        let mut a = CladeArbiter::new("clade");
        let props: HashMap<String, String> = [("name".to_string(), "gamma".to_string())].into();
        let g = a.legacy_load(&props).unwrap();
        assert_eq!(a.clade(g).unwrap().num_units(), 0);
        assert_eq!(a.clade(g).unwrap().active_reference_count(), 0);
        assert_eq!(a.legacy_load(&props), Some(g));
        assert_eq!(a.legacy_load(&HashMap::new()), None);
        assert_eq!(a.group_classify_new_unit(g), Some(g));
        assert_eq!(a.clade(g).unwrap().total_units(), 1);
        // GroupWithName inserts a NULL entry for unknown names.
        assert_eq!(a.group_with_name("delta"), None);
        assert_eq!(
            a.provided_value("systematics.clade.current"),
            Some(PropertyValue::Int(2))
        );
        let mut df = DataFile::from_writer("x", Box::new(std::io::sink()));
        assert!(a.save_clade(g, &mut df));
    }
}
