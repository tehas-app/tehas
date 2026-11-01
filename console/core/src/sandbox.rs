//! What a sandbox container is given, in one place so that a cycle, a planning
//! turn, a manual session and a review cannot drift apart. AGENTS.md rule 4:
//! a change here is a security change, and test/fixtures/argv pins the result
//! word by word.
//!
//! A kind is a closed list and takes no free-form variable or mount. Not given
//! to any container, and not a setting: the Docker socket, SSH keys,
//! ~/.config/tehas, the product checkout, the rest of the host environment. Of
//! ~/.config/tehas/env a container is given the bot's token and the model
//! endpoints' tokens that models.json names, by name, never by value.
//!
//! Every list here is docker options, one word each.

use crate::models::{self, ModelFile};
use crate::slice::{self, Load, Reading, Usable};
use std::env;
use std::fs;

/// Where a container has its own copy of the code index. A path inside the
/// container, not a setting. The host directory behind it belongs to one
/// container and goes with it.
pub const CACHE_DIR: &str = "/home/vscode/.tehas-cache";

/// Where a manual session has its cargo build output. A path off the image's
/// home would build into the container's layer, which the session throws away.
/// A cycle and a planning turn get no CARGO_TARGET_DIR: cargo builds in the
/// workspace, the read-write mount, so on the host's disk.
pub const TARGET_DIR: &str = "/home/vscode/.tehas-target";

// lib/sandbox.sh names the same two for tehas setup's probe of the host's git:
// tehas_git_credential_helper_key and tehas_sandbox_helper_command.
const HELPER_KEY: &str = "credential.https://github.com.helper";
const HELPER: &str = "!gh auth git-credential";

/// The containers that run an agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cycle,
    Turn,
    Session,
    Review,
}

impl Kind {
    pub fn parse(word: &str) -> Option<Kind> {
        match word {
            "cycle" => Some(Kind::Cycle),
            "turn" => Some(Kind::Turn),
            "session" => Some(Kind::Session),
            "review" => Some(Kind::Review),
            _ => None,
        }
    }

    // A run that writes nothing to the tracker is given no token to write
    // with: its name is not among the variables at all, and the GitHub App's
    // token directory is not mounted. This is the one place that decides it.
    fn token(self) -> bool {
        self != Kind::Review
    }
}

fn pair(flag: &str, words: Vec<String>) -> Vec<String> {
    words
        .into_iter()
        .flat_map(|w| [flag.to_string(), w])
        .collect()
}

/// Whose name a container's git carries, and the tree it works in.
pub struct Identity<'a> {
    pub tree: &'a str,
    pub name: &'a str,
    pub email: &'a str,
}

/// The -e options. The same variables as the pi-rukas wrapper sets, without the
/// host's whole environment: the bot token by name, the PI_ENSEMBLE_* settings
/// in `ensemble`, git's configuration, and in `tokens` the model endpoints'
/// tokens by name: Pi reads "$TEHAS_MODEL_KEY_<NAME>" from its environment.
///
/// git inside the sandbox has no configuration of its own: no credential helper
/// and no identity. Without the helper the driver commits but cannot push, and
/// a cycle parks after all the work is done. git is handed the bot's token
/// through gh (which reads GH_TOKEN) and an identity, as environment-level
/// config: it outranks any file, reaches every agent process, and writes
/// nothing into the tree.
pub fn env_options(
    kind: Kind,
    who: &Identity,
    ensemble: &[String],
    tokens: &[String],
) -> Vec<String> {
    let mut vars = Vec::new();
    if kind.token() {
        vars.push("GH_TOKEN".to_string());
    }
    vars.extend([
        "PI_ENSEMBLE_SANDBOX_MODE=1".to_string(),
        format!("PI_ENSEMBLE_WORKSPACE_ROOT={}", who.tree),
        format!("CBM_CACHE_DIR={CACHE_DIR}"),
        "GIT_CONFIG_COUNT=4".to_string(),
        format!("GIT_CONFIG_KEY_0={HELPER_KEY}"),
        "GIT_CONFIG_VALUE_0=".to_string(),
        format!("GIT_CONFIG_KEY_1={HELPER_KEY}"),
        format!("GIT_CONFIG_VALUE_1={HELPER}"),
        "GIT_CONFIG_KEY_2=user.name".to_string(),
        format!("GIT_CONFIG_VALUE_2={}", who.name),
        "GIT_CONFIG_KEY_3=user.email".to_string(),
        format!("GIT_CONFIG_VALUE_3={}", who.email),
        "GIT_TERMINAL_PROMPT=0".to_string(),
    ]);
    vars.extend_from_slice(ensemble);
    vars.extend_from_slice(tokens);
    if kind == Kind::Session {
        vars.push(format!("CARGO_TARGET_DIR={TARGET_DIR}"));
    }
    pair("-e", vars)
}

