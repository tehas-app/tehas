//! `Actor::Anonymous`'s `download` of a public release file on GitHub
//! (Decided 76): the release read through the API, the file asked by its
//! id, at most two redirects followed, each to exactly one of
//! `DOWNLOAD_HOSTS` over `https`, the listed size and the byte stream both
//! held to the limit, the whole call bound in time, no credential in any
//! request, and the adapter's file gone whatever happens. curl is a stub
//! that answers each address from a route the test writes, else a 404, and
//! keeps the arguments and the environment it was given. No test reaches
//! the network.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tehas_core::testdir::TestDir;
use tehas_github::{Curl, GitHubSite, MakeRunner, CURL_ENV, DOWNLOAD_HOSTS, DOWNLOAD_LIMIT};
use tehas_ports::fake::MemoryLedger;
use tehas_ports::{
    Actor, At, Caller, CodeHost, CommitId, FileName, Outcome, Passing, Refusal, Sent, Site,
    TagName, Unread,
};

const CALLER: Caller = Caller::new("test.anonymous-download");
const UNSUPPORTED: Outcome = Outcome::Refused(Refusal::Unsupported);
const NOW: i64 = 1_795_530_000;

const RELEASE: &str = "https://api.github.com/repos/acme/widget/releases/tags/v1.4.1";
const ASSET: &str = "https://api.github.com/repos/acme/widget/releases/assets/11";
const PAGE: &str = "https://github.com/acme/widget/releases/download/v1.4.1/install";
const OBJECT: &str = "https://objects.githubusercontent.com/github-production-release-asset/1/11?X-Amz-Signature=abc&response-content-disposition=attachment%3B+filename%3Dinstall";

/// The file the release lists as `install`, 98 bytes.
fn install() -> Vec<u8> {
    (0..98u8).collect()
}

/// A stub curl in a test directory: it keeps each call's arguments in
/// `argv` and its environment in `env`, one call a block ended by `--`,
/// and answers the address it is given from `routes/<address>`, a route
/// with `.sleep` beside it by never answering, and exits with the code in
/// a route's `.exit`.
struct Stub {
    dir: TestDir,
    _serial: MutexGuard<'static, ()>,
}

/// One test at a time: a script just written cannot be started while
/// another test's fork still holds it open for writing, and each test
/// counts the adapter's directories left in the temporary directory.
static SERIAL: Mutex<()> = Mutex::new(());

