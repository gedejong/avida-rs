//! Port of `cUserFeedback` (avida-core/source/tools/cUserFeedback.h) and the
//! `Avida::Feedback` protocol: an ordered list of error / warning /
//! notification messages.

/// `cUserFeedback::eFeedbackType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackKind {
    /// `UF_ERROR`
    Error,
    /// `UF_WARNING`
    Warning,
    /// `UF_NOTIFICATION`
    Notification,
}

/// `cUserFeedback`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Feedback {
    entries: Vec<(FeedbackKind, String)>,
    errors: i32,
    warnings: i32,
}

impl Feedback {
    /// `cUserFeedback::cUserFeedback()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `Error(fmt, ...)`: the caller formats the message.
    pub fn error(&mut self, msg: impl Into<String>) {
        self.entries.push((FeedbackKind::Error, msg.into()));
        self.errors += 1;
    }

    /// `Warning(fmt, ...)`.
    pub fn warning(&mut self, msg: impl Into<String>) {
        self.entries.push((FeedbackKind::Warning, msg.into()));
        self.warnings += 1;
    }

    /// `Notify(fmt, ...)`.
    pub fn notify(&mut self, msg: impl Into<String>) {
        self.entries.push((FeedbackKind::Notification, msg.into()));
    }

    /// `GetNumMessages()`.
    pub fn num_messages(&self) -> usize {
        self.entries.len()
    }
    /// `GetNumErrors()`.
    pub fn num_errors(&self) -> i32 {
        self.errors
    }
    /// `GetNumWarnings()`.
    pub fn num_warnings(&self) -> i32 {
        self.warnings
    }
    /// `GetNumNotifications()`: `entries - errors - warnings`.
    pub fn num_notifications(&self) -> i32 {
        self.entries.len() as i32 - self.errors - self.warnings
    }
    /// `GetMessageType(i)`.
    pub fn message_type(&self, i: usize) -> FeedbackKind {
        self.entries[i].0
    }
    /// `GetMessage(i)`.
    pub fn message(&self, i: usize) -> &str {
        &self.entries[i].1
    }
    /// All `(kind, message)` pairs in order.
    pub fn messages(&self) -> &[(FeedbackKind, String)] {
        &self.entries
    }

    /// `cUserFeedback::Append(uf)`: copies the entries only.
    ///
    /// QUIRK: the error and warning counters are NOT updated, so messages
    /// appended this way are not reflected in `num_errors`/`num_warnings`
    /// (and inflate `num_notifications`).
    pub fn append(&mut self, other: &Feedback) {
        self.entries.extend(other.entries.iter().cloned());
    }

    /// Like `append` but also adds the counters (used internally when the C++
    /// would have logged straight into the other object).
    pub fn absorb(&mut self, other: &Feedback) {
        self.entries.extend(other.entries.iter().cloned());
        self.errors += other.errors;
        self.warnings += other.warnings;
    }

    /// The line the Avida front end prints for message `i`
    /// (`"error: "` / `"warning: "` prefix, none for notifications), without
    /// the trailing newline (see `CmdLine.cc`, `primitive.cc`).
    pub fn format_message(&self, i: usize) -> String {
        let (k, m) = &self.entries[i];
        match k {
            FeedbackKind::Error => format!("error: {}", m),
            FeedbackKind::Warning => format!("warning: {}", m),
            FeedbackKind::Notification => m.clone(),
        }
    }

    /// All messages formatted as by `format_message`, one per line.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for i in 0..self.entries.len() {
            out.push_str(&self.format_message(i));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_order() {
        let mut f = Feedback::new();
        f.error("e1");
        f.warning("w1");
        f.notify("n1");
        assert_eq!(f.num_messages(), 3);
        assert_eq!(f.num_errors(), 1);
        assert_eq!(f.num_warnings(), 1);
        assert_eq!(f.num_notifications(), 1);
        assert_eq!(f.message_type(1), FeedbackKind::Warning);
        assert_eq!(f.render(), "error: e1\nwarning: w1\nn1\n");
    }

    #[test]
    fn append_quirk_does_not_count() {
        let mut a = Feedback::new();
        let mut b = Feedback::new();
        b.error("x");
        a.append(&b);
        assert_eq!(a.num_messages(), 1);
        assert_eq!(a.num_errors(), 0);
        assert_eq!(a.num_notifications(), 1);
        let mut c = Feedback::new();
        c.absorb(&b);
        assert_eq!(c.num_errors(), 1);
    }
}
