//! The release gate on the speed lane: `.github/workflows/release-gate.yml`,
//! run on a `[self-hosted, speedlane]` runner. A run gates the commit its
//! input names, and its title says which (`release-gate <commit>`): the run's
//! own head is the branch it was started on, which can be another commit.
//!
//! tag-release trusts a green run on the exact commit it tags. With none, it
//! starts one with `gh workflow run` and waits for it, up to a bound.

use serde_json::Value;

pub const WORKFLOW: &str = "release-gate.yml";

/// One run of the workflow, as `gh run list --json` gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub id: u64,
    pub title: String,
    pub status: String,
    pub conclusion: String,
    pub url: String,
}

/// The title of a run that gates `commit`.
pub fn title(commit: &str) -> String {
    format!("release-gate {commit}")
}

/// The fields `parse` reads.
pub const FIELDS: &str = "databaseId,displayTitle,status,conclusion,url";

/// The runs of `gh run list --json FIELDS`. None when the text is not a list
/// of them.
pub fn parse(json: &str) -> Option<Vec<Run>> {
    let v: Value = serde_json::from_str(json).ok()?;
    let text = |r: &Value, k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    v.as_array()?
        .iter()
        .map(|r| {
            Some(Run {
                id: r.get("databaseId")?.as_u64()?,
                title: text(r, "displayTitle"),
                status: text(r, "status"),
                conclusion: text(r, "conclusion"),
                url: text(r, "url"),
            })
        })
        .collect()
}

/// What the runs say of one commit.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    /// A run passed: its URL.
    Green(String),
    /// A run has not ended: its URL.
    Running(String),
    /// Every run that ended failed: the newest one's URL.
    Red(String),
    /// No run gates the commit.
    Missing,
}

/// The verdict of the runs that gate `commit` and are newer than `after`.
pub fn judge(runs: &[Run], commit: &str, after: u64) -> Verdict {
    let want = title(commit);
    let mut mine: Vec<&Run> = runs
        .iter()
        .filter(|r| r.title == want && r.id > after)
        .collect();
    mine.sort_by_key(|r| std::cmp::Reverse(r.id));
    if let Some(r) = mine
        .iter()
        .find(|r| r.status == "completed" && r.conclusion == "success")
    {
        return Verdict::Green(r.url.clone());
    }
    if let Some(r) = mine.iter().find(|r| r.status != "completed") {
        return Verdict::Running(r.url.clone());
    }
    match mine.first() {
        Some(r) => Verdict::Red(r.url.clone()),
        None => Verdict::Missing,
    }
}

/// How the lane ended for the commit.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// A green run: its URL.
    Green(String),
    /// The run it started or waited on failed: its URL.
    Red(String),
    /// The bound passed first; the URL of a run seen, if any.
    TimedOut(Option<String>),
    /// gh could not list the runs.
    NoList,
    /// gh could not start a run.
    NoStart,
}

