//! Port of `cString` and `cStringUtil` (avida-core/source/tools/cString.{h,cc},
//! cStringUtil.{h,cc}) plus the one `cStringList` helper the config code uses.
//!
//! The C++ `cString` is a byte string; all positions are byte offsets and all
//! character classification is ASCII. These functions work on `&str` /
//! `String` byte-wise with the same semantics. Because every delimiter and
//! class used is ASCII, slicing at such positions never splits a UTF-8
//! sequence; operations given arbitrary offsets (`substring`, `clip`, ...)
//! rebuild the result lossily if they would split one.
//!
//! The C++ guards bad arguments with `assert`s that are compiled out in
//! release builds; here out-of-range arguments are clamped instead of panicking.

/// `MAX_STRING_LENGTH` (cString.h).
pub const MAX_STRING_LENGTH: usize = 4096;
/// `CONTINUE_LINE_CHAR` (cString.h).
pub const CONTINUE_LINE_CHAR: u8 = b'\\';

#[inline]
fn mk(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(b).into_owned(),
    }
}

/// `cString::operator[](int)` on a const string: the byte at `i`, `0` for the
/// terminating NUL at `i == len` (and for anything out of range).
#[inline]
pub fn char_at(s: &str, i: usize) -> u8 {
    s.as_bytes().get(i).copied().unwrap_or(0)
}

