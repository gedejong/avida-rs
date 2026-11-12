//! Deme cell events, deme predicates and the deme network
//! (avida-core/source/main at 47f13dadb, as built into the reference
//! binary):
//!
//! * `cDemeCellEvent` (cDemeCellEvent.{h,cc}), `cDeme::SetCellEvent`,
//!   `SetCellEventSlots`, `KillCellEvent`, `GetSlotFlowRate` and the event
//!   loop of `cDeme::ProcessUpdate` (cDeme.cc);
//! * `cDemeResourceThresholdPredicate` (cDemePredicate.h) and the movement
//!   predicates of cOrgMovementPredicate.h, with `cDeme::Add*Pred`,
//!   `cPopulation::AddDemePred` and `cStats::Move`;
//! * `cDemeNetwork` (cDemeNetwork.cc). The reference binary is built
//!   without Boost (`BOOST_IS_AVAILABLE 0`; the Boost branch of
//!   `DemeNetworkFactory` does not even compile, its `deme` parameter is
//!   unnamed), so `cDeme::GetNetwork()` prints an error and calls
//!   `Driver().Abort(INTERNAL_ERROR)`, i.e. `exit(50)`, on first use. Every
//!   network instruction and action does that here too; the topology
//!   network of cDemeTopologyNetwork.cc is never built.
//!
//! The C++ leaves two members of `cDemeCellEvent` uninitialised:
//! `m_use_gradient`, read by `IsDecayed()` every time an event starts, and
//! `m_eventID`, read before the first activation by the movement tasks.
//! Both hold stale stack contents that vary with the configuration and,
//! under address randomisation, from run to run (`m_eventID` was the low
//! half of a different pointer in each run), so both reads stop the run
//! here with an `unsupported:` error (see [`CellEvent::is_decayed`]). The
//! message predicates `cDeme` can create
//! (`EventReceivedCenter`, `EventReceivedLeftSide`) are not reachable from
//! any action and are not ported.

use std::collections::BTreeSet;

use avida_apto::AvidaRng;
use avida_hardware::organism::OrgState;

use crate::stat_tools::DoubleSum;
use crate::world::World;

/// What `cDemeNetwork::DemeNetworkFactory` prints (through
/// `Feedback().Error`) before aborting.
pub const DEME_NETWORK_ERROR: &str = "cDemeNetwork requires Boost; if Boost *is* available, \
#define BOOST_IS_AVAILABLE in defs.h, and (possibly) update your include path.";

/// `Avida::INTERNAL_ERROR`
const INTERNAL_ERROR: i32 = 50;

/// `cDemeCellEvent`. The owning deme (`m_deme`) is the deme that holds the
/// event; `m_iter` is not needed because every C++ loop over
/// `GetNextEventCellID()` runs to its end. `m_dead` and `m_id_set` are
/// never set (`Terminate()` and `SetEventID()` have no callers), so they
/// are left out.
#[derive(Debug, Clone, PartialEq)]
pub struct CellEvent {
    /// `m_event_cells`: deme-relative cell ids.
    event_cells: Vec<i32>,
    /// `m_eventID`; `None` until the first `ActivateEvent()` (the C++
    /// leaves it uninitialised).
    event_id: Option<i32>,
    /// `m_delay`
    delay: i32,
    /// `m_duration`
    duration: i32,
    /// `m_deme_width`, `m_deme_height`
    deme_width: i32,
    deme_height: i32,
    /// `m_event_width`, `m_event_height`
    event_width: i32,
    event_height: i32,
    /// `m_active`
    active: bool,
    /// `m_static_pos`
    static_pos: bool,
    /// `center`: (bottom-left, top-right); zero until set.
    center: ((i32, i32), (i32, i32)),
}

impl CellEvent {
    /// `cDemeCellEvent(x1, y1, x2, y2, delay, duration, deme_width,
    /// deme_height, static_pos, deme, world)`; `max_age` is
    /// `DEMES_MAX_AGE`.
    #[allow(clippy::too_many_arguments)]
    fn new(
        rect: [i32; 4],
        delay: i32,
        duration: i32,
        deme_width: i32,
        deme_height: i32,
        static_pos: bool,
        max_age: i32,
        rng: &mut AvidaRng,
    ) -> Result<CellEvent, String> {
        let [x1, y1, x2, y2] = rect;
        // The asserts on the rectangle are compiled out of the reference
        // build; `m_event_cells.ResizeClear((x2-x1+1) * (y2-y1+1))`.
        let n = (x2 - x1 + 1).wrapping_mul(y2 - y1 + 1);
        if n < 0 || x2 < x1 - 1 || y2 < y1 - 1 {
            return Err(format!(
                "unsupported: a deme cell event with the rectangle ({x1},{y1})-({x2},{y2}) \
                 (C++: std::vector::assign of a negative size)"
            ));
        }
        let mut ev = CellEvent {
            event_cells: vec![0; n as usize],
            event_id: None,
            delay,
            duration,
            deme_width,
            deme_height,
            event_width: x2 - x1,
            event_height: y2 - y1,
            active: false,
            static_pos,
            center: ((0, 0), (0, 0)),
        };
        if ev.delay == -1 {
            ev.delay = rng.get_int(max_age - duration);
        }
        if static_pos {
            let mut i = 0;
            for j in y1..=y2 {
                for k in x1..=x2 {
                    ev.event_cells[i] = j * deme_width + k;
                    i += 1;
                }
            }
            ev.center = center_of(x2 - x1, y2 - y1);
        }
        Ok(ev)
    }

