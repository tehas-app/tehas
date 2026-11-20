//! The download routes against a real state store, a real listener and the
//! host's own downloads ([`OfHome`]) over a real git checkout, with a stub in
//! place of docker: who may fetch what, ranges, and that each file is made
//! once.

use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};
use tehas_core::testdir::TestDir;
use tehas_cycle::relay::Scope;
use tehas_proxy::channel::{key_hash, Asked, Call, Channel, Factory};
use tehas_proxy::download::{sha256_of, OfHome};
use tehas_proxy::jobs::Issuer;
use tehas_proxy::records::NoneAtAll;
use tehas_proxy::serve::{self, Serve};

const KEY: &str = "the-contractor-key-of-box1";

fn call(home: &Path, args: &[&str]) -> Value {
    let text =
        store::call(home, args, &mut io::empty()).unwrap_or_else(|e| panic!("{args:?}: {e:?}"));
    serde_json::from_str(&text).unwrap()
}

struct Fake(PathBuf);

impl Factory for Fake {
    fn store(&self, args: &[&str]) -> Option<Value> {
        let text = store::call(&self.0, args, &mut io::empty()).ok()?;
        serde_json::from_str(&text).ok()
    }
    fn cycle_token(&self) -> Option<(String, i64)> {
        None
    }
    fn relay(
        &self,
        _: &[String],
        _: Option<&str>,
        _: &mut Scope,
    ) -> Result<(String, String, i32), String> {
        Err("no relay here".into())
    }
    fn keep(&self, _: i64, _: &[u8], _: i64) -> Result<(), String> {
        Ok(())
    }
    fn carried(&self) -> Value {
        json!({ "settings": {}, "models": {} })
    }
    fn timeout(&self) -> i64 {
        600
    }
    fn random(&self) -> Option<String> {
        tehas_proxy::jobs::random()
    }
    fn allowed(&self, name: &str) -> bool {
        name == "box1" || name == "box2"
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A product checkout as a factory keeps one: a clone with origin/main.
fn checkout(d: &Path) -> PathBuf {
    let origin = d.join("origin.git");
    fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "--bare", "-b", "main"]);
    let seed = d.join("seed");
    fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "--quiet", "-b", "main"]);
    for n in 1..=2 {
        fs::write(seed.join("README"), format!("widget {n}\n")).unwrap();
        git(&seed, &["add", "README"]);
        git(
            &seed,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "commit",
                "--quiet",
                "-m",
                &format!("c{n}"),
            ],
        );
    }
    git(
        &seed,
        &["push", "--quiet", origin.to_str().unwrap(), "main"],
    );
    let co = d.join("repos/widget");
    fs::create_dir_all(co.parent().unwrap()).unwrap();
    let out = Command::new("git")
        .args(["clone", "--quiet"])
        .arg(&origin)
        .arg(&co)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(out.status.success());
    co
}

/// docker, as far as the downloads ask it: `image inspect` prints the id in
/// `<dir>/id`, `save` prints `<dir>/content` and counts itself.
fn docker(dir: &Path) -> PathBuf {
    let path = dir.join("docker");
    tehas_core::testing::write_exe(
        &path,
        format!(
            "#!/bin/sh\nd={}\ncase \"$1\" in\n image) cat \"$d/id\" ;;\n save) echo x >>\"$d/saves\"; [ -e \"$d/hang\" ] && exec sleep 60; sleep 0.3; cat \"$d/content\" ;;\n *) exit 2 ;;\nesac\n",
            dir.display()
        ),
    )
    .unwrap();
    path
}

fn id(c: char) -> String {
    format!("sha256:{}", c.to_string().repeat(64))
}

fn fetched(_: &Path, issue: u64, dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        format!("{{\"issue\":{issue}}}\n"),
    )
    .unwrap();
    fs::write(dir.join("file-1.md"), "a file\n").unwrap();
    Ok(())
}

fn refused_fetch(_: &Path, _: u64, _: &Path) -> Result<(), String> {
    Err("the issue's images could not be fetched".into())
}

/// A factory with box1 holding a running job of issue 7.
struct Case {
    at: SocketAddr,
    home: TestDir,
    job: i64,
    model: String,
    job_token: String,
    lines: Arc<Mutex<Vec<String>>>,
    chan: &'static Channel<'static>,
}

