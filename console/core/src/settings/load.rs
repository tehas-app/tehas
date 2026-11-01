//! tehas_load: from the environment a command starts with to the one it runs
//! under. Whatever a refused load had done by then stays done, as a login
//! shell needs: env.sh goes on to read the GitHub App's names after a refusal.
//!
//! - instance.env is authoritative. Settings of the same name in the
//!   environment are dropped; only the names of `KEPT` stay, and from
//!   ~/.config/tehas/env the model endpoints' tokens, TEHAS_MODEL_KEY_*.
//!   instance.env that sets a token is refused: it is in a repository.
//! - PI_RUKAS_FOLLOW may be set in instance.env, over the line of versions.env.
//! - Loading runs no git command, makes no network call and writes nothing.
//! - TEHAS_IMPROVE_REPO switches the improvement loop on; unset, there is none.
//!   TEHAS_IMPROVE_RUKAS_REPO needs it and not TEHAS_IMPROVE_RUKAS_SKIP=1.
//!   Neither is TEHAS_REPO: a person triages findings before they are tickets.
//!   TEHAS_REVIEW is the review loop's switch: 0, the default, and tehasbot
//!   review exits at once and calls nothing.
//! - TEHAS_GITHUB_APP_ID, TEHAS_GITHUB_APP_INSTALLATION and TEHAS_BOT_LOGIN name
//!   the GitHub App the factory acts as (lib/github-app.sh), all three or none.
//!   With none, it acts with GH_TOKEN from ~/.config/tehas/env.
//! - TEHAS_MILLWRIGHT_LOGIN names the millwright: an account whose tehas:ready
//!   and planning answers the dispatcher takes within a narrower grant than a
//!   person's (lib/millwright.sh). Unset means there is none. An App's
//!   <slug>[bot] (REST) and <slug> (GraphQL) are one account, never the bot nor
//!   an allowed labeller. A TEHAS_FIXER_* name stops the load.
//! - TEHAS_LOW_GATE names a command the dispatcher runs before it starts the
//!   lowest issues. Unset is valid and means there is no gate. It is asked with
//!   nothing of the poll's environment but PATH, and it may not name a file
//!   under TEHAS_HOME or the instance directory: those are the trees a merged
//!   pull request writes, and the gate runs as the tehas user.
//! - Nothing falls back to a project: a setting with no generic default stays
//!   unset until a command requires it.

use super::rules::{self, Trees, When, ROLES, TABLE};
use std::collections::{BTreeMap, BTreeSet};

/// The names of the two families the environment may give: the home, the
/// instance directory, the tests' hooks, the poll's word to tehasbot status,
/// the two words a cycle hands down (tehas-cycle@.service, tehasbot cycle to
/// tehas work), and what the host chooses in ~/.config/tehas/env, which env.sh
/// reads before a load.
const KEPT: &[&str] = &[
    "TEHAS_HOME",
    "TEHAS_INSTANCE_DIR",
    "TEHAS_WORK_CMD",
    "TEHAS_STATUS_CMD",
    "TEHAS_CONSOLE",
    "TEHAS_CONSOLE_CMD",
    "TEHAS_INDEX_CMD",
    "TEHAS_PLAN_CMD",
    "TEHAS_PLAN_START_CMD",
    "TEHAS_INDEX_START_CMD",
    "TEHAS_INDEX_LOCK_WAIT",
    "TEHAS_DEV_LOCK_WAIT",
    "TEHAS_CYCLE_START_CMD",
    "TEHAS_CYCLE_STOP_CMD",
    "TEHAS_CYCLE_START_WAIT",
    "TEHAS_CGROUP_ROOT",
    "TEHAS_SLICE_WAIT",
    "TEHAS_DISK_WAIT",
    "TEHAS_PREFLIGHT_WAIT",
    "TEHAS_STOP_WAIT",
    "TEHAS_FETCH_WAIT",
    "TEHAS_CHECK_WAIT",
    "TEHAS_CONSOLE_WAIT",
    "TEHAS_WORK_GRACE",
    "TEHAS_WATCH_TICK",
    "TEHAS_WATCH_CLOCK",
    "TEHAS_CAPS_WAIT",
    "TEHAS_TEXT_WAIT",
    "TEHAS_BUILD_TIMEOUT",
    "TEHAS_CODE",
    "TEHAS_CYCLE_RETRY_WAIT",
    "TEHAS_CYCLE_STOP_GH_WAIT",
    "TEHAS_CYCLE_FROM",
    "TEHAS_CYCLE_ATTEMPT",
    "TEHAS_TIDY_CMD",
    "TEHAS_DISPATCH_CMD",
    "TEHAS_HEALTH_CMD",
    "TEHAS_TIDY_WAIT",
    "TEHAS_ANALYTICS_WAIT",
    "TEHAS_FROM_POLL",
    "TEHAS_GITHUB_API",
    "TEHAS_GITHUB_APP_KEY",
    "TEHAS_GITHUB_RENEW_EVERY",
    "TEHAS_ENERGY",
    "TEHAS_ENERGY_URL",
    "TEHAS_MCP",
    "TEHAS_MCP_KEYS",
    "TEHAS_MCP_DASHBOARD_KEYS",
    "TEHAS_PROC_DIR",
    "TEHAS_SETTINGS_CMD",
];

/// The names an earlier version read are refused, not ignored: a line that
/// does nothing in an instance file is a mistake the operator sees. The order
/// picks which of two retired lines is reported.
const RETIRED: [&str; 7] = [
    "TEHAS_FIXER_LOGIN",
    "TEHAS_FIXER_MAX_RETRIES",
    "TEHAS_FIXER_MAX_PLAN_ANSWERS",
    "TEHAS_CLONES",
    "TEHAS_WORK_CPUS",
    "TEHAS_WORK_MEMORY",
    "TEHAS_DISPATCHER",
];

/// What the shell of the source sets for itself. BASH_ENV is not given to it:
/// that file would run in the shell that reads the settings.
const SHELL_OWN: [&str; 5] = ["_", "PWD", "OLDPWD", "SHLVL", "BASH_ENV"];

/// A shell's variables as bytes, and which of them this load has set.
#[derive(Clone, Default)]
pub struct Vars {
    map: BTreeMap<Vec<u8>, Vec<u8>>,
    set: BTreeSet<Vec<u8>>,
}

impl Vars {
    pub fn new(vars: impl IntoIterator<Item = (Vec<u8>, Vec<u8>)>) -> Vars {
        Vars {
            map: vars.into_iter().collect(),
            set: BTreeSet::new(),
        }
    }
    pub fn is_set(&self, name: &str) -> bool {
        self.map.contains_key(name.as_bytes())
    }
    /// The value as text, empty when it is unset. A byte that is no text in a
    /// value breaks every rule that could read it.
    pub fn text(&self, name: &str) -> String {
        self.map
            .get(name.as_bytes())
            .map_or(String::new(), |v| String::from_utf8_lossy(v).into_owned())
    }
    pub fn set(&mut self, name: &str, value: impl Into<Vec<u8>>) {
        self.put(name.as_bytes().to_vec(), value.into());
    }
    fn put(&mut self, name: Vec<u8>, value: Vec<u8>) {
        self.set.insert(name.clone());
        self.map.insert(name, value);
    }
    pub fn unset(&mut self, name: &[u8]) {
        self.map.remove(name);
        self.set.remove(name);
    }
    /// What a child is given as its environment, in the order of the names.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.map.iter().map(|(k, v)| (k.as_slice(), v.as_slice()))
    }
    fn prefixed(&self, prefix: &str) -> Vec<String> {
        self.map
            .keys()
            .filter(|k| k.starts_with(prefix.as_bytes()))
            .map(|k| String::from_utf8_lossy(k).into_owned())
            .collect()
    }
}

