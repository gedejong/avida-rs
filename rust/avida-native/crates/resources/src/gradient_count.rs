//! Moving peaks, halos, walls, hills and probabilistic resources.
//!
//! Ports `cGradientCount` (`avida-core/source/main/cGradientCount.{h,cc}`,
//! reference bodies from git `47f13dadb`; unchanged since `ef5c6d948`).
//!
//! From the C++ file comment: peaks are cones of slope `height / distance`
//! capped with plateaus (`height` is the radius of the edible part, `spread`
//! the radius of the whole cone). Peaks move inside `min/max x/y` (bouncing
//! off the edges) with a smoothness set by `move_a_scaler` (the A of eq. 1
//! in Morrison & De Jong 1999); halo peaks orbit an anchor instead.
//! Depletable peaks stop moving once bitten and are regenerated after
//! `decay` updates or once eaten up. `habitat` 1 makes hills, 2 walls.
//!
//! Robustness: cell and plateau-array accesses that are out of range in the
//! C++ (undefined behaviour, reachable only through misconfiguration such
//! as a wall start or probabilistic centre outside the world, a
//! `ResetGradRes` that stopped at a configuration error and left the grid
//! empty, or `set_grad_height` without a reset) read 0 and skip the write
//! here instead of panicking.
//!
//! Floating point: the C++ expressions `a + b - (c * d)` and `x + (s * y)`
//! are fused by the reference build and use `mul_add` here (marked FMA).

use avida_apto::AvidaRng;

use crate::env::ResourceEnv;
use crate::spatial_res_count::SpatialResCount;

/// The gradient-resource parameters `cResourceCount::Setup` and
/// `cResourceCount::SetGradientCount` take from `cResource`, under their
/// `cResource` getter names. [`Default`] gives the `cResource` constructor
/// defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientParams {
    /// `GetPeakX()`; `-1` picks a random position.
    pub peakx: i32,
    /// `GetPeakY()`; `-1` picks a random position.
    pub peaky: i32,
    /// `GetHeight()`
    pub height: i32,
    /// `GetSpread()`
    pub spread: i32,
    /// `GetPlateau()`; negative turns plateaus off.
    pub plateau: f64,
    /// `GetDecay()` (updates a bitten peak lingers; an `int`, unlike the
    /// global decay rate).
    pub decay: i32,
    /// `GetMaxX()`
    pub max_x: i32,
    /// `GetMinX()`
    pub min_x: i32,
    /// `GetMaxY()`
    pub max_y: i32,
    /// `GetMinY()`
    pub min_y: i32,
    /// `GetAscaler()`
    pub move_a_scaler: f64,
    /// `GetUpdateStep()`
    pub updatestep: i32,
    /// `GetHalo()`
    pub halo: i32,
    /// `GetHaloInnerRadius()`
    pub halo_inner_radius: i32,
    /// `GetHaloWidth()`
    pub halo_width: i32,
    /// `GetHaloAnchorX()`
    pub halo_anchor_x: i32,
    /// `GetHaloAnchorY()`
    pub halo_anchor_y: i32,
    /// `GetMoveSpeed()`; negative values skip moves.
    pub move_speed: i32,
    /// `GetMoveResistance()`
    pub move_resistance: i32,
    /// `GetPlateauInflow()`
    pub plateau_inflow: f64,
    /// `GetPlateauOutflow()`
    pub plateau_outflow: f64,
    /// `GetConeInflow()`
    pub cone_inflow: f64,
    /// `GetConeOutflow()`
    pub cone_outflow: f64,
    /// `GetGradientInflow()`
    pub gradient_inflow: f64,
    /// `GetIsPlateauCommon()`
    pub is_plateau_common: i32,
    /// `GetFloor()`
    pub floor: f64,
    /// `GetHabitat()` (1 hills, 2 walls, others peaks)
    pub habitat: i32,
    /// `GetMinSize()`
    pub min_size: i32,
    /// `GetMaxSize()`
    pub max_size: i32,
    /// `GetConfig()`
    pub config: i32,
    /// `GetCount()`
    pub count: i32,
    /// `GetResistance()` (accepted but unused by the C++ resource code)
    pub resistance: f64,
    /// `GetDamage()`
    pub damage: f64,
    /// `GetDeathOdds()`
    pub death_odds: f64,
    /// `IsPath()`
    pub path: i32,
    /// `IsHammer()`
    pub hammer: i32,
    /// `GetInitialPlatVal()`; `-1` means unset.
    pub init_plat: f64,
    /// `GetThreshold()`
    pub threshold: f64,
    /// `GetRefuge()` (accepted but unused by the C++ resource code)
    pub refuge: i32,
}

impl Default for GradientParams {
    /// The `cResource(const cString&, int)` constructor defaults.
    fn default() -> Self {
        GradientParams {
            peakx: -1,
            peaky: -1,
            height: 0,
            spread: 0,
            plateau: -1.0,
            decay: 1,
            max_x: 0,
            min_x: 0,
            max_y: 0,
            min_y: 0,
            move_a_scaler: 1.0,
            updatestep: 1,
            halo: 0,
            halo_inner_radius: 0,
            halo_width: 0,
            halo_anchor_x: 0,
            halo_anchor_y: 0,
            move_speed: 1,
            move_resistance: 0,
            plateau_inflow: 0.0,
            plateau_outflow: 0.0,
            cone_inflow: 0.0,
            cone_outflow: 0.0,
            gradient_inflow: 0.0,
            is_plateau_common: 0,
            floor: 0.0,
            habitat: 0,
            min_size: 1,
            max_size: 1,
            config: 0,
            count: 1,
            resistance: 1.0,
            damage: 0.0,
            death_odds: 0.0,
            path: 0,
            hammer: 0,
            init_plat: -1.0,
            threshold: 1.0,
            refuge: 0,
        }
    }
}

/// `rng.GetUInt(min, max)` with `int` arguments: both convert to `unsigned`
/// (negative values wrap) and the result converts back to `int`.
#[inline]
fn uint_range(rng: &mut AvidaRng, min: i32, max: i32) -> i32 {
    rng.get_uint_range(min as u32, max as u32) as i32
}

/// `rng.GetUInt(max)` with an `int` argument.
#[inline]
fn uint_max(rng: &mut AvidaRng, max: i32) -> i32 {
    rng.get_uint(max as u32) as i32
}

/// `sqrt((double) dx * dx + dy * dy)` as written in `fillinResourceValues`,
/// `generateHills` and `BuildProbabilisticRes`: the `y` product is an `int`
/// multiply; the `x` product feeds the add and is fused (FMA, exact anyway
/// for grid-sized values).
#[inline]
fn cell_distance(dx: i32, dy: i32) -> f64 {
    let dxf = f64::from(dx);
    dxf.mul_add(dxf, f64::from(dy.wrapping_mul(dy))).sqrt()
}

/// `Apto::Swap(arr[a], arr[b])`, skipped when either index is outside the
/// array (a probabilistic centre outside the world, C++ UB).
fn swap_in_range(v: &mut [i32], a: i32, b: i32) {
    if let (Ok(a), Ok(b)) = (usize::try_from(a), usize::try_from(b)) {
        if a < v.len() && b < v.len() {
            v.swap(a, b);
        }
    }
}

/// `cGradientCount`: a [`SpatialResCount`] whose cell values are generated
/// (cones with plateaus, halos, walls, hills, probabilistic patches) and
/// moved once per update by [`update_count`](Self::update_count).
#[derive(Clone, Debug, PartialEq)]
pub struct GradientCount {
    /// The `cSpatialResCount` base-class part.
    pub base: SpatialResCount,

    // Configuration arguments.
    pub(crate) peakx: i32,
    pub(crate) peaky: i32,
    pub(crate) height: i32,
    pub(crate) spread: i32,
    pub(crate) plateau: f64,
    pub(crate) decay: i32,
    pub(crate) max_x: i32,
    pub(crate) max_y: i32,
    pub(crate) min_x: i32,
    pub(crate) min_y: i32,
    pub(crate) move_a_scaler: f64,
    pub(crate) updatestep: i32,
    pub(crate) halo: i32,
    pub(crate) halo_inner_radius: i32,
    pub(crate) halo_width: i32,
    pub(crate) halo_anchor_x: i32,
    pub(crate) halo_anchor_y: i32,
    pub(crate) move_speed: i32,
    pub(crate) move_resistance: i32,
    pub(crate) plateau_inflow: f64,
    pub(crate) plateau_outflow: f64,
    pub(crate) cone_inflow: f64,
    pub(crate) cone_outflow: f64,
    pub(crate) gradient_inflow: f64,
    pub(crate) is_plateau_common: i32,
    pub(crate) floor: f64,
    pub(crate) habitat: i32,
    pub(crate) min_size: i32,
    pub(crate) max_size: i32,
    pub(crate) config: i32,
    pub(crate) count: i32,
    pub(crate) initial_plat: f64,
    pub(crate) threshold: f64,
    pub(crate) damage: f64,
    pub(crate) geometry: i32,

