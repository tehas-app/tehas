//! The plugins' gate, `tehas-plugin-gate` (docs/specs/2026-11-21-plugins-web.md):
//! the one program that answers a plugin's names. It serves a plugin's view,
//! its kit, its actions and its hooks' findings, each only to a person GitHub
//! says has access to the factory's repository, and the sign-in at
//! `plugins.<domain>`.
//!
//! [`Gate::answer`] makes every answer from a request and the time; the
//! listener in [`serve`] only reads and writes. Every answer carries the
//! headers of [`base`], whatever it is.

pub mod auth;
pub mod catalog;
pub mod csp;
pub mod frame;
pub mod github;
pub mod http;
pub mod page;
pub mod path;

use auth::{Memory, Session, Site};
use catalog::Catalog;
use github::{GitHub, Level, Permission, Person};
use http::{Body, Request, Response};
use path::Route;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// A press within this many seconds of the plugin's last is 429.
pub const PRESS_GAP: u64 = 10 * 60;
/// The largest page of a view the gate reads to work out its policy.
pub const PAGE_MAX: u64 = 8 << 20;
/// The largest verdict kept for a hook.
pub const VERDICT_MAX: u64 = 64 * 1024;
/// The answer to a name that is none of the settings' names, as the edge
/// router says it for a name no plugin can have.
pub const NO_PLUGIN: &str = "No plugin is set at this address.\n";

/// What a press came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Press {
    Asked,
    Running,
    /// The refresh could not be asked, in the gate's words.
    Failed(String),
}

/// Asks for a plugin's refresh, as `tehas plugin run` does. The runner
/// implements it.
pub trait Refresh: Send + Sync {
    fn ask(&self, plugin: &str, action: &str, login: &str) -> Press;
}

/// The settings the gate is given.
#[derive(Debug, Clone)]
pub struct Settings {
    /// TEHAS_DOMAIN.
    pub domain: String,
    /// TEHAS_EDGE_ONE_LEVEL.
    pub one_level: bool,
    /// The names of TEHAS_PLUGINS.
    pub plugins: Vec<String>,
    /// TEHAS_PLUGIN_ACCESS.
    pub access: Level,
    /// TEHAS_HOME.
    pub home: PathBuf,
    /// The App's client id.
    pub client_id: String,
    /// GitHub's web address, `https://github.com`.
    pub web: String,
}

pub struct Gate {
    settings: Settings,
    github: Box<dyn GitHub>,
    catalog: Box<dyn Catalog>,
    refresh: Box<dyn Refresh>,
    memory: Mutex<Memory>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The headers of every answer, and never an `Access-Control-*` one.
pub fn base(r: &mut Response, dashboard: &str) {
    r.headers
        .retain(|(k, _)| !k.to_ascii_lowercase().starts_with("access-control-"));
    r.set("Cache-Control", "private, no-store");
    r.set("Vary", "Cookie");
    r.set("Cross-Origin-Resource-Policy", "same-origin");
    r.set("X-Content-Type-Options", "nosniff");
    r.set("Referrer-Policy", "no-referrer");
    if r.header("Permissions-Policy").is_none() {
        r.set("Permissions-Policy", csp::PERMISSIONS);
    }
    if r.header("Content-Security-Policy").is_none() {
        r.set("Content-Security-Policy", &csp::file_policy(dashboard));
    }
}

/// The content type of a file of a view or the kit, by its name.
pub fn kind(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map_or("", |(_, e)| e)
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "txt" | "md" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn is_html(name: &str) -> bool {
    kind(name).starts_with("text/html")
}

/// A page the gate or the plugin made, with its policy worked out from its
/// bytes. A page with no policy is refused with the reason.
fn document(status: u16, bytes: Vec<u8>, dashboard: &str) -> Response {
    match csp::of(&bytes, dashboard) {
        Ok(policy) => {
            let mut r = Response::with(status, "text/html; charset=utf-8", bytes);
            r.set("Content-Security-Policy", &policy);
            r.set("Permissions-Policy", csp::PAGE_PERMISSIONS);
            r.set("Cross-Origin-Opener-Policy", csp::OPENER);
            r
        }
        Err(why) => document(
            500,
            page::refusal(
                "This page is refused",
                &format!("The plugin's page is not served: {}.", why.why()),
            ),
            dashboard,
        ),
    }
}

fn see(location: &str) -> Response {
    let mut r = Response::empty(303);
    r.set("Location", location);
    r
}

/// The fetch metadata of a request.
struct Fetch<'a> {
    mode: Option<&'a str>,
    site: Option<&'a str>,
    origin: Option<&'a str>,
}

impl Fetch<'_> {
    fn navigation(&self) -> bool {
        self.mode == Some("navigate")
    }
}

