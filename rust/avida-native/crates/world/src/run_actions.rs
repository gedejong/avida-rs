//! The `Process` half of event actions ported after the first set (see
//! [`crate::run`]): random and duplicated injections, the reward toggles,
//! grid dumps and instruction statistics (PopulationActions.cc and
//! PrintActions.cc at 47f13dadb), plus the state those actions keep between
//! calls.

use std::collections::BTreeMap;
use std::io::Write;

use avida_apto::fmt::fmt_double;
use avida_config::string_util::pop;
use avida_genome::genome::{setup_property_map, PropertyMap};
use avida_genome::{Genome, InstructionSequence};
use avida_hardware::organism::{transmission, OrgSource};

use crate::actions::Action;
use crate::world::World;

/// State the C++ keeps in action members, in data recorders or on
/// `std::cout`, held by the world so actions stay plain data.
#[derive(Debug, Default)]
pub struct ActionState {
    /// What actions write to `std::cout`. The run loop copies it to its
    /// stdout after each batch of events, so it stays in order with the
    /// status lines.
    pub stdout: String,
    /// A `cActionPrintInstructionData` recorder is attached (some event in
    /// the event file is `PrintInstructionData`), so
    /// `InstructionExecCountsProvider` is active.
    pub record_inst_exec: bool,
    /// `core.population.inst_exec_counts[<inst set>]` as the recorders last
    /// received it (`NotifyData` in `PerformUpdate`); `None` before the
    /// first update.
    pub inst_exec_counts: Option<BTreeMap<String, Vec<i32>>>,
    /// `Driver().Abort(condition)` was called: the process exits with this
    /// status (`exit(condition)`) after printing what is pending.
    pub exit_code: Option<i32>,
}

/// `Systematics::Source(Systematics::DIVISION, args, true)`
fn division_source(args: &str) -> OrgSource {
    OrgSource::new(transmission::DIVISION, args, true)
}

/// `cString::Substring(0, n)` on an ASCII string.
fn prefix(s: &str, n: usize) -> &str {
    &s[..n.min(s.len())]
}

impl World {
    /// Copies what actions printed to `std::cout` to the run's stdout.
    pub(crate) fn flush_action_stdout(&mut self, stdout: &mut dyn Write) -> Result<(), String> {
        if self.action_state.stdout.is_empty() {
            return Ok(());
        }
        let text = std::mem::take(&mut self.action_state.stdout);
        stdout.write_all(text.as_bytes()).map_err(|e| e.to_string())
    }

    /// Whether the event list holds a `PrintInstructionData` action (its
    /// constructor attaches the recorder that activates the provider).
    pub(crate) fn note_recorders(&mut self) {
        self.action_state.record_inst_exec = self
            .events
            .entries()
            .iter()
            .any(|e| matches!(e.action, Action::PrintInstructionData { .. }));
        self.actions2_note_recorders();
    }

    /// `InstructionExecCountsProvider::UpdateReset` and `HandleOrganism` for
    /// every live organism (run by `UpdateOrganismStats` at the end of an
    /// update), followed by `NotifyData` in `PerformUpdate`: sums of each
    /// organism's `GetLastInstCount()`, per instruction set.
    pub(crate) fn record_inst_exec_counts(&mut self) {
        if !self.action_state.record_inst_exec {
            return;
        }
        let mut counts: BTreeMap<String, Vec<i32>> = BTreeMap::new();
        for is in self.hw.registry().inst_sets() {
            counts.insert(
                is.inst_set_name().to_string(),
                vec![0; is.size().max(0) as usize],
            );
        }
        for &cell in &self.pop.live_org_list {
            let Some(org) = self.pop.orgs[cell as usize].as_ref() else {
                continue;
            };
            let inst_set = org.state.genome().inst_set().to_string();
            // `Apto::ImplicitDefault`: an unknown name maps to an empty array.
            let sums = counts.entry(inst_set).or_default();
            for (j, &v) in org.state.phenotype.last_inst_count.iter().enumerate() {
                if let Some(s) = sums.get_mut(j) {
                    *s = s.wrapping_add(v);
                }
            }
        }
        self.action_state.inst_exec_counts = Some(counts);
    }

