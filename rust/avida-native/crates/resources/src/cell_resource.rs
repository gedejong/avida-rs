//! Per-cell resource definition (the `CELL` environment command).
//!
//! Ports `cCellResource` from `avida-core/source/main/cResource.{h,cc}`.

/// `cCellResource`: initial amount and inflow/outflow for one cell of a
/// spatial resource ("mini-chemostat").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellResource {
    /// `cell_id`
    pub cell_id: i32,
    /// `initial`
    pub initial: f64,
    /// `inflow`
    pub inflow: f64,
    /// `outflow`
    pub outflow: f64,
}

impl CellResource {
    /// `cCellResource(int, double, double, double)`
    pub fn new(cell_id: i32, initial: f64, inflow: f64, outflow: f64) -> Self {
        CellResource {
            cell_id,
            initial,
            inflow,
            outflow,
        }
    }
}

impl Default for CellResource {
    /// `cCellResource()`: cell id `-99`, all amounts zero.
    fn default() -> Self {
        CellResource::new(-99, 0.0, 0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_ccellresource_ctor() {
        let c = CellResource::default();
        assert_eq!(c.cell_id, -99);
        assert_eq!((c.initial, c.inflow, c.outflow), (0.0, 0.0, 0.0));
        let d = CellResource::new(4, 1.5, 0.25, 0.125);
        assert_eq!(
            (d.cell_id, d.initial, d.inflow, d.outflow),
            (4, 1.5, 0.25, 0.125)
        );
    }
}
