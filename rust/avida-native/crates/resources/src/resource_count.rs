//! Runtime amounts of every resource of a world (or of one deme).
//!
//! Ports `cResourceCount` (`avida-core/source/main/cResourceCount.{h,cc}`,
//! reference bodies from git `47f13dadb`, the last pre-Rust commit).

use std::sync::Arc;

use avida_apto::{fmt_g, AvidaRng};

use crate::cell_resource::CellResource;
use crate::env::ResourceEnv;
use crate::geometry::{self, is_spatial};
use crate::gradient_count::{GradientCount, GradientParams};
use crate::spatial_resource::SpatialResource;

/// `cResourceCount::UPDATE_STEP`: fraction of an update per global step.
pub const UPDATE_STEP: f64 = 1.0 / 10000.0;
/// `cResourceCount::EPSILON`: round-off tolerance (only used in an assert).
pub const EPSILON: f64 = 1.0e-15;
/// `cResourceCount::PRECALC_DISTANCE`: steps covered by the precalc tables.
pub const PRECALC_DISTANCE: usize = 100;

/// `VERBOSE_NORMAL` (`avida/core/Definitions.h`); `Setup` prints when the
/// verbosity exceeds it.
pub const VERBOSE_NORMAL: i32 = 1;

/// The parameter list of `cResourceCount::Setup` after `res_index`, as plain
/// data filled from a `cResource` by the caller.
///
/// Notes for the integrator:
/// - `decay` is what the C++ callers pass, i.e. `1.0 - cResource::GetOutflow()`
///   (`cPopulation::SetupResources`, `cDeme::SetupDemeRes`).
/// - QUIRK: `cDeme::SetupDemeRes` passes `GetMaxX(), GetMaxY(), GetMinX(),
///   GetMinY()` into the `max_x, min_x, max_y, min_y` slots, so for deme
///   resources `gradient.min_x = MaxY`, `gradient.max_y = MinX`. The global
///   path (`cPopulation`) passes them in the right order.
/// - The `peaks` .. `update_dynamic` group is accepted and ignored by the C++.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceSetup {
    /// `name`
    pub name: String,
    /// `initial`
    pub initial: f64,
    /// `inflow`
    pub inflow: f64,
    /// `decay` (`1 - outflow`)
    pub decay: f64,
    /// `in_geometry` (`geometry::GLOBAL`, `GRID`, `TORUS` or `PARTIAL`)
    pub geometry: i32,
    /// `in_xdiffuse`
    pub xdiffuse: f64,
    /// `in_xgravity`
    pub xgravity: f64,
    /// `in_ydiffuse`
    pub ydiffuse: f64,
    /// `in_ygravity`
    pub ygravity: f64,
    /// `in_inflowX1`
    pub inflow_x1: i32,
    /// `in_inflowX2`
    pub inflow_x2: i32,
    /// `in_inflowY1`
    pub inflow_y1: i32,
    /// `in_inflowY2`
    pub inflow_y2: i32,
    /// `in_outflowX1`
    pub outflow_x1: i32,
    /// `in_outflowX2`
    pub outflow_x2: i32,
    /// `in_outflowY1`
    pub outflow_y1: i32,
    /// `in_outflowY2`
    pub outflow_y2: i32,
    /// `*in_cell_list_ptr` (`cResource::GetCellListPtr()`)
    pub cell_list: Vec<CellResource>,
    /// `*in_cell_id_list_ptr` (`cResource::GetCellIdListPtr()`), used by
    /// `PARTIAL` resources.
    pub cell_id_list: Vec<i32>,
    /// `verbosity_level`
    pub verbosity: i32,
    /// `in_peaks` (unused)
    pub peaks: i32,
    /// `in_min_height` (unused)
    pub min_height: f64,
    /// `in_min_radius` (unused)
    pub min_radius: f64,
    /// `in_radius_range` (unused)
    pub radius_range: f64,
    /// `in_ah` (unused)
    pub ah: f64,
    /// `in_ar` (unused)
    pub ar: f64,
    /// `in_acx` (unused)
    pub acx: f64,
    /// `in_acy` (unused)
    pub acy: f64,
    /// `in_hstepscale` (unused)
    pub hstepscale: f64,
    /// `in_rstepscale` (unused)
    pub rstepscale: f64,
    /// `in_cstepscalex` (unused)
    pub cstepscalex: f64,
    /// `in_cstepscaley` (unused)
    pub cstepscaley: f64,
    /// `in_hstep` (unused)
    pub hstep: f64,
    /// `in_rstep` (unused)
    pub rstep: f64,
    /// `in_cstepx` (unused)
    pub cstepx: f64,
    /// `in_cstepy` (unused)
    pub cstepy: f64,
    /// `in_update_dynamic` (unused)
    pub update_dynamic: i32,
    /// `in_peakx` .. `in_refuge`
    pub gradient: GradientParams,
    /// `isgradient` (`cResource::GetGradient()`)
    pub is_gradient: bool,
}

impl Default for ResourceSetup {
    /// What `cResource`'s constructor defaults would pass (name empty,
    /// decay `1 - 0`).
    ///
    /// QUIRK: at `47f13dadb` the constructor initialises `inflowX1( )`
    /// (value-initialised, so 0) while the other seven rectangle bounds are
    /// `cResource::NONE` (-99). `cEnvironment::LoadResource` then copies
    /// `inflowX1` into an unset `inflowX2`, so a spatial resource without an
    /// inflow rectangle gets its inflow in column 0 of row `Mod(-99, y)`
    /// (at `ef5c6d948` both bounds stayed -99). `LoadCell` sets all eight to
    /// `NONE` explicitly.
    fn default() -> Self {
        ResourceSetup {
            name: String::new(),
            initial: 0.0,
            inflow: 0.0,
            decay: 1.0,
            geometry: geometry::GLOBAL,
            xdiffuse: 1.0,
            xgravity: 0.0,
            ydiffuse: 1.0,
            ygravity: 0.0,
            inflow_x1: 0,
            inflow_x2: -99,
            inflow_y1: -99,
            inflow_y2: -99,
            outflow_x1: -99,
            outflow_x2: -99,
            outflow_y1: -99,
            outflow_y2: -99,
            cell_list: Vec::new(),
            cell_id_list: Vec::new(),
            verbosity: 0,
            peaks: 0,
            min_height: 0.0,
            min_radius: 0.0,
            radius_range: 0.0,
            ah: 0.0,
            ar: 0.0,
            acx: 0.0,
            acy: 0.0,
            hstepscale: 0.0,
            rstepscale: 0.0,
            cstepscalex: 0.0,
            cstepscaley: 0.0,
            hstep: 0.0,
            rstep: 0.0,
            cstepx: 0.0,
            cstepy: 0.0,
            update_dynamic: 0,
            gradient: GradientParams::default(),
            is_gradient: false,
        }
    }
}

