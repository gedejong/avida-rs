//! C/C++-compatible number formatting, so data files and console output match
//! the reference binary byte for byte.
//!
//! * `std::ostream << double` with default flags is libc++'s
//!   `snprintf("%.*g", precision)` with precision 6 (`fmt_double`).
//! * `printf` conversions `%f %F %e %E %g %G` with flags `- + space # 0`,
//!   width and precision ([`CFloatSpec`], [`format_c`]).
//! * The manipulator combinations of the driver status line
//!   (avida-core/source/targets/avida/Avida2Driver.cc: `ios::left`,
//!   `ios::showpoint`, `setw`, `setprecision`).
//!
//! Digits come from Rust's correctly rounded `{:.*e}` / `{:.*}` (exact
//! decimal expansion, ties to even, as the platform `printf` does); the `%g`
//! style selection and trailing-zero rules are applied on top.
//!
//! Platform notes (macOS libc, which produced the reference outputs): NaN is
//! printed as `nan` with no sign even when negative or with the `+` flag;
//! infinities keep their sign; the `0` flag is ignored for both. glibc would
//! print `-nan`; the macOS behaviour is what the C++ reference binary does.
//!
//! Verified against `tools/fmt_ref.cc` (libc++ `ostream` and `snprintf`) for
//! about 2,900 edge-case and random doubles, see `testdata/fmt_ref.txt`.

/// Parsed `printf` floating-point conversion: `%[flags][width][.precision]conv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CFloatSpec {
    /// `-`: left-justify within the width.
    pub left: bool,
    /// `+`: always print a sign.
    pub plus: bool,
    /// space: print a space where a `+` would go.
    pub space: bool,
    /// `#`: alternate form (always a decimal point; `%g` keeps trailing zeros).
    /// This is `ios::showpoint` on a stream.
    pub alt: bool,
    /// `0`: pad with zeros after the sign.
    pub zero: bool,
    /// Minimum field width.
    pub width: usize,
    /// Precision; `None` means the C default of 6.
    pub precision: Option<usize>,
    /// One of `f F e E g G`.
    pub conv: char,
}

impl CFloatSpec {
    /// A spec with no flags or width.
    pub fn new(conv: char, precision: Option<usize>) -> Self {
        CFloatSpec {
            left: false,
            plus: false,
            space: false,
            alt: false,
            zero: false,
            width: 0,
            precision,
            conv,
        }
    }

    /// Parse a conversion such as `"%7.2f"`, `"%-12.4g"`, `"%#.7g"`.
    /// The `l` length modifier (`%lf`) is accepted and ignored.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let bad = || format!("unsupported float format '{spec}'");
        let rest = spec.strip_prefix('%').ok_or_else(bad)?;
        let mut out = CFloatSpec::new('g', None);
        let mut chars = rest.chars().peekable();
        while let Some(&c) = chars.peek() {
            match c {
                '-' => out.left = true,
                '+' => out.plus = true,
                ' ' => out.space = true,
                '#' => out.alt = true,
                '0' => out.zero = true,
                _ => break,
            }
            chars.next();
        }
        let mut width = 0usize;
        while let Some(d) = chars.peek().and_then(|c| c.to_digit(10)) {
            width = width * 10 + d as usize;
            chars.next();
        }
        out.width = width;
        if chars.peek() == Some(&'.') {
            chars.next();
            let mut prec = 0usize;
            while let Some(d) = chars.peek().and_then(|c| c.to_digit(10)) {
                prec = prec * 10 + d as usize;
                chars.next();
            }
            out.precision = Some(prec);
        }
        if chars.peek() == Some(&'l') {
            chars.next();
        }
        match (chars.next(), chars.next()) {
            (Some(c @ ('f' | 'F' | 'e' | 'E' | 'g' | 'G')), None) => out.conv = c,
            _ => return Err(bad()),
        }
        Ok(out)
    }

    /// Format `v` as `snprintf(buf, n, spec, v)` would.
    pub fn format(&self, v: f64) -> String {
        let upper = self.conv.is_ascii_uppercase();
        let (sign, body, numeric) = if v.is_nan() {
            ("", if upper { "NAN" } else { "nan" }.to_string(), false)
        } else {
            let sign = if v.is_sign_negative() {
                "-"
            } else if self.plus {
                "+"
            } else if self.space {
                " "
            } else {
                ""
            };
            let body = if v.is_infinite() {
                if upper { "INF" } else { "inf" }.to_string()
            } else {
                let prec = self.precision.unwrap_or(6);
                let a = v.abs();
                let s = match self.conv.to_ascii_lowercase() {
                    'f' => fixed_body(a, prec, self.alt),
                    'e' => sci_body(a, prec, self.alt),
                    _ => g_body(a, prec, self.alt),
                };
                if upper {
                    s.to_ascii_uppercase()
                } else {
                    s
                }
            };
            (sign, body, v.is_finite())
        };

        let len = sign.len() + body.len();
        if len >= self.width {
            return format!("{sign}{body}");
        }
        let pad = self.width - len;
        if self.left {
            format!("{sign}{body}{}", " ".repeat(pad))
        } else if self.zero && numeric {
            format!("{sign}{}{body}", "0".repeat(pad))
        } else {
            format!("{}{sign}{body}", " ".repeat(pad))
        }
    }
}

