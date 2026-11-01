//! The poll's tidy of closed issues: tehas's state labels come off them, and
//! `tehas:shipped` goes on the ones a merged pull request closed. A label that
//! says a closed issue awaits a merge, or waits for a cycle, is wrong on the
//! issue page, in label searches and on the dashboard's links.
//!
//!   console tidy run [--dry-run]
//!     tehas-dispatch runs this once a poll, after the rest of the poll, and
//!     passes on --dry-run, which says what would change and changes nothing.
//!     It reads TEHAS_REPO, TEHAS_CONTROL_ISSUE, TEHAS_HOME and TEHAS_TIDY_WAIT
//!     from the environment tehas_load left. Each gh call may take
//!     TEHAS_TIDY_WAIT seconds [60] and none is retried:
//!       - one search for the closed issues that carry a state label, the most
//!         recently updated first, up to `shipped::PAGE`, the rest on later polls;
//!       - one read of what closed them, for the issues that may earn the label;
//!       - one `gh issue edit` per issue, with all its labels.
//!
//! An issue whose cycle holds its lock waits for the cycle. A failed read still
//! takes the state labels off, and earns no label on a guess. tehas:shipped,
//! needs-human-attention and the labels a person chooses never come off.
//! tehasbot status gives the same label from the same rule (src/shipped.rs).
//!
//! Exit: 0 nothing was left to remove, or all of it came off; 1 the search
//! failed, timed out or could not be read, or a label change failed (each is
//! logged, and the issues after it are still tidied); 2 no such verb; 64 an
//! argument `run` does not know; 78 the settings are missing or malformed.

use crate::shipped::{self, PAGE};
use serde_json::Value;
use std::env;
use std::io::Write as _;
use std::path::Path;
use std::time::Duration;
use tehas_core::forge::{Bounded, GhRunner};
use tehas_core::lock::{probe, Probe};
use tehas_core::time;

/// The state labels that say work is in progress or waiting: all that is ever
/// taken off, whatever the search sent back. A new one is added here on purpose,
/// and a test below pins the list.
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

/// The label tidy gives and never takes off. It is not in the search: one that
/// named it would take the label of a merge off the closed issues of a merge.
const SHIPPED: &str = shipped::SHIPPED;

/// A search's answer past this many bytes is refused rather than parsed.
const BIGGEST: usize = 8 * 1024 * 1024;

/// The read answers for a page at most, so a list of a thousand is no search
/// of this poll.
const MOST_ISSUES: usize = 1000;

/// The bound of one gh call when TEHAS_TIDY_WAIT is unset, and the grace
/// between TERM and KILL.
const WAIT: u64 = 60;
const GRACE: Duration = Duration::from_secs(1);

const USAGE: &str = "\
Usage:
  console tidy run             take the state labels off closed issues, and put
                               tehas:shipped on the ones a merged pull request closed
  console tidy run --dry-run   say what it would change, change nothing
  console tidy run -h|--help   print this text and exit 0
";

pub fn main(given: &[String]) -> u8 {
    match given.split_first() {
        Some((verb, rest)) if verb == "run" => run(rest),
        _ => {
            eprintln!("console tidy: usage: run [--dry-run]");
            2
        }
    }
}

fn run(given: &[String]) -> u8 {
    let dry = match arguments(given) {
        Ok(Some(dry)) => dry,
        Ok(None) => {
            print!("{USAGE}");
            return 0;
        }
        Err(word) => {
            eprint!("tehas tidy: unknown argument: {word}\n{USAGE}");
            return 64;
        }
    };
    let settings = match settings(|name| env::var(name).ok()) {
        Ok(settings) => settings,
        Err(why) => {
            eprintln!("tehas: {why}");
            return 78;
        }
    };
    let program =
        crate::forge::gh_program(Some(&settings.home), &env::var("PATH").unwrap_or_default());
    // With no gh to run, the search is what fails, and the log says so.
    let mut gh = Bounded {
        program: program.unwrap_or_else(|| "gh".to_string()),
        bound: Duration::from_secs(settings.wait),
        grace: GRACE,
    };
    let cycles = Path::new(&settings.home).join("state").join("cycles");
    // A lock that cannot be read is no free lock: the issue waits for a later poll.
    let running = |n: u64| {
        !matches!(
            probe(&cycles.join(format!("{n}.lock"))),
            Probe::Absent | Probe::Free
        )
    };
    tidy(&settings, dry, &mut gh, &running, &mut |line| {
        let _ = writeln!(std::io::stdout(), "{} {line}", time::iso(time::now()));
    })
}

