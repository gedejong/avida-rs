//! Avatars (`USE_AVATARS`): the avatar half of `cPopulationInterface`
//! (avida-core/source/main/cPopulationInterface.cc at 47f13dadb, "Avatar
//! support"), the cell avatar lists of `cPopulationCell` (`AddPredAV`,
//! `RemovePreyAV`, `GetRandAV`, ...) and `cPopulation::PlaceAvatar`.
//!
//! # Where the C++ state lives here
//! The C++ keeps an organism's avatars (`m_avatars`, an array of
//! `sIO_avatar`) in its `cPopulationInterface`, its positions in cell lists
//! in the organism (`m_av_in_index`, `m_av_out_index`), and the cell lists
//! (`m_av_pred`, `m_av_prey`, of `cOrganism*`) in the cells. The port keeps
//! all of it in [`Avatars`], keyed by the organism's [`OrgUid`] (the
//! `cOrganism*`; the two offspring of a sexual birth share an id): an
//! organism's interface is created when it is activated, so a new organism
//! starting without avatars is the C++ behaviour, and the entry is dropped
//! when `KillOrganism` deletes the organism.
//!
//! Predators are input avatars and prey output avatars.

use std::collections::BTreeMap;

use avida_hardware::organism::{OrgState, OrgUid};

use crate::world::World;

/// `sIO_avatar`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Avatar {
    /// `av_cell_id`
    pub cell_id: i32,
    /// `av_facing`
    pub facing: i32,
    /// `av_faced_cell`
    pub faced_cell: i32,
    /// `av_input`
    pub input: bool,
    /// `av_output`
    pub output: bool,
}

/// One organism's avatars and list positions.
#[derive(Debug, Clone, Default)]
pub struct OrgAvatars {
    /// `cPopulationInterface::m_avatars`
    pub list: Vec<Avatar>,
    /// `cOrganism::m_av_in_index`
    pub av_in_index: i32,
    /// `cOrganism::m_av_out_index`
    pub av_out_index: i32,
}

/// One cell's avatar lists, as organisms.
#[derive(Debug, Clone, Default)]
pub struct CellAvatars {
    /// `m_av_pred` (input avatars)
    pub pred: Vec<OrgUid>,
    /// `m_av_prey` (output avatars)
    pub prey: Vec<OrgUid>,
}

/// All avatar state of the world.
#[derive(Debug, Default)]
pub struct Avatars {
    /// Per cell, grown on first use.
    pub cells: Vec<CellAvatars>,
    /// Per organism.
    pub orgs: BTreeMap<OrgUid, OrgAvatars>,
}

impl Avatars {
    /// The avatar `av_num` of organism `uid` (`av_num < GetNumAV()`).
    pub fn get(&self, uid: OrgUid, av_num: i32) -> Option<&Avatar> {
        let n = usize::try_from(av_num).ok()?;
        self.orgs.get(&uid)?.list.get(n)
    }

    fn get_mut(&mut self, uid: OrgUid, av_num: i32) -> Option<&mut Avatar> {
        let n = usize::try_from(av_num).ok()?;
        self.orgs.get_mut(&uid)?.list.get_mut(n)
    }

    /// `GetNumAV()`
    pub fn num_av(&self, uid: OrgUid) -> i32 {
        self.orgs.get(&uid).map_or(0, |o| o.list.len() as i32)
    }

    /// The cell's lists (empty for a cell never used).
    pub fn cell(&self, cell_id: i32) -> Option<&CellAvatars> {
        self.cells.get(usize::try_from(cell_id).ok()?)
    }

    /// `(GetNumPredAV(), GetNumPreyAV())` of a cell.
    pub fn counts(&self, cell_id: i32) -> (i32, i32) {
        self.cell(cell_id)
            .map_or((0, 0), |c| (c.pred.len() as i32, c.prey.len() as i32))
    }

    /// `HasAV()`
    pub fn has_av(&self, cell_id: i32) -> bool {
        let (pred, prey) = self.counts(cell_id);
        pred > 0 || prey > 0
    }

