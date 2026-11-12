//! Data attached to groups (`Systematics::GroupData` subclasses,
//! `Group::AttachData` / `GetData`): `SexualAncestry`
//! (avida-core/source/systematics/SexualAncestry.cc) and the
//! `GenomeTestMetrics` holder (avida-core/source/systematics/GenomeTestMetrics.cc).
//!
//! The C++ keys attached data by type in a per-group map; a group carries at
//! most one object of each kind, so here they are two optional fields.

/// Port of `Systematics::SexualAncestry`: a group's id plus its parents'
/// (`[0]`, `[1]`) and grandparents' (`[2]..[5]`) ids, `-1` where unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SexualAncestry {
    id: i32,
    ancestor_ids: [i32; 6],
}

impl SexualAncestry {
    /// A record with no known ancestors (the constructor's start, and its
    /// result for groups without a `parents` property).
    pub fn without_parents(id: i32) -> Self {
        SexualAncestry {
            id,
            ancestor_ids: [-1; 6],
        }
    }

    /// The constructor's body once the first two parents' records are known:
    /// `[0] = p1`, `[2],[3]` = p1's parents; `[1] = p2`, `[4],[5]` = p2's.
    pub fn from_parents(id: i32, p1: Option<&SexualAncestry>, p2: Option<&SexualAncestry>) -> Self {
        let mut sa = Self::without_parents(id);
        if let Some(p) = p1 {
            sa.ancestor_ids[0] = p.id;
            sa.ancestor_ids[2] = p.ancestor_id(0);
            sa.ancestor_ids[3] = p.ancestor_id(1);
        }
        if let Some(p) = p2 {
            sa.ancestor_ids[1] = p.id;
            sa.ancestor_ids[4] = p.ancestor_id(0);
            sa.ancestor_ids[5] = p.ancestor_id(1);
        }
        sa
    }

    /// `GetID()`.
    pub fn id(&self) -> i32 {
        self.id
    }

    /// `GetAncestorID(idx)`, `idx < 6`; out of range gives -1 (the C++
    /// asserts).
    pub fn ancestor_id(&self, idx: usize) -> i32 {
        self.ancestor_ids.get(idx).copied().unwrap_or(-1)
    }

    /// `GetPhyloDistance(g)` with `g`'s record already obtained (the C++
    /// creates and attaches it first; see
    /// [`crate::GenotypeArbiter::sexual_ancestry`]): 0 same group, 1
    /// parent/child, 2 grandparent/grandchild/sibling, 3 uncle/nephew,
    /// 4 first cousins, otherwise 5.
    pub fn phylo_distance(&self, tsa: &SexualAncestry) -> i32 {
        let a = |i: usize| self.ancestor_id(i);
        let t = |i: usize| tsa.ancestor_id(i);
        if self.id == tsa.id() {
            return 0;
        }
        if self.id == t(0) || self.id == t(1) || tsa.id() == a(0) || tsa.id() == a(1) {
            return 1;
        }
        if self.id == t(2)
            || self.id == t(3)
            || self.id == t(4)
            || self.id == t(5)
            || tsa.id() == a(2)
            || tsa.id() == a(3)
            || tsa.id() == a(4)
            || tsa.id() == a(5)
            || a(0) == t(0)
            || a(0) == t(1)
            || a(1) == t(0)
            || a(1) == t(1)
        {
            return 2;
        }
        let uncle = (0..2).any(|i| (2..6).any(|j| a(i) == t(j)));
        let nephew = (0..2).any(|i| (2..6).any(|j| t(i) == a(j)));
        if uncle || nephew {
            return 3;
        }
        if (2..6).any(|i| (2..6).any(|j| a(i) == t(j))) {
            return 4;
        }
        5
    }
}

/// The data part of `Systematics::GenomeTestMetrics`: the outcome of running
/// a group's genome on a test CPU. The computation itself
/// (`cTestCPU::TestGenome`) belongs to the hardware layer and is supplied
/// as a closure to [`crate::GenotypeArbiter::genome_test_metrics`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GenomeTestMetrics {
    /// `IsViable()` (`test_info.IsViable()`)
    pub is_viable: bool,
    /// `GetFitness()` (`test_info.GetGenotypeFitness()`)
    pub fitness: f64,
    /// `GetColonyFitness()`
    pub colony_fitness: f64,
    /// `GetMerit()` (`phenotype.GetMerit().GetDouble()`)
    pub merit: f64,
    /// `GetLinesCopied()` (`phenotype.GetCopiedSize()`)
    pub copied_size: i32,
    /// `GetLinesExecuted()` (`phenotype.GetExecutedSize()`)
    pub executed_size: i32,
    /// `GetGestationTime()`
    pub gestation_time: i32,
    /// `GetTaskCounts()` (`phenotype.GetLastTaskCount()`)
    pub task_counts: Vec<i32>,
}

/// The `m_data` map of `Systematics::Group`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupData {
    /// `GetData<SexualAncestry>()`
    pub sexual_ancestry: Option<SexualAncestry>,
    /// `GetData<GenomeTestMetrics>()`
    pub test_metrics: Option<GenomeTestMetrics>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phylo_distances() {
        // 1 and 2 are founders; 3 = 1x2; 4 = 1x2; 5 = 3x9; 6 = 4x8; 7 = 5x6.
        let f1 = SexualAncestry::without_parents(1);
        let f2 = SexualAncestry::without_parents(2);
        let f8 = SexualAncestry::without_parents(8);
        let f9 = SexualAncestry::without_parents(9);
        let s3 = SexualAncestry::from_parents(3, Some(&f1), Some(&f2));
        let s4 = SexualAncestry::from_parents(4, Some(&f1), Some(&f2));
        let s5 = SexualAncestry::from_parents(5, Some(&s3), Some(&f9));
        let s6 = SexualAncestry::from_parents(6, Some(&s4), Some(&f8));
        let s7 = SexualAncestry::from_parents(7, Some(&s5), Some(&s6));
        assert_eq!(s5.ancestor_id(2), 1);
        assert_eq!(s3.phylo_distance(&s3), 0);
        assert_eq!(f1.phylo_distance(&s3), 1);
        assert_eq!(s3.phylo_distance(&f1), 1);
        assert_eq!(f1.phylo_distance(&s5), 2); // grandparent
        assert_eq!(s3.phylo_distance(&s4), 2); // siblings
        assert_eq!(s4.phylo_distance(&s5), 3); // uncle
        assert_eq!(s5.phylo_distance(&s6), 4); // first cousins
                                               // No -1 on either side at the parent level: unrelated.
        assert_eq!(s7.phylo_distance(&f2), 5);
        // Founders share the "-1" parent ids, which the C++ counts as
        // siblings (QUIRK kept).
        assert_eq!(f8.phylo_distance(&f9), 2);
        // Fully known, disjoint ancestries are unrelated.
        let pair = |id, a, b| {
            SexualAncestry::from_parents(
                id,
                Some(&SexualAncestry::without_parents(a)),
                Some(&SexualAncestry::without_parents(b)),
            )
        };
        let x = SexualAncestry::from_parents(20, Some(&pair(10, 11, 12)), Some(&pair(13, 14, 15)));
        let y = SexualAncestry::from_parents(30, Some(&pair(16, 17, 18)), Some(&pair(19, 21, 22)));
        assert_eq!(x.phylo_distance(&y), 5);
        assert_eq!(x.ancestor_id(5), 15);
    }
}
