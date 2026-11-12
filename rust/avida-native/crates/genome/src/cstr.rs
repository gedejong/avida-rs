//! String helpers with `cString` / `Apto::String` / `cStringUtil`
//! semantics (avida-core/source/tools/cString.{h,cc}, cStringUtil.cc;
//! libs/apto/include/apto/core/String.h, StringUtils.h).
//!
//! The `cString` operations come from `avida_config::string_util`; this
//! module adds the Apto variants it does not provide (Apto's whitespace set
//! and `Trim`, `Apto::EditDistance`, `Apto::Map` key order and
//! `Apto::NearMatch`). All operations are byte-oriented ASCII operations,
//! exactly like the C++.

use avida_config::string_util as su;

pub(crate) use avida_config::string_util::{
    as_double, as_int, edit_distance, format_f, pop, pop_word,
};

/// `cString::Trim()` on a copy.
pub(crate) fn trim(s: &str) -> String {
    su::trimmed(s)
}

/// `cString::ToLower()` / `Apto::BasicString::ToLower()` on a copy (ASCII only).
pub(crate) fn to_lower(s: &str) -> String {
    let mut out = s.to_string();
    su::to_lower(&mut out);
    out
}

/// `Apto::BasicString::IsWhitespace(int idx)`: space, FF, LF, CR, tab, VT.
#[inline]
pub(crate) fn is_apto_ws(b: u8) -> bool {
    matches!(b, b' ' | 0x0c | b'\n' | b'\r' | b'\t' | 0x0b)
}

/// `Apto::BasicString::Trim()` (Apto whitespace set, includes `\f` / `\v`).
pub(crate) fn apto_trim(s: &str) -> String {
    let b = s.as_bytes();
    let mut start_idx: isize = 0;
    while (start_idx as usize) < b.len() && is_apto_ws(b[start_idx as usize]) {
        start_idx += 1;
    }
    let mut end_idx: isize = b.len() as isize - 1;
    while end_idx > start_idx && is_apto_ws(b[end_idx as usize]) {
        end_idx -= 1;
    }
    let len = end_idx - start_idx + 1;
    if len <= 0 {
        return String::new();
    }
    s[start_idx as usize..(start_idx + len) as usize].to_string()
}

/// `Apto::EditDistance` (libs/apto/include/apto/core/StringUtils.h).
///
/// QUIRK: the C++ swaps the `cur` / `prev` rows inside the inner loop
/// instead of after it, so this is not the Levenshtein distance. Kept as is
/// because it decides which name `NearMatch` suggests in error messages.
pub(crate) fn apto_edit_distance(s1: &str, s2: &str) -> i32 {
    let s1 = s1.as_bytes();
    let s2 = s2.as_bytes();
    let sz1 = s1.len();
    let sz2 = s2.len();
    if sz1 == 0 {
        return sz2 as i32;
    }
    if sz2 == 0 {
        return sz1 as i32;
    }

    let mut cur = vec![0i32; sz2 + 1];
    let mut prev: Vec<i32> = (0..=sz2).map(|i| i as i32).collect();

    for (i, &c1) in s1.iter().enumerate() {
        cur[0] = i as i32 + 1;
        for j in 0..sz2 {
            let sub = prev[j] + if c1 == s2[j] { 0 } else { 1 };
            cur[j + 1] = (1 + cur[j]).min(1 + prev[1 + j]).min(sub);
            std::mem::swap(&mut cur, &mut prev);
        }
    }
    prev[sz2]
}

/// Iteration order of the keys of an `Apto::Map<Apto::String, V>` with the
/// default `HashBTree<.., 23>` storage, when keys were only ever inserted
/// (never removed): bucket by bucket, and in insertion order inside each
/// bucket. `keys` must be in first-insertion order and duplicate-free.
pub(crate) fn apto_map_key_order<'a>(keys: &[&'a str]) -> Vec<&'a str> {
    const HASH_FACTOR: u32 = 23;
    let mut buckets: Vec<Vec<&'a str>> = vec![Vec::new(); HASH_FACTOR as usize];
    for k in keys {
        // `HashKey<Apto::String, 23>::Hash`: byte sum modulo 23, as for cString.
        buckets[su::hash_key(k, HASH_FACTOR) as usize].push(k);
    }
    buckets.into_iter().flatten().collect()
}