    /// `GetRandPredAV()`: the first entry (the list is shuffled on insert).
    pub fn first_pred(&self, cell_id: i32) -> Option<OrgUid> {
        self.cell(cell_id).and_then(|c| c.pred.first().copied())
    }

    /// `GetRandPreyAV()`
    pub fn first_prey(&self, cell_id: i32) -> Option<OrgUid> {
        self.cell(cell_id).and_then(|c| c.prey.first().copied())
    }

    /// `GetCellAVs()`: prey first, then predators.
    pub fn cell_avs(&self, cell_id: i32) -> Vec<OrgUid> {
        self.cell(cell_id).map_or_else(Vec::new, |c| {
            c.prey.iter().chain(c.pred.iter()).copied().collect()
        })
    }

    /// `GetCellOutputAVs()`
    pub fn cell_output_avs(&self, cell_id: i32) -> Vec<OrgUid> {
        self.cell(cell_id).map_or_else(Vec::new, |c| c.prey.clone())
    }
}

/// Which cell list an avatar is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum List {
    Pred,
    Prey,
}

impl World {
    fn av_cell_mut(&mut self, cell_id: i32) -> Option<&mut CellAvatars> {
        let c = usize::try_from(cell_id)
            .ok()
            .filter(|&c| c < self.pop.cells.len())?;
        if self.avatars.cells.len() < self.pop.cells.len() {
            self.avatars
                .cells
                .resize_with(self.pop.cells.len(), CellAvatars::default);
        }
        Some(&mut self.avatars.cells[c])
    }

    fn set_list_index(&mut self, uid: OrgUid, which: List, index: i32) {
        let o = self.avatars.orgs.entry(uid).or_default();
        match which {
            List::Pred => o.av_in_index = index,
            List::Prey => o.av_out_index = index,
        }
    }

    /// `cPopulationCell::AddPredAV` / `AddPreyAV`: push, then swap the new
    /// avatar into a random position to keep the list mixed.
    fn cell_add_av(&mut self, cell_id: i32, uid: OrgUid, which: List) {
        let Some(cell) = self.av_cell_mut(cell_id) else {
            self.set_fatal(format!(
                "avatar added to cell {cell_id} outside the world (C++: out-of-bounds cell)"
            ));
            return;
        };
        let list = match which {
            List::Pred => &mut cell.pred,
            List::Prey => &mut cell.prey,
        };
        list.push(uid);
        let n = list.len();
        let loc = self.rng.get_uint_range(0, n as u32) as usize;
        let list = match which {
            List::Pred => &mut self.avatars.cells[cell_id as usize].pred,
            List::Prey => &mut self.avatars.cells[cell_id as usize].prey,
        };
        let exist_org = list[loc];
        list.swap(loc, n - 1);
        self.set_list_index(exist_org, which, (n - 1) as i32);
        self.set_list_index(uid, which, loc as i32);
    }

    /// `cPopulationCell::RemovePredAV` / `RemovePreyAV`: swap with the last
    /// entry and pop.
    fn cell_remove_av(&mut self, cell_id: i32, uid: OrgUid, which: List) {
        let index = self.avatars.orgs.get(&uid).map_or(-1, |o| match which {
            List::Pred => o.av_in_index,
            List::Prey => o.av_out_index,
        });
        let Some(cell) = self.av_cell_mut(cell_id) else {
            return;
        };
        let list = match which {
            List::Pred => &mut cell.pred,
            List::Prey => &mut cell.prey,
        };
        let Some(i) = usize::try_from(index).ok().filter(|&i| i < list.len()) else {
            self.set_fatal(format!(
                "avatar of organism {} not in cell {cell_id}'s list (C++: assert in \
                 cPopulationCell::RemovePredAV/RemovePreyAV)",
                uid.0
            ));
            return;
        };
        let last = list.len() - 1;
        let exist_org = list[last];
        list.swap(i, last);
        list.pop();
        self.set_list_index(exist_org, which, index);
    }

