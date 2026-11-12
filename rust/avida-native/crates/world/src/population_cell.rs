//! Port of `cPopulationCell` (avida-core/source/main/cPopulationCell.{h,cc}
//! at 47f13dadb), minus its occupant.
//!
//! The C++ cell holds `cOrganism*`; here the population keeps organisms in
//! an arena indexed by cell id next to the cell array, so a cell and its
//! organism can be borrowed independently. `InsertOrganism` /
//! `RemoveOrganism` therefore live on the population. Connections are cell
//! ids ([`ConnList`]) instead of cell pointers.
//!
//! Not ported yet: avatar lists and HGT genome fragments.

use avida_phenotype::MutationRates;

use crate::topology::ConnList;

/// `m_cell_data`: data local to the cell that organisms can read and write.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellData {
    pub contents: i32,
    pub org_id: i32,
    pub update: i32,
    pub territory: i32,
    pub current: i32,
    pub forager: i32,
}

/// C++: `cPopulationCell`
#[derive(Debug, Clone, Default)]
pub struct PopulationCell {
    /// `m_connections`: neighbouring cell ids; the first one is the faced cell.
    pub connections: ConnList,
    pub mut_rates: MutationRates,
    /// `m_inputs`
    pub inputs: Vec<i32>,
    pub cell_id: i32,
    pub deme_id: i32,
    pub cell_data: CellData,
    // `m_spec_state` is `Population::spec_state[cell_id]`.
    pub migrant: bool,
    pub x: i32,
    pub y: i32,
    pub visits: i32,
    pub can_input: bool,
    pub can_output: bool,
}

impl PopulationCell {
    /// C++: `cPopulationCell::Setup`
    pub fn setup(&mut self, in_id: i32, in_rates: &MutationRates, x: i32, y: i32) {
        self.cell_id = in_id;
        self.x = x;
        self.y = y;
        self.deme_id = -1;
        self.cell_data.contents = 0;
        self.cell_data.org_id = -1;
        self.cell_data.update = -1;
        self.cell_data.territory = -1;
        self.mut_rates.copy_from(in_rates);
    }

    /// C++: `cPopulationCell::Rotate`. Turns the connection list until
    /// `new_facing` is first. A cell flagged as holding a migrant only
    /// clears the flag. C++ loops forever if `new_facing` is not a
    /// neighbour; here that returns `false` and leaves the list unchanged.
    pub fn rotate(&mut self, new_facing: i32) -> bool {
        if self.migrant {
            self.migrant = false;
            return true;
        }
        self.connections.rotate_to(new_facing)
    }

    /// C++: `GetCellFaced` (the id of the first connection).
    pub fn cell_faced(&self) -> Option<i32> {
        self.connections.first()
    }

    /// C++: `cPopulationCell::GetFacing`. `faced` is the position of the
    /// faced cell. Bit pattern, not a compass order: N 0, NW 1, SW 2, W 3,
    /// NE 4, E 5, S 6, SE 7. C++ asserts when the faced cell is this cell's
    /// own position and returns 0 in release builds, as here.
    pub fn facing(&self, faced: (i32, i32)) -> i32 {
        let (x, y) = faced;
        let mut lr = 0;
        let mut du = 0;
        if x == self.x - 1 || x > self.x + 1 {
            lr = -1;
        } else if x == self.x + 1 || x < self.x - 1 {
            lr = 1;
        }
        if y == self.y - 1 || y > self.y + 1 {
            du = -1;
        } else if y == self.y + 1 || y < self.y - 1 {
            du = 1;
        }
        match (lr, du) {
            (0, -1) => 0,
            (-1, -1) => 1,
            (-1, 0) => 3,
            (-1, 1) => 2,
            (0, 1) => 6,
            (1, 1) => 7,
            (1, 0) => 5,
            (1, -1) => 4,
            _ => 0,
        }
    }

    /// C++: `cPopulationCell::GetFacedDir`: clockwise from north, N = 0.
    pub fn faced_dir(&self, faced: (i32, i32)) -> i32 {
        match self.facing(faced) {
            1 => 7,
            3 => 6,
            2 => 5,
            6 => 4,
            7 => 3,
            5 => 2,
            4 => 1,
            _ => 0,
        }
    }

