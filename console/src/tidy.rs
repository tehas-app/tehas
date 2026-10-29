//! Which of tehas's state labels come off which closed issue, for libexec/tidy.
//! The two searches stay bash - `gh issue list` for the closed issues that carry
//! a state label and `gh api graphql` for what closed them, each under the
//! poll's own time limit - and their answers are handed here, so the rule of
//! what is taken off, what is put on and what is said has one statement. The
//! edits themselves are bash too: the caller makes each one under the same
//! limit.
//!
//!   console tidy states
//!     The state labels, in the order of the search, and `tehas:shipped` last
//!     of all: the names that say work is in progress or waiting are all that
//!     is ever taken off, whatever the search sent back, and the one name tidy
//!     puts on and never takes off. A caller searches for the names before it
//!     and compares the plan against those alone, so the label it may earn is
//!     never asked for. Not `needs-human-attention`, which nothing in tehas
//!     removes, and not the labels a person chooses.
//!
//!   console tidy plan [--control <number>]... <page>
//!     The closed issues, their labels on standard input, as `gh issue list
//!     --state closed --search <the state labels> --json number,state,labels`
//!     answered: one JSON array. <page> is the ceiling of the closer read, the
//!     same number `console shipped page` prints, so the list of issues the
//!     read is asked about cannot outgrow what it answers for.
//!
//!     One line per issue that has a label to take off, tab separated, four
//!     fields: the number, the state labels to take off in the order above
//!     joined by one space, whether the issue already carries `tehas:shipped`
//!     (`yes` or `no`), and whether the closer read is asked about it (`yes`
//!     or `no`). Nothing here decides that a label is earned: the caller puts
//!     the answer of its one read of the poll beside the line.
//!
//!     An open issue is never on the list, nor the control issue, nor one
//!     whose state labels are none of the names above. The read is asked about
//!     an issue that does not carry the label and is among the first <page>
//!     such issues of the answer, in its order: a list past the ceiling of a
//!     search is no answer at all, so the issues after it are named to nothing
//!     and keep their labels for a later poll. The fourth field is the ask
//!     that limits the read, so it is the read that is bounded to <page> and
//!     not any edit; nothing here writes. The caller of a dry run asks for the
//!     plan it would have made and edits nothing, so what it would add is said
//!     for those same first <page> issues and is silent on the rest, as it is
//!     for a poll that edits.
//!     An issue that already carries
//!     the label is not read for it again, and one whose cycle holds its lock
//!     is taken off the list by the caller before the read is asked.
//!
//! Nothing is written to standard error and the exit is always 0: a list the
//! caller cannot read is its own failure to log, and a plan of nothing is a
//! poll with nothing to tidy.

use serde_json::Value;
use std::io::Read;

/// The state labels that say work is in progress or waiting. A new one is added
/// here on purpose, and the test of this verb below pins the list.
const STATES: [&str; 9] = [
    "tehas:running",
    "tehas:awaiting-merge",
    "tehas:question",
    "tehas:planning",
    "tehas:refreshing",
    "tehas:blocked",
    "tehas:ready",
    "tehas:plan",
    "tehas:refine",
];

/// The label tidy gives and never takes off: the record of a merge. `states`
/// prints it after the ones to take off, which is how a caller tells the two
/// apart: the search is made for the names before it, and the plan compares
/// against them alone, so the label the read may earn is never asked for.
const SHIPPED: &str = "tehas:shipped";

pub fn main(given: &[String]) -> u8 {
    let Some(verb) = given.first().map(String::as_str) else {
        return usage();
    };
    match verb {
        "states" => {
            if given.len() != 1 {
                return usage();
            }
            for state in STATES.iter().copied().chain([SHIPPED]) {
                println!("{state}");
            }
            0
        }
        // Whether one answer of the search is a list of issues at all: no list is
        // the failed search the caller logs, and not a poll with nothing to tidy,
        // which is the empty list of a search that found no issue. The caller asks
        // this of the answer before it asks for a plan, so the two tests of the
        // answer - this one and the console's own - are drawn in one place.
        "issues" => {
            let mut found = String::new();
            if given.len() != 1 || std::io::stdin().read_to_string(&mut found).is_err() {
                return usage();
            }
            u8::from(!list(found.as_bytes()))
        }
        "plan" => {
            let mut controls = Vec::new();
            let mut page = None;
            let mut rest = given[1..].iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--control" => match rest.next() {
                        Some(n) if digits(n) => controls.push(n.clone()),
                        _ => return usage(),
                    },
                    _ if page.is_none() && digits(arg) => match arg.parse::<usize>() {
                        Ok(n) if n > 0 => page = Some(n),
                        _ => return usage(),
                    },
                    _ => return usage(),
                }
            }
            let Some(page) = page else { return usage() };
            let mut found = String::new();
            if std::io::stdin().read_to_string(&mut found).is_err() {
                return 0;
            }
            for line in plan(&found, &controls, page) {
                println!("{line}");
            }
            0
        }
        _ => usage(),
    }
}

