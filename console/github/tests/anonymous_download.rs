//! `Actor::Anonymous`'s `download` of a public release file on GitHub
//! (Decided 76): the release read through the API, the file asked by its
//! id, at most two redirects followed, each to exactly one of
//! `DOWNLOAD_HOSTS` over `https`, the listed size and the byte stream both
//! held to the limit, the whole call bound in time, no credential in any
//! request, and the adapter's file gone whatever happens. curl is a stub
//! that answers each address from a route the test writes, else a 404, and
//! keeps the arguments and the environment it was given. The release and
//! the project are the live answers in `tests/recorded/anonymous/`; the
//! file's bytes and the other hosts' answers are the test's own. No test
//! reaches the network.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tehas_core::testdir::TestDir;
use tehas_github::{Curl, GitHubSite, MakeRunner, CURL_ENV, DOWNLOAD_HOSTS, DOWNLOAD_LIMIT};
use tehas_ports::fake::MemoryLedger;
use tehas_ports::{
    Actor, At, Caller, CodeHost, CommitId, FileName, HealthState, Holder, HolderId, HostId, Ledger,
    Outcome, Passing, Pool, Refusal, Sent, Site, TagName, Unread, Whose,
};

const CALLER: Caller = Caller::new("test.anonymous-download");
const UNSUPPORTED: Outcome = Outcome::Refused(Refusal::Unsupported);
const NOW: i64 = 1_795_530_000;

const REPO: &str = "tehas-app/tehas-rec-releases";
const PROJECT: &str = "https://api.github.com/repos/tehas-app/tehas-rec-releases";
const RELEASE: &str =
    "https://api.github.com/repos/tehas-app/tehas-rec-releases/releases/tags/v1.1.0";
const ASSET: &str =
    "https://api.github.com/repos/tehas-app/tehas-rec-releases/releases/assets/796197977";
const PAGE: &str = "https://github.com/tehas-app/tehas-rec-releases/releases/download/v1.1.0/tehas-v1.1.0-linux-x86_64.tar.gz";
/// The file the recorded release lists, 232 bytes.
const FILE: &str = "tehas-v1.1.0-linux-x86_64.tar.gz";
/// The shape the API's redirect for an asset by id has today: seen on an
/// anonymous read of a public release on 2026-11-24, its values made up.
const RELEASE_ASSET: &str = "https://release-assets.githubusercontent.com/github-production-release-asset/1/0d3f6b52-2c1e-4f7a-9d8e-1a2b3c4d5e6f?sp=r&sv=2018-11-09&sr=b&spr=https&se=2026-11-24T12%3A05%3A00Z&rscd=attachment%3B+filename%3Dinstall&rsct=application%2Foctet-stream&skoid=a1&sktid=b2&skt=2026-11-24T11%3A05%3A00Z&ske=2026-11-24T12%3A05%3A00Z&sks=b&skv=2018-11-09&sig=c2lnbmF0dXJl%3D&jwt=eyJhbGciOiJIUzI1NiJ9.eyJleHAiOjF9.c2ln&response-content-disposition=attachment%3B%20filename%3Dinstall&response-content-type=application%2Foctet-stream";
const OBJECT: &str = "https://objects.githubusercontent.com/github-production-release-asset/1/11?X-Amz-Signature=abc&response-content-disposition=attachment%3B+filename%3Dinstall";

/// The bytes served for `FILE`, as many as the release lists.
fn install() -> Vec<u8> {
    (0..232u8).collect()
}

