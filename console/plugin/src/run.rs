//! `tehas-plugin run`: one run of a plugin's image
//! (docs/specs/2026-11-21-plugins.md, "Runs", and the security part, "The
//! container", "Reading /out and /state", "The product", "The model socket").
//!
//! In order: the plugin ready at its pin and granted; its lock; the head of
//! `/product`; a row in `plugin_runs`; the run's directory with its context,
//! the product's clone and the model socket's directory; the container,
//! killed at its timeout; its log; `/out`, read through the tar reader, swapped
//! into the view or read as a hook's verdict; the volumes measured against
//! the disk limit; then the container, its `/work` and `/out` and the run's
//! directory removed, and the row ended with its result and tokens.

use crate::day::{self, FileDay};
use crate::host::{Ended, Host};
use crate::model::{self, Content, Server};
use crate::upstream;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tehas_core::file::{read_regular, write_atomic, Temp};
use tehas_core::grammar::commit_id;
use tehas_core::lock::try_lock;
use tehas_core::plugin::manifest::Manifest;
use tehas_core::plugin::run::{context, Kind, Pr, Run, Trigger};
use tehas_core::plugin::state::{State, Word};
use tehas_core::plugin::{self, out, verdict, Cap, Grants, Limits, Pin};
use tehas_core::{exit, features, sandbox};
use tehas_ops::plugin::exec::{Cmd, Exec, Git};
use tehas_ops::plugin::prepare;
use tehas_ops::plugin::runs::{self, Asked, HookAsk, DISK_REFUSAL, LOG_MAX};

/// `TEHAS_PLUGIN_LIMITS` when it is not set.
pub const LIMITS: Limits = Limits {
    memory: 2,
    cpus: 2,
    disk: 4,
};
/// `TEHAS_PLUGIN_MODEL_DAY` when it is not set.
pub const MODEL_DAY: u64 = 5_000_000;
/// Hooks of all plugins at once.
pub const HOOKS_AT_ONCE: usize = 2;
/// The commits of `/product`'s history.
const DEPTH: &str = "200";
/// The ref of a pull request's head in the product's mirror, while a hook
/// runs on it.
fn pr_ref(pr: u64) -> String {
    format!("refs/tehas/plugin-pr/{pr}")
}

/// What a run is given from outside: the home, the settings, the host's
/// models.json, and how long a hook waits between looks for a place.
pub struct Setup<'a> {
    pub home: &'a Path,
    pub get: &'a dyn Fn(&str) -> String,
    pub models: &'a Path,
    pub poll: Duration,
}

/// What the command line asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    pub name: String,
    pub kind: Kind,
    pub pr: Option<u64>,
    pub trigger: Option<Trigger>,
    pub by: Option<String>,
}

/// What a run came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Came {
    Done,
    Failed(String),
    Timeout,
}

impl Came {
    fn word(&self) -> &'static str {
        match self {
            Came::Done => "done",
            Came::Failed(_) => "failed",
            Came::Timeout => "timeout",
        }
    }
}

fn say(line: &str) {
    tehas_core::outln!("{line}");
}

/// A plugin as a run takes it: pinned, granted at the pin, ready, read.
struct Plugin {
    pin: Pin,
    grants: Grants,
    image: String,
    manifest: Manifest,
}

impl Plugin {
    fn has(&self, cap: Cap) -> bool {
        self.manifest.asks.contains(&cap.ask()) && self.grants.caps.contains(&cap)
    }
}

