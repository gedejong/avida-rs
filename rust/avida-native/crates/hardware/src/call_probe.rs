//! Port-only: a probe that reports, for one test-CPU run, from which `call`
//! sites each genome site was executed (the "subroutine reuse" measure of
//! `PrintReuseSurvey`, ARCHITECTURE.md, "Knockout map and reuse measures").
//!
//! The probe keeps a shadow stack of the `call` sites that are active: a
//! successful `call` at site `c` pushes `c`, a `return` pops (an empty stack
//! is left alone). The shadow stack is independent of the organism's data
//! stack, so an organism that overwrites its return address with `pop` or
//! `push` does not confuse it; it only sees which `return` ran. Each
//! executed instruction is recorded against the innermost active call site
//! (nothing when none is active).
//!
//! The probe is attached to one hardware by the test CPU
//! (`CpuTestInfo::set_call_probe`); an unattached hardware pays one
//! `Option` test per instruction and per `call` or `return`, and nothing
//! it does changes.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// Shared handle to a probe.
pub type CallProbePtr = Arc<Mutex<CallProbe>>;

/// The record of one run.
#[derive(Debug, Default)]
pub struct CallProbe {
    stack: Vec<i32>,
    /// `(site, innermost active call site)` pairs seen.
    seen: BTreeSet<(i32, i32)>,
    /// Successful calls and returns seen.
    pub calls: u64,
}

impl CallProbe {
    /// A new probe in a shared handle.
    pub fn new_ptr() -> CallProbePtr {
        Arc::new(Mutex::new(CallProbe::default()))
    }

    /// The instruction at `site` executes.
    pub fn executed(&mut self, site: i32) {
        if let Some(&c) = self.stack.last() {
            self.seen.insert((site, c));
        }
    }

    /// A `call` at `site` jumped.
    pub fn enter(&mut self, site: i32) {
        self.calls += 1;
        self.stack.push(site);
    }

    /// A `return` ran.
    pub fn leave(&mut self) {
        self.stack.pop();
    }

    /// For each of `len` genome sites, the number of distinct call sites
    /// that were the innermost active one when it executed.
    pub fn contexts(&self, len: usize) -> Vec<usize> {
        let mut out = vec![0usize; len];
        for &(site, _) in &self.seen {
            if let Ok(i) = usize::try_from(site) {
                if i < len {
                    out[i] += 1;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_distinct_innermost_call_sites() {
        let mut p = CallProbe::default();
        p.executed(0); // outside any call
        for c in [3, 5, 3] {
            p.executed(c);
            p.enter(c);
            p.executed(10);
            p.executed(11);
            p.leave();
        }
        p.leave(); // extra return: ignored
        let n = p.contexts(12);
        assert_eq!(n[10], 2);
        assert_eq!(n[11], 2);
        assert_eq!(n[0], 0);
        assert_eq!(n[3], 0);
        assert_eq!(p.calls, 3);
    }

    #[test]
    fn nested_body_counts_its_innermost_call_site() {
        let mut p = CallProbe::default();
        for c in [1, 2] {
            p.enter(c);
            p.executed(20);
            p.enter(7);
            p.executed(30);
            p.leave();
            p.leave();
        }
        let n = p.contexts(40);
        assert_eq!(n[20], 2);
        assert_eq!(n[30], 1);
    }
}