    /// C++: `GetInputAt(int& input_pointer)`
    pub fn input_at(&self, input_pointer: &mut i32) -> i32 {
        *input_pointer %= self.inputs.len() as i32;
        let value = self.inputs[*input_pointer as usize];
        *input_pointer += 1;
        value
    }

    /// C++: `cPopulationCell::SetCellData`. `update` is the current update.
    /// `occupant` describes the organism in this cell, if any: its opinion
    /// (`GetOpinion().first` when `HasOpinion()`) and its forage target.
    pub fn set_cell_data(
        &mut self,
        data: i32,
        org_id: i32,
        update: i32,
        occupant: Option<(Option<i32>, i32)>,
    ) {
        self.cell_data.contents = data;
        self.cell_data.org_id = org_id;
        self.cell_data.update = update;
        if let Some((opinion, forage_target)) = occupant {
            if let Some(opinion) = opinion {
                self.cell_data.territory = opinion;
            }
            self.cell_data.forager = forage_target;
        }
    }

    /// C++: `cPopulationCell::ClearCellData`
    pub fn clear_cell_data(&mut self) {
        self.cell_data.contents = 0;
        self.cell_data.org_id = -1;
        self.cell_data.update = -1;
        self.cell_data.territory = -1;
        self.cell_data.forager = -99;
    }

    /// C++: `cPopulationCell::UpdateCellDataExpired`. `expiration` is
    /// `MARKING_EXPIRE_DATE`.
    pub fn update_cell_data_expired(&mut self, expiration: i32, update: i32) {
        let ud_marked = self.cell_data.update;
        if ud_marked != -1 && expiration != -1 && expiration < update - ud_marked {
            self.clear_cell_data();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell_at(x: i32, y: i32) -> PopulationCell {
        let mut c = PopulationCell::default();
        c.setup(y * 60 + x, &MutationRates::default(), x, y);
        c
    }

    #[test]
    fn facing_codes_including_torus_wrap() {
        let c = cell_at(5, 5);
        assert_eq!(c.facing((5, 4)), 0);
        assert_eq!(c.facing((4, 4)), 1);
        assert_eq!(c.facing((4, 5)), 3);
        assert_eq!(c.facing((4, 6)), 2);
        assert_eq!(c.facing((5, 6)), 6);
        assert_eq!(c.facing((6, 6)), 7);
        assert_eq!(c.facing((6, 5)), 5);
        assert_eq!(c.facing((6, 4)), 4);
        assert_eq!(c.faced_dir((6, 4)), 1);
        assert_eq!(c.faced_dir((4, 4)), 7);
        // A neighbour across the torus edge: x = 59 is "left" of x = 0.
        let edge = cell_at(0, 0);
        assert_eq!(edge.facing((59, 0)), 3);
        assert_eq!(edge.facing((0, 59)), 0);
    }

    #[test]
    fn rotate_and_inputs() {
        let mut c = cell_at(1, 1);
        for id in [3, 2, 1] {
            c.connections.push(id);
        }
        assert_eq!(c.cell_faced(), Some(1));
        assert!(c.rotate(3));
        assert_eq!(c.cell_faced(), Some(3));
        c.migrant = true;
        assert!(c.rotate(2));
        assert_eq!(c.cell_faced(), Some(3));
        assert!(!c.migrant);

        c.inputs = vec![10, 20, 30];
        let mut ptr = 2;
        assert_eq!(c.input_at(&mut ptr), 30);
        assert_eq!(c.input_at(&mut ptr), 10);
        assert_eq!(ptr, 1);
    }

    #[test]
    fn cell_data_expiry() {
        let mut c = cell_at(0, 0);
        c.set_cell_data(7, 4, 10, Some((Some(2), -1)));
        assert_eq!((c.cell_data.territory, c.cell_data.forager), (2, -1));
        c.update_cell_data_expired(5, 15);
        assert_eq!(c.cell_data.contents, 7);
        c.update_cell_data_expired(5, 16);
        assert_eq!((c.cell_data.contents, c.cell_data.org_id), (0, -1));
    }
}