/// A stub curl in a test directory: it keeps each call's arguments in
/// `argv` and its environment in `env`, one call a block ended by `--`,
/// and answers the address it is given from `routes/<address>` (its first
/// 200 characters, under a file name's limit), a route
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
f="$d/routes/$(printf '%s' "${{url#https://}}" | tr -c 'A-Za-z0-9' '_' | cut -c1-200)"
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
        stub.answer_from(
            RELEASE,
            "recorded/anonymous/repos_tehas_app_tehas_rec_releases_releases_tags_v1_1_0",
        );
        stub
    }

    /// `address` answers with the file `answer` under `tests/`.
    fn answer_from(&self, address: &str, answer: &str) {
        let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        std::fs::copy(tests.join(answer), self.route(address)).unwrap();
    }

    fn route(&self, address: &str) -> PathBuf {
        let name: String = address
            .strip_prefix("https://")
            .unwrap_or(address)
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .take(200)
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
    site_with(stub, Arc::new(MemoryLedger::new()))
}

fn site_with(stub: &Stub, ledger: Arc<MemoryLedger>) -> GitHubSite {
    GitHubSite::new(REPO, "tehas-app[bot]", Some(900), no_gh())
        .with_anonymous_curl(Curl::at(stub.program()), ledger)
        .on_clock(|| NOW)
}

fn handle(site: &dyn Site) -> Box<dyn CodeHost> {
    site.code_host(&Actor::Anonymous, CALLER).unwrap()
}

fn tag() -> TagName {
    TagName::new("v1.1.0").unwrap()
}

fn name(n: &str) -> FileName {
    FileName::new(n).unwrap()
}

/// `file` downloaded within `limit`, and what was written.
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
/// `CURL_ENV` (the test process's own, with its `PATH` and `HOME`, is
/// cleared; no test sets a variable, which would race the other threads),
/// `-q` comes first, and no argument is a header, option or
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

#[test]
fn the_hosts_are_exactly_four() {
    assert_eq!(
        DOWNLOAD_HOSTS,
        [
            "api.github.com",
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com",
        ]
    );
    assert_eq!(DOWNLOAD_LIMIT, Duration::from_secs(600));
}

#[test]
fn a_file_served_by_the_api_host_itself_is_downloaded() {
    let stub = Stub::new("github-anon-dl-api");
    stub.answer(ASSET, 200, &[], &install());
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
    assert_eq!(got, Ok(()));
    assert_eq!(into, install());
    assert_eq!(stub.asked(), [RELEASE, ASSET]);
    assert_no_credential(&stub);
    assert!(left_behind().is_empty());
}

#[test]
fn a_redirect_to_github_com_then_to_the_objects_host_is_followed() {
    let stub = Stub::new("github-anon-dl-two");
    stub.redirect(ASSET, PAGE);
    stub.redirect(PAGE, OBJECT);
    stub.answer(OBJECT, 200, &[], &install());
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
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
    let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
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
    let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
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
        "https://evil.release-assets.githubusercontent.com/install",
        "https://release-assets.githubusercontent.com.evil.test/install",
        "https://githubusercontent.com/install",
        "https://github.com:443/install",
        "https://github.com@evil.example/install",
        "https://github.com\\@evil.example/install",
        "https://GITHUB.COM/install",
        "https://github.com",
        "https://github.com/a/../../install",
        "https://github.com/install#x",
        "//evil.example/install",
        "/tehas-app/tehas-rec-releases/install",
        "https://github.com/a%2F..%2F..%2Finstall",
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
        let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
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
        "http://github.com/tehas-app/tehas-rec-releases/releases/download/v1.1.0/tehas-v1.1.0-linux-x86_64.tar.gz",
        "ftp://github.com/install",
    ]
    .iter()
    .enumerate()
    {
        let stub = Stub::new(&format!("github-anon-dl-http-{n}"));
        stub.redirect(ASSET, to);
        let site = site(&stub);
        let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
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
    let (got, into) = download(handle(&site).as_mut(), FILE, 231);
    assert_eq!(got, Err(Outcome::Unread(Unread::TooLarge)));
    assert!(into.is_empty());
    assert_eq!(stub.asked(), [RELEASE], "no download");
    assert!(left_behind().is_empty());
}

#[test]
fn a_byte_stream_over_the_limit_is_cut_and_refused_with_the_file_gone() {
    let stub = Stub::new("github-anon-dl-stream");
    // Listed at 232, under the limit of 300, but served at 200 000.
    stub.redirect(ASSET, OBJECT);
    stub.answer(OBJECT, 200, &[], &vec![b'x'; 200_000]);
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), FILE, 300);
    assert_eq!(got, Err(Outcome::Unread(Unread::TooLarge)));
    assert!(into.is_empty());
    assert!(left_behind().is_empty());
    // curl is given the same cap.
    let argv = stub.calls("argv").pop().unwrap();
    assert!(argv.windows(2).any(|w| w == ["--max-filesize", "300"]));
}

#[test]
fn a_file_served_at_another_size_than_listed_is_inconsistent() {
    let stub = Stub::new("github-anon-dl-short");
    stub.answer(ASSET, 200, &[], &install()[..231]);
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
    assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)));
    assert!(into.is_empty());
    assert!(left_behind().is_empty());
}

#[test]
fn a_file_the_release_does_not_list_or_a_missing_release_is_not_found() {
    let stub = Stub::new("github-anon-dl-missing");
    stub.answer_from(
        PROJECT,
        "recorded/anonymous/repos_tehas_app_tehas_rec_releases",
    );
    let site = site(&stub);
    let mut h = handle(&site);
    let (got, _) = download(h.as_mut(), "other", 1 << 20);
    assert_eq!(got, Err(Outcome::NotFound));
    assert_eq!(stub.asked(), [RELEASE], "no download");
    let mut into = Vec::new();
    let got = h.download(&TagName::new("v9.9.9").unwrap(), &name(FILE), 1, &mut into);
    assert_eq!(got, Err(Outcome::NotFound));
}

