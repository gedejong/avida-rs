//! Actions of the third porting round, on world subsystems the earlier
//! rounds left out (avida-core/source at 47f13dadb):
//!
//! * the energy printers and settings: `cActionSetNumInstBefore0Energy`
//!   (EnvironmentActions.cc), `cActionPrintDemesTotalAvgEnergy`,
//!   `cActionPrintDemeEnergySharingStats`,
//!   `cActionPrintDemeEnergyDistributionStats`, `cActionDumpEnergyGrid`,
//!   `cActionDumpExecutionRatioGrid` (PrintActions.cc) with the
//!   `cPopulation` printers they call;
//! * organism relocation: `cActionMixPopulation` (`cPopulation::MixPopulation`,
//!   `cPopulationCell::RemoveOrganism` / `InsertOrganism`);
//! * the trial machinery: `cActionNewTrial`, `cActionCompeteOrganisms`
//!   (`cPopulation::NewTrial`, `CompeteOrganisms`,
//!   `CompeteOrganisms_ConstructOffspring`);
//! * the message predicates of cOrgMessagePredicate.h and the actions that
//!   register them (`cActionTrackAllMessages`, `cActionDistributeData`,
//!   `cActionDistributeDataEfficiently`), with `cStats::SentMessage` and
//!   `cStats::PrintPredicatedMessages`;
//! * the den actions of predator-prey runs with avatars: `cActionAttackDen`,
//!   `cActionRaidDen`;
//! * `cActionIteratedConsensus` (`cAbstractMonitoringCompeteDemes`,
//!   `ConsensusSupport`);
//! * `cActionSaveStructuredSystematicsGroup` /
//!   `cActionLoadStructuredSystematicsGroup` (SaveLoadActions.cc,
//!   `cPopulation::SaveStructuredSystematicsGroup` /
//!   `LoadStructuredSystematicsGroup`);
//! * the deme network actions `cActionMeasureDemeNetworks`,
//!   `cActionCompeteDemesByNetwork` and `cActionDistributeDataCheaply`,
//!   which abort the run as soon as they reach `cDeme::GetNetwork()`;
//! * the deme cell-event and deme-predicate actions
//!   `cActionDelayedDemeEvent`, `cActionDelayedDemeEventsPerSlots`
//!   (EnvironmentActions.cc), `cActionPred_DemeEventMoveCenter`,
//!   `cActionPred_DemeEventMoveBetweenTargets`,
//!   `cActionPred_DemeEventEventNUniqueIndividualsMovedIntoTarget` and
//!   `cActionPred_DemeResourceThresholdPredicate`, on the deme state of
//!   [`crate::deme_events`].
//!
//! The deme-fitness competitions on the energy model live with the other
//! `cAbstractCompeteDemes` fitness functions in [`crate::actions_deme2`].

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use avida_apto::fmt::fmt_double;
use avida_config::feedback::Feedback;
use avida_config::init_file::InitFile;
use avida_config::string_util::{as_double, as_int, pop_word};
use avida_genome::genome_util::libcxx_random_shuffle;
use avida_genome::{ArgContainer, ArgSchema, ArgType};
use avida_hardware::hardware::{divide_do_mutations, divide_test_fitness_measures};
use avida_hardware::org_interface::OrgMessage;
use avida_hardware::organism::{transmission, OrgSource, Organism};
use avida_systematics::apto_map::AptoMap;
use avida_systematics::unit::{GroupMembership, GroupRef, UnitInfo};

use crate::actions::ParseContext;
use crate::deme_events::{EventSpec, MovementPredType};
use crate::pop_interface::PopulationInterface;
use crate::pop_orgs::systematics_source;
use crate::population::position_offspring as po;
use crate::stat_tools::DoubleSum;
use crate::world::World;

/// `VERBOSE_SILENT`, `VERBOSE_DETAILS` (avida/core/Types.h)
const VERBOSE_SILENT: i32 = 0;
const VERBOSE_DETAILS: i32 = 3;

/// `Avida::INVALID_CONFIG`
const INVALID_CONFIG: i32 = 2;

/// The registered action names this module parses.
pub const ACTIONS3: &[&str] = &[
    "SetNumInstBefore0Energy",
    "PrintDemesTotalAvgEnergy",
    "PrintDemeEnergySharingStats",
    "PrintDemeEnergyDistributionStats",
    "DumpEnergyGrid",
    "DumpExecutionRatioGrid",
    "MixPopulation",
    "NewTrial",
    "CompeteOrganisms",
    "TrackAllMessages",
    "DistributeData",
    "DistributeDataEfficiently",
    "AttackDen",
    "RaidDen",
    "LoadStructuredSystematicsGroup",
    "SaveStructuredSystematicsGroup",
    "IteratedConsensus",
    "MeasureDemeNetworks",
    "CompeteDemesByNetwork",
    "DistributeDataCheaply",
    "DelayedDemeEvent",
    "DelayedDemeEventsPerSlots",
    "Pred_DemeEventMoveCenter",
    "Pred_DemeEventMoveBetweenTargets",
    "Pred_DemeEventNUniqueIndividualsMovedIntoTarget",
    "DemeResourceThresholdPredicate",
];

/// A parsed action of this module. `id` keys the state a C++ action object
/// keeps (its message predicate).
#[derive(Debug, Clone, PartialEq)]
pub enum Action3 {
    /// `cActionAttackDen`
    AttackDen { killprob: f64, juvs_per: i32 },
    /// `cActionLoadStructuredSystematicsGroup`
    LoadStructuredSystematicsGroup { filename: String, role: String },
    /// `cActionIteratedConsensus`; `id` keys its running fitness sums and
    /// per-deme hold state.
    IteratedConsensus {
        id: usize,
        compete_period: i32,
        replace: i32,
        kill: i32,
        restrict_range: i32,
        dont_replace: i32,
    },
    /// `cActionSaveStructuredSystematicsGroup`
    SaveStructuredSystematicsGroup { filename: String, role: String },
    /// `cActionRaidDen`
    RaidDen {
        loss: f64,
        res_id: i32,
        units_per: f64,
    },
    /// `cActionSetNumInstBefore0Energy`
    SetNumInstBefore0Energy { new_value: i32 },
    /// `cActionPrintDemesTotalAvgEnergy`
    PrintDemesTotalAvgEnergy,
    /// `cActionPrintDemeEnergySharingStats`
    PrintDemeEnergySharingStats,
    /// `cActionPrintDemeEnergyDistributionStats`
    PrintDemeEnergyDistributionStats,
    /// `cActionDumpEnergyGrid` (empty filename: `grid_energy.<update>.dat`)
    DumpEnergyGrid { filename: String },
    /// `cActionDumpExecutionRatioGrid` (empty filename:
    /// `grid_exe_ratio.<update>.dat`)
    DumpExecutionRatioGrid { filename: String },
    /// `cActionMixPopulation`
    MixPopulation,
    /// `cActionNewTrial`
    NewTrial,
    /// `cActionCompeteOrganisms`
    CompeteOrganisms {
        comp_type: i32,
        parents_survive: i32,
    },
    /// `cActionTrackAllMessages`
    TrackAllMessages,
    /// `cActionDistributeData`, `cActionDistributeDataEfficiently` and
    /// `cActionDistributeDataCheaply`. `alive` stands for the C++ action
    /// object, which owns the message counter `cStats` points at: it is
    /// dropped with the event entry once the event cannot fire again.
    DistributeData {
        id: usize,
        mode: DistributeMode,
        alive: Arc<()>,
    },
    /// `cActionMeasureDemeNetworks`
    MeasureDemeNetworks,
    /// `cActionCompeteDemesByNetwork`
    CompeteDemesByNetwork,
    /// `cActionDelayedDemeEvent`
    DelayedDemeEvent { spec: EventSpec, total_events: i32 },
    /// `cActionDelayedDemeEventsPerSlots`: `slots` is `[total_slots,
    /// events_per_slot_max, events_per_slot_min, event_flow_levels]`.
    DelayedDemeEventsPerSlots { spec: EventSpec, slots: [i32; 4] },
    /// `cActionPred_DemeEventMoveCenter`,
    /// `cActionPred_DemeEventMoveBetweenTargets`,
    /// `cActionPred_DemeEventEventNUniqueIndividualsMovedIntoTarget`
    DemeMovementPredicate { kind: MovementPredType, times: i32 },
    /// `cActionPred_DemeResourceThresholdPredicate`; `threshold` is `None`
    /// when the C++ member stays uninitialised.
    DemeResourceThresholdPredicate {
        resource_name: String,
        op: String,
        threshold: Option<f64>,
    },
}

/// Which `cActionDistributeData*` fitness function an action uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributeMode {
    /// `cActionDistributeData`
    Plain,
    /// `cActionDistributeDataEfficiently`
    Efficiently,
    /// `cActionDistributeDataCheaply`
    Cheaply,
}

/// A message predicate (`cOrgMessagePredicate`) registered with `cStats`.
#[derive(Debug, Clone, PartialEq)]
pub enum MsgPredicate {
    /// `cOrgMessagePred_CountDemeMessages`: deme id -> messages sent.
    CountDemeMessages(BTreeMap<i32, i32>),
    /// `cOrgMessagePred_AllData`: data, label, sender cell and receiver
    /// cell of every message.
    AllData(Vec<(u32, u32, i32, i32)>),
}

impl MsgPredicate {
    /// `operator()(msg)`; `sender_deme` is the sender's deme id.
    fn sent(&mut self, msg: &OrgMessage, sender_deme: i32) {
        match self {
            MsgPredicate::CountDemeMessages(counts) => {
                *counts.entry(sender_deme).or_insert(0) += 1;
            }
            MsgPredicate::AllData(msgs) => {
                msgs.push((msg.data, msg.label, msg.sender_cell, msg.receiver_cell));
            }
        }
    }