impl Gate {
    pub fn new(
        settings: Settings,
        github: Box<dyn GitHub>,
        catalog: Box<dyn Catalog>,
        refresh: Box<dyn Refresh>,
    ) -> Gate {
        Gate {
            settings,
            github,
            catalog,
            refresh,
            memory: Mutex::new(Memory::default()),
        }
    }

    pub fn dashboard(&self) -> String {
        format!("https://{}", self.settings.domain)
    }

    fn name_of(&self, label: &str) -> String {
        tehas_core::settings::edge_name(label, &self.settings.domain, self.settings.one_level)
    }

    /// The name of a plugin's view.
    pub fn plugin_host(&self, plugin: &str) -> String {
        self.name_of(&format!("plugin-{plugin}"))
    }

    /// The name of the sign-in.
    pub fn signin_host(&self) -> String {
        self.name_of("plugins")
    }

    /// The site of a whole `Host`, against the names the settings make.
    fn site(&self, host: &str) -> Option<Site> {
        if host == self.signin_host() {
            return Some(Site::SignIn);
        }
        self.settings
            .plugins
            .iter()
            .find(|p| self.plugin_host(p) == host)
            .map(|p| Site::Plugin(p.clone()))
    }

    fn host_of(&self, site: &Site) -> String {
        match site {
            Site::SignIn => self.signin_host(),
            Site::Plugin(p) => self.plugin_host(p),
        }
    }

    /// The answer to `req` at `now`, in seconds.
    pub fn answer(&self, req: &Request, now: u64) -> Response {
        let dashboard = self.dashboard();
        let mut r = self.route(req, now);
        if r.header("Content-Type")
            .is_some_and(|k| k.starts_with("text/html"))
            && r.header("Content-Security-Policy").is_none()
        {
            let bytes = r.bytes();
            r = document(r.status, bytes, &dashboard);
        }
        base(&mut r, &dashboard);
        r
    }

    fn route(&self, req: &Request, now: u64) -> Response {
        let one = |name: &str| req.one(name);
        let (Ok(host), Ok(mode), Ok(site), Ok(origin)) = (
            one("Host"),
            one("Sec-Fetch-Mode"),
            one("Sec-Fetch-Site"),
            one("Origin"),
        ) else {
            return Response::empty(400);
        };
        let Some(at) = host.and_then(|h| self.site(&h.to_ascii_lowercase())) else {
            return Response::with(
                404,
                "text/plain; charset=utf-8",
                NO_PLUGIN.as_bytes().to_vec(),
            );
        };
        if req.method == "OPTIONS" {
            return Response::empty(403);
        }
        if !matches!(req.method.as_str(), "GET" | "HEAD" | "POST") {
            return Response::empty(405);
        }
        let fetch = Fetch { mode, site, origin };
        let own = format!("https://{}", self.host_of(&at));
        if !fetch.navigation() && fetch.site != Some("same-origin") {
            return Response::empty(403);
        }
        if req.method == "POST"
            && (fetch.origin != Some(own.as_str()) || fetch.site != Some("same-origin"))
        {
            return Response::empty(403);
        }
        // The one WebSocket path, `/_relay/<id>` and `/_console/<id>`, joins
        // here once it is built, under the rules above and a session.
        if req.has("Upgrade") {
            return Response::empty(404);
        }
        let Some(p) = path::normalise(&req.target) else {
            return Response::empty(404);
        };
        match at {
            Site::SignIn => self.signin(req, &p, now),
            Site::Plugin(name) => self.plugin(req, &name, &p, &fetch, now),
        }
    }

    // --- Who gets in ------------------------------------------------------

