//! The feed against a fake forge: whose mark counts, which title holds an
//! issue, which label is the pinned one, and how the texts are read. Each case
//! was a case of the Protolab's shell test, which read the feed through its
//! pass.

use super::*;
use crate::events::Event;
use crate::forge::{Approval, ForgeError, LabelEvent};
use std::collections::BTreeMap;

const LABEL: &str = crate::kinds::Kind::Improvement.label();
const STAGE: &str = "the marked stage";
const LIST: &str = "alice";

/// A forge that answers from what a case gave it: the label events and the
/// type's setter of each issue, and the owner's types. An issue it holds
/// nothing for fails, so a read nobody expected cannot pass quietly.
#[derive(Default)]
struct Fake {
    events: BTreeMap<u64, Vec<Event>>,
    typed: BTreeMap<u64, Option<Approval>>,
    types: Option<Vec<String>>,
    event_reads: usize,
}

fn fails(call: &str) -> ForgeError {
    ForgeError::new(call, "")
}

impl Forge for Fake {
    fn labels(&mut self, _: u64) -> Result<Vec<String>, ForgeError> {
        Err(fails("the issue"))
    }
    fn label_state(&mut self, _: u64, _: &str) -> Result<Option<LabelEvent>, ForgeError> {
        Err(fails("the issue's label events"))
    }
    fn label_events(&mut self, issue: u64) -> Result<Vec<Event>, ForgeError> {
        self.event_reads += 1;
        self.events
            .get(&issue)
            .cloned()
            .ok_or_else(|| fails("the issue's label events"))
    }
    fn applied_by(&mut self, _: u64, _: &str) -> Result<Option<Approval>, ForgeError> {
        Err(fails("the issue's label events"))
    }
    fn approval(&mut self, _: u64) -> Result<Option<Approval>, ForgeError> {
        Err(fails("the issue's label events"))
    }
    fn issue_types(&mut self) -> Result<Vec<String>, ForgeError> {
        self.types.clone().ok_or_else(|| fails("the issue types"))
    }
    fn typed_by(&mut self, issue: u64, _: &str) -> Result<Option<Approval>, ForgeError> {
        self.typed
            .get(&issue)
            .cloned()
            .ok_or_else(|| fails("the issue's type events"))
    }
    fn add_label(&mut self, _: u64, _: &str) -> Result<(), ForgeError> {
        Err(fails("add a label"))
    }
    fn remove_label(&mut self, _: u64, _: &str) -> Result<(), ForgeError> {
        Err(fails("remove a label"))
    }
}

fn ev(kind: &str, actor: &str, at: &str) -> Event {
    Event {
        event: kind.to_string(),
        label: LABEL.to_string(),
        actor: actor.to_string(),
        at: Some(at.to_string()),
    }
}

fn renamed(actor: &str, at: &str) -> Event {
    Event {
        event: "renamed".to_string(),
        label: String::new(),
        actor: actor.to_string(),
        at: Some(at.to_string()),
    }
}

fn labelled(n: u64, events: Vec<Event>) -> Fake {
    let mut fake = Fake::default();
    fake.events.insert(n, events);
    fake
}

/// The line of a standing that is not an approval, or "allowed by <who>".
fn said(standing: Standing) -> String {
    match standing {
        Standing::TakenOff(line) | Standing::Unread(line) | Standing::Denied(line) => line,
        Standing::Allowed(a) => format!("allowed by {}", a.who),
    }
}

fn label_standing(fake: &mut Fake, n: u64) -> String {
    said(standing(fake, n, LABEL, None, LIST))
}

#[test]
fn a_label_the_bot_applied_is_no_mark() {
    let mut fake = labelled(20, vec![ev("labeled", "tehas-bot", "2026-10-09T08:00:00Z")]);
    assert_eq!(
        label_standing(&mut fake, 20),
        format!("#20: {LABEL} was applied by 'tehas-bot', not an allowed account")
    );
}

