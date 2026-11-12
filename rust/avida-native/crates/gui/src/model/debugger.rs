//! The organism debugger: which organism is watched, what the last step
//! changed, breakpoints, and what happened to the organism.
//!
//! The session reports the inspected cell's occupant in every frame. The
//! debugger pins the organism (by `cOrganism::GetID()`) that the first
//! frame after a selection shows, keeps its latest view, and keeps the view
//! before the last change so a step can be shown as a difference. When the
//! cell empties or another organism moves in, the last view of the watched
//! organism stays on screen, marked as such, until the user moves on.

use std::collections::BTreeSet;

use avida_session::{Frame, RunUntil, StopWhen, UntilOutcome};
use avida_viewmodel::{OrganismView, ThreadView};

use crate::model::format;

/// Time slices a "step instruction" may take before giving up (the
/// scheduler gives every organism a slice well within this).
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

/// The views of the organism tab.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum OrgTab {
    #[default]
    Cpu,
    Phenotype,
    Tasks,
    Io,
}

impl OrgTab {
    pub const ALL: [OrgTab; 4] = [OrgTab::Cpu, OrgTab::Phenotype, OrgTab::Tasks, OrgTab::Io];

    pub fn title(self) -> &'static str {
        match self {
            OrgTab::Cpu => "CPU",
            OrgTab::Phenotype => "Phenotype",
            OrgTab::Tasks => "Tasks",
            OrgTab::Io => "I/O",
        }
    }
}

/// How the last run-until ended, for the debugger's status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopNote {
    pub outcome: UntilOutcome,
    pub slices: u64,
    pub update: i32,
    /// What was asked for.
    pub what: StopWhat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopWhat {
    Step,
    Breakpoint,
}