#[test]
fn a_private_or_missing_project_is_refused() {
    let stub = Stub::new("github-anon-dl-private");
    let site = GitHubSite::new(
        "tehas-app/tehas-rec-gone",
        "tehas-app[bot]",
        Some(900),
        no_gh(),
    )
    .with_anonymous_curl(Curl::at(stub.program()), Arc::new(MemoryLedger::new()))
    .on_clock(|| NOW);
    let (got, _) = download(handle(&site).as_mut(), FILE, 1 << 20);
    assert_eq!(got, Err(UNSUPPORTED));
}

#[test]
fn a_refused_fetch_leaves_nothing() {
    let stub = Stub::new("github-anon-dl-forbidden");
    stub.redirect(ASSET, OBJECT);
    stub.answer(OBJECT, 403, &[], b"<Error>expired</Error>");
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
    assert_eq!(got, Err(Outcome::Forbidden));
    assert!(into.is_empty());
    assert!(left_behind().is_empty());
}

#[test]
fn archive_stays_unsupported_with_no_request() {
    let stub = Stub::new("github-anon-dl-archive");
    let site = site(&stub);
    let at = CommitId::new("f703bfa5b38d4088f9235eed9fde20a16370e0ee").unwrap();
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
            "repos/tehas-app/tehas-rec-releases/releases/latest",
            Duration::from_secs(3_600),
        )
        .unwrap();
    assert!(read.windows(2).any(|w| w == ["--max-time", "30"]));

    // Through a handle: the release read at 30 s, each fetch at 600 s.
    let stub = Stub::new("github-anon-dl-bound");
    stub.redirect(ASSET, OBJECT);
    stub.answer(OBJECT, 200, &[], &install());
    let site = site(&stub);
    assert_eq!(download(handle(&site).as_mut(), FILE, 1 << 20).0, Ok(()));
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
        download(handle(view.as_ref()).as_mut(), FILE, 1 << 20).0,
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
    let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
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
    let (got, into) = download(handle(view.as_ref()).as_mut(), FILE, 1 << 20);
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
    let (got, _) = download(handle(view.as_ref()).as_mut(), FILE, 1 << 20);
    assert_eq!(
        got,
        Err(Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::No
        })
    );
    assert!(stub.asked().is_empty());
}

#[test]
fn every_redirect_is_held_to_the_hosts_and_https_before_it_is_followed() {
    for (n, second) in [
        "https://evil.example/install",
        "https://objects.githubusercontent.com.evil.example/install",
        "http://objects.githubusercontent.com/install",
    ]
    .iter()
    .enumerate()
    {
        let stub = Stub::new(&format!("github-anon-dl-second-{n}"));
        stub.redirect(ASSET, PAGE);
        stub.redirect(PAGE, second);
        stub.answer(second, 200, &[], &install());
        let site = site(&stub);
        let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
        assert_eq!(got, Err(UNSUPPORTED), "{second}");
        assert!(into.is_empty());
        assert_eq!(stub.asked(), [RELEASE, ASSET, PAGE], "{second} never asked");
        assert!(left_behind().is_empty());
    }
}

#[test]
fn the_hop_to_the_objects_host_carries_no_credential() {
    let stub = Stub::new("github-anon-dl-cross");
    stub.redirect(ASSET, PAGE);
    stub.redirect(PAGE, OBJECT);
    stub.answer(OBJECT, 200, &[], &install());
    let site = site(&stub);
    assert_eq!(download(handle(&site).as_mut(), FILE, 1 << 20).0, Ok(()));
    let calls = stub.calls("argv");
    let envs = stub.calls("env");
    let at = calls
        .iter()
        .position(|c| c.last().map(String::as_str) == Some(OBJECT))
        .unwrap();
    let argv = &calls[at];
    assert_eq!(argv[0], "-q");
    for flag in ["-u", "--user", "-H", "-b", "--cookie", "-K", "--config"] {
        assert!(!argv.iter().any(|a| a == flag), "{flag}");
    }
    // Its only headers are the two fixed ones.
    let headers: Vec<_> = argv
        .windows(2)
        .filter(|w| w[0] == "--header")
        .map(|w| w[1].as_str())
        .collect();
    assert_eq!(
        headers,
        [
            "Accept: application/octet-stream",
            "X-GitHub-Api-Version: 2022-11-28"
        ]
    );
    let mut env = envs[at].clone();
    env.sort();
    let mut want: Vec<String> = CURL_ENV.iter().map(|(k, v)| format!("{k}={v}")).collect();
    want.sort();
    assert_eq!(env, want, "the environment cleared");
    assert_no_credential(&stub);
}

