//! The organism debugger's state: which organism is watched, what the last
//! step changed, the breakpoints, and what became of the organism.
//!
//! The session reports the inspected cell's occupant in every frame. The
//! debugger pins the organism (by `cOrganism::GetID()`) that the first
//! frame after a selection shows, keeps its latest view, and keeps the view
//! before the last change so a step can be shown as a difference. When the
//! cell empties or another organism moves in, the last view of the watched
//! organism stays on screen, marked as such, until the user moves on.

use std::collections::BTreeSet;

use avida_session::{Frame, RunUntil, StopWhen, UntilOutcome};
use avida_viewmodel::{MemSpaceView, OrganismView, ThreadView};

/// Time slices a "step instruction" may take before giving up.
pub const STEP_BUDGET: u64 = 2_000_000;
/// Time slices a "run to breakpoint" may take before giving up.
pub const BREAK_BUDGET: u64 = 50_000_000;

/// What became of the watched organism.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fate {
    /// It lives in the selected cell.
    Alive,
    /// The cell is empty: the organism died.
    Died { update: i32 },
    /// Another organism lives in the cell now.
    Replaced { by: i32, update: i32 },
}

/// What a run-until was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopWhat {
    Step,
    Breakpoint,
}

/// How the last run-until ended, for the debugger's status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopNote {
    pub outcome: UntilOutcome,
    pub slices: u64,
    pub update: i32,
    pub what: StopWhat,
}