impl StopNote {
    /// One sentence for the status line.
    pub fn text(&self) -> String {
        let slices = format::plural(self.slices, "time slice");
        match (self.outcome, self.what) {
            (UntilOutcome::Reached, StopWhat::Step) => {
                format!("Stepped after {slices}.")
            }
            (UntilOutcome::Reached, StopWhat::Breakpoint) => {
                format!("Breakpoint reached after {slices}.")
            }
            (UntilOutcome::OrganismGone { occupant: None }, _) => format!(
                "The organism died at {} after {slices}.",
                format::update_phrase(self.update)
            ),
            (UntilOutcome::OrganismGone { occupant: Some(id) }, _) => format!(
                "Organism #{id} took over the cell at {} after {slices}.",
                format::update_phrase(self.update)
            ),
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
    /// The occupant after the watched organism was replaced.
    pub occupant: Option<OrganismView>,
    /// `(memory space, position)` sites to stop at.
    pub breakpoints: BTreeSet<(i32, i32)>,
    /// A memory space the user picked in the listing; `None` lists the
    /// space the shown thread's instruction pointer is in.
    pub pinned_space: Option<i32>,
    /// Keep the instruction pointer in view.
    pub follow_ip: bool,
    /// Move to each offspring (sent to the session).
    pub follow_offspring: bool,
    /// The thread shown.
    pub thread: usize,
    pub tab: OrgTab,
    /// How the last run-until ended.
    pub last_stop: Option<StopNote>,
    /// What the run-until in flight was asked for.
    pending: Option<StopWhat>,
    /// The `(memory space, position)` the memory listing last scrolled to.
    pub scrolled_to: Option<(i32, i32)>,
    /// The memory listing's scroll offset and height last frame.
    pub listing: (f32, f32),
}

impl Default for Debugger {
    fn default() -> Self {
        Debugger {
            org_id: None,
            current: None,
            previous: None,
            fate: Fate::Alive,
            occupant: None,
            breakpoints: BTreeSet::new(),
            pinned_space: None,
            follow_ip: true,
            follow_offspring: false,
            thread: 0,
            tab: OrgTab::Cpu,
            last_stop: None,
            pending: None,
            scrolled_to: None,
            listing: (0.0, 0.0),
        }
    }
}

/// Where a listing of `row_h`-high rows, scrolled to `offset` and `height`
/// high, should scroll so row `row` is in view with a row of margin:
/// `None` when it already is (no jump while stepping through visible
/// code), otherwise an offset that puts the row a third of the way down.
pub fn scroll_target(row: usize, row_h: f32, offset: f32, height: f32) -> Option<f32> {
    let top = row as f32 * row_h;
    let bottom = top + row_h;
    if height <= 0.0 || (top >= offset + row_h && bottom <= offset + height - row_h) {
        return None;
    }
    if top >= offset && bottom <= offset + height && height < 3.0 * row_h {
        return None;
    }
    Some((top - height / 3.0).max(0.0))
}

/// Whether two views of the same organism differ in anything a step can
/// change (memory, threads, phenotype, buffers).
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
    /// Forgets the organism (a new selection); breakpoints and display
    /// choices stay unless `clear_breakpoints`.
    pub fn reset(&mut self, clear_breakpoints: bool) {
        let keep = Debugger {
            breakpoints: if clear_breakpoints {
                BTreeSet::new()
            } else {
                std::mem::take(&mut self.breakpoints)
            },
            follow_ip: self.follow_ip,
            follow_offspring: self.follow_offspring,
            tab: self.tab,
            ..Debugger::default()
        };
        *self = keep;
    }

    /// The debugger of a new session: nobody watched, nothing followed;
    /// the display choices stay.
    pub fn for_new_session(&self) -> Debugger {
        Debugger {
            tab: self.tab,
            follow_ip: self.follow_ip,
            ..Debugger::default()
        }
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
                self.occupant = None;
            }
            (None, None) => {}
            (Some(id), Some(o)) if o.org_id == id => {
                let changed = self.current.as_ref().is_none_or(|c| !same_state(c, o));
                if changed {
                    self.previous = self.current.take();
                }
                self.current = Some(o.clone());
                self.fate = Fate::Alive;
                self.occupant = None;
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
                self.occupant = Some(o.clone());
            }
            (Some(_), None) => {
                if self.fate == Fate::Alive {
                    self.fate = Fate::Died {
                        update: frame.update,
                    };
                }
                self.occupant = None;
            }
        }
        if let Some(c) = &self.current {
            if self.thread >= c.cpu.threads.len() {
                self.thread = c.cpu.cur_thread;
            }
        }
    }

    pub fn is_alive(&self) -> bool {
        self.fate == Fate::Alive && self.current.is_some()
    }

    /// The thread shown, clamped.
    pub fn thread_index(&self) -> usize {
        self.current.as_ref().map_or(0, |c| {
            self.thread.min(c.cpu.threads.len().saturating_sub(1))
        })
    }

    /// The thread shown and the same thread before the last change.
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

    /// Per stack of the shown thread, per depth: changed by the last step.
    pub fn changed_stacks(&self) -> Vec<Vec<bool>> {
        match self.threads() {
            (Some(cur), prev) => cur
                .stacks
                .iter()
                .enumerate()
                .map(|(s, stack)| {
                    stack
                        .iter()
                        .enumerate()
                        .map(|(d, v)| {
                            prev.is_some_and(|p| {
                                p.stacks.get(s).and_then(|ps| ps.get(d)) != Some(v)
                            })
                        })
                        .collect()
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Whether memory site `pos` changed (instruction or flags) with the
    /// last step.
    pub fn memory_changed(&self, space: i32, pos: usize) -> bool {
        match (&self.current, &self.previous) {
            (Some(c), Some(p)) if Some(p.org_id) == self.org_id => {
                let cell =
                    |o: &OrganismView| o.cpu.space(space).and_then(|s| s.memory.get(pos).copied());
                cell(c) != cell(p)
            }
            _ => false,
        }
    }

    /// Instructions the organism executed between its previous view and
    /// the current one (what the last step ran), from its lifetime CPU
    /// cycles, which a divide does not restart.
    pub fn cycles_since_previous(&self) -> Option<u64> {
        let c = self.current.as_ref()?;
        let p = self.previous.as_ref().filter(|p| p.org_id == c.org_id)?;
        Some(
            c.phenotype
                .lifetime_cpu_cycles
                .saturating_sub(p.phenotype.lifetime_cpu_cycles),
        )
    }

    /// The IP position of the shown thread (in the space it is in).
    pub fn ip(&self) -> Option<i32> {
        self.ip_site().map(|(_, p)| p)
    }

    /// `(memory space, position)` of the shown thread's instruction pointer.
    pub fn ip_site(&self) -> Option<(i32, i32)> {
        self.threads()
            .0
            .and_then(|t| t.heads.first())
            .map(|h| (h.mem_space, h.pos))
    }

    /// The memory space the listing shows: the one the user picked, else
    /// the one the shown thread's IP is in (space 0 when unknown).
    pub fn listed_space(&self) -> i32 {
        let known = |s: i32| {
            self.current
                .as_ref()
                .is_some_and(|c| c.cpu.space(s).is_some())
        };
        self.pinned_space
            .filter(|&s| known(s))
            .or_else(|| self.ip_site().map(|(s, _)| s).filter(|&s| known(s)))
            .unwrap_or(0)
    }

    /// Lists memory space `id`, or goes back to following the IP's space
    /// with `None`.
    pub fn pin_space(&mut self, id: Option<i32>) {
        self.pinned_space = id;
        self.scrolled_to = None;
    }

    pub fn toggle_breakpoint(&mut self, space: i32, pos: i32) {
        if !self.breakpoints.remove(&(space, pos)) {
            self.breakpoints.insert((space, pos));
        }
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
        self.occupant = None;
        self.scrolled_to = None;
    }

    /// Starts watching the organism that replaced the watched one.
    pub fn watch_occupant(&mut self) {
        if let Some(o) = self.occupant.take() {
            self.org_id = Some(o.org_id);
            self.current = Some(o);
            self.previous = None;
            self.fate = Fate::Alive;
            self.scrolled_to = None;
            self.last_stop = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use avida_viewmodel::{CpuView, HeadView, MemCell, MemSpaceView, PhenotypeView};

    use super::*;
    use crate::model::fixtures::frame_with_map;

    fn org(id: i32, ax: i32, ip: i32) -> OrganismView {
        OrganismView {
            cell: 3,
            x: 0,
            y: 1,
            org_id: id,
            genotype_id: 7,
            genotype_name: "100-aaaaa".into(),
            lineage_label: 0,
            faced_cell: None,
            phenotype: PhenotypeView {
                merit: 1.0,
                fitness: 1.0,
                cur_bonus: 1.0,
                gestation_time: 0,
                age: 0,
                generation: 0,
                genome_length: 3,
                copied_size: 0,
                executed_size: 0,
                cpu_cycles_used: ip,
                lifetime_cpu_cycles: u64::try_from(ip).unwrap_or(0),
                time_used: 0,
                num_divides: 0,
                birth_update: 0,
                parent_true: true,
                is_modified: false,
                is_multi_thread: false,
                to_die: false,
                cur_task_count: Vec::new(),
                last_task_count: Vec::new(),
                cur_reaction_count: Vec::new(),
                last_reaction_count: Vec::new(),
            },
            cell_inputs: Vec::new(),
            input_buf: Vec::new(),
            output_buf: Vec::new(),
            cpu: CpuView {
                inst_set: "x".into(),
                hw_type: 0,
                memory: vec![
                    MemCell {
                        op: 0,
                        sym: 'a',
                        name_idx: 0,
                        flags: 0,
                    };
                    3
                ],
                spaces: vec![MemSpaceView {
                    id: 0,
                    label: "genome".into(),
                    memory: vec![
                        MemCell {
                            op: 0,
                            sym: 'a',
                            name_idx: 0,
                            flags: 0,
                        };
                        3
                    ],
                }],
                inst_names: Arc::from(vec!["nop-A".to_string()]),
                threads: vec![ThreadView {
                    id: 0,
                    regs: vec![ax, 0, 0],
                    heads: vec![HeadView {
                        pos: ip,
                        mem_space: 0,
                    }],
                    stacks: vec![vec![0; 10]],
                    active_stack: 0,
                    read_label: String::new(),
                    next_label: String::new(),
                    status: String::new(),
                }],
                head_names: vec!["IP".to_string()],
                cur_thread: 0,
            },
        }
    }

    fn frame(o: Option<OrganismView>, update: i32) -> Frame {
        Frame {
            inspect: o,
            update,
            ..frame_with_map()
        }
    }

    #[test]
    fn a_step_is_shown_as_a_difference_and_repeats_keep_it() {
        let mut d = Debugger::default();
        d.observe(3, &frame(Some(org(5, 1, 0)), 1));
        assert_eq!(d.org_id, Some(5));
        assert_eq!(d.changed_registers(), vec![false; 3]);
        d.observe(3, &frame(Some(org(5, 2, 1)), 1));
        assert_eq!(d.changed_registers(), vec![true, false, false]);
        assert_eq!(d.moved_heads(), vec![true]);
        assert_eq!(d.ip(), Some(1));
        assert_eq!(d.cycles_since_previous(), Some(1));
        // A frame with the same state (a re-subscription) keeps the
        // highlight.
        d.observe(3, &frame(Some(org(5, 2, 1)), 1));
        assert_eq!(d.changed_registers(), vec![true, false, false]);
    }

    #[test]
    fn the_listing_scrolls_only_when_the_ip_leaves_the_view() {
        // 10 rows of 20 points visible from the top.
        assert_eq!(scroll_target(3, 20.0, 0.0, 200.0), None);
        assert_eq!(
            scroll_target(9, 20.0, 0.0, 200.0),
            Some(180.0 - 200.0 / 3.0)
        );
        assert_eq!(scroll_target(0, 20.0, 100.0, 200.0), Some(0.0));
        assert_eq!(scroll_target(50, 20.0, 0.0, 0.0), None, "not laid out yet");
    }

    #[test]
    fn death_and_takeover_keep_the_last_view() {
        let mut d = Debugger::default();
        d.observe(3, &frame(Some(org(5, 1, 0)), 1));
        d.observe(3, &frame(None, 4));
        assert_eq!(d.fate, Fate::Died { update: 4 });
        assert_eq!(d.current.as_ref().map(|o| o.org_id), Some(5));
        assert!(!d.is_alive());
        assert!(
            d.step_command(3, 1).is_none(),
            "a dead organism does not step"
        );
        d.observe(3, &frame(Some(org(9, 0, 0)), 6));
        // Died first: the takeover is reported with the newcomer.
        assert_eq!(d.fate, Fate::Replaced { by: 9, update: 6 });
        assert_eq!(d.current.as_ref().map(|o| o.org_id), Some(5));
        d.watch_occupant();
        assert_eq!((d.org_id, d.is_alive()), (Some(9), true));
    }

    #[test]
    fn commands_carry_the_organism_and_breakpoints() {
        let mut d = Debugger::default();
        d.observe(3, &frame(Some(org(5, 1, 0)), 1));
        assert!(d.breakpoint_command(3).is_none(), "no breakpoints");
        d.toggle_breakpoint(0, 2);
        d.toggle_breakpoint(0, 7);
        d.toggle_breakpoint(0, 7);
        let cmd = d.breakpoint_command(3).unwrap();
        assert_eq!(cmd.org_id, 5);
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
        d.reset(false);
        assert_eq!(d.org_id, None);
        assert!(d.breakpoints.contains(&(0, 2)));
        d.reset(true);
        assert!(d.breakpoints.is_empty());
    }
}
