//! A call of the state store in tehas's terms: the verb and what it is given,
//! typed, so no caller writes a verb's name or its flags. [`Verb::args`] is
//! the one place a call becomes the words the store parses; the store's own
//! tests (console/store/src/verb_tests.rs) hold every variant to its grammar.

use crate::order::Order;
use crate::plugin::run::{Kind, Trigger};
use crate::question::Choice;

/// How `cycle-end` ends a cycle's record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum End {
    /// `--stopped`: `tehas stop` took the cycle down.
    Stopped,
    /// `--status <word>`: the cycle's own report.
    Status(String),
    /// `--refused --rc <rc>`: a start that was refused.
    Refused(i32),
}

/// What `plan-show` reads: the open sessions, or one issue's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plans {
    Open,
    Issue(u64),
}

/// The account of a cycle's report, as [`Verb::CycleEndByReport`] holds it:
/// the words of the run's end. The paths of the result and the progress are
/// given when their files stand, as the cycle's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndByReport {
    pub issue: u64,
    pub attempt: String,
    pub status: String,
    pub step: String,
    pub rc: String,
    pub oom: String,
    pub forks: String,
    pub runner_exit: Option<String>,
    pub host_saw: bool,
    pub stalled: Option<String>,
    pub timed: bool,
    pub branch: Option<String>,
    pub pr: Option<String>,
    pub result: Option<String>,
    pub progress: Option<String>,
    pub cap: Option<String>,
    pub park_reason: Option<String>,
    pub failure_kind: Option<String>,
    pub failure_cause: Option<String>,
    pub provider: bool,
    pub parked: bool,
    pub never_ran: bool,
    pub signalled: bool,
}

/// One call of the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verb {
    /// `init` with the four settings of the installation.
    Init {
        version: String,
        commit: String,
        instance: String,
        repo: String,
    },
    Check,
    Copy {
        to: String,
    },
    Load {
        from: String,
    },
    BackupShow,
    Upkeep {
        dry_run: bool,
    },
    /// `publish`, of the views or, with a file, of the health report.
    Publish {
        health: Option<String>,
    },
    CycleStartByHand {
        issue: u64,
    },
    /// `cycle-start`, with the pi-rukas commit of the image when it has one.
    CycleStart {
        issue: u64,
        rukas: Option<String>,
        by_hand: bool,
    },
    CycleEnd {
        issue: u64,
        attempt: String,
        end: End,
    },
    /// `cycle-end` as the cycle's report ends it: the run's whole account.
    /// The account is wide, so it stands boxed, keeping the enum the size of
    /// its other calls.
    CycleEndByReport(Box<EndByReport>),
    /// `cycle-image`: the image and its settings on the record of the start.
    CycleImage {
        issue: u64,
        attempt: String,
        image: String,
        rukas: Option<String>,
        settings: Option<String>,
    },
    /// `cycle-heartbeat`: the step and status of a running cycle, or its end
    /// without a report. `progress` is its steps' numbers as `--flag <digits>`
    /// pairs, in the order they were read.
    CycleHeartbeat {
        issue: u64,
        attempt: Option<String>,
        running: bool,
        status: String,
        step: Option<String>,
        text: String,
        progress: Vec<(String, String)>,
    },
    CycleShow {
        issue: u64,
    },
    CyclesRunning,
    LocksHeld,
    ParkShow {
        issue: u64,
    },
    ParkRetry {
        issue: u64,
    },
    /// `park`: the cycle's park on record, its reason by tehas's word.
    Park {
        issue: u64,
        attempt: String,
        reason: String,
        step: Option<String>,
        pr: Option<String>,
    },
    /// `question-asked`: a question put on the record before it is asked.
    QuestionAsked {
        issue: u64,
        id: String,
        kind: String,
        body_sha: String,
        offers: String,
    },
    /// `question-answered --withdraw`: the question taken off the record.
    QuestionWithdrawn {
        issue: u64,
        id: String,
    },
    QuestionShow {
        issue: u64,
    },
    QuestionList {
        after: Option<u64>,
    },
    AnswerRecord {
        issue: u64,
        by: String,
        choice: Choice,
        option: Option<String>,
        text_file: Option<String>,
    },
    ApprovalRecord {
        issue: u64,
        at: String,
        by: String,
        how: String,
        body_sha: String,
        title_sha: Option<String>,
    },
    ApprovalUndo {
        issue: u64,
        at: String,
    },
    ApprovalCurrent {
        issue: u64,
    },
    /// `approval-view`: the approval of an issue, or of one attempt.
    ApprovalView {
        issue: u64,
        attempt: Option<String>,
    },
    /// `settings-view`: the settings a cycle ran with.
    SettingsView {
        issue: u64,
        attempt: String,
    },
    ApprovalHosts,
    /// `records --issue`: the plugin records of an issue.
    Records {
        issue: u64,
    },
    PlanShow(Plans),
    HistoryShow {
        issue: Option<u64>,
    },
    /// `poll-show`, with `--api --now <now>` for the answer of `tehas api`.
    PollShow {
        api_now: Option<i64>,
    },
    PollIdle {
        since: u64,
        budget: String,
    },
    OrderShow,
    OrderSet(Order),
    OrderClear,
    PluginRunBegin {
        plugin: String,
        commit: String,
        kind: Kind,
        how: Trigger,
        by: String,
        head: String,
    },
    PluginRunEnd {
        run: u64,
        result: String,
        size: Option<usize>,
        tokens: Option<u64>,
    },
    /// `plugin-runs --plugin <name>`, with `--no-calls` to leave the calls out.
    PluginRuns {
        plugin: String,
        no_calls: bool,
    },
}