/// The plugin of `name`, or the exit that says why it may not run.
fn plugin_of(h: &mut dyn Host, s: &Setup, name: &str, person: bool) -> Result<Plugin, u8> {
    let get = s.get;
    let pins = plugin::pins(&get("TEHAS_PLUGINS")).unwrap_or_default();
    let Some(pin) = pins.into_iter().find(|p| p.name == name) else {
        say(&format!(
            "plugin {name}: TEHAS_PLUGINS names no such plugin"
        ));
        return Err(exit::USAGE);
    };
    let no = |why: String| {
        say(&format!("plugin {name}: not run: {why}"));
        Err(exit::NO)
    };
    let grants = match plugin::grants(&get(&plugin::grants_setting(name))) {
        Ok(g) if g.commit == pin.commit => g,
        _ => return no("it is granted nothing at its pin".into()),
    };
    let mut state = match prepare::read_state(s.home, name) {
        Some(st) if st.commit == pin.commit => st,
        _ => return no("it is unbuilt at its pin: run tehas update".into()),
    };
    if state.state == Word::Refused && person && state.refusal.as_deref() == Some(DISK_REFUSAL) {
        state.state = Word::Ready;
        state.refusal = None;
        if let Err(e) = prepare::write_state(s.home, name, &state) {
            return no(format!("state.json could not be written: {e}"));
        }
        say(&format!(
            "plugin {name}: a person's run starts it again after its /state was emptied"
        ));
    }
    if state.state != Word::Ready {
        let why = state.refusal.clone().unwrap_or_default();
        return no(format!("it is {}: {why}", state.state.word()));
    }
    let Some(image) = state.image.clone() else {
        return no("it has no build, so there is nothing to run".into());
    };
    let r = match prepare::read(h.exec(), s.home, name, &pin.commit) {
        Ok(r) => r,
        Err(why) => return no(why),
    };
    let same = state.manifest_sha256.as_deref() == Some(r.sha256.as_str());
    let manifest = match r.manifest {
        Some(m) if r.refusals.is_empty() && same => m,
        _ => return no("its manifest is not the one tehas update read: run tehas update".into()),
    };
    Ok(Plugin {
        pin,
        grants,
        image,
        manifest,
    })
}