    // Internal values.
    pub(crate) initial: bool,
    pub(crate) move_y_scaler: f64,
    pub(crate) counter: i32,
    pub(crate) move_counter: i32,
    pub(crate) topo_counter: i32,
    pub(crate) movesignx: i32,
    pub(crate) movesigny: i32,
    pub(crate) old_peakx: i32,
    pub(crate) old_peaky: i32,
    pub(crate) halo_dir: i32,
    pub(crate) changling: i32,
    pub(crate) just_reset: bool,
    pub(crate) past_height: f64,
    pub(crate) current_height: f64,
    pub(crate) ave_plat_cell_loss: f64,
    pub(crate) common_plat_height: f64,
    pub(crate) skip_moves: i32,
    pub(crate) skip_counter: i32,
    pub(crate) plateau_array: Vec<f64>,
    pub(crate) plateau_cell_ids: Vec<i32>,
    pub(crate) wall_cells: Vec<i32>,
    pub(crate) mean_plat_inflow: f64,
    pub(crate) var_plat_inflow: f64,
    pub(crate) pred_odds: f64,
    pub(crate) predator: bool,
    pub(crate) death_odds: f64,
    pub(crate) deadly: bool,
    pub(crate) path: i32,
    pub(crate) hammer: i32,
    pub(crate) guarded_juvs_per_adult: i32,
    pub(crate) probabilistic: bool,
    pub(crate) prob_res_cells: Vec<i32>,
    pub(crate) min_usedx: i32,
    pub(crate) min_usedy: i32,
    pub(crate) max_usedx: i32,
    pub(crate) max_usedy: i32,
}

impl GradientCount {
    /// `cGradientCount(cWorld*, peakx, ..., worldx, worldy, geometry, ...)`:
    /// stores the parameters, then `ResetGradRes(m_world->GetDefaultContext(),
    /// worldx, worldy)`, which sizes the grid and places (and, for moving
    /// peaks, already moves) the resource.
    ///
    /// `resistance` and `refuge` are not constructor arguments in the C++.
    /// `halo_dir` and `changling`, uninitialised in the C++ until a moving
    /// halo is generated, start at 0.
    pub fn new(
        p: &GradientParams,
        worldx: i32,
        worldy: i32,
        geometry: i32,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
    ) -> Self {
        let mut g = GradientCount {
            base: SpatialResCount::new(),
            peakx: p.peakx,
            peaky: p.peaky,
            height: p.height,
            spread: p.spread,
            plateau: p.plateau,
            decay: p.decay,
            max_x: p.max_x,
            max_y: p.max_y,
            min_x: p.min_x,
            min_y: p.min_y,
            move_a_scaler: p.move_a_scaler,
            updatestep: p.updatestep,
            halo: p.halo,
            halo_inner_radius: p.halo_inner_radius,
            halo_width: p.halo_width,
            halo_anchor_x: p.halo_anchor_x,
            halo_anchor_y: p.halo_anchor_y,
            move_speed: p.move_speed,
            move_resistance: p.move_resistance,
            plateau_inflow: p.plateau_inflow,
            plateau_outflow: p.plateau_outflow,
            cone_inflow: p.cone_inflow,
            cone_outflow: p.cone_outflow,
            gradient_inflow: p.gradient_inflow,
            is_plateau_common: p.is_plateau_common,
            floor: p.floor,
            habitat: p.habitat,
            min_size: p.min_size,
            max_size: p.max_size,
            config: p.config,
            count: p.count,
            initial_plat: p.init_plat,
            threshold: p.threshold,
            damage: p.damage,
            geometry,
            initial: false,
            move_y_scaler: 0.5,
            counter: 0,
            move_counter: 1,
            topo_counter: p.updatestep,
            movesignx: 0,
            movesigny: 0,
            old_peakx: p.peakx,
            old_peaky: p.peaky,
            halo_dir: 0,
            changling: 0,
            just_reset: true,
            past_height: 0.0,
            current_height: 0.0,
            ave_plat_cell_loss: 0.0,
            common_plat_height: 0.0,
            skip_moves: 0,
            skip_counter: 0,
            plateau_array: Vec::new(),
            plateau_cell_ids: Vec::new(),
            wall_cells: Vec::new(),
            mean_plat_inflow: p.plateau_inflow,
            var_plat_inflow: 0.0,
            pred_odds: 0.0,
            predator: false,
            death_odds: p.death_odds,
            deadly: p.death_odds != 0.0,
            path: p.path,
            hammer: p.hammer,
            guarded_juvs_per_adult: 0,
            probabilistic: false,
            prob_res_cells: Vec::new(),
            min_usedx: -1,
            min_usedy: -1,
            max_usedx: -1,
            max_usedy: -1,
        };
        g.reset_grad_res(rng, env, worldx, worldy);
        g
    }

    #[inline]
    fn gx(&self) -> i32 {
        self.base.world_x
    }

    #[inline]
    fn gy(&self) -> i32 {
        self.base.world_y
    }

    /// `Element(i).GetAmount()`; 0 for a cell outside the grid (C++ UB).
    #[inline]
    fn amt(&self, idx: i32) -> f64 {
        usize::try_from(idx)
            .ok()
            .and_then(|i| self.base.grid.get(i))
            .map_or(0.0, |e| e.amount)
    }

    /// `Element(i).SetAmount(v)`; skipped for a cell outside the grid (C++ UB).
    #[inline]
    fn set_amt(&mut self, idx: i32, v: f64) {
        if let Some(e) = usize::try_from(idx)
            .ok()
            .and_then(|i| self.base.grid.get_mut(i))
        {
            e.amount = v;
        }
    }

    /// `temp_height`, repeated in several C++ methods: 1 without plateaus.
    #[inline]
    fn temp_height(&self) -> i32 {
        if self.plateau < 0.0 {
            1
        } else {
            self.height
        }
    }

    /// `cGradientCount::StateAll()`: does nothing (deltas accumulated by
    /// inflow/outflow/flow are not folded; a later `ModifyCell` on a cell
    /// folds that cell's delta, as in the C++).
    pub fn state_all(&mut self) {}

    /// `UpdateCount(cAvidaContext&)`: once per update; regenerates walls or
    /// hills, applies probabilistic inflow/outflow, or moves/refreshes the
    /// peak.
    pub fn update_count(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv) {
        self.old_peakx = self.peakx;
        self.old_peaky = self.peaky;
        if self.habitat == 2 {
            self.generate_barrier(rng);
        } else if self.habitat == 1 {
            self.generate_hills(rng);
        } else if self.probabilistic {
            self.update_probabilistic_res();
        } else {
            self.update_peak_res(rng, env);
        }
    }

    /// `updatePeakRes(cAvidaContext&)`
    fn update_peak_res(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv) {
        let mut has_edible = false;

        // Only look for edible food within the spread of the peak, and only
        // for decay > 1.
        if self.decay > 1 {
            let max_pos_x = (self.peakx + self.spread + 1).min(self.gx() - 1);
            let min_pos_x = (self.peakx - self.spread - 1).max(0);
            let max_pos_y = (self.peaky + self.spread + 1).min(self.gy() - 1);
            let min_pos_y = (self.peaky - self.spread - 1).max(0);
            for ii in min_pos_x..max_pos_x + 1 {
                for jj in min_pos_y..max_pos_y + 1 {
                    if self.amt(jj * self.gx() + ii) >= 1.0 {
                        has_edible = true;
                        // As in the C++, this leaves only the inner loop.
                        break;
                    }
                }
            }
        }

        // Once bitten, start the clock towards a refresh of the whole peak.
        if has_edible && self.base.modified && self.decay > 1 {
            self.counter += 1;
        }

        if has_edible && self.counter < self.decay && self.base.modified {
            if self.predator {
                self.update_predatory_res(rng, env);
            }
            if self.damage != 0.0 {
                self.update_damaging_res(rng, env);
            }
            if self.deadly {
                self.update_deadly_res(rng, env);
            }
            return;
        }

        // Before moving a depletable resource, record the current plateau values.
        if self.decay == 1 {
            self.get_current_plat_values();
        }

        // When the counter matches decay, regenerate the peak.
        if self.counter == self.decay {
            self.generate_peak(rng, env);
        }

        // For moving peaks, update the y-scaler (logistic map).
        if self.move_a_scaler > 1.0 {
            self.move_y_scaler =
                self.move_a_scaler * self.move_y_scaler * (1.0 - self.move_y_scaler);
        }

        // Move every update, or every skip_moves updates.
        if self.skip_counter == self.skip_moves {
            self.move_res(rng);
        } else {
            self.skip_counter += 1;
        }

        if self.move_a_scaler > 1.0
            || self.plateau_inflow != 0.0
            || self.plateau_outflow != 0.0
            || self.cone_inflow != 0.0
            || self.cone_outflow != 0.0
            || self.gradient_inflow != 0.0
            || (self.move_a_scaler == 1.0 && self.just_reset)
        {
            self.fillin_resource_values(env);
        }

        if self.predator {
            self.update_predatory_res(rng, env);
        }
        if self.damage != 0.0 {
            self.update_damaging_res(rng, env);
        }
        if self.deadly {
            self.update_deadly_res(rng, env);
        }
    }

