//! The program against a factory of the test's: the proxy's listener with its
//! job channel over a real socket, a real state store, a product repository on
//! disk, and a stub in place of `tehas work`. A join, one job run to its
//! result through the token renewal and the relay, a drain, and a host left
//! with no credential and no source.

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tehas_core::testdir::TestDir;
use tehas_cycle::relay::Scope;
use tehas_proxy::channel::{key_hash, Channel, Factory};
use tehas_proxy::jobs::Issuer;
use tehas_proxy::log::{Line, Sink};
use tehas_proxy::records::NoneAtAll;
use tehas_proxy::serve::{self, Serve};

const CODE: &str = "printed-code-of-box1";
const CYCLE_TOKEN: &str = "ghs_cycle_token_of_the_factory";

fn call(home: &Path, args: &[&str]) -> Value {
    let text = store::call(home, args, &mut std::io::empty())
        .unwrap_or_else(|e| panic!("{args:?}: {e:?}"));
    serde_json::from_str(&text).unwrap()
}

struct TestFactory {
    home: PathBuf,
    kept: Arc<Mutex<Vec<(i64, String, i64)>>>,
    relayed: Arc<Mutex<Vec<Vec<String>>>>,
}

impl Factory for TestFactory {
    fn store(&self, args: &[&str]) -> Option<Value> {
        let text = store::call(&self.home, args, &mut std::io::empty()).ok()?;
        serde_json::from_str(&text).ok()
    }
    fn cycle_token(&self) -> Option<(String, i64)> {
        Some((CYCLE_TOKEN.to_string(), 4_000_000_000))
    }
    fn relay(
        &self,
        args: &[String],
        input: Option<&str>,
        scope: &mut Scope,
    ) -> Result<(String, String, i32), String> {
        struct Gh(Arc<Mutex<Vec<Vec<String>>>>);
        impl tehas_cycle::relay::Run for Gh {
            fn run(&self, argv: &[String], _: Option<&str>) -> (String, String, i32) {
                self.0.lock().unwrap().push(argv.to_vec());
                (
                    "https://github.com/acme/widget/issues/7#c1\n".into(),
                    String::new(),
                    0,
                )
            }
        }
        tehas_cycle::relay::answer_request(
            args,
            input,
            "acme/widget",
            &Gh(Arc::clone(&self.relayed)),
            scope,
        )
    }
    fn keep(&self, issue: i64, result: &[u8], exit: i64) -> Result<(), String> {
        self.kept
            .lock()
            .unwrap()
            .push((issue, String::from_utf8_lossy(result).into_owned(), exit));
        Ok(())
    }
    fn carried(&self) -> Value {
        // The test's own account: one it may hand the cycle's folders to.
        use std::os::unix::fs::MetadataExt;
        let me = fs::metadata(&self.home).unwrap();
        json!({
            "settings": { "TEHAS_REPO": "acme/widget", "TEHAS_OUTSOURCE": "on" },
            "models": { "providers": { "main": { "baseUrl": "http://factory/v1", "apiKey": "${PI_ENSEMBLE_API_KEY_MAIN}" } } },
            "uid": me.uid(), "gid": me.gid()
        })
    }
    fn timeout(&self) -> i64 {
        60
    }
    fn random(&self) -> Option<String> {
        tehas_proxy::jobs::random()
    }
    fn allowed(&self, name: &str) -> bool {
        name == "box1"
    }
}

struct Quiet;
impl Sink for Quiet {
    fn line(&self, _: &Line) {}
}

