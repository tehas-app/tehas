//! The read-only MCP server's command: `tehasbot mcp serve` runs the program
//! in its container, `tehas mcp key` prints a new key. What each does for a
//! person is in src/usage/mcp.txt.
//!
//!   console tehas|tehasbot mcp serve
//!   console tehas|tehasbot mcp key
//!
//! serve runs under `libexec/with-settings`; key needs no settings. The
//! container is a security boundary: its arguments are pinned by
//! test/mcp_test.sh, and keys reach it by name, never in an argument.
//!
//! Exit: see src/usage/mcp.txt.

use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use tehas_core::mask;
use tehas_core::models;
use tehas_core::settings;

const NAME: &str = "tehas-mcp";
const MASQUERADE: &str = "com.docker.network.bridge.enable_ip_masquerade";

fn say(line: &str) {
    eprintln!("tehas: {line}");
}

fn setting(name: &str) -> String {
    env::var_os(name).map_or(String::new(), |v| v.to_string_lossy().into_owned())
}

/// A command's standard output as `$(...)` keeps it, with its standard error
/// left to the caller. None when it could not start or did not succeed.
fn stdout_of(command: &mut Command) -> Option<String> {
    let out = command.output().ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .trim_end_matches('\n')
            .to_string()
    })
}

/// 32 random bytes, base64url without padding.
fn key() -> u8 {
    let mut bytes = [0u8; 32];
    match File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)) {
        Ok(()) => {
            println!("{}", base64url(&bytes));
            0
        }
        Err(e) => {
            say(&format!("could not read /dev/urandom: {e}"));
            1
        }
    }
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group =
            chunk.iter().fold(0u32, |n, b| n << 8 | u32::from(*b)) << (8 * (3 - chunk.len()));
        for i in 0..=chunk.len() {
            out.push(char::from(ALPHABET[(group >> (18 - 6 * i) & 63) as usize]));
        }
    }
    out
}

/// Whether the setting holds a key. It stops at a bad one, naming the setting
/// and the key's place, never the key. The shape is the server's rule, the
/// byte check of `listed` in console/mcp/src/keys.rs, read here before docker
/// is asked for a container.
fn holds_key(name: &str, value: &str) -> Result<bool, String> {
    let line = value.split('\n').next().unwrap_or_default();
    let keys = line.split([' ', '\t']).filter(|k| !k.is_empty());
    let mut any = false;
    for (i, key) in keys.enumerate() {
        let good = key.len() >= 32
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !good {
            return Err(format!(
                "{name}: key {} is not at least 32 characters of A-Za-z0-9_-",
                i + 1
            ));
        }
        any = true;
    }
    Ok(any)
}

/// Each list is checked where it is written, so a bad key is named by its own
/// setting and place.
fn check_keys(full: &str, dashboard: &str) -> Result<(), String> {
    let full = holds_key("TEHAS_MCP_KEYS", full)?;
    let dashboard = holds_key("TEHAS_MCP_DASHBOARD_KEYS", dashboard)?;
    if full || dashboard {
        Ok(())
    } else {
        Err("TEHAS_MCP_KEYS and TEHAS_MCP_DASHBOARD_KEYS are both empty: give at least one key in either (tehas mcp key makes one)".to_string())
    }
}

/// The digest of the Caddy image the docs server runs. That image is pinned
/// already, so no other is trusted here: it is only a root file system for a
/// static binary.
fn digest(versions: &str) -> Option<&str> {
    let value = versions
        .split('\n')
        .find_map(|line| line.strip_prefix("CADDY_DIGEST="))?;
    let hex = value.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        .then_some(value)
}

