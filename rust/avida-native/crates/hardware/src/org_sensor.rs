//! Port of `cOrgSensor` (avida-core/source/main/cOrgSensor.{h,cc} at
//! 47f13dadb): the sensor the experimental hardware
//! ([`crate::hardware_experimental`]) looks through.
//!
//! [`OrgSensor::set_looking`] is the whole look: argument clean-up
//! (`SetLooking`), the sighting of a named organism (`FindOrg`), global
//! resource values (`GlobalVal`) and the walk over the cells in the cone
//! ahead (`PreWalk`, `SetWalkLimits`, `WalkCells` / `WalkTorus`,
//! `TestCell`, `FindThing`). The population is read through
//! [`OrgInterface`] (cells, avatars, frozen resources, sighted organisms as
//! [`SeenOrg`]); the looking organism itself is out of its cell while it
//! runs, so it is described from its own state wherever the walk meets it.
//! A predator that sights prey with `PRED_CONFUSION` set may be confused
//! (`TestConfusion`): the odds come from the prey density, facings or
//! opinions around the sighted prey.

use crate::org_interface::{OrgInterface, SeenOrg};
use crate::organism::{OrgState, OrgUid};

/// `sOrgDisplay`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OrgDisplay {
    /// `distance`
    pub distance: i32,
    /// `direction`
    pub direction: i32,
    /// `thing_id`
    pub thing_id: i32,
    /// `value`
    pub value: i32,
    /// `message`
    pub message: i32,
}

/// `cOrgSensor::sLookInit`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookInit {
    /// `habitat`
    pub habitat: i32,
    /// `distance`
    pub distance: i32,
    /// `search_type`
    pub search_type: i32,
    /// `id_sought`
    pub id_sought: i32,
}

impl Default for LookInit {
    /// `sLookInit()`
    fn default() -> Self {
        LookInit {
            habitat: i32::MIN,
            distance: i32::MAX,
            search_type: 0,
            id_sought: -1,
        }
    }
}

/// `cOrgSensor::sLookOut`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookOut {
    /// `report_type`
    pub report_type: i32,
    /// `habitat`
    pub habitat: i32,
    /// `distance`
    pub distance: i32,
    /// `search_type`
    pub search_type: i32,
    /// `id_sought`
    pub id_sought: i32,
    /// `count`
    pub count: i32,
    /// `value`
    pub value: i32,
    /// `group`
    pub group: i32,
    /// `forage`
    pub forage: i32,
    /// `deviance`
    pub deviance: i32,
}

impl Default for LookOut {
    /// `sLookOut()`
    fn default() -> Self {
        LookOut {
            report_type: 0,
            habitat: 0,
            distance: -1,
            search_type: 0,
            id_sought: -1,
            count: 0,
            value: 0,
            group: -9,
            forage: -9,
            deviance: 0,
        }
    }
}

/// `cOrgSensor`
#[derive(Debug, Clone, Default)]
pub struct OrgSensor {
    use_avatar: i32,
    return_rel_facing: bool,
    last_seen_display: OrgDisplay,
    has_seen_display: bool,
}

impl OrgSensor {
    /// `cOrgSensor(world, organism)`: `ResetOrgSensor()`.
    pub fn new(use_avatars: i32) -> Self {
        let mut s = OrgSensor::default();
        s.reset(use_avatars);
        s
    }

    /// `Reset()` (`ResetOrgSensor()`). The last seen display is kept, as in
    /// C++. (`m_soloBounds`, resized here in C++, is only used by the
    /// population walk.)
    pub fn reset(&mut self, use_avatars: i32) {
        self.use_avatar = use_avatars;
        self.return_rel_facing = false;
        self.has_seen_display = false;
    }

    /// `SetReturnRelativeFacing(do_set)`
    pub fn set_return_relative_facing(&mut self, do_set: bool) {
        self.return_rel_facing = do_set;
    }

    /// `m_return_rel_facing`
    pub fn return_relative_facing_set(&self) -> bool {
        self.return_rel_facing
    }

    /// `SetLastSeenDisplay(seen_display)`
    pub fn set_last_seen_display(&mut self, seen_display: &OrgDisplay) {
        self.last_seen_display = *seen_display;
        self.has_seen_display = true;
    }

    /// `HasSeenDisplay()`
    pub fn has_seen_display(&self) -> bool {
        self.has_seen_display
    }

    /// `GetLastSeenDisplay()`
    pub fn last_seen_display(&self) -> &OrgDisplay {
        &self.last_seen_display
    }

    /// `ReturnRelativeFacing(sighted_org)` given the two facings (the
    /// caller picks `GetFacedDir()` or `GetAVFacing()` by `USE_AVATARS`, as
    /// the C++ does).
    pub fn return_relative_facing(&mut self, target_facing: i32, org_facing: i32) -> i32 {
        let mut match_heading = target_facing.wrapping_sub(org_facing);
        if match_heading > 4 {
            match_heading -= 8;
        } else if match_heading < -4 {
            match_heading += 8;
        } else if match_heading.abs() == 4 {
            match_heading = 4;
        }
        self.set_return_relative_facing(false);
        match_heading
    }

    /// Whether `ReturnRelativeFacing` reads avatar facings.
    pub fn uses_avatar(&self) -> bool {
        self.use_avatar != 0
    }

    /// `GetTorusTravelDist(travel_dist, x_dist, y_dist, facing, worldx,
    /// worldy)`.
    pub fn get_torus_travel_dist(
        travel_dist: &mut i32,
        x_dist: &mut i32,
        y_dist: &mut i32,
        facing: i32,
        worldx: i32,
        worldy: i32,
    ) {
        if facing == 0 && *y_dist > 0 {
            *y_dist = worldy - *travel_dist;
        } else if facing == 4 && *y_dist < 0 {
            *y_dist = worldy + *travel_dist;
        } else if facing == 2 && *x_dist < 0 {
            *x_dist = worldx + *travel_dist;
        } else if facing == 6 && *x_dist > 0 {
            *x_dist = worldx - *travel_dist;
        } else if facing == 1 {
            if *y_dist > 0 {
                *y_dist = worldy - *travel_dist;
            } else if *x_dist < 0 {
                *x_dist = worldx + *travel_dist;
            }
        } else if facing == 3 {
            if *y_dist < 0 {
                *y_dist = worldy + *travel_dist;
            } else if *x_dist < 0 {
                *x_dist = worldx + *travel_dist;
            }
        } else if facing == 5 {
            if *y_dist < 0 {
                *y_dist = worldy + *travel_dist;
            } else if *x_dist > 0 {
                *x_dist = worldx - *travel_dist;
            }
        } else if facing == 7 {
            if *y_dist > 0 {
                *y_dist = worldy - *travel_dist;
            } else if *x_dist > 0 {
                *x_dist = worldx - *travel_dist;
            }
        }
        *travel_dist = x_dist.wrapping_abs().max(y_dist.wrapping_abs());
    }

    /// `FindDirFromHome()`
    pub fn find_dir_from_home(org: &OrgState) -> i32 {
        let easterly = org.easterly;
        let northerly = org.northerly;
        if northerly > 0 && easterly == 0 {
            4
        } else if northerly > 0 && easterly < 0 {
            5
        } else if northerly == 0 && easterly < 0 {
            6
        } else if northerly < 0 && easterly < 0 {
            7
        } else if northerly < 0 && easterly == 0 {
            0
        } else if northerly < 0 && easterly > 0 {
            1
        } else if northerly == 0 && easterly > 0 {
            2
        } else if northerly > 0 && easterly > 0 {
            3
        } else {
            0
        }
    }

    /// `FindDistanceFromHome()`
    pub fn find_distance_from_home(org: &OrgState) -> i32 {
        org.easterly
            .wrapping_abs()
            .max(org.northerly.wrapping_abs())
    }
}

/// `cOrgSensor::sBounds`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Bounds {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