fn lock_with(path: &Path, shared: bool) -> Result<File, String> {
    let f = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let got = if shared { f.lock_shared() } else { f.lock() };
    got.map(|()| f)
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn git(x: &mut dyn Exec, how: Git, dir: &str, args: &[&str]) -> Result<String, String> {
    let mut all = vec!["-C".to_string(), dir.to_string()];
    all.extend(args.iter().map(|a| a.to_string()));
    let d = x.run(&Cmd::git(how, all));
    if d.ok {
        Ok(d.text())
    } else {
        Err(format!("git {} failed: {}", args.join(" "), d.err))
    }
}

fn docker(x: &mut dyn Exec, args: &[&str]) -> Result<String, String> {
    let d = x.run(&Cmd::new(
        "docker",
        args.iter().map(|a| a.to_string()).collect(),
    ));
    if d.ok {
        Ok(d.text())
    } else {
        Err(format!("docker {} failed: {}", args.join(" "), d.err))
    }
}

fn commit_of(x: &mut dyn Exec, repo: &str, rev: &str) -> Result<String, String> {
    let c = git(
        x,
        Git::Local,
        repo,
        &["rev-parse", "-q", "--verify", &format!("{rev}^{{commit}}")],
    )?;
    if commit_id(&c) {
        Ok(c)
    } else {
        Err(format!("{rev} is no commit in {repo}"))
    }
}

/// The mirror and its default branch, as the settings name them.
fn mirror(get: &dyn Fn(&str) -> String) -> Result<(String, String), String> {
    let (repo, branch) = (get("TEHAS_WORK_REPO"), get("TEHAS_BRANCH"));
    if repo.is_empty() || branch.is_empty() {
        return Err("TEHAS_WORK_REPO and TEHAS_BRANCH name no product mirror".into());
    }
    Ok((repo, branch))
}

/// The default branch's head the poll fetched into the mirror.
pub fn default_head(x: &mut dyn Exec, get: &dyn Fn(&str) -> String) -> Result<String, String> {
    let (repo, branch) = mirror(get)?;
    commit_of(x, &repo, &format!("refs/remotes/origin/{branch}"))
}

/// Every directory 0755 and every file readable by all, so the container's
/// own uid reads it: the product is the host user's, mounted read only.
fn open_up(dir: &Path) -> std::io::Result<()> {
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755))?;
    for e in fs::read_dir(dir)? {
        let path = e?.path();
        let m = fs::symlink_metadata(&path)?;
        if m.is_dir() {
            open_up(&path)?;
        } else if m.is_file() {
            let mode = if m.permissions().mode() & 0o100 != 0 {
                0o755
            } else {
                0o644
            };
            fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

/// `/product`: a clone of the mirror's commit made for the run, with no hook,
/// filter or remote of it, at `head`.
fn clone_product(
    x: &mut dyn Exec,
    get: &dyn Fn(&str) -> String,
    from: &str,
    head: &str,
    dir: &Path,
) -> Result<(), String> {
    let (repo, branch) = mirror(get)?;
    let to = dir.display().to_string();
    let clone = [
        "clone",
        "--quiet",
        "--no-local",
        "--no-checkout",
        "--depth",
        DEPTH,
        "--single-branch",
        "--branch",
        &branch,
        "-c",
        "core.hooksPath=/dev/null",
        &repo,
        &to,
    ];
    let d = x.run(&Cmd::git(
        Git::Local,
        clone.iter().map(|a| a.to_string()).collect(),
    ));
    if !d.ok {
        return Err(format!("the product could not be cloned: {}", d.err));
    }
    let want = format!("+{from}:refs/tehas/head");
    let steps: [&[&str]; 3] = [
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            "--depth",
            DEPTH,
            "origin",
            &want,
        ],
        &["remote", "remove", "origin"],
        &[
            "-c",
            "advice.detachedHead=false",
            "checkout",
            "--quiet",
            "--detach",
            head,
        ],
    ];
    for args in steps {
        git(x, Git::Local, &to, args)?;
    }
    let got = git(x, Git::Local, &to, &["rev-parse", "HEAD"])?;
    if got != head {
        return Err(format!("the product's clone is at {got}, not {head}"));
    }
    open_up(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// A size as `docker system df` words it, in bytes: decimal units.
pub fn bytes_of(size: &str) -> Option<u64> {
    let s = size.trim();
    let at = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (n, unit) = s.split_at(at);
    let n: f64 = n.trim().parse().ok()?;
    let k: f64 = match unit {
        "B" => 1.0,
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => return None,
    };
    (n >= 0.0).then_some((n * k) as u64)
}

/// The size of each named volume, from `docker system df -v`.
fn volume_sizes(x: &mut dyn Exec, names: &[String]) -> Result<Vec<u64>, String> {
    let out = docker(x, &["system", "df", "-v", "--format", "{{json .Volumes}}"])?;
    let list: Vec<Value> =
        serde_json::from_str(&out).map_err(|_| "docker system df gave no list".to_string())?;
    names
        .iter()
        .map(|n| {
            let v = list.iter().find(|v| v["Name"] == n.as_str());
            match v {
                None => Ok(0),
                Some(v) => v["Size"]
                    .as_str()
                    .and_then(bytes_of)
                    .ok_or(format!("docker system df gave no size of {n}")),
            }
        })
        .collect()
}

/// The new view swapped into `view/` with one rename, the old one removed.
fn swap_view(new: &Path, view: &Path) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("the view could not be put in place: {e}");
    if fs::symlink_metadata(view).is_err() {
        return fs::rename(new, view).map_err(fail);
    }
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        new,
        rustix::fs::CWD,
        view,
        rustix::fs::RenameFlags::EXCHANGE,
    )
    .map_err(|e| fail(e.into()))?;
    // `new` now holds the old view.
    fs::remove_dir_all(new).map_err(fail)
}

/// What a request's content may hold: images too only with `model:images`
/// asked and granted.
pub fn content(m: &Manifest, g: &Grants) -> Content {
    let cap = Cap::ModelImages;
    if m.asks.contains(&cap.ask()) && g.caps.contains(&cap) {
        Content::TextAndImages
    } else {
        Content::Text
    }
}

struct Socket {
    server: Option<Server>,
}

fn model_socket(
    s: &Setup,
    dir: &Path,
    id: &str,
    tokens: u64,
    content: Content,
) -> Result<Socket, String> {
    let get = s.get;
    let provider = get("PI_ENSEMBLE_SUBAGENT_PROVIDER");
    let up = upstream::read(s.models, &provider, id)?;
    let day_limit = get("TEHAS_PLUGIN_MODEL_DAY").parse().unwrap_or(MODEL_DAY);
    let day = FileDay::new(&prepare::root(s.home).join("model-day"), day::today);
    let run = model::Run {
        id: id.to_string(),
        tokens,
        day_limit,
        day: Arc::new(day),
        upstream: up,
        curl: "curl".into(),
        content,
    };
    Server::start(dir, run)
        .map(|server| Socket {
            server: Some(server),
        })
        .map_err(|e| format!("the model socket could not be served: {e}"))
}

impl Socket {
    fn tokens(&mut self) -> u64 {
        self.server.take().map_or(0, |s| s.stop().tokens)
    }
}

/// One run, from its row to its end. `head` is `/product`'s commit.
struct Started<'a> {
    p: &'a Plugin,
    kind: Kind,
    trigger: Trigger,
    head: String,
    pr: Option<Pr>,
}