/// The version the server states: the first line of the file, in the
/// characters a version has.
fn version(file: &[u8]) -> String {
    let line = file.split(|b| *b == b'\n').next().unwrap_or_default();
    let kept: String = line
        .iter()
        .filter(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
        .map(|b| char::from(*b))
        .collect();
    if kept.is_empty() {
        "unknown".to_string()
    } else {
        kept
    }
}

/// A network made by hand under this name is used only if it lets nothing out.
fn network() -> Result<(), String> {
    let format = format!("{{{{index .Options \"{MASQUERADE}\"}}}}");
    let seen = stdout_of(
        Command::new("docker")
            .args(["network", "inspect", "--format", &format, NAME])
            .stderr(Stdio::null()),
    );
    match seen.as_deref() {
        Some("false") => Ok(()),
        Some(_) => Err(format!(
            "the network {NAME} lets the container out (masquerade is not false): remove it and start again"
        )),
        None => Command::new("docker")
            .args(["network", "create", "--driver", "bridge", "-o"])
            .arg(format!("{MASQUERADE}=false"))
            .arg(NAME)
            .stdout(Stdio::null())
            .status()
            .ok()
            .filter(|status| status.success())
            .map(|_| ())
            .ok_or(format!("could not make the network {NAME}")),
    }
}

/// Made here, so Docker does not make it as root. The snapshot in it is
/// private: it holds what the agents saw.
fn snapshot_dir(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir)
        .and_then(|()| fs::set_permissions(dir, fs::Permissions::from_mode(0o700)))
        .map_err(|e| format!("could not make {}: {e}", dir.display()))
}

fn remove_container() {
    let _ = Command::new("docker")
        .args(["rm", "-f", NAME])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

struct Run {
    home: String,
    addr: String,
    user: String,
    digest: String,
    version: String,
    private: Vec<String>,
}

/// Every argument the container gets. A change here widens or narrows what a
/// caller on the internet can reach.
fn run_args(run: &Run) -> Vec<String> {
    let home = &run.home;
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "--init",
        "--name",
        NAME,
        "--user",
        &run.user,
        "--network",
        NAME,
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges:true",
        "--memory",
        "256m",
        "--pids-limit",
        "64",
        "-p",
        &format!("{}:8082:8082", run.addr),
        "-v",
        &format!("{home}/live:/live:ro"),
        "-v",
        &format!("{home}/mcp:/sessions:ro"),
        "-v",
        &format!("{home}/logs:/logs:ro"),
        "-v",
        &format!("{home}/libexec/tehas-mcp:/tehas-mcp:ro"),
        "-e",
        "TEHAS_MCP_KEYS",
        "-e",
        "TEHAS_MCP_DASHBOARD_KEYS",
        "--entrypoint",
        "/tehas-mcp",
        &format!("caddy@{}", run.digest),
        "--live",
        "/live",
        "--sessions",
        "/sessions",
        "--logs",
        "/logs",
        "--listen",
        "0.0.0.0:8082",
        "--server-version",
        &run.version,
    ]
    .map(str::to_string)
    .to_vec();
    for name in &run.private {
        args.push("--private".to_string());
        args.push(name.clone());
    }
    args
}

