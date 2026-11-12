//! Port of `cTaskContext` and `cTaskState`
//! (avida-core/source/main/cTaskContext.h, cTaskState.h), plus the
//! [`TaskWorld`] trait standing in for the organism / population / world
//! pointers the C++ evaluators dereference.

use crate::io_buffer::IoBuffer;
use std::collections::HashMap;

/// Per-task persistent state (`cTaskState` and its only subclass in the task
/// library, `cFibSeqState`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    /// `cFibSeqState`
    FibSeq(FibSeqState),
}

/// `cFibSeqState` (cTaskLib.cc): the last two sequence values and the match count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FibSeqState {
    /// `seq[2]`, initially `{1, 0}`.
    pub seq: [i32; 2],
    /// `count`, initially 0.
    pub count: i32,
}

impl Default for FibSeqState {
    fn default() -> Self {
        FibSeqState {
            seq: [1, 0],
            count: 0,
        }
    }
}

/// `Apto::Map<void*, cTaskState*>`: task state keyed by task entry id.
pub type TaskStates = HashMap<usize, TaskState>;

/// `cTaskContext`: everything a task evaluator is handed. Buffers are borrowed
/// (the C++ holds const references), so building a context never copies them.
#[derive(Debug)]
pub struct TaskContext<'a> {
    /// `GetInputBuffer()`
    pub input_buffer: &'a IoBuffer,
    /// `GetOutputBuffer()`
    pub output_buffer: &'a IoBuffer,
    /// `GetNeighborhoodInputBuffers()`
    pub other_input_buffers: &'a [&'a IoBuffer],
    /// `GetNeighborhoodOutputBuffers()`
    pub other_output_buffers: &'a [&'a IoBuffer],
    /// `GetExtendedMemory()`
    pub ext_mem: &'a [i32],
    /// `GetReceivedMessages()` (`NULL` is `None`)
    pub received_messages: Option<&'a IoBuffer>,
    /// `GetLogicId()` / `SetLogicId()`; set by `TaskLib::setup_tests`.
    pub logic_id: i32,
    /// `GetOnDivide()`
    pub on_divide: bool,
    /// `GetTaskValue()` / `SetTaskValue()`: for optimize tasks the function
    /// value the organism is outputting, otherwise 0.
    pub task_value: f64,
    /// `GetTaskEntry()`: id of the task being evaluated (an index into
    /// `TaskLib`), set with [`TaskContext::set_task_entry`].
    pub task_entry: Option<usize>,
    /// `m_task_states`: the organism's per-task state storage (`NULL` is `None`).
    pub task_states: Option<&'a mut TaskStates>,
}

impl<'a> TaskContext<'a> {
    /// `cTaskContext(organism, inputs, outputs, other_inputs, other_outputs,
    /// ext_mem, in_on_divide, in_received_messages)`. The organism and deme
    /// arguments of the C++ constructor are reached through [`TaskWorld`].
    pub fn new(
        input_buffer: &'a IoBuffer,
        output_buffer: &'a IoBuffer,
        other_input_buffers: &'a [&'a IoBuffer],
        other_output_buffers: &'a [&'a IoBuffer],
        ext_mem: &'a [i32],
        on_divide: bool,
        received_messages: Option<&'a IoBuffer>,
    ) -> Self {
        TaskContext {
            input_buffer,
            output_buffer,
            other_input_buffers,
            other_output_buffers,
            ext_mem,
            received_messages,
            logic_id: 0,
            on_divide,
            task_value: 0.0,
            task_entry: None,
            task_states: None,
        }
    }

    /// Minimal context for logic-style tasks: input and output buffers only.
    pub fn simple(input_buffer: &'a IoBuffer, output_buffer: &'a IoBuffer) -> Self {
        Self::new(input_buffer, output_buffer, &[], &[], &[], false, None)
    }

    /// `SetTaskEntry(cTaskEntry*)`.
    pub fn set_task_entry(&mut self, id: usize) {
        self.task_entry = Some(id);
    }

    /// `SetTaskStates(map*)`.
    pub fn set_task_states(&mut self, states: &'a mut TaskStates) {
        self.task_states = Some(states);
    }

    /// `GetLogicId()`.
    pub fn get_logic_id(&self) -> i32 {
        self.logic_id
    }
    /// `SetLogicId(int)`.
    pub fn set_logic_id(&mut self, v: i32) {
        self.logic_id = v;
    }
    /// `GetOnDivide()`.
    pub fn get_on_divide(&self) -> bool {
        self.on_divide
    }
    /// `GetTaskValue()`.
    pub fn get_task_value(&self) -> f64 {
        self.task_value
    }
    /// `SetTaskValue(double)`.
    pub fn set_task_value(&mut self, v: f64) {
        self.task_value = v;
    }
}