    /// The organism in `cell_id`: the occupant, or the organism being run
    /// when it lives there.
    pub(crate) fn org_uid_in_cell(&self, cell_id: i32) -> Option<OrgUid> {
        let c = usize::try_from(cell_id)
            .ok()
            .filter(|&c| c < self.pop.orgs.len())?;
        match self.pop.orgs[c].as_ref() {
            Some(o) => Some(o.state.uid),
            None => self
                .pop
                .running
                .as_ref()
                .filter(|r| r.cell_id == cell_id && !r.removed)
                .map(|r| r.uid),
        }
    }

    /// `AddIOAV(ctx, av_cell_id, av_facing, input, output)`
    pub(crate) fn add_io_av(
        &mut self,
        uid: OrgUid,
        av_cell_id: i32,
        av_facing: i32,
        input: bool,
        output: bool,
    ) {
        let o = self.avatars.orgs.entry(uid).or_default();
        o.list.push(Avatar {
            cell_id: av_cell_id,
            facing: av_facing,
            faced_cell: -1,
            input,
            output,
        });
        let last = o.list.len() as i32 - 1;
        if input {
            self.cell_add_av(av_cell_id, uid, List::Pred);
        }
        if output {
            self.cell_add_av(av_cell_id, uid, List::Prey);
        }
        // Find the created avatar's faced cell
        self.set_av_faced_cell_id(uid, last);
    }

    /// `AddPredPreyAV(ctx, av_cell_id)`: a predator (input) avatar unless
    /// the organism is prey (`IsPreyFT()`).
    pub(crate) fn add_pred_prey_av(&mut self, uid: OrgUid, is_prey: bool, av_cell_id: i32) {
        let o = self.avatars.orgs.entry(uid).or_default();
        o.list.push(Avatar {
            cell_id: av_cell_id,
            facing: 0,
            faced_cell: -1,
            input: !is_prey,
            output: is_prey,
        });
        let last = o.list.len() as i32 - 1;
        if is_prey {
            self.cell_add_av(av_cell_id, uid, List::Prey);
        } else {
            self.cell_add_av(av_cell_id, uid, List::Pred);
        }
        // Find the created avatar's faced cell
        self.set_av_faced_cell_id(uid, last);
    }

    /// `AddPredPreyAV` for the organism living in `cell_id` (injection).
    pub(crate) fn add_pred_prey_av_of_cell(&mut self, cell_id: i32) {
        self.add_pred_prey_av_of_cell_at(cell_id, cell_id);
    }

    /// `AddPredPreyAV(ctx, av_cell_id)` for the organism living in
    /// `cell_id`.
    pub(crate) fn add_pred_prey_av_of_cell_at(&mut self, cell_id: i32, av_cell_id: i32) {
        let Some(org) = self.pop.orgs[cell_id as usize].as_ref() else {
            return;
        };
        let (uid, is_prey) = (org.state.uid, org.state.forage_target > -2);
        self.add_pred_prey_av(uid, is_prey, av_cell_id);
    }

    /// `SwitchPredPrey(ctx, av_num)`
    pub(crate) fn switch_pred_prey(&mut self, uid: OrgUid, av_num: i32) {
        let Some(av) = self.avatars.get(uid, av_num).copied() else {
            return;
        };
        if av.input {
            // Is a predator, switching to a prey (input to output)
            self.cell_remove_av(av.cell_id, uid, List::Pred);
            self.cell_add_av(av.cell_id, uid, List::Prey);
            if let Some(a) = self.avatars.get_mut(uid, av_num) {
                a.input = false;
                a.output = true;
            }
        } else if av.output {
            self.cell_remove_av(av.cell_id, uid, List::Prey);
            self.cell_add_av(av.cell_id, uid, List::Pred);
            if let Some(a) = self.avatars.get_mut(uid, av_num) {
                a.input = true;
                a.output = false;
            }
        }
    }

