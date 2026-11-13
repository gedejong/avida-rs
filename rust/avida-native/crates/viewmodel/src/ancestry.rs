//! Placing genotypes on a line so that relatives sit close together, for
//! [`crate::LayerId::Ancestry`].
//!
//! The phylogeny of the living genotypes is laid out depth first: a
//! genotype comes first, then its descendant clades from the oldest
//! (lowest id) to the youngest. Neighbours on that line are set apart by
//! [`gap`] of the smaller of the two, counted in organisms, which grows
//! faster than the size itself. Two organisms are therefore at least as
//! far apart as the smaller of the two clades that split at their last
//! common ancestor warrants: a lone mutant keeps almost the colour of its
//! parent (and a thousand lone mutants together take little of the
//! range), two large families get clearly different colours.

use std::collections::HashMap;

/// The distance between two neighbouring clades whose smaller one has
/// `smaller` organisms: `smaller ^ 1.5`.
pub fn gap(smaller: u64) -> f64 {
    let n = smaller as f64;
    n * n.sqrt()
}

/// A genotype in the phylogeny.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// Genotype id.
    pub id: i32,
    /// Its parent's id; a parent that is not among the nodes makes this a
    /// root.
    pub parent: i32,
    /// Its living organisms (0 for an extinct ancestor).
    pub organisms: u32,
}

/// A family: a clade of the phylogeny without the clades that were split
/// off from it as families of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Family {
    /// The genotype at the root of the clade.
    pub root: i32,
    /// Its living organisms.
    pub organisms: u32,
    /// Its colour slot, below [`FAMILY_SLOTS`]. A family keeps its slot
    /// while the older families stay.
    pub slot: usize,
}

/// Where a genotype sits: its family and its place inside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    /// Index into [`Layout::families`]; `None` when there are more
    /// founders than [`FAMILY_SLOTS`] and this one is among the small.
    pub family: Option<usize>,
    /// Position on the line of the whole population, in `[0, 1]`.
    pub position: f32,
    /// Position among the genotypes of its family, in `[0, 1]` (1 for a
    /// family of one genotype).
    pub within: f32,
}

/// The phylogeny of the living population, laid out and cut into
/// families.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    /// Every genotype that has organisms.
    pub places: HashMap<i32, Place>,
    /// The families, largest first.
    pub families: Vec<Family>,
}

/// At most this many families.
pub const FAMILY_SLOTS: usize = 7;

/// A clade becomes a family of its own only if both it and what remains
/// of the family it leaves hold at least this share of the population
/// (and at least two organisms).
pub const MIN_FAMILY_SHARE: f64 = 0.02;

impl Layout {
    /// The value [`crate::LayerId::Ancestry`] holds for a genotype: its
    /// family's colour slot plus its place within the family
    /// ([`FAMILY_SLOTS`] plus the place when it has no family). NaN for a
    /// genotype without organisms.
    pub fn value(&self, genotype: i32) -> f32 {
        match self.places.get(&genotype) {
            None => f32::NAN,
            Some(p) => {
                let slot = p.family.map_or(FAMILY_SLOTS, |f| self.families[f].slot);
                slot as f32 + p.within.clamp(0.0, 1.0) * WITHIN_SPAN
            }
        }
    }
}

/// The share of a unit a place within a family takes in [`Layout::value`]
/// (below 1, so the slot is the integer part).
const WITHIN_SPAN: f32 = 0.875;

/// The colour slot (`None`: no family) and the place within the family
/// that a [`Layout::value`] stands for. `None` for NaN.
pub fn decode(value: f32) -> Option<(Option<usize>, f32)> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let slot = value.floor() as usize;
    let within = (value.fract() / WITHIN_SPAN).clamp(0.0, 1.0);
    Some(((slot < FAMILY_SLOTS).then_some(slot), within))
}

/// The position in `[0, 1]` of every genotype that has organisms.
pub fn tree_positions(nodes: &[Node]) -> HashMap<i32, f32> {
    layout(nodes)
        .places
        .into_iter()
        .map(|(id, p)| (id, p.position))
        .collect()
}

