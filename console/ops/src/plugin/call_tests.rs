use super::super::exec::fake::Fake;
use super::super::exec::{Cmd, Done};
use super::super::prepare::tests::{dockerfile, good, C, ID, URL};
use super::*;
use tehas_core::plugin::state::{State, Word};
use tehas_core::testdir::TestDir;

const KEY: &str = "0a1b2c3d";

fn manifest() -> String {
    include_str!("../../../../api/plugin-manifest.tools.json").replace("\"docs\"", "\"floorplan\"")
}

/// The fake, and what each attached run found at /run/call.json.
struct Peek {
    x: Fake,
    calls: Vec<String>,
}

impl Exec for Peek {
    fn run(&mut self, cmd: &Cmd) -> Done {
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
    (
        home,
        Peek {
            x,
            calls: Vec::new(),
        },
    )
}

fn settings(grants: &str) -> impl Fn(&str) -> String {
    let grants = format!("{C} {grants}");
    move |k: &str| match k {
        "TEHAS_PLUGINS" => format!("floorplan={URL}@{C}"),
        "TEHAS_PLUGIN_GRANTS_FLOORPLAN" => grants.clone(),
        "TEHAS_PLUGIN_LIMITS" => "memory=2 cpus=2 disk=4".into(),
        _ => String::new(),
    }
}

fn q() -> Value {
    json!({"q": "where is the install guide"})
}

fn ask(x: &mut Peek, home: &Path, args: &Value, caller: &Caller) -> Result<Value, Failed> {
    call(
        x,
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
fn a_call_not_ready_granted_or_of_a_login_is_refused_and_runs_nothing() {
    let (home, mut x) = world("call-refused", out("{}"));
    let login = ask(&mut x, &home, &q(), &Caller::Login("octocat".into()));
    assert_eq!(login, Err(Failed::Refused(LOGIN.into())));
    let bad_key = ask(&mut x, &home, &q(), &Caller::Mcp("0A1B2C3D".into()));
    assert!(matches!(bad_key, Err(Failed::Refused(_))), "{bad_key:?}");
    let other = call(
        &mut x,
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