fn by_of(trigger: Trigger, by: Option<&str>) -> String {
    match (trigger, by) {
        (Trigger::Action, Some(login)) => login.to_string(),
        (Trigger::Timer, _) => "timer".into(),
        _ => "host".into(),
    }
}

/// The run of `st`, its row begun with `by`. The row is ended whatever
/// happens after it began.
fn one_run(h: &mut dyn Host, s: &Setup, st: &Started, by: &str) -> Came {
    let p = st.p;
    let name = &p.pin.name;
    let begun = h.store(&[
        "plugin-run-begin",
        "--plugin",
        name,
        "--commit",
        &p.pin.commit,
        "--kind",
        st.kind.word(),
        "--how",
        st.trigger.word(),
        "--by",
        by,
        "--head",
        &st.head,
    ]);
    let id = match begun.map(|v| v["run"].as_u64()) {
        Ok(Some(id)) if id > 0 => id,
        Ok(_) => return Came::Failed("the store gave the run no id".into()),
        Err(why) => return Came::Failed(format!("the store did not begin the run: {why}")),
    };
    let dir = prepare::dir(s.home, name).join(format!("run-{id}"));
    let mut tokens = 0;
    let came = run_in(h, s, st, id, &dir, &mut tokens);
    let _ = fs::remove_dir_all(&dir);
    let tokens_word = tokens.to_string();
    let id_word = id.to_string();
    if let Err(why) = h.store(&[
        "plugin-run-end",
        "--run",
        &id_word,
        "--result",
        came.word(),
        "--tokens",
        &tokens_word,
    ]) {
        say(&format!(
            "plugin {name}: run {id}: the store did not end it: {why}"
        ));
    }
    let line = match &came {
        Came::Failed(why) => format!("failed: {why}"),
        c => c.word().to_string(),
    };
    say(&format!(
        "plugin {name}: {} run {id} at {}: {line}, {tokens} tokens",
        st.kind.word(),
        st.head
    ));
    came
}

