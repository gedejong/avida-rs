//! Control commands of `cAnalyze`: variables, configuration, batches,
//! recalculation, functions and loops, and the small utilities (`ECHO`,
//! `STATUS`, `TEST`, `INCLUDE`, `SYSTEM`, `ENVIRONMENT`).

use std::io::Write;

use avida_config::string_util as su;
use avida_config::{Feedback, InitFile};
use avida_genome::Genome;

use crate::analyze::{file_lines, Analyze, GenotypeBatch, MAX_BATCHES, NUM_BATCHES_INCREMENT};
use crate::error::{AnalyzeError, Result};
use crate::flex::{CmpOp, FlexVar};
use crate::genotype::ParentInfoView;
use crate::host::{AnalyzeHost, CpuTestOptions};
use crate::verbose::{VERBOSE_NORMAL, VERBOSE_ON};
use crate::Command;

impl Analyze {
    /// `VarSet`: `SET var value`.
    pub(crate) fn var_set(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let var = su::pop_word(&mut cur_string);
        if cur_string.is_empty() {
            writeln!(self.err, "Error: No variable provided in SET command")?;
            return Ok(());
        }
        let slot = self.get_variable(&var)?;
        let value = su::pop_word(&mut cur_string);
        self.set_var(slot, value);
        if Self::verbosity(host) >= VERBOSE_ON {
            let v = self.var(slot).to_string();
            writeln!(self.out, "Setting {var} to {v}")?;
        }
        Ok(())
    }

    /// `ConfigGet`: `CONFIG_GET setting var`.
    pub(crate) fn config_get(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let cvar = su::pop_word(&mut cur_string);
        let var = su::pop_word(&mut cur_string);
        if cvar.is_empty() || var.is_empty() {
            writeln!(self.err, "Error: Missing variable in CONFIG_GET command")?;
            return Ok(());
        }
        let slot = self.get_variable(&var)?;
        match host.config().get(&cvar) {
            Some(v) => self.set_var(slot, v),
            None => {
                // QUIRK: names the analyze variable, not the setting.
                writeln!(
                    self.err,
                    "Error: Configuration Variable '{var}' was not found."
                )?;
                return Ok(());
            }
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            let v = self.var(slot).to_string();
            writeln!(self.out, "Setting variable {var} to {v}")?;
        }
        Ok(())
    }

    /// `ConfigSet`: `CONFIG_SET setting value`.
    pub(crate) fn config_set(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let cvar = su::pop_word(&mut cur_string);
        if cvar.is_empty() {
            writeln!(
                self.err,
                "Error: No variable provided in CONFIG_SET command"
            )?;
            return Ok(());
        }
        let val = su::pop_word(&mut cur_string);
        if !host.config_mut().set(&cvar, &val) {
            writeln!(
                self.err,
                "Error: Configuration Variable '{cvar}' was not found."
            )?;
            return Ok(());
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Setting configuration variable {cvar} to {val}")?;
        }
        Ok(())
    }

    /// `BatchSet`: `SET_BATCH id`, growing the batch array in steps of
    /// `NUM_BATCHES_INCREMENT` up to `MAX_BATCHES`.
    pub(crate) fn batch_set(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut next_batch = 0;
        if su::count_num_words(&cur_string) > 0 {
            next_batch = su::as_int(&su::pop_word(&mut cur_string));
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Setting current batch to {next_batch}")?;
        }
        if next_batch >= self.num_batches() {
            if next_batch >= MAX_BATCHES {
                return Err(self.exit_with(&format!("  Error: max batches is {MAX_BATCHES}")));
            }
            let old_num_batches = self.num_batches();
            let num_batchsets_needed = ((next_batch - old_num_batches) / NUM_BATCHES_INCREMENT) + 1;
            let new_num_batches =
                (old_num_batches + num_batchsets_needed * NUM_BATCHES_INCREMENT).min(MAX_BATCHES);
            writeln!(self.out, "Increasing max batches to {new_num_batches}")?;
            for i in old_num_batches..new_num_batches {
                self.batches.push(GenotypeBatch {
                    name: format!("Batch{i}"),
                    ..Default::default()
                });
            }
        }
        // QUIRK: a negative id is accepted; later batch accesses fail.
        self.cur_batch = next_batch;
        Ok(())
    }

