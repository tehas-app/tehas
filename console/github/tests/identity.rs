//! The identity port as the adapter answers it: `me` and `standing`, each
//! against the samples in `tests/samples/identity-*.json`, answered from the
//! recordings of the fixture project under `tests/recorded/identity/`,
//! replayed through a runner that writes down every call.

use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::{
    Account, AccountId, AccountKind, Actor, At, Got, Identity, Login, Outcome, Passing, Pool,
    ProjectName, Refusal, Scope, Sent, SignInCode, SignInState, Standing, Unread,
};

const PROJECT: &str = "acme/widget";

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn replay_now() -> i64 {
    1_767_225_600
}

type Asked = Arc<Mutex<Vec<String>>>;

/// Answers from `tests/recorded/identity/`: the viewer question from
/// `me-<case>`, an account read from `user-<id>`, a permission read from
/// `permission-<login>`; `.json` is gh's answer, `.fail` the runner's
/// reason. The second ask of a name is answered from `<name>.2` where that
/// recording exists. A call with no recording fails the test.
struct Replay {
    case: String,
    asked: Asked,
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.asked.lock().unwrap().push(args.join(" "));
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
        let before = self
            .asked
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.as_str() == args.join(" "))
            .count();
        let again = format!("{name}.2");
        let name = if before >= 2
            && (at.join(format!("{again}.json")).exists()
                || at.join(format!("{again}.fail")).exists())
        {
            again
        } else {
            name
        };
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
        asked: Arc::clone(&asked),
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
    /// The id whose account read gives the account's kind, when the
    /// viewer answer names one.
    #[serde(default)]
    kind_from: Option<u64>,
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
fn me_answers_as_each_sample_says() {
    let all: Vec<Me> = samples("identity-me.json");
    assert!(all.len() >= 15);
    for s in all {
        assert_eq!(s.op, "Identity::me");
        assert!(
            matches!(s.actor, Actor::Ours(_) | Actor::Operator),
            "{}",
            s.fixture
        );
        let (mut gh, asked) = adapter(&s.case);
        assert_eq!(gh.me(), s.answer, "{}: {}", s.case, s.fixture);
        let mut want =
            vec!["api graphql -f query=query { viewer { login databaseId } }".to_string()];
        want.extend(s.kind_from.map(|id| format!("api user/{id}")));
        assert_eq!(*asked.lock().unwrap(), want, "{}", s.case);
    }
}

#[test]
fn me_takes_the_kind_from_the_account_read_not_the_login() {
    let kind = |case: &str| adapter(case).0.me().map(|a| (a.login, a.kind));
    let app = Ok((Login::new("tehas-bot"), AccountKind::Bot));
    assert_eq!(kind("app"), app);
    assert_eq!(kind("app-graphql"), app);
    assert_eq!(kind("app-gh"), app);
    assert_eq!(
        kind("operator"),
        Ok((Login::new("alice"), AccountKind::Person))
    );
    assert_eq!(kind("org"), Ok((Login::new("acme"), AccountKind::Unknown)));
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
    let asked = asked.lock().unwrap().clone();
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
        "112", "113", "114", "115", "116", "117", "118", "119", "120", "130",
    ] {
        let (got, _) = standing_of(id);
        assert!(got.is_err(), "{id}: {got:?}");
    }
}

#[test]
fn an_organisation_and_an_app_are_answered_from_the_account_read_alone() {
    for id in ["108", "900", "901"] {
        assert_eq!(
            standing_of(id),
            (Ok(Standing::None), vec![format!("api user/{id}")]),
            "{id}"
        );
    }
}

#[test]
fn a_bot_account_read_that_fails_stays_its_failure() {
    let bot = recorded("user-901.json");
    let now = At::new(replay_now());
    for (answer, failure) in [
        (
            Err(": HTTP 429".to_string()),
            Outcome::Spent {
                pool: Pool::Calls,
                until: now,
            },
        ),
        (
            Err(": secondary rate limit".to_string()),
            Outcome::Spent {
                pool: Pool::Calls,
                until: now,
            },
        ),
        (
            Err("gh ran out of its 30s bound".to_string()),
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::Maybe,
            },
        ),
        (Err(": HTTP 401".to_string()), Outcome::SignedOut),
        (Err(": HTTP 403".to_string()), Outcome::Forbidden),
        (
            Ok(bot[..bot.len() / 2].to_string()),
            Outcome::Unread(Unread::Malformed),
        ),
        (
            Ok(bot.replace("\"login\":\"other-app[bot]\",", "")),
            Outcome::Unread(Unread::Malformed),
        ),
        (
            Ok(bot.replace("\"type\":\"Bot\"", "\"type\":\"Robot\"")),
            Outcome::Unread(Unread::Malformed),
        ),
        (
            Ok(bot.replace("\"id\":901", "\"id\":902")),
            Outcome::Unread(Unread::Inconsistent),
        ),
    ] {
        let asked = Asked::default();
        let stub = Stub {
            answers: vec![answer.clone(), answer.clone()],
            asked: Arc::clone(&asked),
        };
        let mut gh = GitHub::new(PROJECT, Box::new(stub)).on_clock(replay_now);
        assert_eq!(
            gh.standing(&AccountId::new("901")),
            Err(failure),
            "{answer:?}"
        );
        assert!(
            asked.lock().unwrap().iter().all(|a| a == "api user/901"),
            "{answer:?}"
        );
    }
}

/// The standing of the permission answer `permission`, given as the answer
/// about the account 124.
fn standing_answered(permission: &str) -> Got<Standing> {
    let stub = Stub {
        answers: vec![Ok(recorded("user-124.json")), Ok(permission.to_string())],
        asked: Asked::default(),
    };
    GitHub::new(PROJECT, Box::new(stub)).standing(&AccountId::new("124"))
}