    /// `RemoveAllAV()`. QUIRK: the loop pops while counting up to the
    /// shrinking `GetNumAV()`, so only the last half (rounded up) of the
    /// avatars is removed.
    pub(crate) fn remove_all_av(&mut self, uid: OrgUid) {
        let mut i = 0;
        while i < self.avatars.num_av(uid) {
            let Some(av) = self.avatars.orgs.get_mut(&uid).and_then(|o| o.list.pop()) else {
                break;
            };
            // Check that avatar is actually in a cell
            if av.cell_id >= 0 {
                if av.input {
                    self.cell_remove_av(av.cell_id, uid, List::Pred);
                }
                if av.output {
                    self.cell_remove_av(av.cell_id, uid, List::Prey);
                }
            }
            i += 1;
        }
    }

    /// The avatar half of `KillOrganism`: "orgs killed during birth wont
    /// have avatars"; with `NEURAL_NETWORKING` `RemoveAllAV` runs (again).
    /// The entry is dropped once no avatar is left (the C++ interface lives
    /// until the organism is deleted, but holds nothing more).
    pub(crate) fn kill_avatars(&mut self, uid: OrgUid) {
        if self.cfg.use_avatars == 0 {
            return;
        }
        if self.av_cell_id(uid, 0) != -1 {
            self.remove_all_av(uid);
        }
        if self.cfg.neural_networking {
            self.remove_all_av(uid);
        }
        if self.avatars.num_av(uid) == 0 {
            self.avatars.orgs.remove(&uid);
        }
    }

    /// `GetAVFacing(av_num)`
    pub(crate) fn av_facing(&self, uid: OrgUid, av_num: i32) -> i32 {
        self.avatars.get(uid, av_num).map_or(0, |a| a.facing)
    }

    /// `GetAVCellID(av_num)`
    pub(crate) fn av_cell_id(&self, uid: OrgUid, av_num: i32) -> i32 {
        self.avatars.get(uid, av_num).map_or(-1, |a| a.cell_id)
    }

    /// `GetAVFacedCellID(av_num)`
    pub(crate) fn av_faced_cell_id(&self, uid: OrgUid, av_num: i32) -> i32 {
        self.avatars.get(uid, av_num).map_or(-1, |a| a.faced_cell)
    }

    /// The deme geometry avatars use: `(x_size, y_size, deme_size)` with
    /// `y_size = WORLD_Y / NUM_DEMES`.
    fn av_deme_geometry(&self) -> Option<(i32, i32, i32)> {
        let x_size = self.cfg.world_x;
        if self.cfg.num_demes == 0 {
            self.set_fatal("NUM_DEMES 0 with avatars (C++: division by zero)");
            return None;
        }
        let y_size = self.cfg.world_y / self.cfg.num_demes;
        Some((x_size, y_size, x_size * y_size))
    }

    fn check_av_geometry(&self) -> bool {
        let g = self.cfg.world_geometry;
        if g != 1 && g != 2 {
            self.set_fatal(
                "error: Not valid WORLD_GEOMETRY for USE_AVATAR, must be torus or bounded.",
            );
            return false;
        }
        true
    }

    /// `GetAVNumNeighbors(av_num)`
    pub(crate) fn av_num_neighbors(&self, uid: OrgUid, av_num: i32) -> i32 {
        if !self.check_av_geometry() {
            return 0;
        }
        let Some(av) = self.avatars.get(uid, av_num) else {
            return 0;
        };
        if self.cfg.world_geometry == 2 {
            return 8;
        }
        let Some((x_size, y_size, deme_size)) = self.av_deme_geometry() else {
            return 0;
        };
        let deme_cell = av.cell_id % deme_size;
        let x = deme_cell % x_size;
        let y = deme_cell / x_size;
        // Is the cell on a corner..
        if x == 0 || x == x_size - 1 {
            if y == 0 || y == y_size - 1 {
                return 3;
            }
            // Is the cell on a side-edge..
            return 5;
        }
        // Is the cell on a top or bottom edge..
        if y == 0 || y == y_size - 1 {
            return 5;
        }
        // The cell must be on the interior..
        8
    }

    /// `SetAVFacing(ctx, av_facing, av_num)`
    pub(crate) fn set_av_facing(&mut self, uid: OrgUid, av_facing: i32, av_num: i32) {
        if let Some(a) = self.avatars.get_mut(uid, av_num) {
            a.facing = av_facing;
            // Set the new avatar faced cell id
            self.set_av_faced_cell_id(uid, av_num);
        }
    }

