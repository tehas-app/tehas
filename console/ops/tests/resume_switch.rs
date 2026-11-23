//! `tehas resume`'s read of the other switch, the pause mark on the control
//! issue, through the tickets port, on the ports' fake: who took the mark off
//! decides (AGENTS.md rule 5), and an answer that cannot be read is said on
//! standard error.

use tehas_ops::holds;
use tehas_ports::fake::{FakeSite, Fault, Faulty, Op, TicketsOp, World};
use tehas_ports::{Act, Actor, At, Caller, Happened, Mark, Site};

fn pause() -> Mark {
    Mark::Act(Act::Pause)
}

/// The control issue, #1, with these events of the pause mark.
fn control(events: &[(Happened, Option<&str>)]) -> World {
    World::new().ticket(1, |mut t| {
        for (i, (what, by)) in events.iter().enumerate() {
            t = t.event(what.clone(), *by, Some(At::new(1_767_312_000 + i as i64)));
        }
        t
    })
}

fn other_on(site: &impl Site) -> Option<Result<String, String>> {
    let mut t = site
        .tickets(&Actor::Operator, Caller::new("resume"))
        .unwrap();
    holds::other_switch(&mut *t, 1, &|who| who == "alice")
}

fn other(world: &World) -> Option<Result<String, String>> {
    other_on(&FakeSite::new(world))
}

#[test]
fn the_label_on_the_issue_holds_the_factory() {
    let on = World::new().ticket(1, |t| t.mark(pause()));
    assert_eq!(
        other(&on),
        Some(Ok(
            "the factory stays paused: tehas:pause is still on issue #1".to_string()
        ))
    );
}

#[test]
fn who_took_the_label_off_decides() {
    let stranger = control(&[(Happened::Unmarked(pause()), Some("mallory"))]);
    assert_eq!(
        other(&stranger),
        Some(Ok("the factory stays paused: tehas:pause was taken off #1 by 'mallory', who may not authorise work here".to_string()))
    );
    let nobody = control(&[(Happened::Unmarked(pause()), None)]);
    assert_eq!(
        other(&nobody),
        Some(Ok("the factory stays paused: tehas:pause was taken off #1 by 'unknown', who may not authorise work here".to_string()))
    );
    let alice = control(&[(Happened::Unmarked(pause()), Some("alice"))]);
    assert_eq!(other(&alice), None);
    assert_eq!(other(&control(&[])), None);
}

#[test]
fn an_application_nothing_took_back_holds_the_factory_whoever_made_it() {
    for by in ["mallory", "alice"] {
        let applied = control(&[(Happened::Marked(pause()), Some(by))]);
        assert_eq!(
            other(&applied),
            Some(Ok("the factory stays paused: tehas:pause is not on issue #1, but its last tehas:pause event only applied it".to_string()))
        );
    }
}

#[test]
fn of_two_events_in_one_second_the_application_is_read_last() {
    let at = Some(At::new(1_767_312_000));
    let w = World::new().ticket(1, |t| {
        t.event(Happened::Marked(pause()), Some("mallory"), at)
            .event(Happened::Unmarked(pause()), Some("alice"), at)
    });
    assert_eq!(
        other(&w),
        Some(Ok("the factory stays paused: tehas:pause is not on issue #1, but its last tehas:pause event only applied it".to_string()))
    );
}

#[test]
fn an_answer_that_cannot_be_read_is_said_on_standard_error() {
    let w = control(&[]);
    let labels = Faulty::new(FakeSite::new(&w)).on(Op::Tickets(TicketsOp::Several), Fault::Timeout);
    assert_eq!(
        other_on(&labels),
        Some(Err("tehas resume: the labels of issue #1 could not be read: check tehas:pause there yourself".to_string()))
    );
    assert_eq!(
        other(&World::new()),
        Some(Err("tehas resume: the labels of issue #1 could not be read: check tehas:pause there yourself".to_string()))
    );
    let events = Faulty::new(FakeSite::new(&w)).on(Op::Tickets(TicketsOp::History), Fault::Timeout);
    assert_eq!(
        other_on(&events),
        Some(Err(
            "tehas resume: the label events of issue #1 could not be read".to_string()
        ))
    );
}
