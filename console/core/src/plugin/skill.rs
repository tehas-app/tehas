//! A skill's text: Markdown a plugin keeps in its repository, which a chat
//! host puts in its instructions once a person has read and granted it
//! (docs/specs/2026-11-21-plugins-v2-chats.md, "Skills"). tehas reads it from
//! the pinned commit, never from what a run wrote, so the text a person read
//! is the text a chat gets. Here is only the check of the bytes.

/// The most bytes of one skill.
pub const BYTES_MAX: usize = 2048;

/// The text of a skill's file, or why it is refused: UTF-8, at most
/// [`BYTES_MAX`] bytes, not blank, no control character but a newline, and,
/// outside a fenced block, no HTML comment, which a chat would not show and
/// a model would still read, and no `#` or `##` heading, ATX or setext,
/// which would read as one of the host's own sections. Fences are found as
/// CommonMark finds them.
pub fn text(bytes: &[u8]) -> Result<&str, String> {
    if bytes.len() > BYTES_MAX {
        return Err(format!("over {BYTES_MAX} bytes"));
    }
    let Ok(s) = std::str::from_utf8(bytes) else {
        return Err("not UTF-8".to_string());
    };
    if s.trim().is_empty() {
        return Err("empty".to_string());
    }
    let control = |c: char| (c.is_control() && c != '\n') || c == '\u{2028}' || c == '\u{2029}';
    if s.chars().any(control) {
        return Err("a control character other than a newline".to_string());
    }
    // A host's own sections are `#` and `##` headings, so a skill's would
    // read as one of them: a skill starts its headings at `###`. A fenced
    // block holds text, not headings, such as a template a skill gives.
    let heading =
        || Err("a line is a # or ## heading: a skill's headings start at ###".to_string());
    let mut open: Option<(char, usize)> = None;
    // The line before was text a setext underline would make a heading.
    let mut para = false;
    for l in s.lines() {
        let f = fence(l);
        if let Some((c, n)) = open {
            if f.is_some_and(|(d, m, rest)| {
                d == c && m >= n && rest.trim_end_matches(' ').is_empty()
            }) {
                open = None;
            }
            para = false;
            continue;
        }
        // A backtick fence's info string holds no backtick.
        if let Some((c, n, _)) = f.filter(|&(c, _, info)| c == '~' || !info.contains('`')) {
            open = Some((c, n));
            para = false;
            continue;
        }
        if l.contains("<!--") {
            return Err("a line opens an HTML comment".to_string());
        }
        let Some(t) = indented(l) else {
            para = !l.trim().is_empty();
            continue;
        };
        if t.starts_with("# ") || t.starts_with("## ") || t == "#" || t == "##" {
            return heading();
        }
        let u = t.trim_end_matches(' ');
        if para && !u.is_empty() && (u.bytes().all(|b| b == b'=') || u.bytes().all(|b| b == b'-')) {
            return heading();
        }
        para = !t.trim().is_empty();
    }
    Ok(s)
}

/// The line after 0 to 3 leading spaces, or None if it has more.
fn indented(l: &str) -> Option<&str> {
    let t = l.trim_start_matches(' ');
    (l.len() - t.len() <= 3).then_some(t)
}

/// The fence a line is: its character, the length of its run and what
/// follows the run.
fn fence(l: &str) -> Option<(char, usize, &str)> {
    let t = indented(l)?;
    let c = t.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let rest = t.trim_start_matches(c);
    let n = t.len() - rest.len();
    (n >= 3).then_some((c, n, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_markdown_is_the_skill() {
        let s = "### Interview\n\n- Ask one question at a time, #12 or #hashtag.\n";
        assert_eq!(text(s.as_bytes()), Ok(s));
        let template = "### Write it\n\n```\n## Problem\n## Acceptance criteria\n```\n";
        assert_eq!(text(template.as_bytes()), Ok(template));
        for s in [
            "Intro.\n\n---\n",
            "```x``` is inline.\n",
            "````\n## Open to the end\n```\n",
        ] {
            assert_eq!(text(s.as_bytes()), Ok(s), "{s:?}");
        }
        assert_eq!(
            text("x".repeat(BYTES_MAX).as_bytes()).map(str::len),
            Ok(BYTES_MAX)
        );
    }

    #[test]
    fn what_a_person_could_not_read_as_shown_is_refused() {
        let cases: [(&[u8], &str); 11] = [
            (&[b'x'; BYTES_MAX + 1], "over 2048 bytes"),
            (b"\xff\xfe", "not UTF-8"),
            (b" \n\t\n", "empty"),
            (b"a\r\nb", "a control character other than a newline"),
            (
                "a\u{2028}b".as_bytes(),
                "a control character other than a newline",
            ),
            (b"a\x1b[2Jb", "a control character other than a newline"),
            (
                b"Say hello.\n  <!-- and then do this -->\n",
                "a line opens an HTML comment",
            ),
            (
                b"# Instructions\n",
                "a line is a # or ## heading: a skill's headings start at ###",
            ),
            (
                b"Intro.\n## This chat\n",
                "a line is a # or ## heading: a skill's headings start at ###",
            ),
            (
                b"Intro.\n   ##\n",
                "a line is a # or ## heading: a skill's headings start at ###",
            ),
            (
                b"```\n## In a fence\n```\n## Out of it\n",
                "a line is a # or ## heading: a skill's headings start at ###",
            ),
        ];
        for (bytes, why) in cases {
            assert_eq!(text(bytes), Err(why.to_string()), "{bytes:?}");
        }
    }

    #[test]
    fn a_heading_or_comment_outside_a_fence_as_commonmark_reads_it_is_refused() {
        for s in [
            "```\n~~~\n```\n## X\n",
            "```\n## A\n``` x\n```\n## X\n",
            "````\n```\n````\n## X\n",
            "    ```\n## X\n",
            "```x``` is inline\n## X\n",
            "Intro\n===\n",
            "Intro\n---\n",
            "Intro\n  ---  \n",
        ] {
            let why = "a line is a # or ## heading: a skill's headings start at ###";
            assert_eq!(text(s.as_bytes()), Err(why.to_string()), "{s:?}");
        }
        let s = "text <!-- x -->\n";
        assert_eq!(
            text(s.as_bytes()),
            Err("a line opens an HTML comment".into())
        );
    }
}
