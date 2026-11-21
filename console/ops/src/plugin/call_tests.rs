use super::super::exec::fake::Fake;
use super::super::exec::{Cmd, Done};
use super::super::prepare::tests::{dockerfile, good, C, ID, URL};
use super::*;
use tehas_core::access::{Permission, Person};
use tehas_core::plugin::state::{State, Word};
use tehas_core::testdir::TestDir;

const KEY: &str = "0a1b2c3d";

fn manifest() -> String {
    include_str!("../../../../api/plugin-manifest.tools.json").replace("\"docs\"", "\"floorplan\"")
}

/// The fake, what each attached run found at /run/call.json, and whether
/// the slot and the host's lock were held at each `docker rm`.
struct Peek {
    x: Fake,
    calls: Vec<String>,
    locks: std::path::PathBuf,
    held_at_rm: Vec<bool>,
}

impl Exec for Peek {
    fn run(&mut self, cmd: &Cmd) -> Done {
        if cmd.line().starts_with("docker rm") {
            let held = |f: &str| try_lock(&self.locks.join(f)).unwrap().is_none();
            self.held_at_rm
                .push(held("slot-0.lock") && held("by-host.lock"));
        }
        self.x.run(cmd)
    }

    fn pipe(&mut self, from: &Cmd, to: &Cmd) -> Done {
        self.x.pipe(from, to)
    }

    fn attached(&mut self, argv: &[String], limit: Duration, max: usize, cid: &Path) -> Called {
        let src = argv
            .iter()
            .find_map(|a| a.strip_suffix(",dst=/run/call.json,readonly"))
            .and_then(|m| m.strip_prefix("type=bind,src="))
            .unwrap();
        self.calls.push(fs::read_to_string(src).unwrap());
        self.x.attached(argv, limit, max, cid)
    }
}

/// A home with the plugin ready and built at its pin, its store made, and
/// the container answering `answer`.
fn world(name: &str, answer: Called) -> (TestDir, Peek) {
    let m = manifest();
    let d = dockerfile();
    let (home, mut x) = good(name, &[("tehas-plugin.json", &m), ("Dockerfile", &d)]);
    let mut s = State::new(Word::Ready, C);
    s.manifest_sha256 = Some(prepare::sha256(m.as_bytes()));
    s.image = Some(ID.into());
    prepare::write_state(&home, "floorplan", &s).unwrap();
    for dir in ["libexec", "state"] {
        fs::create_dir_all(home.join(dir)).unwrap();
    }
    tehas_core::testing::write_exe(crate::store::program(&home), "#!/bin/sh\nexit 1\n").unwrap();
    let made = ::store::call(
        &home,
        &[
            "init",
            "--version",
            "v0.0.0",
            "--commit",
            C,
            "--instance",
            "instance",
            "--repo",
            "acme/widget",
        ],
        &mut std::io::empty(),
    );
    assert!(made.is_ok(), "{made:?}");
    x.attach("docker run", answer);
    let locks = runs::calls(&home, "floorplan");
    (
        home,
        Peek {
            x,
            calls: Vec::new(),
            locks,
            held_at_rm: Vec::new(),
        },
    )
}

fn settings(grants: &str) -> impl Fn(&str) -> String {
    let grants = format!("{C} {grants}");
    move |k: &str| match k {
        "TEHAS_PLUGINS" => format!("floorplan={URL}@{C}"),
        "TEHAS_PLUGIN_GRANTS_FLOORPLAN" => grants.clone(),
        "TEHAS_PLUGIN_LIMITS" => "memory=2 cpus=2 disk=4".into(),
        "TEHAS_PLUGIN_ACCESS" => "write".into(),
        _ => String::new(),
    }
}

fn q() -> Value {
    json!({"q": "where is the install guide"})
}

/// GitHub with octocat (7) and hubot (8) at write, nobody without access,
/// and an error for any other login. Each ask counted.
#[derive(Default)]
struct People {
    asked: std::sync::Mutex<Vec<String>>,
}

impl GitHub for People {
    fn token(&self, _: &str) -> Result<String, String> {
        Err("not asked".into())
    }
    fn user(&self, _: &str) -> Result<Person, String> {
        Err("not asked".into())
    }
    fn permission(&self, login: &str) -> Result<Permission, String> {
        self.asked.lock().unwrap().push(login.to_string());
        match login {
            "octocat" => Ok(Permission::Of(7, Some(Level::Write), "write".into())),
            "hubot" => Ok(Permission::Of(8, Some(Level::Write), "write".into())),
            "nobody" => Ok(Permission::None),
            _ => Err("GitHub answered 502 for the permission".into()),
        }
    }
    fn public(&self) -> Result<bool, String> {
        Err("not asked".into())
    }
}

