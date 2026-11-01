//! Which pull requests get a preview, for libexec/preview. The shell reads
//! the pages from GitHub; this decides what is done with each.
//!
//!   console preview choose <repo> [max] <pull requests
//!
//! Standard input holds the open pull requests as GitHub answered them: the
//! bytes of each page in turn, one JSON array per page, which is what the run
//! keeps. Pages written over several lines are read as pages too. <repo> is
//! the product repository, compared without regard to capitals as GitHub
//! itself does. [max] is TEHAS_PREVIEW_MAX: the newest of them get a preview.
//! A list that cannot be read chooses nothing, and says `nothing read`.
//!
//! The rules, each on its own line of the answer:
//!
//! - `build <number> <commit> <updated_at>`: open, its head branch in the
//!   repository itself, opened by an owner, member or collaborator, one of
//!   the newest [max]. The list is newest first, as it was asked for.
//! - `over-limit <number> <commit> <updated_at>`: the same but past [max].
//! - `skip elsewhere <number>`: its head is in another repository, a fork, or
//!   a repository that is gone. It is counted as a fork whoever opened it.
//! - `skip outside <number>`: its head is here and an outsider opened it: on
//!   a public repository anyone can open a pull request between two branches
//!   the repository already has.
//!
//! A pull request whose number is not one to nine digits without a zero in
//! front, or whose head is not a commit id, is neither counted nor built, and
//! no one is told about it, as the run does today.
//!
//! A page that does not say who opened one of ours is not a clear answer:
//! then only the line that says so is printed, exit 0, and the run builds and
//! removes nothing. Reading it as "an outsider's" would take a preview from a
//! pull request that serves.
//!
//! Exit: 0 the choice was printed, whatever it is; 2 usage.

use serde_json::Value;
use std::io::{self, Read};

/// The run's decision about one pull request.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Build {
        number: u64,
        sha: String,
        seen: String,
    },
    OverLimit {
        number: u64,
        sha: String,
        seen: String,
    },
    Elsewhere(u64),
    Outside(u64),
}

/// What a whole list of pages came to. `Incomplete` says the list of them was
/// read: one of ours whose answer did not say who opened it. `Unreadable`
/// says the bytes were not the list at all. Both choose nothing.
#[derive(Debug, PartialEq)]
pub enum Choice {
    Incomplete,
    Chosen {
        chosen: Vec<Verdict>,
        elsewhere: usize,
        outside: usize,
    },
}

fn lowercase(text: &str) -> String {
    text.to_lowercase()
}

/// The number as the run matches one: one to nine digits, no zero in front.
fn number_of(pull: &Value) -> Option<u64> {
    let n = pull.get("number").and_then(Value::as_u64)?;
    if (1..=999_999_999).contains(&n) {
        Some(n)
    } else {
        None
    }
}

/// The head is this run's only when it is a commit id it can fetch.
fn sha_of(pull: &Value) -> Option<String> {
    let sha = pull.pointer("/head/sha").and_then(Value::as_str)?;
    if sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(sha.to_ascii_lowercase())
    } else {
        None
    }
}

/// What GitHub says about the head: in this repository, in another one, or
/// an answer that does not say.
enum Head {
    Here,
    Elsewhere,
    Unknown,
}

fn head_of(pull: &Value, repo: &str) -> Head {
    let head = match pull.get("head") {
        Some(head) if head.is_object() => head,
        _ => return Head::Unknown,
    };
    // A deleted fork has no head repository at all: the branch is not here.
    let full = match head.get("repo") {
        Some(Value::Null) => return Head::Elsewhere,
        Some(repo) => repo.get("full_name").and_then(Value::as_str),
        None => None,
    };
    match full {
        Some(full) if lowercase(full) == lowercase(repo) => Head::Here,
        Some(_) => Head::Elsewhere,
        None => Head::Unknown,
    }
}

/// Who opened it. The run reads an answer that does not say as no answer,
/// never as an outsider's.
enum By {
    Ours,
    Outsider,
    /// An association of a shape that says nothing: not text at all.
    NotSaid,
}

