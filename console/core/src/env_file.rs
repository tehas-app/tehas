//! One reading of a `KEY=value` file: what `tehas disk` and `tehas setup`
//! take of `instance.env` and of `~/.config/tehas/env`, where `lib/config.sh`
//! sources them as shell.
//!
//! Two readers of one file must agree, or the same setting answers one command
//! and not the other. The file is the operator's and a program has no business
//! executing it, so it is read and not sourced: a comment, a blank line or a
//! line with no `=` is skipped rather than ending the file, `export ` is taken
//! off the front, and the last assignment of a key wins, because that is the
//! value a shell that sourced the file leaves in the environment.

/// The value `text` of a `KEY=value` file has for `name`, quotes taken off, or
/// nothing when the text holds no assignment of it. An empty value counts as
/// nothing: a key set to nothing is a key the file did not name.
pub fn value(text: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some(rest) = line.strip_prefix(&prefix) else {
            continue;
        };
        let value = unquote(unquote(rest.trim(), '"'), '\'');
        found = (!value.is_empty()).then(|| value.to_string());
    }
    found
}

// One layer of matching quotes; an unmatched one is part of the value.
fn unquote(v: &str, q: char) -> &str {
    match v.strip_prefix(q).and_then(|v| v.strip_suffix(q)) {
        Some(inner) if v.len() >= 2 => inner,
        _ => v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_or_blank_line_does_not_end_the_file() {
        let text = "# the settings of this instance\n\nTEHAS_SCRATCH_DIR=/var/tmp/tehas\n\
                    # a comment that is not an assignment\nTEHAS_DISK_WAIT=20\nno equals here\n";
        assert_eq!(
            value(text, "TEHAS_SCRATCH_DIR").as_deref(),
            Some("/var/tmp/tehas")
        );
        assert_eq!(value(text, "TEHAS_DISK_WAIT").as_deref(), Some("20"));
        assert_eq!(value(text, "TEHAS_REPO"), None);
    }

    #[test]
    fn the_last_assignment_wins() {
        let text = "TEHAS_REPO=a/b\nexport TEHAS_REPO=\"c/d\"\nOTHER_TEHAS_REPO=e/f\n";
        assert_eq!(value(text, "TEHAS_REPO").as_deref(), Some("c/d"));
        assert_eq!(value(text, "OTHER_TEHAS_REPO").as_deref(), Some("e/f"));
    }

    #[test]
    fn quotes_come_off_and_a_stray_one_stays() {
        let text = "X='y'\nZ=\"\nQ=\"a'b\"\n";
        assert_eq!(value(text, "X").as_deref(), Some("y"));
        assert_eq!(value(text, "Z").as_deref(), Some("\""));
        assert_eq!(value(text, "Q").as_deref(), Some("a'b"));
    }

    #[test]
    fn an_empty_assignment_is_no_value() {
        let text = "A=\nB=\"\"\nC=\n";
        assert_eq!(value(text, "A"), None);
        assert_eq!(value(text, "B"), None);
        assert_eq!(value(text, "C"), None);
    }

    #[test]
    fn a_later_empty_assignment_takes_the_key_away() {
        // What a shell leaves after `K=v; K=`: the key is set to nothing, so
        // the file names no value for it.
        let text = "A=one\nA=\n";
        assert_eq!(value(text, "A"), None);
    }

    #[test]
    fn a_key_needs_the_whole_name_before_the_equals() {
        let text = "TEHAS_HOME_X=/x\n";
        assert_eq!(value(text, "TEHAS_HOME"), None);
    }
}