    /// `ActivateEvent()`. QUIRK: a moving event draws its position twice;
    /// the cells come from the second draw, the centre from the first.
    fn activate(&mut self, rng: &mut AvidaRng) {
        self.event_id = Some(rng.get_int(0x7FFF_FFFF));
        if !self.static_pos {
            self.place(rng);
            self.center = center_of(self.event_width, self.event_height);
            self.place(rng);
        }
        self.active = true;
    }

    /// One random placement of a moving event.
    fn place(&mut self, rng: &mut AvidaRng) {
        let rand_x1 = rng.get_int(self.deme_width - self.event_width);
        let rand_y1 = rng.get_int(self.deme_height - self.event_height);
        let mut i = 0;
        for j in rand_y1..=rand_y1 + self.event_height {
            for k in rand_x1..=rand_x1 + self.event_width {
                self.event_cells[i] = j * self.deme_width + k;
                i += 1;
            }
        }
    }

    /// The ids `GetNextEventCellID()` hands out: all event cells while the
    /// event is active, none otherwise.
    fn live_cells(&self) -> Vec<i32> {
        if self.active {
            self.event_cells.clone()
        } else {
            Vec::new()
        }
    }

    /// `IsDecayed()`, i.e. `m_use_gradient`, which the constructor never
    /// initialises (`DecayEventIDFromCenter()` is only called from
    /// commented-out code). `cDeme::SetCellEvent` and `SetCellEventSlots`
    /// build the event in a stack temporary and copy it into the deme, so
    /// the flag is whatever byte that stack slot last held, and the
    /// reference binary's `ProcessUpdate` takes the decayed branch only
    /// when the byte is exactly 1 (`ldrb; cmp #1; b.ne`). Under lldb the
    /// byte was 1 in some configurations (the high half of a pointer into
    /// the binary image), 0 in others (`DelayedDemeEvents` after a
    /// `ReplicateDemes` in the same update, one `DelayedDemeEventsPerSlots`
    /// set-up), and in others a byte of a heap pointer that changed from
    /// run to run with address randomisation (7, 9, 11, 12). Which branch
    /// runs depends on the calls before the event was created and on the
    /// process's address layout, so the port stops.
    fn is_decayed(&self) -> Result<bool, String> {
        Err(
            "unsupported: a deme cell event starts (C++: cDemeCellEvent::IsDecayed reads \
             the uninitialised m_use_gradient, a stale stack byte that differs between \
             configurations and between runs)"
                .to_string(),
        )
    }

    /// `GetEventIDDecay(pos)`
    fn event_id_decay(&self, pos: (i32, i32)) -> i32 {
        let maxd = ((f64::from(self.deme_width) - 1.0).powf(2.0)
            + (f64::from(self.deme_height) - 1.0).powf(2.0))
        .sqrt();
        let top = maxd.ceil();
        let (cx, cy) = self.center_point();
        let cdistance =
            ((f64::from(pos.0) - cx).powf(2.0) + (f64::from(pos.1) - cy).powf(2.0)).sqrt();
        linmap(cdistance, 0.0, top, top * 1.414, 1.0).floor() as i32
    }

    /// `GetCenter()`
    fn center_point(&self) -> (f64, f64) {
        let ((a, b), (c, d)) = self.center;
        (f64::from(a + c) / 2.0, f64::from(b + d) / 2.0)
    }

    /// `InCenter(pos)`. QUIRK: the pairs compare lexicographically, and
    /// the centre of a static event is measured from (0,0), not from its
    /// corner.
    fn in_center(&self, pos: (i32, i32)) -> bool {
        self.center.0 <= pos && pos <= self.center.1
    }

    /// `OnEventCell(rel_cell_id)`
    fn on_event_cell(&self, rel: i32) -> bool {
        self.event_cells.contains(&rel)
    }

    /// `ConfineToTimeSlot(min_delay, max_delay)`
    fn confine_to_time_slot(&mut self, min_delay: i32, max_delay: i32) -> Result<(), String> {
        let span = max_delay - min_delay;
        if span == 0 {
            return Err(
                "unsupported: DelayedDemeEventsPerSlots with an empty time slot (C++: modulus \
                 by zero)"
                    .to_string(),
            );
        }
        self.delay = self.delay % span + min_delay;
        Ok(())
    }

    /// `GetEventID()` where the C++ may read it before the event ever
    /// started.
    fn id_read(&self) -> Result<i32, String> {
        self.event_id.ok_or_else(|| {
            "unsupported: the id of a deme cell event that never started is read (C++: \
             cDemeCellEvent::m_eventID is uninitialised)"
                .to_string()
        })
    }
}

