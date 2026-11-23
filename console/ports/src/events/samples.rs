//! One envelope per name and side with every optional field set, and one
//! with none: the files of `api/` are written from these.

use std::num::{NonZeroU32, NonZeroU64};

use super::{
    ChangeMerged, ChangeOffered, CycleEnded, CycleId, CycleParked, CycleStarted, End, Envelope,
    How, LimitChanged, LimitState, Name, QuestionAsked, QuestionChoice, QuestionNo, RunnerWord,
    Stop, TicketActed, TicketClosed, When,
};
use crate::{Account, AccountId, AccountKind, Act, At, BlueprintNo, ChangeNo, CloseReason};
use crate::{KeyText, Login, QuestionKind, Realm, TicketKey};

/// The samples of one name and side.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Sample {
    /// The name.
    pub name: Name,
    /// The side.
    pub when: When,
    /// Every optional field set.
    pub full: Envelope,
    /// No optional field set.
    pub bare: Envelope,
}

/// The samples of every name and side, in the order of `Name::ALL`. A new
/// name does not compile until it has its samples.
pub fn samples() -> Vec<Sample> {
    Name::ALL
        .into_iter()
        .filter_map(|name| {
            let (full, bare) = match name {
                Name::CycleStarted => pair(cycle_started(true), cycle_started(false)),
                Name::CycleParked => pair(cycle_parked(true), cycle_parked(false)),
                Name::QuestionAsked => pair(question_asked(true), question_asked(false)),
                Name::CycleEnded => pair(cycle_ended(true), cycle_ended(false)),
                Name::ChangeOffered => pair(change_offered(), change_offered()),
                Name::FactoryLimitChanged => pair(limit_changed(true), limit_changed(false)),
                Name::TicketActed => pair(ticket_acted(), ticket_acted()),
                Name::TicketClosed => pair(ticket_closed(), ticket_closed()),
                Name::ChangeMerged => pair(change_merged(true), change_merged(false)),
                Name::Unknown => return None,
            };
            Some(Sample {
                name,
                when: When::After,
                full,
                bare,
            })
        })
        .collect()
}

fn pair<P: super::Payload>(full: P, bare: P) -> (Envelope, Envelope) {
    (Envelope::new(full), Envelope::new(bare))
}

// 2026-11-22T11:20:00Z, and the times after it.
const T0: i64 = 1_795_346_400;

fn at(minutes: i64) -> At {
    At::new(T0 + minutes * 60)
}

fn bp() -> BlueprintNo {
    BlueprintNo::new(12).expect("12 is no zero")
}

fn change(n: u64) -> ChangeNo {
    ChangeNo::new(n).expect("no zero")
}

fn cycle(attempt: u32) -> CycleId {
    CycleId::new(bp(), NonZeroU32::new(attempt).expect("no zero")).expect("short")
}

fn word(s: &str) -> RunnerWord {
    RunnerWord::new(s).expect("a runner word")
}

fn ticket(full: bool) -> Option<TicketKey> {
    full.then(|| TicketKey::Key(KeyText::new("OPS-12").expect("a ticket key")))
}

fn cycle_started(full: bool) -> CycleStarted {
    CycleStarted {
        cycle: cycle(4),
        blueprint: bp(),
        ticket: ticket(full),
        started: at(5),
        how: How::Approved,
        by: full.then(|| Login::new("alice")),
    }
}

fn cycle_parked(full: bool) -> CycleParked {
    CycleParked {
        cycle: cycle(3),
        blueprint: bp(),
        ticket: ticket(full),
        at: at(120),
        end: Stop::Failed,
        step: full.then(|| word("develop")),
        cap: full.then(|| word("step-failed:develop")),
        park_reason: full.then(|| word("verify")),
        failure_kind: full.then(|| word("verify-failed")),
        failure_cause: full.then(|| word("tests-failed")),
        provider: full,
        repeat: full.then(|| NonZeroU32::new(2).expect("no zero")),
        change: full.then(|| change(9)),
    }
}

fn question_asked(full: bool) -> QuestionAsked {
    QuestionAsked {
        question: QuestionNo::new(5).expect("no zero"),
        kind: QuestionKind::Park,
        blueprint: bp(),
        cycle: full.then(|| cycle(3)),
        asked_at: at(120),
        choices: if full {
            vec![
                QuestionChoice {
                    choice: "retry".to_string(),
                    label: "Run the cycle again".to_string(),
                },
                QuestionChoice {
                    choice: "park".to_string(),
                    label: "Leave it for a person".to_string(),
                },
            ]
        } else {
            Vec::new()
        },
    }
}

fn cycle_ended(full: bool) -> CycleEnded {
    CycleEnded {
        cycle: cycle(3),
        blueprint: bp(),
        ticket: ticket(full),
        started: at(0),
        ended: at(120),
        recorded: full.then(|| at(125)),
        end: End::Failed,
        outcome: "failed".to_string(),
        status: full.then_some(10),
        parked: true,
        signalled: full,
        change: full.then(|| change(9)),
    }
}

fn change_offered() -> ChangeOffered {
    ChangeOffered {
        change: change(21),
        blueprint: bp(),
        cycle: cycle(4),
        end: End::Offered,
        at: at(180),
    }
}

fn limit_changed(full: bool) -> LimitChanged {
    if full {
        LimitChanged {
            set: Some(at(200)),
            state: LimitState::Ok,
            cycles: Some(0),
            plan: Some(false),
            until: Some(at(320)),
            reason: Some("draining".to_string()),
            cleared: Some(at(330)),
        }
    } else {
        LimitChanged {
            set: None,
            state: LimitState::None,
            cycles: None,
            plan: None,
            until: None,
            reason: None,
            cleared: Some(at(330)),
        }
    }
}

fn account() -> Account {
    Account {
        realm: Realm::Tickets,
        login: Login::new("alice"),
        id: Some(AccountId::new("1001")),
        kind: AccountKind::Person,
    }
}

fn ticket_acted() -> TicketActed {
    TicketActed {
        ticket: TicketKey::Number(NonZeroU64::new(12).expect("no zero")),
        act: Act::Approve,
        by: account(),
        at: at(-10),
    }
}

fn ticket_closed() -> TicketClosed {
    TicketClosed {
        ticket: TicketKey::Number(NonZeroU64::new(12).expect("no zero")),
        reason: CloseReason::Done,
        at: at(240),
    }
}

fn change_merged(full: bool) -> ChangeMerged {
    ChangeMerged {
        change: change(21),
        blueprint: full.then(bp),
        at: at(230),
    }
}
