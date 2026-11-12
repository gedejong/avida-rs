//! `cActionSetVerbose` (avida-core/source/actions/PrintActions.cc), the
//! `VERBOSE` action analyze scripts use. It lives in the action library, so
//! analyze mode reaches it through [`crate::AnalyzeHost::run_action`]; this
//! port lets a host implement that action without the rest of the library.

use std::io::{self, Write};

use avida_config::{string_util as su, Config};

/// `VERBOSE_SILENT` .. `VERBOSE_DEBUG` (avida/core/Definitions.h).
pub const VERBOSE_SILENT: i32 = 0;
pub const VERBOSE_NORMAL: i32 = 1;
pub const VERBOSE_ON: i32 = 2;
pub const VERBOSE_DETAILS: i32 = 3;
pub const VERBOSE_DEBUG: i32 = 4;

/// `cActionSetVerbose(world, args)` + `Process()`: no argument toggles
/// between NORMAL and ON; otherwise the (upper-cased) first word picks the
/// level. Prints the new level unless it is SILENT (or DEBUG).
pub fn set_verbose(config: &mut Config, args: &str, out: &mut dyn Write) -> io::Result<()> {
    let mut largs = args.to_string();
    let mut verbose = String::new();
    if !largs.is_empty() {
        verbose = su::pop_word(&mut largs);
    }
    verbose.make_ascii_uppercase();

    let cur = config.verbosity;
    config.verbosity = if verbose.is_empty() && cur <= VERBOSE_NORMAL {
        VERBOSE_ON
    } else if verbose.is_empty() && cur >= VERBOSE_ON {
        VERBOSE_NORMAL
    } else {
        match verbose.as_str() {
            "SILENT" => VERBOSE_SILENT,
            "NORMAL" | "QUIET" | "OFF" => VERBOSE_NORMAL,
            "ON" => VERBOSE_ON,
            "DETAILS" | "HIGH" => VERBOSE_DETAILS,
            _ => VERBOSE_NORMAL,
        }
    };

    match config.verbosity {
        VERBOSE_NORMAL => writeln!(out, "Verbose NORMAL: Using standard log messages...")?,
        VERBOSE_ON => writeln!(out, "Verbose ON: Using verbose log messages...")?,
        VERBOSE_DETAILS => writeln!(out, "Verbose DETAILS: Using detailed log messages...")?,
        _ => {}
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_and_levels() {
        let mut c = Config::new();
        c.verbosity = 1;
        let mut out = Vec::new();
        set_verbose(&mut c, "", &mut out).unwrap();
        assert_eq!(c.verbosity, VERBOSE_ON);
        set_verbose(&mut c, "", &mut out).unwrap();
        assert_eq!(c.verbosity, VERBOSE_NORMAL);
        set_verbose(&mut c, "high", &mut out).unwrap();
        assert_eq!(c.verbosity, VERBOSE_DETAILS);
        set_verbose(&mut c, "silent", &mut out).unwrap();
        assert_eq!(c.verbosity, VERBOSE_SILENT);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "Verbose ON: Using verbose log messages...\n\
             Verbose NORMAL: Using standard log messages...\n\
             Verbose DETAILS: Using detailed log messages...\n"
        );
    }
}