/// What a load needs of the host.
pub trait Host {
    /// The file's bytes, or None when it cannot be opened. A file that opens
    /// and cannot be read is empty.
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    /// instance.env run as shell with these variables as its environment.
    fn source(&mut self, file: &str, env: &Vars) -> Sourced;
    /// What nproc says, or 1.
    fn cpus(&mut self) -> i64;
}

/// The exported variables after the source, and whether it ended well.
pub struct Sourced {
    pub vars: Vec<(Vec<u8>, Vec<u8>)>,
    pub ok: bool,
}

fn has(vars: &Vars, name: &str) -> bool {
    !vars.text(name).is_empty()
}

/// Where the instance's settings are: the environment, else the first line of
/// ~/.config/tehas/instance-dir, else ~/.config/tehas.
pub fn instance_dir(vars: &Vars, host: &dyn Host) -> String {
    let dir = vars.text("TEHAS_INSTANCE_DIR");
    if !dir.is_empty() {
        return dir;
    }
    let config = format!("{}/.config/tehas", vars.text("HOME"));
    let line = host.read(&format!("{config}/instance-dir")).map(|bytes| {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let first = text.split('\n').next().unwrap_or("");
        first.trim_matches([' ', '\t']).to_string()
    });
    line.filter(|l| !l.is_empty()).unwrap_or(config)
}

fn sweep(vars: &mut Vars) {
    let dropped: Vec<Vec<u8>> = vars
        .map
        .keys()
        .filter(|k| {
            let name = k.as_slice();
            (name.starts_with(b"TEHAS_") || name.starts_with(b"PI_ENSEMBLE_"))
                && !name.starts_with(b"TEHAS_MODEL_KEY_")
                && !KEPT.iter().any(|kept| kept.as_bytes() == name)
        })
        .cloned()
        .collect();
    for name in dropped {
        vars.unset(&name);
    }
}

/// A token in the instance directory would be in its repository. The lines are
/// read as text, not run: a hostile line needs a reader that does not execute
/// it.
fn scan(text: &[u8], file: &str) -> Result<(), String> {
    let space = |b: &u8| matches!(*b, b' ' | b'\t' | 0x0b | 0x0c | b'\r');
    let strip = |line: &[u8]| match line.iter().position(|b| !space(b)) {
        Some(at) => line[at..].to_vec(),
        None => Vec::new(),
    };
    for line in text.split(|b| *b == b'\n') {
        let line = strip(line);
        let line = strip(line.strip_prefix(b"export ").unwrap_or(&line));
        if line.starts_with(b"TEHAS_MODEL_KEY_") {
            return Err(format!(
                "{file} sets a TEHAS_MODEL_KEY_ token: put it in ~/.config/tehas/env"
            ));
        }
        if line.starts_with(b"TEHAS_MCP_KEYS=") || line.starts_with(b"TEHAS_MCP_DASHBOARD_KEYS=") {
            return Err(format!(
                "{file} sets MCP keys: put TEHAS_MCP_KEYS and TEHAS_MCP_DASHBOARD_KEYS in ~/.config/tehas/env"
            ));
        }
    }
    Ok(())
}

/// What the file changed, so a name it left alone stays as it was: not this
/// load's to export.
fn take(vars: &mut Vars, sourced: Vec<(Vec<u8>, Vec<u8>)>) {
    let own = |name: &[u8]| SHELL_OWN.iter().any(|own| own.as_bytes() == name);
    let after: BTreeMap<_, _> = sourced.into_iter().filter(|(k, _)| !own(k)).collect();
    let gone: Vec<Vec<u8>> = vars
        .map
        .keys()
        .filter(|k| !own(k) && is_name(k) && !after.contains_key(*k))
        .cloned()
        .collect();
    for name in gone {
        vars.unset(&name);
    }
    for (name, value) in after {
        if vars.map.get(&name) != Some(&value) {
            vars.put(name, value);
        }
    }
}