    /// `generatePeak(cAvidaContext&)`: (re)place the peak (random within the
    /// bounds when unset, or on the halo) and pick movement directions.
    fn generate_peak(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv) {
        let temp_height = self.temp_height();
        if self.halo == 0 {
            if self.peakx == -1 {
                self.peakx =
                    uint_range(rng, self.min_x + temp_height, self.max_x - temp_height + 1);
            }
            if self.peaky == -1 {
                self.peaky =
                    uint_range(rng, self.min_y + temp_height, self.max_y - temp_height + 1);
            }
            if self.move_a_scaler > 1.0 {
                // Random x direction; if it is 0, the y direction must not be.
                self.movesignx = rng.get_int_range(-1, 2);
                if self.movesignx == 0 {
                    self.movesigny = if uint_range(rng, 0, 2) == 1 { -1 } else { 1 };
                } else {
                    self.movesigny = rng.get_int_range(-1, 2);
                }
            }
        } else {
            // Halos: a random location on the orbit.
            if self.move_a_scaler > 1.0 {
                self.halo_dir = if uint_range(rng, 0, 2) == 0 { -1 } else { 1 };
                self.set_halo_direction(rng);
                self.changling = if uint_range(rng, 0, 2) == 1 { -1 } else { 1 };
            }
            let (ax, ay) = (self.halo_anchor_x, self.halo_anchor_y);
            let (ir, w, th) = (self.halo_inner_radius, self.halo_width, temp_height);
            let choose_up_down = uint_range(rng, 0, 2);
            if choose_up_down == 0 {
                let choose_ew = uint_range(rng, 0, 2);
                if choose_ew == 0 {
                    self.peakx = uint_range(rng, (ax - ir - w + th + 1).max(0), ax - ir - th);
                } else {
                    self.peakx = uint_range(rng, (ax + ir + th + 1).max(0), ax + ir + w - th);
                }
                self.peaky = uint_range(rng, (ay - ir - w + th + 1).max(0), ay + ir + w - th);
            } else {
                let choose_ns = uint_range(rng, 0, 2);
                if choose_ns == 0 {
                    self.peaky = uint_range(rng, (ay - ir - w + th + 1).max(0), ay - ir - th);
                } else {
                    self.peaky = uint_range(rng, (ay + ir + th + 1).max(0), ay + ir + w - th);
                }
                self.peakx = uint_range(rng, (ax - ir - w + th + 1).max(0), ax + ir + w - th);
            }
        }

        self.base.set_modified(false);
        self.counter = 0;
        self.skip_counter = 0;
        self.just_reset = true;
        self.fillin_resource_values(env);
    }

    /// `fillinResourceValues()`: recompute the cone/plateau values around the
    /// peak, applying plateau, cone and gradient inflow/outflow.
    fn fillin_resource_values(&mut self, env: &mut dyn ResourceEnv) {
        self.reset_used_bounds();
        let (max_pos_x, min_pos_x, max_pos_y, min_pos_y);
        if self.just_reset {
            // After resetUsedBounds() above the whole world is always used
            // (the used-bounds branch is dead in the C++ too).
            if self.min_usedx == -1
                || self.min_usedy == -1
                || self.max_usedx == -1
                || self.max_usedy == -1
            {
                max_pos_x = self.gx() - 1;
                min_pos_x = 0;
                max_pos_y = self.gy() - 1;
                min_pos_y = 0;
            } else {
                max_pos_x = self.max_usedx;
                min_pos_x = self.min_usedx;
                max_pos_y = self.max_usedy;
                min_pos_y = self.min_usedy;
            }
        } else {
            // Within reach of the peak, back to move_speed to clear residue.
            max_pos_x = (self.peakx + self.spread + self.move_speed + 1).min(self.gx() - 1);
            min_pos_x = (self.peakx - self.spread - self.move_speed - 1).max(0);
            max_pos_y = (self.peaky + self.spread + self.move_speed + 1).min(self.gy() - 1);
            min_pos_y = (self.peaky - self.spread - self.move_speed - 1).max(0);
        }

        let update = env.update();
        if self.is_plateau_common == 1 && !self.just_reset && update > 0 {
            // Common depletion: the new peak height is the old one moved by
            // the plateau delta. FMA: `a - b + c - (d * e)`.
            self.current_height = (-self.current_height).mul_add(
                self.plateau_outflow,
                self.current_height - self.ave_plat_cell_loss + self.plateau_inflow,
            );
            self.common_plat_height = (-self.current_height).mul_add(
                self.plateau_outflow,
                self.common_plat_height - self.ave_plat_cell_loss + self.plateau_inflow,
            );
            if self.common_plat_height > self.plateau && self.plateau >= 0.0 {
                self.common_plat_height = self.plateau;
            }
            if self.common_plat_height < 0.0 && self.plateau >= 0.0 {
                self.common_plat_height = 0.0;
            }
            if self.current_height > f64::from(self.height) && self.plateau >= 0.0 {
                self.current_height = f64::from(self.height);
            }
            if self.current_height < 0.0 && self.plateau >= 0.0 {
                self.current_height = 0.0;
            }
        } else {
            self.current_height = f64::from(self.height);
        }

        let gx = self.gx();
        let gy = self.gy();
        let mut plateau_cell = 0usize;
        for ii in min_pos_x..max_pos_x + 1 {
            for jj in min_pos_y..max_pos_y + 1 {
                let mut thisheight = 0.0;
                let thisdist = cell_distance(self.peakx - ii, self.peaky - jj);
                if f64::from(self.spread) >= thisdist {
                    // Theoretical cell value; distance + 1 so the centre is radius 1.
                    thisheight = self.current_height / (thisdist + 1.0);

                    // Floor values; plateaus override this so they can reach 0.
                    if thisheight < self.floor {
                        thisheight = self.floor;
                    }

                    // Plateau cells: where height / (dist + 1) >= 1.
                    let is_plat_cell = (f64::from(self.height) / (thisdist + 1.0)) >= 1.0;
                    if (is_plat_cell && self.plateau >= 0.0)
                        || (self.plateau < 0.0 && thisdist == 0.0 && !self.plateau_array.is_empty())
                    {
                        if self.just_reset || update <= 0 {
                            self.past_height = f64::from(self.height);
                            thisheight = if self.plateau >= 0.0 {
                                self.plateau
                            } else {
                                f64::from(self.height)
                            };
                        } else if self.is_plateau_common == 0 {
                            self.past_height =
                                self.plateau_array.get(plateau_cell).copied().unwrap_or(0.0);
                            // FMA: `past + inflow - (past * outflow)`.
                            thisheight = (-self.past_height).mul_add(
                                self.plateau_outflow,
                                self.past_height + self.plateau_inflow,
                            );
                            thisheight += self.gradient_inflow / (thisdist + 1.0);
                            if thisheight > self.plateau && self.plateau >= 0.0 {
                                thisheight = self.plateau;
                            }
                            if self.plateau < 0.0
                                && thisdist == 0.0
                                && thisheight > f64::from(self.height)
                            {
                                thisheight = f64::from(self.height);
                            }
                        } else if self.is_plateau_common == 1 {
                            thisheight = self.common_plat_height;
                        }
                        if self.initial && self.initial_plat != -1.0 {
                            thisheight = self.initial_plat;
                        }
                        if thisheight < 0.0 {
                            thisheight = 0.0;
                        }
                        if let Some(slot) = self.plateau_array.get_mut(plateau_cell) {
                            *slot = thisheight;
                        }
                        if let Some(slot) = self.plateau_cell_ids.get_mut(plateau_cell) {
                            *slot = jj * gx + ii;
                        }
                        plateau_cell += 1;
                    } else if !is_plat_cell
                        && (self.cone_inflow > 0.0
                            || self.cone_outflow > 0.0
                            || self.gradient_inflow > 0.0)
                        && !self.just_reset
                        && update > 0
                    {
                        // Off-plateau inflow and outflow, following the cell
                        // this one was before the peak moved.
                        let old_cell_x = ii + (self.old_peakx - self.peakx);
                        let old_cell_y = jj + (self.old_peaky - self.peaky);
                        if old_cell_x < 0
                            || old_cell_y < 0
                            || old_cell_y > gy - 1
                            || old_cell_x > gx - 1
                        {
                            // Cone cells that moved onto the world start at 0.
                            thisheight = 0.0;
                        } else {
                            let past_height = self.amt(old_cell_y * gx + old_cell_x);
                            let mut newheight = past_height;
                            if self.cone_inflow > 0.0 || self.cone_outflow > 0.0 {
                                // FMA: `cone_inflow - (past_height * cone_outflow)`.
                                newheight +=
                                    (-past_height).mul_add(self.cone_outflow, self.cone_inflow);
                            }
                            if self.gradient_inflow > 0.0 {
                                newheight += self.gradient_inflow / (thisdist + 1.0);
                            }
                            // Don't exceed the expected slope value.
                            if newheight < thisheight {
                                thisheight = newheight;
                            }
                            if thisheight < 0.0 {
                                thisheight = 0.0;
                            }
                        }
                    }
                }
                self.set_amt(jj * gx + ii, thisheight);
                if thisheight > 0.0 {
                    self.update_bounds(ii, jj);
                }
            }
        }
        self.base.set_curr_peak_x(self.peakx);
        self.base.set_curr_peak_y(self.peaky);
        self.just_reset = false;
    }