#[inline]
fn is_ws_byte(b: u8) -> bool {
    b == b' ' || b == b'\t' || b == b'\r' || b == b'\n'
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

/// `cString::Compare(const char*)`: -1, 0 or 1 (chars compared as signed `char`).
pub fn compare(s: &str, other: &str) -> i32 {
    let a = s.as_bytes();
    let b = other.as_bytes();
    let mut i = 0;
    while i < a.len() && i < b.len() && a[i] == b[i] {
        i += 1;
    }
    if i == a.len() && i == b.len() {
        return 0;
    }
    if i < a.len() {
        let bc = b.get(i).copied().unwrap_or(0) as i8;
        if (a[i] as i8) > bc {
            return 1;
        }
    }
    -1
}

// ---------------------------------------------------------------------------
// Character / string classification
// ---------------------------------------------------------------------------

/// `cString::IsWhitespace(int pos)`: ' ', '\t', '\r', '\n'.
pub fn is_whitespace_at(s: &str, pos: usize) -> bool {
    is_ws_byte(char_at(s, pos))
}
/// `cString::IsUpperLetter(int pos)`.
pub fn is_upper_letter_at(s: &str, pos: usize) -> bool {
    char_at(s, pos).is_ascii_uppercase()
}
/// `cString::IsLowerLetter(int pos)`.
pub fn is_lower_letter_at(s: &str, pos: usize) -> bool {
    char_at(s, pos).is_ascii_lowercase()
}
/// `cString::IsLetter(int pos)`.
pub fn is_letter_at(s: &str, pos: usize) -> bool {
    char_at(s, pos).is_ascii_alphabetic()
}
/// `cString::IsNumber(int pos)`: digit, '-', '+', '.', 'e', 'E'.
pub fn is_number_at(s: &str, pos: usize) -> bool {
    let c = char_at(s, pos);
    c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E')
}
/// `cString::IsNumeric(int pos)`: '0'..='9'.
pub fn is_numeric_at(s: &str, pos: usize) -> bool {
    char_at(s, pos).is_ascii_digit()
}
/// `cString::IsAlphaNumeric(int pos)`: letter or `is_number_at`.
pub fn is_alpha_numeric_at(s: &str, pos: usize) -> bool {
    is_letter_at(s, pos) || is_number_at(s, pos)
}

/// `cString::IsWhitespace()`: all characters whitespace (true for empty).
pub fn is_whitespace(s: &str) -> bool {
    s.bytes().all(is_ws_byte)
}
/// `cString::IsUpperLetter()`.
pub fn is_upper_letter(s: &str) -> bool {
    s.bytes().all(|c| c.is_ascii_uppercase())
}
/// `cString::IsLowerLetter()`.
pub fn is_lower_letter(s: &str) -> bool {
    s.bytes().all(|c| c.is_ascii_lowercase())
}
/// `cString::IsLetter()`.
pub fn is_letter(s: &str) -> bool {
    s.bytes().all(|c| c.is_ascii_alphabetic())
}
/// `cString::IsNumber()`.
pub fn is_number(s: &str) -> bool {
    (0..s.len()).all(|i| is_number_at(s, i))
}
/// `cString::IsNumeric()`.
///
/// QUIRK: the C++ body tests `IsNumber(i)`, not `IsNumeric(i)`, so "-3.4e5"
/// counts as numeric.
pub fn is_numeric(s: &str) -> bool {
    is_number(s)
}
/// `cString::IsAlphaNumeric()`.
pub fn is_alpha_numeric(s: &str) -> bool {
    (0..s.len()).all(|i| is_alpha_numeric_at(s, i))
}

// ---------------------------------------------------------------------------
// Counting
// ---------------------------------------------------------------------------

/// `cString::CountWhitespace(start)`.
pub fn count_whitespace(s: &str, start: usize) -> usize {
    let b = s.as_bytes();
    if start >= b.len() {
        return 0;
    }
    b[start..].iter().take_while(|c| is_ws_byte(**c)).count()
}
/// `cString::CountWordsize(start)`.
pub fn count_wordsize(s: &str, start: usize) -> usize {
    let b = s.as_bytes();
    if start >= b.len() {
        return 0;
    }
    b[start..].iter().take_while(|c| !is_ws_byte(**c)).count()
}
/// `cString::CountLinesize(start)`: characters up to the next '\n'.
pub fn count_linesize(s: &str, start: usize) -> usize {
    let b = s.as_bytes();
    if start >= b.len() {
        return 0;
    }
    b[start..].iter().take_while(|c| **c != b'\n').count()
}
/// `cString::CountNumLines()`: 1 + number of '\n'.
pub fn count_num_lines(s: &str) -> usize {
    1 + s.bytes().filter(|c| *c == b'\n').count()
}
/// `cString::CountNumWords()`.
pub fn count_num_words(s: &str) -> usize {
    let mut num = 0;
    let mut pos = count_whitespace(s, 0);
    while pos < s.len() {
        pos += count_wordsize(s, pos);
        pos += count_whitespace(s, pos);
        num += 1;
    }
    num
}

// ---------------------------------------------------------------------------
// Searching
// ---------------------------------------------------------------------------

/// `cString::FindStr` core, shared by the `find*` functions. `pos` is already
/// clamped to `0..=len`. Returns the match position or -1.
fn find_bytes(hay: &[u8], needle: &[u8], mut pos: usize) -> i32 {
    let size = hay.len();
    let in0 = needle.first().copied().unwrap_or(0);
    while pos < size {
        if size - pos < needle.len() {
            return -1;
        }
        if hay[pos] == in0 {
            let mut i = 1;
            while i < needle.len() {
                if hay[pos + i] != needle[i] {
                    break;
                }
                i += 1;
            }
            // QUIRK: with an empty needle `i == 0 != in_size` never holds for
            // a non-NUL character, so an empty needle is (almost) never found.
            if i == needle.len() && !needle.is_empty() {
                return pos as i32;
            }
        }
        pos += 1;
    }
    -1
}

/// `cString::Find(char, pos)`: first occurrence at or after `pos`, or -1.
pub fn find_char(s: &str, c: char, pos: i32) -> i32 {
    let mut buf = [0u8; 4];
    let enc = c.encode_utf8(&mut buf).as_bytes();
    if enc.len() == 1 {
        let b = s.as_bytes();
        let mut p = pos.clamp(0, b.len() as i32) as usize;
        while p < b.len() {
            if b[p] == enc[0] {
                return p as i32;
            }
            p += 1;
        }
        -1
    } else {
        find(s, c.encode_utf8(&mut buf), pos)
    }
}

/// `cString::Find(const char*/cString&, pos)` (`FindStr`).
pub fn find(s: &str, needle: &str, pos: i32) -> i32 {
    let p = pos.clamp(0, s.len() as i32) as usize;
    find_bytes(s.as_bytes(), needle.as_bytes(), p)
}

/// `cString::FindWord(in, pos)`: find `in` delimited by whitespace.
///
/// QUIRK: the C++ prints a debug line to stderr on every iteration; that
/// output is not reproduced.
pub fn find_word(s: &str, word: &str, pos: i32) -> i32 {
    let size = s.len() as i32;
    let wsize = word.len() as i32;
    let mut pos = pos.clamp(0, size);
    while pos != -1 && pos + wsize < size {
        pos = find(s, word, pos);
        if pos >= 0 {
            let before = pos == 0 || is_whitespace_at(s, (pos - 1) as usize);
            let after = pos == size - 1 || is_whitespace_at(s, (pos + wsize) as usize);
            if before && after {
                return pos;
            }
            pos += 1;
        }
    }
    -1
}

/// `cString::Substring(start, size)`.
pub fn substring(s: &str, start: usize, size: usize) -> String {
    let b = s.as_bytes();
    let st = start.min(b.len());
    let en = st.saturating_add(size).min(b.len());
    mk(&b[st..en])
}

/// `cString::IsSubstring(in, start)`.
pub fn is_substring(s: &str, sub: &str, start: usize) -> bool {
    let b = s.as_bytes();
    let n = sub.as_bytes();
    if start + n.len() > b.len() {
        return false;
    }
    &b[start..start + n.len()] == n
}

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

/// `cString::GetWordAt(start)`: next word at/after `start`.
pub fn get_word_at(s: &str, start: usize) -> String {
    let pos = start + count_whitespace(s, start);
    let size = count_wordsize(s, pos);
    substring(s, pos, size)
}

/// `cString::GetWord(word_id)`.
pub fn get_word(s: &str, word_id: usize) -> String {
    let mut pos = 0;
    let mut cur = 0;
    while pos < s.len() && cur < word_id {
        pos += count_whitespace(s, pos);
        pos += count_wordsize(s, pos);
        cur += 1;
    }
    get_word_at(s, pos)
}

// ---------------------------------------------------------------------------
// Modifiers (operate on &mut String)
// ---------------------------------------------------------------------------

/// `cString::InsertStr`/`Insert(in, pos, excise)`: insert `ins` at `pos` and
/// remove `excise` bytes starting at `pos` (clamped to the string).
pub fn insert_str(s: &mut String, ins: &str, pos: usize, excise: usize) {
    let b = s.as_bytes();
    let pos = pos.min(b.len());
    let excise = excise.min(b.len() - pos);
    let mut out = Vec::with_capacity(b.len() + ins.len());
    out.extend_from_slice(&b[..pos]);
    out.extend_from_slice(ins.as_bytes());
    out.extend_from_slice(&b[pos + excise..]);
    *s = mk(&out);
}

/// `cString::Clip(pos, size)`: remove `size` characters at `pos`
/// (`size < 0` means to the end of the string).
pub fn clip(s: &mut String, pos: usize, size: i32) {
    let size = if size < 0 {
        s.len().saturating_sub(pos)
    } else {
        size as usize
    };
    insert_str(s, "", pos, size);
}
/// `cString::ClipFront(size)`.
pub fn clip_front(s: &mut String, size: usize) {
    insert_str(s, "", 0, size);
}
/// `cString::ClipEnd(size)`.
pub fn clip_end(s: &mut String, size: usize) {
    let pos = s.len().saturating_sub(size);
    insert_str(s, "", pos, size);
}

/// `cString::Replace(old, new, pos)`: replace the first match at/after `pos`;
/// returns its position or -1.
pub fn replace(s: &mut String, old: &str, new: &str, pos: i32) -> i32 {
    let loc = find(s, old, pos);
    if loc >= 0 {
        insert_str(s, new, loc as usize, old.len());
    }
    loc
}

/// `cString::Pop(delim)`: remove and return everything before the first
/// `delim` (dropping it); without a delimiter return everything and empty `s`.
pub fn pop(s: &mut String, delim: char) -> String {
    let pos = find_char(s, delim, 0);
    if pos >= 0 {
        let pos = pos as usize;
        let rv = substring(s, 0, pos);
        let dl = delim.len_utf8();
        insert_str(s, "", 0, pos + dl);
        rv
    } else {
        std::mem::take(s)
    }
}

/// `cString::PopWord()`: remove and return the first word, also removing the
/// leading and following whitespace.
pub fn pop_word(s: &mut String) -> String {
    if s.is_empty() {
        return String::new();
    }
    let start = count_whitespace(s, 0);
    let wsize = count_wordsize(s, start);
    let rv = if wsize > 0 {
        substring(s, start, wsize)
    } else {
        String::new()
    };
    let wend = start + wsize;
    let new_start = wend + count_whitespace(s, wend);
    insert_str(s, "", 0, new_start);
    rv
}

/// `cString::PopLine()`: `pop(s, '\n')`.
pub fn pop_line(s: &mut String) -> String {
    pop(s, '\n')
}

/// `cString::LeftJustify()`: strip leading whitespace, returning the count.
pub fn left_justify(s: &mut String) -> usize {
    let n = count_whitespace(s, 0);
    if n > 0 {
        insert_str(s, "", 0, n);
    }
    n
}

/// `cString::RightJustify()`: strip trailing whitespace, returning the count.
///
/// QUIRK: the scan stops at index 1 (`GetSize()-ws_count-1 > 0`), so the first
/// character is never removed: "   " becomes " ".
pub fn right_justify(s: &mut String) -> usize {
    let mut n = 0;
    while (s.len() as i64) - (n as i64) - 1 > 0 && is_whitespace_at(s, s.len() - n - 1) {
        n += 1;
    }
    if n > 0 {
        let pos = s.len() - n;
        insert_str(s, "", pos, n);
    }
    n
}

/// `cString::Trim()`: strip leading, then trailing whitespace (same quirk as
/// `right_justify` for the remainder).
pub fn trim(s: &mut String) {
    left_justify(s);
    right_justify(s);
}

/// Non-mutating `Trim()`.
pub fn trimmed(s: &str) -> String {
    let mut t = s.to_string();
    trim(&mut t);
    t
}

/// `cString::Reverse()` (byte order).
pub fn reverse(s: &mut String) {
    let mut v = s.as_bytes().to_vec();
    v.reverse();
    *s = mk(&v);
}

/// `cString::ToLower()`.
pub fn to_lower(s: &mut str) {
    s.make_ascii_lowercase();
}
/// `cString::ToUpper()`.
pub fn to_upper(s: &mut str) {
    s.make_ascii_uppercase();
}

/// `cString::CompressWhitespace()`: drop leading whitespace and collapse each
/// internal block to one ' '; trailing whitespace disappears.
pub fn compress_whitespace(s: &mut String) {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut ws = false;
    for &c in &b[count_whitespace(s, 0)..] {
        if is_ws_byte(c) {
            ws = true;
        } else {
            if ws {
                out.push(b' ');
                ws = false;
            }
            out.push(c);
        }
    }
    *s = mk(&out);
}

/// `cString::RemoveWhitespace()`.
pub fn remove_whitespace(s: &mut String) {
    let out: Vec<u8> = s.bytes().filter(|c| !is_ws_byte(*c)).collect();
    *s = mk(&out);
}

/// `cString::RemoveChar(c)`.
pub fn remove_char(s: &mut String, c: char) {
    let mut buf = [0u8; 4];
    let enc = c.encode_utf8(&mut buf).as_bytes();
    if enc.len() == 1 {
        let out: Vec<u8> = s.bytes().filter(|b| *b != enc[0]).collect();
        *s = mk(&out);
    } else {
        *s = s.replace(c, "");
    }
}

/// `cString::RemovePos(pos)`.
pub fn remove_pos(s: &mut String, pos: usize) {
    let out: Vec<u8> = s
        .bytes()
        .enumerate()
        .filter(|(i, _)| *i != pos)
        .map(|(_, c)| c)
        .collect();
    *s = mk(&out);
}

/// `cString::IsContinueLine()`: if the last non-whitespace character is the
/// continuation character `\`, remove it and return true.
pub fn is_continue_line(s: &mut String) -> bool {
    let mut j = s.len() as i64 - 1;
    while j >= 0 {
        if !is_whitespace_at(s, j as usize) {
            if char_at(s, j as usize) == CONTINUE_LINE_CHAR {
                remove_pos(s, j as usize);
                return true;
            }
            return false;
        }
        j -= 1;
    }
    false
}

/// `cString::ParseEscapeSequences()`: `\b \f \n \r \t \\ \" \' \?` are
/// translated; any other escaped character is dropped together with the
/// backslash; a trailing lone backslash is dropped.
pub fn parse_escape_sequences(s: &mut String) {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 1;
            if i == b.len() {
                break;
            }
            match b[i] {
                b'b' => out.push(0x08),
                b'f' => out.push(0x0c),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                c @ (b'\\' | b'"' | b'\'' | b'?') => out.push(c),
                _ => {}
            }
        } else {
            out.push(b[i]);
        }
        i += 1;
    }
    // The C++ assigns through a C string: stop at the first NUL.
    if let Some(z) = out.iter().position(|c| *c == 0) {
        out.truncate(z);
    }
    *s = mk(&out);
}