/// A name a shell can hold.
pub fn is_name(name: &[u8]) -> bool {
    !name.is_empty()
        && !name[0].is_ascii_digit()
        && name.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// A name pi-rukas does not read would reach the container and be ignored, and
/// that role would run on PI_ENSEMBLE_SUBAGENT_MODEL without a word, so it is
/// refused. So is a provider without its model: pi-rukas ignores it. A role
/// model with no provider of its own gets PI_ENSEMBLE_SUBAGENT_PROVIDER,
/// because Pi cannot find a custom provider's model by its id alone. An empty
/// line is not a setting: the name goes, so no container is given it.
fn role_models(vars: &mut Vars, file: &str) -> Result<(), String> {
    let roles = format!(" {} ", ROLES.join(" "));
    let families = [
        ("PI_ENSEMBLE_MODEL_", vars.prefixed("PI_ENSEMBLE_MODEL_")),
        (
            "PI_ENSEMBLE_PROVIDER_",
            vars.prefixed("PI_ENSEMBLE_PROVIDER_"),
        ),
    ];
    for (prefix, names) in families {
        for name in names {
            let role = &name[prefix.len()..];
            if !roles.contains(&format!(" {role} ")) {
                return Err(format!(
                    "{name} names no role pi-rukas gives a model to; the roles are{roles} (see {file})"
                ));
            }
            if !has(vars, &name) {
                vars.unset(name.as_bytes());
                continue;
            }
            valid(vars, &name, file)?;
        }
    }
    let provider = vars.text("PI_ENSEMBLE_SUBAGENT_PROVIDER");
    for role in ROLES {
        let (m, p) = (
            format!("PI_ENSEMBLE_MODEL_{role}"),
            format!("PI_ENSEMBLE_PROVIDER_{role}"),
        );
        if !has(vars, &m) && has(vars, &p) {
            return Err(format!(
                "{p} is set and {m} is not: pi-rukas ignores a provider without its model (see {file})"
            ));
        }
        if has(vars, &m) && !has(vars, &p) && !provider.is_empty() {
            vars.set(&p, provider.clone());
        }
    }
    Ok(())
}

fn retired_names(vars: &mut Vars, file: &str) -> Result<(), String> {
    for name in RETIRED {
        if !vars.is_set(name) {
            continue;
        }
        // The values that ask for what this version does: an earlier version
        // read 0 and a blank line alike as no clones, and rust or a blank line
        // as the Rust poll, so the line stays loadable.
        let value = vars.text(name);
        if matches!(
            (name, value.as_str()),
            ("TEHAS_CLONES", "0" | "") | ("TEHAS_DISPATCHER", "rust" | "")
        ) {
            vars.unset(name.as_bytes());
            continue;
        }
        const SLICE: &str = "the limit around all containers is /etc/systemd/system/tehas.slice";
        let reason = match name {
            "TEHAS_CLONES" => "is set and this version of tehas has no database clones".to_string(),
            "TEHAS_WORK_CPUS" => {
                format!("is set and this version of tehas has no CPU budget to set: {SLICE}")
            }
            "TEHAS_WORK_MEMORY" => {
                format!("is set and this version of tehas has no memory budget to set: {SLICE}")
            }
            "TEHAS_DISPATCHER" => "is set and this version of tehas has no bash poll: every poll is tehas-dispatch, so take the line out".to_string(),
            _ => format!(
                "is no longer read: rename it TEHAS_MILLWRIGHT_{}",
                name.strip_prefix("TEHAS_FIXER_").unwrap_or(name)
            ),
        };
        return Err(format!("{name} {reason} (see {file})"));
    }
    Ok(())
}

/// `export NAME=${NAME:-default}`.
fn or_default(vars: &mut Vars, name: &str, default: String) {
    let value = vars.map.get(name.as_bytes()).filter(|v| !v.is_empty());
    let value = value.cloned().unwrap_or(default.into_bytes());
    vars.set(name, value);
}

/// Derived and optional settings. The one host read, nproc, is made only when
/// the instance names no ceiling: by default a cycle per CPU, less one for the
/// host. The floor of the review cap is three: room for every cycle and two
/// pull requests that wait for a review, and a concurrency that is no number
/// leaves that floor. Agents get neither the host Docker socket nor SSH keys:
/// that is not a setting.
fn defaults(vars: &mut Vars, host: &mut dyn Host, uid: &str) {
    if has(vars, "TEHAS_REPO") {
        let repo = vars.text("TEHAS_REPO");
        let name = repo.rsplit('/').next().unwrap_or("").to_string();
        let home = vars.text("TEHAS_HOME");
        or_default(vars, "TEHAS_WORK_REPO", format!("{home}/repos/{name}"));
        // Where a cycle works, as its container sees it. On the host the tree
        // is $TEHAS_WORK_DIR-<issue>, a fresh clone that tehas work makes for
        // each cycle and the only tree that cycle's container can write.
        vars.set("TEHAS_WORK_DIR", format!("{home}/work/{name}"));
    }
    for setting in TABLE {
        if let Some(default) = setting.default {
            or_default(vars, setting.name, default.to_string());
        }
    }
    let ceiling = if has(vars, "TEHAS_TICKET_CONCURRENCY") {
        String::new()
    } else {
        host.cpus().saturating_sub(1).max(1).to_string()
    };
    or_default(vars, "TEHAS_TICKET_CONCURRENCY", ceiling);
    let tickets = vars.text("TEHAS_TICKET_CONCURRENCY");
    let awaiting = if rules::is_count(&tickets) {
        tickets.parse::<u64>().unwrap_or(0) + 2
    } else {
        3
    };
    or_default(vars, "TEHAS_MAX_AWAITING", awaiting.to_string());
    or_default(
        vars,
        "TEHAS_PLAN_TIMEOUT",
        rules::PLAN_TIMEOUT_MAX.to_string(),
    );
    or_default(
        vars,
        "TEHAS_CHILD_TIMEOUT",
        rules::CHILD_TIMEOUT_MAX.to_string(),
    );
    or_default(
        vars,
        "PI_ENSEMBLE_IMAGE",
        format!("tehas/pi-rukas:uid{uid}"),
    );
    or_default(vars, "PI_ENSEMBLE_HOST_ALIASES", String::new());
    // pi-rukas reads this name, and its own default is more than one, so it is
    // always told.
    let concurrency = vars.text("TEHAS_CONCURRENCY");
    vars.set("PI_ENSEMBLE_MAX_WORKSTREAMS", concurrency);
    // pi-rukas reads the bound of a gate in ms under two names, one for a
    // workstream's gate and one for the gate on the combined tree. A gate
    // refused below is not passed on.
    let verify = vars.text("TEHAS_VERIFY_TIMEOUT");
    if rules::is_count(&verify) {
        let ms = (verify.parse::<u64>().unwrap_or(0) * 1000).to_string();
        vars.set("PI_ENSEMBLE_VERIFY_TIMEOUT_MS", ms.clone());
        vars.set("PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS", ms);
    }
    vars.set("PI_ENSEMBLE_NO_DOCKER_SOCKET", "1");
    vars.set("PI_ENSEMBLE_NO_SSH", "1");
}

fn trees_of(vars: &Vars) -> [String; 3] {
    [
        vars.text("TEHAS_HOME"),
        vars.text("TEHAS_INSTANCE_DIR"),
        vars.text("TEHAS_WORK_REPO"),
    ]
}

fn valid(vars: &Vars, name: &str, file: &str) -> Result<(), String> {
    let [home, instance, work_repo] = trees_of(vars);
    let trees = Trees {
        home: &home,
        instance: &instance,
        work_repo: &work_repo,
    };
    rules::check(name, &vars.text(name), &trees)
        .map_err(|why| format!("{name} is not valid: {why} (see {file})"))
}

fn check_values(vars: &Vars, file: &str) -> Result<(), String> {
    for setting in TABLE {
        let asked = match setting.when {
            When::Always => true,
            When::Given => has(vars, setting.name),
            When::Named => vars.is_set(setting.name),
            When::Required => false,
        };
        if asked {
            valid(vars, setting.name, file)?;
        }
    }
    Ok(())
}

/// An entry for a setting the instance no longer has is a workaround removed
/// by half.
fn workarounds(vars: &Vars, file: &str) -> Result<(), String> {
    for entry in vars.text("TEHAS_RUKAS_WORKAROUNDS").split_whitespace() {
        let name = entry.rsplit_once('@').map_or(entry, |(name, _)| name);
        if name == "TEHAS_RUKAS_WORKAROUNDS" || !has(vars, name) {
            return Err(format!(
                "TEHAS_RUKAS_WORKAROUNDS names {name}, which is not set: remove its entry (see {file})"
            ));
        }
    }
    Ok(())
}

fn number(vars: &Vars, name: &str) -> u64 {
    vars.text(name).parse().unwrap_or(0)
}

/// Settings that only mean something together. Each of the numbers here has
/// passed its rule, so no comparison reads a failed parse as no refusal.
fn together(vars: &Vars, file: &str) -> Result<(), String> {
    let on = |name: &str| vars.text(name) == "1";
    let some = |name: &str| has(vars, name);
    workarounds(vars, file)?;
    // A GitHub App is named by all three or not at all: an ID without the
    // installation makes no token, and a token without the bot's login leaves
    // the dispatcher no name to keep out of the labellers.
    let app = [
        "TEHAS_GITHUB_APP_ID",
        "TEHAS_GITHUB_APP_INSTALLATION",
        "TEHAS_BOT_LOGIN",
    ];
    if app.iter().any(|n| some(n)) && !app.iter().all(|n| some(n)) {
        return Err(format!(
            "TEHAS_GITHUB_APP_ID, TEHAS_GITHUB_APP_INSTALLATION and TEHAS_BOT_LOGIN are set together or not at all (see {file})"
        ));
    }
    if some("TEHAS_ANALYTICS_SRC") && !some("TEHAS_ANALYTICS_SITE") {
        return Err(format!(
            "TEHAS_ANALYTICS_SITE is not set and TEHAS_ANALYTICS_SRC is (see {file})"
        ));
    }
    if some("TEHAS_IMPROVE_RUKAS_REPO") && !some("TEHAS_IMPROVE_REPO") {
        return Err(format!(
            "TEHAS_IMPROVE_REPO is not set and TEHAS_IMPROVE_RUKAS_REPO is (see {file})"
        ));
    }
    if some("TEHAS_IMPROVE_RUKAS_REPO") && on("TEHAS_IMPROVE_RUKAS_SKIP") {
        return Err(format!(
            "TEHAS_IMPROVE_RUKAS_SKIP is 1 and TEHAS_IMPROVE_RUKAS_REPO is set: findings about pi-rukas are either filed there or not at all (see {file})"
        ));
    }
    // A model's findings go to a repository of their own, where a person
    // triages them into tickets of the product: filing them in the product is
    // the one setting that cannot be allowed, for the loop and for the review.
    let repo = vars.text("TEHAS_REPO");
    for name in [
        "TEHAS_IMPROVE_REPO",
        "TEHAS_IMPROVE_RUKAS_REPO",
        "TEHAS_REVIEW_REPO",
    ] {
        if !repo.is_empty() && vars.text(name).to_lowercase() == repo.to_lowercase() {
            return Err(format!(
                "the improvement loop files in {repo}, the product repository: a model writes its findings from what cycles saw, so they go to a repository of their own where a person triages them (see {file})"
            ));
        }
    }
    let millwright = vars.text("TEHAS_MILLWRIGHT_LOGIN");
    let bare = |login: &str| login.strip_suffix("[bot]").unwrap_or(login).to_string();
    let authorising = format!(
        " {} {} ",
        vars.text("TEHAS_ALLOWED_LABELERS"),
        bare(&vars.text("TEHAS_BOT_LOGIN"))
    );
    if !millwright.is_empty() && authorising.contains(&format!(" {} ", bare(&millwright))) {
        return Err(format!(
            "TEHAS_MILLWRIGHT_LOGIN ({millwright}) is in TEHAS_ALLOWED_LABELERS or is TEHAS_BOT_LOGIN: the millwright is an account of its own and works within its own grant, never as the bot or a person (see {file})"
        ));
    }
    if on("TEHAS_WORK_DESCRIBE") && !on("TEHAS_WORK_IMAGES") {
        return Err(format!(
            "TEHAS_WORK_DESCRIBE is 1 and TEHAS_WORK_IMAGES is not: there are no images to describe (see {file})"
        ));
    }
    let taken = on("TEHAS_WORK_IMAGES") || on("TEHAS_WORK_FILES");
    if on("TEHAS_WORK_COMMENTS") && !taken {
        return Err(format!(
            "TEHAS_WORK_COMMENTS is 1 and neither TEHAS_WORK_IMAGES nor TEHAS_WORK_FILES is: nothing is taken from comments (see {file})"
        ));
    }
    if on("TEHAS_WORK_ZIPS") && !taken {
        return Err(format!(
            "TEHAS_WORK_ZIPS is 1 and neither TEHAS_WORK_IMAGES nor TEHAS_WORK_FILES is: an unpacked file goes to the directory of one of them (see {file})"
        ));
    }
    // One axis of parallelism: several cycles of one workstream, or one cycle
    // of several. Both would multiply the agents and the gates on the same
    // CPUs.
    let (streams, tickets) = (
        number(vars, "TEHAS_CONCURRENCY"),
        number(vars, "TEHAS_TICKET_CONCURRENCY"),
    );
    if streams > 1 && tickets > 1 {
        return Err(format!(
            "TEHAS_CONCURRENCY ({streams}) and TEHAS_TICKET_CONCURRENCY ({tickets}) are both above 1: choose one, several issues at a time with one workstream each, or one issue at a time with TEHAS_TICKET_CONCURRENCY=1 (see {file})"
        ));
    }
    // The cycle's time bounds in the order they must sit in: the gate under
    // the stall that must not fire while a gate prints nothing, one child at
    // or under the whole cycle, and the cycle under what the unit outlives.
    // Each is refused, not clamped.
    let (stall, verify, child, work, plan) = (
        number(vars, "TEHAS_WORK_STALL"),
        number(vars, "TEHAS_VERIFY_TIMEOUT"),
        number(vars, "TEHAS_CHILD_TIMEOUT"),
        number(vars, "TEHAS_WORK_TIMEOUT"),
        number(vars, "TEHAS_PLAN_TIMEOUT"),
    );
    if stall <= verify {
        return Err(format!(
            "TEHAS_WORK_STALL ({stall}) is not longer than TEHAS_VERIFY_TIMEOUT ({verify}): a gate that prints nothing would be stopped as stalled (see {file})"
        ));
    }
    if child > work {
        return Err(format!(
            "TEHAS_CHILD_TIMEOUT ({child}) is more than TEHAS_WORK_TIMEOUT ({work}): a child that outlives its cycle is one tehas stopped too late (see {file})"
        ));
    }
    let max = rules::WORK_TIMEOUT_MAX;
    if work > max {
        return Err(format!(
            "TEHAS_WORK_TIMEOUT ({work}) is more than {max}: tehas-cycle@.service stops a cycle after 8 hours, and the stop and the report need the hour after the limit (see {file})"
        ));
    }
    let max = rules::PLAN_TIMEOUT_MAX;
    if plan > max {
        return Err(format!(
            "TEHAS_PLAN_TIMEOUT ({plan}) is more than {max}: a planning turn may take at most two hours, and while one runs no other planning turn starts (see {file})"
        ));
    }
    if on("TEHAS_PREVIEWS") && !some("TEHAS_DOMAIN") {
        return Err(format!(
            "TEHAS_PREVIEWS is 1 and TEHAS_DOMAIN is not set: a preview has no address without it (see {file})"
        ));
    }
    Ok(())
}

/// What only a good load sets: the three sizes in MiB, for the df figures of
/// dispatch, clean and preview, and the dashboard's lab host name, which
/// follows the edge's unless the instance named one.
fn last(vars: &mut Vars) {
    for name in [
        "TEHAS_PREVIEW_MIN_FREE",
        "TEHAS_CYCLE_MIN_FREE",
        "TEHAS_CLEAN_MIN_FREE",
    ] {
        let mb = rules::mem_mb(&vars.text(name)).unwrap_or_default();
        vars.set(&format!("{name}_MB"), mb);
    }
    if has(vars, "TEHAS_DOMAIN") && !has(vars, "TEHAS_PROTOLAB_URL") {
        let lab = format!("https://lab.{}", vars.text("TEHAS_DOMAIN"));
        vars.set("TEHAS_PROTOLAB_URL", lab);
    }
}

/// tehas_load. Err is the line for standard error, without its `tehas: `.
/// `vars` holds what the load had done either way.
pub fn load(vars: &mut Vars, host: &mut dyn Host, uid: &str) -> Result<(), String> {
    or_default(vars, "TEHAS_HOME", format!("{}/tehas", vars.text("HOME")));
    let dir = instance_dir(vars, host);
    let file = format!("{dir}/instance.env");
    let Some(text) = host.read(&file) else {
        return Err(format!(
            "no instance.env in {dir} (run tehas setup, or copy instances/example and fill it in)"
        ));
    };
    sweep(vars);
    vars.set("TEHAS_INSTANCE_DIR", dir);
    scan(&text, &file)?;
    let sourced = host.source(&file, vars);
    // No list at all: the file ended the shell before it was asked for one.
    if !sourced.vars.is_empty() {
        take(vars, sourced.vars);
    }
    if !sourced.ok {
        return Err(format!("{file} could not be read as shell"));
    }
    role_models(vars, &file)?;
    retired_names(vars, &file)?;
    defaults(vars, host, uid);
    check_values(vars, &file)?;
    together(vars, &file)?;
    last(vars);
    Ok(())
}

/// Every setting a command could require that holds no value of its rule, with
/// why, for tehas_require to read. An empty one is tehas_require's to name.
pub fn bad(vars: &Vars) -> Vec<(String, String)> {
    let [home, instance, work_repo] = trees_of(vars);
    let trees = Trees {
        home: &home,
        instance: &instance,
        work_repo: &work_repo,
    };
    let roles = ["PI_ENSEMBLE_MODEL_", "PI_ENSEMBLE_PROVIDER_"]
        .iter()
        .flat_map(|prefix| vars.prefixed(prefix));
    TABLE
        .iter()
        .map(|s| s.name.to_string())
        .chain(roles)
        .filter(|name| has(vars, name))
        .filter_map(|name| {
            let why = rules::check(&name, &vars.text(&name), &trees).err()?;
            Some((name, why))
        })
        .collect()
}

/// `value` as one word of a shell's input.
fn quote(out: &mut Vec<u8>, value: &[u8]) {
    out.push(b'\'');
    for byte in value {
        if *byte == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(*byte);
        }
    }
    out.push(b'\'');
}

