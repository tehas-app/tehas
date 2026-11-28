//! Every call core's `Verb` makes is one this store parses, and reads as the
//! client meant: the client's words are held to the store's grammar here, so
//! the two cannot drift.

use crate::{answer, approval, cycle, db, order, park, plugin, poll, upkeep, verbs};
use std::collections::BTreeSet;
use std::path::PathBuf;
use tehas_core::order::Order;
use tehas_core::plugin::run::{Kind, Trigger};
use tehas_core::question::Choice;
use tehas_core::verb::{End, EndByReport, Plans, Verb};

const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TITLE_SHA: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const HEAD: &str = "76543210fedcba9876543210fedcba9876543210";

/// Samples of the variants; together they cover every arm of `arm`.
fn samples() -> Vec<Verb> {
    let s = String::from;
    vec![
        Verb::Init {
            version: s("v0.0.0"),
            commit: s(COMMIT),
            instance: s("instance"),
            repo: s("acme/widget"),
        },
        Verb::Check,
        Verb::Copy {
            to: s("/tmp/copy.db"),
        },
        Verb::Load {
            from: s("/tmp/copy.db"),
        },
        Verb::BackupShow,
        Verb::Upkeep { dry_run: false },
        Verb::Upkeep { dry_run: true },
        Verb::Publish { health: None },
        Verb::Publish {
            health: Some(s("/tmp/health.json")),
        },
        Verb::CycleStartByHand { issue: 5 },
        Verb::CycleStart {
            issue: 5,
            rukas: Some(s(COMMIT)),
            by_hand: true,
        },
        Verb::CycleEnd {
            issue: 5,
            attempt: s("1"),
            end: End::Stopped,
        },
        Verb::CycleEnd {
            issue: 5,
            attempt: s("2"),
            end: End::Status(s("failed")),
        },
        Verb::CycleEnd {
            issue: 5,
            attempt: s("3"),
            end: End::Refused(65),
        },
        Verb::CycleEndByReport(Box::new(EndByReport {
            issue: 5,
            attempt: s("1"),
            status: s("failed"),
            step: s("commit-pr"),
            rc: s("1"),
            oom: s("0"),
            forks: s("0"),
            runner_exit: Some(s("0")),
            host_saw: true,
            stalled: Some(s("60")),
            timed: true,
            branch: Some(s("feature/issue-5-x")),
            pr: Some(s("12")),
            result: Some(s("/home/tehas/tmp/5.result.json")),
            progress: Some(s("/home/tehas/tmp/5.progress.json")),
            cap: Some(s("step-failed:explore")),
            park_reason: Some(s("quiet")),
            failure_kind: Some(s("network")),
            failure_cause: Some(s("reset")),
            provider: true,
            parked: true,
            never_ran: true,
            signalled: true,
        })),
        Verb::CycleImage {
            issue: 5,
            attempt: s("1"),
            image: s(&format!("sha256:{}", &SHA[..64])),
            rukas: Some(s(COMMIT)),
            settings: Some(s("/home/tehas/tmp/5.settings")),
        },
        Verb::CycleHeartbeat {
            issue: 5,
            attempt: Some(s("1")),
            running: true,
            status: s("running"),
            step: Some(s("explore")),
            text: String::new(),
            progress: vec![(s("--step-number"), s("2")), (s("--step-total"), s("5"))],
        },
        Verb::CycleShow { issue: 5 },
        Verb::CyclesRunning,
        Verb::LocksHeld,
        Verb::ParkShow { issue: 5 },
        Verb::ParkRetry { issue: 5 },
        Verb::Park {
            issue: 5,
            attempt: s("1"),
            reason: s("step-failed:explore"),
            step: Some(s("explore")),
            pr: Some(s("12")),
        },
        Verb::QuestionAsked {
            issue: 5,
            id: s("1234"),
            kind: s("park"),
            body_sha: s(SHA),
            offers: s("retry"),
        },
        Verb::QuestionWithdrawn {
            issue: 5,
            id: s("1234"),
        },
        Verb::QuestionShow { issue: 5 },
        Verb::QuestionList { after: None },
        Verb::QuestionList { after: Some(7) },
        Verb::AnswerRecord {
            issue: 5,
            by: s("alice"),
            choice: Choice::Retry,
            option: None,
            text_file: None,
        },
        Verb::AnswerRecord {
            issue: 5,
            by: s("alice"),
            choice: Choice::Option,
            option: Some(s("2")),
            text_file: None,
        },
        Verb::AnswerRecord {
            issue: 5,
            by: s("alice"),
            choice: Choice::Text,
            option: None,
            text_file: Some(s("/tmp/reply.txt")),
        },
        Verb::ApprovalRecord {
            issue: 5,
            at: s("1700000000"),
            by: s("alice"),
            how: s("host"),
            body_sha: s(SHA),
            title_sha: Some(s(TITLE_SHA)),
        },
        Verb::ApprovalUndo {
            issue: 5,
            at: s("1700000000"),
        },
        Verb::ApprovalCurrent { issue: 5 },
        Verb::ApprovalView {
            issue: 5,
            attempt: Some(s("1")),
        },
        Verb::SettingsView {
            issue: 5,
            attempt: s("1"),
        },
        Verb::ApprovalHosts,
        Verb::Records { issue: 5 },
        Verb::PlanShow(Plans::Open),
        Verb::PlanShow(Plans::Issue(5)),
        Verb::HistoryShow { issue: None },
        Verb::HistoryShow { issue: Some(5) },
        Verb::PollShow { api_now: None },
        Verb::PollShow {
            api_now: Some(1_700_000_000),
        },
        Verb::PollIdle {
            since: 3600,
            budget: s("cpu=5,wall=60,writes=0"),
        },
        Verb::OrderShow,
        Verb::OrderSet(Order {
            issues: vec![3, 1],
            only: true,
            until: Some(1_700_000_000),
            reason: Some(s("a release")),
        }),
        Verb::OrderSet(Order {
            issues: vec![3],
            ..Order::default()
        }),
        Verb::OrderClear,
        Verb::PluginRunBegin {
            plugin: s("floorplan"),
            commit: s(COMMIT),
            kind: Kind::Call,
            how: Trigger::Host,
            by: s("host"),
            head: s(HEAD),
        },
        Verb::PluginRunEnd {
            run: 1,
            result: s("done"),
            size: Some(10),
            tokens: None,
        },
        Verb::PluginRunEnd {
            run: 1,
            result: s("lost"),
            size: None,
            tokens: Some(20),
        },
        Verb::PluginRuns {
            plugin: s("floorplan"),
            no_calls: true,
        },
        Verb::ParksOwed { refine: false },
        Verb::ParksOwed { refine: true },
        Verb::QuestionKnown {
            issue: 5,
            id: s("1234"),
        },
        Verb::QuestionReasked {
            issue: 5,
            id: s("1234"),
            kind: s("park"),
            offers: Some(s("retry")),
        },
        Verb::QuestionAnswered {
            issue: 5,
            id: s("1234"),
            by: s("alice"),
        },
        Verb::AnswerDrop {
            issue: 5,
            id: s("1234"),
        },
        Verb::QuestionAdopt {
            issue: 5,
            id: s("1234"),
            posted_sha: s(SHA),
            body_sha: Some(s(TITLE_SHA)),
        },
        Verb::ParkTold { issue: 5 },
        Verb::PlanNote {
            issue: 5,
            kind: s("held"),
            id: s("1234"),
        },
        Verb::JournalOpen,
        Verb::JournalSettle {
            action: s("1"),
            result: Some(s("ok")),
            evidence: Some(s("{\"comment\":null}")),
            abandoned: false,
        },
        Verb::JournalSettle {
            action: s("1"),
            result: None,
            evidence: None,
            abandoned: true,
        },
        Verb::CycleNote {
            issue: 5,
            said_for: s("1700000000"),
        },
        Verb::CycleStopAsked {
            issue: 5,
            said_only: true,
        },
        Verb::PlanBegin {
            issue: 5,
            by: s("alice"),
            label: s("plan"),
            brief_file: s("/tmp/plan-5.brief"),
            auto: true,
        },
        Verb::PlanBeginUndo { issue: 5 },
        Verb::PlanAnswer {
            issue: 5,
            turn: s("2"),
            by: s("alice"),
            text_file: s("/tmp/answer.md"),
            millwright: true,
        },
        Verb::PlanAnswerUndo {
            issue: 5,
            turn: s("2"),
        },
        Verb::PlanEnd {
            issue: 5,
            turn: s("2"),
            outcome: s("finished"),
            rc: Some(s("0")),
            session: Some(s("s-1")),
            options: vec![s("Yes"), s("No")],
            filed: vec![s("https://github.com/acme/widget/issues/9")],
            out: Some(s("/tmp/5.out")),
        },
        Verb::PlanEndPosted { issue: 5 },
        Verb::PlanEndClosed { issue: 5 },
        Verb::ApprovalClaim {
            issue: 5,
            at: s("1700000000"),
            by: s("alice"),
            how: s("approved"),
            body_sha: s(SHA),
            input_file: s("/tmp/5.input.json"),
            label: Some(s("ready")),
            title_sha: Some(s(TITLE_SHA)),
            comment_id: Some(s("1234")),
            refresh_pr: None,
            refresh_head: None,
        },
        Verb::PollRecord {
            started: s("1700000000.5"),
            cpu_ms: s("12"),
            gh: s("3"),
            writes_log: s("github-writes.jsonl"),
            starts: s("1"),
        },
        Verb::SweepLocks { dry_run: true },
        Verb::PublishDispatch {
            dispatch: s("/home/tehas/tmp/dispatch.json"),
        },
        Verb::HistoryCycles {
            issue: Some(5),
            how: None,
        },
        Verb::HistoryCycles {
            issue: None,
            how: Some(s("refresh")),
        },
    ]
}

