//! PopulationActions.cc actions on demes: deme injection and replication
//! variants, whole-deme copies, the `cAbstractCompeteDemes` fitness
//! functions, consensus and synchronisation competitions
//! (avida-core/source/actions/PopulationActions.cc; the population methods
//! from main/cPopulation.cc).
//!
//! The deme competitions on the energy model
//! (`CompeteDemesByTaskCountAndEfficiency`, `CompeteDemesByEnergyDistribution`,
//! `CompeteDemes_AttackKillAndEnergyConserve`) and `CompeteDemesByMerit` are
//! here too; `MixPopulation`, `NewTrial`, `TrackAllMessages`,
//! `DistributeData`, `DistributeDataEfficiently` and `IteratedConsensus`
//! are in [`crate::actions3`], as are the deme network actions
//! (`MeasureDemeNetworks`, `CompeteDemesByNetwork`,
//! `DistributeDataCheaply`), the `Pred_*` / deme predicate actions and
//! `DelayedDemeEvent*`.

use avida_config::feedback::Feedback;
use avida_config::string_util::{as_double, as_int, pop_word};
use avida_hardware::organism::{transmission, OrgSource};
use avida_phenotype::Merit;

use crate::actions::ParseContext;
use crate::world::World;

/// The registered action names this module parses.
pub const DEME2_ACTIONS: &[&str] = &[
    "InjectModuloDemes",
    "InjectDemesFromNest",
    "InjectDemesRandom",
    "ReplaceFromGermline",
    "DivideDemes",
    "ResetDemes",
    "CopyDeme",
    "DecayPoints",
    "Flash",
    "KillDemePercent",
    "UnitFitness",
    "DemeBalanceTwoTasks",
    "PhenotypeMatch",
    "CompeteDemesByTaskCount",
    "CountOpinions",
    "CountMultipleOpinions",
    "Synchronization",
    "Desynchronization",
    "CompeteDemesByTaskCountAndEfficiency",
    "CompeteDemesByEnergyDistribution",
    "CompeteDemes_AttackKillAndEnergyConserve",
    "CompeteDemesByMerit",
];

/// A `cAbstractCompeteDemes` fitness function.
#[derive(Debug, Clone, PartialEq)]
pub enum DemeFitness {
    /// `cActionUnitFitness`
    Unit,
    /// `cActionDemeBalanceTwoTasks`
    BalanceTwoTasks { min: i32, max: i32 },
    /// `cActionPhenotypeMatch`: the desired phenotypes read by the
    /// constructor.
    PhenotypeMatch { desired: Vec<String> },
    /// `cActionCompeteDemesByTaskCount`
    TaskCount { task_num: i32 },
    /// `cActionCountOpinions`
    CountOpinions { desired: i32, mult: i32, side: i32 },
    /// `cActionCountMultipleOpinions`: opinions 1..=count are rewarded.
    CountMultipleOpinions { count: i32 },
    /// `cActionSynchronization` (`desync` false) / `cActionDesynchronization`
    Synchronization { desync: bool },
    /// `cActionCompeteDemesByTaskCountAndEfficiency`
    TaskCountAndEfficiency {
        initial_deme_energy: f64,
        task_num: i32,
    },
    /// `cActionCompeteDemesByEnergyDistribution`
    EnergyDistribution,
    /// `cAbstractCompeteDemes_AttackKillAndEnergyConserve`
    AttackKillAndEnergyConserve,
    /// `cActionCompeteDemesByMerit`
    Merit,
}

/// A parsed action of this module.
#[derive(Debug, Clone, PartialEq)]
pub enum Deme2Action {
    /// `cActionInjectModuloDemes`
    InjectModuloDemes {
        filename: String,
        mod_num: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionInjectDemesFromNest`
    InjectDemesFromNest {
        num_orgs: i32,
        nest_cellid: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionInjectDemesRandom`
    InjectDemesRandom {
        num_orgs: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    },
    /// `cActionReplaceFromGermline`
    ReplaceFromGermline { p_kill: f64, update_cell_data: i32 },
    /// `cActionDivideDemes`
    DivideDemes,
    /// `cActionResetDemes`
    ResetDemes,
    /// `cActionCopyDeme`
    CopyDeme { id1: i32, id2: i32 },
    /// `cActionDecayPoints`
    DecayPoints { decay_percent: i32 },
    /// `cActionFlash`
    Flash,
    /// `cActionKillDemePercent`
    KillDemePercent { pct_kills: f64 },
    /// A `cAbstractCompeteDemes` action.
    Compete(DemeFitness),
}

/// State the actions of this module keep between calls.
#[derive(Debug, Default)]
pub struct Deme2State {}

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
    fn double_or(&mut self, default: f64) -> f64 {
        self.word().map_or(default, |w| as_double(&w))
    }
}

