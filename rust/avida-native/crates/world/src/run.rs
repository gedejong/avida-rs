//! The run loop and the `Process` half of the event actions.
//!
//! Ports `Avida2Driver::Run` (avida-core/source/targets/avida/Avida2Driver.cc),
//! `cWorld::GetEvents`, `cPopulation::SavePopulation` and the `Process`
//! methods of the actions in [`crate::actions`] (PrintActions.cc,
//! PopulationActions.cc, SaveLoadActions.cc, DriverActions.cc at 47f13dadb).
//!
//! An action whose `Process` needs code that is not ported records a fatal
//! error; the loop stops on it instead of skipping the action.

use std::io::Write;

use avida_genome::feedback::{Feedback as GenomeFeedback, FeedbackKind};
use avida_genome::genome::{setup_property_map, PropertyMap};
use avida_genome::genome_loader::load_genome_detail_file;
use avida_genome::{Genome, InstructionSequence};
use avida_hardware::organism::{transmission, OrgSource};
use avida_hardware::test_cpu::TestCpu;
use avida_systematics::spop::{OrganismSaveData, PopulationSave, SaveOptions};

use crate::actions::Action;
use crate::driver::StatusLine;
use crate::event_list::{trigger_value, TriggerType};
use crate::load_population::LoadPopulationArgs;
use crate::pop_interface::PopulationInterface;
use crate::stats_dispatch;
use crate::world::{make_test_world, World};

/// `Systematics::Source(Systematics::DIVISION, "", true)`
pub(crate) fn injected_source() -> OrgSource {
    OrgSource::new(transmission::DIVISION, "", true)
}

/// A `Write` over an `Avida::Output::File` (the `cHardwareStatusPrinter`'s
/// `m_file`); dropping it closes the file and releases its output id.
struct DataFileWriter(avida_apto::datafile::DataFile);

impl Write for DataFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.ofstream().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.ofstream().flush()
    }
}

impl World {
    /// The `m_trace_filename` tail of `cActionInject::Process` and
    /// `cActionInjectGroup::Process`: a `cHardwareStatusPrinter` on
    /// `trace_filename` (`Output::File::CreateWithPath`, so in the data
    /// directory) becomes the tracer of the organism in `cell_id`, replacing
    /// any mini tracer the injection set up.
    pub(crate) fn set_status_trace(
        &mut self,
        cell_id: i32,
        trace_filename: &str,
    ) -> Result<(), String> {
        let file = self.out.create_with_path(trace_filename).map_err(|e| {
            format!(
                "unsupported: an injection trace file that cannot be opened: {e} (C++: \
                 cHardwareStatusPrinter keeps a NULL file and crashes on its first line)"
            )
        })?;
        let Some(org) = usize::try_from(cell_id)
            .ok()
            .and_then(|c| self.pop.orgs.get_mut(c))
            .and_then(Option::as_mut)
        else {
            return Err(format!(
                "unsupported: a trace file for an injection that left cell {cell_id} empty (C++: \
                 SetTrace on a NULL organism)"
            ));
        };
        let printer = avida_hardware::tracer::HardwareStatusPrinter::new(
            Box::new(DataFileWriter(file)),
            false,
        );
        org.hardware.set_trace(Some(printer.into_ptr()));
        Ok(())
    }

    /// `cEventList::GetTriggerValue(trigger)`
    fn trigger(&self, trigger: TriggerType) -> f64 {
        trigger_value(
            trigger,
            self.stats.update(),
            self.stats.sum_generation.average(),
            self.stats.tot_organisms,
        )
    }

    /// `cWorld::GetEvents(ctx)`
    pub fn process_events(&mut self) {
        let mut events = std::mem::take(&mut self.events);
        if self.pop.sync_events {
            events.sync(|t| self.trigger(t));
            self.pop.sync_events = false;
        }
        events.process(self, |w, t| w.trigger(t), |w, a| w.run_action(a));
        self.events = events;
    }

    /// `cAction::Process(ctx)` of a parsed action.
    pub fn run_action(&mut self, action: &Action) {
        // `Driver().Abort(...)` ends the C++ process at once: no later
        // action of the update runs.
        if self.action_state.exit_code.is_some() {
            return;
        }
        if let Err(e) = self.run_action_inner(action) {
            self.set_fatal(e);
        }
        // Parasite units an action let go of leave their groups now.
        self.flush_parasite_deaths();
    }

