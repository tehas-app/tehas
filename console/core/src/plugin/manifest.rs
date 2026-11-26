//! The reader of `tehas-plugin.json`, a plugin's manifest: bytes in, a typed
//! manifest or every reason it is refused out. A field it does not know is
//! refused, so a manifest made for a later tehas never runs half read.
//!
//! The rules are the 2.2 table of
//! `docs/specs/2026-11-25-plugins-2.0.0-operator.md:415-433`, which is also
//! what `api/plugin-manifest.schema.json` holds for a plugin's author. The
//! tests hold the reader and the schema to one verdict, field by field and
//! bound by bound, so neither is the only one saying a rule.
//!
//! Two rules are the reader's alone, because a schema cannot say them: an
//! entry must be asked for by the name it carries, and an ask must be a
//! grant 2.0 knows. Each is named where it is read.

use super::needs::Need;
use super::{is_action_id, is_name, params};
use serde_json::{Map, Value};

/// The largest manifest read.
pub const BYTES_MAX: usize = 65536;
pub const LABEL_MAX: usize = 24;
/// The longest a path into the repository may be.
pub const PATH_MAX: usize = 255;
/// The most asks. The table is silent on a count, so 1.x's bound stands.
pub const ASKS_MAX: usize = 32;
/// The most actions. The table is silent, so 1.x's bound stands.
pub const ACTIONS_MAX: usize = 2;
/// The most skills. The table is silent, so 1.x's bound stands.
pub const SKILLS_MAX: usize = 8;
/// The most views. The table is silent, so 1.x's bound stands.
pub const VIEWS_MAX: usize = 4;
/// The most realms or ports of an adapter.
pub const REALMS_MAX: usize = 16;
/// One run's seconds. The table gives 600 as the most and names no least,
/// so a run of no seconds is refused as surely as one over the most.
pub const TIMEOUT: (u64, u64) = (1, 600);
/// The longest a `solid` version may be. The table says only a version.
pub const SOLID_MAX: usize = 32;
/// The longest a grant name, a library's name or a module's name may be.
pub const WORD_MAX: usize = 64;
/// The longest a version may be, of a vendored library or of `solid`.
pub const VERSION_MAX: usize = 32;
/// The longest line a chat shows for a skill.
pub const ABOUT_MAX: usize = 120;
/// The chat hosts a skill may go to.
pub const CHATS: [&str; 4] = ["obeya", "wizard", "assistant", "plugin"];
/// A call's seconds, of a 1.x tool.
pub const TOOL_TIMEOUT: (u64, u64) = (1, 10);

/// The fields of version 2, in the order of the table they come from
/// (spec 2.2, :415-433). A field outside this is refused.
pub const FIELDS: [&str; 19] = [
    "plugin",
    "spec",
    "name",
    "label",
    "kind",
    "runtime",
    "timeout",
    "asks",
    "driver",
    "actions",
    "views",
    "skills",
    "hooks",
    "wasm",
    "view_wasm",
    "vendor",
    "solid",
    "origins",
    "adapter",
];
/// The fields every manifest has, in the order a missing one is named.
pub const REQUIRED: [&str; 5] = ["plugin", "spec", "name", "label", "asks"];

/// What a manifest is: an ordinary plugin, or part 10's adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plugin,
    Adapter,
}

/// What runs the plugin (spec 2.2 :420, parts 1 and 10.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runtime {
    StarlingMonkey,
    /// Part 10.1: an adapter's one component, and an adapter's alone.
    Component,
}

/// Where a view opens from the plugins menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Open {
    /// In the dock, a frame at the side of the page.
    Dock,
    /// In a page of its own, a new tab.
    Page,
    /// Over the page.
    Full,
}

/// One view of `views` (spec 2.2 :425).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub id: String,
    /// The view's entry in `dist/view/`.
    pub entry: String,
    pub open: Open,
}

/// One press a view may send (spec 2.2 :424, 6.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub id: String,
    /// What a confirm names.
    pub label: String,
    /// The schema of the press's arguments, as [`params::rules`] took it.
    pub args: Option<Value>,
}

/// One hook or gate point (spec 2.2 :427).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hook {
    /// `hook:<when>:<event>`.
    pub at: String,
    pub entry: String,
}

/// One module of `wasm` or `view_wasm` (spec 2.2 :428-429, part 1.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub path: String,
    /// The module's source, for the rebuild layer. Part 1.1 says the
    /// manifest `may` name it. A recipe compiles a folder, so the registry
    /// writes a source with its last slash, as `src/wasm/score/`: both are
    /// the same folder and both are read.
    pub source: Option<String>,
    /// The build recipe. Part 1.1 :322-324 gives its contents and no JSON
    /// shape, so the reader keeps the bytes and vetting shows them.
    pub recipe: Option<Value>,
}

/// One verified library of `vendor` (spec 2.2 :430, part 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vendor {
    pub name: String,
    pub version: String,
    /// `npm:<name>@<version>` or `git:<host>/<owner>/<repo>@<commit>`. Which
    /// hosts a release fetches from is a setting (part 3 :489-490), not this
    /// file's to hold, so the form is checked here and the host by whoever
    /// fetches.
    pub upstream: String,
    /// A sha256 for each file.
    pub files: Vec<(String, String)>,
}

/// What an origin sends as its body (spec 5.1 :849-852).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    None,
    Bounded(u64),
}

/// One declared destination of `origins` (spec 2.2 :432, 5.1, 10.1). A
/// manifest only ever asks: nothing is granted from these bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub id: String,
    /// An exact origin: `https`, a lowercase A-label host, no port, no path,
    /// no wildcard, no user info, no trailing dot and no address literal.
    pub url: String,
    pub credential: bool,
    pub methods: Vec<String>,
    pub body: Body,
}

/// Part 10.1's block, which an adapter alone carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Adapter {
    /// Such as `tehas:adapter@2.0.0`.
    pub world: String,
    /// The one component, in `dist/wasm/`, in place of a driver.
    pub component: String,
    pub realms: Vec<String>,
    pub ports: Vec<String>,
    /// The kind of credential the adapter needs. Never a credential.
    pub credential: String,
}

/// A skill: text in the repository for the chats it names. 1.x wrote a
/// skill's about and its chats in the manifest; 2.2 holds them in the file's
/// own frontmatter, so a 2.x manifest carries the path alone, in
/// [`Manifest::skills`]. This is what `tehas plugin skills` reads until the
/// host reads the frontmatter too (#3371).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub id: String,
    pub about: String,
    /// The file in the repository, read at the pinned commit.
    pub path: String,
    pub chats: Vec<String>,
}

/// A tool: a run of the plugin's image the chats it names may call. 1.x
/// alone: the 2.2 table has no `tools` field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    pub id: String,
    pub about: String,
    /// The schema of a call's arguments, as [`params::rules`] took it.
    pub params: Value,
    /// A call's seconds.
    pub timeout: u64,
    pub chats: Vec<String>,
}

/// The fields of manifest version 1, which the 2.2 table no longer carries
/// and which this reader therefore never sets: each is refused as an unknown
/// field before anything is read from it.
///
/// They stay because the gate, the runner and `tehas plugin skills` still
/// read them. Taking the image build, `tehas plugin call`, the skills
/// listing and the needs graph out is the contract step that follows this
/// reader (R9), not a part of it, and pulling that forward would put several
/// commands' output schemas into a manifest change. Nothing can reach these
/// fields: a manifest that names one of them is refused for naming it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct V1 {
    pub license: Option<String>,
    pub build: Option<String>,
    pub kit: bool,
    pub view: Option<String>,
    pub tools: Vec<Tool>,
    /// The plugins it needed, and what it wanted from each.
    pub needs: Vec<Need>,
    /// The skills as 1.x wrote them, with their about and their chats.
    pub skills: Vec<Skill>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub label: String,
    /// The spec revision it was built against, such as `2.0.0-r8`. An
    /// erratum is never a revision of its own: `2.0.0-r8` is revision 8 with
    /// its errata, and a change to an interface is a whole revision
    /// (spec 2.2 :416, #3381).
    pub spec: String,
    pub kind: Kind,
    pub runtime: Runtime,
    /// The asks as the manifest words them. A manifest asks, and is granted
    /// nothing by asking.
    pub asks: Vec<String>,
    pub driver: Option<String>,
    /// A run's seconds, with a driver.
    pub timeout: Option<u64>,
    pub actions: Vec<Action>,
    pub views: Vec<View>,
    /// The skill files, each a path under `skills/`.
    pub skills: Vec<String>,
    pub hooks: Vec<Hook>,
    pub wasm: Vec<Module>,
    pub view_wasm: Vec<Module>,
    pub vendor: Vec<Vendor>,
    pub solid: Option<String>,
    pub origins: Vec<Origin>,
    pub adapter: Option<Adapter>,
    /// What 2.2 no longer has, and so what no manifest can fill. See [`V1`].
    pub v1: V1,
}

/// The reasons are collected, so a plugin's author reads them all at once.
struct Refusals(Vec<String>);

impl Refusals {
    fn say(&mut self, why: impl Into<String>) {
        self.0.push(why.into());
    }
}

/// A label: one line of 1 to 24 characters, no control character. The table
/// says only "text", so this bound is the reader's and the schema says it.
fn label(v: &Value, field: &str, out: &mut Refusals) -> Option<String> {
    line(v, field, LABEL_MAX, out)
}

/// One line of 1 to `max` characters, no control character.
fn line(v: &Value, field: &str, max: usize, out: &mut Refusals) -> Option<String> {
    let Some(s) = v.as_str() else {
        out.say(format!("{field}: expected a string"));
        return None;
    };
    let control = s
        .chars()
        .any(|c| c.is_control() || c == '\u{2028}' || c == '\u{2029}');
    if control {
        out.say(format!("{field}: a control character or a line break"));
        return None;
    }
    if !(1..=max).contains(&s.chars().count()) {
        out.say(format!("{field}: expected 1 to {max} characters"));
        return None;
    }
    Some(s.to_string())
}