/// `cOrgSensor::sWalkLimits` (its `tot_bounds` is unused).
#[derive(Debug, Clone, Copy)]
struct WalkLimits {
    visible: bool,
    start: i32,
    end: i32,
}

/// `cOrgSensor::sSearchInfo`
#[derive(Debug, Clone, Copy)]
struct SearchInfo {
    amount_found: f64,
    resource_id: i32,
    has_edible: bool,
    has_some: bool,
}

impl Default for SearchInfo {
    fn default() -> Self {
        SearchInfo {
            amount_found: 0.0,
            resource_id: -9,
            has_edible: false,
            has_some: false,
        }
    }
}

/// `Apto::Coord<int>`
type Coord = (i32, i32);

fn add(a: Coord, b: Coord) -> Coord {
    (a.0.wrapping_add(b.0), a.1.wrapping_add(b.1))
}

fn scale(a: Coord, k: i32) -> Coord {
    (a.0.wrapping_mul(k), a.1.wrapping_mul(k))
}

/// `TestBounds(cell, bounds)`
fn test_bounds(c: Coord, b: &Bounds) -> bool {
    !(c.0 < b.min_x || c.1 < b.min_y || c.0 > b.max_x || c.1 > b.max_y)
}

/// `CorrectTorusEdge(cell, worldBounds)`
fn correct_torus_edge(c: &mut Coord, wb: &Bounds) {
    if c.0 > wb.max_x {
        c.0 = wb.min_x + (c.0 - wb.max_x - 1);
    } else if c.0 < wb.min_x {
        c.0 = wb.max_x - (wb.min_x - c.0 - 1);
    }
    if c.1 > wb.max_y {
        c.1 = wb.min_y + (c.1 - wb.max_y - 1);
    } else if c.1 < wb.min_y {
        c.1 = wb.max_y - (wb.min_y - c.1 - 1);
    }
}

/// `SetCoords(left, right, facing)`: an unknown facing leaves both at
/// `(0, 0)`.
fn set_coords(facing: i32) -> (Coord, Coord) {
    match facing {
        0 | 4 => ((-1, 0), (1, 0)),
        2 | 6 => ((0, -1), (0, 1)),
        1 => ((-1, 0), (0, 1)),
        3 => ((0, -1), (-1, 0)),
        5 => ((1, 0), (0, -1)),
        7 => ((0, 1), (1, 0)),
        _ => ((0, 0), (0, 0)),
    }
}

/// `(int) ((d - 1) * 0.5)` for odd `d`, `(int) (d * 0.5)` for even.
fn cells_either_side(d: i32) -> i32 {
    if d % 2 != 0 {
        (f64::from(d - 1) * 0.5) as i32
    } else {
        (f64::from(d) * 0.5) as i32
    }
}

/// The resource-library facts the sensor reads.
#[derive(Debug, Clone, Copy)]
struct ResInfo {
    habitat: i32,
    global: bool,
    gradient: bool,
    threshold: f64,
    prob_detect: f64,
}

/// The configuration the sensor reads.
#[derive(Debug, Clone, Copy)]
struct LookCfg {
    world_x: i32,
    world_y: i32,
    geometry: i32,
    pred_prey_switch: i32,
    look_disable: i32,
    look_disable_type: i32,
    look_dist: i32,
    use_display: i32,
    use_mimics: bool,
    mimic_odds: f64,
    pred_confusion: i32,
}

/// One look: the sensor, the looking organism and its interface, and the
/// settings read once at the start.
struct Look<'a, 'w> {
    sensor: &'a mut OrgSensor,
    org: &'a mut OrgState,
    w: &'a mut (dyn OrgInterface + 'w),
    cfg: LookCfg,
    res: Vec<ResInfo>,
    /// `m_soloBounds`
    solo_bounds: Vec<Bounds>,
}

/// The walk state `WalkCells` / `WalkTorus` hand over to their shared
/// reporting tail.
struct WalkResult {
    found: bool,
    found_edible: bool,
    found_first_visible: bool,
    first_visible_id: i32,
    first_visible_cell: Coord,
    first_success_cell: Coord,
    first_whole_resource: i32,
    count: i32,
    total_amount: f64,
    dist_used: i32,
}

impl OrgSensor {
    /// `SetLooking(ctx, in_defs, facing, cell_id, use_ft)`. `org` is the
    /// looking organism (`m_organism`); `in_defs` is copied, as the C++
    /// caller discards its changes.
    pub fn set_looking(
        &mut self,
        org: &mut OrgState,
        w: &mut dyn OrgInterface,
        in_defs: &LookInit,
        facing: i32,
        cell_id: i32,
        use_ft: bool,
    ) -> Result<LookOut, String> {
        let mut defs = *in_defs;
        self.set_looking_inout(org, w, &mut defs, facing, cell_id, use_ft)
    }

    /// `SetLooking` with the C++ in/out `in_defs`: the habitat, distance,
    /// search type and id it settles on are written back (the BCR and GP8
    /// look instructions store the adjusted distance in their sessions).
    pub fn set_looking_inout(
        &mut self,
        org: &mut OrgState,
        w: &mut dyn OrgInterface,
        in_defs: &mut LookInit,
        facing: i32,
        cell_id: i32,
        use_ft: bool,
    ) -> Result<LookOut, String> {
        let c = w.config();
        let cfg = LookCfg {
            world_x: c.world_x,
            world_y: c.world_y,
            geometry: c.world_geometry,
            pred_prey_switch: c.pred_prey_switch,
            look_disable: c.look_disable,
            look_disable_type: c.look_disable_type,
            look_dist: c.look_dist,
            use_display: c.use_display,
            use_mimics: c.use_mimics != 0,
            mimic_odds: c.mimic_odds,
            pred_confusion: c.pred_confusion,
        };
        let res: Vec<ResInfo> = w
            .environment()
            .resource_lib()
            .resources()
            .iter()
            .map(|r| ResInfo {
                habitat: r.habitat,
                global: r.geometry == avida_environment::resource::geometry::GLOBAL,
                gradient: r.isgradient,
                threshold: r.threshold,
                prob_detect: r.prob_detect,
            })
            .collect();
        let world_bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: cfg.world_x - 1,
            max_y: cfg.world_y - 1,
        };
        let solo_bounds = vec![world_bounds; res.len()];
        let mut look = Look {
            sensor: self,
            org,
            w,
            cfg,
            res,
            solo_bounds,
        };
        look.set_looking(in_defs, facing, cell_id, use_ft)
    }
}

impl Look<'_, '_> {
    fn is_habitat(&self, habitat: i32) -> bool {
        self.w.environment().is_habitat(habitat)
    }

    fn use_avatar(&self) -> i32 {
        self.sensor.use_avatar
    }

    /// The resource `id` of the library (`m_res_lib.GetResource(id)`);
    /// callers have checked the range.
    fn res(&self, id: i32) -> ResInfo {
        self.res[id as usize]
    }

    /// The looking organism as a [`SeenOrg`].
    fn me(&mut self) -> SeenOrg {
        let opinion = if self.org.has_opinion() {
            Some(self.org.get_opinion().0)
        } else {
            None
        };
        SeenOrg {
            id: self.org.id,
            uid: self.org.uid,
            cell_id: self.w.get_cell_id(),
            av_cell_id: self.w.get_av_cell_id(0),
            faced_dir: self.w.get_faced_dir(),
            av_facing: self.w.get_av_facing(0),
            cur_bonus: self.org.phenotype.cur_bonus,
            opinion,
            forage_target: self.org.forage_target,
            show_ft: self.org.show_ft,
            is_dead: self.org.is_dead,
            is_displaying: self.org.display,
            display: self.org.org_display,
        }
    }

    /// The living organism `uid`, the looker included.
    fn seen(&mut self, uid: OrgUid) -> Option<SeenOrg> {
        if uid == self.org.uid {
            return Some(self.me());
        }
        self.w.seen_org(uid)
    }