/// `--dry-run` or not, None for help, and the first word `run` does not know.
fn arguments(given: &[String]) -> Result<Option<bool>, &str> {
    let help = |word: &str| word == "-h" || word == "--help";
    match given {
        [] => Ok(Some(false)),
        [one] if help(one) => Ok(None),
        [one] if one == "--dry-run" => Ok(Some(true)),
        [first, second, ..] if help(first) || first == "--dry-run" => Err(second),
        [first, ..] => Err(first),
    }
}

struct Settings {
    repo: String,
    control: u64,
    wait: u64,
    home: String,
}

/// What `tehas_require TEHAS_REPO TEHAS_CONTROL_ISSUE` says of the two settings,
/// and the rule of the bound. `lib/config.sh` loaded them; a run with none of
/// them stops here before it calls anything.
fn settings(get: impl Fn(&str) -> Option<String>) -> Result<Settings, String> {
    let file = format!(
        "{}/instance.env",
        get("TEHAS_INSTANCE_DIR")
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| "?".to_string())
    );
    let set = |name: &str| {
        get(name)
            .filter(|v| !v.trim_matches(' ').is_empty())
            .ok_or_else(|| format!("{name} is not set (see {file})"))
    };
    let repo = set("TEHAS_REPO")?;
    if !crate::forge::repo_shaped(&repo) {
        return Err(format!(
            "TEHAS_REPO is not valid: expected owner/name (see {file})"
        ));
    }
    let control = set("TEHAS_CONTROL_ISSUE")?;
    let Some(control) = number(&control) else {
        let why = if control.starts_with('0') || !control.bytes().all(|b| b.is_ascii_digit()) {
            "expected a positive whole number"
        } else {
            "expected at most 9 digits"
        };
        return Err(format!(
            "TEHAS_CONTROL_ISSUE is not valid: {why} (see {file})"
        ));
    };
    let wait = match get("TEHAS_TIDY_WAIT").filter(|w| !w.is_empty()) {
        None => WAIT,
        Some(w) => tehas_core::grammar::digits(&w, 5, false)
            .and_then(|n| u64::try_from(n).ok())
            .ok_or(format!("TEHAS_TIDY_WAIT is not a number of seconds: {w}"))?,
    };
    let home = get("TEHAS_HOME")
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| format!("{}/tehas", get("HOME").unwrap_or_default()));
    Ok(Settings {
        repo,
        control,
        wait,
        home,
    })
}

/// A control issue's number: one to nine digits with no leading zero.
fn number(word: &str) -> Option<u64> {
    u64::try_from(tehas_core::grammar::digits(word, 9, false)?).ok()
}