/// The centre pair the C++ computes from an event's width and height.
fn center_of(w: i32, h: i32) -> ((i32, i32), (i32, i32)) {
    let (w, h) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
    (
        (w.floor() as i32, h.floor() as i32),
        (w.ceil() as i32, h.ceil() as i32),
    )
}

/// `cDemeCellEvent::linmap`
fn linmap(dp: f64, ds: f64, de: f64, rs: f64, re: f64) -> f64 {
    if de - ds == 0.0 {
        0.0
    } else {
        (dp - ds) * ((re - rs) / (de - ds)) + rs
    }
}

/// `cDemeResourceThresholdPredicate`
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceThresholdPred {
    /// `demeResourceName`
    resource_name: String,
    /// `compareOperator`
    op: String,
    /// `resourceThresholdValue`; `None` when the action had fewer than
    /// three arguments (the C++ member is then uninitialised).
    threshold: Option<f64>,
    /// `previouslySatisfied`
    previously_satisfied: bool,
}

/// The state of one movement predicate (cOrgMovementPredicate.h).
#[derive(Debug, Clone, PartialEq)]
pub enum MovementKind {
    /// `cOrgMovementPred_EventMovedIntoCenter`
    MovedIntoCenter {
        event: usize,
        total_times: i32,
        current_times: i32,
    },
    /// `cOrgMovementPred_EventMovedBetweenTargets`
    MovedBetweenTargets {
        events: Vec<usize>,
        total_times: u32,
        total_orgs: u32,
        event_success: Vec<BTreeSet<i32>>,
        successful_orgs: BTreeSet<i32>,
    },
    /// `cOrgMovementPred_EventNUniqueIndividualsMovedIntoTarget`
    NUnique {
        event: usize,
        total_individuals: u32,
        unique_org_ids: BTreeSet<i32>,
    },
}

/// A movement predicate: its kind and `m_event_received` / `m_satisfied`.
/// `m_stats_updated` is left out: only `UpdateStats` reads it, and the
/// body that would use it is commented out.
#[derive(Debug, Clone, PartialEq)]
pub struct MovementPred {
    kind: MovementKind,
    satisfied: bool,
}

impl MovementPred {
    /// `Reset()`
    fn reset(&mut self) {
        self.satisfied = false;
        match &mut self.kind {
            MovementKind::MovedIntoCenter { current_times, .. } => *current_times = 0,
            MovementKind::MovedBetweenTargets {
                events,
                event_success,
                successful_orgs,
                ..
            } => {
                *event_success = vec![BTreeSet::new(); events.len()];
                successful_orgs.clear();
            }
            MovementKind::NUnique { unique_org_ids, .. } => unique_org_ids.clear(),
        }
    }

    /// The events `GetEvent(0)` and `GetEvent(1)` return (the
    /// single-event predicates return theirs for any index).
    fn event_at(&self, j: usize) -> Option<usize> {
        match &self.kind {
            MovementKind::MovedIntoCenter { event, .. } | MovementKind::NUnique { event, .. } => {
                Some(*event)
            }
            MovementKind::MovedBetweenTargets { events, .. } => events.get(j).copied(),
        }
    }
}

/// The cell event and predicate state of one `cDeme`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DemeEvents {
    /// `cell_events`
    pub cell_events: Vec<CellEvent>,
    /// The capacity of the C++ `std::vector<cDemeCellEvent>` (libc++
    /// growth): predicates hold pointers into it.
    capacity: usize,
    /// `event_slot_end_points`: (slot end age, flow level)
    pub slot_end_points: Vec<(i32, i32)>,
    /// `deme_pred_list`
    pub deme_preds: Vec<ResourceThresholdPred>,
    /// `movement_pred_list`
    pub movement_preds: Vec<MovementPred>,
}

impl DemeEvents {
    /// `cell_events.Push(event)`. The movement predicates keep raw
    /// pointers to the events; a push that makes the vector reallocate
    /// leaves them dangling.
    fn push(&mut self, ev: CellEvent) -> Result<(), String> {
        let len = self.cell_events.len();
        if len == self.capacity {
            if !self.movement_preds.is_empty() {
                return Err(
                    "unsupported: a deme cell event added after a movement predicate (C++: \
                     the predicate's cDemeCellEvent pointer dangles once the event vector \
                     reallocates)"
                        .to_string(),
                );
            }
            self.capacity = (2 * self.capacity).max(len + 1);
        }
        self.cell_events.push(ev);
        Ok(())
    }

    /// The predicate part of `cDeme::Reset`.
    pub fn reset_predicates(&mut self) {
        for p in &mut self.deme_preds {
            p.previously_satisfied = false;
        }
        for p in &mut self.movement_preds {
            p.reset();
        }
    }

