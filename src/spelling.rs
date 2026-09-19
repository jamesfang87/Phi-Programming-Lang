/// Returns the Levenshtein edit distance between `a` and `b`, counting a transposition of two
/// adjacent characters as a single edit.
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    let mut table = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in table.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in table[0].iter_mut().enumerate() {
        *cell = j;
    }

    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let substitute = table[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]);
            let mut best = substitute.min(table[i - 1][j] + 1).min(table[i][j - 1] + 1);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(table[i - 2][j - 2] + 1);
            }
            table[i][j] = best;
        }
    }

    table[a.len()][b.len()]
}

/// Returns whether `written` is likely a misspelling of `target`.
pub(crate) fn is_probable_typo_of(written: &str, target: &str) -> bool {
    if written.len() >= 2 && target.starts_with(written) {
        return true;
    }
    let allowed = if written.len() >= 4 { 2 } else { 1 };
    edit_distance(written, target) <= allowed
}

/// Returns the entries of `candidates` that are probable misspellings of `written`, ordered by
/// increasing edit distance and then by spelling, capped at `limit`.
pub(crate) fn nearest_names<'a>(
    written: &str,
    candidates: impl IntoIterator<Item = &'a str>,
    limit: usize,
) -> Vec<String> {
    let mut near: Vec<&str> = candidates
        .into_iter()
        .filter(|candidate| *candidate != written && is_probable_typo_of(written, candidate))
        .collect();
    near.sort_by_key(|candidate| (edit_distance(written, candidate), *candidate));
    near.dedup();
    near.truncate(limit);
    near.into_iter().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transpositions_count_as_one_edit() {
        assert_eq!(edit_distance("strcut", "struct"), 1);
    }

    #[test]
    fn a_two_character_prefix_of_a_keyword_is_a_probable_typo() {
        assert!(is_probable_typo_of("pub", "public"));
        assert!(!is_probable_typo_of("xyz", "public"));
    }

    #[test]
    fn an_exact_match_is_not_a_nearby_name() {
        assert_eq!(nearest_names("dot", ["dot"], 3), Vec::<String>::new());
    }

    #[test]
    fn nearby_names_are_ordered_by_edit_distance() {
        assert_eq!(
            nearest_names("abcd", ["abx", "abce"], 3),
            vec!["abce".to_string(), "abx".to_string()]
        );
    }

    #[test]
    fn nearby_names_are_capped_and_deduplicated() {
        assert_eq!(
            nearest_names("abcdef", ["abcdeg", "abcdeg", "abcdeh", "abcdei"], 2),
            vec!["abcdeg".to_string(), "abcdeh".to_string()]
        );
    }
}