/// Whole seconds within `bounds`.
fn seconds(v: &Value, field: &str, bounds: (u64, u64), out: &mut Refusals) -> Option<u64> {
    match v.as_u64() {
        Some(n) if (bounds.0..=bounds.1).contains(&n) => Some(n),
        _ => {
            out.say(format!(
                "{field} {v}: out of the table's bounds, {} to {} whole seconds",
                bounds.0, bounds.1
            ));
            None
        }
    }
}

/// The keys of an object that are not `known`, each said.
fn closed(m: &Map<String, Value>, known: &[&str], field: &str, out: &mut Refusals) {
    for key in m.keys().filter(|k| !known.contains(&k.as_str())) {
        out.say(format!("{field}{key}: no field of manifest version 2"));
    }
}

/// The value of a required key, said when it has none.
fn need<'a>(
    m: &'a Map<String, Value>,
    key: &str,
    at: &str,
    out: &mut Refusals,
) -> Option<&'a Value> {
    let v = m.get(key);
    if v.is_none() {
        out.say(format!("{at}.{key} is missing"));
    }
    v
}

/// One part of a path: letters, digits and `._-`, and not `.` or `..`.
fn is_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// Parts joined by `/`, none empty.
fn is_parts(s: &str) -> bool {
    !s.is_empty() && s.split('/').all(is_part)
}

/// A path under `prefix`, of parts [`is_part`] accepts. `folder` is whether
/// the last part may be a folder named with its trailing slash, which is how
/// the registry writes a module's source.
fn path(s: &str, prefix: &str, folder: bool) -> bool {
    if s.len() > PATH_MAX {
        return false;
    }
    let Some(rest) = s.strip_prefix(prefix) else {
        return false;
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if !folder && s.ends_with('/') {
        return false;
    }
    is_parts(rest)
}

/// A path to a file under `prefix`, with why it is not one.
fn file_path(v: &Value, field: &str, prefix: &str, out: &mut Refusals) -> Option<String> {
    let Some(s) = v.as_str() else {
        out.say(format!("{field}: expected a path"));
        return None;
    };
    if path(s, prefix, false) {
        return Some(s.to_string());
    }
    out.say(format!(
        "{field} {v}: expected a file under {prefix}, of letters, digits and ._-, with no . or .. and no trailing /"
    ));
    None
}

/// A path to a file or a folder under `prefix`, with why it is not one.
fn folder_path(v: &Value, field: &str, prefix: &str, out: &mut Refusals) -> Option<String> {
    let Some(s) = v.as_str() else {
        out.say(format!("{field}: expected a path"));
        return None;
    };
    if path(s, prefix, true) {
        return Some(s.to_string());
    }
    out.say(format!(
        "{field} {v}: expected a file or a folder under {prefix}, of letters, digits and ._-, with no . or .."
    ));
    None
}

/// A skill's file: a `.md` under `skills/`. Its id is the folder it is in.
fn skill_path(v: &Value, field: &str, out: &mut Refusals) -> Option<String> {
    let s = file_path(v, field, "skills/", out)?;
    if s.rsplit('/')
        .next()
        .is_some_and(|f| f.len() > 3 && f.ends_with(".md"))
    {
        return Some(s);
    }
    out.say(format!("{field} {v}: expected a .md file under skills/"));
    None
}

/// The id a skill's path names: the folder of `skills/<id>/<file>.md`.
fn skill_id(path: &str) -> Option<&str> {
    path.rsplit('/').nth(1)
}

/// `<major>.<minor>.<patch>-r<revision>`, each of digits. The grammar is the
/// table's (#3381): an erratum, whose number is dotted, is never a `spec`,
/// because an erratum changes no interface and a change to an interface is a
/// whole revision. So `2.0.0-r8.1` is refused, not read as revision 8.
fn is_spec(s: &str) -> bool {
    let Some((numbers, revision)) = s.split_once("-r") else {
        return false;
    };
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    digits(revision) && numbers.split('.').count() == 3 && numbers.split('.').all(digits)
}

/// An exact origin, as 5.1 :838-840 gives it: https, a lowercase A-label
/// host, and nothing else. 10.1 :1353-1355 refuses three more that a pattern
/// of this shape cannot say, so they are refused here as well.
fn is_origin(s: &str) -> bool {
    let Some(host) = s.strip_prefix("https://") else {
        return false;
    };
    if host.is_empty()
        || host.contains('/')
        || host.contains(':')
        || host.contains('@')
        || host.ends_with('.')
    {
        return false;
    }
    // An address literal is not a name: it has no label for a pattern to
    // match, and a person approves a name, not a number.
    if host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return false;
    }
    host.split('.').all(|label| {
        let b = label.as_bytes();
        !b.is_empty()
            && b.len() <= 63
            && b[0].is_ascii_lowercase()
            && b[b.len() - 1].is_ascii_alphanumeric()
            && b.iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
    })
}

/// A sha256, lower case, as part 3 writes one.
fn is_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A whole `major.minor.patch`, with the pre-release and build parts a
/// version may carry. Part 3 pins a version so a fetch is one thing.
fn is_version(s: &str) -> bool {
    if s.is_empty() || s.len() > VERSION_MAX {
        return false;
    }
    let core = s.split(['+', '-']).next().unwrap_or(s);
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() == 3
        && numbers
            .iter()
            .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// A library's upstream, part 3 :487-492's two forms and nothing else:
/// `npm:<name>@<version>` with an exact version, or
/// `git:<host>/<owner>/<repo>@<commit>` with a whole commit. A range, a tag,
/// a branch or a URL is refused, because the host builds the fetch URL from
/// the form and never from a manifest's text.
fn is_upstream(s: &str) -> bool {
    if let Some(rest) = s.strip_prefix("npm:") {
        let Some((name, version)) = rest.rsplit_once('@') else {
            return false;
        };
        let name = name.strip_prefix('@').unwrap_or(name);
        return !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
            && is_version(version);
    }
    let Some(rest) = s.strip_prefix("git:") else {
        return false;
    };
    let Some((repo, commit)) = rest.rsplit_once('@') else {
        return false;
    };
    let parts: Vec<&str> = repo.split('/').collect();
    parts.len() == 3
        && commit.len() == 40
        && commit
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
}

/// `tehas:adapter@<major>.<minor>.<patch>`, the world 10.1 :1339 names.
fn is_world(s: &str) -> bool {
    s.strip_prefix("tehas:adapter@").is_some_and(is_version)
}

/// A realm's, a port's or a module's name: a letter, then letters, digits
/// and dashes. The examples 10.1 :1341-1343 give are of this shape.
fn is_word(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return false;
    };
    s.len() <= WORD_MAX
        && first.is_ascii_lowercase()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `hook:<when>:<event>`, as the table :427 words it. The words of `<when>`
/// and `<event>` are part 5's list of points, which this reader does not
/// hold, so the shape is checked here and a point the host has no policy for
/// is refused where it would run.
fn is_hook_at(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("hook:") else {
        return false;
    };
    let parts: Vec<&str> = rest.splitn(2, ':').collect();
    parts.len() == 2
        && parts[0]
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !parts[0].is_empty()
        && parts[1]
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".-_".contains(&b))
        && !parts[1].is_empty()
}

