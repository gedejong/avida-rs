//! Port of `cMutationRates` (avida-core/source/main/cMutationRates.{h,cc}).
//!
//! Field layout follows the C++ `sMutationRates` sub-structs. The `test_*`,
//! `num_*` and `do_meta_copy_mut` methods draw from the world RNG exactly as
//! the inline C++ methods do.

use avida_apto::AvidaRng;
use avida_config::Config;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CopyMuts {
    pub mut_prob: f64,
    pub ins_prob: f64,
    pub del_prob: f64,
    pub uniform_prob: f64,
    pub slip_prob: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DivideMuts {
    pub ins_prob: f64,
    pub del_prob: f64,
    pub mut_prob: f64,
    pub uniform_prob: f64,
    pub slip_prob: f64,
    pub trans_prob: f64,
    pub lgt_prob: f64,
    pub divide_mut_prob: f64,
    pub divide_ins_prob: f64,
    pub divide_del_prob: f64,
    pub divide_uniform_prob: f64,
    pub divide_slip_prob: f64,
    pub divide_trans_prob: f64,
    pub divide_lgt_prob: f64,
    pub divide_poisson_mut_mean: f64,
    pub divide_poisson_ins_mean: f64,
    pub divide_poisson_del_mean: f64,
    pub divide_poisson_slip_mean: f64,
    pub divide_poisson_trans_mean: f64,
    pub divide_poisson_lgt_mean: f64,
    pub parent_mut_prob: f64,
    pub parent_ins_prob: f64,
    pub parent_del_prob: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PointMuts {
    pub ins_prob: f64,
    pub del_prob: f64,
    pub mut_prob: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InjectMuts {
    pub ins_prob: f64,
    pub del_prob: f64,
    pub mut_prob: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MetaMuts {
    pub copy_mut_prob: f64,
    pub standard_dev: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UpdateMuts {
    pub death_prob: f64,
}

// --- Main struct ---

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MutationRates {
    pub copy: CopyMuts,
    pub divide: DivideMuts,
    pub point: PointMuts,
    pub inject: InjectMuts,
    pub meta: MetaMuts,
    pub update: UpdateMuts,
}

impl MutationRates {
    /// C++: `cMutationRates::Setup`
    pub fn setup(&mut self, config: &Config) {
        self.copy.mut_prob = config.copy_mut_prob;
        self.copy.ins_prob = config.copy_ins_prob;
        self.copy.del_prob = config.copy_del_prob;
        self.copy.uniform_prob = config.copy_uniform_prob;
        self.copy.slip_prob = config.copy_slip_prob;
        self.divide.ins_prob = config.div_ins_prob;
        self.divide.del_prob = config.div_del_prob;
        self.divide.mut_prob = config.div_mut_prob;
        self.divide.uniform_prob = config.div_uniform_prob;
        self.divide.slip_prob = config.div_slip_prob;
        self.divide.trans_prob = config.div_trans_prob;
        self.divide.lgt_prob = config.div_lgt_prob;
        self.divide.divide_mut_prob = config.divide_mut_prob;
        self.divide.divide_ins_prob = config.divide_ins_prob;
        self.divide.divide_del_prob = config.divide_del_prob;
        self.divide.divide_uniform_prob = config.divide_uniform_prob;
        self.divide.divide_slip_prob = config.divide_slip_prob;
        self.divide.divide_trans_prob = config.divide_trans_prob;
        self.divide.divide_lgt_prob = config.divide_lgt_prob;
        self.divide.parent_mut_prob = config.parent_mut_prob;
        self.divide.parent_ins_prob = config.parent_ins_prob;
        self.divide.parent_del_prob = config.parent_del_prob;
        self.point.ins_prob = config.point_ins_prob;
        self.point.del_prob = config.point_del_prob;
        self.point.mut_prob = config.point_mut_prob;
        self.inject.ins_prob = config.inject_ins_prob;
        self.inject.del_prob = config.inject_del_prob;
        self.inject.mut_prob = config.inject_mut_prob;
        self.meta.copy_mut_prob = config.meta_copy_mut;
        self.meta.standard_dev = config.meta_std_dev;
        self.update.death_prob = config.death_prob;
        self.divide.divide_poisson_mut_mean = config.divide_poisson_mut_mean;
        self.divide.divide_poisson_ins_mean = config.divide_poisson_ins_mean;
        self.divide.divide_poisson_del_mean = config.divide_poisson_del_mean;
        self.divide.divide_poisson_slip_mean = config.divide_poisson_slip_mean;
        self.divide.divide_poisson_trans_mean = config.divide_poisson_trans_mean;
        self.divide.divide_poisson_lgt_mean = config.divide_poisson_lgt_mean;
    }

    /// C++: `cMutationRates::Clear`
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// C++: `cMutationRates::Copy`
    pub fn copy_from(&mut self, other: &MutationRates) {
        *self = *other;
    }

    /// C++: `cMutationRates::TestCopyMut`
    pub fn test_copy_mut(&self, rng: &mut AvidaRng) -> bool {
        if self.copy.mut_prob == 0.0 {
            false
        } else {
            rng.p(self.copy.mut_prob)
        }
    }

    /// C++: `cMutationRates::TestCopyIns`
    pub fn test_copy_ins(&self, rng: &mut AvidaRng) -> bool {
        if self.copy.ins_prob == 0.0 {
            false
        } else {
            rng.p(self.copy.ins_prob)
        }
    }

    /// C++: `cMutationRates::TestCopyDel`
    pub fn test_copy_del(&self, rng: &mut AvidaRng) -> bool {
        if self.copy.del_prob == 0.0 {
            false
        } else {
            rng.p(self.copy.del_prob)
        }
    }

    /// C++: `cMutationRates::TestCopySlip`
    pub fn test_copy_slip(&self, rng: &mut AvidaRng) -> bool {
        if self.copy.slip_prob == 0.0 {
            false
        } else {
            rng.p(self.copy.slip_prob)
        }
    }

    /// C++: `cMutationRates::TestCopyUniform`
    pub fn test_copy_uniform(&self, rng: &mut AvidaRng) -> bool {
        if self.copy.uniform_prob == 0.0 {
            false
        } else {
            rng.p(self.copy.uniform_prob)
        }
    }

    /// C++: `cMutationRates::TestDivideMut`
    pub fn test_divide_mut(&self, rng: &mut AvidaRng) -> bool {
        rng.p(self.divide.divide_mut_prob)
    }

    /// C++: `cMutationRates::TestDivideIns`
    pub fn test_divide_ins(&self, rng: &mut AvidaRng) -> bool {
        rng.p(self.divide.divide_ins_prob)
    }

    /// C++: `cMutationRates::TestDivideDel`
    pub fn test_divide_del(&self, rng: &mut AvidaRng) -> bool {
        rng.p(self.divide.divide_del_prob)
    }

    /// C++: `cMutationRates::TestDivideUniform`
    pub fn test_divide_uniform(&self, rng: &mut AvidaRng) -> bool {
        if self.divide.divide_uniform_prob == 0.0 {
            false
        } else {
            rng.p(self.divide.divide_uniform_prob)
        }
    }

    /// C++: `cMutationRates::TestDivideSlip`
    pub fn test_divide_slip(&self, rng: &mut AvidaRng) -> bool {
        rng.p(self.divide.divide_slip_prob)
    }

    /// C++: `cMutationRates::TestDivideTrans`
    pub fn test_divide_trans(&self, rng: &mut AvidaRng) -> bool {
        if self.divide.divide_trans_prob == 0.0 {
            false
        } else {
            rng.p(self.divide.divide_trans_prob)
        }
    }

    /// C++: `cMutationRates::TestDivideLGT`
    pub fn test_divide_lgt(&self, rng: &mut AvidaRng) -> bool {
        if self.divide.divide_lgt_prob == 0.0 {
            false
        } else {
            rng.p(self.divide.divide_lgt_prob)
        }
    }

    /// C++: `cMutationRates::NumDividePoissonMut`
    pub fn num_divide_poisson_mut(&self, rng: &mut AvidaRng) -> u32 {
        if self.divide.divide_poisson_mut_mean == 0.0 {
            0
        } else {
            rng.get_rand_poisson(self.divide.divide_poisson_mut_mean)
        }
    }

    /// C++: `cMutationRates::NumDividePoissonIns`
    pub fn num_divide_poisson_ins(&self, rng: &mut AvidaRng) -> u32 {
        if self.divide.divide_poisson_ins_mean == 0.0 {
            0
        } else {
            rng.get_rand_poisson(self.divide.divide_poisson_ins_mean)
        }
    }

    /// C++: `cMutationRates::NumDividePoissonDel`
    pub fn num_divide_poisson_del(&self, rng: &mut AvidaRng) -> u32 {
        if self.divide.divide_poisson_del_mean == 0.0 {
            0
        } else {
            rng.get_rand_poisson(self.divide.divide_poisson_del_mean)
        }
    }

    /// C++: `cMutationRates::NumDividePoissonSlip`
    pub fn num_divide_poisson_slip(&self, rng: &mut AvidaRng) -> u32 {
        if self.divide.divide_poisson_slip_mean == 0.0 {
            0
        } else {
            rng.get_rand_poisson(self.divide.divide_poisson_slip_mean)
        }
    }

    /// C++: `cMutationRates::NumDividePoissonTrans`
    pub fn num_divide_poisson_trans(&self, rng: &mut AvidaRng) -> u32 {
        if self.divide.divide_poisson_trans_mean == 0.0 {
            0
        } else {
            rng.get_rand_poisson(self.divide.divide_poisson_trans_mean)
        }
    }

    /// C++: `cMutationRates::NumDividePoissonLGT`
    pub fn num_divide_poisson_lgt(&self, rng: &mut AvidaRng) -> u32 {
        if self.divide.divide_poisson_lgt_mean == 0.0 {
            0
        } else {
            rng.get_rand_poisson(self.divide.divide_poisson_lgt_mean)
        }
    }

    /// C++: `cMutationRates::DoMetaCopyMut`
    pub fn do_meta_copy_mut(&mut self, rng: &mut AvidaRng) -> f64 {
        if self.meta.copy_mut_prob == 0.0 || !rng.p(self.meta.copy_mut_prob) {
            return 1.0;
        }
        let exp = rng.get_rand_normal() * self.meta.standard_dev;
        let change = 2.0_f64.powf(exp);
        self.copy.mut_prob *= change;
        change
    }

    /// C++: `cMutationRates::TestDeath`
    pub fn test_death(&self, rng: &mut AvidaRng) -> bool {
        if self.update.death_prob == 0.0 {
            false
        } else {
            rng.p(self.update.death_prob)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_reads_config_defaults() {
        let mut rates = MutationRates::default();
        rates.setup(&Config::default());
        assert_eq!(rates.copy.mut_prob, 0.0075);
        assert_eq!(rates.divide.divide_ins_prob, 0.05);
        assert_eq!(rates.divide.divide_del_prob, 0.05);
    }

    #[test]
    fn zero_rates_draw_nothing() {
        let rates = MutationRates::default();
        let mut a = AvidaRng::new(7);
        let mut b = AvidaRng::new(7);
        assert!(!rates.test_copy_mut(&mut a));
        assert!(!rates.test_divide_uniform(&mut a));
        assert_eq!(rates.num_divide_poisson_mut(&mut a), 0);
        assert!(!rates.test_death(&mut a));
        assert_eq!(a.get_double(), b.get_double());
    }

    #[test]
    fn divide_tests_always_draw() {
        // QUIRK: TestDivideMut/Ins/Del/Slip consult the RNG even at 0.0.
        let rates = MutationRates::default();
        let mut a = AvidaRng::new(7);
        let mut b = AvidaRng::new(7);
        assert!(!rates.test_divide_mut(&mut a));
        b.get_double();
        assert_eq!(a.get_double(), b.get_double());
    }

    #[test]
    fn meta_mut_scales_copy_rate() {
        let mut rates = MutationRates::default();
        rates.copy.mut_prob = 0.01;
        rates.meta.copy_mut_prob = 1.0;
        rates.meta.standard_dev = 0.5;
        let mut rng = AvidaRng::new(11);
        let change = rates.do_meta_copy_mut(&mut rng);
        assert!((rates.copy.mut_prob - 0.01 * change).abs() < 1e-15);
    }
}
