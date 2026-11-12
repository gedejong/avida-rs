//! Bit-exact comparison with traces of the original C++.
//!
//! `testdata/*.txt` come from `tools/ref_harness.cc` (see
//! `tools/build_ref.sh`), which runs the pre-Rust `cResourceCount`,
//! `cSpatialResCount`, `cSpatialCountElem` and `cGradientCount` with the real
//! Apto RNG, compiled with the reference build's flags. Each test here
//! replays the same scenario and must print the same lines; doubles are
//! compared as bit patterns.

use avida_apto::AvidaRng;

use crate::cell_resource::CellResource;
use crate::env::{RecordingEnv, ResourceEnv};
use crate::geometry;
use crate::resource_count::{ResourceCount, ResourceSetup, PRECALC_DISTANCE};

pub(crate) fn hx(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

pub(crate) fn compare(name: &str, expected: &str, got: &[String]) {
    let exp: Vec<&str> = expected.lines().collect();
    for (i, (e, g)) in exp.iter().zip(got).enumerate() {
        assert_eq!(*e, g.as_str(), "{name}: first difference at line {}", i + 1);
    }
    assert_eq!(exp.len(), got.len(), "{name}: line count");
}

pub(crate) fn res(name: &str) -> ResourceSetup {
    ResourceSetup {
        name: name.to_string(),
        ..ResourceSetup::default()
    }
}

fn setup(
    rc: &mut ResourceCount,
    idx: usize,
    s: &ResourceSetup,
    rng: &mut AvidaRng,
    env: &mut dyn ResourceEnv,
    out: &mut Vec<String>,
) {
    let lines = rc.setup(idx, s, rng, env).expect("setup");
    out.extend(lines);
}

#[test]
fn global_matches_cpp_reference() {
    let mut out = Vec::new();
    let mut rng = AvidaRng::new(1);
    let mut env = RecordingEnv::new(-1, false);
    let mut rc = ResourceCount::new(2);
    rc.resize_spatial_grids(5, 5);
    // Inflow 100, outflow 0.01, from 0.
    let mut a = res("food");
    a.initial = 0.0;
    a.inflow = 100.0;
    a.decay = 1.0 - 0.01;
    a.verbosity = 2;
    let mut b = res("part");
    b.initial = 3.5;
    b.inflow = 1.25;
    b.decay = 1.0 - 0.3;
    b.geometry = geometry::PARTIAL;
    b.cell_id_list = vec![3, 7, 11];
    setup(&mut rc, 0, &a, &mut rng, &mut env, &mut out);
    setup(&mut rc, 1, &b, &mut rng, &mut env, &mut out);
    assert_eq!(rc.get_cell_id_lists()[1], vec![3, 7, 11]);
    for k in 0..=PRECALC_DISTANCE {
        out.push(format!(
            "P {k} {} {} {} {}",
            hx(rc.decay_precalc(0)[k]),
            hx(rc.inflow_precalc(0)[k]),
            hx(rc.decay_precalc(1)[k]),
            hx(rc.inflow_precalc(1)[k])
        ));
    }
    let steps = [
        0.37,
        0.5,
        1.0,
        0.0001,
        0.00005,
        2.5,
        1.0 / 3.0,
        0.123456789,
        0.99999,
        0.00009999,
        7.0,
        0.25,
        0.0,
        1e-7,
        0.0300003,
    ];
    for (i, s) in steps.iter().enumerate() {
        rc.update(*s);
        let r = rc.get_resources(&mut rng, &mut env).to_vec();
        out.push(format!(
            "G {i} {} {} {}",
            hx(r[0]),
            hx(r[1]),
            hx(rc.update_time())
        ));
    }
    rc.set_inflow("food", 50.0).unwrap();
    rc.set_decay("food", 0.95).unwrap();
    for k in (0..=PRECALC_DISTANCE).step_by(10) {
        out.push(format!(
            "P2 {k} {} {}",
            hx(rc.decay_precalc(0)[k]),
            hx(rc.inflow_precalc(0)[k])
        ));
    }
    rc.update(0.75);
    let ch = [-3.0, 0.5];
    rc.modify(&mut rng, &mut env, &ch);
    let r = rc.read_resources();
    out.push(format!(
        "M {} {} {}",
        hx(r[0]),
        hx(r[1]),
        hx(rc.update_time())
    ));
    rc.update(0.6);
    rc.modify_one(&mut rng, &mut env, 1, -0.125);
    rc.modify_cell(&mut rng, &mut env, &ch, 4);
    let r = rc.read_resources();
    out.push(format!("M {} {}", hx(r[0]), hx(r[1])));
    rc.set(&mut rng, &mut env, 0, 12.5);
    rc.update(1.5);
    let g0 = rc.get(&mut rng, &mut env, 0);
    let g1 = rc.get(&mut rng, &mut env, 1);
    out.push(format!("S {} {}", hx(g0), hx(g1)));
    let cr = rc.get_cell_resources(9, &mut rng, &mut env).to_vec();
    out.push(format!("C {} {}", hx(cr[0]), hx(cr[1])));
    rc.reinitialize_resources(&mut rng, &mut env, 0.25);
    let r = rc.read_resources();
    out.push(format!("R {} {}", hx(r[0]), hx(r[1])));
    let id = |r: Result<usize, String>| r.map(|v| v as i32).unwrap_or(-1);
    out.push(format!(
        "ID {} {} {} {}",
        id(rc.get_resource_count_id("part")),
        rc.get_resource_by_name("food"),
        rc.get_resource_by_name("nope"),
        id(rc.get_resource_count_id("nope"))
    ));
    assert_eq!(
        rc.get_resource_count_id("nope"),
        Err("Error: Unknown resource 'nope'.".to_string())
    );
    out.push(format!(
        "I {} {}",
        hx(rc.get_inflow("food").unwrap()),
        hx(rc.get_decay("part").unwrap())
    ));
    compare("global", include_str!("../testdata/global.txt"), &out);
}

fn dump_spatial(
    tag: &str,
    u: i32,
    rc: &mut ResourceCount,
    rng: &mut AvidaRng,
    env: &mut dyn ResourceEnv,
    out: &mut Vec<String>,
) {
    let sp = rc.get_spatial_res(rng, env).to_vec();
    for (i, row) in sp.iter().enumerate() {
        let mut line = format!("{tag} {u} r{i}");
        for v in row {
            line.push(' ');
            line.push_str(&hx(*v));
        }
        line.push_str(" d");
        for e in rc.get_spatial_resource(i).base().elements() {
            line.push(' ');
            line.push_str(&hx(e.delta()));
        }
        out.push(line);
    }
}

#[test]
fn spatial_matches_cpp_reference() {
    let mut out = Vec::new();
    let mut rng = AvidaRng::new(1);
    let mut env = RecordingEnv::new(-1, false);
    let mut rc = ResourceCount::new(4);
    rc.resize_spatial_grids(5, 5);
    // Pure diffusion on a 5x5 torus from one seeded cell.
    let mut d = res("diffuse");
    d.geometry = geometry::TORUS;
    d.decay = 1.0;
    d.cell_list = vec![CellResource::new(12, 100.0, 0.0, 0.0)];
    // Torus with gravity, inflow/outflow boxes and cell inflow/outflow.
    let mut t = res("torus");
    t.geometry = geometry::TORUS;
    t.initial = 25.0;
    t.inflow = 5.0;
    t.decay = 1.0 - 0.1;
    t.xdiffuse = 1.0;
    t.xgravity = 0.2;
    t.ydiffuse = 0.5;
    t.ygravity = -0.1;
    (t.inflow_x1, t.inflow_x2, t.inflow_y1, t.inflow_y2) = (1, 2, 1, 2);
    (t.outflow_x1, t.outflow_x2, t.outflow_y1, t.outflow_y2) = (3, 5, 0, 1);
    t.verbosity = 2;
    t.cell_list = vec![
        CellResource::new(12, 3.0, 0.5, 0.2),
        CellResource::new(24, 1.0, 0.0, 0.05),
        CellResource::new(25, 9.0, 1.0, 0.5),
        CellResource::new(-3, 9.0, 1.0, 0.5),
    ];
    // Bounded grid with the same flows.
    let mut g = res("grid");
    g.geometry = geometry::GRID;
    g.initial = 25.0;
    g.inflow = 5.0;
    g.decay = 1.0 - 0.1;
    g.xdiffuse = 1.0;
    g.xgravity = -0.3;
    g.ydiffuse = 0.75;
    g.ygravity = 0.4;
    (g.inflow_x1, g.inflow_x2, g.inflow_y1, g.inflow_y2) = (0, 0, 4, 4);
    (g.outflow_x1, g.outflow_x2, g.outflow_y1, g.outflow_y2) = (2, 3, 2, 3);
    let mut gl = res("glob");
    gl.initial = 10.0;
    gl.inflow = 2.0;
    gl.decay = 0.5;
    setup(&mut rc, 0, &d, &mut rng, &mut env, &mut out);
    setup(&mut rc, 1, &t, &mut rng, &mut env, &mut out);
    setup(&mut rc, 2, &g, &mut rng, &mut env, &mut out);
    setup(&mut rc, 3, &gl, &mut rng, &mut env, &mut out);
    for i in 0..25 {
        let e = rc.get_spatial_resource(2).base().element(i);
        let mut line = format!("PTR {i}");
        for k in 0..8 {
            line.push_str(&format!(
                " {},{},{},{}",
                e.elem_ptr(k),
                e.ptr_xdist(k),
                e.ptr_ydist(k),
                hx(e.ptr_dist(k))
            ));
        }
        out.push(line);
    }
    dump_spatial("SP", 0, &mut rc, &mut rng, &mut env, &mut out);
    for u in 1..=8 {
        rc.set_spatial_update(u);
        rc.update(1.0);
        dump_spatial("SP", u, &mut rc, &mut rng, &mut env, &mut out);
        if u == 3 {
            rc.modify_cell(&mut rng, &mut env, &[-0.5, -1.0, 2.0, -1.0], 7);
            dump_spatial("MC", u, &mut rc, &mut rng, &mut env, &mut out);
            let m = |i: usize| u8::from(rc.get_spatial_resource(i).base().modified());
            out.push(format!("MOD {} {} {}", m(0), m(1), m(2)));
        }
        let s: Vec<String> = (0..4).map(|i| hx(rc.get(&mut rng, &mut env, i))).collect();
        out.push(format!("SUM {u} {}", s.join(" ")));
    }
    let oob = rc.get_cell_resources(30, &mut rng, &mut env).to_vec();
    out.push(format!(
        "OOB {} {} {} {}",
        hx(oob[0]),
        hx(oob[1]),
        hx(oob[2]),
        hx(oob[3])
    ));
    rc.set_spatial_update(10);
    dump_spatial("SP", 10, &mut rc, &mut rng, &mut env, &mut out);
    rc.set(&mut rng, &mut env, 1, 50.0);
    dump_spatial("SET", 10, &mut rc, &mut rng, &mut env, &mut out);
    rc.set_cell_resources(6, &[1.5, 2.5, 3.5, 4.5]);
    dump_spatial("SETCELL", 10, &mut rc, &mut rng, &mut env, &mut out);
    rc.reinitialize_resources(&mut rng, &mut env, 0.5);
    dump_spatial("REINIT", 10, &mut rc, &mut rng, &mut env, &mut out);
    rc.reinitialize_resources(&mut rng, &mut env, 0.0);
    dump_spatial("REINIT0", 10, &mut rc, &mut rng, &mut env, &mut out);
    compare("spatial", include_str!("../testdata/spatial.txt"), &out);
}

// ---------------------------------------------------------------- gradient

use crate::gradient_count::GradientParams;
use crate::spatial_res_count::SpatialResCount;

/// The harness world: `cStats::GetUpdate`, `ctx.HasDriver()`, feedback
/// errors, and the stub `cPopulation`, which draws `GetUInt(1000)` per call
/// and prints a `CALL` line into the same trace.
struct TraceEnv {
    update: i32,
    has_driver: bool,
    errors: Vec<String>,
    out: Vec<String>,
}

impl TraceEnv {
    fn new() -> Self {
        TraceEnv {
            update: -1,
            has_driver: false,
            errors: Vec::new(),
            out: Vec::new(),
        }
    }
}

impl ResourceEnv for TraceEnv {
    fn update(&self) -> i32 {
        self.update
    }

    fn has_driver(&self) -> bool {
        self.has_driver
    }

    fn feedback_error(&mut self, msg: &str) {
        self.errors.push(msg.to_string());
    }

    fn execute_predatory_resource(
        &mut self,
        rng: &mut AvidaRng,
        cell_id: i32,
        odds: f64,
        juvs: i32,
        hammer: bool,
    ) {
        let r = rng.get_uint(1000);
        self.out.push(format!(
            "CALL pred {cell_id} {} {juvs} {} {r}",
            hx(odds),
            u8::from(hammer)
        ));
    }

    fn execute_damaging_resource(
        &mut self,
        rng: &mut AvidaRng,
        cell_id: i32,
        damage: f64,
        hammer: bool,
    ) {
        let r = rng.get_uint(1000);
        self.out.push(format!(
            "CALL damage {cell_id} {} {} {r}",
            hx(damage),
            u8::from(hammer)
        ));
    }

    fn execute_deadly_resource(
        &mut self,
        rng: &mut AvidaRng,
        cell_id: i32,
        odds: f64,
        hammer: bool,
    ) {
        let r = rng.get_uint(1000);
        self.out.push(format!(
            "CALL deadly {cell_id} {} {} {r}",
            hx(odds),
            u8::from(hammer)
        ));
    }
}

fn fnv(mut h: u64, v: u64) -> u64 {
    for i in 0..8 {
        h ^= (v >> (8 * i)) & 0xff;
        h = h.wrapping_mul(1_099_511_628_211);
    }
    h
}

const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;

fn grid_hash(s: &SpatialResCount) -> u64 {
    let mut h = FNV_OFFSET;
    for e in s.elements() {
        h = fnv(h, e.amount().to_bits());
    }
    for e in s.elements() {
        h = fnv(h, e.delta().to_bits());
    }
    h
}

fn dump_grad(tag: &str, u: i32, rc: &ResourceCount, id: usize, out: &mut Vec<String>) {
    let s = rc.get_spatial_resource(id);
    let g = s.gradient().expect("gradient resource");
    let b = s.base();
    let mut ph = FNV_OFFSET;
    for v in &g.plateau_array {
        ph = fnv(ph, v.to_bits());
    }
    for v in g
        .plateau_cell_ids
        .iter()
        .chain(&g.wall_cells)
        .chain(&g.prob_res_cells)
    {
        ph = fnv(ph, i64::from(*v) as u64);
    }
    out.push(format!(
        "{tag} {u} r{id} peak={},{} curr={},{} ys={} sign={},{} cnt={},{},{},{} mod={} ch={} cp={} loss={} pin={} \
         used={},{},{},{} grid={:016x} int={:016x}",
        g.peakx,
        g.peaky,
        b.curr_peak_x(),
        b.curr_peak_y(),
        hx(g.move_y_scaler),
        g.movesignx,
        g.movesigny,
        g.counter,
        g.move_counter,
        g.skip_counter,
        g.topo_counter,
        u8::from(b.modified()),
        hx(g.current_height),
        hx(g.common_plat_height),
        hx(g.ave_plat_cell_loss),
        hx(g.plateau_inflow),
        g.min_usedx,
        g.min_usedy,
        g.max_usedx,
        g.max_usedy,
        grid_hash(b),
        ph
    ));
    if g.halo != 0 && g.move_a_scaler > 1.0 {
        out.push(format!(
            "{tag} {u} r{id} halo dir={} ch={}",
            g.halo_dir, g.changling
        ));
    }
}

/// `GradRes` in the harness: `cEnvironment::LoadGradientResource` defaults
/// plus `cPopulation`'s `decay = 1 - outflow` (the harness uses outflow 1).
fn grad_res(name: &str, g: GradientParams) -> ResourceSetup {
    ResourceSetup {
        geometry: geometry::GRID,
        initial: 0.0,
        decay: 0.0,
        xdiffuse: 0.0,
        ydiffuse: 0.0,
        xgravity: 0.0,
        ygravity: 0.0,
        gradient: g,
        is_gradient: true,
        ..res(name)
    }
}

fn expected_for(tag: &str, all: &str) -> String {
    let prefix = format!("{tag} ");
    let mut s = String::new();
    for l in all.lines().filter(|l| l.starts_with(&prefix)) {
        s.push_str(l);
        s.push('\n');
    }
    s
}

const GRADIENT_TXT: &str = include_str!("../testdata/gradient.txt");

#[allow(clippy::too_many_arguments)]
fn run_gradient(
    tag: &str,
    wx: i32,
    wy: i32,
    seed: i32,
    res: &[ResourceSetup],
    updates: i32,
    every: i32,
    frac: f64,
) {
    let mut rng = AvidaRng::new(seed);
    let mut env = TraceEnv::new();
    let mut rc = ResourceCount::new(res.len());
    rc.resize_spatial_grids(wx, wy);
    for (i, r) in res.iter().enumerate() {
        let lines = rc.setup(i, r, &mut rng, &mut env).expect("setup");
        env.out.extend(lines);
    }
    for (i, r) in res.iter().enumerate() {
        if r.is_gradient {
            dump_grad(tag, -1, &rc, i, &mut env.out);
        }
    }
    env.has_driver = true;
    for u in 0..updates {
        env.update = u;
        rc.set_spatial_update(u);
        rc.get_resources(&mut rng, &mut env);
        for (i, r) in res.iter().enumerate() {
            if r.is_gradient {
                dump_grad(tag, u, &rc, i, &mut env.out);
            }
        }
        if every > 0 && u % every == 1 {
            for i in 0..res.len() {
                let px = rc.get_frozen_peak_x(i);
                let py = rc.get_frozen_peak_y(i);
                for c in [py * wx + px, py * wx + px + 1] {
                    if c < 0 || c >= wx * wy {
                        continue;
                    }
                    let mut ch = vec![0.0; res.len()];
                    ch[i] = -rc.get_frozen_cell_res_val(c, i) * frac;
                    rc.modify_cell(&mut rng, &mut env, &ch, c);
                }
            }
        }
    }
    let errors = std::mem::take(&mut env.errors);
    for e in errors {
        env.out.push(format!("{tag} ERR {e}"));
    }
    env.out
        .push(format!("{tag} RNG {}", rng.get_uint(1_000_000_000)));
    compare(tag, &expected_for(tag, GRADIENT_TXT), &env.out);
}

fn gp() -> GradientParams {
    GradientParams::default()
}

#[test]
fn gradient_regular_move_matches_cpp() {
    let g = GradientParams {
        height: 20,
        spread: 120,
        plateau: 1.0,
        decay: 1,
        move_a_scaler: 3.8,
        max_x: 80,
        min_x: 40,
        max_y: 80,
        min_y: 40,
        plateau_inflow: 0.01,
        ..gp()
    };
    run_gradient("REGMOVE", 120, 120, 9, &[grad_res("food0", g)], 40, 3, 0.5);
}

#[test]
fn gradient_halo_move_matches_cpp() {
    let g = GradientParams {
        height: 20,
        spread: 120,
        plateau: 1.0,
        decay: 1,
        move_a_scaler: 3.8,
        halo: 1,
        halo_inner_radius: -10,
        halo_anchor_x: 59,
        halo_anchor_y: 59,
        halo_width: 45,
        plateau_inflow: 0.5,
        updatestep: 5,
        ..gp()
    };
    run_gradient(
        "HALOMOVE",
        120,
        120,
        9,
        &[grad_res("food0", g)],
        40,
        4,
        0.75,
    );
}

#[test]
fn gradient_common_plateau_matches_cpp() {
    let g = GradientParams {
        height: 20,
        spread: 120,
        plateau: 1.0,
        decay: 1,
        move_a_scaler: 3.8,
        halo: 1,
        halo_inner_radius: -20,
        halo_anchor_x: 59,
        halo_anchor_y: 59,
        halo_width: 40,
        plateau_inflow: 0.01,
        is_plateau_common: 1,
        ..gp()
    };
    run_gradient(
        "COMMON",
        120,
        120,
        9,
        &[grad_res("food0", g.clone())],
        30,
        2,
        0.5,
    );
    // With plateau outflow, so the fused common-height update matters.
    let g2 = GradientParams {
        plateau_outflow: 0.03,
        plateau_inflow: 0.07,
        ..g
    };
    run_gradient("COMMON2", 120, 120, 9, &[grad_res("food0", g2)], 30, 2, 0.5);
}

#[test]
fn gradient_halo_generate_matches_cpp() {
    // decay > 1: bitten peaks rot and regenerate.
    let g = GradientParams {
        height: 20,
        spread: 120,
        plateau: 1.0,
        decay: 5,
        move_a_scaler: 1.0,
        updatestep: 200,
        halo: 1,
        halo_inner_radius: -10,
        halo_anchor_x: 59,
        halo_anchor_y: 59,
        halo_width: 40,
        ..gp()
    };
    run_gradient("HALOGEN", 120, 120, 9, &[grad_res("food0", g)], 20, 1, 0.9);
}

#[test]
fn gradient_avatar_walls_matches_cpp() {
    let f = GradientParams {
        height: 21,
        spread: 20,
        plateau: 1.0,
        decay: 1,
        move_a_scaler: 1.0,
        updatestep: 10000,
        peakx: 30,
        peaky: 30,
        min_x: 0,
        max_x: 60,
        min_y: 0,
        max_y: 60,
        plateau_inflow: 0.002,
        init_plat: 1.0,
        ..gp()
    };
    let l = GradientParams {
        habitat: 2,
        plateau: 1.1,
        updatestep: 10_000_000,
        count: 1,
        max_size: 60,
        min_size: 60,
        config: 3,
        peakx: 0,
        peaky: 0,
        ..gp()
    };
    let rt = GradientParams {
        peakx: 59,
        ..l.clone()
    };
    let t = GradientParams {
        config: 4,
        ..l.clone()
    };
    let bt = GradientParams {
        config: 4,
        peaky: 59,
        ..l.clone()
    };
    let res = [
        grad_res("food1", f),
        grad_res("left", l),
        grad_res("right", rt),
        grad_res("top", t),
        grad_res("bottom", bt),
    ];
    run_gradient("AVATARS", 60, 60, 9, &res, 6, 2, 0.5);
}

#[test]
fn gradient_random_walls_and_hills_match_cpp() {
    let w0 = GradientParams {
        habitat: 2,
        plateau: 2.0,
        updatestep: 3,
        count: 3,
        min_size: 4,
        max_size: 9,
        config: 0,
        halo_anchor_x: 15,
        halo_anchor_y: 15,
        halo_inner_radius: 3,
        ..gp()
    };
    let w1 = GradientParams {
        config: 1,
        count: 2,
        ..w0.clone()
    };
    let w2 = GradientParams {
        config: 2,
        count: 2,
        min_size: 20,
        max_size: 25,
        ..w0.clone()
    };
    let h = GradientParams {
        habitat: 1,
        plateau: 3.0,
        updatestep: 2,
        count: 2,
        min_size: 2,
        max_size: 4,
        config: 0,
        halo_anchor_x: 15,
        halo_anchor_y: 15,
        halo_inner_radius: 3,
        ..gp()
    };
    let res = [
        grad_res("w0", w0),
        grad_res("w1", w1),
        grad_res("w2", w2),
        grad_res("hills", h),
    ];
    run_gradient("WALLS", 30, 30, 17, &res, 8, 0, 0.0);
}

#[test]
fn gradient_cone_flows_and_skip_moves_match_cpp() {
    let g = GradientParams {
        height: 4,
        spread: 9,
        plateau: 2.5,
        decay: 1,
        move_a_scaler: 3.2,
        updatestep: 2,
        max_x: 35,
        min_x: 5,
        max_y: 30,
        min_y: 8,
        plateau_inflow: 0.3,
        plateau_outflow: 0.07,
        cone_inflow: 0.2,
        cone_outflow: 0.11,
        gradient_inflow: 0.4,
        floor: 0.05,
        move_speed: 2,
        ..gp()
    };
    let s = GradientParams {
        move_speed: -2,
        cone_inflow: 0.0,
        cone_outflow: 0.0,
        plateau: -1.0,
        height: 3,
        spread: 5,
        max_x: 25,
        min_x: 10,
        max_y: 25,
        min_y: 10,
        init_plat: 2.0,
        ..g.clone()
    };
    run_gradient(
        "CONE",
        40,
        36,
        23,
        &[grad_res("cone", g), grad_res("skip", s)],
        30,
        3,
        0.4,
    );
}

#[test]
fn gradient_halo_speed_and_orbits_match_cpp() {
    let g = GradientParams {
        height: 3,
        spread: 6,
        plateau: 1.5,
        decay: 3,
        move_a_scaler: 3.6,
        updatestep: 1,
        halo: 1,
        halo_inner_radius: 5,
        halo_anchor_x: 25,
        halo_anchor_y: 25,
        halo_width: 14,
        move_speed: 3,
        move_resistance: 2,
        plateau_inflow: 0.2,
        ..gp()
    };
    run_gradient("HALOSPD", 50, 50, 31, &[grad_res("halo", g)], 60, 5, 0.95);
}

#[test]
fn gradient_unclamped_plateau_flows_match_cpp() {
    // Plateau inflow/outflow that stays below the cap, per cell and common,
    // so both fused plateau updates are visible.
    let g = GradientParams {
        height: 3,
        spread: 6,
        plateau: 5.0,
        decay: 1,
        move_a_scaler: 3.3,
        updatestep: 4,
        max_x: 28,
        min_x: 4,
        max_y: 26,
        min_y: 5,
        plateau_inflow: 0.37,
        plateau_outflow: 0.13,
        ..gp()
    };
    let c = GradientParams {
        height: 8,
        spread: 12,
        plateau: 5.0,
        decay: 1,
        peakx: 30,
        peaky: 30,
        max_x: 60,
        min_x: 0,
        max_y: 60,
        min_y: 0,
        plateau_inflow: 0.37,
        plateau_outflow: 0.13,
        is_plateau_common: 1,
        ..gp()
    };
    run_gradient(
        "PLATFLOW",
        60,
        60,
        13,
        &[grad_res("plat", g), grad_res("common", c)],
        25,
        2,
        0.3,
    );
}

#[test]
fn gradient_multi_update_order_matches_cpp() {
    // Several pending spatial updates per call, and one backwards
    // SetSpatialUpdate: resource-major DoUpdates (47f13dadb).
    let a = GradientParams {
        height: 3,
        spread: 8,
        plateau: 2.0,
        decay: 1,
        move_a_scaler: 3.7,
        updatestep: 3,
        max_x: 30,
        min_x: 2,
        max_y: 28,
        min_y: 3,
        plateau_inflow: 0.1,
        ..gp()
    };
    let b = GradientParams {
        move_a_scaler: 3.9,
        updatestep: 2,
        height: 2,
        spread: 5,
        ..a.clone()
    };
    let tag = "JUMP";
    let res = [grad_res("a", a), grad_res("b", b)];
    let mut rng = AvidaRng::new(41);
    let mut env = TraceEnv::new();
    let mut rc = ResourceCount::new(res.len());
    rc.resize_spatial_grids(32, 30);
    for (i, r) in res.iter().enumerate() {
        rc.setup(i, r, &mut rng, &mut env).expect("setup");
    }
    env.has_driver = true;
    for u in [3, 7, 8, 5, 12, 12, 15] {
        env.update = u;
        rc.set_spatial_update(u);
        rc.get_resources(&mut rng, &mut env);
        env.out
            .push(format!("{tag} {u} last={}", rc.spatial_update_state().0));
        for i in 0..res.len() {
            dump_grad(tag, u, &rc, i, &mut env.out);
        }
    }
    env.out
        .push(format!("{tag} RNG {}", rng.get_uint(1_000_000_000)));
    compare(tag, &expected_for(tag, GRADIENT_TXT), &env.out);
}

#[test]
fn gradient_trace_covers_every_scenario() {
    let tags = [
        "REGMOVE", "HALOMOVE", "COMMON", "COMMON2", "HALOGEN", "AVATARS", "WALLS", "CONE",
        "HALOSPD", "PLATFLOW", "JUMP",
    ];
    let covered: usize = tags
        .iter()
        .map(|t| expected_for(t, GRADIENT_TXT).lines().count())
        .sum();
    assert_eq!(covered, GRADIENT_TXT.lines().count());
}

#[test]
fn gradient_actions_match_cpp() {
    // Predatory / damaging / deadly cells, probabilistic resources, variable
    // plateau inflow, SetGradientCount and the per-resource setters.
    let mut rng = AvidaRng::new(5);
    let mut env = TraceEnv::new();
    let mut rc = ResourceCount::new(3);
    rc.resize_spatial_grids(24, 20);
    let p = GradientParams {
        height: 2,
        spread: 3,
        plateau: 1.0,
        decay: 1,
        min_x: 0,
        max_x: 23,
        min_y: 0,
        max_y: 19,
        move_a_scaler: 3.8,
        updatestep: 4,
        damage: 0.25,
        threshold: 0.5,
        death_odds: 0.1,
        hammer: 1,
        ..gp()
    };
    let q = GradientParams {
        height: 2,
        spread: 2,
        plateau: 1.0,
        decay: 1,
        min_x: 2,
        max_x: 20,
        min_y: 2,
        max_y: 17,
        plateau_inflow: 0.05,
        ..gp()
    };
    let z = GradientParams {
        height: 1,
        spread: 1,
        plateau: 1.0,
        min_x: 0,
        max_x: 23,
        min_y: 0,
        max_y: 19,
        ..gp()
    };
    rc.setup(0, &grad_res("pred", p.clone()), &mut rng, &mut env)
        .expect("setup");
    rc.setup(1, &grad_res("prob", q), &mut rng, &mut env)
        .expect("setup");
    rc.setup(2, &grad_res("prob2", z), &mut rng, &mut env)
        .expect("setup");
    env.has_driver = true;
    rc.set_predatory_resource(0, 0.3, 2);
    rc.set_probabilistic_resource(&mut rng, 1, 2.0, 0.1, 0.05, 4.0, 0.0, -1, -1, -1);
    rc.set_probabilistic_resource(&mut rng, 2, 1.5, 0.0, 0.2, 3.0, 2.0, 10, 8, 25);
    for i in 0..3 {
        dump_grad("ACT", -1, &rc, i, &mut env.out);
    }
    for u in 0..25 {
        env.update = u;
        rc.set_spatial_update(u);
        rc.get_resources(&mut rng, &mut env);
        for i in 0..3 {
            dump_grad("ACT", u, &rc, i, &mut env.out);
        }
        let var = |rc: &mut ResourceCount,
                   rng: &mut AvidaRng,
                   mean,
                   var,
                   kind,
                   tag,
                   out: &mut Vec<String>| {
            rc.set_grad_plat_var_inflow(rng, 1, mean, var, kind);
            dump_grad(tag, u, rc, 1, out);
        };
        match u {
            5 => var(&mut rc, &mut rng, 0.05, 0.01, 0, "VAR0", &mut env.out),
            6 => var(&mut rc, &mut rng, 0.05, 0.01, -1, "VARM", &mut env.out),
            7 => var(&mut rc, &mut rng, 0.05, 0.01, 1, "VAR1", &mut env.out),
            8 => var(&mut rc, &mut rng, 0.05, 0.01, 2, "VAR2", &mut env.out),
            9 => var(&mut rc, &mut rng, 0.07, 0.0, 2, "VARZ", &mut env.out),
            10 => {
                let p2 = GradientParams {
                    peakx: 12,
                    peaky: 9,
                    plateau_inflow: 0.2,
                    move_a_scaler: 2.5,
                    threshold: 9.0,
                    damage: 7.0,
                    ..p.clone()
                };
                rc.set_gradient_count(&mut rng, &mut env, 0, &p2);
                dump_grad("RESET", u, &rc, 0, &mut env.out);
            }
            12 => {
                rc.set_gradient_plat_inflow(0, 0.3);
                rc.set_gradient_plat_outflow(0, 0.1);
                rc.set_gradient_cone_inflow(0, 0.2);
                rc.set_gradient_cone_outflow(0, 0.05);
                rc.set_gradient_inflow(0, 0.15);
            }
            14 => rc.set_probabilistic_resource(&mut rng, 1, 3.0, 0.2, 0.0, 2.0, 1.0, 4, 5, 12),
            _ => {}
        }
    }
    let walls = rc.get_wall_cells(0).map_or(-1, |w| w.len() as i32);
    env.out.push(format!(
        "ACT WALL {walls} {} {} {} {}",
        rc.get_min_used_x(1),
        rc.get_min_used_y(1),
        rc.get_max_used_x(1),
        rc.get_max_used_y(1)
    ));
    let cx = rc.get_curr_peak_x(&mut rng, &mut env, 0);
    let cy = rc.get_curr_peak_y(&mut rng, &mut env, 0);
    env.out.push(format!(
        "ACT PEAK {cx} {cy} {} {}",
        rc.get_frozen_peak_x(2),
        rc.get_frozen_peak_y(2)
    ));
    env.out
        .push(format!("ACT RNG {}", rng.get_uint(1_000_000_000)));
    assert!(env.errors.is_empty());
    compare("actions", include_str!("../testdata/actions.txt"), &env.out);
}
