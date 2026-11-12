//! `cUserFeedback` (avida-core/source/tools/cUserFeedback.h), the
//! `Avida::Feedback` sink the loaders in this crate report into.
//!
//! This is a re-export of [`avida_config::feedback`], so one `Feedback`
//! value can be threaded through configuration and instruction set
//! loading, as the C++ driver does with a single `cUserFeedback`.

pub use avida_config::feedback::{Feedback, FeedbackKind};

/// Test convenience: the text of every error message, in order.
#[cfg(test)]
pub(crate) trait ErrorMessages {
    fn error_messages(&self) -> Vec<&str>;
}

#[cfg(test)]
impl ErrorMessages for Feedback {
    fn error_messages(&self) -> Vec<&str> {
        self.messages()
            .iter()
            .filter(|(k, _)| *k == FeedbackKind::Error)
            .map(|(_, m)| m.as_str())
            .collect()
    }
}