    /// `GetSlotFlowRate()` at deme age `age`.
    pub fn slot_flow_rate(&self, age: i32) -> i32 {
        self.slot_end_points
            .iter()
            .find(|&&(end, _)| age <= end)
            .map_or(0, |&(_, flow)| flow)
    }

    /// `DemePredSatisfiedPreviously()` (`UpdateStats` is a no-op).
    pub fn deme_pred_satisfied_previously(&self) -> bool {
        self.deme_preds.iter().any(|p| p.previously_satisfied)
    }

    /// `MovPredSatisfiedPreviously()` (`UpdateStats` has no effect).
    pub fn mov_pred_satisfied_previously(&self) -> bool {
        self.movement_preds.iter().any(|p| p.satisfied)
    }

    /// `GetEventID()` of every event (`GetCellEvent(i)`), `None` where it
    /// is still uninitialised: for the movement tasks.
    pub fn event_ids(&self) -> Vec<Option<i32>> {
        self.cell_events.iter().map(|e| e.event_id).collect()
    }

    /// `(GetEventID(), IsActive())` of `GetMovPredicate(i)->GetEvent(j)`
    /// for j = 0 and 1, `None` beyond the predicate's events.
    pub fn mov_pred_events(&self) -> Vec<MovPredEvents> {
        self.movement_preds
            .iter()
            .map(|p| {
                let at = |j| {
                    p.event_at(j).map(|e| {
                        let ev = &self.cell_events[e];
                        (ev.event_id, ev.active)
                    })
                };
                [at(0), at(1)]
            })
            .collect()
    }
}

/// What a task sees of `GetMovPredicate(i)->GetEvent(j)` for j = 0 and 1:
/// `(GetEventID(), IsActive())`, the id `None` while uninitialised, and
/// `None` past the end of the predicate's event array.
pub type MovPredEvents = [Option<(Option<i32>, bool)>; 2];

/// The kinds of `cPopulation::AddDemePred` the actions use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementPredType {
    /// `"EventMovedIntoCenter"`
    MovedIntoCenter,
    /// `"EventMovedBetweenTargets"`
    MovedBetweenTargets,
    /// `"EventNUniqueIndividualsMovedIntoTarget"`
    NUnique,
}

/// The arguments of `cDeme::SetCellEvent` / `SetCellEventSlots`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSpec {
    pub rect: [i32; 4],
    pub delay: i32,
    pub duration: i32,
    pub static_position: bool,
}

impl World {
    /// `deme.GetNetwork()` in the reference build:
    /// `Feedback().Error(...)` and `Abort(INTERNAL_ERROR)`.
    pub(crate) fn deme_network_abort(&mut self) {
        self.abort_run(
            &format!("error: {DEME_NETWORK_ERROR}\n"),
            INTERNAL_ERROR,
            "cDemeNetwork requires Boost (C++: Abort(INTERNAL_ERROR))",
        );
    }

    /// `cDeme::SetCellEvent(..., total_events)` on deme `d`.
    pub(crate) fn deme_set_cell_event(
        &mut self,
        d: usize,
        spec: EventSpec,
        total_events: i32,
    ) -> Result<(), String> {
        let (width, height) = (self.pop.demes[d].width, self.pop.demes[d].height());
        let max_age = self.cfg.demes_max_age;
        for _ in 0..total_events {
            let ev = CellEvent::new(
                spec.rect,
                spec.delay,
                spec.duration,
                width,
                height,
                spec.static_position,
                max_age,
                &mut self.rng,
            )?;
            self.pop.demes[d].events.push(ev)?;
        }
        Ok(())
    }

    /// `cDeme::SetCellEventSlots(...)` on deme `d`: `slots` is
    /// `[total_slots, events_per_slot_max, events_per_slot_min,
    /// event_flow_levels]`.
    pub(crate) fn deme_set_cell_event_slots(
        &mut self,
        d: usize,
        spec: EventSpec,
        slots: [i32; 4],
    ) -> Result<(), String> {
        let [total_slots, max_events, min_events, levels] = slots;
        if levels == 1 {
            return Err(
                "unsupported: DelayedDemeEventsPerSlots with one event flow level (C++: \
                 integer division by zero; the arm64 reference binary then loops forever)"
                    .to_string(),
            );
        }
        if total_slots == 0 {
            return Err(
                "unsupported: DelayedDemeEventsPerSlots with no slots (C++: integer division \
                 by zero)"
                    .to_string(),
            );
        }
        let increment = (max_events - min_events) / (levels - 1);
        let max_age = self.cfg.demes_max_age;
        let slot_length = max_age / total_slots;
        let (width, height) = (self.pop.demes[d].width, self.pop.demes[d].height());
        for i in 0..total_slots {
            // number of events during this slot
            let slot_flow_level = increment * self.rng.get_int(levels) + min_events;
            let slot_delay = i * slot_length;
            // the last slot is never reached: it is == to MAX_AGE
            self.pop.demes[d]
                .events
                .slot_end_points
                .push((slot_delay + slot_length, slot_flow_level));
            for _ in 0..slot_flow_level {
                let mut ev = CellEvent::new(
                    spec.rect,
                    spec.delay,
                    spec.duration,
                    width,
                    height,
                    spec.static_position,
                    max_age,
                    &mut self.rng,
                )?;
                ev.confine_to_time_slot(slot_delay, slot_delay + slot_length)?;
                self.pop.demes[d].events.push(ev)?;
            }
        }
        // setup stats.flow_rate_tuples
        if min_events <= max_events && increment <= 0 {
            return Err(
                "unsupported: DelayedDemeEventsPerSlots whose flow levels do not increase \
                 (C++: the loop over flow rates never ends)"
                    .to_string(),
            );
        }
        let mut i = min_events;
        while i <= max_events {
            let t = self.stats.flow_rate_tuples.entry(i).or_default();
            t.org_count.clear();
            t.events_killed.clear();
            t.attempts_to_kill_events.clear();
            t.avg_energy_usage_ratio.clear();
            t.total_births.clear();
            t.current_sleeping.clear();
            i += increment;
        }
        Ok(())
    }