    /// `getCurrentPlatValues()`: record what organisms ate from the plateau
    /// since the last fill and the average loss per plateau cell.
    ///
    /// The box is not clipped to the world (unlike `fillinResourceValues`),
    /// so near an edge the plateau indices can pair up with other cells'
    /// stored values, as in the C++.
    fn get_current_plat_values(&mut self) {
        let temp_height = self.temp_height();
        let plateau_box_min_x = self.peakx - temp_height - 1;
        let plateau_box_max_x = self.peakx + temp_height + 1;
        let plateau_box_min_y = self.peaky - temp_height - 1;
        let plateau_box_max_y = self.peaky + temp_height + 1;
        let mut plateau_cell = 0usize;
        let mut amount_devoured = 0.0;
        for ii in plateau_box_min_x..plateau_box_max_x + 1 {
            for jj in plateau_box_min_y..plateau_box_max_y + 1 {
                let dx = f64::from(self.peakx - ii);
                let dy = f64::from(self.peaky - jj);
                // FMA: `dx * dx + dy * dy` (exact for grid-sized values).
                let thisdist = dx.mul_add(dx, dy * dy).sqrt();
                let find_plat_dist = f64::from(temp_height) / (thisdist + 1.0);
                if (find_plat_dist >= 1.0 && self.plateau >= 0.0)
                    || (self.plateau < 0.0 && thisdist == 0.0 && !self.plateau_array.is_empty())
                {
                    let past_cell_height =
                        self.plateau_array.get(plateau_cell).copied().unwrap_or(0.0);
                    let pre_move_height = self.amt(
                        self.plateau_cell_ids
                            .get(plateau_cell)
                            .copied()
                            .unwrap_or(-1),
                    );
                    if pre_move_height < past_cell_height {
                        if let Some(slot) = self.plateau_array.get_mut(plateau_cell) {
                            *slot = pre_move_height;
                        }
                        amount_devoured = amount_devoured + past_cell_height - pre_move_height;
                    }
                    plateau_cell += 1;
                }
            }
        }
        self.ave_plat_cell_loss = amount_devoured / plateau_cell as f64;
    }

    /// `moveRes(cAvidaContext&)`
    fn move_res(&mut self, rng: &mut AvidaRng) {
        // For halo peaks, the current orbit.
        let mut current_orbit = (self.halo_anchor_x - self.peakx)
            .abs()
            .max((self.halo_anchor_y - self.peaky).abs());

        // Time to update the direction of a moving resource?
        if self.move_counter == self.updatestep && self.move_a_scaler > 1.0 {
            self.move_counter = 1;
            if self.halo == 1 {
                current_orbit = self.set_halo_orbit(rng, current_orbit);
            } else {
                self.set_peak_move_movement(rng);
            }
        } else {
            self.move_counter += 1;
        }

        if self.move_a_scaler > 1.0 {
            if self.halo == 1 && self.move_a_scaler > 1.0 {
                self.move_halo_peak(current_orbit);
            } else {
                self.move_peak();
            }
        }
        self.skip_counter = 0;
    }

    /// `setPeakMoveMovement(cAvidaContext&)`: change one axis' direction by
    /// at most one step.
    fn set_peak_move_movement(&mut self, rng: &mut AvidaRng) {
        let choosesign = rng.get_int_range(1, 3);
        if choosesign == 1 {
            self.movesignx = match self.movesignx {
                -1 => rng.get_int_range(-1, 1),
                1 => rng.get_int_range(0, 2),
                _ => rng.get_int_range(-1, 2),
            };
        } else if choosesign == 2 {
            self.movesigny = match self.movesigny {
                -1 => rng.get_int_range(-1, 1),
                1 => rng.get_int_range(0, 2),
                _ => rng.get_int_range(-1, 2),
            };
        }
    }

    /// `setHaloOrbit(cAvidaContext&, int current_orbit)`: either move one
    /// orbit in or out (if the halo is wide enough) or change direction.
    // The bounds tests keep the C++ form (`a + orbit - 1 >= GetY()`).
    #[allow(clippy::int_plus_one)]
    fn set_halo_orbit(&mut self, rng: &mut AvidaRng, mut current_orbit: i32) -> i32 {
        // Change orbit (0) or direction (1).
        let mut random_shift = uint_range(rng, 0, 2);
        let temp_height = self.temp_height();

        if random_shift == 0 {
            // Only if there is room to change orbit.
            if self.halo_width > temp_height * 2 {
                let mut orbit_shift = 1;
                if uint_range(rng, 0, 2) == 0 {
                    orbit_shift = -1;
                }
                current_orbit += orbit_shift;
                // Out of the halo (or the world): go the other way instead.
                if current_orbit > (self.halo_inner_radius + self.halo_width - temp_height - 1)
                    || current_orbit < (self.halo_inner_radius + temp_height + 1)
                    || self.halo_anchor_y + current_orbit - 1 >= self.gy()
                    || self.halo_anchor_x + current_orbit - 1 >= self.gx()
                    || self.halo_anchor_y - current_orbit + 1 < 0
                    || self.halo_anchor_x - current_orbit + 1 < 0
                {
                    orbit_shift *= -1;
                    current_orbit += 2 * orbit_shift;
                }

                if current_orbit < 0 {
                    // Went past the anchor to the other side.
                    current_orbit = current_orbit.abs();
                }

                if (self.halo_anchor_y - self.peaky).abs() > (self.halo_anchor_x - self.peakx).abs()
                {
                    if self.halo_anchor_y > self.peaky {
                        self.peaky = self.old_peaky - orbit_shift;
                    } else {
                        self.peaky = self.old_peaky + orbit_shift;
                    }
                } else if self.halo_anchor_x > self.peakx {
                    self.peakx = self.old_peakx - orbit_shift;
                } else {
                    self.peakx = self.old_peakx + orbit_shift;
                }
            } else {
                // No room to change orbit: change direction instead.
                random_shift = 1;
            }
        }
        if random_shift == 1 {
            self.set_halo_direction(rng);
        }
        current_orbit
    }

    /// `setHaloDirection(cAvidaContext&)`: -1, 1, or (biased by move
    /// resistance) 0.
    fn set_halo_direction(&mut self, rng: &mut AvidaRng) {
        let move_rand = if self.move_resistance > 0 {
            uint_max(rng, 2 + self.move_resistance)
        } else {
            uint_max(rng, 3)
        };
        self.halo_dir = match move_rand {
            0 => -1,
            1 => 1,
            _ => 0,
        };
    }

    /// `moveHaloPeak(int current_orbit)`: move along the (square) orbit,
    /// turning corners.
    fn move_halo_peak(&mut self, current_orbit: i32) {
        // The quadrant decides whether x or y changes; on a corner, keep it.
        let dy = (self.halo_anchor_y - self.peaky).abs();
        let dx = (self.halo_anchor_x - self.peakx).abs();
        if dy > dx {
            self.changling = 1;
        } else if dy < dx {
            self.changling = -1;
        }

        if self.changling == 1 {
            let max_orbit_x = self.halo_anchor_x + current_orbit - 1;
            let min_orbit_x = self.halo_anchor_x - current_orbit + 1;
            let next_posx = self.peakx + (self.halo_dir * self.move_speed);
            let current_x = self.peakx;
            if next_posx > max_orbit_x {
                // Turn the corner; the sign flips when switching from x to y.
                self.peakx = max_orbit_x;
                if self.peaky > self.halo_anchor_y {
                    self.halo_dir *= -1;
                }
                self.peaky += self.halo_dir * (self.move_speed - (self.peakx - current_x).abs());
                self.changling *= -1;
            } else if next_posx < min_orbit_x {
                self.peakx = min_orbit_x;
                if self.peaky <= self.halo_anchor_y {
                    self.halo_dir *= -1;
                }
                self.peaky += self.halo_dir * (self.move_speed - (self.peakx - current_x).abs());
                self.changling *= -1;
            } else {
                self.peakx += self.halo_dir * self.move_speed;
            }
        } else {
            let max_orbit_y = self.halo_anchor_y + current_orbit - 1;
            let min_orbit_y = self.halo_anchor_y - current_orbit + 1;
            let next_posy = self.peaky + (self.halo_dir * self.move_speed);
            let current_y = self.peaky;
            if next_posy > max_orbit_y {
                self.peaky = max_orbit_y;
                if self.peakx >= self.halo_anchor_x {
                    self.halo_dir *= -1;
                }
                self.peakx += self.halo_dir * (self.move_speed - (self.peaky - current_y).abs());
                self.changling *= -1;
            } else if next_posy < min_orbit_y {
                self.peaky = min_orbit_y;
                if self.peakx < self.halo_anchor_x {
                    self.halo_dir *= -1;
                }
                self.peakx += self.halo_dir * (self.move_speed - (self.peaky - current_y).abs());
                self.changling *= -1;
            } else {
                self.peaky += self.halo_dir * self.move_speed;
            }
        }
        self.confirm_halo_peak();
    }

    /// `confirmHaloPeak()`: pull a peak that jumped out of its band back in.
    /// (Uses `m_height`, not `temp_height`, as the C++ does.)
    fn confirm_halo_peak(&mut self) {
        let (ax, ay, ir, w, h) = (
            self.halo_anchor_x,
            self.halo_anchor_y,
            self.halo_inner_radius,
            self.halo_width,
            self.height,
        );
        if self.changling == 1 {
            let l_y_min = ay - ir - w + h - 1;
            let l_y_max = ay - ir - h + 1;
            let r_y_min = ay + ir + h - 1;
            let r_y_max = ay + ir + w - h + 1;
            if self.peaky < ay {
                // top
                if self.peaky < l_y_min {
                    self.peaky = l_y_min;
                } else if self.peaky > l_y_max {
                    self.peaky = l_y_max;
                }
            } else if self.peaky > ay {
                // bottom
                if self.peaky < r_y_min {
                    self.peaky = r_y_min;
                } else if self.peaky > r_y_max {
                    self.peaky = r_y_max;
                }
            }
        } else {
            let l_x_min = ax - ir - w + h - 1;
            let l_x_max = ax - ir - h + 1;
            let r_x_min = ax + ir + h - 1;
            let r_x_max = ax + ir + w - h + 1;
            if self.peakx < ax {
                // left
                if self.peakx < l_x_min {
                    self.peakx = l_x_min;
                } else if self.peakx > l_x_max {
                    self.peakx = l_x_max;
                }
            } else if self.peakx > ax {
                // right
                if self.peakx < r_x_min {
                    self.peakx = r_x_min;
                } else if self.peakx > r_x_max {
                    self.peakx = r_x_max;
                }
            }
        }
    }