/// Runs independent jobs, possibly at the same time: how a front end with a
/// worker pool lends it to [`ResourceCount`] for the spatial updates of its
/// plain grid resources (see [`ResourceCount::set_spatial_executor`]).
pub trait SpatialExecutor: Send + Sync {
    /// Runs every job, in any order or at once, and returns when all are
    /// done.
    fn run<'a>(&self, jobs: Vec<Box<dyn FnOnce() + Send + 'a>>);
}

/// The executor a [`ResourceCount`] may hold. It takes no part in `==` or in
/// copies: it is a way of doing the work, not state.
#[derive(Default)]
struct ExecutorHandle(Option<Arc<dyn SpatialExecutor>>);

impl std::fmt::Debug for ExecutorHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExecutorHandle({})", self.0.is_some())
    }
}

impl PartialEq for ExecutorHandle {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// Grids with fewer cells than this are updated one after the other even
/// with an executor: handing out jobs costs more than they save.
const PARALLEL_MIN_CELLS: i32 = 4096;

/// `cResourceCount`: current amounts of all resources, with lazy global
/// updates in steps of [`UPDATE_STEP`] and once-per-update spatial updates.
///
/// Methods that call the C++ private `DoUpdates` take the world RNG and a
/// [`ResourceEnv`] (the C++ `cAvidaContext`), because spatial updates run
/// gradient movement, which draws random numbers and may call into the
/// population.
#[derive(Debug, PartialEq)]
pub struct ResourceCount {
    resource_name: Vec<String>,
    resource_initial: Vec<f64>,
    resource_count: Vec<f64>,
    decay_rate: Vec<f64>,
    inflow_rate: Vec<f64>,
    decay_precalc: Vec<Vec<f64>>,
    inflow_precalc: Vec<Vec<f64>>,
    geometry: Vec<i32>,
    spatial_resource_count: Vec<SpatialResource>,
    curr_grid_res_cnt: Vec<f64>,
    curr_spatial_res_cnt: Vec<Vec<f64>>,
    verbosity: i32,
    cell_lists: Vec<Vec<i32>>,
    update_time: f64,
    spatial_update_time: f64,
    last_updated: i32,
    spatial_update: i32,
    spatial_executor: ExecutorHandle,
}

impl Default for ResourceCount {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Clone for ResourceCount {
    /// `cResourceCount(const cResourceCount&)`, which is `*this = rc` on a
    /// fresh object; see [`assign_from`](ResourceCount::assign_from).
    ///
    /// The C++ copy constructor leaves `m_last_updated`, `m_spatial_update`
    /// and `verbosity` uninitialised; they are zero here.
    fn clone(&self) -> Self {
        let mut rc = ResourceCount::new(0);
        rc.assign_from(self);
        rc
    }
}

impl ResourceCount {
    /// `cResourceCount(int num_resources = 0)`
    pub fn new(num_resources: usize) -> Self {
        let mut rc = ResourceCount {
            resource_name: Vec::new(),
            resource_initial: Vec::new(),
            resource_count: Vec::new(),
            decay_rate: Vec::new(),
            inflow_rate: Vec::new(),
            decay_precalc: Vec::new(),
            inflow_precalc: Vec::new(),
            geometry: Vec::new(),
            spatial_resource_count: Vec::new(),
            curr_grid_res_cnt: Vec::new(),
            curr_spatial_res_cnt: Vec::new(),
            verbosity: 0,
            cell_lists: Vec::new(),
            update_time: 0.0,
            spatial_update_time: 0.0,
            last_updated: 0,
            spatial_update: 0,
            spatial_executor: ExecutorHandle::default(),
        };
        if num_resources > 0 {
            rc.set_size(num_resources);
        }
        rc
    }

    /// `operator=(const cResourceCount&)`.
    ///
    /// QUIRK: the spatial counts are copied with `*(mine[i]) = *(theirs[i])`
    /// into fresh base-class objects, so a gradient resource is sliced into a
    /// plain grid holding the gradient's current values. `verbosity`,
    /// `m_last_updated` and `m_spatial_update` are not copied.
    pub fn assign_from(&mut self, rc: &ResourceCount) {
        self.set_size(rc.size());
        self.resource_name = rc.resource_name.clone();
        self.resource_initial = rc.resource_initial.clone();
        self.resource_count = rc.resource_count.clone();
        self.decay_rate = rc.decay_rate.clone();
        self.inflow_rate = rc.inflow_rate.clone();
        self.decay_precalc = rc.decay_precalc.clone();
        self.inflow_precalc = rc.inflow_precalc.clone();
        self.geometry = rc.geometry.clone();
        for (mine, theirs) in self
            .spatial_resource_count
            .iter_mut()
            .zip(&rc.spatial_resource_count)
        {
            *mine = SpatialResource::Plain(theirs.base().clone());
        }
        self.curr_grid_res_cnt = rc.curr_grid_res_cnt.clone();
        self.curr_spatial_res_cnt = rc.curr_spatial_res_cnt.clone();
        self.update_time = rc.update_time;
        self.spatial_update_time = rc.spatial_update_time;
        self.cell_lists = rc.cell_lists.clone();
    }

    /// `SetSize(int)`: reset to `num_resources` empty global resources.
    pub fn set_size(&mut self, num_resources: usize) {
        let n = num_resources;
        self.resource_name = vec![String::new(); n];
        self.resource_initial = vec![0.0; n];
        self.resource_count = vec![0.0; n];
        self.decay_rate = vec![0.0; n];
        self.inflow_rate = vec![0.0; n];
        if n > 0 {
            // "This is 1-inflow, so there should be no inflow by default, JEB"
            self.decay_precalc = vec![vec![1.0; PRECALC_DISTANCE + 1]; n];
            self.inflow_precalc = vec![vec![0.0; PRECALC_DISTANCE + 1]; n];
        } else {
            // The C++ only resizes the matrices for n > 0 but SetAll()s them
            // either way; their contents are never read for n == 0.
            for row in &mut self.decay_precalc {
                row.fill(1.0);
            }
            for row in &mut self.inflow_precalc {
                row.fill(0.0);
            }
        }
        self.geometry = vec![geometry::GLOBAL; n];
        self.spatial_resource_count = vec![SpatialResource::default(); n];
        self.curr_grid_res_cnt = vec![0.0; n];
        self.curr_spatial_res_cnt = vec![Vec::new(); n];
        self.cell_lists = vec![Vec::new(); n];
    }