/// The two passes of one poll: `running` says whose cycle holds its lock, and
/// `say` is the log. The status is the command's exit.
fn tidy(
    settings: &Settings,
    dry: bool,
    gh: &mut dyn GhRunner,
    running: &dyn Fn(u64) -> bool,
    say: &mut dyn FnMut(&str),
) -> u8 {
    let repo = settings.repo.as_str();
    let search = search();
    let page = PAGE.to_string();
    let found = gh.run(&[
        "issue",
        "list",
        "--repo",
        repo,
        "--state",
        "closed",
        "--search",
        &search,
        "--limit",
        &page,
        "--json",
        "number,state,labels",
    ]);
    let Ok(found) = found else {
        say("WARN: tidy: the search for closed issues with state labels failed");
        return 1;
    };
    let Some(items) = answer_of(&found) else {
        say(
            "WARN: tidy: the console could not read the answer of the search as a list of issues: ",
        );
        return 1;
    };
    // The locked issues go after the plan's page is counted: an issue that waits
    // keeps its place among the page, and the read is not asked about it.
    let rows: Vec<Row> = plan(&items, &[settings.control], PAGE)
        .into_iter()
        .filter(|row| !running(row.number))
        .collect();

    let asked = rows.iter().filter(|row| row.ask).count();
    let merged = if asked == 0 {
        Some(Vec::new())
    } else {
        closers(gh, settings, &rows)
    };
    if merged.is_none() {
        say(&format!(
            "WARN: tidy: the read of what closed these issues failed, {SHIPPED} was not added for {asked} issues"
        ));
    }

    let mut rc = 0;
    for row in &rows {
        let n = row.number;
        let off = row.off.join(" ");
        // Only a read that answered says anything per issue: a failed read said
        // so once, and repeats of it read as issues a person closed by hand.
        let ship = row.ask && merged.as_ref().is_some_and(|m| m.contains(&n));
        if row.ask && merged.is_some() && !ship {
            say(&format!(
                "WARN: tidy #{n}: no merged pull request was read as the closer, {SHIPPED} was not added"
            ));
        }
        if dry {
            say(&format!("tidy #{n}: would remove {off}"));
            if ship {
                say(&format!("tidy #{n}: would add {SHIPPED}"));
            }
            continue;
        }
        let number = n.to_string();
        let mut args = vec!["issue", "edit", number.as_str(), "--repo", repo];
        for label in &row.off {
            args.extend(["--remove-label", label]);
        }
        if ship {
            args.extend(["--add-label", SHIPPED]);
        }
        if gh.run(&args).is_ok() {
            say(&format!("tidy #{n}: removed {off}"));
            if ship {
                say(&format!("tidy #{n}: added {SHIPPED}"));
            }
        } else {
            say(&format!("WARN: tidy #{n}: could not remove {off}"));
            rc = 1;
        }
    }
    rc
}

/// The state labels as one term of a search: GitHub reads labels joined by
/// commas as any of them.
fn search() -> String {
    let quoted: Vec<String> = STATES.iter().map(|state| format!("\"{state}\"")).collect();
    format!("label:{}", quoted.join(","))
}

/// The issues of the one read of the poll that a merged pull request closed.
/// None when the read did not answer.
fn closers(gh: &mut dyn GhRunner, settings: &Settings, rows: &[Row]) -> Option<Vec<u64>> {
    let mut q = format!("q=repo:{} is:issue is:closed", settings.repo);
    for row in rows.iter().filter(|row| row.ask) {
        q.push_str(&format!(" number:{}", row.number));
    }
    let query = format!("query={}", shipped::QUERY);
    let answer = gh.run(&["api", "graphql", "-f", &q, "-f", &query]).ok()?;
    shipped::numbers(&answer, &[settings.control.to_string()])
}

/// The issues of a search's answer, or None when it is no list of them: a
/// search that names a thing that is no issue did not answer the poll's
/// question, and that is a failed search and not a poll with nothing to tidy.
fn answer_of(found: &str) -> Option<Vec<Value>> {
    if found.len() > BIGGEST {
        return None;
    }
    let Value::Array(items) = serde_json::from_str::<Value>(found).ok()? else {
        return None;
    };
    (items.len() <= MOST_ISSUES && items.iter().all(|item| issue_of(item).is_some()))
        .then_some(items)
}

/// One closed issue with a label to take off.
#[derive(Debug, PartialEq)]
struct Row {
    number: u64,
    /// In the order of `STATES`. Every name is one of them, so it is safe to
    /// pass to the edit and to write to the log.
    off: Vec<&'static str>,
    /// Whether the read of the poll is asked about it.
    ask: bool,
}