/// `Apto::NearMatch(name, map.Keys())` for a string-keyed `Apto::Map`
/// whose keys (in first-insertion order) are `keys`.
pub(crate) fn apto_near_match(v: &str, keys: &[&str]) -> String {
    let mut best_match = String::new();
    let mut best_dist = v.len() as i32;
    for k in apto_map_key_order(keys) {
        let cur_dist = apto_edit_distance(v, k);
        if cur_dist < best_dist {
            best_dist = cur_dist;
            best_match = k.to_string();
        }
    }
    best_match
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pop_and_pop_word() {
        let mut s = "a:b:c".to_string();
        assert_eq!(pop(&mut s, ':'), "a");
        assert_eq!(s, "b:c");
        let mut s = ":x".to_string();
        assert_eq!(pop(&mut s, ':'), "");
        assert_eq!(s, "x");
        let mut s = "abc".to_string();
        assert_eq!(pop(&mut s, ':'), "abc");
        assert_eq!(s, "");

        let mut s = "  INST   nop-A  # x".to_string();
        assert_eq!(pop_word(&mut s), "INST");
        assert_eq!(s, "nop-A  # x");
        assert_eq!(pop_word(&mut s), "nop-A");
        assert_eq!(s, "# x");
    }

    #[test]
    fn trims() {
        assert_eq!(trim("  a b \t"), "a b");
        assert_eq!(trim("   "), "");
        assert_eq!(apto_trim("\x0b a \x0c"), "a");
        assert_eq!(apto_trim("   "), "");
        assert_eq!(apto_trim(""), "");
        assert_eq!(apto_trim("x"), "x");
        assert_eq!(to_lower("Nop-A"), "nop-a");
    }

    #[test]
    fn numbers() {
        assert_eq!(as_int("42"), 42);
        assert_eq!(as_int(" -7x"), -7);
        assert_eq!(as_int("010"), 8);
        assert_eq!(as_int("0x1F"), 31);
        assert_eq!(as_int("0x"), 0);
        assert_eq!(as_int("1.5e3"), 1);
        assert_eq!(as_int(""), 0);
        assert_eq!(as_int("4294967297"), 1);
        assert_eq!(as_int("99999999999999999999"), -1);
        assert_eq!(as_double("1.5"), 1.5);
        assert_eq!(as_double(".25e1x"), 2.5);
        assert_eq!(as_double("1e"), 1.0);
        assert_eq!(as_double("-3."), -3.0);
        assert_eq!(as_double("e5"), 0.0);
        assert_eq!(as_double("0x10"), 16.0);
        assert_eq!(as_double("0x1.8p1"), 3.0);
        assert_eq!(as_double("-inf"), f64::NEG_INFINITY);
        assert!(as_double("nan").is_nan());
    }

    #[test]
    fn edit_distances() {
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("nop-A", "nop-A"), 0);
        assert_eq!(edit_distance("nop-A", "nop-B"), 1);
        assert_eq!(
            apto_edit_distance("abc", "abc"),
            apto_edit_distance("abc", "abc")
        );
        assert_eq!(apto_edit_distance("", "ab"), 2);
    }

    #[test]
    fn hash_order() {
        // 'a' = 97 -> 97 % 23 = 5; 'b' = 98 -> 6; "\x17" = 23 -> 0.
        assert_eq!(
            apto_map_key_order(&["b", "a", "\x17"]),
            vec!["\x17", "a", "b"]
        );
        assert_eq!(format_f(1.0), "1.000000");
        assert_eq!(format_f(0.0), "0.000000");
    }
}
