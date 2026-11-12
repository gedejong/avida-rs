//! PopulationActions.cc actions on single organisms and cells: injections,
//! kills, mutation-rate changes, grid connectivity and cell swaps
//! (avida-core/source/actions/PopulationActions.cc at 47f13dadb).
//!
//! Not ported here (each stops the run as unsupported, or is handled by
//! another module):
//! - `CompeteOrganisms`: the trial machinery (`cPopulation::NewTrial`,
//!   `cPhenotype::NewTrial` and the trial fitness arrays,
//!   `CompeteOrganisms_ConstructOffspring`).
//! - `AttackDen`, `RaidDen`: avatar dens (`cPopulationCell::GetCellAVs` in
//!   randomized order, `cOrganism::IsGuard`).
//! - `AvidianConjugation` donations (`cPopulationInterface::DoHGTDonation`)
//!   are in hgt.rs.
//! - `PrintReproData`, `PrintTopNavTrace`, `PrintNavTrace` and
//!   `FlushTopNavTrace` are in mini_trace.rs.

use std::cell::Cell;
use std::sync::Arc;

use avida_apto::fmt::fmt_double;
use avida_config::feedback::Feedback;
use avida_config::string_util::{as_double, as_int, pop_word};
use avida_genome::genome::{setup_property_map, PropertyMap};
use avida_genome::{Genome, InstSet, InstructionSequence};
use avida_hardware::organism::{transmission, OrgSource};
use avida_phenotype::MutationRates;

use crate::actions::ParseContext;
use crate::run::injected_source;
use crate::stat_tools::DoubleSum;
use crate::topology::{geometry, grid_neighbor};
use crate::world::World;

/// The registered action names this module parses.
pub const POP2_ACTIONS: &[&str] = &[
    "InjectRandom",
    "InjectWellMixed",
    "InjectSequenceWDivMutRate",
    "KillRate",
    "KillInstLimit",
    "KillInstPair",
    "ApplyBottleneck",
    "KillDominantGenotype",
    "KillProbSequence",
    "KillFractionInSequence",
    "KillFractionInSequence_PopLimit",
    "KillRectangle",
    "SerialTransfer",
    "SetMutProb",
    "ModMutProb",
    "SeverGridCol",
    "SeverGridRow",
    "JoinGridCol",
    "JoinGridRow",
    "ConnectCells",
    "DisconnectCells",
    "SwapCells",
    "SwapRandomCells",
    "KillNBelowResourceThreshold",
    "KillWithinRadiusBelowResourceThreshold",
    "KillWithinRadiusMeanBelowResourceThreshold",
    "KillWithinRadiusBelowResourceThresholdTestAll",
    "KillMeanBelowThresholdPaintable",
    "DiffuseHGTGenomeFragments",
    "AvidianConjugation",
    "RemovePredators",
    "SetRedundancy",
    "SetProbFail",
];

/// The mutation types of `cActionSetMutProb` / `cActionModMutProb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutType {
    Point,
    CMut,
    CIns,
    CDel,
    CUniform,
    CSlip,
    DsMut,
    DsIns,
    DsDel,
    DsUniform,
    DsSlip,
    D1Mut,
    D1Ins,
    D1Del,
    D1Uniform,
    D1Slip,
    PMut,
    PIns,
    PDel,
    Death,
    PntMut,
    PntIns,
    PntDel,
    IMut,
    IIns,
    IDel,
}

impl MutType {
    /// The constructor's `if (mutstr == ...)` chain (on the upper-cased
    /// word). `None` for any other word: the C++ leaves `m_mut_type`
    /// uninitialised, which the port takes as matching no case of the
    /// `Process` switches (a no-op).
    fn parse(mutstr: &str) -> Option<MutType> {
        use MutType::*;
        Some(match mutstr {
            "POINT" => Point,
            "COPY_MUT" => CMut,
            "COPY_INS" => CIns,
            "COPY_DEL" => CDel,
            "COPY_UNIFORM" => CUniform,
            "COPY_SLIP" => CSlip,
            "DIV_MUT" => DsMut,
            "DIV_INS" => DsIns,
            "DIV_DEL" => DsDel,
            "DIV_UNIFORM" => DsUniform,
            "DIV_SLIP" => DsSlip,
            "DIVIDE_MUT" => D1Mut,
            "DIVIDE_INS" => D1Ins,
            "DIVIDE_DEL" => D1Del,
            "DIVIDE_UNIFORM" => D1Uniform,
            "DIVIDE_SLIP" => D1Slip,
            "PARENT_MUT" => PMut,
            "PARENT_INS" => PIns,
            "PARENT_DEL" => PDel,
            "DEATH" => Death,
            "POINT_MUT" => PntMut,
            "POINT_INS" => PntIns,
            "POINT_DEL" => PntDel,
            "INJECT_MUT" => IMut,
            "INJECT_INS" => IIns,
            "INJECT_DEL" => IDel,
            _ => return None,
        })
    }

    /// The cell rate `cMutationRates::Set<...>Prob` writes (`None` for
    /// `POINT`, which has no cell case).
    fn cell_rate(self, r: &mut MutationRates) -> Option<&mut f64> {
        use MutType::*;
        Some(match self {
            Point => return None,
            CMut => &mut r.copy.mut_prob,
            CIns => &mut r.copy.ins_prob,
            CDel => &mut r.copy.del_prob,
            CUniform => &mut r.copy.uniform_prob,
            CSlip => &mut r.copy.slip_prob,
            DsMut => &mut r.divide.mut_prob,
            DsIns => &mut r.divide.ins_prob,
            DsDel => &mut r.divide.del_prob,
            DsUniform => &mut r.divide.uniform_prob,
            DsSlip => &mut r.divide.slip_prob,
            D1Mut => &mut r.divide.divide_mut_prob,
            D1Ins => &mut r.divide.divide_ins_prob,
            D1Del => &mut r.divide.divide_del_prob,
            D1Uniform => &mut r.divide.divide_uniform_prob,
            D1Slip => &mut r.divide.divide_slip_prob,
            PMut => &mut r.divide.parent_mut_prob,
            PIns => &mut r.divide.parent_ins_prob,
            PDel => &mut r.divide.parent_del_prob,
            Death => &mut r.update.death_prob,
            PntMut => &mut r.point.mut_prob,
            PntIns => &mut r.point.ins_prob,
            PntDel => &mut r.point.del_prob,
            IMut => &mut r.inject.mut_prob,
            IIns => &mut r.inject.ins_prob,
            IDel => &mut r.inject.del_prob,
        })
    }

    /// The configuration setting the first `Process` switch writes.
    fn config_setting(self, cfg: &mut avida_config::config::Config) -> &mut f64 {
        use MutType::*;
        match self {
            Point | PntMut => &mut cfg.point_mut_prob,
            CMut => &mut cfg.copy_mut_prob,
            CIns => &mut cfg.copy_ins_prob,
            CDel => &mut cfg.copy_del_prob,
            CUniform => &mut cfg.copy_uniform_prob,
            CSlip => &mut cfg.copy_slip_prob,
            DsMut => &mut cfg.div_mut_prob,
            DsIns => &mut cfg.div_ins_prob,
            DsDel => &mut cfg.div_del_prob,
            DsUniform => &mut cfg.div_uniform_prob,
            DsSlip => &mut cfg.div_slip_prob,
            D1Mut => &mut cfg.divide_mut_prob,
            D1Ins => &mut cfg.divide_ins_prob,
            D1Del => &mut cfg.divide_del_prob,
            D1Uniform => &mut cfg.divide_uniform_prob,
            D1Slip => &mut cfg.divide_slip_prob,
            PMut => &mut cfg.parent_mut_prob,
            PIns => &mut cfg.parent_ins_prob,
            PDel => &mut cfg.parent_del_prob,
            Death => &mut cfg.death_prob,
            PntIns => &mut cfg.point_ins_prob,
            PntDel => &mut cfg.point_del_prob,
            IMut => &mut cfg.inject_mut_prob,
            IIns => &mut cfg.inject_ins_prob,
            IDel => &mut cfg.inject_del_prob,
        }
    }

