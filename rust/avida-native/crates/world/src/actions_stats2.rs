//! The `STATS_OUT_FILE` printers of PrintActions.cc that need `cStats` or population
//! state not gathered elsewhere, the `POP_OUT_FILE` printers and the deme
//! summary printers (avida-core/source/actions/PrintActions.cc,
//! main/cStats.cc and main/cPopulation.cc at 47f13dadb).
//!
//! `PrintDemeNetworkTopology` opens its file and then aborts the run in
//! `cDeme::GetNetwork()`, as the reference build does (see
//! [`crate::deme_events`]).

use std::collections::{BTreeMap, BTreeSet};

use avida_apto::Accumulator;
use avida_config::feedback::Feedback;
use avida_config::string_util::pop_word;
use avida_hardware::organism::{OrgState, Organism};
use avida_phenotype::Merit;

use crate::actions::ParseContext;
use crate::stat_tools::DoubleSum;
use crate::world::World;

/// The registered action names this module parses.
pub const STATS2_ACTIONS: &[&str] = &[
    "PrintPhenotypeData",
    "PrintParasitePhenotypeData",
    "PrintHostPhenotypeData",
    "PrintDemeTestamentStats",
    "PrintCurrentMeanDemeDensity",
    "PrintDemesMeritsData",
    "PrintNumOrgsInDeme",
    "SaveDemeFounders",
];

/// Which task counts `PrintPhenotypeData` and its host / parasite variants
/// read (`GetLastTaskCount`, `GetLastHostTaskCount`,
/// `GetLastParasiteTaskCount`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhenotypeKind {
    All,
    Host,
    Parasite,
}

/// A parsed action of this module.
#[derive(Debug, Clone, PartialEq)]
pub enum Stats2Action {
    /// `cActionPrintPhenotypeData` and the host / parasite variants
    /// (`POP_OUT_FILE`)
    PhenotypeData {
        filename: String,
        kind: PhenotypeKind,
    },
    /// `cActionPrintDemeTestamentStats` (`POP_OUT_FILE`)
    DemeTestamentStats { filename: String },
    /// `cActionPrintCurrentMeanDemeDensity` (`POP_OUT_FILE`)
    CurrentMeanDemeDensity { filename: String },
    /// `cActionPrintDemesMeritsData`
    DemesMeritsData,
    /// `cActionPrintNumOrgsInDeme`
    NumOrgsInDeme,
    /// `cActionSaveDemeFounders` (empty filename: `deme_founders-<update>.dat`)
    SaveDemeFounders { filename: String },
}

/// State the actions of this module keep between calls.
#[derive(Debug, Default)]
pub struct Stats2State {}

/// `if (largs == "") m_filename = DEFAULT; else m_filename = largs.PopWord();`
fn out_file(args: &str, default: &str) -> String {
    let mut largs = args.to_string();
    if largs.is_empty() {
        default.to_string()
    } else {
        pop_word(&mut largs)
    }
}

/// The constructor of the action `name` (a [`STATS2_ACTIONS`] spelling) applied to
/// `args`; `None` for any other name.
pub fn parse(
    name: &str,
    args: &str,
    ctx: &ParseContext<'_>,
    feedback: &mut Feedback,
) -> Option<Result<Stats2Action, String>> {
    let _ = (ctx, feedback);
    let phen = |default: &str, kind| Stats2Action::PhenotypeData {
        filename: out_file(args, default),
        kind,
    };
    Some(Ok(match name {
        "PrintPhenotypeData" => phen("phenotype_count.dat", PhenotypeKind::All),
        "PrintHostPhenotypeData" => phen("host_phenotype_count.dat", PhenotypeKind::Host),
        "PrintParasitePhenotypeData" => {
            phen("parasite_phenotype_count.dat", PhenotypeKind::Parasite)
        }
        "PrintDemeTestamentStats" => Stats2Action::DemeTestamentStats {
            filename: out_file(args, "deme_testament.dat"),
        },
        "PrintCurrentMeanDemeDensity" => Stats2Action::CurrentMeanDemeDensity {
            filename: out_file(args, "deme_currentMeanDensity.dat"),
        },
        "PrintDemesMeritsData" => Stats2Action::DemesMeritsData,
        "PrintNumOrgsInDeme" => Stats2Action::NumOrgsInDeme,
        "SaveDemeFounders" => {
            // `if (largs.GetSize()) m_filename = largs.PopWord();`
            let mut largs = args.to_string();
            let filename = if largs.is_empty() {
                String::new()
            } else {
                pop_word(&mut largs)
            };
            Stats2Action::SaveDemeFounders { filename }
        }
        _ => return None,
    }))
}

/// `cDoubleSum`-style accumulation of `-fraction * log2(fraction)` exactly
/// as cPopulation.cc writes it.
fn shannon_term(fraction: f64) -> f64 {
    fraction * fraction.ln() / 2.0f64.ln()
}

/// `cStringUtil::Stringf("%i.", i) + name`
fn deme_col(deme: usize, name: &str) -> String {
    format!("{deme}.{name}")
}

impl World {
    /// `Process(ctx)` of an action of this module.
    pub(crate) fn run_stats2_action(&mut self, action: &Stats2Action) -> Result<(), String> {
        match action {
            Stats2Action::PhenotypeData { filename, kind } => {
                self.stats2_phenotype_data(filename, *kind)
            }
            Stats2Action::DemeTestamentStats { filename } => {
                self.stats2_deme_testament_stats(filename)
            }
            Stats2Action::CurrentMeanDemeDensity { filename } => {
                self.stats2_current_mean_deme_density(filename)
            }
            Stats2Action::DemesMeritsData => self.stats2_demes_merits_data(),
            Stats2Action::NumOrgsInDeme => self.stats2_num_orgs_in_deme(),
            Stats2Action::SaveDemeFounders { filename } => self.stats2_save_deme_founders(filename),
        }
    }

    /// Called once before the first update, after the event list is loaded
    /// (where the C++ constructors attach data recorders).
    pub(crate) fn stats2_note_recorders(&mut self) {}

    /// Called at the end of every update, after the organism statistics are
    /// gathered and before the status line.
    pub(crate) fn stats2_end_update(&mut self) {}

