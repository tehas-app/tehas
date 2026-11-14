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

/// The status words that are not an end: `running` is what a heartbeat carries
/// while a cycle works, and the empty word is the heartbeat of a cycle that
/// never sent one. A cycle whose end is on record is not still at work, so
/// neither word belongs in the endings a host reads. The store's writer of the
/// ten ends maps both to `unknown` (`ending_status`), and health names a row
/// that still carries one as `not an end`, so the two sides of one file agree.
/// Any other word is kept as the caller said it.
pub const NOT_ENDS: [&str; 2] = ["running", ""];

/// The word of an ending whose status is none of the ends, as health names it
/// in its file and in the line it says. It is a display word, not a status the
/// store records: the store's own mapping of such a word is `unknown`.
pub const NOT_AN_END: &str = "not an end";

/// The word `endings.status` holds for a status word of a cycle that did not
/// end (`NOT_ENDS`), and the word itself for any other.
pub fn ending_status(word: &str) -> &str {
    if NOT_ENDS.contains(&word) {
        "unknown"
    } else {
        word
    }
}

/// Whether a status word counts as an end at all: any word but `NOT_ENDS`, and
/// no word at all is not one.
pub fn an_end(word: &str) -> bool {
    !NOT_ENDS.contains(&word)
}

/// The two spellings of the millwright's word. The store writes `millwright`;
/// a record or a row written before #686 part 1 still says `fixer`, and the
/// two name one class. Every program that reads a record's `how` matches this
/// pair: the store's rows and its filters, analytics' marks on a planning
/// answer, and the MCP server's filter of `factory_sessions`. Part 5 of #686
/// removes the old word, and with it the arm of each match that needs it.
pub const MILLWRIGHT_WORDS: [&str; 2] = ["millwright", "fixer"];

/// The fewest characters an MCP API key has.
pub const MCP_KEY_MIN: usize = 32;

/// The keys of one MCP key setting, split on any whitespace, each at least
/// [`MCP_KEY_MIN`] characters of `A-Za-z0-9_-`. The MCP server reads its
/// lists with this, and the console reads the same settings with it before it
/// starts the server's container, so the two never disagree about a list. The
/// error names the setting and the key's place, never the key.
pub fn mcp_keys<'a>(raw: &'a str, setting: &str) -> Result<Vec<&'a str>, String> {
    let keys: Vec<&str> = raw.split_whitespace().collect();
    for (i, k) in keys.iter().enumerate() {
        let ok = k.len() >= MCP_KEY_MIN
            && k.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            return Err(format!(
                "{setting}: key {} is not at least {MCP_KEY_MIN} characters of A-Za-z0-9_-",
                i + 1,
            ));
        }
    }
    Ok(keys)
}

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
    fn mcp_keys_split_on_any_whitespace_and_name_a_bad_one_by_place() {
        let key = "abcdefghijklmnopqrstuvwxyz012345";
        assert_eq!(mcp_keys("", "X"), Ok(vec![]));
        assert_eq!(mcp_keys(" \t\r\n", "X"), Ok(vec![]));
        assert_eq!(mcp_keys(key, "X"), Ok(vec![key]));
        for sep in [" ", "\t", "\n", "\r\n", "\x0b", "\x0c"] {
            let two = format!("{key}{sep}{key}");
            assert_eq!(mcp_keys(&two, "X"), Ok(vec![key, key]), "{sep:?}");
        }
        let bad = format!("{key}\n{}+", &key[..31]);
        assert_eq!(
            mcp_keys(&bad, "TEHAS_MCP_KEYS").unwrap_err(),
            "TEHAS_MCP_KEYS: key 2 is not at least 32 characters of A-Za-z0-9_-"
        );
        assert!(mcp_keys(&key[..31], "X").is_err());
        assert!(mcp_keys(&format!("{key}="), "X").is_err());
    }

    #[test]
    fn the_millwright_pair_is_the_new_word_and_the_old_one() {
        // The one list the store, analytics and the MCP server all read. Each
        // of them has a test of its own that its words are these two.
        assert_eq!(MILLWRIGHT_WORDS, ["millwright", "fixer"]);
    }

    #[test]
    fn the_words_that_are_not_an_end_map_to_unknown() {
        // The set the store's writer and health's reader both name: the word of
        // a cycle still at work, and the empty word of one that never reported.
        assert_eq!(NOT_ENDS, ["running", ""]);
        assert_eq!(ending_status("running"), "unknown");
        assert_eq!(ending_status(""), "unknown");
        assert_eq!(ending_status("aborted"), "aborted");
        for word in NOT_ENDS {
            assert!(!an_end(word), "{word:?}");
        }
        assert!(an_end("merged"));
        // A sandbox that cannot push ends its start with this word (#1741). It
        // is no end the store names, so `cycles.end_reason` reads `unknown`
        // while `endings.status` keeps the word: the readers that name it are
        // part 3. No word is added to `NOT_ENDS` for it.
        assert!(an_end("push-fault"));
        assert_eq!(ending_status("push-fault"), "push-fault");
        assert_eq!(NOT_ENDS, ["running", ""]);
        assert_eq!(NOT_AN_END, "not an end");
    }
}
