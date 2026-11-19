//! The settings tehas 1.0 renamed or removed: the rows of kind `setting` in
//! api/renames-1.0.tsv, built in so the file and the refusal cannot part. A
//! name of a row still set in the instance, or in the factory's credential
//! file, stops the load with 78 and a line that names the new key or why the
//! old one went. tehas does not rewrite the lines: the instance directory is a
//! checkout of a settings repository.

/// The file the release carries for adopters and for the programs that read
/// the factory.
const TSV: &str = include_str!("../../../../api/renames-1.0.tsv");

/// One row: the old name, the new one or empty for a removal, and the note.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    pub old: &'static str,
    pub new: &'static str,
    pub note: &'static str,
}

impl Row {
    /// The line the load stops with, without its `(see <file>)`.
    pub fn why(&self) -> String {
        if self.new.is_empty() {
            format!(
                "{} is no longer read: {}, so take the line out",
                self.old, self.note
            )
        } else {
            format!(
                "{} is no longer read: tehas 1.0 renamed it {} ({}), so rename the line",
                self.old, self.new, self.note
            )
        }
    }
}

/// Every setting row, in the file's order.
pub fn rows() -> Vec<Row> {
    TSV.lines()
        .skip(1)
        .filter_map(|line| {
            let mut f = line.split('\t');
            let (kind, old, new, note) = (f.next()?, f.next()?, f.next()?, f.next()?);
            (kind == "setting").then_some(Row { old, new, note })
        })
        .collect()
}

/// The values a removed line may hold and still load: each asks for what this
/// version does anyway, as an earlier version read it. An empty line asked for
/// no improvement loop and no report; a skip of 0 asks nothing of a loop that
/// is gone, an easy merge of 0 asks for the full refresh every refresh is, and
/// an overlap hold of 0 was its default and held nothing.
/// The manager's version under its old name is only shown, never decided on,
/// and the program that manages the factory writes it on every create: it is
/// dropped, so a factory it made before it writes the new name still runs. A
/// value the timed refresh scan took asks for nothing now that every refresh
/// is asked for one pull request at a time.
pub fn quiet(old: &str, value: &str) -> bool {
    old == "TEHAS_INDUSTRY_VERSION"
        || (old == "TEHAS_REFRESH_EVERY" && minutes(value))
        || matches!(
            (old, value),
            (
                "TEHAS_IMPROVE_RUKAS_SKIP" | "TEHAS_EASY_MERGE" | "TEHAS_OVERLAP_HOLD",
                "0" | ""
            ) | ("TEHAS_REFRESH_CONFLICTS", "0" | "1" | "")
                | (
                    "TEHAS_IMPROVE_REPO"
                        | "TEHAS_IMPROVE_RUKAS_REPO"
                        | "TEHAS_IMPROVE_MAX_OPEN"
                        | "TEHAS_CYCLE_REPORT"
                        | "TEHAS_REFRESH_EVERY",
                    ""
                )
        )
}

/// A value TEHAS_REFRESH_EVERY took: 1 to 1440 in ASCII digits, no leading 0.
/// The digits check matters: u32's parse also takes a leading '+'.
fn minutes(v: &str) -> bool {
    v.bytes().all(|b| b.is_ascii_digit())
        && !v.starts_with('0')
        && v.len() <= 4
        && v.parse::<u32>().is_ok_and(|n| (1..=1440).contains(&n))
}

/// The value the last line of `text` that sets `name` gives it, with the
/// blanks and one pair of quotes around it taken off; None when no line sets
/// it. The lines are read as text, not run.
pub fn value_in(text: &[u8], name: &str) -> Option<String> {
    let head = format!("{name}=");
    text.rsplit(|b| *b == b'\n')
        .find_map(|line| {
            let line = super::load::bare_line(line);
            let rest = line.strip_prefix(head.as_bytes())?.to_vec();
            Some(String::from_utf8_lossy(&rest).trim().to_string())
        })
        .map(|v| {
            for q in ['\'', '"'] {
                if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
                    return v[1..v.len() - 1].to_string();
                }
            }
            v
        })
}

/// Every row that `text`, a file named `file`, still sets with a value that
/// does not load quietly: one line each, in the file's order of rows.
pub fn found_in(text: &[u8], file: &str) -> Vec<String> {
    rows()
        .into_iter()
        .filter(|row| value_in(text, row.old).is_some_and(|v| !quiet(row.old, &v)))
        .map(|row| format!("{} (see {file})", row.why()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_are_read_from_the_file_with_their_columns() {
        let rows = rows();
        assert!(!rows.is_empty());
        for row in &rows {
            assert!(row.old.starts_with("TEHAS_"), "{row:?}");
            assert!(!row.note.is_empty(), "{row:?}");
            assert!(
                !row.note.contains('\t') && !row.new.contains(' '),
                "{row:?}"
            );
        }
        assert!(rows
            .iter()
            .any(|r| r.old == "TEHAS_EASY_MERGE" && r.new.is_empty()));
    }

    #[test]
    fn a_rename_names_the_new_key_and_a_removal_says_why() {
        let rename = Row {
            old: "TEHAS_A",
            new: "TEHAS_B",
            note: "same values",
        };
        assert_eq!(
            rename.why(),
            "TEHAS_A is no longer read: tehas 1.0 renamed it TEHAS_B (same values), so rename the line"
        );
        let gone = Row {
            old: "TEHAS_A",
            new: "",
            note: "the loop is removed",
        };
        assert_eq!(
            gone.why(),
            "TEHAS_A is no longer read: the loop is removed, so take the line out"
        );
    }

    #[test]
    fn a_value_is_the_last_line_that_sets_it_without_its_quotes() {
        let text = b"# TEHAS_EASY_MERGE=9\nTEHAS_EASY_MERGE=1\n  export TEHAS_EASY_MERGE=\"0\"\nTEHAS_EASY_MERGEX=2\n";
        assert_eq!(value_in(text, "TEHAS_EASY_MERGE").as_deref(), Some("0"));
        assert_eq!(value_in(b"A='x y'\n", "A").as_deref(), Some("x y"));
        assert_eq!(value_in(b"A=\n", "A").as_deref(), Some(""));
        assert_eq!(value_in(b"#A=1\n", "A"), None);
    }

    #[test]
    fn every_row_a_file_sets_is_listed_and_a_quiet_value_is_not() {
        let text = b"TEHAS_EASY_MERGE=0\nTEHAS_IMPROVE_REPO=acme/backlog\nTEHAS_BOT_LOGIN=x[bot]\n";
        let found = found_in(text, "/i/instance.env");
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].starts_with("TEHAS_IMPROVE_REPO is no longer read: "));
        assert!(found[1].starts_with("TEHAS_BOT_LOGIN is no longer read: "));
        assert!(found.iter().all(|l| l.ends_with(" (see /i/instance.env)")));
    }
}
