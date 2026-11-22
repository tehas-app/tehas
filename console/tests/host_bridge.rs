//! The contract of the host bridge, /host/bridge.json:
//! api/host-bridge.schema.json and its two samples, api/host-bridge.full.json
//! and api/host-bridge.bare.json, the docs server's 404 under /host/, and the
//! pages that read it. What the reading module makes of an answer is in
//! test/host_bridge_test.mjs. See "The host bridge" in host/docs/api.md.
//! Moved from test/host_bridge_test.sh.

mod common;

use common::*;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tehas_core::testdir::TestDir;

fn text(rel: &str) -> String {
    let p = root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

const SCHEMA: &str = "api/host-bridge.schema.json";

/// The text of api/host-bridge.<name>.json.
fn sample(name: &str) -> String {
    text(&format!("api/host-bridge.{name}.json"))
}

/// The lines test/schema.jq prints for a JSON text: "" when it fits.
fn check(json: &str) -> String {
    schema_errors("host-bridge", json)
}

/// How many objects of a JSON text, at any depth, hold `key`.
fn objects_with(json: &str, key: &str) -> usize {
    let mut stack = vec![json.parse::<Value>().unwrap()];
    let mut n = 0;
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(m) => {
                n += usize::from(m.contains_key(key));
                stack.extend(m.into_iter().map(|(_, x)| x));
            }
            Value::Array(a) => stack.extend(a),
            _ => {}
        }
    }
    n
}

/// The dotted paths of the properties a schema names, an array's items walked
/// as its element.
fn schema_paths(json: &str) -> Vec<String> {
    let mut stack = vec![(String::new(), json.parse::<Value>().unwrap())];
    let mut out = vec![];
    while let Some((prefix, s)) = stack.pop() {
        let Some(props) = s.get("properties").and_then(Value::as_object) else {
            continue;
        };
        for (key, sub) in props {
            let q = format!("{prefix}{key}");
            out.push(q.clone());
            stack.push((format!("{q}."), sub.get("items").unwrap_or(sub).clone()));
        }
    }
    out
}

/// The dotted paths of the fields a JSON text holds, array indexes left out.
fn held_paths(json: &str) -> Vec<String> {
    let mut stack = vec![(String::new(), json.parse::<Value>().unwrap())];
    let mut out = vec![];
    while let Some((path, v)) = stack.pop() {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    let q = if path.is_empty() {
                        k
                    } else {
                        format!("{path}.{k}")
                    };
                    out.push(q.clone());
                    stack.push((q, x));
                }
            }
            Value::Array(a) => stack.extend(a.into_iter().map(|x| (path.clone(), x))),
            _ => {}
        }
    }
    out
}

/// How many lines of the files hold any of `words`, as `grep -c` adds them.
fn lines_holding(files: &[PathBuf], words: &[&str]) -> usize {
    files
        .iter()
        .map(|f| {
            fs::read_to_string(f)
                .unwrap_or_else(|e| panic!("{}: {e}", f.display()))
                .lines()
                .filter(|l| words.iter().any(|w| l.contains(w)))
                .count()
        })
        .sum()
}

fn at(rels: &[&str]) -> Vec<PathBuf> {
    rels.iter().map(|r| root().join(r)).collect()
}

/// The files of a directory whose names end in one of `ends`.
fn files_ending(dir: &Path, ends: &[&str]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file() && ends.iter().any(|e| p.to_string_lossy().ends_with(e)))
        .collect();
    v.sort();
    v
}

fn on_path(name: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|d| Path::new(d).join(name).is_file())
}

