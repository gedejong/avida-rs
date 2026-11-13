//! EnvironmentActions.cc, DriverActions.cc and SaveLoadActions.cc actions:
//! resource and reaction changes, task arguments, exit conditions and the
//! genotype-list loaders (avida-core/source/actions at 47f13dadb), with the
//! `cPopulation` resource helpers they call (`UpdateResource`,
//! `SetResource`, `SetResourceInflow`, `UpdateInflow`, `UpdateGradient*`,
//! `UpdateResourceCount`, `ResetInputs`, ...).
//!
//! Resource ids versus indices: the C++ addresses the population's
//! `cResourceCount` sometimes by `cResource::GetIndex()` (the position among
//! global resources) and sometimes by `GetID()` (the position in the
//! environment). The two differ once deme resources are defined; each action
//! below uses the one its C++ uses, and an id past the end of the global
//! resources (an out-of-bounds access in C++) is a fatal error here.
//!
//! FMA: the seasonal and periodic resource curves and the scaled outflow are
//! fused exactly where the reference build fuses them (checked bit for bit
//! against clang on this machine).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use avida_config::config::Config;
use avida_config::feedback::{Feedback, FeedbackKind};
use avida_config::string_util::{as_double, as_int, pop, pop_word, return_array};
use avida_genome::{ArgContainer, ArgSchema};
use avida_resources::ResourceCount;

use crate::actions::ParseContext;
use crate::population::resource_setup_from;
use crate::world::World;

/// The registered action names this module parses.
pub const ENV_ACTIONS: &[&str] = &[
    // EnvironmentActions.cc
    "InjectResource",
    "InjectScaledResource",
    "OutflowScaledResource",
    "SetResource",
    "SetDemeResource",
    "ZeroResources",
    "SetCellResource",
    "InjectCellResource",
    "ChangeEnvironment",
    "SetGradientResource",
    "SetGradientPlatInflow",
    "SetGradientPlatOutflow",
    "SetGradientConeInflow",
    "SetGradientConeOutflow",
    "SetGradientInflow",
    "SetGradPlatVarInflow",
    "SetPredatoryResource",
    "SetProbabilisticResource",
    "DecInflow",
    "IncInflow",
    "SetReactionValue",
    "SetReactionValueMult",
    "SetReactionInst",
    "SetReactionMinTaskCount",
    "SetReactionMaxTaskCount",
    "SetReactionTask",
    "SetResourceInflow",
    "SetPeriodicResourceInflow",
    "SetResourceOutflow",
    "SetDemeResourceInflow",
    "SetDemeResourceOutflow",
    "SetEnvironmentRandomMask",
    "SetSeasonalResource",
    "SetSeasonalResource1Kyears_1To_1",
    "SetSeasonalResource10Kyears_1To_1",
    "SetPeriodicResource",
    "SetTaskArgInt",
    "SetTaskArgDouble",
    "SetTaskArgString",
    "SetOptimizeMinMax",
    "SetDemeIOGrid",
    "SetFracDemeTreatable",
    "AlterMigrationConnection",
    "SetConfig",
    // DriverActions.cc
    "ExitAveLineageLabelGreater",
    "ExitAveLineageLabelLess",
    "ExitAveGeneration",
    "ExitElapsedTime",
    "ExitDemeResources",
    "Pause",
    // SaveLoadActions.cc
    "LoadParasiteGenotypeList",
    "LoadHostGenotypeList",
];

/// Which `cPopulation::UpdateGradient*` an action calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientFlow {
    PlatInflow,
    PlatOutflow,
    ConeInflow,
    ConeOutflow,
    Inflow,
}

/// The three `cActionSetSeasonalResource*` curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seasonal {
    /// `cActionSetSeasonalResource`
    Base,
    /// `cActionSetSeasonalResource1Kyears_1To_1`
    OneK,
    /// `cActionSetSeasonalResource10Kyears_1To_1`
    TenK,
}

/// The value of a `cActionSetTaskArg*` action.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskArgValue {
    Int(i32),
    Double(f64),
    Str(String),
}

/// A parsed action of this module. Field names follow the C++ members
/// (`m_` dropped).
#[derive(Debug, Clone, PartialEq)]
pub enum EnvAction {
    /// `cActionInjectResource`
    InjectResource { res_name: String, res_count: f64 },
    /// `cActionInjectScaledResource`
    InjectScaledResource { res_name: String, res_count: f64 },
    /// `cActionOutflowScaledResource`
    OutflowScaledResource { res_name: String, res_percent: f64 },
    /// `cActionSetResource`
    SetResource { res_name: String, res_count: f64 },
    /// `cActionSetDemeResource`
    SetDemeResource { res_name: String, res_count: f64 },
    /// `cZeroResources`
    ZeroResources,
    /// `cActionSetCellResource` (`inject == false`) and
    /// `cActionInjectCellResource`
    CellResource {
        cell_list: Vec<i32>,
        res_name: String,
        res_count: f64,
        inject: bool,
    },
    /// `cActionChangeEnvironment`
    ChangeEnvironment { env_string: String },
    /// `cActionSetGradientResource`
    SetGradientResource {
        env_string: String,
        res_name: String,
    },
    /// `cActionSetGradientPlatInflow`, `...PlatOutflow`, `...ConeInflow`,
    /// `...ConeOutflow` and `cActionSetGradientInflow`
    GradientFlow {
        flow: GradientFlow,
        res_name: String,
        value: f64,
    },
    /// `cActionSetGradPlatVarInflow`
    SetGradPlatVarInflow {
        res_name: String,
        mean: f64,
        variance: f64,
        kind: i32,
    },
    /// `cActionSetPredatoryResource`
    SetPredatoryResource {
        res_name: String,
        odds: f64,
        juvs_per: i32,
        detection_prob: f64,
    },
    /// `cActionSetProbabilisticResource` (`res_name == None`: the schema
    /// default, the name of resource 0)
    SetProbabilisticResource {
        res_name: Option<String>,
        initial: f64,
        inflow: f64,
        outflow: f64,
        lambda: f64,
        theta: f64,
        x: i32,
        y: i32,
        count: i32,
    },
    /// `cActionIncInflow` (and `cActionDecInflow`, with the change negated)
    UpdateInflow { res_name: String, change: f64 },
    /// `cActionSetReactionValue`
    SetReactionValue { name: String, value: f64 },
    /// `cActionSetReactionValueMult`
    SetReactionValueMult { name: String, value: f64 },
    /// `cActionSetReactionInst`
    SetReactionInst { name: String, inst: String },
    /// `cActionSetReactionMinTaskCount` (`max == false`) and
    /// `cActionSetReactionMaxTaskCount`
    SetReactionTaskCount { name: String, count: i32, max: bool },
    /// `cActionSetReactionTask`
    SetReactionTask { name: String, task: String },
    /// `cActionSetResourceInflow`
    SetResourceInflow { name: String, inflow: f64 },
    /// `cActionSetPeriodicResourceInflow`; `instance` keys the state the C++
    /// keeps in the action object (`m_first`, `m_start`, `m_1st_inflow`).
    SetPeriodicResourceInflow {
        instance: usize,
        name: String,
        period: i32,
    },
    /// `cActionSetResourceOutflow`
    SetResourceOutflow { name: String, outflow: f64 },
    /// `cActionSetDemeResourceInflow` (`outflow == false`) and
    /// `cActionSetDemeResourceOutflow`
    SetDemeResourceFlow {
        deme_id: i32,
        name: String,
        value: f64,
        outflow: bool,
    },
    /// `cActionSetEnvironmentRandomMask`
    SetEnvironmentRandomMask { mask: u32 },
    /// `cActionSetSeasonalResource*`
    SetSeasonalResource {
        res_name: String,
        curve: Seasonal,
        scale: f64,
    },
    /// `cActionSetPeriodicResource`
    SetPeriodicResource {
        res_name: String,
        amplitude: f64,
        frequency: f64,
        phase_shift: f64,
        init_y: f64,
    },
    /// `cActionSetTaskArgInt`, `...Double`, `...String`
    SetTaskArg {
        task: i32,
        arg: i32,
        value: TaskArgValue,
    },
    /// `cActionSetOptimizeMinMax`
    SetOptimizeMinMax,
    /// `cActionSetDemeIOGrid`
    SetDemeIOGrid {
        input_output: String,
        cell_list: Vec<i32>,
    },
    /// `cActionSetFracDemeTreatable`
    SetFracDemeTreatable { fraction: f64 },
    /// `cActionAlterMigrationConnection`
    AlterMigrationConnection {
        from_deme: i32,
        to_deme: i32,
        alter_amount: f64,
    },
    /// `cActionSetConfig`
    SetConfig { cvar: String, value: String },
    /// `cActionExitAveLineageLabelGreater` (`greater == true`) and
    /// `cActionExitAveLineageLabelLess`
    ExitAveLineageLabel { threshold: f64, greater: bool },
    /// `cActionExitAveGeneration`
    ExitAveGeneration { tgt_gen: f64 },
    /// `cActionExitElapsedTime`: `then` is `time(0)` when the event file
    /// was loaded.
    ExitElapsedTime { time: i64, then: i64 },
    /// `cActionExitDemeResources`
    ExitDemeResources { deme_res: i32 },
    /// `cActionPause` (`Avida2Driver::Pause` does nothing)
    Pause,
    /// `cActionLoadParasiteGenotypeList` (`parasite == true`) and
    /// `cActionLoadHostGenotypeList`
    LoadGenotypeList { filename: String, parasite: bool },
}

