//! The runner against a host that answers for docker, git, systemd and the
//! store: what it runs, in which order, and what it leaves.

use super::*;
use crate::host::Ended;
use serde_json::json;
use std::collections::BTreeMap;
use std::process::Command;
use tehas_core::plugin::out::{Limits as OutLimits, Taken};
use tehas_core::testdir::TestDir;
use tehas_ops::plugin::exec::fake::{ok, Fake as Answers};

pub const C: &str = "0123456789abcdef0123456789abcdef01234567";
pub const H: &str = "89abcdef0123456789abcdef0123456789abcdef";
pub const H2: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const B: &str = "fedcba9876543210fedcba9876543210fedcba98";
pub const IMAGE: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const NODE: &str = "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

type OnRun = Box<dyn FnMut(&[String])>;

/// A host whose answers a test sets.
pub struct Fake {
    pub x: X,
    pub calls: Vec<String>,
    pub ended: Ended,
    pub argv: Vec<Vec<String>>,
    pub limit: Option<Duration>,
    /// `docker cp` sources and the tar each gives.
    pub tars: BTreeMap<String, Vec<u8>>,
    pub on_run: Option<OnRun>,
    /// The calls of gh, each its words one space apart.
    pub gh: Vec<String>,
    /// What GitHub gives for the pull request; None answers 404.
    pub pull: Option<Value>,
    /// What GitHub answers a status.
    pub statuses: Result<String, String>,
    next: u64,
}

impl Fake {
    pub fn new() -> Fake {
        let mut x = X(Answers::new());
        x.on("docker", ok(""));
        x.on("systemctl", ok(""));
        x.on("git", ok(""));
        x.on("system df", ok("[]"));
        Fake {
            x,
            calls: Vec::new(),
            ended: Ended::Exit(0),
            argv: Vec::new(),
            limit: None,
            tars: BTreeMap::new(),
            on_run: None,
            gh: Vec::new(),
            pull: None,
            statuses: Ok("{}".into()),
            next: 7,
        }
    }
}

impl Host for Fake {
    fn exec(&mut self) -> &mut dyn Exec {
        &mut self.x
    }

    fn store(&mut self, args: &[&str]) -> Result<Value, String> {
        self.calls.push(args.join(" "));
        if args[0] == "plugin-run-begin" {
            self.next += 1;
            return Ok(json!({"run": self.next - 1}));
        }
        Ok(json!({}))
    }

    fn container(&mut self, argv: &[String], limit: Duration, _cidfile: &Path) -> Ended {
        self.argv.push(argv.to_vec());
        self.limit = Some(limit);
        if let Some(f) = self.on_run.as_mut() {
            f(argv);
        }
        self.ended.clone()
    }

    fn copy(
        &mut self,
        from: &str,
        target: &Path,
        top: &str,
        limits: OutLimits,
    ) -> Result<Taken, String> {
        let tar = self.tars.get(from).ok_or(format!("no such path: {from}"))?;
        out::read(&tar[..], target, top, limits)
    }

    fn logs(&mut self, container: &str, to: &Path, _max: u64) -> Result<(), String> {
        fs::write(to, format!("the log of {container}\n")).map_err(|e| e.to_string())
    }

    fn github(&mut self, args: &[&str]) -> Result<String, String> {
        self.gh.push(args.join(" "));
        if args.iter().any(|a| a.contains("/statuses/")) {
            return self.statuses.clone();
        }
        self.pull
            .as_ref()
            .map(Value::to_string)
            .ok_or_else(|| ": HTTP 404".to_string())
    }
}

/// A tar of `files` under `top`, as docker cp gives it. A file named
/// `link` is a symlink.
pub fn tar(top: &str, files: &[(&str, &str)]) -> Vec<u8> {
    let d = TestDir::new("plugin-tar");
    for (name, text) in files {
        let path = d.join(top).join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if *name == "link" {
            std::os::unix::fs::symlink("/etc/passwd", &path).unwrap();
        } else {
            fs::write(&path, text).unwrap();
        }
    }
    if files.is_empty() {
        fs::create_dir_all(d.join(top)).unwrap();
    }
    let out = Command::new("tar")
        .arg("-C")
        .arg(d.path())
        .args(["-cf", "-", top])
        .output()
        .unwrap();
    assert!(out.status.success());
    out.stdout
}

