//! The mini, micro and navigation traces of `cHardwareBase`
//! (avida-core/source/cpu/cHardwareBase.{h,cc} at 47f13dadb:
//! `SetMiniTrace`, `SetMicroTrace`, `DeleteMiniTrace`, `PrintMicroTrace`,
//! `GetNavTraceLoc` / `Facing` / `Update`), the mini-trace printer
//! (`cHardwareStatusPrinter` with `m_minitracer` set,
//! cHardwareStatusPrinter.cc) and the experimental hardware's
//! `SetupMiniTraceFileHeader` (cHardwareExperimental.cc).
//!
//! # Who acts on the traces
//! `cPopulation` switches an organism's traces on (`SetupMiniTrace`, at
//! birth) and off (`UpdateQs`, at death and at a splitting divide) by
//! reaching into the organism's hardware. The population code of this port
//! cannot reach the hardware of the running organism (ARCHITECTURE.md,
//! rule 3), yet a dividing parent is exactly that: `ActivateOffspring` ends
//! the parent's mini trace, prints its micro trace and may start a new mini
//! trace for it, all while the parent's divide instruction is executing.
//! So every hardware type hands the trace half of its `cHardwareBase` to the
//! population for the duration of the divide ([`divide_lending_traces`],
//! [`LentTraces`]); the population works on the lent state exactly as the
//! C++ works on the hardware, and the hardware takes it back afterwards.
//! The population lends the traces of an organism that is not running the
//! same way, so all trace bookkeeping is written against [`LentTraces`].

use crate::hardware::{Hardware, HardwareBase};
use crate::org_interface::OrgInterface;
use crate::organism::OrgState;
use crate::tracer::{HardwareTracer, HardwareTracerPtr};
use avida_apto::DataFile;
use avida_genome::Genome;
use std::sync::{Arc, Mutex};

/// The trace members of `cHardwareBase`, moved out of a hardware while the
/// population works on them.
#[derive(Default)]
pub struct LentTraces {
    /// `m_tracer`
    pub tracer: Option<HardwareTracerPtr>,
    /// `m_minitrace`
    pub minitrace: bool,
    /// `m_microtrace`
    pub microtrace: bool,
    /// `m_reprotrace`
    pub reprotrace: bool,
    /// `m_topnavtrace`
    pub topnavtrace: bool,
    /// `m_microtracer`
    pub microtracer: Vec<u8>,
    /// `m_navtraceloc`
    pub navtraceloc: Vec<i32>,
    /// `m_navtracefacing`
    pub navtracefacing: Vec<i32>,
    /// `m_navtraceupdate`
    pub navtraceupdate: Vec<i32>,
    /// What `m_tracer->PrintSuccess(GetOrganism(), 1)` writes for this
    /// hardware type (`PrintMiniTraceSuccess(fp, 1)`): the line
    /// `DeleteMiniTrace(print_reacs, repro_split = true)` ends a trace with.
    pub split_success: String,
}

impl std::fmt::Debug for LentTraces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LentTraces")
            .field("tracer", &self.tracer.is_some())
            .field("minitrace", &self.minitrace)
            .field("microtrace", &self.microtrace)
            .field("reprotrace", &self.reprotrace)
            .field("topnavtrace", &self.topnavtrace)
            .field("microtracer", &self.microtracer.len())
            .field("navtrace", &self.navtraceloc.len())
            .finish()
    }
}

impl LentTraces {
    /// Move the trace members out of `base`. `split_success` is the
    /// hardware's `PrintMiniTraceSuccess(fp, 1)` text.
    pub fn lend(base: &mut HardwareBase, split_success: String) -> Self {
        LentTraces {
            tracer: base.tracer.take(),
            minitrace: std::mem::take(&mut base.minitrace),
            microtrace: std::mem::take(&mut base.microtrace),
            reprotrace: std::mem::take(&mut base.reprotrace),
            topnavtrace: std::mem::take(&mut base.topnavtrace),
            microtracer: std::mem::take(&mut base.microtracer),
            navtraceloc: std::mem::take(&mut base.navtraceloc),
            navtracefacing: std::mem::take(&mut base.navtracefacing),
            navtraceupdate: std::mem::take(&mut base.navtraceupdate),
            split_success,
        }
    }

    /// Put the trace members back into `base`.
    pub fn restore(self, base: &mut HardwareBase) {
        base.tracer = self.tracer;
        base.minitrace = self.minitrace;
        base.microtrace = self.microtrace;
        base.reprotrace = self.reprotrace;
        base.topnavtrace = self.topnavtrace;
        base.microtracer = self.microtracer;
        base.navtraceloc = self.navtraceloc;
        base.navtracefacing = self.navtracefacing;
        base.navtraceupdate = self.navtraceupdate;
    }

    /// Whether any trace is switched on (nothing for `UpdateQs` to do
    /// otherwise).
    pub fn any(&self) -> bool {
        self.minitrace || self.microtrace || self.reprotrace || self.topnavtrace
    }