    /// The `STATS_OUT_FILE` printers whose `cStats` method reads population
    /// or environment state, with that state gathered here. `None` for any
    /// other method.
    pub(crate) fn print_stats_with_world(
        &mut self,
        method: &str,
        filename: &str,
    ) -> Option<Result<(), String>> {
        Some(match method {
            "PrintTargets" | "PrintTopPredTargets" => self.print_targets_action(method, filename),
            "PrintDemeReplicationData" => self.print_deme_replication_data(filename),
            "PrintWinningDeme" => self.print_winning_deme(filename),
            "PrintCellData" => self.print_cell_data(filename),
            "PrintMessageData" => self.print_message_data(filename),
            "PrintDemeGermlineSequestration" => self.print_deme_germline_sequestration(filename),
            "PrintThreadsData" => {
                let live = self.pop.live_org_list.len() as i32;
                self.stats.print_threads_data(&mut self.out, filename, live)
            }
            // `GetDeme(i).GetSleepingCount()` of every deme.
            "PrintSleepData" => {
                let counts: Vec<i32> = self.pop.demes.iter().map(|d| d.sleeping_count).collect();
                self.stats
                    .print_sleep_data(&mut self.out, filename, &counts)
            }
            "PrintGroupsFormedData" => {
                // `GetPopulation().GetFormedGroups()` and the occupied cells
                // whose organism `HasOpinion()`.
                let mut orgs_with_opinion = 0;
                for org in self.pop.orgs.iter_mut().flatten() {
                    if org.state.has_opinion() {
                        orgs_with_opinion += 1;
                    }
                }
                let pop_size = self.pop.size();
                self.stats.print_groups_formed_data(
                    &mut self.out,
                    filename,
                    &self.pop.groups.groups,
                    orgs_with_opinion,
                    pop_size,
                )
            }
            "PrintGroupIds" => {
                self.stats
                    .print_group_ids(&mut self.out, filename, &self.pop.groups.groups)
            }
            "PrintToleranceData" => {
                let track = self.cfg.track_tolerance != 0;
                self.stats
                    .print_tolerance_data(&mut self.out, filename, track)
            }
            "PrintDynamicMaxMinData" => {
                let mut task_max_min = Vec::new();
                for t in self.env.tasklib().0.tasks() {
                    match t.arguments() {
                        Some(args) => task_max_min.push((args.get_double(1), args.get_double(2))),
                        None => {
                            return Some(Err(format!(
                                "PrintDynamicMaxMinData: task '{}' has no arguments (C++: NULL dereference)",
                                t.desc()
                            )))
                        }
                    }
                }
                self.stats
                    .print_dynamic_max_min_data(&mut self.out, filename, &task_max_min)
            }
            "PrintShadedAltruists" => {
                let Some(is) = self.hw.registry().shared_inst_set(0) else {
                    return Some(Err("PrintShadedAltruists: no instruction set".to_string()));
                };
                let mut shades = Vec::new();
                for org in self.pop.orgs.iter().flatten() {
                    let counts = &org.state.phenotype.test_cpu_inst_count;
                    let mut shade_of_gb = 0;
                    for i in 0..is.size() {
                        if is.name(i) == "donate-shadedgb" && !counts.is_empty() {
                            shade_of_gb = counts.get(i as usize).copied().unwrap_or(0);
                        }
                    }
                    shades.push(shade_of_gb);
                }
                let pop_size = self.pop.cells.len() as i32;
                self.stats
                    .print_shaded_altruists(&mut self.out, filename, pop_size, &shades)
            }
            _ => return None,
        })
    }

