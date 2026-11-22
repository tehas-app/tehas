//! The gate's rules, one case each, with GitHub, the catalog and the runner
//! stubbed.

use super::catalog::Facts;
use super::github::Permission;
use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use tehas_core::testdir::TestDir;

const DOMAIN: &str = "example.org";
const P: &str = "plugin-floorplan.example.org";
const Q: &str = "plugin-specs.example.org";
const S: &str = "plugins.example.org";
const T0: u64 = 1_000_000;

#[derive(Default)]
struct Hub {
    // The level GitHub answers, None for 404; Err for an error.
    level: Option<Result<Option<Level>, String>>,
    public: bool,
    // The id the permission answer is about, when not the person's.
    other_id: Option<u64>,
    permission_calls: usize,
    token_calls: usize,
}

#[derive(Clone, Default)]
struct Stub(Arc<Mutex<Hub>>);

impl Stub {
    fn set(&self, f: impl FnOnce(&mut Hub)) {
        f(&mut lock(&self.0));
    }
    fn calls(&self) -> (usize, usize) {
        let h = lock(&self.0);
        (h.token_calls, h.permission_calls)
    }
}

impl GitHub for Stub {
    fn token(&self, code: &str) -> Result<String, String> {
        lock(&self.0).token_calls += 1;
        (code == "good")
            .then(|| "person-token".to_string())
            .ok_or("no".into())
    }
    fn user(&self, token: &str) -> Result<Person, String> {
        assert_eq!(token, "person-token");
        Ok(Person {
            id: 7,
            login: "alice".to_string(),
        })
    }
    fn permission(&self, login: &str) -> Result<Permission, String> {
        assert_eq!(login, "alice");
        let mut h = lock(&self.0);
        h.permission_calls += 1;
        let id = h.other_id.unwrap_or(7);
        match h.level.clone().unwrap_or(Ok(Some(Level::Write))) {
            Err(e) => Err(e),
            Ok(None) => Ok(Permission::None),
            Ok(Some(l)) => Ok(Permission::Of(id, Some(l), "write".into())),
        }
    }
    fn public(&self) -> Result<bool, String> {
        Ok(lock(&self.0).public)
    }
}

struct Books(HashMap<String, Facts>);

impl Catalog for Books {
    fn facts(&self, name: &str) -> Option<Facts> {
        self.0.get(name).cloned()
    }
}

#[derive(Clone, Default)]
struct Runner(Arc<Mutex<(Vec<String>, Option<Press>)>>);

impl Refresh for Runner {
    fn ask(&self, plugin: &str, action: &str, login: &str) -> Press {
        let mut r = lock(&self.0);
        r.0.push(format!("{plugin} {action} {login}"));
        r.1.clone().unwrap_or(Press::Asked)
    }
}

struct Fix {
    gate: Gate,
    hub: Stub,
    runner: Runner,
    dir: TestDir,
}

fn facts(kit: bool) -> Facts {
    Facts {
        view: true,
        kit,
        actions: vec!["refresh".to_string()],
        paths: vec![("/".to_string(), true)],
        keep: kit,
    }
}

fn fixture_with(access: Level, domain: &str, one_level: bool) -> Fix {
    let mut books = HashMap::new();
    books.insert("floorplan".to_string(), facts(true));
    books.insert("specs".to_string(), facts(false));
    fixture_of(access, domain, one_level, TestDir::new("gate"), |_| {
        Box::new(Books(books))
    })
}

/// A gate on `dir`, its catalog made by `catalog` from the home.
fn fixture_of(
    access: Level,
    domain: &str,
    one_level: bool,
    dir: TestDir,
    catalog: impl FnOnce(&std::path::Path) -> Box<dyn Catalog>,
) -> Fix {
    let view = dir.join("plugins/floorplan/view");
    std::fs::create_dir_all(view.join("sub")).unwrap();
    std::fs::write(view.join("index.html"), "<!doctype html><p>map</p>").unwrap();
    std::fs::write(view.join("sub/index.html"), "<p>sub</p>").unwrap();
    std::fs::write(view.join("app.js"), "console.log(1)").unwrap();
    std::fs::create_dir_all(view.join("_kit")).unwrap();
    std::fs::write(view.join("_kit/press.js"), "planted").unwrap();
    std::fs::write(view.join("_session"), "planted").unwrap();
    let kit = dir.join("fun/kit");
    std::fs::create_dir_all(kit.join("vendor")).unwrap();
    std::fs::write(
        kit.join("MANIFEST"),
        "kit 1\nfile aa 3 press.js\nfile bb 3 vendor/solid.js\nfile cc 3 vendor/solid.d.ts\n",
    )
    .unwrap();
    std::fs::write(kit.join("press.js"), "p()").unwrap();
    std::fs::write(kit.join("vendor/solid.js"), "s()").unwrap();
    std::fs::write(kit.join("island.js"), "not listed").unwrap();
    let catalog = catalog(&dir);
    let hub = Stub::default();
    let runner = Runner::default();
    let gate = Gate::new(
        Settings {
            domain: domain.to_string(),
            one_level,
            plugins: vec!["floorplan".into(), "specs".into()],
            access,
            home: dir.to_path_buf(),
            client_id: "Iv1.client".into(),
            web: "https://github.example".into(),
        },
        Box::new(hub.clone()),
        catalog,
        Box::new(runner.clone()),
    );
    Fix {
        gate,
        hub,
        runner,
        dir,
    }
}

fn fixture() -> Fix {
    fixture_with(Level::Write, DOMAIN, false)
}

/// A top-level navigation from another site, such as a link in a mail.
fn nav(host: &str, target: &str, cookies: &str) -> Request {
    let mut h = vec![
        ("Host", host),
        ("Sec-Fetch-Mode", "navigate"),
        ("Sec-Fetch-Site", "cross-site"),
    ];
    if !cookies.is_empty() {
        h.push(("Cookie", cookies));
    }
    Request::new("GET", target, &h)
}

/// A fetch of the page's own script.
fn fetch(host: &str, target: &str, cookies: &str) -> Request {
    let mut h = vec![
        ("Host", host),
        ("Sec-Fetch-Mode", "cors"),
        ("Sec-Fetch-Site", "same-origin"),
    ];
    if !cookies.is_empty() {
        h.push(("Cookie", cookies));
    }
    Request::new("GET", target, &h)
}

fn post(host: &str, target: &str, cookies: &str) -> Request {
    let origin = format!("https://{host}");
    let mut r = fetch(host, target, cookies);
    r.method = "POST".into();
    r.headers.push(("Origin".into(), origin));
    r
}

/// The value a Set-Cookie of the answer gives `name`.
fn set(r: &Response, name: &str) -> Option<String> {
    r.headers
        .iter()
        .filter(|(k, _)| k == "Set-Cookie")
        .filter_map(|(_, v)| v.strip_prefix(&format!("{name}=")))
        .map(|v| v.split(';').next().unwrap().to_string())
        .next()
}

fn location(r: &Response) -> String {
    r.header("Location").unwrap_or("").to_string()
}

fn q(url: &str, name: &str) -> String {
    path::query(url, name).unwrap_or_default()
}

