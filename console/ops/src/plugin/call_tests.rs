//! The tests of one call. A 2.2 manifest carries no tool list: the tool
//! names are asks and the vetted entry will name their parameters, so
//! [`call`] resolves a tool from the 1.x list only and refuses a 2.x plugin's
//! call (the R9 contract step takes the list, and this file's flow helper,
//! out with the rest of the v1 block). The flow past the tool — caller,
//! arguments, slots, locks, the row, the run and its answer — is the code
//! that runs the container, so its tests keep running through
//! [`call_body`], which is [`call`]'s body after the tool is resolved, with
//! the tool's fields held by the test.

use super::super::exec::fake::Fake;
use super::super::exec::{Called, Cmd, Done, Exec};
use super::super::prepare::tests::{good, C, ID, URL};
use super::super::ready::ready;
use super::*;
use tehas_core::access::{AccountId, Permission, Person};
use tehas_core::lock::try_lock;
use tehas_core::plugin::manifest::Tool;
use tehas_core::plugin::state::{State, Word};
use tehas_core::testdir::TestDir;
use tehas_ports::{Got, Outcome, Passing, Sent};

const KEY: &str = "0a1b2c3d";

/// The 2.2 manifest: it asks its one tool by name and carries no list.
fn manifest() -> String {
    r#"{"plugin": 2, "spec": "2.0.0-r8", "name": "floorplan", "label": "Product docs",
        "asks": ["tool:find-doc"], "driver": "dist/driver/main.js", "timeout": 5}"#
        .to_string()
}

