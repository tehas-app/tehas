//! `console preview choose`: which open pull requests get a preview. These are
//! the rules of the header of src/preview.rs, run on the choice itself. What
//! the process prints for a page it cannot read and for a wrong argument is in
//! console/tests/preview_choose.rs.

use super::{choose, lines_of, read_pages};
use serde_json::{json, Value};

/// The commit of pull request `n` in the rows these cases make: 39 zeros and
/// the number's last digit.
fn sha_of(n: i64) -> String {
    format!("{}{}", "0".repeat(39), n % 10)
}

/// One row of the list as GitHub answers it, of the head repository `repo`
/// (none for "") and opened by `who` (no association for "").
fn pull_by(n: i64, repo: &str, who: &str) -> String {
    let mut row = json!({
        "number": n, "title": format!("Change {n}"),
        "html_url": format!("https://github.com/acme/widget/pull/{n}"), "state": "open",
        "updated_at": "2026-10-11T09:00:00Z",
        "head": {"sha": sha_of(n),
                 "repo": if repo.is_empty() { Value::Null } else { json!({"full_name": repo}) }},
        "body": "What the change does.",
    });
    if !who.is_empty() {
        row["author_association"] = json!(who);
    }
    row.to_string()
}

/// Of this repository, opened by a member.
fn pull(n: i64) -> String {
    pull_by(n, "acme/widget", "MEMBER")
}

/// `row` with `edit` made to it.
fn edited(row: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut row: Value = serde_json::from_str(row).unwrap();
    edit(&mut row);
    row.to_string()
}

/// The page of these rows.
fn page_of(rows: &[String]) -> String {
    format!("[{}]", rows.join(","))
}

/// The console's answer for this text on standard input.
fn answer_of(given: &str, max: u64) -> String {
    let pulls = read_pages(given).expect("a page");
    lines_of(&choose(&pulls, "acme/widget", max, "")).join("\n")
}

/// The console's answer for these pull requests, one page of them.
fn choose_of(max: u64, rows: &[String]) -> String {
    answer_of(&page_of(rows), max)
}

/// The first `n` words of each line.
fn words(text: &str, n: usize) -> Vec<String> {
    text.lines()
        .map(|l| l.split(' ').take(n).collect::<Vec<_>>().join(" "))
        .collect()
}

/// The numbers of the build lines.
fn builds(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("build "))
        .map(|l| l.split(' ').next().unwrap().to_string())
        .collect()
}

fn built(n: i64) -> String {
    format!("build {n} {} 2026-10-11T09:00:00Z", sha_of(n))
}

#[test]
fn one_of_ours_is_built_with_its_commit_and_the_time_github_gave() {
    assert_eq!(choose_of(5, &[pull(12)]), built(12));
}

#[test]
fn a_pull_request_from_a_fork_is_never_chosen_whoever_opened_it() {
    for who in ["OWNER", "MEMBER", "COLLABORATOR", "NONE"] {
        assert_eq!(
            choose_of(5, &[pull_by(31, "stranger/widget", who)]),
            "skip elsewhere 31",
            "opened by {who}"
        );
    }
}

#[test]
fn a_head_whose_repository_is_gone_is_a_fork_not_an_unclear_answer() {
    assert_eq!(
        choose_of(5, &[pull_by(32, "", "MEMBER")]),
        "skip elsewhere 32"
    );
}

#[test]
fn an_outsider_between_two_branches_of_the_repository_is_never_chosen() {
    for who in [
        "NONE",
        "CONTRIBUTOR",
        "FIRST_TIME_CONTRIBUTOR",
        "FIRST_TIMER",
        "MANNEQUIN",
        "member",
        "Owner",
    ] {
        assert_eq!(
            choose_of(5, &[pull_by(33, "acme/widget", who)]),
            "skip outside 33",
            "opened by {who}"
        );
    }
}

#[test]
fn an_owner_a_member_and_a_collaborator_are_chosen() {
    for who in ["OWNER", "MEMBER", "COLLABORATOR"] {
        assert_eq!(
            choose_of(5, &[pull_by(34, "acme/widget", who)]),
            built(34),
            "{who}"
        );
    }
}