fn run_in(
    h: &mut dyn Host,
    s: &Setup,
    st: &Started,
    id: u64,
    dir: &Path,
    tokens: &mut u64,
) -> Came {
    let p = st.p;
    let name = p.pin.name.clone();
    let model_id = (s.get)("PI_ENSEMBLE_SUBAGENT_MODEL");
    let dir_text = dir.display().to_string();
    let run = Run {
        manifest: &p.manifest,
        grants: &p.grants,
        commit: &p.pin.commit,
        kind: st.kind,
        trigger: st.trigger,
        id,
        dir: &dir_text,
        image: &p.image,
        limits: plugin::limits(&(s.get)("TEHAS_PLUGIN_LIMITS")).unwrap_or(LIMITS),
        head: &st.head,
        model: &model_id,
        pr: st.pr.clone(),
    };
    let given = match run.given() {
        Ok(g) => g,
        Err(why) => return Came::Failed(why),
    };
    if let Err(e) = fs::create_dir(dir) {
        return Came::Failed(format!("{}: {e}", dir.display()));
    }
    let ctx = match context(&run) {
        Ok(c) => c,
        Err(why) => return Came::Failed(why),
    };
    if let Err(e) = write_atomic(
        Path::new(&run.path("context.json")),
        format!("{ctx}\n").as_bytes(),
        0o644,
        &Temp::Pid,
    ) {
        return Came::Failed(format!("the context could not be written: {e}"));
    }
    if given.product {
        let from = match &st.pr {
            Some(pr) => pr_ref(pr.number),
            None => format!("refs/remotes/origin/{}", (s.get)("TEHAS_BRANCH")),
        };
        if let Err(why) = clone_product(h.exec(), s.get, &from, &st.head, &dir.join("product")) {
            return Came::Failed(why);
        }
    }
    let mut socket = Socket { server: None };
    if let Some(budget) = given.tokens {
        let at = dir.join("model");
        if let Err(e) = fs::create_dir(&at)
            .and_then(|()| fs::set_permissions(&at, fs::Permissions::from_mode(0o755)))
        {
            return Came::Failed(format!("{}: {e}", at.display()));
        }
        socket = match model_socket(s, &at, &model_id, budget, content(&p.manifest, &p.grants)) {
            Ok(sock) => sock,
            Err(why) => return Came::Failed(why),
        };
    }
    let argv = match sandbox::plugin_run(&run) {
        Ok(w) => w,
        Err(why) => return Came::Failed(why),
    };
    let timeout = match st.kind {
        Kind::Refresh => p.manifest.timeout,
        Kind::Hook => p.manifest.hook,
    };
    let Some(timeout) = timeout else {
        return Came::Failed("the manifest gives the run no timeout".into());
    };
    let container = run.container();
    let ended = h.container(
        &argv,
        Duration::from_secs(timeout),
        Path::new(&run.path("cid")),
    );
    *tokens = socket.tokens();
    let log = runs::log(s.home, &name, st.pr.as_ref().map(|pr| pr.number));
    if let Some(parent) = log.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(why) = h.logs(&container, &log, LOG_MAX) {
        say(&format!(
            "plugin {name}: run {id}: its log was not kept: {why}"
        ));
    }
    let mut came = match ended {
        Ended::Exit(0) => Came::Done,
        Ended::Exit(n) => Came::Failed(format!("the plugin exited {n}")),
        Ended::Killed => Came::Timeout,
        Ended::NotRun(why) => Came::Failed(why),
    };
    if came == Came::Done {
        came = match (&st.pr, p.has(Cap::View)) {
            (Some(_), _) => keep_verdict(h, &container, dir),
            (None, true) => keep_view(h, s, &name, &container, id),
            (None, false) => Came::Done,
        };
    }
    if let Some(over) = measure(h, s, &p.pin.name, id, &run.limits) {
        came = Came::Failed(over);
    }
    if let Some(pr) = &st.pr {
        write_hook_end(s, &name, pr, &came, dir);
    }
    clean(h.exec(), &name, &container, id);
    came
}

fn keep_view(h: &mut dyn Host, s: &Setup, name: &str, container: &str, id: u64) -> Came {
    let dir = prepare::dir(s.home, name);
    let new = dir.join(format!("view-{id}"));
    let limits = out::Limits {
        entries: out::ENTRIES,
        bytes: out::BYTES,
    };
    if let Err(why) = h.copy(&format!("{container}:/out"), &new, "out", limits) {
        let _ = fs::remove_dir_all(&new);
        return Came::Failed(format!("/out was refused, the last good view stays: {why}"));
    }
    match swap_view(&new, &dir.join("view")) {
        Ok(()) => Came::Done,
        Err(why) => {
            let _ = fs::remove_dir_all(&new);
            Came::Failed(why)
        }
    }
}

/// `/out/_tehas/verdict.json`, kept in the run's directory as `verdict/read`
/// when it fits the schema.
fn keep_verdict(h: &mut dyn Host, container: &str, dir: &Path) -> Came {
    let at = dir.join("verdict");
    let limits = out::Limits {
        entries: 8,
        bytes: verdict::BYTES_MAX,
    };
    if let Err(why) = h.copy(&format!("{container}:/out/_tehas"), &at, "_tehas", limits) {
        return Came::Failed(format!("/out/_tehas was refused: {why}"));
    }
    let bytes = match read_regular(&at.join("verdict.json"), verdict::BYTES_MAX) {
        Ok((b, _)) => b,
        Err(_) => return Came::Failed("the hook left no /out/_tehas/verdict.json".into()),
    };
    match verdict::read(&bytes) {
        Ok(_) => match fs::write(at.join("read"), &bytes) {
            Ok(()) => Came::Done,
            Err(e) => Came::Failed(format!("the verdict could not be kept: {e}")),
        },
        Err(why) => Came::Failed(format!("its verdict breaks the schema: {why}")),
    }
}