/// `cString::EjectStr(pos, excise)`: remove and return `excise` bytes at `pos`.
pub fn eject_str(s: &mut String, pos: usize, excise: usize) -> String {
    let out = substring(s, pos, excise);
    insert_str(s, "", pos, excise);
    out
}

/// `Apto::HashKey<cString>::Hash`: byte sum modulo `hash_factor`.
pub fn hash_key(s: &str, hash_factor: u32) -> u32 {
    let mut h: u32 = 0;
    for &c in s.as_bytes() {
        // `(unsigned int) key[i]` sign-extends a negative `char`.
        h = h.wrapping_add(c as i8 as i32 as u32);
    }
    h % hash_factor
}

// ---------------------------------------------------------------------------
// Numeric conversion: strtol(…, 0) / strtod
// ---------------------------------------------------------------------------

#[inline]
fn is_c_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `strtol(s, NULL, 0)` on a 64-bit `long` (saturating).
pub fn strtol(s: &str) -> i64 {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && is_c_space(b[i]) {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut base: u32 = 10;
    if i < b.len() && b[i] == b'0' {
        if i + 2 < b.len() && (b[i + 1] == b'x' || b[i + 1] == b'X') && b[i + 2].is_ascii_hexdigit()
        {
            base = 16;
            i += 2;
        } else {
            base = 8;
        }
    }
    let mut acc: u128 = 0;
    let mut overflow = false;
    while i < b.len() {
        let d = match (b[i] as char).to_digit(base) {
            Some(d) => d,
            None => break,
        };
        if !overflow {
            acc = acc * base as u128 + d as u128;
            if acc > (i64::MAX as u128) + 1 {
                overflow = true;
            }
        }
        i += 1;
    }
    if neg {
        if overflow || acc > (i64::MAX as u128) + 1 {
            i64::MIN
        } else {
            (-(acc as i128)) as i64
        }
    } else if overflow || acc > i64::MAX as u128 {
        i64::MAX
    } else {
        acc as i64
    }
}

/// `cString::AsInt()`: `(int) strtol(s, NULL, 0)` ("12abc" is 12, "" is 0,
/// "0x1f" is 31, "010" is 8; the 64-bit result is truncated to 32 bits).
pub fn as_int(s: &str) -> i32 {
    strtol(s) as i32
}

fn hex_float(b: &[u8], mut i: usize) -> Option<(f64, usize)> {
    // b[i..] starts right after "0x". Returns value and end index.
    let mut mant: f64 = 0.0;
    let mut digits = 0;
    let mut exp: i64 = 0;
    while i < b.len() && b[i].is_ascii_hexdigit() {
        mant = mant * 16.0 + (b[i] as char).to_digit(16)? as f64;
        digits += 1;
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        let mut j = i + 1;
        let mut fd = 0;
        while j < b.len() && b[j].is_ascii_hexdigit() {
            mant = mant * 16.0 + (b[j] as char).to_digit(16)? as f64;
            exp -= 4;
            fd += 1;
            j += 1;
        }
        if digits + fd > 0 {
            digits += fd;
            i = j;
        }
    }
    if digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'p' || b[i] == b'P') {
        let mut j = i + 1;
        let mut eneg = false;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            eneg = b[j] == b'-';
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            let mut e: i64 = 0;
            while j < b.len() && b[j].is_ascii_digit() {
                e = (e * 10 + (b[j] - b'0') as i64).min(100_000);
                j += 1;
            }
            exp += if eneg { -e } else { e };
            i = j;
        }
    }
    Some((mant * (2.0f64).powi(exp.clamp(-5000, 5000) as i32), i))
}

