//! tehas-proxy: the model proxy of outsource, the one way a contractor's
//! cycle reaches the factory's model (#2273).
//!
//!   tehas-proxy
//!   tehas-proxy --issue <job> <attempt> <contractor>
//!   tehas-proxy --revoke <contractor>
//!   tehas-proxy --forget <job>...
//!   tehas-proxy --counts
//!   tehas-proxy --stop
//!
//! The listener is the one thing this program does, and it is the only place
//! the factory's model key is used for a contractor. It is off unless the
//! feature `outsource` is on (core/src/features.rs): off, the command says the
//! word that says why and ends 0, and no port is opened.
//!
//! The settings are read as tehas_load leaves them in the environment:
//! TEHAS_HOME, whose state store holds the contractors and the jobs;
//! TEHAS_OUTSOURCE, the switch; TEHAS_OUTSOURCE_PROXY_ADDR, the address and port to
//! listen on, never the dashboard's; TEHAS_OUTSOURCE_TIMEOUT, how long a job
//! may go without a contractor's check-in; TEHAS_OUTSOURCE_PROXY_IDLE, how long one
//! of its model streams may go silent; TEHAS_OUTSOURCE_CONTRACTORS, the names
//! the job channel answers. The instance's models.json names the
//! endpoint and the variable of its key of the provider the instance's agents
//! run on, which is the endpoint a cycle's container is given too. The key is
//! read from this process's environment, written into one request at a time,
//! and never written anywhere else: a contractor is never told it.
//!
//! The same listener answers the job channel (src/channel.rs): a contractor's
//! join, its hello and its ask for work by its key, and a running job's
//! check-in, token renewal, relayed issue writes, result and stop by the job's
//! token, which is issued when the job is taken. It also answers the egress
//! tunnel (src/egress.rs) by the job's model token, to the hosts of
//! TEHAS_EGRESS_HOSTS and the base list, never to a model endpoint, and the
//! downloads (src/download.rs): the sandbox image PI_ENSEMBLE_IMAGE names by
//! the contractor's key, and by the job token the code index, a git bundle of
//! TEHAS_WORK_REPO's tip of the branch, and the issue's files when
//! TEHAS_WORK_IMAGES or TEHAS_WORK_FILES gives a cycle any.
//!
//! The job tokens are this process's own, held in memory. A contractor is
//! given its job's two tokens in the answer to the channel's `job` call: the
//! job token for the channel and the model token for its container. The
//! control socket in the state directory can issue a model token too, written
//! to a job directory, and the asking process is told the path and nothing of
//! the token. A token is never in a command line, never in a log, never in the
//! store: the store keeps their sha256 in the job's row, so a proxy that
//! restarts still takes the tokens of a job that is going, each in its scope.
//!
//! Exit: 0 the feature is off, or the word was done; 78 a setting is missing or
//! malformed, the endpoint is no address this proxy can speak to, or TEHAS_HOME
//! is too long a path for its control socket; 1 it cannot listen, the proxy it
//! asked is not there, or no token could be made.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread::Builder;
use std::time::Duration;

use tehas_core::models::{self, ModelFile};
use tehas_core::{out, outln};
use tehas_proxy::forward::Upstream;
use tehas_proxy::jobs::Issuer;
use tehas_proxy::log;
use tehas_proxy::records;
use tehas_proxy::serve::{self, Serve};

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

fn on() -> bool {
    tehas_core::features::on("outsource")
}

fn seconds(key: &str, fallback: u64) -> u64 {
    env(key).trim().parse().unwrap_or(fallback)
}

/// The tehas home, whose state store holds the factory's records. tehas_load
/// exports it; a proxy started without it stops rather than guess one.
fn home() -> PathBuf {
    let home = env("TEHAS_HOME");
    if home.is_empty() {
        stop(
            "TEHAS_HOME is not set: the proxy has no state store to read",
            78,
        );
    }
    PathBuf::from(home)
}

/// Where the proxy keeps its own things in the state: the socket of its control
/// line, and the directory a job's token is written into.
fn outsource() -> PathBuf {
    home().join("state").join("outsource")
}

