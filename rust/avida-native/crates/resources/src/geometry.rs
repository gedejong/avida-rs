//! World geometry constants and grid arithmetic.
//!
//! Ports `avida-core/source/main/nGeometry.h` and the two helpers of
//! `avida-core/source/tools/AvidaTools.h` (`Mod`, `GridNeighbor`) that the
//! spatial resource code uses.

/// `nGeometry::GLOBAL`
pub const GLOBAL: i32 = 0;
/// `nGeometry::GRID` (bounded grid)
pub const GRID: i32 = 1;
/// `nGeometry::TORUS`
pub const TORUS: i32 = 2;
/// `nGeometry::CLIQUE`
pub const CLIQUE: i32 = 3;
/// `nGeometry::HEX`
pub const HEX: i32 = 4;
/// `nGeometry::PARTIAL`
pub const PARTIAL: i32 = 5;
/// `nGeometry::LATTICE`
pub const LATTICE: i32 = 6;
/// `nGeometry::RANDOM_CONNECTED`
pub const RANDOM_CONNECTED: i32 = 7;
/// `nGeometry::SCALE_FREE`
pub const SCALE_FREE: i32 = 8;

/// `AvidaTools::Mod(int value, int base)`: C remainder shifted into
/// `[0, base)` for negative values.
///
/// For `base == 0` (an unsized grid; division by zero in the C++) this
/// returns `value`, which is what the reference platform computes: arm64
/// `sdiv` by zero yields 0, so `value % 0 == value`.
#[inline]
pub fn modulo(value: i32, base: i32) -> i32 {
    if base == 0 {
        return value;
    }
    let value = value.wrapping_rem(base);
    if value < 0 {
        value.wrapping_add(base)
    } else {
        value
    }
}

/// `AvidaTools::GridNeighbor(cell_id, size_x, size_y, diff_x, diff_y)`: the
/// torus-wrapped neighbour of `cell_id` in a `size_x` by `size_y` grid.
#[inline]
pub fn grid_neighbor(cell_id: i32, size_x: i32, size_y: i32, diff_x: i32, diff_y: i32) -> i32 {
    let new_x = modulo((cell_id % size_x).wrapping_add(diff_x), size_x);
    let new_y = modulo((cell_id / size_x).wrapping_add(diff_y), size_y);
    new_y.wrapping_mul(size_x).wrapping_add(new_x)
}

/// `cResourceCount::IsSpatial` / the `geometry != GLOBAL && geometry != PARTIAL`
/// test repeated throughout `cResourceCount.cc`.
#[inline]
pub fn is_spatial(geometry: i32) -> bool {
    geometry != GLOBAL && geometry != PARTIAL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_values_match_ngeometry_enum() {
        assert_eq!(
            [
                GLOBAL,
                GRID,
                TORUS,
                CLIQUE,
                HEX,
                PARTIAL,
                LATTICE,
                RANDOM_CONNECTED,
                SCALE_FREE
            ],
            [0, 1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[test]
    fn modulo_matches_avidatools_mod() {
        assert_eq!(modulo(7, 5), 2);
        assert_eq!(modulo(-1, 5), 4);
        assert_eq!(modulo(-5, 5), 0);
        assert_eq!(modulo(-99, 5), 1);
        assert_eq!(modulo(-99, 120), 21);
        assert_eq!(modulo(0, 3), 0);
        // Base 0: the arm64 result of the C++ division by zero.
        assert_eq!(modulo(-7, 0), -7);
        assert_eq!(modulo(7, 0), 7);
    }

    // Ported from rust/avida-rust spatial_res_count_helpers.rs
    // (wrapped_elem_index_wraps_and_matches_reference).
    #[test]
    fn wrapped_index_matches_reference() {
        let idx = |x: i32, y: i32| modulo(y, 4) * 5 + modulo(x, 5);
        assert_eq!(idx(2, 1), 7);
        assert_eq!(idx(-1, 0), 4);
        assert_eq!(idx(6, -1), 16);
        assert_eq!(idx(-13, 9), 7);
    }

    #[test]
    fn grid_neighbor_wraps_like_a_torus() {
        // 3x3 grid, cell 4 is the centre.
        assert_eq!(grid_neighbor(4, 3, 3, -1, -1), 0);
        assert_eq!(grid_neighbor(0, 3, 3, -1, -1), 8);
        assert_eq!(grid_neighbor(2, 3, 3, 1, 0), 0);
        assert_eq!(grid_neighbor(6, 3, 3, 0, 1), 0);
        // 1-wide worlds wrap onto themselves.
        assert_eq!(grid_neighbor(1, 1, 3, 1, 0), 1);
        assert_eq!(grid_neighbor(1, 3, 1, 1, 0), 2);
    }

    #[test]
    fn spatial_test_excludes_global_and_partial() {
        assert!(!is_spatial(GLOBAL));
        assert!(!is_spatial(PARTIAL));
        assert!(is_spatial(GRID));
        assert!(is_spatial(TORUS));
        assert!(is_spatial(42));
        assert!(is_spatial(-7));
    }
}