    /// Whether `person` has access now. Err: GitHub could not say, which is
    /// a refusal.
    fn access(&self, person: &Person) -> Result<bool, String> {
        let access = self.settings.access;
        if access <= Level::Triage && self.github.public()? {
            eprintln!(
                "tehas-plugin-gate: the repository is public, so read and triage let no one in"
            );
            return Ok(false);
        }
        match self.github.permission(&person.login)? {
            Permission::None => Ok(false),
            Permission::Of(id, level, role) => {
                let role: String = role
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || " _-".contains(*c))
                    .take(64)
                    .collect();
                eprintln!("tehas-plugin-gate: {} has the role {role:?}", person.login);
                Ok(id == person.id && level.is_some_and(|l| l >= access))
            }
        }
    }

    /// The live session of the request's cookie on `site`, checked again if
    /// its last check is older than 10 minutes, or now with `fresh`. Ok(None):
    /// no session. Err: GitHub could not say.
    fn session(
        &self,
        req: &Request,
        site: &Site,
        fresh: bool,
        now: u64,
    ) -> Result<Option<Session>, String> {
        let Some(id) = auth::cookie(&req.headers, auth::SESSION) else {
            return Ok(None);
        };
        let Some(s) = lock(&self.memory).session(&id, site, now) else {
            return Ok(None);
        };
        if !fresh && now.saturating_sub(s.checked) < auth::CHECK {
            return Ok(Some(s));
        }
        if self.access(&s.person)? {
            lock(&self.memory).checked(s.person.id, now);
            Ok(Some(s))
        } else {
            lock(&self.memory).end(s.person.id);
            Ok(None)
        }
    }

    fn github_refusal(&self) -> Response {
        Response::with(
            503,
            "text/html; charset=utf-8",
            page::refusal(
                "GitHub could not be asked",
                "GitHub did not answer whether you have access, so you are not let in. Try again in a minute.",
            ),
        )
    }

    // --- plugins.<domain> -------------------------------------------------

    fn signin(&self, req: &Request, p: &path::Path, now: u64) -> Response {
        let segs: Vec<&str> = p.0.iter().map(String::as_str).collect();
        let get = matches!(req.method.as_str(), "GET" | "HEAD");
        match (segs.as_slice(), get) {
            (["_auth", "start"], true) => self.start(req),
            (["_auth", "pass"], true) => self.pass(req, now),
            (["_auth", "callback"], true) => self.callback(req, now),
            (["_session"], true) => self.signed(req, &Site::SignIn, now),
            (["_signout"], false) => self.signout(req, &Site::SignIn, now),
            (["_auth", _] | ["_session"] | ["_signout"], _) => Response::empty(405),
            _ => Response::empty(404),
        }
    }

    /// `/_auth/start?for=<name>&back=<path>`: the dock's link. The name is
    /// one of the settings', and the browser goes on to that name's ask.
    fn start(&self, req: &Request) -> Response {
        let asked = path::query(&req.target, "for").unwrap_or_default();
        let Some(name) = self.settings.plugins.iter().find(|p| **p == asked) else {
            return Response::empty(404);
        };
        let back = path::query(&req.target, "back")
            .and_then(|b| path::back(&b))
            .unwrap_or_else(|| "/".to_string());
        see(&format!(
            "https://{}/_auth/ask?back={}",
            self.plugin_host(name),
            path::component(&back)
        ))
    }

    /// `/_auth/pass?state=<ask>&back=<path>`: with a session here, a code for
    /// the ask; without, GitHub's web flow first.
    fn pass(&self, req: &Request, now: u64) -> Response {
        let state = path::query(&req.target, "state").unwrap_or_default();
        let back = path::query(&req.target, "back")
            .and_then(|b| path::back(&b))
            .unwrap_or_else(|| "/".to_string());
        if !auth::is_id(&state) || lock(&self.memory).asked(&state, now).is_none() {
            return self.stale();
        }
        match self.session(req, &Site::SignIn, false, now) {
            Err(_) => return self.github_refusal(),
            Ok(Some(s)) => return self.hand(&state, &back, &s, now),
            Ok(None) => {}
        }
        let Some((flow, nonce)) = lock(&self.memory).flow(&state, &back, now) else {
            return Response::empty(500);
        };
        let callback = format!("https://{}/_auth/callback", self.signin_host());
        let mut r = see(&format!(
            "{}/login/oauth/authorize?client_id={}&redirect_uri={}&state={flow}&allow_signup=false",
            self.settings.web,
            path::component(&self.settings.client_id),
            path::component(&callback),
        ));
        r.add(
            "Set-Cookie",
            &auth::set_cookie(auth::SIGNIN, &nonce, Some(auth::FLOW)),
        );
        r
    }

    /// Sends the browser to the asking name with a code for the ask.
    fn hand(&self, state: &str, back: &str, s: &Session, now: u64) -> Response {
        let Some((code, plugin)) = lock(&self.memory).code(state, back, s, now) else {
            return self.stale();
        };
        see(&format!(
            "https://{}/_auth/take?code={code}",
            self.plugin_host(&plugin)
        ))
    }

    fn stale(&self) -> Response {
        Response::with(
            403,
            "text/html; charset=utf-8",
            page::refusal(
                "This sign-in is used or too old",
                "Close this tab and sign in again from the plugin's page.",
            ),
        )
    }

    /// GitHub's callback: the flow this browser started, the person's token
    /// for its code, who they are, and their access. The token is dropped
    /// here.
    fn callback(&self, req: &Request, now: u64) -> Response {
        let state = path::query(&req.target, "state").unwrap_or_default();
        let code = path::query(&req.target, "code").unwrap_or_default();
        let nonce = auth::cookie(&req.headers, auth::SIGNIN);
        let Some(flow) = lock(&self.memory).finish(&state, nonce.as_deref(), now) else {
            return self.stale();
        };
        let good = !code.is_empty()
            && code.len() <= 128
            && code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if !good {
            return self.stale();
        }
        let person = match self.github.token(&code).and_then(|t| self.github.user(&t)) {
            Ok(p) => p,
            Err(_) => return self.github_refusal(),
        };
        let mut r = match self.access(&person) {
            Err(_) => self.github_refusal(),
            Ok(false) => Response::with(
                403,
                "text/html; charset=utf-8",
                page::refusal(
                    "No access",
                    &format!(
                        "The GitHub account {} does not have the access to the factory's repository that this factory's plugins need.",
                        person.login
                    ),
                ),
            ),
            Ok(true) => {
                let opened = lock(&self.memory).open(Site::SignIn, person.clone(), now, now);
                match opened {
                    None => Response::empty(500),
                    Some(id) => {
                        let s = Session {
                            site: Site::SignIn,
                            person,
                            started: now,
                            used: now,
                            checked: now,
                        };
                        let mut r = self.hand(&flow.ask, &flow.back, &s, now);
                        r.add("Set-Cookie", &auth::set_cookie(auth::SESSION, &id, None));
                        r
                    }
                }
            }
        };
        r.add("Set-Cookie", &auth::clear_cookie(auth::SIGNIN));
        r
    }

    fn signed(&self, req: &Request, site: &Site, now: u64) -> Response {
        match self.session(req, site, false, now) {
            Ok(Some(_)) => Response::with(200, "application/json", br#"{"signed": true}"#.to_vec()),
            Ok(None) => Response::empty(401),
            Err(_) => Response::empty(503),
        }
    }

    /// Ends every session of the person, on every name, and clears the
    /// cookie.
    fn signout(&self, req: &Request, site: &Site, now: u64) -> Response {
        let id = auth::cookie(&req.headers, auth::SESSION);
        let mut memory = lock(&self.memory);
        let person = id.and_then(|id| memory.session(&id, site, now));
        let mut r = match person {
            Some(s) => {
                memory.end(s.person.id);
                Response::with(200, "application/json", br#"{"signed": false}"#.to_vec())
            }
            None => Response::empty(401),
        };
        r.add("Set-Cookie", &auth::clear_cookie(auth::SESSION));
        r
    }

    // --- plugin-<name>.<domain> -------------------------------------------

    fn plugin(
        &self,
        req: &Request,
        name: &str,
        p: &path::Path,
        fetch: &Fetch,
        now: u64,
    ) -> Response {
        let Some(facts) = self.catalog.facts(name) else {
            return Response::empty(404);
        };
        let site = Site::Plugin(name.to_string());
        let get = matches!(req.method.as_str(), "GET" | "HEAD");
        let route = path::route(p);
        match (&route, get) {
            (Route::Auth(step), true) if step == "ask" => return self.ask(req, name, now),
            (Route::Auth(step), true) if step == "take" => return self.take(req, name, now),
            (Route::Auth(_), _) => return Response::empty(404),
            _ => {}
        }
        let session = match self.session(req, &site, false, now) {
            Err(_) => {
                return if fetch.navigation() && get {
                    self.github_refusal()
                } else {
                    Response::empty(503)
                }
            }
            Ok(s) => s,
        };
        let Some(session) = session else {
            if fetch.navigation() && get && !matches!(route, Route::Session | Route::Signout) {
                let raw = req
                    .target
                    .split_once('?')
                    .map_or(req.target.as_str(), |(p, _)| p);
                let back = path::back(raw).unwrap_or_else(|| "/".to_string());
                let start = format!(
                    "https://{}/_auth/start?for={name}&back={}",
                    self.signin_host(),
                    path::component(&back)
                );
                return Response::with(
                    200,
                    "text/html; charset=utf-8",
                    page::sign_in(name, &start),
                );
            }
            return Response::empty(401);
        };
        match (route, get) {
            (Route::File(segs), true) if facts.view => self.file(name, &segs),
            (Route::Kit(file), true) if facts.kit => self.kit(&file),
            (Route::Frame(segs), true) if facts.keep => frame::serve(
                &self.settings.home,
                name,
                &segs,
                &format!("https://{}", self.plugin_host(name)),
            ),
            (Route::Actions, true) => Response::with(
                200,
                "application/json",
                json!({"api": 1, "actions": facts.actions})
                    .to_string()
                    .into_bytes(),
            ),
            (Route::Action(id), false) => self.press(name, &id, &facts, &session, now),
            (Route::Hooks(pr), true) => self.hooks(name, pr),
            (Route::Session, true) => self.signed(req, &site, now),
            (Route::Signout, false) => self.signout(req, &site, now),
            (Route::Action(_) | Route::Signout, true)
            | (Route::Actions | Route::Session, false) => Response::empty(405),
            _ => Response::empty(404),
        }
    }

    /// `/_auth/ask?back=<path>`: sets the ask's nonce here and sends the
    /// browser to the sign-in with its hash.
    fn ask(&self, req: &Request, name: &str, now: u64) -> Response {
        let back = path::query(&req.target, "back")
            .and_then(|b| path::back(&b))
            .unwrap_or_else(|| "/".to_string());
        let Some(nonce) = lock(&self.memory).ask(name, now) else {
            return Response::empty(500);
        };
        let mut r = see(&format!(
            "https://{}/_auth/pass?state={}&back={}",
            self.signin_host(),
            auth::sha256(&nonce),
            path::component(&back)
        ));
        r.add(
            "Set-Cookie",
            &auth::set_cookie(auth::ASK, &nonce, Some(auth::FLOW)),
        );
        r
    }

    /// `/_auth/take?code=<code>`: a code for this name and this browser's
    /// ask opens a session here.
    fn take(&self, req: &Request, name: &str, now: u64) -> Response {
        let code = path::query(&req.target, "code").unwrap_or_default();
        let nonce = auth::cookie(&req.headers, auth::ASK);
        let mut memory = lock(&self.memory);
        let taken = auth::is_id(&code)
            .then(|| memory.take(&code, name, nonce.as_deref(), now))
            .flatten();
        let Some(c) = taken else {
            drop(memory);
            return self.stale();
        };
        let site = Site::Plugin(name.to_string());
        let Some(id) = memory.open(site, c.person, c.checked, now) else {
            return Response::empty(500);
        };
        drop(memory);
        let mut r = see(&format!("https://{}{}", self.plugin_host(name), c.back));
        r.add("Set-Cookie", &auth::set_cookie(auth::SESSION, &id, None));
        r.add("Set-Cookie", &auth::clear_cookie(auth::ASK));
        r
    }

    fn press(
        &self,
        name: &str,
        id: &str,
        facts: &catalog::Facts,
        s: &Session,
        now: u64,
    ) -> Response {
        if !facts.actions.iter().any(|a| a == id) {
            return Response::empty(403);
        }
        match self.access(&s.person) {
            Ok(true) => lock(&self.memory).checked(s.person.id, now),
            Ok(false) => {
                lock(&self.memory).end(s.person.id);
                return Response::empty(403);
            }
            Err(_) => return Response::empty(403),
        }
        let mut memory = lock(&self.memory);
        if memory
            .presses
            .get(name)
            .is_some_and(|t| now.saturating_sub(*t) < PRESS_GAP)
        {
            return Response::empty(429);
        }
        match self.refresh.ask(name, id, &s.person.login) {
            Press::Asked => {
                memory.presses.insert(name.to_string(), now);
                eprintln!(
                    "tehas-plugin-gate: {} pressed {id} of {name}",
                    s.person.login
                );
                Response::empty(202)
            }
            Press::Running => Response::empty(409),
            Press::Failed(why) => {
                eprintln!("tehas-plugin-gate: a press of {id} of {name} was not asked: {why}");
                Response::empty(503)
            }
        }
    }

    /// A file of the view: `index.html` for a directory, never through a
    /// link, never outside `view/`.
    fn file(&self, name: &str, segs: &[String]) -> Response {
        let mut at = catalog::dir(&self.settings.home, name).join("view");
        for s in segs {
            at.push(s);
            if std::fs::symlink_metadata(&at).map_or(true, |m| m.file_type().is_symlink()) {
                return Response::empty(404);
            }
        }
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.is_dir()) {
            at.push("index.html");
        }
        self.serve_file(&at)
    }

    fn serve_file(&self, at: &std::path::Path) -> Response {
        let Ok(meta) = std::fs::symlink_metadata(at) else {
            return Response::empty(404);
        };
        if !meta.is_file() {
            return Response::empty(404);
        }
        let name = at
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_html(&name) {
            if meta.len() > PAGE_MAX {
                return Response::empty(404);
            }
            return match std::fs::read(at) {
                Ok(bytes) => document(200, bytes, &self.dashboard()),
                Err(_) => Response::empty(404),
            };
        }
        match std::fs::File::open(at) {
            Ok(f) => {
                let mut r = Response::empty(200);
                r.set("Content-Type", kind(&name));
                r.body = Body::File(f, meta.len());
                r
            }
            Err(_) => Response::empty(404),
        }
    }

    /// A file of the installed kit's MANIFEST.
    fn kit(&self, file: &str) -> Response {
        let dir = self.settings.home.join("fun/kit");
        let listed = std::fs::read_to_string(dir.join("MANIFEST"))
            .ok()
            .and_then(|m| catalog::kit_files(&m))
            .is_some_and(|files| files.iter().any(|f| f == file));
        if !listed {
            return Response::empty(404);
        }
        let mut at = dir;
        for s in file.split('/') {
            at.push(s);
            if std::fs::symlink_metadata(&at).map_or(true, |m| m.file_type().is_symlink()) {
                return Response::empty(404);
            }
        }
        self.serve_file(&at)
    }

    /// The findings of the plugin's last hook on `pr`, in a page the gate
    /// makes.
    fn hooks(&self, name: &str, pr: u64) -> Response {
        let at = catalog::dir(&self.settings.home, name)
            .join("hooks")
            .join(format!("{pr}.verdict.json"));
        let bytes = match std::fs::symlink_metadata(&at) {
            Ok(m) if m.is_file() && m.len() <= VERDICT_MAX => std::fs::read(&at).ok(),
            _ => None,
        };
        match bytes.and_then(|b| page::findings(name, pr, &b)) {
            Some(page) => Response::with(200, "text/html; charset=utf-8", page),
            None => Response::with(
                404,
                "text/html; charset=utf-8",
                page::refusal(
                    "No findings",
                    &format!("{name} has no findings kept for #{pr}."),
                ),
            ),
        }
    }
}

/// Serves the gate on `listener` until the process ends, one thread a
/// connection, at most `most` at once.
pub fn serve(listener: std::net::TcpListener, gate: std::sync::Arc<Gate>, most: usize) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let open = std::sync::Arc::new(AtomicUsize::new(0));
    for conn in listener.incoming() {
        let Ok(mut stream) = conn else { continue };
        if open.fetch_add(1, Ordering::SeqCst) >= most {
            open.fetch_sub(1, Ordering::SeqCst);
            let mut r = Response::empty(503);
            base(&mut r, &gate.dashboard());
            http::write(&mut stream, r, false);
            continue;
        }
        let (g, o) = (std::sync::Arc::clone(&gate), std::sync::Arc::clone(&open));
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + http::IO_TIMEOUT;
            let (r, head) = match http::read(&mut stream, deadline) {
                Ok(req) => (g.answer(&req, now()), req.method == "HEAD"),
                Err(status) => {
                    let mut r = Response::empty(status);
                    base(&mut r, &g.dashboard());
                    (r, false)
                }
            };
            http::write(&mut stream, r, head);
            o.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests;
