//! Output commands of `cAnalyze`: `PRINT`, `TRACE`, `PRINT_TASKS*`,
//! `DETAIL*`, `HISTOGRAM`, `PRINT_PHENOTYPES`, plus `HTMLPrintStat` and
//! `CompareFlexStat`.

use std::io::Write;

use avida_apto::fmt_double;
use avida_config::string_util as su;

use crate::analyze::{Analyze, GenotypeBatch};
use crate::apto_map::apto_map_key_order;
use crate::data_entry::{flex_compare, get_data_command, load_command_list_str, DataEntryCommand};
use crate::error::{AnalyzeError, Result};
use crate::flex::{CmpOp, FlexVar};
use crate::host::{AnalyzeHost, CpuTestOptions};
use crate::verbose::{VERBOSE_DETAILS, VERBOSE_ON};

/// `COMPARE_RESULT_*` (cAnalyze.h).
pub(crate) mod compare_result {
    pub const OFF: i32 = -2;
    pub const NEG: i32 = -1;
    pub const SAME: i32 = 0;
    pub const POS: i32 = 1;
    pub const ON: i32 = 2;
    pub const DIFF: i32 = 3;
}

/// `cAnalyze::CompareFlexStat(org_stat, parent_stat, compare_type)`.
pub(crate) fn compare_flex_stat(org: &FlexVar, parent: &FlexVar, compare_type: i32) -> i32 {
    use compare_result::*;
    if compare_type == flex_compare::NONE {
        return SAME;
    }
    if org.cmp(parent, CmpOp::Eq) {
        return SAME;
    }
    if compare_type == flex_compare::DIFF {
        return DIFF;
    }
    if compare_type == flex_compare::MAX {
        return if org.cmp(parent, CmpOp::Gt) { POS } else { NEG };
    }
    if compare_type == flex_compare::MIN {
        return if org.cmp(parent, CmpOp::Gt) { NEG } else { POS };
    }
    if org.cmp_int(0, CmpOp::Eq) {
        return OFF;
    }
    if parent.cmp_int(0, CmpOp::Eq) {
        return ON;
    }
    if compare_type == flex_compare::DIFF2 {
        return DIFF;
    }
    if compare_type == flex_compare::MAX2 {
        return if org.cmp(parent, CmpOp::Gt) { POS } else { NEG };
    }
    if compare_type == flex_compare::MIN2 {
        return if org.cmp(parent, CmpOp::Gt) { NEG } else { POS };
    }
    // `assert(false); return 0;`
    0
}

/// The `COLOR_*` settings `HTMLPrintStat` uses.
#[derive(Debug, Clone)]
pub(crate) struct Colors {
    neg2: String,
    neg1: String,
    same: String,
    pos1: String,
    pos2: String,
    diff: String,
}

impl Colors {
    pub(crate) fn of(host: &dyn AnalyzeHost) -> Self {
        let c = host.config();
        Colors {
            neg2: c.color_neg2.clone(),
            neg1: c.color_neg1.clone(),
            same: c.color_same.clone(),
            pos1: c.color_pos1.clone(),
            pos2: c.color_pos2.clone(),
            diff: c.color_diff.clone(),
        }
    }
}

/// `cAnalyze::HTMLPrintStat(value, fp, compare, cell_flags, null_text, true)`.
pub(crate) fn html_print_stat(
    value: &FlexVar,
    compare: i32,
    cell_flags: &str,
    null_text: &str,
    colors: &Colors,
    fp: &mut String,
) -> Result<()> {
    html_print_stat_text(value, compare, cell_flags, null_text, colors, fp, true)
}

/// `cAnalyze::HTMLPrintStat(value, fp, compare, cell_flags, null_text,
/// print_text)`: without `print_text` the cell shows `&nbsp;`.
pub(crate) fn html_print_stat_text(
    value: &FlexVar,
    compare: i32,
    cell_flags: &str,
    null_text: &str,
    colors: &Colors,
    fp: &mut String,
    print_text: bool,
) -> Result<()> {
    use compare_result::*;
    fp.push_str(&format!("<td {cell_flags} "));
    if compare == OFF {
        fp.push_str(&format!("bgcolor=\"#{}\">", colors.neg2));
        if print_text {
            fp.push_str(&format!("{null_text} "));
        } else {
            fp.push_str("&nbsp; ");
        }
        return Ok(());
    }
    let color = match compare {
        NEG => &colors.neg1,
        SAME => &colors.same,
        POS => &colors.pos1,
        ON => &colors.pos2,
        DIFF => &colors.diff,
        _ => {
            return Err(AnalyzeError::fatal(format!(
                "Error! Illegal case in Compare:{compare} (the C++ exits with status 0)"
            )))
        }
    };
    if print_text {
        fp.push_str(&format!("bgcolor=\"#{color}\">{} ", value.print()));
    } else {
        fp.push_str(&format!("bgcolor=\"#{color}\">&nbsp; "));
    }
    Ok(())
}

