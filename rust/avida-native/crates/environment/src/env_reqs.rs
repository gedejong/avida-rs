//! Port of `cEnvReqs` (avida-core/source/main/cEnvReqs.h).

/// `cEnvReqs`: what a task needs from the environment (filled in by the task
/// library while a reaction's trigger is loaded).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EnvReqs {
    /// `m_min_inputs`
    pub min_inputs: i32,
    /// `m_min_outputs`
    pub min_outputs: i32,
    /// `m_true_rand_inputs`
    pub true_rand_inputs: bool,
}

impl EnvReqs {
    /// `cEnvReqs()`.
    pub fn new() -> Self {
        Self::default()
    }
    /// `SetMinInputs(v)`.
    pub fn set_min_inputs(&mut self, v: i32) {
        self.min_inputs = v;
    }
    /// `SetMinOutputs(v)`.
    pub fn set_min_outputs(&mut self, v: i32) {
        self.min_outputs = v;
    }
    /// `SetTrueRandInputs(v)`.
    pub fn set_true_rand_inputs(&mut self, v: bool) {
        self.true_rand_inputs = v;
    }
}