    /// `cActionInjectAllRandomRepro::Process`
    pub(crate) fn inject_all_random_repro(
        &mut self,
        length: i32,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
        sex: i32,
    ) -> Result<(), String> {
        for i in 0..self.pop.cells.len() as i32 {
            let is = self
                .hw
                .registry()
                .shared_inst_set(0)
                .ok_or("InjectAllRandomRepro: no instruction set")?;
            let mut props = PropertyMap::new();
            setup_property_map(&mut props, is.inst_set_name());
            let mut seq = InstructionSequence::with_size(length.wrapping_add(1));
            for j in 0..length {
                let mut inst = is.random_inst(&mut self.rng);
                // QUIRK (guarded): an opcode outside the set would read past
                // the redundancy table in C++; it is redrawn here.
                while inst.op() >= is.size() || is.redundancy(inst) == 0 {
                    inst = is.random_inst(&mut self.rng);
                }
                seq.set(j, inst);
            }
            let last = if sex != 0 { "repro-sex" } else { "repro" };
            let last = match is.get_inst(last) {
                Ok(inst) => inst,
                Err(line) => return Err(self.exit_with_stderr_line(line)),
            };
            seq.set(length, last);
            let genome = Genome::new(is.hardware_type(), &props, seq);
            self.inject(
                &genome,
                division_source("random"),
                i,
                merit,
                lineage_label,
                neutral_metric,
            )?;
        }
        Ok(())
    }

    /// `cActionInjectWholeGenomeDuplications::Process` and
    /// `cActionInjectWholeGenomeDuplicationsNopCPrepend::Process`: replace
    /// every organism by one whose sequence is its own twice (or, with
    /// `nopc_prepend`, a run of `c` as long as its sequence, then the
    /// sequence).
    pub(crate) fn inject_whole_genome_duplications(
        &mut self,
        nopc_prepend: bool,
    ) -> Result<(), String> {
        for i in 0..self.pop.cells.len() {
            let Some(org) = self.pop.orgs[i].as_ref() else {
                continue;
            };
            let mut genome_str = org.state.genome().as_string();
            let mut seq_str = genome_str.clone();
            pop(&mut seq_str, ',');
            pop(&mut seq_str, ',');
            let src = if nopc_prepend {
                let keep = genome_str.len() - seq_str.len().min(genome_str.len());
                genome_str = prefix(&genome_str, keep).to_string();
                genome_str.push_str(&"c".repeat(seq_str.len()));
                genome_str.push_str(&seq_str);
                "whole-genome duplication nopc prepend"
            } else {
                let out = &mut self.action_state.stdout;
                out.push_str(&format!("genome_str: {genome_str}\n"));
                out.push_str(&format!("seq_str: {seq_str}\n"));
                genome_str.push_str(&seq_str);
                out.push_str(&format!("genome_str cat: {genome_str}\n"));
                "whole-genome duplication"
            };
            let genome = Genome::from_string(&genome_str);
            self.inject(&genome, division_source(src), i as i32, -1.0, 0, 0.0)?;
        }
        Ok(())
    }

    /// `cActionPrintCurrReactionCountGrid::Process`. The action's
    /// `first_time` flag only guards header comments, which the file
    /// ignores once its header is out, so the header state stands in for it.
    pub(crate) fn print_curr_reaction_count_grid(&mut self, filename: &str) -> Result<(), String> {
        const UNOCCUPIED: i32 = -1;
        let update = self.stats.update();
        let names: Vec<String> = (0..self.env.num_reactions().max(0) as usize)
            .map(|r| self.env.reaction_name(r).to_string())
            .collect();
        let df = self.out.static_with_path(filename)?;
        df.write_comment(
            "First column is update, second column is reaction name, subsequent columns are individual cells",
        );
        df.write_comment("-1 for a reaction count indicates the cell is not occupied.");
        df.flush_comments();
        for (react, name) in names.iter().enumerate() {
            let mut line = format!("{update} {name}");
            for slot in &self.pop.orgs {
                line.push(' ');
                match slot {
                    None => line.push_str(&UNOCCUPIED.to_string()),
                    Some(org) => line.push_str(
                        &org.state
                            .phenotype
                            .cur_reaction_count
                            .get(react)
                            .copied()
                            .unwrap_or(0)
                            .to_string(),
                    ),
                }
            }
            df.ofstream()
                .write_all(line.as_bytes())
                .map_err(|e| e.to_string())?;
            df.endl();
            df.flush();
        }
        Ok(())
    }

