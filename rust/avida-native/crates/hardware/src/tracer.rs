//! Ports of `cHardwareTracer` (avida-core/source/cpu/cHardwareTracer.h) and
//! `cHardwareStatusPrinter` (cHardwareStatusPrinter.{h,cc}).
//!
//! The C++ tracer receives the hardware and calls back into the organism
//! (`PrintStatus`, `PrintMiniTraceStatus`, `PrintFinalStatus`). Here the
//! hardware renders those texts (it holds the organism and the world at the
//! call site) and hands them over; a tracer that does not need a text never
//! forces it to be built, because the texts are passed as closures.

use std::io::Write;
use std::sync::{Arc, Mutex};

/// `cHardwareTracer`.
pub trait HardwareTracer: Send {
    /// `TraceHardware(ctx, hardware, bonus, mini, exec_success)`.
    ///
    /// `status` renders `cOrganism::PrintStatus`, `mini_status`
    /// `cOrganism::PrintMiniTraceStatus`, `mini_success`
    /// `cOrganism::PrintMiniTraceSuccess(exec_success)`.
    fn trace_hardware(
        &mut self,
        status: &mut dyn FnMut() -> String,
        mini_status: &mut dyn FnMut() -> String,
        mini_success: &mut dyn FnMut(i32) -> String,
        bonus: bool,
        mini: bool,
        exec_success: i32,
    );

    /// `PrintSuccess(organism, exec_success)`.
    fn print_success(&mut self, mini_success: &str);

    /// `TraceTestCPU(time_used, time_allocated, organism)`; `final_status`
    /// is `cOrganism::PrintFinalStatus(fp, time_used, time_allocated)`.
    fn trace_test_cpu(&mut self, final_status: &str);

    /// `m_minitracer && !m_file->HeaderDone()`: the next mini call of
    /// `TraceHardware` starts with `hardware.SetupMiniTraceFileHeader(df,
    /// gen_id, genotype)`. Returns `(gen_id, genotype)` then. A hardware
    /// type whose header is not empty asks before each mini call and
    /// renders the header for [`HardwareTracer::write_mini_header`].
    fn mini_header_due(&self) -> Option<(i32, String)> {
        None
    }

    /// The `SetupMiniTraceFileHeader` calls: `WriteTimeStamp()`, a
    /// `WriteComment` per entry of `comments`, `Endl()`. The mini call that
    /// follows counts as the set-up call (`in_setup`).
    fn write_mini_header(&mut self, _comments: &[String]) {}
}

/// `HardwareTracerPtr`: shared, as the C++ reference-counted pointer.
pub type HardwareTracerPtr = Arc<Mutex<dyn HardwareTracer>>;

/// `cHardwareStatusPrinter`: writes organism status to a stream (the C++
/// `Avida::Output::File`).
pub struct HardwareStatusPrinter {
    out: Box<dyn Write + Send>,
    minitracer: bool,
    header_done: bool,
    io_error: Option<std::io::Error>,
}

impl HardwareStatusPrinter {
    /// `cHardwareStatusPrinter(world, filename, minitracer)` over an
    /// already opened stream.
    pub fn new(out: Box<dyn Write + Send>, minitracer: bool) -> Self {
        HardwareStatusPrinter {
            out,
            minitracer,
            header_done: false,
            io_error: None,
        }
    }

    /// Open `path` (creating parent directories, as
    /// `Output::File::CreateWithPath` does).
    pub fn create(path: &std::path::Path, minitracer: bool) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create {}: {}", parent.display(), e))?;
            }
        }
        let f = std::fs::File::create(path)
            .map_err(|e| format!("cannot open {}: {}", path.display(), e))?;
        Ok(Self::new(Box::new(std::io::BufWriter::new(f)), minitracer))
    }

    /// A shared pointer to a new printer.
    pub fn into_ptr(self) -> HardwareTracerPtr {
        Arc::new(Mutex::new(self))
    }

    /// The first write error, if any (C++ streams fail silently).
    pub fn io_error(&self) -> Option<&std::io::Error> {
        self.io_error.as_ref()
    }

    fn write(&mut self, s: &str) {
        if self.io_error.is_some() {
            return;
        }
        if let Err(e) = self
            .out
            .write_all(s.as_bytes())
            .and_then(|_| self.out.flush())
        {
            self.io_error = Some(e);
        }
    }
}

impl HardwareTracer for HardwareStatusPrinter {
    fn trace_hardware(
        &mut self,
        status: &mut dyn FnMut() -> String,
        mini_status: &mut dyn FnMut() -> String,
        mini_success: &mut dyn FnMut(i32) -> String,
        _bonus: bool,
        mini: bool,
        exec_success: i32,
    ) {
        // `SetupMiniTraceFileHeader` writes nothing for the CPU types ported
        // so far, so the header never becomes "done" and every minitrace
        // call is "in setup", as in the C++.
        let in_setup = self.minitracer && mini && !self.header_done;
        if exec_success == -2 || in_setup {
            if !self.minitracer && !mini {
                let s = status();
                self.write(&s);
            } else if self.minitracer && mini {
                let s = mini_status();
                self.write(&s);
            }
        }
        if exec_success != -2 && (in_setup || (self.minitracer && mini)) {
            let s = mini_success(exec_success);
            self.write(&s);
        }
    }

    fn print_success(&mut self, mini_success: &str) {
        self.write(mini_success);
    }

    fn trace_test_cpu(&mut self, final_status: &str) {
        self.write(final_status);
    }
}

/// A tracer that collects everything in memory (for tests and analyze-mode
/// callers that post-process the trace).
#[derive(Debug, Default, Clone)]
pub struct StringTracer {
    pub text: String,
}

impl HardwareTracer for StringTracer {
    fn trace_hardware(
        &mut self,
        status: &mut dyn FnMut() -> String,
        _mini_status: &mut dyn FnMut() -> String,
        _mini_success: &mut dyn FnMut(i32) -> String,
        _bonus: bool,
        mini: bool,
        exec_success: i32,
    ) {
        if exec_success == -2 && !mini {
            self.text.push_str(&status());
        }
    }

    fn print_success(&mut self, mini_success: &str) {
        self.text.push_str(mini_success);
    }

    fn trace_test_cpu(&mut self, final_status: &str) {
        self.text.push_str(final_status);
    }
}