/// The PI_ENSEMBLE_* the host has set, in the order of its environment. Not the
/// settings of pi-rukas's own docker wrapper, which no container runs: the
/// image, the aliases (given as --add-host) and the socket and SSH switches.
/// The runner does not list them as inputs.
pub fn ensemble() -> Vec<String> {
    env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(|name| name.starts_with("PI_ENSEMBLE_"))
        .filter(|name| {
            !matches!(
                name.as_str(),
                "PI_ENSEMBLE_IMAGE"
                    | "PI_ENSEMBLE_HOST_ALIASES"
                    | "PI_ENSEMBLE_NO_DOCKER_SOCKET"
                    | "PI_ENSEMBLE_NO_SSH"
            )
        })
        .collect()
}

/// The tokens models.json names that are set.
pub fn tokens() -> Vec<String> {
    let names = models::key_names(&ModelFile::read(&models::host_path()).json);
    names.into_iter().filter(|name| set(name)).collect()
}

fn set(name: &str) -> bool {
    env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// The directories of one container. <tree> is the path inside the container
/// and <host> the directory mounted there. <state> is a host directory of this
/// container's own: its copy of the code index, its memory and its sessions go
/// under it. <runs> is the host directory of its transcripts. All are named by
/// the caller: there is no default that lets one container's writes be
/// another's input.
pub struct Paths<'a> {
    pub tree: &'a str,
    pub host: &'a str,
    pub state: &'a str,
    pub runs: &'a str,
}

/// The -v options. The two cache volumes pi-rukas mounts are not mounted: what
/// a container writes under its home, caches of packages among them, stays in
/// its own layer and goes when the container is removed. Pi's settings.json is
/// the one install.sh puts in ~/.pi/agent: its provider retry and timeout.
/// models.json is the container's own, <state>/models.json: the host's with
/// each provider on the endpoint picked now.
///
/// `app` is the host directory of the GitHub App's token when the instance
/// names an app: the image's gh (host/sandbox/gh-shim) reads the token the host
/// keeps fresh there. Not the key, which stays in ~/.config/tehas.
pub fn mount_options(kind: Kind, p: &Paths, agent: &str, app: Option<&str>) -> Vec<String> {
    let (tree, host, state, runs) = (p.tree, p.host, p.state, p.runs);
    let pi = "/home/vscode/.pi/agent";
    let mut mounts = vec![
        format!("{host}:{tree}"),
        format!("{state}/cache:{CACHE_DIR}"),
        format!("{state}/memory:/home/vscode/.vipune"),
        format!("{state}/sessions:{pi}/sessions"),
        format!("{runs}:{pi}/ensemble-runs"),
        format!("{state}/models.json:{pi}/models.json:ro"),
        format!("{agent}/mcp.json:{pi}/mcp.json:ro"),
        format!("{agent}/settings.json:{pi}/settings.json:ro"),
    ];
    if let (Some(app), true) = (app, kind.token()) {
        mounts.push(format!("{app}:/run/tehas/github:ro"));
    }
    // After the shared list, so this is the one build output the session has.
    if kind == Kind::Session {
        mounts.push(format!("{state}/target:{TARGET_DIR}"));
    }
    pair("-v", mounts)
}