#[test]
fn the_most_recent_labelling_decides_whatever_order_its_events_arrive_in() {
    let mut fake = labelled(
        22,
        vec![
            ev("labeled", "mallory", "2026-10-02T00:00:00Z"),
            ev("labeled", "alice", "2026-10-01T00:00:00Z"),
        ],
    );
    fake.events.insert(
        23,
        vec![
            ev("labeled", "alice", "2026-10-02T00:00:00Z"),
            ev("labeled", "mallory", "2026-10-01T00:00:00Z"),
        ],
    );
    assert_eq!(
        label_standing(&mut fake, 22),
        format!("#22: {LABEL} was applied by 'mallory', not an allowed account")
    );
    assert_eq!(label_standing(&mut fake, 23), "allowed by alice");
}

#[test]
fn a_labelling_that_names_no_account_and_one_that_is_not_there_are_no_approval() {
    let mut fake = labelled(24, vec![ev("labeled", "", "2026-10-09T08:00:00Z")]);
    fake.events.insert(25, Vec::new());
    for n in [24, 25] {
        assert_eq!(
            label_standing(&mut fake, n),
            format!("#{n}: {LABEL} was applied by 'unknown', not an allowed account")
        );
    }
}

#[test]
fn a_label_taken_off_is_no_approval_and_its_remover_is_not_judged_as_the_one_who_applied_it() {
    let mut fake = labelled(
        27,
        vec![
            ev("labeled", "alice", "2026-10-01T00:00:00Z"),
            ev("unlabeled", "alice", "2026-10-01T00:01:00Z"),
        ],
    );
    fake.events.insert(
        65,
        vec![
            ev("labeled", "alice", "2026-10-01T00:00:00Z"),
            ev("unlabeled", "mallory", "2026-10-02T00:00:00Z"),
        ],
    );
    assert_eq!(
        label_standing(&mut fake, 27),
        format!("#27: {LABEL} was taken off by 'alice' since it was applied")
    );
    assert_eq!(
        label_standing(&mut fake, 65),
        format!("#65: {LABEL} was taken off by 'mallory' since it was applied")
    );
}

#[test]
fn label_events_that_cannot_be_read_are_no_answer_at_all() {
    let mut fake = Fake::default();
    assert_eq!(
        label_standing(&mut fake, 26),
        "#26: its label events could not be read, nothing built or removed"
    );
}

#[test]
fn the_allowed_list_of_the_standing_is_whole_logins_in_exact_case() {
    let mut fake = labelled(63, vec![ev("labeled", "Alice", "2026-10-09T08:00:00Z")]);
    fake.events
        .insert(64, vec![ev("labeled", "ali", "2026-10-09T08:00:00Z")]);
    for (n, who) in [(63, "Alice"), (64, "ali")] {
        assert_eq!(
            label_standing(&mut fake, n),
            format!("#{n}: {LABEL} was applied by '{who}', not an allowed account")
        );
    }
}

/// The title hold of issue `n`, after its standing allowed it.
fn hold(fake: &mut Fake, n: u64, kind: Option<&str>) -> Option<String> {
    match standing(fake, n, LABEL, kind, LIST) {
        Standing::Allowed(approved) => title_hold(fake, n, approved, LIST),
        other => panic!("not allowed: {}", said(other)),
    }
}

#[test]
fn a_title_changed_by_an_account_outside_the_list_after_the_label_holds_the_issue() {
    let label = ev("labeled", "alice", "2026-10-09T08:00:00Z");
    let mut fake = labelled(
        50,
        vec![label.clone(), renamed("mallory", "2026-10-09T09:00:00Z")],
    );
    assert_eq!(
        hold(&mut fake, 50, None).as_deref(),
        Some("#50: its title was changed by 'mallory' since alice approved it, left as it was")
    );
    // The label given again approves the title as it now is.
    fake.events.insert(
        50,
        vec![
            label,
            renamed("mallory", "2026-10-09T09:00:00Z"),
            ev("labeled", "alice", "2026-10-09T10:00:00Z"),
        ],
    );
    assert_eq!(hold(&mut fake, 50, None), None);
    // The events the standing read are the ones the hold reads.
    assert_eq!(fake.event_reads, 2);
}

