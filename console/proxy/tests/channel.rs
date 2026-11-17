//! The job channel against a real state store: who may call, what each call
//! needs of the job, and what it does. The spec's "Tests" 13.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tehas_core::testdir::TestDir;
use tehas_cycle::relay::Scope;
use tehas_proxy::channel::{key_hash, Answer, Asked, Call, Channel, Factory};
use tehas_proxy::jobs::Issuer;
use tehas_proxy::records::KeptIn;

const KEY: &str = "the-contractor-key-of-box1";

fn call(home: &Path, args: &[&str]) -> Value {
    let text = store::call(home, args, &mut std::io::empty())
        .unwrap_or_else(|e| panic!("{args:?}: {e:?}"));
    serde_json::from_str(&text).unwrap()
}

fn made(name: &str, outsource: bool) -> TestDir {
    let home = TestDir::new(name);
    std::fs::create_dir_all(home.path().join("state")).unwrap();
    let mut args = vec![
        "init",
        "--version",
        "v0.30.0",
        "--commit",
        "cb6912a",
        "--instance",
        "example",
        "--repo",
        "example-owner/example-product",
    ];
    if outsource {
        args.push("--outsource");
    }
    call(home.path(), &args);
    home
}

/// The factory of a test: the real store of `home`, and the rest written down.
struct Fake {
    home: PathBuf,
    kept: Mutex<Vec<(i64, String, i64)>>,
    relayed: Mutex<Vec<Vec<String>>>,
    token: Option<(String, i64)>,
    /// The names TEHAS_OUTSOURCE_CONTRACTORS lists.
    allowed: Vec<&'static str>,
    /// The store gives no answer while this is set: busy, locked, failing.
    down: AtomicBool,
}

impl Fake {
    fn at(home: &Path) -> Fake {
        Fake {
            home: home.to_path_buf(),
            kept: Mutex::new(Vec::new()),
            relayed: Mutex::new(Vec::new()),
            token: Some(("ghs_cycle".to_string(), 99)),
            allowed: vec!["box1"],
            down: AtomicBool::new(false),
        }
    }
}

impl Factory for Fake {
    fn store(&self, args: &[&str]) -> Option<Value> {
        if self.down.load(Ordering::SeqCst) {
            return None;
        }
        let text = store::call(&self.home, args, &mut std::io::empty()).ok()?;
        serde_json::from_str(&text).ok()
    }
    fn cycle_token(&self) -> Option<(String, i64)> {
        self.token.clone()
    }
    fn relay(
        &self,
        args: &[String],
        input: Option<&str>,
        scope: &mut Scope,
    ) -> Result<(String, String, i32), String> {
        struct Gh<'a>(&'a Mutex<Vec<Vec<String>>>);
        impl tehas_cycle::relay::Run for Gh<'_> {
            fn run(&self, argv: &[String], _: Option<&str>) -> (String, String, i32) {
                self.0.lock().unwrap().push(argv.to_vec());
                ("posted\n".to_string(), String::new(), 0)
            }
        }
        tehas_cycle::relay::answer_request(
            args,
            input,
            "example-owner/example-product",
            &Gh(&self.relayed),
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
        json!({ "settings": { "TEHAS_REPO": "example-owner/example-product" }, "models": {} })
    }
    fn timeout(&self) -> i64 {
        600
    }
    fn random(&self) -> Option<String> {
        thread_local!(static N: RefCell<u32> = const { RefCell::new(0) });
        N.with(|n| {
            *n.borrow_mut() += 1;
            Some(format!("jobtoken{:024}", n.borrow()))
        })
    }
    fn allowed(&self, name: &str) -> bool {
        self.allowed.contains(&name)
    }
}

fn ask(
    chan: &Channel,
    call: Call,
    bearer: Option<&str>,
    name: Option<&str>,
    body: Value,
) -> Answer {
    let body = if body.is_null() {
        Vec::new()
    } else {
        body.to_string().into_bytes()
    };
    chan.answer(&Asked {
        call,
        bearer,
        name,
        body: &body,
    })
}

fn by_key(chan: &Channel, call: Call) -> Answer {
    ask(chan, call, Some(KEY), Some("box1"), Value::Null)
}

