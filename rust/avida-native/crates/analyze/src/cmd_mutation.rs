//! Commands of `cAnalyze` (analyze/cAnalyze.cc at 47f13dadb) that test the
//! mutants, knockouts or offspring of genotypes in the test CPU:
//! `SAMPLE_OFFSPRING`, `MAP_TASKS` / `MAP`, `MAP_MUTATIONS`,
//! `ANALYZE_KNOCKOUTS`, `ANALYZE_COMPLEXITY`, `ANALYZE_FITNESS_TWO_SITES`,
//! `ANALYZE_LINEAGE_COMPLEXITY`, `ANALYZE_POP_COMPLEXITY`, `ANALYZE_MUTS`,
//! `ANALYZE_NEWINFO`, `MUTATION_REVERT` and `COMPETE`.
//!
//! Every mutant is tested the way the C++ tests it: a fresh
//! `cAnalyzeGenotype` and `Recalculate(m_ctx)`, i.e. a `cPhenPlastGenotype`
//! (a new test CPU) in the world's default context.

use std::collections::HashMap;
use std::io::Write;

use avida_apto::scheduler::{PriorityScheduler, Probabilistic};
use avida_apto::{fmt_double, AvidaRng, DataFile};
use avida_config::string_util as su;
use avida_genome::{Genome, Instruction, InstructionSequence};
use avida_hardware::test_cpu::CpuTestInfo;

use crate::analyze::Analyze;
use crate::cmd_output::{compare_flex_stat, html_print_stat_text, pop_directory, Colors};
use crate::cmd_population::static_file;
use crate::data_entry::load_command_list;
use crate::error::{AnalyzeError, Result};
use crate::genotype::AnalyzeGenotype;
use crate::host::{AnalyzeHost, TestContext};
use crate::landscape::inst_set_of;
use crate::verbose::VERBOSE_ON;

fn hw<T>(r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(AnalyzeError::world)
}

/// `cAnalyzeGenotype::Recalculate(ctx, test_info, NULL, num_trials)` of a
/// genotype in place (no parent: the parent statistics stay as they are).
pub(crate) fn recalc_in_place(
    host: &mut dyn AnalyzeHost,
    g: &mut AnalyzeGenotype,
    info: &mut CpuTestInfo,
    num_trials: i32,
) -> Result<()> {
    let r = crate::cpu::recalculate(host, &g.genome, info, num_trials)?;
    g.apply_recalc(r, None, num_trials);
    Ok(())
}

/// `cAnalyzeGenotype test_genotype(m_world, genome);
/// test_genotype.Recalculate(m_ctx)`.
pub(crate) fn recalc_new(host: &mut dyn AnalyzeHost, genome: &Genome) -> Result<AnalyzeGenotype> {
    let mut g = AnalyzeGenotype::new(genome.clone());
    recalc_in_place(host, &mut g, &mut CpuTestInfo::default(), 1)?;
    Ok(g)
}

/// `Avida::Output::File::CreateWithPath(world, path)`.
pub(crate) fn create_file(host: &mut dyn AnalyzeHost, path: &str) -> Result<DataFile> {
    host.output().create_with_path(path).map_err(|e| {
        AnalyzeError::fatal(format!(
            "{e} (C++: File::CreateWithPath returns NULL, then dereferenced)"
        ))
    })
}

pub(crate) fn write_raw(df: &mut DataFile, text: &str) -> Result<()> {
    df.ofstream().write_all(text.as_bytes())?;
    Ok(())
}

/// The ops of `genome` up to `length` (the genotype's recorded length);
/// the C++ reads past the sequence when it is shorter.
pub(crate) fn base_ops(genome: &Genome, length: i32, what: &str) -> Result<Vec<i32>> {
    let seq = genome.representation();
    if length > seq.size() {
        return Err(AnalyzeError::fatal(format!(
            "{what}: length {length} exceeds the {}-site genome (C++ reads past the sequence)",
            seq.size()
        )));
    }
    Ok((0..length.max(0)).map(|i| seq.get(i).op()).collect())
}

/// The site probabilities at mutation-selection balance for the relative
/// fitnesses `test_fitness` and mutation rate `mu`: `w_bar` steps down by
/// 1e-6 from 1 until the probabilities sum to 1 (within 0.01). The
/// denominator's `w_bar + f * mu` is fused on the reference build.
fn mut_sel_balance(test_fitness: &[f64], mu: f64) -> Result<Vec<f64>> {
    let n = test_fitness.len() as f64;
    let mut prob = vec![0.0; test_fitness.len()];
    let mut w_bar = 1.0f64;
    let mut steps: u64 = 0;
    loop {
        let mut sum = 0.0;
        for (p, &f) in prob.iter_mut().zip(test_fitness) {
            *p = (mu * w_bar) / (n * (f.mul_add(mu, w_bar) - f));
            sum += *p;
        }
        if (sum - 1.0) * (sum - 1.0) <= 0.0001 {
            return Ok(prob);
        }
        steps += 1;
        if sum.is_nan() || steps > 20_000_000 {
            return Err(AnalyzeError::fatal(
                "mutation-selection balance does not converge (the C++ loops forever)",
            ));
        }
        w_bar -= 0.000001;
    }
}

/// `sum p * log(1/p) / log(n)`.
fn site_entropy(prob: &[f64]) -> f64 {
    let ln_n = (prob.len() as f64).ln();
    let mut entropy = 0.0;
    for &p in prob {
        entropy += p * (1.0 / p).ln() / ln_n;
    }
    entropy
}

/// Per site of `genome` (ops `base`), the entropy at mutation-selection
/// balance of its `num_insts` point mutants, with mutant fitness capped at
/// `base_fitness` (the inner loops of `AnalyzeEntropy` / `IncreasedInfo`).
fn site_entropies(
    host: &mut dyn AnalyzeHost,
    genome: &Genome,
    base: &[i32],
    base_fitness: f64,
    mu: f64,
    num_insts: i32,
) -> Result<Vec<f64>> {
    let mut seq = genome.clone();
    let mut out = Vec::with_capacity(base.len());
    for (line, &cur_inst) in base.iter().enumerate() {
        let mut test_fitness = vec![0.0f64; num_insts.max(0) as usize];
        for mod_inst in 0..num_insts {
            seq.representation_mut()
                .set(line as i32, Instruction::new(mod_inst));
            let f = recalc_new(host, &seq)?.fitness;
            test_fitness[mod_inst as usize] = if f <= base_fitness { f } else { base_fitness };
        }
        let max_fitness = test_fitness
            .iter()
            .fold(0.0f64, |m, &f| if f > m { f } else { m });
        for f in test_fitness.iter_mut() {
            *f /= max_fitness;
        }
        let prob = mut_sel_balance(&test_fitness, mu)?;
        out.push(site_entropy(&prob));
        seq.representation_mut()
            .set(line as i32, Instruction::new(cur_inst));
    }
    Ok(out)
}

/// `m_world->GetRandom()`.
fn world_rng<'h>(host: &'h mut dyn AnalyzeHost, what: &str) -> Result<&'h mut AvidaRng> {
    host.world_rng().ok_or_else(|| {
        AnalyzeError::unsupported(what.to_string(), "cWorld::GetRandom (the host has no RNG)")
    })
}

impl Analyze {
    // -- SAMPLE_OFFSPRING ---------------------------------------------------