fn ask(x: &mut Peek, home: &Path, args: &Value, caller: &Caller) -> Result<Value, Failed> {
    call(
        x,
        &People::default(),
        home,
        &settings("tool:find-doc"),
        "floorplan",
        "find-doc",
        args,
        caller,
    )
}

fn rows(home: &Path) -> Vec<Value> {
    let v = crate::store::call(home, &["plugin-runs", "--plugin", "floorplan"]).unwrap();
    v["runs"].as_array().unwrap().clone()
}

fn out(s: &str) -> Called {
    Called::Out(s.as_bytes().to_vec())
}

#[test]
fn a_good_answer_is_given_and_only_its_time_size_and_result_kept() {
    let answer = r#"{"doc": "docs/install.md"}"#;
    let (home, mut x) = world("call-good", out(&format!("{answer}\n")));
    let got = ask(&mut x, &home, &q(), &Caller::Host).unwrap();
    assert_eq!(got, json!({"doc": "docs/install.md"}));
    let line =
        x.x.ran
            .iter()
            .find(|l| l.starts_with("docker run"))
            .unwrap();
    assert!(line.starts_with("docker run --rm --init --name tehas-plugin-floorplan-1 "));
    assert!(line.contains(&format!(" {ID} limit=5 max=16384")), "{line}");
    assert_eq!(
        serde_json::from_str::<Value>(&x.calls[0]).unwrap(),
        json!({"tool": "find-doc", "args": q()})
    );
    let r = &rows(&home)[0];
    let fields = ["kind", "trigger", "by", "head", "result", "size"].map(|k| r[k].clone());
    assert_eq!(
        fields,
        [
            json!("call"),
            json!("host"),
            json!("host"),
            json!(C),
            json!("done"),
            json!(answer.len() + 1)
        ]
    );
    assert!(!prepare::dir(&home, "floorplan").join("run-1").exists());
}