/// `FILE_TYPE_HTML` when the text after the last '.' (or the whole name
/// without one) is "html".
pub(crate) fn is_html(filename: &str) -> bool {
    let mut ext = filename.to_string();
    while su::find_char(&ext, '.', 0) != -1 {
        su::pop(&mut ext, '.');
    }
    ext == "html"
}

/// `cAnalyze::PopDirectory(in_string, default_dir)`: the first word (or the
/// default), with a trailing '/' added unless it ends in '/' or '\'.
pub(crate) fn pop_directory(in_string: &str, default_dir: &str) -> Result<String> {
    let mut directory = default_dir.to_string();
    if !in_string.is_empty() {
        let mut s = in_string.to_string();
        directory = su::pop_word(&mut s);
    }
    match directory.as_bytes().last() {
        None => Err(AnalyzeError::fatal(
            "PopDirectory of an empty directory name (the C++ reads before the string)",
        )),
        Some(b'/') | Some(b'\\') => Ok(directory),
        Some(_) => {
            directory.push('/');
            Ok(directory)
        }
    }
}

const HTML_HEAD: &str = "<html>\n<body bgcolor=\"#FFFFFF\"\n text=\"#000000\"\n link=\"#0000AA\"\n alink=\"#0000FF\"\n vlink=\"#000044\">\n\n";

/// `CommandDetail_Header(fp, format_type, output_it, time_step)`.
fn detail_header(
    fp: &mut String,
    batch: &GenotypeBatch,
    cmds: &[DataEntryCommand],
    host: &dyn AnalyzeHost,
    html: bool,
    time_step: i32,
) -> Result<()> {
    let first = batch.list.first();
    if !html {
        fp.push_str("#filetype genotype_data\n#format ");
        if time_step > 0 {
            fp.push_str("update ");
        }
        for c in cmds {
            fp.push_str(c.name());
            fp.push(' ');
        }
        fp.push_str("\n\n# Legend:\n");
        let mut count = 0;
        if time_step > 0 {
            count += 1;
            fp.push_str(&format!("# {count}: Update\n"));
        }
        for c in cmds {
            count += 1;
            fp.push_str(&format!("# {count}: {}\n", c.desc(first, host)?));
        }
        fp.push('\n');
    } else {
        fp.push_str(HTML_HEAD);
        fp.push_str(&format!(
            "<h1 align=center>Run {}\n\n<center>\n<table border=1 cellpadding=2><tr>\n",
            batch.name
        ));
        if time_step > 0 {
            fp.push_str("<th bgcolor=\"#AAAAFF\">Update ");
        }
        for c in cmds {
            fp.push_str(&format!(
                "<th bgcolor=\"#AAAAFF\">{} ",
                c.desc(first, host)?
            ));
        }
        fp.push_str("</tr>\n");
    }
    Ok(())
}

/// Where a detail body goes: the console, or a buffer for a file. Verbose
/// progress messages always go to the console, so with the console as
/// target they interleave with the data as in C++.
struct Sink<'a> {
    console: &'a mut dyn Write,
    file: Option<String>,
}

impl Sink<'_> {
    fn data(&mut self, s: &str) -> Result<()> {
        match &mut self.file {
            Some(f) => f.push_str(s),
            None => self.console.write_all(s.as_bytes())?,
        }
        Ok(())
    }

    fn msg(&mut self, s: &str) -> Result<()> {
        self.console.write_all(s.as_bytes())?;
        Ok(())
    }
}

/// `CommandDetail_Body(fp, format_type, output_it, time_step, max_time)`.
#[allow(clippy::too_many_arguments)]
fn detail_body(
    sink: &mut Sink<'_>,
    batch: &mut GenotypeBatch,
    cmds: &[DataEntryCommand],
    host: &mut dyn AnalyzeHost,
    html: bool,
    time_step: i32,
    max_time: i32,
) -> Result<()> {
    let colors = Colors::of(host);
    let n = batch.list.len();
    let mut cur: Option<usize> = if n > 0 { Some(0) } else { None };
    let mut next: Option<usize> = if n > 1 { Some(1) } else { None };
    let mut prev: Option<usize> = None;
    // The index the list iterator returns on its next `Next()`.
    let mut it_pos = 2usize;
    let advance = |it_pos: &mut usize| -> Option<usize> {
        let r = if *it_pos < n { Some(*it_pos) } else { None };
        *it_pos += 1;
        r
    };

    let mut cur_time: i32 = 0;
    while let Some(ci) = cur {
        if cur_time > max_time {
            break;
        }
        if host.config().verbosity >= VERBOSE_DETAILS {
            let g = &batch.list[ci];
            sink.msg(&format!(
                "Detailing genotype {} at depth {}\n",
                g.id_num, g.depth
            ))?;
        }
        let mut row = String::new();
        if html {
            row.push_str("<tr>");
            if time_step > 0 {
                row.push_str(&format!("<td>{cur_time} "));
            }
        } else if time_step > 0 {
            row.push_str(&format!("{cur_time} "));
        }
        for c in cmds {
            let cur_value = c.value(&mut batch.list[ci], host)?;
            if html {
                let mut compare = 0;
                if let Some(pi) = prev {
                    let prev_value = c.value(&mut batch.list[pi], host)?;
                    compare = compare_flex_stat(&cur_value, &prev_value, c.compare_type());
                }
                html_print_stat(
                    &cur_value,
                    compare,
                    c.html_cell_flags(),
                    c.null(),
                    &colors,
                    &mut row,
                )?;
            } else {
                row.push_str(&cur_value.print());
                row.push(' ');
            }
        }
        if html {
            row.push_str("</tr>");
        }
        row.push('\n');
        sink.data(&row)?;

        cur_time = cur_time.checked_add(time_step).ok_or_else(|| {
            AnalyzeError::fatal("detail time overflows int (C++ undefined behaviour)")
        })?;
        if time_step > 0 {
            while let Some(ni) = next {
                if batch.list[ni].update_born >= cur_time {
                    break;
                }
                prev = cur;
                cur = Some(ni);
                next = advance(&mut it_pos);
            }
        } else {
            prev = cur;
            cur = next;
            next = advance(&mut it_pos);
        }
    }
    if html {
        sink.data("</table>\n</center>\n")?;
    }
    Ok(())
}