    /// `movePeak()`: non-halo peaks move inside their bounding box and
    /// bounce off its edges.
    fn move_peak(&mut self) {
        let temp_height = self.temp_height();
        let num_steps = if self.move_speed > 1 {
            self.move_speed
        } else {
            1
        };
        for _ in 0..num_steps {
            let step = (self.move_y_scaler + 0.5) as i32;
            let temp_peakx = self.peakx + step * self.movesignx;
            let temp_peaky = self.peaky + step * self.movesigny;
            let x_room = (temp_height * 2) < (self.max_x - self.min_x).abs();
            let y_room = (temp_height * 2) < (self.max_y - self.min_y).abs();

            // (The C++ also resets temp_peakx/y in the no-room branches; the
            // values are not read again.)
            if x_room {
                if temp_peakx > (self.max_x - temp_height) {
                    self.movesignx = -1;
                }
                if temp_peakx < (self.min_x + temp_height + 1) {
                    self.movesignx = 1;
                }
            } else {
                self.movesignx = 0;
            }

            if y_room {
                if temp_peaky > (self.max_y - temp_height) {
                    self.movesigny = -1;
                }
                if temp_peaky < (self.min_y + temp_height + 1) {
                    self.movesigny = 1;
                }
            } else {
                self.movesigny = 0;
            }
            // FMA: `m_peakx + (m_movesignx * m_move_y_scaler)`, then `+ .5`.
            if x_room {
                self.peakx = (f64::from(self.movesignx)
                    .mul_add(self.move_y_scaler, f64::from(self.peakx))
                    + 0.5) as i32;
            }
            if y_room {
                self.peaky = (f64::from(self.movesigny)
                    .mul_add(self.move_y_scaler, f64::from(self.peaky))
                    + 0.5) as i32;
            }
        }
    }

    /// `generateBarrier(cAvidaContext&)`: habitat 2, walls of `m_plateau`,
    /// regenerated every `updatestep` updates.
    fn generate_barrier(&mut self, rng: &mut AvidaRng) {
        if self.topo_counter != self.updatestep {
            self.topo_counter += 1;
            return;
        }
        self.reset_used_bounds();
        self.topo_counter = 1;
        // Clear any old walls.
        if !self.wall_cells.is_empty() {
            for i in 0..self.wall_cells.len() {
                let c = self.wall_cells[i];
                self.set_amt(c, 0.0);
            }
        } else {
            for ii in 0..self.gx() {
                for jj in 0..self.gy() {
                    let c = jj * self.gx() + ii;
                    self.set_amt(c, 0.0);
                }
            }
        }
        self.wall_cells.clear();

        let gx = self.gx();
        let gy = self.gy();
        let in_inner_radius = |s: &Self, x: i32, y: i32| {
            x < (s.halo_anchor_x + s.halo_inner_radius)
                && y < (s.halo_anchor_y + s.halo_inner_radius)
                && x > (s.halo_anchor_x - s.halo_inner_radius)
                && y > (s.halo_anchor_y - s.halo_inner_radius)
        };
        for _ in 0..self.count {
            // Drop the first block of this barrier.
            let (start_randx, start_randy) = if self.config == 3 || self.config == 4 {
                (self.peakx, self.peaky)
            } else {
                let x = uint_range(rng, 0, gx);
                let y = uint_range(rng, 0, gy);
                (x, y)
            };
            self.set_amt(start_randy * gx + start_randx, self.plateau);
            self.update_bounds(start_randx, start_randy);
            self.wall_cells.push(start_randy * gx + start_randx);

            let mut randx = start_randx;
            let mut randy = start_randy;
            let mut prev_blockx = randx;
            let mut prev_blocky = randy;
            let mut cornerx = prev_blockx;
            let mut cornery = prev_blocky;
            let mut place_corner = false;

            // Size of this barrier, and (for straight walls) its direction.
            let rand_block_count = uint_range(rng, self.min_size, self.max_size + 1);
            let mut direction = uint_range(rng, 0, 2);

            let mut num_blocks = 0;
            while num_blocks < rand_block_count {
                if self.config == 0 {
                    // Random walls: step in the current direction; diagonal
                    // steps also fill a corner so the wall is not porous.
                    prev_blockx = randx;
                    prev_blocky = randy;
                    cornerx = prev_blockx;
                    cornery = prev_blocky;
                    place_corner = false;
                    match direction {
                        0 => randy -= 1,
                        1 => {
                            randy -= 1;
                            randx += 1;
                            place_corner = true;
                            cornery -= 1;
                        }
                        2 => randx += 1,
                        3 => {
                            randy += 1;
                            randx += 1;
                            place_corner = true;
                            cornerx += 1;
                        }
                        4 => randy += 1,
                        5 => {
                            randy += 1;
                            randx -= 1;
                            place_corner = true;
                            cornerx -= 1;
                        }
                        6 => randx -= 1,
                        7 => {
                            randy -= 1;
                            randx -= 1;
                            place_corner = true;
                            cornery += 1;
                        }
                        _ => {}
                    }
                    // About 1 block in 20 turns.
                    if uint_range(rng, 0, 21) == 20 {
                        direction = uint_range(rng, 0, 8);
                    }
                } else if self.config == 1 {
                    // Vertical walls, up or down.
                    if direction == 0 {
                        randy -= 1;
                    } else {
                        randy += 1;
                    }
                } else if self.config == 2 {
                    // Horizontal walls, left or right.
                    if direction == 0 {
                        randx -= 1;
                    } else {
                        randx += 1;
                    }
                } else if self.config == 3 {
                    // North to south.
                    randy += 1;
                } else if self.config == 4 {
                    // West to east.
                    randx += 1;
                }

                let mut count_block = true;
                if randy < gy && randy >= 0 && randx < gx && randx >= 0 {
                    // Don't place (or count) blocks inside the inner radius,
                    // or, for random walls, on an existing block.
                    if in_inner_radius(self, randx, randy)
                        || (self.config == 0 && self.amt(randy * gx + randx) != 0.0)
                    {
                        num_blocks -= 1;
                        count_block = false;
                    }
                    if count_block {
                        self.set_amt(randy * gx + randx, self.plateau);
                        if self.plateau > 0.0 {
                            self.update_bounds(randx, randy);
                        }
                        self.wall_cells.push(randy * gx + randx);
                        if place_corner
                            && cornery < gy
                            && cornery >= 0
                            && cornerx < gx
                            && cornerx >= 0
                            && !in_inner_radius(self, cornerx, cornery)
                        {
                            self.set_amt(cornery * gx + cornerx, self.plateau);
                            if self.plateau > 0.0 {
                                self.update_bounds(cornerx, cornery);
                            }
                            // QUIRK: records the block, not the corner cell.
                            self.wall_cells.push(randy * gx + randx);
                        }
                    }
                } else if self.config == 1 || self.config == 2 {
                    // Straight wall ran off the world: build the other way.
                    randx = start_randx;
                    randy = start_randy;
                    direction = (direction - 1).abs();
                    num_blocks -= 1;
                } else if self.config == 0 {
                    // Random wall ran off the world: back up and retry.
                    randx = prev_blockx;
                    randy = prev_blocky;
                    num_blocks -= 1;
                }
                num_blocks += 1;
            }
        }
    }

    /// `generateHills(cAvidaContext&)`: habitat 1, `count` cones of height
    /// `m_plateau`, regenerated every `updatestep` updates.
    fn generate_hills(&mut self, rng: &mut AvidaRng) {
        if self.topo_counter != self.updatestep {
            self.topo_counter += 1;
            return;
        }
        self.reset_used_bounds();
        self.topo_counter = 1;
        let gx = self.gx();
        let gy = self.gy();
        // Wipe the world (the used-bounds branch is dead after the reset above).
        if self.min_usedx == -1
            || self.min_usedy == -1
            || self.max_usedx == -1
            || self.max_usedy == -1
        {
            for ii in 0..gx {
                for jj in 0..gy {
                    self.set_amt(jj * gx + ii, 0.0);
                }
            }
        } else {
            for ii in self.min_usedx..self.max_usedx + 1 {
                for jj in self.min_usedy..self.max_usedy + 1 {
                    self.set_amt(jj * gx + ii, 0.0);
                }
            }
        }

        for _ in 0..self.count {
            let rand_hill_radius = uint_range(rng, self.min_size, self.max_size + 1);

            // Random hills outside the inner radius (config 0), otherwise one
            // hill at peakx/peaky.
            if self.config == 0 {
                let (ax, ay, ir) = (
                    self.halo_anchor_x,
                    self.halo_anchor_y,
                    self.halo_inner_radius,
                );
                let choose_ew = uint_range(rng, 0, 2);
                if choose_ew == 0 {
                    self.peakx = uint_range(rng, rand_hill_radius, ax - ir - rand_hill_radius);
                } else {
                    self.peakx =
                        uint_range(rng, ax + ir + rand_hill_radius, gx - 1 - rand_hill_radius);
                }
                let choose_ns = uint_range(rng, 0, 2);
                if choose_ns == 0 {
                    self.peaky = uint_range(rng, rand_hill_radius, ay - ir - rand_hill_radius);
                } else {
                    self.peaky =
                        uint_range(rng, ay + ir + rand_hill_radius, gy - 1 - rand_hill_radius);
                }
            }

            let max_pos_x = (self.peakx + rand_hill_radius + 1).min(gx - 1);
            let min_pos_x = (self.peakx - rand_hill_radius - 1).max(0);
            let max_pos_y = (self.peaky + rand_hill_radius + 1).min(gy - 1);
            let min_pos_y = (self.peaky - rand_hill_radius - 1).max(0);

            for ii in min_pos_x..max_pos_x + 1 {
                for jj in min_pos_y..max_pos_y + 1 {
                    let thisdist = cell_distance(self.peakx - ii, self.peaky - jj);
                    // Within the radius, and only where no overlapping hill is higher.
                    if thisdist <= f64::from(rand_hill_radius)
                        && self.amt(jj * gx + ii) < self.plateau / (thisdist + 1.0)
                    {
                        let thisheight = self.plateau / (thisdist + 1.0);
                        self.set_amt(jj * gx + ii, thisheight);
                        if thisheight > 0.0 {
                            self.update_bounds(ii, jj);
                        }
                    }
                }
            }
        }
    }