    fn set_looking(
        &mut self,
        d: &mut LookInit,
        facing: i32,
        cell_id: i32,
        use_ft: bool,
    ) -> Result<LookOut, String> {
        let lib_size = self.res.len() as i32;
        let worldx = self.cfg.world_x;
        let worldy = self.cfg.world_y;
        let pred_experiment = self.cfg.pred_prey_switch > -1;
        let forage = self.org.forage_target;

        if self.cfg.look_disable < 5 && self.cfg.look_disable > 0 {
            let org_type = self.cfg.look_disable_type;
            let is_prey = self.org.forage_target > -2;
            let is_target_type =
                (org_type == 0 && !is_prey) || (org_type == 1 && is_prey) || org_type == 2;
            if is_target_type {
                let randsign = if self.w.rng().get_uint_range(0, 2) != 0 {
                    -1
                } else {
                    1
                };
                let rand = self.w.rng().get_int(i32::MAX).wrapping_mul(randsign);
                match self.cfg.look_disable {
                    1 => d.habitat = rand,
                    2 => d.distance = rand,
                    3 => d.search_type = rand,
                    4 => d.id_sought = rand,
                    _ => {}
                }
            }
        }

        // first reg gives habitat type sought (aligns with org m_target
        // settings and gradient res habitat types)
        // default to look for orgs if invalid habitat & predator
        if pred_experiment && forage <= -2 && !self.is_habitat(d.habitat) {
            d.habitat = -2;
        }
        // default to look for env res if invalid habitat & forager
        else if !self.is_habitat(d.habitat) && d.habitat != -2 {
            d.habitat = 0;
        }

        // second reg gives distance sought--arbitrarily capped at half long
        // axis of world--default to 1 if low invalid number, half-world if
        // high
        let long_axis = (f64::from(worldx.max(worldy)) * 0.5 + 0.5) as i32;
        let mut max_dist = if self.cfg.look_dist != -1 {
            self.cfg.look_dist
        } else {
            long_axis
        };
        if max_dist > worldx.max(worldy) {
            max_dist = worldx.max(worldy);
        }
        if d.distance < 0 {
            d.distance = 1;
        } else if d.distance > max_dist {
            d.distance = max_dist;
        }

        // third register gives type of search used for food resources
        // (habitat 0) and org hunting (habitat -2)
        if d.habitat != -2 && (d.search_type < 0 || d.search_type > 1) {
            d.search_type = 0;
        } else if pred_experiment
            && d.habitat == -2
            && forage > -2
            && (d.search_type < -2 || d.search_type > 2)
        {
            d.search_type = 1;
        } else if pred_experiment
            && d.habitat == -2
            && forage <= -2
            && (d.search_type < -2 || d.search_type > 2)
        {
            d.search_type = -1;
        } else if !pred_experiment && d.habitat == -2 && (d.search_type < -2 || d.search_type > 0) {
            d.search_type = 0;
        }

        // fourth register gives specific instance of resources sought or
        // specific organisms to look for
        if d.id_sought < -1 {
            d.id_sought = -1;
        }
        // override if using lookFT
        if (d.id_sought < -1 || d.id_sought >= lib_size) && use_ft && d.habitat != -2 {
            d.id_sought = forage;
        }
        if d.habitat != -2 {
            // if invalid res id...
            if d.id_sought < -1 || d.id_sought >= lib_size {
                if self.is_habitat(d.habitat) || forage < 0 || forage >= lib_size {
                    d.id_sought = -1;
                } else {
                    d.id_sought = forage;
                }
            }
            if d.id_sought != -1 {
                d.habitat = self.res(d.id_sought).habitat;
            }
        } else {
            // if looking for org...
            let mut done_setting_org = false;
            let mut target_org = None;
            // if invalid number or self, we will just search for any org
            // matching search type, skipping rest of look for specific org
            if d.id_sought < -1 || d.id_sought == self.org.id {
                d.id_sought = -1;
                done_setting_org = true;
            }
            // if valid org id number, does the value represent a living
            // organism
            else if d.id_sought != -1 {
                target_org = self.w.find_live_org(d.id_sought);
                done_setting_org = target_org.is_some();
            }
            if !done_setting_org && d.id_sought != -1 {
                d.id_sought = -1;
            }
            if let (true, Some(target)) = (done_setting_org && d.id_sought != -1, target_org) {
                return self.find_org(target, d.id_sought, d.distance, facing);
            }
        }

        // habitat is 0 and any of the resources are non-gradient types, are
        // we dealing with global resources and can just use the global val
        if d.habitat == 0 || d.habitat > 5 {
            if d.id_sought != -1 && self.res(d.id_sought).global {
                return Ok(self.global_val(d));
            } else if d.id_sought == -1 {
                let mut all_global = true;
                d.id_sought = 0;
                while d.id_sought < lib_size {
                    let r = self.res(d.id_sought);
                    if r.global {
                        let globalval = self.global_val(d);
                        if globalval.value >= 1 && d.search_type == 0 {
                            return Ok(globalval);
                        }
                    } else if r.habitat == 0 || r.habitat > 5 {
                        all_global = false;
                        if d.search_type == 1 {
                            break;
                        }
                    }
                    d.id_sought += 1;
                }
                d.id_sought = -1;
                if all_global {
                    // if all global, but none edible
                    return Ok(self.global_val(d));
                }
            }
        }

        self.pre_walk(d, facing, cell_id)
    }

    /// `FindOrg(ctx, target_org, distance_sought, facing)`; `target_id` is
    /// the target's `GetID()`.
    fn find_org(
        &mut self,
        target_org: OrgUid,
        target_id: i32,
        distance_sought: i32,
        facing: i32,
    ) -> Result<LookOut, String> {
        let mut org_search = LookOut {
            report_type: 1,
            habitat: -2,
            id_sought: target_id,
            search_type: -9,
            ..LookOut::default()
        };
        if self.use_avatar() != 0 && self.use_avatar() != 2 {
            return Ok(org_search);
        }
        let Some(target) = self.seen(target_org) else {
            return Err(format!(
                "look-ahead: living organism {target_id} has no state (C++: cOrgSensor::FindOrg)"
            ));
        };
        let target_cell = if self.use_avatar() != 0 {
            target.av_cell_id
        } else {
            target.cell_id
        };
        self.find_thing(target_cell, distance_sought, facing, &mut org_search);

        if org_search.distance > -1 {
            org_search.value = target.cur_bonus as i32;
            org_search.group = self.group_seen(&target);
            org_search.forage = target.forage_target;
            self.note_seen_display(&target);
            if self.cfg.use_display == 0 || self.cfg.use_display == 1 {
                self.set_potential_display_data(&org_search);
            }
            if self.cfg.use_mimics && org_search.forage == 1 && self.w.rng().p(self.cfg.mimic_odds)
            {
                org_search.forage = target.show_ft;
            }
        }
        Ok(org_search)
    }

    /// `TestConfusion(ctx, stuff_seen, first_org)`: a predator that sights
    /// prey among others may be confused, and then sees six random numbers.
    fn test_confusion(&mut self, stuff_seen: &mut LookOut, first: &SeenOrg) -> Result<(), String> {
        let odds = match self.cfg.pred_confusion {
            1 => self.confusion_odds_density(first)?,
            2 => self.confusion_odds_facings(first)?,
            3 => self.confusion_odds_opinions(first)?,
            _ => 0.0,
        };
        let rng = self.w.rng();
        if rng.get_double() >= odds {
            stuff_seen.distance = rng.get_int_full();
            stuff_seen.count = rng.get_int_full();
            stuff_seen.value = rng.get_int_full();
            stuff_seen.id_sought = rng.get_int_full();
            // this is relative facing for nearest individual
            stuff_seen.group = rng.get_int_full();
            stuff_seen.deviance = rng.get_int_full();
        }
        Ok(())
    }