/// `hooks/<pr>.verdict.json`: the pull request and head the hook read, what
/// the run came to, and the verdict when it read one.
fn write_hook_end(s: &Setup, name: &str, pr: &Pr, came: &Came, dir: &Path) {
    let read = match came {
        Came::Done => fs::read(dir.join("verdict/read"))
            .ok()
            .and_then(|b| verdict::read(&b).ok()),
        _ => None,
    };
    let v = read.map(|v| {
        json!({
            "verdict": match v.verdict { verdict::Word::Pass => "pass", verdict::Word::Note => "note" },
            "summary": v.summary,
            "findings": v.findings.iter().map(|f| {
                let mut o = json!({"text": f.text});
                if let Some(file) = &f.file { o["file"] = json!(file); }
                if let Some(line) = f.line { o["line"] = json!(line); }
                o
            }).collect::<Vec<_>>(),
        })
    });
    let body = json!({
        "api": 1,
        "pr": pr.number,
        "head": pr.head,
        "result": came.word(),
        "verdict": v,
    });
    let path = runs::verdict(s.home, name, pr.number);
    if let Err(e) = write_atomic(&path, format!("{body}\n").as_bytes(), 0o600, &Temp::Pid) {
        say(&format!("plugin {name}: {}: {e}", path.display()));
    }
}

/// The run's volumes against `disk`: why the run failed, if it passed it. A
/// `/state` over it alone is emptied, and the plugin refused until a person
/// runs it.
fn measure(h: &mut dyn Host, s: &Setup, name: &str, id: u64, limits: &Limits) -> Option<String> {
    let vol = |what: &str| format!("tehas-plugin-{name}-{what}");
    let names = [
        vol(&format!("work-{id}")),
        vol(&format!("out-{id}")),
        vol("state"),
    ];
    let sizes = match volume_sizes(h.exec(), &names) {
        Ok(s) => s,
        Err(why) => {
            say(&format!(
                "plugin {name}: run {id}: its volumes were not measured: {why}"
            ));
            return None;
        }
    };
    let disk = u64::from(limits.disk) << 30;
    let total: u64 = sizes.iter().sum();
    if total <= disk {
        return None;
    }
    if sizes[2] > disk {
        let _ = docker(h.exec(), &["volume", "rm", "-f", &names[2]]);
        let mut st = prepare::read_state(s.home, name)
            .unwrap_or_else(|| State::new(Word::Refused, &"0".repeat(40)));
        st.state = Word::Refused;
        st.refusal = Some(DISK_REFUSAL.to_string());
        let _ = prepare::write_state(s.home, name, &st);
    }
    Some(format!(
        "its volumes hold {total} bytes, more than disk={} GiB",
        limits.disk
    ))
}

/// The container, its `/work` and `/out`. What cannot be removed now the next
/// tick removes.
pub fn clean(x: &mut dyn Exec, name: &str, container: &str, id: u64) {
    let _ = docker(x, &["rm", "-f", container]);
    let work = format!("tehas-plugin-{name}-work-{id}");
    let out = format!("tehas-plugin-{name}-out-{id}");
    let _ = docker(x, &["volume", "rm", "-f", &work, &out]);
}

/// `tehas-plugin run`: the exit status.
pub fn run(h: &mut dyn Host, s: &Setup, ask: &Ask) -> u8 {
    if !features::on_in("plugins", s.get) {
        say("plugins are off: TEHAS_PLUGIN is not on, nothing runs");
        return exit::SETTINGS;
    }
    if let Err(why) = mirror(s.get) {
        say(&format!("plugin {}: {why}", ask.name));
        return exit::SETTINGS;
    }
    match ask.kind {
        Kind::Refresh => refresh(h, s, ask),
        Kind::Hook => hook(h, s, ask),
    }
}

