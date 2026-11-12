// Reference-vector generator for the avida-resources crate.
//
// tools/build_ref.sh compiles this with the pre-Rust reference sources (git
// 47f13dadb) of cResourceCount, cSpatialResCount, cSpatialCountElem and
// cGradientCount, small stub headers (tools/stub) and the real Apto RNG,
// using the reference build's release flags, and stores the scenario traces
// in testdata/. Doubles are printed as 16 hex digits of their bit pattern so
// the Rust tests (src/reference_tests.rs) compare bit for bit.

#include <cstdio>
#include <cstring>
#include <stdint.h>
#include <vector>

#define private public
#define protected public
#include "cResourceCount.h"
#include "cGradientCount.h"
#include "cWorld.h"
#undef private
#undef protected
#include "apto/rng/AvidaRNG.h"

static unsigned long long B(double x) { uint64_t u; memcpy(&u, &x, 8); return (unsigned long long)u; }

struct Grad {
  int peakx, peaky, height, spread; double plateau; int decay;
  int max_x, min_x, max_y, min_y; double move_a_scaler; int updatestep;
  int halo, halo_inner_radius, halo_width, halo_anchor_x, halo_anchor_y, move_speed, move_resistance;
  double plateau_inflow, plateau_outflow, cone_inflow, cone_outflow, gradient_inflow;
  int is_plateau_common; double floor; int habitat, min_size, max_size, config, count;
  double resistance, damage, death_odds; int path, hammer; double init_plat, threshold; int refuge;
  Grad() : peakx(-1), peaky(-1), height(0), spread(0), plateau(-1.0), decay(1),
    max_x(0), min_x(0), max_y(0), min_y(0), move_a_scaler(1.0), updatestep(1),
    halo(0), halo_inner_radius(0), halo_width(0), halo_anchor_x(0), halo_anchor_y(0), move_speed(1), move_resistance(0),
    plateau_inflow(0), plateau_outflow(0), cone_inflow(0), cone_outflow(0), gradient_inflow(0),
    is_plateau_common(0), floor(0), habitat(0), min_size(1), max_size(1), config(0), count(1),
    resistance(1.0), damage(0), death_odds(0), path(0), hammer(0), init_plat(-1.0), threshold(1.0), refuge(0) {}
};

struct Res {
  const char* name; double initial, inflow, decay; int geometry;
  double xd, xg, yd, yg; int ix1, ix2, iy1, iy2, ox1, ox2, oy1, oy2;
  Apto::Array<cCellResource> cells; Apto::Array<int> ids; int verbosity; Grad g; bool isgrad;
  // cResource constructor defaults at 47f13dadb (inflowX1 is value-initialised to 0).
  Res(const char* n) : name(n), initial(0), inflow(0), decay(1.0), geometry(nGeometry::GLOBAL),
    xd(1.0), xg(0), yd(1.0), yg(0), ix1(0), ix2(-99), iy1(-99), iy2(-99), ox1(-99), ox2(-99), oy1(-99), oy2(-99),
    verbosity(0), isgrad(false) {}
};

// Mirrors cEnvironment::LoadGradientResource defaults + cPopulation's decay = 1 - outflow.
static Res GradRes(const char* n, const Grad& g) {
  Res r(n);
  r.geometry = nGeometry::GRID; r.initial = 0.0; r.decay = 1.0 - 1.0;
  r.xd = 0.0; r.yd = 0.0; r.xg = 0.0; r.yg = 0.0; r.g = g; r.isgrad = true;
  return r;
}

static void Setup(cResourceCount& rc, cWorld* w, int idx, Res& r) {
  const Grad& g = r.g;
  rc.Setup(w, idx, r.name, r.initial, r.inflow, r.decay, r.geometry, r.xd, r.xg, r.yd, r.yg,
           r.ix1, r.ix2, r.iy1, r.iy2, r.ox1, r.ox2, r.oy1, r.oy2, &r.cells, &r.ids, r.verbosity,
           0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
           g.peakx, g.peaky, g.height, g.spread, g.plateau, g.decay,
           g.max_x, g.min_x, g.max_y, g.min_y, g.move_a_scaler, g.updatestep,
           g.halo, g.halo_inner_radius, g.halo_width, g.halo_anchor_x, g.halo_anchor_y, g.move_speed, g.move_resistance,
           g.plateau_inflow, g.plateau_outflow, g.cone_inflow, g.cone_outflow, g.gradient_inflow, g.is_plateau_common,
           g.floor, g.habitat, g.min_size, g.max_size, g.config, g.count, g.resistance, g.damage, g.death_odds,
           g.path, g.hammer, g.init_plat, g.threshold, g.refuge, r.isgrad);
}