/// The control socket: this user's own, in the state directory, and the only
/// way a token leaves this process.
fn control() -> PathBuf {
    outsource().join("proxy.sock")
}

fn stop(why: &str, code: i32) -> ! {
    eprintln!("tehas-proxy: {why}");
    std::process::exit(code)
}

fn usage(why: &str) -> ! {
    eprintln!("tehas-proxy: {why}");
    eprintln!("usage: tehas-proxy [--issue <job> <attempt> <contractor> | --revoke <contractor> | --forget <job>... | --counts | --stop]");
    std::process::exit(78)
}

/// The endpoint and the key of the provider the instance's agents run on, as
/// models.json writes them and the environment holds the key. A contractor's
/// model is the factory's model: the proxy asks one endpoint for every job and
/// no contractor is told where a request went.
fn endpoint() -> Result<(Upstream, String), String> {
    let provider = env("PI_ENSEMBLE_SUBAGENT_PROVIDER");
    let file = ModelFile::read(&models::host_path());
    let url = file.url(&provider, "main");
    let named = file.key(&provider, "main");
    let key = named
        .strip_prefix('$')
        .map(|name| {
            let name = name.strip_prefix('{').unwrap_or(name);
            env(name.strip_suffix('}').unwrap_or(name))
        })
        .unwrap_or_else(|| named.clone());
    let upstream = Upstream::of(&url)
        .ok_or_else(|| format!("the model endpoint of models.json is no http address: {url}"))?;
    if key.is_empty() {
        return Err(format!("the model key {named} of {provider} is not set"));
    }
    Ok((upstream, key))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => listen(),
        Some("--issue") => ask(&args),
        Some("--revoke") => ask(&args),
        Some("--forget") => ask(&args),
        Some("--counts") => ask(&args),
        Some("--stop") => ask(&args),
        Some("--help" | "-h") => usage("the model proxy of outsource"),
        Some(other) => usage(&format!("{other} is no command of it")),
    }
}

/// Start the listener, or say why the proxy is not there. The words of an
/// instance that has the feature off are `serve::declined`'s, and the command
/// ends 0: the unit is stopped, nothing listens, and status says no word.
fn listen() -> ! {
    let at = env("TEHAS_OUTSOURCE_PROXY_ADDR");
    let timeout = seconds("TEHAS_OUTSOURCE_TIMEOUT", 600);
    let idle = seconds("TEHAS_OUTSOURCE_PROXY_IDLE", 900);
    let (endpoint, key) = match endpoint() {
        Ok(pair) => pair,
        Err(why) if on() => stop(&why, 78),
        Err(_) => stop(serve::OFF, 0),
    };
    if let Some(why) = serve::declined(
        on(),
        &at,
        &endpoint.address(),
        Duration::from_secs(idle),
        timeout,
    ) {
        stop(&why, if on() { 78 } else { 0 });
    }
    let issuer = Box::leak(Box::new(Issuer::backed(std::sync::Arc::new(
        records::KeptIn::at(&home()),
    ))));
    let store = Box::leak(Box::new(records::OfStore::at(&home(), issuer)));
    // The two lists of the factory and this process's own tokens, asked
    // together for every request.
    let records = Box::leak(Box::new(records::OfProxy::of(store, issuer)));
    // One value of the rules, made now and read by every request of the
    // process: the listener's threads borrow it for the life of the program,
    // which is the life the borrow needs.
    // A relayed issue write is masked with the factory's own host names too,
    // as the local relay masks with the ones the console masks.
    let _ = tehas_core::mask::set_private(&tehas_core::mask::host_names(&models::host_path(), ""));
    let factory = Box::leak(Box::new(tehas_proxy::host::OfHost {
        home: home(),
        repo: env("TEHAS_REPO"),
        addr: at.clone(),
        timeout: i64::try_from(timeout).unwrap_or(i64::MAX),
        contractors: env("TEHAS_OUTSOURCE_CONTRACTORS")
            .split_whitespace()
            .map(str::to_string)
            .collect(),
    }));
    // The egress tunnel takes the in-house list, less the model endpoints.
    let egress = Box::leak(Box::new(tehas_proxy::egress::Egress::of_host(
        &env("TEHAS_EGRESS_HOSTS"),
        &ModelFile::read(&models::host_path()),
        Duration::from_secs(idle),
    )));
    let downloads = Box::leak(Box::new(tehas_proxy::download::OfHome {
        home: home(),
        checkout: PathBuf::from(env("TEHAS_WORK_REPO")),
        branch: tehas_core::settings::branch().unwrap_or_default(),
        image: env("PI_ENSEMBLE_IMAGE"),
        docker: PathBuf::from("docker"),
        zstd: on_path("zstd"),
        gives_files: env("TEHAS_WORK_IMAGES") == "1" || env("TEHAS_WORK_FILES") == "1",
        fetch: tehas_cycle::gives::for_job,
        making: Default::default(),
    }));
    let channel = Box::leak(Box::new(
        tehas_proxy::channel::Channel::new(issuer, factory)
            .with_egress(egress)
            .with_downloads(downloads),
    ));
    let proxy: &'static Serve<'static> = Box::leak(Box::new(Serve {
        endpoint,
        key,
        records,
        sink: &log::Journal,
        idle: Duration::from_secs(idle),
        channel: Some(channel),
    }));
    // The control socket is made before the port is bound: a proxy that cannot
    // answer for a token must not be answering model requests either.
    if let Some(why) = serve::socket_path_refused(&control()) {
        stop(&why, 78);
    }
    let socket = match control_listen() {
        Ok(socket) => socket,
        Err(why) => stop(&why, 1),
    };
    eprintln!("{}", serve::started(&at, Duration::from_secs(idle)));
    // The control line runs on a thread of its own and the listener holds the
    // process: it is what makes the proxy a task of its own rather than a step
    // of a poll.
    let control_issuer = issuer.shared();
    let spawned = Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || control_loop(socket, &control_issuer));
    if let Err(e) = spawned {
        stop(&format!("the proxy cannot start its control line: {e}"), 1);
    }
    if let Err(why) = serve::listen(proxy, &at) {
        stop(&why, 1);
    }
    std::process::exit(0)
}