fn agent_dir() -> String {
    format!("{}/.pi/agent", env::var("HOME").unwrap_or_default())
}

/// Whether Pi's settings.json and models.json are there to be mounted, and the
/// tokens models.json names are set. Commands ask before they call Docker or
/// git or write anything: a missing file is a failure, not a default, since
/// Docker would make a root-owned directory in its place. Err is the line for
/// standard error.
pub fn settings_ok() -> Result<(), String> {
    let agent = agent_dir();
    for f in ["settings.json", "models.json"] {
        if !std::path::Path::new(&format!("{agent}/{f}")).is_file() {
            return Err(format!("tehas: no {agent}/{f}: run install.sh"));
        }
    }
    let file = models::host_path();
    models::keys_ok(&ModelFile::read(&file).json, &file, set).map_err(|e| format!("tehas: {e}"))
}

/// The -v options with what they name made on the host, because Docker would
/// create a missing mount source as a root-owned directory. The caller makes
/// <host> and writes <state>/models.json. Asks [`settings_ok`] again, and says
/// why on Err.
pub fn prepared_mounts(kind: Kind, p: &Paths) -> Result<Vec<String>, String> {
    if [p.tree, p.host, p.state, p.runs]
        .iter()
        .any(|d| d.is_empty())
    {
        return Err(String::new());
    }
    settings_ok()?;
    let mut dirs = vec![
        p.runs.to_string(),
        format!("{}/cache", p.state),
        format!("{}/memory", p.state),
        format!("{}/sessions", p.state),
    ];
    let app = env::var("TEHAS_GITHUB_APP_ID")
        .ok()
        .filter(|id| !id.is_empty() && kind.token())
        .map(|_| {
            format!(
                "{}/state/github",
                env::var("TEHAS_HOME").unwrap_or_default()
            )
        });
    dirs.extend(app.clone());
    for dir in dirs {
        fs::create_dir_all(&dir).map_err(|e| format!("tehas: cannot make {dir}: {e}"))?;
    }
    Ok(mount_options(kind, p, &agent_dir(), app.as_deref()))
}

/// The --add-host options: the names the sandbox cannot resolve by itself, from
/// PI_ENSEMBLE_HOST_ALIASES as lib/config.sh checked it.
pub fn hosts(aliases: &str) -> Vec<String> {
    aliases
        .lines()
        .next()
        .unwrap_or("")
        .split(',')
        .filter(|p| !p.is_empty())
        .map(|p| format!("--add-host={p}"))
        .collect()
}