/// Format with a C conversion spec, e.g. `format_c("%7.2f", x)`.
pub fn format_c(spec: &str, v: f64) -> Result<String, String> {
    Ok(CFloatSpec::parse(spec)?.format(v))
}

/// `%.{prec}f` digits of a non-negative finite value.
fn fixed_body(a: f64, prec: usize, alt: bool) -> String {
    let mut s = format!("{a:.prec$}");
    if alt && prec == 0 {
        s.push('.');
    }
    s
}

/// Splits Rust's `{:.*e}` output (`"1.23457e6"`) into mantissa and exponent.
fn rust_sci(a: f64, prec: usize) -> (String, i32) {
    let s = format!("{a:.prec$e}");
    match s.split_once('e') {
        Some((m, e)) => (m.to_string(), e.parse().unwrap_or(0)),
        None => (s, 0),
    }
}

/// C exponent suffix: `e`, sign, at least two digits.
fn c_exponent(x: i32) -> String {
    let sign = if x < 0 { '-' } else { '+' };
    format!("e{sign}{:02}", x.unsigned_abs())
}

/// `%.{prec}e` digits of a non-negative finite value.
fn sci_body(a: f64, prec: usize, alt: bool) -> String {
    let (mut m, x) = rust_sci(a, prec);
    if alt && prec == 0 {
        m.push('.');
    }
    m + &c_exponent(x)
}

/// Removes trailing zeros of the fraction, then a trailing decimal point
/// (the `%g` rule without `#`).
fn strip_fraction_zeros(s: &mut String) {
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
}

/// `%.{prec}g` digits of a non-negative finite value (C11 7.21.6.1).
fn g_body(a: f64, prec: usize, alt: bool) -> String {
    let p = if prec == 0 { 1 } else { prec };
    // X: the exponent an `%e` conversion with precision P - 1 would have.
    let (mut mant, x) = rust_sci(a, p - 1);
    if x < p as i32 && x >= -4 {
        let fprec = (p as i32 - 1 - x) as usize;
        let mut s = format!("{a:.fprec$}");
        if alt {
            if !s.contains('.') {
                s.push('.');
            }
        } else {
            strip_fraction_zeros(&mut s);
        }
        s
    } else {
        if alt {
            if !mant.contains('.') {
                mant.push('.');
            }
        } else if !dtoa_small_int_round_down_tie(a, p) {
            strip_fraction_zeros(&mut mant);
        }
        mant + &c_exponent(x)
    }
}

/// QUIRK (macOS libc): `%g` takes as many significant digits as `dtoa`
/// (mode 2) returns, and dtoa normally trims trailing zeros. Its fast path for
/// integers below 1e15 does not trim when an exact tie (`...|5000`) rounds
/// down to an even last digit, so `printf("%g", 52456050.0)` prints
/// `5.24560e+07`, not `5.2456e+07`. All other paths trim. Non-integers cannot
/// hit this, since a binary fraction's tie digit is preceded by 2 or 7.
/// Verified against the precision sweep in `testdata/fmt_g_sweep.txt`.
fn dtoa_small_int_round_down_tie(a: f64, p: usize) -> bool {
    // `k <= Int_max` (14) and `ilim <= Quick_max` (14) in gdtoa's dtoa.c.
    if p > 14 || a.fract() != 0.0 || a >= 1e15 {
        return false;
    }
    let digits = format!("{}", a as u64);
    if digits.len() <= p {
        return false;
    }
    let (kept, rest) = digits.split_at(p);
    let rest = rest.as_bytes();
    let tie = rest[0] == b'5' && rest[1..].iter().all(|&b| b == b'0');
    let last_even = (kept.as_bytes()[p - 1] - b'0').is_multiple_of(2);
    tie && last_even
}

// ---------------------------------------------------------------------------
// printf / ostream shorthands
// ---------------------------------------------------------------------------

/// `std::ostream << double` with default flags (precision 6): `%g`.
pub fn fmt_double(v: f64) -> String {
    fmt_g(v, 6)
}

/// `%.{precision}g`, i.e. `os << setprecision(p) << v` with default flags.
pub fn fmt_g(v: f64, precision: usize) -> String {
    CFloatSpec::new('g', Some(precision)).format(v)
}

/// `%#.{precision}g`, i.e. `os.setf(ios::showpoint); os << setprecision(p) << v`.
pub fn fmt_g_showpoint(v: f64, precision: usize) -> String {
    let mut spec = CFloatSpec::new('g', Some(precision));
    spec.alt = true;
    spec.format(v)
}

/// `%.{precision}f`, i.e. `os << std::fixed << setprecision(p) << v`.
pub fn fmt_fixed(v: f64, precision: usize) -> String {
    CFloatSpec::new('f', Some(precision)).format(v)
}