#[test]
fn a_title_changed_by_an_allowed_account_after_the_label_is_published() {
    let mut fake = labelled(
        51,
        vec![
            ev("labeled", "alice", "2026-10-09T08:00:00Z"),
            renamed("alice", "2026-10-09T09:00:00Z"),
        ],
    );
    assert_eq!(hold(&mut fake, 51, None), None);
}

fn typed(actor: &str) -> Option<Approval> {
    Some(Approval {
        actor: actor.to_string(),
        at: "2026-10-09T08:00:00Z".to_string(),
    })
}

#[test]
fn with_the_type_mapped_the_type_setter_is_checked_and_the_label_counts_for_nothing() {
    let mut fake = Fake::default();
    fake.typed.insert(40, typed("alice"));
    fake.typed.insert(41, typed("mallory"));
    fake.typed.insert(42, None);
    let typed_standing =
        |fake: &mut Fake, n: u64| said(standing(fake, n, LABEL, Some("Improvement"), LIST));
    assert_eq!(typed_standing(&mut fake, 40), "allowed by alice");
    assert_eq!(
        typed_standing(&mut fake, 41),
        "#41: the issue type Improvement was set by 'mallory', not an allowed account"
    );
    assert_eq!(
        typed_standing(&mut fake, 42),
        "#42: the issue type Improvement was set by 'unknown', not an allowed account"
    );
    // alice's label is no type.
    fake.events
        .insert(43, vec![ev("labeled", "alice", "2026-10-09T08:00:00Z")]);
    assert_eq!(
        typed_standing(&mut fake, 43),
        "#43: its type events could not be read, nothing built or removed"
    );
    assert_eq!(fake.event_reads, 0);
}

#[test]
fn with_the_type_mapped_a_title_changed_by_an_account_outside_the_list_holds_the_issue() {
    let mut fake = labelled(45, vec![renamed("mallory", "2026-10-09T09:00:00Z")]);
    fake.typed.insert(45, typed("alice"));
    assert_eq!(
        hold(&mut fake, 45, Some("Improvement")).as_deref(),
        Some("#45: its title was changed by 'mallory' since alice approved it, left as it was")
    );
    // Events that cannot be read hold it too.
    fake.typed.insert(46, typed("alice"));
    assert_eq!(
        hold(&mut fake, 46, Some("Improvement")).as_deref(),
        Some("#46: its label events could not be read, nothing built or removed")
    );
}

#[test]
fn a_type_the_owner_does_not_have_is_78_and_types_that_cannot_be_read_are_0() {
    let mut fake = Fake {
        types: Some(vec!["Task".to_string(), "Improvement".to_string()]),
        ..Fake::default()
    };
    assert_eq!(
        type_fault(&mut fake, "Improvement", "improvement", "acme/widget"),
        None
    );
    fake.types = Some(vec!["Task".to_string(), "Bug".to_string()]);
    assert_eq!(
        type_fault(&mut fake, "Improvement", "improvement", "acme/widget"),
        Some((
            78,
            "ERROR: TEHAS_ISSUE_TYPES maps improvement to Improvement, which is no issue type of acme/widget's owner".to_string()
        ))
    );
    fake.types = None;
    assert_eq!(
        type_fault(&mut fake, "Improvement", "improvement", "acme/widget"),
        Some((
            0,
            "ERROR: the issue types failed, nothing built or removed".to_string()
        ))
    );
}

#[test]
fn an_allowed_list_that_names_the_bot_is_refused() {
    let line = "ERROR: the bot account tehas-bot[bot] is in TEHAS_ALLOWED_LABELERS; the bot never authorises work";
    assert_eq!(
        bot_refusal("tehas-bot[bot]", "alice tehas-bot").as_deref(),
        Some(line)
    );
    assert_eq!(
        bot_refusal("tehas-bot[bot]", "alice tehas-bot[bot]").as_deref(),
        Some(line)
    );
    assert_eq!(bot_refusal("tehas-bot[bot]", "alice"), None);
}