impl Analyze {
    /// `CommandPrint`: one genome file per genotype through the test CPU.
    pub(crate) fn command_print(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Printing batch {}", self.cur_batch)?;
        } else {
            writeln!(self.out, "Printing organisms...")?;
        }
        let directory = pop_directory(&cur_string, "archive/")?;
        // "Weirdly, PopDirectory() doesn't actually pop, so..."
        su::pop_word(&mut cur_string);

        let bi = self.batch_index(self.cur_batch)?;
        let mut items = Vec::with_capacity(self.batches[bi].list.len());
        for g in &self.batches[bi].list {
            let mut filename = directory.clone();
            if !cur_string.is_empty() {
                filename.push_str(&su::pop_word(&mut cur_string));
            } else {
                filename.push_str(&g.name);
                filename.push_str(".gen");
            }
            items.push((g.genome.clone(), filename));
        }
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        let out = &mut self.out;
        crate::cpu::print_genomes(host, &items, &mut |filename| {
            if verbose {
                writeln!(out, "Printing: {filename}")?;
            }
            Ok(())
        })
    }

    /// The manual-input tail shared by `TRACE`, `RECALCULATE` and
    /// `PopCommonCPUTestParameters`: when exactly `input_size` words remain
    /// they become the inputs; otherwise a verbose warning.
    pub(crate) fn pop_manual_inputs(
        &mut self,
        host: &dyn AnalyzeHost,
        cur_string: &mut String,
    ) -> Result<Option<Vec<i32>>> {
        let words = su::count_num_words(cur_string) as i32;
        if words > 0 {
            let input_size = host.input_size();
            if words == input_size {
                let mut inputs = vec![0; input_size.max(0) as usize];
                let mut k = 0;
                while !cur_string.is_empty() {
                    let v = su::as_int(&su::pop_word(cur_string));
                    if k >= inputs.len() {
                        return Err(AnalyzeError::fatal(
                            "manual inputs overflow the input array",
                        ));
                    }
                    inputs[k] = v;
                    k += 1;
                }
                return Ok(Some(inputs));
            } else if Self::verbosity(host) >= VERBOSE_ON {
                writeln!(
                    self.err,
                    "warning: Invalid number of environment inputs requested for recalculation: {words} specified, {input_size} required."
                )?;
            }
        }
        Ok(None)
    }

    /// `CommandTrace`: trace every genotype through the test CPU.
    pub(crate) fn command_trace(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut sg = 0;
        let dir_word = su::pop_word(&mut cur_string);
        let directory = pop_directory(&dir_word, "archive/")?;
        let mut first_arg = su::pop_word(&mut cur_string);
        if su::is_substring(&first_arg, "sg=", 0) {
            su::pop(&mut first_arg, '=');
            sg = su::as_int(&first_arg);
            if sg < 0 || sg >= host.num_state_grids() {
                writeln!(self.err, "warning: invalid state grid selection")?;
                return Ok(());
            }
            first_arg = su::pop_word(&mut cur_string);
        }
        let use_resources = if !first_arg.is_empty() {
            su::as_int(&first_arg)
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

        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Tracing batch {}", self.cur_batch)?;
        } else {
            writeln!(self.out, "Tracing organisms.")?;
        }

        let bi = self.batch_index(self.cur_batch)?;
        let opts = CpuTestOptions {
            use_resources,
            update,
            use_random_inputs,
            manual_inputs: manual,
            resources: self.resources.clone(),
            resource_time_spent_offset: self.resource_time_spent_offset,
            state_grid: sg,
        };
        let mut items = Vec::new();
        for g in &self.batches[bi].list {
            if g.genome.representation().size() == 0 {
                break;
            }
            items.push((g.genome.clone(), format!("{directory}{}.trace", g.name)));
        }
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        let out = &mut self.out;
        crate::cpu::trace_genomes(host, &items, &opts, &mut |filename| {
            if verbose {
                writeln!(out, "Tracing {filename}")?;
            }
            Ok(())
        })
    }

    /// `CommandPrintTasks` / `CommandPrintTasksQuality`.
    pub(crate) fn command_print_tasks(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
        quality: bool,
    ) -> Result<()> {
        let verbose = Self::verbosity(host) >= VERBOSE_ON;
        match (quality, verbose) {
            (false, true) => writeln!(self.out, "Printing tasks in batch {}", self.cur_batch)?,
            (false, false) => writeln!(self.out, "Printing tasks...")?,
            (true, true) => writeln!(
                self.out,
                "Printing task qualities in batch {}",
                self.cur_batch
            )?,
            (true, false) => writeln!(self.out, "Printing task qualities...")?,
        }
        let mut filename = if quality {
            "tasksquality.dat"
        } else {
            "tasks.dat"
        }
        .to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        host.output()
            .static_with_path(&filename)
            .map_err(AnalyzeError::Fatal)?;

        let mut buf = String::new();
        let bi = self.batch_index(self.cur_batch)?;
        for g in &self.batches[bi].list {
            buf.push_str(&format!("{} ", g.id_num));
            for i in 0..g.task_counts.len() {
                if quality {
                    let q = g.task_qualities.get(i).ok_or_else(|| {
                        AnalyzeError::fatal(
                            "PrintTasksQuality: task_qualities shorter than task_counts",
                        )
                    })?;
                    buf.push_str(&fmt_double(*q));
                } else {
                    buf.push_str(&g.task_counts[i].to_string());
                }
                buf.push(' ');
            }
            buf.push('\n');
        }
        write_static(host, &filename, &buf)
    }

    /// `CommandDetail`.
    pub(crate) fn command_detail(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Detailing batch {}", self.cur_batch)?;
        } else {
            writeln!(self.out, "Detailing...")?;
        }
        let bi = self.batch_index(self.cur_batch)?;
        if self.batches[bi].list.is_empty() {
            return Ok(());
        }
        let mut filename = "detail.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let cmds = load_command_list_str(&cur_string, None);
        let html = is_html(&filename);

        if filename == "cout" {
            let mut head = String::new();
            detail_header(&mut head, &self.batches[bi], &cmds, host, html, -1)?;
            self.out.write_all(head.as_bytes())?;
            let mut sink = Sink {
                console: &mut *self.out,
                file: None,
            };
            detail_body(
                &mut sink,
                &mut self.batches[bi],
                &cmds,
                host,
                html,
                -1,
                i32::MAX,
            )?;
        } else {
            let mut df = host
                .output()
                .create_with_path(&filename)
                .map_err(AnalyzeError::Fatal)?;
            let mut text = String::new();
            detail_header(&mut text, &self.batches[bi], &cmds, host, html, -1)?;
            let mut sink = Sink {
                console: &mut *self.out,
                file: Some(text),
            };
            detail_body(
                &mut sink,
                &mut self.batches[bi],
                &cmds,
                host,
                html,
                -1,
                i32::MAX,
            )?;
            let text = sink.file.take().unwrap_or_default();
            df.ofstream().write_all(text.as_bytes())?;
            df.flush();
        }
        Ok(())
    }

    /// `CommandDetailTimeline`.
    pub(crate) fn command_detail_timeline(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Detailing batch {} based on time", self.cur_batch)?;
        } else {
            writeln!(self.out, "Detailing...")?;
        }
        let mut filename = "detail_timeline.dat".to_string();
        let mut time_step = 100;
        let mut max_time = 100000;
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        if !cur_string.is_empty() {
            time_step = su::as_int(&su::pop_word(&mut cur_string));
        }
        if !cur_string.is_empty() {
            max_time = su::as_int(&su::pop_word(&mut cur_string));
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "  Time step = {time_step}")?;
            writeln!(self.out, "  Max time = {max_time}")?;
        }
        let cmds = load_command_list_str(&cur_string, None);
        let html = is_html(&filename);
        let bi = self.batch_index(self.cur_batch)?;

        let file = if filename == "cout" {
            None
        } else {
            host.output()
                .static_with_path(&filename)
                .map_err(AnalyzeError::Fatal)?;
            Some(String::new())
        };
        let mut head = String::new();
        detail_header(&mut head, &self.batches[bi], &cmds, host, html, time_step)?;
        let mut sink = Sink {
            console: &mut *self.out,
            file,
        };
        sink.data(&head)?;
        detail_body(
            &mut sink,
            &mut self.batches[bi],
            &cmds,
            host,
            html,
            time_step,
            max_time,
        )?;
        if let Some(text) = sink.file.take() {
            write_static(host, &filename, &text)?;
        }
        Ok(())
    }

    /// `CommandDetailBatches`: one line per non-empty batch.
    pub(crate) fn command_detail_batches(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        let mut keyword = "num_cpus".to_string();
        let mut filename = "detail_batch.dat".to_string();
        if !cur_string.is_empty() {
            keyword = su::pop_word(&mut cur_string);
        }
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Detailing batches for {keyword}")?;
        } else {
            writeln!(self.out, "Detailing Batches...")?;
        }
        let Ok(cmd) = get_data_command(&keyword) else {
            writeln!(self.out, "error: no data entry, unable to detail batches")?;
            return Ok(());
        };
        let html = is_html(&filename);
        host.output()
            .static_with_path(&filename)
            .map_err(AnalyzeError::Fatal)?;
        let bi = self.batch_index(self.cur_batch)?;
        let desc = cmd.desc(self.batches[bi].list.first(), host)?;
        let colors = Colors::of(host);

        let mut fp = String::new();
        if !html {
            fp.push_str(&format!(
                "#filetype batch_data\n#format batch_id {keyword}\n\n# Legend:\n#  Column 1 = Batch ID\n#  Remaining entries: {desc}\n\n"
            ));
        } else {
            fp.push_str(HTML_HEAD);
            fp.push_str(&format!(
                "<h1 align=center> Distribution of {desc}\n\n<center>\n<table border=1 cellpadding=2>\n<tr><th bgcolor=\"#AAAAFF\">{desc}</tr>\n"
            ));
        }
        for (i, b) in self.batches.iter_mut().enumerate() {
            if b.list.is_empty() {
                continue;
            }
            if html {
                fp.push_str("<tr><td>");
            }
            fp.push_str(&format!("{i} "));
            for g in b.list.iter_mut() {
                if html {
                    fp.push_str("<td>");
                    let v = cmd.value(g, host)?;
                    html_print_stat(&v, 0, cmd.html_cell_flags(), cmd.null(), &colors, &mut fp)?;
                } else {
                    fp.push_str(&cmd.value(g, host)?.print());
                    fp.push(' ');
                }
            }
            if html {
                fp.push_str("</tr>");
            }
            fp.push('\n');
        }
        if html {
            fp.push_str("</table>\n</center>\n");
        }
        write_static(host, &filename, &fp)
    }

    /// `CommandDetailAverage`: one line of averages per call; the header is
    /// written only when the file was not open yet.
    pub(crate) fn command_detail_average(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Average detailing batch {}", self.cur_batch)?;
        } else {
            writeln!(self.out, "Detailing...")?;
        }
        let mut filename = "detail.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let cmds = load_command_list_str(&cur_string, None);
        let oid = host.output().output_id_from_path(&filename);
        let file_active = host.output().is_open(&oid);
        host.output()
            .static_with_path(&oid)
            .map_err(AnalyzeError::Fatal)?;

        let bi = self.batch_index(self.cur_batch)?;
        let mut fp = String::new();
        if !file_active {
            detail_header(&mut fp, &self.batches[bi], &cmds, host, false, -1)?;
        }
        // QUIRK: the number of averages is the number of argument words,
        // not the number of recognised entries.
        let nucoutputs = su::count_num_words(&cur_string);
        if cmds.len() > nucoutputs {
            return Err(AnalyzeError::fatal(format!(
                "DETAIL_AVERAGE with {} entries but {nucoutputs} argument words (C++ writes past its sum array)",
                cmds.len()
            )));
        }
        // cDoubleSum: s1 and n.
        let mut sums = vec![(0.0f64, 0.0f64); nucoutputs];
        let batch = &mut self.batches[bi];
        for g in batch.list.iter_mut() {
            for (count, c) in cmds.iter().enumerate() {
                for _ in 0..g.num_cpus {
                    let v = c.value(g, host)?.as_double();
                    sums[count].0 += v;
                    sums[count].1 += 1.0;
                }
            }
        }
        fp.push_str(&format!("{} ", batch.name));
        for (s1, n) in &sums {
            let avg = if *n > 0.0 { s1 / n } else { 0.0 };
            fp.push_str(&fmt_double(avg));
            fp.push(' ');
        }
        fp.push('\n');
        write_static(host, &oid, &fp)
    }

    /// `CommandDetailIndex`: the first genotype of each batch in a range.
    pub(crate) fn command_detail_index(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        writeln!(self.out, "Creating a Detail Index...")?;
        if su::count_num_words(&cur_string) < 3 {
            return Err(
                self.exit_with("Error: must include filename, and min and max batch numbers.")
            );
        }
        let filename = su::pop_word(&mut cur_string);
        let min_batch = su::as_int(&su::pop_word(&mut cur_string));
        let max_batch = su::as_int(&su::pop_word(&mut cur_string));
        if max_batch < min_batch {
            return Err(self.exit_with(&format!(
                "Error: min_batch={min_batch}, max_batch={max_batch}  (incorrect order?)"
            )));
        }
        let cmds = load_command_list_str(&cur_string, None);
        host.output()
            .static_with_path(&filename)
            .map_err(AnalyzeError::Fatal)?;
        let bi = self.batch_index(self.cur_batch)?;
        let html = is_html(&filename);

        let mut fp = String::new();
        {
            let first = self.batches[bi].list.first();
            if !html {
                fp.push_str("#filetype genotype_data\n#format ");
                for c in &cmds {
                    fp.push_str(c.name());
                    fp.push(' ');
                }
                fp.push_str("\n\n# Legend:\n# 1: Batch Name\n");
                let mut count = 1;
                for c in &cmds {
                    count += 1;
                    fp.push_str(&format!("# {count}: {}\n", c.desc(first, host)?));
                }
                fp.push('\n');
            } else {
                fp.push_str(HTML_HEAD);
                fp.push_str("<h1 align=center>Batch Index\n\n<center>\n<table border=1 cellpadding=2><tr>\n");
                fp.push_str("<th bgcolor=\"#AAAAFF\">Batch ");
                for c in &cmds {
                    fp.push_str(&format!(
                        "<th bgcolor=\"#AAAAFF\">{} ",
                        c.desc(first, host)?
                    ));
                }
                fp.push_str("</tr>\n");
            }
        }
        let mut batch_id = min_batch;
        while batch_id <= max_batch {
            let b = self.batch_index(batch_id)?;
            let batch = &mut self.batches[b];
            if let Some(g) = batch.list.first_mut() {
                let batch_name = batch.name.clone();
                if html {
                    fp.push_str(&format!(
                        "<tr><th><a href=lineage.{batch_name}.html>{batch_name}</a> "
                    ));
                } else {
                    fp.push_str(&format!("{batch_name} "));
                }
                for c in &cmds {
                    let v = c.value(g, host)?.print();
                    if html {
                        fp.push_str(&format!(
                            "<td align=center><a href=\"{}.{batch_name}.png\">{v}</a> ",
                            c.name()
                        ));
                    } else {
                        fp.push_str(&format!("{v} "));
                    }
                }
                if html {
                    fp.push_str("</tr>");
                }
                fp.push('\n');
            }
            batch_id += 1;
        }
        if html {
            fp.push_str("</table>\n</center>\n");
        }
        write_static(host, &filename, &fp)
    }

    /// `CommandHistogram`.
    pub(crate) fn command_histogram(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Histogram batch {}", self.cur_batch)?;
        } else {
            writeln!(self.out, "Histograming...")?;
        }
        let mut filename = "histogram.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let cmds = load_command_list_str(&cur_string, None);
        let html = is_html(&filename);
        let to_console = filename == "cout";
        if !to_console {
            host.output()
                .static_with_path(&filename)
                .map_err(AnalyzeError::Fatal)?;
        }
        let bi = self.batch_index(self.cur_batch)?;
        let mut fp = String::new();

        // CommandHistogram_Header
        {
            let first = self.batches[bi].list.first();
            if !html {
                fp.push_str("#filetype histogram_data\n#format ");
                for c in &cmds {
                    fp.push_str(c.name());
                    fp.push(' ');
                }
                fp.push_str("\n\n# Histograms:\n");
                for (i, c) in cmds.iter().enumerate() {
                    fp.push_str(&format!("# {}: {}\n", i + 1, c.desc(first, host)?));
                }
                fp.push('\n');
            } else {
                fp.push_str(HTML_HEAD);
                fp.push_str(&format!(
                    "<h1 align=center>Histograms for {}</h1>\n\n<center>\n<table border=1 cellpadding=2><tr>\n",
                    self.batches[bi].name
                ));
                for c in &cmds {
                    fp.push_str(&format!(
                        "<tr><th bgcolor=\"#AAAAFF\"><a href=\"#{}\">{}</a></tr>",
                        c.name(),
                        c.desc(first, host)?
                    ));
                }
                fp.push_str("</tr></table>\n");
            }
        }

        // CommandHistogram_Body
        for c in &cmds {
            let desc = c.desc(self.batches[bi].list.first(), host)?;
            if !html {
                fp.push_str(&format!("# --- {desc} ---\n"));
            } else {
                fp.push_str(&format!(
                    "<table cellpadding=3>\n<tr><th colspan=3><a name=\"{}\">{desc}</th></tr>\n",
                    c.name()
                ));
            }
            // Apto::Map<Apto::String, int>: first-insertion order + counts.
            let mut keys: Vec<String> = Vec::new();
            let mut counts: Vec<i32> = Vec::new();
            for g in self.batches[bi].list.iter_mut() {
                let name = c.value(g, host)?.as_string();
                let ncpu = g.num_cpus;
                match keys.iter().position(|k| *k == name) {
                    Some(p) => counts[p] = counts[p].wrapping_add(ncpu),
                    None => {
                        keys.push(name);
                        counts.push(ncpu);
                    }
                }
            }
            let key_refs: Vec<&str> = keys.iter().map(|k| k.as_str()).collect();
            let order: Vec<usize> = apto_map_key_order(&key_refs)
                .into_iter()
                .map(|k| keys.iter().position(|x| x == k).expect("key"))
                .collect();
            let mut max_count = 0;
            let mut max_name_width = 0usize;
            let mut max_count_width = 0usize;
            for &p in &order {
                let cur_count = counts[p];
                max_count = max_count.max(cur_count);
                max_name_width = max_name_width.max(keys[p].len());
                max_count_width = max_count_width.max(cur_count.to_string().len());
            }
            let max_stars = 75 - max_name_width as i32 - max_count_width as i32;
            for &p in &order {
                let cur_count = counts[p];
                if cur_count == 0 {
                    continue;
                }
                if max_count == 0 {
                    return Err(AnalyzeError::fatal(
                        "HISTOGRAM: division by a zero maximum count (C++ SIGFPE)",
                    ));
                }
                let num_stars = cur_count.wrapping_mul(max_stars) / max_count;
                let stars = "#".repeat(num_stars.max(0) as usize);
                if !html {
                    fp.push_str(&format!(
                        "{:>nw$}  {:>cw$}  {stars}\n",
                        keys[p],
                        cur_count,
                        nw = max_name_width,
                        cw = max_count_width
                    ));
                } else {
                    fp.push_str(&format!(
                        "<tr><td>{}<td>{cur_count}<td>{stars}</tr>\n",
                        keys[p]
                    ));
                }
            }
            if !html {
                fp.push('\n');
            } else {
                fp.push_str("</table><br><br>\n\n");
            }
        }
        if html {
            fp.push_str("</table>\n</center>\n");
        }
        if to_console {
            self.out.write_all(fp.as_bytes())?;
            Ok(())
        } else {
            write_static(host, &filename, &fp)
        }
    }

    /// `CommandPrintPhenotypes`.
    ///
    /// The C++ groups genotypes in an `Apto::Map<cBitArray, p_stats>`
    /// (`HashBTree<.., 23>`) and sorts its values in map order. With the
    /// `HashKey<cBitArray>` specialisation (cBitArray.h: sum of `i*i` over
    /// the set bits, modulo 23) and no removals, map order is bucket by
    /// bucket, first insertion first within a bucket; rows that tie on
    /// organism and genotype count keep that order, because Apto `QSort`
    /// always falls back to its stable insertion sort.
    pub(crate) fn command_print_phenotypes(
        &mut self,
        host: &mut dyn AnalyzeHost,
        mut cur_string: String,
    ) -> Result<()> {
        if Self::verbosity(host) >= VERBOSE_ON {
            writeln!(self.out, "Printing phenotypes in batch {}", self.cur_batch)?;
        } else {
            writeln!(self.out, "Printing phenotypes...")?;
        }
        let mut filename = "phenotype.dat".to_string();
        if !cur_string.is_empty() {
            filename = su::pop_word(&mut cur_string);
        }
        let mut print_ttc = false;
        let mut print_ttpc = false;
        while !cur_string.is_empty() {
            let flag = su::pop_word(&mut cur_string);
            if flag == "total_task_count" {
                print_ttc = true;
            } else if flag == "total_task_performance_count" {
                print_ttpc = true;
            }
        }
        let bi = self.batch_index(self.cur_batch)?;
        let list = &self.batches[bi].list;
        if list.is_empty() {
            return Ok(());
        }
        let num_tasks = list[0].num_tasks();

        struct PStats {
            phen_id: Vec<bool>,
            cpu_count: i32,
            genotype_count: i32,
            total_length: f64,
            total_gest: f64,
            total_task_count: i32,
            total_task_performance_count: i32,
        }
        let mut table: Vec<PStats> = Vec::new();
        for g in list {
            let mut phen_id = vec![false; num_tasks as usize + 1];
            phen_id[0] = g.viable;
            for i in 0..num_tasks {
                if g.task_count(i)? > 0 {
                    phen_id[i as usize + 1] = true;
                }
            }
            let (mut ttc, mut ttpc) = (0i32, 0i32);
            if print_ttc || print_ttpc {
                for i in 0..num_tasks {
                    let c = g.task_count(i)?;
                    ttc += (c > 0) as i32;
                    ttpc = ttpc.wrapping_add(c);
                }
            }
            let len_add = g.num_cpus.wrapping_mul(g.length) as f64;
            let gest_add = g.num_cpus.wrapping_mul(g.gest_time) as f64;
            match table.iter_mut().find(|s| s.phen_id == phen_id) {
                Some(s) => {
                    s.cpu_count = s.cpu_count.wrapping_add(g.num_cpus);
                    s.genotype_count += 1;
                    s.total_length += len_add;
                    s.total_gest += gest_add;
                    s.total_task_count = s.total_task_count.wrapping_add(ttc);
                    s.total_task_performance_count =
                        s.total_task_performance_count.wrapping_add(ttpc);
                }
                None => table.push(PStats {
                    phen_id,
                    cpu_count: g.num_cpus,
                    genotype_count: 1,
                    total_length: len_add,
                    total_gest: gest_add,
                    total_task_count: ttc,
                    total_task_performance_count: ttpc,
                }),
            }
        }

        let mut df = host
            .output()
            .create_with_path(&filename)
            .map_err(AnalyzeError::Fatal)?;
        let mut fp = String::from(
            "# 1: Number of organisms of this phenotype\n\
             # 2: Number of genotypes of this phenotye\n\
             # 3: Average Genome Length\n\
             # 4: Average Gestation Time\n\
             # 5: Viability of Phenotype\n",
        );
        if print_ttc && print_ttpc {
            fp.push_str(
                "# 6: Total # of different tasks performed by this phenotype\n\
                 # 7: Average # of tasks performed by this phenotype\n\
                 # 8+: Tasks performed in this phenotype\n",
            );
        } else if print_ttc {
            fp.push_str(
                "# 6: Total # of different tasks performed by this phenotype\n\
                 # 7+: Tasks performed in this phenotype\n",
            );
        } else if print_ttpc {
            fp.push_str(
                "# 6: Total # of tasks performed by this phenotype\n\
                 # 7+: Tasks performed in this phenotype\n",
            );
        } else {
            fp.push_str("# 6+: Tasks performed in this phenotype\n");
        }
        fp.push('\n');

        // Map order: HashKey<cBitArray, 23> bucket, insertion order within
        // a bucket (sort_by_key is stable).
        table.sort_by_key(|s| {
            let sum = s
                .phen_id
                .iter()
                .enumerate()
                .filter(|&(_, &on)| on)
                .fold(0u32, |h, (i, _)| {
                    h.wrapping_add((i as u32).wrapping_mul(i as u32))
                });
            sum % 23
        });
        // PStatsComparator: cpu_count descending, then genotype_count
        // descending; stable.
        table.sort_by(|a, b| {
            b.cpu_count
                .cmp(&a.cpu_count)
                .then(b.genotype_count.cmp(&a.genotype_count))
        });
        for s in &table {
            fp.push_str(&format!(
                "{} {} {} {} {}  ",
                s.cpu_count,
                s.genotype_count,
                fmt_double(s.total_length / s.cpu_count as f64),
                fmt_double(s.total_gest / s.cpu_count as f64),
                s.phen_id[0] as i32
            ));
            if print_ttc {
                fp.push_str(&format!("{} ", s.total_task_count / s.genotype_count));
            }
            if print_ttpc {
                fp.push_str(&format!(
                    "{} ",
                    s.total_task_performance_count / s.genotype_count
                ));
            }
            for j in 1..=num_tasks as usize {
                fp.push_str(&format!("{} ", s.phen_id[j] as i32));
            }
            fp.push('\n');
        }
        df.ofstream().write_all(fp.as_bytes())?;
        df.flush();
        Ok(())
    }
}