/// `%.{precision}e`, i.e. `os << std::scientific << setprecision(p) << v`.
pub fn fmt_sci(v: f64, precision: usize) -> String {
    CFloatSpec::new('e', Some(precision)).format(v)
}

/// `os << setw(width)` with `ios::left`: pads on the right with spaces. A
/// value longer than `width` is not truncated.
pub fn pad_left_justified(s: &str, width: usize) -> String {
    format!("{s:<width$}")
}

/// `os << setw(width)` with the default right adjustment: pads on the left.
pub fn pad_right_justified(s: &str, width: usize) -> String {
    format!("{s:>width$}")
}

/// One double cell of the Avida2Driver status line (`Gen:`, `Fit:`,
/// `Merit:`): `ios::left | ios::showpoint`, `setw(9)`, `setprecision(7)`.
pub fn status_double(v: f64) -> String {
    pad_left_justified(&fmt_g_showpoint(v, 7), 9)
}

/// An integer cell of the Avida2Driver status line (`UD:` and `Orgs:` use
/// width 6, `Demes:` width 4): `ios::left`, `setw(width)`.
pub fn status_int(v: i64, width: usize) -> String {
    pad_left_justified(&v.to_string(), width)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REF: &str = include_str!("../testdata/fmt_ref.txt");

    /// Same order as FORMATS in tools/fmt_ref.cc.
    const FORMATS: &[&str] = &[
        "%g", "%.1g", "%.3g", "%.7g", "%.10g", "%.17g", "%#.7g", "%#g", "%#.1g", "%-12.4g", "%+g",
        "%e", "%.0e", "%7.1e", "%#.0e", "%f", "%.0f", "%.2f", "%7.2f", "%04.3f", "%1.5f", "%#.0f",
        "%-9.3f",
    ];

    #[test]
    fn matches_cpp_printf_and_ostream() {
        let specs: Vec<CFloatSpec> = FORMATS
            .iter()
            .map(|f| CFloatSpec::parse(f).expect("valid spec"))
            .collect();
        let mut lines = 0;
        let mut failures = Vec::new();
        for line in REF.lines() {
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 1 + FORMATS.len() + 3, "line {line}");
            let v = f64::from_bits(u64::from_str_radix(cols[0], 16).expect("hex"));
            for (i, spec) in specs.iter().enumerate() {
                let expect = cols[1 + i];
                if expect == "-" {
                    continue;
                }
                let got = spec.format(v);
                if got != expect {
                    failures.push(format!(
                        "{} of {v:e} ({}): got {got:?}, C {expect:?}",
                        FORMATS[i], cols[0]
                    ));
                }
            }
            let base = 1 + FORMATS.len();
            let got = [
                fmt_double(v),
                fmt_g_showpoint(v, 7),
                format!("{}|", status_double(v)),
            ];
            for (k, g) in got.iter().enumerate() {
                if g != cols[base + k] {
                    failures.push(format!(
                        "ostream #{k} of {v:e}: got {g:?}, C {:?}",
                        cols[base + k]
                    ));
                }
            }
            lines += 1;
        }
        assert!(lines > 2800);
        assert!(
            failures.is_empty(),
            "{} mismatches:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn g_precision_sweep_matches_cpp() {
        let sweep = include_str!("../testdata/fmt_g_sweep.txt");
        let mut failures = Vec::new();
        for line in sweep.lines() {
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 19, "line {line}");
            let v = f64::from_bits(u64::from_str_radix(cols[0], 16).expect("hex"));
            for (p, expect) in cols[1..].iter().enumerate() {
                let got = fmt_g(v, p);
                if got != *expect {
                    failures.push(format!("%.{p}g of {v:e}: got {got:?}, C {expect:?}"));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} mismatches:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn documented_examples() {
        assert_eq!(fmt_double(52_456_050.0), "5.24560e+07"); // dtoa QUIRK
        assert_eq!(fmt_double(52_456_150.0), "5.24562e+07");
        assert_eq!(fmt_double(1e-5), "1e-05");
        assert_eq!(fmt_double(1234567.0), "1.23457e+06");
        assert_eq!(fmt_double(f64::INFINITY), "inf");
        assert_eq!(fmt_double(f64::NAN), "nan");
        assert_eq!(fmt_double(-0.0), "-0");
        assert_eq!(fmt_double(0.1), "0.1");
        assert_eq!(fmt_fixed(2.5, 0), "2");
        assert_eq!(fmt_fixed(1.0 / 3.0, 3), "0.333");
        assert_eq!(fmt_sci(123.0, 2), "1.23e+02");
        assert_eq!(fmt_g_showpoint(100.0, 7), "100.0000");
        assert_eq!(status_double(0.0), "0.000000 ");
        assert_eq!(status_int(42, 6), "42    ");
        assert_eq!(pad_right_justified("7", 3), "  7");
        assert_eq!(format_c("%7.1e", 12345.0).as_deref(), Ok("1.2e+04"));
        assert_eq!(format_c("%lf", 1.5).as_deref(), Ok("1.500000"));
        assert!(format_c("%d", 1.0).is_err());
        assert!(format_c("7.2f", 1.0).is_err());
    }
}