    /// `cDeme::KillCellEvent(eventID)` on deme `d`: whether an active
    /// event with that id was stopped. `running` is the cell of the
    /// organism being run and `org` its state, which clearing that cell's
    /// data reads (`cPopulationCell::SetCellData` records the occupant's
    /// opinion and forage target).
    pub fn deme_kill_cell_event(
        &mut self,
        d: usize,
        event_id: i32,
        running: Option<i32>,
        org: &mut OrgState,
    ) -> Result<bool, String> {
        self.pop.demes[d].event_kill_attempts_this_slot += 1;
        if event_id <= 0 {
            return Ok(false);
        }
        let Some(i) = self.pop.demes[d]
            .events
            .cell_events
            .iter()
            .position(|e| e.active && e.event_id == Some(event_id))
        else {
            return Ok(false);
        };
        // remove event ID from all cells
        let cells = self.pop.demes[d].events.cell_events[i].live_cells();
        for rel in cells {
            let abs = self.deme_event_cell(d, rel)?;
            if self.pop.cells[abs as usize].cell_data.contents == event_id {
                let update = self.stats.update();
                if running == Some(abs) {
                    self.set_running_cell_data(abs, 0, -1, update, org);
                } else {
                    self.set_cell_data(abs, 0, -1, update);
                }
            }
        }
        let deme = &mut self.pop.demes[d];
        deme.events.cell_events[i].active = false;
        deme.events_killed += 1;
        deme.events_killed_this_slot += 1;
        deme.event_kill_attempts += 1;
        Ok(true)
    }

    /// `GetCellID(rel)` for an event cell: the C++ indexes `cell_ids`
    /// without a check.
    fn deme_event_cell(&self, d: usize, rel: i32) -> Result<i32, String> {
        usize::try_from(rel)
            .ok()
            .and_then(|r| self.pop.demes[d].cell_ids.get(r))
            .copied()
            .ok_or_else(|| {
                format!(
                    "unsupported: a deme cell event covers cell {rel}, outside its deme (C++: \
                     out-of-bounds cDeme::GetCellID)"
                )
            })
    }