/// `strtod(s, NULL)`: longest valid prefix (decimal, hex float, inf, nan).
pub fn strtod(s: &str) -> f64 {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && is_c_space(b[i]) {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let sign = |v: f64| if neg { -v } else { v };
    let rest = &b[i..];
    let lower: Vec<u8> = rest
        .iter()
        .take(8)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if lower.starts_with(b"inf") {
        return sign(f64::INFINITY);
    }
    if lower.starts_with(b"nan") {
        return sign(f64::NAN);
    }
    if rest.len() >= 2 && rest[0] == b'0' && (rest[1] == b'x' || rest[1] == b'X') {
        if let Some((v, _)) = hex_float(b, i + 2) {
            return sign(v);
        }
        return sign(0.0); // just the "0"
    }
    // Decimal: digits [. digits] [e[+-]digits], at least one mantissa digit.
    let start = i;
    let mut j = i;
    let mut nd = 0;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
        nd += 1;
    }
    if j < b.len() && b[j] == b'.' {
        let mut k = j + 1;
        let mut fd = 0;
        while k < b.len() && b[k].is_ascii_digit() {
            k += 1;
            fd += 1;
        }
        if nd + fd > 0 {
            nd += fd;
            j = k;
        }
    }
    if nd == 0 {
        return 0.0;
    }
    if j < b.len() && (b[j] == b'e' || b[j] == b'E') {
        let mut k = j + 1;
        if k < b.len() && (b[k] == b'+' || b[k] == b'-') {
            k += 1;
        }
        if k < b.len() && b[k].is_ascii_digit() {
            while k < b.len() && b[k].is_ascii_digit() {
                k += 1;
            }
            j = k;
        }
    }
    let text = std::str::from_utf8(&b[start..j]).unwrap_or("0");
    sign(text.parse::<f64>().unwrap_or(0.0))
}