    /// `ResetGradRes(cAvidaContext&, int worldx, int worldy)`: validate,
    /// size the grid and generate the resource from scratch.
    ///
    /// The two configuration errors go to [`ResourceEnv::feedback_error`]
    /// and return early, leaving the grid as it was (empty for a new
    /// resource), as in the C++.
    pub fn reset_grad_res(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        worldx: i32,
        worldy: i32,
    ) {
        if self.move_speed >= 2 * (self.halo_inner_radius + self.halo_width)
            && (self.halo_inner_radius + self.halo_width) != 0
            && self.move_speed != 0
        {
            env.feedback_error("Move speed greater or equal to 2*Radius");
            return;
        }
        if self.halo == 1 && (self.halo_width < 2 * self.height && self.plateau >= 0.0) {
            env.feedback_error("Halo width < 2 * height (aka plateau radius)");
            return;
        }
        if self.move_speed < 0 {
            self.skip_moves = self.move_speed.abs();
            self.move_speed = 1;
        }
        let n = (f64::from(4_i32.wrapping_mul(self.height).wrapping_mul(self.height)) + 0.5) as i32;
        let n = n.max(0) as usize;
        self.plateau_array = vec![0.0; n];
        self.plateau_cell_ids = vec![0; n];
        self.prob_res_cells.clear();
        self.wall_cells.clear();
        self.current_height = f64::from(self.height);
        self.common_plat_height = self.plateau;
        self.mean_plat_inflow = self.plateau_inflow;
        self.var_plat_inflow = 0.0;
        self.reset_used_bounds();

        self.initial = true;
        self.base.resize_clear(worldx, worldy, self.geometry);
        if self.habitat == 2 {
            self.topo_counter = self.updatestep;
            self.generate_barrier(rng);
        } else if self.habitat == 1 {
            self.topo_counter = self.updatestep;
            self.generate_hills(rng);
        } else {
            self.generate_peak(rng, env);
            self.update_count(rng, env);
        }
        // The resource has been reset.
        self.initial = false;
    }

    /// `SetGradPlatVarInflow(cAvidaContext&, mean, variance, type)`: a
    /// random plateau inflow around `mean` (type 0 |N(mean)|, <0 below the
    /// mean, 1 above, 2 either side), or exactly `mean` when `variance <= 0`.
    pub fn set_grad_plat_var_inflow(
        &mut self,
        rng: &mut AvidaRng,
        mean: f64,
        variance: f64,
        kind: i32,
    ) {
        if variance > 0.0 {
            self.mean_plat_inflow = mean;
            self.var_plat_inflow = variance;
            if kind == 0 {
                let the_inflow = rng.get_rand_normal_params(mean, variance).abs();
                self.set_grad_plat_inflow(the_inflow);
            } else if kind < 0 {
                let mut the_inflow = rng.get_rand_normal_params(0.0, variance).abs();
                if mean - the_inflow < 0.0 {
                    the_inflow = mean;
                }
                self.set_grad_plat_inflow(mean - the_inflow);
            } else if kind == 1 {
                let the_inflow = rng.get_rand_normal_params(0.0, variance).abs();
                self.set_grad_plat_inflow(mean + the_inflow);
            } else if kind == 2 {
                let mut the_inflow = rng.get_rand_normal_params(0.0, variance);
                // QUIRK: a draw that would go negative becomes `mean + mean`.
                if mean + the_inflow < 0.0 {
                    the_inflow = mean;
                }
                self.set_grad_plat_inflow(mean + the_inflow);
            }
        } else {
            self.set_grad_plat_inflow(mean);
        }
    }

    /// `UpdatePredatoryRes(cAvidaContext&)`: every plateau cell with at least
    /// 1 unit asks the population to try to kill its occupant.
    fn update_predatory_res(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv) {
        if self.predator {
            for i in 0..self.plateau_cell_ids.len() {
                let c = self.plateau_cell_ids[i];
                if self.amt(c) >= 1.0 {
                    env.execute_predatory_resource(
                        rng,
                        c,
                        self.pred_odds,
                        self.guarded_juvs_per_adult,
                        self.hammer != 0,
                    );
                }
            }
        }
    }

    /// `SetPredatoryResource(double odds, int juvsper)`
    pub fn set_predatory_resource(&mut self, odds: f64, juvsper: i32) {
        self.predator = true;
        self.pred_odds = odds;
        self.guarded_juvs_per_adult = juvsper;
    }

    /// `UpdateDamagingRes(cAvidaContext&)`: skipped without a driver (world
    /// still being built).
    fn update_damaging_res(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv) {
        if self.damage != 0.0 {
            for i in 0..self.plateau_cell_ids.len() {
                let c = self.plateau_cell_ids[i];
                if self.amt(c) >= self.threshold && env.has_driver() {
                    env.execute_damaging_resource(rng, c, self.damage, self.hammer != 0);
                }
            }
        }
    }

    /// `SetDeadlyRes(double odds)`
    pub fn set_deadly_res(&mut self, odds: f64) {
        self.death_odds = odds;
        self.deadly = self.death_odds != 0.0;
    }

    /// `SetIsPath(bool)`
    pub fn set_is_path(&mut self, path: bool) {
        self.path = i32::from(path);
    }

    /// `UpdateDeadlyRes(cAvidaContext&)`: skipped without a driver.
    fn update_deadly_res(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv) {
        if self.deadly {
            for i in 0..self.plateau_cell_ids.len() {
                let c = self.plateau_cell_ids[i];
                if self.amt(c) >= self.threshold && env.has_driver() {
                    env.execute_deadly_resource(rng, c, self.death_odds, self.hammer != 0);
                }
            }
        }
    }

    /// `SetProbabilisticResource(ctx, initial, inflow, outflow, lambda,
    /// theta, x, y, num_cells)`: turn this into a probabilistic resource and
    /// scatter it.
    #[allow(clippy::too_many_arguments)]
    pub fn set_probabilistic_resource(
        &mut self,
        rng: &mut AvidaRng,
        initial: f64,
        inflow: f64,
        outflow: f64,
        lambda: f64,
        theta: f64,
        x: i32,
        y: i32,
        num_cells: i32,
    ) {
        self.probabilistic = true;
        self.initial_plat = initial;
        self.plateau_inflow = inflow;
        self.plateau_outflow = outflow;
        self.build_probabilistic_res(rng, lambda, theta, x, y, num_cells);
    }