    /// `SetCellResources(int cell_id, const Apto::Array<double>& res)`: set
    /// the amount of every spatial resource in one cell (globals untouched).
    ///
    /// The C++ asserts that `res` has one entry per resource; a shorter
    /// slice here sets only the resources it covers.
    pub fn set_cell_resources(&mut self, cell_id: i32, res: &[f64]) {
        for (i, &amount) in res.iter().enumerate().take(self.resource_count.len()) {
            if is_spatial(self.geometry[i]) {
                self.spatial_resource_count[i]
                    .base_mut()
                    .set_cell_amount(cell_id, amount);
            }
        }
    }

    /// `Setup(cWorld*, res_index, name, initial, inflow, decay, geometry, ...)`.
    ///
    /// The grids must already be sized ([`resize_spatial_grids`]). Returns the
    /// lines the C++ writes to `cout` when `verbosity > VERBOSE_NORMAL`, for
    /// the caller to print. An unknown geometry is the C++
    /// `cerr << ...; exit(2)` and comes back as `Err` with that message.
    ///
    /// `rng`/`env` stand for `m_world->GetDefaultContext()` and the world,
    /// used when a gradient resource is created (its constructor already
    /// places and moves the peak).
    ///
    /// [`resize_spatial_grids`]: ResourceCount::resize_spatial_grids
    pub fn setup(
        &mut self,
        res_index: usize,
        setup: &ResourceSetup,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
    ) -> Result<Vec<String>, String> {
        let tempx = self.spatial_resource_count[res_index].base().x();
        let tempy = self.spatial_resource_count[res_index].base().y();
        let in_geometry = setup.geometry;

        let geo_name = match in_geometry {
            geometry::GLOBAL => "GLOBAL",
            geometry::GRID => "GRID",
            geometry::TORUS => "TORUS",
            geometry::PARTIAL => "PARTIAL",
            _ => {
                return Err(format!(
                    "[cResourceCount::Setup] Unknown resource geometry {in_geometry}.  Exiting."
                ))
            }
        };

        let mut lines = Vec::new();
        self.verbosity = setup.verbosity;
        if self.verbosity > VERBOSE_NORMAL {
            lines.push(format!(
                "Setting up resource {}({}) with initial quantity={}, decay={}, inflow={}",
                setup.name,
                geo_name,
                fmt_g(setup.initial, 6),
                fmt_g(setup.decay, 6),
                fmt_g(setup.inflow, 6)
            ));
            if in_geometry == geometry::GRID || in_geometry == geometry::TORUS {
                lines.push(format!(
                    "  Inflow rectangle ({},{}) to ({},{})",
                    setup.inflow_x1, setup.inflow_y1, setup.inflow_x2, setup.inflow_y2
                ));
                lines.push(format!(
                    "  Outflow rectangle ({},{}) to ({},{})",
                    setup.outflow_x1, setup.outflow_y1, setup.outflow_x2, setup.outflow_y2
                ));
                lines.push(format!(
                    "  xdiffuse={}, xgravity={}, ydiffuse={}, ygravity={}",
                    fmt_g(setup.xdiffuse, 6),
                    fmt_g(setup.xgravity, 6),
                    fmt_g(setup.ydiffuse, 6),
                    fmt_g(setup.ygravity, 6)
                ));
            }
        }

        self.resource_name[res_index] = setup.name.clone();
        self.resource_initial[res_index] = setup.initial;
        if in_geometry == geometry::GLOBAL {
            self.resource_count[res_index] = setup.initial;
            self.spatial_resource_count[res_index]
                .base_mut()
                .rate_all(0.0);
        } else if in_geometry == geometry::PARTIAL {
            self.resource_count[res_index] = setup.initial;
            self.spatial_resource_count[res_index]
                .base_mut()
                .rate_all(0.0);
            self.cell_lists[res_index] = setup.cell_id_list.clone();
        } else {
            self.resource_count[res_index] = 0.0;
            if setup.is_gradient {
                // `new cGradientCount(...)` places (and moves) the peak and
                // reports configuration errors through the world's feedback.
                let g = GradientCount::new(&setup.gradient, tempx, tempy, in_geometry, rng, env);
                self.spatial_resource_count[res_index] = SpatialResource::Gradient(Box::new(g));
                self.spatial_resource_count[res_index]
                    .base_mut()
                    .rate_all(0.0);
            } else {
                let s = self.spatial_resource_count[res_index].base_mut();
                s.set_initial(setup.initial / f64::from(s.size()));
                let initial = s.initial();
                s.rate_all(initial);
            }
        }
        self.spatial_resource_count[res_index].state_all();
        self.decay_rate[res_index] = setup.decay;
        self.inflow_rate[res_index] = setup.inflow;
        self.geometry[res_index] = in_geometry;
        {
            let s = self.spatial_resource_count[res_index].base_mut();
            s.set_geometry(in_geometry);
            s.set_pointers();
            s.set_cell_list(&setup.cell_list);
        }

        self.fill_decay_precalc(res_index, setup.decay);
        self.fill_inflow_precalc(res_index, setup.decay, setup.inflow);

        let s = self.spatial_resource_count[res_index].base_mut();
        s.set_xdiffuse(setup.xdiffuse);
        s.set_xgravity(setup.xgravity);
        s.set_ydiffuse(setup.ydiffuse);
        s.set_ygravity(setup.ygravity);
        s.set_inflow_x1(setup.inflow_x1);
        s.set_inflow_x2(setup.inflow_x2);
        s.set_inflow_y1(setup.inflow_y1);
        s.set_inflow_y2(setup.inflow_y2);
        s.set_outflow_x1(setup.outflow_x1);
        s.set_outflow_x2(setup.outflow_x2);
        s.set_outflow_y1(setup.outflow_y1);
        s.set_outflow_y2(setup.outflow_y2);
        Ok(lines)
    }

    /// The decay half of the precalc loop in `Setup` / `SetDecay`.
    fn fill_decay_precalc(&mut self, id: usize, decay: f64) {
        let step_decay = decay.powf(UPDATE_STEP);
        let row = &mut self.decay_precalc[id];
        row[0] = 1.0;
        for i in 1..=PRECALC_DISTANCE {
            row[i] = row[i - 1] * step_decay;
        }
    }

    /// The inflow half of the precalc loop in `Setup` / `SetInflow`:
    /// `inflow_precalc(i) = inflow_precalc(i-1) * step_decay + step_inflow`.
    /// The reference binary computes it in the Rust helper
    /// `avd_rc_inflow_precalc_next` (called from `avd_rc_fill_precalc_tables`
    /// and `avd_rc_fill_inflow_precalc_table`), a separate multiply and add
    /// (`fmul`, `fadd`), not the fused statement of the pre-Rust C++.
    fn fill_inflow_precalc(&mut self, id: usize, decay: f64, inflow: f64) {
        let step_decay = decay.powf(UPDATE_STEP);
        let step_inflow = inflow * UPDATE_STEP;
        let row = &mut self.inflow_precalc[id];
        row[0] = 0.0;
        for i in 1..=PRECALC_DISTANCE {
            let product = row[i - 1] * step_decay;
            row[i] = product + step_inflow;
        }
    }

