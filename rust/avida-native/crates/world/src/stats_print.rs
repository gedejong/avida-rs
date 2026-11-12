//! The remaining `cStats::Print*` data files of
//! avida-core/source/main/cStats.cc (at 47f13dadb), as a further
//! `impl Stats` block, together with the small `cStats` recording methods
//! that feed the fields they print.
//!
//! Where the C++ printer reads world state (`m_world->GetPopulation()`, the
//! environment, the config) the value comes in as a plain parameter whose
//! doc names the C++ expression it replaces. Where the C++ indexes out of
//! bounds (undefined behaviour) the port returns an error instead.
//!
//! Floating point: none of these printers contains a contractible
//! `a*b + c`; the sums, variances and errors go through `stat_tools` and
//! `avida_apto::Accumulator`, which already fuse where the C++ does.
//!
//! Not ported here (each needs world data beyond a few plain values):
//! `PrintInterruptData` (organism hardware threads), `PrintSoloTaskSnapshot`
//! (test CPU), `PrintResourceLocData` / `PrintResWallLocData` (gradient
//! resource peaks, wall cells), `PrintCellVisitsData`, `PrintCellData`,
//! `PrintCurrentOpinions`, `PrintOpinionsSetPerDeme`, `PrintOrganismLocation`,
//! `PrintCurrentTaskCounts`, `PrintDenData`, `PrintDirectReciprocityData`,
//! `PrintGroupTolerance` / `PrintGroupMTTolerance` (population tolerance
//! calculations), `PrintBirthLocData`, `PrintPredicatedMessages` (predicate
//! objects), `PrintSuccessfulMates` (`cBirthEntry`), the mini/micro/nav
//! trace printers and `PrintReproData`, and the deme printers
//! (`PrintDemeReplicationData` and its treatable/untreatable variants,
//! `PrintDemeGermlineSequestration`, `PrintDemeOrgGermlineSequestration`,
//! `PrintDemeTreatableCount`, `PrintPerDeme*`, `PrintAvg*DemeTasksExeData`,
//! `PrintDeme{Tasks,TasksExe,Reaction,OrgTasks,OrgTasksExe,OrgReaction,CurrentTaskExe}Data`,
//! `PrintDemes{Tasks,Reactions}Data`, `PrintPerDemeGenPerFounderData`,
//! `PrintDemeMigrationSuicidePoints`, `PrintDemeNetworkTopology`,
//! `PrintWinningDeme`, `PrintDemeGermResourcesData`).

use std::collections::{BTreeMap, BTreeSet};

use avida_apto::{Accumulator, DataFile, OutputManager};

use crate::stat_tools::DoubleSum;
use crate::stats::Stats;

/// C++: `s_inst_circumstances` (cStats.h): one tolerance instruction
/// execution and its circumstances.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InstCircumstances {
    pub update: i32,
    pub inst: i32,
    pub gr_id: i32,
    pub gr_size: i32,
    pub res_level: f64,
    pub odds_immigrants: f64,
    pub odds_offspring_own: f64,
    pub odds_offspring_others: f64,
    pub tol_immigrants: i32,
    pub tol_own: i32,
    pub tol_others: i32,
    pub tol_max: i32,
}

/// C++: `flow_rate_tuple` (cStats.h); fields are the C++ members in
/// snake case (`orgCount` is `org_count`, and so on).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FlowRateTuple {
    pub org_count: Accumulator<i32>,
    pub events_killed: Accumulator<i32>,
    pub attempts_to_kill_events: Accumulator<i32>,
    pub avg_energy_usage_ratio: DoubleSum,
    pub total_births: Accumulator<i32>,
    pub current_sleeping: Accumulator<i32>,
}

/// C++: `cStats::message_log_entry_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageLogEntry {
    pub update: i32,
    pub deme: i32,
    pub src_cell: i32,
    pub dst_cell: i32,
    pub transmit_cell: i32,
    pub msg_data: u32,
    pub msg_label: u32,
    pub dropped: bool,
    pub lost: bool,
}

/// C++: `cStats::ConsensusRecord` (`cOrganism::Opinion` is `int`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsensusRecord {
    pub update: i32,
    pub deme_id: i32,
    pub consensus: i32,
    pub cell_id: i32,
}

/// The organism values `PrintReputationData` reads, one record per occupied
/// cell in cell order. Each field names its C++ source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReputationOrgData {
    /// `org->GetReputation()`
    pub reputation: i32,
    /// `org->GetNumberOfDonations()`
    pub num_donations: i32,
    /// `org->GetNumberOfDonationsReceived()`
    pub num_donations_received: i32,
    /// `org->GetNumberOfDonors()`
    pub num_donors: i32,
    /// `org->GetSelfRawMaterials()`
    pub self_raw_materials: i32,
    /// `org->GetOtherRawMaterials()`
    pub other_raw_materials: i32,
}

/// `m_is_tolerance_inst_names` of `PrintToleranceInstructionData` and
/// `PrintToleranceData` (`num_tol_inst` = 8).
const TOLERANCE_INST_NAMES: [&str; 8] = [
    "inc-tolerance_Immigrants",
    "inc-tolerance_OffspringOwn",
    "inc-tolerance_OffspringOthers",
    "dec-tolerance_Immigrants",
    "dec-tolerance_OffspringOwn",
    "dec-tolerance_OffspringOthers",
    "get-tolerance",
    "get-group-tolerance",
];

/// Instructions `SetGroupAttackInstNames` selects.
const GROUP_ATTACK_INSTS: [&str; 8] = [
    "attack-prey",
    "attack-ft-prey",
    "attack-prey-group",
    "attack-prey-share",
    "attack-prey-group-share",
    "attack-prey-no-share",
    "attack-prey-fake-share",
    "attack-prey-fake-group-share",
];

/// `map[inst_set]` read-only. C++ `Apto::Map::operator[]` would insert an
/// empty array for a missing key, which nothing observes.
fn inst_map_entry<'a, T>(map: &'a BTreeMap<String, Vec<T>>, inst_set: &str) -> &'a [T] {
    map.get(inst_set).map(Vec::as_slice).unwrap_or(&[])
}

/// The six-column layout shared by the Prey/Predator/TopPredator and
/// Male/Female `Average`/`Error`/`Variance` printers: comment, time stamp,
/// update, the statistic of fitness, gestation, merit, age, generation and
/// size, then (Average printers of pred/prey only) the entropy column.
fn write_six_sums(
    df: &mut DataFile,
    comment: &str,
    update: i32,
    sums: [&DoubleSum; 6],
    stat: fn(&DoubleSum) -> f64,
    entropy: Option<(f64, &str)>,
) {
    const COLUMNS: [&str; 6] = [
        "Fitness",
        "Gestation Time",
        "Merit",
        "Creature Age",
        "Generation",
        "Genome Length",
    ];
    df.write_comment(comment);
    df.write_time_stamp();
    df.write(update, "Update");
    for (sum, column) in sums.into_iter().zip(COLUMNS) {
        df.write(stat(sum), column);
    }
    if let Some((value, descr)) = entropy {
        df.write(value, descr);
    }
    df.endl();
}

/// Body of the `Print*InstructionData` printers: one column per
/// instruction, the counter's `Sum()` under the instruction's name.
fn write_inst_exe_data(
    out: &mut OutputManager,
    filename: &str,
    comment: &str,
    update: i32,
    num_insts: usize,
    counts: &[Accumulator<i32>],
    names: &[String],
) -> Result<(), String> {
    let df = out.static_with_path(filename)?;
    df.write_comment(comment);
    df.write_time_stamp();
    df.write(update, "Update");
    for i in 0..num_insts {
        let (Some(count), Some(name)) = (counts.get(i), names.get(i)) else {
            return Err(format!(
                "{filename}: no execution counter or name for instruction {i} \
                 (the C++ indexes out of bounds)"
            ));
        };
        df.write(count.sum(), name);
    }
    df.endl();
    Ok(())
}

/// Shared body of `PrintGroupAttackString`, `PrintLookData`,
/// `PrintLookDataOutput` and `PrintLookEXDataOutput`: a fixed file whose
/// header is comment lines, then `update,<string>` rows.
fn write_update_string_row(
    out: &mut OutputManager,
    file: &str,
    update: i32,
    header: &[&str],
    stamp_after: usize,
    string: &str,
) -> Result<(), String> {
    let df = out.static_with_path(file)?;
    if !df.header_done() {
        for (i, comment) in header.iter().enumerate() {
            df.write_comment(comment);
            if i + 1 == stamp_after {
                df.write_time_stamp();
            }
        }
        df.flush_comments();
        df.endl();
    }
    let _ = writeln!(df.ofstream(), "{update},{string}");
    df.flush();
    Ok(())
}

/// Index of the `Print*Targets` printers; `None` where the C++ index would
/// be out of bounds.
fn target_index(index: i32, len: usize) -> Option<usize> {
    usize::try_from(index).ok().filter(|&i| i < len)
}

impl Stats {
    // ------------------------------------------------------------------
    // Recording side (cStats.h inline methods and their cStats.cc bodies)
    // ------------------------------------------------------------------

    /// C++: `AddCurHostTask`
    pub fn add_cur_host_task(&mut self, task_num: usize) {
        self.tasks_host_current[task_num] += 1;
    }

    /// C++: `AddCurParasiteTask`
    pub fn add_cur_parasite_task(&mut self, task_num: usize) {
        self.tasks_parasite_current[task_num] += 1;
    }

    /// C++: `AddLastHostTask`
    pub fn add_last_host_task(&mut self, task_num: usize) {
        self.tasks_host_last[task_num] += 1;
    }

    /// C++: `AddLastParasiteTask`
    pub fn add_last_parasite_task(&mut self, task_num: usize) {
        self.tasks_parasite_last[task_num] += 1;
    }

    /// C++: `IncNumMigrations`
    pub fn inc_num_migrations(&mut self) {
        self.num_migrations += 1;
    }

    /// C++: `SetInstNames` (called by `cHardwareManager::LoadInstSet`).
    pub fn set_inst_names(&mut self, inst_set: &str, names: Vec<String>) {
        self.m_is_inst_names_map.insert(inst_set.to_string(), names);
    }

    /// C++: `InstFromMessageExeCountsForInstSet` (inserts on first use).
    pub fn inst_from_message_exe_counts_for_inst_set(
        &mut self,
        inst_set: &str,
    ) -> &mut Vec<Accumulator<i32>> {
        // Looked up before inserting: called for every organism every update.
        if !self.m_is_from_message_inst_map.contains_key(inst_set) {
            self.m_is_from_message_inst_map
                .insert(inst_set.to_string(), Vec::new());
        }
        self.m_is_from_message_inst_map
            .get_mut(inst_set)
            .expect("inserted above")
    }

    /// C++: `InstPreyExeCountsForInstSet`
    pub fn inst_prey_exe_counts_for_inst_set(
        &mut self,
        inst_set: &str,
    ) -> &mut Vec<Accumulator<i32>> {
        self.m_is_prey_exe_inst_map
            .entry(inst_set.to_string())
            .or_default()
    }

    /// C++: `InstPredExeCountsForInstSet`
    pub fn inst_pred_exe_counts_for_inst_set(
        &mut self,
        inst_set: &str,
    ) -> &mut Vec<Accumulator<i32>> {
        self.m_is_pred_exe_inst_map
            .entry(inst_set.to_string())
            .or_default()
    }

    /// C++: `InstTopPredExeCountsForInstSet`
    pub fn inst_top_pred_exe_counts_for_inst_set(
        &mut self,
        inst_set: &str,
    ) -> &mut Vec<Accumulator<i32>> {
        self.m_is_tpred_exe_inst_map
            .entry(inst_set.to_string())
            .or_default()
    }

    /// C++: `InstPreyFromSensorExeCountsForInstSet`
    pub fn inst_prey_from_sensor_exe_counts_for_inst_set(
        &mut self,
        inst_set: &str,
    ) -> &mut Vec<Accumulator<i32>> {
        self.m_is_prey_from_sensor_inst_map
            .entry(inst_set.to_string())
            .or_default()
    }

    /// C++: `InstPredFromSensorExeCountsForInstSet`
    pub fn inst_pred_from_sensor_exe_counts_for_inst_set(
        &mut self,
        inst_set: &str,
    ) -> &mut Vec<Accumulator<i32>> {
        self.m_is_pred_from_sensor_inst_map
            .entry(inst_set.to_string())
            .or_default()
    }

    /// C++: `InstTopPredFromSensorExeCountsForInstSet`
    pub fn inst_top_pred_from_sensor_exe_counts_for_inst_set(
        &mut self,
        inst_set: &str,
    ) -> &mut Vec<Accumulator<i32>> {
        self.m_is_tpred_from_sensor_inst_map
            .entry(inst_set.to_string())
            .or_default()
    }

    /// C++: `InstMaleExeCountsForInstSet`
    pub fn inst_male_exe_counts_for_inst_set(
        &mut self,
        inst_set: &str,
    ) -> &mut Vec<Accumulator<i32>> {
        self.m_is_male_exe_inst_map
            .entry(inst_set.to_string())
            .or_default()
    }

    /// C++: `InstFemaleExeCountsForInstSet`
    pub fn inst_female_exe_counts_for_inst_set(
        &mut self,
        inst_set: &str,
    ) -> &mut Vec<Accumulator<i32>> {
        self.m_is_female_exe_inst_map
            .entry(inst_set.to_string())
            .or_default()
    }

    /// C++: `ExecCountsForGroupAttackInstSet`
    pub fn exec_counts_for_group_attack_inst_set(
        &mut self,
        inst_set: &str,
    ) -> &mut BTreeMap<String, Vec<Accumulator<i32>>> {
        self.m_group_attack_exe_map
            .entry(inst_set.to_string())
            .or_default()
    }

    /// C++: `ExecCountsForGroupAttackInst`
    pub fn exec_counts_for_group_attack_inst(
        &mut self,
        inst_set: &str,
        inst: &str,
    ) -> &mut Vec<Accumulator<i32>> {
        self.exec_counts_for_group_attack_inst_set(inst_set)
            .entry(inst.to_string())
            .or_default()
    }

    /// C++: `GetGroupAttackInsts`
    pub fn group_attack_insts(&self, inst_set: &str) -> &[String] {
        inst_map_entry(&self.m_group_attack_names, inst_set)
    }

    /// C++: `SetGroupAttackInstNames`: the attack instructions of the
    /// instruction set, in instruction order.
    pub fn set_group_attack_inst_names(&mut self, inst_set: &str) {
        let names: Vec<String> = self
            .m_is_inst_names_map
            .entry(inst_set.to_string())
            .or_default()
            .iter()
            .filter(|inst| GROUP_ATTACK_INSTS.contains(&inst.as_str()))
            .cloned()
            .collect();
        self.m_group_attack_names
            .insert(inst_set.to_string(), names);
    }

    /// C++: `ZeroMessageInst`
    pub fn zero_message_inst(&mut self) {
        for counts in self.m_is_from_message_inst_map.values_mut() {
            counts.iter_mut().for_each(Accumulator::clear);
        }
    }

    /// C++: `ZeroFTInst`. QUIRK: `m_is_prey_fail_exe_inst_map` is not
    /// cleared (the C++ lists the pred-fail map but not the prey-fail one).
    pub fn zero_ft_inst(&mut self) {
        for map in [
            &mut self.m_is_prey_exe_inst_map,
            &mut self.m_is_pred_exe_inst_map,
            &mut self.m_is_pred_fail_exe_inst_map,
            &mut self.m_is_tpred_exe_inst_map,
            &mut self.m_is_prey_from_sensor_inst_map,
            &mut self.m_is_pred_from_sensor_inst_map,
            &mut self.m_is_tpred_from_sensor_inst_map,
        ] {
            for counts in map.values_mut() {
                counts.iter_mut().for_each(Accumulator::clear);
            }
        }
    }

    /// C++: `ZeroGroupAttackInst`
    pub fn zero_group_attack_inst(&mut self) {
        for per_inst in self.m_group_attack_exe_map.values_mut() {
            for counts in per_inst.values_mut() {
                counts.iter_mut().for_each(Accumulator::clear);
            }
        }
    }

    /// C++: `ZeroMTInst`
    pub fn zero_mt_inst(&mut self) {
        for map in [
            &mut self.m_is_male_exe_inst_map,
            &mut self.m_is_female_exe_inst_map,
        ] {
            for counts in map.values_mut() {
                counts.iter_mut().for_each(Accumulator::clear);
            }
        }
    }

    /// The `if (GetSize() != num_tol_inst) Resize(num_tol_inst)` guard:
    /// `Apto::Array::Resize` keeps the prefix and value-initialises new
    /// `pair<int,int>`s to `(0, 0)`.
    fn size_tolerance_exe_counts(&mut self) {
        if self.m_is_tolerance_exe_counts.len() != TOLERANCE_INST_NAMES.len() {
            self.m_is_tolerance_exe_counts
                .resize(TOLERANCE_INST_NAMES.len(), (0, 0));
        }
    }

    /// `m_is_tolerance_exe_counts[tol_inst]` bump shared by both
    /// `PushToleranceInstExe` overloads.
    fn count_tolerance_inst(&mut self, tol_inst: usize) {
        let update = self.m_update;
        let slot = &mut self.m_is_tolerance_exe_counts[tol_inst];
        if slot.0 == update {
            slot.1 += 1;
        } else {
            *slot = (update, 1);
        }
    }

    /// C++: `PushToleranceInstExe(int tol_inst)` (tolerance tracking off).
    /// `tol_inst` is 0..8, an index into the instruction names.
    pub fn push_tolerance_inst_exe(&mut self, tol_inst: usize) {
        self.size_tolerance_exe_counts();
        self.count_tolerance_inst(tol_inst);
    }

    /// C++: `PushToleranceInstExe(tol_inst, group_id, ...)` (with
    /// `TRACK_TOLERANCE`): counts the execution and records its
    /// circumstances, dropping the records of earlier updates first.
    #[allow(clippy::too_many_arguments)]
    pub fn push_tolerance_inst_exe_circumstances(
        &mut self,
        tol_inst: usize,
        group_id: i32,
        group_size: i32,
        resource_level: f64,
        odds_immi: f64,
        odds_own: f64,
        odds_others: f64,
        tol_immi: i32,
        tol_own: i32,
        tol_others: i32,
        tol_max: i32,
    ) {
        self.size_tolerance_exe_counts();
        if self
            .m_is_tolerance_exe_insts
            .first()
            .is_some_and(|c| c.update != self.m_update)
        {
            self.m_is_tolerance_exe_insts.clear();
        }
        self.count_tolerance_inst(tol_inst);
        self.m_is_tolerance_exe_insts.push(InstCircumstances {
            update: self.m_update,
            inst: tol_inst as i32,
            gr_id: group_id,
            gr_size: group_size,
            res_level: resource_level,
            odds_immigrants: odds_immi,
            odds_offspring_own: odds_own,
            odds_offspring_others: odds_others,
            tol_immigrants: tol_immi,
            tol_own,
            tol_others,
            tol_max,
        });
    }

    /// C++: `ZeroToleranceInst`. QUIRK: the C++ writes slots 0..8 without
    /// sizing the array first (out of bounds before the first push; it has
    /// no callers at 47f13dadb); only existing slots are reset here.
    pub fn zero_tolerance_inst(&mut self) {
        for slot in self
            .m_is_tolerance_exe_counts
            .iter_mut()
            .take(TOLERANCE_INST_NAMES.len())
        {
            *slot = (-1, -1);
        }
        self.m_is_tolerance_exe_insts.clear();
    }

    /// C++: `IncKaboom`
    pub fn inc_kaboom(&mut self) {
        self.num_kabooms += 1;
    }

    /// C++: `IncKaboomPreDivide`
    pub fn inc_kaboom_pre_divide(&mut self) {
        self.num_kabooms_pre += 1;
    }

    /// C++: `IncKaboomPostDivide`
    pub fn inc_kaboom_post_divide(&mut self) {
        self.num_kabooms_post += 1;
    }

    /// C++: `IncKaboomKills`
    pub fn inc_kaboom_kills(&mut self) {
        self.num_kaboom_kills += 1;
    }

    /// C++: `IncSAKin`
    pub fn inc_sa_kin(&mut self, num: i32) {
        self.num_sa_kin = self.num_sa_kin.wrapping_add(num);
    }

    /// C++: `IncSANotKin`
    pub fn inc_sa_not_kin(&mut self, num: i32) {
        self.num_sa_notkin = self.num_sa_notkin.wrapping_add(num);
    }

    /// C++: `IncNSAKin`
    pub fn inc_nsa_kin(&mut self, num: i32) {
        self.num_nsa_kin = self.num_nsa_kin.wrapping_add(num);
    }

    /// C++: `IncNSANotKin`
    pub fn inc_nsa_not_kin(&mut self, num: i32) {
        self.num_nsa_notkin = self.num_nsa_notkin.wrapping_add(num);
    }

    /// C++: `AddHamDistance`
    pub fn add_ham_distance(&mut self, distance: i32) {
        self.hd_list.push(distance);
    }

    /// C++: `IncPercLyse`
    pub fn inc_perc_lyse(&mut self, perc: f64) {
        self.sum_perc_lyse += perc;
    }

    /// C++: `IncSumCPUs`
    pub fn inc_sum_cpus(&mut self, cpu_cycles: i32) {
        self.sum_cpu_cycles += f64::from(cpu_cycles);
    }

    /// C++: `IncDontExplode`
    pub fn inc_dont_explode(&mut self) {
        self.num_stop_explode += 1;
    }

    /// C++: `IncQuorumThresholdUB`
    pub fn inc_quorum_threshold_ub(&mut self, thresh: i32) {
        self.ave_threshold_ub = self.ave_threshold_ub.wrapping_add(thresh);
    }

    /// C++: `IncQuorumNum`
    pub fn inc_quorum_num(&mut self) {
        self.num_quorum += 1;
    }