    /// `BuildProbabilisticRes(ctx, lambda, theta, x, y, num_cells)`: cells
    /// get the resource with half-normal probability in their distance from
    /// `theta` around the centre; `num_cells == -1` makes a single pass.
    ///
    /// Writes the C++ makes past the end of `m_prob_res_cells` (quota smaller
    /// than the cells placed) are skipped.
    pub fn build_probabilistic_res(
        &mut self,
        rng: &mut AvidaRng,
        lambda: f64,
        theta: f64,
        x: i32,
        y: i32,
        mut num_cells: i32,
    ) {
        if self.min_usedx != -1 {
            self.clear_existing_prob_res();
        }
        self.reset_used_bounds();
        let mut cells_used: i32 = 0;
        let worldx = self.gx();
        let worldy = self.gy();
        let world_size = worldx.wrapping_mul(worldy);
        let mut max_idx = world_size - 1;
        let mut max_tries = 1000.min(world_size);

        let mut cell_id_array: Vec<i32> = (0..world_size.max(0)).collect();

        self.peakx = if x == -1 {
            uint_range(rng, 0, worldx)
        } else {
            x
        };
        self.peaky = if y == -1 {
            uint_range(rng, 0, worldy)
        } else {
            y
        };

        if num_cells != -1 {
            // ResizeClear: contents are overwritten or popped below.
            self.prob_res_cells = vec![0; num_cells.max(0) as usize];
        }
        let flows = self.plateau_outflow > 0.0 || self.plateau_inflow > 0.0;

        // Only theta == 0 gives a cell with resource for certain at the centre.
        if theta == 0.0 {
            let c = self.peaky * worldx + self.peakx;
            self.set_amt(c, self.initial_plat);
            if self.initial_plat > 0.0 {
                self.update_bounds(self.peakx, self.peaky);
            }
            if flows {
                if num_cells == -1 {
                    self.prob_res_cells.push(c);
                } else if let Some(slot) = self.prob_res_cells.get_mut(cells_used as usize) {
                    *slot = c;
                }
            }
            cells_used += 1;
            // Move it out of the way instead of removing it.
            swap_in_range(&mut cell_id_array, c, max_idx);
            max_idx -= 1;
        }

        // Without a quota, make a single pass.
        let mut loop_once = false;
        if num_cells == -1 {
            loop_once = true;
            num_cells = cells_used + 1;
        }

        let mut max_unused_idx = max_idx;
        while max_tries != 0 {
            // Loop again until the quota is filled.
            if max_unused_idx == 0 {
                max_tries -= 1;
                if !loop_once {
                    max_unused_idx = max_idx;
                }
            }

            let cell_idx = uint_max(rng, max_unused_idx + 1);
            let Some(&cell_id) = cell_id_array.get(cell_idx as usize) else {
                break;
            };
            let this_x = cell_id % worldx;
            let this_y = cell_id / worldx;
            let cell_dist = cell_distance(self.peakx - this_x, self.peaky - this_y);
            // Half normal. QUIRK: the C++ writes pi as `3.14159`.
            #[allow(clippy::approx_constant)]
            let this_prob = (1.0 / lambda)
                * (2.0 / 3.14159_f64).sqrt()
                * (-0.5 * ((cell_dist - theta) / lambda).powf(2.0)).exp();

            if rng.p(this_prob) {
                self.set_amt(cell_id, self.initial_plat);
                if self.initial_plat > 0.0 {
                    self.update_bounds(this_x, this_y);
                }
                if flows {
                    if loop_once {
                        self.prob_res_cells.push(cell_id);
                    } else if let Some(slot) = self.prob_res_cells.get_mut(cells_used as usize) {
                        *slot = cell_id;
                    }
                }
                cells_used += 1;
                swap_in_range(&mut cell_id_array, cell_idx, max_idx);
                max_idx -= 1;
            } else {
                // Out of the way for this pass, but kept for the next.
                self.set_amt(cell_id, 0.0);
                swap_in_range(&mut cell_id_array, cell_idx, max_unused_idx);
                max_unused_idx -= 1;
            }

            if cells_used >= num_cells && !loop_once {
                break;
            }
            if max_unused_idx <= 0 && loop_once {
                break;
            }
            if max_idx <= 0 {
                break;
            }
        }
        // In case the quota could not be filled.
        while self.prob_res_cells.len() as i32 > cells_used {
            self.prob_res_cells.pop();
        }
    }

    /// `UpdateProbabilisticRes()`: plateau inflow/outflow on the cells that
    /// received resource.
    fn update_probabilistic_res(&mut self) {
        if self.plateau_outflow > 0.0 || self.plateau_inflow > 0.0 {
            for i in 0..self.prob_res_cells.len() {
                let c = self.prob_res_cells[i];
                let curr_val = self.amt(c);
                // FMA: `curr + inflow - (curr * outflow)`.
                let amount =
                    (-curr_val).mul_add(self.plateau_outflow, curr_val + self.plateau_inflow);
                self.set_amt(c, amount);
                if amount > 0.0 {
                    let gx = self.gx();
                    if gx != 0 {
                        self.update_bounds(c % gx, c / gx);
                    }
                }
            }
        }
    }

    /// `clearExistingProbRes()`: zero the used bounding box.
    fn clear_existing_prob_res(&mut self) {
        let gx = self.gx();
        for x in self.min_usedx..self.max_usedx + 1 {
            for y in self.min_usedy..self.max_usedy + 1 {
                self.set_amt(y * gx + x, 0.0);
            }
        }
    }

    /// `updateBounds(int x, int y)`: grow the bounding box of non-zero cells.
    fn update_bounds(&mut self, x: i32, y: i32) {
        if x < self.min_usedx || self.min_usedx == -1 {
            self.min_usedx = x;
        }
        if y < self.min_usedy || self.min_usedy == -1 {
            self.min_usedy = y;
        }
        if x > self.max_usedx || self.max_usedx == -1 {
            self.max_usedx = x;
        }
        if y > self.max_usedy || self.max_usedy == -1 {
            self.max_usedy = y;
        }
    }

    /// `resetUsedBounds()`
    fn reset_used_bounds(&mut self) {
        self.min_usedx = -1;
        self.min_usedy = -1;
        self.max_usedx = -1;
        self.max_usedy = -1;
    }

    /// `GetWallCells()`
    pub fn wall_cells(&self) -> &[i32] {
        &self.wall_cells
    }

    /// `GetMinUsedX()`
    pub fn min_used_x(&self) -> i32 {
        self.min_usedx
    }

    /// `GetMinUsedY()`
    pub fn min_used_y(&self) -> i32 {
        self.min_usedy
    }

    /// `GetMaxUsedX()`
    pub fn max_used_x(&self) -> i32 {
        self.max_usedx
    }

    /// `GetMaxUsedY()`
    pub fn max_used_y(&self) -> i32 {
        self.max_usedy
    }

    /// Peak position (`m_peakx`, `m_peaky`); the published position is the
    /// base's `curr_peak_x/y`, set when values are filled in.
    pub fn peak(&self) -> (i32, i32) {
        (self.peakx, self.peaky)
    }

    /// Current plateau inflow (`m_plateau_inflow`).
    pub fn plateau_inflow(&self) -> f64 {
        self.plateau_inflow
    }

    /// Cells of the plateau, as last filled (`m_plateau_cell_IDs`).
    pub fn plateau_cell_ids(&self) -> &[i32] {
        &self.plateau_cell_ids
    }

    /// Cells of a probabilistic resource that receive inflow/outflow.
    pub fn prob_res_cells(&self) -> &[i32] {
        &self.prob_res_cells
    }

    /// `SetGradInitialPlat(double)`; also marks the next fill as initial.
    pub fn set_grad_initial_plat(&mut self, plat_val: f64) {
        self.initial_plat = plat_val;
        self.initial = true;
    }

    /// `SetGradPeakX(int)`
    pub fn set_grad_peak_x(&mut self, v: i32) {
        self.peakx = v;
    }

    /// `SetGradPeakY(int)`
    pub fn set_grad_peak_y(&mut self, v: i32) {
        self.peaky = v;
    }

    /// `SetGradHeight(int)`
    pub fn set_grad_height(&mut self, v: i32) {
        self.height = v;
    }

    /// `SetGradSpread(int)`
    pub fn set_grad_spread(&mut self, v: i32) {
        self.spread = v;
    }

    /// `SetGradPlateau(double)`
    pub fn set_grad_plateau(&mut self, v: f64) {
        self.plateau = v;
    }

    /// `SetGradDecay(int)`
    pub fn set_grad_decay(&mut self, v: i32) {
        self.decay = v;
    }

    /// `SetGradMaxX(int)`
    pub fn set_grad_max_x(&mut self, v: i32) {
        self.max_x = v;
    }

    /// `SetGradMaxY(int)`
    pub fn set_grad_max_y(&mut self, v: i32) {
        self.max_y = v;
    }

    /// `SetGradMinX(int)`
    pub fn set_grad_min_x(&mut self, v: i32) {
        self.min_x = v;
    }

    /// `SetGradMinY(int)`
    pub fn set_grad_min_y(&mut self, v: i32) {
        self.min_y = v;
    }

    /// `SetGradMoveScaler(double)`
    pub fn set_grad_move_scaler(&mut self, v: f64) {
        self.move_a_scaler = v;
    }

    /// `SetGradUpdateStep(int)`
    pub fn set_grad_update_step(&mut self, v: i32) {
        self.updatestep = v;
    }

    /// `SetGradIsHalo(bool)`: stored as 0/1.
    pub fn set_grad_is_halo(&mut self, halo: bool) {
        self.halo = i32::from(halo);
    }

    /// `SetGradHaloInnerRad(int)`
    pub fn set_grad_halo_inner_rad(&mut self, v: i32) {
        self.halo_inner_radius = v;
    }

    /// `SetGradHaloWidth(int)`
    pub fn set_grad_halo_width(&mut self, v: i32) {
        self.halo_width = v;
    }

    /// `SetGradHaloX(int)`
    pub fn set_grad_halo_x(&mut self, v: i32) {
        self.halo_anchor_x = v;
    }

    /// `SetGradHaloY(int)`
    pub fn set_grad_halo_y(&mut self, v: i32) {
        self.halo_anchor_y = v;
    }

    /// `SetGradMoveSpeed(int)`
    pub fn set_grad_move_speed(&mut self, v: i32) {
        self.move_speed = v;
    }

    /// `SetGradMoveResistance(int)`
    pub fn set_grad_move_resistance(&mut self, v: i32) {
        self.move_resistance = v;
    }

    /// `SetGradPlatInflow(double)`
    pub fn set_grad_plat_inflow(&mut self, v: f64) {
        self.plateau_inflow = v;
    }

    /// `SetGradPlatOutflow(double)`
    pub fn set_grad_plat_outflow(&mut self, v: f64) {
        self.plateau_outflow = v;
    }

    /// `SetGradConeInflow(double)`
    pub fn set_grad_cone_inflow(&mut self, v: f64) {
        self.cone_inflow = v;
    }

    /// `SetGradConeOutflow(double)`
    pub fn set_grad_cone_outflow(&mut self, v: f64) {
        self.cone_outflow = v;
    }

    /// `SetGradientInflow(double)`
    pub fn set_gradient_inflow(&mut self, v: f64) {
        self.gradient_inflow = v;
    }