impl Stub {
    fn new(name: &str) -> Stub {
        let serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let dir = TestDir::new(name);
        std::fs::create_dir(dir.join("routes")).unwrap();
        let script = format!(
            r#"#!/bin/sh
d='{dir}'
for a in "$@"; do printf '%s\n' "$a"; done >> "$d/argv"
printf -- '--\n' >> "$d/argv"
tr '\0' '\n' < /proc/$$/environ >> "$d/env"
printf -- '--\n' >> "$d/env"
for a in "$@"; do url=$a; done
f="$d/routes/$(printf '%s' "${{url#https://}}" | tr -c 'A-Za-z0-9' '_')"
if [ -f "$f.sleep" ]; then exec sleep 30; fi
if [ -f "$f" ]; then cat "$f"; else printf 'HTTP/2 404 \r\n\r\n{{"message":"Not Found"}}'; fi
if [ -f "$f.exit" ]; then exit "$(cat "$f.exit")"; fi
"#,
            dir = dir.path().display(),
        );
        let program = dir.join("curl");
        std::fs::write(&program, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let stub = Stub {
            dir,
            _serial: serial,
        };
        stub.answer(
            RELEASE,
            200,
            &[],
            br#"{"id":7,"tag_name":"v1.4.1","name":"v1.4.1","draft":false,"prerelease":false,
"published_at":"2026-11-24T07:54:05Z","body":"","html_url":"https://github.com/acme/widget/releases/tag/v1.4.1",
"assets":[{"id":11,"name":"install","size":98},{"id":12,"name":"big","size":5000}]}"#,
        );
        stub
    }

    fn route(&self, address: &str) -> PathBuf {
        let name: String = address
            .strip_prefix("https://")
            .unwrap_or(address)
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        self.dir.join("routes").join(name)
    }

    /// `address` answers `status` with `headers` and `body`.
    fn answer(&self, address: &str, status: u16, headers: &[(&str, &str)], body: &[u8]) {
        let mut bytes = format!("HTTP/2 {status} \r\n").into_bytes();
        for (k, v) in headers {
            bytes.extend(format!("{k}: {v}\r\n").bytes());
        }
        bytes.extend(b"\r\n");
        bytes.extend(body);
        std::fs::write(self.route(address), bytes).unwrap();
    }

    /// `address` redirects to `to` with a 302.
    fn redirect(&self, address: &str, to: &str) {
        self.answer(address, 302, &[("location", to)], b"");
    }

    /// curl given `address` exits with `code` after its answer.
    fn exit(&self, address: &str, code: u8) {
        let mut path = self.route(address).into_os_string();
        path.push(".exit");
        std::fs::write(path, code.to_string()).unwrap();
    }

    /// curl given `address` never answers.
    fn hang(&self, address: &str) {
        let mut path = self.route(address).into_os_string();
        path.push(".sleep");
        std::fs::write(path, "").unwrap();
    }

    fn program(&self) -> PathBuf {
        self.dir.join("curl")
    }

    fn calls(&self, file: &str) -> Vec<Vec<String>> {
        let text = std::fs::read_to_string(self.dir.join(file)).unwrap_or_default();
        text.split_terminator("--\n")
            .map(|c| c.lines().map(String::from).collect())
            .collect()
    }

    /// The address each call asked, in order.
    fn asked(&self) -> Vec<String> {
        self.calls("argv")
            .iter()
            .map(|c| c.last().unwrap().clone())
            .collect()
    }

    /// The `--max-time` each call was given.
    fn max_times(&self) -> Vec<String> {
        self.calls("argv")
            .iter()
            .map(|c| {
                let at = c.iter().position(|a| a == "--max-time").unwrap();
                c[at + 1].clone()
            })
            .collect()
    }
}

/// A gh that must never run: an anonymous read goes through curl alone.
fn no_gh() -> MakeRunner {
    Arc::new(|by: &Actor| panic!("a runner of gh was asked for {by:?}"))
}

fn site(stub: &Stub) -> GitHubSite {
    GitHubSite::new("acme/widget", "tehas-bot[bot]", Some(900), no_gh())
        .with_anonymous_curl(Curl::at(stub.program()), Arc::new(MemoryLedger::new()))
        .on_clock(|| NOW)
}

fn handle(site: &dyn Site) -> Box<dyn CodeHost> {
    site.code_host(&Actor::Anonymous, CALLER).unwrap()
}

fn tag() -> TagName {
    TagName::new("v1.4.1").unwrap()
}

fn name(n: &str) -> FileName {
    FileName::new(n).unwrap()
}

/// `install` downloaded within `limit`, and what was written.
fn download(h: &mut dyn CodeHost, file: &str, limit: u64) -> (Result<(), Outcome>, Vec<u8>) {
    let mut into = Vec::new();
    let got = h.download(&tag(), &name(file), limit, &mut into);
    (got, into)
}

/// The adapter's download directories of this process left in the
/// temporary directory.
fn left_behind() -> Vec<PathBuf> {
    let prefix = format!("tehas-github-download-{}-", std::process::id());
    std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .map(|e| e.path())
        .collect()
}

/// No request carries a credential: curl's environment is exactly
/// `CURL_ENV`, `-q` comes first, and no argument is a header, option or
/// value that could carry one, or makes curl follow a redirect itself.
fn assert_no_credential(stub: &Stub) {
    let mut want: Vec<String> = CURL_ENV.iter().map(|(k, v)| format!("{k}={v}")).collect();
    want.sort();
    let envs = stub.calls("env");
    assert!(!envs.is_empty());
    for mut env in envs {
        env.sort();
        assert_eq!(env, want, "curl's environment, whole");
        assert!(env
            .iter()
            .all(|e| !e.to_ascii_lowercase().contains("token")));
    }
    for argv in stub.calls("argv") {
        assert_eq!(argv[0], "-q", "no .curlrc is read");
        for a in &argv {
            let lower = a.to_ascii_lowercase();
            for word in ["authorization", "token", "cookie", "bearer"] {
                assert!(!lower.contains(word), "{a}");
            }
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
                    "--cookie",
                    "-L",
                    "--location",
                    "--location-trusted",
                    "-H",
                    "--oauth2-bearer",
                    "--aws-sigv4",
                ]
                .contains(&a.as_str()),
                "{a}"
            );
        }
        let address = argv.last().unwrap();
        assert!(
            DOWNLOAD_HOSTS
                .iter()
                .any(|h| address.starts_with(&format!("https://{h}/"))),
            "{address}"
        );
    }
}