    /// `Print(update, out)`
    fn print(&self, update: i32, out: &mut String) {
        match self {
            MsgPredicate::CountDemeMessages(counts) => {
                let total = counts.values().fold(0i32, |a, &c| a.wrapping_add(c));
                out.push_str(&format!("{update} COUNT {total} "));
                for c in counts.values() {
                    out.push_str(&format!(" {c}"));
                }
                out.push('\n');
            }
            MsgPredicate::AllData(msgs) => {
                for (data, label, sender, receiver) in msgs {
                    out.push_str(&format!(
                        "{update} ALL {data} {label} {sender} {receiver}\n"
                    ));
                }
            }
        }
    }

    /// `Reset()`
    fn reset(&mut self) {
        match self {
            MsgPredicate::CountDemeMessages(counts) => counts.clear(),
            MsgPredicate::AllData(msgs) => msgs.clear(),
        }
    }
}

/// State the actions of this module keep between calls.
#[derive(Debug, Default)]
pub struct Actions3State {
    /// `cStats::m_message_predicates`, in registration order.
    pub msg_predicates: Vec<MsgPredicate>,
    /// For each predicate, the action object that owns it (`None` for the
    /// predicates `TrackAllMessages` allocates and never frees).
    owners: Vec<Option<Weak<()>>>,
    /// The predicate (`m_message_counter`) of each `DistributeData*`
    /// action object, by action id.
    distribute_counters: BTreeMap<usize, usize>,
    /// The state of each `IteratedConsensus` action object, by action id.
    consensus: BTreeMap<usize, ConsensusState>,
}

/// `cActionIteratedConsensus::state`
#[derive(Debug, Clone, Copy)]
struct HoldState {
    opinion: i32,
    current: i32,
    max: i32,
}

/// The members of one `cActionIteratedConsensus` object.
#[derive(Debug, Default)]
struct ConsensusState {
    /// `_hold` (CONSENSUS_HOLD_TIME when the event file was loaded)
    hold: i32,
    /// `_update_fitness`
    update_fitness: Vec<f64>,
    /// `_state`: deme id -> hold state
    state: BTreeMap<i32, HoldState>,
}

/// Ids for actions whose C++ object keeps state between calls.
static NEXT_ACTION_ID: AtomicUsize = AtomicUsize::new(0);

fn next_id() -> usize {
    NEXT_ACTION_ID.fetch_add(1, Ordering::Relaxed)
}

/// `if (largs.GetSize()) x = largs.PopWord();`
struct Args(String);

impl Args {
    fn word(&mut self) -> Option<String> {
        if self.0.is_empty() {
            None
        } else {
            Some(pop_word(&mut self.0))
        }
    }
    fn int_or(&mut self, default: i32) -> i32 {
        self.word().map_or(default, |w| as_int(&w))
    }
    fn string_or(&mut self, default: &str) -> String {
        self.word().unwrap_or_else(|| default.to_string())
    }
}

/// The constructor of the action `name` (an [`ACTIONS3`] spelling) applied
/// to `args`; `None` for any other name.
pub fn parse(
    name: &str,
    args: &str,
    ctx: &ParseContext<'_>,
    feedback: &mut Feedback,
) -> Option<Result<Action3, String>> {
    let _ = ctx;
    let mut a = Args(args.to_string());
    Some(Ok(match name {
        "SetNumInstBefore0Energy" => Action3::SetNumInstBefore0Energy {
            new_value: a.int_or(0),
        },
        "PrintDemesTotalAvgEnergy" => Action3::PrintDemesTotalAvgEnergy,
        "PrintDemeEnergySharingStats" => Action3::PrintDemeEnergySharingStats,
        "PrintDemeEnergyDistributionStats" => Action3::PrintDemeEnergyDistributionStats,
        "DumpEnergyGrid" => Action3::DumpEnergyGrid {
            filename: a.string_or(""),
        },
        "DumpExecutionRatioGrid" => Action3::DumpExecutionRatioGrid {
            filename: a.string_or(""),
        },
        "MixPopulation" => Action3::MixPopulation,
        "NewTrial" => Action3::NewTrial,
        "CompeteOrganisms" => {
            let comp_type = a.int_or(0);
            let parents_survive = a.int_or(0);
            Action3::CompeteOrganisms {
                comp_type,
                parents_survive,
            }
        }
        "TrackAllMessages" => Action3::TrackAllMessages,
        "LoadStructuredSystematicsGroup" => {
            let mut schema = ArgSchema::new(':', '=', false);
            schema.add_required("filename", 0, ArgType::String);
            schema.add_required("role", 1, ArgType::String);
            let (filename, role) = ArgContainer::load(args, &schema, feedback)
                .map_or_else(Default::default, |c| {
                    (c.get_string(0).to_string(), c.get_string(1).to_string())
                });
            Action3::LoadStructuredSystematicsGroup { filename, role }
        }
        "SaveStructuredSystematicsGroup" => {
            let mut schema = ArgSchema::new(':', '=', false);
            schema.add_string("filename", 0, "");
            schema.add_required("role", 0, ArgType::String);
            // QUIRK: both entries take string slot 0 and the constructor
            // reads slot 0 for both, so the role is also the file name.
            let first = ArgContainer::load(args, &schema, feedback)
                .map_or_else(String::new, |c| c.get_string(0).to_string());
            Action3::SaveStructuredSystematicsGroup {
                filename: first.clone(),
                role: first,
            }
        }
        "IteratedConsensus" => {
            // `cAbstractMonitoringCompeteDemes` reads the period; the
            // subclass then skips that word and reads the rest. QUIRK: with
            // one argument, `_replace` is `"".AsInt()`, i.e. 0.
            let mut compete_period = 100;
            let (mut replace, mut kill, mut restrict_range, mut dont_replace) = (1, 1, 1, 0);
            if !args.is_empty() {
                compete_period = as_int(&pop_word(&mut args.to_string()));
                let mut largs = args.to_string();
                pop_word(&mut largs);
                replace = as_int(&pop_word(&mut largs));
                if !largs.is_empty() {
                    kill = as_int(&pop_word(&mut largs));
                    if !largs.is_empty() {
                        restrict_range = as_int(&pop_word(&mut largs));
                        if !largs.is_empty() {
                            dont_replace = as_int(&pop_word(&mut largs));
                        }
                    }
                }
            }
            Action3::IteratedConsensus {
                id: next_id(),
                compete_period,
                replace,
                kill,
                restrict_range,
                dont_replace,
            }
        }
        "AttackDen" => {
            let killprob = a.word().map_or(0.0, |w| as_double(&w));
            let juvs_per = a.int_or(1);
            Action3::AttackDen { killprob, juvs_per }
        }
        "RaidDen" => {
            let loss = a.word().map_or(0.0, |w| as_double(&w));
            let res_id = a.int_or(0);
            let units_per = a.word().map_or(1.0, |w| as_double(&w));
            Action3::RaidDen {
                loss,
                res_id,
                units_per,
            }
        }
        "DistributeData" | "DistributeDataEfficiently" | "DistributeDataCheaply" => {
            Action3::DistributeData {
                id: next_id(),
                mode: match name {
                    "DistributeDataEfficiently" => DistributeMode::Efficiently,
                    "DistributeDataCheaply" => DistributeMode::Cheaply,
                    _ => DistributeMode::Plain,
                },
                alive: Arc::new(()),
            }
        }
        "MeasureDemeNetworks" => Action3::MeasureDemeNetworks,
        "CompeteDemesByNetwork" => Action3::CompeteDemesByNetwork,
        "DelayedDemeEvent" | "DelayedDemeEventsPerSlots" => {
            let per_slots = name == "DelayedDemeEventsPerSlots";
            let rect = [a.int_or(-1), a.int_or(-1), a.int_or(-1), a.int_or(-1)];
            let delay = a.int_or(-1);
            let duration = a.int_or(if per_slots { -1 } else { 0 });
            let static_position = a.word().is_none_or(|w| as_int(&w) != 0);
            let spec = EventSpec {
                rect,
                delay,
                duration,
                static_position,
            };
            if per_slots {
                let slots = [a.int_or(1), a.int_or(1), a.int_or(1), a.int_or(1)];
                Action3::DelayedDemeEventsPerSlots { spec, slots }
            } else {
                Action3::DelayedDemeEvent {
                    spec,
                    total_events: a.int_or(1),
                }
            }
        }
        "Pred_DemeEventMoveCenter"
        | "Pred_DemeEventMoveBetweenTargets"
        | "Pred_DemeEventNUniqueIndividualsMovedIntoTarget" => Action3::DemeMovementPredicate {
            kind: match name {
                "Pred_DemeEventMoveCenter" => MovementPredType::MovedIntoCenter,
                "Pred_DemeEventMoveBetweenTargets" => MovementPredType::MovedBetweenTargets,
                _ => MovementPredType::NUnique,
            },
            times: a.int_or(1),
        },
        "DemeResourceThresholdPredicate" => {
            // The `assert(CountNumWords() == 3)` is compiled out.
            let resource_name = a.string_or("");
            let op = a.string_or("");
            let threshold = a.word().map(|w| as_double(&w));
            Action3::DemeResourceThresholdPredicate {
                resource_name,
                op,
                threshold,
            }
        }
        _ => return None,
    }))
}

/// `Systematics::Source(DUPLICATION, "")`
fn duplication_source() -> OrgSource {
    OrgSource::new(transmission::DUPLICATION, "", false)
}