    /// `cDeme::ProcessUpdate(ctx)` for deme `d`.
    pub(crate) fn deme_process_update(&mut self, d: usize) -> Result<(), String> {
        // test deme predicate
        for p in 0..self.pop.demes[d].events.deme_preds.len() {
            self.deme_eval_resource_threshold(d, p)?;
            if self.action_state.exit_code.is_some() {
                return Ok(());
            }
        }

        let mut energy_usage = DoubleSum::new();
        if self.pop.demes[d].is_empty() {
            // deme is not processed if no organisms are present
            self.pop.demes[d].total_energy_testament = 0.0;
            return Ok(());
        }
        if self.cfg.energy_enabled {
            for i in 0..self.pop.demes[d].size() {
                let cell = self.pop.demes[d].cell_id(i);
                if let Some(o) = self.pop.orgs[cell as usize].as_ref() {
                    energy_usage.add(o.state.phenotype.execution_ratio);
                }
            }
        }

        let age = self.pop.demes[d].age;
        for i in 0..self.pop.demes[d].events.cell_events.len() {
            let ev = &self.pop.demes[d].events.cell_events[i];
            if ev.active && ev.delay < age && age <= ev.delay + ev.duration {
                self.deme_event_drain(d, i)?;
            }

            let ev = &self.pop.demes[d].events.cell_events[i];
            if !ev.active && ev.delay == age {
                self.pop.demes[d].events_total += 1;
                // start event
                let World { pop, rng, .. } = self;
                pop.demes[d].events.cell_events[i].activate(rng);
                let ev = &self.pop.demes[d].events.cell_events[i];
                // `IsDecayed()` is read once per event cell.
                let decayed = !ev.event_cells.is_empty() && ev.is_decayed()?;
                let update = self.stats.update();
                for rel in ev.live_cells() {
                    // place event ID in cells' data
                    let ev = &self.pop.demes[d].events.cell_events[i];
                    let data = if decayed {
                        ev.event_id_decay(self.pop.demes[d].cell_position(rel))
                    } else {
                        ev.id_read()?
                    };
                    let abs = self.deme_event_cell(d, rel)?;
                    self.set_cell_data(abs, data, -1, update);
                }
            } else if ev.active && ev.delay + ev.duration == age {
                let id = ev.id_read()?;
                let update = self.stats.update();
                for rel in ev.live_cells() {
                    let abs = self.deme_event_cell(d, rel)?;
                    if self.pop.cells[abs as usize].cell_data.contents == id {
                        //set cell data to 0
                        self.set_cell_data(abs, 0, -1, update);
                    }
                }
                // event over
                self.pop.demes[d].events.cell_events[i].active = false;
            }
        }

        let ratio = self.cfg.demes_mim_events_killed_ratio;
        let deme = &mut self.pop.demes[d];
        let end_point = deme
            .events
            .slot_end_points
            .iter()
            .find(|&&(end, _)| end == age)
            .copied();
        if let Some((_, flow)) = end_point {
            // at end point
            if f64::from(deme.events_killed_this_slot) >= ratio * f64::from(flow) {
                deme.consecutive_successful_event_periods += 1;
            } else {
                deme.consecutive_successful_event_periods = 0;
            }
            let t = self.stats.flow_rate_tuples.entry(flow).or_default();
            t.org_count.add(deme.cur_org_count);
            t.events_killed.add(deme.events_killed_this_slot as i32);
            t.attempts_to_kill_events
                .add(deme.event_kill_attempts_this_slot as i32);
            t.avg_energy_usage_ratio.add(energy_usage.average());
            t.total_births.add(deme.birth_count_perslot);
            t.current_sleeping.add(deme.sleeping_count);
            deme.birth_count_perslot = 0;
            deme.events_killed_this_slot = 0;
            deme.event_kill_attempts_this_slot = 0;
        }
        deme.age += 1;
        Ok(())
    }

    /// The first branch of the event loop of `cDeme::ProcessUpdate`: an
    /// active event drains the energy of the organisms and cells that
    /// still show its id.
    fn deme_event_drain(&mut self, d: usize, i: usize) -> Result<(), String> {
        let res = self.env.resource_lib().get_resource_id("CELL_ENERGY");
        let ev = &self.pop.demes[d].events.cell_events[i];
        let id = ev.id_read()?;
        let decay = self.cfg.attack_decay_rate;
        for rel in ev.live_cells() {
            let abs = self.deme_event_cell(d, rel)?;
            if self.pop.cells[abs as usize].cell_data.contents != id {
                continue;
            }
            let res = res.ok_or(
                "unsupported: a deme cell event without a CELL_ENERGY resource (C++: NULL \
                 dereference in cDeme::ProcessUpdate)",
            )?;
            if self.is_occupied(abs) {
                let World { pop, cfg, .. } = self;
                let o = pop.orgs[abs as usize]
                    .as_mut()
                    .ok_or("deme event: occupied cell without its organism")?;
                // remove energy from organism
                let stored = o.state.phenotype.energy_store;
                o.state.phenotype.reduce_energy(cfg, stored * decay);
            }
            // remove energy from cell... organism might not takeup all of a
            // cell's energy. QUIRK: the cell's whole resource vector, scaled
            // in the CELL_ENERGY slot (indexed by its library id), is added
            // back to the cell.
            let World {
                pop, rng, res_env, ..
            } = self;
            let rc = &mut pop.demes[d].resource_count;
            let mut cell_resources = rc.get_cell_resources(rel, rng, res_env).to_vec();
            let slot = cell_resources.get_mut(res).ok_or(
                "unsupported: CELL_ENERGY's library id is beyond the deme resources (C++: \
                 out-of-bounds write in cDeme::ProcessUpdate)",
            )?;
            *slot *= decay;
            rc.modify_cell(rng, res_env, &cell_resources, rel);
        }
        Ok(())
    }

    /// `(*deme_pred_list[p])(ctx, &deme_resource_count)` for deme `d`.
    fn deme_eval_resource_threshold(&mut self, d: usize, p: usize) -> Result<(), String> {
        let pred = self.pop.demes[d].events.deme_preds[p].clone();
        let World {
            pop, rng, res_env, ..
        } = self;
        let rc = &mut pop.demes[d].resource_count;
        let id = rc.get_resource_by_name(&pred.resource_name);
        if id < 0 {
            eprintln!("Error: Unknown resource '{}'.", pred.resource_name);
            return Err(format!(
                "unsupported: DemeResourceThresholdPredicate on the unknown deme resource \
                 '{}' (C++: cResourceCount::Get(-1), an out-of-bounds read)",
                pred.resource_name
            ));
        }
        let level = rc.get(rng, res_env, id as usize);
        let satisfied = match pred.op.as_str() {
            ">=" | "<=" => {
                let t = pred.threshold.ok_or(
                    "unsupported: DemeResourceThresholdPredicate without a threshold (C++: \
                     reads the uninitialised threasholdValue)",
                )?;
                if pred.op == ">=" {
                    level >= t
                } else {
                    level <= t
                }
            }
            op => {
                self.abort_run(
                    &format!("Undefined operator {op}.  Exit!\n"),
                    -1,
                    "DemeResourceThresholdPredicate: undefined operator (C++: exit(-1))",
                );
                return Ok(());
            }
        };
        if satisfied {
            self.pop.demes[d].events.deme_preds[p].previously_satisfied = true;
        }
        Ok(())
    }

