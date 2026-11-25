//! `Actor::Anonymous` on GitHub (Decided 76): a public project's releases
//! and tags read through curl with no credential, a write handle
//! `Forbidden` with no request, a private or missing project refused, and
//! the anonymous budget kept as its own holder's. curl is a stub that
//! answers from `tests/recorded/anonymous/`, recordings of a public
//! project's answers to an anonymous reader, trimmed and renamed
//! `acme/widget`, and keeps the arguments and the environment it was given.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use tehas_core::testdir::TestDir;
use tehas_github::{Curl, GitHubSite, MakeRunner, CURL_ENV};
use tehas_ports::fake::MemoryLedger;
use tehas_ports::{
    Actor, At, Caller, CommitId, HealthState, Holder, HolderId, HostId, Ledger, Outcome, Pool,
    Refusal, Site, TagName, Whose, ALL_MAX,
};

const CALLER: Caller = Caller::new("test.anonymous");
const UNSUPPORTED: Outcome = Outcome::Refused(Refusal::Unsupported);

/// The commit the recorded `v1.4.1`, an annotated tag, peels to.
const V141: &str = "1641015e605f6dcd998166bb3857712e335db7f1";

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
        let serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let dir = TestDir::new(name);
        let recorded = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/anonymous");
        let script = format!(
            r#"#!/bin/sh
d='{dir}'
for a in "$@"; do printf '%s\n' "$a"; done >> "$d/argv"
printf -- '--\n' >> "$d/argv"
tr '\0' '\n' < /proc/$$/environ >> "$d/env"
printf -- '--\n' >> "$d/env"
for a in "$@"; do url=$a; done
f=$(printf '%s' "${{url#https://api.github.com/}}" | tr -c 'A-Za-z0-9' '_')
if [ -f '{recorded}'/"$f" ]; then cat '{recorded}'/"$f"; else printf 'HTTP/2 404 \r\n\r\n{{"message":"Not Found"}}'; fi
"#,
            dir = dir.path().display(),
            recorded = recorded.display(),
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
    GitHubSite::new(repo, "tehas-bot[bot]", Some(900), no_gh())
        .with_anonymous_curl(Curl::at(stub.program()), ledger)
        .on_clock(|| 1_795_530_000)
}

fn tag(name: &str) -> TagName {
    TagName::new(name).unwrap()
}

fn anonymous() -> Holder {
    Holder::Anonymous(HolderId::new("anonymous"))
}