/// Whether a program of this name is on PATH.
fn on_path(name: &str) -> bool {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .any(|dir| dir.join(name).is_file())
}

/// Answer the control socket, a few lines at a time, for the life of the
/// process.
fn control_loop(socket: UnixListener, issuer: &Issuer) {
    let line = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(CONTROL_HANDS));
    for accepted in socket.incoming() {
        let Ok(stream) = accepted else { continue };
        let issuer = issuer.shared();
        let line = std::sync::Arc::clone(&line);
        // A few lines at once is more than a host's start-up can want; one over
        // it waits for nothing, since the asking process times out by itself.
        if line
            .try_update(
                std::sync::atomic::Ordering::Acquire,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_sub(1),
            )
            .is_err()
        {
            drop(stream);
            continue;
        }
        Builder::new()
            .stack_size(128 * 1024)
            .spawn(move || {
                control_one(stream, &issuer);
                line.fetch_add(1, std::sync::atomic::Ordering::Release);
            })
            .ok();
    }
}

/// The control line's answer when no token could be made.
const NO_TOKEN: &str = "failed: no randomness to make a token of";

/// How many lines of the control line the proxy answers at once.
const CONTROL_HANDS: usize = 4;

/// The control socket of a running proxy. A socket left by a proxy that did not
/// end cleanly is this proxy's to take over: it is the port that decides which
/// proxy a host has, and no second proxy binds it.
fn control_listen() -> Result<UnixListener, String> {
    let path = control();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("the proxy cannot make {dir:?}: {e}"))?;
    }
    let _ = std::fs::remove_file(&path);
    UnixListener::bind(&path)
        .map_err(|e| format!("the proxy cannot make its control socket at {path:?}: {e}"))
}