fn target(url: &str) -> String {
    let rest = url.strip_prefix("https://").unwrap();
    format!("/{}", rest.split_once('/').unwrap().1)
}

/// Signs in on `host` from no session at `plugins.<domain>`: the ask, the
/// pass, GitHub's callback and the take. The session's cookie on `host`.
fn sign_in(f: &Fix, host: &str, now: u64) -> String {
    let s = sign_in_full(f, host, now);
    s.0
}

/// As [`sign_in`], also the session's cookie on `plugins.<domain>`.
fn sign_in_full(f: &Fix, host: &str, now: u64) -> (String, String) {
    let ask = f.gate.answer(&nav(host, "/_auth/ask?back=/sub/", ""), now);
    assert_eq!(ask.status, 303);
    let nonce = set(&ask, auth::ASK).unwrap();
    let pass = f.gate.answer(&nav(S, &target(&location(&ask)), ""), now);
    assert_eq!(pass.status, 303, "{}", location(&pass));
    let github = location(&pass);
    assert!(github.starts_with("https://github.example/login/oauth/authorize?client_id=Iv1.client&redirect_uri=https%3A%2F%2Fplugins.example.org%2F_auth%2Fcallback&state="), "{github}");
    let signin = set(&pass, auth::SIGNIN).unwrap();
    let back = format!("/_auth/callback?code=good&state={}", q(&github, "state"));
    let cb = f
        .gate
        .answer(&nav(S, &back, &format!("{}={signin}", auth::SIGNIN)), now);
    assert_eq!(cb.status, 303);
    let s_session = set(&cb, auth::SESSION).unwrap();
    let take = location(&cb);
    assert!(
        take.starts_with(&format!("https://{host}/_auth/take?code=")),
        "{take}"
    );
    let took = f.gate.answer(
        &nav(host, &target(&take), &format!("{}={nonce}", auth::ASK)),
        now,
    );
    assert_eq!(took.status, 303);
    assert_eq!(location(&took), format!("https://{host}/sub/"));
    (
        format!("{}={}", auth::SESSION, set(&took, auth::SESSION).unwrap()),
        format!("{}={s_session}", auth::SESSION),
    )
}

const BASE: [(&str, &str); 5] = [
    ("Cache-Control", "private, no-store"),
    ("Vary", "Cookie"),
    ("Cross-Origin-Resource-Policy", "same-origin"),
    ("X-Content-Type-Options", "nosniff"),
    ("Referrer-Policy", "no-referrer"),
];

fn assert_base(r: &Response, what: &str) {
    for (k, v) in BASE {
        assert_eq!(r.header(k), Some(v), "{k} on {what}");
    }
    let pp = r.header("Permissions-Policy").unwrap_or("");
    assert!(
        pp.starts_with(csp::PERMISSIONS),
        "Permissions-Policy on {what}: {pp}"
    );
    assert!(
        r.header("Content-Security-Policy").is_some(),
        "CSP on {what}"
    );
    assert!(
        !r.headers
            .iter()
            .any(|(k, _)| k.to_ascii_lowercase().starts_with("access-control-")),
        "{what}"
    );
}

#[test]
fn every_answer_carries_the_gates_headers_errors_included() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    std::fs::write(
        f.dir.join("plugins/floorplan/view/bad.html"),
        "<a onclick=x()>",
    )
    .unwrap();
    let mut dup = fetch(P, "/", &c);
    dup.headers.push(("Host".into(), P.into()));
    let mut other = fetch(P, "/", "");
    other.method = "PUT".into();
    let mut opt = fetch(P, "/", "");
    opt.method = "OPTIONS".into();
    let cases: Vec<(&str, Request, u16)> = vec![
        ("unknown host", nav("uat-1.example.org", "/", ""), 404),
        ("two hosts", dup, 400),
        ("options", opt, 403),
        ("put", other, 405),
        (
            "cross-site fetch",
            Request::new(
                "GET",
                "/",
                &[
                    ("Host", P),
                    ("Sec-Fetch-Mode", "cors"),
                    ("Sec-Fetch-Site", "same-site"),
                ],
            ),
            403,
        ),
        ("sign-in page", nav(P, "/", ""), 200),
        ("no session", fetch(P, "/app.js", ""), 401),
        ("page", nav(P, "/", &c), 200),
        ("file", fetch(P, "/app.js", &c), 200),
        ("refused page", nav(P, "/bad.html", &c), 500),
        ("missing", fetch(P, "/nope.js", &c), 404),
        ("reserved", fetch(P, "/_frame/x", &c), 404),
        ("kit", fetch(P, "/_kit/press.js", &c), 200),
        ("actions", fetch(P, "/_actions", &c), 200),
        ("hooks", nav(P, "/_hooks/12", &c), 404),
        ("redirect", nav(P, "/_auth/ask", ""), 303),
        ("stale code", nav(P, "/_auth/take?code=x", ""), 403),
        ("sign-in name", nav(S, "/", ""), 404),
        ("start", nav(S, "/_auth/start?for=floorplan", ""), 303),
        (
            "upgrade",
            Request::new(
                "GET",
                "/",
                &[
                    ("Host", P),
                    ("Sec-Fetch-Mode", "navigate"),
                    ("Sec-Fetch-Site", "same-origin"),
                    ("Upgrade", "websocket"),
                ],
            ),
            404,
        ),
    ];
    for (what, req, status) in cases {
        let r = f.gate.answer(&req, T0 + 1);
        assert_eq!(r.status, status, "{what}");
        assert_base(&r, what);
    }
    let mut busy = Response::empty(503);
    base(&mut busy, "https://example.org");
    assert_base(&busy, "busy");
}

#[test]
fn a_page_carries_the_browsers_policy_and_the_longer_permissions() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    let r = f.gate.answer(&nav(P, "/", &c), T0);
    assert_eq!(r.header("Permissions-Policy"), Some(csp::PAGE_PERMISSIONS));
    assert_eq!(
        r.header("Cross-Origin-Opener-Policy"),
        Some("same-origin-allow-popups")
    );
    let policy = r.header("Content-Security-Policy").unwrap();
    assert!(
        policy.contains("frame-ancestors 'self' https://example.org;"),
        "{policy}"
    );
    let js = f.gate.answer(&fetch(P, "/app.js", &c), T0);
    assert_eq!(
        js.header("Content-Type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(js.header("Permissions-Policy"), Some(csp::PERMISSIONS));
}

#[test]
fn options_is_403_with_no_session_lookup() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    let before = f.hub.calls();
    let mut opt = fetch(P, "/", &c);
    opt.method = "OPTIONS".into();
    // A session due for its check would ask GitHub if it were looked up.
    let r = f.gate.answer(&opt, T0 + auth::CHECK + 1);
    assert_eq!(r.status, 403);
    assert!(r.body.is_empty());
    assert_eq!(f.hub.calls(), before);
}