#[test]
fn the_newest_max_are_built_and_the_rest_say_over_limit() {
    let rows = [pull(26), pull(25), pull(24), pull(21)];
    assert_eq!(
        words(&choose_of(2, &rows), 2),
        ["build 26", "build 25", "over-limit 24", "over-limit 21"]
    );
    // With room for four of them the same list is read as four builds: what
    // makes a row over-limit is the limit of the run, not the pull request.
    assert_eq!(words(&choose_of(4, &rows), 1), ["build"; 4]);
    // The over-limit rows carry the commit too: the run stops that container
    // and marks the state with it.
    let got = choose_of(2, &[pull(26), pull(25), pull(24)]);
    assert!(
        got.contains(&format!("over-limit 24 {} ", sha_of(24))),
        "{got}"
    );
}

#[test]
fn a_fork_or_an_outsider_takes_no_place_from_the_others() {
    let got = choose_of(
        2,
        &[
            pull_by(29, "acme/widget", "NONE"),
            pull_by(28, "acme/widget", "CONTRIBUTOR"),
            pull_by(26, "stranger/widget", "NONE"),
            pull(22),
            pull(21),
        ],
    );
    assert_eq!(builds(&got), ["22", "21"]);
    let got = choose_of(
        2,
        &[
            pull_by(29, "acme/widget", "NONE"),
            pull_by(26, "stranger/widget", "NONE"),
            pull(22),
        ],
    );
    assert_eq!(
        got,
        format!("{}\nskip outside 29\nskip elsewhere 26", built(22))
    );
}

#[test]
fn a_list_that_does_not_say_who_opened_one_of_ours_chooses_nothing() {
    // The row of #16 has its head here and an association that says nothing:
    // the field gone, null, or not text. None of them is read as an
    // outsider's, which would take the preview from #12, the one that serves.
    let edits: [fn(&mut Value); 4] = [
        |row| {
            row.as_object_mut().unwrap().remove("author_association");
        },
        |row| row["author_association"] = Value::Null,
        |row| row["author_association"] = json!(["OWNER"]),
        |row| row["author_association"] = json!(true),
    ];
    for (n, edit) in edits.into_iter().enumerate() {
        assert_eq!(
            choose_of(5, &[pull(12), edited(&pull(16), edit)]),
            "the list of open pull requests does not say who opened each",
            "filter {}",
            n + 1
        );
    }
}

#[test]
fn a_fork_that_does_not_say_who_opened_it_is_still_counted_as_a_fork() {
    // The association is only asked for of a head that is here, so a fork
    // with no association is a fork and stops nothing.
    let fork = edited(&pull_by(26, "stranger/widget", "MEMBER"), |row| {
        row.as_object_mut().unwrap().remove("author_association");
    });
    assert_eq!(
        choose_of(5, &[pull(12), fork]),
        format!("{}\nskip elsewhere 26", built(12))
    );
}

#[test]
fn a_head_that_is_not_a_commit_id_builds_nothing_and_is_not_counted() {
    for sha in [
        "main; id".to_string(),
        String::new(),
        "abc".to_string(),
        "z".repeat(40),
        "a".repeat(39),
    ] {
        let given = edited(&pull(15), |row| row["head"]["sha"] = json!(sha));
        assert_eq!(choose_of(5, &[given]), "", "{sha}");
    }
}

#[test]
fn a_number_that_is_not_one_builds_nothing() {
    for number in ["0", "-3", "1000000000", "\"12\"", "null"] {
        let n: Value = serde_json::from_str(number).unwrap();
        let given = edited(&pull(12), |row| row["number"] = n);
        assert_eq!(choose_of(5, &[given]), "", "{number}");
    }
}

#[test]
fn the_same_pull_request_on_two_pages_counts_once() {
    let twice = pull(12);
    assert_eq!(
        builds(&choose_of(2, &[twice.clone(), twice, pull(11)])),
        ["12", "11"]
    );
}

#[test]
fn two_pages_are_read_as_one_list() {
    let given = format!("{}\n{}\n", page_of(&[pull(12)]), page_of(&[pull(11)]));
    assert_eq!(
        answer_of(&given, 5),
        format!("{}\n{}", built(12), built(11))
    );
}

#[test]
fn the_repository_is_compared_without_regard_to_capitals() {
    assert_eq!(
        choose_of(5, &[pull_by(12, "ACME/Widget", "MEMBER")]),
        built(12)
    );
}

#[test]
fn max_0_leaves_every_one_of_ours_over_limit() {
    assert_eq!(words(&choose_of(0, &[pull(12)]), 1), ["over-limit"]);
}
