#!/bin/sh
# Regenerates crates/resources/testdata/*.txt from the reference C++
# (git 47f13dadb, 2025-01-27, the last pre-Rust commit) compiled with the reference
# build's release flags (Apple clang, arm64, default -ffp-contract=on).
#
# Run from the repository root after the C++ build (needs cbuild/lib/libapto.a):
#   sh rust/avida-native/crates/resources/tools/build_ref.sh [scratch-dir]
set -e
OUT=${1:-${TMPDIR:-/tmp}/avida-resources-ref}
T=rust/avida-native/crates/resources/tools
D=rust/avida-native/crates/resources/testdata
mkdir -p "$OUT/src"
for f in cResourceCount.h cResourceCount.cc cSpatialResCount.h cSpatialResCount.cc \
         cSpatialCountElem.h cSpatialCountElem.cc cGradientCount.h cGradientCount.cc nGeometry.h; do
  git show 47f13dadb:avida-core/source/main/$f > "$OUT/src/$f"
done
# The reference binary (cbuild/work/avida) computes the update-time remainder
# in the Rust helper avd_rc_remainder_update_time (commit 0f87515bc), a
# multiply and a subtract; clang would fuse the pre-Rust statement on arm64.
# Keep the product in a volatile so the harness matches the binary.
sed -i.bak 's|^  update_time -=  num_steps \* UPDATE_STEP;|  { volatile double avd_prod = num_steps * UPDATE_STEP; update_time -= avd_prod; }|' "$OUT/src/cResourceCount.cc"
grep -q 'volatile double avd_prod' "$OUT/src/cResourceCount.cc"
# Likewise the inflow precalc tables: the binary fills them in the Rust
# helpers avd_rc_fill_precalc_tables / avd_rc_fill_inflow_precalc_table, whose
# avd_rc_inflow_precalc_next is an fmul and an fadd (otool -tv), not an fmadd.
sed -i.bak \
  -e 's|^    inflow_precalc(res_index, i) = inflow_precalc(res_index, i-1) \* step_decay + step_inflow;|    { volatile double avd_ip = inflow_precalc(res_index, i-1) * step_decay; inflow_precalc(res_index, i) = avd_ip + step_inflow; }|' \
  -e 's|^    inflow_precalc(id, i) = inflow_precalc(id, i-1) \* step_decay + step_inflow;|    { volatile double avd_ip = inflow_precalc(id, i-1) * step_decay; inflow_precalc(id, i) = avd_ip + step_inflow; }|' \
  "$OUT/src/cResourceCount.cc"
test "$(grep -c 'volatile double avd_ip' "$OUT/src/cResourceCount.cc")" -eq 2
for f in tMatrix.h AvidaTools.h; do
  git show 47f13dadb:avida-core/source/tools/$f > "$OUT/src/$f"
done
FLAGS="-std=c++11 -O3 -funroll-loops -fstrict-aliasing -DNDEBUG -w -I$T/stub -I$OUT/src -Ilibs/apto/include -Icbuild/libs/apto/include"
c++ $FLAGS $T/ref_harness.cc "$OUT/src/cResourceCount.cc" "$OUT/src/cSpatialResCount.cc" \
    "$OUT/src/cSpatialCountElem.cc" "$OUT/src/cGradientCount.cc" cbuild/lib/libapto.a -o "$OUT/resref"
for s in global spatial gradient actions; do
  "$OUT/resref" $s > "$D/$s.txt" 2>/dev/null
done