#[test]
fn the_whole_host_is_matched_against_the_settings_names() {
    let f = fixture();
    for host in [P, Q, "PLUGIN-FLOORPLAN.example.org"] {
        assert_eq!(f.gate.answer(&nav(host, "/", ""), T0).status, 200, "{host}");
    }
    assert_eq!(
        f.gate
            .answer(&nav(S, "/_auth/start?for=specs", ""), T0)
            .status,
        303
    );
    for host in [
        "plugin-floorplan.example.org.evil.test",
        "plugin-floorplan.example.org:443",
        "plugin-other.example.org",
        "plugin-floorplan.other.org",
        "x.plugin-floorplan.example.org",
        "plugin-floorplan",
        "uat-1.example.org",
        "example.org",
        "",
    ] {
        let mut r = f.gate.answer(&nav(host, "/", ""), T0);
        assert_eq!(r.status, 404, "{host}");
        assert_eq!(String::from_utf8(r.bytes()).unwrap(), NO_PLUGIN, "{host}");
    }
    let none = Request::new("GET", "/", &[("Sec-Fetch-Mode", "navigate")]);
    assert_eq!(f.gate.answer(&none, T0).status, 404);
}

#[test]
fn one_level_names_join_the_label_with_a_dash() {
    let f = fixture_with(Level::Write, "demo.example.org", true);
    assert_eq!(
        f.gate.plugin_host("floorplan"),
        "plugin-floorplan-demo.example.org"
    );
    assert_eq!(f.gate.signin_host(), "plugins-demo.example.org");
    let r = f.gate.answer(
        &nav("plugins-demo.example.org", "/_auth/start?for=floorplan", ""),
        T0,
    );
    assert_eq!(
        location(&r),
        "https://plugin-floorplan-demo.example.org/_auth/ask?back=%2F"
    );
    let r = f
        .gate
        .answer(&nav("plugin-floorplan-demo.example.org", "/", ""), T0);
    assert_eq!(r.status, 200);
    for host in [
        "plugin-floorplan.demo.example.org",
        "plugins.demo.example.org",
    ] {
        assert_eq!(f.gate.answer(&nav(host, "/", ""), T0).status, 404, "{host}");
    }
}

#[test]
fn a_request_from_another_site_is_refused_unless_it_is_a_navigation() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    // A preview's script, the same site: not the same origin.
    for site in ["same-site", "cross-site", "none"] {
        let r = Request::new(
            "GET",
            "/app.js",
            &[
                ("Host", P),
                ("Sec-Fetch-Mode", "no-cors"),
                ("Sec-Fetch-Site", site),
                ("Cookie", &c),
            ],
        );
        assert_eq!(f.gate.answer(&r, T0).status, 403, "{site}");
    }
    let bare = Request::new("GET", "/app.js", &[("Host", P), ("Cookie", &c)]);
    assert_eq!(f.gate.answer(&bare, T0).status, 403);
    // A navigation from another site gets the page.
    assert_eq!(f.gate.answer(&nav(P, "/", &c), T0).status, 200);
    // A press needs the name's own Origin and same-origin.
    let mut wrong = post(P, "/_actions/refresh", &c);
    wrong.headers.retain(|(k, _)| k != "Origin");
    wrong
        .headers
        .push(("Origin".into(), "https://uat-1.example.org".into()));
    assert_eq!(f.gate.answer(&wrong, T0).status, 403);
    let mut no_origin = post(P, "/_actions/refresh", &c);
    no_origin.headers.retain(|(k, _)| k != "Origin");
    assert_eq!(f.gate.answer(&no_origin, T0).status, 403);
    let mut no_site = post(P, "/_actions/refresh", &c);
    no_site.headers.retain(|(k, _)| k != "Sec-Fetch-Site");
    assert_eq!(f.gate.answer(&no_site, T0).status, 403);
    // A form of another site, a navigation, runs nothing.
    let form = Request::new(
        "POST",
        "/_actions/refresh",
        &[
            ("Host", P),
            ("Sec-Fetch-Mode", "navigate"),
            ("Sec-Fetch-Site", "same-site"),
            ("Origin", "https://uat-1.example.org"),
            ("Cookie", &c),
        ],
    );
    assert_eq!(f.gate.answer(&form, T0).status, 403);
    let mut forged = form.clone();
    forged.headers.retain(|(k, _)| k != "Origin");
    forged
        .headers
        .push(("Origin".into(), format!("https://{P}")));
    assert_eq!(f.gate.answer(&forged, T0).status, 403);
    assert!(lock(&f.runner.0).0.is_empty());
}

#[test]
fn a_path_is_normalised_before_it_is_matched() {
    for (raw, want) in [
        ("/", Some("/")),
        ("/a/../b", Some("/b")),
        ("//a//b/", Some("/a/b")),
        ("/./a/%2e/b", Some("/a/b")),
        ("/a/%2E%2e/b", Some("/b")),
        ("/%5fkit/x", Some("/_kit/x")),
        ("/a?x=/../..", Some("/a")),
        ("/..", None),
        ("/a/../..", None),
        ("/%2e%2e/etc/passwd", None),
        ("/a%2fb", None),
        ("/a%2Fb", None),
        ("/a%5cb", None),
        ("/a\\b", None),
        ("/a%00", None),
        ("/a%zz", None),
        ("/%ff", None),
        ("a", None),
        ("*", None),
        ("https://evil.test/a", None),
    ] {
        assert_eq!(
            path::normalise(raw).map(|p| p.text()).as_deref(),
            want,
            "{raw}"
        );
    }
}

#[test]
fn a_file_of_the_view_is_served_and_nothing_outside_it() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    std::fs::write(f.dir.join("plugins/floorplan/secret.txt"), "no").unwrap();
    std::os::unix::fs::symlink(
        f.dir.join("plugins/floorplan/secret.txt"),
        f.dir.join("plugins/floorplan/view/link.txt"),
    )
    .unwrap();
    let body = |t: &str| {
        let mut r = f.gate.answer(&nav(P, t, &c), T0);
        (r.status, String::from_utf8(r.bytes()).unwrap())
    };
    assert_eq!(body("/sub/"), (200, "<p>sub</p>".into()));
    assert_eq!(body("/sub").0, 200);
    assert!(body("/").1.contains("map"));
    assert_eq!(body("//app.js"), (200, "console.log(1)".into()));
    assert_eq!(body("/sub/%2e%2e/app.js").0, 200);
    for t in [
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/link.txt",
        "/nope",
        "/sub/..%2fsecret.txt",
    ] {
        assert_eq!(body(t).0, 404, "{t}");
    }
}