static void SetGradientCount(cResourceCount& rc, cAvidaContext& ctx, cWorld* w, int id, const Grad& g) {
  rc.SetGradientCount(ctx, w, id, g.peakx, g.peaky, g.height, g.spread, g.plateau, g.decay,
                      g.max_x, g.min_x, g.max_y, g.min_y, g.move_a_scaler, g.updatestep,
                      g.halo, g.halo_inner_radius, g.halo_width, g.halo_anchor_x, g.halo_anchor_y, g.move_speed, g.move_resistance,
                      g.plateau_inflow, g.plateau_outflow, g.cone_inflow, g.cone_outflow, g.gradient_inflow, g.is_plateau_common,
                      g.floor, g.habitat, g.min_size, g.max_size, g.config, g.count, g.resistance, g.damage, g.death_odds,
                      g.path, g.hammer, g.init_plat, g.threshold, g.refuge);
}

static uint64_t Fnv(uint64_t h, uint64_t v) {
  for (int i = 0; i < 8; i++) { h ^= (v >> (8 * i)) & 0xff; h *= 1099511628211ULL; }
  return h;
}

static uint64_t GridHash(cSpatialResCount* s) {
  uint64_t h = 14695981039346656037ULL;
  for (int i = 0; i < s->GetSize(); i++) h = Fnv(h, B(s->Element(i).GetAmount()));
  for (int i = 0; i < s->GetSize(); i++) h = Fnv(h, B(s->Element(i).delta));
  return h;
}

static void DumpGrad(const char* tag, int u, cResourceCount& rc, int id) {
  cSpatialResCount* s = rc.spatial_resource_count[id];
  cGradientCount* g = dynamic_cast<cGradientCount*>(s);
  uint64_t ph = 14695981039346656037ULL;
  for (int i = 0; i < g->m_plateau_array.GetSize(); i++) ph = Fnv(ph, B(g->m_plateau_array[i]));
  for (int i = 0; i < g->m_plateau_cell_IDs.GetSize(); i++) ph = Fnv(ph, (uint64_t)(int64_t)g->m_plateau_cell_IDs[i]);
  for (int i = 0; i < g->m_wall_cells.GetSize(); i++) ph = Fnv(ph, (uint64_t)(int64_t)g->m_wall_cells[i]);
  for (int i = 0; i < g->m_prob_res_cells.GetSize(); i++) ph = Fnv(ph, (uint64_t)(int64_t)g->m_prob_res_cells[i]);
  printf("%s %d r%d peak=%d,%d curr=%d,%d ys=%016llx sign=%d,%d cnt=%d,%d,%d,%d mod=%d ch=%016llx cp=%016llx loss=%016llx pin=%016llx used=%d,%d,%d,%d grid=%016llx int=%016llx\n",
         tag, u, id, g->m_peakx, g->m_peaky, s->GetCurrPeakX(), s->GetCurrPeakY(), B(g->m_move_y_scaler),
         g->m_movesignx, g->m_movesigny, g->m_counter, g->m_move_counter, g->m_skip_counter, g->m_topo_counter,
         s->GetModified() ? 1 : 0, B(g->m_current_height), B(g->m_common_plat_height), B(g->m_ave_plat_cell_loss),
         B(g->m_plateau_inflow), g->m_min_usedx, g->m_min_usedy, g->m_max_usedx, g->m_max_usedy,
         (unsigned long long)GridHash(s), (unsigned long long)ph);
  if (g->m_halo && g->m_move_a_scaler > 1) printf("%s %d r%d halo dir=%d ch=%d\n", tag, u, id, g->m_halo_dir, g->m_changling);
}