/// Who opened it. `None` asks the question of a head that is here: an answer
/// that does not say is no answer at all, never an outsider's, because
/// reading one as an outsider's would take a preview from a pull request that
/// serves. The sweep keeps on the same answer.
fn by_us(pull: &Value) -> Option<By> {
    match pull.get("author_association") {
        Some(Value::String(who)) => Some(
            if matches!(who.as_str(), "OWNER" | "MEMBER" | "COLLABORATOR") {
                By::Ours
            } else {
                By::Outsider
            },
        ),
        None | Some(Value::Null) => None,
        // An association of another shape is not an outsider's either: it is
        // an answer that does not say, which the caller reads as the whole
        // list being unclear.
        Some(_) => Some(By::NotSaid),
    }
}

/// The time GitHub gives for the pull request, which is what the notice is
/// recorded against. A row of the console names the verdict, the number, the
/// commit and this time, four words and no more, so a time that is not a word
/// of its own is no time at all: the notice reads one as a description that
/// changed, and that is what an answer without a time says anyway.
fn seen_of(pull: &Value) -> String {
    match pull.get("updated_at").and_then(Value::as_str) {
        Some(seen) if !seen.is_empty() && !seen.chars().any(char::is_whitespace) => {
            seen.to_string()
        }
        _ => String::new(),
    }
}

/// The list as the shell kept it: the bytes of every page GitHub answered, one
/// page after another, each of them the array of that page. Each value must be
/// an array and every element of it an object; anything else, a bare pull
/// request included, is a list that cannot be read. A page spread over more
/// than one line is still one page, and an empty one is a page of none.
pub fn read_pages(text: &str) -> Result<Vec<Value>, String> {
    fn why(given: &str) -> String {
        let head: String = given.chars().take(40).collect();
        format!("not a list of pull requests: {head}")
    }
    let trimmed = text.trim();
    let mut rows = Vec::new();
    let mut given = 0usize;
    for answer in serde_json::Deserializer::from_str(trimmed).into_iter::<Value>() {
        given += 1;
        match answer.map_err(|e| why(&e.to_string()))? {
            Value::Array(page) if page.iter().all(Value::is_object) => rows.extend(page),
            other => return Err(why(&other.to_string())),
        }
    }
    // No bytes at all is not a list of none: it is an answer this run did not
    // get, and the run builds and removes on it as little as on a page it
    // could not read. A page of none is the list that was read, and it
    // chooses nothing: the pages were asked for and GitHub sent nothing.
    if given == 0 {
        return Err(why(trimmed));
    }
    Ok(rows)
}

/// The choice: which of these pull requests get a preview, and the two counts
/// the run says out loud.
pub fn choose(pulls: &[Value], repo: &str, max: u64) -> Choice {
    // Before anything is said about the list: one with its head here whose
    // answer does not say who opened it makes the whole list unclear. A fork
    // that says nothing is still a fork, and a head that is nowhere leaves the
    // answer saying nothing at all.
    let unclear = pulls.iter().any(|pull| {
        number_of(pull).is_some()
            && sha_of(pull).is_some()
            && matches!(head_of(pull, repo), Head::Here)
            && matches!(by_us(pull), None | Some(By::NotSaid))
    });
    if unclear {
        return Choice::Incomplete;
    }
    let mut chosen = Vec::new();
    let mut seen: Vec<u64> = Vec::new();
    let (mut elsewhere, mut outside, mut taken) = (0usize, 0usize, 0u64);
    for pull in pulls {
        let (Some(n), Some(sha)) = (number_of(pull), sha_of(pull)) else {
            continue;
        };
        // The pages are asked for one at a time and the list moves while they
        // are read: a pull request opened then comes back on two of them and
        // counts once, so that it takes no preview from another.
        if seen.contains(&n) {
            continue;
        }
        seen.push(n);
        match (head_of(pull, repo), by_us(pull)) {
            // A fork is counted as a fork whoever opened it, and an outsider
            // is counted for the line the run says: neither gets a row, which
            // is what takes them out of the count of previews.
            (Head::Elsewhere, _) => {
                elsewhere += 1;
                chosen.push(Verdict::Elsewhere(n));
            }
            // An answer that does not say where the head is, or an answer
            // with no association at all, says nothing: it is neither built
            // nor counted. An association of another shape leaves the whole
            // list unclear by the check above.
            (Head::Unknown, _) | (Head::Here, None) | (Head::Here, Some(By::NotSaid)) => {}
            (Head::Here, Some(By::Ours)) if taken < max => {
                taken += 1;
                chosen.push(Verdict::Build {
                    number: n,
                    sha,
                    seen: seen_of(pull),
                });
            }
            (Head::Here, Some(By::Ours)) => chosen.push(Verdict::OverLimit {
                number: n,
                sha,
                seen: seen_of(pull),
            }),
            (Head::Here, Some(By::Outsider)) => {
                outside += 1;
                chosen.push(Verdict::Outside(n));
            }
        }
    }
    Choice::Chosen {
        chosen,
        elsewhere,
        outside,
    }
}