    fn run_action_inner(&mut self, action: &Action) -> Result<(), String> {
        match action {
            Action::PrintStats { method, filename } => {
                if let Some(r) = self.print_stats_extra(method, filename) {
                    return r;
                }
                if let Some(r) = self.print_stats_with_world(method, filename) {
                    return r;
                }
                match stats_dispatch::print_stats(&mut self.stats, &mut self.out, method, filename)
                {
                    Some(r) => r,
                    None => Err(format!(
                        "unsupported: action {method} (C++: cStats::{method})"
                    )),
                }
            }
            Action::PrintResourceData { filename, maps } => {
                self.update_res_stats();
                self.stats
                    .print_resource_data(&mut self.out, filename, maps, self.pop.world_x)
            }
            Action::PrintData { filename, format } => {
                let errors = self
                    .stats
                    .print_data_file(&mut self.out, filename, format, ',')?;
                eprint!("{errors}");
                Ok(())
            }
            Action::PrintInstructionData { filename, inst_set } => {
                self.print_instruction_data(filename, inst_set)
            }
            Action::PrintDominantData { filename } => self.print_dominant_data(filename),
            Action::PrintDominantGenotype { filename } => self.print_dominant_genotype(filename),
            Action::Inject {
                filename,
                cell_id,
                merit,
                lineage_label,
                neutral_metric,
                trace_filename,
            } => {
                if filename.is_empty() {
                    eprintln!("error: no organism file specified");
                    return Err("Inject: no organism file specified".to_string());
                }
                if *cell_id < 0 || *cell_id >= self.pop.cells.len() as i32 {
                    self.action_state
                        .stdout
                        .push_str("warning: Inject has invalid cell id!\n");
                    return Ok(());
                }
                let Some(genome) = self.load_genome(filename, "", true)? else {
                    return Ok(());
                };
                self.inject(
                    &genome,
                    injected_source(),
                    *cell_id,
                    *merit,
                    *lineage_label,
                    *neutral_metric,
                )?;
                if !trace_filename.is_empty() {
                    return self.set_status_trace(*cell_id, trace_filename);
                }
                Ok(())
            }
            Action::InjectAll {
                filename,
                instset,
                merit,
                lineage_label,
                neutral_metric,
            } => {
                if filename.is_empty() {
                    eprintln!("error: no organism file specified");
                    return Ok(());
                }
                let Some(genome) = self.load_genome(filename, instset, false)? else {
                    return Ok(());
                };
                for i in 0..self.pop.cells.len() as i32 {
                    self.inject(
                        &genome,
                        injected_source(),
                        i,
                        *merit,
                        *lineage_label,
                        *neutral_metric,
                    )?;
                }
                Ok(())
            }
            Action::InjectRange {
                filename,
                cell_start,
                cell_end,
                merit,
                lineage_label,
                neutral_metric,
            } => {
                if filename.is_empty() {
                    eprintln!("error: no organism file specified");
                    return Ok(());
                }
                if !self.valid_cell_range(*cell_start, *cell_end) {
                    self.action_state
                        .stdout
                        .push_str("warning: InjectRange has invalid range!\n");
                    return Ok(());
                }
                let Some(genome) = self.load_genome(filename, "", false)? else {
                    return Ok(());
                };
                for i in *cell_start..*cell_end {
                    self.inject(
                        &genome,
                        injected_source(),
                        i,
                        *merit,
                        *lineage_label,
                        *neutral_metric,
                    )?;
                }
                self.pop.sync_events = true;
                Ok(())
            }
            Action::InjectSequence {
                sequence,
                cell_start,
                cell_end,
                merit,
                lineage_label,
                neutral_metric,
            } => {
                if !self.valid_cell_range(*cell_start, *cell_end) {
                    self.action_state
                        .stdout
                        .push_str("warning: InjectSequence has invalid range!\n");
                    return Ok(());
                }
                let is = self
                    .hw
                    .registry()
                    .shared_inst_set(0)
                    .ok_or("InjectSequence: no instruction set")?;
                let mut props = PropertyMap::new();
                setup_property_map(&mut props, is.inst_set_name());
                let genome = Genome::new(
                    is.hardware_type(),
                    &props,
                    InstructionSequence::from_string(sequence),
                );
                for i in *cell_start..*cell_end {
                    self.inject(
                        &genome,
                        injected_source(),
                        i,
                        *merit,
                        *lineage_label,
                        *neutral_metric,
                    )?;
                }
                self.pop.sync_events = true;
                Ok(())
            }
            Action::KillProb { killprob } => {
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
            Action::ZeroMuts => {
                for cell in &mut self.pop.cells {
                    cell.mut_rates.clear();
                }
                self.cfg.germline_copy_mut = 0.0;
                self.cfg.germline_ins_mut = 0.0;
                self.cfg.germline_del_mut = 0.0;
                self.cfg.inst_point_mut_prob = 0.0;
                Ok(())
            }
            Action::SavePopulation {
                filename,
                save_historic,
                save_group_info,
                save_avatars,
                save_rebirth,
            } => {
                let path = format!("{}-{}.spop", filename, self.stats.update());
                self.save_population(
                    &path,
                    *save_historic,
                    *save_group_info,
                    *save_avatars,
                    *save_rebirth,
                )
            }
            Action::LoadPopulation {
                filename,
                update,
                cellid_offset,
                lineage_offset,
                load_groups,
                load_birth_cells,
                load_avatars,
                load_rebirth,
                load_parent_dat,
                load_traceq,
            } => {
                // set the update if requested
                if *update >= 0 {
                    self.stats.set_current_update(*update);
                    self.res_env.update = *update;
                }
                let args = LoadPopulationArgs {
                    cellid_offset: *cellid_offset,
                    lineage_offset: *lineage_offset,
                    load_groups: *load_groups,
                    load_birth_cells: *load_birth_cells,
                    load_avatars: *load_avatars,
                    load_rebirth: *load_rebirth,
                    load_parent_dat: *load_parent_dat,
                    load_traceq: *load_traceq,
                };
                if !self.load_population(filename, &args)? {
                    // `Feedback().Error(...)` (printf to stdout), then
                    // `Abort(INVALID_CONFIG)`.
                    self.action_state
                        .stdout
                        .push_str("error: failed to load population\n");
                    self.action_state.exit_code = Some(2);
                    return Err("failed to load population".to_string());
                }
                Ok(())
            }
            Action::SetRandomSeed { seed } => {
                self.cfg.random_seed = *seed;
                self.rng.reset_seed(*seed);
                Ok(())
            }
            Action::Exit => {
                self.done = true;
                Ok(())
            }
            Action::InjectAllRandomRepro {
                length,
                merit,
                lineage_label,
                neutral_metric,
                sex,
            } => {
                self.inject_all_random_repro(*length, *merit, *lineage_label, *neutral_metric, *sex)
            }
            Action::InjectWholeGenomeDuplications { nopc_prepend } => {
                self.inject_whole_genome_duplications(*nopc_prepend)
            }
            Action::ToggleRewardInstruction => {
                self.cfg.merit_bonus_effect = -self.cfg.merit_bonus_effect;
                Ok(())
            }
            Action::ToggleFitnessValley => {
                self.cfg.fitness_valley = i32::from(self.cfg.fitness_valley == 0);
                Ok(())
            }
            Action::PrintCurrReactionCountGrid { filename } => {
                self.print_curr_reaction_count_grid(filename)
            }
            Action::PrintInstructionAbundanceHistogram { filename, inst_set } => {
                self.print_instruction_abundance_histogram(filename, inst_set)
            }
            Action::DumpMaxResGrid { filename } => self.dump_max_res_grid(filename),
            Action::PrintMatingTypeHistogram { filename } => {
                self.print_mating_type_histogram(filename)
            }
            Action::PrintBirthChamberMatingTypeHistogram { filename, hw_type } => {
                self.print_birth_chamber_mating_type_histogram(filename, *hw_type)
            }
            Action::PrintMatingDisplayData { filename } => self.print_mating_display_data(filename),
            Action::PrintFemaleMatePreferenceData { filename } => {
                self.print_female_mate_preference_data(filename)
            }
            Action::PrintBirthChamber { filename, hw_type } => {
                self.print_birth_chamber(filename, *hw_type)
            }
            Action::PrintGroupTolerance {
                filename,
                by_mating_type,
            } => self.print_group_tolerance(filename, *by_mating_type),
            Action::Deme(d) => self.run_deme_action(d),
            Action::Compete(c) => self.run_compete_action(c),
            Action::Parasite(p) => self.run_parasite_action(p),
            Action::PredPrey(p) => self.run_pred_prey_action(p),
            Action::PopPrint(p) => self.run_pop_print_action(p),
            Action::Stats2(a) => self.run_stats2_action(a),
            Action::Print2(a) => self.run_print2_action(a),
            Action::Hist(a) => self.run_hist_action(a),
            Action::Pop2(a) => self.run_pop2_action(a),
            Action::Deme2(a) => self.run_deme2_action(a),
            Action::Env(a) => self.run_env_action(a),
            Action::MiniTrace(m) => self.run_mini_trace_action(m),
            Action::A3(a) => self.run_action3(a),
        }
    }

    /// `avd_popaction_is_valid_cell_range`: `0 <= start < end <= size`.
    fn valid_cell_range(&self, start: i32, end: i32) -> bool {
        let size = self.pop.cells.len() as i32;
        start >= 0 && end <= size && start < end
    }

    /// `cerr << line << endl; exit(1);` (as `cInstSet::GetInst` does for an
    /// unknown instruction name): `line` goes to stderr as it is, without
    /// the `error: ` prefix of a run error, and the run ends with status 1.
    /// Returns `line` for the `Err` that unwinds the run.
    pub(crate) fn exit_with_stderr_line(&mut self, line: String) -> String {
        eprintln!("{line}");
        if self.action_state.exit_code.is_none() {
            self.action_state.exit_code = Some(1);
        }
        line
    }

    /// `Util::LoadGenomeDetailFile` followed by the action's feedback loop.
    /// `abort_on_error` is `cActionInject`, which aborts on an error message.
    pub(crate) fn load_genome(
        &mut self,
        filename: &str,
        instset: &str,
        abort_on_error: bool,
    ) -> Result<Option<Genome>, String> {
        let mut feedback = GenomeFeedback::new();
        let genome = load_genome_detail_file(
            filename,
            &self.working_dir,
            self.hw.registry(),
            &mut feedback,
            instset,
        )
        .map_err(|line| self.exit_with_stderr_line(line))?;
        for (kind, msg) in feedback.messages() {
            match kind {
                FeedbackKind::Error => {
                    eprintln!("error: {msg}");
                    if abort_on_error {
                        return Err(msg.clone());
                    }
                }
                FeedbackKind::Warning => eprintln!("warning: {msg}"),
                _ => {
                    if !abort_on_error {
                        eprintln!("{msg}");
                    }
                }
            }
        }
        Ok(genome)
    }

    /// `cActionPrintDominantData::Process`
    fn print_dominant_data(&mut self, filename: &str) -> Result<(), String> {
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida Dominant Data");
        df.write_time_stamp();
        df.write(update, "Update");

        // QUIRK: without a genotype the C++ returns before `Endl()`, leaving
        // the line open.
        let Some(bg) = self.systematics.dominant_genotype() else {
            return Ok(());
        };
        let sys = &self.systematics;
        let d = |name: &str| sys.property(bg, name).double_value();
        let i = |name: &str| sys.property(bg, name).int_value();

        df.write(
            d("ave_metabolic_rate"),
            "Average Merit of the Dominant Genotype",
        );
        df.write(
            d("ave_gestation_time"),
            "Average Gestation Time of the Dominant Genotype",
        );
        df.write(d("ave_fitness"), "Average Fitness of the Dominant Genotype");
        df.write(d("ave_repro_rate"), "Repro Rate?");
        let genome = Genome::from_string(&sys.property(bg, "genome").string_value());
        df.write(genome.representation().size(), "Size of Dominant Genotype");
        df.write(d("ave_copy_size"), "Copied Size of Dominant Genotype");
        df.write(d("ave_exe_size"), "Executed Size of Dominant Genotype");
        df.write(
            sys.num_units(bg).unwrap_or(0),
            "Abundance of Dominant Genotype",
        );
        df.write(i("last_births"), "Number of Births");
        df.write(i("last_breed_true"), "Number of Dominant Breed True?");
        df.write(sys.depth(bg).unwrap_or(0), "Dominant Gene Depth");
        df.write(i("last_breed_in"), "Dominant Breed In");
        df.write(d("max_fitness"), "Max Fitness?");
        df.write(
            sys.group_id(bg).unwrap_or(-1),
            "Genotype ID of Dominant Genotype",
        );
        df.write(
            sys.property(bg, "name").string_value(),
            "Name of the Dominant Genotype",
        );
        df.endl();
        Ok(())
    }

    /// `cActionPrintDominantGenotype::Process`
    fn print_dominant_genotype(&mut self, filename: &str) -> Result<(), String> {
        let Some(bg) = self.systematics.dominant_genotype() else {
            return Ok(());
        };
        let filename = if filename.is_empty() {
            format!(
                "archive/{}.org",
                self.systematics.property(bg, "name").string_value()
            )
        } else {
            filename.to_string()
        };
        let genome = Genome::from_string(&self.systematics.property(bg, "genome").string_value());
        let update = self.stats.update();
        let mut df = self.out.create_with_path(&filename)?;
        let result = {
            let mut tw = make_test_world(
                &self.cfg,
                &mut self.env,
                &mut self.rng,
                &self.hw,
                &mut self.stats,
                &mut self.host,
                &*self.group_attack_fn,
                self.test_sterilize,
                &self.pop,
            );
            TestCpu::new(&mut tw).and_then(|mut cpu| {
                cpu.print_genome(
                    &mut tw, &genome, &mut df, &filename, update, false, 0, -1, -1,
                )
            })
        };
        self.flush_host();
        result
    }

    /// `cPopulation::SavePopulation(filename, save_historic, save_groupings,
    /// save_avatars, save_rebirth)`. Parasites are rejected at start-up, so
    /// only the organisms themselves are saved.
    pub fn save_population(
        &mut self,
        filename: &str,
        save_historic: bool,
        save_groupings: bool,
        save_avatars: bool,
        save_rebirth: bool,
    ) -> Result<(), String> {
        let mut save = PopulationSave::new(SaveOptions {
            save_historic,
            save_groupings,
            save_avatars,
            save_rebirth,
            use_avatars: self.cfg.use_avatars != 0,
        });
        for (cell, slot) in self.pop.orgs.iter().enumerate() {
            let Some(org) = slot.as_ref() else {
                continue;
            };
            // Handle any parasites
            self.save_parasites(&mut save, cell);
            let Some(g) = self.systematics.genotype_of(&self.pop.membership[cell]) else {
                continue;
            };
            let state = &org.state;
            let data = OrganismSaveData {
                cpu_cycles_used: state.phenotype.cpu_cycles_used,
                lineage_label: state.lineage_label,
                opinion_group: state
                    .opinions
                    .as_ref()
                    .and_then(|o| o.last())
                    .map_or(-1, |o| o.0),
                forage_target: state.forage_target,
                birth_cell: state.phenotype.birth_cell_id,
                av_cell_id: self.av_cell_id(state.uid, 0),
                av_birth_cell: state.phenotype.av_birth_cell_id,
                parent_ft: state.parent_ft,
                had_parent_teacher: state.parent_teacher,
                // `GetParentMerit()`: set at birth and by `LoadPopulation`.
                parent_merit: state.p_merit,
            };
            save.add_organism(&self.systematics, g, cell as i32, &data);
        }
        let mut df = self.out.create_with_path(filename)?;
        save.write(&self.systematics, &mut df);
        Ok(())
    }

    /// The point-mutation pass at the end of an update.
    fn point_mutate_all(&mut self) {
        for c in 0..self.pop.orgs.len() {
            let Some(mut org) = self.pop.orgs[c].take() else {
                continue;
            };
            let num_mut = {
                let mut iface = PopulationInterface {
                    w: self,
                    cell_id: c as i32,
                };
                // `PointMutate(ctx)`: the default override rate is 0.0.
                org.hardware.point_mutate(&org.state, &mut iface, 0.0)
            };
            org.state.num_point_mut += num_mut;
            self.pop.orgs[c] = Some(org);
        }
    }

    pub(crate) fn check_fatal(&mut self) -> Result<(), String> {
        match self.take_fatal() {
            Some(f) => Err(f),
            None => Ok(()),
        }
    }

    /// `Avida2Driver::Run` for a population run (`ANALYZE_MODE == 0`). The
    /// per-update status lines go to `stdout`.
    pub fn run(&mut self, stdout: &mut dyn Write) -> Result<(), String> {
        let mut stepper = Stepper::new(self);
        let result = (|| {
            while stepper.step_update(self, stdout)? {}
            Ok(())
        })();
        // A diagnostic of the port's parallel mode, not a C++ output.
        if std::env::var_os("AVIDA_EXECUTION_STATS").is_some() {
            if let Some(stats) = stepper.parallel_stats() {
                eprintln!("{}", stats.summary());
            }
        }
        result
    }
}

/// `Avida2Driver::Run` cut at its natural pause points, so a front end can
/// run, pause and single-step without changing what the run computes: the
/// same calls in the same order as the C++ loop, whatever the pacing.
///
/// One update is [`Stepper::begin_update`], then [`Stepper::step_slice`]
/// until it returns `false`, then [`Stepper::end_update`];
/// [`Stepper::step_update`] does all three. [`Stepper::step_watched`] in
/// place of `step_slice` also stops between the instructions of one
/// organism's speculative time slices (see [`crate::open_slice`]).
///
/// Under `EXECUTION_MODE 1` (see [`crate::parallel`]) an update is a
/// single slice: the first `step_slice` (or `step_watched`) of the update
/// runs all of it, the next one returns `false` (`UpdateDone`).
#[derive(Debug, Clone)]
pub struct Stepper {
    point_mut_prob: f64,
    speculative: bool,
    /// The parallel execution mode, when `EXECUTION_MODE` is 1.
    parallel: Option<crate::parallel::ParallelExec>,
    started: bool,
    in_update: bool,
    /// `UD_size` of the update in progress.
    ud_size: i32,
    step_size: f64,
    /// Slices of the update in progress already run (an open one
    /// included).
    slice: i32,
    /// The time slice [`Stepper::step_watched`] paused inside.
    open: Option<crate::open_slice::OpenSlice>,
}

/// What one [`Stepper::step_watched`] ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchedStep {
    /// Nothing: the update has no slices left ([`Stepper::step_slice`]
    /// returning `false`).
    UpdateDone,
    /// A whole time slice: of another organism, of the watched one when
    /// its slice is not cut (no `SPECULATIVE`, no burst), or the rest of a
    /// slice left open while the watch moved elsewhere.
    Slice,
    /// One instruction of a speculative time slice of the watched
    /// organism; `closed` when the slice ended with it.
    Instruction { closed: bool },
}