fn is_program(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn prepare(home: &str) -> Result<Run, (u8, String)> {
    let fail = |line: String| (1, line);
    check_keys(
        &setting("TEHAS_MCP_KEYS"),
        &setting("TEHAS_MCP_DASHBOARD_KEYS"),
    )
    .map_err(|line| (78, line))?;
    let bin = format!("{home}/libexec/tehas-mcp");
    if !is_program(Path::new(&bin)) {
        return Err(fail(format!("no {bin}: run install.sh")));
    }
    let versions = fs::read(format!("{home}/versions.env")).unwrap_or_default();
    let versions = String::from_utf8_lossy(&versions);
    let digest = digest(&versions)
        .ok_or(fail(format!(
            "no CADDY_DIGEST in {home}/versions.env: run install.sh"
        )))?
        .to_string();
    let version = version(&fs::read(format!("{home}/docs/version.txt")).unwrap_or_default());
    network().map_err(fail)?;
    snapshot_dir(&Path::new(home).join("mcp")).map_err(fail)?;
    // Standard error stays the command's, as git's complaint about a broken
    // configuration says more than an empty name.
    let email = stdout_of(
        Command::new("git")
            .args(["config", "--global", "user.email"])
            .stderr(Stdio::inherit()),
    )
    .unwrap_or_default();
    let id = |flag: &str| stdout_of(Command::new("id").arg(flag)).unwrap_or_default();
    let addr = setting("TEHAS_HOST_ADDR");
    Ok(Run {
        home: home.to_string(),
        addr: if addr.is_empty() {
            "127.0.0.1".to_string()
        } else {
            addr
        },
        user: format!("{}:{}", id("-u"), id("-g")),
        digest,
        version,
        private: mask::host_names(&models::host_path(), &email),
    })
}

fn serve() -> u8 {
    // A run that skipped the load must not read as a server turned off.
    if let Err(rc) = settings::require(&["TEHAS_HOME", "TEHAS_INSTANCE_DIR"]) {
        return rc;
    }
    match setting("TEHAS_MCP").as_str() {
        "" | "0" => {
            remove_container();
            return 0;
        }
        "1" => {}
        _ => {
            say("TEHAS_MCP is 0 or 1 (in ~/.config/tehas/env)");
            return 78;
        }
    }
    let run = match prepare(&setting("TEHAS_HOME")) {
        Ok(run) => run,
        Err((rc, line)) => {
            say(&line);
            return rc;
        }
    };
    remove_container();
    let error = Command::new("docker").args(run_args(&run)).exec();
    say(&format!("could not run docker: {error}"));
    127
}

pub fn main(given: &[String]) -> u8 {
    match given.first().map(String::as_str) {
        Some("serve") => serve(),
        Some("key") => key(),
        _ => {
            eprintln!("usage: tehasbot mcp serve | tehas mcp key");
            64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "abcdefghijklmnopqrstuvwxyz012345";

    #[test]
    fn a_key_is_the_url_safe_alphabet_with_no_padding() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff, 0xfe]), "-__-");
        assert_eq!(base64url(&[0; 32]).len(), 43);
    }

    #[test]
    fn keys_are_read_in_words_of_the_first_line() {
        assert_eq!(holds_key("X", ""), Ok(false));
        assert_eq!(holds_key("X", " \t "), Ok(false));
        assert_eq!(holds_key("X", KEY), Ok(true));
        assert_eq!(holds_key("X", &format!("{KEY}\t {KEY}")), Ok(true));
        assert_eq!(holds_key("X", &format!("\n{KEY}")), Ok(false));
    }

    #[test]
    fn a_bad_key_is_named_by_setting_and_place_and_not_by_value() {
        let bad = format!("{KEY} {}+", &KEY[..31]);
        let said = holds_key("TEHAS_MCP_KEYS", &bad).unwrap_err();
        assert_eq!(
            said,
            "TEHAS_MCP_KEYS: key 2 is not at least 32 characters of A-Za-z0-9_-"
        );
        assert!(check_keys("", "").unwrap_err().contains("both empty"));
        assert!(check_keys(KEY, "x")
            .unwrap_err()
            .starts_with("TEHAS_MCP_DASHBOARD_KEYS: key 1"));
        assert_eq!(check_keys("", KEY), Ok(()));
    }

    #[test]
    fn the_digest_is_the_first_line_of_its_name_and_must_be_whole() {
        let good = format!("sha256:{}", "a".repeat(64));
        let file = format!("CADDY_VERSION=2\nCADDY_DIGEST={good}\nCADDY_DIGEST=other\n");
        assert_eq!(digest(&file), Some(good.as_str()));
        assert_eq!(digest("CADDY_DIGEST=latest\n"), None);
        assert_eq!(digest(&format!("CADDY_DIGEST={good}\r\n")), None);
        assert_eq!(
            digest(&format!("CADDY_DIGEST={}", good.to_uppercase())),
            None
        );
        assert_eq!(digest(""), None);
    }

    #[test]
    fn the_version_keeps_its_characters_of_the_first_line() {
        assert_eq!(version(b"v0.7.30+abc1234\nnext\n"), "v0.7.30+abc1234");
        assert_eq!(version(b"v1 (x)/2\n"), "v1x2");
        assert_eq!(version(b""), "unknown");
        assert_eq!(version(b"//\n"), "unknown");
    }

    #[test]
    fn no_argument_of_the_container_holds_a_key_or_a_mount_beyond_the_four() {
        let args = run_args(&Run {
            home: "/h".to_string(),
            addr: "127.0.0.1".to_string(),
            user: "1:2".to_string(),
            digest: "sha256:x".to_string(),
            version: "v1".to_string(),
            private: vec!["gpu1".to_string()],
        });
        let mounts: Vec<&String> = args
            .iter()
            .zip(args.iter().skip(1))
            .filter(|(flag, _)| *flag == "-v")
            .map(|(_, mount)| mount)
            .collect();
        assert_eq!(
            mounts,
            [
                "/h/live:/live:ro",
                "/h/mcp:/sessions:ro",
                "/h/logs:/logs:ro",
                "/h/libexec/tehas-mcp:/tehas-mcp:ro"
            ]
        );
        assert_eq!(args[args.len() - 2..], ["--private", "gpu1"]);
    }
}
