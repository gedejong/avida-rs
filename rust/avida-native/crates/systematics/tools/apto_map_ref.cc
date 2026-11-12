// Build: c++ -std=c++11 -O3 -DNDEBUG -I libs/apto/include tools/apto_map_ref.cc cbuild/libs/apto/lib/libapto.a
// Reference vectors for avida-systematics apto_map.rs: random Set/Remove on
// Apto::Map<int,int> and Apto::Map<Apto::String,int> (DefaultHashBTree);
// prints both maps in iteration order after each operation.
#include "apto/core.h"
#include <cstdio>
static unsigned int s = 4242;
static unsigned int rnd(unsigned int m) { s = s * 1103515245u + 12345u; return (s >> 8) % m; }
int main() {
  Apto::Map<int, int> mi;
  Apto::Map<Apto::String, int> ms;
  for (int step = 0; step < 3000; step++) {
    int op = rnd(3);
    int k = (int)rnd(120) - 20;
    char buf[16]; snprintf(buf, 16, "c%u", rnd(90));
    Apto::String sk(buf);
    int ri, rs;
    if (op < 2) { mi.Set(k, step); ms.Set(sk, step); ri = rs = 1; }
    else { ri = mi.Remove(k); rs = ms.Remove(sk); }
    printf("%d %d %d %s %d %d %d %d:", step, op, k, buf, ri, rs, mi.GetSize(), ms.GetSize());
    for (Apto::Map<int, int>::Iterator it = mi.Begin(); it.Next();) printf(" %d=%d", it.Get()->Value1(), *it.Get()->Value2());
    printf(" |");
    for (Apto::Map<Apto::String, int>::Iterator it = ms.Begin(); it.Next();) printf(" %s=%d", (const char*)it.Get()->Value1(), *it.Get()->Value2());
    printf("\n");
  }
  return 0;
}
