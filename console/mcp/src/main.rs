//! tehas-mcp: the binary of the read-only MCP server (lib.rs).
//!
//!   tehas-mcp --live <dir> --sessions <dir> --logs <dir> --listen <addr:port>
//!             --server-version <v> [--io-timeout <seconds>] [--private <name>]...
//!
//! A log is masked as the console masks it, the names given with --private
//! included. A name is not a secret. The host's secret values come as salted
//! sums in TEHAS_MCP_MASK (mask::sums_line), so a value printed bare is masked
//! by a server that never holds it.
//! The keys come from TEHAS_MCP_KEYS and TEHAS_MCP_DASHBOARD_KEYS in the
//! environment, never from an argument: the second list holds the keys that
//! are served only the four tools of `mcp::tools::DASHBOARD`. One of the two
//! must hold a key, and no key may be in both. One line per request on stderr
//! names the key by fingerprint and the key's scope.
//! Exit 78 when the keys or the arguments are wrong, 1 when it cannot listen.
//!
//!   tehas-mcp egress --listen <addr:port> [--allow <host:port>]...
//!
//! The egress proxy of the sandbox containers instead (mcp::egress), with the
//! same exit statuses.

use mcp::keys::{parse_keys, Keys, Setting};
use mcp::tools::Dirs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Connections served at once; one more is answered 503 and closed. Each is a
/// thread, and the container allows 64 processes.
const MAX_CONNECTIONS: usize = 32;

/// The raw value of a setting's list: nothing where the environment has it
/// not. The name it is read under is the setting's own. The two values live
/// as long as the process: the keys are read once, at start-up.
fn of(setting: Setting) -> (&'static Setting, &'static str) {
    let setting = Box::leak(Box::new(setting));
    let list = std::env::var(setting.name()).unwrap_or_default();
    (setting, &*Box::leak(list.into_boxed_str()))
}

fn usage(why: &str) -> ! {
    eprintln!("tehas-mcp: {why}");
    eprintln!(
        "usage: tehas-mcp --live <dir> --sessions <dir> --logs <dir> --listen <addr:port> --server-version <v> [--io-timeout <seconds>] [--private <name>]..."
    );
    std::process::exit(78);
}

// A place among the connections, given back when its thread ends, a panic
// included.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `tehas-mcp egress`: the proxy of mcp::egress, for good.
fn egress(args: &[String]) -> ! {
    let fail = |why: String| -> ! {
        eprintln!("tehas-mcp egress: {why}");
        eprintln!("usage: tehas-mcp egress --listen <addr:port> [--allow <host:port>]...");
        std::process::exit(78);
    };
    let (mut listen, mut allow) = (None, Vec::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let v = it
            .next()
            .unwrap_or_else(|| fail(format!("{a} needs a value")))
            .clone();
        match a.as_str() {
            "--listen" => listen = Some(v),
            "--allow" => allow.push(v),
            other => fail(format!("unknown argument {other}")),
        }
    }
    let listen = listen.unwrap_or_else(|| fail("--listen is missing".to_string()));
    let allowed = mcp::egress::Allowed::parse(&allow).unwrap_or_else(|why| fail(why));
    let listener = TcpListener::bind(&listen).unwrap_or_else(|e| {
        eprintln!("tehas-mcp egress: cannot listen on {listen}: {e}");
        std::process::exit(1);
    });
    eprintln!(
        "tehas-mcp egress: listening on {listen}, {} hosts allowed",
        allowed.len()
    );
    mcp::egress::serve(listener, allowed);
    std::process::exit(1);
}

fn main() {
    let all: Vec<String> = std::env::args().skip(1).collect();
    if all.first().map(String::as_str) == Some("egress") {
        egress(&all[1..]);
    }
    let mut args = all.into_iter();
    let (mut live, mut sessions, mut logs) = (None, None, None);
    let (mut listen, mut version) = (None, None);
    let mut io_timeout: Option<String> = None;
    let mut private = Vec::new();
    while let Some(a) = args.next() {
        if a == "--private" {
            private.push(
                args.next()
                    .unwrap_or_else(|| usage("--private needs a name")),
            );
            continue;
        }
        let slot = match a.as_str() {
            "--live" => &mut live,
            // The sessions directory holds both files the private reads are
            // made from: the snapshot and the outcomes counts.
            "--sessions" => &mut sessions,
            "--logs" => &mut logs,
            "--listen" => &mut listen,
            "--server-version" => &mut version,
            "--io-timeout" => &mut io_timeout,
            other => usage(&format!("unknown argument {other}")),
        };
        *slot = Some(
            args.next()
                .unwrap_or_else(|| usage(&format!("{a} needs a value"))),
        );
    }
    // The deadline for the whole of a request, an argument so that a test cuts
    // a trickling client off in a second. `tehasbot mcp serve` never passes the flag,
    // so the installed server keeps mcp::http::IO_TIMEOUT, which a test of
    // serve.rs asserts. The bound keeps a 0 from meaning no deadline.
    let io_timeout = match io_timeout {
        None => mcp::http::IO_TIMEOUT,
        Some(secs) => match secs.parse::<u64>() {
            Ok(secs) if (1..=3600).contains(&secs) => Duration::from_secs(secs),
            _ => usage("--io-timeout is a whole number of seconds from 1 to 3600"),
        },
    };
    let dir = |v: Option<String>, name: &str| {
        PathBuf::from(v.unwrap_or_else(|| usage(&format!("{name} is missing"))))
    };
    // The host keeps the outcomes counts beside the snapshot and gives the
    // container that one directory, so it is asked for no second one.
    let sessions_dir = dir(sessions, "--sessions");
    let dirs = Dirs {
        live: dir(live, "--live"),
        sessions: sessions_dir,
        logs: dir(logs, "--logs"),
    };
    let listen = listen.unwrap_or_else(|| usage("--listen is missing"));
    let version = version.unwrap_or_else(|| usage("--server-version is missing"));
    if let Err(why) = tehas_core::mask::set_private(&private) {
        usage(&why);
    }
    // The sums of the host's secret values, which the host gives by name. A
    // server run without it masks by the patterns and its own settings alone.
    if let Some(line) = std::env::var_os("TEHAS_MCP_MASK") {
        let set = line
            .to_str()
            .ok_or("is not text".to_string())
            .and_then(tehas_core::mask::set_sums);
        if let Err(why) = set {
            usage(&format!("TEHAS_MCP_MASK: {why}"));
        }
    }
    let keys = match parse_keys(Setting::ALL.map(of)) {
        Ok(k) => k,
        Err(why) => usage(&why),
    };

    let listener = match TcpListener::bind(&listen) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("tehas-mcp: cannot listen on {listen}: {e}");
            std::process::exit(1);
        }
    };
    let bound = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or(listen);
    eprintln!(
        "tehas-mcp: {version} listening on {bound}, {} keys",
        keys.len()
    );
    let shared: Arc<(Keys, Dirs, String, Duration)> = Arc::new((keys, dirs, version, io_timeout));
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::SeqCst);
            mcp::http::refuse(stream, 503);
            continue;
        }
        let slot = Slot(Arc::clone(&active));
        let shared = Arc::clone(&shared);
        // A thread that cannot start drops its closure, and the slot with it.
        let _ = std::thread::Builder::new().spawn(move || {
            let _slot = slot;
            let line = mcp::http::serve(stream, &shared.0, &shared.1, &shared.2, shared.3);
            eprintln!("{line}");
        });
    }
}
