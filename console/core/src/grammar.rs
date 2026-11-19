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
    for line in unfenced(body) {
        let Some(rest) = line.strip_prefix(SPEC_MARK) else {
            continue;
        };
        if found.is_some() {
            return Err(format!("it has more than one `{SPEC_MARK}` line"));
        }
        found = Some(spec_line(rest).map_err(|why| format!("its `{SPEC_MARK}` line {why}"))?);
    }
    Ok(found)
}

/// The trimmed lines of `body` outside fenced code blocks. A fence opens with
/// ``` or ~~~ and closes with a line that starts with the same three.
fn unfenced(body: &str) -> impl Iterator<Item = &str> {
    let mut fence: Option<&str> = None;
    body.lines().filter_map(move |line| {
        let line = line.trim();
        if let Some(open) = fence {
            if line.starts_with(open) {
                fence = None;
            }
            return None;
        }
        if line.starts_with("```") || line.starts_with("~~~") {
            fence = Some(&line[..3]);
            return None;
        }
        Some(line)
    })
}

/// What follows [`SPEC_MARK`] on a `Spec:` line: a link and `sha256:` with 64
/// lower-case hex digits. Err finishes the sentence "its `Spec:` line ...".
fn spec_line(rest: &str) -> Result<SpecMark, String> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    let [link, sum] = words[..] else {
        return Err("is not a link and its sha256".to_string());
    };
    let Some(hex) = sum.strip_prefix("sha256:").filter(|h| is_hex(h, 64, 64)) else {
        return Err("does not end in sha256: and 64 lower-case hex digits".to_string());
    };
    Ok(SpecMark {
        link: link.to_string(),
        sha256: hex.to_string(),
    })
}

/// The version of the pull request spec block that tehas reads and writes.
pub const SPEC_BLOCK_VERSION: u32 = 1;
/// The line that opens a pull request spec block of [`SPEC_BLOCK_VERSION`].
pub const SPEC_BLOCK_OPEN: &str = "<!-- tehas:spec v1 -->";
/// The line that closes a pull request spec block.
pub const SPEC_BLOCK_CLOSE: &str = "<!-- /tehas:spec -->";

/// One requirement of a spec as a pull request answers it: the spec's own id
/// (or a top heading's text), whether it is met, and where or why not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    pub id: String,
    pub met: bool,
    pub note: String,
}

/// The spec block of a pull request body: the spec it was built to, as the
/// issue marks it, and one line per requirement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecBlock {
    pub spec: SpecMark,
    pub requirements: Vec<Requirement>,
}

impl SpecBlock {
    /// The block as lines of a body, which [`spec_block`] reads back as it.
    pub fn text(&self) -> String {
        let mut out = format!("{SPEC_BLOCK_OPEN}\n{}\n", self.spec.line());
        for r in &self.requirements {
            let state = if r.met { "met" } else { "not met" };
            out.push_str(&format!("- {} {state}: {}\n", r.id, r.note));
        }
        out.push_str(SPEC_BLOCK_CLOSE);
        out.push('\n');
        out
    }
}

const SPEC_BLOCK_PREFIX: &str = "<!-- tehas:spec v";

/// The one spec block of a pull request body, outside fenced code blocks.
/// None: no block. Err says why a block is not one tehas can read.
pub fn spec_block(body: &str) -> Result<Option<SpecBlock>, String> {
    let mut found: Option<SpecBlock> = None;
    let mut inside: Option<Vec<&str>> = None;
    for line in unfenced(body) {
        if let Some(lines) = inside.as_mut() {
            if line == SPEC_BLOCK_CLOSE {
                found = Some(block_of(lines)?);
                inside = None;
            } else {
                lines.push(line);
            }
            continue;
        }
        if line == SPEC_BLOCK_OPEN {
            if found.is_some() {
                return Err("it has more than one spec block".to_string());
            }
            inside = Some(Vec::new());
        } else if let Some(v) = line
            .strip_prefix(SPEC_BLOCK_PREFIX)
            .and_then(|r| r.strip_suffix("-->"))
        {
            let v = v.trim();
            let other = v.bytes().all(|b| b.is_ascii_digit())
                && v.parse::<u32>().is_ok_and(|n| n != SPEC_BLOCK_VERSION);
            if !other {
                return Err(format!(
                    "its spec block's open line is not exactly `{SPEC_BLOCK_OPEN}`"
                ));
            }
            return Err(format!(
                "its spec block is version {v}, and tehas reads version {SPEC_BLOCK_VERSION}"
            ));
        }
    }
    if inside.is_some() {
        return Err(format!(
            "its spec block has no close line `{SPEC_BLOCK_CLOSE}`"
        ));
    }
    Ok(found)
}