/// State the actions of this module keep between calls.
#[derive(Debug, Default)]
pub struct EnvState {
    /// `cActionSetPeriodicResourceInflow` per instance: `(m_start,
    /// m_1st_inflow)` once its first `Process` ran.
    pub periodic_inflow: BTreeMap<usize, (i32, f64)>,
    /// `cPopulation::parasite_genotype_list` (sequence strings).
    pub parasite_genotype_list: Vec<String>,
    /// `cPopulation::host_genotype_list` (sequence strings).
    pub host_genotype_list: Vec<String>,
    /// A `Pause` action ran (the command-line driver ignores it; a front
    /// end may honour it).
    pub pause_requested: bool,
}

/// Instance ids of `SetPeriodicResourceInflow` actions.
static NEXT_INSTANCE: AtomicUsize = AtomicUsize::new(0);

/// `cString largs(args)` with `if (largs.GetSize()) x = largs.PopWord()...`.
struct Args(String);

impl Args {
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
    /// `if (largs.GetSize()) m_cell_list = cStringUtil::ReturnArray(largs.PopWord());`
    fn cell_list(&mut self) -> Vec<i32> {
        match self.word() {
            Some(mut w) => return_array(&mut w),
            None => Vec::new(),
        }
    }
}

/// `time(0)`
fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A constructor that calls `Feedback().Error(...)` and `Abort(...)` while
/// the event file loads, where the C++ driver is not yet attached.
fn constructor_abort(name: &str, msg: &str) -> Option<Result<EnvAction, String>> {
    Some(Err(format!(
        "unsupported: {name} {msg} (C++: Feedback().Error and Abort through the driver, \
         which is not attached while the event file loads)"
    )))
}

/// The default of `res_name` in `cActionSetProbabilisticResource` (the
/// name of resource 0, filled in when the action runs).
const RES0: &str = "\u{0}res0";