impl World {
    /// `Process(ctx)` of an action of this module.
    pub(crate) fn run_action3(&mut self, action: &Action3) -> Result<(), String> {
        match action {
            Action3::SetNumInstBefore0Energy { new_value } => {
                self.cfg.num_cycles_exc_before_0_energy = *new_value;
                Ok(())
            }
            Action3::PrintDemesTotalAvgEnergy => self.a3_print_deme_total_avg_energy(),
            Action3::PrintDemeEnergySharingStats => self.a3_print_deme_energy_sharing_stats(),
            Action3::PrintDemeEnergyDistributionStats => {
                self.a3_print_deme_energy_distribution_stats()
            }
            Action3::DumpEnergyGrid { filename } => self.a3_dump_energy_grid(filename, false),
            Action3::DumpExecutionRatioGrid { filename } => {
                self.a3_dump_energy_grid(filename, true)
            }
            Action3::MixPopulation => self.mix_population(),
            Action3::NewTrial => self.new_trial(),
            Action3::CompeteOrganisms {
                comp_type,
                parents_survive,
            } => self.compete_organisms(*comp_type, *parents_survive),
            Action3::TrackAllMessages => {
                // QUIRK: every call registers one more predicate.
                let st = &mut self.actions2.a3;
                st.msg_predicates.push(MsgPredicate::AllData(Vec::new()));
                st.owners.push(None);
                Ok(())
            }
            Action3::DistributeData { id, mode, .. } => self.a3_distribute_data(*id, *mode),
            Action3::MeasureDemeNetworks | Action3::CompeteDemesByNetwork => {
                // Both reach `GetDeme(0).GetNetwork()` first.
                self.deme_network_abort();
                Ok(())
            }
            Action3::DelayedDemeEvent { spec, total_events } => {
                for d in 0..self.pop.demes.len() {
                    self.deme_set_cell_event(d, *spec, *total_events)?;
                }
                Ok(())
            }
            Action3::DelayedDemeEventsPerSlots { spec, slots } => {
                for d in 0..self.pop.demes.len() {
                    self.deme_set_cell_event_slots(d, *spec, *slots)?;
                }
                Ok(())
            }
            Action3::DemeMovementPredicate { kind, times } => {
                self.add_deme_pred(*kind, *times);
                Ok(())
            }
            Action3::DemeResourceThresholdPredicate {
                resource_name,
                op,
                threshold,
            } => {
                self.add_deme_resource_threshold_predicate(resource_name, op, *threshold);
                Ok(())
            }
            Action3::AttackDen { killprob, juvs_per } => self.a3_attack_den(*killprob, *juvs_per),
            Action3::IteratedConsensus {
                id,
                compete_period,
                replace,
                kill,
                restrict_range,
                dont_replace,
            } => self.a3_iterated_consensus(
                *id,
                *compete_period,
                [*replace, *kill, *restrict_range, *dont_replace],
            ),
            Action3::LoadStructuredSystematicsGroup { filename, role } => {
                self.a3_load_structured_systematics_group(role, filename)
            }
            Action3::SaveStructuredSystematicsGroup { filename, role } => {
                // The `<role>-<update>.ssg` name the C++ builds is unused.
                self.a3_save_structured_systematics_group(role, filename)
            }
            Action3::RaidDen {
                loss,
                res_id,
                units_per,
            } => self.a3_raid_den(*loss, *res_id, *units_per),
        }
    }

    /// Called once before the first update, after the event list is loaded:
    /// the `cActionDistributeData` constructors register their message
    /// counters, in event file order, and `cActionDistributeDataCheaply`
    /// sets DEME_NETWORK_TOPOLOGY_FITNESS to 4.
    pub(crate) fn a3_note_recorders(&mut self) {
        let mut cheaply = false;
        let ids: Vec<(usize, Weak<()>)> = self
            .events
            .entries()
            .iter()
            .filter_map(|e| match &e.action {
                crate::actions::Action::A3(Action3::DistributeData { id, alive, mode }) => {
                    cheaply |= *mode == DistributeMode::Cheaply;
                    Some((*id, Arc::downgrade(alive)))
                }
                _ => None,
            })
            .collect();
        if cheaply {
            // link length sum
            self.cfg.deme_network_topology_fitness = 4;
        }
        let consensus_ids: Vec<usize> = self
            .events
            .entries()
            .iter()
            .filter_map(|e| match &e.action {
                crate::actions::Action::A3(Action3::IteratedConsensus { id, .. }) => Some(*id),
                _ => None,
            })
            .collect();
        let hold = self.cfg.consensus_hold_time;
        let st = &mut self.actions2.a3;
        for id in consensus_ids {
            st.consensus.entry(id).or_insert_with(|| ConsensusState {
                hold,
                ..ConsensusState::default()
            });
        }
        for (id, alive) in ids {
            if st.distribute_counters.contains_key(&id) {
                continue;
            }
            st.distribute_counters.insert(id, st.msg_predicates.len());
            st.msg_predicates
                .push(MsgPredicate::CountDemeMessages(BTreeMap::new()));
            st.owners.push(Some(alive));
        }
    }

