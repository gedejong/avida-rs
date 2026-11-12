//! Port of `cResourceLib` (avida-core/source/main/cResourceLib.{h,cc}).

use crate::resource::Resource;
use crate::resource_history::ResourceHistory;
use std::cell::OnceCell;

/// `cResourceLib`: all resources in definition order; a resource's id is its
/// position.
#[derive(Debug, Clone, Default)]
pub struct ResourceLib {
    resources: Vec<Resource>,
    /// `m_initial_levels` (computed lazily by a const method in C++).
    initial_levels: OnceCell<ResourceHistory>,
    num_deme_resources: i32,
}

impl ResourceLib {
    /// `cResourceLib()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `GetSize()`.
    pub fn size(&self) -> i32 {
        self.resources.len() as i32
    }

    /// All resources, indexed by id.
    pub fn resources(&self) -> &[Resource] {
        &self.resources
    }

    /// `AddResource(name)`: appends a new resource and returns its id.
    /// `None` (C++ `NULL`) once the initial levels have been computed.
    pub fn add_resource(&mut self, name: &str) -> Option<usize> {
        if self.initial_levels.get().is_some() {
            return None;
        }
        let id = self.resources.len();
        self.resources.push(Resource::new(name, id as i32));
        Some(id)
    }

    /// `GetResource(const cString&)`: id of the first resource with that
    /// name. QUIRK: an unknown name prints `Error: Unknown resource 'NAME'.`
    /// to stderr, as the C++ does.
    pub fn get_resource_id(&self, name: &str) -> Option<usize> {
        let r = self.find(name);
        if r.is_none() {
            eprintln!("Error: Unknown resource '{}'.", name);
        }
        r
    }

    /// Silent lookup by name (`DoesResourceExist` with the id).
    pub fn find(&self, name: &str) -> Option<usize> {
        self.resources.iter().position(|r| r.name() == name)
    }

    /// `GetResource(int id)`.
    pub fn resource(&self, id: usize) -> &Resource {
        &self.resources[id]
    }

    /// `GetResource(int id)` (mutable).
    pub fn resource_mut(&mut self, id: usize) -> &mut Resource {
        &mut self.resources[id]
    }

    /// `GetResource(name)` without the stderr message.
    pub fn resource_by_name(&self, name: &str) -> Option<&Resource> {
        self.find(name).map(|i| &self.resources[i])
    }

    /// `DoesResourceExist(name)`.
    pub fn does_resource_exist(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// `GetInitialResourceLevels()`: on first call records every resource's
    /// `initial` as the entry for update 0; after that, no resources can be
    /// added.
    pub fn initial_resource_levels(&self) -> &ResourceHistory {
        self.initial_levels.get_or_init(|| {
            let levels: Vec<f64> = self.resources.iter().map(|r| r.initial).collect();
            let mut h = ResourceHistory::new();
            h.add_entry(0, &levels);
            h
        })
    }

    /// Number of deme resources indexed so far (`m_num_deme_resources`).
    pub fn num_deme_resources(&self) -> i32 {
        self.num_deme_resources
    }

    /// `SetResourceIndex(res)`: deme resources are numbered among deme
    /// resources; any other resource gets `size - 1 - num_deme_resources`
    /// (evaluated now). A resource whose index is already set keeps it, but
    /// the deme counter still advances (as in C++).
    pub fn set_resource_index(&mut self, id: usize) {
        if self.resources[id].deme_resource {
            let n = self.num_deme_resources;
            self.resources[id].set_index(n);
            self.num_deme_resources += 1;
        } else {
            let idx = self.resources.len() as i32 - 1 - self.num_deme_resources;
            self.resources[id].set_index(idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_index() {
        let mut lib = ResourceLib::new();
        let a = lib.add_resource("a").unwrap();
        lib.set_resource_index(a);
        let b = lib.add_resource("b").unwrap();
        lib.resource_mut(b).deme_resource = true;
        lib.set_resource_index(b);
        let c = lib.add_resource("c").unwrap();
        lib.set_resource_index(c);
        assert_eq!(lib.resource(a).index(), 0);
        assert_eq!(lib.resource(b).index(), 0);
        assert_eq!(lib.resource(c).index(), 1);
        assert_eq!(lib.num_deme_resources(), 1);
        assert_eq!(lib.find("c"), Some(2));
        assert!(lib.does_resource_exist("b"));
        assert!(!lib.does_resource_exist("z"));
        lib.resource_mut(a).initial = 5.0;
        let h = lib.initial_resource_levels();
        assert_eq!(h.entries()[0].values, vec![5.0, 0.0, 0.0]);
        assert!(lib.add_resource("d").is_none());
    }
}