    /// `cActionPrintInstructionData::Process`: the update, then the
    /// execution counts the recorder last received (none before the first
    /// update), named by the instruction set.
    pub(crate) fn print_instruction_data(
        &mut self,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        let is =
            self.hw.registry().get_inst_set(inst_set).ok_or_else(|| {
                format!("PrintInstructionData: unknown instruction set '{inst_set}'")
            })?;
        let names: Vec<String> = (0..is.size()).map(|i| is.name(i).to_string()).collect();
        let update = self.stats.update();
        let data = self
            .action_state
            .inst_exec_counts
            .as_ref()
            .map(|m| m.get(inst_set).cloned().unwrap_or_default());
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida instruction execution data");
        df.write_time_stamp();
        df.write(update, "Update");
        if let Some(counts) = data {
            for (i, v) in counts.iter().enumerate() {
                let name = names.get(i).map_or("", String::as_str);
                df.write(*v, name);
            }
        }
        df.endl();
        Ok(())
    }

    /// `cActionPrintInstructionAbundanceHistogram::Process`: how often each
    /// instruction occurs in the memories of the organisms of one
    /// instruction set.
    pub(crate) fn print_instruction_abundance_histogram(
        &mut self,
        filename: &str,
        inst_set: &str,
    ) -> Result<(), String> {
        let is = self.hw.registry().get_inst_set(inst_set).ok_or_else(|| {
            format!("PrintInstructionAbundanceHistogram: unknown instruction set '{inst_set}'")
        })?;
        let names: Vec<String> = (0..is.size()).map(|i| is.name(i).to_string()).collect();
        let mut inst_counts = vec![0i32; names.len()];
        for org in self.pop.orgs.iter().flatten() {
            if org.state.genome().inst_set() != is.inst_set_name() {
                continue;
            }
            let mem = org.hardware.memory();
            for y in 0..mem.size() {
                if let Some(c) = inst_counts.get_mut(mem[y].op() as usize) {
                    *c += 1;
                }
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write(update, "Update");
        for (count, name) in inst_counts.iter().zip(&names) {
            df.write(*count, name);
        }
        df.endl();
        Ok(())
    }

    /// `cActionDumpMaxResGrid::Process`: per cell, the largest resource
    /// amount (walls and hills add their plateau height instead).
    pub(crate) fn dump_max_res_grid(&mut self, filename: &str) -> Result<(), String> {
        let filename = if filename.is_empty() {
            format!("grid_dumps/max_res_grid.{}.dat", self.stats.update())
        } else {
            filename.to_string()
        };
        let mut df = self.out.create_with_path(&filename)?;
        let world_x = self.pop.world_x;
        let world_y = self.pop.world_y;
        for j in 0..world_y {
            let mut line = String::new();
            for i in 0..world_x {
                let res_count = self.with_pop_resources(|rc, rng, env| {
                    rc.get_cell_resources(j * world_x + i, rng, env).to_vec()
                });
                let resources = self.env.resource_lib().resources();
                let mut max_resource = 0.0;
                let mut topo_height = 0.0;
                for (h, &amount) in res_count.iter().enumerate() {
                    let Some(res) = resources.get(h) else {
                        continue;
                    };
                    let hab_type = res.habitat;
                    if amount > max_resource && hab_type != 1 && hab_type != 2 {
                        max_resource = amount;
                    } else if (hab_type == 1 || hab_type == 4 || hab_type == 5) && amount > 0.0 {
                        topo_height = res.plateau;
                    } else if hab_type == 2 && amount > 0.0 {
                        // allow walls to trump everything else
                        topo_height = res.plateau;
                        max_resource = 0.0;
                        break;
                    }
                }
                max_resource += topo_height;
                line.push_str(&fmt_double(max_resource));
                line.push(' ');
            }
            line.push('\n');
            df.ofstream()
                .write_all(line.as_bytes())
                .map_err(|e| e.to_string())?;
            df.flush();
        }
        Ok(())
    }
}