/// `none`, or `bounded(<bytes>)`: the standard's bounded (§6) with this
/// host's byte count on it (5.1 :849-852).
fn body(s: &str) -> Option<Body> {
    match s {
        "none" => Some(Body::None),
        _ => {
            let rest = s.strip_prefix("bounded(")?.strip_suffix(')')?;
            (!rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
                .then(|| rest.parse().ok())
                .flatten()
                .map(Body::Bounded)
        }
    }
}

/// Read a manifest of the plugin `name`, the name TEHAS_PLUGINS gives it.
pub fn read(bytes: &[u8], name: &str) -> Result<Manifest, Vec<String>> {
    if bytes.len() > BYTES_MAX {
        return Err(vec![format!("the manifest is over {BYTES_MAX} bytes")]);
    }
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(e) => return Err(vec![format!("the manifest is not JSON: {e}")]),
    };
    let Some(m) = value.as_object() else {
        return Err(vec!["the manifest is not a JSON object".to_string()]);
    };
    let mut out = Refusals(Vec::new());
    for key in m.keys() {
        if !FIELDS.contains(&key.as_str()) {
            out.say(format!("{key}: no field of manifest version 2"));
        }
    }
    for field in REQUIRED.iter().filter(|f| !m.contains_key(**f)) {
        out.say(format!("{field}: required and missing"));
    }
    let field = |key: &str| m.get(key);

    // The major is refused, never upgraded: a manifest of another major is
    // read by the tehas that knows it. The reason names the major so a
    // plugin's author can see which one the host reads.
    if let Some(v) = field("plugin") {
        if v.as_u64() != Some(2) {
            out.say(format!(
                "plugin is {v}: this tehas reads manifest version 2"
            ));
        }
    }
    let spec = match field("spec") {
        None => None,
        Some(v) if v.as_str().is_some_and(is_spec) => Some(v.as_str().unwrap().to_string()),
        Some(v) => {
            out.say(format!("spec {v}: not <major>.<minor>.<patch>-r<revision>"));
            None
        }
    };
    let kind = match field("kind") {
        None => Kind::Plugin,
        Some(Value::String(k)) if k == "plugin" => Kind::Plugin,
        Some(Value::String(k)) if k == "adapter" => Kind::Adapter,
        Some(v) => {
            out.say(format!("kind {v}: the table admits plugin or adapter"));
            Kind::Plugin
        }
    };
    let runtime = match field("runtime") {
        None => Runtime::StarlingMonkey,
        Some(Value::String(r)) if r == "starlingmonkey" => Runtime::StarlingMonkey,
        Some(Value::String(r)) if r == "component" => Runtime::Component,
        Some(v) => {
            out.say(format!(
                "runtime {v}: the table admits starlingmonkey or component"
            ));
            Runtime::StarlingMonkey
        }
    };
    let got_name = field("name").and_then(|v| match v.as_str() {
        Some(s) if is_name(s) && s == name => Some(s.to_string()),
        Some(s) if is_name(s) => {
            out.say(format!("name is {s}, and TEHAS_PLUGINS names it {name}"));
            None
        }
        _ => {
            out.say("name: expected 1 to 12 characters of a-z and 0-9, a letter first");
            None
        }
    });
    let got_label = field("label").and_then(|v| label(v, "label", &mut out));

    let mut asks: Vec<String> = Vec::new();
    match field("asks").map(|v| v.as_array()) {
        None => {}
        Some(None) => out.say("asks: the table wants an array of strings"),
        Some(Some(list)) => {
            if list.len() > ASKS_MAX {
                out.say(format!("asks: at most {ASKS_MAX}"));
            }
            for v in list {
                let Some(word) = v.as_str() else {
                    out.say("asks: the table wants an array of strings");
                    continue;
                };
                if asks.iter().any(|a| a == word) {
                    out.say(format!("asks: {word} is asked twice"));
                    continue;
                }
                match super::grant::check(word) {
                    Ok(()) => asks.push(word.to_string()),
                    Err(why) => out.say(format!("asks: {word}: {why}")),
                }
            }
        }
    }

    let driver = field("driver").and_then(|v| file_path(v, "driver", "dist/", &mut out));
    let timeout = field("timeout").and_then(|v| seconds(v, "timeout", TIMEOUT, &mut out));
    // The table gives timeout "with a driver" and driver "with a driver":
    // each names the other, so one without the other is a manifest that
    // means something it cannot say.
    match (&driver, timeout) {
        (Some(_), None) => out.say("timeout is missing: a driver runs, and a run has seconds"),
        (None, Some(_)) => out.say("driver is missing: only a run has seconds to give"),
        _ => {}
    }

    let actions = field("actions")
        .map(|v| actions(v, &asks, &mut out))
        .unwrap_or_default();
    let views = field("views")
        .map(|v| views(v, &asks, &mut out))
        .unwrap_or_default();
    let skills = field("skills")
        .map(|v| skills(v, &asks, &mut out))
        .unwrap_or_default();
    let hooks = field("hooks")
        .map(|v| hooks(v, &asks, &mut out))
        .unwrap_or_default();
    let wasm = field("wasm")
        .map(|v| modules(v, "wasm", "dist/wasm/", &mut out))
        .unwrap_or_default();
    let view_wasm = field("view_wasm")
        .map(|v| modules(v, "view_wasm", "dist/view/wasm/", &mut out))
        .unwrap_or_default();
    let vendor = field("vendor")
        .map(|v| vendor(v, &mut out))
        .unwrap_or_default();
    let solid = field("solid").and_then(|v| line(v, "solid", SOLID_MAX, &mut out));
    let origins = field("origins")
        .map(|v| origins(v, &mut out))
        .unwrap_or_default();
    let adapter = field("adapter").and_then(|v| adapter(v, &mut out));

    if solid.is_some() && views.is_empty() {
        out.say("solid: a solid-js version is for a compiled view, and there is none");
    }
    // Part 10.1: the block is an adapter's, and an adapter has no driver
    // entry, view, skill, hook, gate, tool or schedule (:1299). It keeps its
    // modules, its origins and, alone, the http ask.
    let with_driver = [
        ("driver", !driver.is_none()),
        ("views", !views.is_empty()),
        ("actions", !actions.is_empty()),
        ("skills", !skills.is_empty()),
        ("hooks", !hooks.is_empty()),
        ("solid", solid.is_some()),
        ("view_wasm", !view_wasm.is_empty()),
    ];
    match (kind, adapter.is_some()) {
        (Kind::Adapter, false) => out.say("adapter is missing: kind adapter is part 10's adapter"),
        (Kind::Plugin, true) => out.say("adapter: the block is an adapter's, and this is a plugin"),
        (Kind::Adapter, true) => {
            for (name, there) in with_driver {
                if there {
                    out.say(format!("{name}: an adapter has no {name}"));
                }
            }
        }
        (Kind::Plugin, false) => {
            if runtime == Runtime::Component {
                out.say("runtime component: it is an adapter's runtime, and this is a plugin");
            }
            if asks.iter().any(|a| a == "http") {
                out.say("asks: http: only an adapter reaches the network");
            }
        }
    }

    if !out.0.is_empty() {
        return Err(out.0);
    }
    // Every required field was read, or a refusal was said.
    match (got_name, got_label, spec) {
        (Some(name), Some(label), Some(spec)) => Ok(Manifest {
            name,
            label,
            spec,
            kind,
            runtime,
            asks,
            driver,
            timeout,
            actions,
            views,
            skills,
            hooks,
            wasm,
            view_wasm,
            vendor,
            solid,
            origins,
            adapter,
            v1: V1::default(),
        }),
        _ => Err(vec!["the manifest could not be read".to_string()]),
    }
}

/// `actions`: id to `{label, args}` (spec 2.2 :424). Each id is asked as
/// `action:<id>`, which is what makes a press something the host agreed to.
fn actions(v: &Value, asks: &[String], out: &mut Refusals) -> Vec<Action> {
    let Some(m) = v.as_object() else {
        out.say("actions: expected an object of id to {label, args}");
        return Vec::new();
    };
    if m.len() > ACTIONS_MAX {
        out.say(format!("actions: at most {ACTIONS_MAX}"));
    }
    let mut got: Vec<Action> = Vec::new();
    for (id, v) in m {
        let at = format!("actions[{id}]");
        if !is_action_id(id) {
            out.say(format!("{at}: expected 1 to 16 characters of a-z and -"));
            continue;
        }
        let Some(a) = v.as_object() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(a, &["label", "args"], &format!("{at}."), out);
        let Some(label) = a
            .get("label")
            .and_then(|v| line(v, &format!("{at}.label"), LABEL_MAX, out))
        else {
            if !a.contains_key("label") {
                out.say(format!("{at}.label is missing"));
            }
            continue;
        };
        let args = match a.get("args") {
            None => None,
            Some(v) => {
                for w in params::rules(v) {
                    out.say(format!("{at}.args: {w}"));
                }
                Some(v.clone())
            }
        };
        let ask = format!("action:{id}");
        if !asks.contains(&ask) {
            out.say(format!("{at}: the action {id} needs the ask {ask}"));
        }
        got.push(Action {
            id: id.to_string(),
            label,
            args,
        });
    }
    got
}

/// `views`: each `{id, entry, open}` (spec 2.2 :425). Each id is asked as
/// `view:<id>`.
fn views(v: &Value, asks: &[String], out: &mut Refusals) -> Vec<View> {
    let Some(list) = v.as_array() else {
        out.say("views: expected a list");
        return Vec::new();
    };
    if list.is_empty() || list.len() > VIEWS_MAX {
        out.say(format!("views: expected 1 to {VIEWS_MAX} views"));
    }
    let mut got: Vec<View> = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let at = format!("views[{i}]");
        let Some(o) = v.as_object() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(o, &["id", "entry", "open"], &format!("{at}."), out);
        let id = need(o, "id", &at, out).and_then(|v| match v.as_str() {
            Some(id) if is_action_id(id) => Some(id.to_string()),
            _ => {
                out.say(format!("{at}.id: expected 1 to 16 characters of a-z and -"));
                None
            }
        });
        let entry = need(o, "entry", &at, out)
            .and_then(|v| file_path(v, &format!("{at}.entry"), "dist/view/", out));
        let open = need(o, "open", &at, out).and_then(|v| match v.as_str() {
            Some("dock") => Some(Open::Dock),
            Some("page") => Some(Open::Page),
            Some("full") => Some(Open::Full),
            _ => {
                out.say(format!("{at}.open: expected dock, page or full"));
                None
            }
        });
        let (Some(id), Some(entry), Some(open)) = (id, entry, open) else {
            continue;
        };
        if got.iter().any(|g| g.id == id) {
            out.say(format!("{at}: the view {id} is there twice"));
        }
        if let Some(g) = got.iter().find(|g| g.entry == entry) {
            out.say(format!(
                "{at}: the entry {entry} is the view {}'s too",
                g.id
            ));
        }
        let ask = format!("view:{id}");
        if !asks.contains(&ask) {
            out.say(format!("{at}: the view {id} needs the ask {ask}"));
        }
        got.push(View { id, entry, open });
    }
    got
}

/// `skills`: each a `.md` under `skills/` (spec 2.2 :426). Each is asked as
/// `skill:<id>`, by the folder the file is in.
fn skills(v: &Value, asks: &[String], out: &mut Refusals) -> Vec<String> {
    let Some(list) = v.as_array() else {
        out.say("skills: expected a list of paths under skills/");
        return Vec::new();
    };
    if list.len() > SKILLS_MAX {
        out.say(format!("skills: at most {SKILLS_MAX}"));
    }
    let mut got: Vec<String> = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let at = format!("skills[{i}]");
        let Some(s) = skill_path(v, &at, out) else {
            continue;
        };
        if got.contains(&s) {
            out.say(format!("{at}: {s} is there twice"));
        }
        if let Some(id) = skill_id(&s) {
            let ask = format!("skill:{id}");
            if !asks.contains(&ask) {
                out.say(format!("{at}: the skill {id} needs the ask {ask}"));
            }
        }
        got.push(s);
    }
    got
}