/// Whether a pull request body answers to the spec whose bytes have the
/// sha256 `pinned`: it holds one spec block tehas can read, and the block names
/// that sha256. Only the sha256 is compared, not the link. Err finishes the
/// sentence "Its pull request does not answer to the approved spec: ...".
pub fn spec_answered(body: Option<&str>, pinned: &str) -> Result<(), String> {
    match spec_block(body.unwrap_or("")) {
        Ok(None) => Err("its body has no spec block".to_string()),
        Ok(Some(b)) if b.spec.sha256 == pinned => Ok(()),
        Ok(Some(b)) => Err(format!(
            "its spec block names sha256 {}, not {pinned}, the one approved",
            b.spec.sha256
        )),
        Err(why) => Err(format!("its spec block cannot be read: {why}")),
    }
}

fn block_of(lines: &[&str]) -> Result<SpecBlock, String> {
    let mut rest = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
        .map(|(i, l)| (i + 1, *l));
    let first = rest.next().map(|(_, l)| l).unwrap_or("");
    let Some(mark) = first.strip_prefix(SPEC_MARK) else {
        return Err(format!(
            "its spec block does not start with a `{SPEC_MARK}` line"
        ));
    };
    let spec =
        spec_line(mark).map_err(|why| format!("the `{SPEC_MARK}` line of its spec block {why}"))?;
    let mut requirements: Vec<Requirement> = Vec::new();
    for (n, line) in rest {
        let r = requirement(line).ok_or_else(|| {
            format!(
                "line {n} of its spec block is neither `- <id> met: <where>` nor `- <id> not met: <why>`"
            )
        })?;
        if requirements.iter().any(|o| o.id == r.id) {
            return Err(format!("its spec block names {} twice", r.id));
        }
        requirements.push(r);
    }
    if requirements.is_empty() {
        return Err("its spec block has no requirement line".to_string());
    }
    Ok(SpecBlock { spec, requirements })
}