    /// `GetResourceCountID(const cString&)`: index of the first resource
    /// with this name. The C++ prints the `Err` text to `cerr` and returns -1.
    pub fn get_resource_count_id(&self, res_name: &str) -> Result<usize, String> {
        self.resource_name
            .iter()
            .position(|n| n == res_name)
            .ok_or_else(|| format!("Error: Unknown resource '{res_name}'."))
    }

    /// `GetInflow(const cString&)`. The C++ returns -1 after printing the
    /// `Err` text.
    pub fn get_inflow(&self, name: &str) -> Result<f64, String> {
        let id = self.get_resource_count_id(name)?;
        Ok(self.inflow_rate[id])
    }

    /// `SetInflow(const cString&, double)`: new inflow and inflow table.
    pub fn set_inflow(&mut self, name: &str, inflow: f64) -> Result<(), String> {
        let id = self.get_resource_count_id(name)?;
        self.inflow_rate[id] = inflow;
        let decay = self.decay_rate[id];
        self.fill_inflow_precalc(id, decay, inflow);
        Ok(())
    }

    /// `GetDecay(const cString&)`. The C++ returns -1 after printing the
    /// `Err` text.
    pub fn get_decay(&self, name: &str) -> Result<f64, String> {
        let id = self.get_resource_count_id(name)?;
        Ok(self.decay_rate[id])
    }

    /// `SetDecay(const cString&, double)`: new decay and decay table.
    ///
    /// As in the C++, the inflow table (which also depends on the decay) is
    /// not recomputed.
    pub fn set_decay(&mut self, name: &str, decay: f64) -> Result<(), String> {
        let id = self.get_resource_count_id(name)?;
        self.decay_rate[id] = decay;
        self.fill_decay_precalc(id, decay);
        Ok(())
    }

    /// `Update(double in_time)`: add elapsed time (fractions of an update);
    /// applied lazily by the next accessor.
    pub fn update(&mut self, in_time: f64) {
        self.update_time += in_time;
        self.spatial_update_time += in_time;
    }

    /// `GetSize()`
    pub fn size(&self) -> usize {
        self.resource_count.len()
    }

    /// `ReadResources()`: global amounts without updating.
    pub fn read_resources(&self) -> &[f64] {
        &self.resource_count
    }

    /// `GetResources(ctx)`: bring everything up to date, then the global
    /// amounts (spatial resources read 0 here).
    pub fn get_resources(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv) -> &[f64] {
        self.do_updates(rng, env, false);
        &self.resource_count
    }

    /// `GetCellResources(int cell_id, ctx)`: bring everything up to date,
    /// then each resource's amount as seen from `cell_id` (the global amount
    /// for global/partial resources).
    pub fn get_cell_resources(
        &mut self,
        cell_id: i32,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
    ) -> &[f64] {
        self.do_updates(rng, env, false);
        self.fill_curr_grid_res_cnt(cell_id);
        &self.curr_grid_res_cnt
    }

    /// `GetFrozenResources(ctx, cell_id)`: like
    /// [`get_cell_resources`](Self::get_cell_resources) without updating.
    pub fn get_frozen_resources(&mut self, cell_id: i32) -> &[f64] {
        self.fill_curr_grid_res_cnt(cell_id);
        &self.curr_grid_res_cnt
    }

    fn fill_curr_grid_res_cnt(&mut self, cell_id: i32) {
        for i in 0..self.resource_count.len() {
            self.curr_grid_res_cnt[i] = if !is_spatial(self.geometry[i]) {
                self.resource_count[i]
            } else {
                self.spatial_resource_count[i].base().get_amount(cell_id)
            };
        }
    }

    /// `GetFrozenCellResVal(ctx, cell_id, res_id)`: one resource, no update.
    pub fn get_frozen_cell_res_val(&self, cell_id: i32, res_id: usize) -> f64 {
        if !is_spatial(self.geometry[res_id]) {
            self.resource_count[res_id]
        } else {
            self.spatial_resource_count[res_id]
                .base()
                .get_amount(cell_id)
        }
    }

    /// `GetCellResVal(ctx, cell_id, res_id)`: one resource, after updating.
    pub fn get_cell_res_val(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        cell_id: i32,
        res_id: usize,
    ) -> f64 {
        self.do_updates(rng, env, false);
        self.get_frozen_cell_res_val(cell_id, res_id)
    }

    /// `GetResourcesGeometry()`
    pub fn get_resources_geometry(&self) -> &[i32] {
        &self.geometry
    }

    /// `GetResourceGeometry(int)`
    pub fn get_resource_geometry(&self, res_id: usize) -> i32 {
        self.geometry[res_id]
    }

    /// `GetSpatialRes(ctx)`: bring everything up to date, then the amount of
    /// every resource in every cell (cell count taken from resource 0, as in
    /// the C++). Nothing is updated when there are no resources.
    pub fn get_spatial_res(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
    ) -> &[Vec<f64>] {
        let num_spatial_resources = self.spatial_resource_count.len();
        if num_spatial_resources > 0 {
            let num_cells = self.spatial_resource_count[0].base().size().max(0) as usize;
            self.do_updates(rng, env, false);
            for i in 0..num_spatial_resources {
                let row = &mut self.curr_spatial_res_cnt[i];
                // The C++ writes past the end if the grids were never sized.
                if row.len() < num_cells {
                    row.resize(num_cells, 0.0);
                }
                for (j, slot) in row.iter_mut().enumerate().take(num_cells) {
                    *slot = self.spatial_resource_count[i].base().get_amount(j as i32);
                }
            }
        }
        &self.curr_spatial_res_cnt
    }

    /// `GetCellIdLists()`: the `PARTIAL` resources' cell lists.
    pub fn get_cell_id_lists(&self) -> &[Vec<i32>] {
        &self.cell_lists
    }

    /// `Modify(ctx, const Apto::Array<double>&)`: update, then add to every
    /// global amount (spatial entries change the unused global slot).
    ///
    /// The C++ asserts one entry per resource; a shorter slice changes only
    /// the resources it covers.
    pub fn modify(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv, res_change: &[f64]) {
        self.do_updates(rng, env, false);
        for (count, &change) in self.resource_count.iter_mut().zip(res_change) {
            *count += change;
        }
    }

    /// `Modify(ctx, int id, double change)`
    pub fn modify_one(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        res_index: usize,
        change: f64,
    ) {
        self.do_updates(rng, env, false);
        self.resource_count[res_index] += change;
    }