    /// The setting `cActionModMutProb::Process` adds to the change.
    /// QUIRK: the uniform and slip types read the `*_MUT_PROB` setting of
    /// their family.
    fn mod_base(self, cfg: &avida_config::config::Config) -> f64 {
        use MutType::*;
        match self {
            Point | PntMut => cfg.point_mut_prob,
            CMut | CUniform | CSlip => cfg.copy_mut_prob,
            CIns => cfg.copy_ins_prob,
            CDel => cfg.copy_del_prob,
            DsMut | DsUniform | DsSlip => cfg.div_mut_prob,
            DsIns => cfg.div_ins_prob,
            DsDel => cfg.div_del_prob,
            D1Mut | D1Uniform | D1Slip => cfg.divide_mut_prob,
            D1Ins => cfg.divide_ins_prob,
            D1Del => cfg.divide_del_prob,
            PMut => cfg.parent_mut_prob,
            PIns => cfg.parent_ins_prob,
            PDel => cfg.parent_del_prob,
            Death => cfg.death_prob,
            PntIns => cfg.point_ins_prob,
            PntDel => cfg.point_del_prob,
            IMut => cfg.inject_mut_prob,
            IIns => cfg.inject_ins_prob,
            IDel => cfg.inject_del_prob,
        }
    }
}

/// Which `KillWithinRadius*` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadiusKill {
    /// `cActionKillWithinRadiusBelowResourceThreshold`: the focal cell's
    /// level decides.
    Focal,
    /// `cActionKillWithinRadiusMeanBelowResourceThreshold`: the mean over
    /// the neighbourhood decides.
    Mean,
    /// `cActionKillWithinRadiusBelowResourceThresholdTestAll`: each cell's
    /// own level decides.
    TestAll,
}

