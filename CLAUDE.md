# Avida — Claude Code Context

This file is automatically loaded by Claude Code in every session for this project.

---

## C++ to Rust Migration — Slice Workflow

Use this workflow for any migration slice. Pair with the program-docs rules below.

### 1. Plan-first execution
- If asked to plan, produce a concrete slice plan with clear scope and validation gates.
- Do not edit attached plan files; implement exactly from the plan unless the user requests changes.
- Keep migration incremental: preserve C ABI boundaries and avoid ownership redesign in one slice.

### 2. Todo discipline
- Reuse existing todos from the plan; do not create duplicates.
- Set the first todo to `in_progress` before making edits.
- Keep exactly one todo `in_progress`; mark each `completed` as work finishes.

### 3. Implementation constraints
- Preserve behavior parity by default (no silent policy changes).
- Keep exported FFI signatures stable unless the slice explicitly changes API surface.
- Prefer helper extraction and deterministic pure-function seams before stateful rewrites.
- **CPU handler guard ordering**: When a C++ instruction handler has a guard (e.g., `if (!occupied) return false;`) BEFORE `FindModifiedRegister`, the guard MUST remain in C++ and execute BEFORE the Rust FFI call. Do NOT pass `FindModifiedRegister` as a function parameter to the Rust FFI — it would be evaluated before the guard, consuming a nop instruction and changing IP position even when the guard fails. Always: guard in C++ first, then call Rust.

### 4. Mandatory validation gates

Rust gates (run inside `rust/avida-rust`):
```
cargo test --all-targets
cargo fmt --check
cargo clippy --all-targets -- -D warnings
./scripts/ci_coverage_check.sh 75
```

Project gates (run from repo root):
```
./build_avida -DAVD_UNIT_TESTS:BOOL=ON -DAPTO_UNIT_TESTS:BOOL=OFF
./cbuild/work/unit-tests
./run_tests --mode=slave
```

### 5. Documentation refresh
After gates pass, update:
- `documentation/Tech-Debt-Backlog.md` — completed item + next candidate
- `rust-migration-waves.md` — slice status and scope notes

If the slice changes long-range prioritization, complexity assumptions, or workstream ordering, also update:
- `rust-migration-remaining-cpp-static-analysis.md`
- `rust-migration-full-plan.md`
- `rust-migration-workstream-catalog.md`
- `rust-migration-execution-playbook.md`

### 6. Commit hygiene
- Commit only when the user requests.
- Stage intended files explicitly; avoid unrelated submodule pointer changes.
- Use concise commit messages focused on *why* the slice was done.

### 7. GitHub issue discipline
- **NEVER close issues.** Issues with the `tracking` label are protected by a CI workflow that auto-reopens them. Only a human maintainer may close tracking issues (by removing the label first).
- **NEVER use `Fixes #N` or `Closes #N`** in commit messages or PR descriptions for tracking issues. These keywords auto-close the referenced issue when merged. Use **`Relates to #N`** or **`Part of #N`** instead.
- When a slice completes work toward a tracking issue, **add a progress comment** to the issue describing what was done, not close it.
- Only use `Fixes #N` for small, self-contained bug-fix issues that are fully resolved by a single PR.

---

## Rust Migration Program Docs

When working at program scale, always read and maintain these documents:

| File | Purpose |
|------|---------|
| `rust-migration-remaining-cpp-static-analysis.md` | Static-analysis baseline |
| `rust-migration-full-plan.md` | Full roadmap |
| `rust-migration-workstream-catalog.md` | Workstream registry |
| `rust-migration-execution-playbook.md` | Mandatory slice gates + playbook |
| `rust-migration-waves.md` | Per-wave status and scope notes |
| `documentation/Tech-Debt-Backlog.md` | Active backlog |

### During planning
- Read the static-analysis baseline and current roadmap before proposing a new slice.
- Align each new slice to the highest-priority remaining workstream/hotspot.
- State where the slice fits in the full plan and what becomes the next candidate.

### During implementation
- Keep changes incremental and ABI-safe unless explicitly asked otherwise.
- After gates pass, update the roadmap/backlog and relevant program docs in the same change set.
- Reflect any reprioritization caused by new findings (risk, complexity, coupling, or test gaps).