/// The constructor of the action `name` (a [`ENV_ACTIONS`] spelling) applied to
/// `args`; `None` for any other name.
pub fn parse(
    name: &str,
    args: &str,
    ctx: &ParseContext<'_>,
    feedback: &mut Feedback,
) -> Option<Result<EnvAction, String>> {
    let _ = ctx;
    let mut a = Args(args.to_string());
    Some(Ok(match name {
        "InjectResource" => EnvAction::InjectResource {
            res_name: a.string(""),
            res_count: a.double(0.0),
        },
        "InjectScaledResource" => EnvAction::InjectScaledResource {
            res_name: a.string(""),
            res_count: a.double(0.0),
        },
        "OutflowScaledResource" => EnvAction::OutflowScaledResource {
            res_name: a.string(""),
            res_percent: a.double(0.0),
        },
        "SetResource" => EnvAction::SetResource {
            res_name: a.string(""),
            res_count: a.double(0.0),
        },
        "SetDemeResource" => EnvAction::SetDemeResource {
            res_name: a.string(""),
            res_count: a.double(0.0),
        },
        "ZeroResources" => EnvAction::ZeroResources,
        "SetCellResource" | "InjectCellResource" => EnvAction::CellResource {
            cell_list: a.cell_list(),
            res_name: a.string(""),
            res_count: a.double(0.0),
            inject: name == "InjectCellResource",
        },
        "ChangeEnvironment" => EnvAction::ChangeEnvironment {
            env_string: args.to_string(),
        },
        "SetGradientResource" => {
            // `env_string = largs; largs.PopWord();
            // m_res_name = largs.PopWord().Pop(':');`
            let env_string = args.to_string();
            let _ = a.word();
            let res_name = a.word().map(|mut w| pop(&mut w, ':')).unwrap_or_default();
            EnvAction::SetGradientResource {
                env_string,
                res_name,
            }
        }
        "SetGradientPlatInflow"
        | "SetGradientPlatOutflow"
        | "SetGradientConeInflow"
        | "SetGradientConeOutflow"
        | "SetGradientInflow" => EnvAction::GradientFlow {
            flow: match name {
                "SetGradientPlatInflow" => GradientFlow::PlatInflow,
                "SetGradientPlatOutflow" => GradientFlow::PlatOutflow,
                "SetGradientConeInflow" => GradientFlow::ConeInflow,
                "SetGradientConeOutflow" => GradientFlow::ConeOutflow,
                _ => GradientFlow::Inflow,
            },
            res_name: a.string(""),
            value: a.double(0.0),
        },
        "SetGradPlatVarInflow" => EnvAction::SetGradPlatVarInflow {
            res_name: a.string(""),
            mean: a.double(0.0),
            variance: a.double(0.0),
            kind: a.int(0),
        },
        "SetPredatoryResource" => EnvAction::SetPredatoryResource {
            res_name: a.string(""),
            odds: a.double(0.0),
            juvs_per: a.int(0),
            detection_prob: a.double(0.0),
        },
        "SetProbabilisticResource" => {
            let mut schema = ArgSchema::new(':', '=', false);
            schema.add_string("res_name", 0, RES0);
            schema.add_double("initial", 0, 0.0);
            schema.add_double("inflow", 1, 0.0);
            schema.add_double("outflow", 2, 0.0);
            schema.add_double("lambda", 3, 1.0);
            schema.add_double("theta", 4, 0.0);
            schema.add_int("x", 0, -1);
            schema.add_int("y", 1, -1);
            schema.add_int("num", 2, -1);
            // `if (args)` is always true, so a failed load dereferences the
            // NULL container.
            let Some(c) = ArgContainer::load(args, &schema, feedback) else {
                return Some(Err(
                    "unsupported: SetProbabilisticResource with arguments its \
                     schema rejects (C++: NULL cArgContainer dereference)"
                        .to_string(),
                ));
            };
            let res_name = c.get_string(0);
            EnvAction::SetProbabilisticResource {
                res_name: (res_name != RES0).then(|| res_name.to_string()),
                initial: c.get_double(0),
                inflow: c.get_double(1),
                outflow: c.get_double(2),
                lambda: c.get_double(3),
                theta: c.get_double(4),
                x: c.get_int(0),
                y: c.get_int(1),
                count: c.get_int(2),
            }
        }
        "DecInflow" | "IncInflow" => {
            let res_name = a.string("");
            let change = a.double(0.0);
            EnvAction::UpdateInflow {
                res_name,
                change: if name == "DecInflow" { -change } else { change },
            }
        }
        "SetReactionValue" => EnvAction::SetReactionValue {
            name: a.string(""),
            value: a.double(0.0),
        },
        "SetReactionValueMult" => EnvAction::SetReactionValueMult {
            name: a.string(""),
            value: a.double(0.0),
        },
        "SetReactionInst" => EnvAction::SetReactionInst {
            name: a.string(""),
            inst: a.string(""),
        },
        "SetReactionMinTaskCount" | "SetReactionMaxTaskCount" => EnvAction::SetReactionTaskCount {
            name: a.string(""),
            count: a.int(0),
            max: name == "SetReactionMaxTaskCount",
        },
        "SetReactionTask" => EnvAction::SetReactionTask {
            name: a.string(""),
            task: a.string(""),
        },
        "SetResourceInflow" => EnvAction::SetResourceInflow {
            name: a.string(""),
            inflow: a.double(0.0),
        },
        "SetPeriodicResourceInflow" => EnvAction::SetPeriodicResourceInflow {
            instance: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            name: a.string(""),
            period: a.int(0),
        },
        "SetResourceOutflow" => EnvAction::SetResourceOutflow {
            name: a.string(""),
            outflow: a.double(0.0),
        },
        "SetDemeResourceInflow" | "SetDemeResourceOutflow" => EnvAction::SetDemeResourceFlow {
            deme_id: a.int(-1),
            name: a.string(""),
            value: a.double(0.0),
            outflow: name == "SetDemeResourceOutflow",
        },
        // `m_mask = largs.PopWord().AsInt()` into an `unsigned int`.
        "SetEnvironmentRandomMask" => EnvAction::SetEnvironmentRandomMask {
            mask: a.int(0) as u32,
        },
        "SetSeasonalResource" => EnvAction::SetSeasonalResource {
            res_name: a.string(""),
            curve: Seasonal::Base,
            scale: 1.0,
        },
        "SetSeasonalResource1Kyears_1To_1" | "SetSeasonalResource10Kyears_1To_1" => {
            EnvAction::SetSeasonalResource {
                res_name: a.string(""),
                curve: if name == "SetSeasonalResource1Kyears_1To_1" {
                    Seasonal::OneK
                } else {
                    Seasonal::TenK
                },
                scale: a.double(1.0),
            }
        }
        "SetPeriodicResource" => EnvAction::SetPeriodicResource {
            res_name: a.string(""),
            amplitude: a.double(1.0),
            frequency: a.double(1.0),
            phase_shift: a.double(0.0),
            init_y: a.double(0.0),
        },
        "SetTaskArgInt" | "SetTaskArgDouble" | "SetTaskArgString" => {
            let task = a.int(0);
            let arg = a.int(0);
            let value = match name {
                "SetTaskArgInt" => TaskArgValue::Int(a.int(0)),
                "SetTaskArgDouble" => TaskArgValue::Double(a.double(0.0)),
                // `if (largs.GetSize()) m_value = largs;` (the rest of it)
                _ => TaskArgValue::Str(std::mem::take(&mut a.0)),
            };
            EnvAction::SetTaskArg { task, arg, value }
        }
        "SetOptimizeMinMax" => EnvAction::SetOptimizeMinMax,
        "SetDemeIOGrid" => {
            // `inputOutput = largs.Pop(':'); cell_list = ReturnArray(largs.Pop(':'));`
            let input_output = pop(&mut a.0, ':');
            let mut cell_list_str = pop(&mut a.0, ':');
            EnvAction::SetDemeIOGrid {
                input_output,
                cell_list: return_array(&mut cell_list_str),
            }
        }
        "SetFracDemeTreatable" => EnvAction::SetFracDemeTreatable {
            fraction: a.double(0.0),
        },
        // The C++ leaves missing members uninitialised; they read as 0 here.
        "AlterMigrationConnection" => EnvAction::AlterMigrationConnection {
            from_deme: a.int(0),
            to_deme: a.int(0),
            alter_amount: a.double(0.0),
        },
        "SetConfig" => {
            let cvar = a.string("");
            let value = a.string("");
            if !Config::default().has_entry(&cvar) {
                return constructor_abort("SetConfig", &format!("of unknown setting '{cvar}'"));
            }
            EnvAction::SetConfig { cvar, value }
        }
        "ExitAveLineageLabelGreater" | "ExitAveLineageLabelLess" => {
            EnvAction::ExitAveLineageLabel {
                threshold: a.double(0.0),
                greater: name == "ExitAveLineageLabelGreater",
            }
        }
        "ExitAveGeneration" => match a.word() {
            Some(w) => EnvAction::ExitAveGeneration {
                tgt_gen: as_double(&w),
            },
            None => return constructor_abort("ExitAveGeneration", "without a generation"),
        },
        "ExitElapsedTime" => match a.word() {
            Some(w) => EnvAction::ExitElapsedTime {
                time: i64::from(as_int(&w)),
                then: now_seconds(),
            },
            None => return constructor_abort("ExitElapsedTime", "without a time"),
        },
        "ExitDemeResources" => match a.word() {
            Some(w) => EnvAction::ExitDemeResources {
                deme_res: as_int(&w),
            },
            None => return constructor_abort("ExitDemeResources", "without an amount"),
        },
        "Pause" => EnvAction::Pause,
        "LoadParasiteGenotypeList" | "LoadHostGenotypeList" => EnvAction::LoadGenotypeList {
            filename: a.string(""),
            parasite: name == "LoadParasiteGenotypeList",
        },
        _ => return None,
    }))
}

/// `cResourceCount::SetInflow` / `SetDecay` / `GetInflow` of an unknown
/// name print their error and go on (writing at index -1 in C++).
fn report(r: Result<(), String>) {
    if let Err(e) = r {
        eprintln!("{e}");
    }
}

