//! Port of the cell-topology builders (avida-core/source/tools/cTopology.h)
//! and of the `tList<cPopulationCell>` connection list they fill.
//!
//! A connection list holds global cell ids. The first element is the cell
//! being faced; `tList::Push` inserts at the FRONT, which fixes the order the
//! builders produce.

use avida_apto::rng::AvidaRng;
use std::collections::{BTreeSet, VecDeque};

/// C++: `nGeometry`.
pub mod geometry {
    pub const GLOBAL: i32 = 0;
    pub const GRID: i32 = 1;
    pub const TORUS: i32 = 2;
    pub const CLIQUE: i32 = 3;
    pub const HEX: i32 = 4;
    pub const PARTIAL: i32 = 5;
    pub const LATTICE: i32 = 6;
    pub const RANDOM_CONNECTED: i32 = 7;
    pub const SCALE_FREE: i32 = 8;
}

/// The subset of `tList<cPopulationCell>` that cell connection lists use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnList {
    cells: VecDeque<i32>,
}

impl ConnList {
    pub fn new() -> Self {
        Self::default()
    }

    /// C++: `tList::Push` (insert at the front).
    pub fn push(&mut self, cell_id: i32) {
        self.cells.push_front(cell_id);
    }

    /// C++: `tList::PushRear`
    pub fn push_rear(&mut self, cell_id: i32) {
        self.cells.push_back(cell_id);
    }

    /// C++: `tList::Remove(T*)`: removes the first match, front to back.
    pub fn remove(&mut self, cell_id: i32) -> bool {
        match self.cells.iter().position(|&c| c == cell_id) {
            Some(pos) => {
                self.cells.remove(pos);
                true
            }
            None => false,
        }
    }

    /// C++: `tList::FindPtr(T*) != NULL`
    pub fn contains(&self, cell_id: i32) -> bool {
        self.cells.contains(&cell_id)
    }

    /// C++: `tList::GetSize`
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// C++: `tList::GetFirst` (`None` where C++ returns NULL).
    pub fn first(&self) -> Option<i32> {
        self.cells.front().copied()
    }

    /// C++: `tList::GetLast`
    pub fn last(&self) -> Option<i32> {
        self.cells.back().copied()
    }

    /// C++: `tList::GetPos(pos)`
    pub fn get(&self, pos: usize) -> Option<i32> {
        self.cells.get(pos).copied()
    }

    /// C++: `tList::CircNext`
    pub fn circ_next(&mut self) {
        if let Some(c) = self.cells.pop_front() {
            self.cells.push_back(c);
        }
    }

    /// C++: `tList::CircPrev`
    pub fn circ_prev(&mut self) {
        if let Some(c) = self.cells.pop_back() {
            self.cells.push_front(c);
        }
    }

    /// The loop of `cPopulationCell::Rotate`: `CircNext` until `cell_id` is
    /// first. C++ never terminates when `cell_id` is absent; this returns
    /// false instead, leaving the list as it was.
    pub fn rotate_to(&mut self, cell_id: i32) -> bool {
        match self.cells.iter().position(|&c| c == cell_id) {
            Some(pos) => {
                self.cells.rotate_left(pos);
                true
            }
            None => false,
        }
    }

    /// Front-to-back iteration (`tConstListIterator`).
    pub fn iter(&self) -> impl Iterator<Item = i32> + '_ {
        self.cells.iter().copied()
    }
}

/// C++: `AvidaTools::Mod`
fn modulo(value: i32, base: i32) -> i32 {
    let v = value % base;
    if v < 0 {
        v + base
    } else {
        v
    }
}

/// C++: `AvidaTools::GridNeighbor`
pub fn grid_neighbor(cell_id: i32, size_x: i32, size_y: i32, diff_x: i32, diff_y: i32) -> i32 {
    let new_x = modulo((cell_id % size_x) + diff_x, size_x);
    let new_y = modulo((cell_id / size_x) + diff_y, size_y);
    new_y * size_x + new_x
}

/// C++: `build_torus`. `slice[i]` is the connection list of cell
/// `offset + i`.
pub fn build_torus(slice: &mut [ConnList], offset: i32, x_size: i32, y_size: i32) {
    const ORDER: [(i32, i32); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
    ];
    for (i, conn) in slice.iter_mut().enumerate() {
        for (dx, dy) in ORDER {
            conn.push(offset + grid_neighbor(i as i32, x_size, y_size, dx, dy));
        }
    }
}