    /// `BatchName`: `NAME_BATCH name`.
    pub(crate) fn batch_name(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if su::count_num_words(&cur_string) == 0 {
            if Self::verbosity(host) >= VERBOSE_ON {
                writeln!(self.out, "  Warning: No name given in NAME_BATCH!")?;
            }
            return Ok(());
        }
        let name = su::pop_word(&mut cur_string);
        self.cur()?.name = name;
        Ok(())
    }

    /// `BatchTag`: `TAG_BATCH tag...` (the whole argument string).
    pub(crate) fn batch_tag(
        &mut self,
        host: &mut dyn AnalyzeHost,
        cur_string: String,
    ) -> Result<()> {
        if su::count_num_words(&cur_string) == 0 {
            if Self::verbosity(host) >= VERBOSE_ON {
                writeln!(self.out, "  Warning: No tag given in TAG_BATCH!")?;
            }
            return Ok(());
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Tagging batch {} with tag '{cur_string}'",
                self.cur_batch
            )?;
        }
        for g in self.cur()?.list.iter_mut() {
            g.tag = cur_string.clone();
        }
        Ok(())
    }

    /// `BatchPurge`: `PURGE_BATCH [id]`.
    pub(crate) fn batch_purge(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut batch_id = self.cur_batch;
        if su::count_num_words(&cur_string) > 0 {
            batch_id = su::as_int(&su::pop_word(&mut cur_string));
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Purging batch {batch_id}")?;
        }
        let bi = self.batch_index(batch_id)?;
        self.batches[bi].list.clear();
        self.batches[bi].clear_flags();
        Ok(())
    }

    /// `BatchDuplicate`: `DUPLICATE from [to]` with the C++ copy constructor.
    pub(crate) fn batch_duplicate(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if cur_string.is_empty() {
            return Err(self.exit_with("Duplicate Error: Must include from ID!"));
        }
        let batch_from = su::as_int(&su::pop_word(&mut cur_string));
        let mut batch_to = self.cur_batch;
        if !cur_string.is_empty() {
            batch_to = su::as_int(&su::pop_word(&mut cur_string));
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Duplicating from batch {batch_from} to batch {batch_to}."
            )?;
        }
        let from = self.batch_index(batch_from)?;
        let to = self.batch_index(batch_to)?;
        if from == to && !self.batches[from].list.is_empty() {
            return Err(AnalyzeError::fatal(
                "DUPLICATE onto the same batch: the C++ iterates the list it appends to and never ends",
            ));
        }
        let copies: Vec<_> = self.batches[from]
            .list
            .iter()
            .map(|g| g.duplicate())
            .collect();
        self.batches[to].list.extend(copies);
        self.batches[to].clear_flags();
        Ok(())
    }

    /// Run the current batch through the test CPU, passing the previous
    /// genotype as parent when it is the recorded parent. One `cCPUTestInfo`
    /// (built from `opts`) serves the whole batch, as in C++.
    fn recalculate_batch(
        &mut self,
        host: &mut dyn AnalyzeHost,
        opts: CpuTestOptions,
        num_trials: i32,
    ) -> Result<()> {
        let mut test_info = opts.test_info();
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Running batch {} through test CPUs...",
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Running through test CPUs...")?;
        }
        let bi = self.batch_index(self.cur_batch)?;
        if Self::verbosity(host) >= VERBOSE_ON && !self.batches[bi].is_lineage {
            writeln!(
                self.err,
                "warning: Batch may not be a lineage; parent and ancestor distances may not be correct"
            )?;
        }
        // cAnalyzeGenotype::Recalculate per genotype, in the default
        // context; the test of a genotype reads nothing earlier ones left
        // but the world RNG (the shared `test_info` is cleared by each
        // test, and its settings are those a fresh one gets).
        let genomes: Vec<Genome> = self.batches[bi]
            .list
            .iter()
            .map(|g| g.genome.clone())
            .collect();
        let mut last: Option<(i32, ParentInfoView)> = None;
        self.run_default_context_jobs(
            host,
            genomes.len(),
            &|lend, i| {
                let mut info = opts.test_info();
                let mut r = None;
                lend(&mut |w| {
                    r = Some(crate::cpu::recalculate_in(
                        w,
                        &genomes[i],
                        &mut info,
                        num_trials,
                    )?);
                    Ok(())
                })?;
                r.ok_or_else(|| {
                    AnalyzeError::fatal("recalculate: the host did not run the test CPU")
                })
            },
            &mut |_, host, i| {
                crate::cpu::recalculate(host, &genomes[i], &mut test_info, num_trials)
            },
            &mut |a, _, i, r| {
                let g = &mut a.batches[bi].list[i];
                let parent = match &last {
                    Some((id, p)) if g.parent_id == *id => Some(p),
                    _ => None,
                };
                g.apply_recalc(r, parent, num_trials);
                last = Some((g.id_num, ParentInfoView::of(g)));
                Ok(())
            },
        )
    }

    /// `BatchRecalculate`: `RECALCULATE [use_resources [update [random_inputs [inputs...]]]]`.
    pub(crate) fn batch_recalculate(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let use_resources = if !cur_string.is_empty() {
            su::as_int(&su::pop_word(&mut cur_string))
        } else {
            0
        };
        let update = if !cur_string.is_empty() {
            su::as_int(&su::pop_word(&mut cur_string))
        } else {
            -1
        };
        let mut use_random_inputs = if !cur_string.is_empty() {
            su::as_int(&su::pop_word(&mut cur_string)) == 1
        } else {
            false
        };
        let manual = self.pop_manual_inputs(host, &mut cur_string)?;
        if manual.is_some() {
            use_random_inputs = false;
        }
        let opts = CpuTestOptions {
            use_resources,
            update,
            use_random_inputs,
            manual_inputs: manual,
            resources: self.resources.clone(),
            resource_time_spent_offset: self.resource_time_spent_offset,
            state_grid: 0,
        };
        self.recalculate_batch(host, opts, 1)
    }

    /// `BatchRecalculateWithArgs`: `RECALC [use_resources] [use_random_inputs]
    /// [use_manual_inputs i1 i2 ..] [update N] [num_trials X]`.
    pub(crate) fn batch_recalculate_with_args(
        &mut self,
        host: &mut dyn AnalyzeHost,
        cur_string: String,
    ) -> Result<()> {
        let mut args = su::split_list(&cur_string, ' ');
        let pop_string = |args: &mut Vec<String>, s: &str| -> bool {
            match args.iter().position(|a| a == s) {
                Some(p) => {
                    args.remove(p);
                    true
                }
                None => false,
            }
        };
        let locate = |args: &[String], s: &str| args.iter().position(|a| a == s);
        let pop_line = |args: &mut Vec<String>, pos: usize| -> Result<String> {
            if pos < args.len() {
                Ok(args.remove(pos))
            } else {
                Err(AnalyzeError::fatal(
                    "RECALC: cStringList::PopLine past the end (C++ NULL dereference)",
                ))
            }
        };

        let use_resources = pop_string(&mut args, "use_resources");
        let mut use_random_inputs = pop_string(&mut args, "use_random_inputs");
        let mut manual: Option<Vec<i32>> = None;
        let mut update = -1;
        let mut num_trials = 1;
        if let Some(pos) = locate(&args, "use_manual_inputs") {
            pop_string(&mut args, "use_manual_inputs");
            let num = host.input_size();
            let mut inputs = vec![0; num.max(0) as usize];
            if args.len() as i64 >= pos as i64 + num as i64 - 2 {
                for v in inputs.iter_mut() {
                    *v = su::as_int(&pop_line(&mut args, pos)?);
                }
            } else {
                return Err(
                    self.exit_with("error: RecalculateWithArgs: Invalid use of use_manual_inputs")
                );
            }
            manual = Some(inputs);
        }
        if let Some(pos) = locate(&args, "update") {
            pop_string(&mut args, "update");
            if args.len() as i64 >= pos as i64 - 1 {
                update = su::as_int(&pop_line(&mut args, pos)?);
            } else {
                return Err(self.exit_with(
                    "error: RecalculateWithArgs: Invalid use of update (did you specify a value?)",
                ));
            }
        }
        if let Some(pos) = locate(&args, "num_trials") {
            pop_string(&mut args, "num_trials");
            if args.len() as i64 >= pos as i64 - 1 {
                num_trials = su::as_int(&pop_line(&mut args, pos)?);
            } else {
                return Err(self.exit_with(
                    "error: RecalculateWithArgs: Invalid use of num_trials (did you specify a value?)",
                ));
            }
        }
        if manual.is_some() {
            use_random_inputs = false;
        }
        let opts = CpuTestOptions {
            use_resources: use_resources as i32,
            update,
            use_random_inputs,
            manual_inputs: manual,
            resources: self.resources.clone(),
            resource_time_spent_offset: self.resource_time_spent_offset,
            state_grid: 0,
        };
        self.recalculate_batch(host, opts, num_trials)
    }

    /// `BatchRename`: `RENAME [first_id]` renumbers and renames `org-<id>`.
    pub(crate) fn batch_rename(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if Self::verbosity(host) <= VERBOSE_NORMAL {
            writeln!(self.out, "Renaming organisms...")?;
        } else {
            writeln!(self.out, "Renaming organisms in batch {}", self.cur_batch)?;
        }
        let mut id_num = su::as_int(&su::pop_word(&mut cur_string));
        for g in self.cur()?.list.iter_mut() {
            g.id_num = id_num;
            g.name = format!("org-{id_num}");
            id_num = id_num.wrapping_add(1);
        }
        Ok(())
    }

    /// `CloseFile`: `CLOSE_FILE path`.
    pub(crate) fn close_file(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let path = su::pop_word(&mut cur_string);
        let out = host.output();
        let oid = out.output_id_from_path(&path);
        out.close(&oid);
        Ok(())
    }

    /// `PrintStatus`: `STATUS`.
    pub(crate) fn print_status(&mut self) -> Result<()> {
        writeln!(self.out, "Status Report:")?;
        for (i, b) in self.batches.iter().enumerate() {
            let i = i as i32;
            if i == self.cur_batch || !b.list.is_empty() {
                write!(self.out, "  Batch {i} -- {} genotypes.", b.list.len())?;
                if i == self.cur_batch {
                    write!(self.out, "  <current>")?;
                }
                if b.is_lineage {
                    write!(self.out, "  <lineage>")?;
                }
                if b.is_aligned {
                    write!(self.out, "  <aligned>")?;
                }
                writeln!(self.out)?;
            }
        }
        Ok(())
    }

    /// `PrintDebug`: `ECHO` / `DEBUG`.
    pub(crate) fn print_debug(&mut self, cur_string: String) -> Result<()> {
        writeln!(self.out, "::: {cur_string}")?;
        Ok(())
    }

    /// `PrintTestInfo`: `TEST`, a self-check of `cFlexVar` conversions and
    /// comparisons.
    pub(crate) fn print_test_info(&mut self) -> Result<()> {
        let vars = [
            FlexVar::Str("Testing".into()),
            FlexVar::Int(1),
            FlexVar::Double(2.0),
            FlexVar::Char(b'3' as i8),
            FlexVar::Str("four".into()),
            FlexVar::Int(9),
            FlexVar::Double(9.0),
            FlexVar::Char(b'9' as i8),
            FlexVar::Str("9".into()),
            FlexVar::Int(0),
        ];
        write!(self.out, "AsString:  ")?;
        for (i, v) in vars.iter().enumerate() {
            write!(self.out, "{i}:{} ", v.as_string())?;
        }
        writeln!(self.out)?;
        write!(self.out, "AsInt:  ")?;
        for (i, v) in vars.iter().enumerate() {
            write!(self.out, "{i}:{} ", v.as_int())?;
        }
        writeln!(self.out)?;
        for i in 0..vars.len() {
            for j in i + 1..vars.len() {
                writeln!(
                    self.out,
                    "     vars[{i}] <= vars[{j}] ?  {}     vars[{j}] <= vars[{i}] ?  {}",
                    vars[i].cmp(&vars[j], CmpOp::Le) as i32,
                    vars[j].cmp(&vars[i], CmpOp::Le) as i32
                )?;
            }
        }
        Ok(())
    }

    /// `IncludeFile`: `INCLUDE` / `RUN file...` (a missing file is
    /// silently empty).
    pub(crate) fn include_file(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        while !cur_string.is_empty() {
            let filename = su::pop_word(&mut cur_string);
            let file = InitFile::new(&filename, host.working_dir(), None, None);
            let lines = file_lines(&file);
            let mut list = Vec::new();
            self.load_command_list(host, &lines, 0, &mut list);
            self.process_commands(host, &mut list)?;
        }
        Ok(())
    }

    /// `CommandSystem`: `SYSTEM command` through `/bin/sh -c`.
    pub(crate) fn command_system(&mut self, cur_string: String) -> Result<()> {
        if cur_string.is_empty() {
            return Err(
                self.exit_with("Error: Keyword \"system\" must be followed by command to run.")
            );
        }
        writeln!(self.out, "Running System Command: {cur_string}")?;
        self.flush()?;
        // The C++ ignores system()'s result.
        let _ = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&cur_string)
            .status();
        Ok(())
    }

    /// `EnvironmentSetup`: `ENVIRONMENT line` through the host.
    pub(crate) fn environment_setup(
        &mut self,
        host: &mut dyn AnalyzeHost,
        cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Running environment command: ")?;
        writeln!(self.out, "  {cur_string}")?;
        let mut fb = Feedback::new();
        host.load_environment_line(&cur_string, &mut fb)?;
        self.print_feedback(&fb)
    }

    /// `FunctionCreate`: `FUNCTION name` ... `END` moves the body into a
    /// new function.
    pub(crate) fn function_create(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
        body: &mut Vec<Command>,
    ) -> Result<()> {
        if su::count_num_words(&cur_string) < 1 {
            write!(
                self.err,
                "Error: Must provide function name when creating function."
            )?;
            let _ = self.err.flush();
            return Err(AnalyzeError::Exit(1));
        }
        let fun_name = su::pop_word(&mut cur_string);
        if self.find_def(host, &fun_name).is_some() {
            return Err(self.exit_with(&format!(
                "Error: Cannot create function '{fun_name}'; already exists."
            )));
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Creating function: {fun_name}")?;
        }
        self.functions.push((fun_name, std::mem::take(body)));
        Ok(())
    }

    /// `FunctionRun`: run the first function named `fun_name` with `$0` =
    /// name, `$1`..`$9` = the argument words and fresh `$A`..`$Z`.
    /// Returns false when there is no such function.
    pub(crate) fn function_run(
        &mut self,
        host: &mut dyn AnalyzeHost,
        fun_name: &str,
        mut args: String,
    ) -> Result<bool> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Running function: {fun_name}")?;
        }
        let Some(body) = self
            .functions
            .iter()
            .find(|(n, _)| n == fun_name)
            .map(|(_, b)| b.clone())
        else {
            return Ok(false);
        };
        let backup_args: Vec<String> = self.arg_variables[..10].to_vec();
        let backup_locals = self.local_variables.clone();
        self.arg_variables[0] = fun_name.to_string();
        for i in 1..10 {
            self.arg_variables[i] = su::pop_word(&mut args);
        }
        for v in self.local_variables.iter_mut() {
            v.clear();
        }
        // The C++ runs the stored list itself; a copy behaves the same
        // because the first definition of a name always wins.
        let mut body = body;
        self.process_commands(host, &mut body)?;
        self.arg_variables[..10].clone_from_slice(&backup_args);
        self.local_variables = backup_locals;
        Ok(true)
    }

    /// `CommandForeach`: `FOREACH var values...` ... `END`.
    pub(crate) fn command_foreach(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
        body: &mut [Command],
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Initiating Foreach loop...")?;
        }
        let var = su::pop_word(&mut cur_string);
        let num_args = su::count_num_words(&cur_string);
        let slot = self.get_variable(&var)?;
        for _ in 0..num_args {
            let value = su::pop_word(&mut cur_string);
            self.set_var(slot, value);
            if Self::verbosity(host) >= VERBOSE_ON {
                let v = self.var(slot).to_string();
                writeln!(self.out, "Foreach: setting {var} to {v}")?;
            }
            self.process_commands(host, body)?;
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Ending Foreach on {var}")?;
        }
        Ok(())
    }

    /// `CommandForRange`: `FORRANGE var min max [step]` ... `END`; integer
    /// values (`%d`) when all three are integral, else doubles (`%f`).
    pub(crate) fn command_forrange(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
        body: &mut [Command],
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Initiating FORRANGE loop...")?;
        }
        let num_args = su::count_num_words(&cur_string);
        if num_args < 3 {
            return Err(self.exit_with("  Error: Must give variable, min and max with FORRANGE!"));
        }
        let var = su::pop_word(&mut cur_string);
        let min_val = su::as_double(&su::pop_word(&mut cur_string));
        let max_val = su::as_double(&su::pop_word(&mut cur_string));
        let mut step_val = 1.0;
        if num_args >= 4 {
            step_val = su::as_double(&su::pop_word(&mut cur_string));
        }
        let slot = self.get_variable(&var)?;

        let integral = |v: f64| v == (v as i32) as f64;
        if integral(min_val) && integral(max_val) && integral(step_val) {
            let (lo, hi, step) = (min_val as i32, max_val as i32, step_val as i32);
            if lo <= hi && step <= 0 {
                return Err(AnalyzeError::fatal(format!(
                    "FORRANGE {lo} {hi} with step {step} never ends (as in C++)"
                )));
            }
            let mut i = lo;
            while i <= hi {
                self.set_var(slot, i.to_string());
                if Self::verbosity(host) >= VERBOSE_ON {
                    let v = self.var(slot).to_string();
                    writeln!(self.out, "FORRANGE: setting {var} to {v}")?;
                }
                self.process_commands(host, body)?;
                match i.checked_add(step) {
                    Some(n) => i = n,
                    None => {
                        return Err(AnalyzeError::fatal(
                            "FORRANGE counter overflows int (C++ undefined behaviour)",
                        ))
                    }
                }
            }
        } else {
            if min_val <= max_val && step_val <= 0.0 {
                return Err(AnalyzeError::fatal(format!(
                    "FORRANGE {min_val} {max_val} with step {step_val} never ends (as in C++)"
                )));
            }
            let mut i = min_val;
            while i <= max_val {
                self.set_var(slot, su::format_f(i));
                if Self::verbosity(host) >= VERBOSE_ON {
                    let v = self.var(slot).to_string();
                    writeln!(self.out, "FORRANGE: setting {var} to {v}")?;
                }
                self.process_commands(host, body)?;
                i += step_val;
            }
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Ending FORRANGE on {var}")?;
        }
        Ok(())
    }
}
