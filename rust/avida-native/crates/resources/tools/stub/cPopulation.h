#ifndef cPopulation_h
#define cPopulation_h
#include <cstdio>
#include <cstring>
#include <stdint.h>
#include "cAvidaContext.h"
static inline unsigned long long Bits(double x) { uint64_t u; memcpy(&u, &x, 8); return (unsigned long long)u; }
// Records each call and draws one RNG value so that interleaving with the
// gradient's own draws is checked by the Rust tests.
class cPopulation {
public:
  void ExecutePredatoryResource(cAvidaContext& ctx, int cell_id, double odds, int juvs, bool hammer) {
    unsigned r = ctx.GetRandom().GetUInt(1000);
    printf("CALL pred %d %016llx %d %d %u\n", cell_id, Bits(odds), juvs, hammer ? 1 : 0, r);
  }
  void ExecuteDamagingResource(cAvidaContext& ctx, int cell_id, double damage, bool hammer) {
    unsigned r = ctx.GetRandom().GetUInt(1000);
    printf("CALL damage %d %016llx %d %u\n", cell_id, Bits(damage), hammer ? 1 : 0, r);
  }
  void ExecuteDeadlyResource(cAvidaContext& ctx, int cell_id, double odds, bool hammer) {
    unsigned r = ctx.GetRandom().GetUInt(1000);
    printf("CALL deadly %d %016llx %d %u\n", cell_id, Bits(odds), hammer ? 1 : 0, r);
  }
};
#endif