impl Verb {
    /// The verb's name, which its answer names too.
    pub fn name(&self) -> &'static str {
        match self {
            Verb::Init { .. } => "init",
            Verb::Check => "check",
            Verb::Copy { .. } => "copy",
            Verb::Load { .. } => "load",
            Verb::BackupShow => "backup",
            Verb::Upkeep { .. } => "upkeep",
            Verb::Publish { .. } => "publish",
            Verb::CycleStartByHand { .. } | Verb::CycleStart { .. } => "cycle-start",
            Verb::CycleEnd { .. } | Verb::CycleEndByReport(_) => "cycle-end",
            Verb::CycleImage { .. } => "cycle-image",
            Verb::CycleHeartbeat { .. } => "cycle-heartbeat",
            Verb::CycleShow { .. } | Verb::CyclesRunning => "cycle-show",
            Verb::LocksHeld => "locks-held",
            Verb::ParkShow { .. } => "park-show",
            Verb::ParkRetry { .. } => "park-retry",
            Verb::Park { .. } => "park",
            Verb::QuestionAsked { .. } => "question-asked",
            Verb::QuestionWithdrawn { .. } => "question-answered",
            Verb::QuestionShow { .. } => "question-show",
            Verb::QuestionList { .. } => "question-list",
            Verb::AnswerRecord { .. } => "answer-record",
            Verb::ApprovalRecord { .. } | Verb::ApprovalUndo { .. } => "approval-record",
            Verb::ApprovalCurrent { .. } => "approval-current",
            Verb::ApprovalView { .. } => "approval-view",
            Verb::SettingsView { .. } => "settings-view",
            Verb::ApprovalHosts => "approval-hosts",
            Verb::Records { .. } => "records",
            Verb::PlanShow(_) => "plan-show",
            Verb::HistoryShow { .. } => "history-show",
            Verb::PollShow { .. } => "poll-show",
            Verb::PollIdle { .. } => "poll-idle",
            Verb::OrderShow => "order-show",
            Verb::OrderSet(_) => "order-set",
            Verb::OrderClear => "order-clear",
            Verb::PluginRunBegin { .. } => "plugin-run-begin",
            Verb::PluginRunEnd { .. } => "plugin-run-end",
            Verb::PluginRuns { .. } => "plugin-runs",
        }
    }

    /// The words of the call, the verb's name first, as the store parses them.
    pub fn args(&self) -> Vec<String> {
        let mut a = Words(vec![self.name().to_string()]);
        match self {
            Verb::Init {
                version,
                commit,
                instance,
                repo,
            } => {
                a.flag("--version", version);
                a.flag("--commit", commit);
                a.flag("--instance", instance);
                a.flag("--repo", repo);
            }
            Verb::Check
            | Verb::LocksHeld
            | Verb::ApprovalHosts
            | Verb::OrderShow
            | Verb::OrderClear => {}
            Verb::Copy { to } => a.flag("--to", to),
            Verb::Load { from } => a.flag("--from", from),
            Verb::BackupShow => a.switch("--show", true),
            Verb::Upkeep { dry_run } => a.switch("--dry-run", *dry_run),
            Verb::Publish { health } => a.maybe("--health", health.as_ref()),
            Verb::CycleStartByHand { issue } => {
                a.issue(*issue);
                a.switch("--by-hand", true);
            }
            Verb::CycleEnd {
                issue,
                attempt,
                end,
            } => {
                a.issue(*issue);
                a.flag("--attempt", attempt);
                match end {
                    End::Stopped => a.switch("--stopped", true),
                    End::Status(s) => a.flag("--status", s),
                    End::Refused(rc) => {
                        a.switch("--refused", true);
                        a.flag("--rc", rc);
                    }
                }
            }
            Verb::CycleStart {
                issue,
                rukas,
                by_hand,
            } => {
                a.issue(*issue);
                a.maybe("--rukas", rukas.as_ref());
                a.switch("--by-hand", *by_hand);
            }
            Verb::CycleEndByReport(b) => {
                let EndByReport {
                    issue,
                    attempt,
                    status,
                    step,
                    rc,
                    oom,
                    forks,
                    runner_exit,
                    host_saw,
                    stalled,
                    timed,
                    branch,
                    pr,
                    result,
                    progress,
                    cap,
                    park_reason,
                    failure_kind,
                    failure_cause,
                    provider,
                    parked,
                    never_ran,
                    signalled,
                } = &**b;
                a.issue(*issue);
                a.flag("--attempt", attempt);
                a.flag("--status", status);
                a.flag("--step", step);
                a.flag("--rc", rc);
                a.flag("--oom", oom);
                a.flag("--forks", forks);
                a.maybe("--runner-exit", runner_exit.as_ref());
                if *host_saw {
                    a.flag("--host-saw", "killed");
                }
                a.maybe("--stalled", stalled.as_ref());
                a.switch("--timed", *timed);
                a.maybe("--branch", branch.as_ref());
                a.maybe("--pr", pr.as_ref());
                a.maybe("--result", result.as_ref());
                a.maybe("--progress", progress.as_ref());
                a.maybe("--cap", cap.as_ref());
                a.maybe("--park-reason", park_reason.as_ref());
                a.maybe("--failure-kind", failure_kind.as_ref());
                a.maybe("--failure-cause", failure_cause.as_ref());
                a.switch("--provider", *provider);
                a.switch("--parked", *parked);
                a.switch("--never-ran", *never_ran);
                a.switch("--signalled", *signalled);
            }
            Verb::CycleImage {
                issue,
                attempt,
                image,
                rukas,
                settings,
            } => {
                a.issue(*issue);
                a.flag("--attempt", attempt);
                a.flag("--image", image);
                a.maybe("--rukas", rukas.as_ref());
                a.maybe("--settings", settings.as_ref());
            }
            Verb::CycleHeartbeat {
                issue,
                attempt,
                running,
                status,
                step,
                text,
                progress,
            } => {
                a.issue(*issue);
                a.maybe("--attempt", attempt.as_ref());
                a.switch(if *running { "--running" } else { "--ended" }, true);
                a.flag("--status", status);
                a.maybe("--step", step.as_ref());
                a.flag("--text", text);
                for (flag, value) in progress {
                    a.flag(flag, value);
                }
            }
            Verb::CycleShow { issue }
            | Verb::ParkShow { issue }
            | Verb::ParkRetry { issue }
            | Verb::QuestionShow { issue }
            | Verb::ApprovalCurrent { issue } => a.issue(*issue),
            Verb::Park {
                issue,
                attempt,
                reason,
                step,
                pr,
            } => {
                a.issue(*issue);
                a.flag("--attempt", attempt);
                a.flag("--reason", reason);
                a.maybe("--step", step.as_ref());
                a.maybe("--pr", pr.as_ref());
            }
            Verb::QuestionAsked {
                issue,
                id,
                kind,
                body_sha,
                offers,
            } => {
                a.issue(*issue);
                a.flag("--id", id);
                a.flag("--kind", kind);
                a.flag("--body-sha", body_sha);
                a.flag("--offers", offers);
            }
            Verb::ApprovalView { issue, attempt } => {
                a.issue(*issue);
                a.maybe("--attempt", attempt.as_ref());
            }
            Verb::SettingsView { issue, attempt } => {
                a.issue(*issue);
                a.flag("--attempt", attempt);
            }
            Verb::Records { issue } => a.flag("--issue", issue),
            Verb::CyclesRunning => a.switch("--running", true),
            Verb::QuestionWithdrawn { issue, id } => {
                a.issue(*issue);
                a.flag("--id", id);
                a.switch("--withdraw", true);
            }
            Verb::QuestionList { after } => a.maybe("--after", after.as_ref()),
            Verb::AnswerRecord {
                issue,
                by,
                choice,
                option,
                text_file,
            } => {
                a.issue(*issue);
                a.flag("--by", by);
                a.flag("--choice", choice.word());
                a.maybe("--option", option.as_ref());
                a.maybe("--text-file", text_file.as_ref());
            }
            Verb::ApprovalRecord {
                issue,
                at,
                by,
                how,
                body_sha,
                title_sha,
            } => {
                a.issue(*issue);
                a.flag("--at", at);
                a.flag("--by", by);
                a.flag("--how", how);
                a.flag("--body-sha", body_sha);
                a.maybe("--title-sha", title_sha.as_ref());
            }
            Verb::ApprovalUndo { issue, at } => {
                a.issue(*issue);
                a.switch("--undo", true);
                a.flag("--at", at);
            }
            Verb::PlanShow(Plans::Open) => a.switch("--open", true),
            Verb::PlanShow(Plans::Issue(n)) => a.issue(*n),
            Verb::HistoryShow { issue } => {
                if let Some(n) = issue {
                    a.issue(*n);
                }
            }
            Verb::PollShow { api_now } => {
                if let Some(now) = api_now {
                    a.switch("--api", true);
                    a.flag("--now", now);
                }
            }
            Verb::PollIdle { since, budget } => {
                a.flag("--since", since);
                a.flag("--budget", budget);
            }
            Verb::OrderSet(o) => {
                let issues: Vec<String> = o.issues.iter().map(u64::to_string).collect();
                a.flag("--issues", issues.join(","));
                a.switch("--only", o.only);
                a.maybe("--until", o.until.as_ref());
                a.maybe("--reason", o.reason.as_ref());
            }
            Verb::PluginRunBegin {
                plugin,
                commit,
                kind,
                how,
                by,
                head,
            } => {
                a.flag("--plugin", plugin);
                a.flag("--commit", commit);
                a.flag("--kind", kind.word());
                a.flag("--how", how.word());
                a.flag("--by", by);
                a.flag("--head", head);
            }
            Verb::PluginRunEnd {
                run,
                result,
                size,
                tokens,
            } => {
                a.flag("--run", run);
                a.flag("--result", result);
                a.maybe("--size", size.as_ref());
                a.maybe("--tokens", tokens.as_ref());
            }
            Verb::PluginRuns { plugin, no_calls } => {
                a.flag("--plugin", plugin);
                a.switch("--no-calls", *no_calls);
            }
        }
        a.0
    }
}