    /// `SetMiniTrace(filename)` with the printer already opened on the file.
    pub fn set_mini_trace(&mut self, tracer: HardwareTracerPtr) {
        self.tracer = Some(tracer);
        self.minitrace = true;
    }

    /// `SetMicroTrace()`
    pub fn set_micro_trace(&mut self) {
        self.microtrace = true;
    }

    /// The hardware half of `DeleteMiniTrace(print_reacs, repro_split)`
    /// (the caller prints the reactions first): the success line when the
    /// parent splits, then the tracer is dropped. Does nothing without a
    /// mini trace.
    pub fn delete_mini_trace(&mut self, repro_split: bool) {
        if !self.minitrace {
            return;
        }
        if repro_split {
            if let Some(t) = &self.tracer {
                let mut guard = t.lock().unwrap_or_else(|p| p.into_inner());
                guard.print_success(&self.split_success);
            }
        }
        self.tracer = None;
        self.minitrace = false;
    }
}

/// `m_interface->Divide(ctx, m_organism, offspring)` (the tail of
/// `cOrganism::ActivateDivide`) with the trace members of `hw` lent to the
/// population for the duration. The rest of `hw` stays readable by the
/// population, which reads the parent's hardware in `ActivateOffspring`
/// (`InheritState` with `EPIGENETIC_METHOD`, `GetMemory` for
/// `PRECALC_PHENOTYPE` with `RESET_INPUTS_ON_DIVIDE`). `split_success` is
/// the hardware's `PrintMiniTraceSuccess(fp, 1)` text.
pub fn divide_lending_traces<H: Hardware>(
    hw: &mut H,
    split_success: String,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    offspring: &Genome,
) -> bool {
    let mut lent = LentTraces::lend(hw.base_mut(), split_success);
    let parent_alive = world.divide_with_traces(org, &*hw, offspring, &mut lent);
    lent.restore(hw.base_mut());
    parent_alive
}

/// `cHardwareStatusPrinter(world, filename, minitracer = true)`: the printer
/// `SetMiniTrace` installs. It keeps the identity of the traced organism the
/// header needs (`SetupMiniTraceFileHeader(df, gen_id, genotype)`, called by
/// `TraceHardware` before the first line).
pub struct MiniTracePrinter {
    file: DataFile,
    gen_id: i32,
    genotype: String,
    /// The header was written by the call now in progress (`in_setup`).
    setup_now: bool,
}

impl MiniTracePrinter {
    /// A printer on `file` (opened by `Output::File::CreateWithPath`) for an
    /// organism of genotype `gen_id`, whose `"genotype"` property is
    /// `genotype`.
    pub fn new(file: DataFile, gen_id: i32, genotype: String) -> Self {
        MiniTracePrinter {
            file,
            gen_id,
            genotype,
            setup_now: false,
        }
    }

    /// A shared pointer to the printer.
    pub fn into_ptr(self) -> HardwareTracerPtr {
        Arc::new(Mutex::new(self))
    }

    fn write(&mut self, s: &str, flush: bool) {
        let fp = self.file.ofstream();
        // C++ streams fail silently.
        let _ = fp.write_all(s.as_bytes());
        if flush {
            let _ = fp.flush();
        }
    }
}

impl HardwareTracer for MiniTracePrinter {
    /// `TraceHardware(ctx, hardware, bonus, minitrace, exec_success)` with
    /// `m_minitracer` true. A hardware type whose
    /// `SetupMiniTraceFileHeader` writes nothing never completes the header,
    /// so each of its mini calls is "in setup", as in the C++.
    fn trace_hardware(
        &mut self,
        _status: &mut dyn FnMut() -> String,
        mini_status: &mut dyn FnMut() -> String,
        mini_success: &mut dyn FnMut(i32) -> String,
        _bonus: bool,
        mini: bool,
        exec_success: i32,
    ) {
        let setup_now = std::mem::take(&mut self.setup_now);
        let in_setup = mini && (setup_now || !self.file.header_done());
        // `!m_minitracer && !minitrace` (PrintStatus) never holds here.
        if (exec_success == -2 || in_setup) && mini {
            let s = mini_status();
            self.write(&s, false);
        }
        if exec_success != -2 && (in_setup || mini) {
            // `PrintMiniTraceSuccess` ends the line with `endl` and flushes.
            let s = mini_success(exec_success);
            self.write(&s, true);
        }
    }

    fn mini_header_due(&self) -> Option<(i32, String)> {
        if self.file.header_done() {
            None
        } else {
            Some((self.gen_id, self.genotype.clone()))
        }
    }

    fn write_mini_header(&mut self, comments: &[String]) {
        self.file.write_time_stamp();
        for c in comments {
            self.file.write_comment(c);
        }
        self.file.endl();
        self.setup_now = true;
    }

    fn print_success(&mut self, mini_success: &str) {
        self.write(mini_success, true);
    }