/// `cActionPhenotypeMatch`'s constructor: `while (!ifs.eof()) { ifs >> p;
/// push_back(p); }`. QUIRK: when the file ends in white space the last
/// read fails and leaves `p` as it was, so the last word (or, for an empty
/// file, an empty string) is pushed twice. A file that cannot be opened
/// never reaches end-of-file (the C++ loops forever).
fn read_desired_phenotypes(path: &str) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| format!("PhenotypeMatch: cannot open '{path}' (C++: reads it forever)"))?;
    let mut out: Vec<String> = text.split_ascii_whitespace().map(str::to_string).collect();
    if text.is_empty() || text.ends_with(|c: char| c.is_ascii_whitespace()) {
        out.push(out.last().cloned().unwrap_or_default());
    }
    Ok(out)
}

/// The constructor of the action `name` (a [`DEME2_ACTIONS`] spelling) applied to
/// `args`; `None` for any other name.
pub fn parse(
    name: &str,
    args: &str,
    ctx: &ParseContext<'_>,
    feedback: &mut Feedback,
) -> Option<Result<Deme2Action, String>> {
    let _ = (ctx, feedback);
    let mut a = Args(args.to_string());
    let action = match name {
        "InjectModuloDemes" => Deme2Action::InjectModuloDemes {
            filename: a.word().unwrap_or_default(),
            mod_num: a.int_or(1),
            merit: a.double_or(-1.0),
            lineage_label: a.int_or(0),
            neutral_metric: a.double_or(0.0),
        },
        "InjectDemesFromNest" => Deme2Action::InjectDemesFromNest {
            num_orgs: a.int_or(1),
            nest_cellid: a.int_or(0),
            merit: a.double_or(-1.0),
            lineage_label: a.int_or(0),
            neutral_metric: a.double_or(0.0),
        },
        "InjectDemesRandom" => Deme2Action::InjectDemesRandom {
            num_orgs: a.int_or(1),
            merit: a.double_or(-1.0),
            lineage_label: a.int_or(0),
            neutral_metric: a.double_or(0.0),
        },
        "ReplaceFromGermline" => {
            let p_kill = a.double_or(0.0);
            Deme2Action::ReplaceFromGermline {
                p_kill,
                update_cell_data: a.int_or(0),
            }
        }
        "DivideDemes" => Deme2Action::DivideDemes,
        "ResetDemes" => Deme2Action::ResetDemes,
        "CopyDeme" => Deme2Action::CopyDeme {
            id1: a.int_or(0),
            id2: a.int_or(1),
        },
        // `m_decay_percent = largs.PopWord().AsInt();` (0 without a word)
        "DecayPoints" => Deme2Action::DecayPoints {
            decay_percent: as_int(&pop_word(&mut a.0)),
        },
        "Flash" => Deme2Action::Flash,
        "KillDemePercent" => Deme2Action::KillDemePercent {
            pct_kills: a.double_or(0.0),
        },
        "UnitFitness" => Deme2Action::Compete(DemeFitness::Unit),
        "DemeBalanceTwoTasks" => {
            // `_min = PopWord().AsInt(); _max = PopWord().AsInt();`
            let min = as_int(&pop_word(&mut a.0));
            let max = as_int(&pop_word(&mut a.0));
            Deme2Action::Compete(DemeFitness::BalanceTwoTasks { min, max })
        }
        "PhenotypeMatch" => {
            let desired = if args.is_empty() {
                Vec::new()
            } else {
                match read_desired_phenotypes(&pop_word(&mut a.0)) {
                    Ok(d) => d,
                    Err(e) => return Some(Err(e)),
                }
            };
            Deme2Action::Compete(DemeFitness::PhenotypeMatch { desired })
        }
        "CompeteDemesByTaskCount" => Deme2Action::Compete(DemeFitness::TaskCount {
            task_num: a.int_or(0),
        }),
        "CountOpinions" => {
            let desired = a.int_or(0);
            let mult = a.int_or(1);
            let side = a.int_or(0);
            Deme2Action::Compete(DemeFitness::CountOpinions {
                desired,
                mult,
                side,
            })
        }
        "CountMultipleOpinions" => {
            Deme2Action::Compete(DemeFitness::CountMultipleOpinions { count: a.int_or(0) })
        }
        "Synchronization" | "Desynchronization" => {
            Deme2Action::Compete(DemeFitness::Synchronization {
                desync: name == "Desynchronization",
            })
        }
        "CompeteDemesByTaskCountAndEfficiency" => {
            if args.is_empty() {
                // `cerr << ... << endl; exit(1);` in the constructor.
                return Some(Err(
                    "unsupported: CompeteDemesByTaskCountAndEfficiency without an \
                     initial deme energy (C++: exit(1) while the event file loads)"
                        .to_string(),
                ));
            }
            let initial_deme_energy = as_double(&pop_word(&mut a.0));
            // QUIRK: `if (largs.GetSize() > 1)` tests the length of what is
            // left, so a one-character task number is ignored.
            let task_num = if a.0.len() > 1 {
                as_int(&pop_word(&mut a.0))
            } else {
                0
            };
            Deme2Action::Compete(DemeFitness::TaskCountAndEfficiency {
                initial_deme_energy,
                task_num,
            })
        }
        "CompeteDemesByEnergyDistribution" => Deme2Action::Compete(DemeFitness::EnergyDistribution),
        "CompeteDemes_AttackKillAndEnergyConserve" => {
            Deme2Action::Compete(DemeFitness::AttackKillAndEnergyConserve)
        }
        "CompeteDemesByMerit" => Deme2Action::Compete(DemeFitness::Merit),
        _ => return None,
    };
    Some(Ok(action))
}