impl Stepper {
    /// Reads the settings `Run` reads before its loop.
    pub fn new(w: &World) -> Self {
        let point_mut_prob =
            w.cfg.point_mut_prob + w.cfg.point_ins_prob + w.cfg.point_del_prob + w.cfg.div_lgt_prob;
        let parallel = crate::parallel::ParallelExec::for_world(w);
        let speculative = w.cfg.speculative
            && w.cfg.thread_slicing_method != 1
            && w.cfg.implicit_repro_end == 0
            && point_mut_prob == 0.0
            && parallel.is_none();
        Stepper {
            point_mut_prob,
            speculative,
            parallel,
            started: false,
            in_update: false,
            ud_size: 0,
            step_size: 0.0,
            slice: 0,
            open: None,
        }
    }

    /// Whether an update has begun and not yet ended.
    pub fn in_update(&self) -> bool {
        self.in_update
    }

    /// Whether the run uses `ProcessStepSpeculative` (`SPECULATIVE` on and
    /// nothing that turns it off; never in the parallel mode).
    pub fn speculative(&self) -> bool {
        self.speculative
    }

    /// Whether the run uses the parallel execution mode
    /// (`EXECUTION_MODE 1`), whose updates are a single slice.
    pub fn parallel(&self) -> bool {
        self.parallel.is_some()
    }