pub fn manifest(asks: &str) -> String {
    let hooks = if asks.contains("hook:after:work") {
        r#", "hooks": [{"at": "after:work", "timeout": 300}]"#
    } else {
        ""
    };
    format!(
        r#"{{"plugin": 1, "kind": "batch", "name": "demo", "label": "Demo", "build": "Dockerfile", "asks": [{asks}], "timeout": 600{hooks}}}"#
    )
}

/// The answers, and a clone that makes its directory as git would.
pub struct X(pub Answers);

impl std::ops::Deref for X {
    type Target = Answers;
    fn deref(&self) -> &Answers {
        &self.0
    }
}

impl std::ops::DerefMut for X {
    fn deref_mut(&mut self) -> &mut Answers {
        &mut self.0
    }
}

impl Exec for X {
    fn run(&mut self, cmd: &Cmd) -> tehas_ops::plugin::exec::Done {
        if cmd.args.first().map(String::as_str) == Some("clone") {
            fs::create_dir_all(cmd.args.last().unwrap()).unwrap();
        }
        self.0.run(cmd)
    }

    fn pipe(&mut self, from: &Cmd, to: &Cmd) -> tehas_ops::plugin::exec::Done {
        self.0.pipe(from, to)
    }
}

pub struct World {
    pub home: TestDir,
    pub vars: BTreeMap<String, String>,
    pub models: PathBuf,
}

impl World {
    pub fn get(&self) -> impl Fn(&str) -> String + '_ {
        move |name: &str| self.vars.get(name).cloned().unwrap_or_default()
    }

    pub fn set(&mut self, k: &str, v: &str) {
        self.vars.insert(k.to_string(), v.to_string());
    }
}

/// A home with the plugin demo ready at C, granted `grants`, whose manifest
/// asks `asks`; `h` answers for its repository and the mirror at H.
pub fn world(name: &str, h: &mut Fake, asks: &str, grants: &str) -> World {
    let home = TestDir::new(name);
    let mut asked = Vec::new();
    for a in grants.split_whitespace() {
        let word = a.split(':').next().unwrap();
        let word = if a.starts_with("action:") || a.starts_with("hook:") {
            a
        } else {
            word
        };
        asked.push(format!("\"{word}\""));
    }
    let m = if asks.is_empty() {
        manifest(&asked.join(", "))
    } else {
        manifest(asks)
    };
    let d = format!("FROM {NODE}\n");
    for (path, text) in [("tehas-plugin.json", &m), ("Dockerfile", &d)] {
        h.x.on(&format!("cat-file -t {C}:{path}"), ok("blob\n"));
        h.x.on(
            &format!("cat-file -s {C}:{path}"),
            ok(&format!("{}\n", text.len())),
        );
        h.x.on(&format!("cat-file blob {C}:{path}"), ok(text));
    }
    h.x.on("refs/remotes/origin/main^{commit}", ok(&format!("{H}\n")));
    h.x.on("rev-parse HEAD", ok(&format!("{H}\n")));
    let r = prepare::read(&mut h.x, &home, "demo", C).unwrap();
    assert!(r.refusals.is_empty(), "{:?}", r.refusals);
    let mut st = State::new(Word::Ready, C);
    st.image = Some(IMAGE.into());
    st.manifest_sha256 = Some(r.sha256);
    fs::create_dir_all(prepare::dir(&home, "demo")).unwrap();
    prepare::write_state(&home, "demo", &st).unwrap();
    let mut vars = BTreeMap::new();
    for (k, v) in [
        ("TEHAS_PLUGIN", "on"),
        ("TEHAS_TEST_FEATURES", "plugins"),
        (
            "TEHAS_PLUGINS",
            &format!("demo=https://example.test/demo@{C}"),
        ),
        ("TEHAS_PLUGIN_GRANTS_DEMO", &format!("{C} {grants}")),
        ("TEHAS_WORK_REPO", "/m/product"),
        ("TEHAS_BRANCH", "main"),
        ("PI_ENSEMBLE_SUBAGENT_PROVIDER", "local"),
        ("PI_ENSEMBLE_SUBAGENT_MODEL", "example-model"),
    ] {
        vars.insert(k.to_string(), v.trim_end().to_string());
    }
    let models = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../instances/example/models.json");
    World { home, vars, models }
}