/// The row of an issue a job may name, as the poll leaves it.
fn issue(home: &Path, n: i64) {
    let c = rusqlite::Connection::open(home.join("state/tehas.db")).unwrap();
    c.execute(
        "INSERT OR IGNORE INTO issues (issue, kind, kind_at, checked_at) VALUES (?1, 'issue', 1, 1)",
        [n],
    )
    .unwrap();
}

/// A factory with box1 joined by its key and one job of issue 7 offered to it.
fn offered(name: &str) -> (TestDir, Issuer) {
    let home = made(name, true);
    let h = home.path();
    issue(h, 7);
    call(
        h,
        &["contractor-add", "--name", "box1", "--key", &key_hash(KEY)],
    );
    let v = call(h, &["job-offer", "--issue", "7", "--contractor", "box1"]);
    assert_eq!(v["verb"], "offered", "{v}");
    (home, Issuer::new())
}

/// The job taken: its token.
fn taken(chan: &Channel) -> String {
    let a = by_key(chan, Call::Job);
    assert_eq!(a.status, 200, "{a:?}");
    a.body["token"].as_str().unwrap().to_string()
}

#[test]
fn a_path_names_its_call_or_none() {
    assert_eq!(Call::of("/outsource/v1/check-in"), Some(Call::CheckIn));
    assert_eq!(Call::of("/outsource/v1/result?x=1"), Some(Call::Result));
    assert_eq!(Call::of("/outsource/v1/models"), None);
    assert_eq!(Call::of("/v1/chat/completions"), None);
}

#[test]
fn a_host_joins_once_with_its_code_and_the_store_keeps_only_hashes() {
    let home = made("chan-join", true);
    let h = home.path();
    let code = "the-printed-code";
    call(
        h,
        &["invite-new", "--name", "box1", "--code", &key_hash(code)],
    );
    let fake = Fake::at(h);
    let issuer = Issuer::new();
    let chan = Channel::new(&issuer, &fake);
    let body = json!({ "code": code, "name": "box1", "key": key_hash(KEY) });
    let a = ask(&chan, Call::Join, None, None, body.clone());
    assert_eq!((a.status, a.body["joined"].as_str()), (200, Some("box1")));
    let again = ask(&chan, Call::Join, None, None, body);
    assert_eq!((again.status, again.refused()), (403, Some("expired")));
    let wrong = json!({ "code": "guessed", "name": "box1", "key": key_hash(KEY) });
    assert_eq!(
        ask(&chan, Call::Join, None, None, wrong).refused(),
        Some("unknown")
    );
    let db = std::fs::read(h.join("state/tehas.db")).unwrap();
    let text = String::from_utf8_lossy(&db);
    assert!(!text.contains(code) && !text.contains(KEY));
    // The joined key says hello; another key does not.
    assert_eq!(by_key(&chan, Call::Hello).status, 200);
    let a = ask(
        &chan,
        Call::Hello,
        Some("another-key"),
        Some("box1"),
        Value::Null,
    );
    assert_eq!((a.status, a.refused()), (401, Some("no contractor")));
}

#[test]
fn a_contractor_takes_the_offer_in_its_hand_once_and_its_job_runs() {
    let (home, issuer) = offered("chan-job");
    let fake = Fake::at(home.path());
    let chan = Channel::new(&issuer, &fake);
    let a = by_key(&chan, Call::Job);
    assert_eq!(a.status, 200);
    assert_eq!(a.body["issue"], 7);
    assert_eq!(a.body["attempt"], 1);
    assert_eq!(a.body["timeout"], 600);
    assert_eq!(
        a.body["settings"]["TEHAS_REPO"],
        "example-owner/example-product"
    );
    let token = a.body["token"].as_str().unwrap();
    let job = a.body["job"].as_i64().unwrap();
    assert_eq!(issuer.peek(token).map(|l| l.job), Some(job));
    let shown = call(home.path(), &["job-show", "--job", &job.to_string()]);
    assert_eq!(shown["state"], "running");
    // Asked again, the hand is empty.
    assert_eq!(by_key(&chan, Call::Job).body["job"], Value::Null);
    // No reply carries the App's key or the factory's endpoint.
    assert!(!a.body.to_string().contains("BEGIN"));
}