fn refresh(h: &mut dyn Host, s: &Setup, ask: &Ask) -> u8 {
    let name = &ask.name;
    let asked = runs::read_request(s.home, name);
    let _ = fs::remove_file(runs::request(s.home, name));
    let trigger = ask
        .trigger
        .or(asked.as_ref().map(|a| a.trigger))
        .unwrap_or(Trigger::Host);
    let by = ask
        .by
        .clone()
        .or(asked.map(|a: Asked| a.by))
        .filter(|b| runs::is_by(b));
    let person = matches!(trigger, Trigger::Host | Trigger::Action);
    let p = match plugin_of(h, s, name, person) {
        Ok(p) => p,
        Err(rc) => return rc,
    };
    let lock = match try_lock(&runs::refresh_lock(s.home, name)) {
        Ok(Some(l)) => l,
        Ok(None) => {
            say(&format!("plugin {name}: running: a run holds its lock"));
            return exit::NO;
        }
        Err(why) => {
            say(&format!("plugin {name}: {why}"));
            return exit::FAILED;
        }
    };
    let head = match default_head(h.exec(), s.get) {
        Ok(c) => c,
        Err(why) => {
            say(&format!("plugin {name}: not run: {why}"));
            return exit::FAILED;
        }
    };
    let _ = write_atomic(
        &runs::tried(s.home, name),
        format!("{head}\n").as_bytes(),
        0o600,
        &Temp::Pid,
    );
    let st = Started {
        p: &p,
        kind: Kind::Refresh,
        trigger,
        head,
        pr: None,
    };
    let came = one_run(h, s, &st, &by_of(trigger, by.as_deref()));
    drop(lock);
    if came == Came::Done {
        exit::OK
    } else {
        exit::FAILED
    }
}

/// A place among the hooks that run at once, waited for.
fn hook_place(s: &Setup) -> Result<File, String> {
    loop {
        for n in 1..=HOOKS_AT_ONCE {
            let at = prepare::root(s.home).join(format!("hook-{n}.lock"));
            if let Some(f) = try_lock(&at)? {
                return Ok(f);
            }
        }
        std::thread::sleep(s.poll);
    }
}

fn hook(h: &mut dyn Host, s: &Setup, ask: &Ask) -> u8 {
    let name = &ask.name;
    let Some(pr) = ask.pr else {
        say(&format!("plugin {name}: a hook names its pull request"));
        return exit::USAGE;
    };
    let p = match plugin_of(h, s, name, false) {
        Ok(p) => p,
        Err(rc) => return rc,
    };
    if !p.has(Cap::HookAfterWork) {
        say(&format!(
            "plugin {name}: not run: it is not granted hook:after:work"
        ));
        return exit::NO;
    }
    let _ = fs::create_dir_all(runs::hooks(s.home, name));
    // One unit of a pull request at a time; then one hook of the plugin, a
    // place among all hooks, and no refresh of the plugin while it runs.
    let mut locks = Vec::new();
    match try_lock(&runs::hook_lock(s.home, name, pr)) {
        Ok(Some(l)) => locks.push(l),
        Ok(None) => {
            say(&format!("plugin {name}: running: the hook on #{pr} runs"));
            return exit::NO;
        }
        Err(why) => {
            say(&format!("plugin {name}: {why}"));
            return exit::FAILED;
        }
    }
    let waits = [
        lock_with(&runs::hooks(s.home, name).join("hook.lock"), false),
        hook_place(s),
        lock_with(&runs::refresh_lock(s.home, name), true),
    ];
    for w in waits {
        match w {
            Ok(l) => locks.push(l),
            Err(why) => {
                say(&format!("plugin {name}: {why}"));
                return exit::FAILED;
            }
        }
    }
    let mut rc = exit::OK;
    let mut ran: Option<String> = None;
    // A newer request that came while it ran runs next.
    while let Some(req) = runs::read_hook(s.home, name, pr) {
        if ran.as_deref() == Some(req.head.as_str()) {
            break;
        }
        ran = Some(req.head.clone());
        rc = match hook_once(h, s, &p, ask, &req) {
            Came::Done => exit::OK,
            _ => exit::FAILED,
        };
    }
    if ran.is_none() {
        say(&format!("plugin {name}: no request of a hook on #{pr}"));
        return exit::NO;
    }
    let _ = fs::remove_file(runs::hook_request(s.home, name, pr));
    drop(locks);
    rc
}

