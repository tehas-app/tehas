//! One call of a plugin's tool (docs/specs/2026-11-21-plugins-v2-chats.md,
//! "A call"; docs/specs/2026-11-21-plugins-mcp.md, "The broker"). `tehas
//! plugin call` and the MCP broker both call [`call`].
//!
//! In order: the plugin ready, the tool in its manifest and granted, the
//! caller (a login checked against the repository's access, as the gate
//! checks it), the arguments against the tool's `params`, a slot and the
//! caller's lock taken now or the call refused as busy, a row in
//! `plugin_runs`, the run's directory with its context and `call.json`, the
//! container attached at the tool's timeout, its answer read, then the row
//! ended and the directory removed. Nothing of the call is kept but the
//! row's time, size and result, and the directory of a row the store did
//! not end.

use super::exec::{Called, Cmd, Exec};
use super::prepare;
use super::ready::{ready, Ready};
use super::runs;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::path::Path;
use std::time::Duration;
use tehas_core::access::{GitHub, Level};
use tehas_core::file::{write_atomic, Temp};
use tehas_core::lock::try_lock;
use tehas_core::plugin::manifest::Tool;
use tehas_core::plugin::run::{context, Kind, Run, Trigger};
use tehas_core::plugin::{self, params, Cap};
use tehas_core::sandbox;

/// The most bytes of an answer.
pub const ANSWER_MAX: usize = 16 << 10;

/// The refusal of a login without access, or one GitHub's answer does not
/// let in.
pub const NO_ACCESS: &str = "the login has no access to the repository";

/// Who asks for a call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    /// A chat host, for the person with this login.
    Login(String),
    /// The MCP broker, for the key with this 8 hex id.
    Mcp(String),
    /// A person on the host.
    Host,
}

/// Why a call gave no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failed {
    /// The arguments are not the tool's.
    Args(String),
    /// The call may not run now, or at all.
    Refused(String),
    /// It ran and did not answer, in tehas's words, never the tool's.
    Tool(String),
}

impl Caller {
    /// The name of its lock, `by-<this>.lock`: a person's by GitHub's user
    /// id, so a renamed login is the same person.
    fn lock(&self, user: Option<u64>) -> String {
        match (self, user) {
            (Caller::Login(_), Some(id)) => format!("login-{id}"),
            (Caller::Login(_), None) => "login".into(),
            (Caller::Mcp(key), _) => format!("mcp-{key}"),
            (Caller::Host, _) => "host".into(),
        }
    }

    /// Who the run row says started it: never a login.
    fn by(&self) -> &str {
        match self {
            Caller::Login(_) => "chat",
            Caller::Mcp(key) => key,
            Caller::Host => "host",
        }
    }

    fn trigger(&self) -> Trigger {
        match self {
            Caller::Login(_) => Trigger::Chat,
            Caller::Mcp(_) => Trigger::Mcp,
            Caller::Host => Trigger::Host,
        }
    }
}