    /// `ModifyCell(ctx, res_change, cell_id)`: update, then apply the change
    /// to the global amount or to the cell (folded immediately, marking the
    /// grid modified when the amount changed).
    ///
    /// An out-of-range `cell_id` leaves spatial resources untouched (the C++
    /// reads outside the grid there); a `res_change` shorter than the number
    /// of resources (a C++ assert) changes only the resources it covers.
    pub fn modify_cell(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        res_change: &[f64],
        cell_id: i32,
    ) {
        self.do_updates(rng, env, false);
        self.apply_cell_change(res_change, cell_id);
    }

    /// The body of [`modify_cell`](Self::modify_cell) after the update.
    fn apply_cell_change(&mut self, res_change: &[f64], cell_id: i32) {
        let n = self.resource_count.len();
        for (i, &change) in res_change.iter().enumerate().take(n) {
            if !is_spatial(self.geometry[i]) {
                self.resource_count[i] += change;
            } else {
                let s = self.spatial_resource_count[i].base_mut();
                if cell_id < 0 || cell_id >= s.size() {
                    continue;
                }
                let temp = s.element(cell_id).amount();
                s.rate(cell_id, change);
                s.state(cell_id);
                if s.element(cell_id).amount() != temp {
                    s.set_modified(true);
                }
            }
        }
    }

    /// Whether every resource is a plain spatial grid with a cell for each of
    /// `num_cells` cells and none is a `PARTIAL` list: then what an organism
    /// reads of a resource, and what it changes, is the amount in its own
    /// cell, and an update of the grids happens once per update of the world
    /// and no sooner (the lazy update of [`do_updates`](Self::do_updates)
    /// has nothing global to advance). The parallel execution mode runs
    /// output tests in organisms' bursts on that basis.
    pub fn are_plain_cell_grids(&self, num_cells: usize) -> bool {
        !self.resource_count.is_empty()
            && self.geometry.iter().all(|&g| is_spatial(g))
            && self.cell_lists.iter().all(Vec::is_empty)
            && self.spatial_resource_count.iter().all(
                |s| matches!(s, SpatialResource::Plain(p) if p.size().max(0) as usize >= num_cells),
            )
    }

    /// `curr_grid_res_cnt` of [`get_cell_resources`](Self::get_cell_resources)
    /// for a cell of [`are_plain_cell_grids`](Self::are_plain_cell_grids)
    /// without updating or taking `&mut self`: the amount of every resource
    /// in `cell_id`, into `out`.
    pub fn frozen_cell_amounts(&self, cell_id: i32, out: &mut Vec<f64>) {
        out.clear();
        out.extend(
            self.spatial_resource_count
                .iter()
                .map(|s| s.base().get_amount(cell_id)),
        );
    }

    /// [`modify_cell`](Self::modify_cell) for a caller that has brought the
    /// grids up to date itself ([`update_resources`](Self::update_resources))
    /// and knows nothing has advanced since: the same change to the cell,
    /// without the lazy update.
    pub fn modify_cell_updated(&mut self, res_change: &[f64], cell_id: i32) {
        self.apply_cell_change(res_change, cell_id);
    }

    /// `Get(ctx, int id)`: update, then the global amount or the spatial sum.
    pub fn get(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv, res_index: usize) -> f64 {
        self.do_updates(rng, env, false);
        if !is_spatial(self.geometry[res_index]) {
            return self.resource_count[res_index];
        }
        self.spatial_resource_count[res_index].base().sum_all()
    }

    /// `Set(ctx, int id, double new_level)`: update, then set the global
    /// amount, or spread `new_level` evenly over the grid.
    pub fn set(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        res_index: usize,
        new_level: f64,
    ) {
        self.do_updates(rng, env, false);
        if !is_spatial(self.geometry[res_index]) {
            self.resource_count[res_index] = new_level;
        } else {
            let s = self.spatial_resource_count[res_index].base_mut();
            for i in 0..s.size() {
                let size = s.size();
                s.set_cell_amount(i, new_level / f64::from(size));
            }
        }
    }

    /// `ResizeSpatialGrids(int, int)`: fresh `x` by `y` grid for every
    /// resource (keeping its current geometry).
    pub fn resize_spatial_grids(&mut self, in_x: i32, in_y: i32) {
        for i in 0..self.resource_count.len() {
            let g = self.geometry[i];
            self.spatial_resource_count[i]
                .base_mut()
                .resize_clear(in_x, in_y, g);
            let n = in_x.wrapping_mul(in_y).max(0) as usize;
            self.curr_spatial_res_cnt[i].resize(n, 0.0);
        }
    }

    /// `GetSpatialResource(int)`
    pub fn get_spatial_resource(&self, id: usize) -> &SpatialResource {
        &self.spatial_resource_count[id]
    }

    /// `GetSpatialResource(int)`, mutable.
    pub fn get_spatial_resource_mut(&mut self, id: usize) -> &mut SpatialResource {
        &mut self.spatial_resource_count[id]
    }

    /// `ReinitializeResources(ctx, double additional_resource)`: every
    /// resource back to its initial plus `additional_resource`.
    pub fn reinitialize_resources(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        additional_resource: f64,
    ) {
        for i in 0..self.resource_name.len() {
            let level = self.resource_initial[i] + additional_resource;
            self.set(rng, env, i, level);
            self.spatial_resource_count[i]
                .base_mut()
                .reset_resource_counts();
            if additional_resource != 0.0 {
                self.spatial_resource_count[i]
                    .base_mut()
                    .rate_all(additional_resource);
                self.spatial_resource_count[i].state_all();
            }
        }
    }

    /// `GetInitialResourceValue(int)`
    pub fn get_initial_resource_value(&self, resource_id: usize) -> f64 {
        self.resource_initial[resource_id]
    }

    /// `GetResName(int)`
    pub fn get_res_name(&self, id: usize) -> &str {
        &self.resource_name[id]
    }

    /// `IsSpatialResource(int res_id)`: geometry is neither `GLOBAL` nor
    /// `PARTIAL`.
    pub fn is_spatial_resource(&self, id: usize) -> bool {
        is_spatial(self.geometry[id])
    }

    /// `GetResourceByName(cString)`: QUIRK: the LAST resource with this name
    /// (unlike [`get_resource_count_id`](Self::get_resource_count_id)), or -1.
    pub fn get_resource_by_name(&self, name: &str) -> i32 {
        let mut result = -1;
        for (i, n) in self.resource_name.iter().enumerate() {
            if n == name {
                result = i as i32;
            }
        }
        result
    }

    /// `GetCurrPeakX(ctx, res_id)`: update, then the peak column.
    pub fn get_curr_peak_x(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        res_id: usize,
    ) -> i32 {
        self.do_updates(rng, env, false);
        self.spatial_resource_count[res_id].base().curr_peak_x()
    }

    /// `GetCurrPeakY(ctx, res_id)`: update, then the peak row.
    pub fn get_curr_peak_y(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        res_id: usize,
    ) -> i32 {
        self.do_updates(rng, env, false);
        self.spatial_resource_count[res_id].base().curr_peak_y()
    }