/// `hooks`: each `{at, entry}` (spec 2.2 :427). The table gives no count, and
/// the one point 1.x had is gone in 2.2, so there is no bound to carry over
/// (dropped: 1.x's cap of one held a single event; 2.2 names points freely).
/// Each `at` is also asked by name, as the same `hook:<when>:<event>` word:
/// an entry the host was never asked to run is refused, as with a view's or
/// an action's entry.
fn hooks(v: &Value, asks: &[String], out: &mut Refusals) -> Vec<Hook> {
    let Some(list) = v.as_array() else {
        out.say("hooks: expected a list");
        return Vec::new();
    };
    let mut got: Vec<Hook> = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let at = format!("hooks[{i}]");
        let Some(o) = v.as_object() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(o, &["at", "entry"], &format!("{at}."), out);
        let at_ = need(o, "at", &at, out).and_then(|v| match v.as_str() {
            Some(s) if is_hook_at(s) => Some(s.to_string()),
            _ => {
                out.say(format!("{at}.at: expected hook:<when>:<event>"));
                None
            }
        });
        let entry = need(o, "entry", &at, out)
            .and_then(|v| file_path(v, &format!("{at}.entry"), "dist/", out));
        if let (Some(at_), Some(entry)) = (at_.clone(), entry) {
            if !asks.iter().any(|a| a == &at_) {
                out.say(format!("{at}: the hook {at_} needs the ask {at_}"));
            } else {
                got.push(Hook { at: at_, entry });
            }
        }
    }
    got
}

/// `wasm` and `view_wasm`: name to `{path, source, recipe}` (spec 2.2
/// :428-429, part 1.1). `path` is the module; `source` and `recipe` are the
/// rebuild layer, which 1.1 :322 says the manifest `may` name.
fn modules(v: &Value, field: &str, prefix: &str, out: &mut Refusals) -> Vec<Module> {
    let Some(m) = v.as_object() else {
        out.say(format!(
            "{field}: expected an object of name to {{path, source, recipe}}"
        ));
        return Vec::new();
    };
    let mut got: Vec<Module> = Vec::new();
    for (name, v) in m {
        let at = format!("{field}[{name}]");
        if !is_word(name) {
            out.say(format!(
                "{at}: expected a name of a letter, then a-z, 0-9 and -"
            ));
            continue;
        }
        let Some(o) = v.as_object() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(o, &["path", "source", "recipe"], &format!("{at}."), out);
        let path = need(o, "path", &at, out)
            .and_then(|v| file_path(v, &format!("{at}.path"), prefix, out));
        let source = match o.get("source") {
            None => None,
            Some(v) => folder_path(v, &format!("{at}.source"), "src/", out),
        };
        let recipe = o.get("recipe").cloned();
        if let Some(path) = path {
            got.push(Module {
                name: name.to_string(),
                path,
                source,
                recipe,
            });
        }
    }
    got
}

/// `vendor`: library to `{version, upstream, files}` (spec 2.2 :430, part 3).
fn vendor(v: &Value, out: &mut Refusals) -> Vec<Vendor> {
    let Some(m) = v.as_object() else {
        out.say("vendor: expected an object of library to {version, upstream, files}");
        return Vec::new();
    };
    let mut got: Vec<Vendor> = Vec::new();
    for (name, v) in m {
        let at = format!("vendor[{name}]");
        if !is_word(name) {
            out.say(format!(
                "{at}: expected a name of a letter, then a-z, 0-9 and -"
            ));
            continue;
        }
        let Some(o) = v.as_object() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(o, &["version", "upstream", "files"], &format!("{at}."), out);
        let version = need(o, "version", &at, out)
            .and_then(|v| line(v, &format!("{at}.version"), VERSION_MAX, out));
        let upstream = need(o, "upstream", &at, out).and_then(|v| match v.as_str() {
            Some(s) if is_upstream(s) => Some(s.to_string()),
            _ => {
                out.say(format!(
                    "{at}.upstream: expected npm:<name>@<version> or git:<host>/<owner>/<repo>@<commit>"
                ));
                None
            }
        });
        let files = need(o, "files", &at, out).and_then(|v| {
            let Some(f) = v.as_object() else {
                out.say(format!("{at}.files: expected an object of file to sha256"));
                return None;
            };
            let mut kept: Vec<(String, String)> = Vec::new();
            for (file, sha) in f {
                let Some(hex) = sha.as_str() else {
                    out.say(format!("{at}.files[{file}]: expected a sha256"));
                    continue;
                };
                if !is_sha256(hex) {
                    out.say(format!("{at}.files[{file}]: expected 64 hexadecimal characters, lower case"));
                    continue;
                }
                if !is_parts(file) {
                    out.say(format!("{at}.files[{file}]: expected a path of letters, digits and ._-, with no . or .."));
                    continue;
                }
                kept.push((file.to_string(), hex.to_string()));
            }
            Some(kept)
        });
        if let (Some(version), Some(upstream), Some(files)) = (version, upstream, files) {
            got.push(Vendor {
                name: name.to_string(),
                version,
                upstream,
                files,
            });
        }
    }
    got
}

/// `origins`: id to `{url, credential, methods, body}` (spec 2.2 :432, 5.1).
/// These are a request: the signed vetted entry or the person's approval by
/// name grants a destination, never these bytes.
fn origins(v: &Value, out: &mut Refusals) -> Vec<Origin> {
    let Some(m) = v.as_object() else {
        out.say("origins: expected an object of id to {{url, credential, methods, body}}");
        return Vec::new();
    };
    let mut got: Vec<Origin> = Vec::new();
    for (id, v) in m {
        let at = format!("origins[{id}]");
        if !is_word(id) {
            out.say(format!(
                "{at}: expected a name of a letter, then a-z, 0-9 and -"
            ));
            continue;
        }
        let Some(o) = v.as_object() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(
            o,
            &["url", "credential", "methods", "body"],
            &format!("{at}."),
            out,
        );
        let url = need(o, "url", &at, out).and_then(|v| match v.as_str() {
            Some(s) if is_origin(s) => Some(s.to_string()),
            _ => {
                out.say(format!(
                    "{at}.url: expected an exact origin, https, a lowercase host, no port, path, pattern, address or user info"
                ));
                None
            }
        });
        let credential = need(o, "credential", &at, out).and_then(|v| {
            v.as_bool().or_else(|| {
                out.say(format!("{at}.credential: expected true or false"));
                None
            })
        });
        let methods = need(o, "methods", &at, out).and_then(|v| {
            let Some(list) = v.as_array() else {
                out.say(format!("{at}.methods: expected a list of methods"));
                return None;
            };
            if list.is_empty() {
                out.say(format!("{at}.methods: expected at least one method"));
                return None;
            }
            let kept: Option<Vec<String>> = list
                .iter()
                .map(|m| {
                    m.as_str()
                        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_uppercase()))
                })
                .map(|m| {
                    if m.is_none() {
                        out.say(format!(
                            "{at}.methods: expected a method in capitals, such as GET"
                        ));
                    }
                    m.map(str::to_string)
                })
                .collect();
            kept
        });
        let b = need(o, "body", &at, out).and_then(|v| match v.as_str().and_then(body) {
            Some(b) => Some(b),
            None => {
                out.say(format!("{at}.body: expected none or bounded(<bytes>)"));
                None
            }
        });
        if let (Some(url), Some(credential), Some(methods), Some(body)) =
            (url, credential, methods, b)
        {
            got.push(Origin {
                id: id.to_string(),
                url,
                credential,
                methods,
                body,
            });
        }
    }
    got
}

/// The `adapter` block (spec 2.2 :433, part 10.1).
fn adapter(v: &Value, out: &mut Refusals) -> Option<Adapter> {
    let at = "adapter";
    let o = v.as_object().inspect(|o| {
        closed(
            o,
            &["world", "component", "realms", "ports", "credential"],
            "adapter.",
            out,
        );
    })?;
    let world = need(o, "world", at, out).and_then(|v| match v.as_str() {
        Some(s) if is_world(s) => Some(s.to_string()),
        _ => {
            out.say("adapter.world: expected tehas:adapter@<major>.<minor>.<patch>".to_string());
            None
        }
    });
    let component = need(o, "component", at, out)
        .and_then(|v| file_path(v, "adapter.component", "dist/wasm/", out));
    let realms = words(o.get("realms"), "adapter.realms", out);
    let ports = words(o.get("ports"), "adapter.ports", out);
    if o.get("realms").is_none() {
        out.say("adapter.realms is missing");
    }
    if o.get("ports").is_none() {
        out.say("adapter.ports is missing");
    }
    let credential =
        need(o, "credential", at, out).and_then(|v| line(v, "adapter.credential", WORD_MAX, out));
    // The kind of a credential, never one: 10.1 :1355-1358. A path or a
    // space is a credential-shaped thing, so it is refused by name rather
    // than dropped, which would leave the block "missing".
    let credential = match credential {
        Some(c) if c.contains('/') || c.contains(' ') => {
            out.say("adapter.credential: a kind of credential, no path and no space");
            None
        }
        other => other,
    };
    Some(Adapter {
        world: world?,
        component: component?,
        realms: realms?,
        ports: ports?,
        credential: credential?,
    })
}

/// A list of [`is_word`] names, of which there is at least one.
fn words(v: Option<&Value>, field: &str, out: &mut Refusals) -> Option<Vec<String>> {
    let list = v?.as_array()?;
    if list.is_empty() {
        out.say(format!("{field}: expected at least one name"));
        return None;
    }
    if list.len() > REALMS_MAX {
        out.say(format!("{field}: at most {REALMS_MAX}"));
        return None;
    }
    let mut got: Vec<String> = Vec::new();
    let mut whole = true;
    for v in list {
        match v.as_str() {
            Some(s) if is_word(s) => got.push(s.to_string()),
            _ => {
                out.say(format!(
                    "{field}: expected a name of a letter, then a-z, 0-9 and -"
                ));
                whole = false;
            }
        }
    }
    whole.then_some(got)
}

