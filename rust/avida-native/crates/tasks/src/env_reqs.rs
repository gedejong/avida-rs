//! Port of `cEnvReqs` (avida-core/source/main/cEnvReqs.h): environment
//! requirements a task asks for while it is being loaded.

/// `cEnvReqs`: minimum input/output buffer sizes and the true-random-inputs flag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvReqs {
    min_inputs: i32,
    min_outputs: i32,
    true_rand_inputs: bool,
}

impl EnvReqs {
    /// `cEnvReqs()`: all zero / false.
    pub fn new() -> Self {
        Self::default()
    }
    /// `GetMinInputs()`.
    pub fn get_min_inputs(&self) -> i32 {
        self.min_inputs
    }
    /// `SetMinInputs(int)`.
    pub fn set_min_inputs(&mut self, v: i32) {
        self.min_inputs = v;
    }
    /// `GetMinOutputs()`.
    pub fn get_min_outputs(&self) -> i32 {
        self.min_outputs
    }
    /// `SetMinOutputs(int)`.
    pub fn set_min_outputs(&mut self, v: i32) {
        self.min_outputs = v;
    }
    /// `SetTrueRandInputs(bool v = true)`.
    pub fn set_true_rand_inputs(&mut self, v: bool) {
        self.true_rand_inputs = v;
    }
    /// `GetTrueRandInputs()`.
    pub fn get_true_rand_inputs(&self) -> bool {
        self.true_rand_inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_setters() {
        let mut e = EnvReqs::new();
        assert_eq!(e.get_min_inputs(), 0);
        assert_eq!(e.get_min_outputs(), 0);
        assert!(!e.get_true_rand_inputs());
        e.set_min_inputs(3);
        e.set_min_outputs(6);
        e.set_true_rand_inputs(true);
        assert_eq!((e.get_min_inputs(), e.get_min_outputs()), (3, 6));
        assert!(e.get_true_rand_inputs());
    }
}