    /// The performance counters of the parallel execution mode.
    pub fn parallel_stats(&self) -> Option<crate::parallel::ParallelStats> {
        self.parallel.as_ref().map(|p| p.stats)
    }

    /// The time slice [`Stepper::step_watched`] paused inside, if any.
    pub fn open_slice(&self) -> Option<&crate::open_slice::OpenSlice> {
        self.open.as_ref()
    }

    /// `(slices run, UD_size)` of the update in progress.
    pub fn slice_progress(&self) -> (i32, i32) {
        (self.slice, self.ud_size)
    }

    /// Events, the update increment and the pre-update work. Returns
    /// `false`, without beginning an update, once the run is done.
    pub fn begin_update(&mut self, w: &mut World, stdout: &mut dyn Write) -> Result<bool, String> {
        if self.in_update {
            return Err("Stepper::begin_update called inside an update".to_string());
        }
        if !self.started {
            self.started = true;
            // `cWorld::SetDriver` (the Avida2Driver constructor): from here
            // on `ctx.HasDriver()`, which damaging and deadly gradients test.
            w.res_env.has_driver = true;
            w.note_recorders();
        }
        if w.done {
            return Ok(false);
        }
        w.process_events();
        w.flush_action_stdout(stdout)?;
        w.check_fatal()?;
        if w.done {
            return Ok(false);
        }

        // Increment the Update.
        w.stats.inc_current_update();
        w.res_env.update = w.stats.update();

        w.process_pre_update();

        // Handle all data collection for previous update.
        if w.stats.update() > 0 {
            w.stats.process_update(w.cfg.ave_time_slice);
        }

        // Process the update.
        self.ud_size = w.calculate_update_size();
        self.step_size = 1.0 / f64::from(self.ud_size);
        self.slice = 0;
        self.in_update = true;
        Ok(true)
    }