/// `Systematics::Source(DUPLICATION, "germline", true)`
fn germline_source() -> OrgSource {
    OrgSource::new(transmission::DUPLICATION, "germline", true)
}

/// `Systematics::Source(DUPLICATION, "")`
fn duplication_source() -> OrgSource {
    OrgSource::new(transmission::DUPLICATION, "", false)
}

impl World {
    /// `Process(ctx)` of an action of this module.
    pub(crate) fn run_deme2_action(&mut self, action: &Deme2Action) -> Result<(), String> {
        match action {
            Deme2Action::InjectModuloDemes {
                filename,
                mod_num,
                merit,
                lineage_label,
                neutral_metric,
            } => self.d2_inject_modulo(filename, *mod_num, *merit, *lineage_label, *neutral_metric),
            Deme2Action::InjectDemesFromNest {
                num_orgs,
                nest_cellid,
                merit,
                lineage_label,
                neutral_metric,
            } => self.d2_inject_germline(
                *num_orgs,
                Some(*nest_cellid),
                *merit,
                *lineage_label,
                *neutral_metric,
            ),
            Deme2Action::InjectDemesRandom {
                num_orgs,
                merit,
                lineage_label,
                neutral_metric,
            } => self.d2_inject_germline(*num_orgs, None, *merit, *lineage_label, *neutral_metric),
            Deme2Action::ReplaceFromGermline {
                p_kill,
                update_cell_data,
            } => self.d2_replace_from_germline(*p_kill, *update_cell_data),
            Deme2Action::DivideDemes => self.d2_divide_demes(),
            Deme2Action::ResetDemes => self.d2_reset_demes(),
            Deme2Action::CopyDeme { id1, id2 } => self.d2_copy_deme(*id1, *id2),
            Deme2Action::DecayPoints { decay_percent } => {
                let decay = f64::from(*decay_percent) / 100.0;
                for deme in &mut self.pop.demes {
                    let sub_points = (deme.points * decay) as i32;
                    // QUIRK: `SubtractNumberOfPoints(n)` only zeroes the
                    // points when `n` exceeds them; it never subtracts.
                    if f64::from(sub_points) > deme.points {
                        deme.points = 0.0;
                    }
                }
                Ok(())
            }
            Deme2Action::Flash => {
                for d in 0..self.pop.demes.len() {
                    let deme = &self.pop.demes[d];
                    let cell = deme.cell_id(deme.size() / 2) as usize;
                    if let Some(org) = self.pop.orgs[cell].as_mut() {
                        org.receive_flash()?;
                    }
                }
                Ok(())
            }
            Deme2Action::KillDemePercent { pct_kills } => self.d2_kill_deme_percent(*pct_kills),
            Deme2Action::Compete(f) => {
                // `cAbstractCompeteDemes::Process`
                let mut fitness = Vec::with_capacity(self.pop.demes.len());
                for d in 0..self.pop.demes.len() {
                    fitness.push(self.d2_fitness(d, f)?);
                }
                self.compete_demes(&fitness)
            }
        }
    }

    /// Called once before the first update, after the event list is loaded
    /// (where the C++ constructors attach data recorders).
    pub(crate) fn deme2_note_recorders(&mut self) {}

    /// Called at the end of every update, after the organism statistics are
    /// gathered and before the status line.
    pub(crate) fn deme2_end_update(&mut self) {}