fn case(name: &str, files: bool, fetch: fn(&Path, u64, &Path) -> Result<(), String>) -> Case {
    case_with(name, true, files, fetch)
}

fn case_with(
    name: &str,
    downloads: bool,
    files: bool,
    fetch: fn(&Path, u64, &Path) -> Result<(), String>,
) -> Case {
    let home = TestDir::new(name);
    let h = home.path();
    fs::create_dir_all(h.join("state")).unwrap();
    call(
        h,
        &[
            "init",
            "--version",
            "v0.30.0",
            "--commit",
            "cb6912a",
            "--instance",
            "example",
            "--repo",
            "example-owner/example-product",
            "--outsource",
        ],
    );
    let c = rusqlite::Connection::open(h.join("state/tehas.db")).unwrap();
    for issue in [7, 8] {
        c.execute(
            "INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (?1, 'issue', 1, 1)",
            [issue],
        )
        .unwrap();
    }
    drop(c);
    call(
        h,
        &["contractor-add", "--name", "box1", "--key", &key_hash(KEY)],
    );
    call(h, &["job-offer", "--issue", "7", "--contractor", "box1"]);
    let tools = h.join("tools");
    fs::create_dir_all(&tools).unwrap();
    fs::write(tools.join("id"), format!("{}\n", id('a'))).unwrap();
    fs::write(tools.join("content"), "the saved image of a\n").unwrap();
    let ofhome: &'static OfHome = Box::leak(Box::new(OfHome {
        home: h.to_path_buf(),
        checkout: checkout(h),
        branch: "main".to_string(),
        image: "tehas-sandbox:test".to_string(),
        docker: docker(&tools),
        save_wait: Duration::from_secs(3),
        zstd: false,
        gives_files: files,
        fetch,
        making: Default::default(),
    }));
    let issuer: &'static Issuer = Box::leak(Box::new(Issuer::new()));
    let fake: &'static Fake = Box::leak(Box::new(Fake(h.to_path_buf())));
    let mut chan = Channel::new(issuer, fake);
    if downloads {
        chan = chan.with_downloads(ofhome);
    }
    let chan: &'static Channel<'static> = Box::leak(Box::new(chan));
    let job = chan.answer(&Asked {
        call: Call::Job,
        bearer: Some(KEY),
        name: Some("box1"),
        body: b"",
    });
    assert_eq!(job.status, 200, "{job:?}");
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink: &'static (dyn tehas_proxy::log::Sink + Sync) =
        Box::leak(Box::new(serve::kept(&lines)));
    let serve: &'static Serve<'static> = Box::leak(Box::new(Serve {
        endpoint: tehas_proxy::forward::Upstream::of("http://127.0.0.1:1/v1").unwrap(),
        key: "sk-never-sent".to_string(),
        records: &NoneAtAll,
        sink,
        idle: Duration::from_secs(5),
        channel: Some(chan),
    }));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            thread::spawn(move || {
                let mut stream = stream;
                serve::one(&mut stream, serve);
            });
        }
    });
    Case {
        at,
        home,
        job: job.body["job"].as_i64().unwrap(),
        model: job.body["model_token"].as_str().unwrap().to_string(),
        job_token: job.body["token"].as_str().unwrap().to_string(),
        lines,
        chan,
    }
}

/// One answer: its status, its head and its body.
struct Got {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Got {
    fn field(&self, name: &str) -> Option<String> {
        self.head.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case(name)
                .then(|| v.trim().to_string())
        })
    }
    fn refused(&self) -> String {
        let v: Value = serde_json::from_slice(&self.body).unwrap_or(Value::Null);
        v["refused"].as_str().unwrap_or_default().to_string()
    }
}

fn ask(at: SocketAddr, method: &str, path: &str, fields: &str) -> Got {
    let mut s = TcpStream::connect(at).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    write!(
        s,
        "{method} /outsource/v1/{path} HTTP/1.1\r\nHost: factory\r\n{fields}Connection: close\r\n\r\n"
    )
    .unwrap();
    let mut all = Vec::new();
    s.read_to_end(&mut all).unwrap();
    let end = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&all[..end]).into_owned();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    Got {
        status,
        head,
        body: all[end + 4..].to_vec(),
    }
}

