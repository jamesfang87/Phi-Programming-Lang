pub(crate) fn is_escape(c: char) -> bool {
    matches!(c, '"' | '\'' | 'n' | 't' | 'r' | '\\' | '0')
}

fn unescape(c: char) -> char {
    match c {
        '\'' => '\'',
        '"' => '"',
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '\\' => '\\',
        '0' => '\0',
        other => other,
    }
}

pub(crate) fn decode_escapes(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            out.push(unescape(chars[i + 1]));
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub(crate) fn split_suffix(text: &str) -> (&str, Option<&str>) {
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'_' && bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            return (&text[..i], Some(&text[i + 1..]));
        }
    }
    (text, None)
}

pub(crate) fn strip_digit_separators(value: &str) -> String {
    value.chars().filter(|&c| c != '_').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_escape_set_accepted_by_the_lexer_is_exactly_what_decoding_handles() {
        for escaped in ['"', '\'', 'n', 't', 'r', '\\', '0'] {
            assert!(is_escape(escaped), "{escaped:?}");
        }
        let decoded = decode_escapes(&['\\', 'n', 'a']);
        assert_eq!(decoded, "\na");
    }

    #[test]
    fn a_trailing_backslash_passes_through_as_text() {
        assert_eq!(decode_escapes(&['a', '\\']), "a\\");
    }

    #[test]
    fn suffixes_split_at_the_underscore_before_a_letter() {
        assert_eq!(split_suffix("1_000_000_i64"), ("1_000_000", Some("i64")));
        assert_eq!(split_suffix("42"), ("42", None));
        assert_eq!(split_suffix("1_000_000"), ("1_000_000", None));
    }

    #[test]
    fn separators_are_stripped_but_the_rest_survives() {
        assert_eq!(strip_digit_separators("1_0.2_5"), "10.25");
    }
}
