//! How the front ends read the problem lines `avida` prints.
//!
//! Both UIs classify a stderr line the same way and both avoid showing the
//! line that is the fatal message itself a second time.

/// Whether a line printed on stderr (or an `error:` line on stdout) is an
/// error rather than a warning: it begins with `error:`, in any case.
pub fn is_error_line(line: &str) -> bool {
    let l = line.trim_start();
    l.get(..6).is_some_and(|p| p.eq_ignore_ascii_case("error:"))
}

/// The line without a leading `error:` or `warning:` label.
pub fn message_body(line: &str) -> &str {
    let l = line.trim();
    for label in ["error:", "warning:"] {
        if l.get(..label.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(label))
        {
            return l[label.len()..].trim_start();
        }
    }
    l
}

/// Whether `line` says what the fatal message `fatal` says on its first
/// line, so showing both would repeat it.
pub fn repeats_fatal(line: &str, fatal: &str) -> bool {
    let first = message_body(fatal.lines().next().unwrap_or(""));
    !first.is_empty() && message_body(line) == first
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_error_prefix() {
        assert!(is_error_line("error: unable to open file 'x'."));
        assert!(is_error_line("  ERROR: boom"));
        assert!(!is_error_line("warning: odd"));
        assert!(!is_error_line("errors are fine"));
        assert!(!is_error_line("err"));
    }

    #[test]
    fn matches_the_fatal_message_with_or_without_label() {
        assert!(repeats_fatal(
            "error: unable to open 'a'.",
            "unable to open 'a'."
        ));
        assert!(repeats_fatal(
            "unable to open 'a'.",
            "error: unable to open 'a'.\nmore"
        ));
        assert!(!repeats_fatal("error: other", "unable to open 'a'."));
        assert!(!repeats_fatal("", ""));
    }
}