#[test]
fn an_answer_that_is_not_one_object_over_the_cap_late_or_failed_is_the_tools_failure() {
    let cases = [
        (
            out(r#"{"a": 1} {"b": 2}"#),
            "the answer is not one JSON object",
            "failed",
        ),
        (out("[1]"), "the answer is not one JSON object", "failed"),
        (out(""), "the answer is not one JSON object", "failed"),
        (
            Called::Out(b"{\"a\": \"\xff\"}".to_vec()),
            "the answer is not one JSON object",
            "failed",
        ),
        (Called::Over, "the answer is over 16 KiB", "failed"),
        (Called::Exit(2), "the tool exited 2", "failed"),
        (Called::Killed, "the tool ran past 5 seconds", "timeout"),
    ];
    for (i, (answer, why, result)) in cases.into_iter().enumerate() {
        let (home, mut x) = world(&format!("call-bad-{i}"), answer);
        assert_eq!(
            ask(&mut x, &home, &q(), &Caller::Host),
            Err(Failed::Tool(why.into())),
            "{i}"
        );
        assert_eq!(rows(&home)[0]["result"], result, "{i}");
        assert!(!prepare::dir(&home, "floorplan").join("run-1").exists());
    }
}

#[test]
fn a_call_killed_before_docker_wrote_its_cid_removes_its_container_while_it_holds_its_slot() {
    for (i, answer) in [Called::Killed, Called::Over].into_iter().enumerate() {
        let (home, mut x) = world(&format!("call-rm-{i}"), answer);
        ask(&mut x, &home, &q(), &Caller::Host).unwrap_err();
        let ran: Vec<_> =
            x.x.ran
                .iter()
                .filter(|l| l.starts_with("docker r"))
                .collect();
        assert_eq!(ran.len(), 2, "{ran:?}");
        assert!(ran[0].starts_with("docker run "), "{ran:?}");
        assert_eq!(ran[1], "docker rm -f tehas-plugin-floorplan-1");
        assert_eq!(x.held_at_rm, [true]);
        // Then the slot is free: with slot-1 held, the next call takes
        // slot-0 and runs.
        let other = try_lock(&runs::calls(&home, "floorplan").join("slot-1.lock"))
            .unwrap()
            .unwrap();
        x.x.attach("docker run", out("{}"));
        assert_eq!(ask(&mut x, &home, &q(), &Caller::Host), Ok(json!({})));
        assert!(x.x.ran_any("--name tehas-plugin-floorplan-2 "));
        drop(other);
    }
    let (home, mut x) = world("call-rm-clean", Called::Exit(2));
    ask(&mut x, &home, &q(), &Caller::Host).unwrap_err();
    assert!(!x.x.ran_any("docker rm"));
}

#[test]
fn a_row_the_store_did_not_end_keeps_its_directory() {
    let (home, mut x) = world("call-end", out("{}"));
    let answers = home.join("answers");
    fs::write(&answers, "plugin-run-end|78|store: broken: no\n").unwrap();
    crate::store::testing::plant(&home, &answers);
    let dir = prepare::dir(&home, "floorplan").join("run-1");
    assert_eq!(
        ask(&mut x, &home, &q(), &Caller::Host),
        Err(Failed::Tool(format!(
            "the store did not end run 1, so {} is kept: store: broken: no",
            dir.display()
        )))
    );
    assert!(dir.join("call.json").exists());
    assert_eq!(rows(&home)[0]["result"], Value::Null);
}

#[test]
fn a_call_not_ready_or_granted_is_refused_and_runs_nothing() {
    let (home, mut x) = world("call-refused", out("{}"));
    let other = call(
        &mut x,
        &People::default(),
        &home,
        &settings("tool:find-doc"),
        "floorplan",
        "other",
        &q(),
        &Caller::Host,
    );
    assert_eq!(
        other,
        Err(Failed::Refused(
            "no tool other is granted at the pin".into()
        ))
    );
    let ungranted = call(
        &mut x,
        &People::default(),
        &home,
        &settings(""),
        "floorplan",
        "find-doc",
        &q(),
        &Caller::Host,
    );
    assert!(matches!(ungranted, Err(Failed::Refused(_))));
    prepare::write_state(&home, "floorplan", &State::refused(C, "the disk")).unwrap();
    assert_eq!(
        ask(&mut x, &home, &q(), &Caller::Host),
        Err(Failed::Refused("it is refused: the disk".into()))
    );
    assert!(!x.x.ran_any("docker run"));
    assert_eq!(rows(&home), Vec::<Value>::new());
}

#[test]
fn arguments_not_of_the_tools_params_are_refused_before_anything_runs() {
    let (home, mut x) = world("call-args", out("{}"));
    for args in [
        json!({}),
        json!({"q": 1}),
        json!({"q": "x", "more": true}),
        json!({"q": "x".repeat(params::ARGS_MAX)}),
    ] {
        let got = ask(&mut x, &home, &args, &Caller::Host);
        assert!(matches!(got, Err(Failed::Args(_))), "{args}: {got:?}");
    }
    assert!(!x.x.ran_any("docker run"));
    assert_eq!(rows(&home), Vec::<Value>::new());
}

#[test]
fn two_calls_of_a_plugin_and_one_of_a_caller_run_at_once_and_more_are_busy() {
    let (home, mut x) = world("call-locks", out("{}"));
    let dir = runs::calls(&home, "floorplan");
    fs::create_dir_all(&dir).unwrap();
    let hold = |f: &str| try_lock(&dir.join(f)).unwrap().unwrap();
    let busy = Err(Failed::Refused("busy".into()));

    let slots = [hold("slot-0.lock"), hold("slot-1.lock")];
    assert_eq!(ask(&mut x, &home, &q(), &Caller::Host), busy);
    assert_eq!(rows(&home), Vec::<Value>::new());
    drop(slots);

    let one = hold("slot-0.lock");
    assert_eq!(ask(&mut x, &home, &q(), &Caller::Host), Ok(json!({})));
    let mine = hold(&format!("by-mcp-{KEY}.lock"));
    assert_eq!(ask(&mut x, &home, &q(), &Caller::Mcp(KEY.into())), busy);
    let other = ask(&mut x, &home, &q(), &Caller::Mcp("ffffffff".into()));
    assert_eq!(other, Ok(json!({})));
    drop((one, mine));
    let host = hold("by-host.lock");
    assert_eq!(ask(&mut x, &home, &q(), &Caller::Host), busy);
    drop(host);

    let r = rows(&home);
    assert_eq!(r.len(), 2);
    assert_eq!(
        (&r[0]["trigger"], &r[0]["by"]),
        (&json!("mcp"), &json!("ffffffff"))
    );
}

#[test]
fn an_mcp_key_id_is_8_lower_case_hex_and_the_rows_by() {
    let (home, mut x) = world("call-mcp", out("{}"));
    let no = Err(Failed::Args("the MCP key id is 8 lower-case hex".into()));
    for key in [
        "0A1B2C3D",
        "0a1b2c3",
        "0a1b2c3d4",
        "0a1b2c3g",
        "",
        "../../x",
    ] {
        assert_eq!(
            ask(&mut x, &home, &q(), &Caller::Mcp(key.into())),
            no,
            "{key}"
        );
    }
    assert_eq!(rows(&home), Vec::<Value>::new());
    assert_eq!(
        ask(&mut x, &home, &q(), &Caller::Mcp(KEY.into())),
        Ok(json!({}))
    );
    let r = &rows(&home)[0];
    assert_eq!((&r["trigger"], &r["by"]), (&json!("mcp"), &json!(KEY)));
}

fn as_login(x: &mut Peek, gh: &dyn GitHub, home: &Path, login: &str) -> Result<Value, Failed> {
    call(
        x,
        gh,
        home,
        &settings("tool:find-doc"),
        "floorplan",
        "find-doc",
        &q(),
        &Caller::Login(login.into()),
    )
}

/// GitHub naming `octocat` user 7 with write first, then, the login renamed
/// and taken, user 9 with no access.
#[derive(Default)]
struct Renamed {
    asked: std::sync::Mutex<u32>,
}

impl GitHub for Renamed {
    fn token(&self, _: &str) -> Result<String, String> {
        Err("not asked".into())
    }
    fn user(&self, _: &str) -> Result<Person, String> {
        Err("not asked".into())
    }
    fn permission(&self, _: &str) -> Result<Permission, String> {
        let mut n = self.asked.lock().unwrap();
        *n += 1;
        Ok(if *n == 1 {
            Permission::Of(7, Some(Level::Write), "write".into())
        } else {
            Permission::Of(9, None, "".into())
        })
    }
    fn public(&self) -> Result<bool, String> {
        Err("not asked".into())
    }
}

#[test]
fn a_login_taken_by_someone_else_is_refused_at_once() {
    let (home, mut x) = world("call-renamed", out("{}"));
    let gh = Renamed::default();
    assert_eq!(as_login(&mut x, &gh, &home, "octocat"), Ok(json!({})));
    // The same login, now another person's with no access: refused on the
    // next call, never passed on the first holder's verdict.
    assert_eq!(
        as_login(&mut x, &gh, &home, "octocat"),
        Err(Failed::Refused(NO_ACCESS.into()))
    );
    assert_eq!(*gh.asked.lock().unwrap(), 2);
}

#[test]
fn a_login_is_checked_against_the_repositorys_access_on_every_call() {
    let (home, mut x) = world("call-login", out("{}"));
    let gh = People::default();
    assert_eq!(as_login(&mut x, &gh, &home, "octocat"), Ok(json!({})));
    assert_eq!(as_login(&mut x, &gh, &home, "octocat"), Ok(json!({})));
    // Asked on every call: a kept verdict could pass for whoever holds the
    // login next.
    assert_eq!(*gh.asked.lock().unwrap(), ["octocat", "octocat"]);
    let no = Err(Failed::Refused(NO_ACCESS.into()));
    assert_eq!(as_login(&mut x, &gh, &home, "nobody"), no);
    let down = as_login(&mut x, &gh, &home, "other");
    assert_eq!(
        down,
        Err(Failed::Refused(
            "GitHub could not say whether the login has access: GitHub answered 502 for the permission".into()
        ))
    );
    // An error is not kept: the next call asks again.
    as_login(&mut x, &gh, &home, "other").unwrap_err();
    assert_eq!(gh.asked.lock().unwrap().len(), 5);
    let r = rows(&home);
    assert_eq!(r.len(), 2);
    assert_eq!(
        (&r[0]["trigger"], &r[0]["by"]),
        (&json!("chat"), &json!("chat"))
    );
    assert!(!home.join("plugins/access.json").exists());
}

#[test]
fn two_people_call_at_once_and_one_person_twice_is_busy() {
    let (home, mut x) = world("call-people", out("{}"));
    let gh = People::default();
    let dir = runs::calls(&home, "floorplan");
    fs::create_dir_all(&dir).unwrap();
    let held = try_lock(&dir.join("by-login-7.lock")).unwrap().unwrap();
    assert_eq!(
        as_login(&mut x, &gh, &home, "octocat"),
        Err(Failed::Refused("busy".into()))
    );
    assert_eq!(as_login(&mut x, &gh, &home, "hubot"), Ok(json!({})));
    drop(held);
    assert_eq!(rows(&home).len(), 1);
}