fn usage() -> u8 {
    eprintln!("console tidy: usage: states | issues | plan [--control <number>]... <page> <answer");
    2
}

/// Whether the bytes are the answer of the search: one JSON list, however empty.
/// An answer that is no list - no JSON, an object, a bare word - is the failed
/// search the caller logs, and not a poll with nothing to tidy.
fn list(found: &[u8]) -> bool {
    match serde_json::from_slice::<Value>(found) {
        Ok(doc) => doc.is_array(),
        Err(_) => false,
    }
}

/// Digits and nothing else: the number of an issue as GitHub named it, or the
/// page of the read as the console printed it. An empty argument is no number.
fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// What the poll does, one line per issue: see the module's `plan`. A search
/// that is no list of issues is no plan at all - the caller refused it at
/// `issues` before it asked here.
fn plan(found: &str, controls: &[String], page: usize) -> Vec<String> {
    let Ok(doc) = serde_json::from_str::<Value>(found) else {
        return Vec::new();
    };
    let Some(items) = doc.as_array() else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let mut asked = 0;
    let mut seen: Vec<String> = Vec::new();
    for item in items {
        // The state is read again here: an issue the search answered as open,
        // or with no state at all, is never touched.
        if item.get("state").and_then(Value::as_str) != Some("CLOSED") {
            continue;
        }
        let Some(number) = item.get("number").and_then(Value::as_u64) else {
            continue;
        };
        let n = number.to_string();
        if controls.contains(&n) {
            continue;
        }
        // Once for each number: a search that named an issue twice would have
        // its labels taken off, its closer read and its edit made twice over.
        if seen.contains(&n) {
            continue;
        }
        seen.push(n.clone());
        // The names to take off come from GitHub, so only a state label above
        // is ever taken from the answer: a name the search sent back that is
        // none of them is neither passed to the edit nor written to the log,
        // where it could carry a line of its own.
        let off: Vec<&str> = item
            .get("labels")
            .and_then(Value::as_array)
            .map(|labels| {
                STATES
                    .iter()
                    .filter(|state| {
                        labels
                            .iter()
                            .any(|label| label.get("name").and_then(Value::as_str) == Some(**state))
                    })
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        if off.is_empty() {
            continue;
        }
        // The label is earned once: an issue that already carries it is not
        // read for it again, and one past the page of the read is not read at
        // all, so neither has a label to add.
        let carries = item
            .get("labels")
            .and_then(Value::as_array)
            .map(|labels| {
                labels
                    .iter()
                    .any(|label| label.get("name").and_then(Value::as_str) == Some(SHIPPED))
            })
            .unwrap_or(false);
        let ask = !carries && asked < page;
        if ask {
            asked += 1;
        }
        let off = off.join(" ");
        lines.push(format!(
            "{n}\t{off}\t{}\t{}",
            if carries { "yes" } else { "no" },
            if ask { "yes" } else { "no" }
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The search's answer for the issues given, each `{"number":n,"state":…,
    /// "labels":[…]}` of the caller's own making.
    fn found(items: &[String]) -> String {
        format!("[{}]", items.join(","))
    }

    fn issue(n: u64, state: &str, labels: &[&str]) -> String {
        let labels: Vec<String> = labels
            .iter()
            .map(|l| format!("{{\"name\":\"{l}\"}}"))
            .collect();
        format!(
            "{{\"number\":{n},\"state\":\"{state}\",\"labels\":[{}]}}",
            labels.join(",")
        )
    }

    fn planned(items: &[String], page: usize) -> Vec<String> {
        plan(&found(items), &[], page)
    }

    /// The same plan as the caller reads it: number, labels, add, asked.
    fn rows(items: &[String], page: usize) -> Vec<(String, String, String, String)> {
        planned(items, page)
            .into_iter()
            .map(|line| {
                let f: Vec<String> = line.split('\t').map(String::from).collect();
                (f[0].clone(), f[1].clone(), f[2].clone(), f[3].clone())
            })
            .collect()
    }

    #[test]
    fn the_states_are_the_nine_names_that_say_work_is_waiting() {
        assert_eq!(
            STATES,
            [
                "tehas:running",
                "tehas:awaiting-merge",
                "tehas:question",
                "tehas:planning",
                "tehas:refreshing",
                "tehas:blocked",
                "tehas:ready",
                "tehas:plan",
                "tehas:refine",
            ]
        );
        // tidy gives this one and never takes it off.
        assert!(!STATES.contains(&SHIPPED));
        assert!(!STATES.contains(&"needs-human-attention"));
    }

    #[test]
    fn a_closed_issue_loses_its_state_label_and_keeps_everything_else() {
        let rows = rows(
            &[issue(
                194,
                "CLOSED",
                &["tehas:awaiting-merge", "needs-human-attention", SHIPPED],
            )],
            100,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "194");
        assert_eq!(rows[0].1, "tehas:awaiting-merge");
        // It carries the label already, so it is not read for it again.
        assert_eq!(rows[0].2, "yes");
        assert_eq!(rows[0].3, "no");
    }

    #[test]
    fn every_state_label_comes_off_in_one_line_and_a_persons_labels_stay() {
        let rows = rows(
            &[issue(
                5,
                "CLOSED",
                &[
                    "tehas:running",
                    "tehas:question",
                    "tehas:planning",
                    "tehas:refreshing",
                    "tehas:blocked",
                    "tehas:ready",
                    "tehas:plan",
                    "tehas:urgent",
                    "tehas:next",
                    "cross-project",
                    "bug",
                ],
            )],
            100,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].1,
            "tehas:running tehas:question tehas:planning tehas:refreshing tehas:blocked tehas:ready tehas:plan"
        );
        for kept in ["tehas:urgent", "tehas:next", "cross-project", "bug"] {
            assert!(!rows[0].1.contains(kept), "{kept} would come off");
        }
    }

    #[test]
    fn tehas_refine_is_a_state_label_too() {
        let rows = rows(&[issue(6, "CLOSED", &["tehas:refine"])], 100);
        assert_eq!(rows[0].1, "tehas:refine");
    }

    #[test]
    fn an_open_issue_is_never_on_the_plan() {
        let items = [
            issue(8, "OPEN", &["tehas:awaiting-merge"]),
            "{\"number\":9,\"labels\":[{\"name\":\"tehas:question\"}]}".to_string(),
            issue(10, "closed", &["tehas:question"]),
        ];
        assert_eq!(planned(&items, 100), Vec::<String>::new());
    }

    #[test]
    fn a_closed_issue_with_no_state_label_is_not_on_the_plan() {
        let items = [issue(10, "CLOSED", &[SHIPPED, "needs-human-attention"])];
        assert!(planned(&items, 100).is_empty());
    }

    #[test]
    fn the_control_issue_is_left_out_of_the_plan() {
        let items = [
            issue(1, "CLOSED", &["tehas:running"]),
            issue(23, "CLOSED", &["tehas:running"]),
        ];
        let rows = plan(&found(&items), &["1".to_string()], 100)
            .into_iter()
            .map(|line| line.split('\t').next().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(rows, ["23"]);
    }

    #[test]
    fn a_name_that_is_no_state_label_is_neither_taken_off_nor_written() {
        // The names come from GitHub. One carrying a newline of its own cannot
        // put a second line in the log, because it is dropped before the plan.
        let items = [
            "{\"number\":55,\"state\":\"CLOSED\",\"labels\":[{\"name\":\"tehas:running\"},\
             {\"name\":\"fake\\nWARN: tidy #999: forged line\"}]}"
                .to_string(),
        ];
        let rows = rows(&items, 100);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "tehas:running");
        assert!(!rows[0].1.contains("fake"));
    }

    #[test]
    fn an_issue_without_the_label_is_asked_about_and_earns_it() {
        let rows = rows(&[issue(13, "CLOSED", &["tehas:running"])], 100);
        assert_eq!(rows[0].3, "yes");
        // It carries no label, which is the third field's own saying.
        assert_eq!(rows[0].2, "no");
    }

    #[test]
    fn an_issue_that_carries_the_label_is_never_asked_about() {
        let items = [issue(44, "CLOSED", &["tehas:awaiting-merge", SHIPPED])];
        let lines = plan(&found(&items), &[], 100);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("\tyes\tno"), "{}", lines[0]);
    }

    #[test]
    fn an_issue_named_twice_is_planned_once() {
        let items = [
            issue(7, "CLOSED", &["tehas:running"]),
            issue(7, "CLOSED", &["tehas:running"]),
        ];
        let rows = rows(&items, 100);
        assert_eq!(rows.len(), 1);
        // One ask, not two: the second copy is no second candidate for the read.
        assert_eq!(rows[0].3, "yes");
    }

    #[test]
    fn the_candidates_past_the_page_are_left_to_a_later_poll() {
        // Three candidates, a read that answers for two: the third keeps its
        // labels and is not named to the read.
        let items = [
            issue(1, "CLOSED", &["tehas:running"]),
            issue(2, "CLOSED", &["tehas:running"]),
            issue(3, "CLOSED", &["tehas:running"]),
        ];
        let rows = rows(&items, 2);
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().map(|r| r.3.as_str()).collect::<Vec<_>>(),
            ["yes", "yes", "no"]
        );
    }

    #[test]
    fn an_issue_that_carries_the_label_takes_no_room_of_the_read() {
        // The read is asked about the issues that may earn the label, so an
        // issue that already carries it leaves its place in the page to one
        // that may still earn it. The label itself still comes off it.
        let items = [
            issue(1, "CLOSED", &["tehas:running", SHIPPED]),
            issue(2, "CLOSED", &["tehas:running"]),
            issue(3, "CLOSED", &["tehas:running"]),
        ];
        let rows = rows(&items, 2);
        assert_eq!(
            rows.iter()
                .map(|r| (r.0.as_str(), r.3.as_str()))
                .collect::<Vec<_>>(),
            [("1", "no"), ("2", "yes"), ("3", "yes")]
        );
        assert_eq!(rows[0].1, "tehas:running");
    }

    #[test]
    fn the_states_end_with_the_one_label_tidy_gives_and_never_takes_off() {
        // The search is made for the names before this one and the plan
        // compares against those alone, so it is never asked for.
        assert_eq!(STATES[STATES.len() - 1], "tehas:refine");
        assert!(!STATES.contains(&SHIPPED));
        assert!(!STATES.contains(&"needs-human-attention"));
    }

    #[test]
    fn an_answer_that_is_no_list_of_issues_is_no_plan_and_no_answer() {
        // `issues` is the caller's test of the search, so the empty list of a
        // poll with nothing to tidy and no list at all read differently. A list
        // of things that are no issues is a list: each item of it is no issue
        // below, and none of them is on the plan.
        for answer in ["", "not json", "{}", "null", "{\"data\":{}}"] {
            assert!(!list(answer.as_bytes()), "{answer} is no list");
            assert_eq!(plan(answer, &[], 100), Vec::<String>::new(), "{answer}");
        }
        assert!(list("[]".as_bytes()));
        assert!(list("[1,2]".as_bytes()));
        assert!(list("[{\"number\":1}]".as_bytes()));
    }

    #[test]
    fn an_item_that_is_no_issue_is_skipped_and_the_others_are_planned() {
        let items = [
            "null".to_string(),
            "\"12\"".to_string(),
            "{\"number\":\"12\",\"state\":\"CLOSED\",\"labels\":[{\"name\":\"tehas:plan\"}]}"
                .to_string(),
            issue(13, "CLOSED", &["tehas:plan"]),
        ];
        let rows = rows(&items, 100);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "13");
    }

    #[test]
    fn an_issue_with_no_labels_key_has_nothing_to_take_off() {
        let items = ["{\"number\":20,\"state\":\"CLOSED\"}".to_string()];
        assert!(planned(&items, 100).is_empty());
    }

    #[test]
    fn a_label_that_is_no_object_names_nothing() {
        let items =
            ["{\"number\":21,\"state\":\"CLOSED\",\"labels\":[\"tehas:plan\"]}".to_string()];
        assert!(planned(&items, 100).is_empty());
    }

    #[test]
    fn the_page_is_a_count_and_no_argument_that_is_not_one_is_refused() {
        // `plan` is reached through main, which takes the page as an argument:
        // a caller that passed a word instead of a number would have the read
        // asked about nothing at all.
        assert!(!digits(""));
        assert!(!digits("100.0"));
        assert!(!digits(" 100"));
        assert!(!digits("-1"));
        assert!(digits("100"));
    }
}