// ---------------------------------------------------------------- global
static void ScenarioGlobal() {
  cWorld world; Apto::RNG::AvidaRNG rng(1);
  world.m_ctx = new cAvidaContext(NULL, rng);
  cAvidaContext& ctx = *world.m_ctx;
  cResourceCount rc(2);
  rc.ResizeSpatialGrids(5, 5);
  Res a("food"); a.initial = 0.0; a.inflow = 100.0; a.decay = 1.0 - 0.01; a.verbosity = 2;
  Res b("part"); b.initial = 3.5; b.inflow = 1.25; b.decay = 1.0 - 0.3; b.geometry = nGeometry::PARTIAL;
  b.ids.Push(3); b.ids.Push(7); b.ids.Push(11);
  Setup(rc, &world, 0, a);
  Setup(rc, &world, 1, b);
  for (int k = 0; k <= 100; k++) printf("P %d %016llx %016llx %016llx %016llx\n", k, B(rc.decay_precalc(0, k)), B(rc.inflow_precalc(0, k)), B(rc.decay_precalc(1, k)), B(rc.inflow_precalc(1, k)));
  const double steps[] = {0.37, 0.5, 1.0, 0.0001, 0.00005, 2.5, 1.0 / 3.0, 0.123456789, 0.99999, 0.00009999, 7.0, 0.25, 0.0, 1e-7, 0.0300003};
  for (unsigned i = 0; i < sizeof(steps) / sizeof(steps[0]); i++) {
    rc.Update(steps[i]);
    const Apto::Array<double>& r = rc.GetResources(ctx);
    printf("G %u %016llx %016llx %016llx\n", i, B(r[0]), B(r[1]), B(rc.update_time));
  }
  rc.SetInflow("food", 50.0);
  rc.SetDecay("food", 0.95);
  for (int k = 0; k <= 100; k += 10) printf("P2 %d %016llx %016llx\n", k, B(rc.decay_precalc(0, k)), B(rc.inflow_precalc(0, k)));
  rc.Update(0.75);
  Apto::Array<double> ch(2); ch[0] = -3.0; ch[1] = 0.5;
  rc.Modify(ctx, ch);
  printf("M %016llx %016llx %016llx\n", B(rc.ReadResources()[0]), B(rc.ReadResources()[1]), B(rc.update_time));
  rc.Update(0.6);
  rc.Modify(ctx, 1, -0.125);
  rc.ModifyCell(ctx, ch, 4);
  printf("M %016llx %016llx\n", B(rc.ReadResources()[0]), B(rc.ReadResources()[1]));
  rc.Set(ctx, 0, 12.5);
  rc.Update(1.5);
  printf("S %016llx %016llx\n", B(rc.Get(ctx, 0)), B(rc.Get(ctx, 1)));
  const Apto::Array<double>& cr = rc.GetCellResources(9, ctx);
  printf("C %016llx %016llx\n", B(cr[0]), B(cr[1]));
  rc.ReinitializeResources(ctx, 0.25);
  printf("R %016llx %016llx\n", B(rc.ReadResources()[0]), B(rc.ReadResources()[1]));
  printf("ID %d %d %d %d\n", rc.GetResourceCountID("part"), rc.GetResourceByName("food"), rc.GetResourceByName("nope"), rc.GetResourceCountID("nope"));
  printf("I %016llx %016llx\n", B(rc.GetInflow("food")), B(rc.GetDecay("part")));
}

// ---------------------------------------------------------------- spatial
static void DumpSpatial(const char* tag, int u, cResourceCount& rc, cAvidaContext& ctx) {
  const Apto::Array<Apto::Array<double> >& sp = rc.GetSpatialRes(ctx);
  for (int i = 0; i < sp.GetSize(); i++) {
    printf("%s %d r%d", tag, u, i);
    for (int j = 0; j < sp[i].GetSize(); j++) printf(" %016llx", B(sp[i][j]));
    printf(" d");
    for (int j = 0; j < rc.spatial_resource_count[i]->GetSize(); j++) printf(" %016llx", B(rc.spatial_resource_count[i]->Element(j).delta));
    printf("\n");
  }
}

