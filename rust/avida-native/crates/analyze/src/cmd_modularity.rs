//! `CALC_FUNCTIONAL_MODULARITY` (`cAnalyze::CommandCalcFunctionalModularity`
//! and `cModularityAnalysis::CalcFunctionalModularity`,
//! analyze/cModularityAnalysis.cc at 47f13dadb): which sites each task
//! depends on, from single-site knockouts in the test CPU. The results are
//! read through the `tasks_done`, `sites_per_task.N`, ... data entries.
//! Also `AVERAGE_MODULARITY` (`cAnalyze::CommandAverageModularity`,
//! analyze/cAnalyze.cc), the older knockout-based summary.

use std::io::Write;

use avida_apto::{fmt_double, AvidaRng};
use avida_config::string_util as su;
use avida_genome::{Genome, Instruction};
use avida_hardware::test_cpu::{CpuTestInfo, TestCpu};

use crate::analyze::Analyze;
use crate::cmd_mutation::{base_ops, recalc_in_place};
use crate::cmd_output::compare_flex_stat;
use crate::cmd_population::static_file;
use crate::data_entry::load_command_list;
use crate::error::{AnalyzeError, Result};
use crate::genotype::{AnalyzeGenotype, ModularityData};
use crate::host::{AnalyzeHost, CpuTestOptions};
use crate::jobs::{active_null_inst, JobWorld};
use crate::landscape::inst_set_of;
use crate::verbose::VERBOSE_ON;

fn hw<T>(r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(AnalyzeError::world)
}

/// Run `genome` in `cpu` and return the colony fitness and the colony
/// organism's task counts (`GetColonyFitness`, `GetColonyOrganism()->
/// GetPhenotype().GetLastTaskCount()`).
fn colony_test(
    world: &mut dyn JobWorld,
    rng: &mut AvidaRng,
    cpu: &mut Option<TestCpu>,
    info: &mut CpuTestInfo,
    genome: &Genome,
) -> Result<(f64, Vec<i32>)> {
    let mut out = None;
    world.with_job_world(rng, &mut |w| {
        if cpu.is_none() {
            *cpu = Some(hw(TestCpu::new(w))?);
        }
        if let Some(c) = cpu.as_mut() {
            hw(c.test_genome(w, info, genome))?;
        }
        let tasks = info
            .colony_organism()
            .map(|o| o.state.phenotype.last_task_count.clone())
            .ok_or_else(|| AnalyzeError::fatal("test CPU left no colony organism"))?;
        out = Some((info.colony_fitness(), tasks));
        Ok(())
    })?;
    out.ok_or_else(|| AnalyzeError::fatal("the host did not run the test CPU"))
}

