//! One call of a plugin's tool (docs/specs/2026-11-21-plugins-v2-chats.md,
//! "A call"; docs/specs/2026-11-21-plugins-mcp.md, "The broker"). `tehas
//! plugin call` and the MCP broker both call [`call`].
//!
//! In order: the plugin ready, the tool in its manifest and granted, the
//! caller, the arguments against the tool's `params`, a slot and the
//! caller's lock taken now or the call refused as busy, a row in
//! `plugin_runs`, the run's directory with its context and `call.json`, the
//! container attached at the tool's timeout, its answer read, then the row
//! ended and the directory removed. Nothing of the call is kept but the
//! row's time, size and result.

use super::exec::{Called, Exec};
use super::prepare;
use super::ready::{ready, Ready};
use super::runs;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::path::Path;
use std::time::Duration;
use tehas_core::file::{write_atomic, Temp};
use tehas_core::lock::try_lock;
use tehas_core::plugin::manifest::Tool;
use tehas_core::plugin::run::{context, Kind, Run, Trigger};
use tehas_core::plugin::{self, params, Cap};
use tehas_core::sandbox;

/// The most bytes of an answer.
pub const ANSWER_MAX: usize = 16 << 10;

/// The refusal of a login until the gate's access check is in core.
pub const LOGIN: &str = "a login is checked once the gate's access check is in core";

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
    /// The name of its lock, `by-<this>.lock`.
    fn lock(&self) -> String {
        match self {
            Caller::Login(_) => "chat".into(),
            Caller::Mcp(key) => format!("mcp-{key}"),
            Caller::Host => "host".into(),
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

fn is_key_id(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// One call of `tool` of `plugin` with `args`: its answer, one JSON object.
pub fn call(
    x: &mut dyn Exec,
    home: &Path,
    get: &dyn Fn(&str) -> String,
    plugin: &str,
    tool: &str,
    args: &Value,
    caller: &Caller,
) -> Result<Value, Failed> {
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
    match caller {
        Caller::Login(_) => return Err(Failed::Refused(LOGIN.into())),
        Caller::Mcp(key) if !is_key_id(key) => {
            return Err(Failed::Refused(
                "the key's id is not 8 hex characters".into(),
            ))
        }
        _ => {}
    }
    params::check(args, &t.params).map_err(Failed::Args)?;
    let limits = plugin::limits(&get("TEHAS_PLUGIN_LIMITS"))
        .ok_or_else(|| Failed::Refused("TEHAS_PLUGIN_LIMITS cannot be read".into()))?;
    let image = r
        .image
        .clone()
        .ok_or_else(|| Failed::Refused("it has no build".into()))?;
    let _held = locks(home, plugin, caller)?;
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
    let _ = fs::remove_dir_all(&dir);
    end(home, id, result, size);
    got
}

/// A slot of two and the caller's lock, taken now, or busy.
fn locks(home: &Path, name: &str, caller: &Caller) -> Result<[File; 2], Failed> {
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
    let by = try_lock(&dir.join(format!("by-{}.lock", caller.lock())))
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
    match x.attached(&argv, limit, ANSWER_MAX, Path::new(&run.path("cid"))) {
        Called::Out(b) => {
            let v = std::str::from_utf8(&b)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
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

/// The row ended with what the call came to.
fn end(home: &Path, id: u64, result: &str, size: Option<usize>) {
    let id = id.to_string();
    let size = size.map(|n| n.to_string());
    let mut args = vec!["plugin-run-end", "--run", &id, "--result", result];
    if let Some(n) = &size {
        args.extend(["--size", n.as_str()]);
    }
    if let Err(no) = crate::store::try_call(home, 3, &args) {
        eprintln!(
            "plugin call: run {id}: the store did not end it: {}",
            no.line
        );
    }
}

#[cfg(test)]
#[path = "call_tests.rs"]
mod tests;