    /// Runs the next scheduled organism for one time slice (or the rest of
    /// a slice [`Stepper::step_watched`] left open). Returns `false` when
    /// the update has no slices left (or no organisms).
    pub fn step_slice(&mut self, w: &mut World) -> Result<bool, String> {
        if !self.in_update {
            return Err("Stepper::step_slice called outside an update".to_string());
        }
        if let Some(open) = self.open.take() {
            w.close_slice(open)?;
            w.check_fatal()?;
            return Ok(true);
        }
        if self.slice >= self.ud_size || w.pop.num_organisms == 0 {
            return Ok(false);
        }
        if let Some(par) = self.parallel.as_mut() {
            // The whole update is one slice.
            self.slice = self.ud_size;
            par.run_update(w, self.ud_size, self.step_size)?;
            w.check_fatal()?;
            return Ok(true);
        }
        self.slice += 1;
        let cell_id = w.pop.scheduler.next();
        w.process_step(self.step_size, cell_id, self.speculative)?;
        w.check_fatal()?;
        Ok(true)
    }

    /// [`Stepper::step_slice`], except that a speculative time slice of the
    /// organism living in `watch` runs one instruction per call: its first
    /// instruction, then each of its burst (up to 32), the last of which
    /// closes the slice. Interleaving the two, with any `watch`, computes
    /// the same run as `step_slice` alone; a slice left open while the
    /// watch moved elsewhere is finished first.
    pub fn step_watched(&mut self, w: &mut World, watch: i32) -> Result<WatchedStep, String> {
        if !self.in_update {
            return Err("Stepper::step_watched called outside an update".to_string());
        }
        if self.parallel.is_some() {
            // No time slices to cut: the update is one step.
            return Ok(if self.step_slice(w)? {
                WatchedStep::Slice
            } else {
                WatchedStep::UpdateDone
            });
        }
        if let Some(open) = self.open.take() {
            if open.cell() != watch {
                w.close_slice(open)?;
                w.check_fatal()?;
                return Ok(WatchedStep::Slice);
            }
            self.open = w.open_slice_next(open)?;
            if self.open.is_none() {
                w.check_fatal()?;
            }
            return Ok(WatchedStep::Instruction {
                closed: self.open.is_none(),
            });
        }
        if self.slice >= self.ud_size || w.pop.num_organisms == 0 {
            return Ok(WatchedStep::UpdateDone);
        }
        self.slice += 1;
        let cell_id = w.pop.scheduler.next();
        let cut = self.speculative
            && cell_id >= 0
            && cell_id == watch
            && w.pop.spec_state[cell_id as usize] == 0;
        if !cut {
            w.process_step(self.step_size, cell_id, self.speculative)?;
            w.check_fatal()?;
            return Ok(WatchedStep::Slice);
        }
        self.open = w.open_slice(self.step_size, cell_id)?;
        if self.open.is_none() {
            w.check_fatal()?;
        }
        Ok(WatchedStep::Instruction {
            closed: self.open.is_none(),
        })
    }