/// A parsed action of this module. Members the C++ action changes between
/// calls are `Cell`s.
#[derive(Debug, Clone, PartialEq)]
pub enum Pop2Action {
    /// `cActionInjectRandom`
    InjectRandom {
        length: i32,
        cell_id: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionInjectWellMixed`
    InjectWellMixed {
        filename: String,
        cell_num: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionInjectSequenceWithDivMutRate`
    InjectSequenceWDivMutRate {
        sequence: String,
        cell_start: i32,
        cell_end: i32,
        div_mut_rate: f64,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionKillProb` (registered as `KillRate`; `cActionKillRate` itself
    /// is never registered).
    KillProb { killprob: f64 },
    /// `cActionKillInstLimit`
    KillInstLimit {
        killprob: f64,
        inst: String,
        limit: f64,
    },
    /// `cActionKillInstPair`
    KillInstPair {
        killprob: f64,
        inst1: String,
        inst2: String,
        limit: f64,
    },
    /// `cActionApplyBottleneck`
    ApplyBottleneck { bottleneck_size: i32 },
    /// `cActionKillDominantGenotype`
    KillDominantGenotype,
    /// `cActionProbKillSequence`
    KillProbSequence { sequence: String, killprob: f64 },
    /// `cActionKillFractionInSequence`
    KillFractionInSequence {
        kill_fraction: f64,
        window_length: i32,
        kill_index: Cell<i32>,
        apply_action: Cell<bool>,
        update_since_switch: Cell<i32>,
    },
    /// `cActionKillFractionInSequence_PopLimit`
    KillFractionInSequencePopLimit {
        kill_fraction: f64,
        pop_size: i32,
        kill_index: Cell<i32>,
    },
    /// `cActionKillRectangle` (the constructor's clamping to the world
    /// happens at `Process`; the world size does not change in between).
    KillRectangle { x1: i32, y1: i32, x2: i32, y2: i32 },
    /// `cActionSerialTransfer`
    SerialTransfer { size: i32, ignore_deads: bool },
    /// `cActionSetMutProb` (`modify == false`) and `cActionModMutProb`;
    /// `start` / `end` as given (normalised against the population size at
    /// `Process`, as the constructor does).
    SetMutProb {
        mut_type: Option<MutType>,
        prob: f64,
        start: i32,
        end: i32,
        modify: bool,
    },
    /// `cActionSeverGridCol` / `cActionSeverGridRow` (`row`)
    SeverGrid {
        row: bool,
        id: i32,
        min: i32,
        max: i32,
    },
    /// `cActionJoinGridCol` / `cActionJoinGridRow` (`row`)
    JoinGrid {
        row: bool,
        id: i32,
        min: i32,
        max: i32,
    },
    /// `cActionConnectCells` (`connect`) / `cActionDisconnectCells`
    ConnectCells {
        connect: bool,
        a_x: i32,
        a_y: i32,
        b_x: i32,
        b_y: i32,
    },
    /// `cActionSwapCells`
    SwapCells { id1: i32, id2: i32 },
    /// `cActionSwapRandomCells`
    SwapRandomCells { num_swaps: i32 },
    /// `cActionKillNBelowResourceThreshold`
    KillNBelowResourceThreshold {
        numkills: i32,
        resname: String,
        threshold: f64,
    },
    /// The three `KillWithinRadius*` actions.
    KillWithinRadius {
        kind: RadiusKill,
        numradii: i32,
        radius: i32,
        resname: String,
        threshold: f64,
        kill_density: f64,
    },
    /// `cActionKillMeanBelowThresholdPaintable`
    KillMeanBelowThresholdPaintable {
        numradii: i32,
        radius: i32,
        resname: String,
        threshold: f64,
        adv_resname: String,
    },
    /// `cActionDiffuseHGTGenomeFragments`
    DiffuseHGTGenomeFragments,
    /// `cActionAvidianConjugation`
    AvidianConjugation { donation_p: f64 },
    /// `cActionRemovePredators`
    RemovePredators,
    /// `cActionSetRedundancy` (`prob_fail == false`) and
    /// `cActionSetProbFail`. QUIRK: both read their value with `AsInt()`.
    SetInstProperty {
        prob_fail: bool,
        inst_name: String,
        value: i32,
    },
}

/// State the actions of this module keep between calls.
#[derive(Debug, Default)]
pub struct Pop2State {}

/// `if (largs.GetSize()) x = largs.PopWord()...`
struct Words(String);

impl Words {
    fn word(&mut self) -> Option<String> {
        if self.0.is_empty() {
            None
        } else {
            Some(pop_word(&mut self.0))
        }
    }
    fn int(&mut self, def: i32) -> i32 {
        self.word().map_or(def, |w| as_int(&w))
    }
    fn double(&mut self, def: f64) -> f64 {
        self.word().map_or(def, |w| as_double(&w))
    }
    fn string(&mut self, def: &str) -> String {
        self.word().unwrap_or_else(|| def.to_string())
    }
    /// `largs.PopWord()` without the size check.
    fn pop(&mut self) -> String {
        pop_word(&mut self.0)
    }
}

/// The constructor of the action `name` (a [`POP2_ACTIONS`] spelling)
/// applied to `args`; `None` for any other name.
pub fn parse(
    name: &str,
    args: &str,
    _ctx: &ParseContext<'_>,
    feedback: &mut Feedback,
) -> Option<Result<Pop2Action, String>> {
    let mut a = Words(args.to_string());
    let action = match name {
        "InjectRandom" => {
            let length = as_int(&a.pop());
            Pop2Action::InjectRandom {
                length,
                cell_id: a.int(0),
                merit: a.double(-1.0),
                lineage_label: a.int(0),
                neutral_metric: a.double(0.0),
            }
        }
        "InjectWellMixed" => Pop2Action::InjectWellMixed {
            filename: a.string(""),
            cell_num: a.int(0),
            merit: a.double(-1.0),
            lineage_label: a.int(0),
            neutral_metric: a.double(0.0),
        },
        "InjectSequenceWDivMutRate" => {
            let sequence = a.pop();
            let cell_start = a.int(0);
            let mut cell_end = a.int(-1);
            let div_mut_rate = a.double(0.0);
            let merit = a.double(-1.0);
            let lineage_label = a.int(0);
            let neutral_metric = a.double(0.0);
            // `avd_popaction_normalize_cell_end`
            if cell_end == -1 {
                cell_end = cell_start.wrapping_add(1);
            }
            Pop2Action::InjectSequenceWDivMutRate {
                sequence,
                cell_start,
                cell_end,
                div_mut_rate,
                merit,
                lineage_label,
                neutral_metric,
            }
        }
        "KillRate" => Pop2Action::KillProb {
            killprob: a.double(0.9),
        },
        "KillInstLimit" => Pop2Action::KillInstLimit {
            killprob: a.double(0.9),
            inst: a.string("nand"),
            limit: f64::from(a.int(5)),
        },
        "KillInstPair" => Pop2Action::KillInstPair {
            killprob: a.double(0.9),
            inst1: a.string("nand"),
            inst2: a.string("nor"),
            limit: f64::from(a.int(1)),
        },
        "ApplyBottleneck" => Pop2Action::ApplyBottleneck {
            bottleneck_size: a.int(100),
        },
        "KillDominantGenotype" => Pop2Action::KillDominantGenotype,
        "KillProbSequence" => Pop2Action::KillProbSequence {
            sequence: a.string(""),
            killprob: a.double(0.9),
        },
        "KillFractionInSequence" => Pop2Action::KillFractionInSequence {
            kill_fraction: a.double(0.01),
            window_length: a.int(1000),
            kill_index: Cell::new(0),
            apply_action: Cell::new(true),
            update_since_switch: Cell::new(0),
        },
        "KillFractionInSequence_PopLimit" => Pop2Action::KillFractionInSequencePopLimit {
            kill_fraction: a.double(0.01),
            pop_size: a.int(1000),
            kill_index: Cell::new(0),
        },
        "KillRectangle" => Pop2Action::KillRectangle {
            x1: a.int(0),
            y1: a.int(0),
            x2: a.int(0),
            y2: a.int(0),
        },
        "SerialTransfer" => {
            let mut size = a.int(1);
            let ignore_deads = a.int(1) != 0;
            if size < 0 {
                size = 1;
            }
            Pop2Action::SerialTransfer { size, ignore_deads }
        }
        "SetMutProb" | "ModMutProb" => {
            let mutstr = a.string("COPY_MUT").to_ascii_uppercase();
            Pop2Action::SetMutProb {
                mut_type: MutType::parse(&mutstr),
                prob: a.double(0.0),
                start: a.int(-1),
                end: a.int(-1),
                modify: name == "ModMutProb",
            }
        }
        "SeverGridCol" | "SeverGridRow" | "JoinGridCol" | "JoinGridRow" => {
            let row = name.ends_with("Row");
            let (id, min, max) = (a.int(-1), a.int(0), a.int(-1));
            if name.starts_with("Sever") {
                Pop2Action::SeverGrid { row, id, min, max }
            } else {
                Pop2Action::JoinGrid { row, id, min, max }
            }
        }
        "ConnectCells" | "DisconnectCells" => Pop2Action::ConnectCells {
            connect: name == "ConnectCells",
            a_x: a.int(-1),
            a_y: a.int(-1),
            b_x: a.int(-1),
            b_y: a.int(-1),
        },
        "SwapCells" => Pop2Action::SwapCells {
            id1: a.int(-1),
            id2: a.int(-1),
        },
        "SwapRandomCells" => Pop2Action::SwapRandomCells {
            num_swaps: a.int(-1),
        },
        "KillNBelowResourceThreshold" => Pop2Action::KillNBelowResourceThreshold {
            numkills: a.int(0),
            resname: a.string(""),
            threshold: a.double(0.0),
        },
        "KillWithinRadiusBelowResourceThreshold"
        | "KillWithinRadiusMeanBelowResourceThreshold"
        | "KillWithinRadiusBelowResourceThresholdTestAll" => Pop2Action::KillWithinRadius {
            kind: match name {
                "KillWithinRadiusBelowResourceThreshold" => RadiusKill::Focal,
                "KillWithinRadiusMeanBelowResourceThreshold" => RadiusKill::Mean,
                _ => RadiusKill::TestAll,
            },
            numradii: a.int(0),
            radius: a.int(0),
            resname: a.string(""),
            threshold: a.double(0.0),
            kill_density: a.double(1.0),
        },
        "KillMeanBelowThresholdPaintable" => Pop2Action::KillMeanBelowThresholdPaintable {
            numradii: a.int(0),
            radius: a.int(0),
            resname: a.string(""),
            threshold: a.double(0.0),
            adv_resname: a.string(""),
        },
        "DiffuseHGTGenomeFragments" => Pop2Action::DiffuseHGTGenomeFragments,
        "AvidianConjugation" => {
            let donation_p = a.double(-1.0);
            if !(0.0..=1.0).contains(&donation_p) {
                feedback.error("Conjugate event must include probability of donation [0..1].");
            }
            Pop2Action::AvidianConjugation { donation_p }
        }
        "RemovePredators" => Pop2Action::RemovePredators,
        "SetRedundancy" | "SetProbFail" => {
            // The constructor's `assert(InstInSet(...))` is compiled out.
            let inst_name = a.pop();
            Pop2Action::SetInstProperty {
                prob_fail: name == "SetProbFail",
                inst_name,
                value: as_int(&a.pop()),
            }
        }
        _ => return None,
    };
    Some(Ok(action))
}

/// `Systematics::Source(Systematics::DIVISION, args, true)`
fn division_source(args: &str) -> OrgSource {
    OrgSource::new(transmission::DIVISION, args, true)
}

/// A cell of a `KillWithinRadius*` square, or `None` where a bounded grid
/// skips it (`row` / `col` may lie outside the world).
fn radius_cell(geometry: i32, world_x: i32, world_y: i32, row: i32, col: i32) -> i32 {
    let (row_adj, col_adj) = if geometry == geometry::TORUS {
        ((row + world_y) % world_y, (col + world_x) % world_x)
    } else if geometry == geometry::GRID {
        (row, col)
    } else {
        (0, 0)
    };
    world_x * row_adj + col_adj
}

impl World {
    /// `Process(ctx)` of an action of this module.
    pub(crate) fn run_pop2_action(&mut self, action: &Pop2Action) -> Result<(), String> {
        match action {
            Pop2Action::InjectRandom {
                length,
                cell_id,
                merit,
                lineage_label,
                neutral_metric,
            } => {
                self.pop2_inject_random(*length, *cell_id, *merit, *lineage_label, *neutral_metric)
            }
            Pop2Action::InjectWellMixed {
                filename,
                cell_num,
                merit,
                lineage_label,
                neutral_metric,
            } => self.pop2_inject_well_mixed(
                filename,
                *cell_num,
                *merit,
                *lineage_label,
                *neutral_metric,
            ),
            Pop2Action::InjectSequenceWDivMutRate {
                sequence,
                cell_start,
                cell_end,
                div_mut_rate,
                merit,
                lineage_label,
                neutral_metric,
            } => {
                let size = self.pop.cells.len() as i32;
                if *cell_start < 0 || *cell_end > size || cell_start >= cell_end {
                    self.pop2_warning("InjectSequenceWithDivMutRate has invalid range!");
                    return Ok(());
                }
                let genome =
                    self.pop2_default_genome(InstructionSequence::from_string(sequence))?;
                for i in *cell_start..*cell_end {
                    self.inject(
                        &genome,
                        injected_source(),
                        i,
                        *merit,
                        *lineage_label,
                        *neutral_metric,
                    )?;
                    let org = self.pop.orgs[i as usize]
                        .as_mut()
                        .ok_or("InjectSequenceWDivMutRate: injected cell is empty")?;
                    org.state.mut_rates.divide.mut_prob = *div_mut_rate;
                }
                self.pop.sync_events = true;
                Ok(())
            }
            Pop2Action::KillProb { killprob } => {
                for i in 0..self.pop.cells.len() as i32 {
                    if !self.is_occupied(i) {
                        continue;
                    }
                    if self.rng.p(*killprob) {
                        self.kill_organism(i, None);
                    }
                }
                Ok(())
            }
            Pop2Action::KillInstLimit {
                killprob,
                inst,
                limit,
            } => self.pop2_kill_inst(*killprob, inst, None, *limit),
            Pop2Action::KillInstPair {
                killprob,
                inst1,
                inst2,
                limit,
            } => self.pop2_kill_inst(*killprob, inst1, Some(inst2), *limit),
            Pop2Action::ApplyBottleneck { bottleneck_size } => {
                self.pop2_apply_bottleneck(*bottleneck_size)
            }
            Pop2Action::KillDominantGenotype => self.pop2_kill_dominant_genotype(),
            Pop2Action::KillProbSequence { sequence, killprob } => {
                self.pop2_kill_prob_sequence(sequence, *killprob)
            }
            Pop2Action::KillFractionInSequence {
                kill_fraction,
                window_length,
                kill_index,
                apply_action,
                update_since_switch,
            } => {
                update_since_switch.set(update_since_switch.get() + 1);
                if update_since_switch.get() >= (f64::from(*window_length) / 2.0) as i32 {
                    // switch context
                    apply_action.set(!apply_action.get());
                    update_since_switch.set(0);
                    eprintln!("applied? {}", i32::from(apply_action.get()));
                }
                if !apply_action.get() {
                    return Ok(());
                }
                let to_kill = (f64::from(self.pop.num_organisms) * kill_fraction) as i32;
                self.pop2_kill_in_sequence(to_kill, kill_index)
            }
            Pop2Action::KillFractionInSequencePopLimit {
                kill_fraction: _,
                pop_size,
                kill_index,
            } => {
                let avg_gestation = self.stats.sum_gestation.average();
                let inst_per_update = f64::from(self.cfg.ave_time_slice);
                let growth_rate = (inst_per_update / avg_gestation * 100.0 + 0.5).floor() / 100.0;
                let num_orgs = self.pop.num_organisms;
                if num_orgs < *pop_size {
                    return Ok(());
                }
                let to_kill = (f64::from(num_orgs) * growth_rate) as i32;
                eprintln!("growth rate: {}  kill: {to_kill}", fmt_double(growth_rate));
                self.pop2_kill_in_sequence(to_kill, kill_index)
            }
            Pop2Action::KillRectangle { x1, y1, x2, y2 } => {
                self.pop2_kill_rectangle(*x1, *y1, *x2, *y2);
                Ok(())
            }
            Pop2Action::SerialTransfer { size, ignore_deads } => {
                self.pop2_serial_transfer(*size, *ignore_deads)
            }
            Pop2Action::SetMutProb {
                mut_type,
                prob,
                start,
                end,
                modify,
            } => {
                self.pop2_set_mut_prob(*mut_type, *prob, *start, *end, *modify);
                Ok(())
            }
            Pop2Action::SeverGrid { row, id, min, max } => {
                self.pop2_sever_grid(*row, *id, *min, *max);
                Ok(())
            }
            Pop2Action::JoinGrid { row, id, min, max } => {
                self.pop2_join_grid(*row, *id, *min, *max);
                Ok(())
            }
            Pop2Action::ConnectCells {
                connect,
                a_x,
                a_y,
                b_x,
                b_y,
            } => {
                let (wx, wy) = (self.pop.world_x, self.pop.world_y);
                let out = |x: i32, y: i32| x < 0 || x >= wx || y < 0 || y >= wy;
                if out(*a_x, *a_y) || out(*b_x, *b_y) {
                    self.pop2_warning(if *connect {
                        "ConnectCells cell out of range"
                    } else {
                        "DisconnectCells cell out of range"
                    });
                    return Ok(());
                }
                let id_a = a_y * wx + a_x;
                let id_b = b_y * wx + b_x;
                if *connect {
                    self.pop.cells[id_a as usize].connections.push_rear(id_b);
                    self.pop.cells[id_b as usize].connections.push_rear(id_a);
                } else {
                    self.pop.cells[id_a as usize].connections.remove(id_b);
                    self.pop.cells[id_b as usize].connections.remove(id_a);
                }
                Ok(())
            }
            Pop2Action::SwapCells { id1, id2 } => {
                let num_cells = self.pop.cells.len() as i32;
                if *id1 < 0 || *id1 >= num_cells || *id2 < 0 || *id2 >= num_cells {
                    self.pop2_warning("SwapCells cell ID out of range");
                    return Ok(());
                }
                if id1 == id2 {
                    self.pop2_warning("SwapCells cell IDs identical");
                }
                self.swap_cells(*id1, *id2, None);
                Ok(())
            }
            Pop2Action::SwapRandomCells { num_swaps } => {
                let num_cells = self.pop.cells.len() as i32;
                for _ in 0..*num_swaps {
                    let id1 = self.rng.get_int_range(0, num_cells);
                    let id2 = self.rng.get_int_range(0, num_cells);
                    // if the id's happen to be the same, it just won't swap
                    if id1 != id2 {
                        self.swap_cells(id1, id2, None);
                    }
                }
                Ok(())
            }
            Pop2Action::KillNBelowResourceThreshold {
                numkills,
                resname,
                threshold,
            } => self.pop2_kill_n_below(*numkills, resname, *threshold),
            Pop2Action::KillWithinRadius {
                kind,
                numradii,
                radius,
                resname,
                threshold,
                kill_density,
            } => self.pop2_kill_within_radius(
                *kind,
                *numradii,
                *radius,
                resname,
                *threshold,
                KillProb::Fixed(*kill_density),
            ),
            Pop2Action::KillMeanBelowThresholdPaintable {
                numradii,
                radius,
                resname,
                threshold,
                adv_resname,
            } => self.pop2_kill_within_radius(
                RadiusKill::Mean,
                *numradii,
                *radius,
                resname,
                *threshold,
                KillProb::Painted(adv_resname),
            ),
            Pop2Action::DiffuseHGTGenomeFragments => {
                // `cPopulationCell::DiffuseGenomeFragments` for every cell:
                // `InitHGTSupport()` (no visible effect), then method 0 does
                // nothing and any other method aborts at the first cell.
                if !self.pop.cells.is_empty() && self.cfg.hgt_diffusion_method != 0 {
                    self.abort_run(
                        "error: Invalid HGT_DIFFUSION_METHOD\n",
                        2,
                        "Invalid HGT_DIFFUSION_METHOD",
                    );
                }
                Ok(())
            }
            Pop2Action::AvidianConjugation { donation_p } => {
                self.avidian_conjugation(*donation_p);
                Ok(())
            }
            Pop2Action::RemovePredators => {
                // `cPopulation::RemovePredators`. QUIRK: `Die` takes the
                // organism out of the live list by moving the last entry
                // into its slot, which the loop then skips.
                let mut i = 0;
                while i < self.pop.live_org_list.len() {
                    let cell = self.pop.live_org_list[i];
                    let is_prey = self.pop.orgs[cell as usize]
                        .as_ref()
                        .is_none_or(|o| o.state.forage_target > -2);
                    if !is_prey {
                        self.kill_organism(cell, None);
                    }
                    i += 1;
                }
                Ok(())
            }
            Pop2Action::SetInstProperty {
                prob_fail,
                inst_name,
                value,
            } => self.pop2_set_inst_property(*prob_fail, inst_name, *value),
        }
    }

    /// Called once before the first update, after the event list is loaded
    /// (where the C++ constructors attach data recorders).
    pub(crate) fn pop2_note_recorders(&mut self) {}

    /// Called at the end of every update, after the organism statistics are
    /// gathered and before the status line.
    pub(crate) fn pop2_end_update(&mut self) {}

    /// `ctx.Driver().Feedback().Warning(msg)`
    fn pop2_warning(&mut self, msg: &str) {
        self.action_state.stdout.push_str("warning: ");
        self.action_state.stdout.push_str(msg);
        self.action_state.stdout.push('\n');
    }

    /// A genome of the default instruction set holding `seq`.
    fn pop2_default_genome(&self, seq: InstructionSequence) -> Result<Genome, String> {
        let is = self
            .hw
            .registry()
            .shared_inst_set(0)
            .ok_or("no instruction set")?;
        let mut props = PropertyMap::new();
        setup_property_map(&mut props, is.inst_set_name());
        Ok(Genome::new(is.hardware_type(), &props, seq))
    }

    /// `cActionInjectRandom::Process`
    fn pop2_inject_random(
        &mut self,
        length: i32,
        cell_id: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    ) -> Result<(), String> {
        if cell_id < 0 || cell_id >= self.pop.cells.len() as i32 {
            self.pop2_warning("InjectRandom has invalid cell id!");
            return Ok(());
        }
        let is = self
            .hw
            .registry()
            .shared_inst_set(0)
            .ok_or("InjectRandom: no instruction set")?;
        let mut seq = InstructionSequence::with_size(length);
        for i in 0..length {
            seq.set(i, is.random_inst(&mut self.rng));
        }
        let genome = self.pop2_default_genome(seq)?;
        self.inject(
            &genome,
            division_source("random"),
            cell_id,
            merit,
            lineage_label,
            neutral_metric,
        )
    }

    /// `cActionInjectWellMixed::Process`
    fn pop2_inject_well_mixed(
        &mut self,
        filename: &str,
        cell_num: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    ) -> Result<(), String> {
        if filename.is_empty() {
            eprintln!("error: no organism file specified");
            return Ok(());
        }
        let world_size = self.pop.cells.len() as i32;
        if cell_num < 0 || cell_num > world_size {
            self.pop2_warning("InjectWellMixed has invalid range!");
            return Ok(());
        }
        let Some(genome) = self.load_genome(filename, "", false)? else {
            return Ok(());
        };
        for _ in 0..cell_num {
            // Find a random cell to use. QUIRK: `GetInt(0, world_size-1)`
            // never picks the last cell.
            let target_cell = self.rng.get_int_range(0, world_size - 1);
            self.inject(
                &genome,
                injected_source(),
                target_cell,
                merit,
                lineage_label,
                neutral_metric,
            )?;
        }
        self.pop.sync_events = true;
        Ok(())
    }

    /// `cActionKillInstLimit::Process` (`inst2 == None`) and
    /// `cActionKillInstPair::Process`.
    fn pop2_kill_inst(
        &mut self,
        killprob: f64,
        inst1: &str,
        inst2: Option<&String>,
        limit: f64,
    ) -> Result<(), String> {
        for i in 0..self.pop.cells.len() as i32 {
            if !self.is_occupied(i) {
                continue;
            }
            let org = self.pop.orgs[i as usize]
                .as_ref()
                .ok_or("KillInst: cell holds the running organism")?;
            let genome = org.state.genome();
            let is = self
                .hw
                .registry()
                .get_inst_set(genome.inst_set())
                .ok_or("KillInst: organism of an unknown instruction set")?;
            let seq = genome.representation();
            let count = |name: &str| -> Result<i32, String> {
                // `GetInst` of an unknown name prints its message and exits.
                is.get_inst(name).map(|inst| seq.count_inst(inst))
            };
            let kill = match count(inst1).and_then(|c1| match inst2 {
                None => Ok(f64::from(c1) >= limit),
                Some(n2) => count(n2).map(|c2| f64::from(c1) >= limit && f64::from(c2) >= limit),
            }) {
                Ok(k) => k,
                Err(msg) => {
                    eprintln!("{msg}");
                    self.abort_run("", 1, &msg);
                    return Ok(());
                }
            };
            if kill && self.rng.p(killprob) {
                self.kill_organism(i, None);
            }
        }
        Ok(())
    }

    /// `cActionApplyBottleneck::Process`
    fn pop2_apply_bottleneck(&mut self, bottleneck_size: i32) -> Result<(), String> {
        let size = self.pop.cells.len() as i32;
        let mut num_to_kill = self.pop.num_organisms - bottleneck_size;
        if num_to_kill > 0 && !(0..size - 1).any(|c| self.is_occupied(c)) {
            return Err("ApplyBottleneck: only the last cell is occupied (C++: \
                        `GetInt(0, size - 1)` never picks it and the loop never ends)"
                .to_string());
        }
        while num_to_kill > 0 {
            let target_cell = self.rng.get_int_range(0, size - 1);
            if self.is_occupied(target_cell) {
                self.kill_organism(target_cell, None);
                num_to_kill -= 1;
            }
        }
        Ok(())
    }

    /// The genotype id of the organism in `cell`.
    fn pop2_genotype_id(&self, cell: i32) -> Option<i32> {
        let g = self
            .systematics
            .genotype_of(&self.pop.membership[cell as usize])?;
        self.systematics.group_id(g)
    }

    /// `cActionKillDominantGenotype::Process`
    fn pop2_kill_dominant_genotype(&mut self) -> Result<(), String> {
        let dom = self
            .systematics
            .dominant_genotype()
            .ok_or("KillDominantGenotype without genotypes (C++: NULL dereference)")?;
        let dom_id = self.systematics.group_id(dom);
        let doomed: Vec<i32> = self
            .pop
            .live_org_list
            .iter()
            .copied()
            .filter(|&c| self.pop2_genotype_id(c) == dom_id)
            .collect();
        for cell in doomed {
            self.kill_organism(cell, None);
        }
        Ok(())
    }

    /// `cActionProbKillSequence::Process`. QUIRK: the test is a bitwise
    /// `&`, so the kill probability is drawn for every living organism.
    fn pop2_kill_prob_sequence(&mut self, sequence: &str, killprob: f64) -> Result<(), String> {
        let arg_genome = self.pop2_default_genome(InstructionSequence::from_string(sequence))?;
        let live = self.pop.live_org_list.clone();
        let mut doomed = Vec::new();
        for cell in live {
            let same = self.pop.orgs[cell as usize]
                .as_ref()
                .is_some_and(|o| *o.state.genome() == arg_genome);
            let draw = self.rng.p(killprob);
            if same & draw {
                doomed.push(cell);
            }
        }
        for cell in doomed {
            self.kill_organism(cell, None);
        }
        Ok(())
    }

    /// The kill loop of `cActionKillFractionInSequence` and
    /// `cActionKillFractionInSequence_PopLimit`.
    fn pop2_kill_in_sequence(
        &mut self,
        mut to_kill: i32,
        kill_index: &Cell<i32>,
    ) -> Result<(), String> {
        if to_kill > self.pop.num_organisms {
            return Err(format!(
                "KillFractionInSequence: {to_kill} kills among {} organisms (C++: the \
                 assert is compiled out and the loop never ends)",
                self.pop.num_organisms
            ));
        }
        let size = self.pop.cells.len() as i32;
        while to_kill > 0 {
            let cell = kill_index.get();
            if self.is_occupied(cell) {
                self.kill_organism(cell, None);
                to_kill -= 1;
            }
            kill_index.set((cell + 1) % size);
        }
        Ok(())
    }

    /// `cActionKillRectangle`: the constructor's clamping, then `Process`.
    fn pop2_kill_rectangle(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) {
        let (wx, wy) = (self.pop.world_x, self.pop.world_y);
        let clamp = |v: i32, n: i32| {
            if v < 0 {
                0
            } else if v > n - 1 {
                n - 1
            } else {
                v
            }
        };
        let (x1, mut x2) = (clamp(x1, wx), clamp(x2, wx));
        let (y1, mut y2) = (clamp(y1, wy), clamp(y2, wy));
        // Account for a rectangle that crosses over the Zero X or Y cell
        if x2 < x1 {
            x2 += wx;
        }
        if y2 < y1 {
            y2 += wy;
        }
        for i in y1..=y2 {
            for j in x1..=x2 {
                let loc = (i % wy) * wx + (j % wx);
                if self.is_occupied(loc) {
                    self.kill_organism(loc, None);
                }
            }
        }
        self.pop.sync_events = true;
    }

    /// `cPopulation::SerialTransfer(transfer_size, ignore_deads, ctx)`
    fn pop2_serial_transfer(
        &mut self,
        transfer_size: i32,
        ignore_deads: bool,
    ) -> Result<(), String> {
        // If we are ignoring all dead organisms, remove them from the population.
        if ignore_deads {
            for i in 0..self.pop.cells.len() as i32 {
                if !self.is_occupied(i) {
                    continue;
                }
                let g = self
                    .systematics
                    .genotype_of(&self.pop.membership[i as usize])
                    .ok_or("SerialTransfer: organism without a genotype")?;
                let fitness = self
                    .genome_test_metrics(g)
                    .ok_or("SerialTransfer: genotype without a genome")?
                    .fitness;
                if let Some(f) = self.take_fatal() {
                    return Err(f);
                }
                if fitness == 0.0 {
                    self.kill_organism(i, None);
                }
            }
        }

        // If removing the dead was enough, stop here.
        if self.pop.num_organisms <= transfer_size {
            return Ok(());
        }

        // Collect a vector of the occupied cells...
        let mut transfer_pool: Vec<i32> = (0..self.pop.cells.len() as i32)
            .filter(|&i| self.is_occupied(i))
            .collect();

        // Remove the proper number of cells.
        let removal_size = self.pop.num_organisms - transfer_size;
        for _ in 0..removal_size {
            let j = self.rng.get_uint(transfer_pool.len() as u32) as usize;
            self.kill_organism(transfer_pool[j], None);
            transfer_pool.swap_remove(j);
        }
        Ok(())
    }

    /// `cActionSetMutProb::Process` and `cActionModMutProb::Process`, with
    /// the constructor's cell-range normalisation.
    fn pop2_set_mut_prob(
        &mut self,
        mut_type: Option<MutType>,
        prob: f64,
        start: i32,
        end: i32,
        modify: bool,
    ) {
        let size = self.pop.cells.len() as i32;
        let (mut start, mut end) = (start, end);
        let mut setconf = false;
        if start < 0 {
            // start == -1  -->  all
            setconf = true;
            start = 0;
            end = size;
        }
        if end < 0 {
            // end == -1 --> Only one cell!
            end = start + 1;
        }
        if end < start {
            std::mem::swap(&mut start, &mut end);
        }
        if end > size {
            end = size;
        }
        let Some(t) = mut_type else {
            return;
        };
        let new_prob = if modify {
            prob + t.mod_base(&self.cfg)
        } else {
            prob
        };
        if setconf {
            // QUIRK: ModMutProb stores the change itself, not the new rate,
            // for the parent, death and point types.
            let conf_value = match t {
                MutType::PMut
                | MutType::PIns
                | MutType::PDel
                | MutType::Death
                | MutType::PntMut
                | MutType::PntIns
                | MutType::PntDel => prob,
                _ => new_prob,
            };
            *t.config_setting(&mut self.cfg) = conf_value;
        }
        for i in start..end {
            if let Some(rate) = t.cell_rate(&mut self.pop.cells[i as usize].mut_rates) {
                *rate = new_prob;
            }
        }
    }

    /// `cActionSeverGridCol::Process` (`row == false`) and
    /// `cActionSeverGridRow::Process`.
    fn pop2_sever_grid(&mut self, row: bool, id: i32, min: i32, max: i32) {
        let (wx, wy) = (self.pop.world_x, self.pop.world_y);
        let (across, along) = if row { (wy, wx) } else { (wx, wy) };
        let id = if id == -1 { across / 2 } else { id };
        let max = if max == -1 { along } else { max };
        if id < 0 || id >= across {
            let what = if row {
                format!("Row ID {id} out of range for SeverGridRow")
            } else {
                format!("Column ID {id} out of range for SeverGridCol")
            };
            self.pop2_warning(&what);
            return;
        }
        let gn = |cell: i32, dx: i32, dy: i32| grid_neighbor(cell, wx, wy, dx, dy);
        for k in min..max {
            let (id_a, id_b, id_a0, id_a1, id_b0, id_b1);
            if row {
                id_a = id * wx + k;
                id_b = gn(id_a, 0, -1);
                id_a0 = gn(id_a, -1, 0);
                id_a1 = gn(id_a, 1, 0);
                id_b0 = gn(id_a, -1, -1);
                id_b1 = gn(id_a, 1, -1);
            } else {
                id_a = k * wx + id;
                id_b = gn(id_a, -1, 0);
                id_a0 = gn(id_a, 0, -1);
                id_a1 = gn(id_a, 0, 1);
                id_b0 = gn(id_a, -1, -1);
                id_b1 = gn(id_a, -1, 1);
            }
            let cells = &mut self.pop.cells;
            for b in [id_b, id_b0, id_b1] {
                cells[id_a as usize].connections.remove(b);
            }
            for a in [id_a, id_a0, id_a1] {
                cells[id_b as usize].connections.remove(a);
            }
        }
    }

    /// `cActionJoinGridCol::Process` (`row == false`) and
    /// `cActionJoinGridRow::Process`.
    fn pop2_join_grid(&mut self, row: bool, id: i32, min: i32, max: i32) {
        let (wx, wy) = (self.pop.world_x, self.pop.world_y);
        let bounded = self.cfg.world_geometry == geometry::GRID;
        let (across, along) = if row { (wy, wx) } else { (wx, wy) };
        let id = if id == -1 { across / 2 } else { id };
        let max = if max == -1 { along } else { max };
        if id < 0 || id >= across {
            let what = if row {
                format!("Row ID {id} out of range for JoinGridRow")
            } else {
                format!("Column ID {id} out of range for JoinGridCol")
            };
            self.pop2_warning(&what);
            return;
        }
        let gn = |cell: i32, dx: i32, dy: i32| grid_neighbor(cell, wx, wy, dx, dy);
        let join = |cells: &mut Vec<crate::population_cell::PopulationCell>, a: i32, b: i32| {
            let list = &mut cells[a as usize].connections;
            if !list.contains(b) {
                list.push(b);
            }
        };
        for k in min..max {
            let (id_a, id_b) = if row {
                let a = id * wx + k;
                (a, gn(a, 0, -1))
            } else {
                let a = k * wx + id;
                (a, gn(a, -1, 0))
            };
            let cells = &mut self.pop.cells;
            // these cells are always joined
            join(cells, id_a, id_b);
            join(cells, id_b, id_a);
            // make sure we don't break the bounded grid at the low edge
            if !bounded || k != 0 {
                let (a0, b0) = if row {
                    (gn(id_a, -1, 0), gn(id_a, -1, -1))
                } else {
                    (gn(id_a, 0, -1), gn(id_a, -1, -1))
                };
                join(cells, id_a, b0);
                join(cells, id_b, a0);
            }
            // ... and at the high edge
            if !bounded || k != along - 1 {
                let (a1, b1) = if row {
                    (gn(id_a, 1, 0), gn(id_a, 1, -1))
                } else {
                    (gn(id_a, 0, 1), gn(id_a, -1, 1))
                };
                join(cells, id_a, b1);
                join(cells, id_b, a1);
            }
        }
    }

    /// `GetResourceCount().GetResourceCountID(name)`; the C++ prints the
    /// error and goes on with -1, which the `GetSpatialResource` call that
    /// follows would index with.
    fn pop2_res_id(&self, action: &str, resname: &str) -> Result<usize, String> {
        self.pop
            .resource_count
            .get_resource_count_id(resname)
            .map_err(|e| {
                eprintln!("{e}");
                format!("{action}: unknown resource '{resname}' (C++: GetSpatialResource(-1))")
            })
    }

    /// `GetSpatialResource(res_id).GetAmount(cell)`
    fn pop2_spatial_amount(&self, res_id: usize, cell: i32) -> Result<f64, String> {
        let res = self.pop.resource_count.get_spatial_resource(res_id);
        if cell < 0 || cell >= res.base().size() {
            return Err(format!(
                "resource {res_id} has no cell {cell} (C++: out-of-range GetAmount)"
            ));
        }
        Ok(res.base().get_amount(cell))
    }

    /// `cActionKillNBelowResourceThreshold::Process`
    fn pop2_kill_n_below(
        &mut self,
        numkills: i32,
        resname: &str,
        threshold: f64,
    ) -> Result<(), String> {
        let res_id = self.pop2_res_id("KillNBelowResourceThreshold", resname)?;
        let size = self.pop.cells.len() as i32;
        let (mut cells_scanned, mut orgs_killed, mut cells_empty) = (0i64, 0i64, 0i64);
        for _ in 0..numkills {
            let target_cell = self.rng.get_int_range(0, size - 1);
            let level = self.pop2_spatial_amount(res_id, target_cell)?;
            cells_scanned += 1;
            if level < threshold {
                if self.is_occupied(target_cell) {
                    self.kill_organism(target_cell, None);
                    orgs_killed += 1;
                } else {
                    cells_empty += 1;
                }
            }
        }
        self.stats.add_num_cells_scanned_at_kill(cells_scanned);
        self.stats.add_num_orgs_killed(orgs_killed);
        self.stats
            .add_num_unoccupied_cell_attempted_to_kill(cells_empty);
        Ok(())
    }

    /// The cells of the square of `radius` around `target_cell`, in the
    /// C++ loop order (rows, then columns), skipping those off a bounded
    /// grid.
    fn pop2_square(&self, target_cell: i32, radius: i32) -> Vec<i32> {
        let (wx, wy) = (self.pop.world_x, self.pop.world_y);
        let geometry = self.cfg.world_geometry;
        let (current_row, current_col) = (target_cell / wx, target_cell % wx);
        let mut cells = Vec::new();
        for row in current_row - radius..=current_row + radius {
            if (row < 0 || row >= wy) && geometry == geometry::GRID {
                continue;
            }
            for col in current_col - radius..=current_col + radius {
                if (col < 0 || col >= wx) && geometry == geometry::GRID {
                    continue;
                }
                cells.push(radius_cell(geometry, wx, wy, row, col));
            }
        }
        cells
    }

    /// The three `KillWithinRadius*` actions and
    /// `cActionKillMeanBelowThresholdPaintable` (`Painted`).
    fn pop2_kill_within_radius(
        &mut self,
        kind: RadiusKill,
        numradii: i32,
        radius: i32,
        resname: &str,
        threshold: f64,
        kill_prob: KillProb<'_>,
    ) -> Result<(), String> {
        let geometry = self.cfg.world_geometry;
        if geometry != geometry::GRID && geometry != geometry::TORUS {
            return Err(format!(
                "unsupported: KillWithinRadius with WORLD_GEOMETRY {geometry} (C++: assert \
                 compiled out, every cell maps to cell 0)"
            ));
        }
        // A negative radius (the asserts are compiled out) leaves the square
        // empty: the target cell is still drawn, nothing is scanned or
        // killed, and the mean variant compares an average of 0.
        let res_id = self.pop2_res_id("KillWithinRadius", resname)?;
        let adv_res_id = match kill_prob {
            KillProb::Painted(adv) => {
                Some(self.pop2_res_id("KillMeanBelowThresholdPaintable", adv)?)
            }
            KillProb::Fixed(_) => None,
        };
        let size = self.pop.cells.len() as i32;
        let (mut cells_scanned, mut orgs_killed, mut cells_empty) = (0i64, 0i64, 0i64);
        for _ in 0..numradii {
            let target_cell = self.rng.get_int_range(0, size - 1);
            let density = match (kill_prob, adv_res_id) {
                (KillProb::Fixed(d), _) => d,
                // Get the level of adversity.  Bounded by [0,1]
                (KillProb::Painted(_), Some(adv)) => {
                    self.pop2_spatial_amount(adv, target_cell)?.clamp(0.0, 1.0)
                }
                (KillProb::Painted(_), None) => 0.0,
            };
            let square = self.pop2_square(target_cell, radius);
            let kill_here = match kind {
                RadiusKill::Focal => self.pop2_spatial_amount(res_id, target_cell)? < threshold,
                RadiusKill::Mean => {
                    let mut sum = DoubleSum::new();
                    for &c in &square {
                        sum.add(self.pop2_spatial_amount(res_id, c)?);
                        cells_scanned += 1;
                    }
                    sum.average() < threshold
                }
                RadiusKill::TestAll => true,
            };
            if !kill_here {
                continue;
            }
            for &c in &square {
                if kind == RadiusKill::Focal {
                    cells_scanned += 1;
                }
                if kind == RadiusKill::TestAll {
                    cells_scanned += 1;
                    if self.pop2_spatial_amount(res_id, c)? >= threshold {
                        continue;
                    }
                }
                if self.is_occupied(c) && self.rng.p(density) {
                    self.kill_organism(c, None);
                    orgs_killed += 1;
                } else {
                    cells_empty += 1;
                }
            }
        }
        self.stats.add_num_cells_scanned_at_kill(cells_scanned);
        self.stats.add_num_orgs_killed(orgs_killed);
        self.stats
            .add_num_unoccupied_cell_attempted_to_kill(cells_empty);
        Ok(())
    }

    /// `cActionSetRedundancy::Process` / `cActionSetProbFail::Process` on
    /// `GetInstSet("(default)")`. The C++ hardware shares the instruction
    /// set by reference, so every organism sees the change; here each
    /// organism's hardware hands its handle back while the set is changed.
    fn pop2_set_inst_property(
        &mut self,
        prob_fail: bool,
        inst_name: &str,
        value: i32,
    ) -> Result<(), String> {
        let shared = self
            .hw
            .registry()
            .shared_inst_set(0)
            .ok_or("SetRedundancy: no instruction set")?;
        let inst = match shared.get_inst(inst_name) {
            Ok(inst) => inst,
            Err(msg) => {
                // `GetInst` prints its message and exits.
                eprintln!("{msg}");
                self.abort_run("", 1, &msg);
                return Ok(());
            }
        };
        let placeholder = Arc::new(InstSet::new(
            "",
            shared.hardware_type(),
            shared.inst_lib().clone(),
            0,
            0,
        ));
        let mut holders = Vec::new();
        for (c, slot) in self.pop.orgs.iter_mut().enumerate() {
            if let Some(org) = slot.as_mut() {
                let base = org.hardware.base_mut();
                if Arc::ptr_eq(&base.inst_set, &shared) {
                    base.inst_set = placeholder.clone();
                    holders.push(c);
                }
            }
        }
        drop(shared);
        let changed = match self.hw.registry_mut().get_inst_set_mut("(default)") {
            Some(is) => {
                if prob_fail {
                    is.set_prob_fail(inst, f64::from(value));
                } else {
                    is.set_redundancy(inst, value);
                }
                true
            }
            None => false,
        };
        let shared = self
            .hw
            .registry()
            .shared_inst_set(0)
            .ok_or("SetRedundancy: no instruction set")?;
        for c in holders {
            if let Some(org) = self.pop.orgs[c].as_mut() {
                org.hardware.base_mut().inst_set = shared.clone();
            }
        }
        if changed {
            Ok(())
        } else {
            Err(
                "unsupported: SetRedundancy/SetProbFail while organisms outside the cells \
                 hold the instruction set (C++: shared cInstSet)"
                    .to_string(),
            )
        }
    }
}

/// The kill probability of a `KillWithinRadius*` square.
#[derive(Clone, Copy)]
enum KillProb<'a> {
    Fixed(f64),
    /// The adversity resource of `KillMeanBelowThresholdPaintable`.
    Painted(&'a str),
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTX: ParseContext<'static> = ParseContext {
        default_inst_set: "heads_default",
    };

    fn p(name: &str, args: &str) -> Pop2Action {
        let mut fb = Feedback::new();
        let a = parse(name, args, &CTX, &mut fb).unwrap().unwrap();
        assert_eq!(fb.num_errors(), 0, "{}", fb.render());
        a
    }

    #[test]
    fn every_name_parses() {
        for n in POP2_ACTIONS {
            let mut fb = Feedback::new();
            assert!(parse(n, "", &CTX, &mut fb).is_some(), "{n}");
            assert!(crate::action_library::is_registered(n), "{n}");
        }
        let mut fb = Feedback::new();
        assert!(parse("Inject", "", &CTX, &mut fb).is_none());
    }

    #[test]
    fn inject_args() {
        assert_eq!(
            p("InjectRandom", "50 3"),
            Pop2Action::InjectRandom {
                length: 50,
                cell_id: 3,
                merit: -1.0,
                lineage_label: 0,
                neutral_metric: 0.0
            }
        );
        match p("InjectSequenceWDivMutRate", "abc 4") {
            Pop2Action::InjectSequenceWDivMutRate {
                cell_start,
                cell_end,
                div_mut_rate,
                ..
            } => assert_eq!((cell_start, cell_end, div_mut_rate), (4, 5, 0.0)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn kill_args() {
        assert_eq!(p("KillRate", ""), Pop2Action::KillProb { killprob: 0.9 });
        assert_eq!(
            p("KillInstLimit", "0.5 nop-A"),
            Pop2Action::KillInstLimit {
                killprob: 0.5,
                inst: "nop-A".into(),
                limit: 5.0
            }
        );
        assert_eq!(
            p("SerialTransfer", "-3 0"),
            Pop2Action::SerialTransfer {
                size: 1,
                ignore_deads: false
            }
        );
    }

    #[test]
    fn mut_prob_args() {
        assert_eq!(
            p("SetMutProb", ""),
            Pop2Action::SetMutProb {
                mut_type: Some(MutType::CMut),
                prob: 0.0,
                start: -1,
                end: -1,
                modify: false
            }
        );
        match p("ModMutProb", "div_ins 0.1 3") {
            Pop2Action::SetMutProb {
                mut_type, modify, ..
            } => {
                assert_eq!(mut_type, Some(MutType::DsIns));
                assert!(modify);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            p("SetMutProb", "nonsense 1"),
            Pop2Action::SetMutProb { mut_type: None, .. }
        ));
    }

    #[test]
    fn conjugation_probability_is_checked() {
        let mut fb = Feedback::new();
        let _ = parse("AvidianConjugation", "", &CTX, &mut fb);
        assert_eq!(fb.num_errors(), 1);
        assert_eq!(
            p("AvidianConjugation", "0.5"),
            Pop2Action::AvidianConjugation { donation_p: 0.5 }
        );
    }

    #[test]
    fn grid_args() {
        assert_eq!(
            p("SeverGridRow", ""),
            Pop2Action::SeverGrid {
                row: true,
                id: -1,
                min: 0,
                max: -1
            }
        );
        assert_eq!(
            p("JoinGridCol", "3 1 4"),
            Pop2Action::JoinGrid {
                row: false,
                id: 3,
                min: 1,
                max: 4
            }
        );
    }
}