/// The console's answer, in the order of the list: build and over-limit rows
/// first, newest first, because those are what the run acts on, and the skips
/// after them. A row is one line: three words for a skip, four for a choice,
/// the last of them the time GitHub gives, which holds no space.
fn lines_of(choice: &Choice) -> Vec<String> {
    match choice {
        Choice::Incomplete => {
            vec!["the list of open pull requests does not say who opened each".to_string()]
        }
        Choice::Chosen { chosen, .. } => {
            let row = |verdict: &Verdict| match verdict {
                Verdict::Build { number, sha, seen } => format!("build {number} {sha} {seen}"),
                Verdict::OverLimit { number, sha, seen } => {
                    format!("over-limit {number} {sha} {seen}")
                }
                Verdict::Elsewhere(number) => format!("skip elsewhere {number}"),
                Verdict::Outside(number) => format!("skip outside {number}"),
            };
            let taken = |want: fn(&Verdict) -> bool| {
                chosen
                    .iter()
                    .filter(|v| want(v))
                    .map(row)
                    .collect::<Vec<_>>()
            };
            let mut lines =
                taken(|v| matches!(v, Verdict::Build { .. } | Verdict::OverLimit { .. }));
            lines.extend(taken(|v| {
                matches!(v, Verdict::Elsewhere(_) | Verdict::Outside(_))
            }));
            lines
        }
    }
}

fn usage() -> u8 {
    eprintln!("console preview choose: usage: <repo> [max] <pull requests");
    2
}

pub fn choose_main(args: &[String]) -> u8 {
    let (repo, max) = match args {
        [repo, max] => match max.parse::<u64>() {
            Ok(max) => (repo.as_str(), max),
            Err(_) => {
                eprintln!("console preview choose: <max> is not a whole number: {max}");
                return 2;
            }
        },
        [repo] => (repo.as_str(), 5),
        _ => return usage(),
    };
    let mut given = String::new();
    if io::stdin().read_to_string(&mut given).is_err() {
        eprintln!("console preview choose: standard input could not be read");
        return 2;
    }
    // Fail closed: a list that cannot be read chooses nothing, said on
    // standard output so that the run reads it as an empty choice. What the
    // bytes were is said on standard error, where the run's log keeps it.
    let pulls = match read_pages(&given) {
        Ok(pulls) => pulls,
        Err(why) => {
            eprintln!("console preview choose: {why}");
            println!("nothing read");
            return 0;
        }
    };
    for line in lines_of(&choose(&pulls, repo, max)) {
        println!("{line}");
    }
    0
}

