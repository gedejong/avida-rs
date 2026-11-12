//! Loading commands of `cAnalyze`: `LOAD`, `LOAD_SEQUENCE`,
//! `LOAD_ORGANISM`, `LOAD_RESOURCES`.

use std::io::Write;

use avida_config::string_util as su;
use avida_config::{Feedback, InitFile};
use avida_environment::ResourceHistory;
use avida_genome::{load_genome_detail_file, Genome, InstructionSequence};

use crate::analyze::{file_lines, Analyze};
use crate::data_entry::load_command_list;
use crate::error::{AnalyzeError, Result};
use crate::genotype::AnalyzeGenotype;
use crate::host::AnalyzeHost;
use crate::verbose::VERBOSE_ON;

/// A genome on the default instruction set (`GetDefaultInstSet()`).
fn default_genome(host: &dyn AnalyzeHost, seq: InstructionSequence) -> Result<Genome> {
    let is = host.inst_sets().default_inst_set().ok_or_else(|| {
        AnalyzeError::fatal("no instruction set loaded (C++ GetDefaultInstSet dereferences NULL)")
    })?;
    Ok(Genome::with_inst_set(
        is.hardware_type(),
        is.inst_set_name(),
        seq,
    ))
}

impl Analyze {
    /// `LoadOrganism`: one `.org`/`.gen` genome file.
    pub(crate) fn load_organism(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut filename = su::pop_word(&mut cur_string);
        writeln!(self.out, "Loading: {filename}")?;

        let mut feedback = Feedback::new();
        let genome = match load_genome_detail_file(
            &filename,
            host.working_dir(),
            host.inst_sets(),
            &mut feedback,
            "",
        ) {
            Ok(g) => g,
            Err(line) => {
                // cInstSet::GetInst prints the line and exits.
                return Err(self.exit_with(&line));
            }
        };
        self.print_feedback(&feedback)?;
        let Some(genome) = genome else {
            return Ok(());
        };
        let mut genotype = AnalyzeGenotype::new(genome);

        // The original name: strip directories and the first ".gen".
        while su::find_char(&filename, '/', 0) != -1 {
            su::pop(&mut filename, '/');
        }
        while su::find_char(&filename, '\\', 0) != -1 {
            su::pop(&mut filename, '\\');
        }
        su::replace(&mut filename, ".gen", "", 0);
        genotype.name = filename;

        let b = self.cur()?;
        b.list.push(genotype);
        b.clear_flags();
        Ok(())
    }

    /// `LoadSequence seq [name]`: a genome on the default instruction set.
    pub(crate) fn load_sequence(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let sequence = su::pop_word(&mut cur_string);
        let mut seq_name = su::pop_word(&mut cur_string);
        writeln!(self.out, "Loading: {sequence}")?;

        let genome = default_genome(host, InstructionSequence::from_string(&sequence))?;
        let mut genotype = AnalyzeGenotype::new(genome);
        genotype.num_cpus = 1;
        if seq_name.is_empty() {
            seq_name = format!("org-Seq{}", self.sequence_count);
        }
        genotype.name = seq_name;
        self.sequence_count += 1;

        let b = self.cur()?;
        b.list.push(genotype);
        b.clear_flags();
        Ok(())
    }

    /// `LoadResources [file=resource.dat] [time_spent_offset]`: replaces the
    /// resource history (even when loading fails).
    pub(crate) fn load_resources(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut resources = ResourceHistory::new();
        let words = su::count_num_words(&cur_string);
        let mut filename = "resource.dat".to_string();
        if words >= 1 {
            filename = su::pop_word(&mut cur_string);
        }
        if words >= 2 {
            self.resource_time_spent_offset = su::as_int(&su::pop_word(&mut cur_string));
        }
        writeln!(self.out, "Loading Resources from: {filename}")?;
        if !resources.load_file(&filename, host.working_dir()) {
            writeln!(self.err, "error: failed to load resource file")?;
        }
        self.resources = Some(std::sync::Arc::new(resources));
        Ok(())
    }

    /// `LoadFile`: a `genotype_data` / `population_data` file whose
    /// `#format` names data entries.
    pub(crate) fn load_file(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let filename = su::pop_word(&mut cur_string);
        writeln!(self.out, "Loading: {filename}")?;

        let input_file = InitFile::new(&filename, host.working_dir(), None, None);
        if !input_file.was_opened() {
            let fb = input_file.feedback().clone();
            self.print_feedback(&fb)?;
            let _ = self.err.flush();
            return Err(AnalyzeError::Exit(1));
        }
        let filetype = input_file.filetype().to_string();
        if filetype != "population_data" && filetype != "genotype_data" {
            return Err(
                self.exit_with(&format!("error: cannot load files of type \"{filetype}\"."))
            );
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Loading file of type: {filetype}")?;
        }

        let mut feedback = Feedback::new();
        let output_list = load_command_list(input_file.format(), Some(&mut feedback));
        self.print_feedback(&feedback)?;
        if feedback.num_errors() > 0 {
            return Ok(());
        }
        let id_inc = input_file.format().iter().any(|f| f == "id");

        let default_genome = default_genome(host, InstructionSequence::with_size(1))?;
        let mut load_count = 0;
        let mut loaded = Vec::new();
        for mut cur_line in file_lines(&input_file) {
            let mut genotype = AnalyzeGenotype::new(default_genome.clone());
            for c in &output_list {
                let word = su::pop_word(&mut cur_line);
                c.set_value(&mut genotype, &word);
            }
            genotype.name = if !id_inc {
                let n = format!("org-{load_count}");
                load_count += 1;
                n
            } else {
                format!("org-{}", genotype.id_num)
            };
            loaded.push(genotype);
        }
        let b = self.cur()?;
        b.list.extend(loaded);
        b.clear_flags();
        Ok(())
    }
}
