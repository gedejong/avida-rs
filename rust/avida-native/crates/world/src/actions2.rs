//! Hub for the action modules of the second porting round
//! (`actions_stats2`, `actions_print2`, `actions_hist`, `actions_pop2`,
//! `actions_deme2`, `actions_env`): their state and their run-loop hooks.

use crate::world::World;

/// State kept between calls by the actions of the second-round modules.
#[derive(Debug, Default)]
pub struct Actions2State {
    pub stats2: crate::actions_stats2::Stats2State,
    pub print2: crate::actions_print2::Print2State,
    pub hist: crate::actions_hist::HistState,
    pub pop2: crate::actions_pop2::Pop2State,
    pub deme2: crate::actions_deme2::Deme2State,
    pub env: crate::actions_env::EnvState,
    pub a3: crate::actions3::Actions3State,
}

impl World {
    /// Called once before the first update.
    pub(crate) fn actions2_note_recorders(&mut self) {
        self.stats2_note_recorders();
        self.print2_note_recorders();
        self.hist_note_recorders();
        self.pop2_note_recorders();
        self.deme2_note_recorders();
        self.env_note_recorders();
        self.a3_note_recorders();
    }

    /// Called at the end of every update.
    pub(crate) fn actions2_end_update(&mut self) {
        self.stats2_end_update();
        self.print2_end_update();
        self.hist_end_update();
        self.pop2_end_update();
        self.deme2_end_update();
        self.env_end_update();
    }
}