    /// `GetFrozenPeakX(ctx, res_id)`
    pub fn get_frozen_peak_x(&self, res_id: usize) -> i32 {
        self.spatial_resource_count[res_id].base().curr_peak_x()
    }

    /// `GetFrozenPeakY(ctx, res_id)`
    pub fn get_frozen_peak_y(&self, res_id: usize) -> i32 {
        self.spatial_resource_count[res_id].base().curr_peak_y()
    }

    /// `SetGradientCount(ctx, world, res_id, peakx, ..., plat_val, threshold,
    /// refuge)`: change a gradient's parameters and regenerate it with
    /// `ResetGradRes`. A no-op for a non-gradient resource (the base-class
    /// virtuals do nothing).
    ///
    /// QUIRK: `cGradientCount` does not override `SetGradResistance`,
    /// `SetGradDamage`, `SetGradThreshold`, `SetGradRefuge` or
    /// `SetGradDeathOdds`, so `p.resistance`, `p.damage`, `p.threshold`,
    /// `p.refuge` and `p.death_odds` are ignored; `p.path` and `p.hammer`
    /// are not passed on by the C++ either. `p.init_plat` is `plat_val`.
    pub fn set_gradient_count(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        res_id: usize,
        p: &GradientParams,
    ) {
        let sr = &mut self.spatial_resource_count[res_id];
        let worldx = sr.base().x();
        let worldy = sr.base().y();
        if let Some(g) = sr.gradient_mut() {
            g.apply_set_gradient_count(p);
        }
        sr.reset_grad_res(rng, env, worldx, worldy);
    }

    /// `SetGradientPlatInflow(res_id, inflow)`
    pub fn set_gradient_plat_inflow(&mut self, res_id: usize, inflow: f64) {
        if let Some(g) = self.spatial_resource_count[res_id].gradient_mut() {
            g.set_grad_plat_inflow(inflow);
        }
    }

    /// `SetGradientPlatOutflow(res_id, outflow)`
    pub fn set_gradient_plat_outflow(&mut self, res_id: usize, outflow: f64) {
        if let Some(g) = self.spatial_resource_count[res_id].gradient_mut() {
            g.set_grad_plat_outflow(outflow);
        }
    }

    /// `SetGradientConeInflow(res_id, inflow)`
    pub fn set_gradient_cone_inflow(&mut self, res_id: usize, inflow: f64) {
        if let Some(g) = self.spatial_resource_count[res_id].gradient_mut() {
            g.set_grad_cone_inflow(inflow);
        }
    }

    /// `SetGradientConeOutflow(res_id, outflow)`
    pub fn set_gradient_cone_outflow(&mut self, res_id: usize, outflow: f64) {
        if let Some(g) = self.spatial_resource_count[res_id].gradient_mut() {
            g.set_grad_cone_outflow(outflow);
        }
    }

    /// `SetGradientInflow(res_id, inflow)`
    pub fn set_gradient_inflow(&mut self, res_id: usize, inflow: f64) {
        if let Some(g) = self.spatial_resource_count[res_id].gradient_mut() {
            g.set_gradient_inflow(inflow);
        }
    }

    /// `SetGradPlatVarInflow(ctx, res_id, mean, variance, type)`
    pub fn set_grad_plat_var_inflow(
        &mut self,
        rng: &mut AvidaRng,
        res_id: usize,
        mean: f64,
        variance: f64,
        kind: i32,
    ) {
        if let Some(g) = self.spatial_resource_count[res_id].gradient_mut() {
            g.set_grad_plat_var_inflow(rng, mean, variance, kind);
        }
    }

    /// `SetPredatoryResource(res_id, odds, juvsper)`
    pub fn set_predatory_resource(&mut self, res_id: usize, odds: f64, juvsper: i32) {
        if let Some(g) = self.spatial_resource_count[res_id].gradient_mut() {
            g.set_predatory_resource(odds, juvsper);
        }
    }

    /// `SetProbabilisticResource(ctx, res_id, initial, inflow, outflow,
    /// lambda, theta, x, y, count)`
    #[allow(clippy::too_many_arguments)]
    pub fn set_probabilistic_resource(
        &mut self,
        rng: &mut AvidaRng,
        res_id: usize,
        initial: f64,
        inflow: f64,
        outflow: f64,
        lambda: f64,
        theta: f64,
        x: i32,
        y: i32,
        count: i32,
    ) {
        if let Some(g) = self.spatial_resource_count[res_id].gradient_mut() {
            g.set_probabilistic_resource(rng, initial, inflow, outflow, lambda, theta, x, y, count);
        }
    }

    /// `GetWallCells(res_id)`: `None` (C++ `NULL`) for a non-gradient.
    pub fn get_wall_cells(&self, res_id: usize) -> Option<&[i32]> {
        self.spatial_resource_count[res_id].wall_cells()
    }

    /// `GetMinUsedX(res_id)`
    pub fn get_min_used_x(&self, res_id: usize) -> i32 {
        self.spatial_resource_count[res_id].min_used_x()
    }

    /// `GetMinUsedY(res_id)`
    pub fn get_min_used_y(&self, res_id: usize) -> i32 {
        self.spatial_resource_count[res_id].min_used_y()
    }

    /// `GetMaxUsedX(res_id)`
    pub fn get_max_used_x(&self, res_id: usize) -> i32 {
        self.spatial_resource_count[res_id].max_used_x()
    }

    /// `GetMaxUsedY(res_id)`
    pub fn get_max_used_y(&self, res_id: usize) -> i32 {
        self.spatial_resource_count[res_id].max_used_y()
    }

    /// `SetSpatialUpdate(int)`: spatial resources are advanced up to this
    /// update by the next accessor (`cPopulation::ProcessPreUpdate` passes
    /// the current update).
    pub fn set_spatial_update(&mut self, update: i32) {
        self.spatial_update = update;
    }

    /// `UpdateGlobalResources(ctx)`: `DoUpdates(ctx, true)`, which touches
    /// only global/partial resources and so needs no RNG.
    pub fn update_global_resources(&mut self) {
        self.do_global_updates();
    }

    /// `UpdateRandomResources(ctx)`: `DoUpdates(ctx, false)`.
    pub fn update_random_resources(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv) {
        self.do_updates(rng, env, false);
    }

    /// `UpdateResources(ctx)`: `DoUpdates(ctx, false)`.
    pub fn update_resources(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv) {
        self.do_updates(rng, env, false);
    }

    /// Unapplied fraction of an update (`update_time`).
    pub fn update_time(&self) -> f64 {
        self.update_time
    }

    /// `(m_last_updated, m_spatial_update)`
    pub fn spatial_update_state(&self) -> (i32, i32) {
        (self.last_updated, self.spatial_update)
    }