fn plural(n: u64, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

impl StopNote {
    /// One sentence for the status line.
    pub fn text(&self) -> String {
        let slices = plural(self.slices, "time slice");
        let at = crate::app::update_phrase(self.update);
        match (self.outcome, self.what) {
            (UntilOutcome::Reached, StopWhat::Step) => format!("Stepped after {slices}."),
            (UntilOutcome::Reached, StopWhat::Breakpoint) => {
                format!("Breakpoint reached after {slices}.")
            }
            (UntilOutcome::OrganismGone { occupant: None }, _) => {
                format!("The organism died at {at} after {slices}.")
            }
            (UntilOutcome::OrganismGone { occupant: Some(id) }, _) => {
                format!("Organism #{id} took over the cell at {at} after {slices}.")
            }
            (UntilOutcome::BudgetExhausted, StopWhat::Step) => {
                format!("The organism did not run within {slices}.")
            }
            (UntilOutcome::BudgetExhausted, StopWhat::Breakpoint) => {
                format!("No breakpoint reached within {slices}.")
            }
            (UntilOutcome::Interrupted, _) => format!("Interrupted after {slices}."),
            (UntilOutcome::RunOver, _) => format!("The run ended after {slices}."),
        }
    }

    /// Whether the note reports a problem rather than a stop as asked.
    pub fn is_problem(&self) -> bool {
        !matches!(
            self.outcome,
            UntilOutcome::Reached | UntilOutcome::Interrupted
        )
    }
}

#[derive(Clone, Debug)]
pub struct Debugger {
    /// The watched organism's id, once a frame has shown it.
    pub org_id: Option<i32>,
    /// Its latest view (kept after it is gone).
    pub current: Option<OrganismView>,
    /// Its view before the last change.
    pub previous: Option<OrganismView>,
    pub fate: Fate,
    /// `(memory space, position)` sites to stop at.
    pub breakpoints: BTreeSet<(i32, i32)>,
    /// The memory space listed (an id from [`avida_viewmodel::CpuView::spaces`]).
    /// Follows the shown thread's instruction pointer while `follow_ip`.
    pub space: i32,
    /// The selection follows the instruction pointer.
    pub follow_ip: bool,
    /// Whether the session moves the watch to each offspring.
    pub follow_offspring: bool,
    /// The thread shown.
    pub thread: usize,
    /// The selected memory line.
    pub sel: usize,
    /// First memory line shown (kept between draws so scrolling is steady).
    pub top: usize,
    pub last_stop: Option<StopNote>,
    pending: Option<StopWhat>,
}

impl Default for Debugger {
    fn default() -> Self {
        Debugger {
            org_id: None,
            current: None,
            previous: None,
            fate: Fate::Alive,
            breakpoints: BTreeSet::new(),
            space: 0,
            follow_ip: true,
            follow_offspring: false,
            thread: 0,
            sel: 0,
            top: 0,
            last_stop: None,
            pending: None,
        }
    }
}

/// Whether two views of one organism differ in anything a step can change.
fn same_state(a: &OrganismView, b: &OrganismView) -> bool {
    a.org_id == b.org_id
        && a.cpu.spaces == b.cpu.spaces
        && a.cpu.threads == b.cpu.threads
        && a.cpu.cur_thread == b.cpu.cur_thread
        && a.phenotype == b.phenotype
        && a.input_buf == b.input_buf
        && a.output_buf == b.output_buf
}

impl Debugger {
    /// Forgets the organism (a new selection). Breakpoints are positions of
    /// one organism's memory, so they go too.
    pub fn reset(&mut self) {
        *self = Debugger {
            follow_offspring: self.follow_offspring,
            ..Debugger::default()
        };
    }

    /// Takes in a frame captured with `cell` inspected.
    pub fn observe(&mut self, cell: i32, frame: &Frame) {
        let shown = frame.inspect.as_ref().filter(|o| o.cell == cell);
        match (self.org_id, shown) {
            (None, Some(o)) => {
                self.org_id = Some(o.org_id);
                self.current = Some(o.clone());
                self.previous = None;
                self.fate = Fate::Alive;
            }
            (None, None) => {}
            (Some(id), Some(o)) if o.org_id == id => {
                if self.current.as_ref().is_none_or(|c| !same_state(c, o)) {
                    self.previous = self.current.take();
                }
                self.current = Some(o.clone());
                self.fate = Fate::Alive;
            }
            (Some(_), Some(o)) => {
                let update = match self.fate {
                    Fate::Replaced { update, .. } => update,
                    _ => frame.update,
                };
                self.fate = Fate::Replaced {
                    by: o.org_id,
                    update,
                };
            }
            (Some(_), None) => {
                if self.fate == Fate::Alive {
                    self.fate = Fate::Died {
                        update: frame.update,
                    };
                }
            }
        }
        if let Some(c) = &self.current {
            if self.thread >= c.cpu.threads.len() {
                self.thread = c.cpu.cur_thread;
            }
        }
        if self.follow_ip {
            self.sync_to_ip();
        } else if self.space_view().is_none() {
            self.space = 0;
        }
    }

    /// Lists the space the shown thread's IP is in, at the IP's line.
    fn sync_to_ip(&mut self) {
        if let (Some(space), Some(pos)) = (self.ip_space(), self.ip()) {
            let known = self
                .current
                .as_ref()
                .is_some_and(|c| c.cpu.space(space).is_some());
            if let (true, Ok(pos)) = (known, usize::try_from(pos)) {
                self.space = space;
                self.sel = pos;
            }
        }
    }

    /// The listed memory space, if the organism has it.
    pub fn space_view(&self) -> Option<&MemSpaceView> {
        self.current.as_ref()?.cpu.space(self.space)
    }

    /// Re-aims the listing after the shown thread changed.
    pub fn thread_changed(&mut self) {
        if self.follow_ip {
            self.sync_to_ip();
        }
    }

    /// Lists the next memory space, leaving follow mode, with the line on
    /// the shown thread's IP when that is in the space, else on line 0.
    /// Returns the space now listed, `None` when there is only one.
    pub fn cycle_space(&mut self) -> Option<i32> {
        let spaces = &self.current.as_ref()?.cpu.spaces;
        if spaces.len() < 2 {
            return None;
        }
        let at = spaces.iter().position(|s| s.id == self.space).unwrap_or(0);
        self.space = spaces[(at + 1) % spaces.len()].id;
        self.follow_ip = false;
        self.sel = match (self.ip_space(), self.ip()) {
            (Some(s), Some(p)) if s == self.space => usize::try_from(p).unwrap_or(0),
            _ => 0,
        };
        self.top = 0;
        Some(self.space)
    }

    pub fn is_alive(&self) -> bool {
        self.fate == Fate::Alive && self.current.is_some()
    }

    /// The shown thread's index, clamped.
    pub fn thread_index(&self) -> usize {
        self.current.as_ref().map_or(0, |c| {
            self.thread.min(c.cpu.threads.len().saturating_sub(1))
        })
    }

    /// The shown thread and the same thread before the last change.
    pub fn threads(&self) -> (Option<&ThreadView>, Option<&ThreadView>) {
        let t = self.thread_index();
        let cur = self.current.as_ref().and_then(|c| c.cpu.threads.get(t));
        let prev = self
            .previous
            .as_ref()
            .filter(|p| Some(p.org_id) == self.org_id)
            .and_then(|p| p.cpu.threads.get(t));
        (cur, prev)
    }

    /// Per register of the shown thread: changed by the last step.
    pub fn changed_registers(&self) -> Vec<bool> {
        match self.threads() {
            (Some(cur), Some(prev)) => cur
                .regs
                .iter()
                .enumerate()
                .map(|(i, v)| prev.regs.get(i) != Some(v))
                .collect(),
            (Some(cur), None) => vec![false; cur.regs.len()],
            _ => Vec::new(),
        }
    }

    /// Per head of the shown thread: moved by the last step.
    pub fn moved_heads(&self) -> Vec<bool> {
        match self.threads() {
            (Some(cur), Some(prev)) => cur
                .heads
                .iter()
                .enumerate()
                .map(|(i, h)| prev.heads.get(i) != Some(h))
                .collect(),
            (Some(cur), None) => vec![false; cur.heads.len()],
            _ => Vec::new(),
        }
    }

    /// Whether site `pos` of the listed space changed with the last step.
    pub fn memory_changed(&self, pos: usize) -> bool {
        match (&self.current, &self.previous) {
            (Some(c), Some(p)) if Some(p.org_id) == self.org_id => {
                let cell = |o: &OrganismView| {
                    o.cpu
                        .space(self.space)
                        .and_then(|s| s.memory.get(pos).copied())
                };
                cell(c) != cell(p)
            }
            _ => false,
        }
    }

    /// Instructions the organism executed between its previous view and now
    /// (its lifetime CPU cycles, which a divide does not restart).
    pub fn cycles_since_previous(&self) -> Option<u64> {
        let c = self.current.as_ref()?;
        let p = self.previous.as_ref().filter(|p| p.org_id == c.org_id)?;
        Some(
            c.phenotype
                .lifetime_cpu_cycles
                .saturating_sub(p.phenotype.lifetime_cpu_cycles),
        )
    }

    /// The IP position of the shown thread (in the memory space it is in).
    pub fn ip(&self) -> Option<i32> {
        self.threads()
            .0
            .and_then(|t| t.heads.first())
            .map(|h| h.pos)
    }

    /// The memory space the shown thread's IP is in.
    pub fn ip_space(&self) -> Option<i32> {
        self.threads()
            .0
            .and_then(|t| t.heads.first())
            .map(|h| h.mem_space)
    }

    /// Sites of the listed memory space.
    pub fn sites(&self) -> usize {
        self.space_view().map_or(0, |s| s.memory.len())
    }

    /// Moves the selected line by `d`, leaving follow mode.
    pub fn move_sel(&mut self, d: i64) {
        let n = self.sites();
        self.follow_ip = false;
        if n > 0 {
            self.sel = (self.sel as i64 + d).clamp(0, n as i64 - 1) as usize;
        }
    }

    /// Jumps to the first or last line, leaving follow mode.
    pub fn set_sel(&mut self, pos: usize) {
        self.follow_ip = false;
        self.sel = pos.min(self.sites().saturating_sub(1));
    }

    /// Selects the instruction pointer's line and follows it.
    pub fn follow(&mut self) {
        self.follow_ip = true;
        self.sync_to_ip();
    }

    /// Toggles a breakpoint at the selected line; the new state, or `None`
    /// without a memory to put it in.
    pub fn toggle_breakpoint(&mut self) -> Option<bool> {
        if self.sites() == 0 {
            return None;
        }
        let site = (self.space, i32::try_from(self.sel).ok()?);
        Some(if self.breakpoints.remove(&site) {
            false
        } else {
            self.breakpoints.insert(site);
            true
        })
    }

    /// The command that runs the watched organism for `n` instructions.
    pub fn step_command(&mut self, cell: i32, n: u32) -> Option<RunUntil> {
        let org_id = self.org_id.filter(|_| self.is_alive())?;
        self.pending = Some(StopWhat::Step);
        Some(RunUntil {
            cell,
            org_id,
            stop: StopWhen::Cycles(n.max(1)),
            max_slices: STEP_BUDGET,
        })
    }

    /// The command that runs until the watched organism's IP reaches a
    /// breakpoint (`None` without breakpoints).
    pub fn breakpoint_command(&mut self, cell: i32) -> Option<RunUntil> {
        let org_id = self.org_id.filter(|_| self.is_alive())?;
        if self.breakpoints.is_empty() {
            return None;
        }
        self.pending = Some(StopWhat::Breakpoint);
        Some(RunUntil {
            cell,
            org_id,
            stop: StopWhen::IpIn(self.breakpoints.iter().copied().collect()),
            max_slices: BREAK_BUDGET,
        })
    }

    /// A run-until ended.
    pub fn until_ended(&mut self, outcome: UntilOutcome, slices: u64, update: i32) -> &StopNote {
        let what = self.pending.take().unwrap_or(StopWhat::Step);
        self.last_stop.insert(StopNote {
            outcome,
            slices,
            update,
            what,
        })
    }

    /// The session moved the watch to `org_id` in another cell (an
    /// offspring, or the organism itself moving).
    pub fn moved(&mut self, org_id: i32) {
        self.org_id = Some(org_id);
        self.current = None;
        self.previous = None;
        self.fate = Fate::Alive;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{frame_with_map, frame_with_org, organism};

    fn frame_of(id: i32, ax: i32, ip: i32, update: i32) -> Frame {
        Frame {
            update,
            ..frame_with_org(organism(id, ax, ip, 8))
        }
    }

    #[test]
    fn a_step_shows_as_a_difference_and_repeats_keep_it() {
        let mut d = Debugger::default();
        d.observe(3, &frame_of(5, 1, 0, 1));
        assert_eq!(d.org_id, Some(5));
        assert_eq!(d.changed_registers(), vec![false; 3]);
        d.observe(3, &frame_of(5, 2, 1, 1));
        assert_eq!(d.changed_registers(), vec![true, false, false]);
        assert_eq!(d.moved_heads(), vec![true, false, false, false]);
        assert_eq!(d.ip(), Some(1));
        assert_eq!(d.cycles_since_previous(), Some(1));
        // The same state again (a re-subscription) keeps the highlight.
        d.observe(3, &frame_of(5, 2, 1, 1));
        assert_eq!(d.changed_registers(), vec![true, false, false]);
    }

    #[test]
    fn the_selection_follows_the_ip_until_moved() {
        let mut d = Debugger::default();
        d.observe(3, &frame_of(5, 0, 2, 1));
        assert_eq!(d.sel, 2);
        d.move_sel(3);
        assert!(!d.follow_ip);
        d.observe(3, &frame_of(5, 0, 4, 1));
        assert_eq!(d.sel, 5, "no longer follows");
        d.follow();
        assert_eq!((d.sel, d.follow_ip), (4, true));
        d.move_sel(-100);
        assert_eq!(d.sel, 0);
        d.move_sel(100);
        assert_eq!(d.sel, 7, "clamped to the last site");
    }

    #[test]
    fn death_and_takeover_keep_the_last_view() {
        let mut d = Debugger::default();
        d.observe(3, &frame_of(5, 1, 0, 1));
        d.observe(
            3,
            &Frame {
                update: 4,
                ..frame_with_map()
            },
        );
        assert_eq!(d.fate, Fate::Died { update: 4 });
        assert_eq!(d.current.as_ref().map(|o| o.org_id), Some(5));
        assert!(!d.is_alive());
        assert!(
            d.step_command(3, 1).is_none(),
            "a dead organism does not step"
        );
        d.observe(3, &frame_of(9, 0, 0, 6));
        assert_eq!(d.fate, Fate::Replaced { by: 9, update: 6 });
        assert_eq!(d.current.as_ref().map(|o| o.org_id), Some(5));
    }

    #[test]
    fn commands_carry_the_organism_and_breakpoints() {
        let mut d = Debugger::default();
        d.observe(3, &frame_of(5, 1, 0, 1));
        assert!(d.breakpoint_command(3).is_none(), "no breakpoints");
        d.set_sel(2);
        assert_eq!(d.toggle_breakpoint(), Some(true));
        d.set_sel(7);
        d.toggle_breakpoint();
        d.toggle_breakpoint();
        let cmd = d.breakpoint_command(3).unwrap();
        assert_eq!((cmd.cell, cmd.org_id), (3, 5));
        assert_eq!(cmd.stop, StopWhen::IpIn(vec![(0, 2)]));
        let note = d.until_ended(UntilOutcome::Reached, 12, 3).clone();
        assert_eq!(note.text(), "Breakpoint reached after 12 time slices.");
        let step = d.step_command(3, 1).unwrap();
        assert_eq!(step.stop, StopWhen::Cycles(1));
        let note = d
            .until_ended(UntilOutcome::OrganismGone { occupant: Some(9) }, 1, 4)
            .clone();
        assert!(note.is_problem());
        assert_eq!(
            note.text(),
            "Organism #9 took over the cell at update 4 after 1 time slice."
        );
        d.reset();
        assert_eq!(d.org_id, None);
        assert!(
            d.breakpoints.is_empty(),
            "a new organism clears breakpoints"
        );
    }
}