static void ScenarioSpatial() {
  cWorld world; Apto::RNG::AvidaRNG rng(1);
  world.m_ctx = new cAvidaContext(NULL, rng);
  cAvidaContext& ctx = *world.m_ctx;
  cResourceCount rc(4);
  rc.ResizeSpatialGrids(5, 5);
  // Pure diffusion on a 5x5 torus from one seeded cell.
  Res d("diffuse"); d.geometry = nGeometry::TORUS; d.initial = 0.0; d.inflow = 0.0; d.decay = 1.0;
  d.cells.Push(cCellResource(12, 100.0, 0.0, 0.0));
  // Torus with gravity, inflow/outflow boxes and cell inflow/outflow.
  Res t("torus"); t.geometry = nGeometry::TORUS; t.initial = 25.0; t.inflow = 5.0; t.decay = 1.0 - 0.1;
  t.xd = 1.0; t.xg = 0.2; t.yd = 0.5; t.yg = -0.1; t.ix1 = 1; t.ix2 = 2; t.iy1 = 1; t.iy2 = 2;
  t.ox1 = 3; t.ox2 = 5; t.oy1 = 0; t.oy2 = 1; t.verbosity = 2;
  t.cells.Push(cCellResource(12, 3.0, 0.5, 0.2)); t.cells.Push(cCellResource(24, 1.0, 0.0, 0.05));
  t.cells.Push(cCellResource(25, 9.0, 1.0, 0.5)); t.cells.Push(cCellResource(-3, 9.0, 1.0, 0.5));
  // Bounded grid with the same flows.
  Res g("grid"); g.geometry = nGeometry::GRID; g.initial = 25.0; g.inflow = 5.0; g.decay = 1.0 - 0.1;
  g.xd = 1.0; g.xg = -0.3; g.yd = 0.75; g.yg = 0.4; g.ix1 = 0; g.ix2 = 0; g.iy1 = 4; g.iy2 = 4;
  g.ox1 = 2; g.ox2 = 3; g.oy1 = 2; g.oy2 = 3;
  Res gl("glob"); gl.initial = 10.0; gl.inflow = 2.0; gl.decay = 0.5;
  Setup(rc, &world, 0, d);
  Setup(rc, &world, 1, t);
  Setup(rc, &world, 2, g);
  Setup(rc, &world, 3, gl);
  for (int i = 0; i < 25; i++) {
    cSpatialCountElem& e = rc.spatial_resource_count[2]->Element(i);
    printf("PTR %d", i);
    for (int k = 0; k < 8; k++) printf(" %d,%d,%d,%016llx", e.GetElemPtr(k), e.GetPtrXdist(k), e.GetPtrYdist(k), B(e.GetPtrDist(k)));
    printf("\n");
  }
  DumpSpatial("SP", 0, rc, ctx);
  for (int u = 1; u <= 8; u++) {
    rc.SetSpatialUpdate(u);
    rc.Update(1.0);
    DumpSpatial("SP", u, rc, ctx);
    if (u == 3) {
      Apto::Array<double> ch(4); ch[0] = -0.5; ch[1] = -1.0; ch[2] = 2.0; ch[3] = -1.0;
      rc.ModifyCell(ctx, ch, 7);
      DumpSpatial("MC", u, rc, ctx);
      printf("MOD %d %d %d\n", rc.spatial_resource_count[0]->GetModified() ? 1 : 0, rc.spatial_resource_count[1]->GetModified() ? 1 : 0, rc.spatial_resource_count[2]->GetModified() ? 1 : 0);
    }
    printf("SUM %d %016llx %016llx %016llx %016llx\n", u, B(rc.Get(ctx, 0)), B(rc.Get(ctx, 1)), B(rc.Get(ctx, 2)), B(rc.Get(ctx, 3)));
  }
  const Apto::Array<double>& oob = rc.GetCellResources(30, ctx);
  printf("OOB %016llx %016llx %016llx %016llx\n", B(oob[0]), B(oob[1]), B(oob[2]), B(oob[3]));
  // Two pending spatial updates in one DoUpdates.
  rc.SetSpatialUpdate(10);
  DumpSpatial("SP", 10, rc, ctx);
  rc.Set(ctx, 1, 50.0);
  DumpSpatial("SET", 10, rc, ctx);
  Apto::Array<double> cell(4); cell[0] = 1.5; cell[1] = 2.5; cell[2] = 3.5; cell[3] = 4.5;
  rc.SetCellResources(6, cell);
  DumpSpatial("SETCELL", 10, rc, ctx);
  rc.ReinitializeResources(ctx, 0.5);
  DumpSpatial("REINIT", 10, rc, ctx);
  rc.ReinitializeResources(ctx, 0.0);
  DumpSpatial("REINIT0", 10, rc, ctx);
}