/// The tool of the 1.x shape, as the vetted entry of a 2.x plugin will
/// carry it: its `params` are the fixture's and its timeout the run's.
fn tool() -> Tool {
    Tool {
        id: "find-doc".into(),
        about: "Find the product doc that answers a question".into(),
        params: json!({"type": "object", "properties": {"q": {"type": "string", "maxLength": 200}},
                       "required": ["q"], "additionalProperties": false}),
        timeout: 5,
        chats: vec!["obeya".into()],
    }
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

/// A home with the plugin ready at the pin, its store made, and the
/// container answering `answer`. The manifest is the 2.2 one, so the ready
/// plugin has asks and grants and no tool list.
fn world(name: &str, answer: Called) -> (TestDir, Peek) {
    let m = manifest();
    let (home, mut x) = good(name, &[("tehas-plugin.json", &m)]);
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

/// The ready plugin of that home, its 1.x tool list held by the test, as
/// the vetted entry of a 2.x plugin will hold it.
fn tool_ready(name: &str, answer: Called) -> (TestDir, Peek, Ready) {
    let (home, x) = world(name, answer);
    let mut x = x;
    let mut r = ready(&mut x, &home, &settings("tool:find-doc"), "floorplan").unwrap();
    assert_eq!(r.manifest.v1.tools, [], "a 2.x reader sets no tool list");
    r.manifest.v1.tools = vec![tool()];
    (home, x, r)
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

/// `call`'s body after the tool is resolved, so that the flow that runs the
/// container stays under test while no 2.x manifest can name a tool. It
/// keeps the order of the real thing: caller, arguments, limits, slots and
/// lock, the row, the run, the row ended, the directory removed.
fn call_body(
    x: &mut dyn Exec,
    gh: Option<&dyn GitHub>,
    home: &Path,
    r: &Ready,
    t: &Tool,
    args: &Value,
    caller: &Caller,
) -> Result<Value, Failed> {
    let get = settings("tool:find-doc");
    let user = match caller {
        Caller::Login(login) => {
            let gh = gh.ok_or_else(|| Failed::Refused(NO_HOST.into()))?;
            Some(login_access(&get, gh, login)?)
        }
        _ => None,
    };
    params::check(args, &t.params).map_err(Failed::Args)?;
    let limits = plugin::limits(&get("TEHAS_PLUGIN_LIMITS"))
        .ok_or_else(|| Failed::Refused("TEHAS_PLUGIN_LIMITS cannot be read".into()))?;
    let image = r
        .image
        .clone()
        .ok_or_else(|| Failed::Refused("it has no build".into()))?;
    let _held = locks(home, &r.pin.name, &caller.lock(user.as_ref()))?;
    let id = begin(home, r, caller)?;
    let dir = prepare::dir(home, &r.pin.name).join(format!("run-{id}"));
    let dir_text = dir.display().to_string();
    let run = Run {
        manifest: &r.manifest,
        grants: &r.grants,
        commit: &r.pin.commit,
        kind: Kind::Call,
        trigger: caller.trigger(),
        id,
        dir: &dir_text,
        image: &image,
        limits,
        head: &r.pin.commit,
        model: "",
        pr: None,
        tool: Some(&t.id),
        event: None,
    };
    let (got, result, size) = run_in(x, &run, t, args);
    end(home, id, result, size).map_err(|line| {
        Failed::Tool(format!(
            "the store did not end run {id}, so {dir_text} is kept: {line}"
        ))
    })?;
    let _ = fs::remove_dir_all(&dir);
    got
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
    fn token(&self, _: &str) -> Got<String> {
        Err(Outcome::Forbidden)
    }
    fn user(&self, _: &str) -> Got<Person> {
        Err(Outcome::Forbidden)
    }
    fn permission(&self, login: &str) -> Got<Permission> {
        self.asked.lock().unwrap().push(login.to_string());
        match login {
            "octocat" => Ok(Permission::Of(
                AccountId::new("7"),
                Some(Level::Write),
                "write".into(),
            )),
            "hubot" => Ok(Permission::Of(
                AccountId::new("8"),
                Some(Level::Write),
                "write".into(),
            )),
            "nobody" => Ok(Permission::None),
            "climber" => Ok(Permission::Of(
                AccountId::new("../7"),
                Some(Level::Write),
                "write".into(),
            )),
            _ => Err(Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::Maybe,
            }),
        }
    }
    fn public(&self) -> Got<bool> {
        Err(Outcome::Forbidden)
    }
}

fn ask(
    x: &mut Peek,
    home: &Path,
    r: &Ready,
    args: &Value,
    caller: &Caller,
) -> Result<Value, Failed> {
    let t = r.manifest.v1.tools[0].clone();
    call_body(x, Some(&People::default()), home, r, &t, args, caller)
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
    let (home, mut x, r) = tool_ready("call-good", out(&format!("{answer}\n")));
    let got = ask(&mut x, &home, &r, &q(), &Caller::Host).unwrap();
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
        (
            out(r#"{"a":1,"a":2}"#),
            "the answer is not one JSON object",
            "failed",
        ),
        (
            out(r#"{"a": [{"b": {"c": 1, "c": 1}}]}"#),
            "the answer is not one JSON object",
            "failed",
        ),
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
        let (home, mut x, r) = tool_ready(&format!("call-bad-{i}"), answer);
        assert_eq!(
            ask(&mut x, &home, &r, &q(), &Caller::Host),
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
        let (home, mut x, r) = tool_ready(&format!("call-rm-{i}"), answer);
        ask(&mut x, &home, &r, &q(), &Caller::Host).unwrap_err();
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
        assert_eq!(ask(&mut x, &home, &r, &q(), &Caller::Host), Ok(json!({})));
        assert!(x.x.ran_any("--name tehas-plugin-floorplan-2 "));
        drop(other);
    }
    let (home, mut x, r) = tool_ready("call-rm-clean", Called::Exit(2));
    ask(&mut x, &home, &r, &q(), &Caller::Host).unwrap_err();
    assert!(!x.x.ran_any("docker rm"));
}

#[test]
fn a_row_the_store_did_not_end_keeps_its_directory() {
    let (home, mut x, r) = tool_ready("call-end", out("{}"));
    let answers = home.join("answers");
    fs::write(&answers, "plugin-run-end|78|store: broken: no\n").unwrap();
    crate::store::testing::plant(&home, &answers);
    let dir = prepare::dir(&home, "floorplan").join("run-1");
    assert_eq!(
        ask(&mut x, &home, &r, &q(), &Caller::Host),
        Err(Failed::Tool(format!(
            "the store did not end run 1, so {} is kept: store: broken: no",
            dir.display()
        )))
    );
    assert!(dir.join("call.json").exists());
    assert_eq!(rows(&home)[0]["result"], Value::Null);
}

#[test]
fn an_answer_without_a_key_twice_reads_as_serde_json_reads_it() {
    let s = r#"{"a": [1, -2, 3.5, 18446744073709551615, null, true, "é"], "b": {"c": {}}}"#;
    let v: Value = serde_json::from_str(s).unwrap();
    assert_eq!(serde_json::from_str::<Strict>(s).unwrap().0, v);
}

/// The tool is what a 2.x manifest cannot give: the ask and the grant are
/// there, the plugin is ready, and the call is still refused, because no
/// list names `find-doc` with its parameters. The 1.x manifest of the
/// fixture is refused before any tool is named, and a refused state is
/// refused as itself, all of it without a run.
#[test]
fn a_call_is_refused_while_no_manifest_names_its_tool() {
    let (home, mut x) = world("call-refused", out("{}"));
    // Ready, asked and granted: the counter-assertion of the refusal below.
    assert!(ready(&mut x, &home, &settings("tool:find-doc"), "floorplan").is_ok());
    let got = call(
        &mut x,
        Some(&People::default()),
        &home,
        &settings("tool:find-doc"),
        "floorplan",
        "find-doc",
        &q(),
        &Caller::Host,
    );
    assert_eq!(
        got,
        Err(Failed::Refused(
            "no tool find-doc is granted at the pin".into()
        ))
    );
    let other = call(
        &mut x,
        Some(&People::default()),
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
        Some(&People::default()),
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
        call(
            &mut x,
            Some(&People::default()),
            &home,
            &settings("tool:find-doc"),
            "floorplan",
            "find-doc",
            &q(),
            &Caller::Host,
        ),
        Err(Failed::Refused("it is refused: the disk".into()))
    );
    // The 1.x fixture names its tools, and is refused at its major first.
    let m = include_str!("../../../../api/plugin-manifest.tools.json")
        .replace("\"docs\"", "\"floorplan\"");
    let (home1, mut x1) = good("call-v1", &[("tehas-plugin.json", &m)]);
    let mut s = State::new(Word::Ready, C);
    s.manifest_sha256 = Some(prepare::sha256(m.as_bytes()));
    s.image = Some(ID.into());
    prepare::write_state(&home1, "floorplan", &s).unwrap();
    let got = call(
        &mut x1,
        Some(&People::default()),
        &home1,
        &settings("tool:find-doc"),
        "floorplan",
        "find-doc",
        &q(),
        &Caller::Host,
    );
    assert!(
        matches!(&got, Err(Failed::Refused(why)) if why.contains("plugin is 1: this tehas reads manifest version 2")),
        "{got:?}"
    );
    assert!(!x1.ran_any("docker run"));
    assert_eq!(rows(&home), Vec::<Value>::new());
}

#[test]
fn arguments_not_of_the_tools_params_are_refused_before_anything_runs() {
    let (home, mut x, r) = tool_ready("call-args", out("{}"));
    for args in [
        json!({}),
        json!({"q": 1}),
        json!({"q": "x", "more": true}),
        json!({"q": "x".repeat(params::ARGS_MAX)}),
    ] {
        let got = ask(&mut x, &home, &r, &args, &Caller::Host);
        assert!(matches!(got, Err(Failed::Args(_))), "{args}: {got:?}");
    }
    assert!(!x.x.ran_any("docker run"));
    assert_eq!(rows(&home), Vec::<Value>::new());
}

#[test]
fn two_calls_of_a_plugin_and_one_of_a_caller_run_at_once_and_more_are_busy() {
    let (home, mut x, r) = tool_ready("call-locks", out("{}"));
    let dir = runs::calls(&home, "floorplan");
    fs::create_dir_all(&dir).unwrap();
    let hold = |f: &str| try_lock(&dir.join(f)).unwrap().unwrap();
    let busy = Err(Failed::Refused("busy".into()));

    let slots = [hold("slot-0.lock"), hold("slot-1.lock")];
    assert_eq!(ask(&mut x, &home, &r, &q(), &Caller::Host), busy);
    assert_eq!(rows(&home), Vec::<Value>::new());
    drop(slots);

    let one = hold("slot-0.lock");
    assert_eq!(ask(&mut x, &home, &r, &q(), &Caller::Host), Ok(json!({})));
    let mine = hold(&format!("by-mcp-{KEY}.lock"));
    assert_eq!(ask(&mut x, &home, &r, &q(), &Caller::Mcp(KEY.into())), busy);
    let other = ask(&mut x, &home, &r, &q(), &Caller::Mcp("ffffffff".into()));
    assert_eq!(other, Ok(json!({})));
    drop((one, mine));
    let host = hold("by-host.lock");
    assert_eq!(ask(&mut x, &home, &r, &q(), &Caller::Host), busy);
    drop(host);

    let r = rows(&home);
    assert_eq!(r.len(), 2);
    assert_eq!(
        (&r[0]["trigger"], &r[0]["by"]),
        (&json!("mcp"), &json!("ffffffff"))
    );
}

/// The key id is checked before anything is read, so a malformed one is
/// refused in a home with no plugin at all.
#[test]
fn an_mcp_key_id_is_8_lower_case_hex_and_the_rows_by() {
    let home = TestDir::new("call-mcp");
    let mut x = Peek {
        x: Fake::new(),
        calls: Vec::new(),
        locks: runs::calls(&home, "floorplan"),
        held_at_rm: Vec::new(),
    };
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
            call(
                &mut x,
                Some(&People::default()),
                &home,
                &settings("tool:find-doc"),
                "floorplan",
                "find-doc",
                &q(),
                &Caller::Mcp(key.into()),
            ),
            no,
            "{key}"
        );
    }
    // The good id names its lock and its row, from the flow helper.
    let (home, mut x, r) = tool_ready("call-mcp-row", out("{}"));
    assert_eq!(
        ask(&mut x, &home, &r, &q(), &Caller::Mcp(KEY.into())),
        Ok(json!({}))
    );
    let r = &rows(&home)[0];
    assert_eq!((&r["trigger"], &r["by"]), (&json!("mcp"), &json!(KEY)));
}

fn as_login(
    x: &mut Peek,
    gh: &dyn GitHub,
    home: &Path,
    r: &Ready,
    login: &str,
) -> Result<Value, Failed> {
    let t = tool();
    call_body(x, Some(gh), home, r, &t, &q(), &Caller::Login(login.into()))
}

/// GitHub naming `octocat` user 7 with write first, then, the login renamed
/// and taken, user 9 with no access.
#[derive(Default)]
struct Renamed {
    asked: std::sync::Mutex<u32>,
}

impl GitHub for Renamed {
    fn token(&self, _: &str) -> Got<String> {
        Err(Outcome::Forbidden)
    }
    fn user(&self, _: &str) -> Got<Person> {
        Err(Outcome::Forbidden)
    }
    fn permission(&self, _: &str) -> Got<Permission> {
        let mut n = self.asked.lock().unwrap();
        *n += 1;
        Ok(if *n == 1 {
            Permission::Of(AccountId::new("7"), Some(Level::Write), "write".into())
        } else {
            Permission::Of(AccountId::new("9"), None, "".into())
        })
    }
    fn public(&self) -> Got<bool> {
        Err(Outcome::Forbidden)
    }
}

#[test]
fn a_login_taken_by_someone_else_is_refused_at_once() {
    let (home, mut x, r) = tool_ready("call-renamed", out("{}"));
    let gh = Renamed::default();
    assert_eq!(as_login(&mut x, &gh, &home, &r, "octocat"), Ok(json!({})));
    // The same login, now another person's with no access: refused on the
    // next call, never passed on the first holder's verdict.
    assert_eq!(
        as_login(&mut x, &gh, &home, &r, "octocat"),
        Err(Failed::Refused(NO_ACCESS.into()))
    );
    assert_eq!(*gh.asked.lock().unwrap(), 2);
}

#[test]
fn a_login_is_checked_against_the_repositorys_access_on_every_call() {
    let (home, mut x, r) = tool_ready("call-login", out("{}"));
    let gh = People::default();
    assert_eq!(as_login(&mut x, &gh, &home, &r, "octocat"), Ok(json!({})));
    assert_eq!(as_login(&mut x, &gh, &home, &r, "octocat"), Ok(json!({})));
    // Asked on every call: a kept verdict could pass for whoever holds the
    // login next.
    assert_eq!(*gh.asked.lock().unwrap(), ["octocat", "octocat"]);
    let no = Err(Failed::Refused(NO_ACCESS.into()));
    assert_eq!(as_login(&mut x, &gh, &home, &r, "nobody"), no);
    let down = as_login(&mut x, &gh, &home, &r, "other");
    assert_eq!(
        down,
        Err(Failed::Refused(
            "GitHub could not say whether the login has access: the site failed; it may have been sent".into()
        ))
    );
    // An error is not kept: the next call asks again.
    as_login(&mut x, &gh, &home, &r, "other").unwrap_err();
    assert_eq!(gh.asked.lock().unwrap().len(), 5);
    let r = rows(&home);
    assert_eq!(r.len(), 2);
    assert_eq!(
        (&r[0]["trigger"], &r[0]["by"]),
        (&json!("chat"), &json!("chat"))
    );
    assert!(!home.join("plugins/access.json").exists());
}

/// An id that could leave the calls' directory, or is no id, names no lock:
/// the call is refused before any lock or row.
#[test]
fn an_account_id_that_cannot_name_a_lock_is_refused() {
    let (home, mut x, r) = tool_ready("call-climber", out("{}"));
    let gh = People::default();
    assert_eq!(
        as_login(&mut x, &gh, &home, &r, "climber"),
        Err(Failed::Refused(
            "GitHub's id of the login cannot name a lock".into()
        ))
    );
    assert_eq!(rows(&home).len(), 0);
    assert!(!runs::calls(&home, "floorplan")
        .join("by-login-../7.lock")
        .exists());
    for (id, safe) in [
        ("7", true),
        ("id-alice_2", true),
        ("", false),
        ("../7", false),
        ("7/8", false),
        ("7 ", false),
        ("7.lock", false),
    ] {
        assert_eq!(super::lock_safe(&AccountId::new(id)), safe, "{id:?}");
    }
    assert!(!super::lock_safe(&AccountId::new("9".repeat(65))));
}

/// With no way to ask the code host, a call for a login is refused before
/// any row.
#[test]
fn a_login_with_nothing_to_ask_is_refused() {
    let (home, mut x, r) = tool_ready("call-unasked", out("{}"));
    let t = tool();
    let got = call_body(
        &mut x,
        None,
        &home,
        &r,
        &t,
        &q(),
        &Caller::Login("octocat".into()),
    );
    assert_eq!(got, Err(Failed::Refused(super::NO_HOST.into())));
    assert_eq!(rows(&home).len(), 0);
}

#[test]
fn two_people_call_at_once_and_one_person_twice_is_busy() {
    let (home, mut x, r) = tool_ready("call-people", out("{}"));
    let gh = People::default();
    let dir = runs::calls(&home, "floorplan");
    fs::create_dir_all(&dir).unwrap();
    let held = try_lock(&dir.join("by-login-7.lock")).unwrap().unwrap();
    assert_eq!(
        as_login(&mut x, &gh, &home, &r, "octocat"),
        Err(Failed::Refused("busy".into()))
    );
    assert_eq!(as_login(&mut x, &gh, &home, &r, "hubot"), Ok(json!({})));
    drop(held);
    assert_eq!(rows(&home).len(), 1);
}