    /// `SetAVCellID(ctx, av_cell_id, av_num)`: move the avatar to another
    /// cell's lists.
    pub(crate) fn set_av_cell_id(&mut self, uid: OrgUid, av_cell_id: i32, av_num: i32) -> bool {
        let Some(av) = self.avatars.get(uid, av_num).copied() else {
            return false;
        };
        // Not necessary to move the avatar
        if av.cell_id == av_cell_id {
            return false;
        }
        // If the avatar was previously in another cell remove it
        if av.cell_id > -1 {
            if av.input {
                self.cell_remove_av(av.cell_id, uid, List::Pred);
            }
            if av.output {
                self.cell_remove_av(av.cell_id, uid, List::Prey);
            }
        }
        if av.input {
            self.cell_add_av(av_cell_id, uid, List::Pred);
        }
        if av.output {
            self.cell_add_av(av_cell_id, uid, List::Prey);
        }
        if let Some(a) = self.avatars.get_mut(uid, av_num) {
            a.cell_id = av_cell_id;
        }
        // Set the avatar's new faced cell
        self.set_av_faced_cell_id(uid, av_num);
        true
    }

    /// `SetAVFacedCellID(ctx, av_num)`: the cell the avatar faces. On a
    /// bounded grid an avatar facing off the edge faces along it instead
    /// (randomly one way or the other where two cells qualify).
    pub(crate) fn set_av_faced_cell_id(&mut self, uid: OrgUid, av_num: i32) {
        if !self.check_av_geometry() {
            return;
        }
        let world_geometry = self.cfg.world_geometry;
        let Some(av) = self.avatars.get(uid, av_num).copied() else {
            return;
        };
        let Some((x_size, y_size, deme_size)) = self.av_deme_geometry() else {
            return;
        };
        let old_cell_id = av.cell_id;
        let facing = av.facing;
        let deme_id = old_cell_id / deme_size;
        let old_deme_cell = old_cell_id % deme_size;
        let mut x = old_deme_cell % x_size;
        let mut y = old_deme_cell / x_size;

        // If this happens to be an avatar in a single cell world, it can't
        // face any cell beyond its own
        if deme_size == 1 {
            if let Some(a) = self.avatars.get_mut(uid, av_num) {
                a.faced_cell = a.cell_id;
            }
            return;
        }

        let mut off_the_edge_facing = false;
        let rng = &mut self.rng;
        let mut coin = || rng.get_int_range(0, 2) != 0;
        // If a bounded grid, do checks for facing off the edge of a bounded
        // world grid..
        if world_geometry == 1 {
            // Check if the avatar is at the end of a single column world
            if x_size == 1 {
                if y == 0 {
                    y += 1;
                    off_the_edge_facing = true;
                } else if y == y_size - 1 {
                    y -= 1;
                    off_the_edge_facing = true;
                }
            // Check if the avatar is at the end of a single row world
            } else if y_size == 1 {
                if x == 0 {
                    x += 1;
                    off_the_edge_facing = true;
                }
                // The C++ at 47f13dadb tests `y == (y_size - 1)` here, which
                // is always true in a single row; the reference binary's
                // `avd_popif_av_faced_cell` (rust/avida-rust) tests the
                // east end, as here.
                else if x == x_size - 1 {
                    x -= 1;
                    off_the_edge_facing = true;
                }
            }

            // The world is neither a single row or column, continuing
            // border facing checks
            if !off_the_edge_facing {
                // West edge..
                if x == 0 {
                    // Northwest corner
                    if y == 0 {
                        if facing == 0 || facing == 7 || facing == 6 {
                            if coin() {
                                x += 1;
                            } else {
                                y += 1;
                            }
                            off_the_edge_facing = true;
                        } else if facing == 5 {
                            y += 1;
                            off_the_edge_facing = true;
                        } else if facing == 1 {
                            x += 1;
                            off_the_edge_facing = true;
                        }
                    }
                    // Southwest corner
                    else if y == y_size - 1 {
                        if facing == 4 || facing == 5 || facing == 6 {
                            if coin() {
                                x += 1;
                            } else {
                                y -= 1;
                            }
                            off_the_edge_facing = true;
                        } else if facing == 7 {
                            x += 1;
                            off_the_edge_facing = true;
                        } else if facing == 3 {
                            y -= 1;
                            off_the_edge_facing = true;
                        }
                    }
                    // West edge facings not checked yet
                    if !off_the_edge_facing {
                        if facing == 5 {
                            // West edge facing southwest
                            y -= 1;
                            off_the_edge_facing = true;
                        } else if facing == 6 {
                            // West edge facing west
                            if coin() {
                                y += 1;
                            } else {
                                y -= 1;
                            }
                            off_the_edge_facing = true;
                        } else if facing == 7 {
                            // West edge facing northwest
                            y += 1;
                            off_the_edge_facing = true;
                        }
                    }
                }
                // East edge..
                else if x == x_size - 1 {
                    // Northeast corner
                    if y == 0 {
                        if facing == 0 || facing == 1 || facing == 2 {
                            if coin() {
                                x -= 1;
                            } else {
                                y += 1;
                            }
                            off_the_edge_facing = true;
                        }
                        // QUIRK: plain `if`s here, not `else if`.
                        if facing == 3 {
                            y += 1;
                            off_the_edge_facing = true;
                        }
                        if facing == 7 {
                            x -= 1;
                            off_the_edge_facing = true;
                        }
                    }
                    // Southeast corner
                    else if y == y_size - 1 {
                        if facing == 2 || facing == 3 || facing == 4 {
                            if coin() {
                                x -= 1;
                            } else {
                                y -= 1;
                            }
                            off_the_edge_facing = true;
                        } else if facing == 1 {
                            y -= 1;
                            off_the_edge_facing = true;
                        } else if facing == 5 {
                            x -= 1;
                            off_the_edge_facing = true;
                        }
                    }
                    // East edge facings not checked yet
                    if !off_the_edge_facing {
                        if facing == 1 {
                            // East edge facing northeast
                            y -= 1;
                            off_the_edge_facing = true;
                        } else if facing == 2 {
                            // East edge facing east
                            if coin() {
                                y += 1;
                            } else {
                                y -= 1;
                            }
                            off_the_edge_facing = true;
                        } else if facing == 3 {
                            // East edge facing southeast
                            y += 1;
                            off_the_edge_facing = true;
                        }
                    }
                }
                // North edge..
                else if y == 0 {
                    if facing == 7 {
                        x -= 1;
                        off_the_edge_facing = true;
                    } else if facing == 0 {
                        if coin() {
                            x += 1;
                        } else {
                            x -= 1;
                        }
                        off_the_edge_facing = true;
                    } else if facing == 1 {
                        x += 1;
                        off_the_edge_facing = true;
                    }
                }
                // South edge..
                else if y == y_size - 1 {
                    if facing == 3 {
                        x += 1;
                        off_the_edge_facing = true;
                    } else if facing == 4 {
                        if coin() {
                            x += 1;
                        } else {
                            x -= 1;
                        }
                        off_the_edge_facing = true;
                    } else if facing == 5 {
                        x -= 1;
                        off_the_edge_facing = true;
                    }
                }
            }
        }

        // Torus world geometry or not a bounded outward facing edge..
        if !off_the_edge_facing || world_geometry == 2 {
            // North facing
            if facing == 0 || facing == 1 || facing == 7 {
                y = (y - 1 + y_size) % y_size;
            }
            // South facing
            if facing == 3 || facing == 4 || facing == 5 {
                y = (y + 1) % y_size;
            }
            // East facing
            if facing == 1 || facing == 2 || facing == 3 {
                x = (x + 1) % x_size;
            }
            // West facing
            if facing == 5 || facing == 6 || facing == 7 {
                x = (x - 1 + x_size) % x_size;
            }
        }

        // Convert the x,y deme coordinates back into a cell id
        let new_deme_cell = y * x_size + x;
        let new_cell_id = deme_id * deme_size + new_deme_cell;
        if let Some(a) = self.avatars.get_mut(uid, av_num) {
            a.faced_cell = new_cell_id;
        }
    }