#[cfg(test)]
mod tests {
    use super::read;
    use super::{Adapter, Body, Kind, Manifest, Module, Open, Runtime, V1};
    use crate::plugin::schema::{api, fits};
    use serde_json::{json, Value};

    fn value(m: &Value) -> Result<Manifest, Vec<String>> {
        read(m.to_string().as_bytes(), "demo")
    }

    /// The manifest is read, or the test panics with every reason.
    fn reads(m: &Value) -> Manifest {
        value(m).unwrap_or_else(|why| panic!("{m}\nrefused: {why:?}"))
    }

    /// The manifest is refused, and one reason says `why`. A refusal is
    /// proven by reaching it, never by the absence of an error.
    fn refuses(m: &Value, why: &str) -> Vec<String> {
        let errs = value(m)
            .err()
            .unwrap_or_else(|| panic!("{m}\nwas accepted"));
        assert!(
            errs.iter().any(|e| e.contains(why)),
            "{m}\nno reason says {why:?} of {errs:?}"
        );
        errs
    }

    fn set(m: &Value, key: &str, v: Value) -> Value {
        let mut m = m.clone();
        m.as_object_mut().unwrap().insert(key.into(), v);
        m
    }

    fn without(m: &Value, key: &str) -> Value {
        let mut m = m.clone();
        m.as_object_mut().unwrap().remove(key);
        m
    }

    fn bare() -> Value {
        json!({"plugin": 2, "spec": "2.0.0-r8", "name": "demo", "label": "Demo", "asks": ["product"]})
    }

    fn sha() -> String {
        "a".repeat(64)
    }

    /// Every field of the table at once, with each entry asked by the name
    /// it carries. This is the shape the registry writes.
    fn full() -> Value {
        json!({
            "plugin": 2, "spec": "2.0.0-r8", "name": "demo", "label": "Demo",
            "asks": ["product", "action:refresh", "view:map", "view:notes",
                     "skill:grill", "hook:after:change.offered"],
            "driver": "dist/driver/main.js", "timeout": 600,
            "actions": {"refresh": {"label": "Refresh", "args": {
                "type": "object", "properties": {}, "additionalProperties": false}}},
            "views": [
                {"id": "map", "entry": "dist/view/map/index.html", "open": "dock"},
                {"id": "notes", "entry": "dist/view/notes.html", "open": "page"}],
            "skills": ["skills/grill/GRILL.md"],
            "hooks": [{"at": "hook:after:change.offered", "entry": "dist/hook/main.js"}],
            "wasm": {"score": {"path": "dist/wasm/score.wasm",
                               "source": "src/wasm/score/", "recipe": "cargo build"}},
            "view_wasm": {"panel": {"path": "dist/view/wasm/panel.wasm"}},
            "vendor": {"left-pad": {"version": "1.3.0", "upstream": "npm:left-pad@1.3.0",
                                    "files": {"index.js": sha()}}},
            "solid": "2.0.3",
            "origins": {"api": {"url": "https://api.example.com", "credential": false,
                                "methods": ["GET"], "body": "bounded(1024)"}}
        })
    }

    fn adapter_block() -> Value {
        json!({"world": "tehas:adapter@2.0.0", "component": "dist/wasm/adapter.wasm",
               "realms": ["code-host", "tickets"], "ports": ["status"], "credential": "github"})
    }

    fn adapter() -> Value {
        json!({
            "plugin": 2, "spec": "2.0.0-r8", "name": "demo", "label": "Demo",
            "kind": "adapter", "runtime": "component",
            "asks": ["http", "files"],
            "origins": {"api": {"url": "https://api.example.com", "credential": true,
                                "methods": ["GET", "POST"], "body": "bounded(2048)"}},
            "adapter": adapter_block()
        })
    }

    /// The sixteen names the standard's §5 gives plain, filled to a count
    /// with token budgets of their own.
    fn asks_of(n: usize) -> Vec<String> {
        let mut a: Vec<String> = [
            "product",
            "history",
            "files",
            "cycles",
            "issues",
            "pages",
            "keep",
            "model:images",
            "feed:advisories",
            "industry:logs",
            "view",
            "menu",
            "record",
            "issue:draft",
            "issue:advisory",
            "viewer:login",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let mut i = 0;
        while a.len() < n {
            i += 1;
            a.push(format!("model:{}", i));
        }
        a.truncate(n);
        a
    }

    #[test]
    fn the_bare_sample_is_read_with_the_tables_defaults() {
        let m = reads(&bare());
        assert_eq!(m.name, "demo");
        assert_eq!(m.label, "Demo");
        assert_eq!(m.spec, "2.0.0-r8");
        assert_eq!(m.kind, Kind::Plugin, "the table's default");
        assert_eq!(m.runtime, Runtime::StarlingMonkey, "the JavaScript path");
        assert_eq!(m.asks, ["product"]);
        assert!(m.driver.is_none() && m.timeout.is_none());
        assert!(m.actions.is_empty() && m.views.is_empty() && m.skills.is_empty());
        assert!(m.hooks.is_empty() && m.wasm.is_empty() && m.view_wasm.is_empty());
        assert!(m.vendor.is_empty() && m.solid.is_none() && m.origins.is_empty());
        assert!(m.adapter.is_none());
        assert_eq!(
            m.v1,
            V1::default(),
            "a 2.x reader never fills the 1.x block"
        );
    }

    #[test]
    fn the_full_sample_is_read_whole() {
        let m = reads(&full());
        assert_eq!(m.timeout, Some(600));
        assert_eq!(m.driver.as_deref(), Some("dist/driver/main.js"));
        assert_eq!(m.actions.len(), 1);
        assert_eq!(m.actions[0].id, "refresh");
        assert_eq!(m.actions[0].label, "Refresh");
        assert_eq!(m.views.len(), 2);
        assert_eq!(
            m.views[0],
            super::View {
                id: "map".into(),
                entry: "dist/view/map/index.html".into(),
                open: Open::Dock
            }
        );
        assert_eq!(m.views[1].open, Open::Page);
        assert_eq!(m.skills, ["skills/grill/GRILL.md"]);
        assert_eq!(m.hooks[0].at, "hook:after:change.offered");
        assert_eq!(m.hooks[0].entry, "dist/hook/main.js");
        assert_eq!(
            m.wasm[0],
            Module {
                name: "score".into(),
                path: "dist/wasm/score.wasm".into(),
                source: Some("src/wasm/score/".into()),
                recipe: Some(json!("cargo build")),
            }
        );
        assert_eq!(m.view_wasm[0].path, "dist/view/wasm/panel.wasm");
        assert_eq!(m.vendor[0].name, "left-pad");
        assert_eq!(m.vendor[0].upstream, "npm:left-pad@1.3.0");
        assert_eq!(m.vendor[0].files, [("index.js".to_string(), sha())]);
        assert_eq!(m.solid.as_deref(), Some("2.0.3"));
        assert_eq!(m.origins[0].url, "https://api.example.com");
        assert_eq!(m.origins[0].methods, ["GET"]);
        assert_eq!(m.origins[0].body, Body::Bounded(1024));
    }

    /// The schema's property names are the reader's `FIELDS`, in the order
    /// of the table, and every bound the reader carries is stated in the
    /// schema too. A single green assertion is not agreement: each bound is
    /// compared against the constant the reader refuses with.
    #[test]
    fn the_schema_names_the_readers_fields_and_bounds() {
        let s = api("plugin-manifest.schema.json");
        assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
        // Object keys arrive sorted, so the set is compared; the table's
        // order is `FIELDS`'s own claim, the schema holding the same names.
        let mut names: Vec<&String> = s["properties"].as_object().unwrap().keys().collect();
        names.sort();
        let mut fields: Vec<&str> = super::FIELDS.to_vec();
        fields.sort();
        assert_eq!(names, fields, "the fields of the table");
        let req: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::as_str)
            .map(Option::unwrap)
            .collect();
        assert_eq!(req, super::REQUIRED.to_vec());
        assert_eq!(s["properties"]["plugin"]["const"], json!(2));
        // #3381: the pattern is exact. A widening here would have to be a
        // revision of the table, never a quiet edit here.
        assert_eq!(
            s["properties"]["spec"]["pattern"],
            json!(r"^[0-9]+\.[0-9]+\.[0-9]+-r[0-9]+$")
        );
        assert_eq!(s["properties"]["label"]["minLength"], json!(1));
        assert_eq!(
            s["properties"]["label"]["maxLength"],
            json!(super::LABEL_MAX)
        );
        assert_eq!(s["properties"]["asks"]["maxItems"], json!(super::ASKS_MAX));
        assert_eq!(s["properties"]["asks"]["uniqueItems"], json!(true));
        assert_eq!(
            s["properties"]["timeout"]["minimum"],
            json!(super::TIMEOUT.0)
        );
        assert_eq!(
            s["properties"]["timeout"]["maximum"],
            json!(super::TIMEOUT.1)
        );
        assert_eq!(
            s["properties"]["actions"]["maxProperties"],
            json!(super::ACTIONS_MAX)
        );
        assert_eq!(s["properties"]["views"]["minItems"], json!(1));
        assert_eq!(
            s["properties"]["views"]["maxItems"],
            json!(super::VIEWS_MAX)
        );
        assert_eq!(
            s["properties"]["skills"]["maxItems"],
            json!(super::SKILLS_MAX)
        );
        assert_eq!(
            s["properties"]["solid"]["maxLength"],
            json!(super::SOLID_MAX)
        );
        assert_eq!(
            s["properties"]["adapter"]["properties"]["realms"]["maxItems"],
            json!(super::REALMS_MAX)
        );
    }

