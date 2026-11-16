//! The labels that put an issue in a flow of its own, where a
//! `tehas:blocked` on its own starts nothing. A test below reads
//! `host/labels.tsv` and refuses a label of the table that is neither in
//! `IN_FLOW` nor in its list of the labels that are not a flow.

/// The labels of a flow: an issue carrying one of them is doing something of
/// its own, so a `tehas:blocked` beside it holds nothing.
pub(crate) const IN_FLOW: [&str; 7] = [
    "tehas:running",
    "tehas:awaiting-merge",
    "tehas:question",
    "tehas:planning",
    "tehas:refreshing",
    "tehas:plan",
    "tehas:refine",
];

/// Whether the labels put an issue in a flow of its own, where a
/// `tehas:blocked` on its own starts nothing.
pub(crate) fn in_flow(labels: &[String]) -> bool {
    labels.iter().any(|l| IN_FLOW.contains(&l.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decide::BLOCKED;

    /// The table every label of tehas is created from.
    const THE_TABLE: &str = include_str!("../../../../host/labels.tsv");

    /// The labels of the table that are not a flow, each with why. Together
    /// with `IN_FLOW` they are the whole table, and no name is in both.
    const NOT_IN_FLOW: [(&str, &str); 16] = [
        ("tehas:ready", "asks for a start, is not one"),
        ("tehas:shipped", "the mark of a merge, given by the bot"),
        ("tehas:pause", "a setting of the control issue, not of work"),
        (
            "needs-human-attention",
            "a person's to take off once they have read the handoff",
        ),
        ("tehas:replaced", "a record: the issue a refine filed"),
        ("tehas:refined", "a record: later parts are planned by hand"),
        (
            "tehas:millwright-escalated",
            "a record: the millwright left it to a person",
        ),
        ("tehas:lean", "a choice of driver, not a state of work"),
        ("tehas:classic", "a choice of driver, not a state of work"),
        ("tehas:blocked", "the label this list decides the force of"),
        (
            "tehas:all-hands",
            "a setting of the control issue, not of work",
        ),
        ("epic", "a kind of issue, not a state of work"),
        (
            "tehas:finish",
            "a request a person applies; the finish reads it, not the flow",
        ),
        (
            "tehas:prototype",
            "a stage of the product, not a state of work",
        ),
        (
            "tehas:improvement",
            "a mark the improvement loop puts on what it files",
        ),
        ("cross-project", "a kind of issue, not a state of work"),
    ];

    /// The names of a table of labels: the first column of every line that is
    /// not blank.
    fn names_of(table: &str) -> Vec<&str> {
        table
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.split('\t').next().unwrap_or(""))
            .collect()
    }

    /// What is wrong with the split of `table` into `in_flow` and `not`: a name
    /// of the table in neither, a name in both, or a name in either that the
    /// table does not have. Empty when the split holds.
    fn split_faults(table: &str, in_flow: &[&str], not: &[&str]) -> Vec<String> {
        let names = names_of(table);
        let mut faults = Vec::new();
        for name in &names {
            match (in_flow.contains(name), not.contains(name)) {
                (false, false) => faults.push(format!("{name}: neither in a flow nor out of one")),
                (true, true) => faults.push(format!("{name}: both in a flow and out of one")),
                _ => {}
            }
        }
        for name in in_flow.iter().chain(not) {
            if !names.contains(name) {
                faults.push(format!("{name}: not in the table"));
            }
        }
        faults
    }

    fn not_in_flow() -> Vec<&'static str> {
        NOT_IN_FLOW.iter().map(|(name, _)| *name).collect()
    }

    #[test]
    fn every_label_of_the_table_is_in_a_flow_or_out_of_one_and_never_both() {
        assert_eq!(
            split_faults(THE_TABLE, &IN_FLOW, &not_in_flow()),
            Vec::<String>::new()
        );
        assert!(!IN_FLOW.contains(&BLOCKED));
        assert!(NOT_IN_FLOW.iter().all(|(_, why)| !why.is_empty()));
    }

    #[test]
    fn a_new_label_in_the_table_with_no_side_is_refused() {
        let table = format!("{THE_TABLE}tehas:example\tffffff\tTehas: a new state\n");
        assert_eq!(
            split_faults(&table, &IN_FLOW, &not_in_flow()),
            ["tehas:example: neither in a flow nor out of one"]
        );
    }

    #[test]
    fn a_flow_label_the_table_lost_or_a_name_on_both_sides_is_refused() {
        let table: String = THE_TABLE
            .lines()
            .filter(|line| !line.starts_with("tehas:refine\t"))
            .map(|line| format!("{line}\n"))
            .collect();
        assert_eq!(
            split_faults(&table, &IN_FLOW, &not_in_flow()),
            ["tehas:refine: not in the table"]
        );
        let mut both = IN_FLOW.to_vec();
        both.push("tehas:ready");
        assert_eq!(
            split_faults(THE_TABLE, &both, &not_in_flow()),
            ["tehas:ready: both in a flow and out of one"]
        );
    }

    #[test]
    fn the_flow_labels_are_the_flow_and_the_blocked_label_alone_is_not() {
        for name in IN_FLOW {
            assert!(in_flow(&[name.to_string()]), "{name} is not a flow");
        }
        assert!(!in_flow(&[BLOCKED.to_string()]));
        assert!(!in_flow(&[]));
    }
}
