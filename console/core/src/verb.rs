//! A call of the state store in tehas's terms: the verb and what it is given,
//! typed, so no caller writes a verb's name or its flags. [`Verb::args`] is
//! the one place a call becomes the words the store parses; the store's own
//! tests (console/store/src/verb_tests.rs) hold every variant to its grammar.

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

/// A start order to set (`order-set`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Order {
    pub issues: Vec<u64>,
    pub only: bool,
    pub until: Option<i64>,
    pub reason: Option<String>,
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
    CycleEnd {
        issue: u64,
        attempt: String,
        end: End,
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
    ApprovalHosts,
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
            Verb::CycleStartByHand { .. } => "cycle-start",
            Verb::CycleEnd { .. } => "cycle-end",
            Verb::CycleShow { .. } | Verb::CyclesRunning => "cycle-show",
            Verb::LocksHeld => "locks-held",
            Verb::ParkShow { .. } => "park-show",
            Verb::ParkRetry { .. } => "park-retry",
            Verb::QuestionWithdrawn { .. } => "question-answered",
            Verb::QuestionShow { .. } => "question-show",
            Verb::QuestionList { .. } => "question-list",
            Verb::AnswerRecord { .. } => "answer-record",
            Verb::ApprovalRecord { .. } | Verb::ApprovalUndo { .. } => "approval-record",
            Verb::ApprovalCurrent { .. } => "approval-current",
            Verb::ApprovalHosts => "approval-hosts",
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
            Verb::CycleShow { issue }
            | Verb::ParkShow { issue }
            | Verb::ParkRetry { issue }
            | Verb::QuestionShow { issue }
            | Verb::ApprovalCurrent { issue } => a.issue(*issue),
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
}