#[test]
fn a_draining_contractor_is_given_no_job() {
    let (home, issuer) = offered("chan-drain");
    call(home.path(), &["contractor-drain", "--name", "box1"]);
    let fake = Fake::at(home.path());
    let chan = Channel::new(&issuer, &fake);
    let a = by_key(&chan, Call::Job);
    assert_eq!((a.status, a.body["job"].clone()), (200, Value::Null));
    assert_eq!(by_key(&chan, Call::Hello).body["draining"], true);
}

#[test]
fn check_in_token_and_relay_need_a_running_job_of_the_current_attempt() {
    let (home, issuer) = offered("chan-running");
    let fake = Fake::at(home.path());
    let chan = Channel::new(&issuer, &fake);
    let token = taken(&chan);
    let t = Some(token.as_str());
    let a = ask(&chan, Call::CheckIn, t, None, json!({}));
    assert_eq!(a.status, 200, "{a:?}");
    let a = ask(&chan, Call::Token, t, None, json!({}));
    assert_eq!(
        (a.status, a.body["token"].as_str()),
        (200, Some("ghs_cycle"))
    );
    // A relayed write the local relay would make is made, on the job's issue.
    let write = json!({ "argv": ["issue", "comment", "7", "--body-file=-", "--repo=example-owner/example-product"], "input": "done" });
    let a = ask(&chan, Call::Relay, t, None, write);
    assert_eq!((a.status, a.body["rc"].as_i64()), (200, Some(0)), "{a:?}");
    // One the local relay refuses is refused here too, and nothing runs.
    let label = json!({ "argv": ["issue", "edit", "7", "--add-label", "x"] });
    let a = ask(&chan, Call::Relay, t, None, label);
    assert_eq!(a.body["rc"], 1);
    assert!(a.body["err"]
        .as_str()
        .unwrap()
        .contains("refuses --add-label"));
    let other = json!({ "argv": ["issue", "comment", "8", "--body", "x"] });
    assert_eq!(ask(&chan, Call::Relay, t, None, other).body["rc"], 1);
    assert_eq!(fake.relayed.lock().unwrap().len(), 1);
    // The job stopping: check-in, token and relay are refused by its state.
    let job = issuer.peek(&token).unwrap().job.to_string();
    call(home.path(), &["job-stop", "--job", &job]);
    for c in [Call::CheckIn, Call::Token, Call::Relay] {
        let a = ask(&chan, c, t, None, json!({ "argv": [] }));
        assert_eq!((a.status, a.refused()), (403, Some("stopping")), "{c:?}");
    }
    // A token the factory never issued is no job.
    let a = ask(&chan, Call::CheckIn, Some("forged"), None, json!({}));
    assert_eq!((a.status, a.refused()), (401, Some("no job")));
}

#[test]
fn a_result_is_taken_while_running_stopping_or_lost_to_a_silence() {
    for (case, then) in [("running", ""), ("stopping", "job-stop"), ("lost", "lose")] {
        let (home, issuer) = offered(&format!("chan-result-{case}"));
        let fake = Fake::at(home.path());
        let chan = Channel::new(&issuer, &fake);
        let token = taken(&chan);
        let job = issuer.peek(&token).unwrap().job.to_string();
        match then {
            "job-stop" => {
                call(home.path(), &["job-stop", "--job", &job]);
            }
            "lose" => {
                call(
                    home.path(),
                    &["job-lose", "--job", &job, "--reason", "timeout"],
                );
            }
            _ => {}
        }
        let a = ask(
            &chan,
            Call::Result,
            Some(&token),
            None,
            json!({ "result": { "status": "merged" }, "exit": 0 }),
        );
        assert_eq!(a.status, 200, "{case}: {a:?}");
        let kept = fake.kept.lock().unwrap().clone();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].0, 7);
        assert_eq!(kept[0].2, 0);
        assert_eq!(
            call(home.path(), &["job-show", "--job", &job])["state"],
            "finished"
        );
        // The token is spent with the job: the model paths and the channel
        // refuse it from now on.
        assert_eq!(issuer.peek(&token), None);
    }
}