    /// The occupant of `cell` when it is living prey (`IsOccupied() &&
    /// !GetOrganism()->IsDead() && GetOrganism()->IsPreyFT()`).
    fn live_prey_in(&mut self, cell: i32) -> Option<SeenOrg> {
        let uid = self.w.cell_organism_uid(cell)?;
        self.seen(uid)
            .filter(|s| !s.is_dead && s.forage_target > -2)
    }

    /// The organism of a prey avatar.
    fn seen_av(&mut self, uid: OrgUid) -> Result<SeenOrg, String> {
        self.seen(uid).ok_or_else(|| {
            "unsupported: a prey avatar without a living organism (C++: dangling cOrganism* \
             in cOrgSensor::GetConfusionOdds*)"
                .to_string()
        })
    }

    /// The avatar cell of the sighted organism (`first_org->GetOrgInterface()
    /// .GetAVCellID()`).
    fn first_av_cell(first: &SeenOrg) -> Result<i32, String> {
        if first.av_cell_id < 0 {
            return Err(
                "unsupported: confusion odds of an organism without an avatar \
                        (C++: GetCell(-1) in cOrgSensor::GetConfusionOdds*)"
                    .to_string(),
            );
        }
        Ok(first.av_cell_id)
    }

    /// `GetConfusionOddsDensity(ctx, odds, first_org)`: the prey around the
    /// sighted prey.
    fn confusion_odds_density(&mut self, first: &SeenOrg) -> Result<f64, String> {
        let mut prey_count = 0;
        if self.use_avatar() == 0 {
            if first.forage_target > -2 {
                prey_count += 1;
                for c in self.w.cell_neighborhood_ids(first.cell_id) {
                    if self.live_prey_in(c).is_some() {
                        prey_count += 1;
                    }
                }
            }
        } else {
            let av_cell = Self::first_av_cell(first)?;
            prey_count += self.w.cell_av_counts(av_cell).1; // self cell
            for c in self.w.cell_neighborhood_ids(av_cell) {
                prey_count += self.w.cell_av_counts(c).1;
            }
        }
        // first org is alone and doesn't count
        prey_count -= 1;
        // 0 friend = 0% confusion, 1 friend = 10%, 10 friends = 100%;
        // `1.0 - (0.10 * prey_count)` is one fused operation in the
        // reference build
        Ok(f64::from(prey_count).mul_add(-0.10, 1.0))
    }

    /// `GetConfusionOddsFacings(ctx, odds, first_org)`: how many of the
    /// eight facings the prey around the sighted prey use.
    fn confusion_odds_facings(&mut self, first: &SeenOrg) -> Result<f64, String> {
        fn tally(facings: &mut [i32; 8], num_used: &mut i32, dir: i32) -> Result<(), String> {
            let slot = usize::try_from(dir)
                .ok()
                .and_then(|d| facings.get_mut(d))
                .ok_or_else(|| {
                    format!(
                        "unsupported: a facing of {dir} (C++: facings[{dir}] out of range in \
                         cOrgSensor::GetConfusionOddsFacings)"
                    )
                })?;
            if *slot == 0 {
                *num_used += 1;
            }
            *slot += 1;
            Ok(())
        }
        let mut facings = [0; 8];
        let mut num_used = 0;
        if self.use_avatar() == 0 {
            if first.forage_target > -2 {
                for c in self.w.cell_neighborhood_ids(first.cell_id) {
                    if let Some(s) = self.live_prey_in(c) {
                        tally(&mut facings, &mut num_used, s.faced_dir)?;
                    }
                }
            }
        } else {
            let av_cell = Self::first_av_cell(first)?;
            // self cell
            for k in self.w.cell_prey_avs(av_cell) {
                if k != first.uid {
                    let s = self.seen_av(k)?;
                    tally(&mut facings, &mut num_used, s.av_facing)?;
                }
                if num_used >= 8 {
                    break;
                }
            }
            // neighbors
            if num_used < 8 {
                for c in self.w.cell_neighborhood_ids(av_cell) {
                    if self.w.cell_av_counts(c).1 > 0 {
                        for k in self.w.cell_prey_avs(c) {
                            let s = self.seen_av(k)?;
                            tally(&mut facings, &mut num_used, s.av_facing)?;
                            if num_used >= 8 {
                                break;
                            }
                        }
                    }
                    if num_used >= 8 {
                        break;
                    }
                }
            }
        }
        // 0 friend = 0% confusion, 1 friend = 1/8%, 8 friends = 100%
        Ok(1.0 - f64::from(num_used) / 8.0)
    }

    /// `GetConfusionOddsOpinions(ctx, odds, first_org)`: how many of the
    /// formed groups the prey around the sighted prey belong to.
    fn confusion_odds_opinions(&mut self, first: &SeenOrg) -> Result<f64, String> {
        let group_ids = self.w.get_formed_group_array();
        let num_groups = group_ids.len() as i32;
        let mut groups_used = vec![0; group_ids.len()];
        let mut num_used = 0;
        // `groups_used[GetGroupIdx(group_ids, op)]`
        let mut tally = |op: i32, num_used: &mut i32| -> Result<(), String> {
            let i = group_ids.iter().position(|&g| g == op).ok_or_else(|| {
                format!(
                    "unsupported: an opinion ({op}) that is no formed group (C++: \
                     groups_used[-1] in cOrgSensor::GetConfusionOddsOpinions)"
                )
            })?;
            if groups_used[i] == 0 {
                *num_used += 1;
            }
            groups_used[i] += 1;
            Ok(())
        };
        if self.use_avatar() == 0 {
            if first.forage_target > -2 {
                for c in self.w.cell_neighborhood_ids(first.cell_id) {
                    let Some(s) = self.live_prey_in(c) else {
                        continue;
                    };
                    // QUIRK: whether the sighted prey has an opinion decides
                    // whether its neighbour's is read.
                    if first.opinion.is_some() {
                        let op = s.opinion.ok_or_else(|| {
                            "unsupported: the opinion of a prey without one (C++: GetOpinion() \
                             of an empty opinion list in cOrgSensor::GetConfusionOddsOpinions)"
                                .to_string()
                        })?;
                        tally(op, &mut num_used)?;
                    }
                }
            }
        } else {
            let av_cell = Self::first_av_cell(first)?;
            // self cell
            for k in self.w.cell_prey_avs(av_cell) {
                if k != first.uid {
                    if let Some(op) = self.seen_av(k)?.opinion {
                        tally(op, &mut num_used)?;
                    }
                }
                if num_used >= num_groups {
                    break;
                }
            }
            // neighbors
            if num_used < num_groups {
                for c in self.w.cell_neighborhood_ids(av_cell) {
                    if self.w.cell_av_counts(c).1 > 0 {
                        for k in self.w.cell_prey_avs(c) {
                            if let Some(op) = self.seen_av(k)?.opinion {
                                tally(op, &mut num_used)?;
                                if num_used >= num_groups {
                                    break;
                                }
                            }
                        }
                    }
                    if num_used >= num_groups {
                        break;
                    }
                }
            }
        }
        // if 10 opinions, 0 friend = 0% confusion, 1 friend = 10%, 10
        // friends = 100%
        Ok(1.0 - f64::from(num_used) / f64::from(num_groups))
    }

    /// `if (!m_return_rel_facing && HasOpinion()) GetOpinion().first; else
    /// ReturnRelativeFacing(org)`
    fn group_seen(&mut self, seen: &SeenOrg) -> i32 {
        match seen.opinion {
            Some(op) if !self.sensor.return_rel_facing => op,
            _ => self.return_relative_facing_to(seen),
        }
    }

    /// `ReturnRelativeFacing(sighted_org)`
    fn return_relative_facing_to(&mut self, seen: &SeenOrg) -> i32 {
        let (target_facing, org_facing) = if self.use_avatar() != 0 {
            (seen.av_facing, self.w.get_av_facing(0))
        } else {
            (seen.faced_dir, self.w.get_faced_dir())
        };
        self.sensor
            .return_relative_facing(target_facing, org_facing)
    }

