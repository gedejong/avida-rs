//! What the simulation prints on its own, outside its data files and status
//! lines, made capturable by a front end.
//!
//! The C++ writes diagnostics straight to `cerr`/`cout` (`cerr << "error:
//! ..."`, `printf("SLIP MUTATION ...")`) from wherever they arise. The
//! simulation crates shadow the std printing macros (`eprint!` and
//! `eprintln!`, and in the hardware and phenotype crates `print!` and
//! `println!`) with macros that call [`stderr`] and [`stdout`] here; each
//! crate's `lib.rs` defines them before its modules.
//!
//! Without a capture these write exactly what the std macros write, so the
//! `avida` binary's output is unchanged. A thread that has called
//! [`start_capture`] (the session's simulation thread) collects the text
//! instead and takes it with [`take_captured`]. Captures are per thread:
//! two sessions in one process, or a session beside test threads, never see
//! each other's text.

use std::cell::RefCell;
use std::fmt::{self, Write as _};

/// Text printed on a capturing thread since the last [`take_captured`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Captured {
    /// What `print!`/`println!` would have written to stdout.
    pub stdout: String,
    /// What `eprint!`/`eprintln!` would have written to stderr.
    pub stderr: String,
}

impl Captured {
    pub fn is_empty(&self) -> bool {
        self.stdout.is_empty() && self.stderr.is_empty()
    }
}

thread_local! {
    static CAPTURE: RefCell<Option<Captured>> = const { RefCell::new(None) };
}

/// Starts collecting this thread's diagnostics (text already collected is
/// kept).
pub fn start_capture() {
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(Captured::default());
        }
    });
}

/// Stops collecting on this thread and returns what was not taken yet.
pub fn stop_capture() -> Captured {
    CAPTURE.with(|c| c.borrow_mut().take().unwrap_or_default())
}

/// Whether this thread collects its diagnostics.
pub fn is_capturing() -> bool {
    CAPTURE.with(|c| c.borrow().is_some())
}

/// The text collected on this thread since the last call (empty without a
/// capture).
pub fn take_captured() -> Captured {
    CAPTURE.with(|c| {
        c.borrow_mut()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    })
}

/// Appends to the capture of this thread; `false` when there is none (or
/// the formatting itself printed, which then goes to the real stream).
fn capture(args: fmt::Arguments<'_>, pick: fn(&mut Captured) -> &mut String) -> bool {
    CAPTURE.with(|c| match c.try_borrow_mut() {
        Ok(mut c) => match c.as_mut() {
            Some(cap) => {
                let _ = pick(cap).write_fmt(args);
                true
            }
            None => false,
        },
        Err(_) => false,
    })
}

/// `eprint!`: to the capture of this thread, or to stderr as
/// `std::eprint!` writes it.
pub fn stderr(args: fmt::Arguments<'_>) {
    if !capture(args, |c| &mut c.stderr) {
        std::eprint!("{args}");
    }
}

/// `print!`: to the capture of this thread, or to stdout as `std::print!`
/// writes it.
pub fn stdout(args: fmt::Arguments<'_>) {
    if !capture(args, |c| &mut c.stdout) {
        std::print!("{args}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capture_collects_per_thread_until_stopped() {
        assert!(!is_capturing());
        assert_eq!(take_captured(), Captured::default());
        start_capture();
        stderr(format_args!("error: {}\n", 7));
        stdout(format_args!("out"));
        let other = std::thread::spawn(|| {
            // Another thread has no capture: this goes to the real stderr.
            stderr(format_args!(""));
            is_capturing()
        });
        assert!(!other.join().unwrap());
        let got = take_captured();
        assert_eq!(got.stderr, "error: 7\n");
        assert_eq!(got.stdout, "out");
        assert!(take_captured().is_empty());
        stderr(format_args!("tail"));
        assert_eq!(stop_capture().stderr, "tail");
        assert!(!is_capturing());
    }
}