struct Words(Vec<String>);

impl Words {
    fn issue(&mut self, n: u64) {
        self.0.push(n.to_string());
    }

    fn switch(&mut self, flag: &str, on: bool) {
        if on {
            self.0.push(flag.to_string());
        }
    }

    fn flag(&mut self, flag: &str, value: impl ToString) {
        self.0.push(flag.to_string());
        self.0.push(value.to_string());
    }

    fn maybe(&mut self, flag: &str, value: Option<&impl ToString>) {
        if let Some(v) = value {
            self.flag(flag, v.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_are_the_name_then_the_issue_then_the_flags_in_order() {
        let end = Verb::CycleEnd {
            issue: 5,
            attempt: "3".into(),
            end: End::Refused(65),
        };
        assert_eq!(
            end.args(),
            [
                "cycle-end",
                "5",
                "--attempt",
                "3",
                "--refused",
                "--rc",
                "65"
            ]
        );
        let set = Verb::OrderSet(Order {
            issues: vec![3, 1],
            only: true,
            until: Some(9),
            reason: None,
        });
        assert_eq!(
            set.args(),
            ["order-set", "--issues", "3,1", "--only", "--until", "9"]
        );
        assert_eq!(Verb::HistoryShow { issue: None }.args(), ["history-show"]);
    }

    #[test]
    fn a_heartbeat_names_running_or_ended_and_its_numbers_in_order() {
        let beat = Verb::CycleHeartbeat {
            issue: 7,
            attempt: Some("2".into()),
            running: true,
            status: "running".into(),
            step: Some("explore".into()),
            text: String::new(),
            progress: vec![("--step-number".to_string(), "2".to_string())],
        };
        assert_eq!(
            beat.args(),
            [
                "cycle-heartbeat",
                "7",
                "--attempt",
                "2",
                "--running",
                "--status",
                "running",
                "--step",
                "explore",
                "--text",
                "",
                "--step-number",
                "2"
            ]
        );
        assert_eq!(
            Verb::Records { issue: 7 }.args(),
            ["records", "--issue", "7"]
        );
    }
}
