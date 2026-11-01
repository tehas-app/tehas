//! `tehas-cycle report <issue>` against a stub store, gh and docker: the order
//! of its writes, its tries, and each try's row in the journal. The cases of
//! the report's words are the shell tests', run a second time with it on.

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const STORE: &str = r#"#!/usr/bin/env bash
d=$(dirname "$0")
printf 'store %s\n' "$*" >>"$d/../calls.log"
if [ -f "$d/../store-fail" ] && [[ "$*" == $(cat "$d/../store-fail")* ]]; then
  echo "store: refused #7: not now" >&2
  exit 1
fi
case $1 in
  poll-begin) echo '{"api":1,"verb":"poll-begin","ok":true,"at":100}' ;;
  journal-record) n=$(($(cat "$d/../n" 2>/dev/null || echo 0) + 1)); echo "$n" >"$d/../n"
    echo "{\"api\":1,\"verb\":\"journal-record\",\"ok\":true,\"action\":$n}" ;;
  cycle-show) echo '{"api":1,"verb":"cycle-show","ok":true,"cycle":null}' ;;
  approval-current) echo '{"api":1,"verb":"approval-current","ok":true,"current":null}' ;;
  history-show) echo '{"api":1,"verb":"history-show","ok":true,"rows":[]}' ;;
  *) echo "{\"api\":1,\"verb\":\"$1\",\"ok\":true}" ;;
esac
"#;

const GH: &str = r#"#!/usr/bin/env bash
d=$(dirname "$0")
body=""
case " $* " in *" --body-file - "* | *" --input - "*) body=$(cat) ;; esac
printf 'gh %s%s\n' "$*" "${body:+ <<< $body}" >>"$d/../calls.log"
if [ -f "$d/../gh-hangs" ]; then exec sleep 30; fi
if [ -f "$d/../gh-fail" ]; then
  while IFS= read -r p; do case "$*" in *"$p"*) echo "gh: HTTP 502" >&2; exit 1 ;; esac; done <"$d/../gh-fail"
fi
case "$1 $2" in
  "pr list") cat "$d/../prs.json" 2>/dev/null || echo '[]' ;;
  "api repos/a/b/issues/7/comments") echo 4002 ;;
  "api repos/a/b/issues/7") cat "$d/../issue.json" 2>/dev/null || echo '{"number":7,"body":"Words."}' ;;
esac
"#;

