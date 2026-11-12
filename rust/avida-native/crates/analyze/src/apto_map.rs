//! `Apto::Map<Apto::String, V>` iteration order and `Apto::NearMatch`
//! (libs/apto/include/apto/core/MapStorage.h, StringUtils.h at the apto
//! commit the reference avida pins).
//!
//! Same algorithms as the crate-private helpers in `avida-genome`'s
//! `cstr.rs`; duplicated here so this crate does not widen that crate's API.

use avida_config::string_util as su;

/// `Apto::EditDistance`.
///
/// QUIRK: the C++ swaps the `cur` / `prev` rows inside the inner loop
/// instead of after it, so this is not the Levenshtein distance. It also
/// reads the never-initialised `cur` row on the first swaps; this port
/// zero-fills it (UNCERTAIN: the C++ result depends on heap contents there).
/// Only used to pick the "best match" in error messages.
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

/// Iteration order of a string-keyed `Apto::Map` (default `HashBTree<.., 23>`
/// storage) whose keys were inserted in `keys` order and never removed:
/// bucket by bucket (byte sum modulo 23), insertion order within a bucket.
pub(crate) fn apto_map_key_order<'a>(keys: &[&'a str]) -> Vec<&'a str> {
    const HASH_FACTOR: u32 = 23;
    let mut buckets: Vec<Vec<&'a str>> = vec![Vec::new(); HASH_FACTOR as usize];
    for k in keys {
        buckets[su::hash_key(k, HASH_FACTOR) as usize].push(k);
    }
    buckets.into_iter().flatten().collect()
}

/// `Apto::NearMatch(name, map.Keys())`: the first key (in map order) with
/// the smallest distance below `name.len()`, or "".
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
    fn bucket_order() {
        // "a" = 97 -> 5, "b" = 98 -> 6, "\x17" = 23 -> 0.
        assert_eq!(
            apto_map_key_order(&["b", "a", "\x17"]),
            vec!["\x17", "a", "b"]
        );
    }

    #[test]
    fn near_match_picks_close_key() {
        // The in-loop row swap makes this far from Levenshtein: the closest
        // spelling does not win.
        assert_eq!(
            apto_near_match("fitnes", &["merit", "fitness", "length"]),
            "length"
        );
        assert_eq!(apto_near_match("q", &["merit"]), "");
    }
}
