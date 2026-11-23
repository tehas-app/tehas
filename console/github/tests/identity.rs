//! The identity port as the adapter answers it: `me` and `standing`, each
//! against the samples in `tests/samples/identity-*.json`, answered from the
//! recordings of the fixture project under `tests/recorded/identity/`,
//! replayed through a runner that writes down every call.

use serde::Deserialize;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::{
    Account, AccountId, Actor, Got, Identity, Login, Outcome, ProjectName, Refusal, Scope,
    SignInCode, SignInState, Standing, Unread,
};

const PROJECT: &str = "acme/widget";

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn replay_now() -> i64 {
    1_767_225_600
}

type Asked = Rc<RefCell<Vec<String>>>;

/// Answers from `tests/recorded/identity/`: the viewer question from
/// `me-<case>`, an account read from `user-<id>`, a permission read from
/// `permission-<login>`; `.json` is gh's answer, `.fail` the runner's
/// reason. A call with no recording fails the test.
struct Replay {
    case: String,
    asked: Asked,
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.borrow_mut().push(args.join(" "));
        let name = match args {
            ["api", "graphql", "-f", q] if q.starts_with("query=") => format!("me-{}", self.case),
            ["api", path] => {
                if let Some(id) = path.strip_prefix("user/") {
                    format!("user-{id}")
                } else {
                    let login = path
                        .strip_prefix(&format!("repos/{PROJECT}/collaborators/"))
                        .and_then(|r| r.strip_suffix("/permission"))
                        .unwrap_or_else(|| panic!("no recording of {path}"));
                    format!("permission-{login}")
                }
            }
            _ => panic!("no recording of {args:?}"),
        };
        let at = here().join("tests/recorded/identity");
        if let Ok(out) = std::fs::read_to_string(at.join(format!("{name}.json"))) {
            return Ok(out);
        }
        match std::fs::read_to_string(at.join(format!("{name}.fail"))) {
            Ok(why) => Err(why.trim_end().to_string()),
            Err(_) => panic!("no recording {name}"),
        }
    }
}

fn adapter(case: &str) -> (GitHub, Asked) {
    let asked = Asked::default();
    let replay = Replay {
        case: case.to_string(),
        asked: Rc::clone(&asked),
    };
    (
        GitHub::new(PROJECT, Box::new(replay)).on_clock(replay_now),
        asked,
    )
}

fn samples<T: for<'de> Deserialize<'de>>(file: &str) -> Vec<T> {
    let at = here().join("tests/samples").join(file);
    let text = std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", at.display()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Me {
    op: String,
    actor: Actor,
    case: String,
    fixture: String,
    answer: Got<Account>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OfStanding {
    op: String,
    actor: Actor,
    who: AccountId,
    fixture: String,
    answer: Got<Standing>,
}

#[test]
fn me_answers_as_each_sample_says_in_one_call() {
    let all: Vec<Me> = samples("identity-me.json");
    assert!(all.len() >= 12);
    for s in all {
        assert_eq!(s.op, "Identity::me");
        assert!(
            matches!(s.actor, Actor::Ours(_) | Actor::Operator),
            "{}",
            s.fixture
        );
        let (mut gh, asked) = adapter(&s.case);
        assert_eq!(gh.me(), s.answer, "{}: {}", s.case, s.fixture);
        assert_eq!(
            *asked.borrow(),
            ["api graphql -f query=query { viewer { login databaseId } }"],
            "{}",
            s.case
        );
    }
}

#[test]
fn standing_answers_as_each_sample_says() {
    let all: Vec<OfStanding> = samples("identity-standing.json");
    assert!(all.len() >= 20);
    for s in all {
        assert_eq!(s.op, "Identity::standing");
        assert!(matches!(s.actor, Actor::Ours(_)), "{}", s.fixture);
        let (mut gh, _) = adapter("none");
        assert_eq!(
            gh.standing(&s.who),
            s.answer,
            "{}: {}",
            s.who.as_str(),
            s.fixture
        );
    }
}

fn standing_of(id: &str) -> (Got<Standing>, Vec<String>) {
    let (mut gh, asked) = adapter("none");
    let got = gh.standing(&AccountId::new(id));
    let asked = asked.borrow().clone();
    (got, asked)
}

#[test]
fn standing_reads_the_account_by_id_then_its_permission_by_login() {
    let (got, asked) = standing_of("102");
    assert_eq!(got, Ok(Standing::Write));
    assert_eq!(
        asked,
        [
            "api user/102",
            "api repos/acme/widget/collaborators/Bob/permission"
        ]
    );
}

#[test]
fn a_missing_account_and_one_with_no_access_are_two_answers() {
    assert_eq!(
        standing_of("107"),
        (Err(Outcome::NotFound), vec!["api user/107".to_string()])
    );
    assert_eq!(standing_of("106").0, Ok(Standing::None));
}

#[test]
fn a_failure_is_never_no_standing() {
    for id in [
        "112", "113", "114", "115", "116", "117", "118", "119", "120",
    ] {
        let (got, _) = standing_of(id);
        assert!(got.is_err(), "{id}: {got:?}");
    }
}

#[test]
fn an_organisation_and_an_app_are_answered_from_the_account_read_alone() {
    assert_eq!(
        standing_of("108"),
        (Ok(Standing::None), vec!["api user/108".to_string()])
    );
    assert_eq!(
        standing_of("900"),
        (
            Err(Outcome::Refused(Refusal::Unsupported)),
            vec!["api user/900".to_string()]
        )
    );
}

#[test]
fn an_answer_about_another_account_is_read_once_more_then_inconsistent() {
    let (got, asked) = standing_of("110");
    assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)));
    let pair = [
        "api user/110",
        "api repos/acme/widget/collaborators/grace/permission",
    ];
    assert_eq!(asked, [pair, pair].concat());
    let (got, asked) = standing_of("121");
    assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)));
    assert_eq!(asked, ["api user/121", "api user/121"]);
}