fn get(at: SocketAddr, path: &str, fields: &str) -> Got {
    ask(at, "GET", path, fields)
}

fn by_key(key: &str) -> String {
    format!("Authorization: Bearer {key}\r\nx-tehas-contractor: box1\r\n")
}

fn by_token(token: &str) -> String {
    format!("Authorization: Bearer {token}\r\n")
}

/// The names in a tar.
fn tar_names(bytes: &[u8], d: &Path) -> Vec<String> {
    let file = d.join("got.tar");
    fs::write(&file, bytes).unwrap();
    let out = Command::new("tar").arg("-tf").arg(&file).output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim_start_matches("./").to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

#[test]
fn the_image_goes_to_a_contractor_s_key_whole_or_by_range_and_is_saved_once() {
    let c = case("download-image", false, fetched);
    let path = format!("image?id={}", id('a'));
    // No key, a wrong key, and a job's tokens are not a contractor.
    assert_eq!(get(c.at, &path, "").status, 401);
    assert_eq!(get(c.at, &path, &by_key("not-the-key")).status, 401);
    assert_eq!(get(c.at, &path, &by_token(&c.job_token)).status, 401);
    assert_eq!(get(c.at, &path, &by_token(&c.model)).status, 401);
    assert!(!c.home.path().join("state/images").exists());
    // Four at once: one save, the same bytes to each.
    let at = c.at;
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            thread::spawn(move || get(at, &path, &by_key(KEY)))
        })
        .collect();
    let content = b"the saved image of a\n".to_vec();
    for t in threads {
        let got = t.join().unwrap();
        assert_eq!(got.status, 200, "{}", got.head);
        assert_eq!(got.body, content);
        assert_eq!(got.field("x-tehas-image"), Some(id('a')));
        assert_eq!(got.field("accept-ranges").as_deref(), Some("bytes"));
        assert!(got.field("x-tehas-uid").is_some_and(|u| !u.is_empty()));
    }
    let saves = fs::read_to_string(c.home.path().join("tools/saves")).unwrap();
    assert_eq!(saves.lines().count(), 1, "{saves}");
    let file = c
        .home
        .path()
        .join(format!("state/images/{}.tar", "a".repeat(64)));
    let got = get(c.at, &path, &by_key(KEY));
    assert_eq!(got.field("x-tehas-sha256"), Some(sha256_of(&file).unwrap()));
    // A range resumes a broken download.
    let part = get(c.at, &path, &format!("{}Range: bytes=4-\r\n", by_key(KEY)));
    assert_eq!(part.status, 206, "{}", part.head);
    assert_eq!(part.body, content[4..].to_vec());
    assert_eq!(
        part.field("content-range"),
        Some(format!("bytes 4-{}/{}", content.len() - 1, content.len()))
    );
    let span = get(c.at, &path, &format!("{}Range: bytes=0-2\r\n", by_key(KEY)));
    assert_eq!(span.body, b"the".to_vec());
    let past = get(
        c.at,
        &path,
        &format!("{}Range: bytes=999-\r\n", by_key(KEY)),
    );
    assert_eq!(past.status, 416);
    // An id that is not the factory's image, or none at all. Each refusal
    // gives its place among the image downloads back, so more of them than
    // there are places leave the next download free to start.
    for _ in 0..=tehas_proxy::download::IMAGES_AT_ONCE {
        let other = get(c.at, &format!("image?id={}", id('b')), &by_key(KEY));
        assert_eq!(
            (other.status, other.refused().as_str()),
            (404, "not the factory's image")
        );
    }
    assert_eq!(get(c.at, &path, &by_key(KEY)).status, 200);
    assert_eq!(get(c.at, "image", &by_key(KEY)).status, 400);
    // A download is a GET.
    assert_eq!(ask(c.at, "POST", &path, &by_key(KEY)).status, 400);
    // The log names the contractor and never its key.
    let lines = c.lines.lock().unwrap().join("\n");
    assert!(lines.contains("GET /outsource/v1/image 200"), "{lines}");
    assert!(lines.contains("contractor box1"), "{lines}");
    assert!(!lines.contains(KEY), "{lines}");
    // Revoked, the contractor gets nothing more.
    call(c.home.path(), &["contractor-revoke", "--name", "box1"]);
    let revoked = get(c.at, &path, &by_key(KEY));
    assert_eq!(
        (revoked.status, revoked.refused().as_str()),
        (403, "revoked")
    );
}