/// The first 4 bytes of an MCP key's sha256 in hex, as tehas-mcp's access
/// log prints them.
fn is_key_id(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The GitHub user id of a login with access, asked of GitHub on every
/// call: a verdict kept for a login could pass for whoever holds that login
/// next. GitHub's error is a refusal: a login it could not check gets no
/// answer.
fn login_access(get: &dyn Fn(&str) -> String, gh: &dyn GitHub, login: &str) -> Result<u64, Failed> {
    let access = Level::of_setting(&get("TEHAS_PLUGIN_ACCESS"))
        .ok_or_else(|| Failed::Refused("TEHAS_PLUGIN_ACCESS is not set".into()))?;
    match tehas_core::access::check(gh, access, login, None) {
        Ok(v) => v.user.ok_or_else(|| Failed::Refused(NO_ACCESS.into())),
        Err(why) => Err(Failed::Refused(format!(
            "GitHub could not say whether the login has access: {why}"
        ))),
    }
}

/// One call of `tool` of `plugin` with `args`: its answer, one JSON object.
/// `gh` checks a login's access; a host or the MCP broker never asks it.
#[allow(clippy::too_many_arguments)]
pub fn call(
    x: &mut dyn Exec,
    gh: &dyn GitHub,
    home: &Path,
    get: &dyn Fn(&str) -> String,
    plugin: &str,
    tool: &str,
    args: &Value,
    caller: &Caller,
) -> Result<Value, Failed> {
    if let Caller::Mcp(key) = caller {
        if !is_key_id(key) {
            return Err(Failed::Args("the MCP key id is 8 lower-case hex".into()));
        }
    }
    let r = ready(x, home, get, plugin).map_err(Failed::Refused)?;
    let cap = Cap::Tool(tool.to_string());
    let t = r
        .manifest
        .tools
        .iter()
        .find(|t| t.id == tool)
        .filter(|_| r.grants.caps.contains(&cap) && r.manifest.asks.contains(&cap.ask()))
        .ok_or_else(|| Failed::Refused(format!("no tool {tool} is granted at the pin")))?
        .clone();
    let user = match caller {
        Caller::Login(login) => Some(login_access(get, gh, login)?),
        _ => None,
    };
    params::check(args, &t.params).map_err(Failed::Args)?;
    let limits = plugin::limits(&get("TEHAS_PLUGIN_LIMITS"))
        .ok_or_else(|| Failed::Refused("TEHAS_PLUGIN_LIMITS cannot be read".into()))?;
    let image = r
        .image
        .clone()
        .ok_or_else(|| Failed::Refused("it has no build".into()))?;
    let _held = locks(home, plugin, &caller.lock(user))?;
    let id = begin(home, &r, caller)?;
    let dir = prepare::dir(home, plugin).join(format!("run-{id}"));
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
    };
    let (got, result, size) = run_in(x, &run, &t, args);
    // A row left open keeps its directory, for a sweep to find.
    end(home, id, result, size).map_err(|line| {
        Failed::Tool(format!(
            "the store did not end run {id}, so {dir_text} is kept: {line}"
        ))
    })?;
    let _ = fs::remove_dir_all(&dir);
    got
}

/// A slot of two and the caller's lock, taken now, or busy.
fn locks(home: &Path, name: &str, by: &str) -> Result<[File; 2], Failed> {
    let dir = runs::calls(home, name);
    let no = |e: String| Failed::Refused(e);
    fs::create_dir_all(&dir).map_err(|e| no(format!("{}: {e}", dir.display())))?;
    let busy = || Failed::Refused("busy".into());
    let slot = match try_lock(&dir.join("slot-0.lock")).map_err(no)? {
        Some(f) => f,
        None => try_lock(&dir.join("slot-1.lock"))
            .map_err(no)?
            .ok_or_else(busy)?,
    };
    let by = try_lock(&dir.join(format!("by-{by}.lock")))
        .map_err(no)?
        .ok_or_else(busy)?;
    Ok([slot, by])
}

fn begin(home: &Path, r: &Ready, caller: &Caller) -> Result<u64, Failed> {
    let c = &r.pin.commit;
    // A call has no product: its head is the plugin's commit.
    let begun = crate::store::try_call(
        home,
        3,
        &[
            "plugin-run-begin",
            "--plugin",
            &r.pin.name,
            "--commit",
            c,
            "--kind",
            Kind::Call.word(),
            "--how",
            caller.trigger().word(),
            "--by",
            caller.by(),
            "--head",
            c,
        ],
    );
    match begun.map(|v| v["run"].as_u64()) {
        Ok(Some(id)) if id > 0 => Ok(id),
        Ok(_) => Err(Failed::Tool("the store gave the call no run".into())),
        Err(no) => Err(Failed::Tool(format!(
            "the store did not begin the call: {}",
            no.line
        ))),
    }
}