/// The lane for `commit`. `list` gives the workflow's runs, `start` starts
/// one, `wait` waits between looks and says false once the bound has passed.
/// A green run already there is taken at once; one that has not ended is
/// waited for; with neither (none, or only red ones) a run is started and
/// waited for. A run it waited for that fails is the answer: it starts no
/// second one.
pub fn gate(
    commit: &str,
    mut list: impl FnMut() -> Option<Vec<Run>>,
    start: impl FnOnce() -> bool,
    mut wait: impl FnMut() -> bool,
    mut say: impl FnMut(&str),
) -> Outcome {
    let Some(runs) = list() else {
        return Outcome::NoList;
    };
    let mut after = 0;
    let mut seen = None;
    match judge(&runs, commit, 0) {
        Verdict::Green(url) => return Outcome::Green(url),
        Verdict::Running(url) => {
            say(&format!("waiting for the release gate: {url}"));
            seen = Some(url);
        }
        Verdict::Red(_) | Verdict::Missing => {
            // Only runs started after this one count from here.
            after = runs.iter().map(|r| r.id).max().unwrap_or(0);
            if !start() {
                return Outcome::NoStart;
            }
            say("started the release gate on the speedlane runner");
        }
    }
    loop {
        if !wait() {
            return Outcome::TimedOut(seen);
        }
        let Some(runs) = list() else {
            return Outcome::NoList;
        };
        match judge(&runs, commit, after) {
            Verdict::Green(url) => return Outcome::Green(url),
            Verdict::Red(url) => return Outcome::Red(url),
            Verdict::Running(url) => {
                if seen.as_deref() != Some(url.as_str()) {
                    say(&format!("waiting for the release gate: {url}"));
                    seen = Some(url);
                }
            }
            Verdict::Missing => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: &str = "1111111111111111111111111111111111111111";
    const OTHER: &str = "2222222222222222222222222222222222222222";

    fn run(id: u64, commit: &str, status: &str, conclusion: &str) -> Run {
        Run {
            id,
            title: title(commit),
            status: status.into(),
            conclusion: conclusion.into(),
            url: format!("u{id}"),
        }
    }

    #[test]
    fn runs_are_read_from_gh_json() {
        let json = format!(
            r#"[{{"databaseId":7,"displayTitle":"release-gate {C}","status":"in_progress","conclusion":"","url":"https://x/7"}}]"#
        );
        assert_eq!(
            parse(&json),
            Some(vec![Run {
                id: 7,
                title: title(C),
                status: "in_progress".into(),
                conclusion: String::new(),
                url: "https://x/7".into(),
            }])
        );
        assert_eq!(parse("[]"), Some(vec![]));
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(r#"[{"displayTitle":"x"}]"#), None);
    }

    #[test]
    fn only_runs_on_the_exact_commit_count() {
        let green_other = [run(5, OTHER, "completed", "success")];
        assert_eq!(judge(&green_other, C, 0), Verdict::Missing);
        // A title that only starts with the commit is another one.
        let mut prefix = run(6, C, "completed", "success");
        prefix.title = format!("release-gate {}", &C[..12]);
        assert_eq!(judge(&[prefix], C, 0), Verdict::Missing);
        let runs = [
            run(3, C, "completed", "failure"),
            run(4, C, "completed", "success"),
            run(5, OTHER, "in_progress", ""),
        ];
        assert_eq!(judge(&runs, C, 0), Verdict::Green("u4".into()));
        assert_eq!(judge(&runs, C, 4), Verdict::Missing);
        let runs = [run(3, C, "completed", "failure"), run(4, C, "queued", "")];
        assert_eq!(judge(&runs, C, 0), Verdict::Running("u4".into()));
        let runs = [
            run(3, C, "completed", "failure"),
            run(4, C, "completed", "cancelled"),
        ];
        assert_eq!(judge(&runs, C, 0), Verdict::Red("u4".into()));
    }

    /// The lane over a list of answers to `list`, one per look; `wait` says
    /// false after `polls` waits. The calls: "list", "start", "wait".
    fn lane(answers: Vec<Option<Vec<Run>>>, starts: bool, polls: usize) -> (Outcome, Vec<String>) {
        let calls = std::cell::RefCell::new(Vec::new());
        let mut answers = answers.into_iter();
        let mut waited = 0;
        let out = gate(
            C,
            || {
                calls.borrow_mut().push("list".to_string());
                answers.next().unwrap_or(Some(vec![]))
            },
            || {
                calls.borrow_mut().push("start".to_string());
                starts
            },
            || {
                calls.borrow_mut().push("wait".to_string());
                waited += 1;
                waited <= polls
            },
            |_| {},
        );
        (out, calls.into_inner())
    }

    #[test]
    fn a_green_run_on_the_commit_is_taken_without_a_start() {
        let (out, calls) = lane(vec![Some(vec![run(4, C, "completed", "success")])], true, 9);
        assert_eq!(out, Outcome::Green("u4".into()));
        assert_eq!(calls, ["list"]);
    }

    #[test]
    fn a_missing_or_red_run_starts_one_and_waits_for_it() {
        let (out, calls) = lane(
            vec![
                Some(vec![
                    run(4, C, "completed", "failure"),
                    run(5, OTHER, "completed", "success"),
                ]),
                Some(vec![run(4, C, "completed", "failure")]),
                Some(vec![
                    run(4, C, "completed", "failure"),
                    run(6, C, "in_progress", ""),
                ]),
                Some(vec![run(6, C, "completed", "success")]),
            ],
            true,
            9,
        );
        assert_eq!(out, Outcome::Green("u6".into()));
        assert_eq!(
            calls,
            ["list", "start", "wait", "list", "wait", "list", "wait", "list"]
        );

        let (out, _) = lane(
            vec![Some(vec![]), Some(vec![run(6, C, "completed", "failure")])],
            true,
            9,
        );
        assert_eq!(out, Outcome::Red("u6".into()));
    }

    #[test]
    fn a_running_one_is_waited_for_and_its_failure_is_the_answer() {
        let (out, calls) = lane(
            vec![
                Some(vec![run(4, C, "queued", "")]),
                Some(vec![run(4, C, "completed", "failure")]),
            ],
            true,
            9,
        );
        assert_eq!(out, Outcome::Red("u4".into()));
        assert!(!calls.contains(&"start".to_string()));
    }

    const GATE: &str = include_str!("../../../../.github/workflows/release-gate.yml");
    const VERIFY: &str = include_str!("../../../../.github/workflows/verify.yml");
    const RELEASE: &str = include_str!("../../../../.github/workflows/release.yml");

    #[test]
    fn the_release_gate_workflow_is_the_one_this_reads() {
        assert!(GATE.contains(&format!("\nrun-name: {}\n", title("${{ inputs.commit }}"))));
        assert!(GATE.contains("\n  workflow_dispatch:\n"));
        assert!(GATE.contains("runs-on: [self-hosted, speedlane]\n"));
        assert!(!GATE.contains("tehas]"));
        for step in [
            "run: scripts/verify.sh\n",
            "TEHAS_TEST_JOBS=",
            "TEHAS_TEST_MEMORY_MAX=",
            "host/sandbox/build.sh\n",
            "vipune search",
        ] {
            assert!(GATE.contains(step), "{step}");
        }
        // The tag is still pushed by tag-release: the gate pushes nothing.
        assert!(GATE.contains("permissions:\n  contents: read\n"));
    }

    #[test]
    fn express_batches_go_to_the_lane_and_the_rest_stay() {
        assert!(VERIFY.contains(
            "runs-on: ${{ startsWith(github.event.pull_request.title, 'Express Lane') && fromJSON('[\"self-hosted\", \"speedlane\"]') || fromJSON('[\"self-hosted\", \"tehas\"]') }}\n"
        ));
        assert!(RELEASE.contains("runs-on: [self-hosted, tehas]\n"));
    }

    /// The jobs of a workflow: each one's name and text.
    fn jobs(text: &str) -> Vec<(&str, String)> {
        let body = text.split_once("\njobs:\n").map_or("", |(_, b)| b);
        let mut out: Vec<(&str, String)> = Vec::new();
        for line in body.lines() {
            match line.strip_prefix("  ").and_then(|l| l.strip_suffix(':')) {
                Some(name) if !name.starts_with(' ') => out.push((name, String::new())),
                _ => {
                    if let Some((_, job)) = out.last_mut() {
                        job.push_str(line);
                        job.push('\n');
                    }
                }
            }
        }
        out
    }

    #[test]
    fn every_ci_run_of_the_gate_runs_every_step() {
        for (name, text) in [("release-gate", GATE), ("verify", VERIFY)] {
            for (job, body) in jobs(text) {
                if body.contains("run: scripts/verify.sh") {
                    assert!(
                        body.contains("\n      TEHAS_VERIFY_FRESH: 1\n"),
                        "{name}/{job}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_names_list_is_only_on_a_hosted_runner_that_runs_no_pull_request_code() {
        let mut seen = 0;
        for (name, text) in [
            ("release-gate", GATE),
            ("verify", VERIFY),
            ("release", RELEASE),
        ] {
            for (job, body) in jobs(text) {
                if !body.contains("secrets.GENERIC_NAMES") {
                    continue;
                }
                seen += 1;
                assert!(body.contains("    runs-on: ubuntu-"), "{name}/{job}");
                assert!(!body.contains("scripts/verify.sh"), "{name}/{job}");
                assert!(!body.contains("self-hosted"), "{name}/{job}");
            }
        }
        assert_eq!(seen, 2);
        let names = jobs(VERIFY)
            .into_iter()
            .find(|(_, b)| b.contains("secrets.GENERIC_NAMES"))
            .map(|(_, b)| b)
            .unwrap_or_default();
        // A pull request's own lint does not run with the list: its base's does.
        let base = names
            .find("git show \"origin/$GITHUB_BASE_REF:scripts/check-generic.sh\"")
            .expect("the base branch's lint");
        let secret = names.find("secrets.GENERIC_NAMES").unwrap_or(0);
        assert!(base < secret);
    }

    #[test]
    fn every_action_is_pinned_to_a_commit() {
        for (name, text) in [
            ("release-gate", GATE),
            ("verify", VERIFY),
            ("release", RELEASE),
        ] {
            for line in text
                .lines()
                .filter(|l| l.trim_start().starts_with("- uses:"))
            {
                let at = line.split('@').nth(1).unwrap_or_default();
                let (sha, tag) = at.split_once(" # ").unwrap_or((at, ""));
                assert!(
                    sha.len() == 40
                        && sha.bytes().all(|b| b.is_ascii_hexdigit())
                        && tag.starts_with('v'),
                    "{name}: {line}"
                );
            }
        }
    }

    #[test]
    fn the_bound_and_gh_failures_end_it() {
        let (out, calls) = lane(vec![Some(vec![])], true, 2);
        assert_eq!(out, Outcome::TimedOut(None));
        assert_eq!(
            calls,
            ["list", "start", "wait", "list", "wait", "list", "wait"]
        );
        let (out, _) = lane(vec![Some(vec![run(4, C, "queued", "")])], true, 0);
        assert_eq!(out, Outcome::TimedOut(Some("u4".into())));
        assert_eq!(lane(vec![None], true, 9).0, Outcome::NoList);
        assert_eq!(lane(vec![Some(vec![])], false, 9).0, Outcome::NoStart);
    }
}