fn set_tokens() {
    for key in ["GH_TOKEN", "GITHUB_TOKEN", "TEHAS_GH_TOKEN"] {
        std::env::set_var(key, "a-token-that-must-not-travel");
    }
}

#[test]
fn the_hosts_are_exactly_three() {
    assert_eq!(
        DOWNLOAD_HOSTS,
        [
            "api.github.com",
            "github.com",
            "objects.githubusercontent.com"
        ]
    );
    assert_eq!(DOWNLOAD_LIMIT, Duration::from_secs(600));
}

#[test]
fn a_file_served_by_the_api_host_itself_is_downloaded() {
    set_tokens();
    let stub = Stub::new("github-anon-dl-api");
    stub.answer(ASSET, 200, &[], &install());
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), "install", 1 << 20);
    assert_eq!(got, Ok(()));
    assert_eq!(into, install());
    assert_eq!(stub.asked(), [RELEASE, ASSET]);
    assert_no_credential(&stub);
    assert!(left_behind().is_empty());
}

#[test]
fn a_redirect_to_github_com_then_to_the_objects_host_is_followed() {
    set_tokens();
    let stub = Stub::new("github-anon-dl-two");
    stub.redirect(ASSET, PAGE);
    stub.redirect(PAGE, OBJECT);
    stub.answer(OBJECT, 200, &[], &install());
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), "install", 1 << 20);
    assert_eq!(got, Ok(()));
    assert_eq!(into, install());
    assert_eq!(stub.asked(), [RELEASE, ASSET, PAGE, OBJECT]);
    assert_no_credential(&stub);
    assert!(left_behind().is_empty());
    // Each request is metered, the redirects among them.
    let row = site
        .meter()
        .into_iter()
        .find(|r| r.caller == CALLER)
        .unwrap();
    assert_eq!(row.calls, 4);
}

#[test]
fn a_redirect_within_the_api_host_is_followed() {
    let stub = Stub::new("github-anon-dl-api-redirect");
    let moved = "https://api.github.com/repositories/1/releases/assets/11";
    stub.redirect(ASSET, moved);
    stub.answer(moved, 200, &[], &install());
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), "install", 1 << 20);
    assert_eq!(got, Ok(()));
    assert_eq!(into, install());
    assert_eq!(stub.asked(), [RELEASE, ASSET, moved]);
    assert_no_credential(&stub);
}

#[test]
fn a_third_redirect_is_refused_with_nothing_written() {
    let stub = Stub::new("github-anon-dl-three");
    let again = "https://objects.githubusercontent.com/again";
    stub.redirect(ASSET, PAGE);
    stub.redirect(PAGE, OBJECT);
    stub.redirect(OBJECT, again);
    stub.answer(again, 200, &[], &install());
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), "install", 1 << 20);
    assert_eq!(got, Err(UNSUPPORTED));
    assert!(into.is_empty());
    assert_eq!(stub.asked(), [RELEASE, ASSET, PAGE, OBJECT], "never asked");
    assert!(left_behind().is_empty());
}

#[test]
fn a_redirect_to_any_other_host_is_refused_with_nothing_written() {
    for (n, to) in [
        "https://evil.example/install",
        "https://objects.githubusercontent.com.evil.example/install",
        "https://github.com.evil.example/install",
        "https://api.github.com.evil.example/install",
        "https://evilgithub.com/install",
        "https://evil.objects.githubusercontent.com/install",
        "https://githubusercontent.com/install",
        "https://github.com:443/install",
        "https://github.com@evil.example/install",
        "https://github.com\\@evil.example/install",
        "https://GITHUB.COM/install",
        "https://github.com",
        "https://github.com/a/../../install",
        "https://github.com/install#x",
        "//evil.example/install",
        "/acme/widget/install",
        "",
    ]
    .iter()
    .enumerate()
    {
        let stub = Stub::new(&format!("github-anon-dl-host-{n}"));
        stub.redirect(ASSET, to);
        if to.starts_with("https://") {
            stub.answer(to, 200, &[], &install());
        }
        let site = site(&stub);
        let (got, into) = download(handle(&site).as_mut(), "install", 1 << 20);
        assert_eq!(got, Err(UNSUPPORTED), "{to:?}");
        assert!(into.is_empty(), "{to:?}");
        assert_eq!(stub.asked(), [RELEASE, ASSET], "{to:?} never asked");
        assert!(left_behind().is_empty(), "{to:?}");
    }
}

