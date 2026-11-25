//! Every call core's `Verb` makes is one this store parses: the client's words
//! are held to the store's grammar here, so the two cannot drift.

use crate::parse;
use tehas_core::plugin::run::{Kind, Trigger};
use tehas_core::question::Choice;
use tehas_core::verb::{End, Order, Plans, Verb};

const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// A sample of each variant. The match in `named` has no wildcard, so a new
/// variant does not build until it has a sample here.
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
        Verb::CycleShow { issue: 5 },
        Verb::CyclesRunning,
        Verb::LocksHeld,
        Verb::ParkShow { issue: 5 },
        Verb::ParkRetry { issue: 5 },
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
            by: s("host"),
            how: s("host"),
            body_sha: s(SHA),
            title_sha: Some(s(SHA)),
        },
        Verb::ApprovalUndo {
            issue: 5,
            at: s("1700000000"),
        },
        Verb::ApprovalCurrent { issue: 5 },
        Verb::ApprovalHosts,
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
            head: s(COMMIT),
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
    ]
}

fn named(v: &Verb) -> &'static str {
    match v {
        Verb::Init { .. }
        | Verb::Check
        | Verb::Copy { .. }
        | Verb::Load { .. }
        | Verb::BackupShow
        | Verb::Upkeep { .. }
        | Verb::Publish { .. }
        | Verb::CycleStartByHand { .. }
        | Verb::CycleEnd { .. }
        | Verb::CycleShow { .. }
        | Verb::CyclesRunning
        | Verb::LocksHeld
        | Verb::ParkShow { .. }
        | Verb::ParkRetry { .. }
        | Verb::QuestionWithdrawn { .. }
        | Verb::QuestionShow { .. }
        | Verb::QuestionList { .. }
        | Verb::AnswerRecord { .. }
        | Verb::ApprovalRecord { .. }
        | Verb::ApprovalUndo { .. }
        | Verb::ApprovalCurrent { .. }
        | Verb::ApprovalHosts
        | Verb::PlanShow(_)
        | Verb::HistoryShow { .. }
        | Verb::PollShow { .. }
        | Verb::PollIdle { .. }
        | Verb::OrderShow
        | Verb::OrderSet(_)
        | Verb::OrderClear
        | Verb::PluginRunBegin { .. }
        | Verb::PluginRunEnd { .. }
        | Verb::PluginRuns { .. } => v.name(),
    }
}

#[test]
fn every_call_the_client_makes_is_one_the_store_parses() {
    for v in samples() {
        let args = v.args();
        assert_eq!(args[0], named(&v));
        assert!(parse(&args).is_some(), "the store does not parse {args:?}");
    }
}