/// The factory's listener on a port of its own, for the life of the test.
fn listen(factory: TestFactory) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = listener.local_addr().unwrap().to_string();
    let issuer: &'static Issuer = Box::leak(Box::new(Issuer::new()));
    let factory: &'static TestFactory = Box::leak(Box::new(factory));
    let chan: &'static Channel<'static> = Box::leak(Box::new(Channel::new(issuer, factory)));
    let serve: &'static Serve<'static> = Box::leak(Box::new(Serve {
        endpoint: tehas_proxy::forward::Upstream::of("http://127.0.0.1:1/v1").unwrap(),
        key: "sk-never-sent".to_string(),
        records: &NoneAtAll,
        sink: &Quiet,
        idle: Duration::from_secs(5),
        channel: Some(chan),
    }));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut stream = stream;
                serve::one(&mut stream, serve);
            });
        }
    });
    at
}

fn exe(path: &Path, body: &str) {
    tehas_core::testing::write_exe(path, body).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

/// Every file under `dir`, read whole.
fn every_file(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            every_file(&p, out);
        } else if let Ok(b) = fs::read(&p) {
            out.push((p, b));
        }
    }
}

#[test]
fn a_host_joins_runs_one_job_to_its_result_and_is_left_clean() {
    let d = TestDir::new("contractor-program");
    let factory_home = d.path().join("factory");
    fs::create_dir_all(factory_home.join("state")).unwrap();
    call(
        &factory_home,
        &[
            "init",
            "--version",
            "v0.30.0",
            "--commit",
            "cb6912a",
            "--instance",
            "example",
            "--repo",
            "acme/widget",
            "--outsource",
        ],
    );
    let c = rusqlite::Connection::open(factory_home.join("state/tehas.db")).unwrap();
    c.execute(
        "INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (7, 'issue', 1, 1)",
        [],
    )
    .unwrap();
    drop(c);
    call(
        &factory_home,
        &["invite-new", "--name", "box1", "--code", &key_hash(CODE)],
    );
    let kept = Arc::new(Mutex::new(Vec::new()));
    let relayed = Arc::new(Mutex::new(Vec::new()));
    let at = listen(TestFactory {
        home: factory_home.clone(),
        kept: Arc::clone(&kept),
        relayed: Arc::clone(&relayed),
    });

    // The product, as GitHub would serve it.
    let origin = d.path().join("git/acme/widget.git");
    fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "--bare", "-b", "main"]);
    let seed = d.path().join("seed");
    fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "--quiet", "-b", "main"]);
    fs::write(seed.join("README"), "widget\n").unwrap();
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
            "seed",
        ],
    );
    git(
        &seed,
        &["push", "--quiet", origin.to_str().unwrap(), "main"],
    );

    // The contractor's host: its home and its tehas home.
    let user_home = d.path().join("box1");
    let tehas_home = user_home.join("tehas");
    fs::create_dir_all(&tehas_home).unwrap();
    let seen = d.path().join("seen");
    fs::create_dir_all(&seen).unwrap();
    // tehas work, as far as a contractor's job needs it: it reads what the
    // job left, renews the token and relays one write through the program,
    // writes its result, and asks for a drain so the loop ends.
    let work = d.path().join("work");
    exe(
        &work,
        &format!(
            r#"#!/bin/sh
seen={seen}
[ "$1" = 7 ] || exit 9
cp "$TEHAS_INSTANCE_DIR/instance.env" "$seen/instance.env"
cp "$HOME/.pi/agent/models.json" "$seen/models.json"
[ -f "$TEHAS_HOME/repos/widget/README" ] && echo cloned >"$seen/clone"
"$TEHAS_CONTRACTOR" token >"$seen/token.out" || exit 8
cat "$TEHAS_HOME/state/github-cycle/token" >"$seen/token.file"
echo '{{"argv":["issue","comment","7","--body-file=-","--repo=acme/widget"],"input":"done"}}' | "$TEHAS_CONTRACTOR" relay >"$seen/relay.out"
mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/work/widget-7" "$TEHAS_HOME/work/widget-7.state/cache"
me="$(id -u):$(id -g)"
"$TEHAS_CONTRACTOR" own --as "$me" "$TEHAS_HOME/work/widget-7" "$TEHAS_HOME/work/widget-7.state/cache" 2>"$seen/own.err"
echo $? >"$seen/own.rc"
"$TEHAS_CONTRACTOR" own --as "$me" "$TEHAS_HOME" 2>"$seen/own-home.err"
echo $? >"$seen/own-home.rc"
"$TEHAS_CONTRACTOR" own --as "$(( $(id -u) + 1 )):$(id -g)" "$TEHAS_HOME/work/widget-7" 2>"$seen/own-other.err"
echo $? >"$seen/own-other.rc"
"$TEHAS_CONTRACTOR" own "$TEHAS_HOME/work/widget-7" 2>/dev/null
echo $? >"$seen/own-bare.rc"
echo '{{"status":"merged","issue":7}}' >"$TEHAS_HOME/state/cycles/7.result.json"
echo 0 >"$TEHAS_HOME/state/cycles/7.runner_exit"
"$TEHAS_CONTRACTOR" drain
"#,
            seen = seen.display()
        ),
    );
    let program = env!("CARGO_BIN_EXE_tehas-contractor");
    let run = |args: &[&str]| {
        Command::new(program)
            .args(args)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", &user_home)
            .env("TEHAS_HOME", &tehas_home)
            .env("TEHAS_OUTSOURCE", "on")
            .env("TEHAS_TEST_FEATURES", "outsource")
            .env("TEHAS_WORK_CMD", &work)
            .env("TEHAS_WORK_GRACE", "5")
            .env(
                "TEHAS_CONTRACTOR_GIT",
                format!("file://{}", d.path().join("git").display()),
            )
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap()
    };

    // Off, every verb is refused before it touches anything.
    let off = Command::new(program)
        .arg("run")
        .env_clear()
        .env("HOME", &user_home)
        .output()
        .unwrap();
    assert_eq!(off.status.code(), Some(78));
    // Not joined yet: run is refused as a missing identity.
    assert_eq!(run(&["run"]).status.code(), Some(78));

    let joined = run(&["join", &at, "box1", CODE]);
    assert_eq!(
        joined.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&joined.stderr)
    );
    let id_file = user_home.join(".config/tehas/contractor.json");
    assert_eq!(
        fs::metadata(&id_file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        call(&factory_home, &["contractor-list"])["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // The code is spent.
    assert_eq!(run(&["join", &at, "box1", CODE]).status.code(), Some(1));

    let offer = call(
        &factory_home,
        &["job-offer", "--issue", "7", "--contractor", "box1"],
    );
    let job = offer["job"].as_i64().unwrap();
    let ran = run(&["run"]);
    let log = String::from_utf8_lossy(&ran.stderr).into_owned();
    assert_eq!(ran.status.code(), Some(0), "{log}");
    assert!(
        log.lines()
            .all(|l| l.starts_with("tehas outsource contractor: ")),
        "{log}"
    );

    // What the cycle was given.
    let instance = fs::read_to_string(seen.join("instance.env")).unwrap();
    assert!(instance.contains("TEHAS_REPO='acme/widget'"), "{instance}");
    assert!(
        instance.contains("PI_ENSEMBLE_API_KEY_MAIN='"),
        "{instance}"
    );
    // The job's models name the factory at the address this host joined with,
    // not the one the factory gave.
    let models = fs::read_to_string(seen.join("models.json")).unwrap();
    assert!(models.contains(&format!("http://{at}/v1")), "{models}");
    assert!(!models.contains("http://factory/v1"), "{models}");
    assert_eq!(
        fs::read_to_string(seen.join("clone")).unwrap().trim(),
        "cloned"
    );
    assert_eq!(
        fs::read_to_string(seen.join("token.out")).unwrap().trim(),
        CYCLE_TOKEN
    );
    assert_eq!(
        fs::read_to_string(seen.join("token.file")).unwrap().trim(),
        CYCLE_TOKEN
    );
    // The cycle's folders went to the account the job named; a folder that is
    // not the cycle's is refused.
    use std::os::unix::fs::MetadataExt;
    let me = fs::metadata(&factory_home).unwrap();
    let read = |f: &str| fs::read_to_string(seen.join(f)).unwrap();
    assert_eq!(read("own.rc").trim(), "0", "{}", read("own.err"));
    assert!(
        read("own.err").contains(&format!("handed to {}:{}", me.uid(), me.gid())),
        "{}",
        read("own.err")
    );
    assert_eq!(read("own-other.rc").trim(), "0");
    assert!(
        read("own-other.err").contains("stay as they are"),
        "{}",
        read("own-other.err")
    );
    assert_eq!(read("own-bare.rc").trim(), "64");
    assert_eq!(read("own-home.rc").trim(), "1");
    assert!(
        read("own-home.err").contains("not a folder of the cycle"),
        "{}",
        read("own-home.err")
    );
    let relay: Value =
        serde_json::from_str(&fs::read_to_string(seen.join("relay.out")).unwrap()).unwrap();
    assert_eq!(relay["rc"], 0, "{relay}");
    assert_eq!(relayed.lock().unwrap().len(), 1);

    // What the factory got.
    let kept = kept.lock().unwrap().clone();
    assert_eq!(kept.len(), 1);
    assert_eq!((kept[0].0, kept[0].2), (7, 0));
    assert!(kept[0].1.contains("merged"));
    assert_eq!(
        call(&factory_home, &["job-show", "--job", &job.to_string()])["state"],
        "finished"
    );

    // What the host kept: its identity, and no GitHub or model credential, no
    // job token and no source.
    let mut files = Vec::new();
    every_file(&user_home, &mut files);
    let names: Vec<String> = files
        .iter()
        .map(|(p, _)| p.strip_prefix(&user_home).unwrap().display().to_string())
        .collect();
    assert!(
        names
            .iter()
            .all(|n| n == ".config/tehas/contractor.json" || n == "tehas/state/outsource/drain"),
        "{names:?}"
    );
    let instance_token = instance
        .lines()
        .find_map(|l| l.strip_prefix("PI_ENSEMBLE_API_KEY_MAIN='"))
        .and_then(|l| l.strip_suffix('\''))
        .unwrap()
        .to_string();
    for (path, bytes) in &files {
        let text = String::from_utf8_lossy(bytes);
        for secret in [CYCLE_TOKEN, instance_token.as_str(), "ghs_", "sk-"] {
            assert!(!text.contains(secret), "{} holds {secret}", path.display());
        }
    }
}

#[test]
fn run_refuses_a_home_that_holds_a_factory_and_removes_nothing() {
    let d = TestDir::new("contractor-factory-home");
    let user_home = d.path().join("factory-owner");
    let tehas_home = user_home.join("tehas");
    let kept = [
        tehas_home.join("state/tehas.db"),
        tehas_home.join("repos/widget/README"),
        tehas_home.join("work/widget-7/notes"),
        tehas_home.join("state/cycles/7.result.json"),
        user_home.join(".pi/agent/models.json"),
    ];
    for f in &kept {
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, "the factory's").unwrap();
    }
    tehas_contractor::identity::write(
        &user_home.join(".config/tehas/contractor.json"),
        &tehas_contractor::identity::Identity {
            factory: "127.0.0.1:1".to_string(),
            name: "box1".to_string(),
            key: "k".to_string(),
        },
    )
    .unwrap();
    let ran = Command::new(env!("CARGO_BIN_EXE_tehas-contractor"))
        .arg("run")
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", &user_home)
        .env("TEHAS_HOME", &tehas_home)
        .env("TEHAS_OUTSOURCE", "on")
        .env("TEHAS_TEST_FEATURES", "outsource")
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&ran.stderr);
    assert_eq!(ran.status.code(), Some(78), "{log}");
    assert!(log.contains("never beside a factory"), "{log}");
    for f in &kept {
        assert_eq!(
            fs::read_to_string(f).unwrap(),
            "the factory's",
            "{}",
            f.display()
        );
    }
}