impl World {
    /// `Process(ctx)` of an action of this module.
    pub(crate) fn run_env_action(&mut self, action: &EnvAction) -> Result<(), String> {
        match action {
            EnvAction::InjectResource {
                res_name,
                res_count,
            } => {
                // `UpdateResource(ctx, res->GetID(), m_res_count)`
                if let Some(id) = self.env.resource_lib().get_resource_id(res_name) {
                    let idx = self.env_global_slot(id, "InjectResource")?;
                    self.with_pop_resources(|rc, rng, env| {
                        rc.modify_one(rng, env, idx, *res_count);
                    });
                }
                Ok(())
            }
            EnvAction::InjectScaledResource {
                res_name,
                res_count,
            } => {
                let ave_merit = self.env_scaled_ave_merit();
                if let Some(id) = self.env.resource_lib().get_resource_id(res_name) {
                    let idx = self.env_global_slot(id, "InjectScaledResource")?;
                    self.with_pop_resources(|rc, rng, env| {
                        rc.modify_one(rng, env, idx, *res_count / ave_merit);
                    });
                }
                Ok(())
            }
            EnvAction::OutflowScaledResource {
                res_name,
                res_percent,
            } => {
                let ave_merit = self.env_scaled_ave_merit();
                let Some(id) = self.env.resource_lib().get_resource_id(res_name) else {
                    return Err(format!(
                        "OutflowScaledResource: unknown resource '{res_name}' (C++: NULL \
                         dereference)"
                    ));
                };
                let idx = self.env_global_slot(id, "OutflowScaledResource")?;
                let mut res_level = self.with_pop_resources(|rc, rng, env| rc.get(rng, env, idx));
                let scaled_perc = 1.0 / (1.0 + ave_merit * (1.0 - res_percent) / res_percent);
                // FMA: `res_level -= res_level * scaled_perc`
                res_level = (-res_level).mul_add(scaled_perc, res_level);
                // QUIRK: the remaining level is added (`UpdateResource`), not
                // set.
                self.with_pop_resources(|rc, rng, env| rc.modify_one(rng, env, idx, res_level));
                Ok(())
            }
            EnvAction::SetResource {
                res_name,
                res_count,
            } => self.env_set_resource_by_name(res_name, *res_count),
            EnvAction::SetDemeResource {
                res_name,
                res_count,
            } => {
                // `cPopulation::SetDemeResource`: by index, in every deme.
                if let Some(id) = self.env.resource_lib().get_resource_id(res_name) {
                    let idx = self.env.resource_lib().resource(id).index();
                    for d in 0..self.pop.demes.len() {
                        let rc = &mut self.pop.demes[d].resource_count;
                        if idx < 0 || idx as usize >= rc.size() {
                            return Err(format!(
                                "SetDemeResource: resource '{res_name}' is not a deme resource \
                                 (C++: out-of-bounds cResourceCount::Set)"
                            ));
                        }
                        rc.set(&mut self.rng, &mut self.res_env, idx as usize, *res_count);
                    }
                }
                Ok(())
            }
            EnvAction::ZeroResources => {
                // `SetResource(ctx, res->GetID(), 0.0)` for every resource.
                for id in 0..self.env.resource_lib().size().max(0) as usize {
                    let idx = self.env_global_slot(id, "ZeroResources")?;
                    self.with_pop_resources(|rc, rng, env| rc.set(rng, env, idx, 0.0));
                }
                Ok(())
            }
            EnvAction::CellResource {
                cell_list,
                res_name,
                res_count,
                inject,
            } => self.env_cell_resource(cell_list, res_name, *res_count, *inject),
            EnvAction::ChangeEnvironment { env_string } => {
                self.env_load_line(env_string);
                self.env_update_resource_count()
            }
            EnvAction::SetGradientResource {
                env_string,
                res_name,
            } => {
                self.env_load_line(env_string);
                self.env_update_gradient_count(res_name)
            }
            EnvAction::GradientFlow {
                flow,
                res_name,
                value,
            } => self.env_update_gradient_flow(*flow, res_name, *value),
            EnvAction::SetGradPlatVarInflow {
                res_name,
                mean,
                variance,
                kind,
            } => {
                for (id, global) in self.env_global_indices(res_name) {
                    let g = self.env_global_index(id, global, "SetGradPlatVarInflow")?;
                    self.pop.resource_count.set_grad_plat_var_inflow(
                        &mut self.rng,
                        g,
                        *mean,
                        *variance,
                        *kind,
                    );
                }
                Ok(())
            }
            EnvAction::SetPredatoryResource {
                res_name,
                odds,
                juvs_per,
                detection_prob,
            } => {
                for (id, global) in self.env_global_indices(res_name) {
                    let g = self.env_global_index(id, global, "SetPredatoryResource")?;
                    let res = self.env.resource_lib_mut().resource_mut(id);
                    res.set_predatory_resource(*odds, *juvs_per, *detection_prob);
                    res.habitat = 5;
                    self.env.add_habitat(5);
                    self.pop
                        .resource_count
                        .set_predatory_resource(g, *odds, *juvs_per);
                }
                Ok(())
            }
            EnvAction::SetProbabilisticResource {
                res_name,
                initial,
                inflow,
                outflow,
                lambda,
                theta,
                x,
                y,
                count,
            } => {
                let res_name = match res_name {
                    Some(n) => n.clone(),
                    None => {
                        if self.env.resource_lib().size() == 0 {
                            return Err("SetProbabilisticResource: no resources (C++: NULL \
                                 dereference of GetResource(0))"
                                .to_string());
                        }
                        self.env.resource_lib().resource(0).name().to_string()
                    }
                };
                // Only the first resource of that name (`break`).
                if let Some(&(id, global)) = self.env_global_indices(&res_name).first() {
                    let g = self.env_global_index(id, global, "SetProbabilisticResource")?;
                    self.pop.resource_count.set_probabilistic_resource(
                        &mut self.rng,
                        g,
                        *initial,
                        *inflow,
                        *outflow,
                        *lambda,
                        *theta,
                        *x,
                        *y,
                        *count,
                    );
                }
                Ok(())
            }
            EnvAction::UpdateInflow { res_name, change } => {
                self.env_update_inflow(res_name, *change);
                Ok(())
            }
            EnvAction::SetReactionValue { name, value } => {
                self.env.set_reaction_value(&mut self.rng, name, *value);
                Ok(())
            }
            EnvAction::SetReactionValueMult { name, value } => {
                self.env.set_reaction_value_mult(name, *value);
                Ok(())
            }
            EnvAction::SetReactionInst { name, inst } => {
                self.env.set_reaction_inst(name, inst);
                Ok(())
            }
            EnvAction::SetReactionTaskCount { name, count, max } => {
                if *max {
                    self.env.set_reaction_max_task_count(name, *count);
                } else {
                    self.env.set_reaction_min_task_count(name, *count);
                }
                Ok(())
            }
            EnvAction::SetReactionTask { name, task } => {
                if self.env.set_reaction_task(name, task) {
                    Ok(())
                } else {
                    self.env_abort("SetReactionTask action failed", 50)
                }
            }
            EnvAction::SetResourceInflow { name, inflow } => {
                self.env_set_resource_inflow(name, *inflow);
                Ok(())
            }
            EnvAction::SetPeriodicResourceInflow {
                instance,
                name,
                period,
            } => {
                let update = self.stats.update();
                let (start, first_inflow) = match self.actions2.env.periodic_inflow.get(instance) {
                    Some(&s) => s,
                    None => {
                        // QUIRK: an unknown resource leaves `m_1st_inflow`
                        // uninitialised in C++; it reads as 0 here.
                        let first = self
                            .env
                            .resource_lib()
                            .get_resource_id(name)
                            .map_or(0.0, |id| self.env.resource_lib().resource(id).inflow);
                        self.actions2
                            .env
                            .periodic_inflow
                            .insert(*instance, (update, first));
                        (update, first)
                    }
                };
                let step = f64::from(update - start);
                let pi = std::f64::consts::PI;
                let inflow =
                    first_inflow * ((pi + step * 2.0 * pi / f64::from(*period)).cos() + 1.0) / 2.0;
                self.env_set_resource_inflow(name, inflow);
                Ok(())
            }
            EnvAction::SetResourceOutflow { name, outflow } => {
                // `cPopulation::SetResourceOutflow`
                self.env.set_resource_outflow(name, *outflow);
                report(self.pop.resource_count.set_decay(name, 1.0 - outflow));
                Ok(())
            }
            EnvAction::SetDemeResourceFlow {
                deme_id,
                name,
                value,
                outflow,
            } => {
                let Some(d) = usize::try_from(*deme_id)
                    .ok()
                    .filter(|&d| d < self.pop.demes.len())
                else {
                    return Err(format!(
                        "SetDemeResource{}flow: deme {deme_id} out of range (C++: \
                         out-of-bounds GetDeme)",
                        if *outflow { "Out" } else { "In" }
                    ));
                };
                // `cPopulation::SetSingleDemeResourceInflow` / `...Outflow`
                if *outflow {
                    self.env.set_resource_outflow(name, *value);
                    report(
                        self.pop.demes[d]
                            .resource_count
                            .set_decay(name, 1.0 - value),
                    );
                } else {
                    self.env.set_resource_inflow(name, *value);
                    report(self.pop.demes[d].resource_count.set_inflow(name, *value));
                }
                Ok(())
            }
            EnvAction::SetEnvironmentRandomMask { mask } => {
                self.env.set_specific_random_mask(*mask);
                self.env_reset_inputs();
                Ok(())
            }
            EnvAction::SetSeasonalResource {
                res_name,
                curve,
                scale,
            } => {
                let time = f64::from(self.stats.update());
                let res_count = match curve {
                    Seasonal::Base => {
                        // FMA: both inner `0.4*tanh(..)+0.5`, `0.5*sin(..)+0.5`
                        // and the outer `-1*a*b+1` are fused.
                        let a = 0.4f64.mul_add(((time - 182500.0) / 50000.0).tanh(), 0.5);
                        let b = 0.5f64.mul_add((time / 58.091).sin(), 0.5);
                        (-a).mul_add(b, 1.0)
                    }
                    Seasonal::OneK | Seasonal::TenK => {
                        let (center, width) = if *curve == Seasonal::OneK {
                            (182500.0, 50000.0)
                        } else {
                            (1825000.0, 500000.0)
                        };
                        let a = ((time - center) / width).tanh() + 1.0;
                        let b = 0.5f64.mul_add((time / 58.091).sin(), 0.5);
                        let c = (-scale * a).mul_add(b, 1.0);
                        c.max(0.0)
                    }
                };
                self.env_set_resource_by_id(res_name, res_count)
            }
            EnvAction::SetPeriodicResource {
                res_name,
                amplitude,
                frequency,
                phase_shift,
                init_y,
            } => {
                let time = f64::from(self.stats.update());
                // FMA: `3.14159/frequency * time - phaseShift * 3.14159`
                // fuses the left product, `amplitude * sin(..) + initY` too.
                // The C++ literal 3.14159, not pi.
                #[allow(clippy::approx_constant)]
                let x = (3.14159 / frequency).mul_add(time, -(phase_shift * 3.14159));
                let res_count = amplitude.mul_add(x.sin(), *init_y) / 2.0;
                self.env_set_resource_by_id(res_name, res_count)
            }
            EnvAction::SetTaskArg { task, arg, value } => self.env_set_task_arg(*task, *arg, value),
            EnvAction::SetOptimizeMinMax => self.env_set_optimize_min_max(),
            EnvAction::SetDemeIOGrid {
                input_output,
                cell_list,
            } => {
                let num_demes = self.pop.demes.len() as i32;
                let deme_size = self.cfg.world_x * (self.cfg.world_y / num_demes.max(1));
                let input = match input_output.as_str() {
                    "Input" => true,
                    "Output" => false,
                    _ => return Ok(()),
                };
                for &c in cell_list {
                    for deme_id in 0..num_demes {
                        let cell_id = c + deme_id * deme_size;
                        let Some(cell) = usize::try_from(cell_id)
                            .ok()
                            .and_then(|i| self.pop.cells.get_mut(i))
                        else {
                            return Err(format!(
                                "SetDemeIOGrid: cell {cell_id} out of range (C++: \
                                 out-of-bounds GetCell)"
                            ));
                        };
                        if input {
                            cell.can_input = true;
                        } else {
                            cell.can_output = true;
                        }
                    }
                }
                Ok(())
            }
            EnvAction::SetFracDemeTreatable { fraction } => {
                for d in 0..self.pop.demes.len() {
                    let treatable = self.rng.p(*fraction);
                    self.pop.demes[d].treatable = treatable;
                }
                Ok(())
            }
            EnvAction::AlterMigrationConnection {
                from_deme,
                to_deme,
                alter_amount,
            } => {
                let n = self.pop.migration_matrix.num_rows();
                let (Some(from), Some(to)) = (
                    usize::try_from(*from_deme).ok().filter(|&f| f < n),
                    usize::try_from(*to_deme).ok().filter(|&t| t < n),
                ) else {
                    return Err(format!(
                        "AlterMigrationConnection: demes {from_deme} -> {to_deme} outside the \
                         loaded migration matrix (C++: out-of-bounds access)"
                    ));
                };
                // `assert(ret)`: the result only matters to debug builds.
                let _ = self
                    .pop
                    .migration_matrix
                    .alter_connection_weight(from, to, *alter_amount);
                Ok(())
            }
            EnvAction::SetConfig { cvar, value } => {
                // Settings the C++ reads afresh are read afresh here too;
                // values either side copies at start-up stay as they were.
                self.cfg.set(cvar, value);
                Ok(())
            }
            EnvAction::ExitAveLineageLabel { threshold, greater } => {
                let ave = self.stats.sum_lineage_label.average();
                if (*greater && ave > *threshold) || (!*greater && ave < *threshold) {
                    self.done = true;
                }
                Ok(())
            }
            EnvAction::ExitAveGeneration { tgt_gen } => {
                if self.cfg.num_demes > 1 {
                    // Using demes; generation might be different.
                    if self.cfg.demes_use_germline != 0
                        && self.stats.m_germline_generation.average() > *tgt_gen
                    {
                        self.done = true;
                    }
                } else if self.stats.sum_generation.average() > *tgt_gen {
                    self.done = true;
                }
                Ok(())
            }
            EnvAction::ExitElapsedTime { time, then } => {
                if now_seconds() - then >= *time {
                    self.done = true;
                }
                Ok(())
            }
            EnvAction::ExitDemeResources { deme_res } => {
                // `int res_amt = GetDeme(0).GetTotalResourceAmountConsumed();`
                let Some(deme) = self.pop.demes.first() else {
                    return Err("ExitDemeResources: no deme 0".to_string());
                };
                let res_amt = deme.total_res_consumed as i32;
                if res_amt >= *deme_res {
                    self.done = true;
                }
                Ok(())
            }
            EnvAction::Pause => {
                self.actions2.env.pause_requested = true;
                Ok(())
            }
            EnvAction::LoadGenotypeList { filename, parasite } => {
                self.env_load_genotype_list(filename, *parasite);
                Ok(())
            }
        }
    }

