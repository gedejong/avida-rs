//! Minimal ports of the `cString` operations the argument machinery needs
//! (`Pop`, `Trim`, `AsInt`, `AsDouble`; avida-core/source/tools/cString.cc).
//! Byte-oriented ASCII semantics like the C++. Private copy; the config crate
//! owns the shared `string_util`.

/// `cString::IsWhitespace(int pos)`: space, tab, CR, LF.
#[inline]
pub(crate) fn is_ws(b: u8) -> bool {
    b == b' ' || b == b'\t' || b == b'\r' || b == b'\n'
}

fn count_whitespace(s: &str, start: usize) -> usize {
    s.as_bytes()[start.min(s.len())..]
        .iter()
        .take_while(|b| is_ws(**b))
        .count()
}

/// `cString::Pop(const char delim)`: remove and return everything before the
/// first `delim` (the delimiter itself is dropped). If the delimiter is not
/// found the whole string is returned and `s` becomes empty.
pub(crate) fn pop(s: &mut String, delim: char) -> String {
    match s.find(delim) {
        Some(pos) => {
            let rv = s[..pos].to_string();
            s.drain(..pos + delim.len_utf8());
            rv
        }
        None => std::mem::take(s),
    }
}

/// `cString::Trim()`.
pub(crate) fn trim(s: &str) -> String {
    // Trim front
    let front = count_whitespace(s, 0);
    let s = &s[front..];
    // Trim trailing. QUIRK: the C++ loop condition `GetSize() - ws_count - 1 > 0`
    // never removes the character at index 0, which can only matter for an
    // all-whitespace string, and that was already emptied by the front trim.
    let bytes = s.as_bytes();
    let mut ws_count = 0usize;
    while (bytes.len() as isize - ws_count as isize - 1) > 0
        && is_ws(bytes[bytes.len() - ws_count - 1])
    {
        ws_count += 1;
    }
    s[..s.len() - ws_count].to_string()
}

fn c_isspace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `cString::AsInt()`: `static_cast<int>(strtol(str, NULL, 0))` on an LP64
/// platform (base auto-detection: `0x` hex, leading `0` octal; saturates to
/// the `long` range and then truncates to 32 bits).
pub(crate) fn as_int(s: &str) -> i32 {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && c_isspace(b[i]) {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut base: i128 = 10;
    if i < b.len() && b[i] == b'0' {
        if i + 2 < b.len() && (b[i + 1] == b'x' || b[i + 1] == b'X') && b[i + 2].is_ascii_hexdigit()
        {
            base = 16;
            i += 2;
        } else {
            base = 8;
        }
    }
    let mut acc: i128 = 0;
    while i < b.len() {
        let d = match b[i] {
            c @ b'0'..=b'9' => (c - b'0') as i128,
            c @ b'a'..=b'f' => (c - b'a') as i128 + 10,
            c @ b'A'..=b'F' => (c - b'A') as i128 + 10,
            _ => break,
        };
        if d >= base {
            break;
        }
        acc = acc * base + d;
        if acc > i64::MAX as i128 + 1 {
            acc = i64::MAX as i128 + 1; // saturate; keeps the loop from overflowing
        }
        i += 1;
    }
    let v: i64 = if neg {
        if acc > i64::MAX as i128 {
            i64::MIN
        } else {
            -(acc as i64)
        }
    } else if acc > i64::MAX as i128 {
        i64::MAX
    } else {
        acc as i64
    };
    v as i32
}

/// `cString::AsDouble()`: `strtod(str, NULL)`. Parses the longest valid
/// decimal floating-point prefix (plus `inf`/`infinity`/`nan`); returns 0.0
/// when there is none. Hexadecimal floats are not supported (they cannot pass
/// the `IsNumber` check that guards every double in the environment file).
pub(crate) fn as_double(s: &str) -> f64 {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && c_isspace(b[i]) {
        i += 1;
    }
    let start = i;
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let rest = &s[i..];
    let lower: String = rest
        .chars()
        .take(8)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if lower.starts_with("inf") {
        return if neg {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    if lower.starts_with("nan") {
        return f64::NAN;
    }
    let mant_start = i;
    let mut digits = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < b.len() && b[i] == b'.' {
        let mut j = i + 1;
        let mut frac = 0;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
            frac += 1;
        }
        if digits + frac > 0 {
            i = j;
            digits += frac;
        }
    }
    if digits == 0 {
        return 0.0;
    }
    let _ = mant_start;
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_digits_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_digits_start {
            i = j;
        }
    }
    s[start..i].parse::<f64>().unwrap_or(0.0)
}