#[test]
fn a_kept_file_is_framed_sandboxed_and_only_by_its_own_view() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    let keep = f.dir.join("plugins/floorplan/keep/p-1");
    std::fs::create_dir_all(&keep).unwrap();
    std::fs::write(
        keep.join("v2.html"),
        "<p onclick=\"x()\">proto</p><script>steal()</script>",
    )
    .unwrap();
    std::fs::write(keep.join("shot.png"), "png").unwrap();
    std::fs::write(keep.join("run.js"), "x()").unwrap();
    std::fs::write(f.dir.join("plugins/floorplan/secret.html"), "no").unwrap();
    std::os::unix::fs::symlink(
        f.dir.join("plugins/floorplan/secret.html"),
        keep.join("link.html"),
    )
    .unwrap();
    let mut r = f.gate.answer(&fetch(P, "/_frame/p-1/v2.html", &c), T0);
    assert_eq!(r.status, 200);
    assert_eq!(r.header("Content-Type"), Some("text/html; charset=utf-8"));
    let policy = r.header("Content-Security-Policy").unwrap().to_string();
    // Whatever the page holds runs nothing: sandboxed without scripts.
    assert!(policy.ends_with("sandbox allow-same-origin"), "{policy}");
    assert!(!policy.contains("script-src"), "{policy}");
    assert!(
        policy.contains(&format!("frame-ancestors https://{P}")),
        "{policy}"
    );
    assert_eq!(r.header("Cache-Control"), Some("private, no-store"));
    assert_eq!(
        r.header("Cross-Origin-Resource-Policy"),
        Some("same-origin")
    );
    assert!(String::from_utf8(r.bytes()).unwrap().contains("proto"));
    let img = f.gate.answer(&fetch(P, "/_frame/p-1/shot.png", &c), T0);
    assert_eq!(img.header("Content-Type"), Some("image/png"));
    for t in [
        "/_frame/p-1/run.js",
        "/_frame/p-1/link.html",
        "/_frame/p-1",
        "/_frame/p-1/../../secret.html",
        "/_frame/p-1/none.html",
    ] {
        assert_eq!(f.gate.answer(&fetch(P, t, &c), T0).status, 404, "{t}");
    }
    // No session: no frame.
    assert_eq!(
        f.gate
            .answer(&fetch(P, "/_frame/p-1/v2.html", ""), T0)
            .status,
        401
    );
    // Another site's fetch: refused before anything is read.
    let mut cross = fetch(P, "/_frame/p-1/v2.html", &c);
    cross.headers.retain(|(k, _)| k != "Sec-Fetch-Site");
    cross
        .headers
        .push(("Sec-Fetch-Site".into(), "cross-site".into()));
    assert_eq!(f.gate.answer(&cross, T0).status, 403);
}

#[test]
fn a_plugin_without_kept_files_frames_nothing() {
    let f = fixture();
    let host = f.gate.plugin_host("specs");
    let c = sign_in(&f, &host, T0);
    let keep = f.dir.join("plugins/specs/keep/p-1");
    std::fs::create_dir_all(&keep).unwrap();
    std::fs::write(keep.join("v2.html"), "<p>x</p>").unwrap();
    assert_eq!(
        f.gate
            .answer(&fetch(&host, "/_frame/p-1/v2.html", &c), T0)
            .status,
        404
    );
}

#[test]
fn paths_of_underscore_are_the_gates_whatever_the_view_holds() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    let mut r = f.gate.answer(&fetch(P, "/_kit/press.js", &c), T0);
    assert_eq!(r.bytes(), b"p()".to_vec());
    let mut s = f.gate.answer(&fetch(P, "/_session", &c), T0);
    assert_eq!(s.bytes(), br#"{"signed": true}"#.to_vec());
    for t in [
        "/_frame/x",
        "/_frame",
        "/_relay/a",
        "/_console/a",
        "/_x",
        "/_",
        "/_auth/other",
        "/_hooks/abc",
        "/_hooks/0",
        "/_actions/refresh/x",
    ] {
        assert_eq!(f.gate.answer(&fetch(P, t, &c), T0).status, 404, "{t}");
    }
}

#[test]
fn a_request_with_no_session_gets_the_sign_in_page_or_401() {
    let f = fixture();
    let mut page = f.gate.answer(&nav(P, "/sub/?x=1", ""), T0);
    assert_eq!(page.status, 200);
    let text = String::from_utf8(page.bytes()).unwrap();
    assert!(
        text.contains("href=\"https://plugins.example.org/_auth/start?for=floorplan&amp;back=%2Fsub%2F\" target=\"_blank\""),
        "{text}"
    );
    assert!(text.contains("/_session"));
    let policy = page.header("Content-Security-Policy").unwrap();
    assert!(policy.contains("script-src 'self' 'sha256-"), "{policy}");
    for req in [
        fetch(P, "/app.js", ""),
        fetch(P, "/_kit/press.js", ""),
        fetch(P, "/_actions", ""),
        fetch(P, "/_session", ""),
        post(P, "/_actions/refresh", ""),
        post(P, "/_signout", ""),
        fetch(
            P,
            "/app.js",
            &format!("{}={}", auth::SESSION, "0".repeat(64)),
        ),
    ] {
        let r = f.gate.answer(&req, T0);
        assert_eq!(r.status, 401, "{}", req.target);
        assert!(r.body.is_empty());
    }
}

#[test]
fn a_session_ends_after_two_hours_unused_or_twelve_in_all() {
    let f = fixture();
    let ok = |c: &str, t| f.gate.answer(&fetch(P, "/app.js", c), t).status;
    let c = sign_in(&f, P, T0);
    assert_eq!(ok(&c, T0 + auth::IDLE), 200);
    assert_eq!(ok(&c, T0 + 2 * auth::IDLE + 1), 401);
    let c = sign_in(&f, P, T0);
    let ok = |t| ok(&c, t);
    let mut t = T0;
    while t + 3600 <= T0 + auth::TOTAL {
        t += 3600;
        assert_eq!(ok(t), 200, "{t}");
    }
    assert_eq!(ok(T0 + auth::TOTAL + 1), 401);
}

#[test]
fn access_is_checked_again_every_ten_minutes_by_user_id() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    let calls = f.hub.calls().1;
    let ok = |t| f.gate.answer(&fetch(P, "/app.js", &c), t).status;
    assert_eq!(ok(T0 + auth::CHECK - 1), 200);
    assert_eq!(f.hub.calls().1, calls);
    assert_eq!(ok(T0 + auth::CHECK), 200);
    assert_eq!(f.hub.calls().1, calls + 1);
    // GitHub's error is a refusal, and the session stays for the next ask.
    f.hub.set(|h| h.level = Some(Err("down".into())));
    assert_eq!(ok(T0 + 2 * auth::CHECK), 503);
    // The login now names someone else: a renamed and reused login.
    f.hub.set(|h| {
        h.level = None;
        h.other_id = Some(8);
    });
    assert_eq!(ok(T0 + 2 * auth::CHECK + 1), 401);
    f.hub.set(|h| h.other_id = None);
    assert_eq!(ok(T0 + 2 * auth::CHECK + 2), 401);
}

#[test]
fn losing_access_signs_out_every_session_of_the_person() {
    let f = fixture();
    let (c, s) = sign_in_full(&f, P, T0);
    f.hub.set(|h| h.level = Some(Ok(None)));
    assert_eq!(
        f.gate
            .answer(&fetch(P, "/app.js", &c), T0 + auth::CHECK)
            .status,
        401
    );
    assert_eq!(
        f.gate
            .answer(&fetch(S, "/_session", &s), T0 + auth::CHECK)
            .status,
        401
    );
}

