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

/// Signs in with GitHub alone at `now` — no second hand-off — and returns the
/// person's cookie on `host` and on `plugins.<domain>`. The two names open at
/// the same hour, which is the one the 12-hour limit runs from.
fn sign_in_github(f: &Fix, host: &str, now: u64) -> (String, String) {
    let ask = f.gate.answer(&nav(host, "/_auth/ask?back=/sub/", ""), now);
    assert_eq!(ask.status, 303);
    let nonce = set(&ask, auth::ASK).unwrap();
    let pass = f.gate.answer(&nav(S, &target(&location(&ask)), ""), now);
    assert_eq!(pass.status, 303, "{}", location(&pass));
    let github = location(&pass);
    let signin = set(&pass, auth::SIGNIN).unwrap();
    let back = format!("/_auth/callback?code=good&state={}", q(&github, "state"));
    let cb = f
        .gate
        .answer(&nav(S, &back, &format!("{}={signin}", auth::SIGNIN)), now);
    assert_eq!(cb.status, 303);
    let s_session = set(&cb, auth::SESSION).unwrap();
    let take = location(&cb);
    let took = f.gate.answer(
        &nav(host, &target(&take), &format!("{}={nonce}", auth::ASK)),
        now,
    );
    assert_eq!(took.status, 303);
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
    assert_eq!(pp, csp::PERMISSIONS, "Permissions-Policy on {what}");
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
    assert_eq!(r.header("Permissions-Policy"), Some(csp::PERMISSIONS));
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
fn every_answer_carries_the_same_permissions_policy_a_page_or_not() {
    let f = fixture();
    let c = sign_in(&f, P, T0);
    std::fs::write(
        f.dir.join("plugins/floorplan/view/bad.html"),
        "<a onclick=x()>",
    )
    .unwrap();
    let keep = f.dir.join("plugins/floorplan/keep/p-1");
    std::fs::create_dir_all(&keep).unwrap();
    std::fs::write(keep.join("v2.html"), "<p>kept</p>").unwrap();
    let answers = [
        ("page", nav(P, "/", &c), 200),
        ("refused page", nav(P, "/bad.html", &c), 500),
        ("file", fetch(P, "/app.js", &c), 200),
        ("framed file", fetch(P, "/_frame/p-1/v2.html", &c), 200),
        ("kit", fetch(P, "/_kit/press.js", &c), 200),
        ("actions", fetch(P, "/_actions", &c), 200),
        ("hooks", nav(P, "/_hooks/12", &c), 404),
        ("redirect", nav(P, "/_auth/ask", ""), 303),
        ("start", nav(S, "/_auth/start?for=floorplan", ""), 303),
        ("no session", fetch(P, "/app.js", ""), 401),
        ("missing", fetch(P, "/nope.js", &c), 404),
    ];
    for (what, req, status) in answers {
        let r = f.gate.answer(&req, T0);
        assert_eq!(r.status, status, "{what}");
        assert_eq!(
            r.header("Permissions-Policy"),
            Some(csp::PERMISSIONS),
            "{what}"
        );
    }
    // A busy answer, made outside `answer`, carries it too.
    let mut busy = Response::empty(503);
    base(&mut busy, "https://example.org");
    assert_eq!(busy.header("Permissions-Policy"), Some(csp::PERMISSIONS));
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

impl Fix {
    /// The steps of a hand-off up to the code, at `now`: the ask's nonce
    /// cookie and the take's address. The person is read at `seen` first, so
    /// the session on the sign-in's name is live when the pass reads it.
    fn hand_off_from(
        &self,
        host: &str,
        back: &str,
        s: &str,
        seen: u64,
        now: u64,
    ) -> (String, String) {
        let ask = self
            .gate
            .answer(&nav(host, &format!("/_auth/ask?back={back}"), ""), now);
        assert_eq!(ask.status, 303, "the ask at {now}");
        let nonce = set(&ask, auth::ASK).expect("the ask set its nonce");
        // The browser carries the person's cookie on the sign-in's name; it
        // was last read at `seen`, which is what makes the pass a hand-off
        // rather than a fresh sign-in.
        self.busy(S, s, seen);
        let pass = self.gate.answer(&nav(S, &target(&location(&ask)), s), now);
        assert!(
            location(&pass).starts_with(&format!("https://{host}/_auth/take?code=")),
            "the pass at {now} did not hand the code to {host}: {}",
            location(&pass)
        );
        (format!("{}={nonce}", auth::ASK), target(&location(&pass)))
    }

    /// One read of a session at `t`, which keeps it from idling out, and the
    /// access check behind it answered good.
    fn busy(&self, host: &str, c: &str, t: u64) {
        self.hub.set(|h| {
            h.level = Some(Ok(Some(Level::Write)));
            h.other_id = None;
        });
        assert_eq!(
            self.gate.answer(&fetch(host, "/_session", c), t).status,
            200,
            "{host} at {t}"
        );
    }
}

/// The steps of a hand-off up to the code: the ask's nonce and the take's
/// address. The person signs in on the plugin's own name at `now` first, so
/// `plugins.<domain>` holds a session to hand off from.
fn hand_off(f: &Fix, host: &str, back: &str, now: u64) -> (String, String) {
    let (_, s) = sign_in_full(f, P, now);
    f.hand_off_from(host, back, &s, now, now)
}

/// Keeps a session alive on every check, and keeps GitHub's answer good, so
/// only the 12-hour limit can end it. The last moment of the 12 hours answers
/// 200 and the moment after it answers 401.
fn assert_ends_at_the_limit(f: &Fix, host: &str, c: &str, started: u64) {
    let mut t = started;
    while t < started + auth::TOTAL {
        t = (t + auth::CHECK).min(started + auth::TOTAL);
        f.hub.set(|h| {
            h.level = Some(Ok(Some(Level::Write)));
            h.other_id = None;
        });
        assert_eq!(
            f.gate.answer(&fetch(host, "/_session", c), t).status,
            200,
            "{host} at {t}"
        );
    }
    f.hub.set(|h| {
        h.level = Some(Ok(Some(Level::Write)));
        h.other_id = None;
    });
    let after = f
        .gate
        .answer(&fetch(host, "/_session", c), started + auth::TOTAL + 1);
    assert_eq!(after.status, 401, "{host} outlives the 12 hours");
}

/// Keeps a session busy from `started` to `until`, asking half an idle window
/// apart and answering 200 the whole way, so only the 12-hour limit could end
/// it.
fn keep_busy(f: &Fix, host: &str, c: &str, started: u64, until: u64) {
    let mut t = started;
    while t <= until {
        f.hub.set(|h| {
            h.level = Some(Ok(Some(Level::Write)));
            h.other_id = None;
        });
        assert_eq!(
            f.gate.answer(&fetch(host, "/_session", c), t).status,
            200,
            "{host} at {t}"
        );
        t += auth::IDLE / 2;
    }
}

#[test]
fn a_hand_off_carries_the_first_sign_ins_start() {
    let f = fixture();
    // One sign-in at T0, kept busy the whole 12 hours so nothing but the
    // limit can end it. 30 minutes short of that end, a second name is handed
    // off from it: its session must end with the first sign-in, not 12 hours
    // after the hand-off. The source session is kept busy right up to the
    // hand-off, which is what an active person does.
    let (d, s) = sign_in_github(&f, P, T0);
    let hand = T0 + auth::TOTAL - 30 * 60;
    // The person is busy on both names they hold up to the hand-off, which is
    // what keeps the limit — and nothing else — the thing that ends them.
    keep_busy(&f, P, &d, T0, hand);
    keep_busy(&f, S, &s, T0, hand);
    let (nonce, take) = f.hand_off_from(Q, "/", &s, hand, hand);
    let took = f.gate.answer(&nav(Q, &take, &nonce), hand);
    assert_eq!(took.status, 303);
    let e = format!("{}={}", auth::SESSION, set(&took, auth::SESSION).unwrap());
    // The new name was live when it took the code…
    assert_eq!(f.gate.answer(&fetch(Q, "/_session", &e), hand).status, 200);
    // …and it ends with the first sign-in.
    assert_eq!(
        f.gate
            .answer(&fetch(Q, "/_session", &e), T0 + auth::TOTAL + 1)
            .status,
        401
    );
    assert_eq!(
        f.gate
            .answer(&fetch(P, "/_session", &d), T0 + auth::TOTAL + 1)
            .status,
        401
    );
}

#[test]
fn a_session_past_twelve_hours_is_refused_rather_than_handed_off() {
    let f = fixture();
    // The carry is bounded by the same 12 hours: a person whose sign-in is
    // already past them is refused at the pass, and signs in with GitHub
    // again, which starts a new 12 hours.
    let (d, s) = sign_in_github(&f, P, T0);
    keep_busy(&f, P, &d, T0, T0 + auth::TOTAL - 30 * 60);
    let late = T0 + auth::TOTAL + 60;
    f.hub.set(|h| {
        h.level = Some(Ok(Some(Level::Write)));
        h.other_id = None;
    });
    let ask = f.gate.answer(&nav(Q, "/_auth/ask?back=/", ""), late);
    assert_eq!(ask.status, 303);
    let pass = f.gate.answer(&nav(S, &target(&location(&ask)), &s), late);
    assert_eq!(pass.status, 303, "{}", location(&pass));
    assert!(
        location(&pass).starts_with("https://github.example/login/oauth/authorize?"),
        "the past session was used rather than refused: {}",
        location(&pass)
    );
}

#[test]
fn a_fresh_github_sign_in_starts_a_new_twelve_hours() {
    let f = fixture();
    // A person who signs in with GitHub starts their own 12 hours: that is a
    // sign-in, not a hand-off, so the name opens at `now`. Q is signed in on
    // its own at `late`, a day after the first sign-in, and it is Q — the name
    // that no hand-off touched — whose session runs the full 12 hours from
    // `late`. P's own session keeps the limit of the first sign-in.
    let (d, _) = sign_in_github(&f, P, T0);
    let late = T0 + auth::TOTAL + 3600;
    let (q, qs) = sign_in_github(&f, Q, late);
    assert_ends_at_the_limit(&f, Q, &q, late);
    assert_ends_at_the_limit(&f, S, &qs, late);
    // The hand-off name from the first sign-in is still bounded by it: the
    // fresh sign-in of another name starts no new hour for it.
    assert_eq!(
        f.gate
            .answer(&fetch(P, "/_session", &d), late + auth::TOTAL + 1)
            .status,
        401
    );
}

#[test]
fn a_hand_off_opens_its_name_checked_at_the_take() {
    let f = fixture();
    // The take opens the new name checked at the take, the same as a sign-in,
    // and not with the source session's last check. The new name has never
    // been read for this person, so the first request on it asks GitHub
    // again rather than riding the rest of the source session's 10 minutes:
    // the manual's rule is that the gate asks again every 10 minutes, and a
    // window measured from a check bought on another name is not that. A
    // person whose access went would otherwise keep the new name to their
    // next poll, and the take is the one moment that decides it.
    let (_, s) = sign_in_github(&f, P, T0);
    f.hub.set(|h| {
        h.level = Some(Ok(Some(Level::Write)));
        h.other_id = None;
    });
    // keep_busy asks half an idle window apart, which lands the source's
    // last read just short of the take. That is the worst case the finding
    // names: the code carries a stamp a hair under 10 minutes old, so the
    // new name inherits a window all but unexpired, and the press below is
    // short-circuited instead of asking who the person is.
    let hand = T0 + 61 * auth::CHECK;
    keep_busy(&f, S, &s, T0, hand - 1);
    let (nonce, take) = f.hand_off_from(Q, "/", &s, hand - 1, hand);
    let took = f.gate.answer(&nav(Q, &take, &nonce), hand);
    assert_eq!(took.status, 303, "{}", location(&took));
    let e = format!("{}={}", auth::SESSION, set(&took, auth::SESSION).unwrap());
    // The take asks GitHub nothing — the code is what was checked, on the
    // source's name.
    let calls = f.hub.calls().1;
    // The press is the request that cannot be answered off a stamp it never
    // earned: it asks GitHub who the person is, and refuses outright while
    // the session behind it has gone unverified for the window. A session
    // opened at the source session's time is inside it here, and the press
    // is a 403 that asked GitHub nothing.
    assert_eq!(
        f.gate
            .answer(&post(Q, "/_actions/refresh", &e), hand + 1)
            .status,
        202,
        "the new name pressed off a check made on another name"
    );
    assert_eq!(f.hub.calls().1, calls + 1, "the press asked no question");
    // Every name of the person still ends with the first sign-in's hour.
    for (host, c) in [(Q, &e), (S, &s)] {
        assert_eq!(
            f.gate
                .answer(&fetch(host, "/_session", c), T0 + auth::TOTAL + 1)
                .status,
            401,
            "{host} outlives the first sign-in"
        );
    }
}

#[test]
fn a_hand_off_opens_its_name_at_the_carried_start() {
    // The table the gate reads, for the two values a hand-off carries: the
    // code is trusted with the first sign-in's hour, and the take opens the
    // new name checked at the take. The source session's check was bought on
    // `plugins.<domain>`, so it is not the new name's standing, and a check
    // of the person stamps every name they hold, which is why the first two
    // lines below are the place to see it.
    let site = Site::Plugin(Q.to_string());
    let person = Person {
        id: 7,
        login: "alice".to_string(),
    };
    // The person signs in at T0; the pass reads them a check window later and
    // makes the code there; the take spends it inside the code's own minute.
    let read = T0 + auth::CHECK;
    let take = read + 1;
    let mut m = Memory::default();
    let src = m
        .open(Site::SignIn, person.clone(), T0, T0, None)
        .expect("the sign-in opens");
    let s = m
        .session(&src, &Site::SignIn, read)
        .expect("the source is live");
    assert_eq!(s.checked, T0, "a read rewrote the session it returned");
    // The pass's own check of the person, which is what the code would carry.
    m.checked(person.id, read);
    let s = m
        .session(&src, &Site::SignIn, read)
        .expect("the source is live");
    assert_eq!(s.checked, read, "the pass did not check the source");
    let ask = auth::sha256("the ask of specs");
    m.asks.insert(
        ask.clone(),
        auth::Ask {
            plugin: "specs".to_string(),
            at: read,
        },
    );
    let (code, name) = m.code(&ask, "/", &s, read).expect("the pass made a code");
    assert_eq!(name, "specs");
    let c = m
        .take(&code, "specs", Some("the ask of specs"), take)
        .expect("the take spent the code");
    assert_eq!(c.signed_in, T0, "the code carried no first sign-in");
    assert_eq!(c.checked, read, "the code carried no check of the source");
    // What the take hands the table: the carried hour, and a check of the
    // take's own. A session opened at `read` instead would answer the next
    // poll unchecked, and a person whose access went would keep the name.
    let id = m
        .open(
            site.clone(),
            c.person.clone(),
            take,
            take,
            Some(c.signed_in),
        )
        .expect("the hand-off opens");
    let s = m.session(&id, &site, take).expect("the hand-off is live");
    assert_eq!(
        s.started, T0,
        "a hand-off did not carry the sign-in's start"
    );
    assert_eq!(
        s.checked, take,
        "a hand-off carried the source session's check"
    );
    // The carried start is clamped to the take: a code cannot open a session
    // in the future, nor one born past its own limit, which would answer 401
    // from its first request with no line anywhere. Each read below is a
    // check window apart, so none is idle, and the clamp is read back one
    // read before the take — where a sign-in's own start would already have
    // been refused — and the session then lives its own 12 hours from there.
    let late = T0 + 8 * 24 * 3600;
    let mut m = Memory::default();
    let id = m
        .open(site.clone(), person.clone(), take, take, Some(late))
        .expect("a late code still opens");
    let mut t = take - auth::CHECK;
    let s = m
        .session(&id, &site, t)
        .expect("the clamped session is live");
    assert_eq!(s.started, take, "the start was not clamped to the take");
    while t < take + auth::TOTAL {
        t = (t + auth::CHECK).min(take + auth::TOTAL);
        assert!(
            m.session(&id, &site, t).is_some(),
            "the clamped session died at {t}"
        );
    }
    assert!(
        m.session(&id, &site, take + auth::TOTAL + 1).is_none(),
        "the clamped session outlived the 12 hours"
    );
    // And a start carried from far in the past ends the name at the hour the
    // first sign-in's own session would have died, not 12 hours after the
    // take. The take is near that hour, so the last window before it is still
    // live and the minute after it is not.
    let mut m = Memory::default();
    let id = m
        .open(site.clone(), person.clone(), take, take, Some(T0))
        .expect("the hand-off opens");
    let end = T0 + auth::TOTAL;
    let mut t = take;
    while t < end {
        t = (t + auth::CHECK).min(end);
        assert!(
            m.session(&id, &site, t).is_some(),
            "the hand-off died before the first sign-in at {t}"
        );
    }
    assert!(
        m.session(&id, &site, end + 1).is_none(),
        "the hand-off outlived the first sign-in"
    );
    // A GitHub sign-in still opens its name at `now`, checked at `now`: the
    // two cases of one open, told apart by the argument alone.
    let mut m = Memory::default();
    let id = m
        .open(site.clone(), person, take, take, None)
        .expect("a sign-in opens");
    let s = m.session(&id, &site, take).expect("the sign-in is live");
    assert_eq!(s.started, take, "a sign-in did not start its own hour");
    assert_eq!(s.checked, take);
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
