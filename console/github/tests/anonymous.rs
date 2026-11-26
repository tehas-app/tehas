//! `Actor::Anonymous` on GitHub (Decided 76): a public project's releases
//! and tags read through curl with no credential, a write handle
//! `Forbidden` with no request, a private or missing project refused, and
//! the anonymous budget kept as its own holder's. curl is a stub that
//! answers from `tests/recorded/anonymous/`, live answers to an anonymous
//! reader (kit run #1, 2026-11-26) of the public `tehas-app/tehas-rec-*`
//! projects and of the renamed `twitter/bootstrap`, and keeps the arguments
//! and the environment it was given. A case GitHub cannot be made to answer
//! reads a recording edited by hand from `tests/synthetic/anonymous/<case>/`
//! first; its README says how.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use tehas_core::testdir::TestDir;
use tehas_github::{Curl, GitHubSite, MakeRunner, CURL_ENV};
use tehas_ports::fake::{MemoryLedger, NoticeLog};
use tehas_ports::{
    Actor, At, Budget, Caller, CommitId, Failure, Health, HealthState, Holder, HolderId, HostId,
    Ledger, LedgerDown, Outcome, Passing, Pool, Refusal, Sent, Site, TagName, Unread, Whose,
    ALL_MAX,
};

const CALLER: Caller = Caller::new("test.anonymous");
const UNSUPPORTED: Outcome = Outcome::Refused(Refusal::Unsupported);

const REPO: &str = "tehas-app/tehas-rec-releases";

/// The commit the recorded `v1.1.0`, an annotated tag, peels to.
const V110: &str = "f703bfa5b38d4088f9235eed9fde20a16370e0ee";

/// A stub curl in a test directory, answering a path under the API's host
/// from its recording, else a 404, and keeping each call's arguments in
/// `argv` and the environment it was started with in `env`, one call a
/// block ended by `--`.
struct Stub {
    dir: TestDir,
    _serial: MutexGuard<'static, ()>,
}

/// One test at a time: a script just written cannot be started while
/// another test's fork still holds it open for writing.
static SERIAL: Mutex<()> = Mutex::new(());

impl Stub {
    fn new(name: &str) -> Stub {
        Stub::answering(name, "recorded/anonymous")
    }

    /// A stub that answers from the synthetic `case` first.
    fn synthetic(name: &str, case: &str) -> Stub {
        Stub::answering(name, &format!("synthetic/anonymous/{case}"))
    }