#[test]
fn a_revoked_contractor_is_told_so_and_its_result_is_refused() {
    let (home, issuer) = offered("chan-revoked");
    let fake = Fake::at(home.path());
    let chan = Channel::new(&issuer, &fake);
    let token = taken(&chan);
    call(home.path(), &["contractor-revoke", "--name", "box1"]);
    let a = ask(&chan, Call::CheckIn, Some(&token), None, json!({}));
    assert_eq!((a.status, a.refused()), (403, Some("revoked")));
    // Its token went with the revocation.
    let a = ask(
        &chan,
        Call::Result,
        Some(&token),
        None,
        json!({ "result": {}, "exit": 0 }),
    );
    assert_eq!(a.status, 401);
    assert!(fake.kept.lock().unwrap().is_empty());
    assert_eq!(by_key(&chan, Call::Hello).refused(), Some("revoked"));
}

#[test]
fn a_lost_job_is_refused_by_its_state_not_as_revoked() {
    let (home, issuer) = offered("chan-lost");
    let fake = Fake::at(home.path());
    let chan = Channel::new(&issuer, &fake);
    let token = taken(&chan);
    let job = issuer.peek(&token).unwrap().job.to_string();
    call(
        home.path(),
        &["job-lose", "--job", &job, "--reason", "timeout"],
    );
    let a = ask(&chan, Call::CheckIn, Some(&token), None, json!({}));
    assert_eq!((a.status, a.refused()), (403, Some("lost")));
}

#[test]
fn a_result_of_a_replaced_attempt_is_refused() {
    let (home, issuer) = offered("chan-stale");
    let fake = Fake::at(home.path());
    let chan = Channel::new(&issuer, &fake);
    let token = taken(&chan);
    let job = issuer.peek(&token).unwrap().job.to_string();
    call(
        home.path(),
        &["job-lose", "--job", &job, "--reason", "timeout"],
    );
    call(
        home.path(),
        &["contractor-add", "--name", "box2", "--key", &key_hash("k2")],
    );
    let v = call(
        home.path(),
        &["job-offer", "--issue", "7", "--contractor", "box2"],
    );
    assert_eq!(v["attempt"], 2);
    let a = ask(
        &chan,
        Call::Result,
        Some(&token),
        None,
        json!({ "result": {}, "exit": 0 }),
    );
    assert_eq!((a.status, a.refused()), (409, Some("stale")));
    assert!(fake.kept.lock().unwrap().is_empty());
}

#[test]
fn a_stopped_job_is_released_and_its_token_spent() {
    let (home, issuer) = offered("chan-stopped");
    let fake = Fake::at(home.path());
    let chan = Channel::new(&issuer, &fake);
    let token = taken(&chan);
    let job = issuer.peek(&token).unwrap().job.to_string();
    let a = ask(
        &chan,
        Call::Stopped,
        Some(&token),
        None,
        json!({ "why": "drain" }),
    );
    assert_eq!(a.status, 200);
    assert_eq!(
        call(home.path(), &["job-show", "--job", &job])["state"],
        "released"
    );
    assert_eq!(issuer.peek(&token), None);
}

#[test]
fn a_factory_with_outsource_off_answers_no_contractor_and_no_job() {
    let home = made("chan-off", false);
    let fake = Fake::at(home.path());
    let issuer = Issuer::new();
    let chan = Channel::new(&issuer, &fake);
    assert_eq!(by_key(&chan, Call::Hello).status, 401);
    assert_eq!(by_key(&chan, Call::Job).status, 401);
    let body = json!({ "code": "c", "name": "box1", "key": key_hash(KEY) });
    assert_eq!(ask(&chan, Call::Join, None, None, body).status, 403);
}

#[test]
fn only_carried_settings_leave_and_no_endpoint_or_key() {
    use tehas_proxy::channel::{carried, settings_of};
    let names: Vec<String> = [
        "TEHAS_REPO",
        "PI_ENSEMBLE_SUBAGENT_MODEL",
        "PI_ENSEMBLE_MODEL_DEVELOPER",
        "PI_ENSEMBLE_API_KEY_MAIN",
        "PI_ENSEMBLE_BASE_URL_MAIN",
        "PI_ENSEMBLE_HOST_ALIASES",
        "TEHAS_GITHUB_APP_ID",
        "TEHAS_HOST_ADDR",
        "PI_ENSEMBLE_POLYFILL_TOKEN",
    ]
    .map(String::from)
    .to_vec();
    let got = settings_of(&names, &|n| format!("value-of-{n}"));
    let keys: Vec<&String> = got.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        vec![
            "PI_ENSEMBLE_MODEL_DEVELOPER",
            "PI_ENSEMBLE_SUBAGENT_MODEL",
            "TEHAS_GITHUB_APP_ID",
            "TEHAS_REPO"
        ]
    );
    assert!(!carried("PI_ENSEMBLE_API_KEY_MAIN"));
}

