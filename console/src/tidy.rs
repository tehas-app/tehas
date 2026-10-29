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
//!     read is asked about cannot outgrow what it answers for. It is a number
//!     of one to nine digits with no leading zero, and no more than that page:
//!     a read asked about more issues than its query answers for is no read.
//!
//!     One line per issue that has a label to take off, tab separated, four
//!     fields: the number, the state labels to take off in the order above
//!     joined by one space, whether the issue already carries `tehas:shipped`
//!     (`yes` or `no`), and whether the closer read is asked about it (`yes`
//!     or `no`). The fourth implies the third - the read is never asked about
//!     an issue that carries the label - and the third is printed for the log
//!     as well as the branch. Nothing here decides that a label is earned: the
//!     caller puts the answer of its one read of the poll beside the line.
//!
//!     The list is bounded and the answer is read under a ceiling of its own:
//!     a stream past 8 MiB or a list past 1000 issues is no answer, so a poll
//!     is never held by a search that will not end. The exit says which of the
//!     three the caller is in, and `console tidy issues` says the same of the
//!     answer alone, so a caller that wants one reading of a bad answer asks
//!     for the plan and reads its status:
//!       0  a list of issues, planned or empty - the empty list of a search
//!          that found nothing is a poll with nothing to tidy;
//!       1  no list at all, or a list of things that are no issues: the failed
//!          search the caller logs, and not a poll with nothing to tidy;
//!       2  the arguments are no page or no control number, or the answer
//!          cannot be read.
//!
//!     What is never on the list:
//!       - an open issue, or one the answer gives no state for;
//!       - the control issue each <number> given with --control names: digits,
//!         no leading zero, and no more digits than a page, so a padded number
//!         is no control either;
//!       - an issue whose state labels are none of the names above;
//!       - an issue the answer named twice, which is planned once.
//!     The read is asked about an issue that does not carry the label and is
//!     among the first <page> such issues of the answer, in its order: a list
//!     past the ceiling of a search is no answer at all, so the issues after it
//!     are named to nothing and keep their labels for a later poll. The fourth
//!     field is the ask that limits the read, so it is the read that is bounded
//!     to <page> and not any edit; nothing here writes. The caller of a dry run
//!     asks for the plan it would have made and edits nothing, so what it would
//!     add is said for those same first <page> issues and is silent on the rest,
//!     as it is for a poll that edits.
//!     One whose cycle holds its lock is taken off the list by the caller
//!     before the read is asked: the caller drops the locked issues before it
//!     names any of them to the read, so a waiting issue spends no place of the
//!     page.

use serde_json::Value;
use std::io::Read as _;

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

/// The label tidy gives and never takes off: the record of a merge, named once
/// with the page of the read in `shipped`, which is where `lib/shipped.sh` reads
/// it from too. `states` prints it after the ones to take off, which is how a
/// caller tells the two apart: the search is made for the names before it, and
/// the plan compares against them alone, so the label the read may earn is
/// never asked for.
const SHIPPED: &str = crate::shipped::SHIPPED;

/// The answer of the search is read under a ceiling of its own: a stream of
/// bytes past it is refused rather than grown into memory.
const BIGGEST: u64 = 8 * 1024 * 1024;

/// The largest list of issues the answer may name. The read answers for a page
/// of them at most, so a list of a thousand is no search of this poll.
const MOST_ISSUES: usize = 1000;

/// What the answer of the search is, in the words of the exit status the
/// caller reads: a list of issues, no list at all, or a stream that could not
/// be read. The three are drawn apart here and nowhere else.
enum Answer {
    Issues(Vec<Value>),
    NoList,
    Unreadable,
}

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
        // Whether one answer of the search is a list of issues at all: no list
        // is the failed search the caller logs, and not a poll with nothing to
        // tidy, which is the empty list of a search that found no issue. `plan`
        // draws the same line and ends 1 on it, so a caller that asks only for
        // the plan gets this answer in its status too.
        "issues" => {
            if given.len() != 1 {
                return usage();
            }
            match answer() {
                Answer::Issues(_) => 0,
                Answer::NoList => 1,
                Answer::Unreadable => 2,
            }
        }
        "plan" => {
            let mut controls = Vec::new();
            let mut page = None;
            let mut rest = given[1..].iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--control" => match rest.next().and_then(|n| number(n)) {
                        Some(n) => controls.push(n),
                        None => return usage(),
                    },
                    _ if page.is_none() => match number(arg) {
                        Some(n) if n <= crate::shipped::PAGE as u64 => {
                            page = Some(n as usize)
                        }
                        _ => return usage(),
                    },
                    _ => return usage(),
                }
            }
            let Some(page) = page else { return usage() };
            match answer() {
                Answer::Issues(items) => {
                    for line in plan(&items, &controls, page) {
                        println!("{line}");
                    }
                    0
                }
                Answer::NoList => 1,
                Answer::Unreadable => 2,
            }
        }
        _ => usage(),
    }
}

