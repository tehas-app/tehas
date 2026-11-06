//! Where a sandbox container may connect, with `TEHAS_EGRESS=1` (#417).
//!
//! A container is put on the internal network `tehas-egress`, which has no
//! way out and no address on the host, and is given `HTTPS_PROXY` and
//! `HTTP_PROXY` naming the proxy container `tehas-egress` on it. The proxy is
//! on that network and on `tehas-egress-out`, a plain bridge, and connects
//! only to the hosts allowed here: the base list below, the host of every
//! endpoint in models.json, and `TEHAS_EGRESS_HOSTS`. A tool that ignores the
//! proxy variables reaches nothing: it fails closed. The network is held to
//! the previews' check in `hostnet`, Docker 27.1 or later included, and a
//! network that falls short stops the container before it starts.
//!
//! The proxy is the static `tehas-mcp` binary's `egress` verb in the Caddy
//! image the docs server is pinned to, as the MCP server is run. It is started
//! by the first container that needs it and started again when its list, its
//! binary or its image changed. Without `TEHAS_EGRESS=1` nothing here runs and
//! a container has Docker's default bridge, as before.

use crate::hostnet;
use crate::models::ModelFile;
use crate::sandbox;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::process::{Command, Stdio};

/// The internal network of the containers, and the proxy's name on it.
pub const NETWORK: &str = "tehas-egress";
pub const NAME: &str = "tehas-egress";
/// The proxy's way out.
pub const OUTSIDE: &str = "tehas-egress-out";
pub const PORT: u16 = 3128;
const LABEL: &str = "tehas.egress";

/// What every cycle reaches, on 443: GitHub for git, gh and the release
/// downloads, and the registries of npm and cargo. The owner's list of
/// 2026-10-22 on #417.
pub const BASE: [&str; 8] = [
    "github.com",
    "api.github.com",
    "codeload.github.com",
    "objects.githubusercontent.com",
    "registry.npmjs.org",
    "crates.io",
    "static.crates.io",
    "index.crates.io",
];

/// Whether the instance asked for the limit.
pub fn on() -> bool {
    std::env::var("TEHAS_EGRESS").is_ok_and(|v| v == "1")
}

