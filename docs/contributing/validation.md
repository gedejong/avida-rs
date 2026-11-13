# Validation gates

Before a change counts as done, run these gates inside `rust/avida-native`. They are the repository's mandatory gates.

## Every change

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must pass. Format, lints with warnings as errors, and the test suite.

The terminal interface's pseudo-terminal tests are timing-sensitive. Run them on a machine that is not saturated, and check for stray `avida-tui` processes afterwards.

## A change to simulation behaviour

For any change that could alter what a run computes, also compare the whole consistency suite against the reference binary. This needs the reference binary built from the tag `cpp-final` (the C++ is no longer in the working tree).

1. Build a reference binary in a separate worktree:

   ```sh
   git worktree add ../avida-cpp cpp-final
   (cd ../avida-cpp && ./build_avida)
   ```

2. Point the suite at it and run it:

   ```sh
   cargo build --release -p avida
   SUITE_REF=/path/to/avida-cpp/cbuild/work/avida python3 tools/run_suite.py
   ```

   The suite caches the reference output, so the reference binary is needed again only when you clear the cache or add a test.

3. Expect: 85 same, 34 trivial, 1 UNSUPPORTED, 1 REF-TIMEOUT, no DIFF. A DIFF is a behaviour change.

`tools/run_suite.py` also takes test-name substrings to run a subset, and the environment variables `SUITE_BIN`, `SUITE_WORK`, `SUITE_OUT` and `SUITE_JOBS`.

## A change to the parallel mode

A change to the parallel mode's scheduling needs a fresh statistical comparison with the exact mode, reported with the change. See [how parity is tested](parity.md). The hooks the parallel mode has in the exact code paths must not change what the exact mode computes, which `tools/run_suite.py` confirms.

## After the gates pass

Update `rust/avida-native/ARCHITECTURE.md` (status, counts, the list of `unsupported:` stops) with factual, metric-backed statements. Do not leave conflicting figures in two places.

## Commits and issues

- Commit only when asked. Stage the intended files explicitly, and avoid unrelated submodule pointer changes.
- Write concise commit messages that say why the change was made.
- Issues labelled `tracking` are protected by a workflow that reopens them. Do not close them, and use "Relates to #N" or "Part of #N" in commit messages and pull requests, not "Fixes" or "Closes". Add a progress comment to the issue instead.
