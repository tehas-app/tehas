//! Argument grammars more than one program takes.

/// A number of at most `max` digits with no leading zero; "0" only when
/// `zero` allows it.
pub fn digits(s: &str, max: usize, zero: bool) -> Option<i64> {
    if zero && s == "0" {
        return Some(0);
    }
    let b = s.as_bytes();
    let ok = !b.is_empty()
        && b.len() <= max
        && (b'1'..=b'9').contains(&b[0])
        && b.iter().all(u8::is_ascii_digit);
    if ok {
        s.parse().ok()
    } else {
        None
    }
}

/// A count or an exit status.
pub fn count(s: &str) -> Option<i64> {
    digits(s, 9, true)
}

/// Lower-case hex of `min` to `max` digits.
pub fn is_hex(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// An image id: sha256: and 64 hex digits.
pub fn is_image(s: &str) -> bool {
    s.strip_prefix("sha256:").is_some_and(|h| is_hex(h, 64, 64))
}

/// The two spellings of the millwright's word. The store writes `millwright`;
/// a record or a row written before #686 part 1 still says `fixer`, and the
/// two name one class. Every program that reads a record's `how` matches this
/// pair: the store's rows and its filters, analytics' marks on a planning
/// answer, and the MCP server's filter of `factory_sessions`. Part 5 of #686
/// removes the old word, and with it the arm of each match that needs it.
pub const MILLWRIGHT_WORDS: [&str; 2] = ["millwright", "fixer"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts() {
        assert_eq!(count("0"), Some(0));
        assert_eq!(count("7"), Some(7));
        assert_eq!(count("999999999"), Some(999_999_999));
        for no in ["", "00", "07", "1000000000", "-1", "+1", "1 ", "1.0"] {
            assert_eq!(count(no), None, "{no:?}");
        }
        assert_eq!(digits("0", 9, false), None);
        assert_eq!(digits("12345678901", 11, false), Some(12_345_678_901));
    }

    #[test]
    fn hex_and_images() {
        let h = "a".repeat(64);
        assert!(is_hex("abc1234", 7, 40));
        assert!(!is_hex("abc123", 7, 40));
        assert!(!is_hex("ABC1234", 7, 40));
        assert!(!is_hex(&"a".repeat(41), 7, 40));
        assert!(is_image(&format!("sha256:{h}")));
        assert!(!is_image(&h));
        assert!(!is_image(&format!("sha256:{}", &h[1..])));
        assert!(!is_image(&format!("sha256:{}", h.to_uppercase())));
    }

    #[test]
    fn the_millwright_pair_is_the_new_word_and_the_old_one() {
        // The one list the store, analytics and the MCP server all read. Each
        // of them has a test of its own that its words are these two.
        assert_eq!(MILLWRIGHT_WORDS, ["millwright", "fixer"]);
    }
}