fn usage() -> u8 {
    eprintln!("console tidy: usage: states | issues | plan [--control <number>]... <page> <answer");
    2
}

/// The answer of the search off standard input, and what it is: the stream is
/// read under `BIGGEST` so a runaway pipe is refused and not buffered.
fn answer() -> Answer {
    let mut found = String::new();
    if std::io::stdin()
        .take(BIGGEST + 1)
        .read_to_string(&mut found)
        .is_err()
    {
        return Answer::Unreadable;
    }
    if found.len() as u64 > BIGGEST {
        return Answer::Unreadable;
    }
    answer_of(&found)
}

/// What an answer of the search is: `answer`'s rule with the bytes given
/// instead of taken from the stream, so the tests ask the one reading a verb
/// gets.
fn answer_of(found: &str) -> Answer {
    let Ok(doc) = serde_json::from_str::<Value>(found) else {
        return Answer::NoList;
    };
    let Some(items) = doc.as_array() else {
        return Answer::NoList;
    };
    if items.len() > MOST_ISSUES {
        return Answer::NoList;
    }
    // An array of things that are no issues is the failed search it is: each
    // item names an issue before the list is taken for one.
    if items.iter().any(|item| issue_of(item).is_none()) {
        return Answer::NoList;
    }
    Answer::Issues(items.clone())
}

/// An issue number of one to nine digits with no leading zero: the number of an
/// issue as GitHub named it, a control issue, or the page of the read as the
/// console printed it. "0", "007", "-1" and "1e2" are none of the three, so a
/// padded number never fails to match the number it spells.
fn number(word: &str) -> Option<u64> {
    u64::try_from(tehas_core::grammar::digits(word, 9, false)?).ok()
}

/// What the poll does, one line per issue: see the module's `plan`. The items
/// are the list `read` accepted, so every one of them names an issue.
fn plan(items: &[Value], controls: &[u64], page: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut asked = 0;
    let mut seen: Vec<u64> = Vec::new();
    for item in items {
        // The state is read again here: an issue the search answered as open,
        // or with no state at all, is never touched.
        if item.get("state").and_then(Value::as_str) != Some("CLOSED") {
            continue;
        }
        let Some(number) = issue_of(item) else {
            continue;
        };
        if controls.contains(&number) {
            continue;
        }
        // Once for each number: a search that named an issue twice would have
        // its labels taken off, its closer read and its edit made twice over.
        if seen.contains(&number) {
            continue;
        }
        seen.push(number);
        // The names to take off come from GitHub, so only a state label above
        // is ever taken from the answer: a name the search sent back that is
        // none of them is neither passed to the edit nor written to the log,
        // where it could carry a line of its own. The names are read once and
        // both readings below come from them.
        let names = names(item);
        let off: Vec<&str> = STATES
            .iter()
            .copied()
            .filter(|state| names.contains(state))
            .collect();
        if off.is_empty() {
            continue;
        }
        // The label is earned once: an issue that already carries it is not
        // read for it again, and one past the page of the read is not read at
        // all, so neither has a label to add.
        let carries = names.contains(&SHIPPED);
        let ask = !carries && asked < page;
        if ask {
            asked += 1;
        }
        lines.push(Row {
            number,
            off,
            carries,
            ask,
        }
        .to_string());
    }
    lines
}

/// The issue a node of the search's answer names: a whole number of its own,
/// and nothing that names no issue - no number, a number written as text, a
/// null, a bool, a list.
fn issue_of(item: &Value) -> Option<u64> {
    match item.get("number")? {
        Value::Number(n) if n.is_u64() => n.as_u64().filter(|i| *i > 0),
        Value::Number(n) if n.is_i64() => n.as_i64().and_then(|i| u64::try_from(i).ok()),
        _ => None,
    }
}

/// The names of the labels the answer gave one issue: a labels key that is no
/// list names nothing, and so does a name that is no string.
fn names(item: &Value) -> Vec<&str> {
    item.get("labels")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|label| label.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default()
}