    /// `cActionInjectModuloDemes::Process`
    fn d2_inject_modulo(
        &mut self,
        filename: &str,
        mod_num: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    ) -> Result<(), String> {
        if filename.is_empty() {
            eprintln!("error: no organism file specified");
            return Ok(());
        }
        let Some(genome) = self.load_genome(filename, "", false)? else {
            return Ok(());
        };
        let energy = self.cfg.energy_enabled;
        let start = usize::from(energy);
        for i in start..self.pop.demes.len() {
            let pick = if mod_num == 0 {
                if energy || i != 0 {
                    return Err(
                        "InjectModuloDemes: modulo 0 (C++: integer division by zero)".to_string(),
                    );
                }
                true
            } else if energy {
                i as i32 % mod_num == 0
            } else {
                i == 0 || i as i32 % mod_num == 0
            };
            if pick {
                let cell = self.pop.demes[i].cell_id(0);
                self.inject(
                    &genome,
                    crate::run::injected_source(),
                    cell,
                    merit,
                    lineage_label,
                    neutral_metric,
                )?;
                self.pop.demes[i].injected_count += 1;
            }
        }
        Ok(())
    }

    /// `cActionInjectDemesFromNest::Process` (`nest` given) /
    /// `cActionInjectDemesRandom::Process`: the latest germline genome of
    /// each deme that has had fewer than `num_orgs` injections. QUIRK: the
    /// random cell is `GetInt(0, deme_size - 1)`, which never picks the
    /// deme's last cell (and the C++ loops forever when every other cell is
    /// occupied).
    fn d2_inject_germline(
        &mut self,
        num_orgs: i32,
        nest: Option<i32>,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
    ) -> Result<(), String> {
        let energy = self.cfg.energy_enabled;
        for i in 0..self.pop.demes.len() {
            // `avd_popaction_seed_deme_action`: with energy, a deme without
            // injections only counts one.
            if energy && self.pop.demes[i].injected_count == 0 {
                self.pop.demes[i].injected_count += 1;
                continue;
            }
            if self.pop.demes[i].injected_count >= num_orgs {
                continue;
            }
            let target_cell = match nest {
                Some(rel) => {
                    let deme = &self.pop.demes[i];
                    if rel < 0 || rel >= deme.size() {
                        return Err(format!(
                            "InjectDemesFromNest: nest cell {rel} outside the deme \
                             (C++: out-of-range read)"
                        ));
                    }
                    deme.cell_id(rel)
                }
                None => {
                    let deme_size = self.pop.demes[i].size();
                    let any_free = (0..deme_size - 1).any(|r| {
                        let c = self.pop.demes[i].cell_id(r) as usize;
                        self.pop.orgs[c].is_none()
                    });
                    if !any_free {
                        return Err("InjectDemesRandom: no free cell it can pick \
                                    (C++: loops forever)"
                            .to_string());
                    }
                    loop {
                        let r = self.rng.get_int_range(0, deme_size - 1);
                        let c = self.pop.demes[i].cell_id(r);
                        if self.pop.orgs[c as usize].is_none() {
                            break c;
                        }
                    }
                }
            };
            let genome = self.pop.demes[i]
                .germline
                .last()
                .cloned()
                .ok_or("InjectDemes: empty germline (C++: GetLatest of an empty germline)")?;
            self.inject(
                &genome,
                germline_source(),
                target_cell,
                merit,
                lineage_label,
                neutral_metric,
            )?;
            self.pop.demes[i].injected_count += 1;
        }
        Ok(())
    }

    /// `cActionReplaceFromGermline::Process`
    fn d2_replace_from_germline(
        &mut self,
        p_kill: f64,
        update_cell_data: i32,
    ) -> Result<(), String> {
        if p_kill <= 0.0 {
            return Ok(());
        }
        let update = self.stats.update();
        for d in 0..self.pop.demes.len() {
            if !self.rng.p(p_kill) {
                continue;
            }
            let size = self.pop.demes[d].size();
            let pos = self.rng.get_int(size);
            let cell = self.pop.demes[d].cell_id(pos);
            if update_cell_data != 0 {
                let data = self.rng.get_int(i32::MAX);
                self.d2_replace_cell_data(d, cell, data, update);
            }
            let genome = self.pop.demes[d].germline.last().cloned().ok_or(
                "ReplaceFromGermline: empty germline (C++: GetLatest of an empty germline)",
            )?;
            self.kill_organism(cell, None);
            self.inject_genome(cell, germline_source(), &genome, 0, true)?;
            self.deme_post_injection(d, cell);
        }
        Ok(())
    }