/// `cString::AsDouble()`.
pub fn as_double(s: &str) -> f64 {
    strtod(s)
}

// ---------------------------------------------------------------------------
// cStringUtil
// ---------------------------------------------------------------------------

/// C `printf("%f", x)`.
pub fn format_f(x: f64) -> String {
    if x.is_nan() {
        if x.is_sign_negative() {
            "-nan".to_string()
        } else {
            "nan".to_string()
        }
    } else if x.is_infinite() {
        if x < 0.0 { "-inf" } else { "inf" }.to_string()
    } else {
        format!("{:.6}", x)
    }
}

/// C `printf("%.*g", precision, x)`, which is also how `std::ostream` prints a
/// `double` with the default flags (precision 6).
pub fn format_g(x: f64, precision: usize) -> String {
    if x.is_nan() {
        return if x.is_sign_negative() { "-nan" } else { "nan" }.to_string();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    let p = precision.max(1);
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.to_string();
    }
    let sci = format!("{:.*e}", p - 1, x);
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let strip = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    if exp < -4 || exp >= p as i32 {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", strip(mant), sign, exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        strip(&format!("{:.*}", decimals, x))
    }
}

/// `std::ostream << double` with default flags: `format_g(x, 6)`.
pub fn format_ostream_double(x: f64) -> String {
    format_g(x, 6)
}

/// `cStringUtil::Convert(int)`: `"%d"`.
pub fn convert_int(x: i32) -> String {
    x.to_string()
}
/// `cStringUtil::Convert(double)`: `"%f"`.
pub fn convert_double(x: f64) -> String {
    format_f(x)
}
/// `cStringUtil::Convert(bool)`: "0" / "1".
pub fn convert_bool(x: bool) -> String {
    if x { "1" } else { "0" }.to_string()
}
/// `cStringUtil::Convert(const cString&, bool)`: `AsInt() != 0`.
pub fn convert_to_bool(s: &str) -> bool {
    as_int(s) != 0
}
/// `cStringUtil::Convert(const cString&, int)`.
pub fn convert_to_int(s: &str) -> i32 {
    as_int(s)
}
/// `cStringUtil::Convert(const cString&, double)`.
pub fn convert_to_double(s: &str) -> f64 {
    as_double(s)
}

/// `cStringUtil::StrLength`: bytes up to the first NUL.
pub fn str_length(s: &str) -> usize {
    s.bytes().position(|c| c == 0).unwrap_or(s.len())
}