    /// The end-of-update work: stats, the status line, point mutations and
    /// the systematics update. Any slices not yet run are run first.
    pub fn end_update(&mut self, w: &mut World, stdout: &mut dyn Write) -> Result<(), String> {
        while self.step_slice(w)? {}
        self.in_update = false;

        // end of update stats... (the parallel mode reads the organisms on
        // its worker threads, with the same result)
        let t_post = std::time::Instant::now();
        w.process_post_update_with(self.parallel.as_ref().and_then(|p| p.pool()));
        if let Some(p) = self.parallel.as_mut() {
            p.stats.post_time += t_post.elapsed();
        }
        let t_after = std::time::Instant::now();
        // What organisms printed to `std::cout` during the steps.
        w.flush_action_stdout(stdout)?;
        w.check_fatal()?;
        // `InstructionExecCountsProvider` gathers in `UpdateOrganismStats`;
        // `PerformUpdate` below hands the values to the recorders.
        w.record_inst_exec_counts();
        w.actions2_end_update();

        if let Some(text) = w.status_line().render(w.cfg.verbosity) {
            writeln!(stdout, "{text}").map_err(|e| e.to_string())?;
        }

        // Do Point Mutations
        if self.point_mut_prob > 0.0 {
            w.point_mutate_all();
            w.check_fatal()?;
        }

        // `m_new_world->PerformUpdate(new_ctx, stats.GetUpdate())`
        let update = w.stats.update();
        w.systematics.update_provided_values(update);
        if let Some(v) = w.systematics.provided_value("systematics.genotype.current") {
            w.stats.m_num_genotypes = v.int_value();
        }
        if let Some(v) = w
            .systematics
            .provided_value("systematics.genotype.current_threshold")
        {
            w.stats.m_threshold_genotypes = v.int_value();
        }
        w.systematics.perform_update(update);

        // Exit conditons...
        if w.pop.num_organisms == 0 {
            w.done = true;
        }
        if let Some(p) = self.parallel.as_mut() {
            p.stats.after_time += t_after.elapsed();
        }
        Ok(())
    }

    /// One whole update (finishing one in progress). Returns `false` once
    /// the run is done.
    pub fn step_update(&mut self, w: &mut World, stdout: &mut dyn Write) -> Result<bool, String> {
        if !self.in_update && !self.begin_update(w, stdout)? {
            return Ok(false);
        }
        self.end_update(w, stdout)?;
        Ok(true)
    }
}

impl World {
    /// The values of the per-update status line.
    pub fn status_line(&self) -> StatusLine {
        StatusLine {
            update: self.stats.update(),
            ave_generation: self.stats.sum_generation.average(),
            ave_fitness: self.stats.sum_fitness.average(),
            num_organisms: self.pop.num_organisms,
            num_demes: self.pop.demes.len() as i32,
            num_occupied_demes: self.stats.m_num_occupied_demes,
            ave_merit: self.stats.sum_merit.average(),
            num_threads: self.stats.m_num_threads,
            num_parasites: self.stats.num_parasites,
            ave_speculative: if self.stats.m_spec_num != 0 {
                f64::from(self.stats.m_spec_total) / f64::from(self.stats.m_spec_num)
            } else {
                0.0
            },
            speculative_waste: self.stats.m_spec_waste,
            update_size: self.calculate_update_size(),
        }
    }
}