/// `cModularityAnalysis::CalcFunctionalModularity(ctx)` for one genotype,
/// in the job context `rng`: `None` when the genotype is not viable or does
/// no task (nothing is stored then).
fn functional_modularity(
    world: &mut dyn JobWorld,
    rng: &mut AvidaRng,
    genome: &Genome,
    opts: &CpuTestOptions,
) -> Result<Option<ModularityData>> {
    let mut cpu = None;
    let mut info = opts.test_info();
    let (base_fitness, base_tasks) = colony_test(world, rng, &mut cpu, &mut info, genome)?;
    let num_tasks = base_tasks.len();
    if !(base_fitness > 0.0 && base_tasks.iter().any(|&t| t > 0)) {
        return Ok(None);
    }
    let set_name = inst_set_of(world.inst_sets(), genome)?
        .inst_set_name()
        .to_string();
    // The tested organisms share the instruction set; the next test clears
    // them anyway.
    info.clear();
    let null_inst = world.activate_null_inst(&set_name)?;

    let max_line = genome.representation().size().max(0) as usize;
    let mut mod_genome = genome.clone();
    let mut mod_matrix = vec![vec![0i32; max_line]; num_tasks];
    let mut site_num_tasks = vec![0i32; max_line];
    let mut sites_per_task = vec![0i32; num_tasks];
    for line in 0..max_line {
        let cur_inst = genome.representation().get(line as i32).op();
        mod_genome.representation_mut().set(line as i32, null_inst);
        let (fitness, test_tasks) = colony_test(world, rng, &mut cpu, &mut info, &mod_genome)?;
        if fitness > 0.0 {
            for task in 0..num_tasks {
                let knocked_out = test_tasks.get(task).copied().ok_or_else(|| {
                    AnalyzeError::fatal("knockout organism has fewer task counts than its base")
                })? == 0;
                if base_tasks[task] != 0 && knocked_out {
                    mod_matrix[task][line] = 1;
                    sites_per_task[task] += 1;
                    site_num_tasks[line] += 1;
                }
            }
        }
        mod_genome
            .representation_mut()
            .set(line as i32, Instruction::new(cur_inst));
    }

    let mut sites_inv_x_tasks = vec![0i32; num_tasks + 1];
    let mut task_length = vec![0i32; num_tasks];
    let mut total_task = 0i32;
    let mut total_inst = 0i32;
    let mut total_all = 0i32;
    let mut sum_task_overlap = 0.0f64;
    for &s in &sites_per_task {
        total_all += s;
        if s != 0 {
            total_task += 1;
        }
    }
    for &n in &site_num_tasks {
        sites_inv_x_tasks[n as usize] += 1;
        if n != 0 {
            total_inst += 1;
        }
    }
    let mut task_overlap = vec![vec![0i32; num_tasks]; num_tasks];
    // Every pair j <= k of tasks that share a site, counted per site (as
    // the C++ triple loop does).
    for line in 0..max_line {
        let used: Vec<usize> = mod_matrix
            .iter()
            .enumerate()
            .filter(|(_, row)| row[line] > 0)
            .map(|(t, _)| t)
            .collect();
        for (a, &j) in used.iter().enumerate() {
            for &k in &used[a..] {
                task_overlap[j][k] += 1;
                if j != k {
                    task_overlap[k][j] += 1;
                }
            }
        }
    }
    if total_task > 1 {
        for (i, row) in task_overlap.iter().enumerate() {
            if row[i] != 0 {
                let mut overlap_per_task = 0i32;
                for (j, &o) in row.iter().enumerate() {
                    if i != j {
                        overlap_per_task += o;
                    }
                }
                sum_task_overlap +=
                    f64::from(overlap_per_task) / f64::from(row[i] * (total_task - 1));
            }
        }
    }
    for i in 0..num_tasks {
        for (j, &m) in mod_matrix[i].iter().enumerate() {
            if m > 0 && task_length[i] == 0 {
                task_length[i] = j as i32;
                break;
            }
        }
    }
    for i in 0..num_tasks {
        for j in (0..max_line).rev() {
            if mod_matrix[i][j] > 0 {
                task_length[i] = j as i32 - task_length[i] + 1;
                break;
            }
        }
    }
    let ave_task_position: Vec<f64> = (0..num_tasks)
        .map(|i| {
            let pos: i32 = (0..max_line)
                .filter(|&j| mod_matrix[i][j] > 0)
                .map(|j| j as i32)
                .sum();
            f64::from(pos) / f64::from(sites_per_task[i])
        })
        .collect();
    Ok(Some(ModularityData {
        tasks_done: total_task,
        insts_tasks: total_inst,
        tasks_prop: f64::from(total_inst) / max_line as f64,
        ave_tasks_per_site: if total_inst != 0 {
            f64::from(total_all) / f64::from(total_inst)
        } else {
            0.0
        },
        ave_sites_per_task: if total_task != 0 {
            f64::from(total_all) / f64::from(total_task)
        } else {
            0.0
        },
        ave_prop_nonoverlap: if total_task != 0 {
            sum_task_overlap / f64::from(total_task)
        } else {
            0.0
        },
        sites_per_task,
        sites_inv_x_tasks,
        task_length,
        ave_task_position,
    }))
}

impl Analyze {
    /// `PopCommonCPUTestParameters(world, cur_string, test_info, ...)`:
    /// `[use_resources [update [random_inputs [manual inputs...]]]]`.
    pub(crate) fn pop_common_cpu_test_parameters(
        &mut self,
        host: &dyn AnalyzeHost,
        cur_string: &mut String,
    ) -> Result<CpuTestOptions> {
        let pop_int = |s: &mut String, d: i32| {
            if s.is_empty() {
                d
            } else {
                su::as_int(&su::pop_word(s))
            }
        };
        let use_resources = pop_int(cur_string, 0);
        let update = pop_int(cur_string, -1);
        let use_random_inputs =
            !cur_string.is_empty() && su::as_int(&su::pop_word(cur_string)) == 1;
        let manual_inputs = self.pop_manual_inputs(host, cur_string)?;
        Ok(CpuTestOptions {
            use_resources,
            update,
            use_random_inputs: use_random_inputs && manual_inputs.is_none(),
            manual_inputs,
            resources: self.resources.clone(),
            resource_time_spent_offset: self.resource_time_spent_offset,
            state_grid: 0,
        })
    }