    /// `if ((IsDisplaying() || USE_DISPLAY) && GetOrgDisplayData() != NULL)
    /// SetLastSeenDisplay(GetOrgDisplayData())`
    fn note_seen_display(&mut self, seen: &SeenOrg) {
        if seen.is_displaying || self.cfg.use_display != 0 {
            if let Some(display) = seen.display {
                self.sensor.set_last_seen_display(&display);
            }
        }
    }

    /// `FindThing(target_cell, distance_sought, facing, thing_search,
    /// target_org)` (`target_org` is unused).
    fn find_thing(
        &mut self,
        target_cell: i32,
        distance_sought: i32,
        facing: i32,
        thing_search: &mut LookOut,
    ) {
        let worldx = self.cfg.world_x;
        let worldy = self.cfg.world_y;
        let target_x = target_cell % worldx;
        let target_y = target_cell / worldx;
        let searching_org_cell = if self.use_avatar() != 0 {
            self.w.get_av_cell_id(0)
        } else {
            self.w.get_cell_id()
        };
        let searching_x = searching_org_cell % worldx;
        let searching_y = searching_org_cell / worldx;
        let mut x_dist = target_x - searching_x;
        let mut y_dist = target_y - searching_y;
        let mut travel_dist = x_dist.wrapping_abs().max(y_dist.wrapping_abs());

        // is the target close enough to see and in my line of sight?
        let mut thing_in_sight = true;
        if self.cfg.geometry == 2 {
            OrgSensor::get_torus_travel_dist(
                &mut travel_dist,
                &mut x_dist,
                &mut y_dist,
                facing,
                worldx,
                worldy,
            );
        }

        // if simply too far or behind you
        if travel_dist > distance_sought {
            thing_in_sight = false;
        } else if self.cfg.geometry != 2 {
            thing_in_sight = !match facing {
                0 => y_dist > 0,
                4 => y_dist < 0,
                2 => x_dist < 0,
                6 => x_dist > 0,
                1 => y_dist > 0 || x_dist < 0,
                3 => y_dist < 0 || x_dist < 0,
                5 => y_dist < 0 || x_dist > 0,
                7 => y_dist > 0 || x_dist > 0,
                _ => false,
            };
        }

        // if not too far in absolute x or y directions, check the distance
        // when we consider offset from center sight line (is it within
        // sight cone?)
        if thing_in_sight {
            let n = cells_either_side(travel_dist);
            let (ax, ay) = (x_dist.wrapping_abs(), y_dist.wrapping_abs());
            if ((facing == 0 || facing == 4) && ax > n) || ((facing == 2 || facing == 6) && ay > n)
            {
                thing_in_sight = false;
            } else if facing == 1 {
                let (cx, cy) = (searching_x + ax, searching_y - ay);
                if target_x < cx - n || target_y > cy + n {
                    thing_in_sight = false;
                }
            } else if facing == 3 {
                let (cx, cy) = (searching_x + ax, searching_y + ay);
                if target_x < cx - n || target_y < cy - n {
                    thing_in_sight = false;
                }
            } else if facing == 5 {
                let (cx, cy) = (searching_x - ax, searching_y + ay);
                if target_x > cx + n || target_y < cy - n {
                    thing_in_sight = false;
                }
            } else if facing == 7 {
                let (cx, cy) = (searching_x - ax, searching_y - ay);
                if target_x > cx + n || target_y > cy + n {
                    thing_in_sight = false;
                }
            }
        }

        if thing_in_sight {
            let (ax, ay) = (x_dist.wrapping_abs(), y_dist.wrapping_abs());
            let offset = match facing {
                4 => x_dist.wrapping_neg(),
                2 => y_dist.wrapping_neg(),
                6 => y_dist,
                1 | 5 => ax - ay,
                3 | 7 => ay - ax,
                _ => x_dist,
            };
            thing_search.distance = travel_dist;
            thing_search.deviance = offset;
        }
    }

    /// `GlobalVal(ctx, in_defs)`
    fn global_val(&mut self, d: &LookInit) -> LookOut {
        let mut val = 0.0;
        if d.id_sought != -1 {
            val = if self.use_avatar() == 0 {
                self.w.get_resource_val(self.org, d.id_sought)
            } else {
                self.w.get_av_resource_val(self.org, d.id_sought, 0)
            };
        }
        let mut stuff_seen = LookOut {
            report_type: 1,
            habitat: d.habitat,
            search_type: d.search_type,
            id_sought: d.id_sought,
            ..LookOut::default()
        };
        // can't use threshold...those only apply to gradient resources, so
        // this is arbitrarily set at any (> 0)
        if val > 0.0 {
            stuff_seen.distance = 0;
            stuff_seen.count = 1;
            stuff_seen.value = (val + 0.5) as i32;
            stuff_seen.group = d.id_sought;
            if self.cfg.use_display == 0 || self.cfg.use_display == 1 {
                self.set_potential_display_data(&stuff_seen);
            }
        }
        stuff_seen
    }

    /// `PreWalk(ctx, in_defs, facing, cell_id)`
    fn pre_walk(&mut self, d: &mut LookInit, facing: i32, cell_id: i32) -> Result<LookOut, String> {
        // rather than doing doupdates at every cell check inside TestCell,
        // we just do it once now since we're in a stall
        self.w.trigger_do_updates(self.org);

        let mut stuff_seen = LookOut {
            report_type: 0,
            habitat: d.habitat,
            search_type: d.search_type,
            id_sought: d.id_sought,
            ..LookOut::default()
        };
        let ua = self.use_avatar();
        if (ua != 0 && ua != 2 && d.habitat == -2) || d.habitat == 3 {
            return Ok(stuff_seen);
        }

        let worldx = self.cfg.world_x;
        let worldy = self.cfg.world_y;
        let faced_cell_int = if ua != 0 {
            self.w.get_av_faced_cell_id(0)
        } else {
            self.w.get_faced_cell_id()
        };
        let faced_cell = (faced_cell_int % worldx, faced_cell_int / worldx);
        let mut center_cell = (cell_id % worldx, cell_id / worldx);
        let this_cell = center_cell;
        let ahead_dir = (faced_cell.0 - this_cell.0, faced_cell.1 - this_cell.1);

        let single_bound = (d.habitat == 0 || d.habitat >= 4)
            && d.id_sought != -1
            && self.res(d.id_sought).gradient;
        let mut val_res = if d.habitat != -2 && d.habitat != 3 {
            self.build_res_array(d, single_bound)
        } else {
            Vec::new()
        };

        let world_bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: worldx - 1,
            max_y: worldy - 1,
        };
        let mut tot_bounds = Bounds {
            min_x: worldx - 1,
            min_y: worldy - 1,
            max_x: -(worldx - 1),
            max_y: -(worldy - 1),
        };
        self.solo_bounds.fill(world_bounds);

        let mut limits = WalkLimits {
            visible: true,
            start: 0,
            end: 0,
        };
        self.set_walk_limits(
            d,
            &mut limits,
            &world_bounds,
            &mut tot_bounds,
            &mut val_res,
            this_cell,
            facing,
            cell_id,
            &mut center_cell,
            ahead_dir,
        );

