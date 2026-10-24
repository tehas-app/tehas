//! tehas-mcp: the binary of the read-only MCP server (lib.rs).
//!
//!   tehas-mcp --live <dir> --sessions <dir> --logs <dir> --listen <addr:port>
//!             --server-version <v> [--private <name>]...
//!
//! A log is masked as the console masks it, the names given with --private
//! included. A name is not a secret.
//! The keys come from TEHAS_MCP_KEYS in the environment, never from an
//! argument. One line per request on stderr names the key by fingerprint.
//! Exit 78 when the keys or the arguments are wrong, 1 when it cannot listen.

use mcp::keys::{parse_keys, Keys};
use mcp::tools::Dirs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Connections served at once; one more is answered 503 and closed. Each is a
/// thread, and the container allows 64 processes.
const MAX_CONNECTIONS: usize = 32;

fn usage(why: &str) -> ! {
    eprintln!("tehas-mcp: {why}");
    eprintln!(
        "usage: tehas-mcp --live <dir> --sessions <dir> --logs <dir> --listen <addr:port> --server-version <v> [--private <name>]..."
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

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut live, mut sessions, mut logs) = (None, None, None);
    let (mut listen, mut version) = (None, None);
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
            "--sessions" => &mut sessions,
            "--logs" => &mut logs,
            "--listen" => &mut listen,
            "--server-version" => &mut version,
            other => usage(&format!("unknown argument {other}")),
        };
        *slot = Some(
            args.next()
                .unwrap_or_else(|| usage(&format!("{a} needs a value"))),
        );
    }
    let dir = |v: Option<String>, name: &str| {
        PathBuf::from(v.unwrap_or_else(|| usage(&format!("{name} is missing"))))
    };
    let dirs = Dirs {
        live: dir(live, "--live"),
        sessions: dir(sessions, "--sessions"),
        logs: dir(logs, "--logs"),
    };
    let listen = listen.unwrap_or_else(|| usage("--listen is missing"));
    let version = version.unwrap_or_else(|| usage("--server-version is missing"));
    if let Err(why) = tehas_core::mask::set_private(&private) {
        usage(&why);
    }
    let keys = match parse_keys(&std::env::var("TEHAS_MCP_KEYS").unwrap_or_default()) {
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
    let shared: Arc<(Keys, Dirs, String)> = Arc::new((keys, dirs, version));
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
            let line = mcp::http::serve(stream, &shared.0, &shared.1, &shared.2);
            eprintln!("{line}");
        });
    }
}