#[test]
fn the_cap_is_held_on_the_stream_not_on_the_content_length() {
    let stub = Stub::new("github-anon-dl-lying-length");
    // GitHub's length and the listing both say 232; the stream is longer.
    stub.redirect(ASSET, OBJECT);
    stub.answer(
        OBJECT,
        200,
        &[("content-length", "232")],
        &vec![b'x'; 200_000],
    );
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), FILE, 300);
    assert_eq!(got, Err(Outcome::Unread(Unread::TooLarge)));
    assert!(into.is_empty());
    assert!(left_behind().is_empty());
}

#[test]
fn a_redirect_with_no_location_is_malformed() {
    for (n, headers) in [vec![], vec![("location", "")]].into_iter().enumerate() {
        let stub = Stub::new(&format!("github-anon-dl-nowhere-{n}"));
        stub.answer(ASSET, 302, &headers, b"");
        let site = site(&stub);
        let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
        assert_eq!(got, Err(Outcome::Unread(Unread::Malformed)));
        assert!(into.is_empty());
        assert_eq!(stub.asked(), [RELEASE, ASSET]);
        assert!(left_behind().is_empty());
    }
}

/// A rate refusal from a host other than the API's is that call's failure
/// alone: the anonymous API budget is not spent, and the next download
/// still asks.
#[test]
fn a_retry_after_from_the_objects_host_does_not_spend_the_api_budget() {
    for (n, (status, headers)) in [
        (429, vec![("retry-after", "3600")]),
        (
            403,
            vec![
                ("retry-after", "3600"),
                ("x-ratelimit-remaining", "0"),
                ("x-ratelimit-reset", "1795599999"),
            ],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let stub = Stub::new(&format!("github-anon-dl-cdn-rate-{n}"));
        stub.redirect(ASSET, OBJECT);
        stub.answer(OBJECT, status, &headers, b"slow down");
        let ledger = Arc::new(MemoryLedger::new());
        let site = site_with(&stub, Arc::clone(&ledger));
        let mut h = handle(&site);
        let (got, into) = download(h.as_mut(), FILE, 1 << 20);
        assert_eq!(
            got,
            Err(Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::Maybe
            }),
            "{status}"
        );
        assert!(into.is_empty());
        let host = HostId::new("github.com");
        let anonymous = Holder::Anonymous(HolderId::new("anonymous"));
        let whose = Whose::Pool(anonymous.clone(), Pool::Calls);
        assert_eq!(ledger.health(&host, &whose).unwrap(), HealthState::Healthy);
        // The budget kept is the recorded release's, never the other host's.
        let kept = ledger.budget(&host, &anonymous, Pool::Calls).unwrap();
        assert_eq!(
            kept.map(|b| (b.left, b.resets.secs())),
            Some((32, 1_795_704_168))
        );
        // The API is still asked.
        let before = stub.asked().len();
        let _ = download(h.as_mut(), FILE, 1 << 20);
        assert!(stub.asked().len() > before, "not parked");
        assert!(left_behind().is_empty());
    }
}

/// A 2xx from the API that ignored `Accept` and sent the asset's JSON is
/// not parsed: it fails on its size. GitHub cannot be made to send it, so
/// the answer is synthetic.
#[test]
fn json_from_the_api_in_place_of_the_bytes_is_inconsistent() {
    let stub = Stub::new("github-anon-dl-json");
    stub.answer_from(
        ASSET,
        "synthetic/anonymous/json-asset/repos_tehas_app_tehas_rec_releases_releases_assets_796197977",
    );
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
    assert_eq!(got, Err(Outcome::Unread(Unread::Inconsistent)));
    assert!(into.is_empty());
    assert_eq!(stub.asked(), [RELEASE, ASSET], "never followed");
    assert!(left_behind().is_empty());
}

/// The API's redirect for an asset by id goes to the release assets host
/// today: one redirect, followed, with no credential on that hop.
#[test]
fn a_redirect_to_the_release_assets_host_is_followed() {
    let stub = Stub::new("github-anon-dl-release-assets");
    stub.redirect(ASSET, RELEASE_ASSET);
    stub.answer(RELEASE_ASSET, 200, &[], &install());
    let site = site(&stub);
    let (got, into) = download(handle(&site).as_mut(), FILE, 1 << 20);
    assert_eq!(got, Ok(()));
    assert_eq!(into, install());
    assert_eq!(stub.asked(), [RELEASE, ASSET, RELEASE_ASSET]);
    assert_no_credential(&stub);
    assert!(left_behind().is_empty());
}