/// C++: `build_grid`
pub fn build_grid(slice: &mut [ConnList], offset: i32, x_size: i32, y_size: i32) {
    build_torus(slice, offset, x_size, y_size);
    for (i, conn) in slice.iter_mut().enumerate() {
        let i = i as i32;
        let x = i % x_size;
        let y = i / x_size;
        let mut remove = |dx: i32, dy: i32| {
            conn.remove(offset + grid_neighbor(i, x_size, y_size, dx, dy));
        };
        if x == 0 {
            remove(-1, -1);
            remove(-1, 0);
            remove(-1, 1);
        }
        if x == x_size - 1 {
            remove(1, -1);
            remove(1, 0);
            remove(1, 1);
        }
        if y == 0 {
            remove(-1, -1);
            remove(0, -1);
            remove(1, -1);
        }
        if y == y_size - 1 {
            remove(-1, 1);
            remove(0, 1);
            remove(1, 1);
        }
    }
}

/// C++: `build_clique`
pub fn build_clique(slice: &mut [ConnList], offset: i32) {
    let n = slice.len();
    for (i, conn) in slice.iter_mut().enumerate() {
        for j in 0..n {
            if j != i {
                conn.push(offset + j as i32);
            }
        }
    }
}

/// C++: `build_hex`
pub fn build_hex(slice: &mut [ConnList], offset: i32, x_size: i32, y_size: i32) {
    build_grid(slice, offset, x_size, y_size);
    for (i, conn) in slice.iter_mut().enumerate() {
        let i = i as i32;
        conn.remove(offset + grid_neighbor(i, x_size, y_size, 1, -1));
        conn.remove(offset + grid_neighbor(i, x_size, y_size, -1, 1));
    }
}

/// C++: `build_random_connected_network`
pub fn build_random_connected_network(
    slice: &mut [ConnList],
    offset: i32,
    x_size: i32,
    y_size: i32,
    rng: &mut AvidaRng,
) {
    let deme_size = x_size * y_size;
    let mut connected: BTreeSet<i32> = BTreeSet::new();
    for i in 0..slice.len() {
        let id_i = offset + i as i32;
        let mut target;
        loop {
            target = rng.get_int_range(0, deme_size);
            if target + offset != id_i {
                break;
            }
        }
        let j = target as usize;
        let id_j = offset + target;
        if !slice[i].contains(id_j) {
            slice[i].push(id_j);
            slice[j].push(id_i);
            if !connected.contains(&id_i) && !connected.contains(&id_j) && !connected.is_empty() {
                let random_index = rng.get_int_range(0, connected.len() as i32);
                // C++ leaves `idValue` at 0 when the index is past the end,
                // which cannot happen for an index below `size()`.
                let id_value = connected
                    .iter()
                    .nth(random_index as usize)
                    .copied()
                    .unwrap_or(0);
                let rc = (id_value - offset) as usize;
                if rng.get_int_range(0, 2) != 0 {
                    slice[i].push(id_value);
                    slice[rc].push(id_i);
                } else {
                    slice[j].push(id_value);
                    slice[rc].push(id_j);
                }
            }
            connected.insert(id_i);
            connected.insert(id_j);
        }
    }

    let extra_edges = rng.get_int_range(0, deme_size);
    for _ in 0..extra_edges {
        let a = rng.get_int_range(0, deme_size);
        let mut b = rng.get_int_range(0, deme_size);
        while a == b {
            b = rng.get_int_range(0, deme_size);
        }
        let (i, j) = (a as usize, b as usize);
        if !slice[i].contains(offset + b) {
            slice[i].push(offset + b);
            slice[j].push(offset + a);
        }
    }
}

/// C++: `build_scale_free`
pub fn build_scale_free(
    slice: &mut [ConnList],
    offset: i32,
    m: i32,
    alpha: f64,
    zero_appeal: f64,
    rng: &mut AvidaRng,
) {
    let id = |i: usize| offset + i as i32;
    let connect = |slice: &mut [ConnList], u: usize, v: usize| {
        slice[u].push(id(v));
        slice[v].push(id(u));
    };
    connect(slice, 0, 1);
    let mut edge_count = 1;
    for u in 2..slice.len() {
        // C++ `vertex_count` starts at 2 and advances with `u`.
        let to_add = (u as i32).min(m);
        let mut added = 0;
        let mut v = 0;
        while added < to_add {
            if !(slice[u].contains(id(v)) || slice[v].contains(id(u))) {
                let mut p_edge = slice[v].len() as f64 / f64::from(edge_count);
                p_edge = p_edge.powf(alpha) + zero_appeal;
                p_edge = p_edge.min(1.0);
                if rng.p(p_edge) {
                    connect(slice, u, v);
                    edge_count += 1;
                    added += 1;
                }
            }
            v += 1;
            if v == u {
                v = 0;
            }
        }
    }
}