fn hook_once(h: &mut dyn Host, s: &Setup, p: &Plugin, ask: &Ask, req: &HookAsk) -> Came {
    let name = &p.pin.name;
    let trigger = ask.trigger.unwrap_or(req.trigger);
    let mut head = req.head.clone();
    let mirrored = p.has(Cap::Product);
    if mirrored {
        let (repo, _) = match mirror(s.get) {
            Ok(m) => m,
            Err(why) => return Came::Failed(why),
        };
        let want = format!("+refs/pull/{}/head:{}", req.pr, pr_ref(req.pr));
        let fetched = git(
            h.exec(),
            Git::Fetch,
            &repo,
            &["fetch", "--quiet", "--no-tags", "origin", &want],
        )
        .and_then(|_| commit_of(h.exec(), &repo, &pr_ref(req.pr)));
        match fetched {
            Ok(c) => {
                if c != head {
                    say(&format!(
                        "plugin {name}: #{} moved to {c} since it was asked for at {head}",
                        req.pr
                    ));
                }
                head = c;
            }
            Err(why) => {
                return Came::Failed(format!("the head of #{} was not fetched: {why}", req.pr))
            }
        }
    }
    let st = Started {
        p,
        kind: Kind::Hook,
        trigger,
        head: head.clone(),
        pr: Some(Pr {
            number: req.pr,
            base: req.base.clone(),
            head,
        }),
    };
    let came = one_run(h, s, &st, &by_of(trigger, None));
    if mirrored {
        if let Ok((repo, _)) = mirror(s.get) {
            let _ = git(
                h.exec(),
                Git::Local,
                &repo,
                &["update-ref", "-d", &pr_ref(req.pr)],
            );
        }
    }
    came
}

/// The words of `tehas-plugin run`: `<name> --kind refresh|hook [--pr N]
/// [--trigger T] [--by L]`. A hook's unit gives `<name>-<pr>` for both.
pub fn words(given: &[String]) -> Option<Ask> {
    let (first, rest) = given.split_first()?;
    let mut kind = None;
    let mut pr: Option<u64> = None;
    let mut trigger = None;
    let mut by = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it.next()?;
        match flag.as_str() {
            "--kind" if kind.is_none() => {
                kind = Some(match value.as_str() {
                    "refresh" => Kind::Refresh,
                    "hook" => Kind::Hook,
                    _ => return None,
                })
            }
            "--pr" if pr.is_none() => pr = Some(number(value)?),
            "--trigger" if trigger.is_none() => trigger = Some(runs::trigger(value)?),
            "--by" if by.is_none() && runs::is_by(value) => by = Some(value.clone()),
            _ => return None,
        }
    }
    let kind = kind?;
    let mut name = first.clone();
    if kind == Kind::Hook && pr.is_none() {
        let (n, p) = first.rsplit_once('-')?;
        pr = Some(number(p)?);
        name = n.to_string();
    }
    if !plugin::is_name(&name) || (kind == Kind::Refresh && pr.is_some()) {
        return None;
    }
    if by.is_some() && trigger != Some(Trigger::Action) {
        return None;
    }
    Some(Ask {
        name,
        kind,
        pr,
        trigger,
        by,
    })
}

fn number(s: &str) -> Option<u64> {
    (s.len() <= 9 && !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

/// The home of a run: TEHAS_HOME.
pub fn home(get: &dyn Fn(&str) -> String) -> Option<PathBuf> {
    Some(PathBuf::from(get("TEHAS_HOME"))).filter(|h| h.is_absolute())
}

#[cfg(test)]
pub(crate) mod tests;
