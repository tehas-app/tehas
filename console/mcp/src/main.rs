//! tehas-mcp: the binary of the read-only MCP server (lib.rs).
//!
//!   tehas-mcp --live <dir> --listen <addr:port> --server-version <v>
//!
//! The keys come from TEHAS_MCP_KEYS in the environment, never from an
//! argument. One line per request on stderr names the key by fingerprint.
//! Exit 78 when the keys or the arguments are wrong, 1 when it cannot listen.

use mcp::keys::parse_keys;
use mcp::rpc::{handle, Request, MAX_BODY};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

const WORKERS: usize = 4;

fn usage(why: &str) -> ! {
    eprintln!("tehas-mcp: {why}");
    eprintln!("usage: tehas-mcp --live <dir> --listen <addr:port> --server-version <v>");
    std::process::exit(78);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut live, mut listen, mut version) = (None, None, None);
    while let Some(a) = args.next() {
        let slot = match a.as_str() {
            "--live" => &mut live,
            "--listen" => &mut listen,
            "--server-version" => &mut version,
            other => usage(&format!("unknown argument {other}")),
        };
        *slot = Some(
            args.next()
                .unwrap_or_else(|| usage(&format!("{a} needs a value"))),
        );
    }
    let live = PathBuf::from(live.unwrap_or_else(|| usage("--live is missing")));
    let listen = listen.unwrap_or_else(|| usage("--listen is missing"));
    let version = version.unwrap_or_else(|| usage("--server-version is missing"));
    let keys = match parse_keys(&std::env::var("TEHAS_MCP_KEYS").unwrap_or_default()) {
        Ok(k) => k,
        Err(why) => usage(&why),
    };

    let server = match tiny_http::Server::http(&listen) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("tehas-mcp: cannot listen on {listen}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!(
        "tehas-mcp: {version} listening on {listen}, {} keys",
        keys.len()
    );
    let shared = Arc::new((keys, live, version));
    let workers: Vec<_> = (0..WORKERS)
        .map(|_| {
            let server = Arc::clone(&server);
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || {
                for req in server.incoming_requests() {
                    serve(req, &shared.0, &shared.1, &shared.2);
                }
            })
        })
        .collect();
    for w in workers {
        let _ = w.join();
    }
}

fn serve(
    mut req: tiny_http::Request,
    keys: &mcp::keys::Keys,
    live: &std::path::Path,
    version: &str,
) {
    let started = Instant::now();
    let method = req.method().as_str().to_string();
    let headers: Vec<(String, String)> = req
        .headers()
        .iter()
        .map(|h| {
            (
                h.field.as_str().as_str().to_string(),
                h.value.as_str().to_string(),
            )
        })
        .collect();
    // One byte more than allowed is enough to know a body is too big.
    let mut body = Vec::new();
    let _ = req
        .as_reader()
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut body);
    let r = handle(
        &Request {
            method: &method,
            headers,
            body: &body,
        },
        keys,
        live,
        version,
    );
    let mut resp = tiny_http::Response::from_data(r.body).with_status_code(r.status);
    for (k, v) in &r.headers {
        if let Ok(h) = tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()) {
            resp.add_header(h);
        }
    }
    eprintln!(
        "{method} {} {}ms key={}",
        r.status,
        started.elapsed().as_millis(),
        r.key.as_deref().unwrap_or("-")
    );
    let _ = req.respond(resp);
}