    /// `cDeme::AddDemeResourceThresholdPredicate(...)` on every deme.
    pub(crate) fn add_deme_resource_threshold_predicate(
        &mut self,
        resource_name: &str,
        op: &str,
        threshold: Option<f64>,
    ) {
        for deme in &mut self.pop.demes {
            deme.events.deme_preds.push(ResourceThresholdPred {
                resource_name: resource_name.to_string(),
                op: op.to_string(),
                threshold,
                previously_satisfied: false,
            });
        }
    }

    /// `cPopulation::AddDemePred(type, times)` for the movement predicate
    /// types.
    pub(crate) fn add_deme_pred(&mut self, kind: MovementPredType, times: i32) {
        for d in 0..self.pop.demes.len() {
            let events = &mut self.pop.demes[d].events;
            if events.cell_events.is_empty() {
                let what = match kind {
                    MovementPredType::MovedIntoCenter | MovementPredType::NUnique => {
                        "An EventMovedIntoCenter cannot be created until a CellEvent is added."
                    }
                    MovementPredType::MovedBetweenTargets => {
                        "An EventMoveBetweenTargets cannot be created until at least one \
                         CellEvent is added."
                    }
                };
                eprintln!("Error: {what}");
                self.abort_run("", 1, what);
                return;
            }
            let n = events.cell_events.len();
            let new_pred = |kind| MovementPred {
                kind,
                satisfied: false,
            };
            match kind {
                MovementPredType::MovedIntoCenter => {
                    for event in 0..n {
                        events
                            .movement_preds
                            .push(new_pred(MovementKind::MovedIntoCenter {
                                event,
                                total_times: times,
                                current_times: 0,
                            }));
                    }
                }
                MovementPredType::NUnique => {
                    for event in 0..n {
                        events.movement_preds.push(new_pred(MovementKind::NUnique {
                            event,
                            total_individuals: times as u32,
                            unique_org_ids: BTreeSet::new(),
                        }));
                    }
                }
                MovementPredType::MovedBetweenTargets => {
                    events
                        .movement_preds
                        .push(new_pred(MovementKind::MovedBetweenTargets {
                            events: (0..n).collect(),
                            total_times: times as u32,
                            // TODO in the C++: eventually a parameter.
                            total_orgs: 1,
                            event_success: vec![BTreeSet::new(); n],
                            successful_orgs: BTreeSet::new(),
                        }));
                }
            }
        }
    }

    /// `cStats::Move(org)` after the organism `org_id` moved into `cell`:
    /// every movement predicate of every deme sees the move.
    pub(crate) fn deme_stats_move(&mut self, cell: i32, org_id: i32) -> Result<(), String> {
        let c = usize::try_from(cell).map_err(|_| "cStats::Move: negative cell")?;
        let mover_deme = self.pop.cells[c].deme_id;
        let num_task_cells = self.host.num_task_cells[c];
        for d in 0..self.pop.demes.len() {
            for p in 0..self.pop.demes[d].events.movement_preds.len() {
                self.deme_movement_pred(d, p, cell, mover_deme, org_id, num_task_cells)?;
            }
        }
        Ok(())
    }