/// `host:port` of an endpoint's url, the port from the scheme when it has
/// none. None for a url with no host or of another scheme.
pub fn url_pair(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let port = match scheme.to_ascii_lowercase().as_str() {
        "https" => 443,
        "http" => 80,
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    if authority.is_empty() || authority.starts_with('[') {
        return None;
    }
    let pair = match authority.split_once(':') {
        Some((h, p)) if !h.is_empty() && p.parse::<u16>().is_ok() => format!("{h}:{p}"),
        Some(_) => return None,
        None => format!("{authority}:{port}"),
    };
    Some(pair.to_ascii_lowercase())
}

/// One word of `TEHAS_EGRESS_HOSTS`: a host, on 443, or host:port.
pub fn host_pair(word: &str) -> String {
    if word.contains(':') {
        word.to_ascii_lowercase()
    } else {
        format!("{}:443", word.to_ascii_lowercase())
    }
}

/// The pairs the proxy is given, sorted and each once: the base, the main
/// and backup endpoint of every provider in <models>, and the words of
/// <extra>.
pub fn allowed(models: &ModelFile, extra: &str) -> Vec<String> {
    let mut pairs: Vec<String> = BASE.iter().map(|h| format!("{h}:443")).collect();
    if let Some(providers) = models.json.get("providers").and_then(|p| p.as_object()) {
        for name in providers.keys() {
            for end in ["main", "backup"] {
                pairs.extend(url_pair(&models.url(name, end)));
            }
        }
    }
    pairs.extend(extra.split_whitespace().map(host_pair));
    pairs.sort();
    pairs.dedup();
    pairs
}

/// The words a sandbox container is given with the limit on: the internal
/// network and the proxy, in both spellings tools read. Checked on 2026-11-06
/// with the pi-rukas image: curl, git, gh, npm, cargo and bun go through it,
/// node with NODE_USE_ENV_PROXY. Nothing bypasses it,
/// not even localhost, which inside the container is the container.
pub fn options() -> Vec<String> {
    let at = format!("http://{NAME}:{PORT}");
    let mut w = vec!["--network".to_string(), NETWORK.to_string()];
    for name in ["HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy"] {
        w.push("-e".to_string());
        w.push(format!("{name}={at}"));
    }
    for name in ["NO_PROXY", "no_proxy"] {
        w.push("-e".to_string());
        w.push(format!("{name}=localhost,127.0.0.1"));
    }
    // Node's fetch reads the proxy variables only when asked: pi in a manual
    // session runs under node 22. The runner of a cycle runs under bun, which
    // reads them anyway.
    w.push("-e".to_string());
    w.push("NODE_USE_ENV_PROXY=1".to_string());
    w
}

/// The digest of the Caddy image the docs server runs. That image is pinned
/// already, so no other is trusted: it is only a root file system for a
/// static binary.
pub fn caddy_digest(versions: &str) -> Option<&str> {
    let value = versions
        .split('\n')
        .find_map(|line| line.strip_prefix("CADDY_DIGEST="))?;
    let hex = value.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        .then_some(value)
}

/// What the proxy container is started from.
pub struct Run {
    pub home: String,
    pub user: String,
    pub digest: String,
    /// The --add-host words the sandbox containers get, so a name the host
    /// aliases resolves for the proxy as it did for them.
    pub hosts: Vec<String>,
    pub allowed: Vec<String>,
    /// Of the binary, so an install that replaced it starts the proxy again.
    pub binary: String,
}

/// The proxy's arguments after `docker run`, without its label.
fn body(run: &Run) -> Vec<String> {
    let mut w: Vec<String> = [
        "--name",
        NAME,
        "--restart",
        "unless-stopped",
        "--user",
        &run.user,
        "--network",
        OUTSIDE,
        "--network",
        NETWORK,
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges:true",
        "--memory",
        "128m",
        "--pids-limit",
        "512",
        "--log-opt",
        "max-size=10m",
        "--log-opt",
        "max-file=2",
    ]
    .map(String::from)
    .to_vec();
    w.extend(run.hosts.iter().cloned());
    w.extend([
        "-v".to_string(),
        format!("{}/libexec/tehas-mcp:/tehas-mcp:ro", run.home),
        "--entrypoint".to_string(),
        "/tehas-mcp".to_string(),
        format!("caddy@{}", run.digest),
        "egress".to_string(),
        "--listen".to_string(),
        format!("0.0.0.0:{PORT}"),
    ]);
    for pair in &run.allowed {
        w.push("--allow".to_string());
        w.push(pair.clone());
    }
    w
}

/// What tells one proxy from another: a running proxy with the same key is
/// the one this run would start.
fn key(run: &Run) -> String {
    let mut h = DefaultHasher::new();
    body(run).hash(&mut h);
    run.binary.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Every argument of the proxy's `docker run`.
pub fn run_args(run: &Run) -> Vec<String> {
    let mut w = vec![
        "run".to_string(),
        "--detach".to_string(),
        "--label".to_string(),
        format!("{LABEL}={}", key(run)),
    ];
    w.extend(body(run));
    w
}

/// The proxy up for <run>, on networks that hold, with <docker> as the
/// Docker it asks: the arguments, and stdout when the call succeeded.
pub fn ensure_with(
    docker: &mut impl FnMut(&[String]) -> Option<String>,
    run: &Run,
) -> Result<(), String> {
    let mut call = |w: &[&str]| docker(&w.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    // The previews' check: an old Docker is not asked to make anything, and a
    // network is held to all four words, not to the option alone.
    if let Some(why) = hostnet::docker_fault(&mut call) {
        return Err(format!(
            "{why}: the network {NETWORK} cannot keep the host out"
        ));
    }
    if call(&["network", "inspect", NETWORK]).is_none() && !hostnet::net_create(&mut call, NETWORK)
    {
        return Err(format!("could not make the network {NETWORK}"));
    }
    if let Some((_, why)) = hostnet::net_fault(&mut call, NETWORK) {
        return Err(format!(
            "the network {NETWORK} does not keep the host out: {why}: remove it and start again"
        ));
    }
    if call(&["network", "inspect", "--format", "{{.Name}}", OUTSIDE]).is_none() {
        call(&["network", "create", "--ipv6=false", OUTSIDE])
            .ok_or(format!("could not make the network {OUTSIDE}"))?;
    }
    let want = format!("true {}", key(run));
    let state = format!("{{{{.State.Running}}}} {{{{index .Config.Labels \"{LABEL}\"}}}}");
    let seen = call(&["inspect", "--type", "container", "--format", &state, NAME]);
    if seen.as_deref() == Some(want.as_str()) {
        return Ok(());
    }
    if seen.is_some() {
        call(&["rm", "-f", NAME]);
    }
    docker(&run_args(run)).ok_or(format!("could not start the proxy {NAME}"))?;
    Ok(())
}

fn docker(args: &[String]) -> Option<String> {
    let out = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The proxy of this host up, or the line that says why not. One caller at a
/// time, under state/egress.lock: two cycles that start together would
/// otherwise each replace the other's proxy.
pub fn ensure(aliases: &str) -> Result<(), String> {
    let home = std::env::var("TEHAS_HOME").unwrap_or_default();
    if home.is_empty() {
        return Err("TEHAS_HOME is not set".to_string());
    }
    let bin = format!("{home}/libexec/tehas-mcp");
    let meta = fs::metadata(&bin).map_err(|_| format!("no {bin}: run install.sh"))?;
    let binary = format!("{} {:?}", meta.len(), meta.modified().ok());
    let versions = fs::read_to_string(format!("{home}/versions.env")).unwrap_or_default();
    let digest = caddy_digest(&versions)
        .ok_or(format!(
            "no CADDY_DIGEST in {home}/versions.env: run install.sh"
        ))?
        .to_string();
    let run = Run {
        home: home.clone(),
        user: sandbox::user(),
        digest,
        hosts: sandbox::hosts(aliases),
        allowed: allowed(
            &ModelFile::read(&crate::models::host_path()),
            &std::env::var("TEHAS_EGRESS_HOSTS").unwrap_or_default(),
        ),
        binary,
    };
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(format!("{home}/state/egress.lock"))
        .map_err(|e| format!("could not open {home}/state/egress.lock: {e}"))?;
    lock.lock()
        .map_err(|e| format!("could not lock {home}/state/egress.lock: {e}"))?;
    ensure_with(&mut docker, &run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run() -> Run {
        Run {
            home: "/h/tehas".into(),
            user: "1000:1000".into(),
            digest: format!("sha256:{}", "a".repeat(64)),
            hosts: vec!["--add-host=models.lan:192.0.2.7".into()],
            allowed: vec!["github.com:443".into(), "models.lan:8080".into()],
            binary: "1 2".into(),
        }
    }

    #[test]
    fn an_endpoint_is_its_host_and_the_port_of_its_url_or_scheme() {
        for (url, want) in [
            ("https://LLM.example/v1", Some("llm.example:443")),
            ("http://models.example:8080/v1", Some("models.example:8080")),
            ("http://u:p@m.example/v1", Some("m.example:80")),
            ("https://m.example?x", Some("m.example:443")),
            ("ftp://m.example", None),
            ("https:///v1", None),
            ("http://m.example:port/", None),
            ("http://[::1]:8080/", None),
            ("", None),
        ] {
            assert_eq!(url_pair(url).as_deref(), want, "{url}");
        }
    }

    #[test]
    fn the_list_is_the_base_the_endpoints_and_the_setting_each_once() {
        let models = ModelFile {
            json: json!({"providers": {
                "a": {"baseUrl": "http://models.example:8080/v1", "backupBaseUrl": "https://fleet.example/v1"},
                "b": {"baseUrl": "https://fleet.example/v1"},
                "c": {"baseUrl": 7}
            }}),
        };
        let got = allowed(&models, " pypi.org  Extra.example:8443 github.com ");
        for want in [
            "github.com:443",
            "index.crates.io:443",
            "models.example:8080",
            "fleet.example:443",
            "pypi.org:443",
            "extra.example:8443",
        ] {
            assert!(got.contains(&want.to_string()), "{want} in {got:?}");
        }
        assert_eq!(got.len(), BASE.len() + 4, "{got:?}");
        let mut sorted = got.clone();
        sorted.sort();
        assert_eq!(got, sorted);
        assert_eq!(
            allowed(&ModelFile { json: json!(null) }, "").len(),
            BASE.len()
        );
    }

    #[test]
    fn a_container_gets_the_internal_network_and_the_proxy_in_both_spellings() {
        let w = options().join(" ");
        assert_eq!(
            w,
            "--network tehas-egress -e HTTPS_PROXY=http://tehas-egress:3128 \
             -e HTTP_PROXY=http://tehas-egress:3128 -e https_proxy=http://tehas-egress:3128 \
             -e http_proxy=http://tehas-egress:3128 -e NO_PROXY=localhost,127.0.0.1 \
             -e no_proxy=localhost,127.0.0.1 -e NODE_USE_ENV_PROXY=1"
        );
    }

    #[test]
    fn the_proxy_is_on_both_networks_read_only_and_given_only_its_binary() {
        let r = run();
        let w = run_args(&r);
        let line = w.join(" ");
        assert!(
            line.starts_with("run --detach --label tehas.egress="),
            "{line}"
        );
        assert!(line.contains(" --name tehas-egress --restart unless-stopped --user 1000:1000 --network tehas-egress-out --network tehas-egress --read-only --cap-drop ALL --security-opt no-new-privileges:true "), "{line}");
        assert!(line.contains(" --add-host=models.lan:192.0.2.7 -v /h/tehas/libexec/tehas-mcp:/tehas-mcp:ro --entrypoint /tehas-mcp caddy@sha256:"), "{line}");
        assert!(
            line.ends_with(
                " egress --listen 0.0.0.0:3128 --allow github.com:443 --allow models.lan:8080"
            ),
            "{line}"
        );
        assert_eq!(w.iter().filter(|x| *x == "-v").count(), 1);
        for no in [
            "-e",
            "--env",
            "-p",
            "--publish",
            "--privileged",
            "--cap-add",
        ] {
            assert!(!w.iter().any(|x| x == no), "{no}");
        }
    }

    #[test]
    fn the_key_moves_with_the_list_and_the_binary() {
        let a = run();
        let mut b = run();
        assert_eq!(key(&a), key(&b));
        b.allowed.push("x.example:443".into());
        assert_ne!(key(&a), key(&b));
        let mut c = run();
        c.binary = "1 3".into();
        assert_ne!(key(&a), key(&c));
    }

    /// A Docker that answers each call by the start of its line, and the calls.
    fn fake<'a>(
        answers: &'a [(&'a str, Option<&'a str>)],
        calls: &'a mut Vec<String>,
    ) -> impl FnMut(&[String]) -> Option<String> + 'a {
        move |w: &[String]| {
            let line = w.join(" ");
            calls.push(line.clone());
            answers
                .iter()
                .find(|(start, _)| line.starts_with(start))
                .and_then(|(_, said)| said.map(str::to_string))
        }
    }

    const VERSION: (&str, Option<&str>) = ("version --format", Some("29.1.3"));
    const FIELDS: &str = "network inspect --format {{.Internal}} {{.Driver}} {{.EnableIPv6}} \
         {{index .Options \"com.docker.network.bridge.gateway_mode_ipv4\"}} tehas-egress";
    const HOLDS: (&str, Option<&str>) = (FIELDS, Some("true bridge false isolated"));
    const THERE: (&str, Option<&str>) = ("network inspect tehas-egress", Some("[]"));
    const OUT: (&str, Option<&str>) = (
        "network inspect --format {{.Name}} tehas-egress-out",
        Some("tehas-egress-out"),
    );

    #[test]
    fn a_host_with_nothing_gets_both_networks_and_the_proxy() {
        let mut calls = vec![];
        let answers = [
            VERSION,
            HOLDS,
            ("network create", Some("")),
            ("run", Some("id")),
        ];
        let r = ensure_with(&mut fake(&answers, &mut calls), &run());
        assert_eq!(r, Ok(()));
        assert_eq!(calls[0], "version --format {{.Server.Version}}");
        assert_eq!(calls[1], "network inspect tehas-egress");
        assert_eq!(calls[2], "network create --internal --ipv6=false -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-egress");
        assert_eq!(calls[3], FIELDS);
        assert_eq!(calls[5], "network create --ipv6=false tehas-egress-out");
        assert!(
            calls[6].starts_with("inspect --type container"),
            "{calls:?}"
        );
        assert!(calls[7].starts_with("run --detach"), "{calls:?}");
        assert_eq!(calls.len(), 8);
    }

    #[test]
    fn the_proxy_that_is_running_with_the_same_key_is_kept() {
        let want = format!("true {}", key(&run()));
        let answers = [
            VERSION,
            HOLDS,
            THERE,
            OUT,
            ("inspect --type container", Some(want.as_str())),
        ];
        let mut calls = vec![];
        assert_eq!(ensure_with(&mut fake(&answers, &mut calls), &run()), Ok(()));
        assert_eq!(calls.len(), 5, "{calls:?}");
    }

    #[test]
    fn a_proxy_with_another_key_or_stopped_is_replaced() {
        for seen in ["true 0", "false 0"] {
            let answers = [
                VERSION,
                HOLDS,
                THERE,
                OUT,
                ("inspect --type container", Some(seen)),
                ("rm -f tehas-egress", Some("")),
                ("run", Some("id")),
            ];
            let mut calls = vec![];
            assert_eq!(ensure_with(&mut fake(&answers, &mut calls), &run()), Ok(()));
            assert_eq!(calls[5], "rm -f tehas-egress");
            assert!(calls[6].starts_with("run --detach"));
        }
    }

    #[test]
    fn a_docker_older_than_27_1_or_unread_is_refused_before_anything_is_made() {
        for (said, why) in [
            (Some("26.1.4"), "older than 27.1"),
            (Some("27.0.3"), "older than 27.1"),
            (Some("latest"), "could not be read"),
            (None, "could not be read"),
        ] {
            let answers = [
                ("version --format", said),
                HOLDS,
                THERE,
                ("network create", Some("")),
                ("run", Some("id")),
            ];
            let mut calls = vec![];
            let r = ensure_with(&mut fake(&answers, &mut calls), &run());
            assert!(r.unwrap_err().contains(why), "{said:?}");
            assert_eq!(calls.len(), 1, "only the version is asked: {calls:?}");
        }
    }

    #[test]
    fn a_network_that_lets_out_or_reaches_the_host_is_refused_and_nothing_is_started() {
        for (said, why) in [
            ("false bridge false isolated", "it is not internal"),
            ("false bridge false <no value>", "it is not internal"),
            ("true macvlan false isolated", "its driver is not bridge"),
            ("true ipvlan false isolated", "its driver is not bridge"),
            ("true bridge true isolated", "it has IPv6"),
            (
                "true bridge false <no value>",
                "it is not isolated from the host",
            ),
            ("true bridge false nat", "it is not isolated from the host"),
        ] {
            let answers = [
                VERSION,
                (FIELDS, Some(said)),
                THERE,
                ("network create", Some("")),
                ("run", Some("id")),
            ];
            let mut calls = vec![];
            let r = ensure_with(&mut fake(&answers, &mut calls), &run());
            let err = r.unwrap_err();
            assert!(err.contains(why), "{said}: {err}");
            assert!(err.contains("remove it and start again"), "{err}");
            assert_eq!(calls.len(), 3, "nothing is made or started: {calls:?}");
        }
    }

    #[test]
    fn a_network_made_here_is_checked_too() {
        // A Docker that took the option and ignored it, or made it with IPv6.
        for said in ["true bridge false <no value>", "true bridge true isolated"] {
            let answers = [
                VERSION,
                (FIELDS, Some(said)),
                ("network create", Some("")),
                ("run", Some("id")),
            ];
            let mut calls = vec![];
            let r = ensure_with(&mut fake(&answers, &mut calls), &run());
            assert!(r.is_err(), "{said}");
            assert_eq!(calls.len(), 4, "{calls:?}");
        }
        let answers = [VERSION];
        let mut calls = vec![];
        let r = ensure_with(&mut fake(&answers, &mut calls), &run());
        assert!(r
            .unwrap_err()
            .contains("could not make the network tehas-egress"));
    }

    #[test]
    fn a_proxy_that_does_not_start_fails_closed() {
        let none = [VERSION, HOLDS, ("network create", Some(""))];
        let mut calls = vec![];
        let r = ensure_with(&mut fake(&none, &mut calls), &run());
        assert!(r.unwrap_err().contains("could not start the proxy"));
    }

    #[test]
    fn the_digest_is_the_first_line_of_its_name_and_must_be_whole() {
        let good = format!("sha256:{}", "0123456789abcdef".repeat(4));
        let file = format!("X=1\nCADDY_DIGEST={good}\nCADDY_DIGEST=sha256:other\n");
        assert_eq!(caddy_digest(&file), Some(good.as_str()));
        assert_eq!(caddy_digest("CADDY_DIGEST=latest\n"), None);
        assert_eq!(caddy_digest(&format!("CADDY_DIGEST={good}\r\n")), None);
        assert_eq!(
            caddy_digest(&format!("CADDY_DIGEST={}", good.to_uppercase())),
            None
        );
        assert_eq!(caddy_digest(""), None);
    }
}
