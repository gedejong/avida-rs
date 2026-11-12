//! What the resource code reaches through `cWorld` / `cAvidaContext`.
//!
//! The C++ gradient code (`cGradientCount.cc`) calls
//! `m_world->GetStats().GetUpdate()`, `ctx.HasDriver()`,
//! `m_world->GetDriver().Feedback().Error(...)` and three
//! `m_world->GetPopulation().Execute*Resource(ctx, ...)` methods. This trait
//! is the seam for those; the world crate implements it. The RNG is passed
//! separately (`&mut AvidaRng`, the one world RNG) and handed back to the
//! population callbacks, which draw from it at exactly the C++ call points.
//!
//! The callbacks run while the [`ResourceCount`](crate::ResourceCount) is
//! mutably borrowed, so an implementation cannot call back into it. In the
//! C++ a killed organism could in principle re-enter `cResourceCount`
//! (for example by depositing resources); such an implementation has to
//! queue that work, which the integrator must check against the C++.

use avida_apto::AvidaRng;

/// World-level services used by spatial/gradient resource updates.
pub trait ResourceEnv {
    /// `cStats::GetUpdate()` (it is `-1` while the world is being built).
    fn update(&self) -> i32;

    /// `cAvidaContext::HasDriver()` of the context the C++ would be using:
    /// `false` for the world's default context during construction, `true`
    /// once a driver is attached.
    fn has_driver(&self) -> bool;

    /// `m_world->GetDriver().Feedback().Error(msg)`; the C++ driver prints
    /// `"error: " + msg` to stdout.
    fn feedback_error(&mut self, msg: &str);

    /// `cPopulation::ExecutePredatoryResource(ctx, cell_id, pred_odds,
    /// juvs_per, hammer)`.
    fn execute_predatory_resource(
        &mut self,
        rng: &mut AvidaRng,
        cell_id: i32,
        pred_odds: f64,
        juvs_per: i32,
        hammer: bool,
    );

    /// `cPopulation::ExecuteDamagingResource(ctx, cell_id, damage, hammer)`.
    fn execute_damaging_resource(
        &mut self,
        rng: &mut AvidaRng,
        cell_id: i32,
        damage: f64,
        hammer: bool,
    );

    /// `cPopulation::ExecuteDeadlyResource(ctx, cell_id, odds, hammer)`.
    fn execute_deadly_resource(
        &mut self,
        rng: &mut AvidaRng,
        cell_id: i32,
        odds: f64,
        hammer: bool,
    );
}

/// A population-less [`ResourceEnv`] that records what it is asked to do.
///
/// Suitable for global/spatial-only resource counts and for tests. The
/// population callbacks draw nothing from the RNG; they are recorded in
/// `calls` as `"pred <cell> <odds> <juvs> <hammer>"`,
/// `"damage <cell> <damage> <hammer>"` and `"deadly <cell> <odds> <hammer>"`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordingEnv {
    /// Returned by [`ResourceEnv::update`].
    pub update: i32,
    /// Returned by [`ResourceEnv::has_driver`].
    pub has_driver: bool,
    /// Messages passed to [`ResourceEnv::feedback_error`].
    pub errors: Vec<String>,
    /// Population callbacks, in call order.
    pub calls: Vec<String>,
}

impl RecordingEnv {
    /// An environment at `update` with or without a driver.
    pub fn new(update: i32, has_driver: bool) -> Self {
        RecordingEnv {
            update,
            has_driver,
            errors: Vec::new(),
            calls: Vec::new(),
        }
    }
}

impl ResourceEnv for RecordingEnv {
    fn update(&self) -> i32 {
        self.update
    }

    fn has_driver(&self) -> bool {
        self.has_driver
    }

    fn feedback_error(&mut self, msg: &str) {
        self.errors.push(msg.to_string());
    }

    fn execute_predatory_resource(
        &mut self,
        _rng: &mut AvidaRng,
        cell_id: i32,
        pred_odds: f64,
        juvs_per: i32,
        hammer: bool,
    ) {
        self.calls.push(format!(
            "pred {cell_id} {pred_odds} {juvs_per} {}",
            u8::from(hammer)
        ));
    }

    fn execute_damaging_resource(
        &mut self,
        _rng: &mut AvidaRng,
        cell_id: i32,
        damage: f64,
        hammer: bool,
    ) {
        self.calls
            .push(format!("damage {cell_id} {damage} {}", u8::from(hammer)));
    }

    fn execute_deadly_resource(
        &mut self,
        _rng: &mut AvidaRng,
        cell_id: i32,
        odds: f64,
        hammer: bool,
    ) {
        self.calls
            .push(format!("deadly {cell_id} {odds} {}", u8::from(hammer)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_env_records_in_order() {
        let mut env = RecordingEnv::new(7, true);
        let mut rng = AvidaRng::new(1);
        assert_eq!(env.update(), 7);
        assert!(env.has_driver());
        env.feedback_error("boom");
        env.execute_predatory_resource(&mut rng, 3, 0.5, 2, true);
        env.execute_damaging_resource(&mut rng, 4, 0.25, false);
        env.execute_deadly_resource(&mut rng, 5, 0.125, true);
        assert_eq!(env.errors, vec!["boom".to_string()]);
        assert_eq!(
            env.calls,
            vec!["pred 3 0.5 2 1", "damage 4 0.25 0", "deadly 5 0.125 1"]
        );
    }
}