#[test]
fn a_redirect_to_http_is_refused_with_nothing_written() {
    for (n, to) in [
        "http://objects.githubusercontent.com/install",
        "http://github.com/acme/widget/releases/download/v1.4.1/install",
        "ftp://github.com/install",
    ]
    .iter()
    .enumerate()
    {
        let stub = Stub::new(&format!("github-anon-dl-http-{n}"));
        stub.redirect(ASSET, to);
        let site = site(&stub);
        let (got, into) = download(handle(&site).as_mut(), "install", 1 << 20);
        assert_eq!(got, Err(UNSUPPORTED), "{to}");
        assert!(into.is_empty());
        assert_eq!(stub.asked(), [RELEASE, ASSET]);
    }
    // And curl itself is held to https for the one address it is given.
    let argv = Curl::new()
        .fetch_argv(OBJECT, Duration::from_secs(600), 1 << 20)
        .unwrap();
    assert!(argv.windows(2).any(|w| w == ["--proto", "=https"]));
    assert!(argv.windows(2).any(|w| w == ["--max-redirs", "0"]));
}

#[test]
fn a_listed_size_over_the_limit_is_refused_before_the_download() {
    let stub = Stub::new("github-anon-dl-listed");
    stub.answer(ASSET, 200, &[], &install());
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), "install", 97);
    assert_eq!(got, Err(Outcome::Unread(Unread::TooLarge)));
    assert!(into.is_empty());
    assert_eq!(stub.asked(), [RELEASE], "no download");
    assert!(left_behind().is_empty());
}

#[test]
fn a_byte_stream_over_the_limit_is_cut_and_refused_with_the_file_gone() {
    let stub = Stub::new("github-anon-dl-stream");
    // Listed at 98, under the limit of 100, but served at 200 000.
    stub.redirect(ASSET, OBJECT);
    stub.answer(OBJECT, 200, &[], &vec![b'x'; 200_000]);
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), "install", 100);
    assert_eq!(got, Err(Outcome::Unread(Unread::TooLarge)));
    assert!(into.is_empty());
    assert!(left_behind().is_empty());
    // curl is given the same cap.
    let argv = stub.calls("argv").pop().unwrap();
    assert!(argv.windows(2).any(|w| w == ["--max-filesize", "100"]));
}

#[test]
fn a_file_served_at_another_size_than_listed_is_inconsistent() {
    let stub = Stub::new("github-anon-dl-short");
    stub.answer(ASSET, 200, &[], &install()[..97]);
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), "install", 1 << 20);
    assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)));
    assert!(into.is_empty());
    assert!(left_behind().is_empty());
}

#[test]
fn a_file_the_release_does_not_list_or_a_missing_release_is_not_found() {
    let stub = Stub::new("github-anon-dl-missing");
    stub.answer("https://api.github.com/repos/acme/widget", 200, &[], b"{}");
    let site = site(&stub);
    let mut h = handle(&site);
    let (got, _) = download(h.as_mut(), "other", 1 << 20);
    assert_eq!(got, Err(Outcome::NotFound));
    assert_eq!(stub.asked(), [RELEASE], "no download");
    let mut into = Vec::new();
    let got = h.download(
        &TagName::new("v9.9.9").unwrap(),
        &name("install"),
        1,
        &mut into,
    );
    assert_eq!(got, Err(Outcome::NotFound));
}

#[test]
fn a_private_or_missing_project_is_refused() {
    let stub = Stub::new("github-anon-dl-private");
    let site = GitHubSite::new("acme/private", "tehas-bot[bot]", Some(900), no_gh())
        .with_anonymous_curl(Curl::at(stub.program()), Arc::new(MemoryLedger::new()))
        .on_clock(|| NOW);
    let (got, _) = download(handle(&site).as_mut(), "install", 1 << 20);
    assert_eq!(got, Err(UNSUPPORTED));
}