    /// `cAssignRandomCellData::ReplaceCellData(cell, new_data, deme)`
    fn d2_replace_cell_data(&mut self, d: usize, cell: i32, new_data: i32, update: i32) {
        let old = self.pop.cells[cell as usize].cell_data.contents;
        let deme = &self.pop.demes[d];
        let holders = (0..deme.size())
            .filter(|&i| self.pop.cells[deme.cell_id(i) as usize].cell_data.contents == old)
            .count();
        let deme_id = deme.id;
        // `deme_to_id.find(deme.GetID())`: the deme's entry is assumed to
        // exist (AssignRandomCellData made it).
        if let Some(set) = self.deme_compete.deme_to_id.get_mut(&deme_id) {
            if holders == 1 {
                set.remove(&old);
            }
        }
        self.set_cell_data(cell, new_data, -1, update);
        if let Some(set) = self.deme_compete.deme_to_id.get_mut(&deme_id) {
            set.insert(new_data);
        }
    }

    /// `cPopulation::DivideDemes(ctx)`
    fn d2_divide_demes(&mut self) -> Result<(), String> {
        let num_demes = self.pop.demes.len();
        for deme_id in 0..num_demes {
            if !self.pop.demes[deme_id].is_full() {
                continue;
            }
            if num_demes < 2 {
                return Err("DivideDemes: a single deme (C++: loops forever)".to_string());
            }
            let mut target_id = deme_id;
            while target_id == deme_id {
                target_id = self.rng.get_uint(num_demes as u32) as usize;
            }
            let deme_size = self.pop.demes[target_id].size();
            for i in 0..deme_size {
                let c = self.pop.demes[target_id].cell_id(i);
                self.kill_organism(c, None);
            }
            let first = self.pop.demes[deme_id].cell_id(0) as usize;
            let num_tasks = self.pop.orgs[first]
                .as_ref()
                .ok_or("DivideDemes: empty first cell (C++: NULL dereference)")?
                .state
                .phenotype
                .last_task_count
                .len();
            let mut tot_tasks = vec![0i32; num_tasks];
            let mut pos = 0;
            while pos < deme_size {
                if pos + 1 >= self.pop.demes[deme_id].size() {
                    return Err("DivideDemes: odd deme size (C++: out-of-range read)".to_string());
                }
                let cell1 = self.pop.demes[deme_id].cell_id(pos + 1);
                let cell2 = self.pop.demes[target_id].cell_id(pos);
                {
                    let org1 = self.pop.orgs[cell1 as usize]
                        .as_ref()
                        .ok_or("DivideDemes: empty odd cell (C++: NULL dereference)")?;
                    for (t, &c) in tot_tasks
                        .iter_mut()
                        .zip(org1.state.phenotype.last_task_count.iter())
                    {
                        *t += c;
                    }
                }
                self.inject_clone(cell2, cell1, duplication_source())?;
                self.kill_organism(cell1, None);
                pos += 2;
            }
            let mut merit = 100i32;
            for &t in &tot_tasks {
                if t > 0 {
                    merit = merit.wrapping_mul(2);
                }
            }
            let mut pos = 0;
            while pos < deme_size {
                for c in [
                    self.pop.demes[deme_id].cell_id(pos),
                    self.pop.demes[target_id].cell_id(pos),
                ] {
                    if self.pop.orgs[c as usize].is_none() {
                        return Err(
                            "DivideDemes: empty cell after the division (C++: NULL dereference)"
                                .to_string(),
                        );
                    }
                    self.update_merit_of(c, None, f64::from(merit));
                }
                pos += 2;
            }
        }
        Ok(())
    }

    /// `cPopulation::ResetDemes()`: every organism is cloned into its own
    /// cell with its own source.
    fn d2_reset_demes(&mut self) -> Result<(), String> {
        for d in 0..self.pop.demes.len() {
            for i in 0..self.pop.demes[d].size() {
                let c = self.pop.demes[d].cell_id(i);
                let Some(org) = self.pop.orgs[c as usize].as_ref() else {
                    continue;
                };
                let src = org.state.src.clone();
                self.inject_clone(c, c, src)?;
            }
        }
        Ok(())
    }

