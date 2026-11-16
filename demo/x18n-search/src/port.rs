//! The Rust port of `packages/core/src/fuzzy.ts`.

#![forbid(unsafe_code)]

use crate::utf16::{index_of, units};

/// Port of `fuzzyScore` (`packages/core/src/fuzzy.ts:2`).
///
/// Subsequence match score of `query` in `text`, case-insensitive; higher is better, -1 when it
/// does not match. Substring hits rank above every subsequence. Positions are UTF-16 code
/// units, as in JS, so scores match for non-ASCII text too.
pub fn fuzzy_score(query: String, text: String) -> f64 {
    let q = query.to_lowercase();
    let s = units(&text.to_lowercase());
    if let Some(exact) = index_of(&s, &units(&q), 0) {
        return 2000.0 - exact.min(1000) as f64;
    }
    let mut score = 0.0;
    // `at` is the UTF-16 index of the last match; JS starts it at -1.
    let mut next_from = 0;
    let mut previous: Option<usize> = None;
    // `for (const char of q)` walks code points, so astral characters stay whole.
    for c in q.chars() {
        let mut buf = [0u16; 2];
        let Some(next) = index_of(&s, c.encode_utf16(&mut buf), next_from) else {
            return -1.0;
        };
        let adjacent = previous.map_or(next == 0, |at| next == at + 1);
        score += if adjacent { 2.0 } else { 1.0 };
        previous = Some(next);
        next_from = next + 1;
    }
    score
}

/// The keys `fuzzy_score` searches, prepared once: lowercased UTF-16 in one buffer, and a mask
/// of the code units each key holds, so most keys that cannot match are rejected without being
/// read. `run` gives exactly `fuzzy_score(query, key)` for every key, on all cores.
pub struct FuzzyScoreIndex {
    units: Vec<u16>,
    /// Each key's range in `units`, and its mask.
    keys: Vec<(u32, u32, u64)>,
}

/// One bit per code unit (modulo 64): a key lacking any bit of the query cannot match it.
fn mask(units: &[u16]) -> u64 {
    units.iter().fold(0, |m, u| m | 1 << (u % 64))
}

impl FuzzyScoreIndex {
    pub fn new(items: Vec<String>) -> Self {
        let mut all = Vec::new();
        let mut keys = Vec::with_capacity(items.len());
        for item in &items {
            let start = all.len();
            all.extend(units(&item.to_lowercase()));
            let at = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
            keys.push((at(start), at(all.len()), mask(&all[start..])));
        }
        Self { units: all, keys }
    }

    pub fn run(&self, query: String) -> Result<Vec<f64>, String> {
        use rayon::prelude::*;
        let lower = query.to_lowercase();
        let q = units(&lower);
        // `for (const char of q)` walks code points: each is one or two code units.
        let chars: Vec<Vec<u16>> = lower
            .chars()
            .map(|c| c.encode_utf16(&mut [0; 2]).to_vec())
            .collect();
        let need = mask(&q);
        Ok(self
            .keys
            .par_iter()
            .with_min_len(1024)
            .map(|&(a, b, m)| {
                if m & need == need {
                    score(&q, &chars, &self.units[a as usize..b as usize])
                } else {
                    -1.0
                }
            })
            .collect())
    }
}

/// The first index at or after `from` where `needle` starts in `hay`.
fn find(hay: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    let (&first, rest) = needle.split_first()?;
    let last = hay.len().checked_sub(needle.len())?;
    (from..=last).find(|&i| hay[i] == first && hay[i + 1..i + needle.len()] == *rest)
}

/// `fuzzy_score` on a lowercased query and key.
fn score(q: &[u16], chars: &[Vec<u16>], s: &[u16]) -> f64 {
    let exact = if q.is_empty() { Some(0) } else { find(s, q, 0) };
    if let Some(exact) = exact {
        return 2000.0 - exact.min(1000) as f64;
    }
    let mut score = 0.0;
    let mut next_from = 0;
    let mut previous: Option<usize> = None;
    for c in chars {
        let Some(next) = find(s, c, next_from) else {
            return -1.0;
        };
        let adjacent = previous.map_or(next == 0, |at| next == at + 1);
        score += if adjacent { 2.0 } else { 1.0 };
        previous = Some(next);
        next_from = next + 1;
    }
    score
}

#[cfg(test)]
mod tests {
    use super::fuzzy_score;

    #[test]
    fn scores_like_js() {
        assert_eq!(fuzzy_score("b".into(), "abc".into()), 1999.0);
        assert_eq!(fuzzy_score("ac".into(), "abc".into()), 3.0);
        assert_eq!(fuzzy_score("x".into(), "abc".into()), -1.0);
        // "😀" is two UTF-16 units: the substring starts at index 3 in JS.
        assert_eq!(fuzzy_score("b😀".into(), "a😀b😀c".into()), 1997.0);
    }

    #[test]
    fn index_scores_like_the_port() {
        let keys = ["abc", "a😀b😀c", "Settings.Title", "", "ΣΊΣΥΦΟΣ", "x.y.z"];
        let index = super::FuzzyScoreIndex::new(keys.iter().map(|k| k.to_string()).collect());
        for q in ["", "b", "ac", "x", "b😀", "SET", "st", "σ", "zz", "😀c"] {
            let want: Vec<f64> = keys
                .iter()
                .map(|k| fuzzy_score(q.into(), k.to_string()))
                .collect();
            assert_eq!(index.run(q.into()).unwrap(), want, "{q}");
        }
    }
}