    /// Each sample is judged by both, and both must give the same verdict:
    /// the field rules, the bounds and the pairings the schema can say are
    /// held to one verdict. The reader's two rules of its own, the grant
    /// vocabulary and an entry's ask, are proven apart, because the schema
    /// cannot say them.
    #[test]
    fn the_reader_and_the_schema_agree() {
        let s = api("plugin-manifest.schema.json");
        let views5 = |m: &Value| {
            let names: Vec<String> = (b'a'..=b'e').map(|c| format!("v{}", c as char)).collect();
            let views: Vec<Value> = names
                .iter()
                .map(|n| json!({"id": n, "entry": format!("dist/view/{n}.html"), "open": "dock"}))
                .collect();
            let asks: Vec<String> = names.iter().map(|n| format!("view:{n}")).collect();
            let mut m = set(m, "views", json!(views));
            let mut a = m["asks"].as_array().unwrap().clone();
            a.extend(asks.iter().map(|w| json!(w)));
            m["asks"] = json!(a);
            m
        };
        let skills_n = |m: &Value, n: u8| {
            let list: Vec<String> = (b'a'..b'a' + n)
                .map(|c| format!("skills/s{}/S.md", c as char))
                .collect();
            let asks: Vec<String> = (b'a'..b'a' + n)
                .map(|c| format!("skill:s{}", c as char))
                .collect();
            let mut m = set(m, "skills", json!(list));
            let mut a = m["asks"].as_array().unwrap().clone();
            a.extend(asks.iter().map(|w| json!(w)));
            m["asks"] = json!(a);
            m
        };
        let cases: Vec<(&str, Value, bool)> = vec![
            ("every field", full(), true),
            ("the bare table row", bare(), true),
            ("an adapter", adapter(), true),
            (
                "an adapter without a runtime",
                without(&adapter(), "runtime"),
                true,
            ),
            ("a major of one", set(&bare(), "plugin", json!(1)), false),
            ("a major of three", set(&bare(), "plugin", json!(3)), false),
            ("a major spelled", set(&bare(), "plugin", json!("2")), false),
            (
                "an erratum as a spec",
                set(&bare(), "spec", json!("2.0.0-r8.1")),
                false,
            ),
            (
                "a spec of another major",
                set(&bare(), "spec", json!("1.9.9-r0")),
                true,
            ),
            (
                "a field it does not know",
                set(&bare(), "license", json!("MIT")),
                false,
            ),
            ("no asks", without(&bare(), "asks"), false),
            (
                "a label of no characters",
                set(&bare(), "label", json!("")),
                false,
            ),
            (
                "a label over 24",
                set(&bare(), "label", json!("L".repeat(25))),
                false,
            ),
            (
                "a driver with no seconds",
                set(&bare(), "driver", json!("dist/driver/main.js")),
                false,
            ),
            (
                "seconds with no driver",
                set(&bare(), "timeout", json!(60)),
                false,
            ),
            ("seconds of no", set(&full(), "timeout", json!(0)), false),
            (
                "seconds over the most",
                set(&full(), "timeout", json!(601)),
                false,
            ),
            (
                "a kind of its own",
                set(&bare(), "kind", json!("worker")),
                false,
            ),
            (
                "a runtime of its own",
                set(&bare(), "runtime", json!("wasmtime")),
                false,
            ),
            (
                "component on a plugin",
                set(&bare(), "runtime", json!("component")),
                false,
            ),
            (
                "a plugin reaching the network",
                set(&bare(), "asks", json!(["http"])),
                false,
            ),
            (
                "a block on a plugin",
                set(&bare(), "adapter", adapter_block()),
                false,
            ),
            (
                "an adapter with no block",
                without(&adapter(), "adapter"),
                false,
            ),
            (
                "an adapter with a driver",
                set(&adapter(), "driver", json!("dist/driver/main.js")),
                false,
            ),
            (
                "three actions",
                set(
                    &full(),
                    "actions",
                    json!({
                "refresh": {"label": "Refresh"}, "ping": {"label": "Ping"}, "send": {"label": "Send"}}),
                ),
                false,
            ),
            ("no views named", set(&full(), "views", json!([])), false),
            ("five views", views5(&full()), false),
            ("nine skills", skills_n(&full(), 9), false),
            ("eight skills", skills_n(&bare(), 8), true),
            (
                "thirty-three asks",
                set(&bare(), "asks", json!(asks_of(33))),
                false,
            ),
            (
                "an ask named twice",
                set(&bare(), "asks", json!(["product", "product"])),
                false,
            ),
            (
                "a module's path with a trailing slash",
                set(
                    &full(),
                    "wasm",
                    json!({
                "score": {"path": "dist/wasm/score.wasm/"}}),
                ),
                false,
            ),
            (
                "a module's source with its slash",
                set(
                    &full(),
                    "wasm",
                    json!({
                "score": {"path": "dist/wasm/score.wasm", "source": "src/wasm/score/"}}),
                ),
                true,
            ),
            (
                "a module's source without one",
                set(
                    &full(),
                    "wasm",
                    json!({
                "score": {"path": "dist/wasm/score.wasm", "source": "src/wasm/score"}}),
                ),
                true,
            ),
            (
                "a driver with a trailing slash",
                set(&full(), "driver", json!("dist/driver/")),
                false,
            ),
            (
                "an entry outside dist/view",
                set(
                    &full(),
                    "views",
                    json!([
                {"id": "map", "entry": "dist/other/index.html", "open": "dock"}]),
                ),
                false,
            ),
            (
                "a view id with a capital",
                set(
                    &full(),
                    "views",
                    json!([
                {"id": "Map", "entry": "dist/view/index.html", "open": "dock"}]),
                ),
                false,
            ),
            (
                "a library of a range",
                set(
                    &full(),
                    "vendor",
                    json!({
                "left-pad": {"version": "1.3.0", "upstream": "npm:left-pad@^1.3.0",
                             "files": {"index.js": sha()}}}),
                ),
                false,
            ),
            (
                "a library with no sha256",
                set(
                    &full(),
                    "vendor",
                    json!({
                "left-pad": {"version": "1.3.0", "upstream": "npm:left-pad@1.3.0",
                             "files": {"index.js": "abc"}}}),
                ),
                false,
            ),
            (
                "an origin with a pattern",
                set(
                    &full(),
                    "origins",
                    json!({
                "api": {"url": "https://*.example.com", "credential": false,
                        "methods": ["GET"], "body": "none"}}),
                ),
                false,
            ),
            (
                "an origin over http",
                set(
                    &full(),
                    "origins",
                    json!({
                "api": {"url": "http://api.example.com", "credential": false,
                        "methods": ["GET"], "body": "none"}}),
                ),
                false,
            ),
            (
                "an origin with a path",
                set(
                    &full(),
                    "origins",
                    json!({
                "api": {"url": "https://api.example.com/x", "credential": false,
                        "methods": ["GET"], "body": "none"}}),
                ),
                false,
            ),
            (
                "a method in lower case",
                set(
                    &full(),
                    "origins",
                    json!({
                "api": {"url": "https://api.example.com", "credential": false,
                        "methods": ["get"], "body": "none"}}),
                ),
                false,
            ),
            (
                "an origin with no method",
                set(
                    &full(),
                    "origins",
                    json!({
                "api": {"url": "https://api.example.com", "credential": false,
                        "methods": [], "body": "none"}}),
                ),
                false,
            ),
            (
                "a body of unbounded",
                set(
                    &full(),
                    "origins",
                    json!({
                "api": {"url": "https://api.example.com", "credential": false,
                        "methods": ["GET"], "body": "unbounded"}}),
                ),
                false,
            ),
            (
                "a solid with no view",
                set(&bare(), "solid", json!("2.0.3")),
                false,
            ),
            (
                "seventeen realms",
                set(
                    &adapter(),
                    "adapter",
                    json!({
                "world": "tehas:adapter@2.0.0", "component": "dist/wasm/adapter.wasm",
                "realms": (1..=17).map(|i| format!("r{i}")).collect::<Vec<_>>(),
                "ports": ["status"], "credential": "github"}),
                ),
                false,
            ),
            (
                "sixteen realms",
                set(
                    &adapter(),
                    "adapter",
                    json!({
                "world": "tehas:adapter@2.0.0", "component": "dist/wasm/adapter.wasm",
                "realms": (1..=16).map(|i| format!("r{i}")).collect::<Vec<_>>(),
                "ports": ["status"], "credential": "github"}),
                ),
                true,
            ),
        ];
        for (what, m, accepts) in cases {
            assert_eq!(fits(&m, &s), accepts, "{what}: the schema");
            assert_eq!(value(&m).is_ok(), accepts, "{what}: the reader");
        }
    }

    /// A field outside the table is refused naming it. The 1.x names are the
    /// ones an author will reach for, so each is said; the decision was that
    /// the block is not silently upgraded, and nothing can reach it.
    #[test]
    fn a_field_it_does_not_know_is_refused() {
        for key in [
            "license", "build", "kit", "view", "tools", "needs", "skilz", "PLUGIN",
        ] {
            let m = set(&bare(), key, json!("x"));
            refuses(&m, &format!("{key}: no field of manifest version 2"));
        }
        reads(&bare());
    }

    /// Each missing required field is named, all five at once.
    #[test]
    fn every_required_field_is_named_when_missing() {
        for key in super::REQUIRED {
            let errs = refuses(&without(&bare(), key), "required and missing");
            assert!(
                errs.iter().any(|e| e.starts_with(&format!("{key}:"))),
                "{key}: {errs:?}"
            );
        }
    }