/// The call in its directory: the answer, the row's result and the answer's
/// size.
fn run_in(
    x: &mut dyn Exec,
    run: &Run,
    t: &Tool,
    args: &Value,
) -> (Result<Value, Failed>, &'static str, Option<usize>) {
    let failed = |why: String| (Err(Failed::Tool(why)), "failed", None);
    let (ctx, argv) = match (context(run), sandbox::plugin_run(run)) {
        (Ok(c), Ok(a)) => (c, a),
        (Err(why), _) | (_, Err(why)) => return (Err(Failed::Refused(why)), "failed", None),
    };
    if let Err(e) = fs::create_dir(run.dir) {
        return failed(format!("{}: {e}", run.dir));
    }
    let files = [
        ("context.json", ctx),
        ("call.json", json!({"tool": t.id, "args": args})),
    ];
    for (file, v) in files {
        let wrote = write_atomic(
            Path::new(&run.path(file)),
            format!("{v}\n").as_bytes(),
            0o644,
            &Temp::Pid,
        );
        if let Err(e) = wrote {
            return failed(format!("{file} could not be written: {e}"));
        }
    }
    let limit = Duration::from_secs(t.timeout);
    let called = x.attached(&argv, limit, ANSWER_MAX, Path::new(&run.path("cid")));
    if !matches!(called, Called::Out(_) | Called::Exit(_)) {
        // A killed client leaves its container running, and the cid file
        // may not be written yet: its name finds it. One gone is fine.
        let rm = ["rm", "-f", &run.container()].map(String::from).to_vec();
        let _ = x.run(&Cmd::new("docker", rm));
    }
    match called {
        Called::Out(b) => {
            let v = std::str::from_utf8(&b)
                .ok()
                .and_then(|s| serde_json::from_str::<Strict>(s).ok())
                .map(|v| v.0)
                .filter(Value::is_object);
            match v {
                Some(v) => (Ok(v), "done", Some(b.len())),
                None => (
                    Err(Failed::Tool("the answer is not one JSON object".into())),
                    "failed",
                    Some(b.len()),
                ),
            }
        }
        Called::Over => failed("the answer is over 16 KiB".into()),
        Called::Exit(n) => failed(format!("the tool exited {n}")),
        Called::Killed => (
            Err(Failed::Tool(format!(
                "the tool ran past {} seconds",
                t.timeout
            ))),
            "timeout",
            None,
        ),
        Called::NotRun(why) => failed(format!("the tool did not start: {why}")),
    }
}

/// A JSON value with no key twice in an object at any depth: serde_json
/// keeps the last, and a host's parser may keep another.
struct Strict(Value);

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Strict, D::Error> {
        d.deserialize_any(Strict(Value::Null))
    }
}

impl<'de> Visitor<'de> for Strict {
    type Value = Strict;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("JSON")
    }
    fn visit_unit<E>(self) -> Result<Strict, E> {
        Ok(Strict(Value::Null))
    }
    fn visit_bool<E>(self, b: bool) -> Result<Strict, E> {
        Ok(Strict(b.into()))
    }
    fn visit_i64<E>(self, n: i64) -> Result<Strict, E> {
        Ok(Strict(n.into()))
    }
    fn visit_u64<E>(self, n: u64) -> Result<Strict, E> {
        Ok(Strict(n.into()))
    }
    fn visit_f64<E>(self, n: f64) -> Result<Strict, E> {
        Ok(Strict(n.into()))
    }
    fn visit_str<E>(self, s: &str) -> Result<Strict, E> {
        Ok(Strict(s.into()))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
        let mut v = Vec::new();
        while let Some(Strict(e)) = a.next_element()? {
            v.push(e);
        }
        Ok(Strict(v.into()))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
        let mut m = serde_json::Map::new();
        while let Some(k) = a.next_key::<String>()? {
            if m.contains_key(&k) {
                return Err(de::Error::custom(format!("{k} twice")));
            }
            let Strict(v) = a.next_value()?;
            m.insert(k, v);
        }
        Ok(Strict(m.into()))
    }
}

/// The row ended with what the call came to, or the store's refusal.
fn end(home: &Path, id: u64, result: &str, size: Option<usize>) -> Result<(), String> {
    let id = id.to_string();
    let size = size.map(|n| n.to_string());
    let mut args = vec!["plugin-run-end", "--run", &id, "--result", result];
    if let Some(n) = &size {
        args.extend(["--size", n.as_str()]);
    }
    crate::store::try_call(home, 3, &args)
        .map(drop)
        .map_err(|no| no.line)
}

#[cfg(test)]
#[path = "call_tests.rs"]
mod tests;