    fn answering(name: &str, first: &str) -> Stub {
        let serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let dir = TestDir::new(name);
        let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        let script = format!(
            r#"#!/bin/sh
d='{dir}'
for a in "$@"; do printf '%s\n' "$a"; done >> "$d/argv"
printf -- '--\n' >> "$d/argv"
tr '\0' '\n' < /proc/$$/environ >> "$d/env"
printf -- '--\n' >> "$d/env"
for a in "$@"; do url=$a; done
f=$(printf '%s' "${{url#https://api.github.com/}}" | tr -c 'A-Za-z0-9' '_')
for r in '{first}' '{recorded}'; do
  if [ -f "$r/$f" ]; then cat "$r/$f"; exit 0; fi
done
printf 'HTTP/2 404 \r\n\r\n{{"message":"Not Found"}}'
"#,
            dir = dir.path().display(),
            first = tests.join(first).display(),
            recorded = tests.join("recorded/anonymous").display(),
        );
        let program = dir.join("curl");
        std::fs::write(&program, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        Stub {
            dir,
            _serial: serial,
        }
    }

    fn program(&self) -> PathBuf {
        self.dir.join("curl")
    }

    /// Each call's lines in `file`.
    fn calls(&self, file: &str) -> Vec<Vec<String>> {
        let text = std::fs::read_to_string(self.dir.join(file)).unwrap_or_default();
        text.split_terminator("--\n")
            .map(|c| c.lines().map(String::from).collect())
            .collect()
    }

    /// The path each call asked, under the API's host.
    fn asked(&self) -> Vec<String> {
        self.calls("argv")
            .iter()
            .map(|c| {
                let url = c.last().unwrap();
                url.strip_prefix("https://api.github.com/")
                    .unwrap()
                    .to_string()
            })
            .collect()
    }
}

/// A gh that must never run: an anonymous read goes through curl alone.
fn no_gh() -> MakeRunner {
    Arc::new(|by: &Actor| panic!("a runner of gh was asked for {by:?}"))
}

fn site(repo: &str, stub: &Stub, ledger: Arc<MemoryLedger>) -> GitHubSite {
    site_told(repo, stub, ledger, Arc::new(NoticeLog::new()))
}

/// The same, with the sink the health notices are told to kept by the
/// test, so a case can assert one report.
fn site_told(
    repo: &str,
    stub: &Stub,
    ledger: Arc<dyn Ledger>,
    notices: Arc<NoticeLog>,
) -> GitHubSite {
    GitHubSite::new(repo, "tehas-app[bot]", Some(900), no_gh())
        .with_anonymous_curl(Curl::at(stub.program()), ledger, notices)
        .on_clock(|| 1_795_530_000)
}

/// Each notice told, as (whose, what).
fn told(log: &NoticeLog) -> Vec<(Whose, Health)> {
    log.all().into_iter().map(|n| (n.whose, n.what)).collect()
}

fn tag(name: &str) -> TagName {
    TagName::new(name).unwrap()
}

fn anonymous() -> Holder {
    Holder::Anonymous(HolderId::new("anonymous"))
}

#[test]
fn a_public_project_s_releases_and_tags_are_read_with_no_credential() {
    // A token in the caller's environment reaches no request. Set under
    // the stub's lock, which every test here holds while it starts curl.
    let stub = Stub::new("github-anonymous-reads");
    std::env::set_var("GH_TOKEN", "a-token-that-must-not-travel");
    std::env::set_var("GITHUB_TOKEN", "a-token-that-must-not-travel");
    std::env::set_var("TEHAS_GH_TOKEN", "a-token-that-must-not-travel");
    let ledger = Arc::new(MemoryLedger::new());
    let site = site(REPO, &stub, Arc::clone(&ledger));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();

    let releases = h.releases(ALL_MAX).unwrap();
    let got: Vec<_> = releases
        .items()
        .iter()
        .map(|r| (r.tag.as_str(), r.draft, r.prerelease))
        .collect();
    assert_eq!(
        got,
        [
            ("v1.4.0", false, false),
            ("v1.2.0", false, true),
            ("v1.1.0", false, false)
        ],
        "no draft shown"
    );
    let v110 = &releases.items()[2];
    assert_eq!(v110.title, "v1.1.0");
    assert_eq!(v110.published.map(At::secs), Some(1_795_702_193));
    assert_eq!(v110.notes, "What changed.");
    assert_eq!(
        v110.files[0].name.as_str(),
        "tehas-v1.1.0-linux-x86_64.tar.gz"
    );
    assert_eq!(v110.files[0].size, 232);
    assert_eq!(
        v110.link.as_str(),
        "https://github.com/tehas-app/tehas-rec-releases/releases/tag/v1.1.0"
    );

    let latest = h.latest_release().unwrap().unwrap();
    assert_eq!(latest.tag, tag("v1.1.0"));

    let tags = h.tags(ALL_MAX).unwrap();
    let got: Vec<_> = tags
        .items()
        .iter()
        .map(|t| (t.name.as_str(), t.annotated, t.at))
        .collect();
    assert_eq!(
        got,
        [
            ("v1.4.0", false, None),
            ("v1.2.0", true, None),
            ("v1.1.0", true, None),
            ("v1.0.0", false, None),
            ("v0.9.0", true, None)
        ]
    );
    assert_eq!(tags.items()[2].commit, CommitId::new(V110).unwrap());

    let resolved = h.resolve_tag(&tag("v1.1.0")).unwrap();
    assert_eq!(resolved, CommitId::new(V110).unwrap(), "peeled");

    assert_eq!(
        stub.asked(),
        [
            "repos/tehas-app/tehas-rec-releases/releases?per_page=100",
            "repos/tehas-app/tehas-rec-releases/releases/latest",
            "repos/tehas-app/tehas-rec-releases/tags?per_page=100",
            "repos/tehas-app/tehas-rec-releases/git/matching-refs/tags/?per_page=100",
            "repos/tehas-app/tehas-rec-releases/git/ref/tags/v1.1.0",
            "repos/tehas-app/tehas-rec-releases/git/tags/6223b38d60aaf9f0d173d1019a5206f3e8222193",
        ]
    );

    // The no-credential guarantee, for every request made: curl's
    // environment is exactly CURL_ENV, and its arguments carry no
    // credential, no config file and no other host.
    let mut want_env: Vec<String> = CURL_ENV.iter().map(|(k, v)| format!("{k}={v}")).collect();
    want_env.sort();
    for mut env in stub.calls("env") {
        env.sort();
        assert_eq!(env, want_env);
    }
    for argv in stub.calls("argv") {
        assert_eq!(argv[0], "-q", "no .curlrc is read");
        for a in &argv {
            let lower = a.to_ascii_lowercase();
            assert!(!lower.contains("authorization"), "{a}");
            assert!(!lower.contains("token"), "{a}");
            assert!(!lower.contains("cookie"), "{a}");
            assert!(
                ![
                    "-u",
                    "--user",
                    "-n",
                    "--netrc",
                    "--netrc-optional",
                    "--netrc-file",
                    "-K",
                    "--config",
                    "-b",
                    "-L",
                    "--location"
                ]
                .contains(&a.as_str()),
                "{a}"
            );
        }
        assert!(argv.last().unwrap().starts_with("https://api.github.com/"));
    }

    // Its calls are metered and its budget kept as the anonymous holder's.
    let host = HostId::new("github.com");
    let kept = ledger.budget(&host, &anonymous(), Pool::Calls).unwrap();
    assert_eq!(
        kept.map(|b| (b.left, b.resets.secs())),
        Some((33, 1_795_704_168))
    );
    let row = site
        .meter()
        .into_iter()
        .find(|r| r.caller == CALLER && r.pool == Pool::Calls)
        .unwrap();
    assert_eq!(row.calls, 6);
}

/// 101 releases: the first page's 100, then the page its `next` link
/// names, by the project's id.
#[test]
fn every_page_of_the_releases_is_read_by_its_next_link() {
    let stub = Stub::new("github-anonymous-pages");
    let site = site(
        "tehas-app/tehas-rec-releases-many",
        &stub,
        Arc::new(MemoryLedger::new()),
    );
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    let releases = h.releases(ALL_MAX).unwrap();
    let items = releases.items();
    assert_eq!(items.len(), 101);
    assert_eq!(items[0].tag, tag("v0.101.0"));
    assert_eq!(items[100].tag, tag("v0.001.0"));
    assert_eq!(
        stub.asked(),
        [
            "repos/tehas-app/tehas-rec-releases-many/releases?per_page=100",
            "repositories/1473249969/releases?per_page=100&page=2",
        ]
    );
}

#[test]
fn an_anonymous_write_handle_is_forbidden_and_makes_no_request() {
    let stub = Stub::new("github-anonymous-writes");
    let site = site(REPO, &stub, Arc::new(MemoryLedger::new()));
    let by = Actor::Anonymous;
    assert_eq!(
        site.ticket_writes(&by, CALLER).err(),
        Some(Outcome::Forbidden)
    );
    assert_eq!(
        site.blueprint_writes(&by, CALLER).err(),
        Some(Outcome::Forbidden)
    );
    assert_eq!(
        site.code_host_writes(&by, CALLER).err(),
        Some(Outcome::Forbidden)
    );
    assert!(stub.asked().is_empty());
}

#[test]
fn an_anonymous_read_outside_the_release_reads_is_unsupported_with_no_request() {
    let stub = Stub::new("github-anonymous-others");
    let site = site(REPO, &stub, Arc::new(MemoryLedger::new()));
    let by = Actor::Anonymous;
    assert_eq!(site.tickets(&by, CALLER).err(), Some(UNSUPPORTED));
    assert_eq!(site.blueprints(&by, CALLER).err(), Some(UNSUPPORTED));
    assert_eq!(site.identity(&by, CALLER).err(), Some(UNSUPPORTED));
    assert_eq!(site.board(&by, CALLER).err(), Some(UNSUPPORTED));
    let mut h = site.code_host(&by, CALLER).unwrap();
    assert_eq!(h.facts().err(), Some(UNSUPPORTED));
    assert_eq!(h.release(&tag("v1.1.0")).err(), Some(UNSUPPORTED));
    // `download` is answered: tests/anonymous_download.rs.
    let mut into = Vec::new();
    let at = CommitId::new(V110).unwrap();
    assert_eq!(h.archive(&at, 1 << 20, &mut into).err(), Some(UNSUPPORTED));
    assert_eq!(
        h.remote(tehas_ports::Access::Fetch).err(),
        Some(UNSUPPORTED)
    );
    assert!(stub.asked().is_empty());
}

#[test]
fn a_site_not_given_curl_answers_anonymous_reads_unsupported() {
    let site = GitHubSite::new(REPO, "tehas-app[bot]", Some(900), no_gh());
    assert_eq!(
        site.code_host(&Actor::Anonymous, CALLER).err(),
        Some(UNSUPPORTED)
    );
}

#[test]
fn a_private_or_missing_project_is_refused_never_not_found() {
    let stub = Stub::new("github-anonymous-private");
    let site = site(
        "tehas-app/tehas-rec-gone",
        &stub,
        Arc::new(MemoryLedger::new()),
    );
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(h.releases(ALL_MAX).err(), Some(UNSUPPORTED));
    assert_eq!(h.tags(ALL_MAX).err(), Some(UNSUPPORTED));
    assert_eq!(h.latest_release().err(), Some(UNSUPPORTED));
    assert_eq!(h.resolve_tag(&tag("v1.1.0")).err(), Some(UNSUPPORTED));
}

/// GitHub never answers `latest` with a prerelease, so the case is
/// synthetic.
#[test]
fn a_prerelease_answered_as_the_latest_is_inconsistent_as_on_the_tokens_path() {
    let stub = Stub::synthetic("github-anonymous-pre", "pre");
    let site = site(REPO, &stub, Arc::new(MemoryLedger::new()));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(
        h.latest_release(),
        Err(Outcome::Unread(Unread::Inconsistent))
    );
}

/// A redirect that names no place to go is not GitHub's answer, even with a
/// body that would read as one.
#[test]
fn a_redirect_with_no_location_is_malformed_never_its_body() {
    let stub = Stub::synthetic("github-anonymous-no-location", "no-location");
    let site = site(REPO, &stub, Arc::new(MemoryLedger::new()));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(
        h.latest_release().err(),
        Some(Outcome::Unread(Unread::Malformed))
    );
    assert_eq!(
        stub.asked(),
        ["repos/tehas-app/tehas-rec-releases/releases/latest"]
    );
}

/// A redirect within the API's host is followed once, to the answer: a
/// renamed project's 301 to its address by id.
#[test]
fn one_redirect_within_the_api_is_followed() {
    let stub = Stub::new("github-anonymous-renamed");
    let site = site("twitter/bootstrap", &stub, Arc::new(MemoryLedger::new()));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    let latest = h.latest_release().unwrap().unwrap();
    assert_eq!(latest.tag, tag("v5.3.9"));
    assert_eq!(
        latest.link.as_str(),
        "https://github.com/twbs/bootstrap/releases/tag/v5.3.9"
    );
    assert_eq!(
        stub.asked(),
        [
            "repos/twitter/bootstrap/releases/latest",
            "repositories/2126244/releases/latest"
        ]
    );
}

/// A second redirect is not followed, and its body, which would read as a
/// release, is never the answer.
#[test]
fn a_second_redirect_is_malformed_never_its_body() {
    let stub = Stub::synthetic("github-anonymous-chain", "chain");
    let site = site(REPO, &stub, Arc::new(MemoryLedger::new()));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(
        h.latest_release().err(),
        Some(Outcome::Unread(Unread::Malformed))
    );
    assert_eq!(
        stub.asked(),
        [
            "repos/tehas-app/tehas-rec-releases/releases/latest",
            "repositories/1473248788/releases/latest"
        ]
    );
}

#[test]
fn a_public_project_with_no_release_or_no_such_tag_says_so() {
    let stub = Stub::new("github-anonymous-none");
    let site = site(
        "tehas-app/tehas-rec-empty",
        &stub,
        Arc::new(MemoryLedger::new()),
    );
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(h.latest_release(), Ok(None));
    let site = self::site(REPO, &stub, Arc::new(MemoryLedger::new()));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(h.resolve_tag(&tag("v9.9.9")).err(), Some(Outcome::NotFound));
}

/// A 429 with no reset time or `retry-after` holds the budget out for a
/// minute, the floor of every rate limit the adapter reads with no reset.
#[test]
fn a_bare_too_many_requests_holds_the_budget_for_a_minute() {
    let stub = Stub::synthetic("github-anonymous-busy", "busy");
    let site = site(REPO, &stub, Arc::new(MemoryLedger::new()));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(
        h.releases(ALL_MAX).err(),
        Some(Outcome::Spent {
            pool: Pool::Calls,
            until: At::new(1_795_530_060),
        })
    );
}

/// A spent budget cannot be made on demand, so the case is synthetic. Its
/// reset is 174168 s off: the budget is held for an hour, the most any
/// server's reset or `retry-after` holds it.
#[test]
fn a_spent_anonymous_budget_is_kept_and_the_next_call_makes_no_request() {
    let stub = Stub::synthetic("github-anonymous-spent", "spent");
    let ledger = Arc::new(MemoryLedger::new());
    let site = site(REPO, &stub, Arc::clone(&ledger));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    let spent = Outcome::Spent {
        pool: Pool::Calls,
        until: At::new(1_795_533_600),
    };
    assert_eq!(h.releases(ALL_MAX).err(), Some(spent.clone()));
    let host = HostId::new("github.com");
    let whose = Whose::Pool(anonymous(), Pool::Calls);
    assert_eq!(
        ledger.health(&host, &whose).unwrap(),
        HealthState::Spent {
            pool: Pool::Calls,
            until: At::new(1_795_533_600)
        }
    );
    // The factory's own holder is not touched.
    let ours = Whose::Pool(Holder::Ours(HolderId::new("900")), Pool::Calls);
    assert_eq!(ledger.health(&host, &ours).unwrap(), HealthState::Healthy);
    assert_eq!(stub.asked().len(), 1);
    assert_eq!(h.latest_release().err(), Some(spent));
    assert_eq!(stub.asked().len(), 1, "no request while spent");
}

#[test]
fn past_the_deadline_an_anonymous_read_makes_no_request() {
    let stub = Stub::new("github-anonymous-deadline");
    let site = site(REPO, &stub, Arc::new(MemoryLedger::new()));
    let view = site.until(At::new(1_795_529_999));
    let mut h = view.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(
        h.releases(ALL_MAX).err(),
        Some(Outcome::Unavailable {
            why: tehas_ports::Passing::Timeout,
            sent: tehas_ports::Sent::No,
        })
    );
    assert!(stub.asked().is_empty());
}

#[test]
fn an_answer_past_the_cap_is_too_large() {
    let stub = Stub::new("github-anonymous-cap");
    let site = GitHubSite::new(REPO, "tehas-app[bot]", Some(900), no_gh())
        .with_anonymous_curl(
            Curl::at(stub.program()).capped(64),
            Arc::new(MemoryLedger::new()),
            Arc::new(NoticeLog::new()),
        )
        .on_clock(|| 1_795_530_000);
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(
        h.latest_release().err(),
        Some(Outcome::Unread(tehas_ports::Unread::TooLarge))
    );
}

/// A 500 cannot be asked of GitHub on demand, so the case is synthetic:
/// the live releases answer with only its status line changed. The host is
/// the state a passing failure fails, so the answer raises the host's
/// notice once; the holder and the pool see no change of theirs. A healing
/// answer raises the host's `Recovered`: `latest` is not in the synthetic
/// case, so it reads the live recording.
#[test]
fn a_failing_answer_raises_the_host_once_and_a_healing_one_raises_recovered() {
    let stub = Stub::synthetic("github-anonymous-server", "server-error");
    let log = Arc::new(NoticeLog::new());
    let site = site_told(REPO, &stub, Arc::new(MemoryLedger::new()), Arc::clone(&log));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    let out = Outcome::Unavailable {
        why: Passing::Server,
        sent: Sent::Maybe,
    };
    assert_eq!(h.releases(ALL_MAX).err(), Some(out.clone()));
    assert_eq!(
        told(&log),
        [(Whose::Host, Health::Unavailable(Passing::Server))]
    );
    assert_eq!(h.releases(ALL_MAX).err(), Some(out));
    assert_eq!(log.all().len(), 1, "a standing failure raises nothing");
    assert!(h.latest_release().is_ok());
    assert_eq!(
        told(&log),
        [
            (Whose::Host, Health::Unavailable(Passing::Server)),
            (Whose::Host, Health::Recovered),
        ],
        "the healing answer tells the host healthy again"
    );
}

/// A ledger that cannot be read or written fails toward reporting: the
/// answer still goes to the caller, each state's change is told from the
/// healthy base, and the ledger-down notice is told once per handle.
#[test]
fn a_ledger_that_fails_fails_toward_reporting() {
    struct Down;

    impl Ledger for Down {
        fn budget(&self, _: &HostId, _: &Holder, _: Pool) -> Result<Option<Budget>, LedgerDown> {
            Err(LedgerDown)
        }
        fn put_budget(&self, _: &HostId, _: &Holder, _: Pool, _: Budget) -> Result<(), LedgerDown> {
            Err(LedgerDown)
        }
        fn health(&self, _: &HostId, _: &Whose) -> Result<HealthState, LedgerDown> {
            Err(LedgerDown)
        }
        fn swap_health(
            &self,
            _: &HostId,
            _: &Whose,
            _: &HealthState,
            _: &HealthState,
        ) -> Result<bool, LedgerDown> {
            Err(LedgerDown)
        }
        fn failed(&self, _: &HostId, _: Failure, _: u32) -> Result<(), LedgerDown> {
            Err(LedgerDown)
        }
        fn failures(&self, _: &HostId, _: At) -> Result<Vec<Failure>, LedgerDown> {
            Err(LedgerDown)
        }
    }

    let stub = Stub::synthetic("github-anonymous-ledger", "server-error");
    let log = Arc::new(NoticeLog::new());
    let site = site_told(REPO, &stub, Arc::new(Down), Arc::clone(&log));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    let out = Outcome::Unavailable {
        why: Passing::Server,
        sent: Sent::Maybe,
    };
    assert_eq!(h.releases(ALL_MAX).err(), Some(out.clone()));
    assert_eq!(
        told(&log),
        [
            (Whose::Host, Health::LedgerDown),
            (Whose::Host, Health::Unavailable(Passing::Server)),
        ],
        "the spent check fails first, then the host's own change is told"
    );
    assert_eq!(h.releases(ALL_MAX).err(), Some(out));
    assert_eq!(
        told(&log)[2..],
        [(Whose::Host, Health::Unavailable(Passing::Server))],
        "the ledger is told down once per handle"
    );
}

/// A budget that cannot be kept fails toward reporting on its own arm:
/// the answer still goes to the caller, no health change is due (the call
/// was answered), and the ledger is told down once per handle.
#[test]
fn a_budget_that_cannot_be_kept_fails_toward_reporting() {
    struct BadBudget;

    impl Ledger for BadBudget {
        fn budget(&self, _: &HostId, _: &Holder, _: Pool) -> Result<Option<Budget>, LedgerDown> {
            Ok(None)
        }
        fn put_budget(&self, _: &HostId, _: &Holder, _: Pool, _: Budget) -> Result<(), LedgerDown> {
            Err(LedgerDown)
        }
        fn health(&self, _: &HostId, _: &Whose) -> Result<HealthState, LedgerDown> {
            Ok(HealthState::Healthy)
        }
        fn swap_health(
            &self,
            _: &HostId,
            _: &Whose,
            _: &HealthState,
            _: &HealthState,
        ) -> Result<bool, LedgerDown> {
            Ok(true)
        }
        fn failed(&self, _: &HostId, _: Failure, _: u32) -> Result<(), LedgerDown> {
            Ok(())
        }
        fn failures(&self, _: &HostId, _: At) -> Result<Vec<Failure>, LedgerDown> {
            Ok(Vec::new())
        }
    }

    let stub = Stub::new("github-anonymous-budget-down");
    let log = Arc::new(NoticeLog::new());
    let site = site_told(REPO, &stub, Arc::new(BadBudget), Arc::clone(&log));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert!(
        h.releases(ALL_MAX).is_ok(),
        "the answer still goes to the caller"
    );
    assert!(h.releases(ALL_MAX).is_ok());
    assert_eq!(
        told(&log),
        [(Whose::Host, Health::LedgerDown)],
        "the ledger is told down once per handle, and the good answer changes no health"
    );
}

/// The law of a held read runs on the adapter's handle too, not only on
/// the fake: the ledger, the sink and the request count are the ones the
/// handle was wired with.
#[test]
fn a_held_anonymous_read_raises_no_recovered_on_the_law() {
    let stub = Stub::new("github-anonymous-held-law");
    let ledger: Arc<dyn Ledger> = Arc::new(MemoryLedger::new());
    let log = Arc::new(NoticeLog::new());
    let site = site_told(REPO, &stub, Arc::clone(&ledger), Arc::clone(&log));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    let host = HostId::new("github.com");
    let requests = || stub.asked().len() as u32;
    let wiring = tehas_ports::laws::health::Wiring {
        ledger: &*ledger,
        sink: log.as_ref(),
        host: &host,
        requests: &requests,
    };
    tehas_ports::laws::health::a_held_read_raises_no_recovered(
        h.as_mut(),
        wiring,
        &anonymous(),
        Pool::Calls,
        At::new(1_795_530_000),
    )
    .unwrap();
    assert!(stub.asked().is_empty(), "a held read sends nothing");
}

/// curl's arguments, whole: GET only, `https` only, no redirect followed
/// by curl, its own time limit and size cap, two fixed headers.
#[test]
fn curls_arguments_are_pinned() {
    let argv = Curl::new()
        .argv(
            "repos/tehas-app/tehas-rec-releases/releases/latest",
            std::time::Duration::from_secs(600),
        )
        .unwrap();
    assert_eq!(
        argv,
        [
            "-q",
            "--silent",
            "--include",
            "--proto",
            "=https",
            "--max-redirs",
            "0",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--max-filesize",
            "16777216",
            "--header",
            "Accept: application/vnd.github+json",
            "--header",
            "X-GitHub-Api-Version: 2022-11-28",
            "--user-agent",
            "tehas",
            "https://api.github.com/repos/tehas-app/tehas-rec-releases/releases/latest",
        ]
    );
    assert_eq!(CURL_ENV, [("PATH", "/usr/bin:/bin"), ("LC_ALL", "C")]);
}