    /// The major is refused, never upgraded, and the reason names it so the
    /// author sees which manifest the host reads. `plugin: 1` is the case
    /// that matters: an old manifest stays an old manifest.
    #[test]
    fn the_major_is_refused_naming_the_major() {
        let errs = refuses(&set(&bare(), "plugin", json!(1)), "plugin is 1");
        assert!(
            errs.iter()
                .any(|e| e.contains("this tehas reads manifest version 2")),
            "{errs:?}"
        );
        refuses(&set(&bare(), "plugin", json!(3)), "plugin is 3");
        refuses(&set(&bare(), "plugin", json!("2")), "plugin is \"2\"");
        reads(&set(&bare(), "plugin", json!(2)));
    }

    /// The version is `<major>.<minor>.<patch>-r<revision>` exactly. An
    /// erratum's dotted number is never a spec value (#3381), so
    /// `2.0.0-r8.1` is refused and `2.0.0-r8` is read, errata included.
    #[test]
    fn the_version_names_its_revision_and_no_erratum() {
        refuses(
            &set(&bare(), "spec", json!("2.0.0-r8.1")),
            "spec \"2.0.0-r8.1\": not <major>.<minor>.<patch>-r<revision>",
        );
        refuses(&set(&bare(), "spec", json!("2.0.0")), "-r<revision>");
        refuses(&set(&bare(), "spec", json!("2.0-r8")), "-r<revision>");
        reads(&set(&bare(), "spec", json!("2.0.0-r8")));
        reads(&set(&bare(), "spec", json!("2.0.0-r0")));
    }

    /// `kind` admits the table's two words; `runtime component` belongs to
    /// an adapter alone, and the adapter block belongs to an adapter alone.
    #[test]
    fn kind_and_runtime_admit_only_the_tables_words() {
        refuses(&set(&bare(), "kind", json!("worker")), "kind \"worker\"");
        refuses(
            &set(&bare(), "runtime", json!("wasmtime")),
            "runtime \"wasmtime\"",
        );
        reads(&set(&bare(), "kind", json!("plugin")));
        reads(&set(&bare(), "runtime", json!("starlingmonkey")));
        let a = reads(&adapter());
        assert_eq!(a.kind, Kind::Adapter);
        assert_eq!(a.runtime, Runtime::Component);
        assert!(a.adapter.is_some());
        // An adapter's block, and none of the driver's parts.
        refuses(
            &set(
                &adapter(),
                "views",
                json!([{"id": "map", "entry": "dist/view/index.html", "open": "dock"}]),
            ),
            "views: an adapter has no views",
        );
        refuses(&without(&adapter(), "adapter"), "adapter is missing");
        refuses(
            &set(&bare(), "adapter", adapter_block()),
            "the block is an adapter's",
        );
    }

    /// A driver and its seconds name each other: one without the other means
    /// something the manifest cannot say.
    #[test]
    fn a_driver_and_its_seconds_come_together() {
        refuses(
            &set(&bare(), "driver", json!("dist/driver/main.js")),
            "timeout is missing",
        );
        refuses(&set(&bare(), "timeout", json!(60)), "driver is missing");
        let mut both = set(&bare(), "driver", json!("dist/driver/main.js"));
        both["timeout"] = json!(60);
        let m = reads(&both);
        assert_eq!(
            (m.driver.as_deref(), m.timeout),
            (Some("dist/driver/main.js"), Some(60))
        );
    }

    /// The seconds of a run, within the table's bounds both ways.
    #[test]
    fn the_seconds_are_within_the_tables_bounds() {
        refuses(
            &set(&full(), "timeout", json!(0)),
            "out of the table's bounds",
        );
        refuses(
            &set(&full(), "timeout", json!(601)),
            "out of the table's bounds",
        );
        refuses(
            &set(&full(), "timeout", json!("600")),
            "out of the table's bounds",
        );
        assert_eq!(
            reads(&set(&full(), "timeout", json!(600))).timeout,
            Some(600)
        );
        assert_eq!(reads(&set(&full(), "timeout", json!(1))).timeout, Some(1));
    }

    /// A label is one line of 1 to 24 characters. The near miss at the
    /// bound is the point: 24 read, 25 refused.
    #[test]
    fn the_label_is_one_line_of_its_bound() {
        refuses(
            &set(&bare(), "label", json!("L".repeat(25))),
            "expected 1 to 24 characters",
        );
        refuses(
            &set(&bare(), "label", json!("a\nb")),
            "a control character or a line break",
        );
        assert_eq!(
            reads(&set(&bare(), "label", json!("L".repeat(24)))).label,
            "L".repeat(24)
        );
    }

    /// Each action's id is asked as `action:<id>`; the pairing is what makes
    /// a press a thing the host agreed to. The counter-assertion adds the
    /// ask to the same manifest and reads it.
    #[test]
    fn an_action_is_asked_by_name_and_keeps_the_cap() {
        let asks = |m: &Value| m["asks"].as_array().unwrap().clone();
        let one = json!({"refresh": {"label": "Refresh"}});
        refuses(
            &set(
                &set(&full(), "actions", one.clone()),
                "asks",
                json!(["product"]),
            ),
            "the action refresh needs the ask action:refresh",
        );
        reads(&set(&full(), "actions", one));
        let three = json!({
            "refresh": {"label": "Refresh"},
            "ping": {"label": "Ping"},
            "send": {"label": "Send"}
        });
        let mut k = asks(&full());
        k.push(json!("action:ping"));
        k.push(json!("action:send"));
        refuses(
            &set(&set(&full(), "actions", three), "asks", json!(k)),
            "actions: at most 2",
        );
        let two = json!({"refresh": {"label": "Refresh"}, "ping": {"label": "Ping"}});
        let mut k = asks(&full());
        k.push(json!("action:ping"));
        reads(&set(&set(&full(), "actions", two), "asks", json!(k)));
        refuses(
            &set(&full(), "actions", json!({"Refresh": {"label": "Refresh"}})),
            "expected 1 to 16 characters of a-z and -",
        );
        let mut k = asks(&full());
        k.push(json!("action:re-fresh"));
        refuses(
            &set(
                &set(
                    &full(),
                    "actions",
                    json!({"re-fresh": {"label": "L".repeat(25)}}),
                ),
                "asks",
                json!(k.clone()),
            ),
            "label: expected 1 to 24 characters",
        );
        reads(&set(
            &set(&full(), "actions", json!({"re-fresh": {"label": "Again"}})),
            "asks",
            json!(k),
        ));
    }