    /// `SetGradPlatIsCommon(bool)`: stored as 0/1.
    pub fn set_grad_plat_is_common(&mut self, common: bool) {
        self.is_plateau_common = i32::from(common);
    }

    /// `SetGradFloor(double)`
    pub fn set_grad_floor(&mut self, v: f64) {
        self.floor = v;
    }

    /// `SetGradHabitat(int)`
    pub fn set_grad_habitat(&mut self, v: i32) {
        self.habitat = v;
    }

    /// `SetGradMinSize(int)`
    pub fn set_grad_min_size(&mut self, v: i32) {
        self.min_size = v;
    }

    /// `SetGradMaxSize(int)`
    pub fn set_grad_max_size(&mut self, v: i32) {
        self.max_size = v;
    }

    /// `SetGradConfig(int)`
    pub fn set_grad_config(&mut self, v: i32) {
        self.config = v;
    }

    /// `SetGradCount(int)`
    pub fn set_grad_count(&mut self, v: i32) {
        self.count = v;
    }

    /// The setter sequence of `cResourceCount::SetGradientCount`, in its
    /// order. `SetGradResistance`, `SetGradDamage`, `SetGradThreshold`,
    /// `SetGradRefuge` and `SetGradDeathOdds` are not overridden by
    /// `cGradientCount`, so (QUIRK) those parameters do not change here;
    /// `path`/`hammer` are not passed on at all.
    pub fn apply_set_gradient_count(&mut self, p: &GradientParams) {
        self.set_grad_peak_x(p.peakx);
        self.set_grad_peak_y(p.peaky);
        self.set_grad_height(p.height);
        self.set_grad_spread(p.spread);
        self.set_grad_plateau(p.plateau);
        self.set_grad_initial_plat(p.init_plat);
        self.set_grad_decay(p.decay);
        self.set_grad_max_x(p.max_x);
        self.set_grad_max_y(p.max_y);
        self.set_grad_min_x(p.min_x);
        self.set_grad_min_y(p.min_y);
        self.set_grad_move_scaler(p.move_a_scaler);
        self.set_grad_update_step(p.updatestep);
        self.set_grad_is_halo(p.halo != 0);
        self.set_grad_halo_inner_rad(p.halo_inner_radius);
        self.set_grad_halo_width(p.halo_width);
        self.set_grad_halo_x(p.halo_anchor_x);
        self.set_grad_halo_y(p.halo_anchor_y);
        self.set_grad_move_speed(p.move_speed);
        self.set_grad_move_resistance(p.move_resistance);
        self.set_grad_plat_inflow(p.plateau_inflow);
        self.set_grad_plat_outflow(p.plateau_outflow);
        self.set_grad_cone_inflow(p.cone_inflow);
        self.set_grad_cone_outflow(p.cone_outflow);
        self.set_gradient_inflow(p.gradient_inflow);
        self.set_grad_plat_is_common(p.is_plateau_common != 0);
        self.set_grad_floor(p.floor);
        self.set_grad_habitat(p.habitat);
        self.set_grad_min_size(p.min_size);
        self.set_grad_max_size(p.max_size);
        self.set_grad_config(p.config);
        self.set_grad_count(p.count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::RecordingEnv;
    use crate::geometry;

    fn rng_after(rng: &mut AvidaRng) -> u32 {
        rng.get_uint(1_000_000_000)
    }

    #[test]
    fn config_errors_go_to_feedback_and_leave_the_grid_empty() {
        let mut env = RecordingEnv::new(-1, false);
        let mut rng = AvidaRng::new(3);
        // ResetGradRes: m_move_speed >= 2 * (inner radius + width).
        let fast = GradientParams {
            move_speed: 16,
            halo_inner_radius: 3,
            halo_width: 5,
            ..GradientParams::default()
        };
        let mut g = GradientCount::new(&fast, 10, 10, geometry::GRID, &mut rng, &mut env);
        // Halo narrower than twice the plateau radius.
        let narrow = GradientParams {
            halo: 1,
            halo_width: 11,
            height: 6,
            plateau: 1.0,
            ..GradientParams::default()
        };
        let h = GradientCount::new(&narrow, 10, 10, geometry::GRID, &mut rng, &mut env);
        assert_eq!(
            env.errors,
            vec![
                "Move speed greater or equal to 2*Radius".to_string(),
                "Halo width < 2 * height (aka plateau radius)".to_string()
            ]
        );
        assert_eq!(g.base.size(), 0);
        assert_eq!(h.base.size(), 0);
        // Nothing was drawn.
        assert_eq!(rng_after(&mut rng), rng_after(&mut AvidaRng::new(3)));
        // Updating the half-built resource is C++ UB; here it is harmless.
        env.update = 5;
        env.has_driver = true;
        for _ in 0..3 {
            g.update_count(&mut rng, &mut env);
        }
        assert_eq!(g.base.size(), 0);
    }

    #[test]
    fn move_speed_check_needs_a_nonzero_radius() {
        // `(inner + width) != 0` guards the check: a non-halo resource with
        // the default zero radius and move speed 1 is fine.
        let mut env = RecordingEnv::new(-1, false);
        let mut rng = AvidaRng::new(3);
        let p = GradientParams {
            height: 2,
            spread: 4,
            plateau: 1.0,
            max_x: 9,
            max_y: 9,
            ..GradientParams::default()
        };
        let g = GradientCount::new(&p, 10, 10, geometry::GRID, &mut rng, &mut env);
        assert!(env.errors.is_empty());
        assert_eq!(g.base.size(), 100);
        // A plateau of height 2 around the peak: cells within distance 1.
        let (px, py) = g.peak();
        assert_eq!(g.base.get_amount(py * 10 + px), 1.0);
        assert_eq!(g.plateau_cell_ids().len(), 4 * 2 * 2);
    }

    #[test]
    fn misconfigured_positions_do_not_panic() {
        let mut env = RecordingEnv::new(-1, false);
        let mut rng = AvidaRng::new(11);
        // A north-south wall starting outside the world (cell 340 of 100,
        // a C++ UB write). Only the start is unchecked in the C++; the
        // following blocks fail the bounds test and are dropped.
        let wall = GradientParams {
            habitat: 2,
            plateau: 1.0,
            config: 3,
            peakx: 40,
            peaky: 30,
            min_size: 5,
            max_size: 5,
            ..GradientParams::default()
        };
        let w = GradientCount::new(&wall, 10, 10, geometry::GRID, &mut rng, &mut env);
        assert_eq!(w.base.sum_all(), 0.0);
        // A probabilistic resource centred outside the world.
        let p = GradientParams {
            height: 1,
            spread: 1,
            plateau: 1.0,
            max_x: 9,
            max_y: 9,
            ..GradientParams::default()
        };
        let mut g = GradientCount::new(&p, 10, 10, geometry::GRID, &mut rng, &mut env);
        g.set_probabilistic_resource(&mut rng, 1.0, 0.1, 0.0, 2.0, 0.0, 50, 50, 5);
        g.update_count(&mut rng, &mut env);
        // A height change without ResetGradRes leaves the plateau arrays short.
        g.set_grad_height(5);
        g.set_grad_spread(6);
        g.probabilistic = false;
        env.update = 3;
        for _ in 0..3 {
            g.update_count(&mut rng, &mut env);
        }
        assert!(env.errors.is_empty());
    }

    #[test]
    fn set_gradient_count_ignores_the_unoverridden_setters() {
        let mut env = RecordingEnv::new(-1, false);
        let mut rng = AvidaRng::new(2);
        let p = GradientParams {
            height: 2,
            spread: 3,
            plateau: 1.0,
            max_x: 9,
            max_y: 9,
            damage: 0.5,
            threshold: 0.25,
            death_odds: 0.1,
            ..GradientParams::default()
        };
        let mut g = GradientCount::new(&p, 10, 10, geometry::GRID, &mut rng, &mut env);
        let q = GradientParams {
            damage: 9.0,
            threshold: 8.0,
            death_odds: 0.0,
            halo: 7,
            is_plateau_common: 3,
            plateau_inflow: 0.4,
            ..p.clone()
        };
        g.apply_set_gradient_count(&q);
        assert_eq!(
            (g.damage, g.threshold, g.death_odds, g.deadly),
            (0.5, 0.25, 0.1, true)
        );
        // bool setters store 0/1.
        assert_eq!((g.halo, g.is_plateau_common), (1, 1));
        assert_eq!(g.plateau_inflow(), 0.4);
    }

    #[test]
    fn var_inflow_type_2_negative_draw_doubles_the_mean() {
        // QUIRK: type 2 replaces a draw that would make the inflow negative
        // with `mean`, giving `mean + mean`.
        let mut env = RecordingEnv::new(-1, false);
        let mut rng = AvidaRng::new(1);
        let p = GradientParams {
            height: 1,
            spread: 1,
            plateau: 1.0,
            max_x: 4,
            max_y: 4,
            ..GradientParams::default()
        };
        let mut g = GradientCount::new(&p, 5, 5, geometry::GRID, &mut rng, &mut env);
        let mut seen_double = false;
        for _ in 0..200 {
            g.set_grad_plat_var_inflow(&mut rng, 0.01, 4.0, 2);
            let v = g.plateau_inflow();
            assert!(v >= 0.0);
            seen_double |= v == 0.02;
        }
        assert!(seen_double);
        g.set_grad_plat_var_inflow(&mut rng, 0.3, 0.0, 2);
        assert_eq!(g.plateau_inflow(), 0.3);
    }
}