#[test]
fn a_store_with_no_answer_is_answered_busy_and_never_revoked() {
    let (home, issuer) = offered("chan-busy");
    let fake = Fake::at(home.path());
    let chan = Channel::new(&issuer, &fake);
    let token = taken(&chan);
    fake.down.store(true, Ordering::SeqCst);
    let t = Some(token.as_str());
    for call in [Call::CheckIn, Call::Token, Call::Relay, Call::Result] {
        let a = ask(&chan, call, t, None, json!({ "result": {}, "argv": [] }));
        assert_eq!((a.status, a.refused()), (503, None), "{call:?}: {a:?}");
    }
    for call in [Call::Hello, Call::Job] {
        let a = by_key(&chan, call);
        assert_eq!((a.status, a.refused()), (503, None), "{call:?}: {a:?}");
    }
    // The store answers again: the job is still running and its token works.
    fake.down.store(false, Ordering::SeqCst);
    let a = ask(&chan, Call::CheckIn, t, None, json!({}));
    assert_eq!(a.status, 200, "{a:?}");
}

#[test]
fn a_name_off_the_allowed_list_is_refused_on_every_call() {
    let (home, issuer) = offered("chan-allowed");
    let h = home.path();
    let code = "the-printed-code";
    call(
        h,
        &["invite-new", "--name", "box2", "--code", &key_hash(code)],
    );
    let mut fake = Fake::at(h);
    let chan_issuer = Issuer::new();
    {
        let chan = Channel::new(&chan_issuer, &fake);
        // box2 is not listed: its join is refused and its code is not spent.
        let body = json!({ "code": code, "name": "box2", "key": key_hash(KEY) });
        let a = ask(&chan, Call::Join, None, None, body);
        assert_eq!((a.status, a.refused()), (403, Some("not allowed")));
    }
    let token = taken(&Channel::new(&issuer, &fake));
    // An empty list refuses every name, box1 included, and its running job.
    fake.allowed.clear();
    let chan = Channel::new(&issuer, &fake);
    for call in [Call::Hello, Call::Job] {
        let a = by_key(&chan, call);
        assert_eq!((a.status, a.refused()), (403, Some("not allowed")));
    }
    let a = ask(&chan, Call::CheckIn, Some(&token), None, json!({}));
    assert_eq!((a.status, a.refused()), (403, Some("not allowed")));
    // The code was never spent: listed now, box2 joins with it.
    fake.allowed.push("box2");
    let chan = Channel::new(&chan_issuer, &fake);
    let body = json!({ "code": code, "name": "box2", "key": key_hash(KEY) });
    assert_eq!(ask(&chan, Call::Join, None, None, body).status, 200);
}

const IMAGE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// The row of box1 in the store's list.
fn box1(home: &Path) -> Value {
    let v = call(home, &["contractor-list"]);
    let row = &v["rows"][0];
    serde_json::from_str(row.as_str().unwrap()).unwrap()
}

#[test]
fn a_hello_writes_the_report_it_carries_and_the_list_answers_it() {
    let (home, issuer) = offered("chan-report");
    let fake = Fake::at(home.path());
    let chan = Channel::new(&issuer, &fake);
    let report = json!({ "report": {
        "image": IMAGE, "rukas": "v0.14.50", "mem_mb": 32000, "disk_kb": 50_000_000,
        "address": "192.0.2.7",
    }});
    let a = ask(&chan, Call::Hello, Some(KEY), Some("box1"), report);
    assert_eq!(a.status, 200, "{a:?}");
    let r = box1(home.path())["report"].clone();
    assert_eq!(r["digest"], IMAGE);
    assert_eq!(r["rukas"], "v0.14.50");
    assert_eq!(
        (r["mem_mb"].as_i64(), r["disk_kb"].as_i64()),
        (Some(32000), Some(50_000_000))
    );
    assert!(!r.to_string().contains("192.0.2.7"));
    // A hello with no report keeps it.
    assert_eq!(by_key(&chan, Call::Hello).status, 200);
    assert_eq!(box1(home.path())["report"]["digest"], IMAGE);
    // A report of a shape the store does not take replaces it with nothing
    // that fits: no image, no memory.
    let bad = json!({ "report": { "image": "latest", "mem_mb": -5, "rukas": "a b" } });
    assert_eq!(
        ask(&chan, Call::Hello, Some(KEY), Some("box1"), bad).status,
        200
    );
    let r = box1(home.path())["report"].clone();
    assert_eq!(
        (r["digest"].clone(), r["mem_mb"].as_i64()),
        (Value::Null, Some(0))
    );
    // Another key's hello writes nothing.
    let other = json!({ "report": { "image": IMAGE, "mem_mb": 1 } });
    let a = ask(&chan, Call::Hello, Some("another-key"), Some("box1"), other);
    assert_eq!(a.status, 401);
    assert_eq!(box1(home.path())["report"]["mem_mb"], 0);
}