    /// `Err` when `cStats` still points at the message counter of a
    /// `DistributeData*` action whose event entry has been deleted (the
    /// event fired for the last time): the C++ then calls a virtual method
    /// of a destroyed object (the reference binary segfaults in
    /// `PrintPredicatedMessages`).
    fn a3_check_predicates_alive(&self) -> Result<(), String> {
        let dead = self
            .actions2
            .a3
            .owners
            .iter()
            .flatten()
            .any(|w| w.strong_count() == 0);
        if dead {
            return Err(
                "unsupported: a message predicate of a DistributeData event that \
                        has fired for the last time is used (C++: cStats keeps a pointer to \
                        the deleted action's cOrgMessagePred_CountDemeMessages)"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// `cStats::SentMessage(msg)`: every message predicate sees a message
    /// the organism in `sender_cell` delivered.
    pub(crate) fn a3_sent_message(&mut self, msg: &OrgMessage, sender_cell: i32) {
        if self.actions2.a3.msg_predicates.is_empty() {
            return;
        }
        if let Err(e) = self.a3_check_predicates_alive() {
            self.set_fatal(e);
            return;
        }
        let sender_deme = self.pop.demes[self.deme_of(sender_cell)].id;
        for p in &mut self.actions2.a3.msg_predicates {
            p.sent(msg, sender_deme);
        }
    }

    /// The predicate lines of `cStats::PrintPredicatedMessages`: each
    /// predicate prints and is reset.
    pub(crate) fn a3_print_and_reset_predicates(&mut self) -> Result<String, String> {
        self.a3_check_predicates_alive()?;
        let update = self.stats.update();
        let mut out = String::new();
        for p in &mut self.actions2.a3.msg_predicates {
            p.print(update, &mut out);
            p.reset();
        }
        Ok(out)
    }

    // ------------------------------------------------------------------
    // Iterated consensus
    // ------------------------------------------------------------------

    /// `cAssignRandomCellData::IsCellDataInDeme(data, deme)`.
    fn a3_cell_data_in_deme(&self, data: i32, deme_id: i32) -> Result<bool, String> {
        self.deme_compete
            .deme_to_id
            .get(&deme_id)
            .map(|set| set.contains(&data))
            .ok_or_else(|| {
                "unsupported: IteratedConsensus in a deme without assigned cell data (C++: \
                 dereferences deme_to_id.end())"
                    .to_string()
            })
    }

    /// `ConsensusSupport::max_support(deme)`: (support, opinion) of the
    /// opinion most organisms of deme `d` share, among those that are cell
    /// data of the deme.
    fn a3_max_support(&mut self, d: usize) -> Result<(u32, i32), String> {
        let deme_id = self.pop.demes[d].id;
        let cells: Vec<usize> = self.pop.demes[d]
            .cell_ids
            .iter()
            .map(|&c| c as usize)
            .collect();
        let mut opinions = Vec::new();
        for c in cells {
            if let Some(o) = self.pop.orgs[c].as_mut() {
                if o.state.has_opinion() {
                    opinions.push(o.state.get_opinion().0);
                }
            }
        }
        let mut counts: BTreeMap<i32, u32> = BTreeMap::new();
        let (mut support, mut opinion) = (0u32, 0i32);
        for op in opinions {
            if self.a3_cell_data_in_deme(op, deme_id)? {
                let n = counts.entry(op).or_insert(0);
                *n += 1;
                if *n > support {
                    support = *n;
                    opinion = op;
                }
            }
        }
        Ok((support, opinion))
    }

    /// `cAbstractMonitoringCompeteDemes::Process` with
    /// `cActionIteratedConsensus::Update` / `Fitness` / `Clear`. `opts` is
    /// `[_replace, _kill, _restrict_range, _dont_replace]`.
    fn a3_iterated_consensus(
        &mut self,
        id: usize,
        compete_period: i32,
        opts: [i32; 4],
    ) -> Result<(), String> {
        let num_demes = self.pop.demes.len();
        let mut st = self
            .actions2
            .a3
            .consensus
            .remove(&id)
            .ok_or("IteratedConsensus: state not registered")?;
        st.update_fitness.resize(num_demes, 0.0);
        let result = (|| -> Result<(), String> {
            for d in 0..num_demes {
                let u = self.a3_consensus_update(d, &mut st, opts)?;
                st.update_fitness[d] += u;
            }
            if compete_period == 0 {
                return Err(
                    "unsupported: IteratedConsensus with a compete period of 0 (C++: \
                            modulus by zero)"
                        .to_string(),
                );
            }
            if self.stats.update() % compete_period == 0 {
                let mut fitness = Vec::with_capacity(num_demes);
                for d in 0..num_demes {
                    let f = f64::from(self.a3_max_support(d)?.0 + 1);
                    fitness.push((st.update_fitness[d] + f).powf(2.0));
                }
                self.compete_demes(&fitness)?;
                st.update_fitness.clear();
                st.state.clear();
            }
            Ok(())
        })();
        self.actions2.a3.consensus.insert(id, st);
        result
    }

    /// `cActionIteratedConsensus::Update(deme)`
    fn a3_consensus_update(
        &mut self,
        d: usize,
        st: &mut ConsensusState,
        opts: [i32; 4],
    ) -> Result<f64, String> {
        let (support, opinion) = self.a3_max_support(d)?;
        let deme_id = self.pop.demes[d].id;
        let size = self.pop.demes[d].size();
        // if we're not at consensus, set our current time to 0:
        if support < size as u32 {
            if let Some(h) = st.state.get_mut(&deme_id) {
                h.current = 0;
            }
            return Ok(0.0);
        }
        let h = st.state.entry(deme_id).or_insert(HoldState {
            opinion,
            current: 0,
            max: 0,
        });
        if opinion != h.opinion {
            h.opinion = opinion;
            h.current = 0;
        }
        h.current += 1;
        if h.current >= st.hold {
            self.a3_consensus_reached(d, opinion, opts)?;
            st.state.remove(&deme_id);
            return Ok(f64::from(size));
        }
        if h.current > h.max {
            h.max = h.current;
            return Ok(f64::from(size));
        }
        Ok(0.0)
    }

    /// `cAssignRandomCellData::ReplaceCellData(old_data, new_data, deme)`:
    /// the deme-relative ids of the cells whose data changed.
    fn a3_replace_cell_data(
        &mut self,
        d: usize,
        old_data: i32,
        new_data: i32,
    ) -> Result<Vec<i32>, String> {
        let update = self.stats.update();
        let mut ids = Vec::new();
        for i in 0..self.pop.demes[d].size() {
            let cell = self.pop.demes[d].cell_id(i);
            if self.pop.cells[cell as usize].cell_data.contents == old_data {
                self.set_cell_data(cell, new_data, -1, update);
                ids.push(i);
            }
        }
        let deme_id = self.pop.demes[d].id;
        let set = self.deme_compete.deme_to_id.get_mut(&deme_id).ok_or(
            "unsupported: IteratedConsensus in a deme without assigned cell data (C++: \
             dereferences deme_to_id.end())",
        )?;
        set.remove(&old_data);
        set.insert(new_data);
        Ok(ids)
    }

    /// `cActionIteratedConsensus::ConsensusReached(deme, support, ctx)`
    fn a3_consensus_reached(
        &mut self,
        d: usize,
        opinion: i32,
        opts: [i32; 4],
    ) -> Result<(), String> {
        let [replace, kill, restrict_range, dont_replace] = opts;
        let deme_id = self.pop.demes[d].id;
        // `cAssignRandomCellData::CellIDFromData(opinion, deme)`
        let cell_id = (0..self.pop.demes[d].size())
            .find(|&i| {
                let c = self.pop.demes[d].cell_id(i);
                self.pop.cells[c as usize].cell_data.contents == opinion
            })
            .unwrap_or(-1);
        self.stats.consensus_reached(deme_id, opinion, cell_id);
        if dont_replace != 0 {
            return Ok(());
        }
        let (mut min_data, mut max_data) = (0, i32::MAX);
        if restrict_range != 0 {
            let set = self.deme_compete.deme_to_id.get(&deme_id).ok_or(
                "unsupported: IteratedConsensus in a deme without assigned cell data (C++: \
                 dereferences deme_to_id.end())",
            )?;
            let (Some(&lo), Some(&hi)) = (set.iter().next(), set.iter().next_back()) else {
                return Err(
                    "unsupported: IteratedConsensus with no cell data in the deme (C++: \
                            dereferences begin() of an empty set)"
                        .to_string(),
                );
            };
            min_data = lo.wrapping_add(1);
            max_data = hi.wrapping_sub(1);
        }
        let new_data = self.rng.get_int_range(min_data, max_data);
        let mut cell_ids = self.a3_replace_cell_data(d, opinion, new_data)?;
        // Now reset the others:
        for _ in 1..replace {
            let size = self.pop.demes[d].size();
            let rel = self.rng.get_int(size);
            let cell = self.pop.demes[d].cell_id(rel);
            let cell_data = self.pop.cells[cell as usize].cell_data.contents;
            let new_data = self.rng.get_int_range(min_data, max_data);
            let extra = self.a3_replace_cell_data(d, cell_data, new_data)?;
            cell_ids.extend(extra);
        }
        if kill != 0 {
            // This is probably only compatible with the "old-style" germline
            for rel in cell_ids {
                let cell = self.pop.demes[d].cell_id(rel);
                if !self.is_occupied(cell) {
                    continue;
                }
                // QUIRK: the deme-relative id is passed to `InjectGenome` as
                // an absolute cell id.
                if self.cfg.demes_use_germline != 0 {
                    self.kill_organism(cell, None);
                    let genome = self.pop.demes[d].germline.last().cloned().ok_or(
                        "IteratedConsensus: empty germline (C++: GetLatest of an empty germline)",
                    )?;
                    let src = OrgSource::new(transmission::DUPLICATION, "germline", true);
                    self.inject_genome(rel, src, &genome, 0, true)?;
                } else {
                    let genome = self.pop.orgs[cell as usize]
                        .as_ref()
                        .map(|o| o.state.genome().clone())
                        .ok_or("IteratedConsensus: occupied cell without an organism")?;
                    self.kill_organism(cell, None);
                    let src = OrgSource::new(transmission::DUPLICATION, "random", true);
                    self.inject_genome(rel, src, &genome, 0, true)?;
                }
                self.deme_post_injection(d, cell);
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Structured systematics groups
    // ------------------------------------------------------------------

    /// `cPopulation::SaveStructuredSystematicsGroup(role, filename)`: one
    /// row per group of `role` that a living organism or one of its
    /// parasites belongs to (in `Apto::Map` order), with the cells it
    /// occupies.
    fn a3_save_structured_systematics_group(
        &mut self,
        role: &str,
        filename: &str,
    ) -> Result<(), String> {
        if filename.is_empty() {
            return Err(
                "unsupported: SaveStructuredSystematicsGroup without a role (C++: the \
                        file name is empty and the reference binary segfaults)"
                    .to_string(),
            );
        }
        let mut df = self.out.create_with_path(filename)?;
        df.set_file_type("systematics_data");
        df.write_comment("Structured Systematics Group Save");
        df.write_time_stamp();

        let role_idx = self.systematics.arbiter_for_role(role);
        let mut index: AptoMap<i32, usize> = AptoMap::new();
        let mut groups: Vec<(GroupRef, Vec<usize>)> = Vec::new();
        for cell in 0..self.pop.cells.len() {
            let Some(org) = self.pop.orgs[cell].as_ref() else {
                continue;
            };
            let Some(r) = role_idx else {
                continue;
            };
            // Handle any parasites, then the organism itself.
            let mut found: Vec<GroupRef> = org
                .state
                .parasites
                .iter()
                .filter_map(|p| crate::parasites::parasite_tag(p)?.membership().group(r))
                .collect();
            if let Some(g) = self.pop.membership[cell].group(r) {
                found.push(g);
            }
            for g in found {
                let Some(id) = self.systematics.group_id(g) else {
                    continue;
                };
                match index.find(&id) {
                    Some(&i) => groups[i].1.push(cell),
                    None => {
                        groups.push((g, vec![cell]));
                        index.set(&id, groups.len() - 1);
                    }
                }
            }
        }

        for &i in index.values() {
            let (g, cells) = &groups[i];
            self.systematics.group_legacy_save(*g, &mut df);
            let cellstr = cells
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            df.write_with_format(cellstr, "Occupied Cell IDs", "cells");
            df.endl();
        }
        Ok(())
    }

    /// `cPopulation::LoadStructuredSystematicsGroup(ctx, role, filename)`
    /// and the action's error handling: each line becomes a group of the
    /// role's arbiter (`LegacyLoad`), and every living organism in its
    /// `cells` is classified into it. QUIRK: `Unit::AddClassification`
    /// refuses a second group of a role the organism already has, so the
    /// group counts the organism without the organism pointing at it.
    fn a3_load_structured_systematics_group(
        &mut self,
        role: &str,
        filename: &str,
    ) -> Result<(), String> {
        let input = InitFile::new(filename, &self.working_dir, None, None);
        for line in input.feedback().render().lines() {
            self.action_state.stdout.push_str(line);
            self.action_state.stdout.push('\n');
        }
        if !input.was_opened() {
            let msg = "failed to load structured systematics group information";
            self.abort_run(&format!("error: {msg}\n"), INVALID_CONFIG, msg);
            return Ok(());
        }
        let role_idx = self.systematics.arbiter_for_role(role).ok_or_else(|| {
            format!(
                "unsupported: LoadStructuredSystematicsGroup for role '{role}', which has no \
                 arbiter (C++: NULL dereference)"
            )
        })?;
        for line_id in 0..input.num_lines() {
            let props = input.get_line_as_dict(line_id as i32);
            let grp = self.systematics.legacy_load(role_idx, &props)?;
            let mut cellstr = props.get("cells").cloned().unwrap_or_default();
            while !cellstr.is_empty() {
                let cell_id = as_int(&avida_config::string_util::pop(&mut cellstr, ','));
                let c = usize::try_from(cell_id)
                    .ok()
                    .filter(|&c| c < self.pop.cells.len())
                    .ok_or_else(|| {
                        format!(
                            "LoadStructuredSystematicsGroup: cell {cell_id} is outside the \
                             world (C++: out-of-bounds access)"
                        )
                    })?;
                let Some(org) = self.pop.orgs[c].as_ref() else {
                    continue;
                };
                let grp = grp.ok_or(
                    "unsupported: LoadStructuredSystematicsGroup: the arbiter loaded no group \
                     (C++: NULL dereference)",
                )?;
                let source = systematics_source(&org.state.src);
                let unit = UnitInfo {
                    source: &source,
                    genome: org.state.genome(),
                    generation: org.state.phenotype.generation,
                };
                if let Some(g) = self.systematics.group_classify_new_unit(grp, &unit, None) {
                    self.pop.membership[c].add_classification(g);
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Dens
    // ------------------------------------------------------------------

    /// `Driver().Feedback().Error(...)` when avatars are off (the action
    /// goes on), and the habitat of each resource.
    fn a3_den_prologue(&mut self, name: &str) -> Vec<i32> {
        if self.cfg.use_avatars <= 0 {
            self.action_state
                .stdout
                .push_str(&format!("error: Den {name} requires use of avatars.\n"));
        }
        self.env
            .resource_lib()
            .resources()
            .iter()
            .map(|r| r.habitat)
            .collect()
    }

    /// `cActionAttackDen::Process`: on every den cell (a habitat 3 or 4
    /// resource above 0), each unguarded juvenile avatar's organism dies
    /// with probability `killprob`.
    fn a3_attack_den(&mut self, killprob: f64, juvs_per: i32) -> Result<(), String> {
        let habitats = self.a3_den_prologue("AttackDen");
        let juv_age = self.cfg.juv_period;
        for i in 0..self.pop.cells.len() as i32 {
            if !self.avatars.has_av(i) {
                continue;
            }
            let cell_res = self
                .pop
                .resource_count
                .get_cell_resources(i, &mut self.rng, &mut self.res_env)
                .to_vec();
            for (j, &amount) in cell_res.iter().enumerate() {
                let habitat = habitats.get(j).copied().unwrap_or(0);
                if !((habitat == 4 || habitat == 3) && amount > 0.0) {
                    continue;
                }
                let mut juvs = Vec::new();
                let mut num_guards = 0;
                for id in self.avatars.cell_avs(i) {
                    let org = self
                        .cell_of_org(id)
                        .and_then(|c| self.pop.orgs[c as usize].as_ref())
                        .ok_or("AttackDen: avatar without a living organism")?;
                    if org.state.phenotype.time_used < juv_age {
                        juvs.push(id);
                    } else if org.state.guard {
                        num_guards += 1;
                    }
                }
                if juvs.is_empty() {
                    break;
                }
                let guarded_juvs = num_guards * juvs_per;
                let unguarded_juvs = juvs.len() as i32 - guarded_juvs;
                for &id in juvs.iter().take(unguarded_juvs.max(0) as usize) {
                    if self.rng.p(killprob) {
                        // `juvs[k]->Die(ctx)`
                        let cell = self.cell_of_org(id).ok_or(
                            "unsupported: AttackDen kills an organism twice (C++: Die on a \
                             deleted organism)",
                        )?;
                        self.kill_organism(cell, None);
                        self.stats.inc_juv_killed();
                    }
                }
                break; // only do this once if two dens overlap
            }
        }
        Ok(())
    }

    /// `cActionRaidDen::Process`: on every den cell holding some of
    /// resource `res_id`, the part not covered by `units_per` per adult
    /// avatar loses the fraction `loss`.
    fn a3_raid_den(&mut self, loss: f64, res_id: i32, units_per: f64) -> Result<(), String> {
        let habitats = self.a3_den_prologue("RaidDen");
        let juv_age = self.cfg.juv_period;
        for i in 0..self.pop.cells.len() as i32 {
            let cell_res = self
                .pop
                .resource_count
                .get_cell_resources(i, &mut self.rng, &mut self.res_env)
                .to_vec();
            for (j, &amount) in cell_res.iter().enumerate() {
                let habitat = habitats.get(j).copied().unwrap_or(0);
                if !((habitat == 4 || habitat == 3) && amount > 0.0) {
                    continue;
                }
                let target = usize::try_from(res_id)
                    .ok()
                    .and_then(|r| cell_res.get(r).copied())
                    .ok_or("RaidDen: no such resource (C++: out-of-range read)")?;
                if target <= 0.0 {
                    break;
                }
                let mut num_guards = 0;
                for id in self.avatars.cell_avs(i) {
                    let org = self
                        .cell_of_org(id)
                        .and_then(|c| self.pop.orgs[c as usize].as_ref())
                        .ok_or("RaidDen: avatar without a living organism")?;
                    if org.state.phenotype.time_used >= juv_age {
                        num_guards += 1;
                    }
                }
                let guarded_res = f64::from(num_guards) * units_per;
                let unguarded_res = target - guarded_res;
                let mut res_change = vec![0.0; cell_res.len()];
                res_change[res_id as usize] = -unguarded_res * loss;
                self.pop.resource_count.modify_cell(
                    &mut self.rng,
                    &mut self.res_env,
                    &res_change,
                    i,
                );
                break; // only do this once if two dens overlap
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Energy printers
    // ------------------------------------------------------------------

    /// The occupant of `cell` for a printer (never the running organism:
    /// actions run between updates).
    fn a3_org(&self, cell: i32) -> Result<Option<&Organism>, String> {
        if !self.is_occupied(cell) {
            return Ok(None);
        }
        self.pop.orgs[cell as usize]
            .as_deref()
            .map(Some)
            .ok_or_else(|| "energy printer reached the running organism".to_string())
    }

    /// `cPopulation::PrintDemeTotalAvgEnergy(ctx)`
    fn a3_print_deme_total_avg_energy(&mut self) -> Result<(), String> {
        let mut avg_energy = DoubleSum::new();
        for d in 0..self.pop.demes.len() {
            avg_energy.add(self.deme_calculate_total_energy(d));
        }
        if let Some(f) = self.fatal_message() {
            return Err(f);
        }
        let update = self.stats.update();
        let df = self.out.static_with_path("deme_totalAvgEnergy.dat")?;
        df.write_comment("Average energy for demes in the population");
        df.write_time_stamp();
        df.write(update, "update");
        df.write(avg_energy.average(), "Total Average Energy");
        df.endl();
        Ok(())
    }

    /// `cPopulation::PrintDemeEnergySharingStats()`. QUIRK: the file is
    /// `deme_enerty_sharing.dat`.
    ///
    /// The C++ never initialises `cPhenotype::num_energy_donations`,
    /// `num_energy_receptions` and `num_energy_applications` (neither the
    /// constructor nor `SetupOffspring` / `SetupInject` set them), so the
    /// printer sums uninitialised memory: a run of the reference binary
    /// whose organisms donate energy shows counts that carry over to
    /// organisms born after a deme competition. The port stops here
    /// instead; the rest of the printer is kept for when that is fixed.
    fn a3_print_deme_energy_sharing_stats(&mut self) -> Result<(), String> {
        if self.pop.num_organisms > 0 {
            return Err("unsupported: PrintDemeEnergySharingStats (C++: \
                        cPhenotype::num_energy_donations / num_energy_receptions / \
                        num_energy_applications are never initialised, the printer \
                        reads uninitialised memory)"
                .to_string());
        }
        let num_demes = self.pop.demes.len() as f64;
        let mut num_requestors = 0.0;
        let mut num_donors = 0.0;
        let mut num_receivers = 0.0;
        let mut num_donations = 0.0;
        let mut num_receptions = 0.0;
        let mut num_applications = 0.0;
        let mut amount_donated = 0.0;
        let mut amount_received = 0.0;
        let mut amount_applied = 0.0;
        for d in 0..self.pop.demes.len() {
            for i in 0..self.pop.demes[d].size() {
                let cell = self.pop.demes[d].cell_id(i);
                let Some(o) = self.a3_org(cell)? else {
                    continue;
                };
                let p = &o.state.phenotype;
                if p.is_energy_requestor {
                    num_requestors += 1.0;
                }
                if p.is_energy_donor {
                    num_donors += 1.0;
                }
                if p.is_energy_receiver {
                    num_receivers += 1.0;
                }
                num_donations += f64::from(p.num_energy_donations);
                num_receptions += f64::from(p.num_energy_receptions);
                num_applications += f64::from(p.num_energy_applications);
                amount_donated += p.total_energy_donated;
                amount_received += p.total_energy_received;
                amount_applied += p.total_energy_applied;
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path("deme_enerty_sharing.dat")?;
        df.write_comment("Average energy donation statistics for each deme in population");
        df.write_time_stamp();
        df.write(update, "update");
        df.write(
            num_requestors / num_demes,
            "Average number of organisms that have requested energy",
        );
        df.write(
            num_donors / num_demes,
            "Average number of organisms that have donated energy",
        );
        df.write(
            num_receivers / num_demes,
            "Average number of organisms that have received energy",
        );
        df.write(
            num_donations / num_demes,
            "Average number of donations per deme",
        );
        df.write(
            num_receptions / num_demes,
            "Average number of receipts per deme",
        );
        df.write(
            num_applications / num_demes,
            "Average number of applications per deme",
        );
        df.write(
            amount_donated / num_demes,
            "Average total amount of energy donated per deme",
        );
        df.write(
            amount_received / num_demes,
            "Average total amount of energy received per deme",
        );
        df.write(
            amount_applied / num_demes,
            "Average total amount of donated energy applied per deme",
        );
        df.endl();
        Ok(())
    }

    /// `cPopulation::PrintDemeEnergyDistributionStats(ctx)`: the stored
    /// energy of each occupant, or the energy resources of an empty cell.
    fn a3_print_deme_energy_distribution_stats(&mut self) -> Result<(), String> {
        let num_demes = self.pop.demes.len() as i32;
        let mut overall_average = DoubleSum::new();
        let mut overall_variance = DoubleSum::new();
        let mut overall_stddev = DoubleSum::new();
        for d in 0..self.pop.demes.len() {
            let mut dist = DoubleSum::new();
            for i in 0..self.pop.demes[d].size() {
                let cell = self.pop.demes[d].cell_id(i);
                let energy = match self.a3_org(cell)? {
                    Some(o) => o.state.phenotype.energy_store,
                    None => self.deme_cell_energy(d, cell),
                };
                dist.add(energy);
            }
            overall_average.add(dist.average());
            overall_variance.add(dist.variance());
            overall_stddev.add(dist.std_deviation());
        }
        let update = self.stats.update();
        let comment = format!(
            "Average distribution of energy among cells in each of {num_demes} {} x {} demes",
            self.pop.world_x,
            self.pop.world_y / num_demes
        );
        let df = self.out.static_with_path("deme_energy_distribution.dat")?;
        df.write_comment(&comment);
        df.write_time_stamp();
        df.write(update, "Update");
        df.write(overall_average.average(), "Average of Average Energy Level");
        df.write(
            overall_variance.average(),
            "Average of Energy Level Variance",
        );
        df.write(
            overall_stddev.average(),
            "Average of Energy Level Standard Deviations",
        );
        df.endl();
        Ok(())
    }

    /// `cActionDumpEnergyGrid::Process` (`exe_ratio == false`: stored
    /// energy, 0 for an empty cell, a file created anew) and
    /// `cActionDumpExecutionRatioGrid::Process` (`GetEnergyUsageRatio()`,
    /// 1 for an empty cell, a static file that later calls append to).
    fn a3_dump_energy_grid(&mut self, filename: &str, exe_ratio: bool) -> Result<(), String> {
        let filename = if filename.is_empty() {
            let base = if exe_ratio {
                "grid_exe_ratio"
            } else {
                "grid_energy"
            };
            format!("{base}.{}.dat", self.stats.update())
        } else {
            filename.to_string()
        };
        let (world_x, world_y) = (self.pop.world_x, self.pop.world_y);
        let mut text = String::new();
        for i in 0..world_y {
            for j in 0..world_x {
                let v = match self.a3_org(i * world_x + j)? {
                    Some(o) if exe_ratio => o.state.phenotype.execution_ratio,
                    Some(o) => o.state.phenotype.energy_store,
                    None if exe_ratio => 1.0,
                    None => 0.0,
                };
                text.push_str(&fmt_double(v));
                text.push(' ');
            }
            text.push('\n');
        }
        if exe_ratio {
            let df = self.out.static_with_path(&filename)?;
            df.ofstream()
                .write_all(text.as_bytes())
                .map_err(|e| e.to_string())?;
        } else {
            let mut df = self.out.create_with_path(&filename)?;
            df.ofstream()
                .write_all(text.as_bytes())
                .map_err(|e| e.to_string())?;
            df.flush();
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // MixPopulation
    // ------------------------------------------------------------------

    /// `cPopulation::MixPopulation(ctx)`: shuffle the occupants (and empty
    /// slots) of all cells. Each cell then `RemoveOrganism`s and
    /// `InsertOrganism`s its new occupant, so the occupant gets the cell's
    /// speculative waste, keeps its previously-seen cell, and the
    /// scheduler is told the new merit (0 for an empty cell). Nothing else
    /// moves with it: cell inputs, visits and the demes' organism counts
    /// stay as they were.
    pub fn mix_population(&mut self) -> Result<(), String> {
        if self.cfg.use_avatars != 0 {
            return Err(
                "unsupported: MixPopulation with USE_AVATARS (C++: the avatars stay \
                        in their cells while the organisms move)"
                    .to_string(),
            );
        }
        let n = self.pop.cells.len();
        // `population[i]` came from cell `perm[i]`.
        let mut perm: Vec<usize> = (0..n).collect();
        libcxx_random_shuffle(&mut perm, &mut self.rng);

        let mut old_orgs = std::mem::take(&mut self.pop.orgs);
        let mut old_membership = std::mem::take(&mut self.pop.membership);
        let old_live_index = std::mem::take(&mut self.pop.live_index);
        let old_prev_seen = std::mem::take(&mut self.host.prev_seen);
        let old_prev_task = std::mem::take(&mut self.host.prev_task);
        let old_num_task_cells = std::mem::take(&mut self.host.num_task_cells);
        let mut new_cell_of = vec![0i32; n];
        for (i, &from) in perm.iter().enumerate() {
            new_cell_of[from] = i as i32;
            self.pop.orgs.push(old_orgs[from].take());
            self.pop
                .membership
                .push(std::mem::take(&mut old_membership[from]));
            self.pop.live_index.push(old_live_index[from]);
            self.host.prev_seen.push(old_prev_seen[from]);
            self.host.prev_task.push(old_prev_task[from]);
            self.host.num_task_cells.push(old_num_task_cells[from]);
        }
        for i in 0..n {
            if let Ok(idx) = usize::try_from(self.pop.live_index[i]) {
                self.pop.live_org_list[idx] = i as i32;
            }
        }
        self.pop
            .groups
            .remap(&|k| usize::try_from(k).map_or(k, |c| new_cell_of[c]));

        for (i, &old_occupant) in new_cell_of.iter().enumerate() {
            // `cell_array[i].RemoveOrganism(ctx)`: the cell's old occupant,
            // already reinserted (with the energy it took up there) when
            // its new cell comes first.
            if let Some(o) = self.pop.orgs[old_occupant as usize].as_ref() {
                let energy = o.state.phenotype.energy_store;
                self.cell_remove_energy(i as i32, energy);
            }
            let merit = if self.pop.orgs[i].is_none() {
                0.0
            } else {
                // `cPopulationCell::InsertOrganism`
                self.stats.m_spec_waste += self.pop.spec_state[i];
                self.pop.spec_state[i] = 0;
                if self.host.prev_seen[i] == -1 {
                    self.host.prev_seen[i] = i as i32;
                }
                self.cell_insert_energy(i as i32, None);
                self.pop.orgs[i]
                    .as_ref()
                    .map_or(0.0, |o| o.state.phenotype.merit.get_double())
            };
            self.adjust_schedule(i as i32, merit);
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Trials
    // ------------------------------------------------------------------

    /// `cPopulation::NewTrial(ctx)`: every organism that has used time in
    /// this trial records its trial fitness and starts a new trial; then
    /// the statistics are recalculated (`cStats::ProcessUpdate`,
    /// `ProcessPostUpdate`).
    pub fn new_trial(&mut self) -> Result<(), String> {
        for c in 0..self.pop.cells.len() {
            let spec_state = self.pop.spec_state[c];
            let Some(mut org) = self.pop.orgs[c].take() else {
                continue;
            };
            // Don't continue if the time used was zero
            if org.state.phenotype.trial_time_used != 0 {
                // Correct gestation time for speculative execution
                let p = &mut org.state.phenotype;
                p.trial_time_used -= spec_state;
                p.time_used -= spec_state;

                // `cOrganism::NewTrial()`
                org.state.phenotype.new_trial(&self.cfg);
                org.state.input_pointer = 0;
                org.state.input_buf.clear();
                org.state.output_buf.clear();
                {
                    let mut iface = PopulationInterface {
                        w: self,
                        cell_id: c as i32,
                    };
                    let Organism { state, hardware } = &mut *org;
                    hardware.reset(state, &mut iface);
                }
                self.pop.spec_state[c] = 0;
            }
            self.pop.orgs[c] = Some(org);
        }

        // Recalculate the stats immediately, so that if they are printed
        // before a new update is processed, they accurately reflect this
        // trial only...
        self.stats.process_update(self.cfg.ave_time_slice);
        self.process_post_update();
        match self.fatal_message() {
            Some(f) => Err(f),
            None => Ok(()),
        }
    }

    /// `cPopulation::CompeteOrganisms(ctx, competition_type,
    /// parents_survive)`
    pub fn compete_organisms(
        &mut self,
        competition_type: i32,
        parents_survive: i32,
    ) -> Result<(), String> {
        self.new_trial()?;

        let num_cells = self.pop.cells.len();
        let verbosity = self.cfg.verbosity;
        let mut total_fitness = 0.0;
        let mut org_fitness = vec![0.0f64; num_cells];
        let mut lowest_fitness = -1.0;
        let mut highest_fitness = -1.0;
        let mut lowest_fitness_copied = -1.0;
        let mut average_fitness_copied = 0.0;
        let mut highest_fitness_copied = -1.0;
        let mut different_orgs_copied = 0;
        let mut num_competed_orgs = 0;
        let mut num_trials: i32 = -1;

        let dynamic_scaling = match competition_type {
            3 => 1,
            4 => 2,
            _ => 0,
        };

        // How many trials were there? -- same for every organism
        for c in 0..num_cells {
            let Some(org) = self.pop.orgs[c].as_mut() else {
                continue;
            };
            let p = &mut org.state.phenotype;
            // We trigger a lot of asserts if the copied size is zero...
            p.child_copied_size = p.genome_length;
            let n = p.cur_trial_fitnesses.len() as i32;
            if num_trials != -1 && num_trials != n {
                self.abort_run(
                    "The number of trials is not the same for every organism in the \
                     population.\nYou need to remove all normal ways of replicating for \
                     CompeteOrganisms to work correctly.\n",
                    1,
                    "CompeteOrganisms: different trial counts (C++: exit(1))",
                );
                return Ok(());
            }
            num_trials = n;
        }

        // If there weren't any trials then end here
        if num_trials == 0 {
            return Ok(());
        }
        if num_trials < 0 {
            return Err(
                "unsupported: CompeteOrganisms on an empty population (C++: arrays \
                        of size -1)"
                    .to_string(),
            );
        }
        let num_trials = num_trials as usize;

        if verbosity > VERBOSE_SILENT {
            self.action_state.stdout.push_str("==Compete Organisms==\n");
        }

        let mut min_trial = vec![0.0f64; num_trials];
        let mut max_trial = vec![0.0f64; num_trials];
        let mut avg_trial = vec![0.0f64; num_trials];
        let mut init = false;
        // What is the min and max fitness in each trial
        for c in 0..num_cells {
            let Some(org) = self.pop.orgs[c].as_ref() else {
                continue;
            };
            num_competed_orgs += 1;
            let tf = &org.state.phenotype.cur_trial_fitnesses;
            for t in 0..num_trials {
                if !init || min_trial[t] > tf[t] {
                    min_trial[t] = tf[t];
                }
                if !init || max_trial[t] < tf[t] {
                    max_trial[t] = tf[t];
                }
                avg_trial[t] += tf[t];
            }
            init = true;
        }
        // divide averages for each trial
        for a in avg_trial.iter_mut() {
            *a /= f64::from(num_competed_orgs);
        }
        if verbosity > VERBOSE_SILENT && num_trials > 1 {
            for t in 0..num_trials {
                self.action_state.stdout.push_str(&format!(
                    "Trial #{t} Min Fitness = {}, Avg fitness = {} Max Fitness = {}\n",
                    self.cout_double(min_trial[t]),
                    self.cout_double(avg_trial[t]),
                    self.cout_double(max_trial[t])
                ));
            }
        }

        let mut using_trials = true;
        for (c, slot) in org_fitness.iter_mut().enumerate() {
            let Some(org) = self.pop.orgs[c].as_ref() else {
                continue;
            };
            let p = &org.state.phenotype;
            let mut tf = p.cur_trial_fitnesses.clone();
            // If there are no trial fitnesses...use the actual fitness.
            if tf.is_empty() {
                using_trials = false;
                tf.push(p.fitness);
            }
            let n = tf.len() as f64;
            let fitness = match competition_type {
                // Geometric Mean (as logs to avoid overflow)
                0 | 3 | 4 => {
                    let mut f = 0.0;
                    for &v in &tf {
                        f += v.ln();
                    }
                    f /= n;
                    f.exp()
                }
                // Product
                5 => {
                    let mut f = 0.0;
                    for &v in &tf {
                        f += v.ln();
                    }
                    f.exp()
                }
                // Geometric Mean of normalized values
                1 => {
                    let mut f = 1.0;
                    for (t, &v) in tf.iter().enumerate() {
                        f *= v / max_trial[t];
                    }
                    ((1.0 / n) * f.ln()).exp()
                }
                // Arithmetic Mean
                2 => {
                    let mut f = 0.0;
                    for &v in &tf {
                        f += v;
                    }
                    f / n
                }
                _ => {
                    let e = "Unknown CompeteOrganisms method";
                    self.abort_run(&format!("error: {e}\n"), INVALID_CONFIG, e);
                    return Ok(());
                }
            };
            if verbosity >= VERBOSE_DETAILS {
                self.action_state.stdout.push_str(&format!(
                    "Trial fitness in cell {c} = {}\n",
                    self.cout_double(fitness)
                ));
            }
            *slot = fitness;
            total_fitness += fitness;
            if highest_fitness == -1.0 || fitness > highest_fitness {
                highest_fitness = fitness;
            }
            if lowest_fitness == -1.0 || fitness < lowest_fitness {
                lowest_fitness = fitness;
            }
        }
        let average_fitness = total_fitness / f64::from(num_competed_orgs);

        if dynamic_scaling == 1 {
            // Rescale by the geometric mean of the difference from the top
            // score and the average
            let mut dynamic_factor = 1.0;
            if highest_fitness > 0.0 {
                dynamic_factor = 2.0 / highest_fitness;
            }
            total_fitness = 0.0;
            for (c, f) in org_fitness.iter_mut().enumerate() {
                if self.pop.orgs[c].is_some() {
                    *f *= dynamic_factor;
                    total_fitness += *f;
                }
            }
        } else if dynamic_scaling == 2 {
            // Rescale geometric mean to 1
            let mut num_org = 0;
            let mut dynamic_factor = 1.0;
            for (c, &f) in org_fitness.iter().enumerate() {
                if self.pop.orgs[c].is_some() && f > 0.0 {
                    num_org += 1;
                    dynamic_factor += f.ln();
                }
            }
            self.action_state.stdout.push_str(&format!(
                "Scaling factor = {}\n",
                self.cout_double(dynamic_factor)
            ));
            if num_org > 0 {
                dynamic_factor = (dynamic_factor / f64::from(num_org)).exp();
            }
            self.action_state.stdout.push_str(&format!(
                "Scaling factor = {}\n",
                self.cout_double(dynamic_factor)
            ));
            total_fitness = 0.0;
            for (c, f) in org_fitness.iter_mut().enumerate() {
                if self.pop.orgs[c].is_some() {
                    *f /= dynamic_factor;
                    total_fitness += *f;
                }
            }
        }

        // Pick which orgs should be in the next generation. (Filling all
        // cells.) QUIRK: a draw no running total exceeds leaves its slot
        // at 0 (value-initialised array).
        let mut new_orgs = vec![0usize; num_cells];
        for (i, slot) in new_orgs.iter_mut().enumerate() {
            let birth_choice = self.rng.get_double_max(total_fitness);
            let mut test_total = 0.0;
            for (test_org, &f) in org_fitness.iter().enumerate() {
                test_total += f;
                if birth_choice < test_total {
                    *slot = test_org;
                    if verbosity >= VERBOSE_DETAILS {
                        self.action_state
                            .stdout
                            .push_str(&format!("Propagating from cell {test_org} to {i}\n"));
                    }
                    if highest_fitness_copied == -1.0 || f > highest_fitness_copied {
                        highest_fitness_copied = f;
                    }
                    if lowest_fitness_copied == -1.0 || f < lowest_fitness_copied {
                        lowest_fitness_copied = f;
                    }
                    average_fitness_copied += f;
                    break;
                }
            }
        }
        // average assumes we fill all cells.
        average_fitness_copied /= num_cells as f64;

        // Track how many of each org we should have.
        let mut org_count = vec![0i32; num_cells];
        for &o in &new_orgs {
            org_count[o] += 1;
        }

        // Reset organism phenotypes that have successfully divided! Must do
        // before injecting children.
        let update = self.stats.update();
        for (c, &count) in org_count.iter().enumerate() {
            if verbosity >= VERBOSE_DETAILS {
                self.action_state.stdout.push_str(&format!(
                    "Cell {c} has {count} copies in the next generation\n"
                ));
            }
            if count > 0 {
                different_orgs_copied += 1;
                let World {
                    pop,
                    cfg,
                    rng,
                    stats,
                    ..
                } = self;
                let org = pop.orgs[c]
                    .as_mut()
                    .ok_or("CompeteOrganisms: an empty cell is copied (C++: NULL dereference)")?;
                let size = org.state.genome().representation().size();
                if using_trials {
                    org.state
                        .phenotype
                        .trial_divide_reset(cfg, size, rng, stats);
                } else {
                    org.state
                        .phenotype
                        .divide_reset(cfg, size, update, rng, stats);
                }
            }
        }

        let mut is_init = vec![false; num_cells];
        // Copy orgs until all org counts are 1.
        while let Some(from) = (0..num_cells).find(|&c| org_count[c] > 1) {
            let to = (0..num_cells)
                .find(|&c| org_count[c] == 0)
                .ok_or("CompeteOrganisms: no cell to copy into (C++: out-of-range read)")?;
            org_count[from] -= 1;
            org_count[to] += 1;
            if verbosity >= VERBOSE_DETAILS {
                self.action_state
                    .stdout
                    .push_str(&format!("Injecting Offspring {from} to {to}\n"));
            }
            self.compete_organisms_construct_offspring(to as i32, from as i32)?;
            is_init[to] = true;
        }

        if parents_survive == 0 {
            // Now create children from remaining cells into themselves
            for (c, &init) in is_init.iter().enumerate() {
                if init {
                    continue;
                }
                if verbosity >= VERBOSE_DETAILS {
                    self.action_state
                        .stdout
                        .push_str(&format!("Re-injecting Self {c} to {c}\n"));
                }
                self.compete_organisms_construct_offspring(c as i32, c as i32)?;
            }
        }

        if verbosity > VERBOSE_SILENT {
            self.action_state.stdout.push_str(&format!(
                "Competed: Min fitness = {}, Avg fitness = {} Max fitness = {}\n\
                 Copied  : Min fitness = {}, Avg fitness = {}, Max fitness = {}\n\
                 Copied  : Different organisms = {different_orgs_copied}\n",
                self.cout_double(lowest_fitness),
                self.cout_double(average_fitness),
                self.cout_double(highest_fitness),
                self.cout_double(lowest_fitness_copied),
                self.cout_double(average_fitness_copied),
                self.cout_double(highest_fitness_copied),
            ));
        }

        // copy stats to cStats, so that these can be remembered and printed
        let st = &mut self.stats;
        st.avg_trial_fitnesses = avg_trial;
        st.avg_competition_fitness = average_fitness;
        st.min_competition_fitness = lowest_fitness;
        st.max_competition_fitness = highest_fitness;
        st.avg_competition_copied_fitness = average_fitness_copied;
        st.min_competition_copied_fitness = lowest_fitness_copied;
        st.max_competition_copied_fitness = highest_fitness_copied;
        st.num_orgs_replicated = different_orgs_copied;

        self.new_trial()
    }

    /// `organism->OffspringGenome() = organism->GetGenome();` followed by
    /// `cPopulation::CompeteOrganisms_ConstructOffspring(cell_id, *organism)`
    /// for the organism in `parent_cell`.
    fn compete_organisms_construct_offspring(
        &mut self,
        cell_id: i32,
        parent_cell: i32,
    ) -> Result<(), String> {
        let pc = parent_cell as usize;
        let mut parent = self.pop.orgs[pc]
            .take()
            .ok_or("CompeteOrganisms: copy of an empty cell (C++: NULL dereference)")?;
        parent.state.offspring_genome = parent.state.genome().clone();

        // Do mutations on the child genome, but restore it to its current
        // state afterward.
        let save_child = parent.state.offspring_genome.clone();
        let result = {
            let mut iface = PopulationInterface {
                w: self,
                cell_id: parent_cell,
            };
            let Organism { state, hardware } = &mut *parent;
            let is = hardware.inst_set().clone();
            divide_do_mutations(&is, state, &mut iface, hardware.memory_mut(), 1.0, i32::MAX).map(
                |_| {
                    let child = state.offspring_genome.clone();
                    divide_test_fitness_measures(state, &mut iface, false);
                    child
                },
            )
        };
        parent.state.offspring_genome = save_child;
        let child_genome = match result {
            Ok(g) => g,
            Err(e) => {
                self.pop.orgs[pc] = Some(parent);
                return Err(e);
            }
        };
        let parent_generation = parent.state.phenotype.generation;
        let parent_phenotype = parent.state.phenotype.clone();
        let parent_mut_rates = parent.state.mut_rates;
        self.pop.orgs[pc] = Some(parent);

        let mut org = Organism::new(self, &child_genome, parent_generation, duplication_source())?;

        // Classify the offspring
        let mut membership = GroupMembership::new();
        {
            let source = systematics_source(&org.state.src);
            let unit = UnitInfo {
                source: &source,
                genome: org.state.genome(),
                generation: org.state.phenotype.generation,
            };
            let parents = [&self.pop.membership[pc]];
            self.systematics
                .self_classify(&unit, &parents, &mut membership);
        }

        // Setup the phenotype...
        let size = child_genome.representation().size();
        org.state
            .phenotype
            .setup_offspring(&self.cfg, &parent_phenotype, size, &mut self.rng);

        // Prep the cell..
        if self.cfg.birth_method == po::FULL_SOUP_ELDEST && self.is_occupied(cell_id) {
            // Have to manually take this cell out of the reaper Queue.
            if let Some(pos) = self.pop.reaper_queue.iter().position(|&x| x == cell_id) {
                self.pop.reaper_queue.remove(pos);
            }
        }

        // Setup the mutation rate based on the population cell...
        if self.cfg.mut_rate_source == 1 {
            org.state
                .mut_rates
                .copy_from(&self.pop.cells[cell_id as usize].mut_rates);
        } else {
            org.state.mut_rates.copy_from(&parent_mut_rates);
        }

        // Activate the organism in the population...
        self.activate_organism(org, membership, cell_id, true, true, None);
        if let Some(f) = self.fatal.borrow_mut().take() {
            return Err(f);
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Data distribution
    // ------------------------------------------------------------------

    /// `cActionDistributeData::Process` /
    /// `cActionDistributeDataEfficiently` / `cActionDistributeDataCheaply`:
    /// compete the demes by how many of their organisms hold the deme's
    /// (first) cell data as their opinion, then reset the message counter.
    fn a3_distribute_data(&mut self, id: usize, mode: DistributeMode) -> Result<(), String> {
        let pred = *self
            .actions2
            .a3
            .distribute_counters
            .get(&id)
            .ok_or("DistributeData: message counter not registered")?;
        let mut fitness = Vec::with_capacity(self.pop.demes.len());
        for d in 0..self.pop.demes.len() {
            let received = self.a3_received_data(d)?;
            let size = self.pop.demes[d].size();
            let f = if mode == DistributeMode::Plain || received < size as u32 {
                (f64::from(received) + 1.0).powf(2.0)
            } else if mode == DistributeMode::Cheaply {
                // sum the euclidean lengths of all links in the network:
                // `deme.GetNetwork().Fitness(false)`
                self.deme_network_abort();
                return Ok(());
            } else {
                // Now, reward for reducing the number of messages that were
                // used (`m_msg_counts[id]` inserts a 0 count).
                let size = f64::from(size * 1000);
                let deme_id = self.pop.demes[d].id;
                let msg_count = match &mut self.actions2.a3.msg_predicates[pred] {
                    MsgPredicate::CountDemeMessages(counts) => *counts.entry(deme_id).or_insert(0),
                    MsgPredicate::AllData(_) => 0,
                };
                (f64::from(received) + 1.0 + size / f64::from(msg_count)).powf(2.0)
            };
            fitness.push(f);
        }
        self.compete_demes(&fitness)?;
        self.actions2.a3.msg_predicates[pred].reset();
        Ok(())
    }

    /// `cActionDistributeData::received_data(deme)`: how many organisms of
    /// deme `d` hold the deme's first cell data as their opinion.
    fn a3_received_data(&mut self, d: usize) -> Result<u32, String> {
        let deme_id = self.pop.demes[d].id;
        // `*cAssignRandomCellData::GetDataInDeme(deme).begin()`
        let data = *self
            .deme_compete
            .deme_to_id
            .entry(deme_id)
            .or_default()
            .iter()
            .next()
            .ok_or(
                "unsupported: DistributeData in a deme without assigned cell data (C++: \
                 dereferences the end of an empty set)",
            )?;
        let mut count = 0;
        for i in 0..self.pop.demes[d].size() {
            let c = self.pop.demes[d].cell_id(i) as usize;
            if let Some(o) = self.pop.orgs[c].as_mut() {
                if o.state.has_opinion() && o.state.get_opinion().0 == data {
                    count += 1;
                }
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTX: ParseContext<'static> = ParseContext {
        default_inst_set: "heads_default",
    };

    fn p(name: &str, args: &str) -> Action3 {
        parse(name, args, &CTX, &mut Feedback::new())
            .unwrap()
            .unwrap()
    }

    #[test]
    fn every_listed_name_parses_and_is_registered() {
        for n in ACTIONS3 {
            assert!(parse(n, "", &CTX, &mut Feedback::new()).is_some(), "{n}");
            assert!(crate::action_library::is_registered(n), "{n}");
        }
        assert!(parse("Exit", "", &CTX, &mut Feedback::new()).is_none());
    }

    #[test]
    fn defaults_and_arguments() {
        assert_eq!(
            p("SetNumInstBefore0Energy", ""),
            Action3::SetNumInstBefore0Energy { new_value: 0 }
        );
        assert_eq!(
            p("CompeteOrganisms", "3 1"),
            Action3::CompeteOrganisms {
                comp_type: 3,
                parents_survive: 1
            }
        );
        assert_eq!(
            p("DumpEnergyGrid", "e.dat"),
            Action3::DumpEnergyGrid {
                filename: "e.dat".into()
            }
        );
    }

    #[test]
    fn argument_quirks() {
        // One argument: `_replace` reads an empty word.
        match p("IteratedConsensus", "40") {
            Action3::IteratedConsensus {
                compete_period,
                replace,
                kill,
                restrict_range,
                dont_replace,
                ..
            } => assert_eq!(
                (compete_period, replace, kill, restrict_range, dont_replace),
                (40, 0, 1, 1, 0)
            ),
            a => panic!("{a:?}"),
        }
        match p("IteratedConsensus", "") {
            Action3::IteratedConsensus {
                compete_period,
                replace,
                ..
            } => assert_eq!((compete_period, replace), (100, 1)),
            a => panic!("{a:?}"),
        }
        // Both schema entries share string slot 0.
        assert_eq!(
            p("SaveStructuredSystematicsGroup", "role=genotype"),
            Action3::SaveStructuredSystematicsGroup {
                filename: "genotype".into(),
                role: "genotype".into()
            }
        );
        assert_eq!(
            p(
                "LoadStructuredSystematicsGroup",
                "filename=a.ssg:role=genotype"
            ),
            Action3::LoadStructuredSystematicsGroup {
                filename: "a.ssg".into(),
                role: "genotype".into()
            }
        );
        assert_eq!(
            p("RaidDen", "0.5"),
            Action3::RaidDen {
                loss: 0.5,
                res_id: 0,
                units_per: 1.0
            }
        );
        assert_eq!(
            p("AttackDen", ""),
            Action3::AttackDen {
                killprob: 0.0,
                juvs_per: 1
            }
        );
    }

    #[test]
    fn predicates_print_like_the_cpp() {
        let msg = OrgMessage {
            data: 7,
            label: 9,
            sender_cell: 3,
            receiver_cell: 4,
            sender_org_id: 1,
            message_type: 0,
            trans_cell: -1,
        };
        let mut count = MsgPredicate::CountDemeMessages(BTreeMap::new());
        count.sent(&msg, 2);
        count.sent(&msg, 0);
        count.sent(&msg, 2);
        let mut out = String::new();
        count.print(5, &mut out);
        assert_eq!(out, "5 COUNT 3  1 2\n");
        count.reset();
        let mut out = String::new();
        count.print(6, &mut out);
        assert_eq!(out, "6 COUNT 0 \n");
        let mut all = MsgPredicate::AllData(Vec::new());
        all.sent(&msg, 0);
        let mut out = String::new();
        all.print(8, &mut out);
        assert_eq!(out, "8 ALL 7 9 3 4\n");
    }
}
