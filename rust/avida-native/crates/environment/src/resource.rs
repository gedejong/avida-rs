//! Port of `cCellResource` and `cResource` (avida-core/source/main/cResource.{h,cc}).
//!
//! Every field of the C++ classes is a public field here, named after the C++
//! member without its `m_` prefix. Fields with setter logic beyond plain
//! assignment (`index`, `geometry`, the boolean string parsers, `deadly`,
//! the predator triple) keep that logic in methods.

/// `nGeometry` (avida-core/source/main/nGeometry.h).
pub mod geometry {
    /// `nGeometry::GLOBAL`
    pub const GLOBAL: i32 = 0;
    /// `nGeometry::GRID`
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
}

/// `avd_env_geometry_type` (rust/avida-rust/src/environment_helpers.rs), the
/// helper `cResource::SetGeometry` calls on the lower-cased string. Only the
/// four geometries a resource may use are recognised.
pub fn geometry_from_str(s: &str) -> Option<i32> {
    match s {
        "global" => Some(geometry::GLOBAL),
        "grid" => Some(geometry::GRID),
        "torus" => Some(geometry::TORUS),
        "partial" => Some(geometry::PARTIAL),
        _ => None,
    }
}

/// `avd_env_parse_bool_string` (rust/avida-rust/src/environment_helpers.rs):
/// "true"/"1" and "false"/"0" on an already lower-cased string.
pub fn parse_bool_str(s: &str) -> Option<bool> {
    match s {
        "false" | "0" => Some(false),
        "true" | "1" => Some(true),
        _ => None,
    }
}

/// `cCellResource`: a resource in one cell (mini-chemostat).
#[derive(Debug, Clone, Copy, PartialEq)]
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

impl Default for CellResource {
    /// `cCellResource::cCellResource()`: id `cResource::NONE`, zero flows.
    fn default() -> Self {
        CellResource {
            cell_id: Resource::NONE,
            initial: 0.0,
            inflow: 0.0,
            outflow: 0.0,
        }
    }
}

impl CellResource {
    /// `cCellResource::cCellResource(cell_id, initial, inflow, outflow)`.
    pub fn new(cell_id: i32, initial: f64, inflow: f64, outflow: f64) -> Self {
        CellResource {
            cell_id,
            initial,
            inflow,
            outflow,
        }
    }
}