    /// Called once before the first update, after the event list is loaded
    /// (where the C++ constructors attach data recorders).
    pub(crate) fn env_note_recorders(&mut self) {}

    /// Called at the end of every update, after the organism statistics are
    /// gathered and before the status line.
    pub(crate) fn env_end_update(&mut self) {}

    /// `Driver().Feedback().Error(msg)` (printed to stdout) and
    /// `Driver().Abort(code)`.
    fn env_abort(&mut self, msg: &str, code: i32) -> Result<(), String> {
        self.action_state
            .stdout
            .push_str(&format!("error: {msg}\n"));
        self.action_state.exit_code = Some(code);
        Err(msg.to_string())
    }

    /// The population resource slot a resource *id* addresses when the C++
    /// passes `res->GetID()` to `cResourceCount` (equal to its index unless
    /// deme resources come first).
    fn env_global_slot(&self, id: usize, action: &str) -> Result<usize, String> {
        if id < self.pop.resource_count.size() {
            Ok(id)
        } else {
            Err(format!(
                "{action}: resource id {id} beyond the {} global resources (C++: out-of-bounds \
                 cResourceCount access)",
                self.pop.resource_count.size()
            ))
        }
    }

    /// `SumMerit().Average()` (1 when not positive) over `AVE_TIME_SLICE`.
    fn env_scaled_ave_merit(&self) -> f64 {
        let mut ave_merit = self.stats.sum_merit.average();
        if ave_merit <= 0.0 {
            ave_merit = 1.0; // make sure that we don't get NAN's or negative numbers
        }
        ave_merit / f64::from(self.cfg.ave_time_slice)
    }