#[test]
fn a_save_that_hangs_is_killed_and_gives_its_place_back() {
    let c = case("download-image-hang", false, fetched);
    let path = format!("image?id={}", id('a'));
    fs::write(c.home.path().join("tools/hang"), "").unwrap();
    // More hung saves than there are places: each is killed at its bound, so
    // none of them holds up the next.
    for _ in 0..=tehas_proxy::download::IMAGES_AT_ONCE {
        let began = std::time::Instant::now();
        let hung = get(c.at, &path, &by_key(KEY));
        assert_eq!(hung.status, 503, "{}", hung.head);
        assert!(
            hung.refused().starts_with("the image was not saved"),
            "{}",
            hung.refused()
        );
        assert!(began.elapsed() < Duration::from_secs(20));
    }
    assert!(!c
        .home
        .path()
        .join(format!("state/images/{}.tar", "a".repeat(64)))
        .exists());
    fs::remove_file(c.home.path().join("tools/hang")).unwrap();
    let got = get(c.at, &path, &by_key(KEY));
    assert_eq!(got.status, 200, "{}", got.head);
    assert_eq!(got.body, b"the saved image of a\n".to_vec());
}

#[test]
fn the_image_before_the_current_one_stays_and_older_ones_go() {
    let c = case("download-image-previous", false, fetched);
    let tools = c.home.path().join("tools");
    let images = c.home.path().join("state/images");
    for (n, ch) in ['a', 'b', 'c'].into_iter().enumerate() {
        fs::write(tools.join("id"), format!("{}\n", id(ch))).unwrap();
        let got = get(c.at, &format!("image?id={}", id(ch)), &by_key(KEY));
        assert_eq!(got.status, 200, "{}", got.head);
        let held = |x: char| {
            images
                .join(format!("{}.tar", x.to_string().repeat(64)))
                .is_file()
        };
        assert!(held(ch));
        if n == 2 {
            assert!(held('b'), "the previous image went");
            assert!(!held('a'), "an older image stayed");
        }
        // Sleep so the files' times differ.
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_job_s_bundle_is_its_tip_for_its_job_token_alone() {
    let c = case("download-bundle", false, fetched);
    assert_eq!(get(c.at, "bundle", "").status, 401);
    assert_eq!(get(c.at, "bundle", &by_token(&"x".repeat(43))).status, 401);
    let model = get(c.at, "bundle", &by_token(&c.model));
    assert_eq!((model.status, model.refused().as_str()), (403, "scope"));
    // A contractor's key is no job.
    assert_eq!(get(c.at, "bundle", &by_key(KEY)).status, 401);
    let got = get(c.at, "bundle", &by_token(&c.job_token));
    assert_eq!(got.status, 200, "{}", got.head);
    let co = c.home.path().join("repos/widget");
    let tip = git(&co, &["rev-parse", "refs/remotes/origin/main"]);
    assert_eq!(got.field("x-tehas-tip"), Some(tip.clone()));
    assert_eq!(got.field("x-tehas-branch").as_deref(), Some("main"));
    // A clone of the bundle is the checkout at its tip: same tree, same head.
    let d = c.home.path().join("contractor");
    fs::create_dir_all(&d).unwrap();
    let bundle = d.join("source.bundle");
    fs::write(&bundle, &got.body).unwrap();
    let heads = git(&d, &["bundle", "list-heads", bundle.to_str().unwrap()]);
    assert_eq!(heads, format!("{tip} refs/remotes/origin/main"));
    let ws = d.join("ws");
    let out = Command::new("git")
        .args(["clone", "--quiet", "--no-checkout"])
        .arg(&bundle)
        .arg(&ws)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    git(
        &ws,
        &[
            "fetch",
            "--quiet",
            bundle.to_str().unwrap(),
            "+refs/remotes/origin/main:refs/remotes/origin/main",
        ],
    );
    assert_eq!(
        git(&ws, &["rev-parse", "refs/remotes/origin/main^{tree}"]),
        git(&co, &["rev-parse", "refs/remotes/origin/main^{tree}"])
    );
    assert_eq!(
        git(&ws, &["rev-list", "--count", "refs/remotes/origin/main"]),
        "2"
    );
    // Made once per tip.
    let kept = c.home.path().join("state/outsource/downloads");
    let made = |dir: &Path| {
        fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".bundle"))
            .count()
    };
    assert_eq!(get(c.at, "bundle", &by_token(&c.job_token)).body, got.body);
    assert_eq!(made(&kept), 1);
    // A job that ended takes nothing more.
    let stopped = c.chan.answer(&Asked {
        call: Call::Stopped,
        bearer: Some(&c.job_token),
        name: None,
        body: b"",
    });
    assert_eq!(stopped.status, 200);
    assert_eq!(get(c.at, "bundle", &by_token(&c.job_token)).status, 401);
}