struct Host {
    dir: PathBuf,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn exe(p: &Path, text: &str) {
    fs::write(p, text).unwrap();
    fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
}

impl Host {
    fn new(name: &str) -> Host {
        let dir = std::env::temp_dir().join(format!("tehas-apply-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for d in ["libexec", "bin", "state/cycles", "logs", "work"] {
            fs::create_dir_all(dir.join(d)).unwrap();
        }
        exe(&dir.join("libexec/store"), STORE);
        exe(&dir.join("bin/gh"), GH);
        exe(&dir.join("bin/docker"), "#!/bin/sh\nexit 1\n");
        exe(&dir.join("bin/systemctl"), "#!/bin/sh\nexit 0\n");
        Host { dir }
    }

    /// The cycle's end as the runner left it.
    fn result(&self, status: &str, step: &str, branch: &str, cap: &str) {
        let r = json!({"schemaVersion": 5, "issues": [{"issue": 7, "status": status,
            "failedStep": step, "branch": branch, "cap": cap}]});
        fs::write(self.dir.join("state/cycles/7.result.json"), r.to_string()).unwrap();
        fs::write(self.dir.join("state/cycles/7.progress.json"), "{}").unwrap();
    }

    fn report(&self, rc: i64, signalled: bool) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tehas-cycle"))
            .args(["report", "7", "--attempt", "1", "--rc", &rc.to_string()])
            .args(["--started", "1000", "--by", "alice", "--how", "label"])
            .args(["--signalled", if signalled { "1" } else { "0" }])
            .args(["--approval-at", "900", "--requeued", "0", "--now", "1060"])
            .args(["--console", "true", "--status-cmd", "true"])
            .env(
                "PATH",
                format!(
                    "{}/bin:{}",
                    self.dir.display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("TEHAS_HOME", &self.dir)
            .env("HOME", &self.dir)
            .env("TEHAS_REPO", "a/b")
            .env("TEHAS_WORK_DIR", self.dir.join("work/widget"))
            .env("TEHAS_CYCLE_RETRY_WAIT", "0")
            .env("TEHAS_CYCLE_STOP_GH_WAIT", "1")
            .env("TEHAS_ANALYTICS", "off")
            .env_remove("TEHAS_IMPROVE_REPO")
            .output()
            .unwrap()
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The calls that start with one of these, in order, by their index.
    fn at(&self, what: &str) -> usize {
        self.calls()
            .iter()
            .position(|c| c.starts_with(what))
            .unwrap_or_else(|| panic!("no call of {what}: {:#?}", self.calls()))
    }

    fn count(&self, what: &str) -> usize {
        self.calls().iter().filter(|c| c.starts_with(what)).count()
    }
}

fn out(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn a_park_goes_label_record_question_asked_label_end() {
    let h = Host::new("park");
    h.result("aborted", "develop", "", "");
    let o = h.report(20, false);
    assert!(o.status.success(), "{}", out(&o));
    let order = [
        "gh issue edit 7 --repo a/b --remove-label tehas:running --add-label needs-human-attention",
        "store park 7 --attempt 1 --reason aborted --step develop",
        "gh api repos/a/b/issues/7/comments --input - --jq .id",
        "store question-asked 7 --id 4002 --kind park --body-sha",
        "gh issue edit 7 --repo a/b --add-label tehas:question",
        "store cycle-end 7 --attempt 1 --status aborted",
        "store publish",
    ];
    let at: Vec<usize> = order.iter().map(|c| h.at(c)).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{:#?}", h.calls());
}

#[test]
fn each_try_of_a_write_is_a_row_of_its_own_named_by_its_cycle() {
    let h = Host::new("tries");
    h.result("aborted", "develop", "", "");
    fs::write(h.dir.join("gh-fail"), "--add-label needs-human-attention\n").unwrap();
    let o = h.report(20, false);
    assert!(o.status.success(), "{}", out(&o));
    assert_eq!(h.count("gh issue edit 7 --repo a/b --remove-label"), 3);
    let rows: Vec<String> = h
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("store journal-record") && c.contains("label-add"))
        .collect();
    assert!(rows.len() >= 3, "{rows:#?}");
    assert!(
        rows[0].contains(r#""cycle":{"attempt":1,"issue":7}"#),
        "{}",
        rows[0]
    );
    assert!(h.count("store journal-settle") >= 3);
    let said = out(&o);
    assert!(said.contains("WARN: label change on #7 failed: --remove-label tehas:running --add-label needs-human-attention"), "{said}");
    // The label stayed, and the question says so.
    let q = h.at("gh api repos/a/b/issues/7/comments");
    assert!(h.calls()[q].contains("The label of this issue could not be changed"));
}

#[test]
fn a_body_goes_to_gh_on_standard_input() {
    let h = Host::new("stdin");
    h.result("aborted", "develop", "", "");
    h.report(20, false);
    let q = h.at("gh api repos/a/b/issues/7/comments");
    assert!(h.calls()[q].contains(r#"<<< {"body":"Tehas cycle stopped"#));
    let record = h.at("store journal-record --poll 100 --kind comment-post");
    assert!(!h.calls()[record].contains("Tehas cycle stopped"));
    assert!(h.calls()[record].contains("sha256"));
}

#[test]
fn a_question_whose_label_fails_is_taken_off_the_record_and_says_so() {
    let h = Host::new("withdraw");
    h.result("aborted", "develop", "", "");
    fs::write(h.dir.join("gh-fail"), "--add-label tehas:question\n").unwrap();
    h.report(20, false);
    assert!(h.at("store question-asked") < h.at("store question-answered 7 --id 4002 --withdraw"));
    let last = h.at("gh issue comment 7");
    assert!(h.calls()[last].contains("The boxes above cannot be used"));
}

#[test]
fn a_park_the_store_will_not_take_asks_no_question() {
    let h = Host::new("unrecorded");
    h.result("aborted", "develop", "", "");
    fs::write(h.dir.join("store-fail"), "park 7").unwrap();
    let o = h.report(20, false);
    assert!(out(&o).contains("WARN: could not record the park of #7 (refused: store: refused #7: not now), so it asks no question"));
    assert_eq!(h.count("gh api repos/a/b/issues/7/comments"), 0);
    let c = h.at("gh issue comment 7");
    assert!(h.calls()[c].contains("To retry, apply `tehas:ready`."));
}

#[test]
fn after_a_stop_each_call_is_one_bounded_try() {
    let h = Host::new("stop");
    h.result("aborted", "develop", "", "");
    fs::write(h.dir.join("gh-hangs"), "").unwrap();
    let t0 = std::time::Instant::now();
    let o = h.report(20, true);
    assert!(o.status.success(), "{}", out(&o));
    assert_eq!(h.count("gh issue edit 7 --repo a/b --remove-label"), 1);
    assert!(t0.elapsed().as_secs() < 25, "{:?}", t0.elapsed());
    // The report still ended the cycle's record.
    assert_eq!(h.count("store cycle-end"), 1);
}

#[test]
fn an_end_the_store_will_not_take_is_exit_70_and_what_follows_still_runs() {
    let h = Host::new("end");
    h.result("aborted", "develop", "", "");
    fs::write(h.dir.join("store-fail"), "cycle-end").unwrap();
    let o = h.report(20, false);
    assert_eq!(o.status.code(), Some(70));
    let said = out(&o);
    assert!(said.contains("ERROR: the end of #7 is not on record (refused: store: refused #7: not now): cycle-end 7 --attempt 1"), "{said}");
    assert_eq!(h.count("store publish"), 1);
    assert!(h.dir.join("state/dispatch.again").exists());
}

#[test]
fn a_write_the_journal_cannot_record_is_made_with_no_row() {
    for verb in ["poll-begin", "journal-record", "journal-attempt"] {
        let h = Host::new(&format!("journal-{verb}"));
        h.result("aborted", "develop", "", "");
        fs::write(h.dir.join("store-fail"), verb).unwrap();
        let o = h.report(20, false);
        assert!(o.status.success(), "{}", out(&o));
        let order = [
            "gh issue edit 7 --repo a/b --remove-label tehas:running --add-label needs-human-attention",
            "store park 7",
            "gh api repos/a/b/issues/7/comments --input - --jq .id",
            "store question-asked 7 --id 4002",
            "gh issue edit 7 --repo a/b --add-label tehas:question",
            "store cycle-end 7",
        ];
        let at: Vec<usize> = order.iter().map(|c| h.at(c)).collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{:#?}", h.calls());
        let said = out(&o);
        assert_eq!(
            said.matches("WARN: the journal will not record").count(),
            1,
            "{said}"
        );
        assert!(!said.contains("WARN: label change"), "{said}");
        // Once it failed, the report asks it no more: a hung store would
        // spend what a stop leaves.
        assert_eq!(h.count(&format!("store {verb}")), 1, "{:#?}", h.calls());
    }
}

#[test]
fn a_write_with_no_row_keeps_its_tries() {
    let h = Host::new("journal-tries");
    h.result("aborted", "develop", "", "");
    fs::write(h.dir.join("store-fail"), "journal-record").unwrap();
    fs::write(h.dir.join("gh-fail"), "--add-label needs-human-attention\n").unwrap();
    let o = h.report(20, false);
    assert!(o.status.success());
    assert_eq!(h.count("gh issue edit 7 --repo a/b --remove-label"), 3);
    assert!(out(&o).contains("WARN: label change on #7 failed"));
    let q = h.at("gh api repos/a/b/issues/7/comments");
    assert!(h.calls()[q].contains("The label of this issue could not be changed"));
}

#[test]
fn after_a_stop_a_write_with_no_row_is_one_try() {
    let h = Host::new("journal-stop");
    h.result("aborted", "develop", "", "");
    fs::write(h.dir.join("store-fail"), "poll-begin").unwrap();
    fs::write(h.dir.join("gh-fail"), "--add-label needs-human-attention\n").unwrap();
    let o = h.report(20, true);
    assert!(o.status.success());
    assert_eq!(h.count("gh issue edit 7 --repo a/b --remove-label"), 1);
}

#[test]
fn a_merged_cycle_removes_its_workspace_and_its_notes() {
    let h = Host::new("merged");
    h.result("merged", "done", "feature/issue-7-x", "");
    let work = h.dir.join("work");
    fs::write(work.join(".tehas-workspaces"), "").unwrap();
    fs::create_dir_all(work.join("widget-7")).unwrap();
    fs::create_dir_all(work.join("widget-7.images")).unwrap();
    fs::write(h.dir.join("state/cycles/7.oom"), "2\n").unwrap();
    fs::write(h.dir.join("state/cycles/7.done"), "merged\n").unwrap();
    let prs: Value = json!([{"url": "https://github.com/a/b/pull/9", "state": "MERGED",
        "body": "Fixes #7", "headRefName": "feature/issue-7-x"}]);
    fs::write(h.dir.join("prs.json"), prs.to_string()).unwrap();
    let o = h.report(0, false);
    assert!(o.status.success(), "{}", out(&o));
    assert!(!work.join("widget-7").exists());
    assert!(!work.join("widget-7.images").exists());
    assert!(!h.dir.join("state/cycles/7.oom").exists());
    assert!(!h.dir.join("state/cycles/7.done").exists());
    assert!(h.dir.join("state/cycles/7.result.json").exists());
    assert!(
        h.at("gh issue comment 7")
            > h.at("gh issue edit 7 --repo a/b --remove-label tehas:running")
    );
    let end = h.at("store cycle-end");
    assert!(h.calls()[end].contains("--oom 2"), "{}", h.calls()[end]);
    assert!(h.calls()[end].contains("--pr 9"));
}

#[test]
fn a_report_not_named_its_programs_is_refused() {
    let h = Host::new("no-programs");
    let o = Command::new(env!("CARGO_BIN_EXE_tehas-cycle"))
        .args(["report", "7", "--attempt", "1", "--console", "true"])
        .env("TEHAS_HOME", &h.dir)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(64), "{}", out(&o));
    assert_eq!(h.calls().len(), 0);
}