// ---------------------------------------------------------------- gradient
struct Bite { int every; double frac; };

static void RunGradient(const char* tag, int wx, int wy, int seed, std::vector<Res>& res, int updates, Bite bite) {
  cWorld world; Apto::RNG::AvidaRNG rng(seed);
  world.m_ctx = new cAvidaContext(NULL, rng);
  cResourceCount rc(res.size());
  rc.ResizeSpatialGrids(wx, wy);
  for (unsigned i = 0; i < res.size(); i++) Setup(rc, &world, i, res[i]);
  for (unsigned i = 0; i < res.size(); i++) if (res[i].isgrad) DumpGrad(tag, -1, rc, i);
  cAvidaContext ctx(&world.GetDriver(), rng);
  for (int u = 0; u < updates; u++) {
    world.m_stats.m_update = u;
    rc.SetSpatialUpdate(u);
    rc.GetResources(ctx);
    for (unsigned i = 0; i < res.size(); i++) if (res[i].isgrad) DumpGrad(tag, u, rc, i);
    if (bite.every > 0 && u % bite.every == 1) {
      for (unsigned i = 0; i < res.size(); i++) {
        int px = rc.GetFrozenPeakX(ctx, i), py = rc.GetFrozenPeakY(ctx, i);
        int cells[2] = { py * wx + px, py * wx + px + 1 };
        for (int c = 0; c < 2; c++) {
          if (cells[c] < 0 || cells[c] >= wx * wy) continue;
          Apto::Array<double> ch(res.size()); ch.SetAll(0.0);
          ch[i] = -rc.GetFrozenCellResVal(ctx, cells[c], i) * bite.frac;
          rc.ModifyCell(ctx, ch, cells[c]);
        }
      }
    }
  }
  for (unsigned i = 0; i < world.GetDriver().Feedback().errors.size(); i++) printf("%s ERR %s\n", tag, world.GetDriver().Feedback().errors[i].c_str());
  printf("%s RNG %u\n", tag, rng.GetUInt(1000000000));
}

// Several pending spatial updates per DoUpdates call, and one call that sets
// the spatial update backwards: checks the resource-major update order.
static void RunJump(const char* tag, int wx, int wy, int seed, std::vector<Res>& res) {
  cWorld world; Apto::RNG::AvidaRNG rng(seed);
  world.m_ctx = new cAvidaContext(NULL, rng);
  cResourceCount rc(res.size());
  rc.ResizeSpatialGrids(wx, wy);
  for (unsigned i = 0; i < res.size(); i++) Setup(rc, &world, i, res[i]);
  cAvidaContext ctx(&world.GetDriver(), rng);
  const int sched[] = { 3, 7, 8, 5, 12, 12, 15 };
  for (unsigned k = 0; k < sizeof(sched) / sizeof(sched[0]); k++) {
    int u = sched[k];
    world.m_stats.m_update = u;
    rc.SetSpatialUpdate(u);
    rc.GetResources(ctx);
    printf("%s %d last=%d\n", tag, u, rc.m_last_updated);
    for (unsigned i = 0; i < res.size(); i++) if (res[i].isgrad) DumpGrad(tag, u, rc, i);
  }
  printf("%s RNG %u\n", tag, rng.GetUInt(1000000000));
}