    /// `MoveAV(ctx, av_num)`: move the avatar into its faced cell; `mover`
    /// is the running organism, living in `true_cell`.
    pub(crate) fn move_av(
        &mut self,
        uid: OrgUid,
        true_cell: i32,
        mover: &mut OrgState,
        av_num: i32,
    ) -> bool {
        let Some(av) = self.avatars.get(uid, av_num).copied() else {
            return false;
        };
        if self.cfg.neural_networking
            || self.move_organisms(av.cell_id, av.faced_cell, true_cell, mover)
        {
            let faced = self.av_faced_cell_id(uid, av_num);
            return self.set_av_cell_id(uid, faced, av_num);
        }
        false
    }

    /// `RotateAV(ctx, increment, av_num)`. QUIRK: the new facing is set on
    /// avatar 0 whatever `av_num` is (`SetAVFacing(ctx, new_facing)`).
    pub(crate) fn rotate_av(&mut self, uid: OrgUid, increment: i32, av_num: i32) -> bool {
        let Some(av) = self.avatars.get(uid, av_num).copied() else {
            return false;
        };
        let increment = if increment >= 0 {
            increment % 8
        } else {
            -(increment.wrapping_neg() % 8)
        };
        let new_facing = (av.facing + increment + 8) % 8;
        self.set_av_facing(uid, new_facing, 0);
        true
    }

