//! Generational arena for groups: the Rust stand-in for the
//! `Apto::SmartPtr<Genotype>` / `SmartPtr<Clade>` graph.
//!
//! A group lives in its arbiter's arena from creation until the C++ would
//! have dropped it from every list (`GenotypeArbiter::removeGenotype`
//! deleting its historic handle, `CladeArbiter::removeClade`). Handles
//! carry a generation, so a handle kept past that point (in a unit's
//! membership, or as the arbiter's coalescent genotype) never aliases a later
//! group that reuses the slot.

/// Stable handle of a group inside its arbiter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GroupHandle {
    index: u32,
    generation: u32,
}

impl GroupHandle {
    /// Slot index (for diagnostics).
    pub fn index(self) -> u32 {
        self.index
    }
}

#[derive(Clone, Debug)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// Arena of groups indexed by [`GroupHandle`].
#[derive(Clone, Debug)]
pub(crate) struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Arena {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }
}

impl<T> Arena<T> {
    pub(crate) fn insert(&mut self, value: T) -> GroupHandle {
        if let Some(i) = self.free.pop() {
            let s = &mut self.slots[i as usize];
            s.value = Some(value);
            GroupHandle {
                index: i,
                generation: s.generation,
            }
        } else {
            self.slots.push(Slot {
                generation: 0,
                value: Some(value),
            });
            GroupHandle {
                index: (self.slots.len() - 1) as u32,
                generation: 0,
            }
        }
    }

    pub(crate) fn get(&self, h: GroupHandle) -> Option<&T> {
        let s = self.slots.get(h.index as usize)?;
        if s.generation != h.generation {
            return None;
        }
        s.value.as_ref()
    }

    pub(crate) fn get_mut(&mut self, h: GroupHandle) -> Option<&mut T> {
        let s = self.slots.get_mut(h.index as usize)?;
        if s.generation != h.generation {
            return None;
        }
        s.value.as_mut()
    }

    pub(crate) fn contains(&self, h: GroupHandle) -> bool {
        self.get(h).is_some()
    }

    pub(crate) fn remove(&mut self, h: GroupHandle) -> Option<T> {
        let s = self.slots.get_mut(h.index as usize)?;
        if s.generation != h.generation || s.value.is_none() {
            return None;
        }
        let v = s.value.take();
        s.generation = s.generation.wrapping_add(1);
        self.free.push(h.index);
        v
    }

    pub(crate) fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_handles_do_not_alias() {
        let mut a: Arena<i32> = Arena::default();
        let h1 = a.insert(1);
        assert_eq!(a.remove(h1), Some(1));
        let h2 = a.insert(2);
        assert_eq!(h1.index(), h2.index());
        assert_ne!(h1, h2);
        assert_eq!(a.get(h1), None);
        assert_eq!(a.get(h2), Some(&2));
        assert!(!a.contains(h1));
        assert_eq!(a.len(), 1);
    }
}