    fn trace_test_cpu(&mut self, final_status: &str) {
        self.write(final_status, false);
    }
}

/// The comments of `cHardwareExperimental::SetupMiniTraceFileHeader(df,
/// gen_id, genotype)`, after its `WriteTimeStamp()`: `update` is the
/// current update, `genome_size` the size of `m_organism->GetGenome()`.
pub fn experimental_header(
    update: i32,
    org_id: i32,
    gen_id: i32,
    genotype: &str,
    genome_size: i32,
) -> Vec<String> {
    let mut v = vec![
        format!("Update Born: {update}"),
        format!("Org ID: {org_id}"),
        format!("Genotype ID: {gen_id}"),
        format!("Genotype: {genotype}"),
        format!("Genome Length: {genome_size}"),
    ];
    v.extend(
        [
            " ",
            "Exec Stats Columns:",
            "CPU Cycle",
            "Current Update",
            "Register Contents (CPU Cycle Origin of Contents)",
            "Current Thread",
            "IP Position",
            "RH Position",
            "WH Position",
            "FH Position",
            "CPU Cycle of Last Output",
            "Current Merit",
            "Current Bonus",
            "Forager Type",
            "Group ID (opinion)",
            "Current Cell",
            "Avatar Cell",
            "Faced Direction",
            "Faced Cell Occupied?",
            "Faced Cell Has Hill?",
            "Faced Cell Has Wall?",
            "Queued Instruction",
            "Trailing NOPs",
            "Did Queued Instruction Execute (-1=no, paying cpu costs; 0=failed; 1=yes)",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex as StdMutex;

    /// A sink the test can read back.
    #[derive(Clone, Default)]
    struct Shared(Arc<StdMutex<Vec<u8>>>);
    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn text(s: &Shared) -> String {
        String::from_utf8(s.0.lock().unwrap().clone()).unwrap()
    }

    /// Drops the time stamp line.
    fn no_stamp(t: &str) -> String {
        t.lines()
            .filter(|l| !(l.len() == 26 && l.starts_with("# ") && l.as_bytes()[5] == b' '))
            .map(|l| format!("{l}\n"))
            .collect()
    }

    fn call(p: &mut MiniTracePrinter, mini: bool, exec_success: i32) {
        p.trace_hardware(
            &mut || "FULL\n".to_string(),
            &mut || "status ".to_string(),
            &mut |e| format!("{e}\n"),
            false,
            mini,
            exec_success,
        );
    }

    #[test]
    fn header_then_status_and_success_lines() {
        let sink = Shared::default();
        let file = DataFile::from_writer("t.trc", Box::new(sink.clone()));
        let mut p = MiniTracePrinter::new(file, 7, String::new());
        // The non-mini call of `SingleProcess` prints nothing.
        call(&mut p, false, -2);
        assert_eq!(p.mini_header_due(), Some((7, String::new())));
        p.write_mini_header(&["A".to_string(), "B".to_string()]);
        call(&mut p, true, -2);
        call(&mut p, true, 1);
        assert_eq!(p.mini_header_due(), None);
        call(&mut p, false, -2);
        call(&mut p, true, -2);
        call(&mut p, true, -1);
        assert_eq!(
            no_stamp(&text(&sink)),
            "# A\n# B\n\n\nstatus 1\nstatus -1\n"
        );
    }

    #[test]
    fn header_set_up_after_the_instruction_prints_status_and_success() {
        // `TestForMiniTrace(parent)` during a splitting divide: the first
        // mini call is the one after the instruction.
        let sink = Shared::default();
        let file = DataFile::from_writer("t.trc", Box::new(sink.clone()));
        let mut p = MiniTracePrinter::new(file, 1, String::new());
        p.write_mini_header(&[]);
        call(&mut p, true, 1);
        assert_eq!(no_stamp(&text(&sink)), "\n\nstatus 1\n");
    }

    #[test]
    fn lend_and_restore_round_trip() {
        let cfg = avida_config::config::Config::default();
        let lib = Arc::new(avida_genome::InstLib::new(Vec::new(), Vec::new(), 0, 0));
        let is = Arc::new(avida_genome::InstSet::new("t", 3, lib, 10, 1));
        let mut base = HardwareBase::new(&cfg, is);
        base.microtrace = true;
        base.microtracer = b"abc".to_vec();
        let mut lent = LentTraces::lend(&mut base, "1\n".to_string());
        assert!(!base.microtrace && base.microtracer.is_empty());
        assert!(lent.any());
        let sink = Shared::default();
        let file = DataFile::from_writer("t.trc", Box::new(sink.clone()));
        lent.set_mini_trace(MiniTracePrinter::new(file, 1, String::new()).into_ptr());
        lent.delete_mini_trace(true);
        assert_eq!(text(&sink), "1\n");
        assert!(lent.tracer.is_none() && !lent.minitrace);
        lent.restore(&mut base);
        assert!(base.microtrace && base.microtracer == b"abc");
    }
}
