//! Tiny subsequence fuzzy matcher. Case-insensitive; higher score = better.
//! No external crate — deterministic and unit-tested.

/// Score `haystack` against `needle` as a case-insensitive subsequence.
/// `None` if `needle` is not a subsequence. Empty needle scores 0.
/// Rewards contiguous runs and an early first-match.
pub fn fuzzy_score(needle: &str, haystack: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.to_lowercase().chars().collect();
    let need: Vec<char> = needle.to_lowercase().chars().collect();

    let mut score = 0i32;
    let mut hi = 0usize;
    let mut first_match: Option<usize> = None;
    let mut prev_match: Option<usize> = None;

    for &nc in &need {
        let mut found = None;
        while hi < hay.len() {
            if hay[hi] == nc {
                found = Some(hi);
                hi += 1;
                break;
            }
            hi += 1;
        }
        let pos = found?; // not a subsequence
        first_match.get_or_insert(pos);
        score += 1;
        if let Some(prev) = prev_match {
            if pos == prev + 1 {
                score += 3; // contiguity bonus
            }
        }
        prev_match = Some(pos);
    }
    // Earlier first match is better (small penalty for a late start).
    score -= first_match.unwrap_or(0) as i32;
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_subsequence_and_misses_nonsubsequence() {
        assert!(fuzzy_score("ali", "Alice").is_some());
        assert!(fuzzy_score("ace", "Alice").is_some()); // gaps allowed
        assert!(fuzzy_score("zzz", "Alice").is_none());
        assert!(fuzzy_score("aliz", "Alice").is_none()); // 'z' breaks the subsequence
    }

    #[test]
    fn empty_needle_matches_everything() {
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }

    #[test]
    fn contiguous_and_prefix_score_higher() {
        // Prefix-contiguous "ali" beats scattered "ace".
        assert!(fuzzy_score("ali", "Alice").unwrap() > fuzzy_score("ace", "Alice").unwrap());
        // Same needle, earlier first match scores higher.
        assert!(fuzzy_score("bob", "Bob Jones").unwrap() > fuzzy_score("bob", "Sad Bob").unwrap());
    }
}
