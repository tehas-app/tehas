//! The adapter's table of labels against `host/labels.tsv`, the table's one
//! source: the same labels, each with the same word, in the same order.

use tehas_github::table;
use tehas_ports::Mark;

const THE_TABLE: &str = include_str!("../../../host/labels.tsv");

/// The mark a word of the file's last column names: `Act::Approve`,
/// `State::Running`, `Choice::Lean`, `Mark::Epic`.
fn word(w: &str) -> Mark {
    let json = match w.split_once("::") {
        Some(("Mark", v)) => format!(r#"{{"type":"{v}"}}"#),
        Some((t, v)) => format!(r#"{{"type":"{t}","value":"{v}"}}"#),
        None => panic!("{w} is no word"),
    };
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("{w}: {e}"))
}

#[test]
fn the_table_is_host_labels_tsv() {
    let file: Vec<(String, Mark)> = THE_TABLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            (cols[0].to_string(), word(cols[cols.len() - 1]))
        })
        .collect();
    let ours: Vec<(String, Mark)> = table::words()
        .into_iter()
        .map(|(l, m)| (l.to_string(), m))
        .collect();
    assert_eq!(ours, file);
    for (label, mark) in &file {
        assert!(!matches!(mark, Mark::Unknown), "{label}");
        assert_eq!(table::word(label).as_ref(), Some(mark), "{label}");
        assert_eq!(table::label(mark).as_deref(), Some(label.as_str()));
    }
}

#[test]
fn a_label_of_no_word_is_none_and_a_projects_mark_is_its_name() {
    assert_eq!(table::word("bug"), None);
    let bug = Mark::Project(tehas_ports::MarkName::new("bug").unwrap());
    assert_eq!(table::label(&bug).as_deref(), Some("bug"));
    assert_eq!(table::label(&Mark::Unknown), None);
}

#[test]
fn the_approval_is_the_word_approve() {
    assert_eq!(
        table::word(table::APPROVAL_LABEL),
        Some(Mark::Act(tehas_ports::Act::Approve))
    );
}

#[test]
fn every_row_of_the_file_gives_its_name_colour_and_text() {
    let rows: Vec<&str> = THE_TABLE.lines().collect();
    let specs = table::specs();
    assert_eq!(specs.len(), rows.len());
    for (spec, row) in specs.iter().zip(&rows) {
        let cols: Vec<&str> = row.split('\t').collect();
        assert_eq!(cols.len(), 4, "{row}");
        assert_eq!(spec.name.as_str(), cols[0]);
        assert!(table::is_label(cols[0]), "{row}");
        assert_eq!(spec.colour.as_ref().map(|c| c.as_str()), Some(cols[1]));
        assert_eq!(spec.about.as_deref(), Some(cols[2]));
    }
}
