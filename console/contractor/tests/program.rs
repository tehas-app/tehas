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
use tehas_proxy::download::OfHome;
use tehas_proxy::egress::Egress;
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

type Remembered = (String, Vec<String>);

struct TestFactory {
    home: PathBuf,
    kept: Arc<Mutex<Vec<(i64, String, i64)>>>,
    relayed: Arc<Mutex<Vec<Vec<String>>>>,
    /// The vipune calls the factory's memory ran: the database, the arguments.
    remembered: Arc<Mutex<Vec<Remembered>>>,
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
    fn repo(&self) -> String {
        "acme/widget".into()
    }
    fn memory(&self, cycle: &str, planned: &tehas_cycle::memory::Planned) -> (String, String, i32) {
        self.remembered
            .lock()
            .unwrap()
            .push((cycle.to_string(), planned.args.clone()));
        ("remembered\n".into(), String::new(), 0)
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
fn listen_with(
    factory: TestFactory,
    egress: Option<&'static Egress>,
    downloads: Option<&'static OfHome>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = listener.local_addr().unwrap().to_string();
    let issuer: &'static Issuer = Box::leak(Box::new(Issuer::new()));
    let factory: &'static TestFactory = Box::leak(Box::new(factory));
    let mut chan = Channel::new(issuer, factory);
    if let Some(egress) = egress {
        chan = chan.with_egress(egress);
    }
    if let Some(downloads) = downloads {
        chan = chan.with_downloads(downloads);
    }
    let chan: &'static Channel<'static> = Box::leak(Box::new(chan));
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
    // The factory's own checkout of it, its code index for the current
    // image, and the issue's files as its in-house fetch makes them.
    let checkout = factory_home.join("repos/widget");
    fs::create_dir_all(factory_home.join("repos")).unwrap();
    let cloned = Command::new("git")
        .args(["clone", "--quiet"])
        .arg(&origin)
        .arg(&checkout)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(cloned.status.success());
    let image = format!("sha256:{}", "a".repeat(64));
    fs::create_dir_all(factory_home.join("state/index")).unwrap();
    fs::write(factory_home.join("state/index/widget.db"), "an index\n").unwrap();
    fs::write(
        factory_home.join("state/index.json"),
        json!({ "commit": "c0ffee", "indexer_image": image }).to_string(),
    )
    .unwrap();
    let docker = factory_home.join("docker");
    exe(&docker, &format!("#!/bin/sh\necho {image}\n"));
    fn fetched(_: &Path, issue: u64, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("manifest.json"),
            format!("{{\"issue\":{issue}}}\n"),
        )
        .unwrap();
        Ok(())
    }
    let downloads: &'static OfHome = Box::leak(Box::new(OfHome {
        home: factory_home.clone(),
        checkout: checkout.clone(),
        branch: "main".to_string(),
        image: "tehas-sandbox:test".to_string(),
        docker,
        zstd: false,
        gives_files: true,
        fetch: fetched,
        making: Default::default(),
    }));
    let kept = Arc::new(Mutex::new(Vec::new()));
    let relayed = Arc::new(Mutex::new(Vec::new()));
    let remembered = Arc::new(Mutex::new(Vec::new()));
    let at = listen_with(
        TestFactory {
            home: factory_home.clone(),
            kept: Arc::clone(&kept),
            relayed: Arc::clone(&relayed),
            remembered: Arc::clone(&remembered),
        },
        None,
        Some(downloads),
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
git -C "$TEHAS_HOME/repos/widget" config --get remote.origin.url >"$seen/origin"
git -C "$TEHAS_HOME/repos/widget" rev-parse refs/remotes/origin/main >"$seen/tip"
cat "$TEHAS_HOME/state/index/widget.db" >"$seen/index"
cat "$TEHAS_HOME/work/widget-7.images/manifest.json" >"$seen/manifest"
"$TEHAS_CONTRACTOR" token >"$seen/token.out" || exit 8
cat "$TEHAS_HOME/state/github-cycle/token" >"$seen/token.file"
echo '{{"argv":["issue","comment","7","--body-file=-","--repo=acme/widget"],"input":"done"}}' | "$TEHAS_CONTRACTOR" relay >"$seen/relay.out"
echo '{{"kind":"vipune","argv":["search","the cache","-p","x/y"],"env":{{"VIPUNE_PROJECT":"acme/widget"}}}}' | "$TEHAS_CONTRACTOR" relay >"$seen/vipune.out"
echo '{{"kind":"vipune","argv":["delete","1"]}}' | "$TEHAS_CONTRACTOR" relay >"$seen/vipune-refused.out"
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
    // The source came from the factory's bundle, at the factory's tip, with
    // the repository's address as its origin.
    let factory_tip = Command::new("git")
        .arg("-C")
        .arg(&checkout)
        .args(["rev-parse", "refs/remotes/origin/main"])
        .output()
        .unwrap();
    assert_eq!(
        fs::read_to_string(seen.join("tip")).unwrap().trim(),
        String::from_utf8_lossy(&factory_tip.stdout).trim()
    );
    assert_eq!(
        fs::read_to_string(seen.join("origin")).unwrap().trim(),
        format!("file://{}/acme/widget.git", d.path().join("git").display())
    );
    assert_eq!(
        fs::read_to_string(seen.join("index")).unwrap(),
        "an index\n"
    );
    assert_eq!(
        fs::read_to_string(seen.join("manifest")).unwrap(),
        "{\"issue\":7}\n"
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
    // A vipune call goes over the same route and reaches the factory's
    // memory for the job's own database; a refused one answers vipune's
    // usage error and runs nothing.
    let said: Value =
        serde_json::from_str(&fs::read_to_string(seen.join("vipune.out")).unwrap()).unwrap();
    assert_eq!(
        (said["out"].as_str(), said["rc"].as_i64()),
        (Some("remembered\n"), Some(0)),
        "{said}"
    );
    let refused: Value =
        serde_json::from_str(&fs::read_to_string(seen.join("vipune-refused.out")).unwrap())
            .unwrap();
    assert_eq!(
        (refused["out"].as_str(), refused["rc"].as_i64()),
        (Some(""), Some(64)),
        "{refused}"
    );
    assert_eq!(
        remembered.lock().unwrap().as_slice(),
        [(
            format!("job-{job}"),
            vec!["search".to_string(), "the cache".to_string()]
        )]
    );

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

/// Where the test's GitHub resolves: a port of 127.0.0.1 that keeps the
/// first bytes it is sent.
static GITHUB: Mutex<Option<std::net::SocketAddr>> = Mutex::new(None);

fn resolve(host: &str, _: u16) -> std::io::Result<Vec<std::net::SocketAddr>> {
    match (host, *GITHUB.lock().unwrap()) {
        ("github.com", Some(at)) => Ok(vec![at]),
        _ => Err(std::io::Error::other("no such name")),
    }
}

#[test]
fn the_contractor_s_git_goes_out_only_through_the_factory_s_egress_route() {
    use std::io::Read;
    let d = TestDir::new("contractor-egress");
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
    let key = "the-key-of-box1";
    call(
        &factory_home,
        &["contractor-add", "--name", "box1", "--key", &key_hash(key)],
    );
    call(
        &factory_home,
        &["job-offer", "--issue", "7", "--contractor", "box1"],
    );
    // GitHub, as far as the test goes: it keeps the first byte of what came.
    let github = TcpListener::bind("127.0.0.1:0").unwrap();
    *GITHUB.lock().unwrap() = Some(github.local_addr().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for s in github.incoming().flatten() {
            let mut s = s;
            let mut first = [0u8; 1];
            let _ = s.read_exact(&mut first);
            let _ = tx.send(first[0]);
        }
    });
    let allowed = tehas_core::egress::Allowed::parse(&tehas_core::egress::listed("")).unwrap();
    let egress: &'static Egress = Box::leak(Box::new(Egress::with(
        allowed,
        Vec::new(),
        resolve,
        |_| true,
        Duration::from_secs(5),
    )));
    let at = listen_with(
        TestFactory {
            home: factory_home.clone(),
            kept: Arc::new(Mutex::new(Vec::new())),
            relayed: Arc::new(Mutex::new(Vec::new())),
            remembered: Arc::new(Mutex::new(Vec::new())),
        },
        Some(egress),
        None,
    );
    let job = tehas_contractor::http::post(
        &at,
        "job",
        tehas_contractor::http::As::Contractor { name: "box1", key },
        &json!({}),
        Duration::from_secs(10),
    );
    let tehas_contractor::http::Reply::Ok(job) = job else {
        panic!("{job:?}");
    };
    let model = job["model_token"].as_str().unwrap().to_string();
    let local = tehas_contractor::egress::Local::start(&at, &model).unwrap();
    let git = |url: &str| {
        tehas_contractor::host::git_with("ghs_cycle", &local.url())
            .args(["ls-remote", url])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap()
    };
    // GitHub is listed: git's TLS hello arrives there through the local end
    // and the factory. The handshake fails, as no TLS answers it.
    let out = git("https://github.com/acme/widget.git");
    let first = rx
        .recv_timeout(Duration::from_secs(20))
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)));
    assert_eq!(first, 0x16, "a TLS handshake record");
    // A host the factory does not list is refused, and git says so.
    let out = git("https://example.com/acme/widget.git");
    assert!(!out.status.success());
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    // A token of no job opens nothing.
    let forged = tehas_contractor::egress::Local::start(&at, &"x".repeat(32)).unwrap();
    let out = tehas_contractor::host::git_with("ghs_cycle", &forged.url())
        .args(["ls-remote", "https://github.com/acme/widget.git"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
}

/// A factory of issue 7's job for box1, by its key, with `downloads` on its
/// listener: its home, its address and the key.
fn factory_with(d: &Path, downloads: Option<&'static OfHome>) -> (PathBuf, String, &'static str) {
    let home = d.join("factory");
    fs::create_dir_all(home.join("state")).unwrap();
    call(
        &home,
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
    let c = rusqlite::Connection::open(home.join("state/tehas.db")).unwrap();
    c.execute(
        "INSERT INTO issues (issue, kind, kind_at, checked_at) VALUES (7, 'issue', 1, 1)",
        [],
    )
    .unwrap();
    drop(c);
    let key = "the-key-of-box1";
    call(
        &home,
        &["contractor-add", "--name", "box1", "--key", &key_hash(key)],
    );
    call(
        &home,
        &["job-offer", "--issue", "7", "--contractor", "box1"],
    );
    let at = listen_with(
        TestFactory {
            home: home.clone(),
            kept: Arc::new(Mutex::new(Vec::new())),
            relayed: Arc::new(Mutex::new(Vec::new())),
            remembered: Arc::new(Mutex::new(Vec::new())),
        },
        None,
        downloads,
    );
    (home, at, key)
}

/// Run box1 for one job against the factory at `at`, whose home is
/// `factory`: its exit, its log and what the cycle saw.
fn run_box(d: &Path, factory: &Path, at: &str, key: &str) -> (Option<i32>, String, PathBuf) {
    let user_home = d.join("box1");
    let tehas_home = user_home.join("tehas");
    fs::create_dir_all(user_home.join(".config/tehas")).unwrap();
    fs::create_dir_all(&tehas_home).unwrap();
    let id = json!({ "factory": at, "name": "box1", "key": key });
    let id_file = user_home.join(".config/tehas/contractor.json");
    fs::write(&id_file, id.to_string()).unwrap();
    fs::set_permissions(&id_file, fs::Permissions::from_mode(0o600)).unwrap();
    let seen = d.join("seen");
    fs::create_dir_all(&seen).unwrap();
    let work = d.join("work");
    exe(
        &work,
        &format!(
            r#"#!/bin/sh
seen={seen}
[ -f "$TEHAS_HOME/repos/widget/README" ] && echo cloned >"$seen/clone"
[ -e "$TEHAS_HOME/state/index" ] && echo indexed >"$seen/index"
[ -e "$TEHAS_HOME/work/widget-7.images" ] && echo given >"$seen/files"
mkdir -p "$TEHAS_HOME/state/cycles"
echo '{{"status":"merged","issue":7}}' >"$TEHAS_HOME/state/cycles/7.result.json"
echo 0 >"$TEHAS_HOME/state/cycles/7.runner_exit"
"$TEHAS_CONTRACTOR" drain
"#,
            seen = seen.display()
        ),
    );
    // A job that does not start never runs the stub, which would drain: the
    // run is ended once the factory has the job back.
    let mut ran = Command::new(env!("CARGO_BIN_EXE_tehas-contractor"))
        .arg("run")
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
            format!("file://{}", d.join("git").display()),
        )
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    while ran.try_wait().unwrap().is_none() {
        if call(factory, &["job-show", "--job", "1"])["state"] == "released" {
            std::thread::sleep(Duration::from_millis(300));
            let _ = ran.kill();
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the run did not end"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let out = ran.wait_with_output().unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        seen,
    )
}

#[test]
fn a_job_with_no_index_and_no_files_starts_and_one_with_no_source_does_not() {
    let d = TestDir::new("contractor-no-index");
    let checkout = d.path().join("checkout");
    fs::create_dir_all(&checkout).unwrap();
    let origin = d.path().join("origin.git");
    fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "--bare", "-b", "main"]);
    git(&checkout, &["init", "--quiet", "-b", "main"]);
    fs::write(checkout.join("README"), "widget\n").unwrap();
    git(&checkout, &["add", "README"]);
    git(
        &checkout,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "x",
        ],
    );
    git(
        &checkout,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(&checkout, &["push", "--quiet", "origin", "main"]);
    git(&checkout, &["fetch", "--quiet", "origin"]);
    fn never(_: &Path, _: u64, _: &Path) -> Result<(), String> {
        Err("not asked".into())
    }
    let downloads: &'static OfHome = Box::leak(Box::new(OfHome {
        home: d.path().join("factory"),
        checkout,
        branch: "main".to_string(),
        image: String::new(),
        docker: PathBuf::from("/nonexistent"),
        zstd: false,
        gives_files: false,
        fetch: never,
        making: Default::default(),
    }));
    let (home, at, key) = factory_with(d.path(), Some(downloads));
    let (code, log, seen) = run_box(d.path(), &home, &at, key);
    assert_eq!(code, Some(0), "{log}");
    assert!(
        log.contains("this cycle searches without a code index"),
        "{log}"
    );
    assert!(seen.join("clone").exists(), "{log}");
    assert!(!seen.join("index").exists());
    assert!(!seen.join("files").exists());

    // No source from the factory: the job does not start, and nothing is
    // cloned from anywhere else.
    let e = TestDir::new("contractor-no-source");
    let (home, at, key) = factory_with(e.path(), None);
    let (_, log, seen) = run_box(e.path(), &home, &at, key);
    assert!(
        log.contains("the factory gave no source: 404 no downloads here"),
        "{log}"
    );
    assert!(!seen.join("clone").exists());
    let job = call(&home, &["job-show", "--job", "1"]);
    assert_eq!(job["state"], "released", "{job}");
}