/// One line of the control socket. A token is written to the socket and to no
/// log; the asking process is the only one that can read it.
fn control_one(stream: UnixStream, issuer: &Issuer) {
    let Ok(reader) = stream.try_clone() else {
        return;
    };
    let mut writer = stream;
    let Ok(Some(line)) = BufReader::new(reader).lines().next().transpose() else {
        return;
    };
    let mut words = line.split_whitespace();
    let ask = words.next().unwrap_or("");
    let count = |w: Option<&str>| w.and_then(|w| w.parse::<i64>().ok());
    match ask {
        "issue" => {
            let (job, attempt, contractor) =
                (count(words.next()), count(words.next()), words.next());
            let (Some(job), Some(attempt), Some(contractor)) = (job, attempt, contractor) else {
                let _ = writeln!(writer, "issue wants a job, an attempt and a contractor");
                return;
            };
            match issuer.issue_held(job, attempt, contractor, &mut tehas_proxy::jobs::random) {
                Some(token) => {
                    let _ = writeln!(writer, "{token}");
                }
                None => {
                    let _ = writeln!(writer, "{NO_TOKEN}");
                }
            }
        }
        "forget" => {
            let jobs: Vec<i64> = words.filter_map(|w| w.parse().ok()).collect();
            let _ = writeln!(writer, "{}", issuer.forget_jobs(&jobs));
        }
        "revoke" => {
            let Some(contractor) = words.next() else {
                let _ = writeln!(writer, "revoke wants a contractor");
                return;
            };
            let _ = writeln!(writer, "{}", issuer.forget_contractor(contractor));
        }
        "counts" => {
            for row in serve::request_rows(&issuer.take_counts()) {
                let _ = writeln!(writer, "{}", row.join("\t"));
            }
        }
        "stop" => {
            let _ = writeln!(writer, "bye");
            std::process::exit(0);
        }
        other => {
            let _ = writeln!(writer, "{other} is no word of the control socket");
        }
    }
}

/// Ask the proxy that is running: a job's token, written where the process that
/// starts its container reads it; or the words that end a contractor's tokens;
/// or what the proxy has counted. The token never passes through a command line
/// of anyone, and the caller is told the path it went to and nothing of it.
fn ask(args: &[String]) -> ! {
    let word = match args.first().map(String::as_str) {
        Some("--issue") => {
            let rest = &args[1..];
            if rest.len() < 3 {
                usage("issue wants a job, an attempt and a contractor");
            }
            format!("issue {} {} {}\n", rest[0], rest[1], rest[2])
        }
        Some("--revoke") => format!("revoke {}\n", args.get(1).unwrap_or(&String::new())),
        Some("--forget") => format!("forget {}\n", args[1..].join(" ")),
        Some("--counts") => "counts\n".to_string(),
        Some("--stop") => "stop\n".to_string(),
        _ => usage("ask wants a command of it"),
    };
    if !on() {
        stop(serve::OFF, 0);
    }
    let path = control();
    let said = exchange(&path, &word).unwrap_or_else(|why| {
        stop(
            &format!("no model proxy is running at {}: {why}", path.display()),
            1,
        )
    });
    if said.contains("no word of") || said.contains("wants") {
        stop(&said, 78);
    }
    if said.starts_with("failed:") {
        stop(said.trim(), 1);
    }
    if let Some(token) = word.strip_prefix("issue ") {
        // The token came back: write it where the container's models.json will
        // be made from, and name the path and nothing of the token.
        let Some(job) = token.split_whitespace().next() else {
            stop("issue wants a job", 78)
        };
        let dir = outsource().join("jobs").join(job);
        match serve::write_token(&dir, said.trim()) {
            Ok(path) => {
                // The path is what the starting process is told, never the
                // token itself.
                outln!("{}", path.display());
                std::process::exit(0)
            }
            Err(e) => stop(
                &format!("the token could not be written to {dir:?}: {e}"),
                1,
            ),
        }
    }
    out!("{said}");
    std::process::exit(0)
}

/// One line to the control socket and the answer to it.
fn exchange(path: &PathBuf, line: &str) -> std::io::Result<String> {
    let mut socket = UnixStream::connect(path)?;
    socket.set_read_timeout(Some(Duration::from_secs(20)))?;
    socket.set_write_timeout(Some(Duration::from_secs(20)))?;
    socket.write_all(line.as_bytes())?;
    socket.flush()?;
    let mut said = String::new();
    BufReader::new(socket).read_line(&mut said)?;
    Ok(said)
}