    /// One movement predicate's `operator()(org)`.
    fn deme_movement_pred(
        &mut self,
        d: usize,
        p: usize,
        cell: i32,
        mover_deme: i32,
        org_id: i32,
        num_task_cells: i32,
    ) -> Result<(), String> {
        let deme_id = self.pop.demes[d].id;
        let mut log: Option<String> = None;
        {
            let deme = &mut self.pop.demes[d];
            let size = deme.size();
            let position = deme.cell_position(cell);
            let events = &mut deme.events;
            let pred = &mut events.movement_preds[p];
            match &mut pred.kind {
                MovementKind::MovedIntoCenter {
                    event,
                    total_times,
                    current_times,
                } => {
                    let ev = &events.cell_events[*event];
                    if mover_deme == deme_id && ev.active && ev.in_center(position) {
                        *current_times += 1;
                        if *current_times >= *total_times {
                            pred.satisfied = true;
                        }
                    }
                }
                MovementKind::NUnique {
                    event,
                    total_individuals,
                    unique_org_ids,
                } => {
                    let ev = &events.cell_events[*event];
                    // is the organism in an event cell
                    if mover_deme == deme_id && ev.active && ev.on_event_cell(cell % size) {
                        unique_org_ids.insert(org_id);
                        // have enough unique organisms visited the event cells?
                        if unique_org_ids.len() as u64 >= u64::from(*total_individuals) {
                            pred.satisfied = true;
                        }
                    }
                }
                MovementKind::MovedBetweenTargets {
                    events: evs,
                    total_times,
                    total_orgs,
                    event_success,
                    successful_orgs,
                } => {
                    // QUIRK: any move anywhere clears the flag.
                    pred.satisfied = false;
                    for (i, &e) in evs.iter().enumerate() {
                        let ev = &events.cell_events[e];
                        let other = (i + 1) % 2;
                        if !(ev.active && mover_deme == deme_id) {
                            continue;
                        }
                        if !ev.on_event_cell(cell % size) {
                            continue;
                        }
                        event_success[i].insert(org_id);
                        let other_set = event_success.get(other).ok_or(
                            "unsupported: an EventMovedBetweenTargets predicate with a \
                             single event sees a move onto it (C++: out-of-bounds \
                             m_event_success[1])",
                        )?;
                        if !other_set.contains(&org_id) {
                            continue;
                        }
                        // See how many times this org has gone back and forth.
                        let num_backforth = (num_task_cells / 2) as u32;
                        if num_backforth < *total_times {
                            continue;
                        }
                        successful_orgs.insert(org_id);
                        let mut text = format!(
                            "Organism {org_id} in deme {deme_id} has touched both targets \
                             {num_backforth} times. {} orgs have done this out of \
                             {total_orgs}\n",
                            successful_orgs.len()
                        );
                        for (q, set) in event_success.iter().enumerate() {
                            text.push_str(&format!("ORGS FOR EVENT {q}"));
                            for id in set {
                                text.push_str(&format!(" {id}"));
                            }
                            text.push('\n');
                        }
                        log = Some(text);
                        if successful_orgs.len() as u64 >= u64::from(*total_orgs) {
                            pred.satisfied = true;
                            break;
                        }
                    }
                }
            }
        }
        if let Some(text) = log {
            // `ofstream predlog("data/predlog.dat", ios::app)`: relative to
            // the process's current directory, whatever DATA_DIR says.
            use std::io::Write as _;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("data/predlog.dat")
            {
                let _ = f.write_all(text.as_bytes());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(rect: [i32; 4], static_pos: bool) -> CellEvent {
        let mut rng = AvidaRng::new(1);
        CellEvent::new(rect, 5, 2, 5, 5, static_pos, 100, &mut rng).unwrap()
    }

    #[test]
    fn static_event_cells_and_centre() {
        let e = ev([1, 1, 3, 2], true);
        assert_eq!(e.event_cells, vec![6, 7, 8, 11, 12, 13]);
        assert_eq!(e.center, ((1, 0), (1, 1)));
        // lexicographic pair comparison
        assert!(e.in_center((1, 0)));
        assert!(e.in_center((1, 1)));
        assert!(!e.in_center((0, 9)));
        assert!(e.on_event_cell(12));
        assert!(e.live_cells().is_empty());
    }

    #[test]
    fn decayed_ids_fall_off_from_the_centre() {
        let e = ev([0, 0, 2, 2], true);
        // centre (1,1); maxd = sqrt(32), top = 6
        assert_eq!(e.event_id_decay((1, 1)), 8);
        assert_eq!(e.event_id_decay((3, 1)), 5);
        assert_eq!(e.event_id_decay((4, 4)), 3);
        assert!(e.is_decayed().is_err());
    }

    #[test]
    fn moving_events_fill_their_cells_on_activation() {
        let mut e = ev([0, 0, 1, 1], false);
        let mut rng = AvidaRng::new(3);
        e.activate(&mut rng);
        assert!(e.active && e.event_id.is_some());
        let cells = e.live_cells();
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[1], cells[0] + 1);
        assert_eq!(cells[2], cells[0] + 5);
    }

    #[test]
    fn slots_and_predicates() {
        let mut de = DemeEvents {
            slot_end_points: vec![(10, 3), (20, 1)],
            ..DemeEvents::default()
        };
        assert_eq!(de.slot_flow_rate(4), 3);
        assert_eq!(de.slot_flow_rate(15), 1);
        assert_eq!(de.slot_flow_rate(25), 0);
        de.push(ev([0, 0, 0, 0], true)).unwrap();
        de.movement_preds.push(MovementPred {
            kind: MovementKind::NUnique {
                event: 0,
                total_individuals: 1,
                unique_org_ids: BTreeSet::from([4]),
            },
            satisfied: true,
        });
        assert!(de.mov_pred_satisfied_previously());
        assert_eq!(de.mov_pred_events(), vec![[Some((None, false)); 2]]);
        de.reset_predicates();
        assert!(!de.mov_pred_satisfied_previously());
        // capacity 1 is full: the next push would reallocate
        assert!(de.push(ev([0, 0, 0, 0], true)).is_err());
        let mut e = ev([0, 0, 0, 0], true);
        e.delay = -7;
        e.confine_to_time_slot(10, 20).unwrap();
        assert_eq!(e.delay, 3);
    }
}