/// What the poll does, one row per issue. Never on the list: an open issue or
/// one the answer gives no state for, a control issue, an issue with no state
/// label, and the second naming of an issue. The read is asked about an issue
/// that lacks the label and is among the first `page` such issues: past the
/// ceiling of a search GitHub answers nothing at all, so the rest keep their
/// labels for a later poll.
fn plan(items: &[Value], controls: &[u64], page: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut asked = 0;
    let mut seen: Vec<u64> = Vec::new();
    for item in items {
        if item.get("state").and_then(Value::as_str) != Some("CLOSED") {
            continue;
        }
        let Some(number) = issue_of(item) else {
            continue;
        };
        if controls.contains(&number) || seen.contains(&number) {
            continue;
        }
        seen.push(number);
        // The names come from GitHub: one that is no state label reaches neither
        // the edit nor the log, where it could carry a line of its own.
        let names = names(item);
        let off: Vec<&'static str> = STATES
            .iter()
            .copied()
            .filter(|state| names.contains(state))
            .collect();
        if off.is_empty() {
            continue;
        }
        let ask = !names.contains(&SHIPPED) && asked < page;
        if ask {
            asked += 1;
        }
        rows.push(Row { number, off, ask });
    }
    rows
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

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

    /// The plan of an answer written out by a case: nothing when the answer is
    /// no list of issues at all.
    fn planned(found: &str, controls: &[u64], page: usize) -> Option<Vec<Row>> {
        answer_of(found).map(|items| plan(&items, controls, page))
    }

    fn rows(items: &[String], page: usize) -> Vec<Row> {
        planned(&found(items), &[], page).unwrap_or_default()
    }

    fn numbers(rows: &[Row]) -> Vec<u64> {
        rows.iter().map(|row| row.number).collect()
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
        assert!(!STATES.contains(&"needs-human-attention"));
    }

    #[test]
    fn the_search_is_the_states_and_never_the_label_of_a_merge() {
        // Moved from test/tidy_test.sh with `console tidy states`: a search that
        // named the label would take it off the closed issues of a merge.
        assert_eq!(SHIPPED, "tehas:shipped");
        assert!(!STATES.contains(&SHIPPED));
        let term = search();
        let term = term.strip_prefix("label:").unwrap();
        let named: Vec<&str> = term.split(',').map(|n| n.trim_matches('"')).collect();
        assert_eq!(named, STATES);
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
        assert_eq!(rows[0].number, 194);
        assert_eq!(rows[0].off, ["tehas:awaiting-merge"]);
        // It carries the label already, so it is not read for it again.
        assert!(!rows[0].ask);
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
                    "tehas:refine",
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
            rows[0].off.join(" "),
            "tehas:running tehas:question tehas:planning tehas:refreshing tehas:blocked tehas:ready tehas:plan tehas:refine"
        );
    }

    #[test]
    fn an_issue_that_is_not_closed_is_never_on_the_plan() {
        let items = [
            issue(8, "OPEN", &["tehas:awaiting-merge"]),
            issue(10, "closed", &["tehas:question"]),
            "{\"number\":9,\"labels\":[{\"name\":\"tehas:question\"}]}".to_string(),
        ];
        assert_eq!(rows(&items, 100), Vec::new());
    }

    #[test]
    fn a_closed_issue_with_no_state_label_is_not_on_the_plan() {
        let items = [
            issue(10, "CLOSED", &[SHIPPED, "needs-human-attention"]),
            "{\"number\":20,\"state\":\"CLOSED\"}".to_string(),
            "{\"number\":21,\"state\":\"CLOSED\",\"labels\":[\"tehas:plan\"]}".to_string(),
        ];
        assert_eq!(rows(&items, 100), Vec::new());
    }

    #[test]
    fn the_control_issue_is_left_out_of_the_plan() {
        let items = [
            issue(1, "CLOSED", &["tehas:running"]),
            issue(23, "CLOSED", &["tehas:running"]),
        ];
        assert_eq!(numbers(&planned(&found(&items), &[1], 100).unwrap()), [23]);
    }

    #[test]
    fn a_name_that_is_no_state_label_is_neither_taken_off_nor_written() {
        let items = [
            "{\"number\":55,\"state\":\"CLOSED\",\"labels\":[{\"name\":\"tehas:running\"},\
             {\"name\":\"fake\\nWARN: tidy #999: forged line\"}]}"
                .to_string(),
        ];
        assert_eq!(rows(&items, 100)[0].off, ["tehas:running"]);
    }

    #[test]
    fn an_issue_named_twice_is_planned_once() {
        let items = [
            issue(7, "CLOSED", &["tehas:running"]),
            issue(7, "CLOSED", &["tehas:running"]),
        ];
        let rows = rows(&items, 100);
        assert_eq!(numbers(&rows), [7]);
        assert!(rows[0].ask);
    }

    #[test]
    fn the_candidates_past_the_page_are_left_to_a_later_poll() {
        let items = [
            issue(1, "CLOSED", &["tehas:running"]),
            issue(2, "CLOSED", &["tehas:running"]),
            issue(3, "CLOSED", &["tehas:running"]),
        ];
        let rows = rows(&items, 2);
        assert_eq!(numbers(&rows), [1, 2, 3]);
        assert_eq!(
            rows.iter().map(|r| r.ask).collect::<Vec<_>>(),
            [true, true, false]
        );
    }

    #[test]
    fn an_issue_that_carries_the_label_takes_no_room_of_the_read() {
        let items = [
            issue(1, "CLOSED", &["tehas:running", SHIPPED]),
            issue(2, "CLOSED", &["tehas:running"]),
            issue(3, "CLOSED", &["tehas:running"]),
        ];
        let rows = rows(&items, 2);
        assert_eq!(
            rows.iter().map(|r| r.ask).collect::<Vec<_>>(),
            [false, true, true]
        );
        assert_eq!(rows[0].off, ["tehas:running"]);
    }

    #[test]
    fn an_answer_that_is_no_list_of_issues_is_no_answer() {
        for answer in [
            "",
            "not json",
            "{}",
            "null",
            "{\"data\":{}}",
            "123",
            "[1,2]",
            "[null]",
            "[\"12\"]",
            "[{\"number\":\"12\",\"state\":\"CLOSED\"}]",
        ] {
            assert!(answer_of(answer).is_none(), "{answer} is no list");
        }
        assert_eq!(answer_of("[]"), Some(Vec::new()));
    }

    #[test]
    fn an_answer_past_its_ceiling_is_no_list() {
        assert!(answer_of(&format!("[{}]", " ".repeat(BIGGEST))).is_none());
        let many: Vec<String> = (1..=1001)
            .map(|n| issue(n, "CLOSED", &["tehas:running"]))
            .collect();
        assert!(answer_of(&found(&many)).is_none());
        assert_eq!(
            answer_of(&found(&many[..1000])).map(|l| l.len()),
            Some(1000)
        );
    }

    fn given(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn arguments_are_a_dry_run_or_help_and_nothing_else() {
        assert_eq!(arguments(&given(&[])), Ok(Some(false)));
        assert_eq!(arguments(&given(&["--dry-run"])), Ok(Some(true)));
        assert_eq!(arguments(&given(&["--help"])), Ok(None));
        assert_eq!(arguments(&given(&["-h"])), Ok(None));
        assert_eq!(arguments(&given(&["--dryrun"])), Err("--dryrun"));
        assert_eq!(arguments(&given(&["--dry-run", "--help"])), Err("--help"));
        assert_eq!(arguments(&given(&["--help", "x"])), Err("x"));
        assert_eq!(run(&given(&["--dryrun"])), 64);
    }

    #[test]
    fn a_verb_that_is_not_run_is_usage() {
        assert_eq!(main(&given(&[])), 2);
        // The verbs of the bash that made these calls itself are gone.
        for verb in ["states", "plan", "issues"] {
            assert_eq!(main(&given(&[verb])), 2);
        }
    }

    /// Settings from a list of names, and what they make.
    fn read(names: &[(&str, &str)]) -> Result<Settings, String> {
        let names: HashMap<String, String> = names
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        settings(|name| names.get(name).cloned())
    }

    fn fails(names: &[(&str, &str)]) -> String {
        read(names).err().expect("settings that must be refused")
    }

    const GOOD: [(&str, &str); 2] = [("TEHAS_REPO", "acme/widget"), ("TEHAS_CONTROL_ISSUE", "1")];

    #[test]
    fn the_settings_default_as_the_shell_defaulted_them() {
        let s = read(&[GOOD[0], GOOD[1], ("HOME", "/h")]).unwrap();
        assert_eq!(s.repo, "acme/widget");
        assert_eq!(s.control, 1);
        assert_eq!(s.wait, 60);
        assert_eq!(s.home, "/h/tehas");
        let s = read(&[
            GOOD[0],
            GOOD[1],
            ("TEHAS_TIDY_WAIT", ""),
            ("TEHAS_HOME", "/t"),
        ])
        .unwrap();
        assert_eq!(s.wait, 60);
        assert_eq!(s.home, "/t");
    }

    #[test]
    fn a_missing_or_malformed_setting_says_which_and_where() {
        let dir = ("TEHAS_INSTANCE_DIR", "/i");
        assert_eq!(fails(&[dir]), "TEHAS_REPO is not set (see /i/instance.env)");
        assert_eq!(
            fails(&[("TEHAS_REPO", "   ")]),
            "TEHAS_REPO is not set (see ?/instance.env)"
        );
        assert_eq!(
            fails(&[dir, ("TEHAS_REPO", "widget")]),
            "TEHAS_REPO is not valid: expected owner/name (see /i/instance.env)"
        );
        assert_eq!(
            fails(&[dir, GOOD[0]]),
            "TEHAS_CONTROL_ISSUE is not set (see /i/instance.env)"
        );
        for bad in ["0", "09", "1.0", "-1", "1e2"] {
            assert_eq!(
                fails(&[dir, GOOD[0], ("TEHAS_CONTROL_ISSUE", bad)]),
                "TEHAS_CONTROL_ISSUE is not valid: expected a positive whole number (see /i/instance.env)"
            );
        }
        assert_eq!(
            fails(&[dir, GOOD[0], ("TEHAS_CONTROL_ISSUE", "1234567890")]),
            "TEHAS_CONTROL_ISSUE is not valid: expected at most 9 digits (see /i/instance.env)"
        );
        for bad in ["0", "007", "soon", "100000", "-1"] {
            assert_eq!(
                fails(&[GOOD[0], GOOD[1], ("TEHAS_TIDY_WAIT", bad)]),
                format!("TEHAS_TIDY_WAIT is not a number of seconds: {bad}")
            );
        }
    }

    /// gh for a case: the search and the read answer as the case says, an edit of
    /// a number in `refuses` fails, and every call is written down.
    struct Stub {
        search: Result<String, String>,
        read: Result<String, String>,
        refuses: Vec<u64>,
        calls: Vec<String>,
    }

    impl Stub {
        fn new(search: &[String], read: &str) -> Stub {
            Stub {
                search: Ok(found(search)),
                read: Ok(read.to_string()),
                refuses: Vec::new(),
                calls: Vec::new(),
            }
        }

        fn edits(&self) -> Vec<&str> {
            self.called("issue edit")
        }

        fn called(&self, start: &str) -> Vec<&str> {
            self.calls
                .iter()
                .filter(|c| c.starts_with(start))
                .map(String::as_str)
                .collect()
        }
    }

    impl GhRunner for Stub {
        fn run(&mut self, args: &[&str]) -> Result<String, String> {
            self.calls.push(args.join(" "));
            match args[..2] {
                ["issue", "list"] => self.search.clone(),
                ["api", "graphql"] => self.read.clone(),
                _ if self.refuses.iter().any(|n| n.to_string() == args[2]) => {
                    Err("refused".to_string())
                }
                _ => Ok(String::new()),
            }
        }
    }

    /// A whole answer of the read: `merged` closed by a merge, `by_hand` not.
    fn closed(merged: &[u64], by_hand: &[u64]) -> String {
        let node = |n: &u64, closer: &str| {
            format!("{{\"number\":{n},\"timelineItems\":{{\"nodes\":[{{\"closer\":{closer}}}]}}}}")
        };
        let nodes: Vec<String> = merged
            .iter()
            .map(|n| node(n, "{\"merged\":true}"))
            .chain(by_hand.iter().map(|n| node(n, "null")))
            .collect();
        format!(
            "{{\"data\":{{\"search\":{{\"nodes\":[{}]}}}}}}",
            nodes.join(",")
        )
    }

    /// One poll against the stub, a dry one when asked, with the issues of
    /// `locked` held by their cycles: its status and its log lines.
    fn poll(gh: &mut Stub, dry: bool, locked: &[u64]) -> (u8, Vec<String>) {
        let settings = read(&GOOD).unwrap();
        let lines = RefCell::new(Vec::new());
        let rc = tidy(&settings, dry, gh, &|n| locked.contains(&n), &mut |line| {
            lines.borrow_mut().push(line.to_string())
        });
        (rc, lines.into_inner())
    }

    fn closed_with(n: u64, label: &str) -> String {
        issue(n, "CLOSED", &[label])
    }

    #[test]
    fn the_search_and_the_read_are_the_calls_a_poll_makes() {
        let mut gh = Stub::new(&[closed_with(12, "tehas:running")], &closed(&[12], &[]));
        poll(&mut gh, false, &[]);
        let term = "label:\"tehas:running\",\"tehas:awaiting-merge\",\"tehas:question\",\"tehas:planning\",\"tehas:refreshing\",\"tehas:blocked\",\"tehas:ready\",\"tehas:plan\",\"tehas:refine\"";
        assert_eq!(
            gh.calls[0],
            format!(
                "issue list --repo acme/widget --state closed --search {term} --limit 100 --json number,state,labels"
            )
        );
        assert_eq!(
            gh.calls[1],
            format!(
                "api graphql -f q=repo:acme/widget is:issue is:closed number:12 -f query={}",
                shipped::QUERY
            )
        );
        assert_eq!(gh.calls.len(), 3);
    }

    #[test]
    fn a_merged_closer_earns_the_label_in_the_one_edit_of_the_issue() {
        let mut gh = Stub::new(
            &[
                issue(30, "CLOSED", &["tehas:ready", "tehas:running"]),
                closed_with(31, "tehas:plan"),
            ],
            &closed(&[30], &[31]),
        );
        let (rc, log) = poll(&mut gh, false, &[]);
        assert_eq!(rc, 0);
        assert_eq!(
            gh.edits(),
            [
                "issue edit 30 --repo acme/widget --remove-label tehas:running --remove-label tehas:ready --add-label tehas:shipped",
                "issue edit 31 --repo acme/widget --remove-label tehas:plan",
            ]
        );
        assert_eq!(
            log,
            [
                "tidy #30: removed tehas:running tehas:ready",
                "tidy #30: added tehas:shipped",
                "WARN: tidy #31: no merged pull request was read as the closer, tehas:shipped was not added",
                "tidy #31: removed tehas:plan",
            ]
        );
    }

    #[test]
    fn a_read_that_found_nothing_is_an_answer_and_says_so_per_issue() {
        // `shipped list` found no merge and `shipped whole` said the answer was
        // whole, so the shell took this for a poll that shipped nothing.
        let mut gh = Stub::new(&[closed_with(47, "tehas:question")], &closed(&[], &[]));
        let (rc, log) = poll(&mut gh, false, &[]);
        assert_eq!(rc, 0);
        assert_eq!(
            log,
            [
                "WARN: tidy #47: no merged pull request was read as the closer, tehas:shipped was not added",
                "tidy #47: removed tehas:question",
            ]
        );
    }

    #[test]
    fn a_read_that_failed_is_one_line_and_the_labels_still_come_off() {
        for read in [
            Err("rate limited".to_string()),
            Ok("not json".to_string()),
            Ok("{\"errors\":[{\"message\":\"no\"}],\"data\":null}".to_string()),
        ] {
            let mut gh = Stub::new(
                &[
                    closed_with(40, "tehas:question"),
                    closed_with(41, "tehas:plan"),
                ],
                "",
            );
            gh.read = read;
            let (rc, log) = poll(&mut gh, false, &[]);
            assert_eq!(rc, 0);
            assert_eq!(
                gh.edits(),
                [
                    "issue edit 40 --repo acme/widget --remove-label tehas:question",
                    "issue edit 41 --repo acme/widget --remove-label tehas:plan",
                ]
            );
            assert_eq!(
                log,
                [
                    "WARN: tidy: the read of what closed these issues failed, tehas:shipped was not added for 2 issues",
                    "tidy #40: removed tehas:question",
                    "tidy #41: removed tehas:plan",
                ]
            );
        }
    }

    #[test]
    fn nothing_to_ask_about_makes_no_read() {
        let mut gh = Stub::new(&[issue(44, "CLOSED", &["tehas:plan", SHIPPED])], "");
        let (rc, _) = poll(&mut gh, false, &[]);
        assert_eq!(rc, 0);
        assert!(gh.called("api").is_empty());
        assert_eq!(gh.edits().len(), 1);
    }

    #[test]
    fn a_locked_issue_waits_and_is_not_asked_about() {
        let mut gh = Stub::new(
            &[
                closed_with(11, "tehas:running"),
                closed_with(12, "tehas:running"),
            ],
            &closed(&[11, 12], &[]),
        );
        let (rc, log) = poll(&mut gh, false, &[11]);
        assert_eq!(rc, 0);
        assert!(gh.calls[1].contains("is:closed number:12 -f"));
        assert_eq!(gh.edits().len(), 1);
        assert!(gh.edits()[0].starts_with("issue edit 12 "));
        assert!(!log.iter().any(|l| l.contains("#11")));
    }

    #[test]
    fn the_control_issue_is_neither_edited_nor_asked_about() {
        let mut gh = Stub::new(
            &[
                closed_with(1, "tehas:running"),
                closed_with(23, "tehas:running"),
            ],
            &closed(&[1, 23], &[]),
        );
        poll(&mut gh, false, &[]);
        assert!(gh.calls[1].contains("is:closed number:23 -f"));
        assert_eq!(gh.edits().len(), 1);
        assert!(gh.edits()[0].starts_with("issue edit 23 "));
    }

    #[test]
    fn a_dry_run_reads_as_a_poll_does_and_edits_nothing() {
        let mut gh = Stub::new(
            &[
                closed_with(194, "tehas:plan"),
                closed_with(195, "tehas:ready"),
            ],
            &closed(&[194], &[195]),
        );
        let (rc, log) = poll(&mut gh, true, &[]);
        assert_eq!(rc, 0);
        assert!(gh.edits().is_empty());
        assert_eq!(gh.called("api").len(), 1);
        assert_eq!(
            log,
            [
                "tidy #194: would remove tehas:plan",
                "tidy #194: would add tehas:shipped",
                "WARN: tidy #195: no merged pull request was read as the closer, tehas:shipped was not added",
                "tidy #195: would remove tehas:ready",
            ]
        );
    }

    #[test]
    fn an_edit_that_fails_is_logged_and_the_next_issue_is_still_tidied() {
        let mut gh = Stub::new(
            &[
                closed_with(20, "tehas:question"),
                closed_with(21, "tehas:ready"),
            ],
            &closed(&[], &[20, 21]),
        );
        gh.refuses = vec![20];
        let (rc, log) = poll(&mut gh, false, &[]);
        assert_eq!(rc, 1);
        assert!(log.contains(&"WARN: tidy #20: could not remove tehas:question".to_string()));
        assert!(log.contains(&"tidy #21: removed tehas:ready".to_string()));
        assert_eq!(gh.edits().len(), 2);
    }

    #[test]
    fn a_search_that_failed_or_is_no_list_stops_the_poll_with_1() {
        let mut gh = Stub::new(&[], "");
        gh.search = Err("no gh".to_string());
        let (rc, log) = poll(&mut gh, false, &[]);
        assert_eq!(rc, 1);
        assert_eq!(
            log,
            ["WARN: tidy: the search for closed issues with state labels failed"]
        );
        assert_eq!(gh.calls.len(), 1);

        gh.search = Ok("{\"data\":{}}".to_string());
        let (rc, log) = poll(&mut gh, false, &[]);
        assert_eq!(rc, 1);
        assert_eq!(
            log,
            ["WARN: tidy: the console could not read the answer of the search as a list of issues: "]
        );
    }

    #[test]
    fn an_empty_search_is_one_call_and_no_line() {
        let mut gh = Stub::new(&[], "");
        let (rc, log) = poll(&mut gh, false, &[]);
        assert_eq!(rc, 0);
        assert!(log.is_empty());
        assert_eq!(gh.calls.len(), 1);
    }
}