#[test]
fn the_index_is_served_for_the_current_image_and_never_for_another() {
    let c = case("download-index", false, fetched);
    let h = c.home.path();
    // No index built: none to give, which a contractor runs without.
    let none = get(c.at, "index", &by_token(&c.job_token));
    assert_eq!((none.status, none.refused().as_str()), (404, "no index"));
    let index = h.join("state/index");
    fs::create_dir_all(index.join("logs")).unwrap();
    fs::write(index.join("widget.db"), "an index\n").unwrap();
    fs::write(index.join("logs/run.log"), "a log\n").unwrap();
    fs::write(index.join("widget.db.stage.1"), "half\n").unwrap();
    let record = |image: &str| {
        fs::write(
            h.join("state/index.json"),
            json!({ "commit": "c0ffee", "branch": "main", "indexer_image": image }).to_string(),
        )
        .unwrap()
    };
    record(&id('b'));
    let other = get(c.at, "index", &by_token(&c.job_token));
    assert_eq!(
        (other.status, other.refused().as_str()),
        (404, "an index of another image")
    );
    record(&id('a'));
    assert_eq!(get(c.at, "index", &by_token(&c.model)).status, 403);
    let got = get(c.at, "index", &by_token(&c.job_token));
    assert_eq!(got.status, 200, "{}", got.head);
    assert_eq!(got.field("x-tehas-tip").as_deref(), Some("c0ffee"));
    assert_eq!(got.field("x-tehas-image"), Some(id('a')));
    assert_eq!(got.field("x-tehas-pack").as_deref(), Some("tar"));
    let names = tar_names(&got.body, h);
    assert!(names.contains(&"widget.db".to_string()), "{names:?}");
    assert!(
        !names
            .iter()
            .any(|n| n.starts_with("logs") || n.contains("stage")),
        "{names:?}"
    );
}

#[test]
fn a_job_s_files_are_its_own_issue_s_and_go_with_the_job() {
    // An instance that gives a cycle no files serves none.
    let off = case("download-files-off", false, fetched);
    let none = get(off.at, "attachments", &by_token(&off.job_token));
    assert_eq!(
        (none.status, none.refused().as_str()),
        (404, "no files given")
    );

    let c = case("download-files", true, fetched);
    assert_eq!(get(c.at, "attachments", &by_token(&c.model)).status, 403);
    let got = get(c.at, "attachments", &by_token(&c.job_token));
    assert_eq!(got.status, 200, "{}", got.head);
    assert_eq!(got.field("x-tehas-issue").as_deref(), Some("7"));
    let names = tar_names(&got.body, c.home.path());
    assert!(names.contains(&"manifest.json".to_string()), "{names:?}");
    assert!(names.contains(&"file-1.md".to_string()), "{names:?}");
    let dir = c
        .home
        .path()
        .join(format!("state/outsource/downloads/job-{}", c.job));
    assert!(dir.is_dir());
    let stopped = c.chan.answer(&Asked {
        call: Call::Stopped,
        bearer: Some(&c.job_token),
        name: None,
        body: b"",
    });
    assert_eq!(stopped.status, 200);
    assert!(!dir.exists(), "the job's files outlived it");

    // A fetch the in-house start would stop on stops the job too.
    let bad = case("download-files-refused", true, refused_fetch);
    let got = get(bad.at, "attachments", &by_token(&bad.job_token));
    assert_eq!(got.status, 409);
    assert_eq!(got.refused(), "the issue's images could not be fetched");
}