/// `cStringUtil::ToRomanNumeral`.
pub fn to_roman_numeral(v: i32) -> String {
    if v < 0 {
        return format!("-{}", to_roman_numeral(v.wrapping_neg()));
    }
    if v > 3999 {
        return String::new();
    }
    const T: [(i32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut v = v;
    let mut out = String::new();
    for (n, r) in T {
        while v >= n {
            out.push_str(r);
            v -= n;
        }
    }
    out
}

/// `cStringUtil::Distance`: Hamming-style distance with `offset` shift.
pub fn distance(s1: &str, s2: &str, offset: i32) -> i32 {
    if offset < 0 {
        return distance(s2, s1, offset.wrapping_neg());
    }
    let size1 = s1.len() as i32;
    let size2 = s2.len() as i32;
    let overlap = (size1 - offset).min(size2);
    let mut diffs = size1 + size2 - 2 * overlap;
    for i in 0..overlap {
        if s1.as_bytes()[(i + offset) as usize] != s2.as_bytes()[i as usize] {
            diffs += 1;
        }
    }
    diffs
}

/// `cStringUtil::EditDistance(s1, s2)`.
pub fn edit_distance(string1: &str, string2: &str) -> i32 {
    let s1 = string1.as_bytes();
    let s2 = string2.as_bytes();
    let size1 = s1.len();
    let size2 = s2.len();
    if size1 == 0 {
        return size2 as i32;
    }
    if size2 == 0 {
        return size1 as i32;
    }
    let mut cur_row = vec![0i32; size1];
    let mut prev_row: Vec<i32> = (0..size1).map(|i| i as i32 + 1).collect();
    for i in 0..size2 {
        cur_row[0] = if s1[0] == s2[i] {
            i as i32
        } else if (i as i32) < prev_row[0] {
            i as i32 + 1
        } else {
            prev_row[0] + 1
        };
        for j in 1..size1 {
            if s1[j] == s2[i] {
                cur_row[j] = prev_row[j - 1];
            } else {
                cur_row[j] = if prev_row[j] < prev_row[j - 1] {
                    prev_row[j]
                } else {
                    prev_row[j - 1]
                };
                if cur_row[j - 1] < cur_row[j] {
                    cur_row[j] = cur_row[j - 1];
                }
                cur_row[j] += 1;
            }
        }
        std::mem::swap(&mut cur_row, &mut prev_row);
    }
    prev_row[size1 - 1]
}

/// `cStringUtil::EditDistance(s1, s2, info, gap)`: returns the distance and the
/// comma separated change list (`M<pos><old><new>`, `D<pos><old>`, `I<pos><new>`).
pub fn edit_distance_info(string1: &str, string2: &str, gap: u8) -> (i32, String) {
    let s1 = string1.as_bytes();
    let s2 = string2.as_bytes();
    let size1 = s1.len();
    let size2 = s2.len();
    if size1 == 0 {
        return (size2 as i32, String::new());
    }
    if size2 == 0 {
        return (size1 as i32, String::new());
    }
    let mut dist = vec![vec![0f64; size1 + 1]; size2 + 1];
    let mut mutm = vec![vec![b'N'; size1 + 1]; size2 + 1];
    for i in 0..=size1 {
        dist[0][i] = i as f64;
        mutm[0][i] = b'I';
    }
    for (i, row) in dist.iter_mut().enumerate() {
        row[0] = i as f64;
        mutm[i][0] = b'D';
    }
    mutm[0][0] = b'N';
    for i in 0..size2 {
        for j in 0..size1 {
            if s1[j] == s2[i] {
                dist[i + 1][j + 1] = dist[i][j];
                mutm[i + 1][j + 1] = b'N';
                continue;
            }
            let mut mut_dist = dist[i][j] + 1.0;
            if s1[j] == gap || s2[i] == gap {
                mut_dist -= 0.0001;
            }
            let ins_dist = dist[i + 1][j] + if s1[j] != gap { 1.0 } else { 0.0 };
            let del_dist = dist[i][j + 1] + if s2[i] != gap { 1.0 } else { 0.0 };
            if mut_dist < ins_dist && mut_dist < del_dist {
                dist[i + 1][j + 1] = mut_dist;
                mutm[i + 1][j + 1] = b'M';
            } else if ins_dist < del_dist {
                dist[i + 1][j + 1] = ins_dist;
                mutm[i + 1][j + 1] = b'I';
            } else {
                dist[i + 1][j + 1] = del_dist;
                mutm[i + 1][j + 1] = b'D';
            }
        }
    }
    let mut pos1 = size1;
    let mut pos2 = size2;
    let mut info: Vec<u8> = Vec::new();
    while pos1 > 0 || pos2 > 0 {
        if mutm[pos2][pos1] == b'N' {
            pos1 -= 1;
            pos2 -= 1;
            continue;
        }
        let old_char = if pos2 > 0 { s2[pos2 - 1] } else { 0 };
        let new_char = if pos1 > 0 { s1[pos1 - 1] } else { 0 };
        let mut m: Vec<u8> = Vec::new();
        match mutm[pos2][pos1] {
            b'M' => {
                m.push(b'M');
                m.extend((pos2 as i64 - 1).to_string().bytes());
                m.push(old_char);
                m.push(new_char);
                pos1 -= 1;
                pos2 -= 1;
            }
            b'D' => {
                m.push(b'D');
                m.extend((pos2 as i64 - 1).to_string().bytes());
                m.push(old_char);
                pos2 -= 1;
            }
            _ => {
                m.push(b'I');
                m.extend((pos1 as i64 - 1).to_string().bytes());
                m.push(new_char);
                pos1 -= 1;
            }
        }
        if !info.is_empty() {
            m.push(b',');
        }
        m.extend_from_slice(&info);
        info = m;
    }
    (dist[size2][size1] as i32, mk(&info))
}

/// `cStringUtil::ReturnArray`: parse "x,y..z,a" into integers (consumes `s`).
pub fn return_array(s: &mut String) -> Vec<i32> {
    let mut out = Vec::new();
    while !s.is_empty() {
        let mut chunk = pop(s, ',');
        if find_char(&chunk, '.', 0) != -1 {
            let start_str = pop(&mut chunk, '.');
            remove_char(&mut chunk, '.');
            let stop_str = pop_word(&mut chunk);
            let start = as_int(&start_str);
            let stop = as_int(&stop_str);
            let mut i = start;
            while i <= stop {
                out.push(i);
                if i == i32::MAX {
                    break;
                }
                i += 1;
            }
        } else {
            out.push(as_int(&chunk));
        }
    }
    out
}

/// `cStringList::Load(list, separator)`: repeatedly `Pop(separator)`.
pub fn split_list(list: &str, sep: char) -> Vec<String> {
    let mut l = list.to_string();
    let mut out = Vec::new();
    while !l.is_empty() {
        out.push(pop(&mut l, sep));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> String {
        x.to_string()
    }

    #[test]
    fn as_int_cases() {
        assert_eq!(as_int("12abc"), 12);
        assert_eq!(as_int(""), 0);
        assert_eq!(as_int("  -7"), -7);
        assert_eq!(as_int("+5"), 5);
        assert_eq!(as_int("0x1f"), 31);
        assert_eq!(as_int("0X1F"), 31);
        assert_eq!(as_int("010"), 8);
        assert_eq!(as_int("08"), 0);
        assert_eq!(as_int("0x"), 0);
        assert_eq!(as_int("abc"), 0);
        assert_eq!(as_int("1.9"), 1);
        assert_eq!(as_int("4294967296"), 0); // 2^32 truncates
        assert_eq!(as_int("4294967297"), 1);
        assert_eq!(as_int("99999999999999999999999"), -1); // LONG_MAX -> (int) -1
        assert_eq!(as_int("-99999999999999999999999"), 0); // LONG_MIN -> 0
        assert_eq!(as_int("2147483648"), i32::MIN);
    }

    #[test]
    fn as_double_cases() {
        assert_eq!(as_double("1.5x"), 1.5);
        assert_eq!(as_double(""), 0.0);
        assert_eq!(as_double("  .5"), 0.5);
        assert_eq!(as_double("5."), 5.0);
        assert_eq!(as_double("-1e3"), -1000.0);
        assert_eq!(as_double("1e"), 1.0);
        assert_eq!(as_double("1e+"), 1.0);
        assert_eq!(as_double("1E-2z"), 0.01);
        assert_eq!(as_double("."), 0.0);
        assert_eq!(as_double("0x10"), 16.0);
        assert_eq!(as_double("0x1p4"), 16.0);
        assert_eq!(as_double("0x.8"), 0.5);
        assert!(as_double("inf").is_infinite());
        assert!(as_double("-INFINITY") < 0.0);
        assert!(as_double("nan").is_nan());
        assert_eq!(as_double("0.0075"), 0.0075);
        assert_eq!(as_double("abc"), 0.0);
    }

    #[test]
    fn g_format() {
        assert_eq!(format_ostream_double(0.0075), "0.0075");
        assert_eq!(format_ostream_double(1.0), "1");
        assert_eq!(format_ostream_double(100000.0), "100000");
        assert_eq!(format_ostream_double(1234567.0), "1.23457e+06");
        assert_eq!(format_ostream_double(0.0000123), "1.23e-05");
        assert_eq!(format_ostream_double(0.0), "0");
        assert_eq!(format_ostream_double(-2.5), "-2.5");
        assert_eq!(format_ostream_double(0.1 + 0.2), "0.3");
        assert_eq!(format_g(1e100, 6), "1e+100");
        assert_eq!(format_g(999999.5, 6), "1e+06");
    }

    #[test]
    fn convert_formats() {
        assert_eq!(convert_double(0.0075), "0.007500");
        assert_eq!(convert_double(1.0), "1.000000");
        assert_eq!(convert_double(-2.5), "-2.500000");
        assert_eq!(convert_int(-12), "-12");
        assert_eq!(convert_bool(true), "1");
        assert_eq!(convert_bool(false), "0");
        assert!(convert_to_bool("2"));
        assert!(!convert_to_bool("0"));
        assert!(!convert_to_bool("x"));
    }

    #[test]
    fn pop_and_words() {
        let mut x = s("a,b,,c");
        assert_eq!(pop(&mut x, ','), "a");
        assert_eq!(x, "b,,c");
        assert_eq!(pop(&mut x, ','), "b");
        assert_eq!(pop(&mut x, ','), "");
        assert_eq!(x, "c");
        assert_eq!(pop(&mut x, ','), "c");
        assert_eq!(x, "");

        let mut x = s("  hello   world  ");
        assert_eq!(pop_word(&mut x), "hello");
        assert_eq!(x, "world  ");
        assert_eq!(pop_word(&mut x), "world");
        assert_eq!(x, "");
        assert_eq!(pop_word(&mut x), "");
        let mut x = s("   ");
        assert_eq!(pop_word(&mut x), "");
        assert_eq!(x, "");

        let mut x = s("l1\nl2\n\nl4");
        assert_eq!(pop_line(&mut x), "l1");
        assert_eq!(pop_line(&mut x), "l2");
        assert_eq!(pop_line(&mut x), "");
        assert_eq!(pop_line(&mut x), "l4");
        assert_eq!(x, "");
    }

    #[test]
    fn justify_quirks() {
        let mut x = s("  ab  ");
        assert_eq!(left_justify(&mut x), 2);
        assert_eq!(x, "ab  ");
        assert_eq!(right_justify(&mut x), 2);
        assert_eq!(x, "ab");
        let mut x = s("   ");
        assert_eq!(right_justify(&mut x), 2);
        assert_eq!(x, " ");
        let mut x = s("   ");
        trim(&mut x);
        assert_eq!(x, "");
        let mut x = s("\t x y \n");
        trim(&mut x);
        assert_eq!(x, "x y");
        assert_eq!(trimmed(" a "), "a");
    }

    #[test]
    fn whitespace_ops() {
        let mut x = s("  a \t b\n\nc  ");
        compress_whitespace(&mut x);
        assert_eq!(x, "a b c");
        let mut x = s(" a b\tc ");
        remove_whitespace(&mut x);
        assert_eq!(x, "abc");
        let mut x = s("a.b..c");
        remove_char(&mut x, '.');
        assert_eq!(x, "abc");
        let mut x = s("abc");
        remove_pos(&mut x, 1);
        assert_eq!(x, "ac");
        assert!(is_whitespace(""));
        assert!(is_whitespace(" \t\r\n"));
        assert!(!is_whitespace(" a"));
        assert_eq!(count_whitespace("  ab", 0), 2);
        assert_eq!(count_whitespace("  ab", 5), 0);
        assert_eq!(count_wordsize("ab  c", 0), 2);
        assert_eq!(count_linesize("ab\ncd", 0), 2);
        assert_eq!(count_num_lines("a\nb\n"), 3);
        assert_eq!(count_num_words("  a bb   ccc "), 3);
        assert_eq!(count_num_words(""), 0);
    }

    #[test]
    fn continue_line() {
        let mut x = s("abc \\  ");
        assert!(is_continue_line(&mut x));
        assert_eq!(x, "abc   ");
        let mut x = s("abc");
        assert!(!is_continue_line(&mut x));
        let mut x = s("a\\b");
        assert!(!is_continue_line(&mut x));
        let mut x = s("   ");
        assert!(!is_continue_line(&mut x));
    }

    #[test]
    fn classification() {
        assert!(is_number("-3.4e5"));
        assert!(is_numeric("-3.4e5")); // QUIRK
        assert!(!is_number("12a"));
        assert!(is_alpha_numeric("ab-1"));
        assert!(is_letter("abC"));
        assert!(!is_letter("ab1"));
        assert!(is_upper_letter("ABC"));
        assert!(is_lower_letter("abc"));
        assert!(is_number_at("a1", 1));
        assert!(!is_numeric_at("a1", 0));
        assert!(!is_whitespace_at("a", 5));
    }

    #[test]
    fn find_ops() {
        assert_eq!(find_char("hello", 'l', 0), 2);
        assert_eq!(find_char("hello", 'l', 3), 3);
        assert_eq!(find_char("hello", 'z', 0), -1);
        assert_eq!(find("hello", "llo", 0), 2);
        assert_eq!(find("hello", "lo!", 0), -1);
        assert_eq!(find("hello", "", 0), -1); // QUIRK
        assert_eq!(find("abcabc", "bc", 2), 4);
        assert_eq!(find("abc", "abcd", 0), -1);
        assert_eq!(find_word("foo bar baz", "bar", 0), 4);
        assert_eq!(find_word("foobar baz", "bar", 0), -1);
        assert_eq!(substring("hello", 1, 3), "ell");
        assert!(is_substring("hello", "ell", 1));
        assert!(!is_substring("hello", "ell", 2));
        assert!(!is_substring("hello", "lo!", 3));
    }

    #[test]
    fn word_access() {
        assert_eq!(get_word("  a bb  ccc", 0), "a");
        assert_eq!(get_word("  a bb  ccc", 1), "bb");
        assert_eq!(get_word("  a bb  ccc", 2), "ccc");
        assert_eq!(get_word("  a bb  ccc", 3), "");
        assert_eq!(get_word_at("ab cd", 2), "cd");
        assert_eq!(get_word("", 0), "");
    }

    #[test]
    fn modifiers() {
        let mut x = s("hello world");
        assert_eq!(replace(&mut x, "world", "there", 0), 6);
        assert_eq!(x, "hello there");
        assert_eq!(replace(&mut x, "zzz", "q", 0), -1);
        let mut x = s("a$x b$x");
        replace(&mut x, "$x", "1", 0);
        assert_eq!(x, "a1 b$x");
        let mut x = s("abcdef");
        clip(&mut x, 2, -1);
        assert_eq!(x, "ab");
        let mut x = s("abcdef");
        clip(&mut x, 1, 2);
        assert_eq!(x, "adef");
        clip_front(&mut x, 1);
        assert_eq!(x, "def");
        clip_end(&mut x, 1);
        assert_eq!(x, "de");
        let mut x = s("abc");
        insert_str(&mut x, "XY", 1, 1);
        assert_eq!(x, "aXYc");
        let mut x = s("abcdef");
        assert_eq!(eject_str(&mut x, 1, 3), "bcd");
        assert_eq!(x, "aef");
        let mut x = s("Hello");
        to_upper(&mut x);
        assert_eq!(x, "HELLO");
        to_lower(&mut x);
        assert_eq!(x, "hello");
        reverse(&mut x);
        assert_eq!(x, "olleh");
    }

    #[test]
    fn escapes() {
        let mut x = s("a\\nb\\tc\\\\d\\qe\\");
        parse_escape_sequences(&mut x);
        assert_eq!(x, "a\nb\tc\\de");
    }

    #[test]
    fn compare_matches_strcmp_sign() {
        assert_eq!(compare("abc", "abc"), 0);
        assert_eq!(compare("abc", "abd"), -1);
        assert_eq!(compare("abd", "abc"), 1);
        assert_eq!(compare("ab", "abc"), -1);
        assert_eq!(compare("abc", "ab"), 1);
        assert_eq!(compare("", ""), 0);
    }

    #[test]
    fn distances() {
        assert_eq!(distance("abc", "abc", 0), 0);
        assert_eq!(distance("abc", "abd", 0), 1);
        assert_eq!(distance("abc", "bc", 0), 3);
        assert_eq!(distance("abc", "bc", 1), 1); // the shifted-out 'a' counts
        assert_eq!(distance("bc", "abc", -1), 1);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", ""), 3);
        assert_eq!(edit_distance("abc", "abc"), 0);
        let (d, info) = edit_distance_info("abc", "abd", b' ');
        assert_eq!(d, 1);
        assert_eq!(info, "M2dc");
        let _ = info;
    }

    #[test]
    fn roman_and_misc() {
        assert_eq!(to_roman_numeral(1994), "MCMXCIV");
        assert_eq!(to_roman_numeral(4), "IV");
        assert_eq!(to_roman_numeral(0), "");
        assert_eq!(to_roman_numeral(4000), "");
        assert_eq!(to_roman_numeral(-9), "-IX");
        assert_eq!(str_length("abc"), 3);
        assert_eq!(hash_key("ABC", 17), hash_key("CBA", 17));
        let mut l = s("1,3..5,9");
        assert_eq!(return_array(&mut l), vec![1, 3, 4, 5, 9]);
        assert_eq!(split_list("a b  c", ' '), vec!["a", "b", "", "c"]);
        assert_eq!(split_list("", ' '), Vec::<String>::new());
    }
}