/// `cResource`: everything known about one resource.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    name: String,
    /// 0-based order of appearance in the environment file (library index).
    id: i32,
    /// 0-based order of appearance among resources of the same kind
    /// (deme / non-deme); -1 until assigned.
    index: i32,
    /// `initial`
    pub initial: f64,
    /// `inflow`
    pub inflow: f64,
    /// `outflow`
    pub outflow: f64,
    /// `geometry` (an `nGeometry` value)
    pub geometry: i32,
    /// `inflowX1`
    pub inflow_x1: i32,
    /// `inflowX2`
    pub inflow_x2: i32,
    /// `inflowY1`
    pub inflow_y1: i32,
    /// `inflowY2`
    pub inflow_y2: i32,
    /// `outflowX1`
    pub outflow_x1: i32,
    /// `outflowX2`
    pub outflow_x2: i32,
    /// `outflowY1`
    pub outflow_y1: i32,
    /// `outflowY2`
    pub outflow_y2: i32,
    /// `xdiffuse`
    pub xdiffuse: f64,
    /// `xgravity`
    pub xgravity: f64,
    /// `ydiffuse`
    pub ydiffuse: f64,
    /// `ygravity`
    pub ygravity: f64,
    /// `deme_resource`
    pub deme_resource: bool,
    /// `org_resources`. QUIRK: never initialised or used by the C++
    /// (`SetOrgResource` is declared but has no definition); `false` here.
    pub org_resources: bool,
    /// `energy_resource` (only implemented for spatial resources)
    pub energy_resource: bool,
    /// `peaks`
    pub peaks: i32,
    /// `min_height`
    pub min_height: f64,
    /// `height_range`
    pub height_range: f64,
    /// `min_radius`
    pub min_radius: f64,
    /// `radius_range`
    pub radius_range: f64,
    /// `ah`
    pub ah: f64,
    /// `ar`
    pub ar: f64,
    /// `acx`
    pub acx: f64,
    /// `acy`
    pub acy: f64,
    /// `hstepscale`
    pub hstepscale: f64,
    /// `rstepscale`
    pub rstepscale: f64,
    /// `cstepscalex`
    pub cstepscalex: f64,
    /// `cstepscaley`
    pub cstepscaley: f64,
    /// `hstep`
    pub hstep: f64,
    /// `rstep`
    pub rstep: f64,
    /// `cstepx`
    pub cstepx: f64,
    /// `cstepy`
    pub cstepy: f64,
    /// `update_dynamic`
    pub update_dynamic: i32,
    /// `m_peakx`
    pub peakx: i32,
    /// `m_peaky`
    pub peaky: i32,
    /// `m_height`
    pub height: i32,
    /// `m_spread`
    pub spread: i32,
    /// `m_plateau`
    pub plateau: f64,
    /// `m_decay`
    pub decay: i32,
    /// `m_max_x`
    pub max_x: i32,
    /// `m_max_y`
    pub max_y: i32,
    /// `m_min_x`
    pub min_x: i32,
    /// `m_min_y`
    pub min_y: i32,
    /// `m_move_a_scaler`
    pub move_a_scaler: f64,
    /// `m_updatestep`
    pub updatestep: i32,
    /// `m_halo`
    pub halo: i32,
    /// `m_halo_inner_radius`
    pub halo_inner_radius: i32,
    /// `m_halo_width`
    pub halo_width: i32,
    /// `m_halo_anchor_x`
    pub halo_anchor_x: i32,
    /// `m_halo_anchor_y`
    pub halo_anchor_y: i32,
    /// `m_move_speed`
    pub move_speed: i32,
    /// `m_move_resistance`
    pub move_resistance: i32,
    /// `m_plateau_inflow`
    pub plateau_inflow: f64,
    /// `m_plateau_outflow`
    pub plateau_outflow: f64,
    /// `m_cone_inflow`
    pub cone_inflow: f64,
    /// `m_cone_outflow`
    pub cone_outflow: f64,
    /// `m_gradient_inflow`
    pub gradient_inflow: f64,
    /// `m_is_plateau_common`
    pub is_plateau_common: i32,
    /// `m_floor`
    pub floor: f64,
    /// `m_habitat`
    pub habitat: i32,
    /// `m_min_size`
    pub min_size: i32,
    /// `m_max_size`
    pub max_size: i32,
    /// `m_config`
    pub config: i32,
    /// `m_count`
    pub count: i32,
    /// `m_resistance`
    pub resistance: f64,
    /// `m_init_plat` (`GetInitialPlatVal` / `SetPlatInitial`)
    pub init_plat: f64,
    /// `m_threshold`
    pub threshold: f64,
    /// `m_refuge`
    pub refuge: i32,
    /// `isgradient`
    pub isgradient: bool,
    /// `cell_list`
    pub cell_list: Vec<CellResource>,
    /// `cell_id_list` (SLG partial resources)
    pub cell_id_list: Vec<i32>,
    /// `m_predator_odds`
    pub predator_odds: f64,
    /// `m_predator`
    pub predator: bool,
    /// `m_death_odds`
    pub death_odds: f64,
    /// `m_deadly`
    pub deadly: bool,
    /// `m_path`
    pub path: bool,
    /// `m_hammer`
    pub hammer: bool,
    /// `m_guard_juvs_per`
    pub guard_juvs_per: f64,
    /// `m_damage`
    pub damage: f64,
    /// `m_prob_detect`
    pub prob_detect: f64,
    /// `hgt_metabolize`
    pub hgt_metabolize: bool,
    /// `collectable`
    pub collectable: bool,
}