    /// C++: `IncJuvKilled`
    pub fn inc_juv_killed(&mut self) {
        self.juv_killed += 1;
    }

    /// C++: `IncGuardFail`
    pub fn inc_guard_fail(&mut self) {
        self.num_guard_fail += 1;
    }

    /// C++: `SetCompetitionTrialFitnesses`
    pub fn set_competition_trial_fitnesses(&mut self, fitnesses: Vec<f64>) {
        self.avg_trial_fitnesses = fitnesses;
    }

    /// C++: `SetCompetitionFitnesses`
    pub fn set_competition_fitnesses(
        &mut self,
        avg: f64,
        min: f64,
        max: f64,
        cp_avg: f64,
        cp_min: f64,
        cp_max: f64,
    ) {
        self.avg_competition_fitness = avg;
        self.min_competition_fitness = min;
        self.max_competition_fitness = max;
        self.avg_competition_copied_fitness = cp_avg;
        self.min_competition_copied_fitness = cp_min;
        self.max_competition_copied_fitness = cp_max;
    }

    /// C++: `SetCompetitionOrgsReplicated`
    pub fn set_competition_orgs_replicated(&mut self, num: i32) {
        self.num_orgs_replicated = num;
    }

    /// C++: `AddNumOrgsKilled(long)`; the `long` is converted to the
    /// accumulator's `int` (truncating, as the C++ implicit conversion).
    pub fn add_num_orgs_killed(&mut self, num: i64) {
        self.sum_orgs_killed.add(num as i32);
    }

    /// C++: `AddNumUnoccupiedCellAttemptedToKill(long)`
    pub fn add_num_unoccupied_cell_attempted_to_kill(&mut self, num: i64) {
        self.sum_unoccupied_cell_kill_attempts.add(num as i32);
    }

    /// C++: `AddNumCellsScannedAtKill(long)`
    pub fn add_num_cells_scanned_at_kill(&mut self, num: i64) {
        self.sum_cells_scanned_at_kill.add(num as i32);
    }

    /// C++: `IncNumOccupiedDemes`
    pub fn inc_num_occupied_demes(&mut self) {
        self.m_num_occupied_demes += 1;
    }

    /// C++: `ClearNumOccupiedDemes`
    pub fn clear_num_occupied_demes(&mut self) {
        self.m_num_occupied_demes = 0;
    }

    /// C++: `GenomeFragmentMetabolized`; `fragment_size` is
    /// `fragment.GetSize()`.
    pub fn genome_fragment_metabolized(&mut self, fragment_size: i32) {
        self.m_hgt_metabolized.add(f64::from(fragment_size));
    }

    /// C++: `GenomeFragmentInserted`; `fragment_size` is
    /// `fragment.GetSize()`.
    pub fn genome_fragment_inserted(&mut self, fragment_size: i32) {
        self.m_hgt_inserted.add(f64::from(fragment_size));
    }

    /// C++: `LogMessage(msg, dropped, lost)`. Arguments: `deme` is
    /// `msg.GetSender()->GetDeme()->GetID()`, `src_cell`
    /// `msg.GetSenderCellID()`, `dst_cell` `msg.GetReceiverCellID()`,
    /// `transmit_cell` `msg.GetTransCellID()`, `msg_data` `msg.GetData()`,
    /// `msg_label` `msg.GetLabel()`.
    #[allow(clippy::too_many_arguments)]
    pub fn log_message(
        &mut self,
        deme: i32,
        src_cell: i32,
        dst_cell: i32,
        transmit_cell: i32,
        msg_data: u32,
        msg_label: u32,
        dropped: bool,
        lost: bool,
    ) {
        self.m_message_log.push(MessageLogEntry {
            update: self.m_update,
            deme,
            src_cell,
            dst_cell,
            transmit_cell,
            msg_data,
            msg_label,
            dropped,
            lost,
        });
    }

    /// C++: `LogRetMessage(msg)`: the sender's deme is recorded as 0 and
    /// `dropped`/`lost` as false. Arguments as for [`Stats::log_message`].
    pub fn log_ret_message(
        &mut self,
        src_cell: i32,
        dst_cell: i32,
        transmit_cell: i32,
        msg_data: u32,
        msg_label: u32,
    ) {
        self.m_retmessage_log.push(MessageLogEntry {
            update: self.m_update,
            deme: 0,
            src_cell,
            dst_cell,
            transmit_cell,
            msg_data,
            msg_label,
            dropped: false,
            lost: false,
        });
    }

    /// C++: `NetworkTopology`
    pub fn network_topology(&mut self, ns: &BTreeMap<String, f64>) {
        for (name, &value) in ns {
            self.m_network_stats
                .entry(name.clone())
                .or_default()
                .add(value);
        }
    }

    /// C++: `OutgoingMigrant`
    pub fn outgoing_migrant(&mut self) {
        self.m_outgoing.add(1.0);
    }

    /// C++: `IncomingMigrant`
    pub fn incoming_migrant(&mut self) {
        self.m_incoming.add(1.0);
    }

    /// C++: `ProfilingData`
    pub fn profiling_data(&mut self, pf: &BTreeMap<String, f64>) {
        for (name, &value) in pf {
            self.m_profiling.entry(name.clone()).or_default().add(value);
        }
    }

    /// C++: `SentFlash(organism)`. `deme_cell` is `None` when
    /// `organism.GetOrgInterface().GetDeme()` is null, otherwise
    /// `(deme->GetID(), deme->GetRelativeCellID(organism.GetCellID()))`.
    pub fn sent_flash(&mut self, deme_cell: Option<(i32, i32)>) {
        self.m_flash_count += 1;
        if let Some((deme_id, rel_cell_id)) = deme_cell {
            self.m_flash_times
                .entry(self.m_update)
                .or_default()
                .entry(deme_id)
                .or_default()
                .push(rel_cell_id);
        }
    }

    /// C++: `ConsensusReached(deme, consensus, cellid)`; `deme_id` is
    /// `deme.GetID()`.
    pub fn consensus_reached(&mut self, deme_id: i32, consensus: i32, cell_id: i32) {
        self.m_consensi.push(ConsensusRecord {
            update: self.m_update,
            deme_id,
            consensus,
            cell_id,
        });
    }

    /// C++: `CompeteDemes`
    pub fn compete_demes(&mut self, fitness: &[f64]) {
        self.m_deme_fitness = fitness.to_vec();
    }

    /// C++: `GermlineReplication(source_germline, _)`; `germline_size` is
    /// `source_germline.Size()`.
    pub fn germline_replication(&mut self, germline_size: i32) {
        self.m_germline_generation.add(f64::from(germline_size));
    }

    /// C++: `DemePostReplication(_, target_deme)`: `target_deme_id` is
    /// `target_deme.GetID()`, `genotype_ids` `target_deme.GetGenotypeIDs()`.
    pub fn deme_post_replication(&mut self, target_deme_id: i32, genotype_ids: Vec<i32>) {
        self.m_deme_founders.insert(target_deme_id, genotype_ids);
    }

    /// C++: `TrackDemeGLSReplication`
    pub fn track_deme_gls_replication(
        &mut self,
        source_deme_id: i32,
        target_deme_id: i32,
        founders: Vec<(i32, String)>,
    ) {
        self.m_gls_deme_founders
            .insert((source_deme_id, target_deme_id), founders);
    }

    /// C++: `IncDemeReactionDiversityReplicationData`
    pub fn inc_deme_reaction_diversity_replication_data(
        &mut self,
        deme_div: f64,
        switch_pen: f64,
        shannon_div: f64,
        num_orgs_perf_reaction: f64,
        per_repro: f64,
    ) {
        self.m_switching.push_back(switch_pen);
        self.m_deme_diversity.push_back(deme_div);
        self.m_shannon_div.push_back(shannon_div);
        self.m_num_orgs_perf_reaction
            .push_back(num_orgs_perf_reaction);
        let norm_shan = shannon_div / num_orgs_perf_reaction.ln();
        self.m_shannon_div_norm.push_back(norm_shan);
        self.m_percent_reproductives.push_back(per_repro);
    }

    /// C++: `IncDonateToDonor`
    pub fn inc_donate_to_donor(&mut self) {
        self.m_donate_to_donor += 1;
    }

    /// C++: `IncDonateToFacing`
    pub fn inc_donate_to_facing(&mut self) {
        self.m_donate_to_facing += 1;
    }

    /// C++: `AddStringBitsMatchedValue`
    pub fn add_string_bits_matched_value(&mut self, name: &str, value: i32) {
        self.m_string_bits_matched
            .entry(name.to_string())
            .or_default()
            .add(f64::from(value));
    }

    /// C++: `AddTag`
    pub fn add_tag(&mut self, tag: i32, value: i32) {
        let slot = self.m_tags.entry(tag).or_insert(0);
        *slot = slot.wrapping_add(value);
    }

    /// C++: `IncPerfectMatch`
    pub fn inc_perfect_match(&mut self, amount: i32) {
        self.m_perfect_match.add(f64::from(amount));
    }

    /// C++: `IncPerfectMatchOrg`
    pub fn inc_perfect_match_org(&mut self) {
        self.m_perfect_match_org.add(1.0);
    }

    // ------------------------------------------------------------------
    // Data files
    // ------------------------------------------------------------------

    /// C++: `PrintErrorData`
    pub fn print_error_data(&self, out: &mut OutputManager, filename: &str) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida Standard Error Data");
        df.write_time_stamp();