/// Answers each call from the list in turn.
struct Stub {
    answers: Vec<Result<String, String>>,
    asked: Asked,
}

impl GhRunner for Stub {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let mut asked = self.asked.borrow_mut();
        asked.push(args.join(" "));
        self.answers
            .get(asked.len() - 1)
            .cloned()
            .unwrap_or_else(|| panic!("asked {args:?} past the answers"))
    }
}

fn recorded(name: &str) -> String {
    std::fs::read_to_string(here().join("tests/recorded/identity").join(name)).unwrap()
}

#[test]
fn a_rename_between_the_reads_is_settled_by_the_second_pair() {
    let asked = Asked::default();
    let stub = Stub {
        answers: vec![
            Ok(recorded("user-110.json")),
            Ok(recorded("permission-grace.json")),
            Ok(recorded("user-110.json").replace("\"grace\"", "\"grace2\"")),
            Ok(recorded("permission-alice.json").replace("101", "110")),
        ],
        asked: Rc::clone(&asked),
    };
    let mut gh = GitHub::new(PROJECT, Box::new(stub));
    assert_eq!(gh.standing(&AccountId::new("110")), Ok(Standing::Admin));
    assert_eq!(
        asked.borrow()[3],
        "api repos/acme/widget/collaborators/grace2/permission"
    );
}

#[test]
fn a_login_that_is_no_path_segment_stays_one() {
    let asked = Asked::default();
    let stub = Stub {
        answers: vec![
            Ok(recorded("user-105.json").replace("\"erin\"", "\"../../x?y\"")),
            Err(": HTTP 404".to_string()),
        ],
        asked: Rc::clone(&asked),
    };
    let mut gh = GitHub::new(PROJECT, Box::new(stub));
    assert_eq!(gh.standing(&AccountId::new("105")), Err(Outcome::NotFound));
    assert_eq!(
        asked.borrow()[1],
        "api repos/acme/widget/collaborators/..%2F..%2Fx%3Fy/permission"
    );
}

/// A runner no call may reach.
struct Unasked;

impl GhRunner for Unasked {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        panic!("asked {args:?}")
    }
}

#[test]
fn what_no_caller_asks_yet_is_unsupported_before_any_request() {
    let mut gh = GitHub::new(PROJECT, Box::new(Unasked));
    let no = Outcome::Refused(Refusal::Unsupported);
    let id = AccountId::new("101");
    let login = Login::new("alice");
    assert_eq!(gh.account(&id), Err(no.clone()));
    assert_eq!(gh.account_by_login(&login), Err(no.clone()));
    assert_eq!(gh.is_organisation(&login), Err(no.clone()));
    assert_eq!(gh.assignable(&id), Err(no));
    assert!(matches!(
        gh.token(Scope::Full),
        Err(Outcome::Refused(Refusal::Unsupported))
    ));
    assert!(matches!(
        gh.token_over(Scope::Read, &[ProjectName::new(PROJECT)]),
        Err(Outcome::Refused(Refusal::Unsupported))
    ));
    assert!(matches!(
        gh.signed_in(&SignInCode::new("c")),
        Err(Outcome::Refused(Refusal::Unsupported))
    ));
    assert_eq!(
        gh.sign_in_link(&SignInState::new("s")).as_str(),
        "https://github.com/login"
    );
    assert_eq!(
        standing_of_unasked("abc"),
        Err(Outcome::Refused(Refusal::Invalid))
    );
}

fn standing_of_unasked(id: &str) -> Got<Standing> {
    GitHub::new(PROJECT, Box::new(Unasked)).standing(&AccountId::new(id))
}