    /// `cPopulationCell::GetRandAV(ctx)`: any avatar of the cell, all
    /// equally likely (draws).
    pub(crate) fn cell_rand_av(&mut self, cell_id: i32) -> Option<OrgUid> {
        let (num_pred, num_prey) = self.avatars.counts(cell_id);
        if num_pred + num_prey == 0 {
            return None;
        }
        let rand = self.rng.get_uint_range(0, (num_pred + num_prey) as u32) as i32;
        let cell = self.avatars.cell(cell_id)?;
        Some(if rand < num_pred {
            cell.pred[rand as usize]
        } else {
            cell.prey[(rand - num_pred) as usize]
        })
    }

    /// `cPopulationCell::HasOutputAV(org)`: an output avatar the organism
    /// can receive messages from.
    pub(crate) fn cell_has_output_av_for(&self, cell_id: i32, uid: OrgUid) -> bool {
        let Some(cell) = self.avatars.cell(cell_id) else {
            return false;
        };
        if cell.prey.is_empty() {
            return false;
        }
        if self.cfg.self_communication {
            return true;
        }
        cell.prey.iter().any(|&o| o != uid)
    }

    /// `FindAV(input, output, av_num)`. QUIRK: the index is `i + av_num %
    /// num_AV`, which leaves `m_avatars` for a negative `av_num` or once it
    /// passes the end; the C++ then reads outside the array, so the run
    /// stops there. A miss returns -1, which every caller (the neuron
    /// instructions) passes on to an accessor whose only guard is
    /// `av_num < GetNumAV()`, so it reads `m_avatars[-1]`: that stops too.
    pub(crate) fn find_av(&self, uid: OrgUid, input: bool, output: bool, av_num: i32) -> i32 {
        let num_av = self.avatars.num_av(uid);
        if num_av == 0 {
            // The C++ asserts and then takes `av_num % 0`.
            self.set_fatal("FindAV without avatars (C++: modulo by zero)");
            return -1;
        }
        for i in 0..num_av {
            let index = i + av_num % num_av;
            let Some(a) = self.avatars.get(uid, index) else {
                self.set_fatal(format!(
                    "FindAV index {index} outside the organism's {num_av} avatars \
                     (C++: out-of-bounds read of m_avatars)"
                ));
                return -1;
            };
            if a.input == input && a.output == output {
                return index;
            }
        }
        self.set_fatal("FindAV without a matching avatar (C++: the caller reads m_avatars[-1])");
        -1
    }