/// The arms of `Verb`, numbered 0 to `ARMS - 1`. The match has no wildcard,
/// so a new variant does not build until it has an arm; numbered `ARMS`, with
/// `ARMS` raised, the test below fails until a sample covers it.
const ARMS: usize = 65;

fn arm(v: &Verb) -> usize {
    match v {
        Verb::Init { .. } => 0,
        Verb::Check => 1,
        Verb::Copy { .. } => 2,
        Verb::Load { .. } => 3,
        Verb::BackupShow => 4,
        Verb::Upkeep { .. } => 5,
        Verb::Publish { .. } => 6,
        Verb::CycleStartByHand { .. } => 7,
        Verb::CycleEnd { .. } => 8,
        Verb::CycleShow { .. } => 9,
        Verb::CyclesRunning => 10,
        Verb::LocksHeld => 11,
        Verb::ParkShow { .. } => 12,
        Verb::ParkRetry { .. } => 13,
        Verb::QuestionWithdrawn { .. } => 14,
        Verb::CycleStart { .. } => 32,
        Verb::CycleEndByReport(_) => 33,
        Verb::CycleImage { .. } => 34,
        Verb::CycleHeartbeat { .. } => 35,
        Verb::Park { .. } => 36,
        Verb::QuestionAsked { .. } => 37,
        Verb::ApprovalView { .. } => 38,
        Verb::SettingsView { .. } => 39,
        Verb::Records { .. } => 40,
        Verb::QuestionShow { .. } => 15,
        Verb::QuestionList { .. } => 16,
        Verb::AnswerRecord { .. } => 17,
        Verb::ApprovalRecord { .. } => 18,
        Verb::ApprovalUndo { .. } => 19,
        Verb::ApprovalCurrent { .. } => 20,
        Verb::ApprovalHosts => 21,
        Verb::PlanShow(_) => 22,
        Verb::HistoryShow { .. } => 23,
        Verb::PollShow { .. } => 24,
        Verb::PollIdle { .. } => 25,
        Verb::OrderShow => 26,
        Verb::OrderSet(_) => 27,
        Verb::OrderClear => 28,
        Verb::PluginRunBegin { .. } => 29,
        Verb::PluginRunEnd { .. } => 30,
        Verb::PluginRuns { .. } => 31,
        Verb::ParksOwed { .. } => 41,
        Verb::QuestionKnown { .. } => 42,
        Verb::QuestionReasked { .. } => 43,
        Verb::QuestionAnswered { .. } => 44,
        Verb::AnswerDrop { .. } => 45,
        Verb::QuestionAdopt { .. } => 46,
        Verb::ParkTold { .. } => 47,
        Verb::PlanNote { .. } => 48,
        Verb::JournalOpen => 49,
        Verb::JournalSettle { .. } => 50,
        Verb::CycleNote { .. } => 51,
        Verb::CycleStopAsked { .. } => 52,
        Verb::PlanBegin { .. } => 53,
        Verb::PlanBeginUndo { .. } => 54,
        Verb::PlanAnswer { .. } => 55,
        Verb::PlanAnswerUndo { .. } => 56,
        Verb::PlanEnd { .. } => 57,
        Verb::PlanEndPosted { .. } => 58,
        Verb::PlanEndClosed { .. } => 59,
        Verb::ApprovalClaim { .. } => 60,
        Verb::PollRecord { .. } => 61,
        Verb::SweepLocks { .. } => 62,
        Verb::PublishDispatch { .. } => 63,
        Verb::HistoryCycles { .. } => 64,
    }
}