#[test]
fn a_restarted_proxy_takes_the_token_of_a_running_job_by_its_kept_hash() {
    let (home, _) = offered("chan-restart");
    let h = home.path();
    let fake = Fake::at(h);
    let before = Issuer::backed(Arc::new(KeptIn::at(h)));
    let token = {
        let chan = Channel::new(&before, &fake);
        taken(&chan)
    };
    // The store holds the token's hash, never the token.
    let db = std::fs::read(h.join("state/tehas.db")).unwrap();
    let wal = std::fs::read(h.join("state/tehas.db-wal")).unwrap_or_default();
    for file in [&db, &wal] {
        assert!(!String::from_utf8_lossy(file).contains(&token));
    }
    let job = before.peek(&token).unwrap().job;
    drop(before);
    // The proxy restarted: a new process with an empty table.
    let after = Issuer::backed(Arc::new(KeptIn::at(h)));
    assert!(after.is_empty());
    let chan = Channel::new(&after, &fake);
    let t = Some(token.as_str());
    let a = ask(&chan, Call::CheckIn, t, None, json!({}));
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(after.peek(&token).map(|l| l.job), Some(job));
    // A token of the same shape that was never issued is no job.
    let guessed: String = token.chars().rev().collect();
    let a = ask(&chan, Call::CheckIn, Some(&guessed), None, json!({}));
    assert_eq!((a.status, a.refused()), (401, Some("no job")));
    // Once the job ends, a third process takes the token for nothing.
    let a = ask(
        &chan,
        Call::Result,
        t,
        None,
        json!({ "result": {"ok": true}, "exit": 0 }),
    );
    assert_eq!(a.status, 200, "{a:?}");
    let again = Issuer::backed(Arc::new(KeptIn::at(h)));
    let chan = Channel::new(&again, &fake);
    let a = ask(&chan, Call::CheckIn, t, None, json!({}));
    assert_eq!((a.status, a.refused()), (401, Some("no job")));
}

#[test]
fn a_restarted_proxy_refuses_the_token_of_a_revoked_contractor() {
    let (home, _) = offered("chan-restart-revoked");
    let h = home.path();
    let fake = Fake::at(h);
    let before = Issuer::backed(Arc::new(KeptIn::at(h)));
    let token = taken(&Channel::new(&before, &fake));
    call(h, &["contractor-revoke", "--name", "box1"]);
    let after = Issuer::backed(Arc::new(KeptIn::at(h)));
    let chan = Channel::new(&after, &fake);
    let a = ask(&chan, Call::CheckIn, Some(&token), None, json!({}));
    assert_eq!((a.status, a.refused()), (401, Some("no job")));
}

/// A model path judged by a proxy's own records of the store at `home`: the
/// status a request with `token` is answered with, 200 when it may go on.
fn model_path(home: &Path, issuer: &Issuer, token: &str) -> u16 {
    use tehas_proxy::records::{OfProxy, OfStore};
    use tehas_proxy::request::{judge, Ask};
    let store = OfStore::at(home, issuer);
    let records = OfProxy::of(&store, issuer);
    match judge(Some(token), &Ask::new("/v1/chat/completions"), &records) {
        Ok(_) => 200,
        Err(why) => why.status(),
    }
}