impl Resource {
    /// `cResource::NONE`
    pub const NONE: i32 = -99;

    /// `cResource::cResource(name, id)`.
    pub fn new(name: &str, id: i32) -> Self {
        Resource {
            name: name.to_string(),
            id,
            index: -1,
            initial: 0.0,
            inflow: 0.0,
            outflow: 0.0,
            geometry: geometry::GLOBAL,
            // QUIRK: the C++ initialiser is `inflowX1( )` (value-initialised to
            // 0), not `cResource::NONE` like the other seven box coordinates.
            // `LoadResource` then copies it into `inflowX2`, and an HGT resource
            // fails the "misconfigured" check unless `inflowx1` is set to -99.
            inflow_x1: 0,
            inflow_x2: Self::NONE,
            inflow_y1: Self::NONE,
            inflow_y2: Self::NONE,
            outflow_x1: Self::NONE,
            outflow_x2: Self::NONE,
            outflow_y1: Self::NONE,
            outflow_y2: Self::NONE,
            xdiffuse: 1.0,
            xgravity: 0.0,
            ydiffuse: 1.0,
            ygravity: 0.0,
            deme_resource: false,
            org_resources: false,
            energy_resource: false,
            peaks: 0,
            min_height: 0.0,
            height_range: 0.0,
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
            peakx: -1,
            peaky: -1,
            height: 0,
            spread: 0,
            plateau: -1.0,
            decay: 1,
            max_x: 0,
            max_y: 0,
            min_x: 0,
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
            init_plat: -1.0,
            threshold: 1.0,
            refuge: 0,
            isgradient: false,
            cell_list: Vec::new(),
            cell_id_list: Vec::new(),
            predator_odds: 0.0,
            predator: false,
            death_odds: 0.0,
            deadly: false,
            path: false,
            hammer: false,
            guard_juvs_per: 0.0,
            damage: 0.0,
            prob_detect: 1.0,
            hgt_metabolize: false,
            collectable: true,
        }
    }

    /// `GetName()`.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// `GetID()`: position in the resource library.
    pub fn id(&self) -> i32 {
        self.id
    }
    /// `GetIndex()`: position among resources of the same kind, -1 if unset.
    pub fn index(&self) -> i32 {
        self.index
    }
    /// `SetIndex(index)`: can only be assigned once.
    pub fn set_index(&mut self, index: i32) {
        if self.index < 0 {
            self.index = index;
        }
    }

    /// `SetGeometry(cString)`: case-insensitive; false (unchanged) when the
    /// name is unknown.
    pub fn set_geometry(&mut self, geometry: &str) -> bool {
        let lower = geometry.to_ascii_lowercase();
        match geometry_from_str(&lower) {
            Some(g) => {
                self.geometry = g;
                true
            }
            None => false,
        }
    }

    /// `SetDemeResource(cString)`: "true"/"false"/"1"/"0", case-insensitive.
    pub fn set_deme_resource(&mut self, value: &str) -> bool {
        match parse_bool_str(&value.to_ascii_lowercase()) {
            Some(v) => {
                self.deme_resource = v;
                true
            }
            None => false,
        }
    }

    /// `SetEnergyResource(cString)`: as `set_deme_resource`.
    pub fn set_energy_resource(&mut self, value: &str) -> bool {
        match parse_bool_str(&value.to_ascii_lowercase()) {
            Some(v) => {
                self.energy_resource = v;
                true
            }
            None => false,
        }
    }

    /// `SetDeadly(death_odds)`: also sets `deadly = death_odds != 0`.
    pub fn set_deadly(&mut self, death_odds: f64) {
        self.death_odds = death_odds;
        self.deadly = self.death_odds != 0.0;
    }

