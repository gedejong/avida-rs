//! The run log (what the `avida` binary would print, plus session errors)
//! and transient notifications.

use std::collections::VecDeque;

/// How many of the newest lines are searched for the fatal message's twin.
const FATAL_LOOKBACK: usize = 6;

/// Lines kept in the log; older ones are dropped.
pub const LOG_CAPACITY: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogKind {
    /// Front-end notes (opened, saved, exported).
    Info,
    /// A line `avida` prints to stdout.
    Output,
    /// A line `avida` prints to stderr while setting up.
    Stderr,
    /// A command the session could not carry out.
    Error,
    /// The error that stopped the run.
    Fatal,
}

impl LogKind {
    pub fn is_problem(self) -> bool {
        matches!(self, LogKind::Stderr | LogKind::Error | LogKind::Fatal)
    }

    /// Errors stop a command or the run; stderr lines are warnings
    /// (`avida` keeps going after them).
    pub fn is_error(self) -> bool {
        matches!(self, LogKind::Error | LogKind::Fatal)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub kind: LogKind,
    pub text: String,
}

/// Which lines the log panel shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LogFilter {
    #[default]
    All,
    /// Errors, fatal errors and stderr only.
    Problems,
}

impl LogFilter {
    pub fn admits(self, kind: LogKind) -> bool {
        match self {
            LogFilter::All => true,
            LogFilter::Problems => kind.is_problem(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EventLog {
    entries: VecDeque<LogEntry>,
    dropped: u64,
    errors: usize,
    warnings: usize,
    version: u64,
}

impl EventLog {
    pub fn push(&mut self, kind: LogKind, text: impl Into<String>) {
        if self.entries.len() == LOG_CAPACITY {
            if let Some(old) = self.entries.pop_front() {
                self.count(old.kind, false);
            }
            self.dropped += 1;
        }
        self.count(kind, true);
        self.entries.push_back(LogEntry {
            kind,
            text: text.into(),
        });
        self.version += 1;
    }

    /// Logs a line `avida` printed on stderr: an error when it begins
    /// with `error:`, a warning otherwise.
    pub fn push_stderr(&mut self, text: impl Into<String>) {
        let text = text.into();
        let kind = if avida_viewmodel::problem::is_error_line(&text) {
            LogKind::Error
        } else {
            LogKind::Stderr
        };
        self.push(kind, text);
    }

    /// Logs the error that stopped the run. Avida usually prints the same
    /// message with an `error:` label just before it stops; that earlier
    /// line is removed so the message is shown and counted once.
    pub fn push_fatal(&mut self, text: impl Into<String>) {
        let text = text.into();
        let found = self
            .entries
            .iter()
            .enumerate()
            .rev()
            .take(FATAL_LOOKBACK)
            .find(|(_, e)| {
                (e.kind.is_problem() || e.kind == LogKind::Output)
                    && avida_viewmodel::problem::is_error_line(&e.text)
                    && avida_viewmodel::problem::repeats_fatal(&e.text, &text)
            })
            .map(|(i, _)| i);
        if let Some(i) = found {
            if let Some(old) = self.entries.remove(i) {
                self.count(old.kind, false);
            }
        }
        self.push(LogKind::Fatal, text);
    }

    fn count(&mut self, kind: LogKind, add: bool) {
        let counter = if kind.is_error() {
            &mut self.errors
        } else if kind.is_problem() {
            &mut self.warnings
        } else {
            return;
        };
        if add {
            *counter += 1;
        } else {
            *counter -= 1;
        }
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &LogEntry> + ExactSizeIterator {
        self.entries.iter()
    }

    pub fn filtered(&self, filter: LogFilter) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| filter.admits(e.kind))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Errors, fatal errors and stderr lines currently in the log.
    pub fn problem_count(&self) -> usize {
        self.errors + self.warnings
    }

    /// Errors and fatal errors currently in the log.
    pub fn error_count(&self) -> usize {
        self.errors
    }

    /// Stderr lines currently in the log.
    pub fn warning_count(&self) -> usize {
        self.warnings
    }

    /// Lines dropped because the log was full.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// The newest non-empty line.
    pub fn last_line(&self) -> Option<&LogEntry> {
        self.entries
            .iter()
            .rev()
            .find(|e| !e.text.trim().is_empty())
    }

    /// The newest line worth a glance in the status bar: not the
    /// per-update progress line avida prints (`UD: 40  Gen: ...`), which
    /// the top bar already shows in words.
    pub fn last_note(&self) -> Option<&LogEntry> {
        self.entries.iter().rev().find(|e| {
            let text = e.text.trim();
            !text.is_empty() && !text.starts_with("UD:")
        })
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.errors = 0;
        self.warnings = 0;
        self.version += 1;
    }

    /// The lines that pass `filter`, one per line.
    pub fn to_text(&self, filter: LogFilter) -> String {
        let mut out = String::new();
        for e in self.entries.iter().filter(|e| filter.admits(e.kind)) {
            out.push_str(&e.text);
            out.push('\n');
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// A notification shown for a few seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    /// `egui` input time at which it disappears.
    pub expires: f64,
}

impl ToastKind {
    /// Seconds a toast of this kind stays up.
    pub fn duration(self) -> f64 {
        match self {
            ToastKind::Info | ToastKind::Success => 4.0,
            ToastKind::Error => 9.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_problems_and_filters() {
        let mut log = EventLog::default();
        log.push(LogKind::Output, "UD: 1");
        log.push(LogKind::Error, "cannot step");
        log.push(LogKind::Output, "");
        log.push(LogKind::Stderr, "warning: version mismatch");
        log.push(LogKind::Output, "");
        assert_eq!(log.len(), 5);
        assert_eq!(log.problem_count(), 2);
        assert_eq!((log.error_count(), log.warning_count()), (1, 1));
        assert_eq!(log.filtered(LogFilter::Problems).len(), 2);
        log.clear();
        log.push(LogKind::Output, "UD: 1");
        log.push(LogKind::Error, "cannot step");
        log.push(LogKind::Output, "");
        assert_eq!(log.filtered(LogFilter::Problems).len(), 1);
        assert_eq!(
            log.last_line().map(|e| e.text.as_str()),
            Some("cannot step")
        );
        assert_eq!(log.to_text(LogFilter::All), "UD: 1\ncannot step\n\n");
    }

    #[test]
    fn error_labelled_stderr_is_an_error_and_the_fatal_is_not_doubled() {
        let mut log = EventLog::default();
        log.push_stderr("warning: odd");
        log.push_stderr("error: unable to open file 'm.org'.");
        assert_eq!((log.error_count(), log.warning_count()), (1, 1));
        log.push_fatal("unable to open file 'm.org'.");
        assert_eq!((log.error_count(), log.warning_count()), (1, 1));
        let kinds: Vec<_> = log.entries().map(|e| e.kind).collect();
        assert_eq!(kinds, vec![LogKind::Stderr, LogKind::Fatal]);
    }

    #[test]
    fn drops_the_oldest_lines_when_full() {
        let mut log = EventLog::default();
        log.push(LogKind::Fatal, "first");
        for i in 0..LOG_CAPACITY {
            log.push(LogKind::Output, format!("{i}"));
        }
        assert_eq!(log.len(), LOG_CAPACITY);
        assert_eq!(log.dropped(), 1);
        assert_eq!(log.problem_count(), 0);
        assert_eq!(log.entries().next().map(|e| e.text.as_str()), Some("0"));
    }
}