#[test]
fn every_call_the_client_makes_is_one_the_store_parses() {
    let mut covered = BTreeSet::new();
    for v in samples() {
        covered.insert(arm(&v));
        let args = v.args();
        assert!(
            crate::parse(&args).is_some(),
            "the store does not parse {args:?}"
        );
    }
    assert_eq!(covered, (0..ARMS).collect(), "an arm of Verb has no sample");
}

fn record(call: verbs::Call) -> Option<crate::Verb> {
    Some(crate::Verb::Record(call))
}

/// The calls whose values could be read into the wrong field: the store reads
/// each as the client meant it.
#[test]
fn the_store_reads_each_value_where_the_client_put_it() {
    let s = String::from;
    let pairs: Vec<(Verb, Option<crate::Verb>)> = vec![
        (
            Verb::Init {
                version: s("v0.0.0"),
                commit: s(COMMIT),
                instance: s("instance"),
                repo: s("acme/widget"),
            },
            Some(crate::Verb::Init {
                force: false,
                settings: db::Settings {
                    version: s("v0.0.0"),
                    commit: s(COMMIT),
                    instance: s("instance"),
                    repo: s("acme/widget"),
                },
            }),
        ),
        (
            Verb::Copy {
                to: s("/tmp/copy.db"),
            },
            record(verbs::Call::Upkeep(upkeep::Call::Copy {
                to: s("/tmp/copy.db"),
            })),
        ),
        (
            Verb::Load {
                from: s("/tmp/copy.db"),
            },
            record(verbs::Call::Upkeep(upkeep::Call::Load {
                from: s("/tmp/copy.db"),
            })),
        ),
        (
            Verb::CycleEnd {
                issue: 5,
                attempt: s("3"),
                end: End::Refused(65),
            },
            record(verbs::Call::Cycle(cycle::Call::End {
                issue: 5,
                attempt: 3,
                end: cycle::End::Refused(Some(65)),
            })),
        ),
        (
            Verb::QuestionWithdrawn {
                issue: 5,
                id: s("1234"),
            },
            record(verbs::Call::Park(park::Call::Answered {
                issue: 5,
                id: 1234,
                by: None,
            })),
        ),
        (
            Verb::AnswerRecord {
                issue: 5,
                by: s("alice"),
                choice: Choice::Option,
                option: Some(s("2")),
                text_file: None,
            },
            record(verbs::Call::Answer(answer::Call::Record {
                issue: 5,
                by: s("alice"),
                choice: Choice::Option,
                option: Some(2),
                text: None,
            })),
        ),
        (
            Verb::AnswerRecord {
                issue: 5,
                by: s("alice"),
                choice: Choice::Text,
                option: None,
                text_file: Some(s("/tmp/reply.txt")),
            },
            record(verbs::Call::Answer(answer::Call::Record {
                issue: 5,
                by: s("alice"),
                choice: Choice::Text,
                option: None,
                text: Some(PathBuf::from("/tmp/reply.txt")),
            })),
        ),
        (
            Verb::ApprovalRecord {
                issue: 5,
                at: s("1700000000"),
                by: s("alice"),
                how: s("host"),
                body_sha: s(SHA),
                title_sha: Some(s(TITLE_SHA)),
            },
            record(verbs::Call::Approval(approval::Call::Record(Box::new(
                approval::Record {
                    issue: 5,
                    at: 1_700_000_000,
                    by: s("alice"),
                    how: "host",
                    label: s("host"),
                    body_sha: s(SHA),
                    title_sha: Some(s(TITLE_SHA)),
                    comment_id: None,
                    refresh: None,
                    input: None,
                    easy: false,
                },
            )))),
        ),
        (
            Verb::ApprovalUndo {
                issue: 5,
                at: s("1700000000"),
            },
            record(verbs::Call::Approval(approval::Call::Undo {
                issue: 5,
                at: 1_700_000_000,
            })),
        ),
        (
            Verb::PollShow {
                api_now: Some(1_700_000_000),
            },
            record(verbs::Call::Poll(poll::Call::Show(poll::Show {
                api: true,
                now: Some(1_700_000_000),
            }))),
        ),
        (
            Verb::OrderSet(Order {
                issues: vec![3, 1],
                only: true,
                until: Some(1_700_000_000),
                reason: Some(s("a release")),
            }),
            record(verbs::Call::Order(order::Call::Set(Order {
                issues: vec![3, 1],
                only: true,
                until: Some(1_700_000_000),
                reason: Some(s("a release")),
            }))),
        ),
        (
            Verb::PluginRunBegin {
                plugin: s("floorplan"),
                commit: s(COMMIT),
                kind: Kind::Call,
                how: Trigger::Host,
                by: s("host"),
                head: s(HEAD),
            },
            record(verbs::Call::Plugin(plugin::Call::Begin(plugin::Begin {
                plugin: s("floorplan"),
                commit: s(COMMIT),
                kind: "call",
                how: "host",
                by: s("host"),
                head: s(HEAD),
            }))),
        ),
        (
            Verb::PluginRunEnd {
                run: 1,
                result: s("lost"),
                size: Some(10),
                tokens: Some(20),
            },
            record(verbs::Call::Plugin(plugin::Call::End {
                run: 1,
                result: "lost",
                tokens: 20,
                size: Some(10),
            })),
        ),
    ];
    for (v, want) in pairs {
        assert_eq!(crate::parse(&v.args()), want, "{v:?}");
    }
}