    /// `SetPredatoryResource(odds, juvsper, prob)`. The C++ takes `juvsper`
    /// as an `int` and stores it in the double `m_guard_juvs_per`.
    pub fn set_predatory_resource(&mut self, odds: f64, juvsper: i32, prob: f64) {
        self.predator = true;
        self.predator_odds = odds;
        self.guard_juvs_per = juvsper as f64;
        self.prob_detect = prob;
    }

    /// `IsNest()`: habitat 3 or 4.
    pub fn is_nest(&self) -> bool {
        self.habitat == 3 || self.habitat == 4
    }

    /// `SetCellIdList(id_list)`.
    pub fn set_cell_id_list(&mut self, id_list: &[i32]) {
        self.cell_id_list = id_list.to_vec();
    }

    /// `AddCellResource(cell)`.
    pub fn add_cell_resource(&mut self, cell: CellResource) {
        self.cell_list.push(cell);
    }

    /// `GetCellResourcePtr(id)`: first cell resource with that cell id.
    pub fn cell_resource_mut(&mut self, cell_id: i32) -> Option<&mut CellResource> {
        self.cell_list.iter_mut().find(|c| c.cell_id == cell_id)
    }

    /// `UpdateCellResource(ptr, initial, inflow, outflow)`.
    pub fn update_cell_resource(cell: &mut CellResource, initial: f64, inflow: f64, outflow: f64) {
        cell.initial = initial;
        cell.inflow = inflow;
        cell.outflow = outflow;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_constructor() {
        let r = Resource::new("food", 3);
        assert_eq!(r.name(), "food");
        assert_eq!(r.id(), 3);
        assert_eq!(r.index(), -1);
        assert_eq!(r.geometry, geometry::GLOBAL);
        assert_eq!(r.inflow_x1, 0); // QUIRK
        assert_eq!(r.inflow_x2, Resource::NONE);
        assert_eq!(r.outflow_y2, Resource::NONE);
        assert_eq!(r.xdiffuse, 1.0);
        assert_eq!(r.ydiffuse, 1.0);
        assert_eq!(r.peakx, -1);
        assert_eq!(r.plateau, -1.0);
        assert_eq!(r.decay, 1);
        assert_eq!(r.move_a_scaler, 1.0);
        assert_eq!(r.updatestep, 1);
        assert_eq!(r.move_speed, 1);
        assert_eq!(r.min_size, 1);
        assert_eq!(r.max_size, 1);
        assert_eq!(r.count, 1);
        assert_eq!(r.resistance, 1.0);
        assert_eq!(r.init_plat, -1.0);
        assert_eq!(r.threshold, 1.0);
        assert_eq!(r.prob_detect, 1.0);
        assert!(r.collectable);
        assert!(!r.hgt_metabolize);
    }

    #[test]
    fn setters() {
        let mut r = Resource::new("x", 0);
        assert!(r.set_geometry("TORUS"));
        assert_eq!(r.geometry, geometry::TORUS);
        assert!(!r.set_geometry("hex"));
        assert_eq!(r.geometry, geometry::TORUS);
        assert!(r.set_deme_resource("TRUE"));
        assert!(r.deme_resource);
        assert!(r.set_deme_resource("0"));
        assert!(!r.deme_resource);
        assert!(!r.set_energy_resource("yes"));
        r.set_index(4);
        r.set_index(7);
        assert_eq!(r.index(), 4);
        r.set_deadly(0.5);
        assert!(r.deadly);
        r.set_deadly(0.0);
        assert!(!r.deadly);
        r.habitat = 4;
        assert!(r.is_nest());
        r.add_cell_resource(CellResource::new(5, 1.0, 2.0, 3.0));
        let c = r.cell_resource_mut(5).unwrap();
        Resource::update_cell_resource(c, 4.0, 5.0, 6.0);
        assert_eq!(r.cell_list[0], CellResource::new(5, 4.0, 5.0, 6.0));
        assert!(r.cell_resource_mut(6).is_none());
        assert_eq!(CellResource::default().cell_id, Resource::NONE);
    }
}