    /// Each view's id is asked as `view:<id>`; the counter-assertion is the
    /// same manifest with the ask. The bound is held from both sides: four
    /// read, five refused, none named.
    #[test]
    fn a_view_is_asked_by_name_and_keeps_the_cap() {
        let one = json!([{"id": "map", "entry": "dist/view/map.html", "open": "dock"}]);
        refuses(
            &set(
                &set(&full(), "views", one.clone()),
                "asks",
                json!(["product"]),
            ),
            "the view map needs the ask view:map",
        );
        reads(&set(&full(), "views", one.clone()));
        refuses(
            &set(&full(), "views", json!([])),
            "views: expected 1 to 4 views",
        );
        let five: Vec<Value> = (b'a'..=b'e')
            .map(|c| {
                let n = format!("v{}", c as char);
                json!({"id": n, "entry": format!("dist/view/{n}.html"), "open": "dock"})
            })
            .collect();
        let mut k: Vec<String> = (b'a'..=b'e')
            .map(|c| format!("view:v{}", c as char))
            .collect();
        let mut asks = full()["asks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        asks.append(&mut k);
        refuses(
            &set(&set(&full(), "views", json!(five)), "asks", json!(asks)),
            "views: expected 1 to 4",
        );
        let four: Vec<Value> = (b'a'..=b'd')
            .map(|c| {
                let n = format!("v{}", c as char);
                json!({"id": n, "entry": format!("dist/view/{n}.html"), "open": "full"})
            })
            .collect();
        let mut asks: Vec<String> = (b'a'..=b'd')
            .map(|c| format!("view:v{}", c as char))
            .collect();
        let mut base: Vec<String> = full()["asks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .filter(|w| !w.starts_with("view:"))
            .collect();
        base.append(&mut asks);
        let m = reads(&set(
            &set(&full(), "views", json!(four)),
            "asks",
            json!(base),
        ));
        assert_eq!(m.views[3].open, Open::Full);
        // The same id twice, and the same entry twice, are said.
        refuses(
            &set(
                &full(),
                "views",
                json!([
                {"id": "map", "entry": "dist/view/a.html", "open": "dock"},
                {"id": "map", "entry": "dist/view/b.html", "open": "dock"}]),
            ),
            "the view map is there twice",
        );
        refuses(
            &set(
                &full(),
                "views",
                json!([
                {"id": "map", "entry": "dist/view/a.html", "open": "dock"},
                {"id": "notes", "entry": "dist/view/a.html", "open": "dock"}]),
            ),
            "dist/view/a.html is the view map's too",
        );
        // A trailing slash is a folder, and a view's entry is a file.
        refuses(
            &set(
                &full(),
                "views",
                json!([
                {"id": "map", "entry": "dist/view/", "open": "dock"}]),
            ),
            "no trailing /",
        );
        let _ = one;
    }

    /// A skill is a `.md` under `skills/`, in a folder of its own, asked as
    /// `skill:<id>` by the folder's name. Eight read, nine are refused.
    #[test]
    fn a_skill_is_a_dot_md_in_its_folder_and_needs_its_ask() {
        refuses(
            &set(
                &set(&full(), "skills", json!(["skills/grill/GRILL.md"])),
                "asks",
                json!(["product"]),
            ),
            "the skill grill needs the ask skill:grill",
        );
        reads(&set(&full(), "skills", json!(["skills/grill/GRILL.md"])));
        refuses(
            &set(&full(), "skills", json!(["skills/grill/GRILL.txt"])),
            "expected a .md file under skills/",
        );
        // The id is the folder the file is in: a file straight under
        // skills/ would be the skill "skills", and must be asked as that.
        refuses(
            &set(&full(), "skills", json!(["skills/GRILL.md"])),
            "the skill skills needs the ask skill:skills",
        );
        let nine: Vec<String> = (b'a'..b'a' + 9)
            .map(|c| format!("skills/s{}/S.md", c as char))
            .collect();
        let nine_asks: Vec<String> = (b'a'..b'a' + 9)
            .map(|c| format!("skill:s{}", c as char))
            .collect();
        refuses(
            &set(
                &set(&full(), "skills", json!(nine)),
                "asks",
                json!(nine_asks),
            ),
            "skills: at most 8",
        );
        let eight: Vec<String> = (b'a'..b'a' + 8)
            .map(|c| format!("skills/s{}/S.md", c as char))
            .collect();
        let mut asks: Vec<String> = full()["asks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        asks.extend((b'a'..b'a' + 8).map(|c| format!("skill:s{}", c as char)));
        reads(&set(
            &set(&full(), "skills", json!(eight)),
            "asks",
            json!(asks),
        ));
    }

    /// A hook's point is asked by the same word it carries, and 1.x's cap of
    /// one is gone (dropped: it held a single event, and 2.2 names points
    /// freely), so two asked points are read.
    #[test]
    fn a_hook_is_asked_by_name_and_no_longer_capped() {
        let one = json!([{"at": "hook:after:change.offered", "entry": "dist/hook/main.js"}]);
        refuses(
            &set(
                &set(&full(), "hooks", one.clone()),
                "asks",
                json!(["product"]),
            ),
            "the hook hook:after:change.offered needs the ask hook:after:change.offered",
        );
        reads(&set(&full(), "hooks", one.clone()));
        // The table writes the point as hook:<when>:<event>; the bare form is
        // refused, the prefixed one read — the counter-assertion in the pair.
        refuses(
            &set(
                &full(),
                "hooks",
                json!([{"at": "after:change.offered", "entry": "dist/hook/main.js"}]),
            ),
            "expected hook:<when>:<event>",
        );
        let two = json!([
            {"at": "hook:after:change.offered", "entry": "dist/hook/after.js"},
            {"at": "hook:before:cycle.started", "entry": "dist/hook/before.js"}]);
        let mut asks: Vec<String> = full()["asks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        asks.push("hook:before:cycle.started".to_string());
        reads(&set(&set(&full(), "hooks", two), "asks", json!(asks)));
        // An entry it does not ask for is refused naming the ask.
        refuses(
            &set(
                &full(),
                "hooks",
                json!([{"at": "hook:before:cycle.started", "entry": "dist/hook/before.js"}]),
            ),
            "needs the ask hook:before:cycle.started",
        );
        let _ = one;
    }

    /// `source` names a folder, so a trailing slash is one spelling of it;
    /// every other path is a file and refuses one. The class is the point:
    /// the boundary is proven at each of the four file-shaped fields.
    #[test]
    fn a_source_is_a_folder_and_a_path_is_a_file() {
        reads(&set(
            &full(),
            "wasm",
            json!({"score": {"path": "dist/wasm/score.wasm", "source": "src/wasm/score"}}),
        ));
        reads(&set(
            &full(),
            "wasm",
            json!({"score": {"path": "dist/wasm/score.wasm", "source": "src/wasm/score/"}}),
        ));
        refuses(
            &set(
                &full(),
                "wasm",
                json!({"score": {"path": "dist/wasm/score.wasm/"}}),
            ),
            "no trailing /",
        );
        refuses(
            &set(
                &full(),
                "view_wasm",
                json!({"panel": {"path": "dist/view/wasm/panel.wasm/"}}),
            ),
            "no trailing /",
        );
        refuses(
            &set(&full(), "driver", json!("dist/driver/")),
            "no trailing /",
        );
        refuses(
            &set(
                &full(),
                "views",
                json!([{"id": "map", "entry": "dist/view/map/", "open": "dock"}]),
            ),
            "no trailing /",
        );
        refuses(
            &set(
                &adapter(),
                "adapter",
                json!({
                "world": "tehas:adapter@2.0.0", "component": "dist/wasm/adapter.wasm/",
                "realms": ["r"], "ports": ["p"], "credential": "github"}),
            ),
            "no trailing /",
        );
        // And each of those reads without the slash.
        reads(&set(&full(), "driver", json!("dist/driver/main.js")));
        let a = reads(&set(
            &adapter(),
            "adapter",
            json!({
            "world": "tehas:adapter@2.0.0", "component": "dist/wasm/adapter.wasm",
            "realms": ["r"], "ports": ["p"], "credential": "github"}),
        ));
        assert_eq!(
            a.adapter,
            Some(Adapter {
                world: "tehas:adapter@2.0.0".into(),
                component: "dist/wasm/adapter.wasm".into(),
                realms: ["r".into()].to_vec(),
                ports: ["p".into()].to_vec(),
                credential: "github".into(),
            })
        );
    }

    /// The `http` ask belongs to an adapter alone; a plugin that reaches for
    /// it is refused naming the reason.
    #[test]
    fn the_network_is_an_adapters_alone() {
        refuses(
            &set(&bare(), "asks", json!(["http"])),
            "asks: http: only an adapter reaches the network",
        );
        let a = reads(&adapter());
        assert!(a.asks.contains(&"http".to_string()));
        assert!(a.origins[0].credential);
    }

    /// An origin is an exact name. The three a pattern of that shape cannot
    /// say, 10.1 refuses and the reader refuses too; the lookalike a person
    /// meant is read.
    #[test]
    fn an_origin_is_an_exact_name() {
        let origin = |url: &str| {
            set(
                &full(),
                "origins",
                json!({"api": {"url": url, "credential": false, "methods": ["GET"], "body": "none"}}),
            )
        };
        refuses(&origin("https://1.2.3.4"), "expected an exact origin");
        refuses(
            &origin("https://api.example.com./"),
            "expected an exact origin",
        );
        refuses(
            &origin("https://user@api.example.com"),
            "expected an exact origin",
        );
        refuses(
            &origin("https://API.example.com"),
            "expected an exact origin",
        );
        let m = reads(&origin("https://api.example.com"));
        assert_eq!(m.origins[0].body, Body::None);
        let m = reads(&origin("https://api.example.gov.uk"));
        assert_eq!(m.origins[0].url, "https://api.example.gov.uk");
    }

    /// A vendored library is named whole: one of the two forms, a version
    /// the reader can hold, and a sha256 for each file.
    #[test]
    fn a_vendor_library_is_named_whole() {
        let lib = |upstream: &str| {
            set(
                &full(),
                "vendor",
                json!({"left-pad": {"version": "1.3.0", "upstream": upstream, "files": {"index.js": sha()}}}),
            )
        };
        reads(&lib("npm:left-pad@1.3.0"));
        reads(&lib("npm:@scope/pad@1.3.0"));
        reads(&lib(&format!(
            "git:github.com/owner/repo@{}",
            "ab".repeat(20)
        )));
        refuses(
            &lib("npm:left-pad@1"),
            "expected npm:<name>@<version> or git:<host>/<owner>/<repo>@<commit>",
        );
        refuses(
            &lib("https://github.com/owner/repo"),
            "expected npm:<name>@<version>",
        );
        let m = reads(&lib(
            "git:github.com/owner/repo@0000000000000000000000000000000000000000",
        ));
        assert_eq!(m.vendor.len(), 1);
    }

    /// An ask must be a grant 2.0 knows, and each name is given once. The
    /// counter-assertion is the near miss: `feed:notes` is a grant where the
    /// bare front `feed:` is not, and `history` is read where `weather` is
    /// refused.
    #[test]
    fn the_asks_are_grants_each_given_once() {
        refuses(
            &set(&bare(), "asks", json!(["weather"])),
            "asks: weather: is not a grant",
        );
        refuses(
            &set(&bare(), "asks", json!(["feed:"])),
            "feed: wants a value",
        );
        refuses(
            &set(&bare(), "asks", json!(["product", "product"])),
            "product is asked twice",
        );
        reads(&set(&bare(), "asks", json!(["feed:notes", "history"])));
        reads(&set(&bare(), "asks", json!(asks_of(32))));
        refuses(
            &set(&bare(), "asks", json!(asks_of(33))),
            "asks: at most 32",
        );
        refuses(
            &set(&bare(), "asks", json!(["product", 2])),
            "an array of strings",
        );
    }

    /// The bytes before the rules: too many, not JSON, or not an object.
    #[test]
    fn the_manifest_is_bytes_of_one_object() {
        let why = read(&vec![b'x'; 65537], "demo").err().unwrap();
        assert!(why[0].contains("over 65536"), "{why:?}");
        assert_eq!(
            read(b"[]", "demo").err().unwrap(),
            vec!["the manifest is not a JSON object"]
        );
        assert!(read(b"nope", "demo").err().unwrap()[0].starts_with("the manifest is not JSON"));
    }

    /// The manifest is the settings' plugin: its name must be the one
    /// TEHAS_PLUGINS gives, so two pins cannot mean two plugins.
    #[test]
    fn the_manifest_names_the_plugin_the_settings_give() {
        let errs = read(
            set(&bare(), "name", json!("other")).to_string().as_bytes(),
            "demo",
        )
        .err()
        .unwrap();
        assert!(
            errs.iter()
                .any(|e| e == "name is other, and TEHAS_PLUGINS names it demo"),
            "{errs:?}"
        );
        refuses(
            &set(&bare(), "name", json!("Demo!")),
            "expected 1 to 12 characters",
        );
        reads(&bare());
    }
}