    /// `cPopulation::CopyDeme(deme1_id, deme2_id, ctx)`
    fn d2_copy_deme(&mut self, id1: i32, id2: i32) -> Result<(), String> {
        let n = self.pop.demes.len() as i32;
        if id1 < 0 || id1 >= n || id2 < 0 || id2 >= n {
            return Err(format!(
                "CopyDeme: deme {id1} or {id2} does not exist (C++: out-of-range read)"
            ));
        }
        let (d1, d2) = (id1 as usize, id2 as usize);
        for i in 0..self.pop.demes[d1].size() {
            let from = self.pop.demes[d1].cell_id(i);
            if i >= self.pop.demes[d2].size() {
                return Err(
                    "CopyDeme: the target deme is smaller (C++: out-of-range read)".to_string(),
                );
            }
            let to = self.pop.demes[d2].cell_id(i);
            if self.pop.orgs[from as usize].is_none() {
                self.kill_organism(to, None);
                continue;
            }
            self.inject_clone(to, from, duplication_source())?;
        }
        Ok(())
    }

    /// `cActionKillDemePercent::Process`
    fn d2_kill_deme_percent(&mut self, pct_kills: f64) -> Result<(), String> {
        let mut cells_scanned: i64 = 0;
        let mut orgs_killed: i64 = 0;
        let mut cells_empty: i64 = 0;
        for d in 0..self.pop.demes.len() {
            let deme = &self.pop.demes[d];
            // `IsTreatableNow()`: treatable, and the current age is one of
            // the (non-empty) treatment ages.
            let treatable = deme.treatable
                && !deme.treatment_ages.is_empty()
                && deme.treatment_ages.contains(&deme.age);
            if !treatable {
                continue;
            }
            let n = deme.width * deme.height();
            for c in 0..n {
                cells_scanned += 1;
                let target_cell = self.pop.demes[d].cell_id(c);
                if self.rng.p(pct_kills) {
                    if self.pop.orgs[target_cell as usize].is_some() {
                        self.kill_organism(target_cell, None);
                        orgs_killed += 1;
                    } else {
                        cells_empty += 1;
                    }
                }
            }
        }
        self.stats.add_num_cells_scanned_at_kill(cells_scanned);
        self.stats.add_num_orgs_killed(orgs_killed);
        self.stats
            .add_num_unoccupied_cell_attempted_to_kill(cells_empty);
        Ok(())
    }