/// What a container is not allowed to do, so that an agent's code cannot give
/// itself more of the host than it was given: every capability Docker grants
/// by default is taken away, and a program cannot gain rights through its mode
/// or file capabilities. The same ten words for a cycle, a planning turn, a
/// manual session, a review and the push check before a cycle. host/docs/host.md
/// documents them in a table, and test/work_caps_test.sh compares that table
/// with what a command is given, so adding one option is an edit here, a row
/// there, and nothing else.
///
/// Only CHOWN and FOWNER are given back:
///
///   CHOWN    npm's install drops a package to the unprivileged user with
///            chown(2) and lchown(2), and Playwright's install of Chromium
///            copies a cache tree that way. Both run in a gate of the product.
///            Without it the gate fails on EPERM.
///   FOWNER   npm moves its staging directory onto its destination with rename,
///            and rename of a file one does not own is refused even when the
///            directory is writable. tar's unpack, cargo's install of a built
///            crate and a gate's own mv do the same.
///
/// Nothing else in the list is named. In particular:
///
///   SETUID, SETGID   nothing a cycle runs becomes another user. The image has
///                    no sudo, the container runs as one uid and tehas gives it
///                    no setuid program to reach another. Playwright's Chromium
///                    runs its own sandbox only with a capability that would
///                    let it make a setuid helper, and it is run with
///                    --no-sandbox instead, which pi-rukas's browser tool
///                    passes. no-new-privileges does not break anything a cycle
///                    does: it stops a program gaining rights, and nothing in
///                    the image is meant to have any.
///   NET_RAW          the agents answer over HTTP with curl and fetch through
///                    git, npm and cargo, none of which opens a raw socket. It
///                    is off Docker's own default list too.
///   SYS_ADMIN        the one capability that matters for a container's escape,
///                    and the one Chromium's sandbox would want. Nothing here
///                    mounts anything: tehas makes every mount on the host
///                    before it starts a container.
///
/// The root filesystem stays writable, unlike a preview's: a cycle's gate
/// installs into its own tree and its caches, and a read-only root would need
/// one --tmpfs per directory a tool writes in, which is a list nobody can pin.
/// One --tmpfs is given all the same, for /tmp: not to hide the root but to
/// bound it. The writable layer it replaces sits on the host's root disk with
/// no limit of its own; on 2026-10-20 one cycle filled the host's disk with a
/// log under /tmp and four others died with it. Why the bit stays exec, what
/// the container's own TMPDIR is, and what the size means are in the manual, at
/// "What a container may not do" in host/docs/host.md. The size is half the
/// container's --memory: tmpfs pages count against that limit and cannot be
/// reclaimed, so a /tmp as large as the limit ends in OOM kills of the agents
/// (#761). At half, a full /tmp is ENOSPC for its writer and the other half is
/// left to the processes. size=0 would be no bound at all, so a ceiling under
/// 10 MiB is no ceiling, and gives no options.
pub fn security(ceiling_mb: u64) -> Option<Vec<String>> {
    if !(10..=999_999_999).contains(&ceiling_mb) {
        return None;
    }
    let tmp = format!(
        "/tmp:rw,exec,nosuid,nodev,size={}m,mode=1777",
        ceiling_mb / 2
    );
    Some(
        [
            "--cap-drop",
            "ALL",
            "--cap-add",
            "CHOWN",
            "--cap-add",
            "FOWNER",
            "--security-opt",
            "no-new-privileges:true",
            "--tmpfs",
            &tmp,
        ]
        .map(String::from)
        .to_vec(),
    )
}

/// What a container starts with, once the slice said it can be used.
#[derive(Debug, PartialEq)]
pub struct Taken {
    /// The fourteen words of its limits.
    pub limits: Vec<String>,
    /// The ten words of [`security`].
    pub security: Vec<String>,
    pub slice: Usable,
}