#[test]
fn without_downloads_every_download_is_404_and_hello_names_no_image() {
    let c = case_with("download-none", false, false, fetched);
    for path in ["image?id=x", "index", "bundle", "attachments"] {
        let got = get(c.at, path, &by_token(&c.job_token));
        assert_eq!(got.status, 404, "{path}");
        assert_eq!(got.refused(), "no downloads here");
    }
    let hello = c.chan.answer(&Asked {
        call: Call::Hello,
        bearer: Some(KEY),
        name: Some("box1"),
        body: b"",
    });
    assert!(hello.body.get("image").is_none(), "{hello:?}");
    assert!(!c.home.path().join("state/images").exists());
    // With them, hello names the image a contractor must run.
    let d = case("download-hello", false, fetched);
    let hello = d.chan.answer(&Asked {
        call: Call::Hello,
        bearer: Some(KEY),
        name: Some("box1"),
        body: b"",
    });
    assert_eq!(hello.body["image"], json!(id('a')));
}

#[test]
fn the_files_of_a_job_lost_to_a_silence_go_when_the_next_job_s_are_made() {
    let c = case("download-files-lost", true, fetched);
    let h = c.home.path();
    assert_eq!(
        get(c.at, "attachments", &by_token(&c.job_token)).status,
        200
    );
    let lost = h.join(format!("state/outsource/downloads/job-{}", c.job));
    assert!(lost.is_dir());
    // Lost to a silence: no finish, no stopped, no revoke ever comes.
    call(
        h,
        &[
            "job-lose",
            "--job",
            &c.job.to_string(),
            "--reason",
            "timeout",
        ],
    );
    call(h, &["job-offer", "--issue", "8", "--contractor", "box1"]);
    let next = c.chan.answer(&Asked {
        call: Call::Job,
        bearer: Some(KEY),
        name: Some("box1"),
        body: b"",
    });
    assert_eq!(next.status, 200, "{next:?}");
    let token = next.body["token"].as_str().unwrap();
    let job = next.body["job"].as_i64().unwrap();
    let got = get(c.at, "attachments", &by_token(token));
    assert_eq!(got.status, 200, "{}", got.head);
    assert_eq!(got.field("x-tehas-issue").as_deref(), Some("8"));
    assert!(!lost.exists(), "a lost job's files outlived it");
    assert!(h
        .join(format!("state/outsource/downloads/job-{job}"))
        .is_dir());
}

static SLOW_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SLOW_RELEASED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Issue 7's fetch hangs until released; any other issue's is at once.
fn slow_for_7(home: &Path, issue: u64, dir: &Path) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    if issue == 7 {
        SLOW_STARTED.store(true, Ordering::SeqCst);
        for _ in 0..400 {
            if SLOW_RELEASED.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
    fetched(home, issue, dir)
}

#[test]
fn a_slow_fetch_of_one_job_s_files_holds_up_no_other_job_s() {
    use std::sync::atomic::Ordering;
    let c = case("download-files-apart", true, slow_for_7);
    let h = c.home.path();
    // A second running job, of issue 8, on a second contractor.
    call(
        h,
        &[
            "contractor-add",
            "--name",
            "box2",
            "--key",
            &key_hash("key-of-box2"),
        ],
    );
    call(h, &["job-offer", "--issue", "8", "--contractor", "box2"]);
    let second = c.chan.answer(&Asked {
        call: Call::Job,
        bearer: Some("key-of-box2"),
        name: Some("box2"),
        body: b"",
    });
    assert_eq!(second.status, 200, "{second:?}");
    let token = second.body["token"].as_str().unwrap().to_string();
    let at = c.at;
    let first = c.job_token.clone();
    let slow = thread::spawn(move || get(at, "attachments", &by_token(&first)));
    while !SLOW_STARTED.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(10));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(get(at, "attachments", &by_token(&token)));
    });
    let quick = rx.recv_timeout(Duration::from_secs(5));
    SLOW_RELEASED.store(true, Ordering::SeqCst);
    let quick = quick.expect("issue 8's files waited on issue 7's fetch");
    assert_eq!(quick.status, 200, "{}", quick.head);
    assert_eq!(quick.field("x-tehas-issue").as_deref(), Some("8"));
    assert_eq!(slow.join().unwrap().status, 200);
}