        if !limits.visible {
            // nothing in range
            stuff_seen.report_type = 0;
        } else {
            let torus = self.cfg.geometry == 2;
            let walk = self.walk(
                d,
                facing,
                &limits,
                center_cell,
                &tot_bounds,
                &world_bounds,
                &val_res,
                ahead_dir,
                torus,
            );
            self.report(d, facing, &mut stuff_seen, &walk, torus)?;
        }
        Ok(stuff_seen)
    }

    /// `SetWalkLimits(...)`: the distances to walk, the bounds of the
    /// resources sought, and the fast-forwarded first centre cell.
    #[allow(clippy::too_many_arguments)]
    fn set_walk_limits(
        &mut self,
        d: &LookInit,
        limits: &mut WalkLimits,
        world_bounds: &Bounds,
        tot_bounds: &mut Bounds,
        val_res: &mut Vec<i32>,
        this_cell: Coord,
        facing: i32,
        cell: i32,
        center_cell: &mut Coord,
        ahead_dir: Coord,
    ) {
        let worldx = self.cfg.world_x;
        limits.start = 0;
        limits.end = d.distance;

        // set geometric bounds, and fast-forward, if possible
        if d.habitat != -2 && d.habitat != 3 {
            let mut has_global = false;
            let mut in_res = false;
            let mut global_only = true;
            let mut temp_start_dist = d.distance;
            let mut i = 0;
            while i < val_res.len() {
                let r = val_res[i];
                if self.res(r).gradient {
                    let res_bounds = self.get_bounds(r);
                    let this_start_dist =
                        self.get_min_dist(worldx, &res_bounds, cell, d.distance, facing);
                    // drop any out of range...
                    if this_start_dist == -1 {
                        let last = val_res.len() - 1;
                        val_res.swap(i, last);
                        val_res.pop();
                        continue;
                    }
                    global_only = false;
                    tot_bounds.min_x = tot_bounds.min_x.min(res_bounds.min_x);
                    tot_bounds.min_y = tot_bounds.min_y.min(res_bounds.min_y);
                    tot_bounds.max_x = tot_bounds.max_x.max(res_bounds.max_x);
                    tot_bounds.max_y = tot_bounds.max_y.max(res_bounds.max_y);
                    in_res = res_bounds.min_x <= this_cell.0
                        && res_bounds.max_x >= this_cell.0
                        && res_bounds.min_y <= this_cell.1
                        && res_bounds.max_y >= this_cell.1;
                    if this_start_dist < temp_start_dist {
                        temp_start_dist = this_start_dist;
                    }
                } else {
                    in_res = true;
                    // if any res is global, we just need to make sure we
                    // check at least one cell
                    if self.res(r).global {
                        has_global = true;
                    }
                    // if any res is spatial and non-gradient, we can't
                    // bound things because those res don't track the
                    // variables we use for bounding
                    else {
                        global_only = false;
                        *tot_bounds = *world_bounds;
                        temp_start_dist = 0;
                        break;
                    }
                }
                i += 1;
            }
            limits.start = temp_start_dist;
            if val_res.is_empty() {
                // nothing in range
                limits.visible = false;
                return;
            }

            // Code to allow searching to continue past world edges in torus
            if in_res && self.cfg.geometry == 2 {
                let wb = world_bounds;
                limits.end = match facing {
                    0 | 4 => wb.max_y - 1,
                    2 | 6 => wb.max_x - 1,
                    1 | 5 | 7 | 3 => (wb.max_x - 1).abs().max((wb.max_y - 1).abs()),
                    _ => limits.end,
                };
                limits.end = limits.end.min(d.distance);
            } else {
                limits.end = Self::get_max_dist(worldx, cell, d.distance, tot_bounds);
            }
            *center_cell = add(*center_cell, scale(ahead_dir, limits.start));
            if self.cfg.geometry == 2 {
                correct_torus_edge(center_cell, world_bounds);
            } else if !test_bounds(*center_cell, world_bounds) {
                // start cell extended beyond world bounds (can occur on
                // diagonals if dist from world edge < dist to closest edge
                // of resource)
                limits.visible = false;
                return;
            }

            if has_global && global_only {
                limits.end = 0;
                limits.start = 0;
                *tot_bounds = *world_bounds;
            }
        } else if d.habitat == -2 {
            *tot_bounds = *world_bounds;
        }
    }

    /// `WalkCells(...)` (bounded grids) and `WalkTorus(...)`: the walk over
    /// the cells of the sight cone, nearest distance first, each distance
    /// from the outermost side cells in towards the centre line.
    #[allow(clippy::too_many_arguments)]
    fn walk(
        &mut self,
        d: &LookInit,
        facing: i32,
        limits: &WalkLimits,
        mut center_cell: Coord,
        tot_bounds: &Bounds,
        world_bounds: &Bounds,
        val_res: &[i32],
        ahead_dir: Coord,
        torus: bool,
    ) -> WalkResult {
        let habitat_used = d.habitat;
        let search_type = d.search_type;
        let res_walk = habitat_used != -2 && habitat_used != 3;
        let mut r = WalkResult {
            found: false,
            found_edible: false,
            found_first_visible: false,
            first_visible_id: -9,
            first_visible_cell: (0, 0),
            first_success_cell: (-1, -1),
            first_whole_resource: -9,
            count: 0,
            total_amount: 0.0,
            dist_used: d.distance,
        };
        let mut do_left = true;
        let mut do_right = true;
        let mut count_center = true;
        let mut any_valid_side_cells = false;

        let diagonal = !matches!(facing, 0 | 2 | 4 | 6);
        let (left, right) = set_coords(facing);
        let stop_at_first_found =
            search_type == 0 || (habitat_used == -2 && (search_type == -1 || search_type == 1));

        // START WALKING
        let mut first_step = true;
        let mut dist = limits.start;
        while dist <= limits.end {
            if torus {
                if res_walk && !test_bounds(center_cell, tot_bounds) {
                    count_center = false;
                }
            } else {
                if !test_bounds(center_cell, world_bounds)
                    || (res_walk && !test_bounds(center_cell, tot_bounds))
                {
                    count_center = false;
                }
                // if looking l,r,u,d and center_cell is outside of the
                // world -- we're done with both sides and center
                if !diagonal && !count_center {
                    break;
                }
            }

            // work on SIDE of center cells for this distance
            let num_cells_either_side = if dist > 0 { cells_either_side(dist) } else { 0 };
            // look left then right
            for do_lr in 0..=1 {
                let direction = if do_lr == 1 { right } else { left };
                if !torus {
                    if !do_left && direction == left {
                        continue;
                    }
                    if !do_right && direction == right {
                        break;
                    }
                }

                // walk in from the farthest cell on side towards the center
                let mut j = num_cells_either_side;
                while j > 0 {
                    let mut valid_cell = true;
                    let mut this_cell = add(center_cell, scale(direction, j));
                    if torus {
                        correct_torus_edge(&mut this_cell, world_bounds);
                        // QUIRK: the side cell is admitted by the centre
                        // cell's resource bounds, as in the bounded walk.
                        if res_walk && !test_bounds(center_cell, tot_bounds) {
                            if diagonal {
                                break;
                            }
                            valid_cell = false;
                        } else {
                            any_valid_side_cells = true;
                        }
                    } else if !test_bounds(this_cell, world_bounds)
                        || (res_walk && !test_bounds(center_cell, tot_bounds))
                    {
                        // on diagonals...if any side cell is beyond specific
                        // parts of world bounds, we can exclude this side for
                        // this and any larger distances
                        if diagonal {
                            let (tcx, tcy) = this_cell;
                            let (wb, tb) = (world_bounds, tot_bounds);
                            if direction == left {
                                if (facing == 1 && tcy < wb.min_y)
                                    || (facing == 3 && tcx > wb.max_x)
                                    || (facing == 5 && tcy > wb.max_y)
                                    || (facing == 7 && tcx < wb.min_x)
                                    || (facing == 1 && tcy < tb.min_y)
                                    || (facing == 3 && tcx > tb.max_x)
                                    || (facing == 5 && tcy > tb.max_y)
                                    || (facing == 7 && tcx < tb.min_x)
                                {
                                    do_left = false;
                                }
                            } else if direction == right
                                && ((facing == 1 && tcx > wb.max_x)
                                    || (facing == 3 && tcy > wb.max_y)
                                    || (facing == 5 && tcx < wb.min_x)
                                    || (facing == 7 && tcy < wb.min_y)
                                    || (facing == 1 && tcx > tb.max_x)
                                    || (facing == 3 && tcy > tb.max_y)
                                    || (facing == 5 && tcx < tb.min_x)
                                    || (facing == 7 && tcy < tb.min_y))
                            {
                                do_right = false;
                            }
                            break;
                        }
                        valid_cell = false;
                    } else {
                        any_valid_side_cells = true;
                    }

                    // Now we can look at the current side cell because we
                    // know it's in the world.
                    if valid_cell {
                        let info =
                            self.test_cell(d, this_cell, val_res, first_step, stop_at_first_found);
                        first_step = false;
                        if self.tally(&mut r, &info, this_cell) && stop_at_first_found {
                            r.dist_used = dist;
                            break; // end search this side
                        }
                    }
                    j -= 1;
                }
                if stop_at_first_found && r.found_edible {
                    break; // end both side searches
                }
            }
            if stop_at_first_found && r.found_edible {
                break; // end side and center searches (found on side)
            }

            // work on CENTER cell for this dist
            if count_center {
                let info = self.test_cell(d, center_cell, val_res, first_step, stop_at_first_found);
                first_step = false;
                if self.tally(&mut r, &info, center_cell) && stop_at_first_found {
                    r.dist_used = dist;
                    break; // end side and center searches (found in center)
                }
            }
            // before we check cells at the next distance...
            // stop if we never found any valid cells at the current
            // distance; valid dist_used was previous set of cells checked
            if !any_valid_side_cells && !count_center {
                dist -= 1;
                r.dist_used = dist;
                break;
            }
            center_cell = add(center_cell, ahead_dir);
            if torus {
                correct_torus_edge(&mut center_cell, world_bounds);
            }
            dist += 1;
        } // END WALKING
        r
    }

    /// The bookkeeping after each `TestCell` of the walk. Returns whether
    /// the cell had something edible.
    fn tally(&mut self, r: &mut WalkResult, info: &SearchInfo, cell: Coord) -> bool {
        if !r.found_first_visible && info.has_some {
            r.first_visible_id = info.resource_id;
            r.first_visible_cell = cell;
            r.found_first_visible = true;
        }
        if info.amount_found > 0.0 {
            r.found = true;
            r.total_amount += info.amount_found;
            if info.has_edible {
                // count cells with individual edible resources (not sum of
                // res in cell >= threshold)
                r.count += 1;
                r.found_edible = true;
                if r.first_success_cell == (-1, -1) {
                    r.first_success_cell = cell;
                }
                if r.first_whole_resource == -9 {
                    r.first_whole_resource = info.resource_id;
                }
                return true;
            }
        }
        false
    }

    /// The "reached end output" tail of `WalkCells` / `WalkTorus`.
    fn report(
        &mut self,
        d: &LookInit,
        facing: i32,
        stuff_seen: &mut LookOut,
        r: &WalkResult,
        torus: bool,
    ) -> Result<(), String> {
        let worldx = self.cfg.world_x;
        let habitat_used = d.habitat;
        let search_type = d.search_type;
        let distance_sought = d.distance;
        stuff_seen.habitat = habitat_used;
        stuff_seen.search_type = search_type;
        stuff_seen.id_sought = d.id_sought;

        if !r.found && !r.found_first_visible {
            stuff_seen.report_type = 0;
        } else if !r.found && r.found_first_visible {
            stuff_seen.report_type = 1;
            stuff_seen.distance = r.dist_used;
            stuff_seen.count = 0;
            stuff_seen.value = 0;
            stuff_seen.group = r.first_visible_id;
            let target_cell = r.first_visible_cell.1 * worldx + r.first_visible_cell.0;
            self.find_thing(target_cell, distance_sought, facing, stuff_seen);
        } else if r.found {
            stuff_seen.report_type = 1;
            stuff_seen.distance = r.dist_used;
            stuff_seen.count = r.count;
            stuff_seen.value = r.total_amount as i32;
            stuff_seen.group = -9;
            stuff_seen.forage = -9;

            // overwrite defaults for more specific search types
            if habitat_used != -2 {
                // if we were looking for resources, return id of nearest
                stuff_seen.group = r.first_whole_resource;
                let target_cell = r.first_success_cell.1 * worldx + r.first_success_cell.0;
                self.find_thing(target_cell, distance_sought, facing, stuff_seen);
            } else if r.found_edible {
                // if searching for orgs, return info on closest one we
                // encountered (==only one if stop_at_first_found)
                let good_cell = r.first_success_cell.1 * worldx + r.first_success_cell.0;
                let mut first = self.w.cell_organism_uid(good_cell);
                if self.use_avatar() != 0 {
                    first = match search_type.cmp(&0) {
                        std::cmp::Ordering::Equal => self.w.cell_rand_av(good_cell),
                        std::cmp::Ordering::Greater => self.w.cell_first_pred_av(good_cell),
                        std::cmp::Ordering::Less => self.w.cell_first_prey_av(good_cell),
                    };
                }
                self.find_thing(good_cell, distance_sought, facing, stuff_seen);
                let Some(first_org) = first.and_then(|u| self.seen(u)) else {
                    return Err(format!(
                        "look-ahead: no organism in sighted cell {good_cell} (C++: NULL \
                         cOrganism* in cOrgSensor::WalkCells)"
                    ));
                };
                stuff_seen.id_sought = first_org.id;
                stuff_seen.value = first_org.cur_bonus as i32;
                stuff_seen.group = self.group_seen(&first_org);
                stuff_seen.forage = first_org.forage_target;
                self.note_seen_display(&first_org);
                if self.cfg.use_mimics && stuff_seen.forage == 1 {
                    // QUIRK: the bounded walk shows mimics without drawing
                    // against MIMIC_ODDS; the torus walk draws.
                    if !torus || self.w.rng().p(self.cfg.mimic_odds) {
                        stuff_seen.forage = first_org.show_ft;
                    }
                }
                if self.cfg.pred_confusion != 0
                    && first_org.forage_target > -2
                    && self.org.forage_target <= -2
                {
                    self.test_confusion(stuff_seen, &first_org)?;
                }
            }
            if self.cfg.use_display == 0 || self.cfg.use_display == 1 {
                self.set_potential_display_data(stuff_seen);
            }
        }
        Ok(())
    }

    /// `TestCell(ctx, in_defs, target_cell_coords, val_res, first_step,
    /// stop_at_first_found)`
    fn test_cell(
        &mut self,
        d: &LookInit,
        target: Coord,
        val_res: &[i32],
        first_step: bool,
        stop_at_first_found: bool,
    ) -> SearchInfo {
        let worldx = self.cfg.world_x;
        let target_cell_num = target.0 + target.1 * worldx;
        let mut info = SearchInfo::default();
        let habitat_used = d.habitat;
        let search_type = d.search_type;

        // if looking for resources or topological features
        if habitat_used != -2 && habitat_used != 3 {
            for &k in val_res {
                if !test_bounds(target, &self.solo_bounds[k as usize]) {
                    continue;
                }
                let cell_res = self.w.get_frozen_cell_res_val(target_cell_num, k);
                let rk = self.res(k);
                let edible_threshold = rk.threshold;
                if cell_res > 0.0 {
                    info.has_some = true;
                }
                if habitat_used == 0 || habitat_used > 5 {
                    if search_type == 0 && cell_res >= edible_threshold {
                        // get FIRST whole resource id
                        if !info.has_edible {
                            info.resource_id = k;
                        }
                        info.has_edible = true;
                        // avoid counting global res more than once (ever)
                        if first_step || !rk.global {
                            info.amount_found += (cell_res / edible_threshold).floor();
                        }
                    } else if search_type == 1
                        && cell_res < edible_threshold
                        && cell_res > 0.0
                        && (first_step || !rk.global)
                    {
                        info.amount_found += cell_res;
                    }
                } else if (habitat_used == 1 || habitat_used == 2) && cell_res > 0.0 {
                    // hills and walls work with any vals > 0
                    if !info.has_edible {
                        info.resource_id = k;
                    }
                    info.has_edible = true;
                    info.amount_found += cell_res;
                } else if habitat_used == 5 && cell_res > 0.0 {
                    // simulated predators work with any vals > 0 and have
                    // chance of detection failing
                    if self.w.rng().p(rk.prob_detect) {
                        if !info.has_edible {
                            info.resource_id = k;
                        }
                        info.has_edible = true;
                        info.amount_found += cell_res;
                    }
                } else if habitat_used == 4 {
                    if search_type == 0 && cell_res >= edible_threshold {
                        if !info.has_edible {
                            info.resource_id = k;
                        }
                        info.has_edible = true;
                        info.amount_found += (cell_res / edible_threshold).floor();
                    } else if search_type == 1 && cell_res < edible_threshold && cell_res > 0.0 {
                        info.amount_found += cell_res;
                    }
                }
                if stop_at_first_found && info.has_edible {
                    break;
                }
            }
        }
        // if we're looking for other organisms (looking for specific org
        // already handled)
        else if habitat_used == -2 {
            if self.use_avatar() == 0 {
                let seen = match self.w.cell_organism_uid(target_cell_num) {
                    Some(u) if u == self.org.uid => {
                        Some((self.org.forage_target, self.org.is_dead))
                    }
                    Some(u) => self.w.seen_org(u).map(|o| (o.forage_target, o.is_dead)),
                    None => None,
                };
                if let Some((type_seen, false)) = seen {
                    let counts = match search_type.cmp(&0) {
                        std::cmp::Ordering::Equal => true,
                        std::cmp::Ordering::Greater => type_seen == -2,
                        std::cmp::Ordering::Less => type_seen != -2,
                    };
                    if counts {
                        info.amount_found += 1.0;
                        info.has_edible = true;
                    }
                }
            }
            if self.use_avatar() == 2 {
                let (num_pred, num_prey) = self.w.cell_av_counts(target_cell_num);
                let n = match search_type.cmp(&0) {
                    std::cmp::Ordering::Equal => num_pred + num_prey,
                    std::cmp::Ordering::Greater => num_pred,
                    std::cmp::Ordering::Less => num_prey,
                };
                if n > 0 {
                    info.amount_found += f64::from(n);
                    info.has_edible = true;
                }
            }
        }
        info
    }

    /// `GetMinDist(worldx, bounds, cell_id, distance_sought, facing)`
    fn get_min_dist(
        &self,
        worldx: i32,
        bounds: &Bounds,
        cell_id: i32,
        distance_sought: i32,
        facing: i32,
    ) -> i32 {
        let org_x = cell_id % worldx;
        let org_y = cell_id / worldx;
        if org_x <= bounds.max_x
            && org_x >= bounds.min_x
            && org_y <= bounds.max_y
            && org_y >= bounds.min_y
        {
            return 0; // standing on it
        }

        // now for the direction
        let min_x = bounds.min_x.max(0);
        let min_y = bounds.min_y.max(0);
        let max_x = bounds.max_x.min(self.cfg.world_x - 1);
        let max_y = bounds.max_y.min(self.cfg.world_y - 1);

        // if completely behind you
        let behind = match facing {
            0 => min_y > org_y,
            4 => max_y < org_y,
            2 => max_x < org_x,
            6 => min_x > org_x,
            1 => min_y > org_y || max_x < org_x,
            3 => max_y < org_y || max_x < org_x,
            5 => max_y < org_y || min_x > org_x,
            7 => min_y > org_y || min_x > org_x,
            _ => false,
        };
        if behind {
            return -1;
        }

        // if not completely behind you, does min travel distance exceed
        // distance used?
        let inside_x = org_x > min_x && org_x < max_x;
        let inside_y = org_y > min_y && org_y < max_y;
        let travel_dist = match facing {
            0 => org_y - max_y,
            4 => min_y - org_y,
            2 => min_x - org_x,
            6 => org_x - max_x,
            1 if inside_x => org_y - max_y,
            1 if inside_y => min_x - org_x,
            1 => (org_x - min_x).abs().max((org_y - max_y).abs()),
            3 if inside_x => min_y - org_y,
            3 if inside_y => min_x - org_x,
            3 => (org_x - min_x).abs().max((org_y - min_y).abs()),
            5 if inside_x => min_y - org_y,
            5 if inside_y => org_x - max_x,
            5 => (org_x - max_x).abs().max((org_y - min_y).abs()),
            7 if inside_x => org_y - max_y,
            7 if inside_y => org_x - max_x,
            7 => (org_x - max_x).abs().max((org_y - max_y).abs()),
            _ => 0,
        };
        if travel_dist > distance_sought {
            return -1;
        }
        if travel_dist > 0 {
            return travel_dist;
        }
        // still going?
        0
    }

    /// `GetMaxDist(worldx, cell_id, distance_sought, bounds)`: the maximum
    /// possible distance to the farthest boundary.
    fn get_max_dist(worldx: i32, cell_id: i32, distance_sought: i32, bounds: &Bounds) -> i32 {
        let org_x = cell_id % worldx;
        let org_y = cell_id / worldx;
        let max_x_disp = (org_x - bounds.max_x)
            .abs()
            .max((org_x - bounds.min_x).abs());
        let max_y_disp = (org_y - bounds.max_y)
            .abs()
            .max((org_y - bounds.min_y).abs());
        max_x_disp.max(max_y_disp).min(distance_sought)
    }

    /// `GetBounds(ctx, res_id)`: the cells the resource's gradient uses
    /// (also stored as its `m_soloBounds` entry).
    fn get_bounds(&mut self, res_id: i32) -> Bounds {
        let (wx, wy) = (self.cfg.world_x, self.cfg.world_y);
        let mut b = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: wx - 1,
            max_y: wy - 1,
        };
        let (min_x, min_y, max_x, max_y) = self.w.resource_used_bounds(res_id);
        if min_x >= 0 && min_x < wx {
            b.min_x = min_x;
        }
        if min_y >= 0 && min_y < wy {
            b.min_y = min_y;
        }
        if max_x >= 0 && max_x < wx {
            b.max_x = max_x;
        }
        if max_y >= 0 && max_y < wy {
            b.max_y = max_y;
        }
        // no edible drawn in world
        if min_x == -1 && min_y == -1 && max_x == -1 && max_y == -1 {
            let this_cell = self.w.get_cell_id();
            b.min_x = this_cell % wx;
            b.max_x = b.min_x;
            b.min_y = this_cell / wx;
            b.max_y = b.min_y;
        }
        self.solo_bounds[res_id as usize] = b;
        b
    }

    /// `BuildResArray(in_defs, single_bound)`: hills and walls are treated
    /// as generic (no individual instances).
    fn build_res_array(&self, d: &LookInit, single_bound: bool) -> Vec<i32> {
        if single_bound {
            vec![d.id_sought]
        } else {
            (0..self.res.len() as i32)
                .filter(|&i| self.res(i).habitat == d.habitat)
                .collect()
        }
    }

    /// `SetPotentialDisplayData(stuff_seen)`. A fresh `new sOrgDisplay`
    /// leaves `message` indeterminate in C++; it starts at 0 here.
    fn set_potential_display_data(&mut self, stuff_seen: &LookOut) {
        let mut display = self.org.queued_display.take().unwrap_or_default();
        display.distance = OrgSensor::find_distance_from_home(self.org);
        display.direction = OrgSensor::find_dir_from_home(self.org);
        display.thing_id = if stuff_seen.habitat == -2 {
            stuff_seen.id_sought
        } else {
            stuff_seen.group
        };
        display.value = stuff_seen.value;
        self.sensor.set_return_relative_facing(false);
        self.org.queued_display = Some(display);
    }
}
