//! Number and state formatting shared by the widgets.

use avida_session::RunState;
use avida_viewmodel::format_number;

/// A whole number with thousands separators: `1,234,567`.
pub fn count(v: i64) -> String {
    let digits = v.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if v < 0 {
        out.push('-');
    }
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A real number, compactly (`avida_viewmodel::format_number`), with
/// separators for large whole values.
/// A file size: `3.7 MiB`.
pub fn bytes(b: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut v = b as f64;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

pub fn real(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 && v.abs() >= 1e4 {
        return count(v as i64);
    }
    format_number(v)
}

/// The key that goes with letters in shortcuts: Cmd on macOS, Ctrl
/// elsewhere.
pub fn command_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

/// A shortcut written with `Cmd` (`"Cmd K"`) as this platform names it.
pub fn shortcut(keys: &str) -> String {
    keys.replace("Cmd", command_key())
}

/// A metric value: counts with separators, reals compactly.
pub fn metric(v: f64, is_count: bool) -> String {
    if is_count && v.is_finite() {
        count(v.round() as i64)
    } else {
        real(v)
    }
}

/// `1 time slice`, `2,048 time slices`.
pub fn plural(n: u64, what: &str) -> String {
    let s = if n == 1 { "" } else { "s" };
    format!("{} {what}{s}", count(i64::try_from(n).unwrap_or(i64::MAX)))
}

/// A large count compactly: `950`, `12.4k`, `3.1M`, `2.0G`.
pub fn compact(n: f64) -> String {
    let a = n.abs();
    let (v, unit) = if a >= 1e9 {
        (n / 1e9, "G")
    } else if a >= 1e6 {
        (n / 1e6, "M")
    } else if a >= 1e4 {
        (n / 1e3, "k")
    } else {
        return count(n.round() as i64);
    };
    format!("{v:.1}{unit}")
}

/// A CPU word as unsigned hexadecimal: `0x0f0f0f0f`.
pub fn hex(v: i32) -> String {
    format!("0x{:08x}", v as u32)
}

/// The label of the run-state badge.
pub fn run_state(state: RunState) -> &'static str {
    match state {
        RunState::Paused => "Paused",
        RunState::Running => "Running",
        RunState::Finished => "Finished",
        RunState::Failed => "Failed",
    }
}

/// `update 41` / `before the first update`.
pub fn update_phrase(update: i32) -> String {
    if update < 0 {
        "before the first update".to_string()
    } else {
        format!("update {}", count(i64::from(update)))
    }
}

/// `born at update 1,200`, or `born before the first update` for what
/// was injected before the run began.
pub fn born(update: i32) -> String {
    if update < 0 {
        "born before the first update".to_string()
    } else {
        format!("born at {}", update_phrase(update))
    }
}

/// A rate such as `1,234 updates/s` or `0.5 updates/s`.
pub fn rate(updates_per_second: f64) -> String {
    format!("{} updates/s", rate_number(updates_per_second))
}

/// The number of [`rate`] alone: `1,234`, `12`, `0.5`.
pub fn rate_number(updates_per_second: f64) -> String {
    if updates_per_second >= 100.0 {
        count(updates_per_second.round() as i64)
    } else if updates_per_second >= 10.0 {
        format!("{:.0}", updates_per_second)
    } else {
        format!("{:.1}", updates_per_second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_get_separators() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1000), "1,000");
        assert_eq!(count(-1234567), "-1,234,567");
        assert_eq!(count(i64::MIN), "-9,223,372,036,854,775,808");
    }

    #[test]
    fn reals_are_compact() {
        assert_eq!(real(0.25), "0.25");
        assert_eq!(real(12345.0), "12,345");
        assert_eq!(real(3.0), "3");
        assert_eq!(metric(12.4, true), "12");
        assert_eq!(metric(f64::NAN, true), "NaN");
        assert_eq!(hex(-1), "0xffffffff");
        assert_eq!(update_phrase(-1), "before the first update");
        assert_eq!(update_phrase(1200), "update 1,200");
        assert_eq!(born(-1), "born before the first update");
        assert_eq!(born(3), "born at update 3");
        assert_eq!(rate(1234.4), "1,234 updates/s");
        assert_eq!(rate(2.24), "2.2 updates/s");
        assert_eq!(plural(1, "slice"), "1 slice");
        assert_eq!(plural(2048, "slice"), "2,048 slices");
        assert_eq!(compact(950.0), "950");
        assert_eq!(compact(12_400.0), "12.4k");
        assert_eq!(compact(3_140_000.0), "3.1M");
    }
}