static void ScenarioGradients() {
  { // gradient_regular_move/config/environment.cfg
    Grad g; g.height = 20; g.spread = 120; g.plateau = 1; g.decay = 1; g.move_a_scaler = 3.8;
    g.max_x = 80; g.min_x = 40; g.max_y = 80; g.min_y = 40; g.plateau_inflow = 0.01;
    std::vector<Res> r; r.push_back(GradRes("food0", g));
    RunGradient("REGMOVE", 120, 120, 9, r, 40, Bite{3, 0.5});
  }
  { // gradient_halo_move
    Grad g; g.height = 20; g.spread = 120; g.plateau = 1; g.decay = 1; g.move_a_scaler = 3.8;
    g.halo = 1; g.halo_inner_radius = -10; g.halo_anchor_x = 59; g.halo_anchor_y = 59; g.halo_width = 45;
    g.plateau_inflow = 0.5; g.updatestep = 5;
    std::vector<Res> r; r.push_back(GradRes("food0", g));
    RunGradient("HALOMOVE", 120, 120, 9, r, 40, Bite{4, 0.75});
  }
  { // gradient_inflow_common
    Grad g; g.height = 20; g.spread = 120; g.plateau = 1; g.decay = 1; g.move_a_scaler = 3.8;
    g.halo = 1; g.halo_inner_radius = -20; g.halo_anchor_x = 59; g.halo_anchor_y = 59; g.halo_width = 40;
    g.plateau_inflow = 0.01; g.is_plateau_common = 1;
    std::vector<Res> r; r.push_back(GradRes("food0", g));
    RunGradient("COMMON", 120, 120, 9, r, 30, Bite{2, 0.5});
    // Same with plateau outflow, so the fused common-height update matters.
    r[0].g.plateau_outflow = 0.03; r[0].g.plateau_inflow = 0.07;
    RunGradient("COMMON2", 120, 120, 9, r, 30, Bite{2, 0.5});
  }
  { // gradient_halo_generate (decay > 1: bitten peaks rot and regenerate)
    Grad g; g.height = 20; g.spread = 120; g.plateau = 1; g.decay = 5; g.move_a_scaler = 1; g.updatestep = 200;
    g.halo = 1; g.halo_inner_radius = -10; g.halo_anchor_x = 59; g.halo_anchor_y = 59; g.halo_width = 40;
    std::vector<Res> r; r.push_back(GradRes("food0", g));
    RunGradient("HALOGEN", 120, 120, 9, r, 20, Bite{1, 0.9});
  }
  { // avatars: walls (habitat=2, config 3/4) next to a static food cone (60x60)
    Grad f; f.height = 21; f.spread = 20; f.plateau = 1; f.decay = 1; f.move_a_scaler = 1; f.updatestep = 10000;
    f.peakx = 30; f.peaky = 30; f.min_x = 0; f.max_x = 60; f.min_y = 0; f.max_y = 60; f.plateau_inflow = 0.002; f.init_plat = 1;
    Grad l; l.habitat = 2; l.plateau = 1.1; l.updatestep = 10000000; l.count = 1; l.max_size = 60; l.min_size = 60; l.config = 3; l.peakx = 0; l.peaky = 0;
    Grad rt = l; rt.peakx = 59;
    Grad t = l; t.config = 4;
    Grad bt = l; bt.config = 4; bt.peaky = 59;
    std::vector<Res> r; r.push_back(GradRes("food1", f)); r.push_back(GradRes("left", l)); r.push_back(GradRes("right", rt));
    r.push_back(GradRes("top", t)); r.push_back(GradRes("bottom", bt));
    RunGradient("AVATARS", 60, 60, 9, r, 6, Bite{2, 0.5});
  }
  { // random walls (config 0/1/2) and hills
    Grad w0; w0.habitat = 2; w0.plateau = 2.0; w0.updatestep = 3; w0.count = 3; w0.min_size = 4; w0.max_size = 9; w0.config = 0;
    w0.halo_anchor_x = 15; w0.halo_anchor_y = 15; w0.halo_inner_radius = 3;
    Grad w1 = w0; w1.config = 1; w1.count = 2;
    Grad w2 = w0; w2.config = 2; w2.count = 2; w2.min_size = 20; w2.max_size = 25;
    Grad h; h.habitat = 1; h.plateau = 3.0; h.updatestep = 2; h.count = 2; h.min_size = 2; h.max_size = 4; h.config = 0;
    h.halo_anchor_x = 15; h.halo_anchor_y = 15; h.halo_inner_radius = 3;
    std::vector<Res> r; r.push_back(GradRes("w0", w0)); r.push_back(GradRes("w1", w1)); r.push_back(GradRes("w2", w2)); r.push_back(GradRes("hills", h));
    RunGradient("WALLS", 30, 30, 17, r, 8, Bite{0, 0});
  }
  { // cone inflow/outflow, gradient inflow, floor, plateau outflow, move_speed and skip moves
    Grad g; g.height = 4; g.spread = 9; g.plateau = 2.5; g.decay = 1; g.move_a_scaler = 3.2; g.updatestep = 2;
    g.max_x = 35; g.min_x = 5; g.max_y = 30; g.min_y = 8; g.plateau_inflow = 0.3; g.plateau_outflow = 0.07;
    g.cone_inflow = 0.2; g.cone_outflow = 0.11; g.gradient_inflow = 0.4; g.floor = 0.05; g.move_speed = 2;
    Grad s = g; s.move_speed = -2; s.cone_inflow = 0; s.cone_outflow = 0; s.plateau = -1; s.height = 3; s.spread = 5;
    s.max_x = 25; s.min_x = 10; s.max_y = 25; s.min_y = 10; s.init_plat = 2.0;
    std::vector<Res> r; r.push_back(GradRes("cone", g)); r.push_back(GradRes("skip", s));
    RunGradient("CONE", 40, 36, 23, r, 30, Bite{3, 0.4});
  }
  { // halo with move speed > 1, move resistance, orbit changes (wide halo)
    Grad g; g.height = 3; g.spread = 6; g.plateau = 1.5; g.decay = 3; g.move_a_scaler = 3.6; g.updatestep = 1;
    g.halo = 1; g.halo_inner_radius = 5; g.halo_anchor_x = 25; g.halo_anchor_y = 25; g.halo_width = 14;
    g.move_speed = 3; g.move_resistance = 2; g.plateau_inflow = 0.2;
    std::vector<Res> r; r.push_back(GradRes("halo", g));
    RunGradient("HALOSPD", 50, 50, 31, r, 60, Bite{5, 0.95});
  }
  { // unclamped plateau inflow/outflow, per cell and common, so both fused plateau updates matter
    Grad g; g.height = 3; g.spread = 6; g.plateau = 5.0; g.decay = 1; g.move_a_scaler = 3.3; g.updatestep = 4;
    g.max_x = 28; g.min_x = 4; g.max_y = 26; g.min_y = 5; g.plateau_inflow = 0.37; g.plateau_outflow = 0.13;
    Grad c; c.height = 8; c.spread = 12; c.plateau = 5.0; c.decay = 1; c.peakx = 30; c.peaky = 30;
    c.max_x = 60; c.min_x = 0; c.max_y = 60; c.min_y = 0; c.plateau_inflow = 0.37; c.plateau_outflow = 0.13; c.is_plateau_common = 1;
    std::vector<Res> r; r.push_back(GradRes("plat", g)); r.push_back(GradRes("common", c));
    RunGradient("PLATFLOW", 60, 60, 13, r, 25, Bite{2, 0.3});
  }
  { // two moving peaks, several pending updates per call
    Grad a; a.height = 3; a.spread = 8; a.plateau = 2; a.decay = 1; a.move_a_scaler = 3.7; a.updatestep = 3;
    a.max_x = 30; a.min_x = 2; a.max_y = 28; a.min_y = 3; a.plateau_inflow = 0.1;
    Grad b = a; b.move_a_scaler = 3.9; b.updatestep = 2; b.height = 2; b.spread = 5;
    std::vector<Res> r; r.push_back(GradRes("a", a)); r.push_back(GradRes("b", b));
    RunJump("JUMP", 32, 30, 41, r);
  }
}