#[test]
fn the_schema_is_json_schema_2020_12_and_allows_unknown_fields_at_the_top_and_in_each_element() {
    let s = text(SCHEMA);
    assert_eq!(
        s.parse::<Value>().unwrap()["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(objects_with(&s, "additionalProperties"), 0);
    let mut more = sample("full").parse::<Value>().unwrap();
    more["later"] = json!(1);
    more["ticketNotes"][0]["later"] = json!(1);
    more["menu"][0]["later"] = json!({});
    assert_eq!(check(&more.to_string()), "");
}

#[test]
fn api_host_bridge_name_json_fits_the_schema() {
    for name in ["full", "bare"] {
        assert_eq!(check(&sample(name)), "", "api/host-bridge.{name}.json");
    }
}

#[test]
fn the_bare_sample_holds_only_bridge_host_and_time_the_full_one_every_field_of_the_schema() {
    let bare = sample("bare").parse::<Value>().unwrap();
    let keys: Vec<&String> = bare.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["bridge", "host", "time"]);
    let paths = schema_paths(&text(SCHEMA));
    let held = held_paths(&sample("full"));
    assert!(
        paths.contains(&"ticketNotes.issue".to_string()),
        "{paths:?}"
    );
    let missing: Vec<&String> = paths.iter().filter(|p| !held.contains(p)).collect();
    assert!(missing.is_empty(), "the full sample lacks {missing:?}");
}

#[test]
fn the_check_finds_a_bad_version_a_missing_host_a_bad_time_and_a_ticket_link_without_n() {
    let mut bad = sample("full").parse::<Value>().unwrap();
    bad["bridge"] = json!(2);
    bad.as_object_mut().unwrap().remove("host");
    bad["time"] = json!("noon");
    bad["ticketLink"] = json!("/host/ticket");
    let out = check(&bad.to_string());
    has(&out, ".bridge: 2, not 1");
    has(&out, ".: host is missing");
    has(&out, r#".time: "noon" is not a date-time"#);
    has(&out, r#".ticketLink: "/host/ticket" does not match"#);
}

#[test]
fn the_check_finds_a_ticket_link_off_host_or_with_n_twice_and_a_host_too_long() {
    for link in ["https://x.example/{n}", "/host/{n}/{n}", "/other/{n}"] {
        let mut bad = sample("full").parse::<Value>().unwrap();
        bad["ticketLink"] = json!(link);
        has(
            &check(&bad.to_string()),
            &format!(r#".ticketLink: "{link}" does not match"#),
        );
    }
    let mut bad = sample("full").parse::<Value>().unwrap();
    bad["host"] = json!("h".repeat(41));
    has(&check(&bad.to_string()), ".host: longer than 40");
    bad["host"] = json!("two\nlines");
    has(
        &check(&bad.to_string()),
        r#".host: "two\nlines" does not match"#,
    );
}

#[test]
fn the_check_finds_each_elements_breach() {
    let mut bad = sample("full").parse::<Value>().unwrap();
    bad["ticketNotes"][0]["issue"] = json!(0);
    bad["ticketNotes"][1]["issue"] = json!(1000000000u64);
    bad["ticketNotes"][2]["text"] = json!("t".repeat(61));
    bad["ticketNotes"][2]["link"] = json!("javascript:1");
    let out = check(&bad.to_string());
    has(&out, ".ticketNotes[0].issue: 0 is under 1");
    has(&out, ".ticketNotes[1].issue: 1000000000 is over 999999999");
    has(&out, ".ticketNotes[2].text: longer than 60");
    has(
        &out,
        r#".ticketNotes[2].link: "javascript:1" does not match"#,
    );
}

#[test]
fn the_check_finds_a_menu_item_that_opens_neither_in_the_dock_nor_as_a_page() {
    let mut bad = sample("full").parse::<Value>().unwrap();
    bad["menu"][0]["open"] = json!("frame");
    has(
        &check(&bad.to_string()),
        r#".menu[0].open: "frame" is not one of ["dock","page"]"#,
    );
}

#[test]
fn the_check_finds_more_notes_than_a_list_holds() {
    let mut bad = sample("full").parse::<Value>().unwrap();
    bad["ticketNotes"] = (1..=201)
        .map(|n| json!({"issue": n, "text": "x"}))
        .collect();
    has(
        &check(&bad.to_string()),
        ".ticketNotes: more than 200 items",
    );
}

#[test]
fn an_answer_that_still_carries_tokens_fits_the_schema_the_field_is_unknown_now_and_ignored() {
    let mut old = sample("full").parse::<Value>().unwrap();
    old["tokens"] =
        json!({"days": [{"date": "7 Nov", "input": -1}], "models": "x", "unknown": 1.5});
    assert_eq!(check(&old.to_string()), "");
    assert!(text(SCHEMA).parse::<Value>().unwrap()["properties"]
        .get("tokens")
        .is_none());
}

#[test]
fn the_business_front_page_draws_no_token_table() {
    let pages = at(&[
        "host/fun/business.html",
        "host/fun/business.css",
        "host/fun/business-view.js",
    ]);
    assert_eq!(
        lines_holding(&pages, &["host-tokens", "token-use", "token-table"]),
        0
    );
    assert_eq!(
        lines_holding(
            &at(&["host/fun/host-bridge.js"]),
            &["drawTokens", "tokensOf"]
        ),
        0
    );
}

#[test]
fn the_docs_server_answers_every_path_under_host_with_404_before_anything_else_it_serves() {
    let caddyfile = text("host/edge/docs.Caddyfile");
    has(&caddyfile, "@host path /host/*");
    has(&caddyfile, "respond @host 404");
    // The first thing in the route, so no handler below can answer under
    // /host/.
    let (mut r, mut h, mut f) = (0, 0, 0);
    for (i, line) in caddyfile.lines().enumerate() {
        let n = i + 1;
        if line.starts_with("    route {") {
            r = n;
        }
        if line.contains("respond @host 404") {
            h = n;
        }
        let handler = ["handle", "rewrite", "file_server"]
            .iter()
            .any(|w| line.contains(w));
        if handler && r > 0 && f == 0 {
            f = n;
        }
    }
    assert!(
        r > 0 && h > 0 && h < f,
        "the 404 is not first: route {r}, 404 {h}, first handler {f}"
    );
    if on_path("caddy") {
        let out = Command::new("caddy")
            .args(["validate", "--adapter", "caddyfile", "--config"])
            .arg(root().join("host/edge/docs.Caddyfile"))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// The status curl reads for `url`.
fn curl_code(url: &str) -> String {
    let out = Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", url])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// What the contract freezes: the host owns /host/, so a file put there is never
// served. Bare /host is left out: the manual's page "The host" is named so.
#[test]
fn the_docs_server_answers_404_for_any_path_under_host_even_where_a_file_stands() {
    if !(on_path("caddy") && on_path("curl")) {
        eprintln!("SKIP: no caddy or no curl here; the lines of the Caddyfile are checked above");
        return;
    }
    let t = TestDir::new("host-bridge-caddy");
    let srv = t.join("srv");
    for d in ["docs/host/ticket", "docs/host/x", "fun", "../xdg"] {
        fs::create_dir_all(srv.join(d)).unwrap();
    }
    // The docs server's own tree, /srv/live in its container: not a home.
    let served = t.join("srv/live");
    fs::create_dir_all(&served).unwrap();
    fs::write(served.join("status.json"), "{}\n").unwrap();
    fs::write(srv.join("docs/_layout.html"), "page\n").unwrap();
    fs::write(srv.join("docs/host.md"), "# The host\n").unwrap();
    for f in ["bridge.json", "index.html", "ticket/7", "x/y"] {
        fs::write(srv.join("docs/host").join(f), "tehas\n").unwrap();
    }
    let paths = [
        "/host/",
        "/host/bridge.json",
        "/host/ticket/7",
        "/host/x/y",
        "/host/index.html",
        "/host/.x",
        "/Host/bridge.json",
    ];
    let want: String = paths.iter().map(|p| format!("{p} 404 ")).collect();
    let caddyfile = text("host/edge/docs.Caddyfile");
    let log = t.join("caddy.log");
    let (mut codes, mut page, mut up) = (String::new(), String::new(), false);
    // A random port can be taken: caddy then exits on the bind, so try another.
    for try_n in 1..=5 {
        let port = 20000 + (std::process::id() as u64 * 7919 + try_n * 104729) % 20000;
        let config = caddyfile
            .replace("/srv/", &format!("{}/", srv.display()))
            .lines()
            .map(|l| {
                if l.starts_with(":8080 {") {
                    l.replacen(":8080 {", &format!(":{port} {{"), 1)
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let file = t.join("Caddyfile");
        fs::write(&file, config).unwrap();
        let mut caddy = Command::new("caddy")
            .args(["run", "--adapter", "caddyfile", "--config"])
            .arg(&file)
            .env("XDG_DATA_HOME", t.join("xdg"))
            .env("XDG_CONFIG_HOME", t.join("xdg"))
            .stdout(fs::File::create(&log).unwrap())
            .stderr(fs::File::create(t.join("caddy.err")).unwrap())
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let end = Instant::now() + Duration::from_secs(30);
        while Instant::now() < end {
            if caddy.try_wait().unwrap().is_some() {
                break;
            }
            if curl_code(&format!("{base}/live/status.json")) == "200" {
                up = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if up {
            for p in paths {
                codes.push_str(&format!("{p} {} ", curl_code(&format!("{base}{p}"))));
            }
            page = curl_code(&format!("{base}/host"));
        }
        let _ = caddy.kill();
        let _ = caddy.wait();
        let said = fs::read_to_string(&log).unwrap_or_default()
            + &fs::read_to_string(t.join("caddy.err")).unwrap_or_default();
        if up {
            break;
        }
        assert!(
            said.contains("address already in use") && try_n < 5,
            "caddy never answered status.json (try {try_n}): {said}"
        );
    }
    assert_eq!(codes, want);
    // The manual's page "The host" is /host, outside the host's namespace.
    assert_ne!(
        page, "404",
        "the docs server answered 404 for /host, the manual's page"
    );
}

#[test]
fn both_front_pages_and_dashboard_load_the_reading_module_before_the_views_and_tehas_registers_no_service_worker(
) {
    for page in [
        "host/fun/index.html",
        "host/fun/business.html",
        "host/docs/dashboard.frag",
    ] {
        let body = text(page);
        has(&body, r#"<script src="/fun/host-bridge.js"></script>"#);
        let (mut b, mut m) = (0, 0);
        for (i, line) in body.lines().enumerate() {
            if line.contains("host-bridge.js") {
                b = i + 1;
            }
            if line.contains(r#"type="module""#) && m == 0 {
                m = i + 1;
            }
        }
        assert!(
            b > 0 && b < m,
            "{page}: the module comes before the reading module"
        );
    }
    let fun = root().join("host/fun");
    let register =
        regex::Regex::new("serviceWorker.register|navigator.serviceWorker.register").unwrap();
    let registers: Vec<PathBuf> = files_ending(&fun, &[".js", ".html"])
        .into_iter()
        .filter(|f| register.is_match(&fs::read_to_string(f).unwrap()))
        .collect();
    assert!(registers.is_empty(), "{registers:?}");
    has(&text("host/fun/host-bridge.js"), "r.unregister()");
}

#[test]
fn an_answer_that_still_carries_notices_fits_the_schema_and_no_page_or_module_draws_them() {
    let mut old = sample("full").parse::<Value>().unwrap();
    old["notices"] = json!([{"id": "n-1", "level": "loud", "text": ""}]);
    assert_eq!(check(&old.to_string()), "");
    assert!(text(SCHEMA).parse::<Value>().unwrap()["properties"]
        .get("notices")
        .is_none());
    let pages = at(&[
        "host/fun/index.html",
        "host/fun/business.html",
        "host/fun/host-bridge.js",
        "host/fun/panels.css",
    ]);
    assert_eq!(
        lines_holding(&pages, &["host-notice", "drawNotices", "notices"]),
        0
    );
}

#[test]
fn tehas_api_lists_the_feature_host_bridge_and_no_longer_host_bridge_tokens_and_the_manual_names_it(
) {
    has(&text("console/src/api.rs"), r#""host-bridge","#);
    assert_eq!(
        lines_holding(
            &at(&["console/src/api.rs", "host/docs/api.md"]),
            &["host-bridge-tokens"]
        ),
        0
    );
    let manual = text("host/docs/api.md");
    has(&manual, "## The host bridge");
    has(
        &manual,
        "`host-bridge` ([the host bridge](#the-host-bridge))",
    );
}

#[test]
fn the_contract_freezes_the_host_namespace_and_names_the_tests_that_hold_it() {
    let contract = text("host/docs/contract.md");
    let row = contract
        .lines()
        .find(|l| l.starts_with("| Host namespace |"))
        .expect("contract.md has no row for the host namespace");
    for words in [
        "`/host/`",
        "never serves a path under it",
        "answers 404 there",
        "reads as no host",
        "`bridge: 1`",
        "`api/host-bridge.schema.json`",
        "`console/tests/host_bridge.rs`",
        "`test/json_contract_test.sh`",
        "`test/host_bridge_test.mjs`",
        "`test/panels_dock_test.mjs`",
    ] {
        has(row, words);
    }
    for f in [
        "console/tests/host_bridge.rs",
        "test/json_contract_test.sh",
        "test/host_bridge_test.mjs",
        "test/panels_dock_test.mjs",
    ] {
        assert!(
            root().join(f).is_file(),
            "the contract names {f}, which is not there"
        );
    }
}