/// `- <id> met: <text>` or `- <id> not met: <text>`, split at the first `:`.
fn requirement(line: &str) -> Option<Requirement> {
    let (head, note) = line.strip_prefix("- ")?.split_once(':')?;
    let (id, met) = match head.strip_suffix(" not met") {
        Some(id) => (id, false),
        None => (head.strip_suffix(" met")?, true),
    };
    let (id, note) = (id.trim(), note.trim());
    if id.is_empty() || id.contains('`') || note.is_empty() {
        return None;
    }
    Some(Requirement {
        id: id.to_string(),
        met,
        note: note.to_string(),
    })
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

    fn block() -> SpecBlock {
        SpecBlock {
            spec: SpecMark {
                link: "https://github.com/user-attachments/files/123/spec.md".into(),
                sha256: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08".into(),
            },
            requirements: vec![
                Requirement {
                    id: "R1".into(),
                    met: true,
                    note: "src/search.rs, search() with tests in tests/search.rs".into(),
                },
                Requirement {
                    id: "R2".into(),
                    met: false,
                    note: "the export needs an API the backend does not have yet".into(),
                },
            ],
        }
    }

    #[test]
    fn the_text_of_a_block_reads_back_as_it() {
        let b = block();
        assert_eq!(SPEC_BLOCK_VERSION, 1);
        assert_eq!(
            b.text(),
            "<!-- tehas:spec v1 -->\n\
             Spec: https://github.com/user-attachments/files/123/spec.md sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08\n\
             - R1 met: src/search.rs, search() with tests in tests/search.rs\n\
             - R2 not met: the export needs an API the backend does not have yet\n\
             <!-- /tehas:spec -->\n"
        );
        assert_eq!(spec_block(&b.text()), Ok(Some(b.clone())));
        let body = format!("Adds search.\n\n{}\nCloses #7.\n", b.text());
        assert_eq!(spec_block(&body), Ok(Some(b)));
    }

    #[test]
    fn a_block_reads_crlf_blank_lines_and_spaces() {
        let b = block();
        let crlf = b.text().replace('\n', "\r\n");
        assert_eq!(spec_block(&crlf), Ok(Some(b.clone())));
        let spaced = b
            .text()
            .replace("\n- R2", "\n\n  - R2")
            .replace("\nSpec:", "\n\nSpec:");
        assert_eq!(spec_block(&spaced), Ok(Some(b)));
    }

    #[test]
    fn a_body_with_no_block_has_none() {
        let mark = format!("Spec: a sha256:{}", "b".repeat(64));
        let fenced = format!("```\n{}```\n", block().text());
        let tilde = format!("~~~md\n{}~~~\nafter", block().text());
        for body in ["", "Adds search.", &mark, &fenced, &tilde, SPEC_BLOCK_CLOSE] {
            assert_eq!(spec_block(body), Ok(None), "{body:?}");
        }
        // The issue's pi-spec block is another thing.
        assert_eq!(
            spec_block("<!-- pi-spec v1 -->\nx\n<!-- /pi-spec -->"),
            Ok(None)
        );
    }

    #[test]
    fn an_id_that_ends_in_met_is_split_at_its_state() {
        let mut b = block();
        b.requirements[0].id = "Commitmet".into();
        b.requirements[1].id = "Rmet".into();
        let text = b.text();
        assert!(text.contains("- Commitmet met: "), "{text}");
        assert!(text.contains("- Rmet not met: "), "{text}");
        assert_eq!(spec_block(&text), Ok(Some(b)));
        let note = block().text().replace("search() with", "search(): with");
        assert_eq!(
            spec_block(&note).unwrap().unwrap().requirements[0].note,
            "src/search.rs, search(): with tests in tests/search.rs"
        );
    }

    #[test]
    fn a_heading_id_with_spaces_is_an_id() {
        let mut b = block();
        b.requirements[0].id = "Search results".into();
        assert_eq!(spec_block(&b.text()), Ok(Some(b)));
    }

    #[test]
    fn a_block_tehas_cannot_read_is_refused_with_why() {
        let text = block().text();
        let err = |body: &str| spec_block(body).unwrap_err();
        assert_eq!(
            err(text.strip_suffix("<!-- /tehas:spec -->\n").unwrap()),
            "its spec block has no close line `<!-- /tehas:spec -->`"
        );
        assert_eq!(
            err(&format!("{text}\n{text}")),
            "it has more than one spec block"
        );
        assert_eq!(
            err(&text.replace("v1", "v2")),
            "its spec block is version 2, and tehas reads version 1"
        );
        let inexact = "its spec block's open line is not exactly `<!-- tehas:spec v1 -->`";
        for open in [
            "<!-- tehas:spec v1-->",
            "<!-- tehas:spec v -->",
            "<!-- tehas:spec v1 draft -->",
        ] {
            assert_eq!(
                err(&text.replace(SPEC_BLOCK_OPEN, open)),
                inexact,
                "{open:?}"
            );
        }
        assert_eq!(
            err(&text.replace("Spec: ", "Specs: ")),
            "its spec block does not start with a `Spec:` line"
        );
        assert_eq!(
            err(&text.replace(".md sha256:", ".md x sha256:")),
            "the `Spec:` line of its spec block is not a link and its sha256"
        );
        assert_eq!(
            err(&text.replace("sha256:9f", "sha256:9F")),
            "the `Spec:` line of its spec block does not end in sha256: and 64 lower-case hex digits"
        );
        let neither = |n: usize| {
            format!("line {n} of its spec block is neither `- <id> met: <where>` nor `- <id> not met: <why>`")
        };
        let r2 = "- R2 not met: the export needs an API the backend does not have yet";
        for bad in [
            "- R2 done: x",
            "- R2 not met:",
            "- R2 not met: ",
            "- met: x",
            "-  met: x",
            "- `R2` met: x",
            "R2 met: x",
            "* R2 met: x",
            "- R2 met x",
            SPEC_BLOCK_OPEN,
        ] {
            assert_eq!(err(&text.replace(r2, bad)), neither(3), "{bad:?}");
        }
        assert_eq!(
            err(&text.replace("\n- R2", "\n\n- R2 met x\n- R2")),
            neither(4)
        );
        let none = format!(
            "{SPEC_BLOCK_OPEN}\n{}\n\n{SPEC_BLOCK_CLOSE}",
            block().spec.line()
        );
        assert_eq!(err(&none), "its spec block has no requirement line");
        assert_eq!(
            err(&format!("{SPEC_BLOCK_OPEN}\n{SPEC_BLOCK_CLOSE}")),
            "its spec block does not start with a `Spec:` line"
        );
        assert_eq!(
            err(&text.replace("- R2 not", "- R1 not")),
            "its spec block names R1 twice"
        );
    }

    #[test]
    fn a_body_answers_to_the_spec_its_block_names_by_sha256() {
        let b = block();
        let pinned = b.spec.sha256.clone();
        let body = format!("Fixes #7\r\n\r\n{}", b.text().replace('\n', "\r\n"));
        assert_eq!(spec_answered(Some(&body), &pinned), Ok(()));
        // The link is not compared: the same bytes at a new link are the same spec.
        let moved = body.replace("files/123/", "files/456/");
        assert_eq!(spec_answered(Some(&moved), &pinned), Ok(()));
        let none = Err("its body has no spec block".to_string());
        assert_eq!(spec_answered(None, &pinned), none);
        assert_eq!(spec_answered(Some(""), &pinned), none);
        assert_eq!(
            spec_answered(Some(&format!("```\n{}```\n", b.text())), &pinned),
            none
        );
        let other = "0".repeat(64);
        assert_eq!(
            spec_answered(Some(&body), &other),
            Err(format!(
                "its spec block names sha256 {pinned}, not {other}, the one approved"
            ))
        );
        let open = format!("{SPEC_BLOCK_OPEN}\n{}\n", b.spec.line());
        assert_eq!(
            spec_answered(Some(&open), &pinned),
            Err(format!(
                "its spec block cannot be read: its spec block has no close line `{SPEC_BLOCK_CLOSE}`"
            ))
        );
    }
}