    /// `cPopulation::SetResource(ctx, name, level)`: by index; nothing for
    /// an unknown name (after the resource library's error message).
    fn env_set_resource_by_name(&mut self, res_name: &str, level: f64) -> Result<(), String> {
        if let Some(id) = self.env.resource_lib().get_resource_id(res_name) {
            let idx = self.env.resource_lib().resource(id).index();
            let idx = usize::try_from(idx)
                .ok()
                .filter(|&i| i < self.pop.resource_count.size())
                .ok_or_else(|| {
                    format!(
                        "SetResource: resource '{res_name}' has no global slot (C++: \
                         out-of-bounds cResourceCount::Set)"
                    )
                })?;
            self.with_pop_resources(|rc, rng, env| rc.set(rng, env, idx, level));
        }
        Ok(())
    }

    /// `SetResource(ctx, res->GetID(), level)` after looking the resource up
    /// by name (the seasonal and periodic actions).
    fn env_set_resource_by_id(&mut self, res_name: &str, level: f64) -> Result<(), String> {
        if let Some(id) = self.env.resource_lib().get_resource_id(res_name) {
            let idx = self.env_global_slot(id, "SetResource")?;
            self.with_pop_resources(|rc, rng, env| rc.set(rng, env, idx, level));
        }
        Ok(())
    }

    /// `cActionSetCellResource::Process` / `cActionInjectCellResource::Process`
    fn env_cell_resource(
        &mut self,
        cell_list: &[i32],
        res_name: &str,
        res_count: f64,
        inject: bool,
    ) -> Result<(), String> {
        // The constructor dereferences the looked-up resource.
        let Some(id) = self.env.resource_lib().get_resource_id(res_name) else {
            return Err(format!(
                "{}CellResource: unknown resource '{res_name}' (C++: NULL dereference while \
                 the event file loads)",
                if inject { "Inject" } else { "Set" }
            ));
        };
        let num_cells = self.pop.cells.len() as i32;
        for &cell_id in cell_list {
            if cell_id < 0 || cell_id >= num_cells {
                return Err(format!(
                    "CellResource: cell {cell_id} out of range (C++: out-of-bounds cell access)"
                ));
            }
            let mut counts = self.with_pop_resources(|rc, rng, env| {
                rc.get_cell_resources(cell_id, rng, env).to_vec()
            });
            if let Some(c) = counts.get_mut(id) {
                if inject {
                    *c += res_count;
                } else {
                    *c = res_count;
                }
                self.pop.resource_count.set_cell_resources(cell_id, &counts);
            }
        }
        Ok(())
    }

    /// `cEnvironment::LoadLine(line, feedback)`, with the messages printed to
    /// `cerr` as the actions do.
    fn env_load_line(&mut self, line: &str) {
        let mut fb = Feedback::new();
        let _ = self.env.load_line(line, &mut fb, &self.cfg);
        for (kind, msg) in fb.messages() {
            match kind {
                FeedbackKind::Error => eprintln!("error: {msg}"),
                FeedbackKind::Warning => eprintln!("warning: {msg}"),
                FeedbackKind::Notification => eprintln!("{msg}"),
            }
        }
    }