    /// Row `id` of `decay_precalc`.
    pub fn decay_precalc(&self, id: usize) -> &[f64] {
        &self.decay_precalc[id]
    }

    /// Row `id` of `inflow_precalc`.
    pub fn inflow_precalc(&self, id: usize) -> &[f64] {
        &self.inflow_precalc[id]
    }

    /// `verbosity` as last set by `Setup`.
    pub fn verbosity(&self) -> i32 {
        self.verbosity
    }

    /// The `update_time` bookkeeping at the top of `DoUpdates`: how many
    /// whole `UPDATE_STEP`s have accumulated, keeping the remainder.
    fn take_global_steps(&mut self) -> i32 {
        let num_steps = (self.update_time / UPDATE_STEP) as i32;
        // Preserve remainder of update_time: `update_time -= num_steps *
        // UPDATE_STEP`. The reference build computes this in the Rust
        // helper `avd_rc_remainder_update_time`, which is not fused.
        self.update_time -= f64::from(num_steps) * UPDATE_STEP;
        num_steps
    }

    /// `DoNonSpatialUpdates(ctx, res_id, num_steps)`: advance one
    /// global/partial resource by `num_steps` steps through the precalc
    /// tables.
    fn do_non_spatial_updates(&mut self, res_id: usize, mut num_steps: i32) {
        while num_steps > PRECALC_DISTANCE as i32 {
            self.resource_count[res_id] *= self.decay_precalc[res_id][PRECALC_DISTANCE];
            self.resource_count[res_id] += self.inflow_precalc[res_id][PRECALC_DISTANCE];
            num_steps -= PRECALC_DISTANCE as i32;
        }
        // A negative step count needs a negative `update_time`, i.e. a
        // negative `Update()`; the C++ then reads before the table.
        if num_steps < 0 {
            return;
        }
        let k = num_steps as usize;
        self.resource_count[res_id] *= self.decay_precalc[res_id][k];
        self.resource_count[res_id] += self.inflow_precalc[res_id][k];
    }

    /// `DoSpatialUpdates(ctx, res_id, num_updates)`: `num_updates` rounds
    /// of gradient movement, rectangle inflow and outflow, `CELL`
    /// inflow/outflow, diffusion/gravity and the fold of the deltas.
    fn do_spatial_updates(
        &mut self,
        rng: &mut AvidaRng,
        env: &mut dyn ResourceEnv,
        res_id: usize,
        num_updates: i32,
    ) {
        let inflow = self.inflow_rate[res_id];
        let decay = self.decay_rate[res_id];
        let sr = &mut self.spatial_resource_count[res_id];
        for _ in 0..num_updates {
            sr.update_count(rng, env);
            let s = sr.base_mut();
            s.source(inflow);
            s.sink(decay);
            if s.cell_list_size() > 0 {
                s.cell_inflow();
                s.cell_outflow();
            }
            s.flow_all();
            sr.state_all();
        }
    }

    /// `DoUpdates(ctx, true)`: global/partial resources only (no RNG use,
    /// `m_last_updated` untouched).
    fn do_global_updates(&mut self) {
        let num_steps = self.take_global_steps();
        for res_id in 0..self.resource_count.len() {
            if !is_spatial(self.geometry[res_id]) {
                self.do_non_spatial_updates(res_id, num_steps);
            }
        }
    }

    /// Lends `executor` to the spatial updates: the pending updates of the
    /// plain grid resources (which neither draw random numbers nor call the
    /// population, and do not depend on one another) then run as jobs of
    /// the executor, each resource doing exactly what
    /// [`do_spatial_updates`](Self::do_spatial_updates) does to it, so the
    /// amounts are the same bits. Gradient resources and the global ones
    /// are done in their order as before.
    pub fn set_spatial_executor(&mut self, executor: Option<Arc<dyn SpatialExecutor>>) {
        self.spatial_executor = ExecutorHandle(executor);
    }

    /// The part of [`do_updates`](Self::do_updates) the executor takes:
    /// returns, per resource, whether it was done here.
    fn do_plain_spatial_updates_at_once(&mut self, num_updates: i32) -> Vec<bool> {
        let Some(executor) = self.spatial_executor.0.clone() else {
            return Vec::new();
        };
        if num_updates <= 0 {
            return Vec::new();
        }
        let mut done = vec![false; self.resource_count.len()];
        let mut jobs: Vec<Box<dyn FnOnce() + Send + '_>> = Vec::new();
        for (res_id, ((sr, &inflow), &decay)) in self
            .spatial_resource_count
            .iter_mut()
            .zip(&self.inflow_rate)
            .zip(&self.decay_rate)
            .enumerate()
        {
            if !is_spatial(self.geometry[res_id]) {
                continue;
            }
            let SpatialResource::Plain(s) = sr else {
                continue;
            };
            if s.size() < PARALLEL_MIN_CELLS {
                continue;
            }
            done[res_id] = true;
            jobs.push(Box::new(move || {
                for _ in 0..num_updates {
                    s.source(inflow);
                    s.sink(decay);
                    if s.cell_list_size() > 0 {
                        s.cell_inflow();
                        s.cell_outflow();
                    }
                    s.flow_all();
                    s.state_all();
                }
            }));
        }
        if jobs.len() < 2 {
            return Vec::new();
        }
        executor.run(jobs);
        done
    }