/// Everything a task evaluator pulls from the organism (`ctx.GetOrganism()`),
/// the demes, the population and the world that is not plain buffer data.
///
/// Methods are named after the C++ call they replace (snake_case). The
/// "organism" is the one the [`TaskContext`] was built for; the implementor
/// resolves it. Logic-style tasks (`not`, `nand`, ..., math, match, optimize)
/// never call into the trait.
pub trait TaskWorld {
    // ---- cOrganism ------------------------------------------------------
    /// `cOrganism::GetCellID()`
    fn get_cell_id(&self) -> i32;
    /// `cOrganism::GetAVCellID()`
    fn get_av_cell_id(&self) -> i32;
    /// `cOrganism::GetPrevSeenCellID()`
    fn get_prev_seen_cell_id(&self) -> i32;
    /// `cOrganism::SetPrevSeenCellID(int)`
    fn set_prev_seen_cell_id(&mut self, id: i32);
    /// `cOrganism::GetPrevTaskCellID()`
    fn get_prev_task_cell_id(&self) -> i32;
    /// `cOrganism::SetPrevTaskCellID(int)`
    fn set_prev_task_cell_id(&mut self, id: i32);
    /// `cOrganism::AddReachedTaskCell()`
    fn add_reached_task_cell(&mut self);
    /// `cOrganism::GetCellData()`
    fn get_cell_data(&self) -> i32;
    /// `cOrganism::GetForageTarget()`
    fn get_forage_target(&self) -> i32;
    /// `cOrganism::GetGradientMovement()`
    fn get_gradient_movement(&self) -> f64;
    /// `cOrganism::HasOpinion()`
    fn has_opinion(&self) -> bool;
    /// `cOrganism::GetOpinion().first`
    fn get_opinion_first(&self) -> i32;
    /// `cOrganism::GetEventKilled()`
    fn get_event_killed(&self) -> bool;
    /// `cOrganism::GetLyseDisplay()`
    fn get_lyse_display(&self) -> bool;
    /// `cOrganism::GetInputAt(int)`
    fn get_input_at(&self, i: i32) -> i32;
    /// `cOrganism::GetPhenotype().GetKaboomExecuted()`
    fn get_kaboom_executed(&self) -> bool;
    /// `cOrganism::GetPhenotype().GetKaboomExecuted2()`
    fn get_kaboom_executed2(&self) -> bool;
    /// `cOrganism::GetNumberStringsProduced(int)`
    fn get_number_strings_produced(&self, tag: i32) -> i32;
    /// `cOrganism::GetNumberStringsOnHand(int)`
    fn get_number_strings_on_hand(&self, i: i32) -> i32;
    /// `cOrganism::UpdateTag(int new_tag, int bits)`
    fn update_tag(&mut self, tag: i32, bits: i32);
    /// `cOrganism::GetTagLabel()`
    fn get_tag_label(&self) -> i32;
    /// `cOrganism::SetLineageLabel(int)`
    fn set_lineage_label(&mut self, label: i32);
    /// `cOrganism::GetReputation()`
    fn get_reputation(&self) -> i32;
    /// `cOrganism::SetReputation(int)`
    fn set_reputation(&mut self, rep: i32);
    /// `cOrganism::GetNumberOfDonations()`
    fn get_number_of_donations(&self) -> i32;

    // ---- cOrganism::GetStateGrid() (cStateGrid) -------------------------
    /// `cStateGrid::GetName()`
    fn state_grid_name(&self) -> String;
    /// `cStateGrid::GetStateID(const cString&)`, negative when unknown.
    fn state_grid_state_id(&self, name: &str) -> i32;
    /// `cStateGrid::GetNumStates()`
    fn state_grid_num_states(&self) -> i32;
    /// `cStateGrid::GetWidth()`
    fn state_grid_width(&self) -> i32;
    /// `cStateGrid::GetHeight()`
    fn state_grid_height(&self) -> i32;
    /// `cStateGrid::GetStateAt(int)`
    fn state_grid_state_at(&self, cell: i32) -> i32;

