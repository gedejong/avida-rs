// Reference vectors for avida-systematics sparse_list.rs: random operations on
// Apto::List<long, SparseVector> with entry handles; prints the list after each op.
#include "apto/core/List.h"
#include <cstdio>
typedef Apto::List<long, Apto::SparseVector> L;
static unsigned int s = 12345;
static unsigned int rnd(unsigned int m) { s = s * 1103515245u + 12345u; return (s >> 8) % m; }
int main() {
  const int N = 70;
  L l;
  L::EntryHandle* h[N] = {0};
  bool present[N] = {false};
  for (int step = 0; step < 4000; step++) {
    int op = rnd(5); int v = rnd(N);
    switch (op) {
      case 0: if (!present[v]) { l.Push(v, &h[v]); present[v] = true; } break;
      case 1: if (!present[v]) { l.PushRear(v, &h[v]); present[v] = true; } break;
      case 2: if (!present[v]) { l.Push(v); present[v] = true; } break;
      case 3: if (present[v] && h[v] && h[v]->IsValid()) { h[v]->Remove(); present[v] = false; } break;
      case 4: if (present[v]) { l.Remove(v); present[v] = false; } break;
    }
    printf("%d %d %d:", step, op, v);
    L::Iterator it = l.Begin(); while (it.Next()) printf(" %ld", *it.Get());
    printf("\n");
  }
  return 0;
}