    /// `cStats::<method>(filename)` for a `STATS_OUT_FILE` action whose
    /// printer lives in this module; `None` for any other method.
    pub(crate) fn print_stats_extra(
        &mut self,
        method: &str,
        filename: &str,
    ) -> Option<Result<(), String>> {
        Some(match method {
            "PrintInterruptData" => self.stats2_interrupt_data(filename),
            "PrintCellVisitsData" => self.stats2_cell_visits_data(),
            "PrintPredicatedMessages" => self.stats2_predicated_messages(filename),
            "PrintDemeOrgGermlineSequestration" => {
                self.stats2_deme_org_germline_sequestration(filename)
            }
            "PrintDemeGermResourcesData" => self.stats2_deme_germ_resources_data(filename),
            "PrintDemeTreatableReplicationData" => {
                self.stats2_deme_treatable_replication_data(filename, true)
            }
            "PrintDemeUntreatableReplicationData" => {
                self.stats2_deme_treatable_replication_data(filename, false)
            }
            "PrintDemeTreatableCount" => self.stats2_deme_treatable_count(filename),
            "PrintDemeNetworkTopology" => {
                // The comment and time stamp stay buffered until the first
                // `Endl`, which never comes: the file is left empty.
                match self.out.static_with_path(filename) {
                    Ok(_) => {
                        self.deme_network_abort();
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            "PrintPerDemeTasksData" => self.stats2_per_deme_tasks(filename, false),
            "PrintPerDemeTasksExeData" => self.stats2_per_deme_tasks(filename, true),
            "PrintAvgDemeTasksExeData" => self.stats2_avg_deme_tasks_exe(filename, None),
            "PrintAvgTreatableDemeTasksExeData" => {
                self.stats2_avg_deme_tasks_exe(filename, Some(true))
            }
            "PrintAvgUntreatableDemeTasksExeData" => {
                self.stats2_avg_deme_tasks_exe(filename, Some(false))
            }
            "PrintPerDemeReactionData" => self.stats2_per_deme_reaction_data(filename),
            "PrintDemeTasksData" => self.stats2_deme_tasks_data(filename, false),
            "PrintDemeTasksExeData" => self.stats2_deme_tasks_data(filename, true),
            "PrintDemeReactionData" => self.stats2_deme_reaction_data(filename),
            "PrintDemeOrgTasksData" => self.stats2_deme_org_tasks(filename, false),
            "PrintDemeOrgTasksExeData" => self.stats2_deme_org_tasks(filename, true),
            "PrintDemeOrgReactionData" => self.stats2_deme_org_reaction_data(filename),
            "PrintDemeCurrentTaskExeData" => self.stats2_deme_current_task_exe_data(filename),
            "PrintCurrentTaskCounts" => self.stats2_current_task_counts(filename),
            "PrintDemesTasksData" => self.stats2_demes_tasks_data(filename),
            "PrintDemesReactionsData" => self.stats2_demes_reactions_data(filename),
            "PrintPerDemeGenPerFounderData" => self.stats2_per_deme_gen_per_founder(filename),
            "PrintDemeMigrationSuicidePoints" => {
                self.stats2_deme_migration_suicide_points(filename)
            }
            "PrintOrganismLocation" => self.stats2_organism_location(filename),
            "PrintCurrentOpinions" => self.stats2_current_opinions(filename),
            "PrintOpinionsSetPerDeme" => self.stats2_opinions_set_per_deme(filename),
            "PrintDenData" => self.stats2_den_data(filename),
            "PrintReputationData" => self.stats2_reputation_data(filename),
            "PrintDirectReciprocityData" => self.stats2_direct_reciprocity_data(filename),
            "PrintStringMatchData" => self.stats2_string_match_data(filename),
            "PrintMimicDisplays" => self.stats2_mimic_displays(filename),
            _ => return None,
        })
    }

    /// The organism in `cell` (`None` for an empty cell). Printers run
    /// between updates, so an occupied cell always holds its organism.
    fn stats2_org(&self, cell: i32) -> Result<Option<&Organism>, String> {
        if !self.is_occupied(cell) {
            return Ok(None);
        }
        self.pop.orgs[cell as usize]
            .as_deref()
            .map(Some)
            .ok_or_else(|| "stats printer reached the running organism".to_string())
    }

    fn stats2_state(&self, cell: i32) -> Result<Option<&OrgState>, String> {
        Ok(self.stats2_org(cell)?.map(|o| &o.state))
    }

    fn stats2_reaction_names(&self) -> Vec<String> {
        (0..self.env.num_reactions().max(0) as usize)
            .map(|r| self.env.reaction_name(r).to_string())
            .collect()
    }

    // ------------------------------------------------------------------
    // cStats.cc printers
    // ------------------------------------------------------------------

    /// `cStats::PrintInterruptData(filename)`
    fn stats2_interrupt_data(&mut self, filename: &str) -> Result<(), String> {
        const NUM_INTERRUPT_MSG_TYPES: usize = 10;
        let mut total_orgs_interrupted: u32 = 0;
        let mut total_threads: u32 = 0;
        let mut interrupt_type_counts = [0i32; NUM_INTERRUPT_MSG_TYPES];
        for d in 0..self.pop.demes.len() {
            for j in 0..self.pop.demes[d].size() {
                let cell = self.pop.demes[d].cell_id(j);
                let Some(org) = self.stats2_org(cell)? else {
                    continue;
                };
                if !org.is_interrupted() {
                    continue;
                }
                total_orgs_interrupted += 1;
                let num_threads = org.hardware.num_threads();
                total_threads = total_threads.wrapping_add(num_threads as u32);
                for k in 0..num_threads {
                    let t = org.hardware.thread_message_trigger_type(k);
                    let slot = usize::try_from(t)
                        .ok()
                        .and_then(|t| interrupt_type_counts.get_mut(t))
                        .ok_or_else(|| {
                            format!(
                                "unsupported: PrintInterruptData with message trigger type {t} \
                                 (C++: out-of-bounds write to interruptTypeCounts)"
                            )
                        })?;
                    *slot += 1;
                }
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Total number of organisms interrupted\n");
        df.write(update, "update");
        df.write(total_orgs_interrupted, "Total organisms interrupted");
        df.write(total_threads, "Total threads");
        for c in interrupt_type_counts {
            df.write(c, "Interrupt Counts");
        }
        df.endl();
        Ok(())
    }

    /// `cStats::PrintCellVisitsData(filename)`: QUIRK, the file name is
    /// ignored; the data always goes to `visits.m`.
    fn stats2_cell_visits_data(&mut self) -> Result<(), String> {
        let update = self.stats.update();
        let xsize = self.pop.world_x;
        let visits: Vec<i32> = self.pop.cells.iter().map(|c| c.visits).collect();
        let df = self.out.static_with_path("visits.m")?;
        df.write_raw(&format!("visits{update:07} = [ ..."));
        for (i, v) in visits.into_iter().enumerate() {
            df.write_block_element(v, i as i32, xsize);
        }
        df.write_raw("];");
        df.flush();
        Ok(())
    }

    /// `cStats::PrintPredicatedMessages(filename)`: the header, then each
    /// registered message predicate prints its lines and is reset
    /// ([`crate::actions3`]).
    fn stats2_predicated_messages(&mut self, filename: &str) -> Result<(), String> {
        let text = self.a3_print_and_reset_predicates()?;
        let df = self.out.static_with_path(filename)?;
        df.write_column_desc("update [update]", "");
        df.write_column_desc("predicate name: [pname]", "");
        df.write_column_desc("predicate data: [pdata]", "");
        df.flush_comments();
        df.ofstream()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// `cStats::PrintDemeOrgGermlineSequestration(filename)`: one line per
    /// deme with each cell's germ flag and point mutations, the deme's
    /// cumulative reaction counts and its germ / organism / mutation
    /// totals.
    fn stats2_deme_org_germline_sequestration(&mut self, filename: &str) -> Result<(), String> {
        let num_reactions = self.env.num_reactions().max(0) as usize;
        let use_germline_1 = self.cfg.demes_use_germline == 1;
        struct DemeRow {
            id: i32,
            res: f64,
            cells: Vec<(i32, i32)>,
            react_count: Vec<i32>,
            num_germ: i32,
            num_present: i32,
            num_mut: i32,
        }
        let mut rows = Vec::with_capacity(self.pop.demes.len());
        for d in 0..self.pop.demes.len() {
            let deme = &self.pop.demes[d];
            // `AvidaArray<int> react_count = deme.GetReactionCount();` zeroed.
            let mut react_count = vec![0i32; num_reactions];
            let mut cells = Vec::with_capacity(deme.size().max(0) as usize);
            let mut num_germ = 0;
            let mut num_mut = 0i32;
            let mut num_present = 0;
            for j in 0..deme.size() {
                match self.stats2_state(deme.cell_id(j))? {
                    Some(s) => {
                        let is_germ = i32::from(s.phenotype.is_germ_cell);
                        cells.push((is_germ, s.num_point_mut));
                        num_germ += is_germ;
                        num_mut = num_mut.wrapping_add(s.num_point_mut);
                        num_present += 1;
                        let org_react = s.phenotype.cumulative_reaction_count(&self.cfg);
                        for (k, r) in react_count.iter_mut().enumerate() {
                            *r = r.wrapping_add(org_react.get(k).copied().unwrap_or(0));
                        }
                    }
                    // Cell is not occuppied.
                    None => cells.push((2, 0)),
                }
            }
            if use_germline_1 {
                // germ are tucked away and not living among the deme
                // population
                num_germ = deme.germline.len() as i32;
            }
            rows.push(DemeRow {
                id: deme.id,
                res: deme.total_res_consumed,
                cells,
                react_count,
                num_germ,
                num_present,
                num_mut,
            });
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Cell data per udpate.");
        df.write_time_stamp();
        for r in rows {
            df.write(update, "Update [update]");
            df.write(r.id, "Deme ID for cell [demeid]");
            df.write(r.res, "Deme resources consumed [demeres]");
            for (is_germ, num_point) in r.cells {
                df.write(is_germ, "Org is germ line [isgerm]");
                df.write(num_point, "Number of point mutations [numPoint]");
            }
            for c in r.react_count {
                df.write(c, "reaction");
            }
            df.write(r.num_germ, "numGerm");
            df.write(r.num_present, "numPresent");
            df.write(r.num_mut, "numMut");
            df.endl();
        }
        Ok(())
    }

    /// `cDeme::GetAveVarWorkLoad()`. QUIRK: the sum gains one value per
    /// reaction (the running workload), not one per organism.
    fn stats2_ave_var_work_load(&self, d: usize) -> Result<(f64, f64), String> {
        let slope = self.cfg.inst_point_mut_slope;
        let mut work_load = DoubleSum::new();
        for &cell in &self.pop.demes[d].cell_ids {
            let Some(s) = self.stats2_state(cell)? else {
                continue;
            };
            let mut cur_org_w = 0.0f64;
            let curr_react = s.phenotype.cumulative_reaction_count(&self.cfg);
            for (j, &c) in curr_react.iter().enumerate() {
                // `avd_deme_reaction_weight(slope, j)`
                let weight = if slope > 0.0 { slope * j as f64 } else { 1.0 };
                // The first task never has any work associated with it.
                if j > 0 {
                    // `cur_org_w += weight * curr_react[j]` (contracted to
                    // one fused multiply-add by the reference compiler).
                    cur_org_w = weight.mul_add(f64::from(c), cur_org_w);
                }
                work_load.add(cur_org_w);
            }
        }
        Ok((work_load.average(), work_load.variance()))
    }

    /// `cStats::PrintDemeGermResourcesData(filename)`: deme 0 only, and only
    /// once its consumed resources pass the next threshold.
    fn stats2_deme_germ_resources_data(&mut self, filename: &str) -> Result<(), String> {
        let deme_id = 0;
        let Some(deme) = self.pop.demes.get(deme_id) else {
            return Err("PrintDemeGermResourcesData: no deme 0".to_string());
        };
        let deme_res = deme.total_res_consumed;
        // `StaticWithPath` opens (and creates) the file either way.
        self.out.static_with_path(filename)?;
        if deme_res > self.stats.m_resource_print_thresh {
            // update thresh
            self.stats.m_resource_print_thresh += f64::from(self.cfg.res_for_deme_rep);
            let (count, percent) = self.stats2_germline_num_percent(deme_id)?;
            let (work, _) = self.stats2_ave_var_work_load(deme_id)?;
            let update = self.stats.update();
            let df = self.out.static_with_path(filename)?;
            df.write_comment("Avida deme germ/soma and resources amassed data");
            df.write_time_stamp();
            df.write(update, "Update [update]");
            df.write(deme_res, "Mean amount of resources consumed");
            df.write(count, "Mean number of organisms flagged as germ");
            df.write(percent, "Mean percent of organisms flagged as germ");
            df.write(work, "Mean workload of organisms");
            df.endl();
        }
        Ok(())
    }

    /// `cDeme::GetGermlineNumPercent()`
    fn stats2_germline_num_percent(&self, d: usize) -> Result<(f64, f64), String> {
        let mut count = 0.0;
        let mut total_count = 0.0;
        for &cell in &self.pop.demes[d].cell_ids {
            if let Some(s) = self.stats2_state(cell)? {
                if s.phenotype.is_germ_cell {
                    count += 1.0;
                }
                total_count += 1.0;
            }
        }
        Ok((count, (count / total_count) * 100.0))
    }

    /// `cStats::PrintDemeTreatableReplicationData(filename)` (`treatable`)
    /// and `PrintDemeUntreatableReplicationData`: the replications since the
    /// last call, which are then cleared.
    fn stats2_deme_treatable_replication_data(
        &mut self,
        filename: &str,
        treatable: bool,
    ) -> Result<(), String> {
        let update = self.stats.update();
        let r = &mut self.pop.deme_repl_stats;
        let (num, gest, births, merit, generation, density) = if treatable {
            (
                &mut r.deme_num_repls_treatable,
                &mut r.deme_gestation_time_treatable,
                &mut r.deme_births_treatable,
                &mut r.deme_merit_treatable,
                &mut r.deme_generation_treatable,
                &mut r.deme_density_treatable,
            )
        } else {
            (
                &mut r.deme_num_repls_untreatable,
                &mut r.deme_gestation_time_untreatable,
                &mut r.deme_births_untreatable,
                &mut r.deme_merit_untreatable,
                &mut r.deme_generation_untreatable,
                &mut r.deme_density_untreatable,
            )
        };
        let df = self.out.static_with_path(filename)?;
        df.write_comment(if treatable {
            "Avida deme replication data for treatable deme"
        } else {
            "Avida deme replication data for untreatable deme"
        });
        df.write_time_stamp();
        df.write(update, "Update [update]");
        df.write(*num, "Number of deme replications [numrepl]");
        df.write(gest.average(), "Mean deme gestation time [gesttime]");
        df.write(
            births.average(),
            "Mean number of births within replicated demes [numbirths]",
        );
        df.write(
            merit.average(),
            "Mean heritable merit of replicated demes [merit]",
        );
        df.write(
            generation.average(),
            "Mean generation of replicated demes [generation]",
        );
        df.write(
            density.average(),
            "Mean density of replicated demes [density]",
        );
        df.endl();
        *num = 0;
        gest.clear();
        births.clear();
        merit.clear();
        generation.clear();
        density.clear();
        Ok(())
    }

    /// `cStats::PrintDemeTreatableCount(filename)`
    fn stats2_deme_treatable_count(&mut self, filename: &str) -> Result<(), String> {
        let treatable = self.pop.demes.iter().filter(|d| d.treatable).count() as i32;
        let untreatable = self.pop.demes.len() as i32 - treatable;
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida deme replication data for untreatable deme");
        df.write_time_stamp();
        df.write(update, "Update [update]");
        df.write(treatable, "Number demes treatable");
        df.write(untreatable, "Number demes untreatable");
        df.write(
            f64::from(treatable) / f64::from(untreatable),
            "Treatable:untreatable ratio",
        );
        df.endl();
        Ok(())
    }

    /// The `"%i." + task name` columns of one per-deme array per deme.
    fn stats2_write_per_deme(
        &mut self,
        filename: &str,
        comments: &[&str],
        names: &[String],
        values: impl Fn(&crate::deme::Deme) -> Vec<i32>,
    ) -> Result<(), String> {
        let columns: Vec<Vec<i32>> = self.pop.demes.iter().map(values).collect();
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment(comments[0]);
        df.write_time_stamp();
        for c in &comments[1..] {
            df.write_comment(c);
        }
        df.write(update, "Update");
        for (i, col) in columns.iter().enumerate() {
            for (j, name) in names.iter().enumerate() {
                df.write(col.get(j).copied().unwrap_or(0), &deme_col(i, name));
            }
        }
        df.endl();
        Ok(())
    }

    /// `cStats::PrintPerDemeTasksData(filename)` (whether each deme did each
    /// task in its last lifetime) and `PrintPerDemeTasksExeData` (`exe`:
    /// how often).
    fn stats2_per_deme_tasks(&mut self, filename: &str, exe: bool) -> Result<(), String> {
        let names = self.stats.task_names.clone();
        let comments: &[&str] = if exe {
            &[
                "Avida deme tasks exe data",
                "First column gives the current update, next columns give the number",
                "of times a task has contributed to the merit of all organisms",
                "in a particular deme",
            ]
        } else {
            &[
                "Avida deme tasks data",
                "First column gives the current update, next columns give the number",
                "of organisms that have the particular task as a component of their merit",
                "in a particular deme",
            ]
        };
        self.stats2_write_per_deme(filename, comments, &names, |d| {
            if exe {
                d.last_task_exe_count.clone()
            } else {
                d.last_task_exe_count
                    .iter()
                    .map(|&c| i32::from(c > 0))
                    .collect()
            }
        })
    }

    /// `cStats::PrintAvgDemeTasksExeData(filename)` and the treatable
    /// (`Some(true)`) / untreatable (`Some(false)`) variants: the mean of
    /// the demes' last task execution counts (`Accumulator<int>::Mean`).
    fn stats2_avg_deme_tasks_exe(
        &mut self,
        filename: &str,
        treatable: Option<bool>,
    ) -> Result<(), String> {
        let num_demes = self.pop.demes.len();
        let names = self.stats.task_names.clone();
        let num_tasks = names.len();
        let mut means = Vec::with_capacity(num_tasks);
        for t in 0..num_tasks {
            let mut tasksum = Accumulator::<i32>::new();
            for deme in &self.pop.demes {
                if treatable.is_none_or(|want| deme.treatable == want) {
                    tasksum.add(deme.last_task_exe_count.get(t).copied().unwrap_or(0));
                }
            }
            means.push(tasksum.mean());
        }
        let (title, which) = match treatable {
            None => ("Avida average deme tasks data", "the demes"),
            Some(true) => (
                "Avida average tasks data for treatable demes",
                "treatable demes",
            ),
            Some(false) => (
                "Avida average tasks data for untreatable demes",
                "untreatable demes",
            ),
        };
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment(title);
        df.write_time_stamp();
        df.write_comment(
            "First column is the update, remaining columns are the average number of times",
        );
        df.write_comment(&format!("each task has been executed by {which}"));
        df.write_comment(&format!(
            "Data based on {num_demes} demes and {num_tasks} tasks"
        ));
        df.write(update, "Update");
        for (m, name) in means.into_iter().zip(&names) {
            df.write(m, name);
        }
        df.endl();
        Ok(())
    }

    /// `cStats::PrintPerDemeReactionData(filename)`
    fn stats2_per_deme_reaction_data(&mut self, filename: &str) -> Result<(), String> {
        let names = self.stats2_reaction_names();
        self.stats2_write_per_deme(
            filename,
            &[
                "Avida deme reactions data",
                "First column gives the current update, all further columns give the number",
                "of currently living organisms each reaction has affected.",
            ],
            &names,
            |d| d.last_reaction_count.clone(),
        )
    }

    /// `cStats::PrintDemeTasksData(filename)` (fraction of occupied demes
    /// that did each task) and `PrintDemeTasksExeData` (`exe`: mean
    /// executions per occupied deme). QUIRK: the per-task sums start at the
    /// number of tasks (`SetAll(num_tasks)`), not 0.
    fn stats2_deme_tasks_data(&mut self, filename: &str, exe: bool) -> Result<(), String> {
        let names = self.stats.task_names.clone();
        let num_tasks = names.len();
        let mut deme_tasks = vec![num_tasks as i32; num_tasks];
        let mut occupied_demes = 0;
        for deme in &self.pop.demes {
            if deme.cur_org_count != 0 {
                occupied_demes += 1;
                for (j, s) in deme_tasks.iter_mut().enumerate() {
                    let c = deme.last_task_exe_count.get(j).copied().unwrap_or(0);
                    *s = s.wrapping_add(if exe { c } else { i32::from(c > 0) });
                }
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        if exe {
            df.write_comment("Avida deme tasks exe data");
            df.write_time_stamp();
            df.write_comment("First column gives the current update, next columns give the number");
            df.write_comment("of times per deme that a given task counted as a component of an");
            df.write_comment("organisms's merit during the lifetime of the deme");
        } else {
            df.write_comment("Avida deme tasks data");
            df.write_time_stamp();
            df.write_comment("First column gives the current update, next columns give the number");
            df.write_comment(
                "of organisms per deme that had the given task as a component of their merit",
            );
            df.write_comment("during the lifetime of the deme");
        }
        df.write(update, "Update");
        for (s, name) in deme_tasks.into_iter().zip(&names) {
            df.write(f64::from(s) / f64::from(occupied_demes), name);
        }
        df.endl();
        Ok(())
    }

    /// `cStats::PrintDemeReactionData(filename)`: mean reaction counts per
    /// occupied deme.
    fn stats2_deme_reaction_data(&mut self, filename: &str) -> Result<(), String> {
        let names = self.stats2_reaction_names();
        let mut deme_reactions = vec![0i32; names.len()];
        let mut occupied_demes = 0;
        for deme in &self.pop.demes {
            if deme.cur_org_count != 0 {
                occupied_demes += 1;
                for (j, s) in deme_reactions.iter_mut().enumerate() {
                    *s = s.wrapping_add(deme.last_reaction_count.get(j).copied().unwrap_or(0));
                }
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida deme reactions data");
        df.write_time_stamp();
        df.write_comment(
            "First column gives the current update, all further columns give the number",
        );
        df.write_comment("of times each reaction has affected a deme.");
        df.write(update, "Update");
        for (s, name) in deme_reactions.into_iter().zip(&names) {
            df.write(f64::from(s) / f64::from(occupied_demes), name);
        }
        df.endl();
        Ok(())
    }

    /// `cStats::PrintDemeOrgTasksData(filename)` and
    /// `PrintDemeOrgTasksExeData` (`exe`): the organism task counts at the
    /// deme's last division.
    fn stats2_deme_org_tasks(&mut self, filename: &str, exe: bool) -> Result<(), String> {
        let names = self.stats.task_names.clone();
        let comments: &[&str] = if exe {
            &[
                "Avida deme org tasks exe data",
                "First column gives the current update, next columns give the number",
                "of times a task has contributed to the merit of all organisms",
                "in a particular deme when the deme last divided.",
            ]
        } else {
            &[
                "Avida deme org tasks data",
                "First column gives the current update, next columns give the number",
                "of organisms that have the particular task as a component of their merit",
                "in a particular deme when the deme last divided.",
            ]
        };
        self.stats2_write_per_deme(filename, comments, &names, |d| {
            if exe {
                d.last_org_task_exe_count.clone()
            } else {
                d.last_org_task_count.clone()
            }
        })
    }

    /// `cStats::PrintDemeOrgReactionData(filename)`
    fn stats2_deme_org_reaction_data(&mut self, filename: &str) -> Result<(), String> {
        let names = self.stats2_reaction_names();
        self.stats2_write_per_deme(
            filename,
            &[
                "Avida deme org reactions data",
                "First column gives the current update, all further columns give the number",
                "of currently living organisms each reaction has affected",
                "in a particular deme when the deme last divided.",
            ],
            &names,
            |d| d.last_org_reaction_count.clone(),
        )
    }

    /// `cStats::PrintDemeCurrentTaskExeData(filename)`
    fn stats2_deme_current_task_exe_data(&mut self, filename: &str) -> Result<(), String> {
        let names = self.stats.task_names.clone();
        self.stats2_write_per_deme(
            filename,
            &[
                "Avida deme current task exe data",
                "First column gives update number, next columns give the number",
                "of times a given task has been executed in a given deme by",
                "some organism in that deme.",
            ],
            &names,
            |d| d.cur_task_exe_count.clone(),
        )
    }

    /// `cStats::PrintCurrentTaskCounts(filename)`: a grid of each
    /// organism's current count of task 0.
    fn stats2_current_task_counts(&mut self, filename: &str) -> Result<(), String> {
        let (wx, wy) = (self.pop.world_x, self.pop.world_y);
        let mut text = format!("Update {}:\n", self.stats.update());
        for y in 0..wy {
            for x in 0..wx {
                match self.stats2_state(y * wx + x)? {
                    Some(s) => {
                        let c = s.phenotype.cur_task_count.first().copied().ok_or(
                            "PrintCurrentTaskCounts: no tasks (C++: out-of-bounds array read)",
                        )?;
                        text.push_str(&format!("{c}\t"));
                    }
                    None => text.push_str("---\t"),
                }
            }
            text.push('\n');
        }
        text.push('\n');
        let df = self.out.static_with_path(filename)?;
        df.ofstream()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        df.flush();
        Ok(())
    }

    /// `cStats::PrintDemesTasksData(filename)`: each deme's
    /// `GetTaskCount()`. Only deme output (`DoDemeOutput`, not ported: an
    /// organism reaching it stops the run) counts tasks there, so every
    /// count is 0.
    fn stats2_demes_tasks_data(&mut self, filename: &str) -> Result<(), String> {
        let names = self.stats.task_names.clone();
        self.stats2_write_demes_zero(filename, "Avida current tasks done by each deme", &names)
    }

    /// `cStats::PrintDemesReactionsData(filename)`: each deme's
    /// `GetReactionCount()`, 0 for the reason given at
    /// [`World::stats2_demes_tasks_data`].
    fn stats2_demes_reactions_data(&mut self, filename: &str) -> Result<(), String> {
        let names = self.stats2_reaction_names();
        self.stats2_write_demes_zero(
            filename,
            "Avida current reactions done by each deme",
            &names,
        )
    }

    fn stats2_write_demes_zero(
        &mut self,
        filename: &str,
        comment: &str,
        names: &[String],
    ) -> Result<(), String> {
        let num_demes = self.pop.demes.len();
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment(comment);
        df.write_time_stamp();
        df.write(update, "Update");
        for d in 0..num_demes {
            for name in names {
                df.write(0, &deme_col(d, name));
            }
        }
        df.endl();
        Ok(())
    }

    /// `cStats::PrintPerDemeGenPerFounderData(filename)`
    fn stats2_per_deme_gen_per_founder(&mut self, filename: &str) -> Result<(), String> {
        let values: Vec<f64> = self
            .pop
            .demes
            .iter()
            .map(|d| {
                if d.cur_org_count == 0 {
                    -1.0
                } else {
                    d.generations_per_lifetime
                }
            })
            .collect();
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida org generations between deme founders");
        df.write_time_stamp();
        df.write_comment(
            "First column gives the current update, all further columns give the number",
        );
        df.write_comment(
            "number of generations that passed between the parent and current deme's founders",
        );
        df.write(update, "Update");
        for (i, v) in values.into_iter().enumerate() {
            df.write(v, &format!("deme.{i}"));
        }
        df.endl();
        Ok(())
    }

    /// `cStats::PrintDemeMigrationSuicidePoints(filename)`
    fn stats2_deme_migration_suicide_points(&mut self, filename: &str) -> Result<(), String> {
        let mut max_points = 0.0f64;
        let mut min_points = -1.0f64;
        let mut total_points = 0.0f64;
        let mut max_suicides = 0i32;
        let mut min_suicides = -1i32;
        let mut total_suicides = 0.0f64;
        let mut max_migrations = 0i32;
        let mut min_migrations = -1i32;
        let mut total_migrations = 0.0f64;
        let mut deme_count = 0i32;
        for deme in &self.pop.demes {
            let temp_points = deme.points;
            let temp_suicides = deme.suicides as i32;
            let temp_migrations = deme.migrations_out as i32;
            // Calculate Min
            if min_points == -1.0 || temp_points < min_points {
                min_points = temp_points;
            }
            if min_suicides == -1 || temp_suicides < min_suicides {
                min_suicides = temp_suicides;
            }
            if min_migrations == -1 || temp_migrations < min_migrations {
                min_migrations = temp_migrations;
            }
            // Calculate Max
            if temp_points > max_points {
                max_points = temp_points;
            }
            if temp_suicides > max_suicides {
                max_suicides = temp_suicides;
            }
            if temp_migrations > max_migrations {
                max_migrations = temp_migrations;
            }
            total_points += temp_points;
            total_suicides += f64::from(temp_suicides);
            total_migrations += f64::from(temp_migrations);
            if temp_points > 0.0 {
                deme_count += 1;
            }
        }
        let num_demes = self.pop.demes.len() as i32;
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida average stats");
        df.write_time_stamp();
        df.write(update, "Update");
        df.write(
            total_points / f64::from(num_demes),
            "AveragePoints[avpoints]",
        );
        df.write(min_points, "MinPoints[minpoints]");
        df.write(max_points, "MaxPoints[maxpoints]");
        df.write(deme_count, "DemesWithPoints[demeswithpoints]");
        df.write(
            total_suicides / f64::from(num_demes),
            "AverageSuicides[avsuicides]",
        );
        df.write(min_suicides, "MinSuicides[minsuicides]");
        df.write(max_suicides, "MaxSuicides[maxsuicides]");
        df.write(
            total_migrations / f64::from(num_demes),
            "AverageMigrations[avmigrations]",
        );
        df.write(min_migrations, "MinMigrations[minmigrations]");
        df.write(max_migrations, "MaxMigrations[maxmigrations]");
        df.write(
            total_suicides / total_migrations,
            "SuicideMigrationRate[suicidemigrationrate]",
        );
        df.endl();
        Ok(())
    }

    /// `cStats::PrintOrganismLocation(filename)`: one line per organism.
    fn stats2_organism_location(&mut self, filename: &str) -> Result<(), String> {
        let mut rows = Vec::new();
        for i in 0..self.pop.cells.len() as i32 {
            if let Some(s) = self.stats2_state(i)? {
                rows.push((i, s.id));
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Organism location data");
        df.write_time_stamp();
        for (i, id) in rows {
            df.write(update, "Update [update]");
            df.write(i, "Cell ID [cellid]");
            df.write(id, "Organism ID [orgid]");
            df.endl();
        }
        Ok(())
    }

    /// `cStats::PrintCurrentOpinions(filename)`: each cell's organism's
    /// opinion and the (last) cell whose cell data equals it.
    fn stats2_current_opinions(&mut self, filename: &str) -> Result<(), String> {
        // Build the cell id map:
        let mut data_id_map: BTreeMap<i32, i32> = BTreeMap::new();
        for cell in &self.pop.cells {
            data_id_map.insert(cell.cell_data.contents, cell.cell_id);
        }
        let mut rows = Vec::with_capacity(self.pop.cells.len());
        for i in 0..self.pop.cells.len() as i32 {
            let cell_id = self.pop.cells[i as usize].cell_id;
            let opinion = self
                .stats2_state(i)?
                .and_then(|s| s.opinions.as_ref().and_then(|l| l.last()).map(|o| o.0));
            rows.push(match opinion {
                Some(op) => (cell_id, op, data_id_map.get(&op).copied().unwrap_or(-1)),
                None => (cell_id, 0, -1),
            });
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Current opinions of each organism.");
        df.write_time_stamp();
        df.write_comment("1: Update [update]");
        df.write_comment("2: Global cell ID [globalid]");
        df.write_comment("3: Current opinion [opinion]");
        df.write_comment("4: Cell ID of opinion [cellid]");
        df.flush_comments();
        for (cell_id, opinion, data_cell) in rows {
            df.write_anonymous(update);
            df.write_anonymous(cell_id);
            df.write_anonymous(opinion);
            df.write_anonymous(data_cell);
            df.endl();
        }
        Ok(())
    }

    /// `cStats::PrintOpinionsSetPerDeme(filename)`. QUIRK: each deme's
    /// values are added once per cell of the deme.
    fn stats2_opinions_set_per_deme(&mut self, filename: &str) -> Result<(), String> {
        let mut treatable_opinion_counts = Accumulator::<i32>::new();
        let mut untreatable_opinion_counts = Accumulator::<i32>::new();
        let mut treatable_density_counts = DoubleSum::new();
        let mut untreatable_density_counts = DoubleSum::new();
        for d in 0..self.pop.demes.len() {
            let deme = &self.pop.demes[d];
            // `cDeme::GetNumOrgsWithOpinion()`
            let mut with_opinion = 0;
            for &cell in &deme.cell_ids {
                if self
                    .stats2_state(cell)?
                    .is_some_and(|s| s.opinions.as_ref().is_some_and(|o| !o.is_empty()))
                {
                    with_opinion += 1;
                }
            }
            let density = deme.density();
            for _ in 0..deme.size() {
                if deme.treatable {
                    treatable_opinion_counts.add(with_opinion);
                    treatable_density_counts.add(density);
                } else {
                    untreatable_opinion_counts.add(with_opinion);
                    untreatable_density_counts.add(density);
                }
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Current fractions of opinions set in deme.");
        df.write_comment("This files shows data for both treatable and untreatable demes.");
        df.write_time_stamp();
        df.write(update, "Update [update]");
        if treatable_opinion_counts.count() > 0 && untreatable_opinion_counts.count() > 0 {
            df.write(
                treatable_opinion_counts.mean(),
                "Average number of opinions set in Treatable demes",
            );
            df.write(
                untreatable_opinion_counts.mean(),
                "Average number of opinions set in Unreatable demes",
            );
            df.write(
                treatable_density_counts.average(),
                "Average density of Treatable demes",
            );
            df.write(
                untreatable_density_counts.average(),
                "Average density of Unreatable demes",
            );
        } else {
            df.write(
                untreatable_opinion_counts.mean(),
                "Average number of opinions set in demes",
            );
            df.write(
                untreatable_density_counts.average(),
                "Average density of demes",
            );
        }
        df.endl();
        Ok(())
    }

    /// `cStats::PrintDenData(filename)` (nothing without avatars). QUIRK:
    /// a cell's avatars are counted as foragers / off-nest guards once for
    /// every non-den resource listed before its first den resource.
    fn stats2_den_data(&mut self, filename: &str) -> Result<(), String> {
        if self.cfg.use_avatars <= 0 {
            return Ok(());
        }
        let juv_age = self.cfg.juv_period;
        let habitats: Vec<i32> = self
            .env
            .resource_lib()
            .resources()
            .iter()
            .map(|r| r.habitat)
            .collect();
        let mut num_juvs = 0;
        let mut num_adults = 0;
        let mut num_guards_nest = 0;
        let mut num_foragers = 0;
        let mut num_guards_off = 0;
        let mut num_loiterers = 0;
        let mut active_dens = 0;
        for i in 0..self.pop.cells.len() as i32 {
            if !self.avatars.has_av(i) {
                continue;
            }
            let cell_res =
                self.with_pop_resources(|rc, rng, env| rc.get_cell_resources(i, rng, env).to_vec());
            // (time used, guard) of each avatar's organism.
            let mut avs = Vec::new();
            for id in self.avatars.cell_avs(i) {
                let s = self
                    .avatars_org_cell(id)
                    .and_then(|c| self.pop.orgs[c as usize].as_ref())
                    .map(|o| (o.state.phenotype.time_used, o.state.guard))
                    .ok_or("PrintDenData: avatar without a living organism")?;
                avs.push(s);
            }
            let mut is_active = false;
            for (j, &amount) in cell_res.iter().enumerate() {
                let habitat = habitats.get(j).copied().unwrap_or(0);
                if (habitat == 4 || habitat == 3) && amount > 0.0 {
                    for &(time_used, guard) in &avs {
                        if time_used < juv_age {
                            num_juvs += 1;
                            is_active = true;
                        } else {
                            num_adults += 1;
                            if guard {
                                num_guards_nest += 1;
                            } else {
                                num_loiterers += 1;
                            }
                        }
                    }
                    active_dens += i32::from(is_active);
                    break; // only do this once if two dens overlap
                }
                for &(_, guard) in &avs {
                    num_adults += 1;
                    if guard {
                        num_guards_off += 1;
                    } else {
                        num_foragers += 1;
                    }
                }
            }
        }
        if let Some(e) = self.res_env.unsupported.take() {
            return Err(e);
        }
        let update = self.stats.update();
        let juv_killed = self.stats.juv_killed;
        let num_guard_fail = self.stats.num_guard_fail;
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Number of juveniles and adults in dens");
        df.write_time_stamp();
        for d in [
            "Update [update]",
            "ActiveDens [active_dens]",
            "Juveniles [juveniles]",
            "Adults [adults]",
            "Guards On Nest[guards nest]",
            "Loiterers [loiterers]",
            "Juveniles Killed [juveniles killed]",
            "Foragers [foragers]",
            "Guards Off Nest [guards off]",
            "Guard Fails [guard fails]",
        ] {
            df.write_column_desc(d, "");
        }
        df.flush_comments();
        df.write(update, "Update");
        df.write(active_dens, "ActiveDens");
        df.write(num_juvs, "Juveniles");
        df.write(num_adults, "Adults");
        df.write(num_guards_nest, "GuardsOnNest");
        df.write(num_loiterers, "Loiterers");
        df.write(juv_killed, "JuvenilesKilled");
        df.write(num_foragers, "Foragers");
        df.write(num_guards_off, "GuardsOffNest");
        df.write(num_guard_fail, "Guard Failures");
        df.endl();
        self.stats.num_guard_fail = 0;
        Ok(())
    }

    /// The cell of the organism `uid`.
    fn avatars_org_cell(&self, uid: avida_hardware::organism::OrgUid) -> Option<i32> {
        self.cell_of_org(uid)
    }

    /// `cStats::PrintReputationData(filename)`
    fn stats2_reputation_data(&mut self, filename: &str) -> Result<(), String> {
        let mut reputations = DoubleSum::new();
        let mut donations = DoubleSum::new();
        let mut donors = DoubleSum::new();
        let mut own_raw_mat = DoubleSum::new();
        let mut other_raw_mat = DoubleSum::new();
        let mut num_alt = 0;
        let mut num_coop = 0;
        for i in 0..self.pop.cells.len() as i32 {
            let Some(s) = self.stats2_state(i)? else {
                continue;
            };
            // `GetReputation()`: the newest opinion, 0 without one.
            let cur_rep = s
                .opinions
                .as_ref()
                .and_then(|l| l.last())
                .map_or(0, |o| o.0);
            reputations.add(f64::from(cur_rep));
            donations.add(f64::from(s.num_donate));
            own_raw_mat.add(f64::from(s.self_raw_materials));
            other_raw_mat.add(f64::from(s.other_raw_materials));
            donors.add(s.donor_list.len() as f64);
            if s.num_donate > 0 {
                num_alt += 1;
            }
            // `(received && donations) > 0`
            if s.num_donate_received != 0 && s.num_donate != 0 {
                num_coop += 1;
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment(
            "Avida organism reputation information -- average donations, min donations, max donations",
        );
        df.write_time_stamp();
        df.write(update, "Update [update]");
        df.write(reputations.average(), "Avg. reputation [reputation]");
        df.write(donations.average(), "Avg. donations [donation]");
        df.write(donors.average(), "Avg. number of donor partners [partners]");
        df.write(num_alt, "Number of altruists [altruists]");
        df.write(num_coop, "Number of cooperators [cooperators]");
        df.write(own_raw_mat.average(), "Avg. own raw mat [ownrawmat]");
        df.write(other_raw_mat.average(), "Avg. other raw mat [otherrawmat]");
        df.endl();
        Ok(())
    }

    /// `cStats::PrintDirectReciprocityData(filename)`
    fn stats2_direct_reciprocity_data(&mut self, filename: &str) -> Result<(), String> {
        let mut donations = DoubleSum::new();
        let mut donors = DoubleSum::new();
        let mut num_donations_received = DoubleSum::new();
        let mut num_alt = 0;
        let mut num_coop = 0;
        let mut num_lin_2 = 0;
        let mut num_lin_1 = 0;
        let mut total_org = 0;
        for i in 0..self.pop.cells.len() as i32 {
            let Some(s) = self.stats2_state(i)? else {
                continue;
            };
            donations.add(f64::from(s.num_donate));
            num_donations_received.add(f64::from(s.num_donate_received));
            donors.add(s.donor_list.len() as f64);
            if s.num_donate > 0 {
                num_alt += 1;
            }
            if s.num_donate_received != 0 && s.num_donate != 0 {
                num_coop += 1;
            }
            if s.lineage_label == 1 {
                num_lin_1 += 1;
            }
            if s.lineage_label == 2 {
                num_lin_2 += 1;
            }
            total_org += 1;
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida organism direct reciprocity information");
        df.write_time_stamp();
        df.write(update, "Update [update]");
        df.write(donations.average(), "Avg. donations [donation]");
        df.write(
            num_donations_received.average(),
            "Avg. donations received [received]",
        );
        df.write(donors.average(), "Avg. number of donor partners [partners]");
        df.write(num_alt, "Number of altruists [altruists]");
        df.write(num_coop, "Number of cooperators [cooperators]");
        df.write(num_lin_1, "Number of organisms of lineage 1 [lineage1]");
        df.write(num_lin_2, "Number of organisms of lineage 2 [lineage2]");
        df.write(total_org, "Number of organisms in population [popsize]");
        df.endl();
        Ok(())
    }

    /// `cStats::PrintStringMatchData(filename)`. Reading a string count
    /// through `(*m_string_map)[j]` inserts a zero entry for a missing tag,
    /// which is mirrored here.
    fn stats2_string_match_data(&mut self, filename: &str) -> Result<(), String> {
        let num = self.env.tasklib().0.get_number_of_match_strings() as i32;
        let match_strings: Vec<String> = (0..num)
            .map(|k| self.env.tasklib().0.get_match_string(k))
            .collect();
        let mut strings_stored: BTreeMap<i32, DoubleSum> = BTreeMap::new();
        let mut strings_produced: BTreeMap<i32, DoubleSum> = BTreeMap::new();
        let mut total = DoubleSum::new();
        let mut instant_perfect_match = 0i32;
        let mut instant_perfect_match_org = 0;
        let mut nothing = 0;
        let mut specialists = 0;
        let mut generalists = 0;
        for i in 0..self.pop.cells.len() {
            if !self.is_occupied(i as i32) {
                continue;
            }
            let Some(org) = self.pop.orgs[i].as_mut() else {
                return Err("stats printer reached the running organism".to_string());
            };
            let mut min = -1;
            let mut type_prod = 0;
            for j in 0..num {
                let (prod, onhand) = match org.state.string_map.as_mut() {
                    None => (0, 0),
                    Some(m) => {
                        let pos = match m.binary_search_by_key(&j, |e| e.0) {
                            Ok(p) => p,
                            Err(p) => {
                                m.insert(p, (j, 0, 0));
                                p
                            }
                        };
                        (m[pos].1, m[pos].2)
                    }
                };
                if min == -1 || onhand < min {
                    min = onhand;
                }
                strings_stored.entry(j).or_default().add(f64::from(onhand));
                total.add(f64::from(onhand));
                strings_produced.entry(j).or_default().add(f64::from(prod));
                if prod != 0 {
                    type_prod += 1;
                }
            }
            instant_perfect_match = instant_perfect_match.wrapping_add(min);
            if min > 0 {
                instant_perfect_match_org += 1;
            }
            if type_prod == 0 {
                nothing += 1;
            }
            if type_prod == 1 {
                specialists += 1;
            }
            if type_prod > 1 {
                generalists += 1;
            }
        }
        let update = self.stats.update();
        let perfect_match = self.stats.m_perfect_match.sum();
        self.stats.m_perfect_match.clear();
        let perfect_match_org = self.stats.m_perfect_match_org.sum();
        self.stats.m_perfect_match_org.clear();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida organism string donation information");
        df.write_time_stamp();
        df.write(update, "Update [update]");
        // print the string info
        for (k, s) in match_strings.iter().enumerate() {
            let k = k as i32;
            let produced = strings_produced.get(&k).map_or(0.0, DoubleSum::average);
            df.write(produced, &format!("produced{s}"));
            let stored = strings_stored.get(&k).map_or(0.0, DoubleSum::average);
            df.write(stored, &format!("stored{s}"));
        }
        df.write(total.average(), "totalStoredAverage");
        // Print number of perfect matches
        df.write(perfect_match, "PerfectMatchStringElapse[ps]");
        df.write(perfect_match_org, "PerfectMatchOrgElapse[pso]");
        df.write(instant_perfect_match, "PerfectMatchStringInstant[psi]");
        df.write(instant_perfect_match_org, "PerfectMatchOrgInstant[psoi]");
        df.write(nothing, "Producednothing[nothing]");
        df.write(generalists, "Generalists[generalists]");
        df.write(specialists, "Specialists[specialists]");
        df.endl();
        Ok(())
    }

    /// `cStats::PrintMimicDisplays(filename)`: how many mimics (forage
    /// target 1) display each non-negative forage target.
    fn stats2_mimic_displays(&mut self, filename: &str) -> Result<(), String> {
        let poss_fts: Vec<i32> = self
            .env
            .target_ids()
            .iter()
            .copied()
            .filter(|&t| t >= 0)
            .collect();
        let mut displayed_fts = vec![0i32; poss_fts.len()];
        for &cell in &self.pop.live_org_list {
            let Some(s) = self.stats2_state(cell)? else {
                continue;
            };
            if s.forage_target == 1 {
                // fts may not be sequentially numbered
                if let Some(j) = poss_fts.iter().position(|&ft| ft == s.show_ft) {
                    displayed_fts[j] += 1;
                }
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("FTs being displayed by mimics on update boundary (true mimic ft == 1).");
        df.write_time_stamp();
        df.write(update, "Update");
        for (ft, n) in poss_fts.into_iter().zip(displayed_fts) {
            df.write(ft, "Displayed FT");
            df.write(n, "Num Orgs Displaying this FT");
        }
        df.endl();
        Ok(())
    }

    // ------------------------------------------------------------------
    // cPopulation.cc and PrintActions.cc printers
    // ------------------------------------------------------------------

    /// `cPopulation::PrintPhenotypeData(filename)` and the host / parasite
    /// variants: phenotype diversity by which tasks (and how often) each
    /// organism did in its last gestation.
    fn stats2_phenotype_data(&mut self, filename: &str, kind: PhenotypeKind) -> Result<(), String> {
        let mut ids: BTreeSet<i32> = BTreeSet::new();
        let mut complete: BTreeSet<String> = BTreeSet::new();
        let mut average_shannon_diversity = 0.0f64;
        let mut num_orgs = 0i32;
        let mut average_num_tasks = 0.0f64;
        // implementing a very poor man's hash...
        let mut phenotypes: Vec<i32> = Vec::new();
        let mut phenotype_counts: Vec<i32> = Vec::new();
        for i in 0..self.pop.cells.len() as i32 {
            let Some(s) = self.stats2_state(i)? else {
                continue;
            };
            num_orgs += 1;
            let counts = match kind {
                PhenotypeKind::All => &s.phenotype.last_task_count,
                PhenotypeKind::Host => &s.phenotype.last_host_tasks,
                PhenotypeKind::Parasite => &s.phenotype.last_para_tasks,
            };
            let mut total_tasks = 0i32;
            let mut id = 0i32;
            let mut key = String::new();
            for (j, &c) in counts.iter().enumerate() {
                if c > 0 {
                    id = id.wrapping_add(1i32.wrapping_shl(j as u32));
                    average_num_tasks += 1.0;
                }
                key.push_str(&format!("{c}-"));
                total_tasks = total_tasks.wrapping_add(c);
            }
            ids.insert(id);
            complete.insert(key);
            // add one to our count for this key
            match phenotypes.iter().position(|&p| p == id) {
                Some(k) => phenotype_counts[k] += 1,
                // this is a new key
                None => {
                    phenotypes.push(id);
                    phenotype_counts.push(1);
                }
            }
            // go through again to calculate Shannon Diversity of task
            // counts now that we know the total number of tasks done
            let mut shannon_diversity = 0.0f64;
            for &c in counts {
                if c == 0 {
                    continue;
                }
                let fraction = f64::from(c) / f64::from(total_tasks);
                shannon_diversity -= shannon_term(fraction);
            }
            average_shannon_diversity += shannon_diversity;
        }
        let mut shannon_diversity_of_phenotypes = 0.0f64;
        for &c in &phenotype_counts {
            let fraction = f64::from(c) / f64::from(num_orgs);
            shannon_diversity_of_phenotypes -= shannon_term(fraction);
        }
        average_shannon_diversity /= f64::from(num_orgs);
        average_num_tasks /= f64::from(num_orgs);

        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_time_stamp();
        df.write(update, "Update");
        df.write(ids.len() as i32, "Unique Phenotypes (by task done)");
        df.write(
            shannon_diversity_of_phenotypes,
            "Shannon Diversity of Phenotypes (by task done)",
        );
        df.write(complete.len() as i32, "Unique Phenotypes (by task count)");
        df.write(
            average_shannon_diversity,
            "Average Phenotype Shannon Diversity (by task count)",
        );
        df.write(
            average_num_tasks,
            "Average Task Diversity (number of different tasks)",
        );
        df.endl();
        Ok(())
    }

    /// `cPopulation::PrintDemeTestamentStats(filename)`: the energy
    /// testament sums; the three "to" sums are cleared afterwards.
    fn stats2_deme_testament_stats(&mut self, filename: &str) -> Result<(), String> {
        let update = self.stats.update();
        let e = &mut self.stats.energy;
        let accepted_by_organisms = self.stats.energy_testament_accepted_by_organisms.average();
        let df = self.out.static_with_path(filename)?;
        df.write_time_stamp();
        df.write(update, "Update");
        df.write(
            e.testament_to_deme_organisms.average(),
            "Energy Testament to Deme Organisms",
        );
        df.write(
            e.testament_to_future_deme.average(),
            "Energy Testament to Future Deme",
        );
        df.write(
            e.testament_to_neighbor_organisms.average(),
            "Energy Testament to Neighbor Organisms",
        );
        df.write(
            e.testament_accepted_by_deme.average(),
            "Energy Testament Accepted by Future Deme",
        );
        df.write(
            accepted_by_organisms,
            "Energy Testament Accepted by Organisms",
        );
        df.endl();
        e.testament_to_deme_organisms.clear();
        e.testament_to_future_deme.clear();
        e.testament_to_neighbor_organisms.clear();
        Ok(())
    }

    /// `cPopulation::PrintCurrentMeanDemeDensity(filename)`
    fn stats2_current_mean_deme_density(&mut self, filename: &str) -> Result<(), String> {
        let mut deme_density = DoubleSum::new();
        for deme in &self.pop.demes {
            deme_density.add(deme.density());
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_time_stamp();
        df.write(update, "Update");
        df.write(deme_density.average(), "Current mean deme density");
        df.endl();
        Ok(())
    }

    /// `cPopulation::PrintDemesMeritsData()`: each deme's `CalcCurMerit()`.
    /// The deme bonus only changes through deme output (`DoDemeOutput`,
    /// not ported: an organism reaching it stops the run), so it is
    /// `DEFAULT_BONUS`.
    fn stats2_demes_merits_data(&mut self) -> Result<(), String> {
        // `BASE_MERIT_METHOD == BASE_MERIT_CONST ? BASE_CONST_MERIT : 1.0`
        let merit_base = if self.cfg.base_merit_method == 0 {
            f64::from(self.cfg.base_const_merit)
        } else {
            1.0
        };
        let merit = Merit::new(merit_base * self.cfg.default_bonus).get_double();
        let num_demes = self.pop.demes.len();
        let update = self.stats.update();
        let df = self.out.static_with_path("deme_merits.dat")?;
        df.write_comment("Each deme's current calculated merit");
        df.write_time_stamp();
        df.write(update, "Update");
        for deme_id in 0..num_demes {
            df.write(merit, &format!("Deme {deme_id}"));
        }
        df.endl();
        Ok(())
    }

    /// `cActionPrintNumOrgsInDeme::Process`
    fn stats2_num_orgs_in_deme(&mut self) -> Result<(), String> {
        let counts: Vec<i32> = self.pop.demes.iter().map(|d| d.cur_org_count).collect();
        let update = self.stats.update();
        let df = self.out.static_with_path("deme_org_count.dat")?;
        df.write_comment("Avida deme resource data");
        df.write_time_stamp();
        df.write_raw(&format!("deme_global_resources_{update:07} = [ ..."));
        for (d, n) in counts.into_iter().enumerate() {
            df.write_block_element(d as i32, 0, 2);
            df.write_block_element(n, 1, 2);
        }
        df.write_raw("];");
        df.endl();
        Ok(())
    }

    /// `cActionSaveDemeFounders::Process` with `cPopulation::DumpDemeFounders`
    fn stats2_save_deme_founders(&mut self, filename: &str) -> Result<(), String> {
        let filename = if filename.is_empty() {
            format!("deme_founders-{}.dat", self.stats.update())
        } else {
            filename.to_string()
        };
        let mut text = String::from(
            "#filetype deme_founders\n#format deme_id num_founders genotype_ids\n\n\
             #  1: Deme ID\n#  2: Number of founders\n#  3+: founder genotype ids\n\n",
        );
        for (i, deme) in self.pop.demes.iter().enumerate() {
            if deme.cur_org_count == 0 {
                continue;
            }
            text.push_str(&format!("{i} {}", deme.founder_genotype_ids.len()));
            for id in &deme.founder_genotype_ids {
                text.push_str(&format!(" {id}"));
            }
            text.push('\n');
        }
        let mut df = self.out.create_with_path(&filename)?;
        df.ofstream()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        df.flush();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTX: ParseContext<'static> = ParseContext {
        default_inst_set: "heads_default",
    };

    fn p(name: &str, args: &str) -> Stats2Action {
        let mut fb = Feedback::new();
        parse(name, args, &CTX, &mut fb).unwrap().unwrap()
    }

    #[test]
    fn pop_out_file_defaults_and_explicit_names() {
        assert_eq!(
            p("PrintPhenotypeData", ""),
            Stats2Action::PhenotypeData {
                filename: "phenotype_count.dat".into(),
                kind: PhenotypeKind::All
            }
        );
        assert_eq!(
            p("PrintHostPhenotypeData", "h.dat x"),
            Stats2Action::PhenotypeData {
                filename: "h.dat".into(),
                kind: PhenotypeKind::Host
            }
        );
        assert_eq!(
            p("PrintParasitePhenotypeData", ""),
            Stats2Action::PhenotypeData {
                filename: "parasite_phenotype_count.dat".into(),
                kind: PhenotypeKind::Parasite
            }
        );
        assert_eq!(
            p("PrintDemeTestamentStats", ""),
            Stats2Action::DemeTestamentStats {
                filename: "deme_testament.dat".into()
            }
        );
        assert_eq!(
            p("PrintCurrentMeanDemeDensity", "d.dat"),
            Stats2Action::CurrentMeanDemeDensity {
                filename: "d.dat".into()
            }
        );
    }

    #[test]
    fn argumentless_and_founder_actions() {
        assert_eq!(
            p("PrintDemesMeritsData", "x"),
            Stats2Action::DemesMeritsData
        );
        assert_eq!(p("PrintNumOrgsInDeme", ""), Stats2Action::NumOrgsInDeme);
        assert_eq!(
            p("SaveDemeFounders", ""),
            Stats2Action::SaveDemeFounders {
                filename: String::new()
            }
        );
        assert_eq!(
            p("SaveDemeFounders", "f.dat"),
            Stats2Action::SaveDemeFounders {
                filename: "f.dat".into()
            }
        );
        let mut fb = Feedback::new();
        assert!(parse("PrintAverageData", "", &CTX, &mut fb).is_none());
    }

    #[test]
    fn every_listed_name_parses() {
        for n in STATS2_ACTIONS {
            let mut fb = Feedback::new();
            assert!(matches!(parse(n, "", &CTX, &mut fb), Some(Ok(_))), "{n}");
            assert!(crate::action_library::is_registered(n), "{n}");
        }
    }

    #[test]
    fn shannon_term_matches_formula() {
        assert_eq!(shannon_term(0.5), 0.5 * 0.5f64.ln() / 2.0f64.ln());
        assert_eq!(shannon_term(1.0), 0.0);
    }
}