#[test]
fn sign_out_ends_the_persons_sessions_on_every_name() {
    let f = fixture();
    let (c, s) = sign_in_full(&f, P, T0);
    // The second name takes its session from plugins.<domain>: no GitHub
    // sign-in again.
    let tokens = f.hub.calls().0;
    let ask = f.gate.answer(&nav(Q, "/_auth/ask", ""), T0);
    let nonce = set(&ask, auth::ASK).unwrap();
    let pass = f.gate.answer(&nav(S, &target(&location(&ask)), &s), T0);
    let take = location(&pass);
    assert!(
        take.starts_with("https://plugin-specs.example.org/_auth/take?code="),
        "{take}"
    );
    let took = f.gate.answer(
        &nav(Q, &target(&take), &format!("{}={nonce}", auth::ASK)),
        T0,
    );
    let d = format!("{}={}", auth::SESSION, set(&took, auth::SESSION).unwrap());
    assert_eq!(f.hub.calls().0, tokens);
    assert_eq!(f.gate.answer(&fetch(Q, "/_session", &d), T0).status, 200);

    let out = f.gate.answer(&post(P, "/_signout", &c), T0);
    assert_eq!(out.status, 200);
    let cleared = out.headers.iter().any(|(k, v)| {
        k == "Set-Cookie" && v.starts_with("__Host-tehas-plugin=;") && v.contains("Max-Age=0")
    });
    assert!(cleared);
    for (host, cookie) in [(P, &c), (Q, &d), (S, &s)] {
        assert_eq!(
            f.gate.answer(&fetch(host, "/_session", cookie), T0).status,
            401,
            "{host}"
        );
    }
    // A GET is no sign-out.
    let (c, _) = sign_in_full(&f, P, T0);
    assert_eq!(f.gate.answer(&fetch(P, "/_signout", &c), T0).status, 405);
    assert_eq!(f.gate.answer(&fetch(P, "/_session", &c), T0).status, 200);
}

#[test]
fn the_cookies_are_host_only_https_only_and_out_of_scripts_reach() {
    assert_eq!(
        auth::set_cookie(auth::SESSION, "v", None),
        "__Host-tehas-plugin=v; Path=/; Secure; HttpOnly; SameSite=Lax"
    );
    let id = "a".repeat(64);
    let h = |v: &str| vec![("Cookie".to_string(), v.to_string())];
    assert_eq!(
        auth::cookie(&h(&format!("x=1; {}={id}", auth::SESSION)), auth::SESSION),
        Some(id.clone())
    );
    // Twice, or not one of the gate's ids: none.
    assert_eq!(
        auth::cookie(
            &h(&format!("{0}={id}; {0}={id}", auth::SESSION)),
            auth::SESSION
        ),
        None
    );
    assert_eq!(
        auth::cookie(&h(&format!("{}=short", auth::SESSION)), auth::SESSION),
        None
    );
}

/// The steps of a hand-off up to the code: the ask's nonce and the take's
/// address.
fn hand_off(f: &Fix, host: &str, back: &str, now: u64) -> (String, String) {
    let (_, s) = sign_in_full(f, P, now);
    let ask = f
        .gate
        .answer(&nav(host, &format!("/_auth/ask?back={back}"), ""), now);
    let nonce = set(&ask, auth::ASK).unwrap();
    let pass = f.gate.answer(&nav(S, &target(&location(&ask)), &s), now);
    (format!("{}={nonce}", auth::ASK), target(&location(&pass)))
}

#[test]
fn a_hand_off_code_works_once_within_a_minute_for_its_name_and_nonce() {
    let f = fixture();
    let (nonce, take) = hand_off(&f, Q, "/", T0);
    assert_eq!(
        f.gate
            .answer(&nav(Q, &take, &nonce), T0 + auth::CODE)
            .status,
        303
    );
    assert_eq!(
        f.gate
            .answer(&nav(Q, &take, &nonce), T0 + auth::CODE)
            .status,
        403
    );

    let (nonce, take) = hand_off(&f, Q, "/", T0);
    assert_eq!(
        f.gate
            .answer(&nav(Q, &take, &nonce), T0 + auth::CODE + 1)
            .status,
        403
    );

    let (nonce, take) = hand_off(&f, Q, "/", T0);
    assert_eq!(f.gate.answer(&nav(P, &take, &nonce), T0).status, 403);
    // Used up by the wrong name.
    assert_eq!(f.gate.answer(&nav(Q, &take, &nonce), T0).status, 403);

    let (_, take) = hand_off(&f, Q, "/", T0);
    assert_eq!(f.gate.answer(&nav(Q, &take, ""), T0).status, 403);

    let (other, _) = hand_off(&f, Q, "/", T0);
    let (_, take) = hand_off(&f, Q, "/", T0);
    assert_eq!(f.gate.answer(&nav(Q, &take, &other), T0).status, 403);

    // The code alone opens nothing.
    let (_, take) = hand_off(&f, Q, "/", T0);
    let r = f.gate.answer(&nav(Q, &take, ""), T0);
    assert!(set(&r, auth::SESSION).is_none());
}

#[test]
fn back_is_a_normalised_path_under_the_asking_name() {
    let f = fixture();
    for (back, want) in [
        ("/sub/../app.js", "/app.js"),
        ("//evil.test/x", "/"),
        ("https://evil.test/", "/"),
        ("/_signout", "/"),
        ("/%2e%2e/x", "/"),
        ("/a%3Fb", "/a%3Fb"),
    ] {
        let (nonce, take) = hand_off(&f, Q, &path::component(back), T0);
        let r = f.gate.answer(&nav(Q, &take, &nonce), T0);
        assert_eq!(location(&r), format!("https://{Q}{want}"), "{back}");
    }
}

#[test]
fn the_asking_name_comes_from_the_settings() {
    let f = fixture();
    assert_eq!(
        f.gate
            .answer(&nav(S, "/_auth/start?for=other", ""), T0)
            .status,
        404
    );
    assert_eq!(f.gate.answer(&nav(S, "/_auth/start", ""), T0).status, 404);
    let r = f
        .gate
        .answer(&nav(S, "/_auth/start?for=specs&back=/a", ""), T0);
    assert_eq!(
        location(&r),
        "https://plugin-specs.example.org/_auth/ask?back=%2Fa"
    );
    // A state no name asked for is refused before GitHub.
    let r = f.gate.answer(
        &nav(S, &format!("/_auth/pass?state={}", "a".repeat(64)), ""),
        T0,
    );
    assert_eq!(r.status, 403);
    assert!(set(&r, auth::SIGNIN).is_none());
    // GitHub's callback needs the flow's nonce.
    let ask = f.gate.answer(&nav(P, "/_auth/ask", ""), T0);
    let pass = f.gate.answer(&nav(S, &target(&location(&ask)), ""), T0);
    let state = q(&location(&pass), "state");
    let cb = f.gate.answer(
        &nav(S, &format!("/_auth/callback?code=good&state={state}"), ""),
        T0,
    );
    assert_eq!(cb.status, 403);
    assert_eq!(f.hub.calls().0, 0);
}

#[test]
fn the_persons_token_comes_from_the_flows_code() {
    let f = fixture();
    let ask = f.gate.answer(&nav(P, "/_auth/ask", ""), T0);
    let pass = f.gate.answer(&nav(S, &target(&location(&ask)), ""), T0);
    let signin = format!("{}={}", auth::SIGNIN, set(&pass, auth::SIGNIN).unwrap());
    let state = q(&location(&pass), "state");
    let cb = f.gate.answer(
        &nav(
            S,
            &format!("/_auth/callback?code=bad&state={state}"),
            &signin,
        ),
        T0,
    );
    assert_eq!(cb.status, 503);
    assert!(set(&cb, auth::SESSION).is_none());
}