    /// `cPopulation::PlaceAvatar(ctx, parent)`: the cell an offspring's
    /// avatar starts in, -1 when it would be born on a deadly edge.
    pub(crate) fn place_avatar(&mut self, parent: OrgUid) -> i32 {
        let world_x = self.pop.world_x;
        let world_y = self.pop.world_y;
        let avatar_target_cell = match self.cfg.avatar_birth {
            // Random
            1 => self.rng.get_uint((world_x * world_y) as u32) as i32,
            // Parent Facing
            2 => self.av_faced_cell_id(parent, 0),
            // Next Cell. QUIRK: always 0 (-1 + 1).
            3 => 0,
            // World Center
            4 => (f64::from(world_x * world_y) * 0.5) as i32,
            // Parent cell
            _ => self.av_cell_id(parent, 0),
        };
        if self.cfg.deadly_boundaries == 1
            && self.cfg.world_geometry == 1
            && avatar_target_cell >= 0
        {
            let wx = self.cfg.world_x;
            let dest_x = avatar_target_cell % wx;
            let dest_y = avatar_target_cell / wx;
            if dest_x == 0 || dest_y == 0 || dest_x == wx - 1 || dest_y == self.cfg.world_y - 1 {
                return -1;
            }
        }
        avatar_target_cell
    }

    /// The avatar block of `ActivateOffspring` for the offspring just
    /// placed in `target` ("only assign an avatar cell if the org lived
    /// through birth and it isn't the parent").
    pub(crate) fn place_offspring_avatar(
        &mut self,
        target: i32,
        org_survived: bool,
        parent: OrgUid,
    ) {
        if self.cfg.use_avatars == 0 || !org_survived {
            return;
        }
        let avatar_target_cell = self.place_avatar(parent);
        if avatar_target_cell == -1 {
            self.kill_organism(target, None);
            return;
        }
        let t = target as usize;
        let Some(org) = self.pop.orgs[t].as_mut() else {
            return;
        };
        org.state.phenotype.av_birth_cell_id = avatar_target_cell;
        let (uid, is_prey) = (org.state.uid, org.state.forage_target > -2);
        self.try_write_birth_loc_data_of_cell(target);
        self.add_pred_prey_av(uid, is_prey, avatar_target_cell);
        if self.cfg.avatar_birth_facing == 1 {
            let rots = self.rng.get_uint_range(0, 8);
            // `offspring->Rotate(ctx, rots)`: one avatar step per call.
            for _ in 0..rots {
                self.rotate_av(uid, 1, 0);
            }
        }
    }

    /// `DumpTargetGrid` with avatars: a predator's forage target if the
    /// cell holds one, else a prey's, else -99.
    pub(crate) fn avatar_grid_target(&self, cell_id: i32) -> i32 {
        if !self.avatars.has_av(cell_id) {
            return -99;
        }
        self.avatars
            .first_pred(cell_id)
            .or_else(|| self.avatars.first_prey(cell_id))
            .and_then(|uid| self.forage_target_of(uid))
            .unwrap_or(-99)
    }

    /// `(GetAVCellID(), GetAVFacing())` of organism `uid`.
    pub(crate) fn avatar_cell_and_facing(&self, uid: OrgUid) -> (i32, i32) {
        (self.av_cell_id(uid, 0), self.av_facing(uid, 0))
    }

    /// The cell of the living organism `uid` (scanning the live list).
    pub(crate) fn cell_of_org(&self, uid: OrgUid) -> Option<i32> {
        if let Some(r) = self.pop.running.as_ref() {
            if r.uid == uid && !r.removed {
                return Some(r.cell_id);
            }
        }
        self.pop.live_org_list.iter().copied().find(|&c| {
            self.pop.orgs[c as usize]
                .as_ref()
                .is_some_and(|o| o.state.uid == uid)
        })
    }

    /// `GetForageTarget()` of a living organism other than the running one.
    fn forage_target_of(&self, uid: OrgUid) -> Option<i32> {
        let c = self.cell_of_org(uid)?;
        self.pop.orgs[c as usize]
            .as_ref()
            .map(|o| o.state.forage_target)
    }
}
