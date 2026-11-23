//! One reading of a `KEY=value` file: what `tehas disk` and `tehas setup`
//! take of `instance.env` and of `~/.config/tehas/env`, where `lib/config.sh`
//! sources them as shell.
//!
//! Two readers of one file must agree, or the same setting answers one command
//! and not the other. The file is the operator's and a program has no business
//! executing it, so it is read and not sourced: a comment, a blank line or a
//! line with no `=` is skipped rather than ending the file, `export ` is taken
//! off the front, and the last assignment of a key wins, because that is the
//! value a shell that sourced the file leaves in the environment. Where the
//! shell would expand a value, [`literal`] refuses it, so the two never part
//! in silence.
//!
//! [`entries`] reads a whole file strictly, for a check of a host env file
//! that is not this machine's: there a line that is no assignment is refused,
//! and a value of exactly [`REDACTED`], unquoted, stands for one that is set
//! and not shown. No working file holds that line: a shell that sources it
//! stops at the `<`. Quoted, `'<redacted>'` is an ordinary value.

use std::collections::BTreeMap;

/// The value that stands for a set value not shown, written unquoted.
pub const REDACTED: &str = "<redacted>";

/// What a file gives a name: a value, quotes taken off, or [`REDACTED`].
#[derive(Debug, PartialEq, Eq)]
pub enum Entry {
    Value(String),
    Redacted,
}

/// The value `text` of a `KEY=value` file has for `name`, quotes taken off, or
/// nothing when the text holds no assignment of it. An empty value counts as
/// nothing: a key set to nothing is a key the file did not name.
pub fn value(text: &str, name: &str) -> Option<String> {
    last(text, name).and_then(|(value, _)| (!value.is_empty()).then_some(value))
}

/// As [`value`], but a value a shell would expand is refused rather than
/// taken as written: a `$` or a backtick outside single quotes. The shell
/// that sources the file and this reader would part on it. The refusal names
/// the key and not the value, which may be a token.
pub fn literal(text: &str, name: &str) -> Result<Option<String>, String> {
    match last(text, name) {
        Some((value, false)) if value.contains(['$', '`']) => Err(format!(
            "{name} holds a $ or a backtick: the file is read, not run as shell, so write the value itself"
        )),
        Some((value, _)) => Ok((!value.is_empty()).then_some(value)),
        None => Ok(None),
    }
}

/// Every name `text` assigns, each with what its last assignment gives, for
/// a file that must hold nothing else. Err names the first line that is not
/// `NAME=value`, by its number, or the name whose value a shell would
/// expand; never a value.
pub fn entries(text: &str) -> Result<BTreeMap<String, Entry>, String> {
    let mut found = BTreeMap::new();
    for (at, line) in text.lines().enumerate() {
        let Some(line) = statement(line) else {
            continue;
        };
        let Some((name, rest)) = line
            .split_once('=')
            .filter(|(name, _)| crate::settings::load::is_name(name.as_bytes()))
        else {
            return Err(format!("line {} is not NAME=value", at + 1));
        };
        let rest = rest.trim();
        let entry = if rest == REDACTED {
            Entry::Redacted
        } else {
            let (value, single) = parse(rest);
            if !single && value.contains(['$', '`']) {
                return Err(format!(
                    "{name} holds a $ or a backtick: the file is read, not run as shell, so write the value itself"
                ));
            }
            Entry::Value(value)
        };
        found.insert(name.to_string(), entry);
    }
    Ok(found)
}

/// A line without its blanks and its `export `, or None for a blank line or
/// a comment.
fn statement(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    Some(line.strip_prefix("export ").unwrap_or(line).trim())
}

/// A value as written, quotes taken off, and whether it was in single quotes.
fn parse(rest: &str) -> (String, bool) {
    let single = unquote(rest, '\'') != rest;
    let value = unquote(unquote(rest, '"'), '\'');
    (value.to_string(), single)
}

/// The last assignment of `name`, quotes taken off, and whether it was in
/// single quotes.
fn last(text: &str, name: &str) -> Option<(String, bool)> {
    let prefix = format!("{name}=");
    text.lines()
        .filter_map(statement)
        .filter_map(|line| line.strip_prefix(&prefix))
        .map(|rest| parse(rest.trim()))
        .next_back()
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
    fn a_value_a_shell_would_expand_is_refused_and_not_shown() {
        for text in [
            "GH_TOKEN=$(cat /secret/token)\n",
            "GH_TOKEN=\"$HOME/x\"\n",
            "GH_TOKEN=`cat t`\n",
            "GH_TOKEN=${X}\n",
        ] {
            let why = literal(text, "GH_TOKEN").unwrap_err();
            assert!(why.starts_with("GH_TOKEN holds a $ or a backtick"), "{why}");
            assert!(!why.contains("secret") && !why.contains("HOME"), "{why}");
        }
        // Single quotes keep a `$` as the shell keeps it.
        assert_eq!(literal("X='a$b'\n", "X"), Ok(Some("a$b".into())));
        assert_eq!(literal("X=/srv/x\n", "X"), Ok(Some("/srv/x".into())));
        assert_eq!(literal("X=\n", "X"), Ok(None));
        assert_eq!(literal("", "X"), Ok(None));
        // The last assignment decides, as in a shell.
        assert_eq!(literal("X=$Y\nX=/z\n", "X"), Ok(Some("/z".into())));
    }

    #[test]
    fn entries_read_every_assignment_and_the_unquoted_marker() {
        let text =
            "# host\n\nexport TEHAS_MODEL_KEY_A=<redacted>\nTEHAS_MODEL_KEY_B='<redacted>'\n\
                    TEHAS_MCP=on\nTEHAS_MCP=off\nTEHAS_MODEL_KEY_C=\"<redacted>\"\n";
        let got = entries(text).unwrap();
        assert_eq!(got["TEHAS_MODEL_KEY_A"], Entry::Redacted);
        // Quoted, it is a value a shell would set, so it is checked as one.
        assert_eq!(got["TEHAS_MODEL_KEY_B"], Entry::Value(REDACTED.into()));
        assert_eq!(got["TEHAS_MODEL_KEY_C"], Entry::Value(REDACTED.into()));
        assert_eq!(got["TEHAS_MCP"], Entry::Value("off".into()));
        assert_eq!(got.len(), 4);
    }

    #[test]
    fn entries_refuse_a_line_that_is_no_assignment_by_its_number() {
        for (text, why) in [
            ("A=1\nno equals here\n", "line 2 is not NAME=value"),
            ("1A=x\n", "line 1 is not NAME=value"),
            ("=secret\n", "line 1 is not NAME=value"),
            ("A B=secret\n", "line 1 is not NAME=value"),
            (
                "K=$(cat secret)\n",
                "K holds a $ or a backtick: the file is read, not run as shell, so write the value itself",
            ),
        ] {
            assert_eq!(entries(text).unwrap_err(), why, "{text:?}");
        }
        assert_eq!(
            entries("K='a$b'\n").unwrap()["K"],
            Entry::Value("a$b".into())
        );
    }

    #[test]
    fn a_key_needs_the_whole_name_before_the_equals() {
        let text = "TEHAS_HOME_X=/x\n";
        assert_eq!(value(text, "TEHAS_HOME"), None);
    }
}