fn level_lets_in(access: Level, has: Result<Option<Level>, String>, public: bool) -> u16 {
    let f = fixture_with(access, DOMAIN, false);
    f.hub.set(|h| {
        h.level = Some(has);
        h.public = public;
    });
    let ask = f.gate.answer(&nav(P, "/_auth/ask", ""), T0);
    let pass = f.gate.answer(&nav(S, &target(&location(&ask)), ""), T0);
    let signin = format!("{}={}", auth::SIGNIN, set(&pass, auth::SIGNIN).unwrap());
    let state = q(&location(&pass), "state");
    f.gate
        .answer(
            &nav(
                S,
                &format!("/_auth/callback?code=good&state={state}"),
                &signin,
            ),
            T0,
        )
        .status
}

#[test]
fn a_person_gets_in_with_at_least_the_access_asked() {
    use Level::*;
    assert_eq!(level_lets_in(Write, Ok(Some(Read)), false), 403);
    assert_eq!(level_lets_in(Write, Ok(Some(Triage)), false), 403);
    assert_eq!(level_lets_in(Write, Ok(Some(Write)), false), 303);
    assert_eq!(level_lets_in(Write, Ok(Some(Admin)), false), 303);
    assert_eq!(level_lets_in(Admin, Ok(Some(Maintain)), false), 403);
    assert_eq!(level_lets_in(Read, Ok(Some(Read)), false), 303);
    assert_eq!(level_lets_in(Write, Ok(None), false), 403);
    assert_eq!(level_lets_in(Write, Err("down".into()), false), 503);
    // A public repository gives every account read.
    assert_eq!(level_lets_in(Read, Ok(Some(Admin)), true), 403);
    assert_eq!(level_lets_in(Triage, Ok(Some(Admin)), true), 403);
    assert_eq!(level_lets_in(Write, Ok(Some(Write)), true), 303);
}

