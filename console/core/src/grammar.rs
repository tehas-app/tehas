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

/// A full commit id as git writes it: 40 lower-case hex digits.
pub fn commit_id(s: &str) -> bool {
    is_hex(s, 40, 40)
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

/// The word that opens the line of an issue body that marks its spec.
pub const SPEC_MARK: &str = "Spec:";

/// The spec an issue body marks: the link of its file and the sha256 its
/// bytes must have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecMark {
    pub link: String,
    pub sha256: String,
}

impl SpecMark {
    /// The line that marks this spec, which [`spec_mark`] reads back as it.
    pub fn line(&self) -> String {
        format!("{SPEC_MARK} {} sha256:{}", self.link, self.sha256)
    }
}

/// The one line `Spec: <link> sha256:<64 hex>` of an issue body. A line is a
/// mark when it starts with [`SPEC_MARK`], outside a fenced code block. None:
/// no such line. Err says why a mark is not one tehas can read: a line of
/// another form, or more than one line. Whether tehas may fetch the link is the
/// link rule's to say (console/attach), not this grammar's.
pub fn spec_mark(body: &str) -> Result<Option<SpecMark>, String> {
    let mut found: Option<SpecMark> = None;
    let mut fence: Option<&str> = None;
    for line in body.lines() {
        let line = line.trim();
        if let Some(open) = fence {
            if line.starts_with(open) {
                fence = None;
            }
            continue;
        }
        if line.starts_with("```") || line.starts_with("~~~") {
            fence = Some(&line[..3]);
            continue;
        }
        let Some(rest) = line.strip_prefix(SPEC_MARK) else {
            continue;
        };
        if found.is_some() {
            return Err(format!("it has more than one `{SPEC_MARK}` line"));
        }
        let words: Vec<&str> = rest.split_whitespace().collect();
        let [link, sum] = words[..] else {
            return Err(format!(
                "its `{SPEC_MARK}` line is not a link and its sha256"
            ));
        };
        let Some(hex) = sum.strip_prefix("sha256:").filter(|h| is_hex(h, 64, 64)) else {
            return Err(format!(
                "its `{SPEC_MARK}` line does not end in sha256: and 64 lower-case hex digits"
            ));
        };
        found = Some(SpecMark {
            link: link.to_string(),
            sha256: hex.to_string(),
        });
    }
    Ok(found)
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
        assert!(commit_id(&"0a".repeat(20)));
        assert!(!commit_id(&"0A".repeat(20)));
        assert!(!commit_id(&"a".repeat(39)));
        assert!(!commit_id(&"a".repeat(41)));
        assert!(!commit_id(&format!("{}g", "a".repeat(39))));
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

    #[test]
    fn a_body_marks_its_spec_with_one_line() {
        let h = "0123456789abcdef".repeat(4);
        let link = "https://github.com/user-attachments/files/1/spec.md";
        let want = SpecMark {
            link: link.into(),
            sha256: h.clone(),
        };
        let body = format!("Make it fast.\r\n\r\nSpec: {link} sha256:{h}\r\n");
        assert_eq!(spec_mark(&body), Ok(Some(want.clone())));
        let spaced = format!("  Spec:\t{link}   sha256:{h}  ");
        assert_eq!(spec_mark(&spaced), Ok(Some(want)));
    }

    #[test]
    fn the_line_of_a_mark_reads_back_as_it() {
        let m = SpecMark {
            link: "https://github.com/user-attachments/files/1/spec.md".into(),
            sha256: "c".repeat(64),
        };
        assert_eq!(m.line(), format!("Spec: {} sha256:{}", m.link, m.sha256));
        assert_eq!(spec_mark(&m.line()), Ok(Some(m.clone())));
        assert_eq!(spec_mark(&format!("Words.\n\n{}\n", m.line())), Ok(Some(m)));
    }

    #[test]
    fn a_body_with_no_mark_marks_nothing() {
        for body in [
            "",
            "Make it fast.",
            "spec: x sha256:y",
            "- Spec: a list item is no mark",
            "The Spec: line is mid-sentence here",
            "```\nSpec: in a fence\n```",
            "~~~md\nSpec: in a fence\n~~~\nafter",
        ] {
            assert_eq!(spec_mark(body), Ok(None), "{body:?}");
        }
    }

    #[test]
    fn a_mark_of_another_form_is_refused_with_why() {
        let h = "a".repeat(64);
        let form = Err("its `Spec:` line is not a link and its sha256".to_string());
        let sum = Err(
            "its `Spec:` line does not end in sha256: and 64 lower-case hex digits".to_string(),
        );
        assert_eq!(spec_mark("Spec:"), form);
        assert_eq!(spec_mark("Spec: docs/specs/a.md"), form);
        assert_eq!(spec_mark(&format!("Spec: a b sha256:{h}")), form);
        assert_eq!(spec_mark("Spec: a sha256:"), sum);
        assert_eq!(spec_mark(&format!("Spec: a {h}")), sum);
        assert_eq!(spec_mark(&format!("Spec: a sha256:{}", &h[1..])), sum);
        assert_eq!(
            spec_mark(&format!("Spec: a sha256:{}", h.to_uppercase())),
            sum
        );
        assert_eq!(spec_mark(&format!("Spec: a sha512:{h}")), sum);
        // A fence closes, and a line after it is read again.
        assert_eq!(spec_mark("```\nx\n```\nSpec: a"), form);
    }

    #[test]
    fn two_marks_are_refused() {
        let line = format!("Spec: a sha256:{}", "b".repeat(64));
        assert_eq!(
            spec_mark(&format!("{line}\n{line}")),
            Err("it has more than one `Spec:` line".to_string())
        );
    }
}