    /// `CommandCalcFunctionalModularity`: one analyze job per genotype of
    /// the current batch, each in its own job context.
    pub(crate) fn command_calc_functional_modularity(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Calculating Functional Modularity...")?;
        let opts = self.pop_common_cpu_test_parameters(host, &mut cur_string)?;
        let bi = self.batch_index(self.cur_batch)?;
        let units: Vec<_> = self.batches[bi]
            .list
            .iter()
            .enumerate()
            .map(|(gi, g)| ((gi, g.genome.clone(), None), 1))
            .collect();
        // A job activates the NULL instruction of its genome's set, which
        // worker threads cannot do: they take over once every set the
        // batch uses has it.
        let set_names: Vec<String> = self.batches[bi]
            .list
            .iter()
            .map(|g| g.genome.inst_set().to_string())
            .collect();
        let ready = |host: &dyn AnalyzeHost| {
            set_names.iter().all(|name| {
                host.inst_sets()
                    .get_inst_set(name)
                    .is_some_and(|set| active_null_inst(set).is_some())
            })
        };
        self.run_job_units(
            host,
            units,
            &|(_, genome, data), world, seeds| {
                let mut rng = seeds.next_rng()?;
                *data = functional_modularity(world, &mut rng, genome, &opts)?;
                Ok(())
            },
            &ready,
            &mut |a, _, (gi, _, data)| {
                if let Some(data) = data {
                    a.batches[bi].list[gi].data.set_modularity(data);
                }
                Ok(())
            },
        )?;
        self.job_queue_start(host)
    }
}

/// `cStringList::PopString(s) != ""`: remove the first `s`.
fn pop_arg(args: &mut Vec<String>, s: &str) -> bool {
    match args.iter().position(|a| a == s) {
        Some(p) => {
            args.remove(p);
            true
        }
        None => false,
    }
}