        df.write(self.m_update, "Update");
        df.write(self.sum_merit.std_error(), "Merit");
        df.write(self.sum_gestation.std_error(), "Gestation Time");
        df.write(self.sum_fitness.std_error(), "Fitness");
        df.write(self.sum_repro_rate.std_error(), "Repro Rate?");
        df.write(0, "Size");
        df.write(self.sum_copy_size.std_error(), "Copied Size");
        df.write(self.sum_exe_size.std_error(), "Executed Size");
        df.write(0, "(deprecated) Abundance");
        df.write(-1, "(deprecated)");
        df.write(-1, "(deprecated)");
        df.write(0, "(deprecated) Genotype Depth");
        df.write(self.sum_generation.std_error(), "Generation");
        df.write(self.sum_neutral_metric.std_error(), "Neutral Metric");
        df.write(self.sum_lineage_label.std_error(), "Lineage Label");
        df.write(
            self.rave_true_replication_rate.std_error(),
            "True Replication Rate (based on births/update, time-averaged)",
        );
        df.endl();
        Ok(())
    }

    /// C++: `PrintVarianceData`
    pub fn print_variance_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida Variance Data");
        df.write_time_stamp();

        df.write(self.m_update, "Update");
        df.write(self.sum_merit.variance(), "Merit");
        df.write(self.sum_gestation.variance(), "Gestation Time");
        df.write(self.sum_fitness.variance(), "Fitness");
        df.write(self.sum_repro_rate.variance(), "Repro Rate?");
        df.write(0, "(deprecated) Size");
        df.write(self.sum_copy_size.variance(), "Copied Size");
        df.write(self.sum_exe_size.variance(), "Executed Size");
        df.write(0, "(deprecated) Abundance");
        df.write(-1, "(deprecated)");
        df.write(-1, "(deprecated)");
        df.write(0, "(deprecated) Genotype Depth");
        df.write(self.sum_generation.variance(), "Generation");
        df.write(self.sum_neutral_metric.variance(), "Neutral Metric");
        df.write(self.sum_lineage_label.variance(), "Lineage Label");
        df.write(
            self.rave_true_replication_rate.variance(),
            "True Replication Rate (based on births/update, time-averaged)",
        );
        df.endl();
        Ok(())
    }

    /// C++: `PrintParasiteData`
    pub fn print_parasite_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida Dominant Parasite Data");
        df.write_time_stamp();
        df.write(self.m_update, "Update");
        df.write(self.num_parasites, "Number of Extant Parasites");
        df.endl();
        Ok(())
    }

    /// C++: `PrintPreyAverageData`
    pub fn print_prey_average_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Prey Average Data",
            self.m_update,
            self.prey_sums(),
            DoubleSum::average,
            Some((self.prey_entropy, "Total Prey Genotypic Entropy")),
        );
        Ok(())
    }

    /// C++: `PrintPredatorAverageData`
    pub fn print_predator_average_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Predator Average Data",
            self.m_update,
            self.pred_sums(),
            DoubleSum::average,
            Some((self.pred_entropy, "Total Predator Genotypic Entropy")),
        );
        Ok(())
    }

    /// C++: `PrintTopPredatorAverageData`
    pub fn print_top_predator_average_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Top predator Average Data",
            self.m_update,
            self.tpred_sums(),
            DoubleSum::average,
            Some((self.tpred_entropy, "Total Top Predator Genotypic Entropy")),
        );
        Ok(())
    }

    /// C++: `PrintPreyErrorData`
    pub fn print_prey_error_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Prey Standard Error Data",
            self.m_update,
            self.prey_sums(),
            DoubleSum::std_error,
            None,
        );
        Ok(())
    }

    /// C++: `PrintPredatorErrorData`
    pub fn print_predator_error_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Predator Standard Error Data",
            self.m_update,
            self.pred_sums(),
            DoubleSum::std_error,
            None,
        );
        Ok(())
    }

    /// C++: `PrintTopPredatorErrorData`
    pub fn print_top_predator_error_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Top predator Standard Error Data",
            self.m_update,
            self.tpred_sums(),
            DoubleSum::std_error,
            None,
        );
        Ok(())
    }

    /// C++: `PrintPreyVarianceData`
    pub fn print_prey_variance_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Prey Variance Data",
            self.m_update,
            self.prey_sums(),
            DoubleSum::variance,
            None,
        );
        Ok(())
    }

    /// C++: `PrintPredatorVarianceData`
    pub fn print_predator_variance_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Predator Variance Data",
            self.m_update,
            self.pred_sums(),
            DoubleSum::variance,
            None,
        );
        Ok(())
    }

    /// C++: `PrintTopPredatorVarianceData`
    pub fn print_top_predator_variance_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Top predator Variance Data",
            self.m_update,
            self.tpred_sums(),
            DoubleSum::variance,
            None,
        );
        Ok(())
    }

    /// C++: `PrintMaleAverageData`
    pub fn print_male_average_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Male Average Data",
            self.m_update,
            self.male_sums(),
            DoubleSum::average,
            None,
        );
        Ok(())
    }

    /// C++: `PrintFemaleAverageData`
    pub fn print_female_average_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Female Average Data",
            self.m_update,
            self.female_sums(),
            DoubleSum::average,
            None,
        );
        Ok(())
    }

    /// C++: `PrintMaleErrorData`
    pub fn print_male_error_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Male Standard Error Data",
            self.m_update,
            self.male_sums(),
            DoubleSum::std_error,
            None,
        );
        Ok(())
    }

    /// C++: `PrintFemaleErrorData`
    pub fn print_female_error_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Female Standard Error Data",
            self.m_update,
            self.female_sums(),
            DoubleSum::std_error,
            None,
        );
        Ok(())
    }

    /// C++: `PrintMaleVarianceData`
    pub fn print_male_variance_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Male Variance Data",
            self.m_update,
            self.male_sums(),
            DoubleSum::variance,
            None,
        );
        Ok(())
    }

    /// C++: `PrintFemaleVarianceData`
    pub fn print_female_variance_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        write_six_sums(
            out.static_with_path(filename)?,
            "Female Variance Data",
            self.m_update,
            self.female_sums(),
            DoubleSum::variance,
            None,
        );
        Ok(())
    }

    fn prey_sums(&self) -> [&DoubleSum; 6] {
        [
            &self.sum_prey_fitness,
            &self.sum_prey_gestation,
            &self.sum_prey_merit,
            &self.sum_prey_creature_age,
            &self.sum_prey_generation,
            &self.sum_prey_size,
        ]
    }

    fn pred_sums(&self) -> [&DoubleSum; 6] {
        [
            &self.sum_pred_fitness,
            &self.sum_pred_gestation,
            &self.sum_pred_merit,
            &self.sum_pred_creature_age,
            &self.sum_pred_generation,
            &self.sum_pred_size,
        ]
    }

    fn tpred_sums(&self) -> [&DoubleSum; 6] {
        [
            &self.sum_tpred_fitness,
            &self.sum_tpred_gestation,
            &self.sum_tpred_merit,
            &self.sum_tpred_creature_age,
            &self.sum_tpred_generation,
            &self.sum_tpred_size,
        ]
    }

    fn male_sums(&self) -> [&DoubleSum; 6] {
        [
            &self.sum_male_fitness,
            &self.sum_male_gestation,
            &self.sum_male_merit,
            &self.sum_male_creature_age,
            &self.sum_male_generation,
            &self.sum_male_size,
        ]
    }

    fn female_sums(&self) -> [&DoubleSum; 6] {
        [
            &self.sum_female_fitness,
            &self.sum_female_gestation,
            &self.sum_female_merit,
            &self.sum_female_creature_age,
            &self.sum_female_generation,
            &self.sum_female_size,
        ]
    }

    /// C++: `PrintPreyInstructionData`
    pub fn print_prey_instruction_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        let counts = inst_map_entry(&self.m_is_prey_exe_inst_map, inst_set);
        write_inst_exe_data(
            out,
            filename,
            "Prey org instruction execution data",
            self.m_update,
            counts.len(),
            counts,
            inst_map_entry(&self.m_is_inst_names_map, inst_set),
        )
    }

    /// C++: `PrintPredatorInstructionData`
    pub fn print_predator_instruction_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        let counts = inst_map_entry(&self.m_is_pred_exe_inst_map, inst_set);
        write_inst_exe_data(
            out,
            filename,
            "Predator org instruction execution data",
            self.m_update,
            counts.len(),
            counts,
            inst_map_entry(&self.m_is_inst_names_map, inst_set),
        )
    }

    /// C++: `PrintTopPredatorInstructionData`
    pub fn print_top_predator_instruction_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        let counts = inst_map_entry(&self.m_is_tpred_exe_inst_map, inst_set);
        write_inst_exe_data(
            out,
            filename,
            "Top predator org instruction execution data",
            self.m_update,
            counts.len(),
            counts,
            inst_map_entry(&self.m_is_inst_names_map, inst_set),
        )
    }

    /// C++: `PrintPreyFromSensorInstructionData`
    pub fn print_prey_from_sensor_instruction_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        let counts = inst_map_entry(&self.m_is_prey_from_sensor_inst_map, inst_set);
        write_inst_exe_data(
            out,
            filename,
            "Prey org instruction execution data using values originating from sensory input",
            self.m_update,
            counts.len(),
            counts,
            inst_map_entry(&self.m_is_inst_names_map, inst_set),
        )
    }

    /// C++: `PrintPredatorFromSensorInstructionData`
    pub fn print_predator_from_sensor_instruction_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        let counts = inst_map_entry(&self.m_is_pred_from_sensor_inst_map, inst_set);
        write_inst_exe_data(
            out,
            filename,
            "Predator org instruction execution data using values originating from sensory input",
            self.m_update,
            counts.len(),
            counts,
            inst_map_entry(&self.m_is_inst_names_map, inst_set),
        )
    }

    /// C++: `PrintTopPredatorFromSensorInstructionData`
    pub fn print_top_predator_from_sensor_instruction_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        let counts = inst_map_entry(&self.m_is_tpred_from_sensor_inst_map, inst_set);
        write_inst_exe_data(
            out,
            filename,
            "Top predator org instruction execution data using values originating from sensory input",
            self.m_update,
            counts.len(),
            counts,
            inst_map_entry(&self.m_is_inst_names_map, inst_set),
        )
    }

    /// C++: `PrintMaleInstructionData`. QUIRK: the column count comes from
    /// the *predator* map (`m_is_pred_exe_inst_map[inst_set].GetSize()`).
    pub fn print_male_instruction_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        write_inst_exe_data(
            out,
            filename,
            "Male instruction execution data",
            self.m_update,
            inst_map_entry(&self.m_is_pred_exe_inst_map, inst_set).len(),
            inst_map_entry(&self.m_is_male_exe_inst_map, inst_set),
            inst_map_entry(&self.m_is_inst_names_map, inst_set),
        )
    }

    /// C++: `PrintFemaleInstructionData`. QUIRK: as
    /// [`Stats::print_male_instruction_data`], sized by the predator map.
    pub fn print_female_instruction_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        write_inst_exe_data(
            out,
            filename,
            "Female instruction execution data",
            self.m_update,
            inst_map_entry(&self.m_is_pred_exe_inst_map, inst_set).len(),
            inst_map_entry(&self.m_is_female_exe_inst_map, inst_set),
            inst_map_entry(&self.m_is_inst_names_map, inst_set),
        )
    }

    /// C++: `PrintTotalsData`
    pub fn print_totals_data(&self, out: &mut OutputManager, filename: &str) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write(self.m_update, "Update");
        // QUIRK: the C++ `tot_executed` is an `int` that wraps; the Rust
        // field is 64-bit, so its low 32 bits are the C++ value.
        df.write(
            (self.tot_executed as i32).wrapping_add(self.num_executed),
            "Total Instructions Executed",
        );
        df.write(self.num_executed, "Instructions Executed This Update");
        df.write(self.tot_organisms, "Total Organisms");
        df.write(0, "(deprecated) Total Genotypes");
        df.write(0, "(deprecated) Total Threshold");
        df.write(0, "(deprecated) Total Species");
        df.write(0, "(deprecated) Total Lineages");
        df.endl();
        Ok(())
    }

    /// C++: `PrintThreadsData`. `num_live_orgs` is
    /// `m_world->GetPopulation().GetLiveOrgList().GetSize()`.
    pub fn print_threads_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        num_live_orgs: i32,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write(self.m_update, "Update");
        df.write(self.tot_organisms, "Total Organisms");
        df.write(num_live_orgs, "Total Living Organisms");
        df.write(self.m_num_threads, "Total Living Org Threads");
        df.endl();
        Ok(())
    }

    /// C++: `PrintMessageData`. `deme_messages[i]` is
    /// `(GetMessageSuccessfullySent(), GetMessageDropped(),
    /// GetMessageSendFailed())` of `pop.GetDeme(i)`; the totals are
    /// `unsigned int` and wrap.
    pub fn print_message_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        deme_messages: &[(u32, u32, u32)],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Number of organism to organism messages\n");
        df.write(self.m_update, "update");
        let mut total_sent = 0u32;
        let mut total_dropped = 0u32;
        let mut total_failed = 0u32;
        for &(sent, dropped, failed) in deme_messages {
            total_sent = total_sent.wrapping_add(sent);
            total_dropped = total_dropped.wrapping_add(dropped);
            total_failed = total_failed.wrapping_add(failed);
        }
        df.write(total_sent, "Sent successfully");
        df.write(total_dropped, "Dropped");
        df.write(total_failed, "Failed");
        df.endl();
        Ok(())
    }

    /// C++: `PrintHostTasksData`. The file name `tasksq.dat` is replaced by
    /// `host_tasks.dat` (no quality file is printed).
    pub fn print_host_tasks_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let file = if filename == "tasksq.dat" {
            "host_tasks.dat"
        } else {
            filename
        };
        let df = out.static_with_path(file)?;
        df.write_comment("Avida Host Tasks data");
        df.write_time_stamp();
        df.write_comment("First column gives the current update, next columns give the number");
        df.write_comment("of Hosts that have the particular task");

        df.write(self.m_update, "Update");
        for (i, &count) in self.tasks_host_last.iter().enumerate() {
            df.write(count, &self.task_names[i]);
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintParasiteTasksData`. The file name `tasksq.dat` is replaced
    /// by `parasite_tasks.dat`.
    pub fn print_parasite_tasks_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let file = if filename == "tasksq.dat" {
            "parasite_tasks.dat"
        } else {
            filename
        };
        let df = out.static_with_path(file)?;
        df.write_comment("Avida tasks data");
        df.write_time_stamp();
        df.write_comment("First column gives the current update, next columns give the number");
        df.write_comment("of Parasites that have the particular task");

        df.write(self.m_update, "Update");
        for (i, &count) in self.tasks_parasite_last.iter().enumerate() {
            df.write(count, &self.task_names[i]);
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintNewTasksData`; zeroes `new_task_count` afterwards.
    pub fn print_new_tasks_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida new tasks data");
        df.write_time_stamp();
        df.write_comment(
            "First column gives the current update, all further columns give the number",
        );
        df.write_comment(
            "of times the particular task has newly evolved since the last time printed.",
        );

        df.write(self.m_update, "Update");
        for (i, &count) in self.new_task_count.iter().enumerate() {
            df.write(count, &self.task_names[i]);
        }
        df.endl();
        self.new_task_count.fill(0);
        Ok(())
    }

    /// C++: `PrintNewTasksDataPlus`; zeroes the new/prev/cur task counts
    /// afterwards.
    pub fn print_new_tasks_data_plus(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida new tasks data");
        df.write_time_stamp();
        df.write_comment("First column gives the current update, all further columns are in sets of 3, giving the number");
        df.write_comment("of times the particular task has newly evolved since the last time printed, then the average");
        df.write_comment("number of tasks the parent of the organism evolving the new task performed, then the average");
        df.write_comment(
            "number of tasks the organism evolving the new task performed.  One set of 3 for each task",
        );

        df.write(self.m_update, "Update");
        for (i, &new_count) in self.new_task_count.iter().enumerate() {
            df.write(
                new_count,
                &format!("{} - num times newly evolved", self.task_names[i]),
            );
            let mut prev_ave = -1.0;
            let mut cur_ave = -1.0;
            if new_count > 0 {
                prev_ave = f64::from(self.prev_task_count[i]) / f64::from(new_count);
                cur_ave = f64::from(self.cur_task_count[i]) / f64::from(new_count);
            }
            df.write(prev_ave, "ave num tasks parent performed");
            df.write(cur_ave, "ave num tasks cur org performed");
        }
        df.endl();
        self.new_task_count.fill(0);
        self.prev_task_count.fill(0);
        self.cur_task_count.fill(0);
        Ok(())
    }

    /// C++: `PrintNewReactionData`; zeroes `new_reaction_count` afterwards.
    pub fn print_new_reaction_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida new reactions data");
        df.write_time_stamp();
        df.write_comment(
            "First column gives the current update, all further columns give the number",
        );
        df.write_comment(
            "of times the particular reaction has newly evolved since the last time printed.",
        );

        df.write(self.m_update, "Update");
        for (i, &count) in self.new_reaction_count.iter().enumerate() {
            df.write(count, &self.reaction_names[i]);
        }
        df.endl();
        self.new_reaction_count.fill(0);
        Ok(())
    }

    /// C++: `PrintDynamicMaxMinData`. `task_max_min[i]` is
    /// `(GetArguments().GetDouble(1), GetArguments().GetDouble(2))` of
    /// `m_world->GetEnvironment().GetTask(i)`.
    pub fn print_dynamic_max_min_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        task_max_min: &[(f64, f64)],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida dynamic max min data");
        df.write_time_stamp();
        df.write_comment("First column gives the current update, 2nd and 3rd give max and min Fx");
        df.write(self.m_update, "Update");
        for i in 0..self.task_last_count.len() {
            let Some(&(max, min)) = task_max_min.get(i) else {
                return Err(format!("{filename}: no task arguments for task {i}"));
            };
            df.write(max, &format!("{} Max", self.task_names[i]));
            df.write(min, &format!("{} Min", self.task_names[i]));
        }
        df.endl();
        Ok(())
    }

    /// Shared body of the reaction printers: comments, update, one column
    /// per reaction.
    fn write_reaction_columns<V: avida_apto::DataValue + Copy>(
        &self,
        out: &mut OutputManager,
        filename: &str,
        comments: [&str; 3],
        values: &[V],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment(comments[0]);
        df.write_time_stamp();
        df.write_comment(comments[1]);
        df.write_comment(comments[2]);

        df.write(self.m_update, "Update");
        for (i, &value) in values.iter().enumerate() {
            df.write(value, &self.reaction_names[i]);
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintReactionData`
    pub fn print_reaction_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        self.write_reaction_columns(
            out,
            filename,
            [
                "Avida reaction data",
                "First column gives the current update, all further columns give the number",
                "of currently living organisms each reaction has affected.",
            ],
            &self.m_reaction_last_count,
        )
    }

    /// C++: `PrintCurrentReactionData`
    pub fn print_current_reaction_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        self.write_reaction_columns(
            out,
            filename,
            [
                "Avida reaction data",
                "First column gives the current update, all further columns give the number",
                "of currently living organisms each reaction has affected.",
            ],
            &self.m_reaction_cur_count,
        )
    }

    /// C++: `PrintReactionRewardData`
    pub fn print_reaction_reward_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        self.write_reaction_columns(
            out,
            filename,
            [
                "Avida reaction data",
                "First column gives the current update, all further columns give the add bonus reward",
                "currently living organisms have garnered from each reaction.",
            ],
            &self.m_reaction_last_add_reward,
        )
    }

    /// C++: `PrintCurrentReactionRewardData`
    pub fn print_current_reaction_reward_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        self.write_reaction_columns(
            out,
            filename,
            [
                "Avida reaction data",
                "First column gives the current update, all further columns give the add bonus reward",
                "currently living organisms have garnered from each reaction.",
            ],
            &self.m_reaction_cur_add_reward,
        )
    }

    /// C++: `PrintReactionExeData`
    pub fn print_reaction_exe_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        self.write_reaction_columns(
            out,
            filename,
            [
                "Avida reaction execution data",
                "First column gives the current update, all further columns give the number",
                "of times the particular reaction has been triggered this update.",
            ],
            &self.m_reaction_exe_count,
        )
    }

    /// C++: `PrintSpatialResData(filename, i)`: appends resource `i`'s grid
    /// to `resource_<name>.m` as a Matlab matrix. `world_x` is
    /// `m_world->GetPopulation().GetWorldX()`; `_filename` is unused, as in
    /// the C++.
    pub fn print_spatial_res_data(
        &self,
        out: &mut OutputManager,
        _filename: &str,
        i: usize,
        world_x: i32,
    ) -> Result<(), String> {
        let (Some(name), Some(grid)) = (self.resource_names.get(i), self.spatial_res_count.get(i))
        else {
            return Err(format!("PrintSpatialResData: no spatial resource {i}"));
        };
        if world_x == 0 {
            // The C++ divides by zero here.
            return Err("PrintSpatialResData: world width is 0".to_string());
        }
        let df = out.static_with_path(&format!("resource_{name}.m"))?;
        df.write_raw(&format!("{name}{:07} = [ ...", self.m_update));
        for (j, &value) in grid.iter().enumerate() {
            df.write_block_element(value, j as i32, world_x);
        }
        df.write_raw("];");
        df.flush();
        Ok(())
    }

    /// C++: `PrintMutationRateData`
    pub fn print_mutation_rate_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida copy mutation rate data");
        df.write_time_stamp();

        let r = &self.sum_copy_mut_rate;
        let l = &self.sum_log_copy_mut_rate;
        df.write(self.m_update, "Update");
        df.write(r.mean(), "Average copy mutation rate");
        df.write(r.variance(), "Variance in copy mutation rate");
        df.write(
            r.std_deviation(),
            "Standard Deviation in copy mutation rate",
        );
        df.write(r.skewness(), "Skew in copy mutation rate");
        df.write(r.kurtosis(), "Kurtosis in copy mutation rate");

        df.write(l.mean(), "Average log(copy mutation rate)");
        df.write(l.variance(), "Variance in log(copy mutation rate)");
        df.write(
            l.std_deviation(),
            "Standard Deviation in log(copy mutation rate)",
        );
        df.write(l.skewness(), "Skew in log(copy mutation rate)");
        df.write(l.kurtosis(), "Kurtosis in log(copy mutation rate)");
        df.endl();
        Ok(())
    }

    /// C++: `PrintDivideMutData`
    pub fn print_divide_mut_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida divide mutation rate data");
        df.write_time_stamp();

        let r = &self.sum_div_mut_rate;
        let l = &self.sum_log_div_mut_rate;
        df.write(self.m_update, "Update");
        df.write(r.mean(), "Average divide mutation rate");
        df.write(r.variance(), "Variance in divide mutation rate");
        df.write(
            r.std_deviation(),
            "Standard Deviation in divide mutation rate",
        );
        df.write(r.skewness(), "Skew in divide mutation rate");
        df.write(r.kurtosis(), "Kurtosis in divide mutation rate");

        df.write(l.mean(), "Average log(divide mutation rate)");
        df.write(l.variance(), "Variance in log(divide mutation rate)");
        df.write(
            l.std_deviation(),
            "Standard Deviation in log(divide mutation rate)",
        );
        df.write(l.skewness(), "Skew in log(divide mutation rate)");
        df.write(l.kurtosis(), "Kurtosis in log(divide mutation rate)");
        df.endl();
        Ok(())
    }

    /// Shared body of `PrintSenseData` / `PrintSenseExeData`. QUIRK: the
    /// first comment ends in `\n`, so the header has a blank comment line;
    /// there is no time stamp. (The sense arrays are never sized at
    /// 47f13dadb, so only the update column is printed.)
    fn write_sense_columns(
        &self,
        out: &mut OutputManager,
        filename: &str,
        comment: &str,
        counts: &[i32],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida sense instruction usage\n");
        df.write_comment(comment);
        df.write(self.m_update, "update");
        for (i, &count) in counts.iter().enumerate() {
            let Some(name) = self.sense_names.get(i) else {
                return Err(format!("{filename}: no sense name {i}"));
            };
            df.write(count, name);
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintSenseData`
    pub fn print_sense_data(&self, out: &mut OutputManager, filename: &str) -> Result<(), String> {
        self.write_sense_columns(
            out,
            filename,
            "total number of organisms whose parents executed sense instructions with given labels",
            &self.sense_last_count,
        )
    }

    /// C++: `PrintSenseExeData`
    pub fn print_sense_exe_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        self.write_sense_columns(
            out,
            filename,
            "total number of sense instructions executed by the parents of current organisms with given labels",
            &self.sense_last_exe_count,
        )
    }

    /// C++: `PrintInternalTasksData`. The file name `in_tasksq.dat` is a
    /// flag that prints `in_tasks.dat` and `in_taskquality.dat`.
    pub fn print_internal_tasks_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let mut file = filename;
        if filename == "in_tasksq.dat" {
            file = "in_tasks.dat";
            self.print_internal_tasks_qual_data(out, "in_taskquality.dat")?;
        }
        let df = out.static_with_path(file)?;
        df.write_comment("Avida tasks data: tasks performed with internal resources");
        df.write_time_stamp();
        df.write_comment("First column gives the current update, next columns give the number");
        df.write_comment(
            "of organisms that have the particular task, performed with internal resources, ",
        );
        df.write_comment("as a component of their merit");

        df.write(self.m_update, "Update");
        for (i, &count) in self.task_internal_last_count.iter().enumerate() {
            df.write(count, &self.task_names[i]);
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintInternalTasksQualData`
    pub fn print_internal_tasks_qual_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida tasks quality data: tasks performed using internal resources");
        df.write_time_stamp();
        df.write_comment(
            "First column gives the current update, rest give average and max task quality ",
        );
        df.write_comment("for those tasks performed using internal resources");
        df.write(self.m_update, "Update");
        for (i, &count) in self.task_internal_last_count.iter().enumerate() {
            let mut qual = 0.0;
            if count > 0 {
                qual = self.task_internal_last_quality[i] / f64::from(count);
            }
            df.write(qual, &format!("{} Average", self.task_names[i]));
            df.write(
                self.task_internal_last_max_quality[i],
                &format!("{} Max", self.task_names[i]),
            );
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintSleepData`. `deme_sleeping_counts[i]` is
    /// `pop.GetDeme(i).GetSleepingCount()`. QUIRK: the first comment ends in
    /// `\n` and there is no time stamp.
    pub fn print_sleep_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        deme_sleeping_counts: &[i32],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Number of organisms sleeping\n");
        df.write_comment("total number of organisms sleeping");
        df.write(self.m_update, "update");
        for (i, &count) in deme_sleeping_counts.iter().enumerate() {
            df.write(count, &format!("DemeID {i}"));
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintCompetitionData`
    pub fn print_competition_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Competition results\n");
        df.write_comment("results of the current competitions");

        df.write(self.m_update, "update");
        df.write(self.avg_competition_fitness, "average competition fitness");
        df.write(self.min_competition_fitness, "min competition fitness");
        df.write(self.max_competition_fitness, "max competition fitness");
        df.write(
            self.avg_competition_copied_fitness,
            "average copied fitness",
        );
        df.write(self.min_competition_copied_fitness, "min copied fitness");
        df.write(self.max_competition_copied_fitness, "max copied fitness");
        df.write(self.num_orgs_replicated, "number of organisms copied");

        // Only print trial info if there were multiple trials.
        if self.avg_trial_fitnesses.len() > 1 {
            for (i, &fitness) in self.avg_trial_fitnesses.iter().enumerate() {
                df.write(fitness, &format!("trial.{i} fitness"));
            }
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintDemeAverageData`
    pub fn print_deme_average_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida Average Deme Data");
        df.write_time_stamp();

        df.write(self.m_update, "Update");
        df.write(self.m_num_occupied_demes, "Count");
        df.write(self.sum_deme_age.mean(), "Age");
        df.write(self.sum_deme_birth_count.mean(), "Births");
        df.write(self.sum_deme_org_count.mean(), "Organisms");
        df.write(self.sum_deme_generation.mean(), "Generation");
        df.write(
            self.sum_deme_last_birth_count.mean(),
            "Births (at last replication)",
        );
        df.write(
            self.sum_deme_last_org_count.mean(),
            "Organisms (at last replication)",
        );
        df.write(self.sum_deme_merit.average(), "Merit");
        df.write(self.sum_deme_gestation_time.mean(), "Gestation Time");
        df.write(
            self.sum_deme_normalized_time_used.average(),
            "Time Used (normalized by org fitness)",
        );
        df.write(
            self.sum_deme_generations_per_lifetime.average(),
            "Generations between current and last founders",
        );
        df.write(self.sum_deme_events_killed.mean(), "Events killed");
        df.write(
            self.sum_deme_events_kill_attempts.mean(),
            "Attempts to kill event",
        );
        df.endl();
        Ok(())
    }

    /// C++: `PrintFlowRateTuples`; clears every tuple afterwards.
    pub fn print_flow_rate_tuples(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Flow Rate Tuples");
        df.write_time_stamp();

        df.write(self.m_update, "Update");
        for (&rate, tuple) in &self.flow_rate_tuples {
            let flow_rate_str = format!("flow rate {rate}");
            df.write(rate, &flow_rate_str);
            df.write(
                tuple.org_count.mean(),
                &format!("{flow_rate_str} deme pop size"),
            );
            df.write(
                tuple.events_killed.mean(),
                &format!("{flow_rate_str} events killed"),
            );
            df.write(
                tuple.attempts_to_kill_events.mean(),
                &format!("{flow_rate_str} events attempted to kill"),
            );
            df.write(
                tuple.avg_energy_usage_ratio.average(),
                &format!("{flow_rate_str} exe ratio"),
            );
            df.write(
                tuple.total_births.mean(),
                &format!("{flow_rate_str} total births"),
            );
            df.write(
                tuple.current_sleeping.mean(),
                &format!("{flow_rate_str} total sleeping"),
            );
        }
        df.endl();

        for tuple in self.flow_rate_tuples.values_mut() {
            tuple.org_count.clear();
            tuple.events_killed.clear();
            tuple.attempts_to_kill_events.clear();
            tuple.avg_energy_usage_ratio.clear();
            tuple.total_births.clear();
            tuple.current_sleeping.clear();
        }
        Ok(())
    }

    /// C++: `PrintKilledPreyFTData`. `attack_prey_ft_list` is
    /// `m_world->GetEnvironment().GetAttackPreyFTList()`; `killed_lists`
    /// holds `GetPhenotype().GetKilledPreyFTData()` of each live organism.
    pub fn print_killed_prey_ft_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        attack_prey_ft_list: &[i32],
        killed_lists: &[Vec<i32>],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Record of orgs killed by predators by victim forage type.");
        df.write_time_stamp();
        df.write(self.m_update, "Update");

        let mut org_targets = vec![0i32; attack_prey_ft_list.len()];
        for killed_list in killed_lists {
            for (i, &killed) in killed_list.iter().enumerate() {
                let Some(slot) = org_targets.get_mut(i) else {
                    return Err(format!(
                        "{filename}: killed-prey list longer than the attack forage-type list"
                    ));
                };
                *slot = slot.wrapping_add(killed);
            }
        }
        for (&target, &count) in attack_prey_ft_list.iter().zip(&org_targets) {
            df.write(target, "Killed FT ID");
            df.write(count, "Num Orgs Killed");
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintAttacks`
    pub fn print_attacks(&self, out: &mut OutputManager, filename: &str) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Predator Average Attack and Kills Data");
        df.write_time_stamp();

        df.write(self.m_update, "Update");
        df.write(self.sum_attacks.count(), "Num Predators");
        df.write(self.sum_attacks.sum(), "Total Attacks");
        df.write(self.sum_attacks.max(), "Max Attacks");
        df.write(self.sum_attacks.average(), "Average Attacks");
        df.write(self.sum_kills.sum(), "Total Kills");
        df.write(self.sum_kills.max(), "Max Kills");
        df.write(self.sum_kills.average(), "Average Kills");
        df.endl();
        Ok(())
    }

    /// C++: `SetGroupAttackInstNames` / `PrintGroupAttackData`: one
    /// `update,inst,count0,...` row per attack instruction, written
    /// straight to the stream after a header of comments.
    pub fn print_group_attack_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        if !df.header_done() {
            df.write_comment("Attack Instruction Execution Circumstances");
            df.write_comment("Gives count of attacks with given number of neighbors present.");
            df.write_time_stamp();
            df.write_comment("Instruction");
            df.write_comment("Update");
            for n in 0..=8 {
                df.write_comment(&format!("{n}Neighbors"));
            }
            df.write_comment("8+Neighbors");
            for n in 0..=8 {
                df.write_comment(&format!("{n}InGroupNeighbors"));
            }
            df.write_comment("8+InGroupNeighbors");
            df.flush_comments();
            df.endl();
        }

        let per_inst = self.m_group_attack_exe_map.get(inst_set);
        for inst in inst_map_entry(&self.m_group_attack_names, inst_set) {
            let counts = per_inst
                .and_then(|m| m.get(inst))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let Some((last, rest)) = counts.split_last() else {
                return Err(format!(
                    "{filename}: no attack counters for {inst} (the C++ indexes out of bounds)"
                ));
            };
            let mut row = format!("{},{inst},", self.m_update);
            for count in rest {
                row.push_str(&format!("{},", count.sum()));
            }
            row.push_str(&last.sum().to_string());
            let _ = writeln!(df.ofstream(), "{row}");
            df.flush();
        }
        Ok(())
    }

    /// C++: `PrintGroupAttackBits(raw_bits)`: appends the raw byte and a
    /// newline to `group_attack_bits-<update>.dat`.
    pub fn print_group_attack_bits(
        &self,
        out: &mut OutputManager,
        raw_bits: u8,
    ) -> Result<(), String> {
        let df = out.static_with_path(&format!("group_attack_bits-{}.dat", self.m_update))?;
        if !df.header_done() {
            df.write_comment("Condensed Group Attack Results during this update (this is a binary file to be post-process converted to bit string -- left to right bits): \n# Size (3 bit): potential group size, not including self \n# Success: (2 bit) 0 == sucess, 1 == no prey failure, 2 == no friends failure, 3 == chance failure \n# Share (2 bit): 0 == no, 1 == yes, 2 == fake \n# Inst (1 bit): 0 == solo attack inst, 1 == group attack inst");
            df.write_time_stamp();
            df.endl();
        }
        let _ = df.ofstream().write_all(&[raw_bits, b'\n']);
        df.flush();
        Ok(())
    }

    /// C++: `PrintGroupAttackString(string)` (`attack_outcomes.dat`).
    pub fn print_group_attack_string(
        &self,
        out: &mut OutputManager,
        string: &str,
    ) -> Result<(), String> {
        write_update_string_row(
            out,
            "attack_outcomes.dat",
            self.m_update,
            &[
                "Group Attack Circumstances",
                "Update",
                "Size: potential group size, not including self",
                "Success: 0 == sucess, 1 == no prey failure, 2 == no friends failure, 3 == chance failure",
                "SharedKill: 0 == no, 1 == yes, 2 == fake",
                "Inst: 0 == solo attack inst, 1 == group attack inst",
            ],
            1,
            string,
        )
    }

    /// C++: `PrintLookData(string)` (`looksettings.dat`).
    pub fn print_look_data(&self, out: &mut OutputManager, string: &str) -> Result<(), String> {
        write_update_string_row(
            out,
            "looksettings.dat",
            self.m_update,
            &[
                "Final (used) Look Settings",
                "Update",
                "ThisOrgForageTarget",
                "AnyFound",
                "Habitat",
                "Distance",
                "SearchType",
                "SoughtID",
            ],
            1,
            string,
        )
    }

    /// C++: `PrintLookDataOutput(string)` (`lookoutput.dat`).
    pub fn print_look_data_output(
        &self,
        out: &mut OutputManager,
        string: &str,
    ) -> Result<(), String> {
        write_update_string_row(
            out,
            "lookoutput.dat",
            self.m_update,
            &[
                "Final (used) Look Output to Registers",
                "Update",
                "ThisOrgForageTarget",
                "AnyFound",
                "Habitat",
                "Distance",
                "SearchType",
                "SoughtID",
                "Count",
                "Value",
                "Group",
                "FT",
            ],
            1,
            string,
        )
    }

    /// C++: `PrintLookEXDataOutput(string)`. QUIRK: shares `lookoutput.dat`
    /// with `PrintLookDataOutput`; whichever runs first writes the header.
    pub fn print_look_ex_data_output(
        &self,
        out: &mut OutputManager,
        string: &str,
    ) -> Result<(), String> {
        write_update_string_row(
            out,
            "lookoutput.dat",
            self.m_update,
            &[
                "Final (used) Look Output to Registers",
                "Update",
                "ThisOrgForageTarget",
                "AnyFound",
                "Habitat",
                "SoughtID",
                "TravelDistance",
                "Deviance",
                "CountValue",
            ],
            1,
            string,
        )
    }

    /// C++: `PrintGermlineData`; clears `m_germline_generation` afterwards.
    pub fn print_germline_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida germline data");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        df.write(
            self.m_germline_generation.average(),
            "Mean germline generation of replicated germlines [replgen]",
        );
        df.endl();
        self.m_germline_generation.clear();
        Ok(())
    }

    /// C++: `PrintDemeGLSFounders`: rows `update source target n {id genome}*`
    /// written straight to the stream; clears the record afterwards.
    pub fn print_deme_gls_founders(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida gls deme founder data.");
        df.write_time_stamp();
        df.write_column_desc("Update [update]", "");
        df.write_column_desc("Soure Deme ID [sdemeid]", "");
        df.write_column_desc("Target Deme ID [tdemeid]", "");
        df.write_column_desc("Number of founders [size]", "");
        df.write_column_desc("{target genotype ID, target genome... founder 0, ...}", "");
        df.flush_comments();

        for (&(source, target), founders) in &self.m_gls_deme_founders {
            let mut row = format!("{} {source} {target} {}", self.m_update, founders.len());
            for (id, genome) in founders {
                row.push_str(&format!(" {id} {genome}"));
            }
            let _ = write!(df.ofstream(), "{row}");
            df.endl();
        }
        self.m_gls_deme_founders.clear();
        Ok(())
    }

    /// C++: `PrintDemeFoundersData`: rows `update deme n id*` written
    /// straight to the stream; clears the record afterwards.
    pub fn print_deme_founders_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida deme founder data.");
        df.write_time_stamp();
        df.write_column_desc("Update [update]", "");
        df.write_column_desc("Deme ID [demeid]", "");
        df.write_column_desc("Number of founders [size]", "");
        df.write_column_desc("{Genotype ID of founder 0, ...}", "");
        df.flush_comments();

        for (&deme_id, founders) in &self.m_deme_founders {
            let mut row = format!("{} {deme_id} {}", self.m_update, founders.len());
            for id in founders {
                row.push_str(&format!(" {id}"));
            }
            let _ = write!(df.ofstream(), "{row}");
            df.endl();
        }
        self.m_deme_founders.clear();
        Ok(())
    }

    /// C++: `PrintDemesFitnessData`. The per-deme column is commented out in
    /// the C++, so only the update is printed.
    pub fn print_demes_fitness_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida competition fitness for each deme");
        df.write_time_stamp();
        df.write(self.m_update, "Update");
        df.endl();
        Ok(())
    }

    /// C++: `PrintDemeCompetitionData`; clears `m_deme_fitness` afterwards.
    pub fn print_deme_competition_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida compete demes data");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");

        let mut avg = 0.0;
        for &f in &self.m_deme_fitness {
            avg += f;
        }
        if avg > 0.0 {
            avg /= self.m_deme_fitness.len() as f64;
        }
        df.write(avg, "Avg. deme fitness [avgfit]");
        if let Some(&first) = self.m_deme_fitness.first() {
            // std::max_element: the first element no other exceeds.
            let mut max = first;
            for &f in &self.m_deme_fitness[1..] {
                if max < f {
                    max = f;
                }
            }
            df.write(max, "Max. deme fitness [maxfit]");
        } else {
            df.write(0.0, "Max. deme fitness [maxfit]");
        }
        df.endl();
        self.m_deme_fitness.clear();
        Ok(())
    }

    /// C++: `PrintSynchronizationData`; resets the flash count and times.
    pub fn print_synchronization_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida synchronization data");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        df.write(self.m_flash_count, "Flash count [fcount]");
        df.endl();
        self.m_flash_count = 0;
        self.m_flash_times.clear();
        Ok(())
    }

    /// C++: `PrintDetailedSynchronizationData`: one row per flash; clears
    /// the flash times afterwards.
    pub fn print_detailed_synchronization_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Detailed Avida synchronization data");
        df.write_comment("Rows are (update, demeid, cellid) tuples, representing the update at which that cell flashed.");
        df.write_time_stamp();
        for (&update, demes) in &self.m_flash_times {
            for (&deme_id, cells) in demes {
                for &cell in cells {
                    df.write(update, "Update [update]");
                    df.write(deme_id, "Deme ID [demeid]");
                    df.write(
                        cell,
                        "Deme-relative cell ID that issued a flash at this update [relcellid]",
                    );
                    df.endl();
                }
            }
        }
        self.m_flash_times.clear();
        Ok(())
    }

    /// C++: `PrintSimpleConsensusData`; clears the records afterwards.
    pub fn print_simple_consensus_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida consensus data");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        df.write(self.m_consensi.len() as f64, "Consensus count [count]");
        df.endl();
        self.m_consensi.clear();
        Ok(())
    }

    /// C++: `PrintConsensusData`; clears the records afterwards.
    pub fn print_consensus_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida consensus data");
        df.write_time_stamp();
        df.write_column_desc("Update [update]", "");
        df.write_column_desc("Deme ID [demeid]", "");
        df.write_column_desc("Consensus value [consensus]", "");
        df.write_column_desc("Cell ID [cellid]", "");
        df.flush_comments();
        for record in &self.m_consensi {
            df.write(record.update, "Update [update]");
            df.write(record.deme_id, "Deme ID [demeid]");
            df.write(record.consensus, "Consensus value [consensus]");
            df.write(record.cell_id, "Cell ID [cellid]");
            df.endl();
        }
        self.m_consensi.clear();
        Ok(())
    }

    /// C++: `PrintNumOrgsKilledData`; clears the kill accumulators
    /// afterwards. (`Mean()` of an empty accumulator is NaN.)
    pub fn print_num_orgs_killed_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Organisms killed using kill actions");
        df.write_time_stamp();
        df.write_comment("First column is the current update and the second column lists the number of organisms killed");

        df.write(self.m_update, "Update");
        df.write(self.sum_orgs_killed.mean(), "Avg Num Orgs Killed");
        df.write(
            self.sum_unoccupied_cell_kill_attempts.mean(),
            "Avg Num Unoccupied Cell Kill Attempts",
        );
        df.write(
            self.sum_cells_scanned_at_kill.mean(),
            "Avg Num Cells Scanned By Kill Event",
        );
        df.endl();

        self.sum_orgs_killed.clear();
        self.sum_unoccupied_cell_kill_attempts.clear();
        self.sum_cells_scanned_at_kill.clear();
        Ok(())
    }

    /// C++: `PrintMigrationData`
    pub fn print_migration_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Number of migrations made using the migratedemes event");
        df.write_time_stamp();
        df.write_comment("First column is the current update and the second column lists the number of migrations made");

        df.write(self.m_update, "Update");
        df.write(self.num_migrations, "Num Migrations");
        df.endl();
        Ok(())
    }

    /// C++: `PrintStringMatchData`; clears the perfect-match sums
    /// afterwards. `match_strings[k]` is
    /// `m_world->GetEnvironment().GetMatchString(k).GetData()` (their count
    /// is `GetNumberOfMatchStrings()`); `org_strings` has one entry per
    /// occupied cell in cell order, whose `j`th element is
    /// `(org->GetNumberStringsOnHand(j), org->GetNumberStringsProduced(j))`
    /// (a missing element counts as `(0, 0)`, as for an organism without a
    /// string map).
    pub fn print_string_match_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
        match_strings: &[String],
        org_strings: &[Vec<(i32, i32)>],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida organism string donation information");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");

        let num = match_strings.len();
        let mut strings_stored = vec![DoubleSum::new(); num];
        let mut strings_produced = vec![DoubleSum::new(); num];
        let mut total = DoubleSum::new();
        let mut instant_perfect_match = 0i32;
        let mut instant_perfect_match_org = 0i32;
        let mut nothing = 0i32;
        let mut specialists = 0i32;
        let mut generalists = 0i32;

        for org in org_strings {
            let mut min = -1;
            let mut type_prod = 0;
            for j in 0..num {
                let (onhand, produced) = org.get(j).copied().unwrap_or((0, 0));
                if min == -1 || onhand < min {
                    min = onhand;
                }
                strings_stored[j].add(f64::from(onhand));
                total.add(f64::from(onhand));
                strings_produced[j].add(f64::from(produced));
                if produced != 0 {
                    type_prod += 1;
                }
            }
            // QUIRK: with no match strings `min` stays -1 and is added.
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

        for (k, name) in match_strings.iter().enumerate() {
            df.write(strings_produced[k].average(), &format!("produced{name}"));
            df.write(strings_stored[k].average(), &format!("stored{name}"));
        }
        df.write(total.average(), "totalStoredAverage");

        df.write(self.m_perfect_match.sum(), "PerfectMatchStringElapse[ps]");
        self.m_perfect_match.clear();
        df.write(self.m_perfect_match_org.sum(), "PerfectMatchOrgElapse[pso]");
        self.m_perfect_match_org.clear();
        df.write(instant_perfect_match, "PerfectMatchStringInstant[psi]");
        df.write(instant_perfect_match_org, "PerfectMatchOrgInstant[psoi]");
        df.write(nothing, "Producednothing[nothing]");
        df.write(generalists, "Generalists[generalists]");
        df.write(specialists, "Specialists[specialists]");
        df.endl();
        Ok(())
    }

    /// C++: `PrintReputationData`. `orgs` has one record per occupied cell
    /// in cell order (see [`ReputationOrgData`]).
    pub fn print_reputation_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        orgs: &[ReputationOrgData],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        let mut reputations = DoubleSum::new();
        let mut donations = DoubleSum::new();
        let mut donors = DoubleSum::new();
        let mut own_raw_mat = DoubleSum::new();
        let mut other_raw_mat = DoubleSum::new();
        let mut num_alt = 0i32;
        let mut num_coop = 0i32;

        df.write_comment(
            "Avida organism reputation information -- average donations, min donations, max donations",
        );
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");

        for org in orgs {
            reputations.add(f64::from(org.reputation));
            donations.add(f64::from(org.num_donations));
            own_raw_mat.add(f64::from(org.self_raw_materials));
            other_raw_mat.add(f64::from(org.other_raw_materials));
            donors.add(f64::from(org.num_donors));
            if org.num_donations > 0 {
                num_alt += 1;
            }
            // QUIRK: `(received && donations) > 0`, i.e. both non-zero.
            if org.num_donations_received != 0 && org.num_donations != 0 {
                num_coop += 1;
            }
        }
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

    /// C++: `PrintShadedAltruists`. `pop_size` is
    /// `m_world->GetPopulation().GetSize()` (cells); `shades` has one value
    /// per occupied cell in cell order: `shade_of_gb`, i.e.
    /// `org->GetPhenotype().GetTestCPUInstCount()[i]` for the last
    /// instruction `i` of the default instruction set named
    /// `donate-shadedgb`, or 0 when there is none or the test-CPU count is
    /// empty. The two ratios are `float`s, as in the C++.
    pub fn print_shaded_altruists(
        &self,
        out: &mut OutputManager,
        filename: &str,
        pop_size: i32,
        shades: &[i32],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("The number of organisms in different bins of shaded altruism");

        let mut bins = [0i32; 11]; // shaded_100, shaded_90, ..., shaded_0
        let mut total_shaded = 0i32;
        for &shade in shades {
            if shade == 100 {
                bins[0] += 1;
            }
            for (k, bin) in bins.iter_mut().enumerate().skip(1) {
                if shade > 100 - 10 * k as i32 {
                    *bin += 1;
                }
            }
            total_shaded = total_shaded.wrapping_add(shade);
        }
        let high_alt = bins[1] as f32 / pop_size as f32;
        let avg_shade = total_shaded as f32 / pop_size as f32;

        df.write_comment("Bins of orgs of shaded strategies.");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        df.write(pop_size, "Population [population]");
        for (k, &count) in bins.iter().enumerate() {
            let pct = 100 - 10 * k;
            df.write(count, &format!("shaded-{pct} [shaded{pct}]"));
        }
        df.write(f64::from(high_alt), "percent-high-alt  [highalt]");
        df.write(f64::from(avg_shade), "avg-shade [avgshade]");
        df.endl();
        Ok(())
    }

    /// C++: `PrintKaboom`; resets the kaboom counters afterwards.
    pub fn print_kaboom(&mut self, out: &mut OutputManager, filename: &str) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("The number of kabooms.");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        df.write(self.num_kabooms, "number of kabooms");
        df.write(self.num_kaboom_kills, "number of orgs killed by kabooms");
        df.write(
            self.num_sa_kin,
            "Number of organisms with suicidal altruism inst considered kin",
        );
        df.write(
            self.num_sa_notkin,
            "Number of organisms with suicidal altruism inst considered non-kin",
        );
        df.write(
            self.num_nsa_kin,
            "Number of organisms without suicidal altruism inst considered kin",
        );
        df.write(
            self.num_nsa_notkin,
            "Number of organisms without suicidal altruism inst considered non-kin",
        );
        df.endl();

        self.hd_list.clear();
        self.num_kabooms = 0;
        self.num_kaboom_kills = 0;
        self.num_kabooms_pre = 0;
        self.num_kabooms_post = 0;
        self.sum_perc_lyse = 0.0;
        self.sum_cpu_cycles = 0.0;
        self.num_sa_kin = 0;
        self.num_sa_notkin = 0;
        self.num_nsa_kin = 0;
        self.num_nsa_notkin = 0;
        Ok(())
    }

    /// C++: `PrintQuorum`; resets the quorum counters afterwards. The
    /// average is a `float` (`(float)ave_threshold_ub/(float)num_quorum`).
    pub fn print_quorum(&mut self, out: &mut OutputManager, filename: &str) -> Result<(), String> {
        let ave_thresh_ub = self.ave_threshold_ub as f32 / self.num_quorum as f32;
        let df = out.static_with_path(filename)?;
        df.write_comment("Quorum sensing with threshold.");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        df.write(
            self.num_stop_explode,
            "Number of explosions stopped by quorum sensing",
        );
        df.write(
            f64::from(ave_thresh_ub),
            "Average quorum sense threshold upper bound per qs instruction",
        );
        df.endl();
        self.num_stop_explode = 0;
        self.num_quorum = 0;
        self.ave_threshold_ub = 0;
        Ok(())
    }

    /// C++: `PrintGroupsFormedData`. `formed_groups` is
    /// `m_world->GetPopulation().GetFormedGroups()` (group id to size);
    /// `orgs_with_opinion` is the number of occupied cells whose organism
    /// `HasOpinion()` (the C++ adds those bools into a double one by one,
    /// which is exact); `pop_size` is `GetPopulation().GetSize()` (cells).
    pub fn print_groups_formed_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        formed_groups: &BTreeMap<i32, i32>,
        orgs_with_opinion: i32,
        pop_size: i32,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Information about the groups joined and used by the organisms");

        let mut avg_size = 0.0;
        let mut avg_size_wout_empty = 0.0;
        let mut max_size = 0.0;
        let mut min_size = 100_000_000_000.0;
        let mut active_groups = 0.0;
        for &size in formed_groups.values() {
            let cur_size = f64::from(size);
            avg_size += cur_size;
            if cur_size > max_size {
                max_size = cur_size;
            }
            if cur_size < min_size {
                min_size = cur_size;
            }
            if cur_size > 0.0 {
                active_groups += 1.0;
                avg_size_wout_empty += cur_size;
            }
        }
        let mut groups_per_org = f64::from(orgs_with_opinion);

        avg_size /= formed_groups.len() as f64;
        avg_size_wout_empty /= active_groups;
        groups_per_org /= f64::from(pop_size);
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        df.write(formed_groups.len() as f64, "number of groups [num]");
        df.write(avg_size, "average size of groups [avgsize]");
        df.write(
            avg_size_wout_empty,
            "average size of  non-emptygroups [avgsizene]",
        );
        df.write(max_size, "max size of groups [maxsize]");
        df.write(min_size, "min size of groups [minsize]");
        df.write(active_groups, "active groups [actgroup]");
        df.write(groups_per_org, "groups per org life [groupsperorg]");
        df.endl();
        Ok(())
    }

    /// C++: `PrintGroupIds`: one row per formed group, then QUIRK an extra
    /// `Endl` (a blank line after every call). `formed_groups` is
    /// `m_world->GetPopulation().GetFormedGroups()`.
    pub fn print_group_ids(
        &self,
        out: &mut OutputManager,
        filename: &str,
        formed_groups: &BTreeMap<i32, i32>,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("The ids of groups used.");
        df.write_time_stamp();
        for (&group_id, &size) in formed_groups {
            df.write(self.m_update, "Update [update]");
            df.write(group_id, "group id [groupid]");
            df.write(f64::from(size), "size of groups [grsize]");
            df.endl();
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintToleranceInstructionData`: executions of each tolerance
    /// instruction during the current update.
    pub fn print_tolerance_instruction_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        self.size_tolerance_exe_counts();
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida tolerance instruction executions per update");
        df.write_time_stamp();
        df.write(self.m_update, "Update");
        for (&(update, count), name) in self
            .m_is_tolerance_exe_counts
            .iter()
            .zip(TOLERANCE_INST_NAMES)
        {
            df.write(if update == self.m_update { count } else { 0 }, name);
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintToleranceData`: the circumstances of each tolerance
    /// instruction executed in the current update. `track_tolerance` is
    /// `TRACK_TOLERANCE`; when it is off the C++ reports the error and
    /// aborts with `INVALID_CONFIG`.
    pub fn print_tolerance_data(
        &self,
        out: &mut OutputManager,
        filename: &str,
        track_tolerance: bool,
    ) -> Result<(), String> {
        if !track_tolerance {
            return Err("TRACK_TOLERANCE option must be turned on in avida.cfg for PrintToleranceData to function.".to_string());
        }
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida circumstance data for each tolerance instruction pre-execution");
        df.write_time_stamp();
        for c in &self.m_is_tolerance_exe_insts {
            if c.update != self.m_update {
                continue;
            }
            let Some(name) = usize::try_from(c.inst)
                .ok()
                .and_then(|i| TOLERANCE_INST_NAMES.get(i))
            else {
                return Err(format!(
                    "{filename}: unknown tolerance instruction {}",
                    c.inst
                ));
            };
            df.write(c.update, "Update [update]");
            df.write(*name, "Tolerance instruction [inst]");
            df.write(c.gr_id, "group id [groupid]");
            df.write(c.gr_size, "size of group [grsize]");
            df.write(c.res_level, "group resource available [grfood]");
            df.write(
                c.odds_immigrants,
                "odds for immigrants coming into the group [oddsimmigrants]",
            );
            df.write(
                c.odds_offspring_own,
                "odds for org's own offspring to stay in group [oddsown]",
            );
            df.write(
                c.odds_offspring_others,
                "odds for offspring in group [oddsothers]",
            );
            df.write(
                c.tol_immigrants,
                "org's tolerance for immigrants [tol-immi]",
            );
            df.write(c.tol_own, "org's tolerance for own offspring [tol-own]");
            df.write(
                c.tol_others,
                "org's tolerance for other offspring in the group [tol-others]",
            );
            df.write(c.tol_max, "tolerance max [tol-max]");
            df.endl();
        }
        Ok(())
    }

    /// C++: `PrintTargets`. `pred_prey_switch` is `PRED_PREY_SWITCH`,
    /// `target_ids` `m_world->GetEnvironment().GetTargetIDs()` (whose
    /// membership is also `IsTargetID`), `forage_targets` the
    /// `GetForageTarget()` of each live organism. QUIRK: a forage target not
    /// in the list is counted at index `this_target`; where that index is
    /// out of bounds (undefined behaviour in C++) this returns an error.
    pub fn print_targets(
        &self,
        out: &mut OutputManager,
        filename: &str,
        pred_prey_switch: i32,
        target_ids: &BTreeSet<i32>,
        forage_targets: &[i32],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Targets in use on update boundary.");
        df.write_comment(
            "-2: is predator, -1: no targets(default), >=0: id of environmental resource targeted).",
        );
        df.write_comment("Format is update + target0 + count0 + target1 + count1 ...");
        df.write_time_stamp();
        df.write(self.m_update, "Update");

        let mut has_pred = false;
        let mut offset: i32 = 1;
        if pred_prey_switch == -2 || pred_prey_switch > -1 || target_ids.contains(&-2) {
            has_pred = true;
            offset = 2;
        }

        // ft's may not be sequentially numbered
        let mut dec_prey = false;
        let mut dec_pred = false;
        let mut num_targets: i32 = 0;
        for &t in target_ids {
            num_targets += 1;
            if t == -1 && !dec_prey {
                offset -= 1;
                dec_prey = true;
            }
            if t == -2 && !dec_pred {
                offset -= 1;
                dec_pred = true;
            }
        }

        let oob = || format!("{filename}: target index out of bounds (undefined in C++)");
        let tot_targets = usize::try_from(num_targets + offset).map_err(|_| oob())?;
        let mut target_list = vec![0i32; tot_targets];
        *target_list.get_mut(0).ok_or_else(oob)? = -1;
        if has_pred || target_ids.contains(&-2) {
            *target_list.get_mut(0).ok_or_else(oob)? = -2;
            *target_list.get_mut(1).ok_or_else(oob)? = -1;
        }
        for (i, &raw) in target_ids.iter().enumerate() {
            if raw >= 0 {
                let at = target_index(i as i32 + offset, tot_targets).ok_or_else(oob)?;
                target_list[at] = raw;
            }
        }

        let mut org_targets = vec![0i32; tot_targets];
        for &forage_target in forage_targets {
            let this_target = forage_target.max(-2);
            let this_index = target_list
                .iter()
                .position(|&t| t == this_target)
                .map_or(this_target, |i| i as i32);
            let at = target_index(this_index, tot_targets).ok_or_else(oob)?;
            org_targets[at] += 1;
        }
        for (&target, &count) in target_list.iter().zip(&org_targets) {
            df.write(target, "Target ID");
            df.write(count, "Num Orgs Targeting ID");
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintTopPredTargets`. Parameters as for
    /// [`Stats::print_targets`]; here forage targets below -3 count as -3.
    pub fn print_top_pred_targets(
        &self,
        out: &mut OutputManager,
        filename: &str,
        pred_prey_switch: i32,
        target_ids: &BTreeSet<i32>,
        forage_targets: &[i32],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Targets in use on update boundary.");
        df.write_comment("-3: is top predator, -2: is predator, -1: no targets(default), >=0: id of environmental resource targeted).");
        df.write_comment("Format is update + target0 + count0 + target1 + count1 ...");
        df.write_time_stamp();
        df.write(self.m_update, "Update");

        let mut has_pred = false;
        let mut offset: i32 = 1;
        if pred_prey_switch == -2 || pred_prey_switch > -1 {
            has_pred = true;
            offset = 3;
        }

        // ft's may not be sequentially numbered
        let mut dec_prey = false;
        let mut dec_pred = false;
        let mut dec_tpred = false;
        let mut num_targets: i32 = 0;
        for &t in target_ids {
            num_targets += 1;
            if t == -1 && !dec_prey {
                offset -= 1;
                dec_prey = true;
            }
            if t == -2 && !dec_pred {
                offset -= 1;
                dec_pred = true;
            }
            if t == -3 && !dec_tpred {
                offset -= 1;
                dec_tpred = true;
            }
        }

        let oob = || format!("{filename}: target index out of bounds (undefined in C++)");
        let tot_targets = usize::try_from(num_targets + offset).map_err(|_| oob())?;
        let mut target_list = vec![0i32; tot_targets];
        *target_list.get_mut(0).ok_or_else(oob)? = -1;
        if has_pred {
            *target_list.get_mut(0).ok_or_else(oob)? = -3;
            *target_list.get_mut(1).ok_or_else(oob)? = -2;
            *target_list.get_mut(2).ok_or_else(oob)? = -1;
        }
        for (i, &raw) in target_ids.iter().enumerate() {
            if raw >= 0 {
                let at = target_index(i as i32 + offset, tot_targets).ok_or_else(oob)?;
                target_list[at] = raw;
            }
        }

        let mut org_targets = vec![0i32; tot_targets];
        for &forage_target in forage_targets {
            let this_target = forage_target.max(-3);
            let this_index = target_list
                .iter()
                .position(|&t| t == this_target)
                .map_or(this_target, |i| i as i32);
            let at = target_index(this_index, tot_targets).ok_or_else(oob)?;
            org_targets[at] += 1;
        }
        for (&target, &count) in target_list.iter().zip(&org_targets) {
            df.write(target, "Target ID");
            df.write(count, "Num Orgs Targeting ID");
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintMimicDisplays`. `target_ids` is
    /// `GetEnvironment().GetTargetIDs()`; `forage_targets` holds
    /// `(GetForageTarget(), GetShowForageTarget())` of each live organism.
    pub fn print_mimic_displays(
        &self,
        out: &mut OutputManager,
        filename: &str,
        target_ids: &BTreeSet<i32>,
        forage_targets: &[(i32, i32)],
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("FTs being displayed by mimics on update boundary (true mimic ft == 1).");
        df.write_time_stamp();
        df.write(self.m_update, "Update");

        let poss_fts: Vec<i32> = target_ids.iter().copied().filter(|&t| t >= 0).collect();
        let mut displayed_fts = vec![0i32; poss_fts.len()];
        for &(forage_target, shown_ft) in forage_targets {
            if forage_target == 1 {
                // fts may not be sequentially numbered
                if let Some(j) = poss_fts.iter().position(|&ft| ft == shown_ft) {
                    displayed_fts[j] += 1;
                }
            }
        }
        for (&ft, &count) in poss_fts.iter().zip(&displayed_fts) {
            df.write(ft, "Displayed FT");
            df.write(count, "Num Orgs Displaying this FT");
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintDemeNetworkData`; clears the network stats afterwards.
    pub fn print_deme_network_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Deme network statistics");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        for (name, sum) in &self.m_network_stats {
            df.write(sum.average(), name);
        }
        df.endl();
        self.m_network_stats.clear();
        Ok(())
    }

    /// C++: `PrintHGTData`; clears the HGT sums afterwards.
    pub fn print_hgt_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Horizontal gene transfer statistics");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        df.write(
            self.m_hgt_metabolized.count(),
            "Total count of metabolized genome fragments [metcount]",
        );
        df.write(
            self.m_hgt_metabolized.sum(),
            "Total size of metabolized genome fragments [metsize]",
        );
        df.write(
            self.m_hgt_inserted.count(),
            "Total count of insertion events [inscount]",
        );
        df.write(
            self.m_hgt_inserted.sum(),
            "Total size of insertion events [inssize]",
        );
        df.endl();
        self.m_hgt_metabolized.clear();
        self.m_hgt_inserted.clear();
        Ok(())
    }

    /// C++: `PrintMessageLog`; clears the log afterwards. `bool` columns go
    /// through `Write(int)`.
    pub fn print_message_log(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Log of all messages sent in population.");
        df.write_time_stamp();
        for e in &self.m_message_log {
            df.write(e.update, "Update [update]");
            df.write(e.deme, "Deme ID [deme]");
            df.write(e.src_cell, "Source [src]");
            df.write(e.dst_cell, "Destination [dst]");
            df.write(e.transmit_cell, "Transmission_cell [trs]");
            df.write(e.msg_data, "Message data [data]");
            df.write(e.msg_label, "Message label [label]");
            df.write(i32::from(e.dropped), "Dropped [dropped]");
            df.write(i32::from(e.lost), "Lost [lost]");
            df.endl();
        }
        self.m_message_log.clear();
        Ok(())
    }

    /// C++: `PrintRetMessageLog`; clears the log afterwards.
    pub fn print_ret_message_log(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Log of all messages sent in population.");
        df.write_time_stamp();
        for e in &self.m_retmessage_log {
            df.write(e.update, "Update [update]");
            df.write(e.deme, "Deme ID [deme]");
            df.write(e.src_cell, "Source [src]");
            df.write(e.dst_cell, "Destination [dst]");
            df.write(e.transmit_cell, "Transmission_cell [trs]");
            df.write(e.msg_data, "Message data [data]");
            df.write(e.msg_label, "Message label [label]");
            df.endl();
        }
        self.m_retmessage_log.clear();
        Ok(())
    }

    /// C++: `PrintIntrinsicTaskSwitchingCostData`; clears the switch times
    /// afterwards. QUIRK: the four `WriteColumnDesc` lines and the first
    /// row's four `Write` descriptions all reach the header (8 columns).
    pub fn print_intrinsic_task_switching_cost_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Number of cyles it takes to change between tasks");
        df.write_time_stamp();
        df.write_column_desc("Update [update]", "");
        df.write_column_desc("Task 1 [t1]", "");
        df.write_column_desc("Task 2 [t2]", "");
        df.write_column_desc("Mean cycles [mc]", "");
        for (&(t1, t2), sum) in self.intrinsic_task_switch_time.iter_mut() {
            df.write(self.m_update, "Update [update]");
            df.write(t1, "Task 1 [t1]");
            df.write(t2, "Task 2 [t2]");
            df.write(sum.average(), "Mean cycles [mc]");
            sum.clear();
            df.endl();
        }
        self.intrinsic_task_switch_time.clear();
        Ok(())
    }

    /// C++: `PrintAgePolyethismData`; clears each task's age sum afterwards.
    /// The task count is `env.GetNumTasks()`, i.e. `task_names.len()`.
    pub fn print_age_polyethism_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let num_tasks = self.task_names.len() as i32;
        let df = out.static_with_path(filename)?;
        df.write_comment("Tasks, mean organism age, and variance of ages");
        df.write_time_stamp();
        df.write_column_desc("Update [update]", "");
        for i in 0..num_tasks {
            df.write_column_desc(&format!("Task {i} Organism Age Mean [meanorgage{i}]"), "");
            df.write_column_desc(
                &format!("Task {i} Organism Age Standard Error [errorgage{i}]"),
                "",
            );
        }
        df.flush_comments();
        df.write(self.m_update, "Update");
        for i in 0..num_tasks {
            let av_comment = format!("Task {i} Organism Age Mean [meanorgage{i}]");
            let err_comment = format!("Task {i} Organism Age Standard Error [errorgage{i}]");
            let ages = self.reaction_age_map.entry(i).or_default();
            if ages.count() > 0.0 {
                df.write(ages.average(), &av_comment);
                df.write(ages.std_error(), &err_comment);
            } else {
                df.write(0, &av_comment);
                df.write(0, &err_comment);
            }
            ages.clear();
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintDemeReactionDiversityReplicationData`: keeps the last 100
    /// entries of each series and prints their means. QUIRK: two column
    /// descriptions differ between the empty and non-empty case, so the
    /// header depends on the first call.
    pub fn print_deme_reaction_diversity_replication_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida deme reaction diversity replication data");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");

        for series in [
            &mut self.m_switching,
            &mut self.m_deme_diversity,
            &mut self.m_shannon_div,
            &mut self.m_num_orgs_perf_reaction,
            &mut self.m_shannon_div_norm,
            &mut self.m_percent_reproductives,
        ] {
            while series.len() > 100 {
                series.pop_front();
            }
        }

        // std::accumulate(begin, end, 0.0) / size()
        let mean = |s: &std::collections::VecDeque<f64>| {
            let mut sum = 0.0;
            for &v in s {
                sum += v;
            }
            sum / s.len() as f64
        };
        let columns: [(&std::collections::VecDeque<f64>, &str, &str); 6] = [
            (
                &self.m_deme_diversity,
                "Mean number of different reactions by deme [demereact]",
                "Mean number of different reactions by deme [demereact]",
            ),
            (
                &self.m_switching,
                "Mean number of deme switch penalties per org  [orgpen]",
                "Mean number of deme switch penalties per org  [orgpen]",
            ),
            (
                &self.m_shannon_div,
                "Mean shannon mutual information per deme [shannon]",
                "Mean shannon mutual entropy [shannon]",
            ),
            (
                &self.m_shannon_div_norm,
                "Mean shannon normalized mutual information per deme [shannonnorm]",
                "Mean shannon normalalized mutual information [shannonnorm]",
            ),
            (
                &self.m_num_orgs_perf_reaction,
                "Mean number of orgs that perform a reaction [meanreact]",
                "Mean number of orgs that perform a reaction [meanreact]",
            ),
            (
                &self.m_percent_reproductives,
                "Mean percent of orgs that replicate [meanperrepros]",
                "Mean percent of orgs that replicate [meanperrepros]",
            ),
        ];
        for (series, empty_descr, descr) in columns {
            if series.is_empty() {
                df.write(0.0, empty_descr);
            } else {
                df.write(mean(series), descr);
            }
        }
        df.endl();
        Ok(())
    }

    /// C++: `PrintMultiProcessData`; clears the migrant counts afterwards.
    pub fn print_multi_process_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Avida-MP data");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        df.write(self.m_outgoing.count(), "Outgoing migrants [outgoing]");
        df.write(self.m_incoming.count(), "Incoming migrants [incoming]");
        df.endl();
        self.m_outgoing.clear();
        self.m_incoming.clear();
        Ok(())
    }

    /// C++: `PrintProfilingData`; clears the profiling stats afterwards.
    pub fn print_profiling_data(
        &mut self,
        out: &mut OutputManager,
        filename: &str,
    ) -> Result<(), String> {
        let df = out.static_with_path(filename)?;
        df.write_comment("Profiling statistics");
        df.write_time_stamp();
        df.write(self.m_update, "Update [update]");
        for (name, sum) in &self.m_profiling {
            df.write(sum.average(), name);
        }
        df.endl();
        self.m_profiling.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Stands for the `WriteTimeStamp` line in expected files.
    const STAMP: &str = "<time stamp>";
    const INST_SET: &str = "(default)";

    type InstPrinter = fn(&Stats, &mut OutputManager, &str, &str) -> Result<(), String>;
    type InstCounts = for<'a> fn(&'a mut Stats, &str) -> &'a mut Vec<Accumulator<i32>>;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn stats_with(tasks: &[&str], reactions: &[&str]) -> Stats {
        Stats::new(strings(tasks), strings(reactions), 0, 0.0)
    }

    fn stats_at(update: i32) -> Stats {
        let mut stats = stats_with(&[], &[]);
        stats.m_update = update;
        stats
    }

    fn acc(values: &[i32]) -> Accumulator<i32> {
        let mut a = Accumulator::new();
        values.iter().for_each(|&v| a.add(v));
        a
    }

    fn set_counts(counts: &mut Vec<Accumulator<i32>>, values: &[i32]) {
        *counts = values.iter().map(|&v| acc(&[v])).collect();
    }

    /// A fresh output directory for one test.
    fn output(tag: &str) -> (PathBuf, OutputManager) {
        let dir = std::env::temp_dir().join(format!(
            "avida-world-stats-print-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let out = OutputManager::new(dir.to_str().unwrap());
        (dir, out)
    }

    /// `# ` + `ctime()`, e.g. `# Fri Mar 20 08:37:22 2026`.
    fn is_stamp(line: &str) -> bool {
        let b = line.as_bytes();
        b.len() == 26 && line.starts_with("# ") && b[15] == b':' && b[18] == b':'
    }

    fn mask(text: &str) -> String {
        text.split('\n')
            .map(|l| {
                if is_stamp(l) {
                    format!("# {STAMP}")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn reference_text(rel: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../avida-core/tests")
            .join(rel);
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// The column names after `Update` and the integer rows of a reference
    /// file.
    fn reference_table(rel: &str) -> (Vec<String>, Vec<Vec<i32>>) {
        let mut names = Vec::new();
        let mut rows = Vec::new();
        for line in reference_text(rel).lines() {
            if let Some(rest) = line.strip_prefix("# ") {
                if let Some((num, name)) = rest.split_once(": ") {
                    if num.trim().parse::<usize>().is_ok() && name != "Update" {
                        names.push(name.to_string());
                    }
                }
            } else if !line.is_empty() {
                rows.push(
                    line.split_whitespace()
                        .map(|v| v.parse().unwrap())
                        .collect(),
                );
            }
        }
        (names, rows)
    }

    /// The whole file as written so far, time stamp masked.
    fn produced(out: &mut OutputManager, dir: &Path, file: &str) -> String {
        out.flush_all();
        let path = dir.join(file);
        mask(&fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
    }

    /// A header as `DataFile` writes it before its first row: a `# ` line
    /// per comment, the numbered column descriptions, a blank line.
    fn header<S: AsRef<str>>(comments: &[&str], columns: &[S]) -> String {
        let mut s = String::new();
        for c in comments {
            s.push_str(&format!("# {c}\n"));
        }
        for (i, col) in columns.iter().enumerate() {
            s.push_str(&format!("# {:2}: {}\n", i + 1, col.as_ref()));
        }
        s.push('\n');
        s
    }

    fn assert_file(out: &mut OutputManager, dir: &Path, file: &str, expected: &str) {
        assert_eq!(produced(out, dir, file), expected, "{file}");
    }

    /// Asserts that the file written so far is the start of the reference
    /// file `avida-core/tests/<reference>` (whole lines, time stamps masked).
    fn assert_reference_prefix(out: &mut OutputManager, dir: &Path, file: &str, reference: &str) {
        let got = produced(out, dir, file);
        assert!(got.ends_with('\n'), "{file} ends mid-line");
        let want = mask(&reference_text(reference));
        let n = got.matches('\n').count();
        let prefix: String = want.split_inclusive('\n').take(n).collect();
        assert_eq!(got, prefix, "{file} vs {reference}");
    }

    #[test]
    fn error_and_variance_data_match_reference() {
        let (dir, mut out) = output("error");
        let stats = stats_at(0);
        stats.print_error_data(&mut out, "error.dat").unwrap();
        stats.print_variance_data(&mut out, "variance.dat").unwrap();
        assert_reference_prefix(
            &mut out,
            &dir,
            "error.dat",
            "avatars/expected/data/error.dat",
        );
        assert_reference_prefix(
            &mut out,
            &dir,
            "variance.dat",
            "avatars/expected/data/variance.dat",
        );
    }

    #[test]
    fn parasite_data_matches_reference() {
        let (dir, mut out) = output("parasite");
        let mut stats = stats_at(601);
        stats.num_parasites = 8;
        stats
            .print_parasite_data(&mut out, "ParasiteData.dat")
            .unwrap();
        stats.m_update = 603;
        stats.num_parasites = 73;
        stats
            .print_parasite_data(&mut out, "ParasiteData.dat")
            .unwrap();
        assert_reference_prefix(
            &mut out,
            &dir,
            "ParasiteData.dat",
            "parasites_inject_stride/expected/data/ParasiteData.dat",
        );
    }

    #[test]
    fn totals_threads_and_message_data() {
        let (dir, mut out) = output("totals");
        let mut stats = stats_at(3);
        // QUIRK: the C++ total is an int; only the low 32 bits survive.
        stats.tot_executed = (1 << 32) + 5;
        stats.num_executed = 7;
        stats.tot_organisms = 40;
        stats.print_totals_data(&mut out, "totals.dat").unwrap();
        let columns = [
            "Update",
            "Total Instructions Executed",
            "Instructions Executed This Update",
            "Total Organisms",
            "(deprecated) Total Genotypes",
            "(deprecated) Total Threshold",
            "(deprecated) Total Species",
            "(deprecated) Total Lineages",
        ];
        assert_file(
            &mut out,
            &dir,
            "totals.dat",
            &(header(&[], &columns) + "3 12 7 40 0 0 0 0 \n"),
        );

        // pp-evolved: threads.dat at updates 0, 50 and 100.
        for (update, total, live, threads) in [
            (0, 916, 915, 1050),
            (50, 962, 936, 3055),
            (100, 1350, 929, 3630),
        ] {
            stats.m_update = update;
            stats.tot_organisms = total;
            stats.m_num_threads = threads;
            stats
                .print_threads_data(&mut out, "threads.dat", live)
                .unwrap();
        }
        assert_reference_prefix(
            &mut out,
            &dir,
            "threads.dat",
            "pp-evolved/expected/data/threads.dat",
        );

        // The unsigned totals wrap.
        stats.m_update = 8;
        stats
            .print_message_data(&mut out, "messages.dat", &[(u32::MAX, 1, 2), (2, 3, 4)])
            .unwrap();
        assert_file(
            &mut out,
            &dir,
            "messages.dat",
            &(header(
                &["Number of organism to organism messages\n"],
                &["update", "Sent successfully", "Dropped", "Failed"],
            ) + "8 1 4 6 \n"),
        );
    }

    #[test]
    fn reaction_printers() {
        let (dir, mut out) = output("reactions");
        let foods = [
            "food1", "food2", "food3", "food4", "food5", "food6", "food7", "food8", "food9",
        ];
        let mut stats = stats_with(&[], &foods);
        stats.m_update = 0;
        stats
            .print_reaction_data(&mut out, "reactions.dat")
            .unwrap();
        stats.m_update = 100;
        stats.m_reaction_last_count[0] = 50;
        stats
            .print_reaction_data(&mut out, "reactions.dat")
            .unwrap();
        assert_reference_prefix(
            &mut out,
            &dir,
            "reactions.dat",
            "avatars/expected/data/reactions.dat",
        );

        let mut stats = stats_with(&[], &["NAND"]);
        for (update, count) in [(0, 0), (10, 0), (20, 2)] {
            stats.m_update = update;
            stats.m_reaction_exe_count[0] = count;
            stats
                .print_reaction_exe_data(&mut out, "reactions_exe.dat")
                .unwrap();
        }
        assert_reference_prefix(
            &mut out,
            &dir,
            "reactions_exe.dat",
            "hoarding_collect_split_230u/expected/data/reactions_exe.dat",
        );

        let mut stats = stats_with(&[], &["NOT", "NAND"]);
        stats.m_update = 5;
        stats.m_reaction_cur_count = vec![3, 4];
        stats.m_reaction_last_add_reward = vec![1.5, 0.25];
        stats.m_reaction_cur_add_reward = vec![2.0, 0.125];
        stats
            .print_current_reaction_data(&mut out, "cur_reactions.dat")
            .unwrap();
        stats
            .print_reaction_reward_data(&mut out, "reward.dat")
            .unwrap();
        stats
            .print_current_reaction_reward_data(&mut out, "cur_reward.dat")
            .unwrap();
        let living = [
            "Avida reaction data",
            STAMP,
            "First column gives the current update, all further columns give the number",
            "of currently living organisms each reaction has affected.",
        ];
        let reward = [
            "Avida reaction data",
            STAMP,
            "First column gives the current update, all further columns give the add bonus reward",
            "currently living organisms have garnered from each reaction.",
        ];
        let columns = ["Update", "NOT", "NAND"];
        assert_file(
            &mut out,
            &dir,
            "cur_reactions.dat",
            &(header(&living, &columns) + "5 3 4 \n"),
        );
        assert_file(
            &mut out,
            &dir,
            "reward.dat",
            &(header(&reward, &columns) + "5 1.5 0.25 \n"),
        );
        assert_file(
            &mut out,
            &dir,
            "cur_reward.dat",
            &(header(&reward, &columns) + "5 2 0.125 \n"),
        );
    }

    #[test]
    fn host_and_parasite_task_printers() {
        let (dir, mut out) = output("host-parasite");
        let tasks = [
            "Not", "Nand", "And", "OrNot", "Or", "AndNot", "Nor", "Xor", "Equals",
        ];
        let mut stats = stats_with(&tasks, &[]);
        for update in [0, 100] {
            stats.m_update = update;
            stats
                .print_parasite_tasks_data(&mut out, "tasksq.dat")
                .unwrap();
        }
        assert_reference_prefix(
            &mut out,
            &dir,
            "parasite_tasks.dat",
            "parasites_inject_stride/expected/data/parasite_tasks.dat",
        );

        stats.add_last_host_task(1);
        stats.add_last_host_task(1);
        stats.add_cur_host_task(0); // current counts are not printed
        stats.print_host_tasks_data(&mut out, "tasksq.dat").unwrap();
        let mut columns = vec!["Update"];
        columns.extend(tasks);
        let comments = [
            "Avida Host Tasks data",
            STAMP,
            "First column gives the current update, next columns give the number",
            "of Hosts that have the particular task",
        ];
        assert_file(
            &mut out,
            &dir,
            "host_tasks.dat",
            &(header(&comments, &columns) + "100 0 2 0 0 0 0 0 0 0 \n"),
        );
    }

    #[test]
    fn new_task_and_reaction_printers_reset_their_counts() {
        let (dir, mut out) = output("new-tasks");
        let mut stats = stats_with(&["Not", "Nand"], &["NOT", "NAND"]);
        stats.m_update = 4;
        stats.new_task_count = vec![2, 0];
        stats
            .print_new_tasks_data(&mut out, "newtasks.dat")
            .unwrap();
        assert_eq!(stats.new_task_count, [0, 0]);

        stats.new_task_count = vec![2, 0];
        stats.prev_task_count = vec![3, 0];
        stats.cur_task_count = vec![5, 0];
        stats
            .print_new_tasks_data_plus(&mut out, "newtasksplus.dat")
            .unwrap();
        assert_eq!(stats.new_task_count, [0, 0]);
        assert_eq!(stats.prev_task_count, [0, 0]);
        assert_eq!(stats.cur_task_count, [0, 0]);

        stats.new_reaction_count = vec![0, 1];
        stats
            .print_new_reaction_data(&mut out, "newreactions.dat")
            .unwrap();
        assert_eq!(stats.new_reaction_count, [0, 0]);

        let tasks_comments = [
            "Avida new tasks data",
            STAMP,
            "First column gives the current update, all further columns give the number",
            "of times the particular task has newly evolved since the last time printed.",
        ];
        assert_file(
            &mut out,
            &dir,
            "newtasks.dat",
            &(header(&tasks_comments, &["Update", "Not", "Nand"]) + "4 2 0 \n"),
        );
        let plus_comments = [
            "Avida new tasks data",
            STAMP,
            "First column gives the current update, all further columns are in sets of 3, giving the number",
            "of times the particular task has newly evolved since the last time printed, then the average",
            "number of tasks the parent of the organism evolving the new task performed, then the average",
            "number of tasks the organism evolving the new task performed.  One set of 3 for each task",
        ];
        let plus_columns = [
            "Update",
            "Not - num times newly evolved",
            "ave num tasks parent performed",
            "ave num tasks cur org performed",
            "Nand - num times newly evolved",
            "ave num tasks parent performed",
            "ave num tasks cur org performed",
        ];
        assert_file(
            &mut out,
            &dir,
            "newtasksplus.dat",
            &(header(&plus_comments, &plus_columns) + "4 2 1.5 2.5 0 -1 -1 \n"),
        );
        let reaction_comments = [
            "Avida new reactions data",
            STAMP,
            "First column gives the current update, all further columns give the number",
            "of times the particular reaction has newly evolved since the last time printed.",
        ];
        assert_file(
            &mut out,
            &dir,
            "newreactions.dat",
            &(header(&reaction_comments, &["Update", "NOT", "NAND"]) + "4 0 1 \n"),
        );
    }

    #[test]
    fn dynamic_max_min_data() {
        let (dir, mut out) = output("max-min");
        let mut stats = stats_with(&["Not", "Nand"], &[]);
        stats.m_update = 1;
        stats
            .print_dynamic_max_min_data(&mut out, "maxmin.dat", &[(1.0, 0.5), (2.0, -1.0)])
            .unwrap();
        let comments = [
            "Avida dynamic max min data",
            STAMP,
            "First column gives the current update, 2nd and 3rd give max and min Fx",
        ];
        let columns = ["Update", "Not Max", "Not Min", "Nand Max", "Nand Min"];
        assert_file(
            &mut out,
            &dir,
            "maxmin.dat",
            &(header(&comments, &columns) + "1 1 0.5 2 -1 \n"),
        );
        assert!(stats
            .print_dynamic_max_min_data(&mut out, "maxmin2.dat", &[(1.0, 0.5)])
            .is_err());
    }

    #[test]
    fn spatial_res_data_matches_reference() {
        let reference = "spatial_res_100u/expected/data/resource_ResA.m";
        let text = reference_text(reference);
        let block10: Vec<f64> = text
            .split("ResA0000010 = [ ...\n")
            .nth(1)
            .unwrap()
            .split("];")
            .next()
            .unwrap()
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert_eq!(block10.len(), 100);

        let (dir, mut out) = output("spatial");
        let mut stats = Stats::new(Vec::new(), Vec::new(), 1, 0.0);
        stats.resource_names = strings(&["ResA"]);
        stats.spatial_res_count = vec![vec![0.2; 100]];
        stats.m_update = 0;
        stats
            .print_spatial_res_data(&mut out, "unused", 0, 10)
            .unwrap();
        stats.m_update = 10;
        stats.spatial_res_count[0] = block10;
        stats
            .print_spatial_res_data(&mut out, "unused", 0, 10)
            .unwrap();
        assert_reference_prefix(&mut out, &dir, "resource_ResA.m", reference);

        assert!(stats
            .print_spatial_res_data(&mut out, "unused", 1, 10)
            .is_err());
        assert!(stats
            .print_spatial_res_data(&mut out, "unused", 0, 0)
            .is_err());
    }

    #[test]
    fn mutation_rate_printers() {
        let (dir, mut out) = output("mut-rates");
        let mut stats = stats_at(2);
        for x in [1.0, 3.0] {
            stats.sum_copy_mut_rate.push(x);
            stats.sum_log_copy_mut_rate.push(x);
            stats.sum_div_mut_rate.push(x);
            stats.sum_log_div_mut_rate.push(x);
        }
        stats
            .print_mutation_rate_data(&mut out, "mutation_rates.dat")
            .unwrap();
        stats
            .print_divide_mut_data(&mut out, "divide_mutation_rates.dat")
            .unwrap();
        let columns = |kind: &str| {
            let mut cols = vec!["Update".to_string()];
            for rate in [
                format!("{kind} mutation rate"),
                format!("log({kind} mutation rate)"),
            ] {
                cols.push(format!("Average {rate}"));
                cols.push(format!("Variance in {rate}"));
                cols.push(format!("Standard Deviation in {rate}"));
                cols.push(format!("Skew in {rate}"));
                cols.push(format!("Kurtosis in {rate}"));
            }
            cols
        };
        let row = "2 2 2 1.41421 0 1 2 2 1.41421 0 1 \n";
        assert_file(
            &mut out,
            &dir,
            "mutation_rates.dat",
            &(header(&["Avida copy mutation rate data", STAMP], &columns("copy")) + row),
        );
        assert_file(
            &mut out,
            &dir,
            "divide_mutation_rates.dat",
            &(header(
                &["Avida divide mutation rate data", STAMP],
                &columns("divide"),
            ) + row),
        );
    }

    #[test]
    fn sense_printers() {
        let (dir, mut out) = output("sense");
        let mut stats = stats_at(3);
        stats.print_sense_data(&mut out, "sense.dat").unwrap();
        let comments = [
            "Avida sense instruction usage\n",
            "total number of organisms whose parents executed sense instructions with given labels",
        ];
        assert_file(
            &mut out,
            &dir,
            "sense.dat",
            &(header(&comments, &["update"]) + "3 \n"),
        );

        stats.sense_names = strings(&["a", "b"]);
        stats.sense_last_exe_count = vec![1, 2];
        stats
            .print_sense_exe_data(&mut out, "sense_exe.dat")
            .unwrap();
        let comments = [
            "Avida sense instruction usage\n",
            "total number of sense instructions executed by the parents of current organisms with given labels",
        ];
        assert_file(
            &mut out,
            &dir,
            "sense_exe.dat",
            &(header(&comments, &["update", "a", "b"]) + "3 1 2 \n"),
        );

        stats.sense_last_count = vec![1, 2, 3];
        assert!(stats.print_sense_data(&mut out, "sense2.dat").is_err());
    }

    #[test]
    fn internal_task_printers() {
        let (dir, mut out) = output("internal");
        let mut stats = stats_with(&["Not", "Nand"], &[]);
        stats.m_update = 6;
        stats.task_internal_last_count = vec![2, 0];
        stats.task_internal_last_quality = vec![1.5, 0.0];
        stats.task_internal_last_max_quality = vec![1.0, 0.0];
        stats
            .print_internal_tasks_data(&mut out, "in_tasksq.dat")
            .unwrap();
        let comments = [
            "Avida tasks data: tasks performed with internal resources",
            STAMP,
            "First column gives the current update, next columns give the number",
            "of organisms that have the particular task, performed with internal resources, ",
            "as a component of their merit",
        ];
        assert_file(
            &mut out,
            &dir,
            "in_tasks.dat",
            &(header(&comments, &["Update", "Not", "Nand"]) + "6 2 0 \n"),
        );
        let comments = [
            "Avida tasks quality data: tasks performed using internal resources",
            STAMP,
            "First column gives the current update, rest give average and max task quality ",
            "for those tasks performed using internal resources",
        ];
        let columns = [
            "Update",
            "Not Average",
            "Not Max",
            "Nand Average",
            "Nand Max",
        ];
        assert_file(
            &mut out,
            &dir,
            "in_taskquality.dat",
            &(header(&comments, &columns) + "6 0.75 1 0 0 \n"),
        );
    }

    #[test]
    fn sleep_data_matches_reference() {
        let (dir, mut out) = output("sleep");
        let mut stats = stats_at(16);
        stats.print_sleep_data(&mut out, "sleep.dat", &[0]).unwrap();
        stats.m_update = 32;
        stats.print_sleep_data(&mut out, "sleep.dat", &[0]).unwrap();
        assert_reference_prefix(
            &mut out,
            &dir,
            "sleep.dat",
            "energy_asex_1000u/expected/data/sleep.dat",
        );
    }

    #[test]
    fn competition_data_prints_trials_only_when_several() {
        let (dir, mut out) = output("competition");
        let mut stats = stats_at(9);
        stats.set_competition_fitnesses(1.5, 1.0, 2.0, 1.25, 0.5, 3.0);
        stats.set_competition_orgs_replicated(40);
        stats.set_competition_trial_fitnesses(vec![1.5]);
        stats
            .print_competition_data(&mut out, "competition.dat")
            .unwrap();
        stats.set_competition_trial_fitnesses(vec![1.0, 2.0]);
        stats
            .print_competition_data(&mut out, "competition_trials.dat")
            .unwrap();

        let comments = [
            "Competition results\n",
            "results of the current competitions",
        ];
        let mut columns = vec![
            "update",
            "average competition fitness",
            "min competition fitness",
            "max competition fitness",
            "average copied fitness",
            "min copied fitness",
            "max copied fitness",
            "number of organisms copied",
        ];
        assert_file(
            &mut out,
            &dir,
            "competition.dat",
            &(header(&comments, &columns) + "9 1.5 1 2 1.25 0.5 3 40 \n"),
        );
        columns.extend(["trial.0 fitness", "trial.1 fitness"]);
        assert_file(
            &mut out,
            &dir,
            "competition_trials.dat",
            &(header(&comments, &columns) + "9 1.5 1 2 1.25 0.5 3 40 1 2 \n"),
        );
    }

    #[test]
    fn deme_average_and_flow_rate_printers() {
        let (dir, mut out) = output("deme-average");
        let mut stats = stats_at(12);
        stats.m_num_occupied_demes = 2;
        stats.sum_deme_age = acc(&[10, 20]);
        stats.sum_deme_birth_count = acc(&[4]);
        stats.sum_deme_org_count = acc(&[6]);
        stats.sum_deme_generation = acc(&[1]);
        stats.sum_deme_last_birth_count = acc(&[2]);
        stats.sum_deme_last_org_count = acc(&[3]);
        stats.sum_deme_merit.add(100.0);
        stats.sum_deme_gestation_time = acc(&[50]);
        stats.sum_deme_normalized_time_used.add(0.5);
        stats.sum_deme_generations_per_lifetime.add(1.5);
        stats.sum_deme_events_killed = acc(&[0]);
        stats.sum_deme_events_kill_attempts = acc(&[1]);
        stats
            .print_deme_average_data(&mut out, "deme_average.dat")
            .unwrap();
        let columns = [
            "Update",
            "Count",
            "Age",
            "Births",
            "Organisms",
            "Generation",
            "Births (at last replication)",
            "Organisms (at last replication)",
            "Merit",
            "Gestation Time",
            "Time Used (normalized by org fitness)",
            "Generations between current and last founders",
            "Events killed",
            "Attempts to kill event",
        ];
        assert_file(
            &mut out,
            &dir,
            "deme_average.dat",
            &(header(&["Avida Average Deme Data", STAMP], &columns)
                + "12 2 15 4 6 1 2 3 100 50 0.5 1.5 0 1 \n"),
        );

        let mut five = FlowRateTuple {
            org_count: acc(&[4, 6]),
            events_killed: acc(&[1]),
            attempts_to_kill_events: acc(&[2]),
            total_births: acc(&[3]),
            current_sleeping: acc(&[0]),
            ..FlowRateTuple::default()
        };
        five.avg_energy_usage_ratio.add(0.5);
        let mut ten = FlowRateTuple {
            org_count: acc(&[8]),
            events_killed: acc(&[0]),
            attempts_to_kill_events: acc(&[0]),
            total_births: acc(&[1]),
            current_sleeping: acc(&[2]),
            ..FlowRateTuple::default()
        };
        ten.avg_energy_usage_ratio.add(1.0);
        stats.flow_rate_tuples.insert(10, ten);
        stats.flow_rate_tuples.insert(5, five);
        stats
            .print_flow_rate_tuples(&mut out, "flow_rates.dat")
            .unwrap();
        let mut columns = vec!["Update".to_string()];
        for rate in [5, 10] {
            let r = format!("flow rate {rate}");
            columns.push(r.clone());
            for suffix in [
                "deme pop size",
                "events killed",
                "events attempted to kill",
                "exe ratio",
                "total births",
                "total sleeping",
            ] {
                columns.push(format!("{r} {suffix}"));
            }
        }
        assert_file(
            &mut out,
            &dir,
            "flow_rates.dat",
            &(header(&["Flow Rate Tuples", STAMP], &columns)
                + "12 5 5 1 2 0.5 3 0 10 8 0 0 1 1 2 \n"),
        );
        assert!(stats
            .flow_rate_tuples
            .values()
            .all(|t| t.org_count.count() == 0 && t.avg_energy_usage_ratio.count() == 0.0));
    }

    #[test]
    fn predator_prey_and_mating_sums() {
        // avatars: update 0 rows.
        let (dir, mut out) = output("pred-prey-reference");
        let mut stats = stats_at(0);
        stats.sum_prey_merit.add(109.0);
        stats.sum_prey_creature_age.add(1.0);
        stats
            .print_prey_average_data(&mut out, "prey_average.dat")
            .unwrap();
        stats
            .print_prey_error_data(&mut out, "prey_error.dat")
            .unwrap();
        for update in [0, 10] {
            stats.m_update = update;
            stats
                .print_predator_variance_data(&mut out, "predator_variance.dat")
                .unwrap();
        }
        for file in [
            "prey_average.dat",
            "prey_error.dat",
            "predator_variance.dat",
        ] {
            assert_reference_prefix(
                &mut out,
                &dir,
                file,
                &format!("avatars/expected/data/{file}"),
            );
        }

        // Every printer, with two samples per statistic.
        let (dir, mut out) = output("pred-prey");
        let mut stats = stats_at(7);
        let fill = |sums: [&mut DoubleSum; 6]| {
            let samples: [&[f64]; 6] = [
                &[1.0, 3.0],
                &[10.0, 30.0],
                &[100.0, 100.0],
                &[1.0, 2.0],
                &[0.0, 0.0],
                &[],
            ];
            for (sum, values) in sums.into_iter().zip(samples) {
                values.iter().for_each(|&v| sum.add(v));
            }
        };
        fill([
            &mut stats.sum_prey_fitness,
            &mut stats.sum_prey_gestation,
            &mut stats.sum_prey_merit,
            &mut stats.sum_prey_creature_age,
            &mut stats.sum_prey_generation,
            &mut stats.sum_prey_size,
        ]);
        fill([
            &mut stats.sum_pred_fitness,
            &mut stats.sum_pred_gestation,
            &mut stats.sum_pred_merit,
            &mut stats.sum_pred_creature_age,
            &mut stats.sum_pred_generation,
            &mut stats.sum_pred_size,
        ]);
        fill([
            &mut stats.sum_tpred_fitness,
            &mut stats.sum_tpred_gestation,
            &mut stats.sum_tpred_merit,
            &mut stats.sum_tpred_creature_age,
            &mut stats.sum_tpred_generation,
            &mut stats.sum_tpred_size,
        ]);
        fill([
            &mut stats.sum_male_fitness,
            &mut stats.sum_male_gestation,
            &mut stats.sum_male_merit,
            &mut stats.sum_male_creature_age,
            &mut stats.sum_male_generation,
            &mut stats.sum_male_size,
        ]);
        fill([
            &mut stats.sum_female_fitness,
            &mut stats.sum_female_gestation,
            &mut stats.sum_female_merit,
            &mut stats.sum_female_creature_age,
            &mut stats.sum_female_generation,
            &mut stats.sum_female_size,
        ]);
        stats.prey_entropy = 0.75;
        stats.pred_entropy = 0.5;
        stats.tpred_entropy = 0.25;

        type Printer = fn(&Stats, &mut OutputManager, &str) -> Result<(), String>;
        let average = "7 2 20 100 1.5 0 0";
        let error = "7 1 10 0 0.5 0 0 \n";
        let variance = "7 2 200 0 0.5 0 0 \n";
        let cases: [(Printer, &str, Option<&str>, String); 15] = [
            (
                Stats::print_prey_average_data,
                "Prey Average Data",
                Some("Total Prey Genotypic Entropy"),
                format!("{average} 0.75 \n"),
            ),
            (
                Stats::print_predator_average_data,
                "Predator Average Data",
                Some("Total Predator Genotypic Entropy"),
                format!("{average} 0.5 \n"),
            ),
            (
                Stats::print_top_predator_average_data,
                "Top predator Average Data",
                Some("Total Top Predator Genotypic Entropy"),
                format!("{average} 0.25 \n"),
            ),
            (
                Stats::print_prey_error_data,
                "Prey Standard Error Data",
                None,
                error.to_string(),
            ),
            (
                Stats::print_predator_error_data,
                "Predator Standard Error Data",
                None,
                error.to_string(),
            ),
            (
                Stats::print_top_predator_error_data,
                "Top predator Standard Error Data",
                None,
                error.to_string(),
            ),
            (
                Stats::print_prey_variance_data,
                "Prey Variance Data",
                None,
                variance.to_string(),
            ),
            (
                Stats::print_predator_variance_data,
                "Predator Variance Data",
                None,
                variance.to_string(),
            ),
            (
                Stats::print_top_predator_variance_data,
                "Top predator Variance Data",
                None,
                variance.to_string(),
            ),
            (
                Stats::print_male_average_data,
                "Male Average Data",
                None,
                format!("{average} \n"),
            ),
            (
                Stats::print_female_average_data,
                "Female Average Data",
                None,
                format!("{average} \n"),
            ),
            (
                Stats::print_male_error_data,
                "Male Standard Error Data",
                None,
                error.to_string(),
            ),
            (
                Stats::print_female_error_data,
                "Female Standard Error Data",
                None,
                error.to_string(),
            ),
            (
                Stats::print_male_variance_data,
                "Male Variance Data",
                None,
                variance.to_string(),
            ),
            (
                Stats::print_female_variance_data,
                "Female Variance Data",
                None,
                variance.to_string(),
            ),
        ];
        for (i, (print, comment, entropy, row)) in cases.into_iter().enumerate() {
            let file = format!("sums{i}.dat");
            print(&stats, &mut out, &file).unwrap();
            let mut columns = vec![
                "Update",
                "Fitness",
                "Gestation Time",
                "Merit",
                "Creature Age",
                "Generation",
                "Genome Length",
            ];
            columns.extend(entropy);
            assert_file(
                &mut out,
                &dir,
                &file,
                &(header(&[comment, STAMP], &columns) + &row),
            );
        }
    }

    fn check_inst_printer(
        tag: &str,
        reference: &str,
        rows: usize,
        counts: InstCounts,
        print: InstPrinter,
    ) {
        let (names, table) = reference_table(reference);
        let file = Path::new(reference).file_name().unwrap().to_str().unwrap();
        let (dir, mut out) = output(tag);
        let mut stats = stats_at(0);
        stats.set_inst_names(INST_SET, names);
        for row in table.iter().take(rows) {
            stats.m_update = row[0];
            set_counts(counts(&mut stats, INST_SET), &row[1..]);
            print(&stats, &mut out, file, INST_SET).unwrap();
        }
        assert_reference_prefix(&mut out, &dir, file, reference);
    }

    #[test]
    fn instruction_printers_match_reference() {
        check_inst_printer(
            "prey-inst",
            "avatars/expected/data/prey_instruction.dat",
            3,
            Stats::inst_prey_exe_counts_for_inst_set,
            Stats::print_prey_instruction_data,
        );
        check_inst_printer(
            "pred-inst",
            "pp-evolved/expected/data/predator_instruction.dat",
            3,
            Stats::inst_pred_exe_counts_for_inst_set,
            Stats::print_predator_instruction_data,
        );
        check_inst_printer(
            "prey-sensor",
            "pp-evolved/expected/data/prey_from_sensor_exec.dat",
            3,
            Stats::inst_prey_from_sensor_exe_counts_for_inst_set,
            Stats::print_prey_from_sensor_instruction_data,
        );
        check_inst_printer(
            "pred-sensor",
            "pp-evolved/expected/data/predator_from_sensor_exec.dat",
            3,
            Stats::inst_pred_from_sensor_exe_counts_for_inst_set,
            Stats::print_predator_from_sensor_instruction_data,
        );
    }

    #[test]
    fn top_predator_and_mating_instruction_printers() {
        let (dir, mut out) = output("tpred-mating-inst");
        let mut stats = stats_at(1);
        stats.set_inst_names(INST_SET, strings(&["nop-A", "nop-B", "nop-C"]));
        set_counts(
            stats.inst_top_pred_exe_counts_for_inst_set(INST_SET),
            &[1, 2, 3],
        );
        set_counts(
            stats.inst_top_pred_from_sensor_exe_counts_for_inst_set(INST_SET),
            &[4, 5, 6],
        );
        stats
            .print_top_predator_instruction_data(&mut out, "tpred.dat", INST_SET)
            .unwrap();
        stats
            .print_top_predator_from_sensor_instruction_data(&mut out, "tpred_sensor.dat", INST_SET)
            .unwrap();
        let columns = ["Update", "nop-A", "nop-B", "nop-C"];
        assert_file(
            &mut out,
            &dir,
            "tpred.dat",
            &(header(
                &["Top predator org instruction execution data", STAMP],
                &columns,
            ) + "1 1 2 3 \n"),
        );
        assert_file(
            &mut out,
            &dir,
            "tpred_sensor.dat",
            &(header(
                &[
                    "Top predator org instruction execution data using values originating from sensory input",
                    STAMP,
                ],
                &columns,
            ) + "1 4 5 6 \n"),
        );

        // QUIRK: the male/female column count is the predator map's.
        set_counts(stats.inst_pred_exe_counts_for_inst_set(INST_SET), &[0, 0]);
        set_counts(
            stats.inst_male_exe_counts_for_inst_set(INST_SET),
            &[4, 5, 6],
        );
        set_counts(
            stats.inst_female_exe_counts_for_inst_set(INST_SET),
            &[7, 8, 9],
        );
        stats
            .print_male_instruction_data(&mut out, "male.dat", INST_SET)
            .unwrap();
        stats
            .print_female_instruction_data(&mut out, "female.dat", INST_SET)
            .unwrap();
        let columns = ["Update", "nop-A", "nop-B"];
        assert_file(
            &mut out,
            &dir,
            "male.dat",
            &(header(&["Male instruction execution data", STAMP], &columns) + "1 4 5 \n"),
        );
        assert_file(
            &mut out,
            &dir,
            "female.dat",
            &(header(&["Female instruction execution data", STAMP], &columns) + "1 7 8 \n"),
        );
        // More predator counters than male ones: out of bounds in C++.
        set_counts(stats.inst_pred_exe_counts_for_inst_set(INST_SET), &[0; 4]);
        assert!(stats
            .print_male_instruction_data(&mut out, "male2.dat", INST_SET)
            .is_err());
    }

    #[test]
    fn zeroing_instruction_counters() {
        let mut stats = stats_at(0);
        let key = INST_SET.to_string();
        for map in [
            &mut stats.m_is_from_message_inst_map,
            &mut stats.m_is_prey_exe_inst_map,
            &mut stats.m_is_pred_exe_inst_map,
            &mut stats.m_is_prey_fail_exe_inst_map,
            &mut stats.m_is_pred_fail_exe_inst_map,
            &mut stats.m_is_tpred_exe_inst_map,
            &mut stats.m_is_prey_from_sensor_inst_map,
            &mut stats.m_is_pred_from_sensor_inst_map,
            &mut stats.m_is_tpred_from_sensor_inst_map,
            &mut stats.m_is_male_exe_inst_map,
            &mut stats.m_is_female_exe_inst_map,
        ] {
            map.insert(key.clone(), vec![acc(&[3])]);
        }
        let sum = |map: &BTreeMap<String, Vec<Accumulator<i32>>>| map[INST_SET][0].sum();

        stats.zero_ft_inst();
        assert_eq!(sum(&stats.m_is_prey_exe_inst_map), 0);
        assert_eq!(sum(&stats.m_is_pred_fail_exe_inst_map), 0);
        assert_eq!(sum(&stats.m_is_tpred_from_sensor_inst_map), 0);
        // QUIRK: ZeroFTInst skips the prey-fail map.
        assert_eq!(sum(&stats.m_is_prey_fail_exe_inst_map), 3);
        assert_eq!(sum(&stats.m_is_male_exe_inst_map), 3);
        assert_eq!(sum(&stats.m_is_from_message_inst_map), 3);

        stats.zero_mt_inst();
        assert_eq!(sum(&stats.m_is_male_exe_inst_map), 0);
        assert_eq!(sum(&stats.m_is_female_exe_inst_map), 0);
        stats.zero_message_inst();
        assert_eq!(sum(&stats.m_is_from_message_inst_map), 0);
    }

    #[test]
    fn group_attack_data() {
        let (dir, mut out) = output("group-attack");
        let mut stats = stats_at(4);
        stats.set_inst_names(
            INST_SET,
            strings(&["nop-A", "attack-prey", "nop-B", "attack-prey-group"]),
        );
        stats.set_group_attack_inst_names(INST_SET);
        assert_eq!(
            stats.group_attack_insts(INST_SET),
            ["attack-prey", "attack-prey-group"]
        );
        for inst in ["attack-prey", "attack-prey-group"] {
            stats
                .exec_counts_for_group_attack_inst(INST_SET, inst)
                .resize(20, Accumulator::new());
        }
        stats.exec_counts_for_group_attack_inst(INST_SET, "attack-prey")[1].add(2);
        stats.exec_counts_for_group_attack_inst(INST_SET, "attack-prey-group")[19].add(5);
        stats
            .print_group_attack_data(&mut out, "attack_data.dat", INST_SET)
            .unwrap();

        let mut expected = format!(
            "# Attack Instruction Execution Circumstances\n\
             # Gives count of attacks with given number of neighbors present.\n\
             # {STAMP}\n# Instruction\n# Update\n"
        );
        for kind in ["Neighbors", "InGroupNeighbors"] {
            for n in 0..=8 {
                expected.push_str(&format!("# {n}{kind}\n"));
            }
            expected.push_str(&format!("# 8+{kind}\n"));
        }
        expected.push('\n');
        let mut prey = vec!["0"; 20];
        prey[1] = "2";
        let mut group = vec!["0"; 20];
        group[19] = "5";
        expected.push_str(&format!("4,attack-prey,{}\n", prey.join(",")));
        expected.push_str(&format!("4,attack-prey-group,{}\n", group.join(",")));
        assert_file(&mut out, &dir, "attack_data.dat", &expected);

        stats.zero_group_attack_inst();
        assert_eq!(
            stats.exec_counts_for_group_attack_inst(INST_SET, "attack-prey-group")[19].sum(),
            0
        );

        // No counters for an attack instruction: out of bounds in C++.
        stats.set_inst_names("other", strings(&["attack-prey"]));
        stats.set_group_attack_inst_names("other");
        assert!(stats
            .print_group_attack_data(&mut out, "attack_data2.dat", "other")
            .is_err());
    }

    #[test]
    fn group_attack_bits_appends_the_raw_byte() {
        let (dir, mut out) = output("attack-bits");
        let stats = stats_at(7);
        stats.print_group_attack_bits(&mut out, 0xA5).unwrap();
        stats.print_group_attack_bits(&mut out, b'x').unwrap();
        out.flush_all();
        let bytes = fs::read(dir.join("group_attack_bits-7.dat")).unwrap();
        let (head, tail) = bytes.split_at(bytes.len() - 4);
        assert_eq!(tail, [0xA5, b'\n', b'x', b'\n']);
        let expected = format!(
            "# Condensed Group Attack Results during this update (this is a binary file to be \
             post-process converted to bit string -- left to right bits): \n\
             # Size (3 bit): potential group size, not including self \n\
             # Success: (2 bit) 0 == sucess, 1 == no prey failure, 2 == no friends failure, \
             3 == chance failure \n\
             # Share (2 bit): 0 == no, 1 == yes, 2 == fake \n\
             # Inst (1 bit): 0 == solo attack inst, 1 == group attack inst\n\
             # {STAMP}\n\n\n"
        );
        assert_eq!(mask(std::str::from_utf8(head).unwrap()), expected);
    }

    #[test]
    fn update_string_rows() {
        let (dir, mut out) = output("string-rows");
        let mut stats = stats_at(3);
        stats
            .print_group_attack_string(&mut out, "2,0,1,1")
            .unwrap();
        stats.print_look_data(&mut out, "a,b").unwrap();
        stats.print_look_data_output(&mut out, "c").unwrap();
        stats.m_update = 4;
        stats.print_look_ex_data_output(&mut out, "d").unwrap();
        stats
            .print_group_attack_string(&mut out, "0,3,0,0")
            .unwrap();

        let comments = |first: &str, rest: &[&str]| {
            let mut s = format!("# {first}\n# {STAMP}\n");
            for c in rest {
                s.push_str(&format!("# {c}\n"));
            }
            s + "\n"
        };
        assert_file(
            &mut out,
            &dir,
            "attack_outcomes.dat",
            &(comments(
                "Group Attack Circumstances",
                &[
                    "Update",
                    "Size: potential group size, not including self",
                    "Success: 0 == sucess, 1 == no prey failure, 2 == no friends failure, 3 == chance failure",
                    "SharedKill: 0 == no, 1 == yes, 2 == fake",
                    "Inst: 0 == solo attack inst, 1 == group attack inst",
                ],
            ) + "3,2,0,1,1\n4,0,3,0,0\n"),
        );
        let look = [
            "Update",
            "ThisOrgForageTarget",
            "AnyFound",
            "Habitat",
            "Distance",
            "SearchType",
            "SoughtID",
        ];
        assert_file(
            &mut out,
            &dir,
            "looksettings.dat",
            &(comments("Final (used) Look Settings", &look) + "3,a,b\n"),
        );
        // QUIRK: PrintLookEXDataOutput shares the file and its header.
        let mut look_output = look.to_vec();
        look_output.extend(["Count", "Value", "Group", "FT"]);
        assert_file(
            &mut out,
            &dir,
            "lookoutput.dat",
            &(comments("Final (used) Look Output to Registers", &look_output) + "3,c\n4,d\n"),
        );
    }

    #[test]
    fn killed_prey_and_attack_printers() {
        let (dir, mut out) = output("attacks");
        let mut stats = stats_at(5);
        stats
            .print_killed_prey_ft_data(
                &mut out,
                "killed.dat",
                &[2, 7],
                &[vec![1, 0], vec![2, 3], vec![]],
            )
            .unwrap();
        assert_file(
            &mut out,
            &dir,
            "killed.dat",
            &(header(
                &[
                    "Record of orgs killed by predators by victim forage type.",
                    STAMP,
                ],
                &[
                    "Update",
                    "Killed FT ID",
                    "Num Orgs Killed",
                    "Killed FT ID",
                    "Num Orgs Killed",
                ],
            ) + "5 2 3 7 3 \n"),
        );
        assert!(stats
            .print_killed_prey_ft_data(&mut out, "killed2.dat", &[2], &[vec![1, 1]])
            .is_err());

        let columns = [
            "Update",
            "Num Predators",
            "Total Attacks",
            "Max Attacks",
            "Average Attacks",
            "Total Kills",
            "Max Kills",
            "Average Kills",
        ];
        let comments = ["Predator Average Attack and Kills Data", STAMP];
        // QUIRK: an empty cDoubleSum's Max() is DBL_MIN.
        stats.print_attacks(&mut out, "attacks_empty.dat").unwrap();
        assert_file(
            &mut out,
            &dir,
            "attacks_empty.dat",
            &(header(&comments, &columns) + "5 0 0 2.22507e-308 0 0 2.22507e-308 0 \n"),
        );
        stats.sum_attacks.add(2.0);
        stats.sum_attacks.add(4.0);
        stats.sum_kills.add(1.0);
        stats.sum_kills.add(0.0);
        stats.print_attacks(&mut out, "attacks.dat").unwrap();
        assert_file(
            &mut out,
            &dir,
            "attacks.dat",
            &(header(&comments, &columns) + "5 2 6 4 3 1 1 0.5 \n"),
        );
    }

    #[test]
    fn germline_and_founder_printers() {
        let (dir, mut out) = output("founders");
        let mut stats = stats_at(2);
        stats.germline_replication(3);
        stats.germline_replication(5);
        stats.print_germline_data(&mut out, "germline.dat").unwrap();
        assert_eq!(stats.m_germline_generation.count(), 0.0);
        assert_file(
            &mut out,
            &dir,
            "germline.dat",
            &(header(
                &["Avida germline data", STAMP],
                &[
                    "Update [update]",
                    "Mean germline generation of replicated germlines [replgen]",
                ],
            ) + "2 4 \n"),
        );

        stats.track_deme_gls_replication(
            0,
            1,
            vec![(5, "abc".to_string()), (6, "def".to_string())],
        );
        stats
            .print_deme_gls_founders(&mut out, "gls_founders.dat")
            .unwrap();
        assert!(stats.m_gls_deme_founders.is_empty());
        // FlushComments: no blank line before the rows.
        assert_file(
            &mut out,
            &dir,
            "gls_founders.dat",
            &format!(
                "# Avida gls deme founder data.\n# {STAMP}\n\
                 #  1: Update [update]\n#  2: Soure Deme ID [sdemeid]\n\
                 #  3: Target Deme ID [tdemeid]\n#  4: Number of founders [size]\n\
                 #  5: {{target genotype ID, target genome... founder 0, ...}}\n\
                 2 0 1 2 5 abc 6 def\n"
            ),
        );

        stats.deme_post_replication(3, vec![10, 11]);
        stats.deme_post_replication(1, vec![]);
        stats
            .print_deme_founders_data(&mut out, "founders.dat")
            .unwrap();
        assert!(stats.m_deme_founders.is_empty());
        assert_file(
            &mut out,
            &dir,
            "founders.dat",
            &format!(
                "# Avida deme founder data.\n# {STAMP}\n\
                 #  1: Update [update]\n#  2: Deme ID [demeid]\n\
                 #  3: Number of founders [size]\n#  4: {{Genotype ID of founder 0, ...}}\n\
                 2 1 0\n2 3 2 10 11\n"
            ),
        );
    }

    #[test]
    fn deme_competition_printers() {
        let (dir, mut out) = output("deme-compete");
        let mut stats = stats_at(100);
        stats.compete_demes(&[6.0, 6.0]);
        stats
            .print_deme_competition_data(&mut out, "deme_compete.dat")
            .unwrap();
        assert!(stats.m_deme_fitness.is_empty());
        assert_reference_prefix(
            &mut out,
            &dir,
            "deme_compete.dat",
            "multi_level_selection/expected/data/deme_compete.dat",
        );

        let columns = [
            "Update [update]",
            "Avg. deme fitness [avgfit]",
            "Max. deme fitness [maxfit]",
        ];
        stats
            .print_deme_competition_data(&mut out, "compete2.dat")
            .unwrap();
        stats.compete_demes(&[1.0, 3.0, 2.0]);
        stats
            .print_deme_competition_data(&mut out, "compete2.dat")
            .unwrap();
        assert_file(
            &mut out,
            &dir,
            "compete2.dat",
            &(header(&["Avida compete demes data", STAMP], &columns) + "100 0 0 \n100 2 3 \n"),
        );

        stats
            .print_demes_fitness_data(&mut out, "demes_fitness.dat")
            .unwrap();
        assert_file(
            &mut out,
            &dir,
            "demes_fitness.dat",
            &(header(
                &["Avida competition fitness for each deme", STAMP],
                &["Update"],
            ) + "100 \n"),
        );
    }

    #[test]
    fn synchronization_and_consensus_printers() {
        let (dir, mut out) = output("sync");
        let mut stats = stats_at(2);
        stats.sent_flash(Some((0, 4)));
        stats.sent_flash(Some((1, 0)));
        stats.sent_flash(None);
        stats
            .print_detailed_synchronization_data(&mut out, "detailed_sync.dat")
            .unwrap();
        assert!(stats.m_flash_times.is_empty());
        assert_file(
            &mut out,
            &dir,
            "detailed_sync.dat",
            &(header(
                &[
                    "Detailed Avida synchronization data",
                    "Rows are (update, demeid, cellid) tuples, representing the update at which that cell flashed.",
                    STAMP,
                ],
                &[
                    "Update [update]",
                    "Deme ID [demeid]",
                    "Deme-relative cell ID that issued a flash at this update [relcellid]",
                ],
            ) + "2 0 4 \n2 1 0 \n"),
        );
        stats
            .print_synchronization_data(&mut out, "sync.dat")
            .unwrap();
        assert_eq!(stats.m_flash_count, 0);
        assert_file(
            &mut out,
            &dir,
            "sync.dat",
            &(header(
                &["Avida synchronization data", STAMP],
                &["Update [update]", "Flash count [fcount]"],
            ) + "2 3 \n"),
        );

        stats.m_update = 5;
        stats.consensus_reached(0, 7, 12);
        stats.consensus_reached(1, -1, 3);
        stats
            .print_consensus_data(&mut out, "consensus.dat")
            .unwrap();
        assert!(stats.m_consensi.is_empty());
        assert_file(
            &mut out,
            &dir,
            "consensus.dat",
            &format!(
                "# Avida consensus data\n# {STAMP}\n#  1: Update [update]\n\
                 #  2: Deme ID [demeid]\n#  3: Consensus value [consensus]\n\
                 #  4: Cell ID [cellid]\n5 0 7 12 \n5 1 -1 3 \n"
            ),
        );
        stats.consensus_reached(0, 7, 12);
        stats
            .print_simple_consensus_data(&mut out, "simple_consensus.dat")
            .unwrap();
        assert_file(
            &mut out,
            &dir,
            "simple_consensus.dat",
            &(header(
                &["Avida consensus data", STAMP],
                &["Update [update]", "Consensus count [count]"],
            ) + "5 1 \n"),
        );
    }

    #[test]
    fn kill_and_migration_printers() {
        let (dir, mut out) = output("kills");
        let mut stats = stats_at(6);
        stats.add_num_orgs_killed(3);
        stats.add_num_orgs_killed(5);
        stats.add_num_unoccupied_cell_attempted_to_kill(1);
        stats.add_num_cells_scanned_at_kill(10);
        stats
            .print_num_orgs_killed_data(&mut out, "killed.dat")
            .unwrap();
        stats
            .print_num_orgs_killed_data(&mut out, "killed.dat")
            .unwrap();
        assert_file(
            &mut out,
            &dir,
            "killed.dat",
            &(header(
                &[
                    "Organisms killed using kill actions",
                    STAMP,
                    "First column is the current update and the second column lists the number of organisms killed",
                ],
                &[
                    "Update",
                    "Avg Num Orgs Killed",
                    "Avg Num Unoccupied Cell Kill Attempts",
                    "Avg Num Cells Scanned By Kill Event",
                ],
            ) + "6 4 1 10 \n6 nan nan nan \n"),
        );

        stats.inc_num_migrations();
        stats.inc_num_migrations();
        stats
            .print_migration_data(&mut out, "migration.dat")
            .unwrap();
        assert_file(
            &mut out,
            &dir,
            "migration.dat",
            &(header(
                &[
                    "Number of migrations made using the migratedemes event",
                    STAMP,
                    "First column is the current update and the second column lists the number of migrations made",
                ],
                &["Update", "Num Migrations"],
            ) + "6 2 \n"),
        );
    }

    #[test]
    fn string_match_data() {
        // string_producing_and_donating: 111 organisms that hold no strings.
        let (dir, mut out) = output("stringmatch");
        let mut stats = stats_at(100);
        let match_strings = strings(&["0000", "1111"]);
        let orgs = vec![vec![(0, 0), (0, 0)]; 111];
        stats
            .print_string_match_data(&mut out, "stringmatch.dat", &match_strings, &orgs)
            .unwrap();
        assert_reference_prefix(
            &mut out,
            &dir,
            "stringmatch.dat",
            "string_producing_and_donating/expected/data/stringmatch.dat",
        );

        stats.inc_perfect_match(4);
        stats.inc_perfect_match_org();
        let orgs = [
            vec![(2, 1), (3, 0)],
            vec![(0, 1), (1, 1)],
            vec![(0, 0), (0, 0)],
        ];
        stats
            .print_string_match_data(&mut out, "stringmatch2.dat", &match_strings, &orgs)
            .unwrap();
        assert_eq!(stats.m_perfect_match.count(), 0.0);
        assert_eq!(stats.m_perfect_match_org.count(), 0.0);
        let columns = [
            "Update [update]",
            "produced0000",
            "stored0000",
            "produced1111",
            "stored1111",
            "totalStoredAverage",
            "PerfectMatchStringElapse[ps]",
            "PerfectMatchOrgElapse[pso]",
            "PerfectMatchStringInstant[psi]",
            "PerfectMatchOrgInstant[psoi]",
            "Producednothing[nothing]",
            "Generalists[generalists]",
            "Specialists[specialists]",
        ];
        assert_file(
            &mut out,
            &dir,
            "stringmatch2.dat",
            &(header(
                &["Avida organism string donation information", STAMP],
                &columns,
            ) + "100 0.666667 0.666667 0.333333 1.33333 1 4 1 2 1 1 1 1 \n"),
        );
    }

    #[test]
    fn reputation_data() {
        // string_producing_and_donating: 111 organisms, reputations summing
        // to 168.
        let (dir, mut out) = output("reputation");
        let stats = stats_at(100);
        let orgs: Vec<ReputationOrgData> = (0..111)
            .map(|i| ReputationOrgData {
                reputation: if i < 57 { 2 } else { 1 },
                ..ReputationOrgData::default()
            })
            .collect();
        stats
            .print_reputation_data(&mut out, "reputation.dat", &orgs)
            .unwrap();
        assert_reference_prefix(
            &mut out,
            &dir,
            "reputation.dat",
            "string_producing_and_donating/expected/data/reputation.dat",
        );

        let orgs = [
            ReputationOrgData {
                reputation: 1,
                num_donations: 2,
                num_donations_received: 1,
                num_donors: 1,
                self_raw_materials: 3,
                other_raw_materials: 1,
            },
            ReputationOrgData {
                reputation: 3,
                num_donations: 0,
                num_donations_received: 2,
                num_donors: 2,
                self_raw_materials: 1,
                other_raw_materials: 0,
            },
        ];
        stats
            .print_reputation_data(&mut out, "reputation2.dat", &orgs)
            .unwrap();
        let columns = [
            "Update [update]",
            "Avg. reputation [reputation]",
            "Avg. donations [donation]",
            "Avg. number of donor partners [partners]",
            "Number of altruists [altruists]",
            "Number of cooperators [cooperators]",
            "Avg. own raw mat [ownrawmat]",
            "Avg. other raw mat [otherrawmat]",
        ];
        assert_file(
            &mut out,
            &dir,
            "reputation2.dat",
            &(header(
                &[
                    "Avida organism reputation information -- average donations, min donations, max donations",
                    STAMP,
                ],
                &columns,
            ) + "100 2 1 1.5 1 1 2 0.5 \n"),
        );
    }

    #[test]
    fn shaded_altruists() {
        let (dir, mut out) = output("shaded");
        let stats = stats_at(8);
        stats
            .print_shaded_altruists(&mut out, "shaded.dat", 10, &[100, 95, 50, 0, 15])
            .unwrap();
        let mut columns = vec![
            "Update [update]".to_string(),
            "Population [population]".to_string(),
        ];
        for pct in (0..=100).rev().step_by(10) {
            columns.push(format!("shaded-{pct} [shaded{pct}]"));
        }
        columns.push("percent-high-alt  [highalt]".to_string());
        columns.push("avg-shade [avgshade]".to_string());
        assert_file(
            &mut out,
            &dir,
            "shaded.dat",
            &(header(
                &[
                    "The number of organisms in different bins of shaded altruism",
                    "Bins of orgs of shaded strategies.",
                    STAMP,
                ],
                &columns,
            ) + "8 10 1 2 2 2 2 2 3 3 3 4 4 0.2 26 \n"),
        );
    }

    #[test]
    fn kaboom_and_quorum_printers_reset_their_counters() {
        let (dir, mut out) = output("kaboom");
        let mut stats = stats_at(3);
        stats.inc_kaboom();
        stats.inc_kaboom();
        stats.inc_kaboom_pre_divide();
        stats.inc_kaboom_kills();
        stats.inc_sa_kin(3);
        stats.inc_sa_not_kin(1);
        stats.inc_nsa_kin(0);
        stats.inc_nsa_not_kin(2);
        stats.add_ham_distance(4);
        stats.inc_perc_lyse(0.5);
        stats.inc_sum_cpus(100);
        stats.print_kaboom(&mut out, "kaboom.dat").unwrap();
        assert_eq!(
            (
                stats.num_kabooms,
                stats.num_kabooms_pre,
                stats.num_sa_kin,
                stats.sum_cpu_cycles
            ),
            (0, 0, 0, 0.0)
        );
        assert!(stats.hd_list.is_empty());
        assert_file(
            &mut out,
            &dir,
            "kaboom.dat",
            &(header(
                &["The number of kabooms.", STAMP],
                &[
                    "Update [update]",
                    "number of kabooms",
                    "number of orgs killed by kabooms",
                    "Number of organisms with suicidal altruism inst considered kin",
                    "Number of organisms with suicidal altruism inst considered non-kin",
                    "Number of organisms without suicidal altruism inst considered kin",
                    "Number of organisms without suicidal altruism inst considered non-kin",
                ],
            ) + "3 2 1 3 1 0 2 \n"),
        );

        stats.inc_dont_explode();
        stats.inc_quorum_threshold_ub(10);
        stats.inc_quorum_threshold_ub(5);
        stats.inc_quorum_num();
        stats.inc_quorum_num();
        stats.print_quorum(&mut out, "quorum.dat").unwrap();
        stats.print_quorum(&mut out, "quorum.dat").unwrap();
        assert_file(
            &mut out,
            &dir,
            "quorum.dat",
            &(header(
                &["Quorum sensing with threshold.", STAMP],
                &[
                    "Update [update]",
                    "Number of explosions stopped by quorum sensing",
                    "Average quorum sense threshold upper bound per qs instruction",
                ],
            ) + "3 1 7.5 \n3 0 nan \n"),
        );
    }

    #[test]
    fn group_printers_match_reference() {
        let (dir, mut out) = output("groups");
        let mut stats = stats_at(0);
        for (update, size) in [(10, 43), (20, 279), (30, 399)] {
            stats.m_update = update;
            stats
                .print_groups_formed_data(
                    &mut out,
                    "groupformation.dat",
                    &BTreeMap::from([(0, size)]),
                    size,
                    400,
                )
                .unwrap();
        }
        for (update, size) in [(0, 1), (1, 2), (2, 2)] {
            stats.m_update = update;
            stats
                .print_group_ids(&mut out, "groupids.dat", &BTreeMap::from([(0, size)]))
                .unwrap();
        }
        for file in ["groupformation.dat", "groupids.dat"] {
            assert_reference_prefix(
                &mut out,
                &dir,
                file,
                &format!("tolerance_variations/expected/data/{file}"),
            );
        }
    }

    #[test]
    fn tolerance_printers_match_reference() {
        let (dir, mut out) = output("tolerance");
        let mut stats = stats_at(0);
        let push = |stats: &mut Stats, inst: usize, size: i32| {
            stats.push_tolerance_inst_exe_circumstances(
                inst, 0, size, 100.0, 100.0, 100.0, 100.0, 10, 10, 10, 10,
            );
        };
        // tolerance_variations, updates 0-2.
        for inst in [6, 7, 0, 3] {
            push(&mut stats, inst, 1);
        }
        stats
            .print_tolerance_instruction_data(&mut out, "toleranceinstruction.dat")
            .unwrap();
        stats
            .print_tolerance_data(&mut out, "tolerance.dat", true)
            .unwrap();
        stats.m_update = 1;
        stats
            .print_tolerance_instruction_data(&mut out, "toleranceinstruction.dat")
            .unwrap();
        stats
            .print_tolerance_data(&mut out, "tolerance.dat", true)
            .unwrap();
        stats.m_update = 2;
        for inst in [6, 7, 0, 6, 7, 0, 3, 3] {
            push(&mut stats, inst, 2);
        }
        // The update-0 records were dropped by the first push of update 2.
        assert_eq!(stats.m_is_tolerance_exe_insts.len(), 8);
        stats
            .print_tolerance_instruction_data(&mut out, "toleranceinstruction.dat")
            .unwrap();
        for file in ["toleranceinstruction.dat", "tolerance.dat"] {
            assert_reference_prefix(
                &mut out,
                &dir,
                file,
                &format!("tolerance_variations/expected/data/{file}"),
            );
        }

        assert_eq!(
            stats.print_tolerance_data(&mut out, "tolerance2.dat", false),
            Err("TRACK_TOLERANCE option must be turned on in avida.cfg for PrintToleranceData to function.".to_string())
        );

        stats.zero_tolerance_inst();
        assert!(stats
            .m_is_tolerance_exe_counts
            .iter()
            .all(|&c| c == (-1, -1)));
        assert!(stats.m_is_tolerance_exe_insts.is_empty());
        // Without tracking: counts only.
        let mut plain = stats_at(4);
        plain.zero_tolerance_inst(); // unsized: nothing to reset
        plain.push_tolerance_inst_exe(1);
        plain.push_tolerance_inst_exe(1);
        assert_eq!(plain.m_is_tolerance_exe_counts[1], (4, 2));
        assert!(plain.m_is_tolerance_exe_insts.is_empty());
    }

    fn repeat(pairs: &[(i32, usize)]) -> Vec<i32> {
        pairs
            .iter()
            .flat_map(|&(t, n)| std::iter::repeat_n(t, n))
            .collect()
    }

    fn target_columns(pairs: usize) -> Vec<&'static str> {
        let mut columns = vec!["Update"];
        for _ in 0..pairs {
            columns.extend(["Target ID", "Num Orgs Targeting ID"]);
        }
        columns
    }

    #[test]
    fn targets_match_reference() {
        let (dir, mut out) = output("targets");
        let stats = stats_at(0);
        // pp-evolved: PRED_PREY_SWITCH 0, one resource target.
        stats
            .print_targets(
                &mut out,
                "targets.dat",
                0,
                &BTreeSet::from([0]),
                &repeat(&[(-2, 191), (-1, 704), (0, 20)]),
            )
            .unwrap();
        assert_reference_prefix(
            &mut out,
            &dir,
            "targets.dat",
            "pp-evolved/expected/data/targets.dat",
        );

        // avatars: PRED_PREY_SWITCH 2, targets 1-9.
        let (dir, mut out) = output("targets-avatars");
        stats
            .print_targets(
                &mut out,
                "targets.dat",
                2,
                &(1..=9).collect(),
                &repeat(&[(1, 9)]),
            )
            .unwrap();
        assert_reference_prefix(
            &mut out,
            &dir,
            "targets.dat",
            "avatars/expected/data/targets.dat",
        );
    }

    #[test]
    fn target_printers_by_hand() {
        let (dir, mut out) = output("targets-hand");
        let stats = stats_at(4);
        let comments = [
            "Targets in use on update boundary.",
            "-2: is predator, -1: no targets(default), >=0: id of environmental resource targeted).",
            "Format is update + target0 + count0 + target1 + count1 ...",
            STAMP,
        ];
        // QUIRK: target 2 is not in the list, so it counts at index 2
        // (target 1's slot).
        stats
            .print_targets(
                &mut out,
                "targets.dat",
                -1,
                &BTreeSet::from([0, 1]),
                &[-1, 0, 2],
            )
            .unwrap();
        assert_file(
            &mut out,
            &dir,
            "targets.dat",
            &(header(&comments, &target_columns(3)) + "4 -1 1 0 1 1 1 \n"),
        );
        // Indices the C++ would write out of bounds.
        for bad in [5, -2] {
            assert!(stats
                .print_targets(
                    &mut out,
                    "targets2.dat",
                    -1,
                    &BTreeSet::from([0, 1]),
                    &[bad]
                )
                .is_err());
        }

        stats
            .print_top_pred_targets(
                &mut out,
                "tpred_targets.dat",
                0,
                &BTreeSet::from([0, 1]),
                &[-5, -2, -1, 1, 1],
            )
            .unwrap();
        let comments = [
            "Targets in use on update boundary.",
            "-3: is top predator, -2: is predator, -1: no targets(default), >=0: id of environmental resource targeted).",
            "Format is update + target0 + count0 + target1 + count1 ...",
            STAMP,
        ];
        assert_file(
            &mut out,
            &dir,
            "tpred_targets.dat",
            &(header(&comments, &target_columns(5)) + "4 -3 1 -2 1 -1 1 0 0 1 2 \n"),
        );

        stats
            .print_mimic_displays(
                &mut out,
                "mimics.dat",
                &BTreeSet::from([-2, -1, 0, 3]),
                &[(1, 3), (1, 3), (0, 0), (1, 9)],
            )
            .unwrap();
        assert_file(
            &mut out,
            &dir,
            "mimics.dat",
            &(header(
                &[
                    "FTs being displayed by mimics on update boundary (true mimic ft == 1).",
                    STAMP,
                ],
                &[
                    "Update",
                    "Displayed FT",
                    "Num Orgs Displaying this FT",
                    "Displayed FT",
                    "Num Orgs Displaying this FT",
                ],
            ) + "4 0 0 3 2 \n"),
        );
    }

    #[test]
    fn named_statistic_printers() {
        let (dir, mut out) = output("named");
        let mut stats = stats_at(9);
        stats.network_topology(&BTreeMap::from([
            ("b".to_string(), 2.0),
            ("a".to_string(), 1.0),
        ]));
        stats.network_topology(&BTreeMap::from([
            ("b".to_string(), 4.0),
            ("a".to_string(), 3.0),
        ]));
        stats
            .print_deme_network_data(&mut out, "network.dat")
            .unwrap();
        assert!(stats.m_network_stats.is_empty());
        assert_file(
            &mut out,
            &dir,
            "network.dat",
            &(header(
                &["Deme network statistics", STAMP],
                &["Update [update]", "a", "b"],
            ) + "9 2 3 \n"),
        );

        stats.profiling_data(&BTreeMap::from([("step".to_string(), 2.0)]));
        stats.profiling_data(&BTreeMap::from([
            ("step".to_string(), 4.0),
            ("io".to_string(), 1.0),
        ]));
        stats
            .print_profiling_data(&mut out, "profiling.dat")
            .unwrap();
        assert!(stats.m_profiling.is_empty());
        assert_file(
            &mut out,
            &dir,
            "profiling.dat",
            &(header(
                &["Profiling statistics", STAMP],
                &["Update [update]", "io", "step"],
            ) + "9 1 3 \n"),
        );

        stats.genome_fragment_metabolized(10);
        stats.genome_fragment_metabolized(20);
        stats.genome_fragment_inserted(5);
        stats.print_hgt_data(&mut out, "hgt.dat").unwrap();
        assert_eq!(stats.m_hgt_metabolized.count(), 0.0);
        assert_file(
            &mut out,
            &dir,
            "hgt.dat",
            &(header(
                &["Horizontal gene transfer statistics", STAMP],
                &[
                    "Update [update]",
                    "Total count of metabolized genome fragments [metcount]",
                    "Total size of metabolized genome fragments [metsize]",
                    "Total count of insertion events [inscount]",
                    "Total size of insertion events [inssize]",
                ],
            ) + "9 2 30 1 5 \n"),
        );

        stats.outgoing_migrant();
        stats.outgoing_migrant();
        stats.incoming_migrant();
        stats.print_multi_process_data(&mut out, "mp.dat").unwrap();
        assert_eq!(stats.m_outgoing.count(), 0.0);
        assert_file(
            &mut out,
            &dir,
            "mp.dat",
            &(header(
                &["Avida-MP data", STAMP],
                &[
                    "Update [update]",
                    "Outgoing migrants [outgoing]",
                    "Incoming migrants [incoming]",
                ],
            ) + "9 2 1 \n"),
        );
    }

    #[test]
    fn message_log_printers() {
        let (dir, mut out) = output("message-log");
        let mut stats = stats_at(2);
        stats.log_message(0, 1, 2, 3, 42, 7, true, false);
        stats.log_ret_message(1, 2, 3, u32::MAX, 0);
        stats
            .print_message_log(&mut out, "message_log.dat")
            .unwrap();
        stats
            .print_ret_message_log(&mut out, "ret_message_log.dat")
            .unwrap();
        assert!(stats.m_message_log.is_empty() && stats.m_retmessage_log.is_empty());
        let mut columns = vec![
            "Update [update]",
            "Deme ID [deme]",
            "Source [src]",
            "Destination [dst]",
            "Transmission_cell [trs]",
            "Message data [data]",
            "Message label [label]",
        ];
        let comments = ["Log of all messages sent in population.", STAMP];
        assert_file(
            &mut out,
            &dir,
            "ret_message_log.dat",
            &(header(&comments, &columns) + "2 0 1 2 3 4294967295 0 \n"),
        );
        columns.extend(["Dropped [dropped]", "Lost [lost]"]);
        assert_file(
            &mut out,
            &dir,
            "message_log.dat",
            &(header(&comments, &columns) + "2 0 1 2 3 42 7 1 0 \n"),
        );
    }

    #[test]
    fn task_timing_printers() {
        let (dir, mut out) = output("task-timing");
        let mut stats = stats_with(&["Not", "Nand"], &[]);
        stats.m_update = 3;
        let switch = &mut stats.intrinsic_task_switch_time;
        switch.entry((0, 1)).or_default().add(2.0);
        switch.entry((0, 1)).or_default().add(4.0);
        switch.entry((1, 0)).or_default().add(5.0);
        stats
            .print_intrinsic_task_switching_cost_data(&mut out, "switching.dat")
            .unwrap();
        assert!(stats.intrinsic_task_switch_time.is_empty());
        // QUIRK: the column descriptions and the first row's descriptions
        // both reach the header.
        let columns = [
            "Update [update]",
            "Task 1 [t1]",
            "Task 2 [t2]",
            "Mean cycles [mc]",
        ];
        assert_file(
            &mut out,
            &dir,
            "switching.dat",
            &(header(
                &["Number of cyles it takes to change between tasks", STAMP],
                &[columns, columns].concat(),
            ) + "3 0 1 3 \n3 1 0 5 \n"),
        );

        stats.reaction_age_map.entry(0).or_default().add(10.0);
        stats.reaction_age_map.entry(0).or_default().add(20.0);
        stats
            .print_age_polyethism_data(&mut out, "age_polyethism.dat")
            .unwrap();
        assert_eq!(stats.reaction_age_map[&0].count(), 0.0);
        // FlushComments: no blank line before the rows.
        assert_file(
            &mut out,
            &dir,
            "age_polyethism.dat",
            &format!(
                "# Tasks, mean organism age, and variance of ages\n# {STAMP}\n\
                 #  1: Update [update]\n\
                 #  2: Task 0 Organism Age Mean [meanorgage0]\n\
                 #  3: Task 0 Organism Age Standard Error [errorgage0]\n\
                 #  4: Task 1 Organism Age Mean [meanorgage1]\n\
                 #  5: Task 1 Organism Age Standard Error [errorgage1]\n\
                 3 15 5 0 0 \n"
            ),
        );
    }

    #[test]
    fn deme_reaction_diversity_replication_data() {
        // multi_level_selection: nothing recorded in updates 0-5.
        let (dir, mut out) = output("diversity");
        let mut stats = stats_at(0);
        for update in 0..6 {
            stats.m_update = update;
            stats
                .print_deme_reaction_diversity_replication_data(&mut out, "deme_rx_repl.dat")
                .unwrap();
        }
        assert_reference_prefix(
            &mut out,
            &dir,
            "deme_rx_repl.dat",
            "multi_level_selection/expected/data/deme_rx_repl.dat",
        );

        // QUIRK: a first call with data names two columns differently. Only
        // the last 100 entries count.
        stats.inc_deme_reaction_diversity_replication_data(1000.0, 0.5, 2.0, 4.0, 0.25);
        for _ in 0..100 {
            stats.inc_deme_reaction_diversity_replication_data(2.0, 0.5, 2.0, 4.0, 0.25);
        }
        stats
            .print_deme_reaction_diversity_replication_data(&mut out, "deme_rx_repl2.dat")
            .unwrap();
        assert_eq!(stats.m_deme_diversity.len(), 100);
        let columns = [
            "Update [update]",
            "Mean number of different reactions by deme [demereact]",
            "Mean number of deme switch penalties per org  [orgpen]",
            "Mean shannon mutual entropy [shannon]",
            "Mean shannon normalalized mutual information [shannonnorm]",
            "Mean number of orgs that perform a reaction [meanreact]",
            "Mean percent of orgs that replicate [meanperrepros]",
        ];
        assert_file(
            &mut out,
            &dir,
            "deme_rx_repl2.dat",
            &(header(
                &["Avida deme reaction diversity replication data", STAMP],
                &columns,
            ) + "5 2 0.5 2 1.4427 4 0.25 \n"),
        );
    }
}