/// The limits and the security options from a reading of the host, or the
/// words a person is told, led by <who>. <what> names what does not start.
pub fn take(r: &Reading, load: &Load, who: &str, what: &str) -> Result<Taken, String> {
    let slice =
        slice::usable(r, &load.reserve, &load.containers).map_err(|why| format!("{who}: {why}"))?;
    let limits = slice::limits(&slice, load)
        .ok_or_else(|| format!("{who}: the container limits could not be made, not starting"))?;
    let security = security(slice.ceiling_mb).ok_or_else(|| {
        format!("{who}: tehas_sandbox_security gave 0 words, wants 0: not starting {what}")
    })?;
    Ok(Taken {
        limits,
        security,
        slice,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    const WHO: Identity = Identity {
        tree: "/w/widget",
        name: "A Bot",
        email: "bot@example.com",
    };

    // The whole list of a kind, literally: what test/fixtures/argv holds for
    // the same inputs. name holds a space, so it is compared by word.
    fn env_of(kind: Kind) -> Vec<String> {
        env_options(
            kind,
            &WHO,
            &["PI_ENSEMBLE_SUBAGENT_MODEL".to_string()],
            &["TEHAS_MODEL_KEY_MAIN".to_string()],
        )
    }

    fn shared_env() -> Vec<String> {
        let mut e = words(
            "-e PI_ENSEMBLE_SANDBOX_MODE=1 -e PI_ENSEMBLE_WORKSPACE_ROOT=/w/widget \
             -e CBM_CACHE_DIR=/home/vscode/.tehas-cache -e GIT_CONFIG_COUNT=4 \
             -e GIT_CONFIG_KEY_0=credential.https://github.com.helper -e GIT_CONFIG_VALUE_0= \
             -e GIT_CONFIG_KEY_1=credential.https://github.com.helper",
        );
        e.extend(["-e", "GIT_CONFIG_VALUE_1=!gh auth git-credential"].map(String::from));
        e.extend(words("-e GIT_CONFIG_KEY_2=user.name"));
        e.extend(["-e", "GIT_CONFIG_VALUE_2=A Bot"].map(String::from));
        e.extend(words(
            "-e GIT_CONFIG_KEY_3=user.email -e GIT_CONFIG_VALUE_3=bot@example.com \
             -e GIT_TERMINAL_PROMPT=0 -e PI_ENSEMBLE_SUBAGENT_MODEL -e TEHAS_MODEL_KEY_MAIN",
        ));
        e
    }

    #[test]
    fn a_cycle_and_a_turn_are_given_the_token_by_name_and_first() {
        let mut want = words("-e GH_TOKEN");
        want.extend(shared_env());
        assert_eq!(env_of(Kind::Cycle), want);
        assert_eq!(env_of(Kind::Turn), want);
    }

    #[test]
    fn a_review_has_no_token_in_any_form() {
        let env = env_of(Kind::Review);
        assert_eq!(env, shared_env());
        assert!(!env.iter().any(|w| w.starts_with("GH_TOKEN")));
    }

    #[test]
    fn a_session_is_given_its_build_output_last() {
        let mut want = words("-e GH_TOKEN");
        want.extend(shared_env());
        want.extend(words("-e CARGO_TARGET_DIR=/home/vscode/.tehas-target"));
        assert_eq!(env_of(Kind::Session), want);
    }

    const PATHS: Paths = Paths {
        tree: "/in",
        host: "/out",
        state: "/s",
        runs: "/r",
    };

    const EIGHT: &str = "-v /out:/in -v /s/cache:/home/vscode/.tehas-cache \
        -v /s/memory:/home/vscode/.vipune -v /s/sessions:/home/vscode/.pi/agent/sessions \
        -v /r:/home/vscode/.pi/agent/ensemble-runs \
        -v /s/models.json:/home/vscode/.pi/agent/models.json:ro \
        -v /a/mcp.json:/home/vscode/.pi/agent/mcp.json:ro \
        -v /a/settings.json:/home/vscode/.pi/agent/settings.json:ro";

    #[test]
    fn every_kind_has_the_eight_mounts_in_order() {
        for kind in [Kind::Cycle, Kind::Turn, Kind::Review] {
            assert_eq!(mount_options(kind, &PATHS, "/a", None), words(EIGHT));
        }
    }

    #[test]
    fn the_apps_token_directory_is_read_only_and_never_a_reviews() {
        let want = format!("{EIGHT} -v /g:/run/tehas/github:ro");
        for kind in [Kind::Cycle, Kind::Turn] {
            assert_eq!(mount_options(kind, &PATHS, "/a", Some("/g")), words(&want));
        }
        assert_eq!(
            mount_options(Kind::Review, &PATHS, "/a", Some("/g")),
            words(EIGHT)
        );
    }

    #[test]
    fn a_sessions_build_output_comes_after_the_shared_list() {
        let want =
            format!("{EIGHT} -v /g:/run/tehas/github:ro -v /s/target:/home/vscode/.tehas-target");
        assert_eq!(
            mount_options(Kind::Session, &PATHS, "/a", Some("/g")),
            words(&want)
        );
    }

    #[test]
    fn nothing_that_controls_the_host_is_in_any_list() {
        for kind in [Kind::Cycle, Kind::Turn, Kind::Session, Kind::Review] {
            let mut all = env_of(kind);
            all.extend(mount_options(kind, &PATHS, "/a", Some("/g")));
            let all = all.join(" ");
            for word in ["docker.sock", ".ssh", ".config/tehas", "--privileged"] {
                assert!(!all.contains(word), "{kind:?} has {word}");
            }
        }
    }

    #[test]
    fn host_aliases_are_one_option_each() {
        assert_eq!(
            hosts("models:192.0.2.9,,b:192.0.2.10"),
            ["--add-host=models:192.0.2.9", "--add-host=b:192.0.2.10"]
        );
        assert!(hosts("").is_empty());
    }

    #[test]
    fn the_security_options_are_the_ten_documented_words() {
        assert_eq!(
            security(2048).unwrap(),
            words(
                "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER \
                 --security-opt no-new-privileges:true \
                 --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777"
            )
        );
    }

    // From test/work_caps_test.sh.
    #[test]
    fn the_tmp_tmpfs_is_half_the_ceiling_and_no_ceiling_gives_no_options() {
        for (ceiling, size) in [(2048, "1024m"), (5478, "2739m"), (10, "5m")] {
            let sec = security(ceiling).unwrap();
            assert!(sec[9].contains(&format!("size={size},")), "{ceiling}");
        }
        for ceiling in [0, 1, 9, 1_000_000_000] {
            assert_eq!(security(ceiling), None);
        }
    }

    fn host(max: &str) -> Reading {
        let mut show = serde_json::Map::new();
        show.insert("FragmentPath".into(), "/etc/tehas.slice".into());
        show.insert("ActiveState".into(), "active".into());
        show.insert("MemoryMax".into(), max.into());
        Reading {
            show: Some(show),
            docker: Some("systemd 2".into()),
            controllers: Some("cpu memory pids".into()),
            cpus: Some("16".into()),
            root: "/sys/fs/cgroup".into(),
            ..Reading::default()
        }
    }

    fn load() -> Load {
        Load {
            reserve: "2g".into(),
            containers: "4".into(),
            cycles: "4".into(),
            ..Load::default()
        }
    }

    #[test]
    fn a_good_host_gives_fourteen_words_and_ten() {
        let t = take(&host("8589934592"), &load(), "tehas work", "a cycle").unwrap();
        assert_eq!(t.limits.len(), 14);
        assert_eq!(t.limits[1], "tehas.slice");
        assert_eq!(t.limits[3], "2048m");
        assert_eq!(t.security.len(), 10);
        assert!(t.security[9].contains("size=1024m,"));
    }

    #[test]
    fn a_slice_that_cannot_be_used_says_why_after_the_command() {
        let mut r = host("8589934592");
        r.show = None;
        assert_eq!(
            take(&r, &load(), "tehas work", "a cycle"),
            Err("tehas work: systemctl gave no answer".to_string())
        );
    }

    #[test]
    fn a_concurrency_that_is_no_number_starts_nothing() {
        let mut l = load();
        l.cycles = String::new();
        assert_eq!(
            take(&host("8589934592"), &l, "tehas plan", "a turn"),
            Err("tehas plan: the container limits could not be made, not starting".to_string())
        );
    }

    #[test]
    fn a_ceiling_under_ten_mib_starts_nothing() {
        let mut l = load();
        l.reserve = "6m".into();
        assert_eq!(
            take(&host("33554432"), &l, "tehas plan", "a turn"),
            Err(
                "tehas plan: tehas_sandbox_security gave 0 words, wants 0: not starting a turn"
                    .to_string()
            )
        );
    }
}