/// The two tokens of a job taken: the job token and the model token.
fn taken_pair(chan: &Channel) -> (String, String) {
    let a = by_key(chan, Call::Job);
    assert_eq!(a.status, 200, "{a:?}");
    let job = a.body["token"].as_str().unwrap().to_string();
    let model = a.body["model_token"].as_str().unwrap().to_string();
    assert_ne!(job, model);
    (job, model)
}

#[test]
fn each_scope_is_refused_on_the_other_s_paths() {
    let (home, issuer) = offered("chan-scopes");
    let h = home.path();
    let fake = Fake::at(h);
    let chan = Channel::new(&issuer, &fake);
    let (job, model) = taken_pair(&chan);
    // The model token reaches the model paths and no call of the channel.
    assert_eq!(model_path(h, &issuer, &model), 200);
    for c in [
        Call::CheckIn,
        Call::Token,
        Call::Relay,
        Call::Result,
        Call::Stopped,
    ] {
        let a = ask(
            &chan,
            c,
            Some(&model),
            None,
            json!({ "argv": [], "result": {} }),
        );
        assert_eq!((a.status, a.refused()), (403, Some("scope")), "{c:?}");
    }
    assert!(fake.kept.lock().unwrap().is_empty());
    assert!(fake.relayed.lock().unwrap().is_empty());
    // The job token reaches the channel and no model path.
    assert_eq!(model_path(h, &issuer, &job), 403);
    let a = ask(&chan, Call::CheckIn, Some(&job), None, json!({}));
    assert_eq!(a.status, 200, "{a:?}");
    // The job ends: both tokens end with it.
    let a = ask(&chan, Call::Stopped, Some(&job), None, json!({}));
    assert_eq!(a.status, 200, "{a:?}");
    assert_eq!(model_path(h, &issuer, &model), 401);
    assert_eq!(issuer.peek(&model), None);
}

#[test]
fn both_scopes_survive_a_restart_and_keep_their_scope() {
    let (home, _) = offered("chan-restart-scopes");
    let h = home.path();
    let fake = Fake::at(h);
    let before = Issuer::backed(Arc::new(KeptIn::at(h)));
    let (job, model) = taken_pair(&Channel::new(&before, &fake));
    let db = std::fs::read(h.join("state/tehas.db")).unwrap();
    let wal = std::fs::read(h.join("state/tehas.db-wal")).unwrap_or_default();
    for file in [&db, &wal] {
        let text = String::from_utf8_lossy(file);
        assert!(!text.contains(&job) && !text.contains(&model));
    }
    drop(before);
    // A new process: each token comes back in its own scope only.
    let after = Issuer::backed(Arc::new(KeptIn::at(h)));
    let chan = Channel::new(&after, &fake);
    let a = ask(&chan, Call::CheckIn, Some(&model), None, json!({}));
    assert_eq!((a.status, a.refused()), (403, Some("scope")));
    assert_eq!(model_path(h, &after, &job), 403);
    assert_eq!(model_path(h, &after, &model), 200);
    let a = ask(&chan, Call::CheckIn, Some(&job), None, json!({}));
    assert_eq!(a.status, 200, "{a:?}");
}

#[test]
fn a_revoked_contractor_s_tokens_of_both_scopes_are_refused() {
    let (home, issuer) = offered("chan-revoked-scopes");
    let h = home.path();
    let fake = Fake::at(h);
    let backed = Issuer::backed(Arc::new(KeptIn::at(h)));
    let (job, model) = taken_pair(&Channel::new(&backed, &fake));
    drop(issuer);
    call(h, &["contractor-revoke", "--name", "box1"]);
    // The process that issued them: the model path refuses the model token,
    // and the channel says revoked and takes both back.
    assert_eq!(model_path(h, &backed, &model), 403);
    let chan = Channel::new(&backed, &fake);
    let a = ask(&chan, Call::CheckIn, Some(&job), None, json!({}));
    assert_eq!((a.status, a.refused()), (403, Some("revoked")));
    assert!(backed.is_empty(), "a token of the revoked job is held");
    // A restarted proxy finds neither.
    let after = Issuer::backed(Arc::new(KeptIn::at(h)));
    assert_eq!(model_path(h, &after, &model), 401);
    let chan = Channel::new(&after, &fake);
    let a = ask(&chan, Call::CheckIn, Some(&job), None, json!({}));
    assert_eq!((a.status, a.refused()), (401, Some("no job")));
}