    /// `Fitness(deme, ctx)` of a `cAbstractCompeteDemes` action.
    fn d2_fitness(&mut self, d: usize, f: &DemeFitness) -> Result<f64, String> {
        let size = self.pop.demes[d].size();
        let cells: Vec<usize> = self.pop.demes[d]
            .cell_ids
            .iter()
            .map(|&c| c as usize)
            .collect();
        // `org->HasOpinion() ? org->GetOpinion().first`, per cell.
        let opinions = |w: &mut World| -> Vec<Option<i32>> {
            cells
                .iter()
                .map(|&c| {
                    w.pop.orgs[c].as_mut().and_then(|o| {
                        if o.state.has_opinion() {
                            Some(o.state.get_opinion().0)
                        } else {
                            None
                        }
                    })
                })
                .collect()
        };
        Ok(match f {
            DemeFitness::Unit => 1.0,
            DemeFitness::BalanceTwoTasks { min, max } => {
                let mut t1 = 0.0;
                let mut t2 = 0.0;
                for &c in &cells {
                    if let Some(org) = self.pop.orgs[c].as_ref() {
                        let r = &org.state.phenotype.cur_reaction_count;
                        if r.len() < 2 {
                            return Err("DemeBalanceTwoTasks: fewer than two reactions \
                                        (C++: out-of-range read)"
                                .to_string());
                        }
                        if r[0] != 0 {
                            t1 += 1.0;
                        }
                        if r[1] != 0 {
                            t2 += 1.0;
                        }
                    }
                }
                // `float val = ...` returned as a double.
                let val: f32 = if f64::from(*min) <= t1 && t1 <= f64::from(*max) {
                    (2.0 + t2) as f32
                } else {
                    1.0
                };
                f64::from(val)
            }
            DemeFitness::PhenotypeMatch { desired } => {
                if desired.len() != size as usize {
                    return Ok(1.0);
                }
                // QUIRK: `atoi(&curp)` reads one `char` as a C string; here
                // it is the digit's value (0 for anything else), the read
                // past the character being undefined in C++.
                let digit = |p: &str, j: usize| -> i32 {
                    match p.as_bytes().get(j) {
                        Some(b) if b.is_ascii_digit() => i32::from(b - b'0'),
                        _ => 0,
                    }
                };
                let mut total_dist = 0.0;
                for (i, &c) in cells.iter().enumerate() {
                    let p = &desired[i];
                    if let Some(org) = self.pop.orgs[c].as_ref() {
                        for (j, &react) in org.state.phenotype.cur_reaction_count.iter().enumerate()
                        {
                            let des = digit(p, j);
                            total_dist += f64::from((des - react).abs());
                            if des > react {
                                total_dist += 1.0;
                            }
                        }
                    } else {
                        for j in 0..p.len() {
                            total_dist += f64::from(digit(p, j) + 1);
                        }
                    }
                }
                1.0 + 1.0 / total_dist
            }
            DemeFitness::TaskCount { task_num } => {
                let count = usize::try_from(*task_num)
                    .ok()
                    .and_then(|t| self.pop.demes[d].cur_task_exe_count.get(t))
                    .copied()
                    .ok_or("CompeteDemesByTaskCount: no such task (C++: out-of-range read)")?;
                let fitness = f64::from(count).powf(2.0);
                if fitness == 0.0 {
                    0.1
                } else {
                    fitness
                }
            }
            DemeFitness::CountOpinions {
                desired,
                mult,
                side,
            } => {
                let ops = opinions(self);
                let have_opinion = ops.iter().filter(|o| o.is_some()).count() as i32;
                let support = ops.iter().filter(|o| **o == Some(*desired)).count() as i32;
                if have_opinion < size {
                    return Ok(f64::from(have_opinion + 1));
                }
                // QUIRK: the support count is `unsigned int`, so the
                // comparisons and differences with the multiplicity are
                // unsigned (a negative difference wraps to about 4.3e9).
                let (sup, mul) = (support as u32, *mult as u32);
                let sz = f64::from(size);
                match side {
                    -1 if sup >= mul => (sz - f64::from(sup.wrapping_sub(mul)) + 1.0).powf(2.0),
                    0 => (sz - f64::from(mul.wrapping_sub(sup)) + 1.0).powf(2.0),
                    1 if sup <= mul => (sz - f64::from(mul.wrapping_sub(sup)) + 1.0).powf(2.0),
                    // Any other sidedness: `assert(false)` (compiled out in
                    // the release build), then the deme size.
                    _ => sz,
                }
            }
            DemeFitness::CountMultipleOpinions { count } => {
                let ops = opinions(self);
                let ocount = ops.iter().filter(|o| o.is_some()).count() as i32;
                if ocount < size {
                    return Ok(f64::from(ocount + 1));
                }
                let unique: std::collections::BTreeSet<i32> = ops.into_iter().flatten().collect();
                let rewarded = unique.iter().filter(|&&o| o >= 1 && o <= *count).count();
                rewarded as f64 + f64::from(ocount) + 1.0
            }
            DemeFitness::Synchronization { desync } => self.d2_sync_fitness(d, *desync)?,
            DemeFitness::TaskCountAndEfficiency {
                initial_deme_energy,
                task_num,
            } => {
                let energy_used = initial_deme_energy - self.deme_calculate_total_energy(d);
                let count = usize::try_from(*task_num)
                    .ok()
                    .and_then(|t| self.pop.demes[d].cur_task_exe_count.get(t))
                    .copied()
                    .ok_or(
                        "CompeteDemesByTaskCountAndEfficiency: no such task (C++: out-of-range read)",
                    )?;
                let fitness = (f64::from(count) * (initial_deme_energy / energy_used)).powf(2.0);
                if fitness == 0.0 {
                    0.1
                } else {
                    fitness
                }
            }
            DemeFitness::Merit => {
                // `deme.UpdateCurMerit()`: base merit times `m_cur_bonus`,
                // which only `cDeme::DoDemeOutput` moves away from
                // DEFAULT_BONUS. No cell is ever enabled for deme input or
                // output (`cPopulationCell::SetCanInput` / `SetCanOutput`
                // have no callers), so the bonus stays DEFAULT_BONUS and the
                // deme's input buffer stays empty: `HasDoneInput()` and
                // (QUIRK: it also tests the input buffer) `HasDoneOutput()`
                // are false.
                let merit_base = if self.cfg.base_merit_method == 0 {
                    f64::from(self.cfg.base_const_merit)
                } else {
                    1.0
                };
                let cur_merit = Merit::new(merit_base * self.cfg.default_bonus).get_double();
                let messaging_bonus = if self.pop.demes[d].msg_successfully_sent > 0 {
                    1.5
                } else {
                    1.0
                };
                let (input_bonus, output_bonus) = (1.0, 1.0);
                cur_merit * messaging_bonus * input_bonus * output_bonus
            }
            DemeFitness::EnergyDistribution => {
                let mut min_energy = -1.0;
                let mut max_energy = -1.0;
                for &c in &cells {
                    if let Some(org) = self.pop.orgs[c].as_ref() {
                        let current_energy = org.state.phenotype.energy_store;
                        if min_energy == -1.0 || current_energy < min_energy {
                            min_energy = current_energy;
                        }
                        if max_energy == -1.0 || current_energy > max_energy {
                            max_energy = current_energy;
                        }
                    }
                }
                if min_energy == -1.0 || max_energy == -1.0 {
                    0.0001
                } else if min_energy == max_energy {
                    100.0 * max_energy
                } else {
                    100.0 * (max_energy / (max_energy - min_energy).powf(2.0))
                }
            }
            DemeFitness::AttackKillAndEnergyConserve => {
                let events_killed = f64::from(self.pop.demes[d].events_killed);
                let total_events = f64::from(self.pop.demes[d].events_total);
                let energy_remaining = self.deme_calculate_total_energy(d);
                // QUIRK: `CalculateTotalInitialEnergyResources` sums the
                // initial values of the first `energy_res_ids.size()`
                // resources, not of the energy resources.
                let deme = &self.pop.demes[d];
                let mut initial_energy = 0.0;
                for i in 0..deme.energy_res_ids.len() {
                    if i >= deme.resource_count.size() {
                        return Err(
                            "CompeteDemes_AttackKillAndEnergyConserve: initial value of \
                                    a missing resource (C++: out-of-range read)"
                                .to_string(),
                        );
                    }
                    initial_energy += deme.resource_count.get_initial_resource_value(i);
                }
                ((events_killed / total_events) + (energy_remaining / initial_energy)) / 2.0
            }
        })
    }