    // ---- cDeme (the organism's deme) ------------------------------------
    /// `cDeme::GetCellPosition(cell_id).first`
    fn deme_cell_position_x(&self, cell_id: i32) -> i32;
    /// `cDeme::GetRelativeCellID(int)`
    fn deme_get_relative_cell_id(&self, cell_id: i32) -> i32;
    /// `cDeme::GetNumEvents()`
    fn deme_num_events(&self) -> i32;
    /// `cDeme::GetCellEvent(i)->GetEventID()`
    fn deme_cell_event_id(&self, i: i32) -> i32;
    /// `cDeme::GetNumMovementPredicates()`
    fn deme_num_movement_predicates(&self) -> i32;
    /// `cDeme::GetMovPredicate(i)->GetEvent(j)`: `(GetEventID(), IsActive())`,
    /// or `None` when the event pointer is null.
    fn deme_mov_predicate_event(&self, predicate: i32, event: i32) -> Option<(i32, bool)>;

    // ---- cPopulation / cWorld ------------------------------------------
    /// `cPopulation::NumberOfOrganismsInGroup(int)`
    fn number_of_organisms_in_group(&mut self, group_id: i32) -> i32;
    /// `m_world->GetConfig().WORLD_X.Get()`
    fn world_x(&self) -> i32;
    /// `m_world->GetConfig().WORLD_Y.Get()`
    fn world_y(&self) -> i32;
    /// `m_world->GetConfig().USE_AVATARS.Get()`
    fn use_avatars(&self) -> bool;
    /// `m_world->GetConfig().MATCH_ALREADY_PRODUCED.Get()`
    fn match_already_produced(&self) -> bool;
    /// Pheromone amount for the `nand-resourceDependent` / `nor-resourceDependent`
    /// tasks. Replaces this C++ block: if `resource_count.GetSize() > 0`, look
    /// up the resource named "pheromone"; when it exists and
    /// `strncmp(resource_count.GetResName(id), "pheromone", 9) == 0`, the value
    /// is `GetOrgInterface().GetResources(ctx)[id]`; otherwise 0.0.
    fn pheromone_resource_amount(&self) -> f64;
    /// `cOrgInterface::GetResources(ctx)` (the organism's cell resource levels).
    fn get_resources(&self) -> Vec<f64>;
    /// `cOrgInterface::GetAVResources(ctx)`
    fn get_av_resources(&self) -> Vec<f64>;
    /// For `xor-max`: `GetReactionLib().GetReaction(task_id)`; when it exists,
    /// `ChangeResource(reaction, GetResourceLib().GetResource(resource_index)->GetName())`
    /// and `true` is returned. `false` when the reaction does not exist.
    fn change_reaction_resource(&mut self, task_id: usize, resource_index: i32) -> bool;
    /// `cPopulationCell::IsOccupied()` and, when occupied,
    /// `GetOrganism()->GetPhenotype().GetKaboomExecuted()`; `None` when the
    /// cell is empty.
    fn cell_kaboom_executed(&self, cell_id: i32) -> Option<bool>;
    /// `GetOrganism()->GetPhenotype().ClearKaboomExecuted()` for the organism
    /// in the given cell.
    fn clear_kaboom_executed(&mut self, cell_id: i32);
    /// `std::rand()` (the C library generator, not the world RNG).
    fn std_rand(&mut self) -> i32;
    /// `Task_ConsumePublicGood` is about to pick one of `producers` (more
    /// than one) organisms out of a `std::set<cOrganism*>`, which orders
    /// them by heap address, so the C++ choice cannot be reproduced. A host
    /// that must match the C++ records a stop here; by default the
    /// evaluator carries on with the producers in cell order.
    fn unreproducible_producer_choice(&mut self, _producers: usize) {}
    /// `cout << text` from inside a task (`Task_Optimize`'s `quality > 1!`
    /// line). By default it goes to [`avida_apto::diag::stdout`]: standard
    /// output, or the capture of a thread that collects its output (an
    /// analyze worker, a session's simulation thread). A host that buffers
    /// its own standard output must forward it to keep the C++ order.
    fn task_stdout(&mut self, text: &str) {
        avida_apto::diag::stdout(format_args!("{text}"));
    }

    // ---- cStats --------------------------------------------------------
    /// `cStats::AddTag(int tag, int value)`
    fn stats_add_tag(&mut self, tag: i32, value: i32);
    /// `cStats::AddStringBitsMatchedValue(cString name, int value)`
    fn stats_add_string_bits_matched_value(&mut self, name: &str, value: i32);
    /// `cStats::IncPerfectMatch(int)`
    fn stats_inc_perfect_match(&mut self, amount: i32);
    /// `cStats::IncPerfectMatchOrg()`
    fn stats_inc_perfect_match_org(&mut self);
}