impl Analyze {
    /// `CommandAverageModularity file [0|1] columns...`: one line in `file`
    /// (a static file, so several calls append) with abundance-weighted
    /// modularity statistics of the genotypes of the current batch that
    /// have positive fitness and do a task, from single-site NULL
    /// knockouts that lower a column without killing the organism. With no
    /// column arguments it only writes the header comments.
    ///
    /// QUIRK: whether a genotype "does tasks" is read from its task counts
    /// by column index, whatever the columns are; the boolean mode `1` is
    /// accepted but changes nothing; the integer mean position `sum /
    /// num_inst` is truncated, and the deviation uses only the squared
    /// distance of the last site of each task, not a sum.
    pub(crate) fn command_average_modularity(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Average Modularity calculations")?;
        let filename = su::pop_word(&mut cur_string);
        let mut args = su::split_list(&cur_string, ' ');
        writeln!(self.out, "Found {} args.", args.len())?;
        let mut print_mode = 0;
        if pop_arg(&mut args, "0") {
            print_mode = 0;
        }
        if pop_arg(&mut args, "1") {
            print_mode = 1;
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
            writeln!(self.out, "text files.")?;
            write!(self.out, "  Format: ")?;
            for c in &cmds {
                write!(self.out, "{} ", c.name())?;
            }
            writeln!(self.out)?;
        }
        static_file(host, &filename)?;
        if args.is_empty() {
            let header = "# Avida analyze modularity data\n\
                # 1: organism length\n\
                # 2: number of tasks done\n\
                # 3: number of sites used in tasks\n\
                # 4: proportion of sites used in tasks\n\
                # 5: average number of tasks done per site\n\
                # 6: average number sites per task done\n\
                # 7: average number tasks per site per task\n\
                # 8: average proportion of the non-overlaping region of a task\n\
                # 9-17: average StDev in positions used for task 1-9\n\
                # 18-26: average number of sites necessary for each of the tasks\n\
                # 27-36: number of sites involved in 0-9 tasks\n\
                # 37-45: average task length (distance from first to last inst used)\n\n";
            return write_static(host, &filename, header);
        }

        let mut num_orgs = 0i32;
        let mut av_length = 0.0f64;
        let mut av_task = 0.0f64;
        let mut av_inst = 0.0f64;
        let mut av_inst_len = 0.0f64;
        let mut av_site_task = 0.0f64;
        let mut av_task_site = 0.0f64;
        let mut av_t_s_norm = 0.0f64;
        let mut av_task_overlap = 0.0f64;
        let mut std_task_position = vec![0.0f64; num_cols];
        let mut org_task = vec![0.0f64; num_cols];
        let mut av_num_inst = vec![0.0f64; num_cols];
        let mut av_inst_task = vec![0.0f64; num_cols + 1];
        let mut av_task_length = vec![0.0f64; num_cols];

        let bi = self.batch_index(self.cur_batch)?;
        for gi in 0..self.batches[bi].list.len() {
            let num_cpus = self.batches[bi].list[gi].num_cpus;
            if verbose {
                writeln!(self.out, "  Mapping {}", self.batches[bi].list[gi].name)?;
            }
            self.out.flush()?;
            recalc_in_place(
                host,
                &mut self.batches[bi].list[gi],
                &mut CpuTestInfo::default(),
                1,
            )?;
            let g = &self.batches[bi].list[gi];
            let mut does_tasks = false;
            for i in 0..num_cols {
                if g.task_count(i as i32)? > 0 {
                    does_tasks = true;
                    break;
                }
            }
            if !(g.fitness > 0.0 && does_tasks) {
                continue;
            }
            num_orgs = num_orgs.wrapping_add(num_cpus);
            let max_line = g.length;
            let base = base_ops(&g.genome, max_line, "AVERAGE_MODULARITY")?;
            let mut mod_genome = g.genome.clone();
            let set_name = inst_set_of(host.inst_sets(), &mod_genome)?
                .inst_set_name()
                .to_string();
            let null_inst = host.activate_null_inst(&set_name)?;
            let ml = base.len();
            let mut mod_matrix = vec![vec![0i32; ml]; num_cols];
            let mut task_overlap = vec![vec![0i32; num_cols]; num_cols];
            let mut num_task = vec![0i32; ml];
            let mut num_inst = vec![0i32; num_cols];
            let mut sum = vec![0i32; num_cols];
            let mut inst_task = vec![0i32; num_cols + 1];
            let mut task_length = vec![0i32; num_cols];

            for (line, &cur_inst) in base.iter().enumerate() {
                mod_genome.representation_mut().set(line as i32, null_inst);
                let mut test = AnalyzeGenotype::new(mod_genome.clone());
                recalc_in_place(host, &mut test, &mut CpuTestInfo::default(), 1)?;
                for (col, c) in cmds.iter().enumerate() {
                    let test_value = c.value(&mut test, host)?;
                    let base_value = c.value(&mut self.batches[bi].list[gi], host)?;
                    let compare = compare_flex_stat(&test_value, &base_value, c.compare_type());
                    if compare < 0 && test.fitness != 0.0 {
                        mod_matrix[col][line] = 1;
                        num_inst[col] += 1;
                        num_task[line] += 1;
                    }
                }
                mod_genome
                    .representation_mut()
                    .set(line as i32, Instruction::new(cur_inst));
            }

            let total_task = num_inst.iter().filter(|&&n| n != 0).count() as i32;
            let total_inst = num_task.iter().filter(|&&n| n != 0).count() as i32;
            let total_all = num_inst.iter().fold(0i32, |a, &n| a.wrapping_add(n));
            let nc = f64::from(num_cpus);
            av_length += f64::from(max_line.wrapping_mul(num_cpus));
            av_task += f64::from(total_task.wrapping_mul(num_cpus));
            av_inst += f64::from(total_inst.wrapping_mul(num_cpus));
            av_inst_len += f64::from(total_inst) * nc / f64::from(max_line);
            if total_task != 0 {
                av_site_task += nc * f64::from(total_all) / f64::from(total_task);
            }
            if total_inst != 0 {
                av_task_site += nc * f64::from(total_all) / f64::from(total_inst);
            }
            if total_inst != 0 && total_task != 0 {
                av_t_s_norm +=
                    nc * f64::from(total_all) / f64::from(total_inst.wrapping_mul(total_task));
            }
            for i in 0..num_cols {
                if num_inst[i] > 0 {
                    av_num_inst[i] += f64::from(num_inst[i].wrapping_mul(num_cpus));
                    org_task[i] += nc;
                }
            }

            for j in 0..num_cols {
                for k in j..num_cols {
                    let shared = mod_matrix[j]
                        .iter()
                        .zip(&mod_matrix[k])
                        .filter(|&(&a, &b)| a > 0 && b > 0)
                        .count() as i32;
                    task_overlap[j][k] += shared;
                    if j != k {
                        task_overlap[k][j] += shared;
                    }
                }
            }
            let mut sum_task_overlap = 0.0f64;
            if total_task > 1 {
                for (i, row) in task_overlap.iter().enumerate() {
                    let mut overlap_per_task = 0.0f64;
                    for (j, &o) in row.iter().enumerate() {
                        if i != j {
                            overlap_per_task += f64::from(o);
                        }
                    }
                    if row[i] != 0 {
                        sum_task_overlap +=
                            overlap_per_task / f64::from(row[i].wrapping_mul(total_task - 1));
                    }
                }
            }
            if total_task != 0 {
                av_task_overlap += nc * sum_task_overlap / f64::from(total_task);
            }

            for (row, len) in mod_matrix.iter().zip(task_length.iter_mut()) {
                if let Some(first) = row.iter().position(|&m| m > 0) {
                    *len = first as i32;
                }
                if let Some(last) = row.iter().rposition(|&m| m > 0) {
                    *len = last as i32 - *len + 1;
                }
            }
            for (av, &len) in av_task_length.iter_mut().zip(&task_length) {
                *av += f64::from(num_cpus.wrapping_mul(len));
            }
            for (row, s) in mod_matrix.iter().zip(sum.iter_mut()) {
                for (j, &m) in row.iter().enumerate() {
                    if m > 0 {
                        *s += j as i32;
                    }
                }
            }
            let mut temp = 0.0f64;
            for i in 0..num_cols {
                if num_inst[i] > 1 {
                    let av_sum = f64::from(sum[i] / num_inst[i]);
                    for (j, &m) in mod_matrix[i].iter().enumerate() {
                        if m > 0 {
                            let d = av_sum - j as f64;
                            temp = d * d;
                        }
                    }
                    std_task_position[i] = (temp / f64::from(num_inst[i] - 1))
                        .sqrt()
                        .mul_add(nc, std_task_position[i]);
                }
            }
            for &n in &num_task {
                inst_task[n as usize] += 1;
            }
            for (av, &n) in av_inst_task.iter_mut().zip(&inst_task) {
                *av += f64::from(n.wrapping_mul(num_cpus));
            }
        }

        let mut text = String::new();
        if num_orgs != 0 {
            let no = f64::from(num_orgs);
            for v in [
                av_length,
                av_task,
                av_inst,
                av_inst_len,
                av_task_site,
                av_site_task,
                av_t_s_norm,
            ] {
                text.push_str(&format!("{} ", fmt_double(v / no)));
            }
            text.push_str(&format!("{} ", fmt_double(1.0 - av_task_overlap / no)));
            for (v, &o) in std_task_position
                .iter()
                .zip(&org_task)
                .chain(av_num_inst.iter().zip(&org_task))
            {
                if o > 0.0 {
                    text.push_str(&format!("{} ", fmt_double(v / o)));
                } else {
                    text.push_str("0 ");
                }
            }
            for v in av_inst_task.iter().chain(&av_task_length) {
                text.push_str(&format!("{} ", fmt_double(v / no)));
            }
        } else {
            for _ in 0..8 + 4 * num_cols + 1 {
                text.push_str("0 ");
            }
        }
        text.push('\n');
        write_static(host, &filename, &text)
    }
}

/// Write raw text to the static file at `path` and flush it (`endl`).
fn write_static(host: &mut dyn AnalyzeHost, path: &str, text: &str) -> Result<()> {
    let df = static_file(host, path)?;
    df.ofstream().write_all(text.as_bytes())?;
    df.flush();
    Ok(())
}
