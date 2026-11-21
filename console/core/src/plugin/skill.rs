//! A skill's text: Markdown a plugin keeps in its repository, which a chat
//! host puts in its instructions once a person has read and granted it
//! (docs/specs/2026-11-21-plugins-v2-chats.md, "Skills"). tehas reads it from
//! the pinned commit, never from what a run wrote, so the text a person read
//! is the text a chat gets. Here is only the check of the bytes.

/// The most bytes of one skill.
pub const BYTES_MAX: usize = 2048;

/// The text of a skill's file, or why it is refused: UTF-8, at most
/// [`BYTES_MAX`] bytes, not blank, no control character but a newline, and no
/// line that opens an HTML comment, which a chat would not show and a model
/// would still read.
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
    if s.lines().any(|l| l.trim_start().starts_with("<!--")) {
        return Err("a line opens an HTML comment".to_string());
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_markdown_is_the_skill() {
        let s = "## Interview\n\n- Ask one question at a time.\n";
        assert_eq!(text(s.as_bytes()), Ok(s));
        assert_eq!(
            text("x".repeat(BYTES_MAX).as_bytes()).map(str::len),
            Ok(BYTES_MAX)
        );
    }

    #[test]
    fn what_a_person_could_not_read_as_shown_is_refused() {
        let cases: [(&[u8], &str); 7] = [
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
        ];
        for (bytes, why) in cases {
            assert_eq!(text(bytes), Err(why.to_string()), "{bytes:?}");
        }
    }
}