/// The whole verb: `main` took "preview" off the arguments.
pub fn main(given: &[String]) -> u8 {
    match given.first().map(String::as_str) {
        Some("choose") => choose_main(&given[1..]),
        Some("tell") => crate::notice::main(given),
        Some("network") => crate::edge::network_main(&given[1..]),
        _ => {
            eprintln!("console preview: usage: tell <number> <time> <repo> <root> <address>");
            eprintln!("console preview choose: usage: <repo> [max] <pull requests");
            eprintln!("console preview network: usage: check|create|ensure <network>");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REPO: &str = "acme/widget";

    /// An open pull request of ours, as the list answers it.
    fn ours(n: u64) -> Value {
        json!({
            "number": n, "title": "Change", "html_url": "u", "state": "open",
            "updated_at": "2026-10-11T09:00:00Z",
            "head": {"sha": format!("{n:040x}"), "repo": {"full_name": "Acme/Widget"}},
            "author_association": "MEMBER",
        })
    }

    fn sha40(byte: u8) -> String {
        (0..40).map(|_| format!("{byte:01x}")).collect()
    }

    #[test]
    fn a_fork_is_never_chosen_whatever_else_is_true() {
        let fork = |who: &str| {
            let mut pull = ours(31);
            pull["head"]["repo"]["full_name"] = json!("stranger/widget");
            pull["author_association"] = json!(who);
            pull
        };
        // An owner's fork is still a fork: the head is not in the product
        // repository, so its code is not the repository's.
        for who in ["OWNER", "MEMBER", "COLLABORATOR", "NONE", ""] {
            let mut pull = fork(who);
            if who.is_empty() {
                pull.as_object_mut().unwrap().remove("author_association");
            }
            let choice = choose(&[pull], REPO, 5);
            assert_eq!(
                choice,
                Choice::Chosen {
                    chosen: vec![Verdict::Elsewhere(31)],
                    elsewhere: 1,
                    outside: 0,
                },
                "{who}"
            );
            assert_eq!(lines_of(&choice), ["skip elsewhere 31"]);
        }
    }

    #[test]
    fn a_deleted_fork_is_elsewhere_and_not_unclear() {
        let mut pull = ours(32);
        pull["head"]["repo"] = json!(null);
        let choice = choose(&[pull], REPO, 5);
        assert_eq!(
            lines_of(&choice),
            ["skip elsewhere 32"],
            "a head repository that is gone is not an unclear answer"
        );
    }

    #[test]
    fn an_outsider_between_our_own_branches_is_never_chosen() {
        // The head is in the repository: on a public repository anyone can
        // open a pull request between two branches that are already there.
        for who in [
            "NONE",
            "CONTRIBUTOR",
            "FIRST_TIME_CONTRIBUTOR",
            "FIRST_TIMER",
            "MANNEQUIN",
            "member",
            "Owner",
        ] {
            let mut pull = ours(33);
            pull["author_association"] = json!(who);
            let choice = choose(&[pull], REPO, 5);
            assert_eq!(
                choice,
                Choice::Chosen {
                    chosen: vec![Verdict::Outside(33)],
                    elsewhere: 0,
                    outside: 1,
                },
                "{who}"
            );
            assert_eq!(lines_of(&choice), ["skip outside 33"]);
        }
    }

    #[test]
    fn an_owner_a_member_and_a_collaborator_are_chosen() {
        for who in ["OWNER", "MEMBER", "COLLABORATOR"] {
            let mut pull = ours(34);
            pull["author_association"] = json!(who);
            let choice = choose(&[pull], REPO, 5);
            assert_eq!(
                lines_of(&choice),
                [format!("build 34 {:040x} 2026-10-11T09:00:00Z", 34u64)],
                "{who}"
            );
        }
    }

    #[test]
    fn the_newest_max_are_built_and_the_rest_are_over_limit() {
        let pulls: Vec<Value> = (21..=26).rev().map(ours).collect();
        let choice = choose(&pulls, REPO, 2);
        let Choice::Chosen { chosen, .. } = &choice else {
            panic!("a clear list");
        };
        assert!(matches!(chosen[0], Verdict::Build { number: 26, .. }));
        assert!(matches!(chosen[1], Verdict::Build { number: 25, .. }));
        for row in &chosen[2..] {
            assert!(matches!(row, Verdict::OverLimit { .. }), "{row:?}");
        }
        let lines = lines_of(&choice);
        assert_eq!(lines.len(), 6);
        assert!(lines[2].starts_with("over-limit 24 "));
        assert!(lines[5].starts_with("over-limit 21 "));
    }

    #[test]
    fn those_who_are_not_ours_take_no_place_from_the_others() {
        // Three outsiders and a fork, and both of ours still get a preview
        // out of a limit of two.
        let outsider = |n: u64, who: &str| {
            let mut pull = ours(n);
            pull["author_association"] = json!(who);
            pull
        };
        let mut fork = ours(26);
        fork["head"]["repo"]["full_name"] = json!("stranger/widget");
        fork["author_association"] = json!("NONE");
        let pulls = vec![
            outsider(29, "NONE"),
            outsider(28, "CONTRIBUTOR"),
            fork,
            ours(22),
            outsider(21, "FIRST_TIMER"),
            ours(20),
        ];
        let choice = choose(&pulls, REPO, 2);
        let Choice::Chosen { chosen, .. } = &choice else {
            panic!("a clear list");
        };
        // The two of ours are the previews; the rest are said, and take no
        // place: the choice keeps the order of the list, as GitHub gave it.
        let previews: Vec<u64> = chosen
            .iter()
            .filter_map(|row| match row {
                Verdict::Build { number, .. } => Some(*number),
                _ => None,
            })
            .collect();
        assert_eq!(previews, [22, 20]);
        assert_eq!(
            choice,
            Choice::Chosen {
                chosen: vec![
                    Verdict::Outside(29),
                    Verdict::Outside(28),
                    Verdict::Elsewhere(26),
                    Verdict::Build {
                        number: 22,
                        sha: format!("{:040x}", 22u64),
                        seen: "2026-10-11T09:00:00Z".into(),
                    },
                    Verdict::Outside(21),
                    Verdict::Build {
                        number: 20,
                        sha: format!("{:040x}", 20u64),
                        seen: "2026-10-11T09:00:00Z".into(),
                    },
                ],
                elsewhere: 1,
                outside: 3,
            }
        );
    }

    #[test]
    fn a_list_that_does_not_say_who_opened_one_of_ours_reads_nothing() {
        for shape in [
            Box::new(|pull: &mut Value| {
                pull.as_object_mut().unwrap().remove("author_association");
            }) as Box<dyn Fn(&mut Value)>,
            Box::new(|pull: &mut Value| pull["author_association"] = json!(null)),
            Box::new(|pull: &mut Value| pull["author_association"] = json!(["OWNER"])),
            Box::new(|pull: &mut Value| pull["author_association"] = json!(true)),
        ] {
            let mut quiet = ours(16);
            shape(&mut quiet);
            // An unclear answer about one of ours is not read as an
            // outsider's: it would take a preview from #12, which serves.
            let choice = choose(&[ours(12), quiet], REPO, 5);
            assert_eq!(choice, Choice::Incomplete);
            assert_eq!(
                lines_of(&choice),
                ["the list of open pull requests does not say who opened each"]
            );
        }
    }

    #[test]
    fn a_fork_that_does_not_say_who_opened_it_reads_as_a_fork() {
        // The association is only asked for of a head that is here.
        let mut fork = ours(26);
        fork["head"]["repo"]["full_name"] = json!("stranger/widget");
        fork.as_object_mut().unwrap().remove("author_association");
        let choice = choose(&[ours(12), fork], REPO, 5);
        let Choice::Chosen { chosen, .. } = &choice else {
            panic!("a clear list");
        };
        assert!(matches!(chosen[0], Verdict::Build { number: 12, .. }));
        assert_eq!(lines_of(&choice)[1], "skip elsewhere 26");
    }

    #[test]
    fn an_outsider_who_says_nothing_is_no_answer() {
        // The head names no repository and the answer says nobody: neither a
        // fork nor an outsider, and nothing to count.
        let mut head = ours(16);
        head["head"].as_object_mut().unwrap().remove("repo");
        head.as_object_mut().unwrap().remove("author_association");
        let choice = choose(&[head], REPO, 5);
        assert_eq!(
            choice,
            Choice::Chosen {
                chosen: Vec::new(),
                elsewhere: 0,
                outside: 0,
            },
            "without a repository or an association the answer says nothing"
        );
        assert!(lines_of(&choice).is_empty());
    }

    #[test]
    fn a_head_that_is_not_a_commit_id_is_neither_built_nor_counted() {
        for sha in ["main; id", "", "abc", &"z".repeat(40), &"a".repeat(39)] {
            let mut pull = ours(15);
            pull["head"]["sha"] = json!(sha);
            let choice = choose(&[pull], REPO, 5);
            assert_eq!(
                choice,
                Choice::Chosen {
                    chosen: Vec::new(),
                    elsewhere: 0,
                    outside: 0,
                },
                "{sha}"
            );
        }
    }

    #[test]
    fn a_number_that_is_not_one_builds_nothing() {
        for number in [
            json!(0),
            json!(-3),
            json!(1_000_000_000),
            json!("12"),
            json!(null),
        ] {
            let mut pull = ours(12);
            pull["number"] = number.clone();
            assert_eq!(
                choose(&[pull], REPO, 5),
                Choice::Chosen {
                    chosen: Vec::new(),
                    elsewhere: 0,
                    outside: 0,
                },
                "{number}"
            );
        }
    }

    #[test]
    fn a_head_that_does_not_say_where_it_is_is_no_answer() {
        for head in [json!(null), json!("main"), json!({}), json!({"repo": {}})] {
            let mut pull = ours(12);
            pull["head"] = head.clone();
            assert_eq!(
                choose(&[pull], REPO, 5),
                Choice::Chosen {
                    chosen: Vec::new(),
                    elsewhere: 0,
                    outside: 0,
                },
                "{head}"
            );
        }
    }

    #[test]
    fn a_head_repository_that_is_not_text_is_no_answer() {
        let mut pull = ours(12);
        pull["head"]["repo"]["full_name"] = json!(12);
        assert_eq!(
            choose(&[pull], REPO, 5),
            Choice::Chosen {
                chosen: Vec::new(),
                elsewhere: 0,
                outside: 0,
            }
        );
    }

    #[test]
    fn the_repository_is_compared_without_regard_to_capitals() {
        // The label and the setting are spelled as they were that day; GitHub
        // takes a repository's name in any capitals.
        let pull = ours(12);
        assert!(matches!(
            lines_of(&choose(std::slice::from_ref(&pull), "ACME/WIDGET", 5))[..],
            [_, ..]
        ));
        assert!(lines_of(&choose(&[pull], "acme/widget", 5))[0].starts_with("build 12 "));
    }

    #[test]
    fn one_pull_request_on_two_pages_counts_once() {
        let twice = ours(12);
        let pulls = vec![twice.clone(), twice.clone(), ours(11)];
        let choice = choose(&pulls, REPO, 2);
        let Choice::Chosen { chosen, .. } = &choice else {
            panic!("a clear list");
        };
        // Counted twice it would take the second preview from #11.
        assert!(matches!(chosen[0], Verdict::Build { number: 12, .. }));
        assert!(matches!(chosen[1], Verdict::Build { number: 11, .. }));
    }

    #[test]
    fn the_time_goes_with_the_build_and_the_rest_says_nothing() {
        let mut pull = ours(12);
        pull["updated_at"] = json!(null);
        assert_eq!(
            lines_of(&choose(&[pull], REPO, 5)),
            [format!("build 12 {:040x} ", 12u64)],
            "an answer with no time is a description that changed"
        );
    }

    #[test]
    fn pages_are_read_whole() {
        let page = |first: u64, count: u64| -> String {
            let rows: Vec<Value> = (first..first + count).map(ours).collect();
            serde_json::to_string(&rows).unwrap()
        };
        // Two arrays, as the shell appends the pages it reads.
        let given = format!("{}\n{}\n", page(101, 2), page(11, 1));
        let pulls = read_pages(&given).unwrap();
        assert_eq!(pulls.len(), 3);
        let choice = choose(&pulls, REPO, 5);
        let Choice::Chosen { chosen, .. } = &choice else {
            panic!("a clear list");
        };
        assert_eq!(chosen.len(), 3);
        // A page is an array, never a bare pull request, which would be read
        // as "a row that says nothing" instead of a page that is not a page.
        assert!(read_pages(&ours(21).to_string()).is_err());
        // A page of none is a page that was read, and it builds nothing. No
        // bytes at all is no answer, which reads as a list that was not read.
        assert!(read_pages("[]").unwrap().is_empty());
        assert!(read_pages("[]\n[]\n").unwrap().is_empty());
        assert!(read_pages("  \n").is_err());
    }

    #[test]
    fn a_list_that_is_not_json_reads_nothing() {
        assert!(read_pages("not json at all").is_err());
        assert!(read_pages("[1,2\n").is_err());
        assert!(read_pages(&format!("[{}]\ngarbage", ours(12))).is_err());
    }

    #[test]
    fn max_of_zero_leaves_everything_over_limit() {
        let choice = choose(&[ours(12)], REPO, 0);
        assert!(matches!(
            choice,
            Choice::Chosen { ref chosen, .. } if matches!(chosen[0], Verdict::OverLimit { number: 12, .. })
        ));
    }

    #[test]
    fn a_commit_id_is_lowercased_for_the_state_of_the_run() {
        let mut pull = ours(12);
        pull["head"]["sha"] = json!("ABCDEF0123456789ABCDEF0123456789ABCDEF01");
        assert_eq!(
            lines_of(&choose(&[pull], REPO, 5)),
            ["build 12 abcdef0123456789abcdef0123456789abcdef01 2026-10-11T09:00:00Z"]
        );
        assert_eq!(sha40(0).len(), 40);
    }

    #[test]
    fn a_usage_without_a_repository_is_64() {
        assert_eq!(choose_main(&[]), 2);
    }
}