/// One line of the plan: the four words the caller reads, in the order the
/// module names. The row is built by `plan` and printed here, so the field
/// order and the words `yes` and `no` of the two verdicts are stated once.
struct Row<'a> {
    number: u64,
    off: Vec<&'a str>,
    carries: bool,
    ask: bool,
}

impl std::fmt::Display for Row<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\t{}\t{}\t{}",
            self.number,
            self.off.join(" "),
            if self.carries { "yes" } else { "no" },
            if self.ask { "yes" } else { "no" }
        )
    }
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

    /// The plan of an answer written out by a case, read the way `main` reads
    /// it: nothing when the answer is no list of issues at all.
    fn planned(found: &str, controls: &[u64], page: usize) -> Option<Vec<String>> {
        match answer_of(found) {
            Answer::Issues(items) => Some(plan(&items, controls, page)),
            Answer::NoList | Answer::Unreadable => None,
        }
    }

    fn planned_issues(items: &[String], page: usize) -> Option<Vec<String>> {
        planned(&found(items), &[], page)
    }

    /// The same plan as the caller reads it: number, labels, carries, asked.
    /// The row's shape is checked here, so a field added to or dropped from
    /// `Row`'s Display is this test's failure and not a silent ask of `no`.
    fn rows(items: &[String], page: usize) -> Vec<(String, String, String, String)> {
        planned_issues(items, page)
            .unwrap_or_default()
            .into_iter()
            .map(|line| {
                let f: Vec<String> = line.split('\t').map(String::from).collect();
                assert_eq!(f.len(), 4, "the plan's row is four fields: {line}");
                assert!(
                    f[2] == "yes" || f[2] == "no",
                    "the third field is yes or no: {line}"
                );
                assert!(
                    f[3] == "yes" || f[3] == "no",
                    "the fourth field is yes or no: {line}"
                );
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
    }

    #[test]
    fn the_states_end_with_the_one_label_tidy_gives_and_never_takes_off() {
        // The search is made for the names before this one and the plan
        // compares against those alone, so the label of a merge is never asked
        // for, and it is not one of the names that say work is waiting.
        assert_eq!(STATES[STATES.len() - 1], "tehas:refine");
        assert!(!STATES.contains(&SHIPPED));
        assert!(!STATES.contains(&"needs-human-attention"));
    }

    #[test]
    fn the_label_of_a_merge_is_named_once_for_both_commands() {
        // lib/shipped.sh prints the name it edits with and tidy compares the
        // same constant: a rename reaches the removal rule and the add together.
        assert_eq!(SHIPPED, crate::shipped::SHIPPED);
        assert_eq!(SHIPPED, "tehas:shipped");
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
        // Its own rule only: the list itself is pinned by the test above.
        let rows = rows(&[issue(6, "CLOSED", &["tehas:refine"])], 100);
        assert_eq!(rows[0].1, "tehas:refine");
    }

    #[test]
    fn an_open_issue_is_never_on_the_plan() {
        let items = [
            issue(8, "OPEN", &["tehas:awaiting-merge"]),
            issue(10, "closed", &["tehas:question"]),
        ];
        assert_eq!(planned_issues(&items, 100).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn an_issue_the_answer_gives_no_state_for_is_not_on_the_plan() {
        let items = ["{\"number\":9,\"labels\":[{\"name\":\"tehas:question\"}]}".to_string()];
        assert_eq!(planned_issues(&items, 100).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_closed_issue_with_no_state_label_is_not_on_the_plan() {
        let items = [issue(10, "CLOSED", &[SHIPPED, "needs-human-attention"])];
        assert!(planned_issues(&items, 100).unwrap().is_empty());
    }

    #[test]
    fn the_control_issue_is_left_out_of_the_plan() {
        let items = [
            issue(1, "CLOSED", &["tehas:running"]),
            issue(23, "CLOSED", &["tehas:running"]),
        ];
        let rows = planned(&found(&items), &[1], 100)
            .unwrap()
            .into_iter()
            .map(|line| line.split('\t').next().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(rows, ["23"]);
    }

    #[test]
    fn a_control_number_is_the_number_and_not_its_spelling() {
        // A padded control names no issue, so it leaves the control issue on
        // the plan: the number is read as a number on both sides of the rule.
        let items = [
            issue(9, "CLOSED", &["tehas:running"]),
            issue(23, "CLOSED", &["tehas:running"]),
        ];
        let rows = plan(&items_of(&found(&items)), &[9], 100)
            .into_iter()
            .map(|line| line.split('\t').next().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(rows, ["23"]);
        // The verb refuses a padded control before the plan is made at all.
        for bad in ["09", "0", "0000000000000000009", "", "1.0", "-1", "1e2"] {
            let given: Vec<String> = ["plan", "--control", bad, "100"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            assert_eq!(main(&given), 2, "--control \"{bad}\" is no number");
        }
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
        let lines = planned_issues(&items, 100).unwrap();
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
    fn an_answer_that_is_no_list_of_issues_is_no_plan_and_no_answer() {
        // The empty list of a poll with nothing to tidy and no list at all read
        // differently, in this one place: no JSON, an object, a bare word and an
        // array of things that are no issues are the failed search, and a list
        // of issues is the answer, however empty.
        for answer in ["", "not json", "{}", "null", "{\"data\":{}}", "123", "[1,2]"] {
            assert!(planned(answer, &[], 100).is_none(), "{answer} is no list");
        }
        assert!(planned("[]", &[], 100).unwrap().is_empty());
        assert!(planned(&found(&[issue(1, "CLOSED", &[])]), &[], 100)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn an_answer_past_its_ceiling_is_no_list() {
        // A stream of bytes with no list in it is refused, and so is a list of
        // more issues than a poll ever reads: neither is planned at all.
        assert!(planned(&"[1".repeat(100), &[], 100).is_none());
        let many: Vec<String> = (1..=1001)
            .map(|n| issue(n, "CLOSED", &["tehas:running"]))
            .collect();
        assert!(planned(&found(&many), &[], 100).is_none());
        let just_enough: Vec<String> = (1..=1000)
            .map(|n| issue(n, "CLOSED", &["tehas:running"]))
            .collect();
        assert_eq!(planned(&found(&just_enough), &[], 100).unwrap().len(), 1000);
    }

    #[test]
    fn an_item_that_is_no_issue_is_no_answer_at_all() {
        // The old answer planned the items that were issues and dropped the
        // rest; the reading now refuses the whole answer, because a search that
        // named a thing that is no issue did not answer the poll's question.
        let items = [
            "null".to_string(),
            "\"12\"".to_string(),
            "{\"number\":\"12\",\"state\":\"CLOSED\",\"labels\":[{\"name\":\"tehas:plan\"}]}"
                .to_string(),
            issue(13, "CLOSED", &["tehas:plan"]),
        ];
        assert!(planned(&found(&items), &[], 100).is_none());
    }

    #[test]
    fn an_issue_with_no_labels_key_has_nothing_to_take_off() {
        let items = ["{\"number\":20,\"state\":\"CLOSED\"}".to_string()];
        assert!(planned_issues(&items, 100).unwrap().is_empty());
    }

    #[test]
    fn a_label_that_is_no_object_names_nothing() {
        let items =
            ["{\"number\":21,\"state\":\"CLOSED\",\"labels\":[\"tehas:plan\"]}".to_string()];
        assert!(planned_issues(&items, 100).unwrap().is_empty());
    }

    #[test]
    fn the_page_is_the_page_of_the_read_and_no_argument_that_is_not_one_is_refused() {
        // `plan` is reached through main, which takes the page as an argument:
        // a caller that passed a word instead of a number would have the read
        // asked about nothing at all, and one that passed a page past the read's
        // own would have it asked about more issues than it answers for.
        for bad in ["", "0", "001", "100.0", " 100", "-1", "1e2", "101", "200000000"] {
            let given: Vec<String> = ["plan", bad].iter().map(|s| s.to_string()).collect();
            assert_eq!(main(&given), 2, "page \"{bad}\" is no page");
        }
        assert!(number("100").is_some());
        assert_eq!(number("7"), Some(7));
    }

    #[test]
    fn the_verbs_answer_their_status_for_the_answer_they_cannot_read() {
        // A word where a number belongs is usage for every verb, and the plan
        // of an answer that is no list ends 1 where an empty list ends 0.
        let states: Vec<String> = ["states", "extra"].iter().map(|s| s.to_string()).collect();
        assert_eq!(main(&states), 2);
        let word: Vec<String> = ["plan", "hundred"].iter().map(|s| s.to_string()).collect();
        assert_eq!(main(&word), 2);
        assert_eq!(usage(), 2);
    }

    /// The list of an answer, for the cases that pass items to `plan` directly.
    fn items_of(found: &str) -> Vec<Value> {
        match answer_of(found) {
            Answer::Issues(items) => items,
            _ => panic!("the case's answer is no list of issues"),
        }
    }
}
