//! Helpers shared by the integration tests.
#![allow(dead_code)]

use std::io::Write;
use std::sync::{Arc, Mutex};

use avida_apto::DataFile;
use avida_genome::{Genome, InstructionSequence};
use avida_systematics::{Source, TransmissionType};

/// A `Write` sink whose bytes can be read after the `DataFile` is dropped.
#[derive(Clone, Default)]
pub struct Shared(pub Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Shared {
    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

/// A data file writing into memory, plus the handle to read it back.
pub fn mem_file() -> (DataFile, Shared) {
    let sink = Shared::default();
    (DataFile::from_writer("mem", Box::new(sink.clone())), sink)
}

/// The default ancestor of avida-core/support/config (`default-heads.org`
/// as stored in detail-100.spop: genotype 1).
pub const ANCESTOR: &str = "rucavccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccutycasvab";

/// `ANCESTOR` with position `pos` replaced by `inst`.
pub fn mutate(pos: usize, inst: char) -> String {
    let mut s: Vec<char> = ANCESTOR.chars().collect();
    s[pos] = inst;
    s.into_iter().collect()
}

pub fn genome(seq: &str) -> Genome {
    Genome::with_inst_set(0, "heads_default", InstructionSequence::from_string(seq))
}

pub fn div(external: bool) -> Source {
    Source::new(TransmissionType::Division, "", external)
}

/// Drops the time stamp line(s), which the C++ prints in local time.
pub fn without_timestamp(text: &str) -> String {
    text.lines()
        .filter(|l| {
            !(l.starts_with("# ")
                && ["Mon ", "Tue ", "Wed ", "Thu ", "Fri ", "Sat ", "Sun "]
                    .iter()
                    .any(|d| l[2..].starts_with(d)))
        })
        .map(|l| format!("{l}\n"))
        .collect()
}