### Documentation quality bar
- Keep docs factual and metric-backed; avoid vague progress notes.
- Preserve consistency across all migration docs (no conflicting next-candidate entries).
- If static-analysis assumptions change materially, refresh the baseline document before closing the slice.

---

## Current Migration State (as of 2026-03-26)

**~800+ FFI exports. 716 Rust tests. ~26K LOC Rust across 43+ modules.**
**9 `#[repr(C)]` type replacements**: Merit, CpuStack, CodeLabel, MutationRates, PhenotypeCoreMetrics, PhenotypeStatusFlags, PhenotypeLifetimeData, BirthEntryScalars, CpuRegisters. Plus GradientConfig, GradientState.

### Completed milestones:
- **Issue #69**: Apto::Array → AvidaArray (std::vector) — 99% complete (1,100→14 occurrences)
- **Issue #70**: FFI ownership bridge — FfiVec<T>, FfiString, AvidaRNG (bit-exact port), generic handle utilities
- **Issue #43**: Rust type system infrastructure
- **Issue #48**: cPhenotype — 121 scalar fields in Rust, 11 energy methods, array bulk ops via FFI
- **Issue #49**: cBirthEntry — 8 scalar fields in Rust
- **Issue #50**: cTaskLib — 199/204 task eval functions (97.5%) in Rust; 5 remaining blocked on complex C++ deps
- **Issues #51-52**: Wave 12 — newtype IDs, foundations established
- **Issues #53-56**: Wave 13 — cResourceCount (41 FFI, 1,278 LOC Rust), cGradientCount (66 fields in Rust), cStats/cDeme foundations
- **Issue #58**: Organism FFI — ~120 functions (read-only + mutable + method delegation: DoOutput, DoInput, SendMessage, Die, Kaboom, Move, Rotate, etc.)
- **Issue #57**: Hardware state FFI — 41+ functions in cHardwareFFI.cc (heads, stacks, memory, registers, nop helpers, thread/cycle, labels/search, cAvidaContext RNG)
- **Issue #57**: CPU instruction handlers — 285/513 (55.6%) delegate to Rust (register, conditional, stack, flow, head, mask, consensus, label, search, RNG-probabilistic, organism-read/write, I/O, messaging, donation, movement, sensing, death/kazi ops)
- **Issue #74**: Viewer scaffold + simulation bridge — eframe app connected to live simulation via FFI (parked)

### Key infrastructure:
- AvidaArray<T> shim with operator+/+=, RemoveAt, iterator support
- FfiVec<T> for Rust-owned containers
- AvidaRNG bit-exact Knuth subtractive generator in Rust
- Newtype IDs: ReactionId, ResourceId, TaskId, CellId, OrganismId, DemeId
- ConfigSnapshot (80 fields) + TaskContextSnapshot (26 fields) for Rust config/organism access
- Buffer-pointer FFI pattern for variable-length data (RoyalRoad, AllOnes)
- Parametric FFI pattern for tasks needing computed C++ values (FormSpatialGroup, resource-dependent)
- 3 latent C++ bugs fixed (cDeme::Reset init, cResourceCount spatial cache, cBirthEntry Rule of Three)

### PO priorities (updated 2026-03-26):

**Situation**: CPU handlers past majority (55.6%). Organism FFI at ~120 functions (read+write+method delegation). I/O, messaging, death, donation, movement all delegating. Remaining 228 handlers are the harder tail:
- ~65 division/reproduction (genome manipulation — Wave 14 ownership)
- ~48 head/memory with mutation testing (TestCopyMut — needs mutation FFI)
- ~40 sensing/environment (resource library access)
- ~40 movement variants (complex rotation/label logic)
- ~35 donation variants (genome edit distance, systematics access)

1. **Next: Push handler count toward 60-65%** — port remaining sensing, movement, and head/memory handlers where FFI exists. Add mutation-testing FFI (`avd_ctx_test_copy_mut`, `avd_ctx_test_divide_mut`) to unblock the ~48 mutation-testing handlers.
2. **Then: Assess genome-level FFI** — `Inst_HeadCopy` with mutation, `Inst_HCopy`, and genome instruction access. This is the gateway to the 65 division/reproduction handlers.
3. **Viewer is last** (#73-#81) — the ncurses TUI works.
4. **Remaining 5 pure-C++ tasks** (#50) — defer.
5. **Remaining 14 Apto::Array occurrences** — defer.