    /// `DoUpdates(ctx, global_only)`.
    ///
    /// Resource-major, as at `47f13dadb`: each resource in id order is
    /// either advanced by the accumulated global steps or (unless
    /// `global_only`) run through all `m_spatial_update - m_last_updated`
    /// pending spatial updates before the next resource is touched; then
    /// `m_last_updated = m_spatial_update`, which moves it backwards if the
    /// spatial update was set lower. With two or more gradient resources and
    /// more than one pending update this orders RNG draws differently from
    /// the update-major loop of `ef5c6d948`.
    fn do_updates(&mut self, rng: &mut AvidaRng, env: &mut dyn ResourceEnv, global_only: bool) {
        let num_steps = self.take_global_steps();
        let num_spatial_updates = self.spatial_update.wrapping_sub(self.last_updated);
        let done = if global_only {
            Vec::new()
        } else {
            self.do_plain_spatial_updates_at_once(num_spatial_updates)
        };
        for res_id in 0..self.resource_count.len() {
            if !is_spatial(self.geometry[res_id]) {
                self.do_non_spatial_updates(res_id, num_steps);
            } else if !global_only && !done.get(res_id).copied().unwrap_or(false) {
                self.do_spatial_updates(rng, env, res_id, num_spatial_updates);
            }
        }
        if !global_only {
            self.last_updated = self.spatial_update;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::RecordingEnv;

    fn global(name: &str, inflow: f64, outflow: f64) -> ResourceSetup {
        ResourceSetup {
            name: name.to_string(),
            inflow,
            decay: 1.0 - outflow,
            ..ResourceSetup::default()
        }
    }

    #[test]
    fn global_chemostat_matches_closed_form() {
        // Inflow 100 per update, outflow 1% per update, from 0: after t
        // updates of 10000 steps the level is the geometric sum
        // step_inflow * (1 - d^n) / (1 - d) with d = 0.99^(1/10000).
        let mut rng = AvidaRng::new(1);
        let mut env = RecordingEnv::new(-1, false);
        let mut rc = ResourceCount::new(1);
        rc.setup(0, &global("food", 100.0, 0.01), &mut rng, &mut env)
            .unwrap();
        let d = 0.99f64.powf(UPDATE_STEP);
        for t in 1..=5 {
            rc.update(1.0);
            let got = rc.get_resources(&mut rng, &mut env)[0];
            let n = f64::from(t) * 10000.0;
            let want = 100.0 * UPDATE_STEP * (1.0 - d.powf(n)) / (1.0 - d);
            assert!((got - want).abs() < 1e-6 * want, "t={t}: {got} vs {want}");
        }
        // About 99.5 after the first update and approaching 100 / -ln(0.99).
        assert!(rc.read_resources()[0] > 480.0);
    }

    #[test]
    fn gradient_setters_are_no_ops_on_plain_resources() {
        let mut rng = AvidaRng::new(1);
        let mut env = RecordingEnv::new(-1, false);
        let mut rc = ResourceCount::new(1);
        rc.resize_spatial_grids(4, 4);
        let mut s = global("grid", 0.0, 0.0);
        s.geometry = geometry::GRID;
        s.initial = 16.0;
        rc.setup(0, &s, &mut rng, &mut env).unwrap();
        let before = rc.get_spatial_resource(0).clone();
        rc.set_gradient_count(&mut rng, &mut env, 0, &GradientParams::default());
        rc.set_gradient_plat_inflow(0, 1.0);
        rc.set_grad_plat_var_inflow(&mut rng, 0, 1.0, 2.0, 0);
        rc.set_predatory_resource(0, 0.5, 1);
        rc.set_probabilistic_resource(&mut rng, 0, 1.0, 1.0, 0.0, 1.0, 0.0, 1, 1, 3);
        assert_eq!(rc.get_spatial_resource(0), &before);
        assert_eq!(rc.get_wall_cells(0), None);
        assert_eq!(rc.get_min_used_x(0), -1);
        assert_eq!(rng.get_uint(1000), AvidaRng::new(1).get_uint(1000));
    }

    #[test]
    fn backwards_spatial_update_resets_last_updated() {
        // 47f13dadb: m_last_updated = m_spatial_update after every
        // non-global DoUpdates, even when that moves it backwards; the
        // updates in between are then run again.
        let mut rng = AvidaRng::new(1);
        let mut env = RecordingEnv::new(-1, false);
        let mut rc = ResourceCount::new(1);
        rc.resize_spatial_grids(2, 2);
        let mut s = global("grid", 4.0, 0.0);
        s.geometry = geometry::TORUS;
        (s.inflow_x1, s.inflow_x2, s.inflow_y1, s.inflow_y2) = (0, 1, 0, 1);
        rc.setup(0, &s, &mut rng, &mut env).unwrap();
        rc.set_spatial_update(3);
        assert_eq!(rc.get(&mut rng, &mut env, 0), 12.0);
        rc.set_spatial_update(1);
        assert_eq!(rc.get(&mut rng, &mut env, 0), 12.0);
        assert_eq!(rc.spatial_update_state(), (1, 1));
        rc.set_spatial_update(2);
        assert_eq!(rc.get(&mut rng, &mut env, 0), 16.0);
        // The global-only path leaves it alone.
        rc.set_spatial_update(9);
        rc.update_global_resources();
        assert_eq!(rc.spatial_update_state(), (2, 9));
    }

    #[test]
    fn unsized_grid_with_inflow_rectangle_does_not_panic() {
        // A spatial resource set up before ResizeSpatialGrids (or a gradient
        // whose ResetGradRes stopped at a configuration error) has a 0x0
        // grid; the inflow rectangle then indexes nothing.
        let mut rng = AvidaRng::new(1);
        let mut env = RecordingEnv::new(-1, false);
        let mut rc = ResourceCount::new(1);
        let mut s = global("grid", 4.0, 0.5);
        s.geometry = geometry::TORUS;
        (s.inflow_x1, s.inflow_x2, s.inflow_y1, s.inflow_y2) = (0, 1, 0, 1);
        (s.outflow_x1, s.outflow_x2, s.outflow_y1, s.outflow_y2) = (0, 1, 0, 1);
        rc.setup(0, &s, &mut rng, &mut env).unwrap();
        rc.set_spatial_update(2);
        assert_eq!(rc.get(&mut rng, &mut env, 0), 0.0);
    }

    /// Runs every job on a thread of its own.
    struct ThreadedExecutor;

    impl SpatialExecutor for ThreadedExecutor {
        fn run<'a>(&self, jobs: Vec<Box<dyn FnOnce() + Send + 'a>>) {
            std::thread::scope(|s| {
                for job in jobs {
                    s.spawn(job);
                }
            });
        }
    }

    #[test]
    fn executor_gives_the_same_amounts_as_the_plain_loop() {
        let build = |executor: Option<Arc<dyn SpatialExecutor>>| {
            let mut rng = AvidaRng::new(1);
            let mut env = RecordingEnv::new(-1, false);
            let mut rc = ResourceCount::new(3);
            rc.resize_spatial_grids(80, 80);
            for (i, (inflow, outflow)) in
                [(640.0, 0.05), (90.0, 0.2), (5.0, 0.0)].iter().enumerate()
            {
                let mut s = global("grid", *inflow, *outflow);
                s.geometry = geometry::GRID;
                s.xdiffuse = 0.2;
                s.ydiffuse = 0.3;
                (s.inflow_x1, s.inflow_x2, s.inflow_y1, s.inflow_y2) =
                    (0, 40 + 10 * i as i32, 0, 79);
                (s.outflow_x1, s.outflow_x2, s.outflow_y1, s.outflow_y2) = (0, 79, 0, 79);
                rc.setup(i, &s, &mut rng, &mut env).unwrap();
            }
            rc.set_spatial_executor(executor);
            for update in 1..=4 {
                rc.set_spatial_update(update * 3);
                let _ = rc.get_cell_resources(100, &mut rng, &mut env);
            }
            (0..3)
                .map(|i| rc.get_spatial_resource(i).clone())
                .collect::<Vec<_>>()
        };
        let plain = build(None);
        let lent = build(Some(Arc::new(ThreadedExecutor)));
        assert_eq!(plain, lent);
        assert!(plain[0].base().get_amount(0) > 0.0);
    }
}