static void ScenarioGradientActions() {
  // Predatory / damaging / deadly fields, probabilistic resources, variable plateau inflow, SetGradientCount.
  cWorld world; Apto::RNG::AvidaRNG rng(5);
  world.m_ctx = new cAvidaContext(NULL, rng);
  cResourceCount rc(3);
  rc.ResizeSpatialGrids(24, 20);
  Grad p; p.height = 2; p.spread = 3; p.plateau = 1; p.decay = 1; p.min_x = 0; p.max_x = 23; p.min_y = 0; p.max_y = 19;
  p.move_a_scaler = 3.8; p.updatestep = 4; p.damage = 0.25; p.threshold = 0.5; p.death_odds = 0.1; p.hammer = 1;
  Grad q; q.height = 2; q.spread = 2; q.plateau = 1.0; q.decay = 1;
  q.min_x = 2; q.max_x = 20; q.min_y = 2; q.max_y = 17; q.plateau_inflow = 0.05;
  Grad z; z.height = 1; z.spread = 1; z.plateau = 1; z.min_x = 0; z.max_x = 23; z.min_y = 0; z.max_y = 19;
  Res r0 = GradRes("pred", p); Res r1 = GradRes("prob", q); Res r2 = GradRes("prob2", z);
  Setup(rc, &world, 0, r0); Setup(rc, &world, 1, r1); Setup(rc, &world, 2, r2);
  cAvidaContext ctx(&world.GetDriver(), rng);
  rc.SetPredatoryResource(0, 0.3, 2);
  rc.SetProbabilisticResource(ctx, 1, 2.0, 0.1, 0.05, 4.0, 0.0, -1, -1, -1);
  rc.SetProbabilisticResource(ctx, 2, 1.5, 0.0, 0.2, 3.0, 2.0, 10, 8, 25);
  for (int i = 0; i < 3; i++) DumpGrad("ACT", -1, rc, i);
  for (int u = 0; u < 25; u++) {
    world.m_stats.m_update = u;
    rc.SetSpatialUpdate(u);
    rc.GetResources(ctx);
    for (int i = 0; i < 3; i++) DumpGrad("ACT", u, rc, i);
    if (u == 5) { rc.SetGradPlatVarInflow(ctx, 1, 0.05, 0.01, 0); DumpGrad("VAR0", u, rc, 1); }
    if (u == 6) { rc.SetGradPlatVarInflow(ctx, 1, 0.05, 0.01, -1); DumpGrad("VARM", u, rc, 1); }
    if (u == 7) { rc.SetGradPlatVarInflow(ctx, 1, 0.05, 0.01, 1); DumpGrad("VAR1", u, rc, 1); }
    if (u == 8) { rc.SetGradPlatVarInflow(ctx, 1, 0.05, 0.01, 2); DumpGrad("VAR2", u, rc, 1); }
    if (u == 9) { rc.SetGradPlatVarInflow(ctx, 1, 0.07, 0.0, 2); DumpGrad("VARZ", u, rc, 1); }
    if (u == 10) {
      Grad p2 = p; p2.peakx = 12; p2.peaky = 9; p2.plateau_inflow = 0.2; p2.move_a_scaler = 2.5; p2.threshold = 9.0; p2.damage = 7.0;
      SetGradientCount(rc, ctx, &world, 0, p2);
      DumpGrad("RESET", u, rc, 0);
    }
    if (u == 12) { rc.SetGradientPlatInflow(0, 0.3); rc.SetGradientPlatOutflow(0, 0.1); rc.SetGradientConeInflow(0, 0.2); rc.SetGradientConeOutflow(0, 0.05); rc.SetGradientInflow(0, 0.15); }
    if (u == 14) rc.SetProbabilisticResource(ctx, 1, 3.0, 0.2, 0.0, 2.0, 1.0, 4, 5, 12);
  }
  printf("ACT WALL %d %d %d %d %d\n", rc.GetWallCells(0) ? rc.GetWallCells(0)->GetSize() : -1, rc.GetMinUsedX(1), rc.GetMinUsedY(1), rc.GetMaxUsedX(1), rc.GetMaxUsedY(1));
  printf("ACT PEAK %d %d %d %d\n", rc.GetCurrPeakX(ctx, 0), rc.GetCurrPeakY(ctx, 0), rc.GetFrozenPeakX(ctx, 2), rc.GetFrozenPeakY(ctx, 2));
  printf("ACT RNG %u\n", rng.GetUInt(1000000000));
}

int main(int argc, char** argv) {
  const char* which = argc > 1 ? argv[1] : "all";
  if (!strcmp(which, "global")) ScenarioGlobal();
  else if (!strcmp(which, "spatial")) ScenarioSpatial();
  else if (!strcmp(which, "gradient")) ScenarioGradients();
  else if (!strcmp(which, "actions")) ScenarioGradientActions();
  return 0;
}