#[test]
fn the_role_name_is_read_first_and_the_permission_word_only_without_one() {
    let user = r#""user":{"login":"peggy","id":124}"#;
    let of = |rest: &str| standing_answered(&format!("{{{user}{rest}}}"));
    // GitHub's documented disagreements: the role is named, the word folds
    // it. The role wins, as in the dispatcher's may_write.
    assert_eq!(
        of(r#","role_name":"maintain","permission":"write""#),
        Ok(Standing::Maintain)
    );
    assert_eq!(
        of(r#","role_name":"triage","permission":"read""#),
        Ok(Standing::Triage)
    );
    // A base role the word contradicts is still the role.
    assert_eq!(
        of(r#","role_name":"read","permission":"admin""#),
        Ok(Standing::Read)
    );
    assert_eq!(
        of(r#","role_name":"none","permission":"write""#),
        Ok(Standing::None)
    );
    // No role, or a null one: the word.
    assert_eq!(of(r#","permission":"admin""#), Ok(Standing::Admin));
    assert_eq!(
        of(r#","role_name":null,"permission":"read""#),
        Ok(Standing::Read)
    );
    assert_eq!(
        of(r#","role_name":null"#),
        Err(Outcome::Unread(Unread::Malformed))
    );
    assert_eq!(
        of(r#","permission":"superuser""#),
        Err(Outcome::Unread(Unread::Malformed))
    );
    // The legacy word is one of GitHub's four: it never names maintain or
    // triage, so read alone it never answers them.
    for word in ["maintain", "triage", "Write", ""] {
        assert_eq!(
            of(&format!(r#","permission":"{word}""#)),
            Err(Outcome::Unread(Unread::Malformed)),
            "{word}"
        );
    }
    // A role that is no string is no answer.
    assert_eq!(
        of(r#","role_name":7,"permission":"write""#),
        Err(Outcome::Unread(Unread::Malformed))
    );
}

#[test]
fn a_custom_role_is_never_write_whatever_its_permission_word() {
    let user = r#""user":{"login":"peggy","id":124}"#;
    for role in ["deployer", "", "Write", "ADMIN"] {
        let of = |word: &str| {
            standing_answered(&format!(
                r#"{{{user},"role_name":"{role}","permission":"{word}"}}"#
            ))
        };
        for word in ["admin", "maintain", "write", "triage", "read"] {
            assert_eq!(of(word), Ok(Standing::Read), "{role:?} {word}");
        }
        assert_eq!(of("none"), Ok(Standing::None), "{role:?}");
        assert_eq!(
            of("superuser"),
            Err(Outcome::Unread(Unread::Malformed)),
            "{role:?}"
        );
    }
    assert_eq!(
        standing_answered(&format!(r#"{{{user},"role_name":"deployer"}}"#)),
        Err(Outcome::Unread(Unread::Malformed))
    );
}

#[test]
fn each_role_reads_as_the_recorded_answers_say() {
    for (id, standing) in [
        ("101", Ok(Standing::Admin)),
        ("102", Ok(Standing::Write)),
        ("103", Ok(Standing::Maintain)),
        ("104", Ok(Standing::Triage)),
        ("105", Ok(Standing::Read)),
        ("106", Ok(Standing::None)),
        ("124", Ok(Standing::Read)),
        ("125", Ok(Standing::Read)),
        ("126", Ok(Standing::None)),
        ("127", Ok(Standing::Read)),
        ("128", Ok(Standing::Write)),
        ("129", Ok(Standing::Admin)),
        ("130", Err(Outcome::Unread(Unread::Malformed))),
    ] {
        assert_eq!(standing_of(id).0, standing, "{id}");
    }
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
        let mut asked = self.asked.lock().unwrap();
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
        asked: Arc::clone(&asked),
    };
    let mut gh = GitHub::new(PROJECT, Box::new(stub));
    assert_eq!(gh.standing(&AccountId::new("110")), Ok(Standing::Admin));
    assert_eq!(
        asked.lock().unwrap()[3],
        "api repos/acme/widget/collaborators/grace2/permission"
    );
}

#[test]
fn a_rename_to_a_free_login_between_the_reads_is_settled_by_the_second_pair() {
    let (got, asked) = standing_of("122");
    assert_eq!(got, Ok(Standing::Admin));
    assert_eq!(
        asked,
        [
            "api user/122",
            "api repos/acme/widget/collaborators/olivia/permission",
            "api user/122",
            "api repos/acme/widget/collaborators/olivia2/permission",
        ]
    );
}

#[test]
fn a_permission_read_that_finds_nothing_twice_is_not_found_once_renamed_inconsistent() {
    let (got, asked) = standing_of("117");
    assert_eq!(got, Err(Outcome::NotFound));
    let pair = [
        "api user/117",
        "api repos/acme/widget/collaborators/mallory/permission",
    ];
    assert_eq!(asked, [pair, pair].concat());
    let (got, asked) = standing_of("123");
    assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)));
    assert_eq!(asked.len(), 4);
}

#[test]
fn a_login_that_is_no_path_segment_stays_one() {
    let asked = Asked::default();
    let stub = Stub {
        answers: vec![
            Ok(recorded("user-105.json").replace("\"erin\"", "\"../../x?y\"")),
            Err(": HTTP 404".to_string()),
            Ok(recorded("user-105.json").replace("\"erin\"", "\"../../x?y\"")),
            Err(": HTTP 404".to_string()),
        ],
        asked: Arc::clone(&asked),
    };
    let mut gh = GitHub::new(PROJECT, Box::new(stub));
    assert_eq!(gh.standing(&AccountId::new("105")), Err(Outcome::NotFound));
    assert_eq!(
        asked.lock().unwrap()[1],
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