    /// `cPopulation::UpdateResourceCount(verbosity, world)`: fresh deme
    /// resource counts, then every resource set up again from the
    /// environment.
    fn env_update_resource_count(&mut self) -> Result<(), String> {
        let resources = self.env.resource_lib().resources().to_vec();
        let num_deme_res = resources.iter().filter(|r| r.deme_resource).count();
        // The fresh `cResourceCount(num_deme_res)` the C++ hands each deme
        // has empty spatial grids that `Setup` then reads (`GetX()`,
        // `GetY()`, the cell list): a global geometry never uses them, a grid,
        // torus or partial deme resource reads the unsized grid.
        if let Some(r) = resources.iter().find(|r| {
            r.deme_resource && r.geometry != avida_environment::resource::geometry::GLOBAL
        }) {
            return Err(format!(
                "unsupported: ChangeEnvironment with the spatial deme resource '{}' (C++: \
                 cPopulation::UpdateResourceCount sets up unsized deme grids)",
                r.name()
            ));
        }
        for deme in &mut self.pop.demes {
            deme.resource_count = ResourceCount::new(num_deme_res);
        }
        let verbosity = self.cfg.verbosity;
        let mut global_res_index = 0;
        let mut deme_res_index = 0;
        for res in &resources {
            if !res.deme_resource {
                if global_res_index >= self.pop.resource_count.size() {
                    return Err(format!(
                        "unsupported: ChangeEnvironment adding resource '{}' (C++: \
                         out-of-bounds cResourceCount::Setup)",
                        res.name()
                    ));
                }
                let setup = resource_setup_from(res, verbosity);
                let lines = self.with_pop_resources(|rc, rng, env| {
                    rc.setup(global_res_index, &setup, rng, env)
                })?;
                for line in lines {
                    self.action_state.stdout.push_str(&line);
                    self.action_state.stdout.push('\n');
                }
                global_res_index += 1;
            } else {
                // `cDeme::SetupDemeRes` for every deme. QUIRK: it passes
                // `GetMaxX(), GetMaxY(), GetMinX(), GetMinY()` where
                // `cResourceCount::Setup` takes max_x, min_x, max_y, min_y
                // (only gradients read them). `energy_res_ids` is not
                // cleared, so each call adds the id again.
                let mut setup = resource_setup_from(res, verbosity);
                setup.gradient.min_x = res.max_y;
                setup.gradient.max_y = res.min_x;
                for deme in &mut self.pop.demes {
                    let lines = deme.resource_count.setup(
                        deme_res_index,
                        &setup,
                        &mut self.rng,
                        &mut self.res_env,
                    )?;
                    for line in lines {
                        self.action_state.stdout.push_str(&line);
                        self.action_state.stdout.push('\n');
                    }
                    if res.energy_resource {
                        deme.energy_res_ids.push(deme_res_index);
                    }
                }
                deme_res_index += 1;
            }
        }
        Ok(())
    }

    /// The `(environment id, global_res_index)` of every resource named
    /// `res_name`, where `global_res_index` is the counter the
    /// `cPopulation::UpdateGradient*` loops keep (the number of global
    /// resources up to and including this one, minus one).
    fn env_global_indices(&self, res_name: &str) -> Vec<(usize, i32)> {
        let mut out = Vec::new();
        let mut global_res_index = -1;
        for (i, res) in self.env.resource_lib().resources().iter().enumerate() {
            if !res.deme_resource {
                global_res_index += 1;
            }
            if res.name() == res_name {
                out.push((i, global_res_index));
            }
        }
        out
    }

    /// `global_res_index` as a slot, or the fatal error for the C++
    /// out-of-bounds access (a deme resource before any global one).
    fn env_global_index(&self, id: usize, global: i32, action: &str) -> Result<usize, String> {
        usize::try_from(global)
            .ok()
            .filter(|&g| g < self.pop.resource_count.size())
            .ok_or_else(|| {
                format!(
                    "{action}: resource {id} has no global slot (C++: cResourceCount index \
                     {global} out of bounds)"
                )
            })
    }

    /// `cPopulation::UpdateGradientCount(ctx, verbosity, world, res_name)`
    fn env_update_gradient_count(&mut self, res_name: &str) -> Result<(), String> {
        for (id, global) in self.env_global_indices(res_name) {
            let g = self.env_global_index(id, global, "SetGradientResource")?;
            let params = resource_setup_from(self.env.resource_lib().resource(id), 0).gradient;
            self.with_pop_resources(|rc, rng, env| rc.set_gradient_count(rng, env, g, &params));
        }
        Ok(())
    }

    /// `cPopulation::UpdateGradientPlatInflow` and its siblings: the value
    /// is stored in the environment resource and in the gradient count.
    fn env_update_gradient_flow(
        &mut self,
        flow: GradientFlow,
        res_name: &str,
        value: f64,
    ) -> Result<(), String> {
        for (id, global) in self.env_global_indices(res_name) {
            let g = self.env_global_index(id, global, "SetGradient*flow")?;
            let res = self.env.resource_lib_mut().resource_mut(id);
            let rc = &mut self.pop.resource_count;
            match flow {
                GradientFlow::PlatInflow => {
                    res.plateau_inflow = value;
                    rc.set_gradient_plat_inflow(g, value);
                }
                GradientFlow::PlatOutflow => {
                    res.plateau_outflow = value;
                    rc.set_gradient_plat_outflow(g, value);
                }
                GradientFlow::ConeInflow => {
                    res.cone_inflow = value;
                    rc.set_gradient_cone_inflow(g, value);
                }
                GradientFlow::ConeOutflow => {
                    res.cone_outflow = value;
                    rc.set_gradient_cone_outflow(g, value);
                }
                GradientFlow::Inflow => {
                    res.gradient_inflow = value;
                    rc.set_gradient_inflow(g, value);
                }
            }
        }
        Ok(())
    }

    /// `cPopulation::SetResourceInflow(name, inflow)`
    fn env_set_resource_inflow(&mut self, name: &str, inflow: f64) {
        self.env.set_resource_inflow(name, inflow);
        report(self.pop.resource_count.set_inflow(name, inflow));
    }

    /// `cPopulation::UpdateInflow(name, change)`: the population's inflow
    /// only (the environment keeps its value).
    fn env_update_inflow(&mut self, res_name: &str, change: f64) {
        let matches = self
            .env
            .resource_lib()
            .resources()
            .iter()
            .filter(|r| r.name() == res_name)
            .count();
        let rc = &mut self.pop.resource_count;
        for _ in 0..matches {
            let current = rc.get_inflow(res_name).unwrap_or_else(|e| {
                eprintln!("{e}");
                -1.0
            });
            report(rc.set_inflow(res_name, current + change));
        }
        // QUIRK: an unknown name reads as -1 here, so it also prints the
        // warning.
        let after = rc.get_inflow(res_name).unwrap_or_else(|e| {
            eprintln!("{e}");
            -1.0
        });
        if after < 0.0 {
            self.action_state
                .stdout
                .push_str("WARNING: update to inflow rate results in negative resource inflow!\n");
        }
    }

    /// `cPopulation::ResetInputs(ctx)`: new inputs for every cell (and a
    /// cleared input buffer for its organism), then for every deme.
    fn env_reset_inputs(&mut self) {
        for c in 0..self.pop.cells.len() {
            self.env
                .setup_inputs(&mut self.rng, &mut self.pop.cells[c].inputs, true);
            if let Some(org) = self.pop.orgs[c].as_mut() {
                // `GetOrganism()->ResetInput()`
                org.state.input_pointer = 0;
                org.state.input_buf.clear();
            }
        }
        // `cDeme::ResetInputs(ctx)` draws new deme inputs (only the deme I/O
        // instructions, which are not ported, read them); the draws still
        // advance the random number generator.
        for _ in 0..self.pop.demes.len() {
            let mut deme_inputs = Vec::new();
            self.env.setup_inputs(&mut self.rng, &mut deme_inputs, true);
        }
    }