fn refresh_ask() -> Ask {
    Ask {
        name: "demo".into(),
        kind: Kind::Refresh,
        pr: None,
        trigger: None,
        by: None,
    }
}

fn hook_ask() -> Ask {
    Ask {
        name: "demo".into(),
        kind: Kind::Hook,
        pr: Some(42),
        trigger: None,
        by: None,
    }
}

fn go(h: &mut Fake, w: &World, ask: &Ask) -> u8 {
    let get = w.get();
    let s = Setup {
        home: &w.home,
        get: &get,
        models: &w.models,
        poll: Duration::from_millis(10),
    };
    run(h, &s, ask)
}

fn view(w: &World) -> PathBuf {
    prepare::dir(&w.home, "demo").join("view")
}

#[test]
fn a_refresh_with_every_grant_runs_its_container_and_swaps_its_view_in() {
    let mut h = Fake::new();
    let w = world("plugin-run-all", &mut h, "", "view product model:2000");
    fs::create_dir_all(view(&w)).unwrap();
    fs::write(view(&w).join("old.html"), "old").unwrap();
    h.tars.insert(
        "tehas-plugin-demo-7:/out".into(),
        tar("out", &[("index.html", "<p>map</p>"), ("map/a.js", "1")]),
    );
    let dir = prepare::dir(&w.home, "demo").join("run-7");
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let saw = seen.clone();
    let d = dir.clone();
    h.on_run = Some(Box::new(move |_| {
        let ctx: Value =
            serde_json::from_slice(&fs::read(d.join("context.json")).unwrap()).unwrap();
        saw.borrow_mut().push(ctx);
        let socket = d.join("model/model.sock");
        assert!(
            fs::symlink_metadata(&socket).is_ok(),
            "the socket is served"
        );
        let mode = fs::metadata(d.join("model")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        let mode = fs::metadata(d.join("context.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);
    }));
    assert_eq!(go(&mut h, &w, &refresh_ask()), 0);
    assert_eq!(
        seen.borrow()[0],
        json!({"kind": "refresh", "trigger": "host", "commit": C, "head": H,
               "model": {"id": "example-model", "tokens": 2000}})
    );
    assert_eq!(
        h.calls,
        [
            format!("plugin-run-begin --plugin demo --commit {C} --kind refresh --how host --by host --head {H}"),
            "plugin-run-end --run 7 --result done --tokens 0".to_string(),
        ]
    );
    // The argument list is the pure function's, and the timeout the manifest's.
    let m: Manifest = tehas_core::plugin::manifest::read(
        manifest(r#""view", "product", "model""#).as_bytes(),
        "demo",
    )
    .unwrap();
    let g = plugin::grants(&format!("{C} view product model:2000")).unwrap();
    let dir_text = dir.display().to_string();
    let want = sandbox::plugin_run(&Run {
        manifest: &m,
        grants: &g,
        commit: C,
        kind: Kind::Refresh,
        trigger: Trigger::Host,
        id: 7,
        dir: &dir_text,
        image: IMAGE,
        limits: LIMITS,
        head: H,
        model: "example-model",
        pr: None,
        tool: None,
    })
    .unwrap();
    assert_eq!(h.argv, [want]);
    assert_eq!(h.limit, Some(Duration::from_secs(600)));
    // The product: a clone of the mirror's commit, no hook, no remote.
    let git: Vec<&String> = h.x.ran.iter().filter(|l| l.starts_with("git")).collect();
    let product = dir.display().to_string() + "/product";
    let wanted = [
        format!("git clone --quiet --no-local --no-checkout --depth 200 --single-branch --branch main -c core.hooksPath=/dev/null /m/product {product}"),
        format!("git -C {product} fetch --quiet --no-tags --depth 200 origin +refs/remotes/origin/main:refs/tehas/head"),
        format!("git -C {product} remote remove origin"),
        format!("git -C {product} -c advice.detachedHead=false checkout --quiet --detach {H}"),
        format!("git -C {product} rev-parse HEAD"),
    ];
    for want in &wanted {
        assert!(git.contains(&want), "{want} in {git:?}");
    }
    // The view is the new one, and nothing of the run is left.
    assert_eq!(
        fs::read_to_string(view(&w).join("index.html")).unwrap(),
        "<p>map</p>"
    );
    assert!(!view(&w).join("old.html").exists());
    assert!(!prepare::dir(&w.home, "demo").join("view-7").exists());
    assert!(!dir.exists());
    assert!(h.x.ran_any("docker rm -f tehas-plugin-demo-7"));
    assert!(h
        .x
        .ran_any("docker volume rm -f tehas-plugin-demo-work-7 tehas-plugin-demo-out-7"));
    assert_eq!(
        fs::read_to_string(runs::log(&w.home, "demo", None)).unwrap(),
        "the log of tehas-plugin-demo-7\n"
    );
    assert_eq!(
        fs::read_to_string(runs::tried(&w.home, "demo")).unwrap(),
        format!("{H}\n")
    );
}

#[test]
fn a_refresh_the_timer_asked_for_says_so_in_its_row() {
    let mut h = Fake::new();
    let w = world("plugin-run-timer", &mut h, "", "");
    let asked = Asked {
        trigger: Trigger::Timer,
        by: "timer".into(),
    };
    runs::ask_refresh(&mut h.x, &w.home, "demo", &asked).unwrap();
    assert_eq!(go(&mut h, &w, &refresh_ask()), 0);
    assert!(
        h.calls[0].contains("--how timer --by timer"),
        "{}",
        h.calls[0]
    );
    assert!(!runs::request(&w.home, "demo").exists());
    // No product: no clone; no view: /out is not read.
    assert!(!h.x.ran_any("clone"));
    assert!(!view(&w).exists());
}

#[test]
fn a_failed_or_killed_run_keeps_the_last_good_view() {
    for (ended, result, rc) in [
        (Ended::Exit(2), "failed", 1),
        (Ended::Killed, "timeout", 1),
        (
            Ended::NotRun("docker run exited 125: no /work".into()),
            "failed",
            1,
        ),
    ] {
        let mut h = Fake::new();
        let w = world("plugin-run-fails", &mut h, "", "view");
        fs::create_dir_all(view(&w)).unwrap();
        fs::write(view(&w).join("index.html"), "good").unwrap();
        h.ended = ended;
        h.tars.insert(
            "tehas-plugin-demo-7:/out".into(),
            tar("out", &[("index.html", "new")]),
        );
        assert_eq!(go(&mut h, &w, &refresh_ask()), rc);
        assert_eq!(
            h.calls[1],
            format!("plugin-run-end --run 7 --result {result} --tokens 0")
        );
        assert_eq!(
            fs::read_to_string(view(&w).join("index.html")).unwrap(),
            "good"
        );
        assert!(h.x.ran_any("docker rm -f tehas-plugin-demo-7"));
    }
}

#[test]
fn an_out_with_a_link_fails_the_run_and_leaves_the_view() {
    let mut h = Fake::new();
    let w = world("plugin-run-link", &mut h, "", "view");
    fs::create_dir_all(view(&w)).unwrap();
    fs::write(view(&w).join("index.html"), "good").unwrap();
    h.tars.insert(
        "tehas-plugin-demo-7:/out".into(),
        tar("out", &[("index.html", "x"), ("link", "")]),
    );
    assert_eq!(go(&mut h, &w, &refresh_ask()), 1);
    assert!(h.calls[1].contains("--result failed"));
    assert_eq!(
        fs::read_to_string(view(&w).join("index.html")).unwrap(),
        "good"
    );
    assert!(!prepare::dir(&w.home, "demo").join("view-7").exists());
}

#[test]
fn volumes_past_the_disk_fail_the_run_and_a_full_state_refuses_the_plugin() {
    let mut h = Fake::new();
    let w = world("plugin-run-disk", &mut h, "", "");
    h.x.on(
        "system df",
        ok(r#"[{"Name":"tehas-plugin-demo-work-7","Size":"1.5GB"},{"Name":"tehas-plugin-demo-state","Size":"5GB"},{"Name":"other","Size":"90GB"}]"#),
    );
    assert_eq!(go(&mut h, &w, &refresh_ask()), 1);
    assert!(h.calls[1].contains("--result failed"));
    assert!(h.x.ran_any("docker volume rm -f tehas-plugin-demo-state"));
    let st = prepare::read_state(&w.home, "demo").unwrap();
    assert_eq!(st.state, Word::Refused);
    assert_eq!(st.refusal.as_deref(), Some(DISK_REFUSAL));
    assert_eq!(st.image.as_deref(), Some(IMAGE));
    // The timer does not run it again; a person does.
    let timer = Ask {
        trigger: Some(Trigger::Timer),
        ..refresh_ask()
    };
    h.x.on("system df", ok("[]"));
    assert_eq!(go(&mut h, &w, &timer), 3);
    assert_eq!(go(&mut h, &w, &refresh_ask()), 0);
    assert_eq!(
        prepare::read_state(&w.home, "demo").unwrap().state,
        Word::Ready
    );
}

#[test]
fn a_run_that_may_not_start_writes_no_row() {
    let mut h = Fake::new();
    let mut w = world("plugin-run-not", &mut h, "", "");
    let lock = File::create(runs::refresh_lock(&w.home, "demo")).unwrap();
    lock.lock().unwrap();
    assert_eq!(go(&mut h, &w, &refresh_ask()), 3);
    drop(lock);
    let other = Ask {
        name: "other".into(),
        ..refresh_ask()
    };
    assert_eq!(go(&mut h, &w, &other), 64);
    w.set("TEHAS_PLUGIN_GRANTS_DEMO", &format!("{H} product"));
    assert_eq!(go(&mut h, &w, &refresh_ask()), 3);
    w.set("TEHAS_PLUGIN_GRANTS_DEMO", C);
    prepare::write_state(&w.home, "demo", &State::refused(C, "no")).unwrap();
    assert_eq!(go(&mut h, &w, &refresh_ask()), 3);
    w.set("TEHAS_WORK_REPO", "");
    assert_eq!(go(&mut h, &w, &refresh_ask()), 78);
    w.set("TEHAS_PLUGIN", "off");
    assert_eq!(go(&mut h, &w, &refresh_ask()), 78);
    assert!(h.calls.is_empty());
    assert!(h.argv.is_empty());
}

#[test]
fn a_manifest_changed_since_the_update_is_not_run() {
    let mut h = Fake::new();
    let w = world("plugin-run-changed", &mut h, "", "");
    let mut st = prepare::read_state(&w.home, "demo").unwrap();
    st.manifest_sha256 = Some("0".repeat(64));
    prepare::write_state(&w.home, "demo", &st).unwrap();
    assert_eq!(go(&mut h, &w, &refresh_ask()), 3);
    assert!(h.calls.is_empty());
}

fn ask_hook_on(w: &World, h: &mut Fake, head: &str) {
    let ask = HookAsk {
        pr: 42,
        head: head.into(),
        base: B.into(),
        issue: Some(7),
        trigger: Trigger::Cycle,
    };
    runs::ask_hook(&mut h.x, &w.home, "demo", &ask).unwrap();
}

#[test]
fn a_hook_reads_its_pull_request_and_keeps_its_verdict() {
    let mut h = Fake::new();
    let w = world("plugin-run-hook", &mut h, "", "product hook:after:work");
    ask_hook_on(&w, &mut h, H);
    h.x.on("refs/tehas/plugin-pr/42^{commit}", ok(&format!("{H}\n")));
    let verdict = r#"{"verdict": "note", "summary": "One new finding.", "findings": [{"file": "a.js", "line": 3, "text": "no sign-in"}]}"#;
    h.tars.insert(
        "tehas-plugin-demo-7:/out/_tehas".into(),
        tar("_tehas", &[("verdict.json", verdict)]),
    );
    assert_eq!(go(&mut h, &w, &hook_ask()), 0);
    assert!(h.x.ran_any(
        "git -C /m/product fetch --quiet --no-tags origin +refs/pull/42/head:refs/tehas/plugin-pr/42"
    ));
    assert!(h
        .x
        .ran_any("origin +refs/tehas/plugin-pr/42:refs/tehas/head"));
    assert!(h
        .x
        .ran_any("git -C /m/product update-ref -d refs/tehas/plugin-pr/42"));
    assert_eq!(
        h.calls[0],
        format!("plugin-run-begin --plugin demo --commit {C} --kind hook --how cycle --by host --head {H}")
    );
    assert_eq!(h.limit, Some(Duration::from_secs(300)));
    let argv = &h.argv[0];
    assert!(
        argv.contains(&"type=volume,src=tehas-plugin-demo-state,dst=/state,readonly".to_string())
    );
    let kept: Value =
        serde_json::from_slice(&fs::read(runs::verdict(&w.home, "demo", 42)).unwrap()).unwrap();
    assert_eq!(
        kept,
        json!({"api": 1, "pr": 42, "head": H, "result": "done", "verdict": {
            "verdict": "note", "summary": "One new finding.",
            "findings": [{"file": "a.js", "line": 3, "text": "no sign-in"}]}})
    );
    assert!(!runs::hook_request(&w.home, "demo", 42).exists());
    assert!(runs::log(&w.home, "demo", Some(42)).exists());
}

#[test]
fn a_hook_with_a_bad_verdict_did_not_finish() {
    let mut h = Fake::new();
    let w = world("plugin-run-hook-bad", &mut h, "", "hook:after:work");
    ask_hook_on(&w, &mut h, H);
    h.tars.insert(
        "tehas-plugin-demo-7:/out/_tehas".into(),
        tar("_tehas", &[("verdict.json", r#"{"verdict": "fine"}"#)]),
    );
    assert_eq!(go(&mut h, &w, &hook_ask()), 1);
    let kept: Value =
        serde_json::from_slice(&fs::read(runs::verdict(&w.home, "demo", 42)).unwrap()).unwrap();
    assert_eq!(kept["result"], "failed");
    assert_eq!(kept["verdict"], Value::Null);
    // No product: the mirror is not asked for the pull request.
    assert!(!h.x.ran_any("refs/pull/42"));
}

#[test]
fn a_newer_request_that_came_during_a_hook_runs_after_it() {
    let mut h = Fake::new();
    let w = world("plugin-run-hook-again", &mut h, "", "hook:after:work");
    ask_hook_on(&w, &mut h, H);
    h.tars.insert(
        "tehas-plugin-demo-7:/out/_tehas".into(),
        tar(
            "_tehas",
            &[("verdict.json", r#"{"verdict": "pass", "summary": "ok"}"#)],
        ),
    );
    h.tars.insert(
        "tehas-plugin-demo-8:/out/_tehas".into(),
        tar(
            "_tehas",
            &[("verdict.json", r#"{"verdict": "pass", "summary": "ok"}"#)],
        ),
    );
    let home = w.home.to_path_buf();
    let mut first = true;
    h.on_run = Some(Box::new(move |_| {
        if first {
            first = false;
            let ask = HookAsk {
                pr: 42,
                head: H2.into(),
                base: B.into(),
                issue: None,
                trigger: Trigger::Cycle,
            };
            let path = runs::hook_request(&home, "demo", 42);
            fs::write(path, ask.json().to_string()).unwrap();
        }
    }));
    assert_eq!(go(&mut h, &w, &hook_ask()), 0);
    assert_eq!(h.argv.len(), 2);
    assert!(
        h.calls[2].ends_with(&format!("--head {H2}")),
        "{:?}",
        h.calls
    );
}

#[test]
fn a_hook_with_no_request_or_grant_does_not_run() {
    let mut h = Fake::new();
    let w = world("plugin-run-hook-none", &mut h, "", "hook:after:work");
    assert_eq!(go(&mut h, &w, &hook_ask()), 3);
    let mut h = Fake::new();
    let w = world("plugin-run-hook-ungranted", &mut h, "", "product");
    ask_hook_on(&w, &mut h, H);
    assert_eq!(go(&mut h, &w, &hook_ask()), 3);
    assert!(h.calls.is_empty());
}

#[test]
fn the_words_of_a_run() {
    let w = |s: &[&str]| words(&s.iter().map(|x| x.to_string()).collect::<Vec<_>>());
    assert_eq!(w(&["demo", "--kind", "refresh"]), Some(refresh_ask()));
    assert_eq!(w(&["demo-42", "--kind", "hook"]), Some(hook_ask()));
    assert_eq!(
        w(&["demo", "--kind", "hook", "--pr", "42"]),
        Some(hook_ask())
    );
    assert_eq!(
        w(&[
            "demo",
            "--kind",
            "refresh",
            "--trigger",
            "action",
            "--by",
            "someone"
        ]),
        Some(Ask {
            trigger: Some(Trigger::Action),
            by: Some("someone".into()),
            ..refresh_ask()
        })
    );
    for bad in [
        &["demo"][..],
        &["demo", "--kind", "other"],
        &["demo", "--kind", "refresh", "--pr", "3"],
        &["demo", "--kind", "hook"],
        &["demo-x", "--kind", "hook"],
        &["demo", "--kind", "refresh", "--by", "someone"],
        &["demo", "--kind", "refresh", "--trigger", "later"],
        &["Demo", "--kind", "refresh"],
        &["demo", "--kind"],
    ] {
        assert_eq!(w(bad), None, "{bad:?}");
    }
}

#[test]
fn a_size_of_docker_system_df_is_read_in_bytes() {
    assert_eq!(bytes_of("0B"), Some(0));
    assert_eq!(bytes_of("12.5kB"), Some(12_500));
    assert_eq!(bytes_of("1.5GB"), Some(1_500_000_000));
    assert_eq!(bytes_of("3 MB"), Some(3_000_000));
    assert_eq!(bytes_of("N/A"), None);
    assert_eq!(bytes_of("1XB"), None);
}

#[test]
fn a_view_is_swapped_in_whole() {
    let d = TestDir::new("plugin-swap");
    let (new, view) = (d.join("view-1"), d.join("view"));
    fs::create_dir_all(&new).unwrap();
    fs::write(new.join("a"), "1").unwrap();
    swap_view(&new, &view).unwrap();
    assert_eq!(fs::read_to_string(view.join("a")).unwrap(), "1");
    fs::create_dir_all(&new).unwrap();
    fs::write(new.join("b"), "2").unwrap();
    swap_view(&new, &view).unwrap();
    assert!(!view.join("a").exists());
    assert_eq!(fs::read_to_string(view.join("b")).unwrap(), "2");
    assert!(!new.exists());
}

#[test]
fn images_reach_the_model_only_when_asked_and_granted() {
    let read = |asks: &str| -> Manifest {
        tehas_core::plugin::manifest::read(manifest(asks).as_bytes(), "demo").unwrap()
    };
    let g = |words: &str| plugin::grants(&format!("{C} {words}")).unwrap();
    let both = read(r#""model", "model:images""#);
    assert_eq!(
        content(&both, &g("model:2000 model:images")),
        Content::TextAndImages
    );
    assert_eq!(content(&both, &g("model:2000")), Content::Text);
    let text = read(r#""model""#);
    assert_eq!(content(&text, &g("model:2000 model:images")), Content::Text);
}

#[test]
fn a_hook_granted_the_model_gets_no_socket() {
    let mut h = Fake::new();
    let w = world(
        "plugin-run-hook-model",
        &mut h,
        "",
        "model:2000 hook:after:work",
    );
    ask_hook_on(&w, &mut h, H);
    h.tars.insert(
        "tehas-plugin-demo-7:/out/_tehas".into(),
        tar(
            "_tehas",
            &[("verdict.json", r#"{"verdict": "pass", "summary": "ok"}"#)],
        ),
    );
    let d = prepare::dir(&w.home, "demo").join("run-7");
    let ctx = std::rc::Rc::new(std::cell::RefCell::new(Value::Null));
    let seen = ctx.clone();
    h.on_run = Some(Box::new(move |_| {
        assert!(!d.join("model").exists(), "no socket's directory");
        *seen.borrow_mut() =
            serde_json::from_slice(&fs::read(d.join("context.json")).unwrap()).unwrap();
    }));
    assert_eq!(go(&mut h, &w, &hook_ask()), 0);
    assert_eq!(ctx.borrow().get("model"), None);
    assert!(!h.argv[0].iter().any(|a| a.contains("dst=/run/tehas")));
}

/// A world whose GitHub knows the repository, the bot and the pull request
/// at `head`, its plugin granted `grants`.
fn on_github(name: &str, h: &mut Fake, grants: &str, head: &str) -> World {
    let mut w = world(name, h, "", grants);
    w.set("TEHAS_REPO", "acme/widget");
    w.set("TEHAS_BOT_LOGIN", "acme-bot[bot]");
    w.set("TEHAS_DOMAIN", "example.org");
    h.pull = Some(json!({
        "state": "open", "merged": false,
        "user": {"login": "acme-bot[bot]"},
        "head": {"sha": head, "repo": {"full_name": "acme/widget"}},
        "base": {"sha": B},
    }));
    for id in ["7", "8", "9"] {
        h.tars.insert(
            format!("tehas-plugin-demo-{id}:/out/_tehas"),
            tar(
                "_tehas",
                &[(
                    "verdict.json",
                    r#"{"verdict": "note", "summary": "One new finding at @octocat."}"#,
                )],
            ),
        );
    }
    w
}

#[test]
fn a_hooks_end_posts_one_status_on_the_head_it_read() {
    let mut h = Fake::new();
    let w = on_github("plugin-run-status", &mut h, "view hook:after:work", H);
    ask_hook_on(&w, &mut h, H);
    assert_eq!(go(&mut h, &w, &hook_ask()), 0);
    let post = format!("api -X POST repos/acme/widget/statuses/{H} -f state=success -f context=tehas/plugin/demo -f description=One new finding at @\u{200b}octocat. -f target_url=https://plugin-demo.example.org/_hooks/42");
    assert_eq!(h.gh, ["api repos/acme/widget/pulls/42".to_string(), post]);
    assert_eq!(h.argv.len(), 1);
    assert!(!runs::unwritten(&w.home, "demo").exists());
}

#[test]
fn a_hook_that_did_not_finish_is_an_error_with_no_target_without_a_view() {
    let mut h = Fake::new();
    let w = on_github("plugin-run-status-error", &mut h, "hook:after:work", H);
    ask_hook_on(&w, &mut h, H);
    h.ended = Ended::Killed;
    assert_eq!(go(&mut h, &w, &hook_ask()), 1);
    assert_eq!(
        h.gh[1],
        format!("api -X POST repos/acme/widget/statuses/{H} -f state=error -f context=tehas/plugin/demo -f description=the plugin's hook did not finish")
    );
}

#[test]
fn a_head_that_moved_while_the_hook_ran_is_asked_for_again() {
    let mut h = Fake::new();
    let w = on_github("plugin-run-status-moved", &mut h, "hook:after:work", H2);
    ask_hook_on(&w, &mut h, H);
    assert_eq!(go(&mut h, &w, &hook_ask()), 0);
    // The verdict of H is posted on H, then H2 is asked for and run.
    assert_eq!(h.argv.len(), 2);
    let posts: Vec<&String> = h.gh.iter().filter(|c| c.contains("/statuses/")).collect();
    assert_eq!(posts.len(), 2);
    assert!(posts[0].contains(&format!("statuses/{H} ")));
    assert!(posts[1].contains(&format!("statuses/{H2} ")));
    assert!(
        h.calls[2].ends_with(&format!("--head {H2}")),
        "{:?}",
        h.calls
    );
    // The first request's start, then the new head's.
    let starts =
        h.x.ran
            .iter()
            .filter(|c| c.starts_with("systemctl"))
            .count();
    assert_eq!(starts, 2);
    assert!(!runs::hook_request(&w.home, "demo", 42).exists());
}

#[test]
fn a_status_github_refuses_the_app_is_kept_for_tehas_plugin() {
    let mut h = Fake::new();
    let w = on_github("plugin-run-status-refused", &mut h, "hook:after:work", H);
    ask_hook_on(&w, &mut h, H);
    h.statuses = Err(": HTTP 403".into());
    assert_eq!(go(&mut h, &w, &hook_ask()), 0);
    // A refusal is not tried again.
    assert_eq!(h.gh.iter().filter(|c| c.contains("/statuses/")).count(), 1);
    assert_eq!(
        runs::status_unwritten(&w.home, "demo").as_deref(),
        Some("GitHub refused the status on #42: HTTP 403: the App needs Commit statuses: Read and write")
    );
    // Any other failure is tried once more, and a status written clears it.
    h.gh.clear();
    h.statuses = Err(": HTTP 502".into());
    ask_hook_on(&w, &mut h, H);
    assert_eq!(go(&mut h, &w, &hook_ask()), 0);
    assert_eq!(h.gh.iter().filter(|c| c.contains("/statuses/")).count(), 2);
    assert!(runs::status_unwritten(&w.home, "demo").is_some());
    h.statuses = Ok("{}".into());
    ask_hook_on(&w, &mut h, H);
    assert_eq!(go(&mut h, &w, &hook_ask()), 0);
    assert_eq!(runs::status_unwritten(&w.home, "demo"), None);
}
