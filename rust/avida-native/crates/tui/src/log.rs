//! The event log fed by session events.

use std::collections::VecDeque;

/// Lines kept.
pub const LOG_CAPACITY: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Output,
    Stderr,
    Error,
    Fatal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub kind: LogKind,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct EventLog {
    lines: VecDeque<LogLine>,
    /// Lines dropped from the front so far.
    dropped: usize,
    /// Warnings and errors pushed so far, also those dropped from the front.
    problems: usize,
}

impl LogKind {
    /// Whether the line is a warning or an error (stderr, error, fatal).
    pub fn is_problem(self) -> bool {
        matches!(self, LogKind::Stderr | LogKind::Error | LogKind::Fatal)
    }
}

impl EventLog {
    pub fn push(&mut self, kind: LogKind, text: impl Into<String>) {
        if kind.is_problem() {
            self.problems += 1;
        }
        self.lines.push_back(LogLine {
            kind,
            text: text.into(),
        });
        while self.lines.len() > LOG_CAPACITY {
            self.lines.pop_front();
            self.dropped += 1;
        }
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

    /// Logs the error that stopped the run, removing the `error:` line
    /// avida printed with the same message just before it.
    pub fn push_fatal(&mut self, text: impl Into<String>) {
        let text = text.into();
        let found = self
            .lines
            .iter()
            .enumerate()
            .rev()
            .take(6)
            .find(|(_, l)| {
                (l.kind.is_problem() || l.kind == LogKind::Output)
                    && avida_viewmodel::problem::is_error_line(&l.text)
                    && avida_viewmodel::problem::repeats_fatal(&l.text, &text)
            })
            .map(|(i, _)| i);
        if let Some(i) = found {
            if let Some(old) = self.lines.remove(i) {
                if old.kind.is_problem() {
                    self.problems -= 1;
                }
            }
        }
        self.push(LogKind::Fatal, text);
    }

    /// Warnings and errors logged so far (kept when old lines are dropped).
    pub fn problems(&self) -> usize {
        self.problems
    }

    /// Lines dropped from the front because the log was full.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &LogLine> {
        self.lines.iter()
    }

    pub fn get(&self, i: usize) -> Option<&LogLine> {
        self.lines.get(i)
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_problems_even_after_they_scroll_out() {
        let mut l = EventLog::default();
        l.push(LogKind::Stderr, "WARNING: x");
        l.push(LogKind::Error, "boom");
        l.push(LogKind::Info, "fine");
        for i in 0..LOG_CAPACITY {
            l.push(LogKind::Output, format!("line {i}"));
        }
        assert_eq!(l.problems(), 2);
        assert!(l.iter().all(|x| !x.kind.is_problem()));
    }

    #[test]
    fn error_labelled_stderr_is_an_error_and_the_fatal_is_not_doubled() {
        let mut l = EventLog::default();
        l.push_stderr("error: unable to open file 'm.org'.");
        assert_eq!(l.get(0).map(|x| x.kind), Some(LogKind::Error));
        l.push_fatal("unable to open file 'm.org'.");
        assert_eq!(l.len(), 1);
        assert_eq!(l.get(0).map(|x| x.kind), Some(LogKind::Fatal));
        assert_eq!(l.problems(), 1);
    }

    #[test]
    fn is_bounded() {
        let mut l = EventLog::default();
        for i in 0..LOG_CAPACITY + 10 {
            l.push(LogKind::Output, format!("line {i}"));
        }
        assert_eq!(l.len(), LOG_CAPACITY);
        assert_eq!(l.get(0).map(|x| x.text.as_str()), Some("line 10"));
    }
}