    /// `cActionSynchronization::Fitness` / `cActionDesynchronization::Fitness`.
    /// QUIRK: the flash times are kept as update -> deme -> cells, but the
    /// fitness looks the deme id up as an update and then reads the deme
    /// ids as cells and the cells as flash updates.
    fn d2_sync_fitness(&self, d: usize, desync: bool) -> Result<f64, String> {
        let deme_id = self.pop.demes[d].id;
        let size = self.pop.demes[d].size();
        let Some(deme_flashes) = self.stats.m_flash_times.get(&deme_id) else {
            return Ok(1.0);
        };
        let update = self.stats.update();
        let window = self.cfg.sync_fitness_window;
        let mut window_flashes: std::collections::BTreeMap<i32, Vec<i32>> = Default::default();
        for (&k, js) in deme_flashes {
            for &j in js {
                if j > update - window {
                    window_flashes.entry(k).or_default().push(j);
                }
            }
        }
        if window_flashes.len() < size as usize {
            return Ok(1.0 + window_flashes.len() as f64);
        }
        let mut flash_count = vec![0u32; window.max(0) as usize];
        for js in window_flashes.values() {
            for &j in js {
                let slot = usize::try_from(update - j)
                    .ok()
                    .and_then(|i| flash_count.get_mut(i))
                    .ok_or("Synchronization: flash outside the window (C++: out-of-range write)")?;
                *slot += 1;
            }
        }
        let max = f64::from(
            *flash_count
                .iter()
                .max()
                .ok_or("Synchronization: empty window (C++: max_element of nothing)")?,
        );
        if desync {
            Ok(1.0 + 2.0 * f64::from(size) - max)
        } else {
            let mean =
                flash_count.iter().map(|&c| f64::from(c)).sum::<f64>() / flash_count.len() as f64;
            Ok(1.0 + f64::from(size) + max - mean)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTX: ParseContext<'static> = ParseContext {
        default_inst_set: "heads_default",
    };

    fn p(name: &str, args: &str) -> Deme2Action {
        let mut fb = Feedback::new();
        parse(name, args, &CTX, &mut fb).unwrap().unwrap()
    }

    #[test]
    fn every_listed_name_parses() {
        for n in DEME2_ACTIONS {
            if *n == "PhenotypeMatch" {
                continue;
            }
            assert!(parse(n, "", &CTX, &mut Feedback::new()).is_some(), "{n}");
            assert!(crate::action_library::is_registered(n), "{n}");
        }
    }

    #[test]
    fn defaults() {
        assert_eq!(p("CopyDeme", ""), Deme2Action::CopyDeme { id1: 0, id2: 1 });
        assert_eq!(
            p("CountOpinions", "3"),
            Deme2Action::Compete(DemeFitness::CountOpinions {
                desired: 3,
                mult: 1,
                side: 0
            })
        );
        assert_eq!(
            p("InjectModuloDemes", "x.org 3"),
            Deme2Action::InjectModuloDemes {
                filename: "x.org".into(),
                mod_num: 3,
                merit: -1.0,
                lineage_label: 0,
                neutral_metric: 0.0
            }
        );
        assert!(parse(
            "PhenotypeMatch",
            "/no/such/file",
            &CTX,
            &mut Feedback::new()
        )
        .unwrap()
        .is_err());
    }
}
