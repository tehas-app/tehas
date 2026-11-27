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
/// `permission-<login>`, a login's account from `login-<login>` (in lower
/// case, a bot's `[bot]` as written), an assignee check from `assignee-<login>`; `.json` is gh's answer, `.fail` the runner's
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
                } else if let Some(login) = path.strip_prefix("users/") {
                    // GitHub answers a login in any case.
                    let login = login.replace("%5B", "[").replace("%5D", "]");
                    format!("login-{}", login.to_ascii_lowercase())
                } else if let Some(login) =
                    path.strip_prefix(&format!("repos/{PROJECT}/assignees/"))
                {
                    format!("assignee-{login}")
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
    let until = At::new(replay_now() + 60);
    for (answer, failure) in [
        (
            Err(": HTTP 429".to_string()),
            Outcome::Spent {
                pool: Pool::Calls,
                until,
            },
        ),
        (
            Err(": secondary rate limit".to_string()),
            Outcome::Spent {
                pool: Pool::Calls,
                until,
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
    // A word above the base role: the role caps it.
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
        for word in ["admin", "write", "read"] {
            assert_eq!(of(word), Ok(Standing::Read), "{role:?} {word}");
        }
        assert_eq!(of("none"), Ok(Standing::None), "{role:?}");
        for word in ["maintain", "triage"] {
            assert_eq!(
                of(word),
                Err(Outcome::Unread(Unread::Malformed)),
                "{role:?} {word}"
            );
        }
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

/// The standing of an answer about the account 124 with this role_name and
/// legacy word, each given as JSON text, or left out when `None`.
fn standing_of_pair(role: Option<&str>, word: Option<&str>) -> Got<Standing> {
    let mut answer = r#"{"user":{"login":"peggy","id":124}"#.to_string();
    if let Some(role) = role {
        answer.push_str(&format!(r#","role_name":{role}"#));
    }
    if let Some(word) = word {
        answer.push_str(&format!(r#","permission":{word}"#));
    }
    answer.push('}');
    standing_answered(&answer)
}

/// The standing of a base role with a legacy word, both quoted.
fn role_with_word(role: &str, word: &str) -> Got<Standing> {
    standing_of_pair(Some(&format!("\"{role}\"")), Some(&format!("\"{word}\"")))
}

#[test]
fn role_write_with_word_read_is_triage() {
    assert_eq!(role_with_word("write", "read"), Ok(Standing::Triage));
}

#[test]
fn role_write_with_word_none_is_none() {
    assert_eq!(role_with_word("write", "none"), Ok(Standing::None));
}

#[test]
fn role_admin_with_word_write_is_maintain() {
    assert_eq!(role_with_word("admin", "write"), Ok(Standing::Maintain));
}

#[test]
fn role_admin_with_word_read_is_triage() {
    assert_eq!(role_with_word("admin", "read"), Ok(Standing::Triage));
}

#[test]
fn role_admin_with_word_none_is_none() {
    assert_eq!(role_with_word("admin", "none"), Ok(Standing::None));
}

#[test]
fn role_maintain_with_word_read_is_triage() {
    assert_eq!(role_with_word("maintain", "read"), Ok(Standing::Triage));
}

#[test]
fn role_maintain_with_word_none_is_none() {
    assert_eq!(role_with_word("maintain", "none"), Ok(Standing::None));
}

#[test]
fn role_triage_with_word_none_is_none() {
    assert_eq!(role_with_word("triage", "none"), Ok(Standing::None));
}

#[test]
fn role_read_with_word_none_is_none() {
    assert_eq!(role_with_word("read", "none"), Ok(Standing::None));
}

#[test]
fn role_read_with_word_admin_is_read_the_role_caps_it() {
    assert_eq!(role_with_word("read", "admin"), Ok(Standing::Read));
}

#[test]
fn role_triage_with_word_write_is_triage_the_role_caps_it() {
    assert_eq!(role_with_word("triage", "write"), Ok(Standing::Triage));
}

#[test]
fn a_base_role_beside_a_word_this_copy_does_not_know_is_malformed() {
    let malformed = Err(Outcome::Unread(Unread::Malformed));
    assert_eq!(role_with_word("write", "owner"), malformed);
    assert_eq!(role_with_word("admin", "maintain"), malformed);
    assert_eq!(standing_of_pair(Some("\"read\""), Some("7")), malformed);
}

#[test]
fn a_custom_or_empty_role_with_a_missing_or_null_word_is_malformed() {
    let malformed = Err(Outcome::Unread(Unread::Malformed));
    for (role, word) in [
        ("\"deployer\"", None),
        ("\"deployer\"", Some("null")),
        ("\"\"", None),
        ("\"\"", Some("null")),
    ] {
        assert_eq!(
            standing_of_pair(Some(role), word),
            malformed,
            "{role} {word:?}"
        );
    }
}

#[test]
fn every_agreeing_pair_is_the_role() {
    for (role, word, standing) in [
        ("admin", "admin", Standing::Admin),
        ("maintain", "write", Standing::Maintain),
        ("write", "write", Standing::Write),
        ("triage", "read", Standing::Triage),
        ("read", "read", Standing::Read),
        ("none", "none", Standing::None),
    ] {
        assert_eq!(role_with_word(role, word), Ok(standing), "{role} {word}");
    }
}

#[test]
fn a_base_role_without_a_word_stays_the_role() {
    for (role, standing) in [
        ("admin", Standing::Admin),
        ("maintain", Standing::Maintain),
        ("write", Standing::Write),
        ("triage", Standing::Triage),
        ("read", Standing::Read),
        ("none", Standing::None),
    ] {
        let quoted = format!("\"{role}\"");
        assert_eq!(
            standing_of_pair(Some(&quoted), None),
            Ok(standing),
            "{role}"
        );
        assert_eq!(
            standing_of_pair(Some(&quoted), Some("null")),
            Ok(standing),
            "{role}"
        );
    }
}

/// The rule before a disagreeing answer took the lower reading: the role
/// alone decided.
fn before(role: Option<&str>, word: Option<&str>) -> Got<Standing> {
    let malformed = Err(Outcome::Unread(Unread::Malformed));
    let word = word.and_then(|w| w.strip_prefix('"')?.strip_suffix('"'));
    let by_word = || match word {
        Some("admin") => Ok(Standing::Admin),
        Some("write") => Ok(Standing::Write),
        Some("read") => Ok(Standing::Read),
        Some("none") => Ok(Standing::None),
        _ => malformed.clone(),
    };
    let role = match role {
        None | Some("null") => return by_word(),
        Some(r) => match r.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
            Some(r) => r,
            None => return malformed,
        },
    };
    match role {
        "admin" => Ok(Standing::Admin),
        "maintain" => Ok(Standing::Maintain),
        "write" => Ok(Standing::Write),
        "triage" => Ok(Standing::Triage),
        "read" => Ok(Standing::Read),
        "none" => Ok(Standing::None),
        _ => match word {
            Some("admin" | "maintain" | "write" | "triage" | "read") => Ok(Standing::Read),
            Some("none") => Ok(Standing::None),
            _ => malformed,
        },
    }
}

#[test]
fn no_role_and_word_answer_a_higher_standing_than_before() {
    let quote = |w: &str| format!("\"{w}\"");
    let roles = [
        "admin",
        "maintain",
        "write",
        "triage",
        "read",
        "none",
        "deployer",
        "",
        "Write",
        "ADMIN",
        "superuser",
    ]
    .map(quote);
    let words = [
        "admin", "write", "read", "none", "maintain", "triage", "owner",
    ]
    .map(quote);
    let mut role_axis: Vec<Option<&str>> = vec![None, Some("null"), Some("7")];
    role_axis.extend(roles.iter().map(|q| Some(q.as_str())));
    let mut word_axis: Vec<Option<&str>> = vec![None, Some("null")];
    word_axis.extend(words.iter().map(|q| Some(q.as_str())));
    let (mut lowered, mut refused) = (0, 0);
    for role in &role_axis {
        for word in &word_axis {
            let old = before(*role, *word);
            let new = standing_of_pair(*role, *word);
            match (&old, &new) {
                (Ok(o), Ok(n)) if n < o => lowered += 1,
                (Ok(o), Ok(n)) => assert_eq!(n, o, "{role:?} {word:?}"),
                (Ok(_), Err(Outcome::Unread(Unread::Malformed))) => refused += 1,
                _ => assert_eq!(new, old, "{role:?} {word:?}"),
            }
        }
    }
    // Lowered: the nine pairs whose word is below the base role. Refused:
    // each of the six base roles beside maintain, triage or an unknown
    // word (18), and each of the five custom or empty roles beside
    // maintain or triage (10).
    assert_eq!((lowered, refused), (9, 28));
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
    assert_eq!(gh.account(&id), Err(no));
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
    assert!(matches!(
        gh.sign_in_link(&SignInState::new("s")),
        Err(Outcome::Refused(Refusal::Unsupported))
    ));
    assert_eq!(
        standing_of_unasked("abc"),
        Err(Outcome::Refused(Refusal::Invalid))
    );
}

fn standing_of_unasked(id: &str) -> Got<Standing> {
    GitHub::new(PROJECT, Box::new(Unasked)).standing(&AccountId::new(id))
}

fn by_login(login: &str) -> (Got<Account>, Vec<String>) {
    let (mut gh, asked) = adapter("none");
    let got = gh.account_by_login(&Login::new(login));
    let asked = asked.lock().unwrap().clone();
    (got, asked)
}

fn account(login: &str, id: u64, kind: AccountKind) -> Got<Account> {
    let mut a = Account::new(tehas_ports::Realm::CodeHost, Login::new(login), kind);
    a.id = Some(AccountId::new(id.to_string()));
    Ok(a)
}

// The App's bot and the organisation of the same name are two accounts
// (GitHub's own shape: the organisation tehas-app is 344514095, the bot
// tehas-app[bot] 347290281). A bot is asked by its whole login and known by
// its id; its bare slug is never asked for it.
#[test]
fn a_bot_is_asked_by_its_whole_login_and_never_by_the_bare_slug() {
    let (bot, asked) = by_login("tehas-app[bot]");
    assert_eq!(bot, account("tehas-app", 347290281, AccountKind::Bot));
    assert_eq!(asked, ["api users/tehas-app%5Bbot%5D"]);
    let (org, asked) = by_login("tehas-app");
    assert_eq!(org, Err(Outcome::NotFound), "an organisation is no account");
    assert_eq!(asked, ["api users/tehas-app"]);
    let (gh_form, asked) = by_login("app/tehas-app");
    assert_eq!(gh_form, bot);
    assert_eq!(asked, ["api users/tehas-app%5Bbot%5D"]);
    // A person and an App that share a name are each themselves.
    assert_eq!(
        by_login("acme-factory").0,
        account("acme-factory", 131, AccountKind::Person)
    );
    assert_eq!(
        by_login("acme-factory[bot]").0,
        account("acme-factory", 932, AccountKind::Bot)
    );
    let (mut gh, _) = adapter("none");
    assert_eq!(gh.is_organisation(&Login::new("tehas-app")), Ok(true));
    assert_eq!(gh.is_organisation(&Login::new("tehas-app[bot]")), Ok(false));
    assert_eq!(gh.is_organisation(&Login::new("acme-factory")), Ok(false));
    assert_eq!(
        gh.is_organisation(&Login::new("acme-factory[bot]")),
        Ok(false)
    );
}

#[test]
fn the_factorys_bot_by_login_is_ours_by_its_id() {
    use tehas_github::GitHubSite;
    use tehas_ports::{Caller, Site};
    let site = |bot_id: Option<u64>| {
        let make: tehas_github::MakeRunner = Arc::new(|_| {
            Ok(Box::new(Replay {
                case: "none".into(),
                asked: Asked::default(),
            }) as Box<dyn GhRunner>)
        });
        GitHubSite::new(PROJECT, "tehas-app[bot]", bot_id, make).on_clock(replay_now)
    };
    let ask = |bot_id, login: &str| {
        site(bot_id)
            .identity(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap()
            .account_by_login(&Login::new(login))
            .map(|a| a.kind)
    };
    assert_eq!(
        ask(Some(347290281), "tehas-app[bot]"),
        Ok(AccountKind::Ours)
    );
    // The bot's login alone, with no id recorded, is not ours.
    assert_eq!(ask(None, "tehas-app[bot]"), Ok(AccountKind::Bot));
    // The organisation of the bare slug is never the bot.
    assert_eq!(ask(Some(347290281), "tehas-app"), Err(Outcome::NotFound));
    assert_eq!(ask(Some(344514095), "tehas-app[bot]"), Ok(AccountKind::Bot));
}

#[test]
fn a_login_is_held_to_the_answer_and_a_bad_one_asks_nothing() {
    assert_eq!(
        by_login("Alice").0,
        account("alice", 101, AccountKind::Person),
        "a login's case is the site's"
    );
    let (mirror, _) = by_login("mirror");
    assert_eq!(mirror, Err(Outcome::Unread(Unread::Inconsistent)));
    assert_eq!(by_login("ghost").0, Err(Outcome::NotFound));
    assert_eq!(
        by_login("down").0,
        Err(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe
        })
    );
    for bad in [
        "",
        "[bot]",
        "app/",
        "-x",
        "a b",
        "a/b",
        "x[bot][bot]",
        "../users",
    ] {
        let (got, asked) = by_login(bad);
        assert_eq!(got, Err(Outcome::Refused(Refusal::Invalid)), "{bad}");
        assert!(asked.is_empty(), "{bad}");
    }
}

fn assignable(id: &str) -> (Got<bool>, Vec<String>) {
    let (mut gh, asked) = adapter("none");
    let got = gh.assignable(&AccountId::new(id));
    let asked = asked.lock().unwrap().clone();
    (got, asked)
}

#[test]
fn assignable_reads_the_account_by_id_then_githubs_check_of_its_login() {
    let (got, asked) = assignable("101");
    assert_eq!(got, Ok(true));
    assert_eq!(
        asked,
        ["api user/101", "api repos/acme/widget/assignees/alice"]
    );
    // Not an assignee: the check finds nothing twice under the same login.
    let (got, asked) = assignable("102");
    assert_eq!(got, Ok(false));
    assert_eq!(
        asked,
        [
            "api user/102",
            "api repos/acme/widget/assignees/Bob",
            "api user/102"
        ]
    );
    // Renamed between the reads: asked again under the new login.
    let (got, asked) = assignable("122");
    assert_eq!(got, Ok(true));
    assert_eq!(
        asked,
        [
            "api user/122",
            "api repos/acme/widget/assignees/olivia",
            "api user/122",
            "api repos/acme/widget/assignees/olivia2"
        ]
    );
    assert_eq!(assignable("103").0, Err(Outcome::Forbidden));
    // A bot and an organisation are never assignees.
    assert_eq!(
        assignable("900"),
        (Ok(false), vec!["api user/900".to_string()])
    );
    assert_eq!(
        assignable("108"),
        (Ok(false), vec!["api user/108".to_string()])
    );
    assert_eq!(assignable("107").0, Err(Outcome::NotFound));
    assert_eq!(assignable("0").0, Err(Outcome::Refused(Refusal::Invalid)));
}

#[test]
fn the_spellings_of_a_bot_name_one_account() {
    let (mut gh, _) = adapter("none");
    let spellings = [
        Login::new("tehas-app[bot]"),
        Login::new("app/tehas-app"),
        Login::new("TEHAS-APP[bot]"),
    ];
    tehas_ports::laws::identity::an_accounts_spellings_fold_to_one_login(&mut gh, &spellings)
        .unwrap();
}