    /// `cActionSetTaskArgInt/Double/String::Process`
    fn env_set_task_arg(
        &mut self,
        task: i32,
        arg: i32,
        value: &TaskArgValue,
    ) -> Result<(), String> {
        let num_tasks = self.env.num_tasks();
        if task < 0 || task >= num_tasks {
            let (kind, code) = match value {
                TaskArgValue::Int(_) => ("Int", 50),
                TaskArgValue::Double(_) => ("Double", 2),
                TaskArgValue::Str(_) => ("String", 2),
            };
            let msg = format!("Task specified in SetTaskArg{kind} action does not exist");
            return self.env_abort(&msg, code);
        }
        let Some(args) = self.env.tasklib_mut().0.task_args_mut(task as usize) else {
            return Err(format!(
                "SetTaskArg: task {task} has no arguments (C++: NULL cArgContainer dereference)"
            ));
        };
        match value {
            TaskArgValue::Int(v) => args.set_int(arg, *v),
            TaskArgValue::Double(v) => args.set_double(arg, *v),
            TaskArgValue::Str(v) => args.set_string(arg, v),
        }
        Ok(())
    }

    /// `cActionSetOptimizeMinMax::Process`: each task's arguments 1 and 2
    /// become the largest and smallest last task value in the population.
    fn env_set_optimize_min_max(&mut self) -> Result<(), String> {
        for j in 0..self.env.num_tasks().max(0) as usize {
            let mut max_fx = 0.0;
            let mut min_fx = 0.0;
            let mut first = true;
            for i in 0..self.pop.cells.len() {
                let Some(org) = self.pop.orgs[i].as_ref() else {
                    continue;
                };
                let val = org
                    .state
                    .phenotype
                    .last_task_value
                    .get(j)
                    .copied()
                    .unwrap_or(0.0);
                if first {
                    min_fx = val;
                    max_fx = val;
                    first = false;
                    continue;
                }
                if val < min_fx {
                    min_fx = val;
                }
                if val > max_fx {
                    max_fx = val;
                }
            }
            let Some(args) = self.env.tasklib_mut().0.task_args_mut(j) else {
                return Err(format!(
                    "SetOptimizeMinMax: task {j} has no arguments (C++: NULL cArgContainer \
                     dereference)"
                ));
            };
            args.set_double(1, max_fx);
            args.set_double(2, min_fx);
        }
        Ok(())
    }

    /// `cPopulation::LoadGenotypeList(filename, ctx, list)`: one sequence
    /// per line of the file (a file that cannot be opened is reported and
    /// leaves the list as it was).
    fn env_load_genotype_list(&mut self, filename: &str, parasite: bool) {
        let file = avida_config::init_file::InitFile::new(filename, &self.working_dir, None, None);
        if !file.was_opened() {
            for (kind, msg) in file.feedback().messages() {
                match kind {
                    FeedbackKind::Error => eprintln!("error: {msg}"),
                    FeedbackKind::Warning => eprintln!("warning: {msg}"),
                    FeedbackKind::Notification => eprintln!("{msg}"),
                }
            }
            return;
        }
        let list: Vec<String> = (0..file.num_lines())
            .map(|i| file.get_line(i as i32))
            .collect();
        if parasite {
            self.actions2.env.parasite_genotype_list = list;
        } else {
            self.actions2.env.host_genotype_list = list;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTX: ParseContext<'static> = ParseContext {
        default_inst_set: "heads_default",
    };

    fn p(name: &str, args: &str) -> EnvAction {
        let mut fb = Feedback::new();
        let a = parse(name, args, &CTX, &mut fb).unwrap().unwrap();
        assert_eq!(fb.num_errors(), 0, "{}", fb.render());
        a
    }

    #[test]
    fn resource_amount_actions() {
        assert_eq!(
            p("InjectResource", "Food 12.5"),
            EnvAction::InjectResource {
                res_name: "Food".into(),
                res_count: 12.5
            }
        );
        assert_eq!(
            p("SetResource", ""),
            EnvAction::SetResource {
                res_name: String::new(),
                res_count: 0.0
            }
        );
        assert_eq!(
            p("SetCellResource", "1,3..5 Food 2"),
            EnvAction::CellResource {
                cell_list: vec![1, 3, 4, 5],
                res_name: "Food".into(),
                res_count: 2.0,
                inject: false,
            }
        );
        assert_eq!(
            p("DecInflow", "Food 0.5"),
            EnvAction::UpdateInflow {
                res_name: "Food".into(),
                change: -0.5
            }
        );
    }

    #[test]
    fn environment_strings_keep_the_whole_argument() {
        assert_eq!(
            p("ChangeEnvironment", "RESOURCE Food:inflow=5"),
            EnvAction::ChangeEnvironment {
                env_string: "RESOURCE Food:inflow=5".into()
            }
        );
        assert_eq!(
            p(
                "SetGradientResource",
                "GRADIENT_RESOURCE food:height=4:spread=3"
            ),
            EnvAction::SetGradientResource {
                env_string: "GRADIENT_RESOURCE food:height=4:spread=3".into(),
                res_name: "food".into()
            }
        );
        assert_eq!(
            p("SetTaskArgString", "1 0 a b c"),
            EnvAction::SetTaskArg {
                task: 1,
                arg: 0,
                value: TaskArgValue::Str("a b c".into())
            }
        );
        assert_eq!(
            p("SetDemeIOGrid", "Input:0,2"),
            EnvAction::SetDemeIOGrid {
                input_output: "Input".into(),
                cell_list: vec![0, 2]
            }
        );
    }

    #[test]
    fn schema_and_defaults() {
        assert_eq!(
            p("SetProbabilisticResource", "initial=3:num=2"),
            EnvAction::SetProbabilisticResource {
                res_name: None,
                initial: 3.0,
                inflow: 0.0,
                outflow: 0.0,
                lambda: 1.0,
                theta: 0.0,
                x: -1,
                y: -1,
                count: 2,
            }
        );
        assert_eq!(
            p("SetSeasonalResource1Kyears_1To_1", "Food"),
            EnvAction::SetSeasonalResource {
                res_name: "Food".into(),
                curve: Seasonal::OneK,
                scale: 1.0
            }
        );
        assert_eq!(
            p("SetEnvironmentRandomMask", "-1"),
            EnvAction::SetEnvironmentRandomMask { mask: u32::MAX }
        );
        // Each periodic inflow action has its own state.
        let (a, b) = (
            p("SetPeriodicResourceInflow", "Food 10"),
            p("SetPeriodicResourceInflow", "Food 10"),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn driver_actions() {
        assert_eq!(
            p("ExitAveLineageLabelLess", "0.5"),
            EnvAction::ExitAveLineageLabel {
                threshold: 0.5,
                greater: false
            }
        );
        assert_eq!(
            p("ExitAveGeneration", "3"),
            EnvAction::ExitAveGeneration { tgt_gen: 3.0 }
        );
        assert!(matches!(
            p("ExitElapsedTime", "10"),
            EnvAction::ExitElapsedTime { time: 10, .. }
        ));
        assert_eq!(p("Pause", ""), EnvAction::Pause);
        let mut fb = Feedback::new();
        for name in ["ExitAveGeneration", "ExitElapsedTime", "ExitDemeResources"] {
            assert!(parse(name, "", &CTX, &mut fb).unwrap().is_err(), "{name}");
        }
        assert!(parse("SetConfig", "NO_SUCH_SETTING 1", &CTX, &mut fb)
            .unwrap()
            .is_err());
        assert_eq!(
            p("SetConfig", "COPY_MUT_PROB 0.01"),
            EnvAction::SetConfig {
                cvar: "COPY_MUT_PROB".into(),
                value: "0.01".into()
            }
        );
    }
}