/// The lines that take a shell from `start` to `vars`, for it to eval: what
/// the load dropped, what it set, and `_tehas_bad`.
pub fn script(start: &Vars, vars: &Vars) -> Vec<u8> {
    let mut out = Vec::new();
    for name in start.map.keys() {
        if !vars.map.contains_key(name) && is_name(name) {
            out.extend_from_slice(b"unset -v ");
            out.extend_from_slice(name);
            out.push(b'\n');
        }
    }
    for name in &vars.set {
        if let (true, Some(value)) = (is_name(name), vars.map.get(name)) {
            out.extend_from_slice(b"export ");
            out.extend_from_slice(name);
            out.push(b'=');
            quote(&mut out, value);
            out.push(b'\n');
        }
    }
    out.extend_from_slice(b"declare -gA _tehas_bad\n_tehas_bad=(");
    for (name, why) in bad(vars) {
        out.extend_from_slice(format!("[{name}]=").as_bytes());
        quote(&mut out, why.as_bytes());
        out.push(b' ');
    }
    out.extend_from_slice(b")\n_tehas_loaded=1\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "/i/instance.env";
    const MINIMAL: [&str; 5] = [
        "TEHAS_REPO=acme/widget",
        "TEHAS_CONTROL_ISSUE=1",
        "TEHAS_ALLOWED_LABELERS=alice",
        "PI_ENSEMBLE_SUBAGENT_MODEL=test-model",
        "PI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider",
    ];

    /// A host whose instance.env is the lines given, each KEY=value, which
    /// the source adds to the environment it is handed, as `set -a` would.
    struct Fake {
        lines: Vec<String>,
        text: Option<Vec<u8>>,
        pointer: Option<Vec<u8>>,
        cpus: i64,
        cpus_asked: u32,
        ok: bool,
        ended: bool,
    }

    impl Fake {
        fn new(lines: &[&str]) -> Fake {
            let lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            Fake {
                text: Some(lines.join("\n").into_bytes()),
                lines,
                pointer: None,
                cpus: 8,
                cpus_asked: 0,
                ok: true,
                ended: false,
            }
        }
        fn with(extra: &[&str]) -> Fake {
            Fake::new(&[&MINIMAL[..], extra].concat())
        }
    }

    impl Host for Fake {
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            if path.ends_with("instance-dir") {
                return self.pointer.clone();
            }
            self.text.clone()
        }
        fn source(&mut self, _file: &str, env: &Vars) -> Sourced {
            let mut vars: BTreeMap<Vec<u8>, Vec<u8>> =
                env.iter().map(|(k, v)| (k.to_vec(), v.to_vec())).collect();
            for line in &self.lines {
                let (k, v) = line.split_once('=').unwrap();
                vars.insert(k.as_bytes().to_vec(), v.as_bytes().to_vec());
            }
            vars.insert(b"_".to_vec(), b"/usr/bin/env".to_vec());
            vars.insert(b"SHLVL".to_vec(), b"9".to_vec());
            if self.ended {
                vars.clear();
            }
            Sourced {
                vars: vars.into_iter().collect(),
                ok: self.ok,
            }
        }
        fn cpus(&mut self) -> i64 {
            self.cpus_asked += 1;
            self.cpus
        }
    }

    fn start(pairs: &[(&str, &str)]) -> Vars {
        let mut all = vec![
            ("HOME", "/h"),
            ("TEHAS_INSTANCE_DIR", "/i"),
            ("PATH", "/bin"),
        ];
        all.extend_from_slice(pairs);
        Vars::new(
            all.iter()
                .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec())),
        )
    }

    fn run(host: &mut Fake) -> (Vars, Result<(), String>) {
        let mut vars = start(&[]);
        let got = load(&mut vars, host, "1000");
        (vars, got)
    }
    fn loads(extra: &[&str]) -> Vars {
        let (vars, got) = run(&mut Fake::with(extra));
        assert_eq!(got, Ok(()));
        vars
    }
    fn refused(extra: &[&str]) -> String {
        run(&mut Fake::with(extra)).1.unwrap_err()
    }

    #[test]
    fn a_minimal_instance_loads_with_every_default() {
        let vars = loads(&[]);
        for (name, value) in [
            ("TEHAS_HOME", "/h/tehas"),
            ("TEHAS_WORK_REPO", "/h/tehas/repos/widget"),
            ("TEHAS_WORK_DIR", "/h/tehas/work/widget"),
            ("TEHAS_CONCURRENCY", "1"),
            ("TEHAS_TICKET_CONCURRENCY", "7"),
            ("TEHAS_MAX_AWAITING", "9"),
            ("TEHAS_PLAN_TIMEOUT", "7200"),
            ("TEHAS_CHILD_TIMEOUT", "7200"),
            ("PI_ENSEMBLE_IMAGE", "tehas/pi-rukas:uid1000"),
            ("PI_ENSEMBLE_MAX_WORKSTREAMS", "1"),
            ("PI_ENSEMBLE_VERIFY_TIMEOUT_MS", "1800000"),
            ("PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS", "1800000"),
            ("PI_ENSEMBLE_NO_DOCKER_SOCKET", "1"),
            ("PI_ENSEMBLE_NO_SSH", "1"),
            ("TEHAS_PREVIEW_MIN_FREE_MB", "5120"),
            ("TEHAS_CYCLE_MIN_FREE_MB", "10240"),
            ("TEHAS_CLEAN_MIN_FREE_MB", "30720"),
        ] {
            assert_eq!(vars.text(name), value, "{name}");
        }
        assert!(vars.is_set("TEHAS_SIZE_MAX_FILES"));
        assert!(!vars.is_set("TEHAS_PROTOLAB_URL"));
    }

    #[test]
    fn an_empty_line_takes_the_default() {
        let vars = loads(&[
            "TEHAS_PLAN_TIMEOUT=",
            "TEHAS_CHILD_TIMEOUT=",
            "TEHAS_THEME=",
        ]);
        assert_eq!(vars.text("TEHAS_PLAN_TIMEOUT"), "7200");
        assert_eq!(vars.text("TEHAS_CHILD_TIMEOUT"), "7200");
        assert_eq!(vars.text("TEHAS_THEME"), "pixel");
    }

    #[test]
    fn a_ceiling_the_instance_names_asks_no_host() {
        let mut host = Fake::with(&["TEHAS_TICKET_CONCURRENCY=4"]);
        let (vars, got) = run(&mut host);
        assert_eq!(got, Ok(()));
        assert_eq!(vars.text("TEHAS_MAX_AWAITING"), "6");
        assert_eq!(host.cpus_asked, 0);
    }

    #[test]
    fn a_host_of_one_cpu_runs_one_cycle() {
        for cpus in [1, 0, -3] {
            let mut host = Fake::with(&[]);
            host.cpus = cpus;
            let (vars, _) = run(&mut host);
            assert_eq!(vars.text("TEHAS_TICKET_CONCURRENCY"), "1");
        }
    }

    #[test]
    fn a_concurrency_that_is_no_number_leaves_the_floor_of_three() {
        let mut host = Fake::with(&["TEHAS_TICKET_CONCURRENCY=two"]);
        let (vars, got) = run(&mut host);
        assert!(got
            .unwrap_err()
            .starts_with("TEHAS_TICKET_CONCURRENCY is not valid"));
        assert_eq!(vars.text("TEHAS_MAX_AWAITING"), "3");
    }

    #[test]
    fn the_environment_gives_only_the_kept_names() {
        let mut vars = start(&[
            ("TEHAS_REPO", "other/thing"),
            ("TEHAS_INDEXING", "1"),
            ("PI_ENSEMBLE_DEBUG", "1"),
            ("TEHAS_WORK_CMD", "fake-work"),
            ("TEHAS_MODEL_KEY_FLEET", "t0"),
            ("PI_RUKAS_FOLLOW", "release"),
        ]);
        let before = vars.clone();
        assert_eq!(load(&mut vars, &mut Fake::with(&[]), "1"), Ok(()));
        assert_eq!(vars.text("TEHAS_REPO"), "acme/widget");
        assert_eq!(vars.text("TEHAS_INDEXING"), "0");
        assert!(!vars.is_set("PI_ENSEMBLE_DEBUG"));
        assert_eq!(vars.text("TEHAS_WORK_CMD"), "fake-work");
        assert_eq!(vars.text("TEHAS_MODEL_KEY_FLEET"), "t0");
        assert_eq!(vars.text("PI_RUKAS_FOLLOW"), "release");
        let text = String::from_utf8(script(&before, &vars)).unwrap();
        assert!(text.contains("unset -v PI_ENSEMBLE_DEBUG\n"), "{text}");
        assert!(!text.contains("TEHAS_WORK_CMD"), "{text}");
        assert!(!text.contains("SHLVL"), "{text}");
        assert!(!text.contains("export _="), "{text}");
    }

    #[test]
    fn no_instance_file_is_refused_before_the_sweep() {
        let mut host = Fake::new(&[]);
        host.text = None;
        let mut vars = start(&[("TEHAS_REPO", "other/thing")]);
        let got = load(&mut vars, &mut host, "1");
        assert_eq!(
            got.unwrap_err(),
            "no instance.env in /i (run tehas setup, or copy instances/example and fill it in)"
        );
        assert_eq!(vars.text("TEHAS_REPO"), "other/thing");
        assert_eq!(vars.text("TEHAS_HOME"), "/h/tehas");
    }

    #[test]
    fn the_pointer_file_names_the_instance_or_the_config_is_it() {
        let mut host = Fake::with(&[]);
        host.pointer = Some(b"  /elsewhere  \nmore\n".to_vec());
        let home_only = || Vars::new([(b"HOME".to_vec(), b"/h".to_vec())]);
        let mut vars = home_only();
        assert_eq!(load(&mut vars, &mut host, "1"), Ok(()));
        assert_eq!(vars.text("TEHAS_INSTANCE_DIR"), "/elsewhere");
        host.pointer = Some(b"\n".to_vec());
        let mut vars = home_only();
        assert_eq!(load(&mut vars, &mut host, "1"), Ok(()));
        assert_eq!(vars.text("TEHAS_INSTANCE_DIR"), "/h/.config/tehas");
    }

    #[test]
    fn a_token_in_the_instance_is_refused_and_never_named() {
        for line in [
            "TEHAS_MODEL_KEY_X=sekret",
            "  export   TEHAS_MODEL_KEY_X=sekret",
            "\texport TEHAS_MODEL_KEY_X",
        ] {
            let got = scan(line.as_bytes(), FILE).unwrap_err();
            assert_eq!(
                got,
                "/i/instance.env sets a TEHAS_MODEL_KEY_ token: put it in ~/.config/tehas/env"
            );
        }
        for line in [
            "TEHAS_MCP_KEYS=sekret",
            "export TEHAS_MCP_DASHBOARD_KEYS=sekret",
        ] {
            let got = scan(line.as_bytes(), FILE).unwrap_err();
            assert!(got.ends_with("sets MCP keys: put TEHAS_MCP_KEYS and TEHAS_MCP_DASHBOARD_KEYS in ~/.config/tehas/env"));
            assert!(!got.contains("sekret"));
        }
        assert_eq!(
            scan(b"# TEHAS_MODEL_KEY_X=1\nTEHAS_MCP_KEYS\n", FILE),
            Ok(())
        );
        assert_eq!(scan(b"exportTEHAS_MCP_KEYS=1\n", FILE), Ok(()));
    }

    #[test]
    fn a_token_is_not_given_to_the_source() {
        struct NoSource;
        impl Host for NoSource {
            fn read(&self, _: &str) -> Option<Vec<u8>> {
                Some(b"TEHAS_MODEL_KEY_X=1\n".to_vec())
            }
            fn source(&mut self, _: &str, _: &Vars) -> Sourced {
                panic!("the file was run")
            }
            fn cpus(&mut self) -> i64 {
                1
            }
        }
        let mut vars = start(&[]);
        assert!(load(&mut vars, &mut NoSource, "1").is_err());
    }

    #[test]
    fn a_file_that_fails_as_shell_keeps_what_it_set() {
        let mut host = Fake::with(&[]);
        host.ok = false;
        let (vars, got) = run(&mut host);
        assert_eq!(
            got.unwrap_err(),
            "/i/instance.env could not be read as shell"
        );
        assert_eq!(vars.text("TEHAS_REPO"), "acme/widget");
        assert!(!vars.is_set("TEHAS_THEME"));
    }

    #[test]
    fn a_file_that_ended_its_shell_is_refused_and_takes_nothing_away() {
        let mut host = Fake::with(&[]);
        host.ok = false;
        host.ended = true;
        let (vars, got) = run(&mut host);
        assert!(got.is_err());
        assert_eq!(vars.text("PATH"), "/bin");
    }

    #[test]
    fn what_a_file_unsets_goes() {
        let mut vars = start(&[("EDITOR", "vi")]);
        take(&mut vars, vec![(b"PATH".to_vec(), b"/x".to_vec())]);
        assert!(!vars.is_set("EDITOR"));
        assert_eq!(vars.text("PATH"), "/x");
    }

    #[test]
    fn a_role_model_gets_the_subagent_provider_unless_it_names_its_own() {
        let vars = loads(&[
            "PI_ENSEMBLE_MODEL_OPS=small",
            "PI_ENSEMBLE_MODEL_DEVELOPER=big",
            "PI_ENSEMBLE_PROVIDER_DEVELOPER=other",
        ]);
        assert_eq!(vars.text("PI_ENSEMBLE_PROVIDER_OPS"), "test-provider");
        assert_eq!(vars.text("PI_ENSEMBLE_PROVIDER_DEVELOPER"), "other");
    }

    #[test]
    fn an_empty_role_model_is_not_set() {
        let vars = loads(&["PI_ENSEMBLE_MODEL_DEVELOPER="]);
        assert!(!vars.is_set("PI_ENSEMBLE_MODEL_DEVELOPER"));
    }

    #[test]
    fn a_role_pi_rukas_does_not_know_is_refused() {
        let roles = "DEVELOPER OPS EXPLORE ADVERSARIAL_DEVELOPER CODE_REVIEW_SPECIALIST";
        for name in [
            "PI_ENSEMBLE_MODEL_REVIEWER",
            "PI_ENSEMBLE_MODEL_",
            "PI_ENSEMBLE_PROVIDER_",
            "PI_ENSEMBLE_MODEL_developer",
        ] {
            assert_eq!(
                refused(&[&format!("{name}=m")]),
                format!("{name} names no role pi-rukas gives a model to; the roles are {roles}  (see {FILE})")
            );
        }
    }

    #[test]
    fn a_provider_needs_its_model() {
        assert_eq!(
            refused(&["PI_ENSEMBLE_PROVIDER_OPS=p"]),
            format!("PI_ENSEMBLE_PROVIDER_OPS is set and PI_ENSEMBLE_MODEL_OPS is not: pi-rukas ignores a provider without its model (see {FILE})")
        );
    }

    #[test]
    fn a_role_model_of_two_words_is_refused() {
        assert_eq!(
            refused(&["PI_ENSEMBLE_MODEL_EXPLORE=two words"]),
            format!("PI_ENSEMBLE_MODEL_EXPLORE is not valid: expected one word (see {FILE})")
        );
    }

    #[test]
    fn a_retired_name_is_refused_or_taken_as_what_it_asked_for() {
        assert_eq!(
            refused(&["TEHAS_FIXER_LOGIN="]),
            format!("TEHAS_FIXER_LOGIN is no longer read: rename it TEHAS_MILLWRIGHT_LOGIN (see {FILE})")
        );
        assert_eq!(
            refused(&["TEHAS_WORK_MEMORY=8g", "TEHAS_CLONES=1"]),
            format!(
                "TEHAS_CLONES is set and this version of tehas has no database clones (see {FILE})"
            )
        );
        assert_eq!(
            refused(&["TEHAS_DISPATCHER=bash"]),
            format!("TEHAS_DISPATCHER is set and this version of tehas has no bash poll: every poll is tehas-dispatch, so take the line out (see {FILE})")
        );
        for line in [
            "TEHAS_CLONES=",
            "TEHAS_CLONES=0",
            "TEHAS_DISPATCHER=",
            "TEHAS_DISPATCHER=rust",
        ] {
            let vars = loads(&[line]);
            assert!(!vars.is_set(line.split('=').next().unwrap()));
        }
    }

    #[test]
    fn the_first_setting_of_the_table_is_the_one_told() {
        assert!(refused(&["TEHAS_THEME=neon", "TEHAS_TOP=9"]).starts_with("TEHAS_THEME"));
        assert!(refused(&["TEHAS_TOP=9", "TEHAS_BRANCH=a b"]).starts_with("TEHAS_TOP"));
    }

    #[test]
    fn what_a_load_requires_later_does_not_refuse_it() {
        let vars = loads(&[
            "TEHAS_REPO=widget",
            "TEHAS_ALLOWED_LABELERS=alice ",
            "TEHAS_CONTAINER_MEMORY=1m",
            "TEHAS_REVIEW_REPO=acme",
        ]);
        let found: Vec<(String, String)> = bad(&vars);
        let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "TEHAS_REPO",
                "TEHAS_ALLOWED_LABELERS",
                "TEHAS_CONTAINER_MEMORY",
                "TEHAS_REVIEW_REPO"
            ]
        );
        assert_eq!(found[0].1, "expected owner/name");
    }

    #[test]
    fn an_optional_setting_is_checked_only_when_it_is_there() {
        loads(&[
            "TEHAS_BRANCH=",
            "TEHAS_BUILD_TIMEOUT=",
            "TEHAS_UPDATE_FROM=",
        ]);
        assert!(refused(&["TEHAS_BUILD_TIMEOUT=0"]).starts_with("TEHAS_BUILD_TIMEOUT is not valid"));
        assert!(refused(&["PI_RUKAS_FOLLOW="]).starts_with("PI_RUKAS_FOLLOW is not valid"));
    }

    #[test]
    fn the_settings_that_only_mean_something_together() {
        for (lines, want) in [
            (
                &["TEHAS_RUKAS_WORKAROUNDS=TEHAS_X@1.2"][..],
                "TEHAS_RUKAS_WORKAROUNDS names TEHAS_X, which is not set: remove its entry",
            ),
            (
                &["TEHAS_RUKAS_WORKAROUNDS=TEHAS_RUKAS_WORKAROUNDS@1.2"][..],
                "TEHAS_RUKAS_WORKAROUNDS names TEHAS_RUKAS_WORKAROUNDS, which is not set",
            ),
            (
                &["TEHAS_GITHUB_APP_ID=1"][..],
                "TEHAS_GITHUB_APP_ID, TEHAS_GITHUB_APP_INSTALLATION and TEHAS_BOT_LOGIN are set together or not at all",
            ),
            (
                &["TEHAS_ANALYTICS_SRC=https://a.example/x.js"][..],
                "TEHAS_ANALYTICS_SITE is not set and TEHAS_ANALYTICS_SRC is",
            ),
            (
                &["TEHAS_IMPROVE_RUKAS_REPO=a/b"][..],
                "TEHAS_IMPROVE_REPO is not set and TEHAS_IMPROVE_RUKAS_REPO is",
            ),
            (
                &["TEHAS_IMPROVE_REPO=a/b", "TEHAS_IMPROVE_RUKAS_REPO=a/c", "TEHAS_IMPROVE_RUKAS_SKIP=1"][..],
                "TEHAS_IMPROVE_RUKAS_SKIP is 1 and TEHAS_IMPROVE_RUKAS_REPO is set",
            ),
            (
                &["TEHAS_REVIEW_REPO=Acme/Widget"][..],
                "the improvement loop files in acme/widget, the product repository",
            ),
            (
                &["TEHAS_MILLWRIGHT_LOGIN=alice[bot]"][..],
                "TEHAS_MILLWRIGHT_LOGIN (alice[bot]) is in TEHAS_ALLOWED_LABELERS or is TEHAS_BOT_LOGIN",
            ),
            (
                &["TEHAS_WORK_DESCRIBE=1"][..],
                "TEHAS_WORK_DESCRIBE is 1 and TEHAS_WORK_IMAGES is not",
            ),
            (
                &["TEHAS_WORK_COMMENTS=1"][..],
                "TEHAS_WORK_COMMENTS is 1 and neither TEHAS_WORK_IMAGES nor TEHAS_WORK_FILES is",
            ),
            (
                &["TEHAS_WORK_ZIPS=1"][..],
                "TEHAS_WORK_ZIPS is 1 and neither TEHAS_WORK_IMAGES nor TEHAS_WORK_FILES is",
            ),
            (
                &["TEHAS_CONCURRENCY=2", "TEHAS_TICKET_CONCURRENCY=3"][..],
                "TEHAS_CONCURRENCY (2) and TEHAS_TICKET_CONCURRENCY (3) are both above 1",
            ),
            (
                &["TEHAS_WORK_STALL=1800"][..],
                "TEHAS_WORK_STALL (1800) is not longer than TEHAS_VERIFY_TIMEOUT (1800)",
            ),
            (
                &["TEHAS_WORK_TIMEOUT=3600", "TEHAS_CHILD_TIMEOUT=3601"][..],
                "TEHAS_CHILD_TIMEOUT (3601) is more than TEHAS_WORK_TIMEOUT (3600)",
            ),
            (
                &["TEHAS_WORK_TIMEOUT=25201"][..],
                "TEHAS_WORK_TIMEOUT (25201) is more than 25200",
            ),
            (
                &["TEHAS_PLAN_TIMEOUT=7201"][..],
                "TEHAS_PLAN_TIMEOUT (7201) is more than 7200",
            ),
            (
                &["TEHAS_PREVIEWS=1"][..],
                "TEHAS_PREVIEWS is 1 and TEHAS_DOMAIN is not set",
            ),
        ] {
            let got = refused(lines);
            assert!(got.starts_with(want), "{got}");
            assert!(got.ends_with(&format!("(see {FILE})")), "{got}");
        }
    }

    #[test]
    fn a_workaround_for_a_setting_the_instance_has_loads() {
        loads(&[
            "PI_ENSEMBLE_COMMENT_RETENTION=0",
            "TEHAS_RUKAS_WORKAROUNDS=PI_ENSEMBLE_COMMENT_RETENTION@0.12.80",
        ]);
    }

    #[test]
    fn a_millwright_with_a_name_like_the_bots_is_its_own_account() {
        loads(&[
            "TEHAS_GITHUB_APP_ID=1",
            "TEHAS_GITHUB_APP_INSTALLATION=2",
            "TEHAS_BOT_LOGIN=acme-app[bot]",
            "TEHAS_MILLWRIGHT_LOGIN=acme-app-mill[bot]",
        ]);
    }

    #[test]
    fn a_domain_gives_the_lab_its_address_and_a_written_one_is_kept() {
        let vars = loads(&["TEHAS_DOMAIN=example.org"]);
        assert_eq!(vars.text("TEHAS_PROTOLAB_URL"), "https://lab.example.org");
        let vars = loads(&[
            "TEHAS_DOMAIN=example.org",
            "TEHAS_PROTOLAB_URL=https://p.example",
        ]);
        assert_eq!(vars.text("TEHAS_PROTOLAB_URL"), "https://p.example");
    }

    #[test]
    fn a_refused_load_keeps_its_defaults_and_not_what_comes_last() {
        let (vars, got) = run(&mut Fake::with(&[
            "TEHAS_PREVIEWS=1",
            "TEHAS_GITHUB_APP_ID=7",
        ]));
        assert!(got.is_err());
        assert_eq!(vars.text("TEHAS_GITHUB_APP_ID"), "7");
        assert_eq!(vars.text("TEHAS_THEME"), "pixel");
        assert!(!vars.is_set("TEHAS_CYCLE_MIN_FREE_MB"));
    }

    #[test]
    fn the_script_quotes_a_value_as_one_word() {
        let before = start(&[]);
        let mut vars = before.clone();
        vars.set("TEHAS_X", "it's a $HOME\n`line`");
        vars.set("TEHAS_Y", vec![0xff, b'a']);
        vars.unset(b"PATH");
        let text = script(&before, &vars);
        let text = String::from_utf8_lossy(&text);
        assert!(
            text.contains("export TEHAS_X='it'\\''s a $HOME\n`line`'\n"),
            "{text}"
        );
        assert!(text.contains("unset -v PATH\n"), "{text}");
        assert!(script(&before, &vars).windows(2).any(|w| w == [0xff, b'a']));
    }

    #[test]
    fn the_script_says_nothing_of_a_name_no_shell_can_hold() {
        let before = Vars::new([
            (b"BASH_FUNC_x%%".to_vec(), b"() { :; }".to_vec()),
            (b"a.b".to_vec(), b"1".to_vec()),
        ]);
        let mut vars = before.clone();
        vars.unset(b"a.b");
        vars.put(b"1x".to_vec(), b"1".to_vec());
        let text = script(&before, &vars);
        assert_eq!(
            String::from_utf8_lossy(&text),
            "declare -gA _tehas_bad\n_tehas_bad=()\n_tehas_loaded=1\n"
        );
    }

    #[test]
    fn the_map_of_what_is_bad_is_quoted() {
        let mut vars = start(&[]);
        vars.set("TEHAS_REPO", "widget");
        let text = script(&start(&[]), &vars);
        let text = String::from_utf8_lossy(&text);
        assert!(
            text.contains("_tehas_bad=([TEHAS_REPO]='expected owner/name' )\n"),
            "{text}"
        );
    }
}