/// Append `text` to the static (manager-owned) file at `path`.
pub(crate) fn write_static(host: &mut dyn AnalyzeHost, path: &str, text: &str) -> Result<()> {
    let df = host
        .output()
        .static_with_path(path)
        .map_err(AnalyzeError::Fatal)?;
    df.ofstream().write_all(text.as_bytes())?;
    df.flush();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_flex_stat_cases() {
        use compare_result::*;
        let i = FlexVar::Int;
        assert_eq!(compare_flex_stat(&i(3), &i(3), flex_compare::MAX), SAME);
        assert_eq!(compare_flex_stat(&i(4), &i(3), flex_compare::MAX), POS);
        assert_eq!(compare_flex_stat(&i(2), &i(3), flex_compare::MAX), NEG);
        assert_eq!(compare_flex_stat(&i(2), &i(3), flex_compare::MIN), POS);
        assert_eq!(compare_flex_stat(&i(0), &i(3), flex_compare::MAX2), OFF);
        assert_eq!(compare_flex_stat(&i(3), &i(0), flex_compare::MAX2), ON);
        assert_eq!(compare_flex_stat(&i(3), &i(5), flex_compare::DIFF2), DIFF);
        assert_eq!(compare_flex_stat(&i(3), &i(5), flex_compare::NONE), SAME);
        // FILTER compares against the string test value.
        let s = FlexVar::Str("10.0".into());
        assert_eq!(
            compare_flex_stat(&FlexVar::Double(12.5), &s, flex_compare::MAX),
            POS
        );
    }

    #[test]
    fn directories_and_extensions() {
        assert_eq!(pop_directory("", "archive/").unwrap(), "archive/");
        assert_eq!(
            pop_directory("alt_archive", "archive/").unwrap(),
            "alt_archive/"
        );
        assert_eq!(
            pop_directory("third_archive/ dominant.gen", "x").unwrap(),
            "third_archive/"
        );
        assert!(is_html("a.b.html"));
        assert!(is_html("html"));
        assert!(!is_html("detail.dat"));
    }
}