#[test]
fn githubs_permission_answer_is_read_from_its_booleans() {
    let answer = |perms: &str| {
        github::permission_of(
            200,
            format!(r#"{{"permission":"write","role_name":"custom","user":{{"id":7,"login":"alice","permissions":{perms}}}}}"#).as_bytes(),
        )
    };
    let of = |l| Ok(Permission::Of(7, l, "custom".into()));
    assert_eq!(
        answer(r#"{"pull":true,"triage":false,"push":false}"#),
        of(Some(Level::Read))
    );
    assert_eq!(
        answer(r#"{"pull":true,"triage":true,"push":true,"maintain":false}"#),
        of(Some(Level::Write))
    );
    assert_eq!(answer(r#"{"admin":true}"#), of(Some(Level::Admin)));
    assert_eq!(answer(r#"{}"#), of(None));
    assert_eq!(github::permission_of(404, b""), Ok(Permission::None));
    assert!(github::permission_of(500, b"").is_err());
    assert!(github::permission_of(200, b"{}").is_err());
}

#[test]
fn a_pages_policy_names_the_hashes_of_its_own_blocks() {
    let doc = b"<!doctype html><style>p{color:red}</style><SCRIPT>go()</SCRIPT><script src=\"a.js\"></script><script type=module>m()</script><!-- <a onclick=x> -->";
    let b = csp::blocks(doc).unwrap();
    assert_eq!(b.scripts, vec![csp::hash(b"go()"), csp::hash(b"m()")]);
    assert_eq!(b.styles, vec![csp::hash(b"p{color:red}")]);
    // The sha256 of nothing, as a browser writes it.
    assert_eq!(
        csp::hash(b""),
        "'sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU='"
    );
    let policy = csp::policy(&b, "https://example.org");
    assert_eq!(
        policy,
        format!(
            "default-src 'none'; script-src 'self' {} {}; style-src 'self' {}; style-src-attr 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; connect-src 'self'; frame-src 'self'; frame-ancestors 'self' https://example.org; form-action 'self'; base-uri 'none'; object-src 'none'; worker-src 'none'; manifest-src 'none'",
            csp::hash(b"go()"),
            csp::hash(b"m()"),
            csp::hash(b"p{color:red}")
        )
    );
    assert_eq!(
        csp::of(b"<p>plain</p>", "https://e")
            .unwrap()
            .matches("sha256")
            .count(),
        0
    );
}

#[test]
fn a_page_with_an_inline_handler_or_a_meta_refresh_is_refused() {
    for doc in [
        "<a href=x onclick=\"go()\">",
        "<img src=x ONERROR=go()>",
        "<body onload='x'>",
        "<svg><a/onmouseover=x>",
        "<div title='a' onfocus>",
    ] {
        assert!(
            matches!(csp::blocks(doc.as_bytes()), Err(csp::Refused::Handler(_))),
            "{doc}"
        );
    }
    for doc in [
        "<meta http-equiv=\"refresh\" content=\"0;url=https://evil.test\">",
        "<META HTTP-EQUIV=Refresh content=0>",
    ] {
        assert_eq!(
            csp::blocks(doc.as_bytes()),
            Err(csp::Refused::Refresh),
            "{doc}"
        );
    }
    // A comment ends where a browser ends it, so it hides nothing.
    for doc in [
        "<!--><meta http-equiv=\"refresh\" content=\"0;url=https://x\"><!-- -->",
        "<!---><meta http-equiv=refresh content=0>",
        "<!-- a --!><meta http-equiv=refresh content=0><!-- -->",
    ] {
        assert_eq!(
            csp::blocks(doc.as_bytes()),
            Err(csp::Refused::Refresh),
            "{doc}"
        );
    }
    for doc in [
        "<!-- <meta http-equiv=refresh content=0> -->",
        "<!----><p>a</p><!-- --> <!-- <a onclick=x> --!>",
    ] {
        assert!(csp::blocks(doc.as_bytes()).is_ok(), "{doc}");
    }
    // Text that only looks like an attribute.
    assert!(csp::blocks(b"<p>onclick=go()</p><script>a.onclick=1</script>").is_ok());
    let f = fixture();
    let c = sign_in(&f, P, T0);
    std::fs::write(
        f.dir.join("plugins/floorplan/view/bad.html"),
        "<p onclick=x()>leak</p>",
    )
    .unwrap();
    let mut r = f.gate.answer(&nav(P, "/bad.html", &c), T0);
    assert_eq!(r.status, 500);
    let text = String::from_utf8(r.bytes()).unwrap();
    assert!(
        text.contains("inline event handler") && !text.contains("leak"),
        "{text}"
    );
}

#[test]
fn the_kit_is_served_from_its_manifest_when_the_plugin_asks_for_it() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    let mut r = f.gate.answer(&fetch(P, "/_kit/vendor/solid.js", &c), T0);
    assert_eq!(r.status, 200);
    assert_eq!(
        r.header("Content-Type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(r.bytes(), b"s()".to_vec());
    // Not listed, or listed and not installed.
    for t in [
        "/_kit/island.js",
        "/_kit/vendor/solid.d.ts",
        "/_kit/MANIFEST",
        "/_kit/vendor/../press.js/x",
    ] {
        assert_eq!(f.gate.answer(&fetch(P, t, &c), T0).status, 404, "{t}");
    }
    // specs asks for no kit.
    let d = sign_in(&f, Q, T0);
    assert_eq!(
        f.gate.answer(&fetch(Q, "/_kit/press.js", &d), T0).status,
        404
    );
    // A kit that is not kit 1 serves nothing.
    std::fs::write(
        f.dir.join("fun/kit/MANIFEST"),
        "kit 2\nfile aa 3 press.js\n",
    )
    .unwrap();
    assert_eq!(
        f.gate.answer(&fetch(P, "/_kit/press.js", &c), T0).status,
        404
    );
    assert_eq!(
        catalog::kit_files("kit 1\nfile a 1 x.js\n"),
        Some(vec!["x.js".to_string()])
    );
    assert_eq!(catalog::kit_files("kit 1\nbad line\n"), None);
}

#[test]
fn the_actions_granted_are_listed() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    let mut r = f.gate.answer(&fetch(P, "/_actions", &c), T0);
    assert_eq!(r.bytes(), br#"{"actions":["refresh"],"api":1}"#.to_vec());
    assert_eq!(f.gate.answer(&post(P, "/_actions", &c), T0).status, 405);
}

#[test]
fn a_press_asks_for_a_refresh_and_answers_by_the_spec() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    let calls = f.hub.calls().1;
    assert_eq!(
        f.gate.answer(&post(P, "/_actions/refresh", &c), T0).status,
        202
    );
    // The access was checked on the press.
    assert_eq!(f.hub.calls().1, calls + 1);
    assert_eq!(
        lock(&f.runner.0).0,
        vec!["floorplan refresh alice".to_string()]
    );
    assert_eq!(
        f.gate
            .answer(&post(P, "/_actions/refresh", &c), T0 + PRESS_GAP - 1)
            .status,
        429
    );
    lock(&f.runner.0).1 = Some(Press::Running);
    assert_eq!(
        f.gate
            .answer(&post(P, "/_actions/refresh", &c), T0 + PRESS_GAP)
            .status,
        409
    );
    lock(&f.runner.0).1 = None;
    assert_eq!(
        f.gate
            .answer(&post(P, "/_actions/refresh", &c), T0 + PRESS_GAP)
            .status,
        202
    );
    assert_eq!(
        f.gate.answer(&post(P, "/_actions/other", &c), T0).status,
        403
    );
    assert_eq!(
        f.gate.answer(&fetch(P, "/_actions/refresh", &c), T0).status,
        405
    );
    assert_eq!(
        f.gate.answer(&post(P, "/_actions/refresh", ""), T0).status,
        401
    );
    // No access on the press: 403, and signed out.
    let d = sign_in(&f, Q, T0);
    f.hub.set(|h| h.level = Some(Ok(Some(Level::Read))));
    assert_eq!(
        f.gate.answer(&post(Q, "/_actions/refresh", &d), T0).status,
        403
    );
    assert_eq!(f.gate.answer(&fetch(Q, "/_session", &d), T0).status, 401);
    f.hub.set(|h| h.level = None);
    let e = sign_in(&f, Q, T0);
    f.hub.set(|h| h.level = Some(Err("down".into())));
    assert_eq!(
        f.gate.answer(&post(Q, "/_actions/refresh", &e), T0).status,
        403
    );
}

#[test]
fn a_hooks_findings_are_shown_escaped() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    let hooks = f.dir.join("plugins/floorplan/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    const H: &str = "89abcdef0123456789abcdef0123456789abcdef";
    let read = tehas_core::plugin::verdict::read(
        br#"{"verdict":"note","summary":"<script>alert(1)</script>","findings":[{"file":"a\"b.rs","line":3,"text":"<img src=x onerror=y>"}]}"#,
    )
    .unwrap();
    let end = |pr: u64, result: &str, verdict| tehas_core::plugin::verdict::End {
        pr,
        head: H.into(),
        result: result.into(),
        verdict,
    };
    // The runner's own writer, so the page reads the file as it is written.
    crate::run::write_end(&f.dir, "floorplan", &end(12, "done", Some(read)));
    let mut r = f.gate.answer(&nav(P, "/_hooks/12", &c), T0);
    assert_eq!(r.status, 200);
    let text = String::from_utf8(r.bytes()).unwrap();
    assert!(
        text.contains(
            "<code title=\"89abcdef0123456789abcdef0123456789abcdef\">89abcdef0123</code>"
        ),
        "{text}"
    );
    assert!(text.contains("<strong>note</strong>"), "{text}");
    assert!(
        text.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{text}"
    );
    assert!(
        text.contains("a&quot;b.rs:3") && text.contains("&lt;img src=x onerror=y&gt;"),
        "{text}"
    );
    assert!(!text.contains("<img") && !text.contains("<script>alert"));
    // A hook that did not finish says so, with its head and no verdict.
    crate::run::write_end(&f.dir, "floorplan", &end(15, "timeout", None));
    let mut r = f.gate.answer(&nav(P, "/_hooks/15", &c), T0);
    assert_eq!(r.status, 200);
    let text = String::from_utf8(r.bytes()).unwrap();
    assert!(text.contains("did not finish: it ended timeout") && text.contains("89abcdef0123"));
    assert!(!text.contains("Verdict"), "{text}");
    assert_eq!(f.gate.answer(&nav(P, "/_hooks/13", &c), T0).status, 404);
    std::fs::write(hooks.join("14.verdict.json"), "not json").unwrap();
    assert_eq!(f.gate.answer(&nav(P, "/_hooks/14", &c), T0).status, 404);
    // The flat verdict of the hook's own file is no end.
    std::fs::write(
        hooks.join("16.verdict.json"),
        r#"{"verdict":"note","summary":"flat","findings":[]}"#,
    )
    .unwrap();
    assert_eq!(f.gate.answer(&nav(P, "/_hooks/16", &c), T0).status, 404);
    // An end of another pull request in the file is not shown.
    std::fs::copy(hooks.join("12.verdict.json"), hooks.join("17.verdict.json")).unwrap();
    assert_eq!(f.gate.answer(&nav(P, "/_hooks/17", &c), T0).status, 404);
}

#[test]
fn an_upgrade_is_refused_until_its_path_is_built() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    let r = Request::new(
        "GET",
        "/_relay/voice",
        &[
            ("Host", P),
            ("Sec-Fetch-Mode", "websocket"),
            ("Sec-Fetch-Site", "same-origin"),
            ("Origin", "https://plugin-floorplan.example.org"),
            ("Upgrade", "websocket"),
            ("Cookie", &c),
        ],
    );
    assert_eq!(f.gate.answer(&r, T0).status, 404);
}

#[test]
fn a_plugin_is_granted_by_its_manifest_and_its_grants_line() {
    const C: &str = "0123456789abcdef0123456789abcdef01234567";
    let g = tehas_core::plugin::grants(&format!("{C} view action:refresh")).unwrap();
    let asks: Vec<String> = ["view", "action:refresh"].map(String::from).to_vec();
    let actions = vec!["refresh".to_string(), "other".to_string()];
    let f = catalog::facts_of(&asks, true, &actions, Some(&g), C);
    assert_eq!(
        f,
        Facts {
            view: true,
            kit: true,
            actions: vec!["refresh".into()],
            paths: Vec::new(),
            keep: false,
        }
    );
    // Another commit's grants grant nothing.
    let other = "1".repeat(40);
    assert_eq!(
        catalog::facts_of(&asks, true, &actions, Some(&g), &other),
        Facts::default()
    );
    let more: Vec<String> = ["view", "product"].map(String::from).to_vec();
    assert_eq!(
        catalog::facts_of(&more, true, &actions, Some(&g), C),
        Facts::default()
    );
    assert!(catalog::ready(
        &format!(r#"{{"api":1,"state":"ready","commit":"{C}"}}"#),
        C
    ));
    assert!(!catalog::ready(
        &format!(r#"{{"api":1,"state":"refused","commit":"{C}"}}"#),
        C
    ));
    assert!(!catalog::ready(
        &format!(r#"{{"api":1,"state":"ready","commit":"{other}"}}"#),
        C
    ));
}

#[test]
fn the_listener_writes_the_answer_and_closes() {
    use std::io::{Read, Write};
    let f = fixture();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let gate = Arc::new(f.gate);
    std::thread::spawn(move || serve(listener, gate, 4));
    let ask = |raw: &str| {
        let mut s = std::net::TcpStream::connect(addr).unwrap();
        s.write_all(raw.as_bytes()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    };
    let out = ask(
        "HEAD / HTTP/1.1\r\nHost: plugin-floorplan.example.org\r\nSec-Fetch-Mode: navigate\r\n\r\n",
    );
    assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
    assert!(
        out.contains("Cache-Control: private, no-store\r\n") && out.ends_with("\r\n\r\n"),
        "{out}"
    );
    let out = ask("GET / HTTP/1.0\r\nbad header\r\n\r\n");
    assert!(
        out.starts_with("HTTP/1.1 400 ") && out.contains("Vary: Cookie\r\n"),
        "{out}"
    );
    let out = ask(
        "POST /_signout HTTP/1.1\r\nHost: plugins.example.org\r\nContent-Length: 99999\r\n\r\n",
    );
    assert!(
        out.starts_with("HTTP/1.1 413 ") && out.contains("X-Content-Type-Options: nosniff\r\n"),
        "{out}"
    );
}

#[test]
fn a_path_is_served_only_inside_a_granted_views_folder() {
    let seg = |p: &str| -> Vec<String> {
        p.split('/')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    };
    let views = |v: &[(&str, bool)]| -> Vec<(String, bool)> {
        v.iter().map(|(p, g)| (p.to_string(), *g)).collect()
    };
    let v1 = views(&[("/", true)]);
    for p in ["", "index.html", "health/", "a/b.js"] {
        assert!(catalog::covered(&v1, &seg(p)), "{p}");
    }
    let health = views(&[("/health/", true)]);
    for (p, yes) in [
        ("health", true),
        ("health/index.html", true),
        ("health/a/b.js", true),
        ("", false),
        ("index.html", false),
        ("healthy/x", false),
        ("other/health/x", false),
    ] {
        assert_eq!(catalog::covered(&health, &seg(p)), yes, "{p}");
    }
    // The longest folder decides: / granted and /health/ not leaves
    // /health/ 404, never served by the grant of /.
    let both = views(&[("/", true), ("/health/", false)]);
    for (p, yes) in [
        ("index.html", true),
        ("map/x.js", true),
        ("health", false),
        ("health/x", false),
        ("healthy/x", true),
    ] {
        assert_eq!(catalog::covered(&both, &seg(p)), yes, "{p}");
    }
    let inner = views(&[("/", false), ("/health/", true)]);
    assert!(catalog::covered(&inner, &seg("health/x")));
    assert!(!catalog::covered(&inner, &seg("index.html")));
    assert!(!catalog::covered(&[], &seg("index.html")));
}

/// Commits the manifest in a repository under `home` once for each label,
/// clones it bare as the plugin's `repo.git`, and gives the commits.
fn plugin_repo(home: &std::path::Path, labels: &[&str]) -> Vec<String> {
    let git = |dir: &std::path::Path, args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.org"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    let work = home.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "-q"]);
    let mut commits = Vec::new();
    for label in labels {
        let m = format!(
            r#"{{"plugin": 1, "kind": "batch", "name": "floorplan", "label": "{label}", "build": "Dockerfile", "asks": ["view"], "timeout": 600, "view": {{"label": "Map"}}}}"#
        );
        std::fs::write(work.join("tehas-plugin.json"), m).unwrap();
        git(&work, &["add", "tehas-plugin.json"]);
        git(&work, &["commit", "-qm", label]);
        commits.push(git(&work, &["rev-parse", "HEAD"]));
    }
    let repo = home.join("plugins/floorplan/repo.git");
    git(
        home,
        &[
            "clone",
            "-q",
            "--bare",
            work.to_str().unwrap(),
            repo.to_str().unwrap(),
        ],
    );
    commits
}

#[test]
fn a_view_is_served_only_when_it_was_made_at_the_pin() {
    let dir = TestDir::new("gate-made-at");
    let at = plugin_repo(&dir, &["Floorplan", "Floorplan 2"]);
    let (c, d) = (at[0].clone(), at[1].clone());
    // The pin moved from C to D, which is built, ready and granted.
    std::fs::create_dir_all(dir.join("plugins/floorplan")).unwrap();
    std::fs::write(
        dir.join("plugins/floorplan/state.json"),
        format!(r#"{{"api":1,"state":"ready","commit":"{d}"}}"#),
    )
    .unwrap();
    let pins = tehas_core::plugin::pins(&format!("floorplan=https://example.test/fp@{d}")).unwrap();
    let mut grants = HashMap::new();
    grants.insert(
        "floorplan".to_string(),
        tehas_core::plugin::grants(&format!("{d} view")).unwrap(),
    );
    let f = fixture_of(Level::Write, DOMAIN, false, dir, |home| {
        Box::new(catalog::Installed::new(home.to_path_buf(), pins, grants))
    });
    let c_cookie = sign_in(&f, P, T0);
    let stamp = tehas_ops::plugin::runs::view_commit(&f.dir.join("plugins/floorplan/view"));
    // A view from before the record says no commit.
    assert_eq!(f.gate.answer(&nav(P, "/", &c_cookie), T0).status, 404);
    std::fs::create_dir_all(stamp.parent().unwrap()).unwrap();
    std::fs::write(&stamp, format!("{c}\n")).unwrap();
    assert_eq!(f.gate.answer(&nav(P, "/", &c_cookie), T0).status, 404);
    assert_eq!(f.gate.answer(&nav(P, "/app.js", &c_cookie), T0).status, 404);
    // A refresh at D swaps its view in; state.json is as it was.
    std::fs::write(&stamp, format!("{d}\n")).unwrap();
    assert_eq!(f.gate.answer(&nav(P, "/", &c_cookie), T0).status, 200);
    // The record itself is the gate's path, never served.
    assert_eq!(
        f.gate
            .answer(&nav(P, "/_tehas/commit", &c_cookie), T0)
            .status,
        404
    );
}