    /// `SampleOffspring`: replace every genotype of the current batch by
    /// the offspring it produces in `number` single-generation tests with
    /// the environment's mutation rates (identical offspring of one parent
    /// are merged), in reverse order of discovery.
    pub(crate) fn sample_offspring(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let pop_int = |s: &mut String, d: i32| {
            if s.is_empty() {
                d
            } else {
                su::as_int(&su::pop_word(s))
            }
        };
        let number_to_sample = pop_int(&mut cur_string, 1000);
        let use_resources = pop_int(&mut cur_string, 0);
        let update = pop_int(&mut cur_string, -1);
        let use_random_inputs =
            !cur_string.is_empty() && su::as_int(&su::pop_word(&mut cur_string)) == 1;
        let manual = self.pop_manual_inputs(host, &mut cur_string)?;
        let mut info = CpuTestInfo::new(1);
        match manual {
            Some(inputs) => info.use_manual_inputs(inputs),
            None => info.use_random_inputs(use_random_inputs),
        }
        info.set_resource_options(
            use_resources,
            self.resources.clone(),
            update,
            self.resource_time_spent_offset,
        );
        let bi = self.batch_index(self.cur_batch)?;
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Sampling {number_to_sample} offspring from each of the {} organisms in batch {}...",
                self.batches[bi].list.len(),
                self.cur_batch
            )?;
        } else {
            writeln!(self.out, "Sampling offspring...")?;
        }
        host.with_test_world(TestContext::Default, &mut |w| {
            *info.mutation_rates_mut() = *w.environment.mut_rates();
            Ok(())
        })?;
        let mut cpu = crate::cpu::create_test_cpu(host)?;
        let parents = std::mem::take(&mut self.batches[bi].list);
        let mut offspring: Vec<AnalyzeGenotype> = Vec::new();
        for parent in parents {
            let mut genome_hash: HashMap<String, usize> = HashMap::new();
            for _ in 0..number_to_sample {
                host.with_test_world(TestContext::Default, &mut |w| {
                    hw(cpu.test_genome(w, &mut info, &parent.genome)).map(|_| ())
                })?;
                let child = info
                    .test_organism(0)
                    .ok_or_else(|| AnalyzeError::fatal("SAMPLE_OFFSPRING: no test organism"))?
                    .state
                    .offspring_genome
                    .clone();
                let key = child.representation().as_string();
                match genome_hash.get(&key) {
                    Some(&i) => offspring[i].num_cpus += 1,
                    None => {
                        let mut g = AnalyzeGenotype::new(child);
                        g.id_num = parent.id_num;
                        g.num_cpus = 1;
                        genome_hash.insert(key, offspring.len());
                        offspring.push(g);
                    }
                }
            }
        }
        // `offspring_list.Push` prepends; popping it onto the batch reverses.
        offspring.reverse();
        self.batches[bi].list = offspring;
        Ok(())
    }

    // -- MAP_TASKS ----------------------------------------------------------

    /// `CommandMapTasks`: for each genotype, a table of the chosen
    /// statistics with each site knocked out (replaced by the NULL
    /// instruction), as text or HTML.
    pub(crate) fn command_map_tasks(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Constructing genotype-phenotype maps")?;
        let directory = pop_directory(&su::pop_word(&mut cur_string), "phenotype/")?;
        let mut args = su::split_list(&cur_string, ' ');
        writeln!(self.out, "Found {} args.", args.len())?;
        let pop_string = |args: &mut Vec<String>, s: &str| -> bool {
            match args.iter().position(|a| a == s) {
                Some(p) => {
                    args.remove(p);
                    true
                }
                None => false,
            }
        };
        let mut print_mode = 0;
        let mut html = false;
        let mut link_maps = false;
        let mut link_insts = false;
        let mut use_resources = 0;
        if pop_string(&mut args, "0") {
            print_mode = 0;
        }
        if pop_string(&mut args, "1") {
            print_mode = 1;
        }
        if pop_string(&mut args, "text") {
            html = false;
        }
        if pop_string(&mut args, "html") {
            html = true;
        }
        if pop_string(&mut args, "link_maps") {
            link_maps = true;
        }
        if pop_string(&mut args, "link_insts") {
            link_insts = true;
        }
        if pop_string(&mut args, "use_resources=2") {
            use_resources = 2;
        }
        let mut manual: Option<Vec<i32>> = None;
        if let Some(pos) = args.iter().position(|a| a == "use_manual_inputs") {
            pop_string(&mut args, "use_manual_inputs");
            let size = host.input_size();
            if args.len() as i64 >= pos as i64 + i64::from(size) - 1 {
                let mut inputs = vec![0; size.max(0) as usize];
                for v in inputs.iter_mut() {
                    if pos >= args.len() {
                        return Err(AnalyzeError::fatal(
                            "MAP_TASKS: cStringList::PopLine past the end (C++ NULL dereference)",
                        ));
                    }
                    *v = su::as_int(&args.remove(pos));
                }
                manual = Some(inputs);
            } else {
                return Err(
                    self.exit_with("error: CommandMapTask: Invalid use of use_manual_inputs")
                );
            }
        }
        writeln!(self.out, "There are {} column args.", args.len())?;
        let cmds = load_command_list(&args, None);
        writeln!(self.out, "Args are loaded.")?;
        let num_cols = cmds.len();
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            write!(self.out, "  outputing as ")?;
            if print_mode == 1 {
                write!(self.out, "boolean ")?;
            }
            if !html {
                writeln!(self.out, "text files.")?;
            } else {
                write!(self.out, "HTML files")?;
                if link_maps {
                    write!(self.out, "; linking files together")?;
                    write!(self.out, "; linking inst names to descs")?;
                }
                writeln!(self.out, ".")?;
            }
            write!(self.out, "  Format: ")?;
            for c in &cmds {
                write!(self.out, "{} ", c.name())?;
            }
            writeln!(self.out)?;
        }

        let colors = Colors::of(host);
        let (color_pos, color_lethal) = {
            let c = host.config();
            (c.color_mut_pos.clone(), c.color_mut_lethal.clone())
        };
        let bi = self.batch_index(self.cur_batch)?;
        let batch_name = self.batches[bi].name.clone();
        for gi in 0..self.batches[bi].list.len() {
            let name = self.batches[bi].list[gi].name.clone();
            if verbose {
                writeln!(self.out, "  Mapping {name}")?;
            }
            let ext = if html { "html" } else { "dat" };
            let filename = format!("{directory}tasksites.{name}.{ext}");
            let mut df = create_file(host, &filename)?;
            let (mut next_file, mut prev_file) = (String::new(), String::new());
            if link_maps {
                if let Some(n) = self.batches[bi].list.get(gi + 1) {
                    next_file = format!("tasksites.{}.html", n.name);
                }
                if gi > 0 {
                    prev_file = format!("tasksites.{}.html", self.batches[bi].list[gi - 1].name);
                }
            }
            let mut test_info = CpuTestInfo::default();
            if let Some(inputs) = &manual {
                test_info.use_manual_inputs(inputs.clone());
            }
            test_info.set_resource_options(use_resources, self.resources.clone(), 0, 0);
            recalc_in_place(host, &mut self.batches[bi].list[gi], &mut test_info, 1)?;

            let mut fp = String::new();
            let mut base_values = Vec::with_capacity(num_cols);
            for c in &cmds {
                base_values.push(c.value(&mut self.batches[bi].list[gi], host)?);
            }
            if !html {
                fp.push_str(&format!(
                    "-1 {batch_name} {} ",
                    self.batches[bi].list[gi].id_num
                ));
                for v in &base_values {
                    fp.push_str(&format!("{} ", v.print()));
                }
                fp.push('\n');
            } else {
                fp.push_str("<html>\n<head>\n");
                if link_insts {
                    fp.push_str("<script language=\"javascript\">\nfunction Inst(inst_name)\n{\nvar filename = \"help.\" + inst_name + \".html\";\nnewwin = window.open(filename, 'Instruction', 'toolbar=0,status=0,location=0,directories=0,menubar=0,scrollbars=1,height=150,width=300');\nnewwin.focus();\n}\n</script>\n");
                }
                fp.push_str("</head>\n");
                fp.push_str(&format!(
                    "<body>\n<div align=\"center\">\n<h1 align=\"center\">Run {batch_name}, ID {}</h1>\n\n",
                    self.batches[bi].list[gi].id_num
                ));
                fp.push_str("<table width=90%><tr><td align=left>");
                if !prev_file.is_empty() {
                    fp.push_str(&format!("<a href=\"{prev_file}\">Prev</a>"));
                } else {
                    fp.push_str("&nbsp;");
                }
                fp.push_str("<td align=right>");
                if !next_file.is_empty() {
                    fp.push_str(&format!("<a href=\"{next_file}\">Next</a>"));
                } else {
                    fp.push_str("&nbsp;");
                }
                fp.push_str("</tr></table>\n");
                fp.push_str("<table border=1 cellpadding=2>\n");
                fp.push_str("<tr><td colspan=3> ");
                for c in &cmds {
                    fp.push_str(&format!(
                        "<th>{} ",
                        c.desc(Some(&self.batches[bi].list[gi]), host)?
                    ));
                }
                fp.push_str("</tr>\n");
                fp.push_str("<tr><th colspan=3>Base Creature");
                let (hw_type, is_name) = {
                    let is = host.inst_sets().default_inst_set().ok_or_else(|| {
                        AnalyzeError::fatal("MAP_TASKS without an instruction set")
                    })?;
                    (is.hardware_type(), is.inst_set_name().to_string())
                };
                let mut null_genotype = AnalyzeGenotype::new(Genome::with_inst_set(
                    hw_type,
                    &is_name,
                    InstructionSequence::with_size(1),
                ));
                for (c, cur_value) in cmds.iter().zip(&base_values) {
                    let null_value = c.value(&mut null_genotype, host)?;
                    let compare = compare_flex_stat(cur_value, &null_value, c.compare_type());
                    let color = if compare > 0 {
                        &color_pos
                    } else {
                        &color_lethal
                    };
                    fp.push_str(&format!("<th bgcolor=\"#{color}\">"));
                    if c.has_arg("blank") {
                        fp.push_str("&nbsp; ");
                    } else {
                        fp.push_str(&format!("{} ", cur_value.print()));
                    }
                }
                fp.push_str("</tr>\n");
            }

            let g = &self.batches[bi].list[gi];
            let max_line = g.length;
            let base_genome = g.genome.clone();
            let base = base_ops(&base_genome, max_line, "MAP_TASKS")?;
            let mut mod_genome = base_genome.clone();
            let mut col_pass = vec![0i32; num_cols];
            let mut col_fail = vec![0i32; num_cols];
            let set_name = inst_set_of(host.inst_sets(), &base_genome)?
                .inst_set_name()
                .to_string();
            // The tested organism still shares the instruction set; every
            // test starts with `Clear()` anyway, so drop it before the set
            // grows (in C++ the organism sees the change in place).
            test_info.clear();
            let null_inst = host.activate_null_inst(&set_name)?;
            for (line_num, &cur_inst) in base.iter().enumerate() {
                let line = line_num as i32;
                let symbol = base_genome.representation().get(line).symbol();
                let cur_symbol = symbol.chars().next().unwrap_or('\0');
                mod_genome.representation_mut().set(line, null_inst);
                let mut test_genotype = AnalyzeGenotype::new(mod_genome.clone());
                recalc_in_place(host, &mut test_genotype, &mut test_info, 1)?;
                let inst_name = inst_set_of(host.inst_sets(), &base_genome)?
                    .name(cur_inst)
                    .to_string();
                if html {
                    fp.push_str("<tr><td align=right>");
                }
                fp.push_str(&format!("{} ", line_num + 1));
                if html {
                    fp.push_str("<td align=center>");
                }
                fp.push_str(&format!("{cur_symbol} "));
                if html {
                    fp.push_str("<td align=center>");
                }
                if link_insts {
                    fp.push_str(&format!("<a href=\"javascript:Inst('{inst_name}')\">"));
                }
                fp.push_str(&format!("{inst_name} "));
                if link_insts {
                    fp.push_str("</a>");
                }
                for (col, c) in cmds.iter().enumerate() {
                    let test_value = c.value(&mut test_genotype, host)?;
                    let base_value = c.value(&mut self.batches[bi].list[gi], host)?;
                    let compare = compare_flex_stat(&test_value, &base_value, c.compare_type());
                    if html {
                        html_print_stat_text(
                            &test_value,
                            compare,
                            c.html_cell_flags(),
                            c.null(),
                            &colors,
                            &mut fp,
                            !c.has_arg("blank"),
                        )?;
                    } else {
                        fp.push_str(&format!("{} ", test_value.print()));
                    }
                    if compare == -2 {
                        col_fail[col] += 1;
                    } else if compare == 2 {
                        col_pass[col] += 1;
                    }
                }
                if html {
                    fp.push_str("</tr>");
                }
                fp.push('\n');
                mod_genome
                    .representation_mut()
                    .set(line, Instruction::new(cur_inst));
            }
            if html {
                fp.push_str("<tr><th colspan=3>Totals");
                for i in 0..num_cols {
                    if col_pass[i] > 0 {
                        fp.push_str(&format!("<th bgcolor=\"#{color_pos}\">{}", col_pass[i]));
                    } else if col_fail[i] > 0 {
                        fp.push_str(&format!("<th bgcolor=\"#{color_lethal}\">{}", col_fail[i]));
                    } else {
                        fp.push_str("<th>0");
                    }
                }
                fp.push_str("</tr>\n</table>\n</div>\n");
            }
            write_raw(&mut df, &fp)?;
        }
        Ok(())
    }

    // -- MAP_MUTATIONS ------------------------------------------------------

    /// `CommandMapMutations`: the relative fitness of every point mutation
    /// and of the knockout of every site, per genotype, as text or HTML.
    ///
    /// QUIRK: the directory word is also scanned for `text` / `html`; the
    /// mutated-cell colours are written without their '#'; once the NULL
    /// instruction has been activated (by the first genotype's knockouts)
    /// later genotypes also test mutations to it.
    pub(crate) fn command_map_mutations(
        &mut self,
        host: &mut dyn AnalyzeHost,
        cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Constructing genome mutations maps...")?;
        let directory = pop_directory(&cur_string, "mutations/")?;
        let mut args = su::split_list(&cur_string, ' ');
        let mut html = false;
        if let Some(p) = args.iter().position(|a| a == "text") {
            args.remove(p);
            html = false;
        }
        if args.iter().any(|a| a == "html") {
            html = true;
        }
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            writeln!(
                self.out,
                "  outputing as {}",
                if html { "HTML files." } else { "text files." }
            )?;
        }
        let (c_neut, c_lethal, c_neg, c_pos) = {
            let c = host.config();
            (
                c.color_mut_neut.clone(),
                c.color_mut_lethal.clone(),
                c.color_mut_neg.clone(),
                c.color_mut_pos.clone(),
            )
        };
        let bi = self.batch_index(self.cur_batch)?;
        let batch_name = self.batches[bi].name.clone();
        for gi in 0..self.batches[bi].list.len() {
            let name = self.batches[bi].list[gi].name.clone();
            if verbose {
                writeln!(self.out, "  Creating mutation map for {name}")?;
            }
            let filename = format!(
                "{directory}mut_map.{name}.{}",
                if html { "html" } else { "dat" }
            );
            if verbose {
                writeln!(self.out, "  Using filename \"{filename}\"")?;
            }
            static_file(host, &filename)?;
            recalc_in_place(
                host,
                &mut self.batches[bi].list[gi],
                &mut CpuTestInfo::default(),
                1,
            )?;
            let g = &self.batches[bi].list[gi];
            let base_fitness = g.fitness;
            let max_line = g.length;
            let id = g.id_num;
            let base_genome = g.genome.clone();
            let base = base_ops(&base_genome, max_line, "MAP_MUTATIONS")?;
            let mut seq = base_genome.clone();
            let (set_name, names) = {
                let is = inst_set_of(host.inst_sets(), &base_genome)?;
                (
                    is.inst_set_name().to_string(),
                    (0..is.size())
                        .map(|i| is.name(i).to_string())
                        .collect::<Vec<_>>(),
                )
            };
            let num_insts = names.len() as i32;
            let mut fp = String::new();
            if !html {
                fp.push_str("# 1: Genome instruction ID (pre-mutation)\n");
                for (i, n) in names.iter().enumerate() {
                    fp.push_str(&format!("# {}: Fit if mutated to '{n}'\n", i + 1));
                }
                for (k, label) in [
                    "Knockout",
                    "Fraction Lethal",
                    "Fraction Detremental",
                    "Fraction Neutral",
                    "Fraction Beneficial",
                    "Average Fitness",
                    "Expected Entropy",
                    "Original Instruction Name",
                ]
                .iter()
                .enumerate()
                {
                    fp.push_str(&format!("# {}: {label}\n", num_insts + 2 + k as i32));
                }
                fp.push('\n');
            } else {
                fp.push_str(&format!(
                    "<html>\n<body bgcolor=\"#FFFFFF\"\n text=\"#000000\"\n link=\"#0000AA\"\n alink=\"#0000FF\"\n vlink=\"#000044\">\n\n<h1 align=center>Mutation Map for Run {batch_name}, ID {id}</h1>\n<center>\n\n"
                ));
                fp.push_str("<table border=1 cellpadding=2>\n<tr><th>Genome ");
                for n in &names {
                    fp.push_str(&format!("<th>{n} "));
                }
                fp.push_str("<th>Knockout <th>Frac. Lethal <th>Frac. Detremental <th>Frac. Neutral <th>Frac. Beneficial <th>Ave. Fitness <th>Expected Entropy </tr>\n\n");
            }

            let (mut total_dead, mut total_neg, mut total_neut, mut total_pos) = (0, 0, 0, 0);
            let mut total_fitness = 0.0f64;
            let mut col_fitness = vec![0.0f64; names.len() + 1];
            let null_inst = host.activate_null_inst(&set_name)?;
            let category = |f: f64| -> &str {
                if f == 1.0 {
                    &c_neut
                } else if f == 0.0 {
                    &c_lethal
                } else if f < 1.0 {
                    &c_neg
                } else {
                    &c_pos
                }
            };
            for (line_num, &cur_inst) in base.iter().enumerate() {
                let line = line_num as i32;
                let cur_symbol = base_genome
                    .representation()
                    .get(line)
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or('\0');
                let (mut row_dead, mut row_neg, mut row_neut, mut row_pos) = (0, 0, 0, 0);
                let mut row_fitness = 0.0f64;
                if html {
                    fp.push_str(&format!(
                        "<tr><td align=right>{} ({cur_symbol}) ",
                        names[cur_inst as usize]
                    ));
                } else {
                    fp.push_str(&format!("{cur_inst} "));
                }
                for mod_inst in 0..num_insts {
                    if mod_inst == cur_inst {
                        if html {
                            fp.push_str("<th bgcolor=\"#FFFFFF\">");
                        }
                        continue;
                    }
                    seq.representation_mut()
                        .set(line, Instruction::new(mod_inst));
                    let test_fitness = recalc_new(host, &seq)?.fitness / base_fitness;
                    row_fitness += test_fitness;
                    total_fitness += test_fitness;
                    col_fitness[mod_inst as usize] += test_fitness;
                    if test_fitness == 1.0 {
                        row_neut += 1;
                        total_neut += 1;
                    } else if test_fitness == 0.0 {
                        row_dead += 1;
                        total_dead += 1;
                    } else if test_fitness < 1.0 {
                        row_neg += 1;
                        total_neg += 1;
                    } else {
                        row_pos += 1;
                        total_pos += 1;
                    }
                    if html {
                        fp.push_str(&format!("<th bgcolor=\"{}\">", category(test_fitness)));
                    }
                    fp.push_str(&format!("{} ", fmt_double(test_fitness)));
                }
                seq.representation_mut().set(line, null_inst);
                let test_fitness = recalc_new(host, &seq)?.fitness / base_fitness;
                col_fitness[names.len()] += test_fitness;
                if html {
                    fp.push_str(&format!("<th bgcolor=\"{}\">", category(test_fitness)));
                }
                fp.push_str(&format!("{} ", fmt_double(test_fitness)));
                let denom = f64::from(num_insts - 1);
                for (count, color) in [
                    (row_dead, &c_lethal),
                    (row_neg, &c_neg),
                    (row_neut, &c_neut),
                    (row_pos, &c_pos),
                ] {
                    if html {
                        fp.push_str(&format!("<th bgcolor=\"#{color}\">"));
                    }
                    fp.push_str(&format!("{} ", fmt_double(f64::from(count) / denom)));
                }
                if html {
                    fp.push_str("<th>");
                }
                fp.push_str(&format!("{} ", fmt_double(row_fitness / denom)));
                if html {
                    fp.push_str("<th>");
                }
                fp.push_str("0 ");
                if html {
                    fp.push_str("</tr>");
                }
                fp.push('\n');
                seq.representation_mut()
                    .set(line, Instruction::new(cur_inst));
            }
            if html {
                fp.push_str("<tr><th>Totals");
                for c in &col_fitness {
                    fp.push_str(&format!("<th>{} ", fmt_double(c / f64::from(max_line))));
                }
                let total_tests = f64::from(max_line.wrapping_mul(num_insts - 1));
                for v in [total_dead, total_neg, total_neut, total_pos] {
                    fp.push_str(&format!("<th>{} ", fmt_double(f64::from(v) / total_tests)));
                }
                fp.push_str(&format!("<th>{} ", fmt_double(total_fitness / total_tests)));
                fp.push_str("<th>0 </table>\n</center>\n");
            }
            let df = static_file(host, &filename)?;
            write_raw(df, &fp)?;
            df.flush();
        }
        Ok(())
    }

    // -- ANALYZE_KNOCKOUTS --------------------------------------------------

    /// `AnalyzeKnockouts`: counts of lethal / detrimental / neutral /
    /// beneficial single knockouts, and (with `max_knockouts > 1`) the
    /// counts after the pairwise knockout corrections.
    pub(crate) fn analyze_knockouts(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Analyzing the effects of knockouts...")?;
        let mut filename = "knockouts.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let mut max_knockouts = 1;
        if !cur_string.is_empty() {
            max_knockouts = su::as_int(&su::pop_word(&mut cur_string));
        }
        {
            let df = static_file(host, &filename)?;
            df.write_comment("Analysis of knockouts in genomes");
            df.write_time_stamp();
        }
        let bi = self.batch_index(self.cur_batch)?;
        for gi in 0..self.batches[bi].list.len() {
            if Self::verbosity(host) >= VERBOSE_ON {
                writeln!(self.out, "  Knockout: {}", self.batches[bi].list[gi].name)?;
            }
            recalc_in_place(
                host,
                &mut self.batches[bi].list[gi],
                &mut CpuTestInfo::default(),
                1,
            )?;
            let g = &self.batches[bi].list[gi];
            let base_fitness = g.fitness;
            let id = g.id_num;
            let base_genome = g.genome.clone();
            let base = base_ops(&base_genome, g.length, "ANALYZE_KNOCKOUTS")?;
            let set_name = inst_set_of(host.inst_sets(), &base_genome)?
                .inst_set_name()
                .to_string();
            let null_inst = host.activate_null_inst(&set_name)?;
            let mut mod_seq = base_genome.clone();
            let (mut dead, mut neg, mut neut, mut pos) = (0, 0, 0, 0);
            let mut ko_effect = vec![0i32; base.len()];
            for (line, &cur_inst) in base.iter().enumerate() {
                mod_seq.representation_mut().set(line as i32, null_inst);
                let ko_fitness = recalc_new(host, &mod_seq)?.fitness;
                if ko_fitness == 0.0 {
                    dead += 1;
                    ko_effect[line] = -2;
                } else if ko_fitness < base_fitness {
                    neg += 1;
                    ko_effect[line] = -1;
                } else if ko_fitness == base_fitness {
                    neut += 1;
                    ko_effect[line] = 0;
                } else if ko_fitness > base_fitness {
                    pos += 1;
                    ko_effect[line] = 1;
                } else {
                    writeln!(self.err, "ERROR: illegal state in AnalyzeKnockouts()")?;
                }
                mod_seq
                    .representation_mut()
                    .set(line as i32, Instruction::new(cur_inst));
            }
            let mut ko_pair_effect = ko_effect.clone();
            if max_knockouts > 1 {
                for line1 in 0..base.len() {
                    for line2 in line1 + 1..base.len() {
                        mod_seq.representation_mut().set(line1 as i32, null_inst);
                        mod_seq.representation_mut().set(line2 as i32, null_inst);
                        let ko_fitness = recalc_new(host, &mod_seq)?.fitness;
                        if ko_fitness >= base_fitness
                            && ko_effect[line1] < 0
                            && ko_effect[line2] < 0
                        {
                            ko_pair_effect[line1] = 0;
                            ko_pair_effect[line2] = 0;
                        }
                        if ko_fitness < base_fitness
                            && ko_effect[line1] >= 0
                            && ko_effect[line2] >= 0
                        {
                            ko_pair_effect[line1] = -1;
                            ko_pair_effect[line2] = -1;
                        }
                        mod_seq
                            .representation_mut()
                            .set(line1 as i32, Instruction::new(base[line1]));
                        mod_seq
                            .representation_mut()
                            .set(line2 as i32, Instruction::new(base[line2]));
                    }
                }
            }
            let count = |v: i32| ko_pair_effect.iter().filter(|&&e| e == v).count() as i32;
            let df = static_file(host, &filename)?;
            df.write(id, "Genotype ID");
            df.write(dead, "Count of lethal knockouts");
            df.write(neg, "Count of detrimental knockouts");
            df.write(neut, "Count of neutral knockouts");
            df.write(pos, "Count of beneficial knockouts");
            df.write(
                count(-2),
                "Count of lethal knockouts after paired knockout tests.",
            );
            df.write(
                count(-1),
                "Count of detrimental knockouts after paired knockout tests.",
            );
            df.write(
                count(0),
                "Count of neutral knockouts after paired knockout tests.",
            );
            df.write(
                count(1),
                "Count of beneficial knockouts after paired knockout tests.",
            );
            df.endl();
        }
        Ok(())
    }

    // -- ANALYZE_COMPLEXITY -------------------------------------------------

    /// `AnalyzeComplexity mut_rate [dir [use_resources [batch_frequency]]]`:
    /// per site, the instruction probabilities at mutation-selection
    /// balance and the site's complexity.
    ///
    /// QUIRK: a site whose mutants are all dead prints an error line and
    /// is left mutated to the last instruction for the following sites.
    pub(crate) fn analyze_complexity(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Analyzing genome complexity...")?;
        let words = su::count_num_words(&cur_string);
        if words < 1 {
            writeln!(
                self.out,
                "Error: AnalyzeComplexity has no parameters, skipping."
            )?;
            return Ok(());
        }
        let mut_rate = su::as_double(&su::pop_word(&mut cur_string));
        let directory = pop_directory(&su::pop_word(&mut cur_string), "complexity/")?;
        let mut use_resources = 0;
        if words >= 3 {
            use_resources = su::as_int(&su::pop_word(&mut cur_string));
        }
        let mut batch_frequency = 1;
        if words == 4 {
            batch_frequency = su::as_int(&su::pop_word(&mut cur_string));
            if batch_frequency <= 0 {
                batch_frequency = 1;
            }
        }
        crate::cpu::create_test_cpu(host)?;
        let bi = self.batch_index(self.cur_batch)?;
        let lineage_filename = format!(
            "{directory}{}.complexity.dat",
            if self.batches[bi].is_lineage {
                "lineage"
            } else {
                "nonlineage"
            }
        );
        let mut lineage_df = create_file(host, &lineage_filename)?;
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        let mut gi = 0;
        while gi < self.batches[bi].list.len() {
            let name = self.batches[bi].list[gi].name.clone();
            if verbose {
                writeln!(self.out, "  Analyzing complexity for {name}")?;
            }
            let mut df = create_file(host, &format!("{directory}{name}.complexity.dat"))?;
            let mut lineage_line = format!("{} ", self.batches[bi].list[gi].id_num);
            let mut info = CpuTestInfo::default();
            info.set_resource_options(
                use_resources,
                self.resources.clone(),
                self.batches[bi].list[gi].update_born,
                self.resource_time_spent_offset,
            );
            recalc_in_place(host, &mut self.batches[bi].list[gi], &mut info, 1)?;
            let g = &self.batches[bi].list[gi];
            writeln!(self.out, "{}", fmt_double(g.fitness))?;
            let base_genome = g.genome.clone();
            let base = base_ops(&base_genome, g.length, "ANALYZE_COMPLEXITY")?;
            let mut seq = base_genome.clone();
            let num_insts = inst_set_of(host.inst_sets(), &base_genome)?.size();
            let mut fp = String::new();
            for (line_num, &cur_inst) in base.iter().enumerate() {
                let line = line_num as i32;
                fp.push_str(&format!("{cur_inst} "));
                let mut test_fitness = vec![0.0f64; num_insts as usize];
                for mod_inst in 0..num_insts {
                    seq.representation_mut()
                        .set(line, Instruction::new(mod_inst));
                    test_fitness[mod_inst as usize] = recalc_new(host, &seq)?.fitness;
                }
                let cur_inst_fitness = test_fitness[cur_inst as usize];
                for f in test_fitness.iter_mut() {
                    if *f > cur_inst_fitness {
                        *f = cur_inst_fitness;
                    }
                    *f /= cur_inst_fitness;
                }
                let max_fitness = test_fitness
                    .iter()
                    .fold(0.0f64, |m, &f| if f > m { f } else { m });
                if max_fitness > 0.0 {
                    for f in test_fitness.iter_mut() {
                        *f /= max_fitness;
                    }
                } else {
                    fp.push_str("All zero fitness, ERROR.\n");
                    continue;
                }
                let prob = mut_sel_balance(&test_fitness, mut_rate)?;
                for p in &prob {
                    fp.push_str(&format!("{} ", fmt_double(*p)));
                }
                let complexity = 1.0 - site_entropy(&prob);
                fp.push_str(&format!("{}\n", fmt_double(complexity)));
                lineage_line.push_str(&format!("{} ", fmt_double(complexity)));
                seq.representation_mut()
                    .set(line, Instruction::new(cur_inst));
            }
            write_raw(&mut df, &fp)?;
            drop(df);
            lineage_line.push('\n');
            write_raw(&mut lineage_df, &lineage_line)?;
            let mut stop = false;
            for _ in 0..batch_frequency - 1 {
                gi += 1;
                match self.batches[bi].list.get(gi) {
                    Some(s) => {
                        if verbose {
                            writeln!(self.out, "Skipping: {}", s.name)?;
                        }
                    }
                    None => {
                        stop = true;
                        break;
                    }
                }
            }
            if stop {
                break;
            }
            gi += 1;
        }
        Ok(())
    }

    // -- ANALYZE_FITNESS_TWO_SITES ------------------------------------------

    /// `AnalyzeFitnessLandscapeTwoSites [dir [use_resources
    /// [batch_frequency]]]`: one file per site pair with the fitness of
    /// every instruction combination at the two sites.
    ///
    /// QUIRK: the batch frequency is read and reported but not used.
    pub(crate) fn analyze_fitness_two_sites(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(
            self.out,
            "Fitness for all instruction combinations at two sites..."
        )?;
        let words = su::count_num_words(&cur_string);
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            writeln!(self.out, "  Number of arguments passed: {words}")?;
        }
        let directory = pop_directory(
            &su::pop_word(&mut cur_string),
            "fitness_landscape_two_sites/",
        )?;
        if verbose {
            writeln!(self.out, "  - Analysis results to directory: {directory}")?;
        }
        let mut use_resources = 0;
        if words >= 2 {
            use_resources = su::as_int(&su::pop_word(&mut cur_string));
        }
        if verbose {
            writeln!(
                self.out,
                "  - Use resorces set to: {use_resources} (0=false, true other int)"
            )?;
        }
        let mut batch_frequency = 1;
        if words >= 3 {
            batch_frequency = su::as_int(&su::pop_word(&mut cur_string));
            if batch_frequency <= 0 {
                batch_frequency = 1;
            }
        }
        if verbose {
            writeln!(self.out, "  - Batch frequency set to: {batch_frequency}")?;
        }
        let bi = self.batch_index(self.cur_batch)?;
        for gi in 0..self.batches[bi].list.len() {
            let name = self.batches[bi].list[gi].name.clone();
            if verbose {
                writeln!(self.out, "  Analyzing complexity for {name}")?;
            }
            let mut info = CpuTestInfo::default();
            info.set_resource_options(
                use_resources,
                self.resources.clone(),
                self.batches[bi].list[gi].update_born,
                self.resource_time_spent_offset,
            );
            recalc_in_place(host, &mut self.batches[bi].list[gi], &mut info, 1)?;
            let g = &self.batches[bi].list[gi];
            let base_genome = g.genome.clone();
            let base = base_ops(&base_genome, g.length, "ANALYZE_FITNESS_TWO_SITES")?;
            let mut seq = base_genome.clone();
            let num_insts = inst_set_of(host.inst_sets(), &base_genome)?.size();
            for site1 in 0..base.len() {
                for site2 in site1 + 1..base.len() {
                    let mut df = create_file(
                        host,
                        &format!("{directory}{name}_FitLand_sites-{site1}_and_{site2}.dat"),
                    )?;
                    df.write_comment("Two-site fitness landscape, all possible instructions");
                    df.write_comment(&format!("Site 1: {site1} Site 2: {site2}"));
                    df.write_comment("Rows #- instruction, site 1");
                    df.write_comment("Columns #- instruction, site 2");
                    df.write_time_stamp();
                    for m1 in 0..num_insts {
                        for m2 in 0..num_insts {
                            seq.representation_mut()
                                .set(site1 as i32, Instruction::new(m1));
                            seq.representation_mut()
                                .set(site2 as i32, Instruction::new(m2));
                            let f = recalc_new(host, &seq)?.fitness;
                            df.write(f, &format!("Instruction, site 2: {m2} "));
                        }
                        df.endl();
                    }
                    seq.representation_mut()
                        .set(site1 as i32, Instruction::new(base[site1]));
                    seq.representation_mut()
                        .set(site2 as i32, Instruction::new(base[site2]));
                }
            }
        }
        Ok(())
    }

    // -- ANALYZE_LINEAGE_COMPLEXITY -----------------------------------------

    /// `AnalyzeLineageComplexitySitesN [n [dir]]`: counts of neutral or
    /// beneficial one- or two-site mutants of each genotype (against its
    /// recorded fitness) and the complexity they imply.
    ///
    /// QUIRK: the output always goes to `complexity.dat` (the directory is
    /// only reported); the recorded length and fitness are used without a
    /// recalculation; for `n >= 3` nothing is tested and the denominator is 0.
    pub(crate) fn analyze_lineage_complexity(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(
            self.out,
            "Analyzing genome complexity of a lineage for n sites..."
        )?;
        let words = su::count_num_words(&cur_string);
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        if verbose {
            writeln!(self.out, "  Number of arguments passed: {words}")?;
        }
        let mut n = 2;
        if words < 1 {
            if verbose {
                writeln!(
                    self.out,
                    "  - No specific n-mutant selected, using default n-mutant with n = {n}"
                )?;
            }
        } else {
            n = su::as_int(&su::pop_word(&mut cur_string));
            if n < 1 {
                n = 1;
            }
            if verbose {
                writeln!(self.out, "  - n-mutant passed, using n = {n}")?;
            }
        }
        let directory = pop_directory(
            &su::pop_word(&mut cur_string),
            "complexity_nmutant_lineage/",
        )?;
        if verbose {
            writeln!(self.out, "  - Analysis results to directory: {directory}")?;
        }
        crate::cpu::create_test_cpu(host)?;
        let mut df = create_file(host, "complexity.dat")?;
        df.write_comment("Lineage Complexity Analysis");
        df.write_time_stamp();
        let bi = self.batch_index(self.cur_batch)?;
        for gi in 0..self.batches[bi].list.len() {
            let g = &self.batches[bi].list[gi];
            if verbose {
                writeln!(self.out, "  Analyzing complexity for {}", g.name)?;
            }
            let gen_length = g.length;
            let gen_fitness = g.fitness;
            let id = g.id_num;
            let base_genome = g.genome.clone();
            let mut seq = base_genome.clone();
            let num_insts = inst_set_of(host.inst_sets(), &base_genome)?.size();
            let mut posneutmut: i32 = 0;
            let mut posmut: i32 = 0;
            writeln!(
                self.out,
                "The base genome fitness is: {}",
                fmt_double(gen_fitness)
            )?;
            if n == 1 {
                let base = base_ops(&base_genome, gen_length, "ANALYZE_LINEAGE_COMPLEXITY")?;
                for (gene, &cur_inst) in base.iter().enumerate() {
                    for mod_inst in 0..num_insts {
                        if mod_inst == cur_inst {
                            continue;
                        }
                        seq.representation_mut()
                            .set(gene as i32, Instruction::new(mod_inst));
                        let f = recalc_new(host, &seq)?.fitness;
                        writeln!(self.out, "Mod Fitness: {}", fmt_double(f))?;
                        if f >= gen_fitness {
                            posneutmut += 1;
                        }
                        if f > gen_fitness {
                            posmut += 1;
                        }
                    }
                    seq.representation_mut()
                        .set(gene as i32, Instruction::new(cur_inst));
                }
            }
            if n == 2 {
                let base = base_ops(&base_genome, gen_length, "ANALYZE_LINEAGE_COMPLEXITY")?;
                let len = base.len();
                for g1 in 0..len.saturating_sub(1) {
                    for g2 in g1 + 1..len {
                        for m1 in 0..num_insts {
                            for m2 in 0..num_insts {
                                seq.representation_mut()
                                    .set(g1 as i32, Instruction::new(m1));
                                seq.representation_mut()
                                    .set(g2 as i32, Instruction::new(m2));
                                let f = recalc_new(host, &seq)?.fitness;
                                if f >= gen_fitness {
                                    posneutmut += 1;
                                }
                                if f > gen_fitness {
                                    posmut += 1;
                                }
                            }
                        }
                        seq.representation_mut()
                            .set(g1 as i32, Instruction::new(base[g1]));
                        seq.representation_mut()
                            .set(g2 as i32, Instruction::new(base[g2]));
                    }
                }
            }
            let ni = f64::from(num_insts);
            let len = f64::from(gen_length);
            let denominator = if n == 1 {
                f64::from(num_insts.wrapping_mul(gen_length))
            } else if n == 2 {
                ni.powf(2.0) * len * f64::from(gen_length - 1) * 0.5
            } else {
                0.0
            };
            let wn = f64::from(posneutmut) / denominator;
            let totalcombo = ni.powf(len);
            let entropy = if posneutmut > 0 {
                (wn * totalcombo).ln() / ni.ln()
            } else {
                0.0
            };
            let complexity = len - entropy;
            writeln!(self.out, "Complexity: {}", fmt_double(complexity))?;
            df.write(id, "Genotype ID");
            df.write(gen_fitness, "Genotype Fitness");
            df.write(gen_length, "Genotype Length");
            df.write(posmut, "Positive Mutations");
            df.write(posneutmut, "Positive and Neutral Mutations");
            df.write(entropy, "Entropy");
            df.write(complexity, "Complexity");
            df.endl();
        }
        Ok(())
    }

    // -- ANALYZE_POP_COMPLEXITY ---------------------------------------------

    /// `AnalyzePopComplexity [dir]`: the per-site complexity of the
    /// instruction distribution in the current batch (organism-weighted).
    ///
    /// QUIRK: the file name embeds the whole argument string
    /// (`<dir>pop<args>.complexity.dat`); the per-site probabilities are
    /// single-precision `float`s; every genotype is read over the first
    /// genotype's length, and a shorter genome counts instruction 0 at the
    /// sites past its end (the C++ reads past the sequence; on the
    /// reference build those reads give 0, checked against its output).
    pub(crate) fn analyze_pop_complexity(
        &mut self,
        host: &mut dyn AnalyzeHost,
        cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Analyzing population complexity ...")?;
        let directory = pop_directory(&cur_string, "pop_complexity/")?;
        let filename = format!("{directory}pop{cur_string}.complexity.dat");
        let mut df = create_file(host, &filename)?;
        let bi = self.batch_index(self.cur_batch)?;
        let Some(first) = self.batches[bi].list.first() else {
            return Ok(());
        };
        let seq_length = first.length;
        let num_insts = inst_set_of(host.inst_sets(), &first.genome)?.size();
        let mut inst_stat = vec![0i32; (seq_length.max(0) * num_insts.max(0)) as usize];
        let mut samples: i32 = 0;
        for g in &self.batches[bi].list {
            let seq = g.genome.representation();
            let ops: Vec<i32> = (0..seq_length.max(0))
                .map(|i| if i < seq.size() { seq.get(i).op() } else { 0 })
                .collect();
            for _ in 0..g.num_cpus {
                for (line, &op) in ops.iter().enumerate() {
                    if op >= num_insts {
                        return Err(AnalyzeError::fatal(
                            "ANALYZE_POP_COMPLEXITY: instruction outside the set (C++ tMatrix overrun)",
                        ));
                    }
                    inst_stat[line * num_insts as usize + op as usize] += 1;
                }
                samples += 1;
            }
        }
        let mut fp = String::new();
        for line in 0..seq_length.max(0) as usize {
            let mut entropy = 0.0f64;
            for inst in 0..num_insts as usize {
                let c = inst_stat[line * num_insts as usize + inst];
                if c == 0 {
                    continue;
                }
                let prob = f64::from(c as f32 / samples as f32);
                entropy += prob * (1.0 / prob).ln() / f64::from(num_insts).ln();
            }
            fp.push_str(&format!("{} ", fmt_double(1.0 - entropy)));
        }
        fp.push('\n');
        write_raw(&mut df, &fp)
    }

    // -- ANALYZE_MUTS -------------------------------------------------------

    /// `AnalyzeMuts [file [all_combos]]`: along an aligned batch, the
    /// fitness of every combination of the mutations between the first and
    /// last sequence (up to 30 of them), grouped by number of mutations.
    pub(crate) fn analyze_muts(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Analyzing Mutations")?;
        let bi = self.batch_index(self.cur_batch)?;
        if !self.batches[bi].is_aligned {
            writeln!(self.out, "  Error: sequences not aligned.")?;
            return Ok(());
        }
        let mut filename = "analyze_muts.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let sequences: Vec<Vec<u8>> = self.batches[bi]
            .list
            .iter()
            .map(|g| g.aligned_sequence.as_bytes().to_vec())
            .collect();
        let Some(first_seq) = sequences.first().cloned() else {
            return Err(AnalyzeError::fatal(
                "ANALYZE_MUTS of an empty batch (C++ NULL dereference)",
            ));
        };
        let last_seq = sequences[sequences.len() - 1].clone();
        let sequence_length = first_seq.len();
        let mut mut_count = vec![0i32; sequence_length];
        for w in sequences.windows(2) {
            for (j, c) in mut_count.iter_mut().enumerate() {
                let (a, b) = (w[0].get(j), w[1].get(j));
                if a.is_none() || b.is_none() {
                    return Err(AnalyzeError::fatal(
                        "ANALYZE_MUTS: aligned sequences of different lengths (C++ reads past a string)",
                    ));
                }
                if a != b {
                    *c += 1;
                }
            }
        }
        let text = |s: &[u8]| String::from_utf8_lossy(s).into_owned();
        let mut fp = format!("# {}\n# {}\n# ", text(&first_seq), text(&last_seq));
        for &c in &mut_count {
            if c == 0 {
                fp.push(' ');
            } else if c > 9 {
                fp.push('+');
            } else {
                fp.push_str(&c.to_string());
            }
        }
        fp.push_str("\n# ");
        for (i, &c) in first_seq.iter().enumerate() {
            let same = last_seq.get(i) == Some(&c);
            fp.push(if same { ' ' } else { '^' });
        }
        fp.push_str("\n\n");
        let total_diffs = su::distance(&text(&first_seq), &text(&last_seq), 0);
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "  {total_diffs} mutations being tested.")?;
        }
        let mut mut_positions = Vec::new();
        for (i, c) in first_seq.iter().enumerate() {
            if last_seq.get(i) != Some(c) {
                mut_positions.push(i);
            }
        }
        if total_diffs > 30 {
            let df = static_file(host, &filename)?;
            write_raw(df, &fp)?;
            df.flush();
            writeln!(
                self.err,
                "  Warning: too many mutations ({total_diffs}) to scan through combos."
            )?;
            return Ok(());
        }
        if mut_positions.len() as i32 > total_diffs {
            return Err(AnalyzeError::fatal(
                "ANALYZE_MUTS: more differing sites than the distance (C++ heap overflow)",
            ));
        }
        let total_combos: i32 = 1 << total_diffs;
        writeln!(self.out, "  Scanning through {total_combos} combos.")?;
        let n = total_diffs as usize + 1;
        let mut total_fitness = vec![0.0f64; n];
        let mut total_sqr_fitness = vec![0.0f64; n];
        let mut max_fitness = vec![0.0f64; n];
        let mut max_sequence = vec![String::new(); n];
        let mut test_count = vec![0i32; n];
        let mut cpu = crate::cpu::create_test_cpu(host)?;
        let (hw_type, is_name) = {
            let is = host
                .inst_sets()
                .default_inst_set()
                .ok_or_else(|| AnalyzeError::fatal("ANALYZE_MUTS without an instruction set"))?;
            (is.hardware_type(), is.inst_set_name().to_string())
        };
        let combo_step = total_combos / 79;
        for combo_id in 0..total_combos {
            // QUIRK: with fewer than 79 combinations the step is 0; on the
            // reference (arm64) `x % 0` is `x`, so only combination 0 prints.
            let tick = if combo_step == 0 {
                combo_id == 0
            } else {
                combo_id % combo_step == 0
            };
            if tick {
                write!(self.out, ".")?;
            }
            let mut test_sequence = first_seq.clone();
            let mut diff_count = 0usize;
            for (mut_id, &pos) in mut_positions.iter().enumerate() {
                if (combo_id >> mut_id) & 1 == 1 {
                    test_sequence[pos] = last_seq[pos];
                    diff_count += 1;
                }
            }
            let test_text = text(&test_sequence);
            let genome = Genome::with_inst_set(
                hw_type,
                &is_name,
                InstructionSequence::from_string(&test_text),
            );
            let mut info = CpuTestInfo::default();
            host.with_test_world(TestContext::Default, &mut |w| {
                hw(cpu.test_genome(w, &mut info, &genome)).map(|_| ())
            })?;
            let fitness = info.genotype_fitness();
            total_fitness[diff_count] += fitness;
            total_sqr_fitness[diff_count] = fitness.mul_add(fitness, total_sqr_fitness[diff_count]);
            if fitness > max_fitness[diff_count] {
                max_fitness[diff_count] = fitness;
                max_sequence[diff_count] = test_text;
            }
            test_count[diff_count] += 1;
        }
        for i in 0..n {
            let genome = Genome::with_inst_set(
                hw_type,
                &is_name,
                InstructionSequence::from_string(&max_sequence[i]),
            );
            let max_genotype = recalc_new(host, &genome)?;
            fp.push_str(&format!(
                "{i} {} {} {} {} {} {} {} {} ",
                test_count[i],
                fmt_double(total_fitness[i] / f64::from(test_count[i])),
                fmt_double(max_fitness[i]),
                fmt_double(max_genotype.merit),
                max_genotype.gest_time,
                max_genotype.length,
                max_genotype.copy_length,
                max_genotype.exe_length
            ));
            // PrintTasks(fp, 3, 12). QUIRK: tasks 3..11 whatever the
            // environment's size; the C++ reads past a shorter task array,
            // which gives 0 on the reference build (checked against its
            // output for 9 tasks and for the empty genotype's empty array).
            for t in 3..12 {
                let c = max_genotype.task_counts.get(t).copied().unwrap_or(0);
                fp.push_str(&format!("{c} "));
            }
            fp.push_str(&max_sequence[i]);
            fp.push('\n');
        }
        let df = static_file(host, &filename)?;
        write_raw(df, &fp)?;
        df.flush();
        Ok(())
    }

    // -- ANALYZE_NEWINFO ----------------------------------------------------

    /// `AnalyzeEntropy(genotype, mu)`: the summed per-site entropy at
    /// mutation-selection balance (recalculating the genotype first).
    fn analyze_entropy(
        &mut self,
        host: &mut dyn AnalyzeHost,
        bi: usize,
        gi: usize,
        mu: f64,
    ) -> Result<f64> {
        let g = &mut self.batches[bi].list[gi];
        recalc_in_place(host, g, &mut CpuTestInfo::default(), 1)?;
        if g.fitness == 0.0 {
            return Ok(f64::from(g.length));
        }
        let base_fitness = g.fitness;
        let base_genome = g.genome.clone();
        let size = base_genome.representation().size();
        let base = base_ops(&base_genome, size, "AnalyzeEntropy")?;
        let num_insts = inst_set_of(host.inst_sets(), &base_genome)?.size();
        let mut entropy = 0.0;
        for e in site_entropies(host, &base_genome, &base, base_fitness, mu, num_insts)? {
            entropy += e;
        }
        Ok(entropy)
    }

    /// `IncreasedInfo(genotype1, genotype2, mu)`: the information per site
    /// that genotype 1 has and genotype 2 lacks.
    fn increased_info(
        &mut self,
        host: &mut dyn AnalyzeHost,
        bi: usize,
        g1: usize,
        g2: usize,
        mu: f64,
    ) -> Result<f64> {
        if self.batches[bi].list[g1].length != self.batches[bi].list[g2].length {
            return Err(self.exit_with(
                "Error: Two genotypes don't have same length.(cAnalyze::IncreasedInfo)",
            ));
        }
        recalc_in_place(
            host,
            &mut self.batches[bi].list[g1],
            &mut CpuTestInfo::default(),
            1,
        )?;
        let a = &self.batches[bi].list[g1];
        if a.fitness == 0.0 {
            return Ok(0.0);
        }
        let (genome1, fitness1) = (a.genome.clone(), a.fitness);
        let num_lines = genome1.representation().size();
        let base1 = base_ops(&genome1, num_lines, "IncreasedInfo")?;
        // Both genomes are mutated over genotype 1's instruction set size.
        let num_insts = inst_set_of(host.inst_sets(), &genome1)?.size();
        let info1: Vec<f64> = site_entropies(host, &genome1, &base1, fitness1, mu, num_insts)?
            .into_iter()
            .map(|e| 1.0 - e)
            .collect();
        let mut increased = 0.0;
        recalc_in_place(
            host,
            &mut self.batches[bi].list[g2],
            &mut CpuTestInfo::default(),
            1,
        )?;
        let b = &self.batches[bi].list[g2];
        if b.fitness == 0.0 {
            for i in &info1 {
                increased += i;
            }
            return Ok(increased);
        }
        let (genome2, fitness2) = (b.genome.clone(), b.fitness);
        let base2 = base_ops(&genome2, num_lines, "IncreasedInfo")?;
        let entropies2 = site_entropies(host, &genome2, &base2, fitness2, mu, num_insts)?;
        for (i1, e2) in info1.iter().zip(&entropies2) {
            if *i1 > 1.0 - e2 {
                increased += i1 - (1.0 - e2);
            }
        }
        Ok(increased)
    }

    /// `AnalyzeNewInfo mu [dir]`: along a lineage, the information each
    /// child gains and loses about the environment relative to its parent.
    pub(crate) fn analyze_new_info(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(
            self.out,
            "Analyze new information in child about environment ..."
        )?;
        if su::count_num_words(&cur_string) < 1 {
            writeln!(self.out, "This command requires mutation rate, skipping.")?;
            return Ok(());
        }
        let mu = su::as_double(&su::pop_word(&mut cur_string));
        let directory = pop_directory(&su::pop_word(&mut cur_string), "newinfo/")?;
        let bi = self.batch_index(self.cur_batch)?;
        if !self.batches[bi].is_lineage {
            writeln!(
                self.out,
                "This command requires the lineage in the batch, skipping."
            )?;
            return Ok(());
        }
        let mut df = create_file(host, &format!("{directory}lineage.newinfo.dat"))?;
        write_raw(
            &mut df,
            "# Legend:\n# 1:Child Genotype ID\n# 2:Parent Genotype ID\n# 3:Information of Child about Environment I(C:E)\n# 4:Information of Parent about Environment I(P:E)\n# 5:I(C:E)-I(P:E)\n# 6:Information Gained in Child\n# 7:Information Decreased in Child\n# 8:Net Increasing of Information in Child\n\n",
        )?;
        if self.batches[bi].list.is_empty() {
            return Ok(());
        }
        let mut parent = 0usize;
        let h_p_e = self.analyze_entropy(host, bi, parent, mu)?;
        let mut i_p_e = f64::from(self.batches[bi].list[parent].length) - h_p_e;
        for child in 1..self.batches[bi].list.len() {
            if Self::verbosity(host) >= VERBOSE_ON {
                writeln!(
                    self.out,
                    "Analyze new information for {}",
                    self.batches[bi].list[child].name
                )?;
            }
            if i_p_e == 0.0 {
                return Err(self.exit_with(
                    "Error: Information between parent and its enviroment is zero.(cAnalyze::AnalyzeNewInfo)",
                ));
            }
            let h_c_e = self.analyze_entropy(host, bi, child, mu)?;
            let i_c_e = f64::from(self.batches[bi].list[child].length) - h_c_e;
            let net_gain = i_c_e - i_p_e;
            let gained = self.increased_info(host, bi, child, parent, mu)?;
            let lost = self.increased_info(host, bi, parent, child, mu)?;
            let line = format!(
                "{} {} {} {} {} {} {} {}\n",
                self.batches[bi].list[child].id_num,
                self.batches[bi].list[parent].id_num,
                fmt_double(i_c_e),
                fmt_double(i_p_e),
                fmt_double(net_gain),
                fmt_double(gained),
                fmt_double(lost),
                fmt_double(gained - lost)
            );
            write_raw(&mut df, &line)?;
            parent = child;
            i_p_e = i_c_e;
        }
        Ok(())
    }

    // -- MUTATION_REVERT ----------------------------------------------------

    /// `MutationRevert [file [max_dist [max_depth]]]`: along an aligned
    /// lineage, the fitness of descendants with each parent→child
    /// mutation reverted (50 trials with random inputs each).
    ///
    /// QUIRK: the look-ahead iterator wraps around the batch: after the
    /// last genotype one `-1` block is printed, then it continues with the
    /// first genotype again.
    pub(crate) fn mutation_revert(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut filename = "XXX.dat".to_string();
        let mut max_dist = -1;
        let mut max_depth = 5;
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        if !cur_string.is_empty() {
            max_dist = su::as_int(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            max_depth = su::as_int(&su::pop_word(&mut cur_string));
        }
        let bi = self.batch_index(self.cur_batch)?;
        if !self.batches[bi].is_lineage {
            writeln!(
                self.out,
                "Error: This command requires a lineage.  Skipping."
            )?;
            return Ok(());
        }
        static_file(host, &filename)?;
        self.command_align(host)?;
        let mut test_info = CpuTestInfo::default();
        test_info.use_random_inputs(true);
        let (hw_type, is_name) = {
            let is = host
                .inst_sets()
                .default_inst_set()
                .ok_or_else(|| AnalyzeError::fatal("MUTATION_REVERT without an instruction set"))?;
            (is.hardware_type(), is.inst_set_name().to_string())
        };
        let n = self.batches[bi].list.len();
        let mut fot = String::new();
        // `tListIterator` positions: Some(i) for genotype i, None for the
        // list's root, after which `Next()` starts over.
        let next = |p: Option<usize>| -> Option<usize> {
            match p {
                None => (n > 0).then_some(0),
                Some(i) => (i + 1 < n).then_some(i + 1),
            }
        };
        let mut parent = next(None);
        let mut pos = parent;
        loop {
            pos = next(pos);
            let (Some(gi), Some(pi)) = (pos, parent) else {
                break;
            };
            let list = &self.batches[bi].list;
            let (g, p) = (&list[gi], &list[pi]);
            fot.push_str(&format!(
                "{} {} {} {} {} {} {} ",
                g.id_num,
                fmt_double(g.fitness),
                g.update_born,
                g.parent_dist,
                p.id_num,
                fmt_double(p.fitness),
                p.update_born
            ));
            let align_parent = p.aligned_sequence.as_bytes().to_vec();
            let align_child = g.aligned_sequence.as_bytes().to_vec();
            let mut reversion: Vec<u8> = Vec::with_capacity(align_parent.len());
            for (k, &pc) in align_parent.iter().enumerate() {
                let c = *align_child.get(k).ok_or_else(|| {
                    AnalyzeError::fatal("MUTATION_REVERT: aligned sequences differ in length (C++ reads past a string)")
                })?;
                reversion.push(if pc == c {
                    b' '
                } else if pc == b'_' && c != b'_' {
                    b'+'
                } else if pc != b'_' && c == b'_' {
                    b'-'
                } else {
                    pc
                });
            }
            let mut cum_dist: i32 = 0;
            let mut look = pos;
            for _ in 0..max_depth {
                look = next(look);
                let other = match look {
                    Some(oi) if cum_dist <= max_dist || max_dist == -1 => oi,
                    _ => {
                        fot.push_str("-1 -1 -1 -1 -1 ");
                        continue;
                    }
                };
                let o = &self.batches[bi].list[other];
                cum_dist = cum_dist.wrapping_add(o.parent_dist);
                if cum_dist > max_dist && max_dist != -1 {
                    break;
                }
                let align_other = o.aligned_sequence.as_bytes();
                let mut reverted = Vec::new();
                for (k, &r) in reversion.iter().enumerate() {
                    if r == b'+' {
                        continue;
                    } else if r == b'-' {
                        reverted.push(align_parent[k]);
                    } else if r != b' ' {
                        reverted.push(r);
                    } else {
                        let c = *align_other.get(k).ok_or_else(|| {
                            AnalyzeError::fatal("MUTATION_REVERT: aligned sequences differ in length (C++ reads past a string)")
                        })?;
                        if c != b'_' {
                            reverted.push(c);
                        }
                    }
                }
                let (oid, ofit, oborn) = (o.id_num, o.fitness, o.update_born);
                let rev_genome = Genome::with_inst_set(
                    hw_type,
                    &is_name,
                    InstructionSequence::from_string(&String::from_utf8_lossy(&reverted)),
                );
                let mut new_genotype = AnalyzeGenotype::new(rev_genome);
                recalc_in_place(host, &mut new_genotype, &mut test_info, 50)?;
                fot.push_str(&format!(
                    "{oid} {} {oborn} {cum_dist} {} ",
                    fmt_double(ofit),
                    fmt_double(new_genotype.fitness)
                ));
            }
            fot.push('\n');
            parent = pos;
        }
        let df = static_file(host, &filename)?;
        write_raw(df, &fot)?;
        df.flush();
        Ok(())
    }

    // -- COMPETE ------------------------------------------------------------

    /// `BatchCompete size from [to [next_id [next_update]]]`: sample `size`
    /// offspring from batch `from` in proportion to fitness (a
    /// `Probabilistic` scheduler with its own RNG seeded from the world
    /// RNG), mutate them with the configured copy / insertion / deletion
    /// rates and append them to batch `to` with fresh ids.
    ///
    /// The offspring genome is the one the test CPU produced while
    /// recalculating the parent (random inputs), as in C++.
    pub(crate) fn batch_compete(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if cur_string.is_empty() {
            return Err(self.exit_with("Compete Error: Must include target batch size!"));
        }
        let batch_size = su::as_int(&su::pop_word(&mut cur_string));
        if cur_string.is_empty() {
            return Err(self.exit_with("Compete Error: Must include from ID!"));
        }
        let batch_from = su::as_int(&su::pop_word(&mut cur_string));
        let mut batch_to = self.cur_batch;
        if !cur_string.is_empty() {
            batch_to = su::as_int(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            let initial_next_id = su::as_int(&su::pop_word(&mut cur_string));
            if initial_next_id >= 0 {
                self.temp_next_id = initial_next_id;
            }
        }
        if !cur_string.is_empty() {
            let initial_next_update = su::as_int(&su::pop_word(&mut cur_string));
            if initial_next_update >= 0 {
                self.temp_next_update = initial_next_update;
            }
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(
                self.out,
                "Compete {batch_size} organisms from batch {batch_from} to batch {batch_to};"
            )?;
            writeln!(
                self.out,
                "assigning new IDs starting with {}.",
                self.temp_next_id
            )?;
        }
        let from = self.batch_index(batch_from)?;
        let parent_batch_size = self.batches[from].list.len();
        let seed = {
            let rng = world_rng(host, "COMPETE")?;
            let max = rng.max_seed();
            rng.get_int(max)
        };
        let mut schedule = Probabilistic::new(parent_batch_size, AvidaRng::new(seed));
        let mut info = CpuTestInfo::default();
        info.use_random_inputs(true);
        let mut offspring: Vec<Option<Genome>> = Vec::with_capacity(parent_batch_size);
        for pos in 0..parent_batch_size {
            recalc_in_place(host, &mut self.batches[from].list[pos], &mut info, 1)?;
            let g = &self.batches[from].list[pos];
            let fitness = if g.viable {
                let child = info
                    .test_organism(0)
                    .ok_or_else(|| AnalyzeError::fatal("COMPETE: no test organism"))?
                    .state
                    .offspring_genome
                    .clone();
                offspring.push(Some(child));
                // cMerit: the fitness scaled by 1000 (negative values clamp to 0).
                avida_phenotype::Merit::new(g.fitness * 1000.0).get_double()
            } else {
                offspring.push(None);
                0.0
            };
            schedule.adjust_priority(pos as i32, fitness);
        }
        let (copy_mut_prob, ins_mut_prob, del_mut_prob) = {
            let c = host.config();
            (c.copy_mut_prob, c.divide_ins_prob, c.divide_del_prob)
        };
        let mut new_genotypes = Vec::new();
        let mut i = 0;
        while i < batch_size {
            let array_pos = schedule.next();
            if array_pos < 0 {
                writeln!(
                    self.out,
                    "Warning: No organisms in origin batch have positive fitness, cannot sample to destination batch."
                )?;
                break;
            }
            let parent = &self.batches[from].list[array_pos as usize];
            let parent_id = parent.id_num;
            let mut child_genome = offspring[array_pos as usize].clone().ok_or_else(|| {
                AnalyzeError::fatal("COMPETE sampled a genotype without an offspring")
            })?;
            let inst_set = crate::landscape::shared_inst_set_of(host.inst_sets(), &child_genome)?;
            let rng = world_rng(host, "COMPETE")?;
            if copy_mut_prob > 0.0 {
                for n in 0..child_genome.representation().size() {
                    if rng.p(copy_mut_prob) {
                        let inst = inst_set.random_inst(rng);
                        child_genome.representation_mut().set(n, inst);
                    }
                }
            }
            if rng.p(ins_mut_prob) {
                let ins_line = rng.get_int(child_genome.representation().size() + 1);
                let inst = inst_set.random_inst(rng);
                child_genome.representation_mut().insert(ins_line, inst);
            }
            if rng.p(del_mut_prob) {
                let del_line = rng.get_int(child_genome.representation().size());
                child_genome.representation_mut().remove(del_line, 1);
            }
            let child_id = self.temp_next_id;
            self.temp_next_id = child_id.wrapping_add(1);
            let mut g = AnalyzeGenotype::new(child_genome);
            g.set_parent_id(parent_id);
            g.id_num = child_id;
            g.name = format!("org-{child_id}");
            g.update_born = self.temp_next_update;
            new_genotypes.push(g);
            i += 1;
        }
        self.temp_next_update = self.temp_next_update.wrapping_add(1);
        let to = self.batch_index(batch_to)?;
        self.batches[to].list.extend(new_genotypes);
        self.batches[to].is_lineage = false;
        self.batches[to].is_aligned = false;
        Ok(())
    }
}