/// The geometry switch of `cPopulation::SetupCellGrid` for one deme.
/// `scale_free` is (`SCALE_FREE_M`, `SCALE_FREE_ALPHA`,
/// `SCALE_FREE_ZERO_APPEAL`).
pub fn build_deme_topology(
    geometry: i32,
    slice: &mut [ConnList],
    offset: i32,
    x_size: i32,
    y_size: i32,
    scale_free: (i32, f64, f64),
    rng: &mut AvidaRng,
) -> Result<(), String> {
    match geometry {
        geometry::GRID => build_grid(slice, offset, x_size, y_size),
        geometry::TORUS => build_torus(slice, offset, x_size, y_size),
        geometry::CLIQUE => build_clique(slice, offset),
        geometry::HEX => build_hex(slice, offset, x_size, y_size),
        geometry::RANDOM_CONNECTED => {
            build_random_connected_network(slice, offset, x_size, y_size, rng)
        }
        geometry::SCALE_FREE => {
            build_scale_free(slice, offset, scale_free.0, scale_free.1, scale_free.2, rng)
        }
        // C++ `build_lattice` with z_size 1 indexes one cell past the deme
        // (`slice.Range(0, gridsize)` is inclusive), which is undefined
        // behaviour for the last deme.
        geometry::LATTICE => {
            return Err(
                "unsupported: WORLD_GEOMETRY=6 (C++: cTopology.h:build_lattice reads past the \
                 cell array)"
                    .to_string(),
            )
        }
        other => {
            return Err(format!(
                "unsupported: WORLD_GEOMETRY={other} (C++: cPopulation::SetupCellGrid asserts)"
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lists(n: usize) -> Vec<ConnList> {
        vec![ConnList::new(); n]
    }

    fn ids(c: &ConnList) -> Vec<i32> {
        c.iter().collect()
    }

    #[test]
    fn torus_order_is_reverse_push_order() {
        let mut l = lists(9);
        build_torus(&mut l, 0, 3, 3);
        // Centre cell 4: pushes NW,N,NE,E,SE,S,SW,W = 0,1,2,5,8,7,6,3 so the
        // list reads W first.
        assert_eq!(ids(&l[4]), [3, 6, 7, 8, 5, 2, 1, 0]);
        // Corner wraps.
        assert_eq!(ids(&l[0]), [2, 5, 3, 4, 1, 7, 6, 8]);
    }

    #[test]
    fn grid_drops_wrapped_neighbours() {
        let mut l = lists(9);
        build_grid(&mut l, 0, 3, 3);
        assert_eq!(ids(&l[4]), [3, 6, 7, 8, 5, 2, 1, 0]);
        assert_eq!(ids(&l[0]), [3, 4, 1]);
        assert_eq!(ids(&l[5]), [4, 7, 8, 2, 1]);
    }

    #[test]
    fn offsets_are_global_ids() {
        let mut l = lists(4);
        build_grid(&mut l, 10, 2, 2);
        // 2x2: every wrapped duplicate is removed once per edge rule.
        assert_eq!(ids(&l[0]), [11, 12]);
    }

    #[test]
    fn hex_and_clique() {
        let mut l = lists(9);
        build_hex(&mut l, 0, 3, 3);
        assert_eq!(ids(&l[4]), [3, 7, 8, 5, 1, 0]);
        let mut c = lists(3);
        build_clique(&mut c, 0);
        assert_eq!(ids(&c[0]), [2, 1]);
    }

    #[test]
    fn rotation_matches_tlist() {
        let mut c = ConnList::new();
        for i in [1, 2, 3] {
            c.push_rear(i);
        }
        c.circ_next();
        assert_eq!(ids(&c), [2, 3, 1]);
        c.circ_prev();
        assert_eq!(ids(&c), [1, 2, 3]);
        assert!(c.rotate_to(3));
        assert_eq!(ids(&c), [3, 1, 2]);
        assert!(!c.rotate_to(9));
    }

    #[test]
    fn random_builders_are_symmetric() {
        let mut rng = AvidaRng::new(7);
        let mut l = lists(16);
        build_random_connected_network(&mut l, 0, 4, 4, &mut rng);
        let mut s = lists(16);
        build_scale_free(&mut s, 0, 3, 1.0, 0.0, &mut rng);
        for g in [&l, &s] {
            for (i, c) in g.iter().enumerate() {
                assert!(!c.is_empty());
                for n in c.iter() {
                    assert!(g[n as usize].contains(i as i32));
                }
            }
        }
    }
}