/// A runner that answers every call with one answer and keeps what it was
/// asked.
struct Answers {
    answer: Result<String, String>,
    asked: Vec<Vec<String>>,
}

impl GhRunner for Answers {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked
            .push(args.iter().map(|a| (*a).to_string()).collect());
        self.answer.clone()
    }
}

fn answers(answer: Result<&str, &str>) -> Answers {
    Answers {
        answer: answer.map(str::to_string).map_err(str::to_string),
        asked: Vec::new(),
    }
}

#[test]
fn only_the_pinned_label_read_from_github_puts_an_issue_at_the_stage() {
    let home = crate::testdir::TestDir::new("feed-pins");
    std::fs::create_dir_all(home.join(crate::label_pins::FILE).parent().unwrap()).unwrap();
    let pins = BTreeMap::from([(LABEL.to_string(), "LA_proto".to_string())]);
    crate::label_pins::save(&home, &pins).unwrap();
    let pinned = format!(r#"{{"name":"{LABEL}","node_id":"LA_proto"}}"#);
    let mut gh = answers(Ok(&pinned));
    assert_eq!(
        pin_refusal(&mut gh, &home, "acme/widget", LABEL, STAGE),
        Ok(None)
    );
    assert_eq!(
        gh.asked,
        [[
            "api".to_string(),
            ["repos/acme/widget/labels/", LABEL].concat()
        ]]
    );

    let other = format!(r#"{{"name":"{LABEL}","node_id":"LA_bug"}}"#);
    assert_eq!(
        pin_refusal(&mut answers(Ok(&other)), &home, "acme/widget", LABEL, STAGE),
        Ok(Some(format!("refused: the label {LABEL} is not the one pinned in state/labels (pinned LA_proto, now LA_bug): no ticket is at {STAGE}")))
    );
    assert_eq!(
        pin_refusal(
            &mut answers(Err("no answer")),
            &home,
            "acme/widget",
            LABEL,
            STAGE
        ),
        Err(format!(
            "the label {LABEL} could not be read, nothing built or removed"
        ))
    );

    let bare = crate::testdir::TestDir::new("feed-no-pins");
    let refused = pin_refusal(
        &mut answers(Ok(&pinned)),
        &bare,
        "acme/widget",
        LABEL,
        STAGE,
    );
    assert!(
        refused.clone().unwrap().unwrap().starts_with(&format!(
            "refused: the label {LABEL} (LA_proto) has no pin in state/labels"
        )),
        "{refused:?}"
    );
}

#[test]
fn the_texts_are_the_issue_of_the_answer_or_none() {
    let answer = r#"{"data":{"repository":{"issue":{"body":"hi","author":{"login":"alice"},"editor":null,"comments":{"nodes":[]}}}}}"#;
    let mut gh = answers(Ok(answer));
    let issue = texts("acme/widget", &mut gh, 7).unwrap();
    assert_eq!(issue["body"], "hi");
    assert_eq!(issue["author"]["login"], "alice");
    let asked = &gh.asked[0];
    for word in ["owner=acme", "name=widget", "number=7"] {
        assert!(asked.iter().any(|a| a == word), "{asked:?}");
    }
    let query = asked.iter().find(|a| a.starts_with("query=")).unwrap();
    assert!(query.contains("comments(first: 100)"), "{query}");
    assert!(query.contains(WHO), "{query}");

    for bad in [
        r#"{"data":{"repository":{"issue":null}}}"#,
        r#"{"message":"Bad credentials"}"#,
        "not json",
    ] {
        assert!(
            texts("acme/widget", &mut answers(Ok(bad)), 7).is_none(),
            "{bad}"
        );
    }
    assert!(texts("acme/widget", &mut answers(Err("no answer")), 7).is_none());
    // A repository with no owner asks nothing.
    let mut gh = answers(Ok(answer));
    assert!(texts("widget", &mut gh, 7).is_none());
    assert!(gh.asked.is_empty());
}