#[test]
fn a_refused_fetch_leaves_nothing() {
    let stub = Stub::new("github-anon-dl-forbidden");
    stub.redirect(ASSET, OBJECT);
    stub.answer(OBJECT, 403, &[], b"<Error>expired</Error>");
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), "install", 1 << 20);
    assert_eq!(got, Err(Outcome::Forbidden));
    assert!(into.is_empty());
    assert!(left_behind().is_empty());
}

#[test]
fn archive_stays_unsupported_with_no_request() {
    let stub = Stub::new("github-anon-dl-archive");
    let site = site(&stub);
    let at = CommitId::new("1641015e605f6dcd998166bb3857712e335db7f1").unwrap();
    let mut into = Vec::new();
    assert_eq!(
        handle(&site).archive(&at, 1 << 20, &mut into).err(),
        Some(UNSUPPORTED)
    );
    assert!(stub.asked().is_empty());
}

/// A download's time limit is at most 600 s; the reads keep their 30 s.
#[test]
fn the_download_is_bound_at_600_seconds_and_the_reads_at_30() {
    let curl = Curl::new();
    let argv = curl
        .fetch_argv(OBJECT, Duration::from_secs(3_600), 1 << 20)
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
            "600",
            "--max-filesize",
            "1048576",
            "--header",
            "Accept: application/octet-stream",
            "--header",
            "X-GitHub-Api-Version: 2022-11-28",
            "--user-agent",
            "tehas",
            OBJECT,
        ]
    );
    let read = curl
        .argv(
            "repos/acme/widget/releases/latest",
            Duration::from_secs(3_600),
        )
        .unwrap();
    assert!(read.windows(2).any(|w| w == ["--max-time", "30"]));

    // Through a handle: the release read at 30 s, each fetch at 600 s.
    let stub = Stub::new("github-anon-dl-bound");
    stub.redirect(ASSET, OBJECT);
    stub.answer(OBJECT, 200, &[], &install());
    let site = site(&stub);
    assert_eq!(
        download(handle(&site).as_mut(), "install", 1 << 20).0,
        Ok(())
    );
    // The 600 s are the whole call's: each hop is given what is left, in
    // whole seconds.
    let times = stub.max_times();
    assert_eq!(times[0], "30");
    assert_eq!(times.len(), 3);
    for t in &times[1..] {
        let secs: u64 = t.parse().unwrap();
        assert!((590..=600).contains(&secs), "{t}");
    }
}

#[test]
fn a_deadline_nearer_than_600_seconds_bounds_the_download() {
    let stub = Stub::new("github-anon-dl-deadline");
    stub.answer(ASSET, 200, &[], &install());
    let site = site(&stub);
    let view = site.until(At::new(NOW + 7));
    assert_eq!(
        download(handle(view.as_ref()).as_mut(), "install", 1 << 20).0,
        Ok(())
    );
    assert_eq!(stub.max_times(), ["7", "7"]);
}

#[test]
fn a_fetch_curl_says_timed_out_is_a_timeout_with_the_file_gone() {
    let stub = Stub::new("github-anon-dl-28");
    stub.answer(ASSET, 200, &[], &install()[..10]);
    stub.exit(ASSET, 28);
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), "install", 1 << 20);
    assert_eq!(
        got,
        Err(Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::Maybe
        })
    );
    assert!(into.is_empty());
    assert!(left_behind().is_empty());
}

#[test]
fn a_fetch_that_never_answers_is_stopped_at_its_bound() {
    let stub = Stub::new("github-anon-dl-hang");
    stub.hang(ASSET);
    let site = site(&stub);
    let view = site.until(At::new(NOW + 1));
    let started = Instant::now();
    let (got, into) = download(handle(view.as_ref()).as_mut(), "install", 1 << 20);
    assert_eq!(
        got,
        Err(Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::Maybe
        })
    );
    assert!(started.elapsed() < Duration::from_secs(20), "stopped");
    assert!(into.is_empty());
    assert!(left_behind().is_empty());
}

#[test]
fn past_the_deadline_a_download_makes_no_request() {
    let stub = Stub::new("github-anon-dl-past");
    let site = site(&stub);
    let view = site.until(At::new(NOW - 1));
    let (got, _) = download(handle(view.as_ref()).as_mut(), "install", 1 << 20);
    assert_eq!(
        got,
        Err(Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::No
        })
    );
    assert!(stub.asked().is_empty());
}