/// Lays out the phylogeny and cuts it into families.
///
/// The cut is greedy: starting from one family per founder, the clade
/// whose removal splits a family most evenly (the smaller part is
/// largest) becomes a family, until there are [`FAMILY_SLOTS`] or no
/// split leaves both parts above [`MIN_FAMILY_SHARE`]. Of clades that
/// split equally well the one nearest the root is taken, so a family's
/// root does not move while organisms come and go below it.
pub fn layout(nodes: &[Node]) -> Layout {
    let index: HashMap<i32, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let parent: Vec<Option<usize>> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| index.get(&n.parent).copied().filter(|&p| p != i))
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    let mut roots: Vec<usize> = Vec::new();
    for (i, p) in parent.iter().enumerate() {
        match p {
            Some(p) => children[*p].push(i),
            None => roots.push(i),
        }
    }
    for c in children.iter_mut() {
        c.sort_unstable_by_key(|&i| nodes[i].id);
    }
    roots.sort_unstable_by_key(|&i| nodes[i].id);

    // Parents before children. A node on a parent cycle is reached from no
    // root and gets no position.
    let mut order: Vec<usize> = Vec::with_capacity(nodes.len());
    let mut stack: Vec<usize> = roots.iter().rev().copied().collect();
    while let Some(i) = stack.pop() {
        order.push(i);
        stack.extend(children[i].iter().rev());
    }

    // Organisms and laid-out length of each clade, children first.
    let mut size = vec![0u64; nodes.len()];
    let mut length = vec![0f64; nodes.len()];
    for &i in order.iter().rev() {
        let own = u64::from(nodes[i].organisms);
        let mut total = own;
        let mut len = 0.0;
        let mut prev = own;
        for &c in &children[i] {
            if size[c] == 0 {
                continue;
            }
            if prev > 0 {
                len += gap(prev.min(size[c]));
            }
            len += length[c];
            total += size[c];
            prev = size[c];
        }
        size[i] = total;
        length[i] = len;
    }

    // Offsets, parents first; the roots are siblings of one another.
    let mut start = vec![0f64; nodes.len()];
    let mut at = 0.0;
    let mut prev = 0u64;
    for &r in &roots {
        if size[r] == 0 {
            continue;
        }
        if prev > 0 {
            at += gap(prev.min(size[r]));
        }
        start[r] = at;
        at += length[r];
        prev = size[r];
    }
    let whole = at;
    let mut out: HashMap<i32, f32> = HashMap::new();
    for &i in &order {
        let own = u64::from(nodes[i].organisms);
        if own > 0 {
            // One living genotype alone has no line to sit on.
            let pos = if whole > 0.0 { start[i] / whole } else { 0.5 };
            out.insert(nodes[i].id, pos as f32);
        }
        let mut at = start[i];
        let mut prev = own;
        for &c in &children[i] {
            if size[c] == 0 {
                continue;
            }
            if prev > 0 {
                at += gap(prev.min(size[c]));
            }
            start[c] = at;
            at += length[c];
            prev = size[c];
        }
    }

    // Families.
    let total: u64 = roots.iter().map(|&r| size[r]).sum();
    let mut eff = size.clone();
    let mut fam: Vec<Option<usize>> = vec![None; nodes.len()];
    let mut fam_roots: Vec<usize> = Vec::new();
    let mut by_size: Vec<usize> = roots.iter().copied().filter(|&r| size[r] > 0).collect();
    by_size.sort_by_key(|&r| (std::cmp::Reverse(size[r]), nodes[r].id));
    for &r in by_size.iter().take(FAMILY_SLOTS) {
        fam[r] = Some(fam_roots.len());
        fam_roots.push(r);
    }
    for &i in &order {
        if let Some(p) = parent[i] {
            fam[i] = fam[p];
        }
    }
    let threshold = ((MIN_FAMILY_SHARE * total as f64).ceil() as u64).max(2);
    while fam_roots.len() < FAMILY_SLOTS {
        let mut best: Option<(u64, usize)> = None;
        for &i in &order {
            let Some(f) = fam[i] else { continue };
            let root = fam_roots[f];
            if i == root {
                continue;
            }
            let v = eff[i].min(eff[root] - eff[i]);
            if best.is_none_or(|(b, _)| v > b) {
                best = Some((v, i));
            }
        }
        let Some((v, cut)) = best else { break };
        if v < threshold {
            break;
        }
        let old = fam[cut];
        let moved = eff[cut];
        let mut p = parent[cut];
        while let Some(a) = p {
            if fam[a] != old {
                break;
            }
            eff[a] -= moved;
            p = parent[a];
        }
        let new = Some(fam_roots.len());
        fam_roots.push(cut);
        let mut stack = vec![cut];
        while let Some(i) = stack.pop() {
            fam[i] = new;
            stack.extend(children[i].iter().filter(|&&c| fam[c] == old));
        }
    }

    // Slots by the root's id, older families first, so that a family
    // founded later never takes the colour of one that is still there.
    let mut by_age: Vec<usize> = (0..fam_roots.len()).collect();
    by_age.sort_by_key(|&f| nodes[fam_roots[f]].id);
    let mut taken = [false; FAMILY_SLOTS];
    let mut slot_of = vec![0usize; fam_roots.len()];
    for f in by_age {
        let mut slot = nodes[fam_roots[f]].id.rem_euclid(FAMILY_SLOTS as i32) as usize;
        while taken[slot] {
            slot = (slot + 1) % FAMILY_SLOTS;
        }
        taken[slot] = true;
        slot_of[f] = slot;
    }
    // Largest first.
    let mut rank: Vec<usize> = (0..fam_roots.len()).collect();
    rank.sort_by_key(|&f| (std::cmp::Reverse(eff[fam_roots[f]]), nodes[fam_roots[f]].id));
    let mut index_of = vec![0usize; fam_roots.len()];
    for (k, &f) in rank.iter().enumerate() {
        index_of[f] = k;
    }
    let families: Vec<Family> = rank
        .iter()
        .map(|&f| Family {
            root: nodes[fam_roots[f]].id,
            organisms: eff[fam_roots[f]].min(u64::from(u32::MAX)) as u32,
            slot: slot_of[f],
        })
        .collect();

    // The span of each family on the line.
    let mut span = vec![(f32::INFINITY, f32::NEG_INFINITY); families.len()];
    for (i, n) in nodes.iter().enumerate() {
        if let (Some(f), Some(&p)) = (fam[i], out.get(&n.id)) {
            let s = &mut span[index_of[f]];
            *s = (s.0.min(p), s.1.max(p));
        }
    }
    let mut places = HashMap::with_capacity(out.len());
    for (i, n) in nodes.iter().enumerate() {
        let Some(&position) = out.get(&n.id) else {
            continue;
        };
        if places.contains_key(&n.id) {
            continue;
        }
        let family = fam[i].map(|f| index_of[f]);
        let within = match family.map(|k| span[k]) {
            Some((lo, hi)) if hi > lo => (position - lo) / (hi - lo),
            _ => 1.0,
        };
        places.insert(
            n.id,
            Place {
                family,
                position,
                within,
            },
        );
    }
    Layout { places, families }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: i32, parent: i32, organisms: u32) -> Node {
        Node {
            id,
            parent,
            organisms,
        }
    }

    #[test]
    fn relatives_are_closer_than_strangers() {
        // An extinct founder with two families of 40; each family is a
        // genotype with a lone mutant.
        let nodes = [
            node(1, -1, 0),
            node(2, 1, 39),
            node(3, 2, 1),
            node(4, 1, 39),
            node(5, 4, 1),
        ];
        let p = tree_positions(&nodes);
        assert_eq!(p.len(), 4);
        let d = |a: i32, b: i32| (p[&a] - p[&b]).abs();
        assert!(d(2, 3) < 0.3 * d(2, 4), "{p:?}");
        assert!(d(4, 5) < 0.3 * d(3, 4), "{p:?}");
        assert!(p.values().all(|v| (0.0..=1.0).contains(v)));
        // Depth first, oldest clade first.
        assert!(p[&2] < p[&3] && p[&3] < p[&4] && p[&4] < p[&5]);
    }

    #[test]
    fn a_single_genotype_sits_in_the_middle() {
        let p = tree_positions(&[node(7, 3, 10)]);
        assert_eq!(p[&7], 0.5);
    }

    #[test]
    fn several_founders_and_a_long_trunk() {
        // 5000 extinct ancestors in a chain must not overflow the stack.
        let mut nodes: Vec<Node> = (0..5000).map(|i| node(i, i - 1, 0)).collect();
        nodes.push(node(5000, 4999, 3));
        nodes.push(node(9000, -1, 3));
        let p = tree_positions(&nodes);
        assert_eq!(p.len(), 2);
        assert!(p[&5000] < p[&9000]);
    }

    #[test]
    fn nothing_alive_and_cycles_give_no_positions() {
        assert!(tree_positions(&[node(1, -1, 0)]).is_empty());
        assert!(tree_positions(&[node(1, 2, 4), node(2, 1, 4)]).is_empty());
    }

    /// A founder with 100 organisms of its own, two clades of 500 and
    /// one of 3.
    fn two_big_clades() -> Vec<Node> {
        vec![
            node(1, -1, 100),
            node(2, 1, 0),
            node(3, 2, 495),
            node(4, 3, 5),
            node(5, 1, 500),
            node(6, 1, 3),
        ]
    }

    #[test]
    fn big_clades_become_families_and_small_ones_stay() {
        let l = layout(&two_big_clades());
        // Genotype 2 heads the clade of 3 and 4 (the cut nearest the
        // root); 5 is the other; 6 is below the threshold and stays with
        // the founder.
        let roots: Vec<(i32, u32)> = l.families.iter().map(|f| (f.root, f.organisms)).collect();
        assert_eq!(roots, vec![(2, 500), (5, 500), (1, 103)]);
        let family = |id: i32| l.families[l.places[&id].family.unwrap()].root;
        assert_eq!(
            [1, 3, 4, 5, 6].map(family),
            [1, 2, 2, 5, 1],
            "{:?}",
            l.places
        );
        let mut slots: Vec<usize> = l.families.iter().map(|f| f.slot).collect();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), 3);
        let total: u32 = l.families.iter().map(|f| f.organisms).sum();
        assert_eq!(total, 1103);
    }

    #[test]
    fn a_family_keeps_its_slot_when_a_younger_one_appears_or_sizes_change() {
        let before = layout(&two_big_clades());
        let mut nodes = two_big_clades();
        nodes[5].organisms = 600; // genotype 6 grows into a family
        nodes[4].organisms = 200;
        let after = layout(&nodes);
        assert_eq!(after.families.len(), 4);
        for f in &before.families {
            let same = after.families.iter().find(|g| g.root == f.root).unwrap();
            assert_eq!(same.slot, f.slot, "family of {}", f.root);
        }
    }

    #[test]
    fn more_founders_than_slots_leaves_the_small_ones_without_a_family() {
        let nodes: Vec<Node> = (0..10).map(|i| node(i, -1, 10 + i as u32)).collect();
        let l = layout(&nodes);
        assert_eq!(l.families.len(), FAMILY_SLOTS);
        assert_eq!(l.places.values().filter(|p| p.family.is_none()).count(), 3);
        assert!(l.places[&0].family.is_none() && l.places[&9].family.is_some());
    }

    #[test]
    fn values_carry_the_slot_and_the_place() {
        let l = layout(&two_big_clades());
        for (&id, place) in &l.places {
            let (slot, within) = decode(l.value(id)).unwrap();
            assert_eq!(slot, place.family.map(|f| l.families[f].slot));
            assert!((within - place.within).abs() < 1e-4);
        }
        assert!(l.value(99).is_nan() && decode(f32::NAN).is_none());
        assert_eq!(decode(FAMILY_SLOTS as f32 + 0.5).unwrap().0, None);
    }

    #[test]
    fn within_spans_each_family() {
        let l = layout(&two_big_clades());
        assert!(l.places.values().all(|p| (0.0..=1.0).contains(&p.within)));
        // Genotypes 3 and 4 are the two ends of their family.
        assert_eq!((l.places[&3].within, l.places[&4].within), (0.0, 1.0));
        assert_eq!(l.places[&5].within, 1.0);
    }
}