#[test]
fn a_public_project_s_releases_and_tags_are_read_with_no_credential() {
    // A token in the caller's environment reaches no request.
    std::env::set_var("GH_TOKEN", "a-token-that-must-not-travel");
    std::env::set_var("GITHUB_TOKEN", "a-token-that-must-not-travel");
    std::env::set_var("TEHAS_GH_TOKEN", "a-token-that-must-not-travel");
    let stub = Stub::new("github-anonymous-reads");
    let ledger = Arc::new(MemoryLedger::new());
    let site = site("acme/widget", &stub, Arc::clone(&ledger));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();

    let releases = h.releases(ALL_MAX).unwrap();
    let tags: Vec<_> = releases.items().iter().map(|r| r.tag.as_str()).collect();
    assert_eq!(tags, ["v1.4.1", "v1.4.0"], "both pages");
    let first = &releases.items()[0];
    assert_eq!(first.title, "v1.4.1");
    assert!(!first.draft && !first.prerelease);
    assert_eq!(first.published.map(At::secs), Some(1_795_506_845));
    assert!(first.notes.starts_with("### Highlights"));
    assert_eq!(first.files[0].name.as_str(), "install");
    assert_eq!(first.files[0].size, 7213);
    assert_eq!(
        first.link.as_str(),
        "https://github.com/acme/widget/releases/tag/v1.4.1"
    );

    let latest = h.latest_release().unwrap().unwrap();
    assert_eq!(latest.tag, tag("v1.4.1"));

    let tags = h.tags(ALL_MAX).unwrap();
    let got: Vec<_> = tags
        .items()
        .iter()
        .map(|t| (t.name.as_str(), t.annotated, t.at))
        .collect();
    assert_eq!(
        got,
        [
            ("v1.4.1", true, None),
            ("v1.4.0", true, None),
            ("v0.6.0", false, None)
        ]
    );
    assert_eq!(tags.items()[0].commit, CommitId::new(V141).unwrap());

    let resolved = h.resolve_tag(&tag("v1.4.1")).unwrap();
    assert_eq!(resolved, CommitId::new(V141).unwrap(), "peeled");

    assert_eq!(
        stub.asked(),
        [
            "repos/acme/widget/releases?per_page=100",
            "repositories/1/releases?per_page=100&page=2",
            "repos/acme/widget/releases/latest",
            "repos/acme/widget/tags?per_page=100",
            "repos/acme/widget/git/matching-refs/tags/?per_page=100",
            "repos/acme/widget/git/ref/tags/v1.4.1",
            "repos/acme/widget/git/tags/3763a98b53f3cb1a8a9fd075389403f393770eec",
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
        Some((51, 1_795_536_548))
    );
    let row = site
        .meter()
        .into_iter()
        .find(|r| r.caller == CALLER && r.pool == Pool::Calls)
        .unwrap();
    assert_eq!(row.calls, 7);
}

#[test]
fn an_anonymous_write_handle_is_forbidden_and_makes_no_request() {
    let stub = Stub::new("github-anonymous-writes");
    let site = site("acme/widget", &stub, Arc::new(MemoryLedger::new()));
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
    let site = site("acme/widget", &stub, Arc::new(MemoryLedger::new()));
    let by = Actor::Anonymous;
    assert_eq!(site.tickets(&by, CALLER).err(), Some(UNSUPPORTED));
    assert_eq!(site.blueprints(&by, CALLER).err(), Some(UNSUPPORTED));
    assert_eq!(site.identity(&by, CALLER).err(), Some(UNSUPPORTED));
    assert_eq!(site.board(&by, CALLER).err(), Some(UNSUPPORTED));
    let mut h = site.code_host(&by, CALLER).unwrap();
    assert_eq!(h.facts().err(), Some(UNSUPPORTED));
    assert_eq!(h.release(&tag("v1.4.1")).err(), Some(UNSUPPORTED));
    let mut into = Vec::new();
    let file = tehas_ports::FileName::new("install").unwrap();
    assert_eq!(
        h.download(&tag("v1.4.1"), &file, 1 << 20, &mut into).err(),
        Some(UNSUPPORTED)
    );
    let at = CommitId::new(V141).unwrap();
    assert_eq!(h.archive(&at, 1 << 20, &mut into).err(), Some(UNSUPPORTED));
    assert_eq!(
        h.remote(tehas_ports::Access::Fetch).err(),
        Some(UNSUPPORTED)
    );
    assert!(stub.asked().is_empty());
}

#[test]
fn a_site_not_given_curl_answers_anonymous_reads_unsupported() {
    let site = GitHubSite::new("acme/widget", "tehas-bot[bot]", Some(900), no_gh());
    assert_eq!(
        site.code_host(&Actor::Anonymous, CALLER).err(),
        Some(UNSUPPORTED)
    );
}

#[test]
fn a_private_or_missing_project_is_refused_never_not_found() {
    let stub = Stub::new("github-anonymous-private");
    let site = site("acme/private", &stub, Arc::new(MemoryLedger::new()));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(h.releases(ALL_MAX).err(), Some(UNSUPPORTED));
    assert_eq!(h.tags(ALL_MAX).err(), Some(UNSUPPORTED));
    assert_eq!(h.latest_release().err(), Some(UNSUPPORTED));
    assert_eq!(h.resolve_tag(&tag("v1.4.1")).err(), Some(UNSUPPORTED));
}

#[test]
fn a_public_project_with_no_release_or_no_such_tag_says_so() {
    let stub = Stub::new("github-anonymous-none");
    let site = site("acme/empty", &stub, Arc::new(MemoryLedger::new()));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(h.latest_release(), Ok(None));
    let site = self::site("acme/widget", &stub, Arc::new(MemoryLedger::new()));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(h.resolve_tag(&tag("v9.9.9")).err(), Some(Outcome::NotFound));
}

#[test]
fn a_spent_anonymous_budget_is_kept_and_the_next_call_makes_no_request() {
    let stub = Stub::new("github-anonymous-spent");
    let ledger = Arc::new(MemoryLedger::new());
    let site = site("acme/spent", &stub, Arc::clone(&ledger));
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    let spent = Outcome::Spent {
        pool: Pool::Calls,
        until: At::new(1_795_536_548),
    };
    assert_eq!(h.releases(ALL_MAX).err(), Some(spent.clone()));
    let host = HostId::new("github.com");
    let whose = Whose::Pool(anonymous(), Pool::Calls);
    assert_eq!(
        ledger.health(&host, &whose).unwrap(),
        HealthState::Spent {
            pool: Pool::Calls,
            until: At::new(1_795_536_548)
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
    let site = site("acme/widget", &stub, Arc::new(MemoryLedger::new()));
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
    let site = GitHubSite::new("acme/widget", "tehas-bot[bot]", Some(900), no_gh())
        .with_anonymous_curl(
            Curl::at(stub.program()).capped(64),
            Arc::new(MemoryLedger::new()),
        )
        .on_clock(|| 1_795_530_000);
    let mut h = site.code_host(&Actor::Anonymous, CALLER).unwrap();
    assert_eq!(
        h.latest_release().err(),
        Some(Outcome::Unread(tehas_ports::Unread::TooLarge))
    );
}

/// curl's arguments, whole: GET only, `https` only, no redirect followed
/// by curl, its own time limit and size cap, two fixed headers.
#[test]
fn curls_arguments_are_pinned() {
    let argv = Curl::new()
        .argv(
            "repos/acme/widget/releases/latest",
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
            "https://api.github.com/repos/acme/widget/releases/latest",
        ]
    );
    assert_eq!(CURL_ENV, [("PATH", "/usr/bin:/bin"), ("LC_ALL", "C")]);
}
