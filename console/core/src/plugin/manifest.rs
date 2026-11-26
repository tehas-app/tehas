//! The reader of `tehas-plugin.json`, a plugin's manifest: bytes in, a typed
//! manifest or every reason it is refused out. A field it does not know is
//! refused, so a manifest made for a later tehas never runs half read.
//! The wire has one owner: [`Wire`] and the structs beside it name the
//! fields and shapes serde reads, one per closed object, as
//! api/plugin-manifest.schema.json holds them for a plugin's author; the
//! rules below say what each holds, and the tests hold the two to each
//! other.
//!
//! Each field is a [`Field`]: it holds the typed value when the JSON that
//! came can be read as one, and always the JSON text that came, which is
//! what a refusal echoes. A field of the wrong shape is therefore not a
//! parse failure: it reaches the rules as the value that came, and the
//! rules say it in tehas's own words, all of them at once.

use super::needs::Need;
use super::{ask, is_action_id, is_name, params, Cap};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use tehas_ports::events::{Name, When};

/// The largest manifest read.
pub const BYTES_MAX: usize = 65536;
pub const LABEL_MAX: usize = 24;
pub const LICENSE_MAX: usize = 64;
pub const BUILD_MAX: usize = 255;
/// The most asks: the plain ones, an action each and a skill each, with room
/// for the capabilities that follow.
pub const ASKS_MAX: usize = 32;
pub const ACTIONS_MAX: usize = 2;
pub const SKILLS_MAX: usize = 8;
pub const TOOLS_MAX: usize = 8;
/// The most plugins one plugin needs.
pub const NEEDS_MAX: usize = 2;
/// The longest line a chat shows for a skill.
pub const ABOUT_MAX: usize = 120;
/// The chat hosts a skill may go to.
pub const CHATS: [&str; 4] = ["obeya", "wizard", "assistant", "plugin"];
pub const TIMEOUT: (u64, u64) = (60, 14400);
pub const HOOK_TIMEOUT: (u64, u64) = (60, 1800);
pub const TOOL_TIMEOUT: (u64, u64) = (1, 10);

/// The fields of version 1, in the order a refusal names them.
pub const FIELDS: [&str; 16] = [
    "plugin", "kind", "name", "label", "license", "build", "asks", "timeout", "ui", "view",
    "views", "actions", "hooks", "skills", "tools", "needs",
];
/// The most views of `views`.
pub const VIEWS_MAX: usize = 4;
pub const REQUIRED: [&str; 4] = ["plugin", "name", "label", "asks"];
/// Required of a plugin with an image. One with none is text alone.
pub const BUILT: [&str; 3] = ["kind", "build", "timeout"];

/// The value of one field on the wire: the JSON text that came, and that
/// text read as `T` when it can be. A field of the wrong shape is not a
/// parse failure: it reaches the rules as the text that came, which is
/// what a refusal echoes, and `got` tells the rules what the text reads
/// as. A `u64` is a whole number in the JSON, never a float that happens
/// to be whole: a field that came as `1.0` has no `got`, as the reader
/// has always refused it.
#[derive(Clone, Debug)]
struct Field<T> {
    /// The JSON that came, compact: the echo of a refusal.
    text: String,
    /// Whether it came as a list, which a rule says as its own refusal.
    list: bool,
    /// The text read as `T`, when it reads as one.
    got: Option<T>,
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let came = serde_json::Value::deserialize(deserializer)?;
        Ok(Field {
            list: came.is_array(),
            text: came.to_string(),
            got: T::deserialize(came).ok(),
        })
    }
}

/// A key the reader does not know, kept for its name alone: the rules
/// refuse it and echo nothing of its value.
#[derive(Clone, Debug)]
struct Unknown;

impl<'de> Deserialize<'de> for Unknown {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde_json::Value::deserialize(deserializer)?;
        Ok(Unknown)
    }
}

/// The keys of one closed object that it may not hold, in the order a
/// refusal names them.
type Keys = BTreeMap<String, Unknown>;

/// The manifest as the wire holds it: the field names, one owner. Every
/// known field is optional here because the rules say a missing one with
/// `is missing`, in the order [`FIELDS`] names them, and a key outside
/// them lands in `rest`, refused the same way. A field is kept even when
/// it is `null`: a null field was given, and the rules say it as a field
/// of the wrong shape, not as a missing one.
#[derive(Debug, Deserialize)]
struct Wire {
    plugin: Option<Field<u64>>,
    kind: Option<Field<String>>,
    name: Option<Field<String>>,
    label: Option<Field<String>>,
    license: Option<Field<String>>,
    build: Option<Field<String>>,
    asks: Option<Field<Vec<Field<String>>>>,
    timeout: Option<Field<u64>>,
    ui: Option<Field<UiWire>>,
    view: Option<Field<ViewWire>>,
    views: Option<Field<Vec<Field<ViewEntry>>>>,
    actions: Option<Field<Vec<Field<ActionWire>>>>,
    hooks: Option<Field<Vec<Field<HookWire>>>>,
    skills: Option<Field<Vec<Field<SkillWire>>>>,
    tools: Option<Field<Vec<Field<ToolWire>>>>,
    needs: Option<Field<Vec<Field<NeedWire>>>>,
    #[serde(flatten)]
    rest: Keys,
}

/// `ui`: kit 1 and nothing else.
#[derive(Debug, Deserialize)]
struct UiWire {
    kit: Option<Field<u64>>,
    #[serde(flatten)]
    rest: Keys,
}

/// `view`: one panel in the dock.
#[derive(Debug, Deserialize)]
struct ViewWire {
    label: Option<Field<String>>,
    #[serde(flatten)]
    rest: Keys,
}

/// One entry of `views`.
#[derive(Debug, Deserialize)]
struct ViewEntry {
    id: Option<Field<String>>,
    label: Option<Field<String>>,
    path: Option<Field<String>>,
    open: Option<Field<String>>,
    #[serde(flatten)]
    rest: Keys,
}

/// One entry of `actions`.
#[derive(Debug, Deserialize)]
struct ActionWire {
    id: Option<Field<String>>,
    label: Option<Field<String>>,
    #[serde(flatten)]
    rest: Keys,
}

/// One entry of `hooks`.
#[derive(Debug, Deserialize)]
struct HookWire {
    at: Option<Field<String>>,
    timeout: Option<Field<u64>>,
    #[serde(flatten)]
    rest: Keys,
}

/// One entry of `skills`.
#[derive(Debug, Deserialize)]
struct SkillWire {
    id: Option<Field<String>>,
    about: Option<Field<String>>,
    path: Option<Field<String>>,
    chats: Option<Field<Vec<Field<String>>>>,
    #[serde(flatten)]
    rest: Keys,
}

/// One entry of `tools`.
#[derive(Debug, Deserialize)]
struct ToolWire {
    id: Option<Field<String>>,
    about: Option<Field<String>>,
    /// The plugin's own JSON: [`params::rules`] checks its shape at read
    /// and [`params::check`] holds a call to it, which is why it is kept
    /// whole for [`Tool`].
    params: Option<Field<serde_json::Value>>,
    timeout: Option<Field<u64>>,
    chats: Option<Field<Vec<Field<String>>>>,
    #[serde(flatten)]
    rest: Keys,
}

/// One entry of `needs`.
#[derive(Debug, Deserialize)]
struct NeedWire {
    name: Option<Field<String>>,
    skills: Option<Field<Vec<Field<String>>>>,
    #[serde(flatten)]
    rest: Keys,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub id: String,
    pub label: String,
}

/// A skill: text in the repository for the chats it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub id: String,
    pub about: String,
    /// The file in the repository, read at the pinned commit.
    pub path: String,
    pub chats: Vec<String>,
}

/// A tool: a run of the plugin's image the chats it names may call.
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub label: String,
    pub license: Option<String>,
    /// The Dockerfile. None: no image and no runs.
    pub build: Option<String>,
    /// The asks as the manifest words them.
    pub asks: Vec<String>,
    /// A refresh's seconds, with a build.
    pub timeout: Option<u64>,
    /// `ui: {"kit": 1}`: the gate serves the kit to the view.
    pub kit: bool,
    /// The view's label, when the manifest has one.
    pub view: Option<String>,
    /// The views of `views`, which excludes `view`.
    pub views: Vec<View>,
    pub actions: Vec<Action>,
    /// The timeout of the hook at `after:change.offered`, when it has one.
    pub hook: Option<u64>,
    pub skills: Vec<Skill>,
    pub tools: Vec<Tool>,
    /// The plugins it needs, and what it wants from each.
    pub needs: Vec<Need>,
}

/// The reasons are collected, so a plugin's author reads them all at once.
struct Refusals(Vec<String>);

impl Refusals {
    fn say(&mut self, why: impl Into<String>) {
        self.0.push(why.into());
    }
}

/// A label: one line of 1 to 24 characters, no control character.
fn label(v: &Field<String>, field: &str, out: &mut Refusals) -> Option<String> {
    line(v, field, LABEL_MAX, out)
}

/// One line of 1 to `max` characters, no control character.
fn line(v: &Field<String>, field: &str, max: usize, out: &mut Refusals) -> Option<String> {
    let Some(s) = v.got.as_ref() else {
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

/// Whole seconds within `bounds`. A float is not whole seconds here: only
/// what serde reads as a `u64` is, as the reader has always refused it.
fn seconds(v: &Field<u64>, field: &str, bounds: (u64, u64), out: &mut Refusals) -> Option<u64> {
    match v.got {
        Some(n) if (bounds.0..=bounds.1).contains(&n) => Some(n),
        _ => {
            out.say(format!(
                "{field}: expected whole seconds from {} to {}",
                bounds.0, bounds.1
            ));
            None
        }
    }
}

/// The keys of a closed object that it may not hold, each said. The wire
/// already kept only the unknown ones, so the known names live once, in
/// the struct of that object.
fn closed(rest: &Keys, field: &str, out: &mut Refusals) {
    for key in rest.keys() {
        out.say(format!("{field}{key}: no field of manifest version 1"));
    }
}

/// An SPDX expression or `proprietary`, shown and never acted on, so only
/// its characters are checked.
fn is_license(s: &str) -> bool {
    (1..=LICENSE_MAX).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+:() -".contains(&b))
}

/// A path in the repository: parts of letters, digits and `._-` joined by
/// `/`, none of them `.`, none holding `..`.
fn is_build(s: &str) -> bool {
    s.len() <= BUILD_MAX
        && s.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && !part.contains("..")
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
}

/// A path of [`is_build`]'s shape to a file named `<something>.md`.
fn is_markdown(s: &str) -> bool {
    is_build(s)
        && s.rsplit('/')
            .next()
            .is_some_and(|f| f.len() > 3 && f.ends_with(".md"))
}

/// Read a manifest of the plugin `name`, the name TEHAS_PLUGINS gives it.
pub fn read(bytes: &[u8], name: &str) -> Result<Manifest, Vec<String>> {
    if bytes.len() > BYTES_MAX {
        return Err(vec![format!("the manifest is over {BYTES_MAX} bytes")]);
    }
    // One pass over the bytes. A field takes any JSON, so the wire itself
    // only fails to read when the top of the manifest is not an object.
    let wire: Wire = match serde_json::from_slice(bytes) {
        Ok(w) => w,
        Err(e) if e.is_data() => return Err(vec!["the manifest is not a JSON object".to_string()]),
        Err(e) => return Err(vec![format!("the manifest is not JSON: {e}")]),
    };
    let mut out = Refusals(Vec::new());
    for key in wire.rest.keys() {
        out.say(format!("{key}: no field of manifest version 1"));
    }
    // Each field name once, with whether it came, in the order a refusal
    // names them: the missing checks read this, the rules read the wire.
    let came: [(&str, bool); 16] = [
        ("plugin", wire.plugin.is_some()),
        ("kind", wire.kind.is_some()),
        ("name", wire.name.is_some()),
        ("label", wire.label.is_some()),
        ("license", wire.license.is_some()),
        ("build", wire.build.is_some()),
        ("asks", wire.asks.is_some()),
        ("timeout", wire.timeout.is_some()),
        ("ui", wire.ui.is_some()),
        ("view", wire.view.is_some()),
        ("views", wire.views.is_some()),
        ("actions", wire.actions.is_some()),
        ("hooks", wire.hooks.is_some()),
        ("skills", wire.skills.is_some()),
        ("tools", wire.tools.is_some()),
        ("needs", wire.needs.is_some()),
    ];
    for (name, given) in came {
        if REQUIRED.contains(&name) && !given {
            out.say(format!("{name} is missing"));
        }
    }
    let built = wire.build.is_some();
    if built {
        for (name, given) in came {
            if BUILT.contains(&name) && !given {
                out.say(format!("{name} is missing"));
            }
        }
    } else if wire.timeout.is_some() {
        out.say("timeout needs a build: a plugin with no build never runs");
    }

    if let Some(v) = &wire.plugin {
        if v.got != Some(1) {
            out.say(format!(
                "plugin is {}: this tehas reads manifest version 1",
                v.text
            ));
        }
    }
    if let Some(v) = &wire.kind {
        match v.got.as_deref() {
            Some("batch") => {}
            Some(kind) => out.say(format!("kind {kind} is reserved for plugins v2")),
            None => out.say("kind: expected a string"),
        }
    }
    let got_name = wire.name.as_ref().and_then(|v| match v.got.as_deref() {
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
    let got_label = wire
        .label
        .as_ref()
        .and_then(|v| label(v, "label", &mut out));
    let license = wire.license.as_ref().and_then(|v| match v.got.as_deref() {
        Some(s) if is_license(s) => Some(s.to_string()),
        _ => {
            out.say(format!(
                "license: expected an SPDX expression or proprietary, 1 to {LICENSE_MAX} characters"
            ));
            None
        }
    });
    let build = wire.build.as_ref().and_then(|v| match v.got.as_deref() {
        Some(s) if is_build(s) => Some(s.to_string()),
        _ => {
            out.say("build: expected a path in the repository, of letters, digits and ._/-, with no .. and no / at either end");
            None
        }
    });

    let mut asks: Vec<String> = Vec::new();
    let mut caps: Vec<Cap> = Vec::new();
    match wire.asks.as_ref().map(|v| v.got.as_ref()) {
        None => {}
        Some(None) => out.say("asks: expected a list of capabilities"),
        Some(Some(list)) => {
            if list.len() > ASKS_MAX {
                out.say(format!("asks: at most {ASKS_MAX}"));
            }
            for v in list {
                let Some(word) = v.got.as_deref() else {
                    out.say("asks: expected a list of capabilities");
                    continue;
                };
                match ask(word) {
                    Ok(cap) if caps.contains(&cap) => {
                        out.say(format!("asks: {word} is asked twice"))
                    }
                    Ok(cap) if !built && cap.needs_run() => out.say(format!(
                        "asks: {word} needs a build: a plugin with no build never runs"
                    )),
                    Ok(cap) => {
                        caps.push(cap);
                        asks.push(word.to_string());
                    }
                    Err(why) => out.say(format!("asks: {why}")),
                }
            }
        }
    }
    let timeout = wire
        .timeout
        .as_ref()
        .and_then(|v| seconds(v, "timeout", TIMEOUT, &mut out));

    let mut kit = false;
    if let Some(v) = &wire.ui {
        match v.got.as_ref() {
            Some(ui) => {
                closed(&ui.rest, "ui.", &mut out);
                match &ui.kit {
                    Some(k) if k.got == Some(1) => kit = true,
                    Some(k) => out.say(format!("ui.kit is {}: this tehas serves kit 1", k.text)),
                    None => out.say("ui.kit is missing"),
                }
            }
            None => out.say("ui: expected an object"),
        }
    }

    let view = match &wire.view {
        None => None,
        Some(v) if v.list => {
            out.say("view: expected an object; more than one view is views");
            None
        }
        Some(v) => match v.got.as_ref() {
            Some(t) => {
                closed(&t.rest, "view.", &mut out);
                if !caps.contains(&Cap::View) {
                    out.say("view needs the ask view");
                }
                match &t.label {
                    Some(l) => label(l, "view.label", &mut out),
                    None => {
                        out.say("view.label is missing");
                        None
                    }
                }
            }
            None => {
                out.say("view: expected an object");
                None
            }
        },
    };

    let mut actions: Vec<Action> = Vec::new();
    match wire.actions.as_ref().map(|v| v.got.as_ref()) {
        None => {}
        Some(None) => out.say("actions: expected a list"),
        Some(Some(list)) => {
            if list.len() > ACTIONS_MAX {
                out.say(format!("actions: at most {ACTIONS_MAX}"));
            }
            for (i, v) in list.iter().enumerate() {
                let at = format!("actions[{i}]");
                let Some(a) = v.got.as_ref() else {
                    out.say(format!("{at}: expected an object"));
                    continue;
                };
                closed(&a.rest, &format!("{at}."), &mut out);
                let id = match &a.id {
                    Some(f) if f.got.as_deref().is_some_and(is_action_id) => f.got.clone(),
                    Some(_) => {
                        out.say(format!("{at}.id: expected 1 to 16 characters of a-z and -"));
                        None
                    }
                    None => {
                        out.say(format!("{at}.id is missing"));
                        None
                    }
                };
                let l = match &a.label {
                    Some(l) => label(l, &format!("{at}.label"), &mut out),
                    None => {
                        out.say(format!("{at}.label is missing"));
                        None
                    }
                };
                let Some(id) = id else { continue };
                if actions.iter().any(|a| a.id == id) {
                    out.say(format!("{at}: the action {id} is there twice"));
                }
                if !caps.contains(&Cap::Action(id.clone())) {
                    out.say(format!("{at}: the action {id} needs the ask action:{id}"));
                }
                if let Some(label) = l {
                    actions.push(Action { id, label });
                }
            }
        }
    }

    let mut hook = None;
    let one = super::point(When::After, Name::ChangeOffered);
    match wire.hooks.as_ref().map(|v| v.got.as_ref()) {
        None => {}
        Some(None) => out.say("hooks: expected a list"),
        Some(Some(list)) => {
            if list.len() > 1 {
                out.say(format!("hooks: at most one, at {one}"));
            }
            if !caps.contains(&Cap::HOOK) {
                out.say(format!("hooks need the ask {}", Cap::HOOK.ask()));
            }
            for (i, v) in list.iter().enumerate() {
                let at = format!("hooks[{i}]");
                let Some(h) = v.got.as_ref() else {
                    out.say(format!("{at}: expected an object"));
                    continue;
                };
                closed(&h.rest, &format!("{at}."), &mut out);
                match h.at.as_ref().map(|f| f.got.as_deref()) {
                    Some(Some(point)) if point == one => {}
                    Some(Some("after:work")) => {
                        out.say(format!("{at}.at: the hook point after:work is now {one}"))
                    }
                    Some(Some(point)) => out.say(format!(
                        "{at}.at: the hook point {point} is reserved for plugins v2"
                    )),
                    Some(None) => out.say(format!("{at}.at: expected {one}")),
                    None => out.say(format!("{at}.at is missing")),
                }
                match &h.timeout {
                    Some(t) => hook = seconds(t, &format!("{at}.timeout"), HOOK_TIMEOUT, &mut out),
                    None => out.say(format!("{at}.timeout is missing")),
                }
            }
        }
    }

    let skills = match &wire.skills {
        None => Vec::new(),
        Some(v) => skills(v, &caps, &mut out),
    };
    let tools = match &wire.tools {
        None => Vec::new(),
        Some(v) => {
            if !built {
                out.say("tools need a build: a plugin with no build never runs");
            }
            tools(v, &caps, &mut out)
        }
    };
    let needs = match &wire.needs {
        None => Vec::new(),
        Some(v) => needs(v, &caps, name, &mut out),
    };
    let views = match &wire.views {
        None => Vec::new(),
        Some(v) => {
            if wire.view.is_some() {
                out.say("view and views: a manifest has one or the other");
            }
            views(v, &caps, &mut out)
        }
    };

    if !out.0.is_empty() {
        return Err(out.0);
    }
    // Every required field was read, or a refusal was said.
    match (got_name, got_label) {
        (Some(name), Some(label)) => Ok(Manifest {
            name,
            label,
            license,
            build,
            asks,
            timeout,
            kit,
            view,
            views,
            actions,
            hook,
            skills,
            tools,
            needs,
        }),
        _ => Err(vec!["the manifest could not be read".to_string()]),
    }
}

/// `skills`: at most [`SKILLS_MAX`], each asked as `skill:<id>`, each a
/// Markdown file of the repository for one or more chat hosts.
fn skills(v: &Field<Vec<Field<SkillWire>>>, caps: &[Cap], out: &mut Refusals) -> Vec<Skill> {
    let Some(list) = v.got.as_ref() else {
        out.say("skills: expected a list");
        return Vec::new();
    };
    if list.len() > SKILLS_MAX {
        out.say(format!("skills: at most {SKILLS_MAX}"));
    }
    let mut got: Vec<Skill> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let at = format!("skills[{i}]");
        let Some(sk) = v.got.as_ref() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(&sk.rest, &format!("{at}."), out);
        for (key, given) in [
            ("id", sk.id.is_some()),
            ("about", sk.about.is_some()),
            ("path", sk.path.is_some()),
            ("chats", sk.chats.is_some()),
        ] {
            if !given {
                out.say(format!("{at}.{key} is missing"));
            }
        }
        let id = sk.id.as_ref().and_then(|v| match v.got.as_deref() {
            Some(id) if is_action_id(id) => Some(id.to_string()),
            _ => {
                out.say(format!("{at}.id: expected 1 to 16 characters of a-z and -"));
                None
            }
        });
        let about = sk
            .about
            .as_ref()
            .and_then(|v| line(v, &format!("{at}.about"), ABOUT_MAX, out));
        let path = sk.path.as_ref().and_then(|v| match v.got.as_deref() {
            Some(p) if is_markdown(p) => Some(p.to_string()),
            _ => {
                out.say(format!(
                    "{at}.path: expected a .md file in the repository, of letters, digits and ._/-, with no .."
                ));
                None
            }
        });
        let chats = sk.chats.as_ref().and_then(|v| chats_of(v, &at, out));
        let Some(id) = id else { continue };
        if ids.contains(&id) {
            out.say(format!("{at}: the skill {id} is there twice"));
        }
        ids.push(id.clone());
        if !caps.contains(&Cap::Skill(id.clone())) {
            out.say(format!("{at}: the skill {id} needs the ask skill:{id}"));
        }
        if let (Some(about), Some(path), Some(chats)) = (about, path, chats) {
            got.push(Skill {
                id,
                about,
                path,
                chats,
            });
        }
    }
    got
}

/// `<at>.chats`: one or more of [`CHATS`], each once.
fn chats_of(v: &Field<Vec<Field<String>>>, at: &str, out: &mut Refusals) -> Option<Vec<String>> {
    let names: Option<Vec<String>> = v.got.as_ref().and_then(|l| {
        l.iter()
            .map(|c| {
                c.got
                    .as_deref()
                    .filter(|c| CHATS.contains(c))
                    .map(String::from)
            })
            .collect()
    });
    match names {
        Some(n) if !n.is_empty() && n.iter().enumerate().all(|(i, c)| !n[..i].contains(c)) => {
            Some(n)
        }
        _ => {
            out.say(format!(
                "{at}.chats: expected one or more of {}, each once",
                CHATS.join(", ")
            ));
            None
        }
    }
}

/// `tools`: at most [`TOOLS_MAX`], each asked as `tool:<id>`, with the
/// schema of its arguments, its timeout and the chats that may call it.
fn tools(v: &Field<Vec<Field<ToolWire>>>, caps: &[Cap], out: &mut Refusals) -> Vec<Tool> {
    let Some(list) = v.got.as_ref() else {
        out.say("tools: expected a list");
        return Vec::new();
    };
    if list.len() > TOOLS_MAX {
        out.say(format!("tools: at most {TOOLS_MAX}"));
    }
    let mut got: Vec<Tool> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let at = format!("tools[{i}]");
        let Some(t) = v.got.as_ref() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(&t.rest, &format!("{at}."), out);
        for (key, given) in [
            ("id", t.id.is_some()),
            ("about", t.about.is_some()),
            ("params", t.params.is_some()),
            ("timeout", t.timeout.is_some()),
            ("chats", t.chats.is_some()),
        ] {
            if !given {
                out.say(format!("{at}.{key} is missing"));
            }
        }
        let id = t.id.as_ref().and_then(|v| match v.got.as_deref() {
            Some(id) if is_action_id(id) => Some(id.to_string()),
            _ => {
                out.say(format!("{at}.id: expected 1 to 16 characters of a-z and -"));
                None
            }
        });
        let about = t
            .about
            .as_ref()
            .and_then(|v| line(v, &format!("{at}.about"), ABOUT_MAX, out));
        let schema = t.params.as_ref().and_then(|v| {
            // The schema is the plugin's own JSON: params.rs checks its
            // shape here and holds a call to it later, and Tool keeps it
            // whole, so it is carried as the document that came.
            let doc = v.got.clone()?;
            let why = params::rules(&doc);
            for w in &why {
                out.say(format!("{at}.params: {w}"));
            }
            why.is_empty().then_some(doc)
        });
        let timeout = t
            .timeout
            .as_ref()
            .and_then(|v| seconds(v, &format!("{at}.timeout"), TOOL_TIMEOUT, out));
        let chats = t.chats.as_ref().and_then(|v| chats_of(v, &at, out));
        let Some(id) = id else { continue };
        if ids.contains(&id) {
            out.say(format!("{at}: the tool {id} is there twice"));
        }
        ids.push(id.clone());
        if !caps.contains(&Cap::Tool(id.clone())) {
            out.say(format!("{at}: the tool {id} needs the ask tool:{id}"));
        }
        if let (Some(about), Some(params), Some(timeout), Some(chats)) =
            (about, schema, timeout, chats)
        {
            got.push(Tool {
                id,
                about,
                params,
                timeout,
                chats,
            });
        }
    }
    got
}

/// Where a view opens from the plugins menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Open {
    /// In the dock, a frame at the side of the page.
    Dock,
    /// In a page of its own, a new tab.
    Page,
}

/// One view of `views`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub id: String,
    pub label: String,
    /// A folder of `/out`, starting and ending with `/`.
    pub path: String,
    pub open: Open,
}

/// A view's path: `/`, or `/` and parts of letters, digits and `._-`, each
/// ending with `/`, none of them `.` or `..`.
fn is_view_path(s: &str) -> bool {
    s == "/"
        || (s.len() <= 128
            && s.starts_with('/')
            && s.ends_with('/')
            && s[1..s.len() - 1].split('/').all(|p| {
                !p.is_empty()
                    && p != "."
                    && p != ".."
                    && p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            }))
}

/// `views`: at most [`VIEWS_MAX`], each asked as `view:<id>`, each its own
/// folder of `/out`. A folder may hold another view's: the gate serves a
/// path by the view whose folder is the longest that holds it.
fn views(v: &Field<Vec<Field<ViewEntry>>>, caps: &[Cap], out: &mut Refusals) -> Vec<View> {
    let Some(list) = v.got.as_ref() else {
        out.say("views: expected a list");
        return Vec::new();
    };
    if list.is_empty() || list.len() > VIEWS_MAX {
        out.say(format!("views: expected 1 to {VIEWS_MAX} views"));
    }
    let mut got: Vec<View> = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let at = format!("views[{i}]");
        let Some(o) = v.got.as_ref() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(&o.rest, &format!("{at}."), out);
        let id = match o.id.as_ref().and_then(|f| f.got.as_deref()) {
            Some(id) if is_action_id(id) => Some(id.to_string()),
            _ => {
                out.say(format!("{at}.id: expected 1 to 16 characters of a-z and -"));
                None
            }
        };
        let label = match &o.label {
            Some(l) => label(l, &format!("{at}.label"), out),
            None => {
                out.say(format!("{at}.label is missing"));
                None
            }
        };
        let path = match o.path.as_ref().and_then(|f| f.got.as_deref()) {
            Some(p) if is_view_path(p) => Some(p.to_string()),
            _ => {
                out.say(format!(
                    "{at}.path: expected a folder of /out, starting and ending with /"
                ));
                None
            }
        };
        let open = match o.open.as_ref().and_then(|f| f.got.as_deref()) {
            Some("dock") => Some(Open::Dock),
            Some("page") => Some(Open::Page),
            _ => {
                out.say(format!("{at}.open: expected dock or page"));
                None
            }
        };
        let (Some(id), Some(label), Some(path), Some(open)) = (id, label, path, open) else {
            continue;
        };
        if got.iter().any(|g| g.id == id) {
            out.say(format!("{at}: the view {id} is there twice"));
        }
        if let Some(g) = got.iter().find(|g| g.path == path) {
            out.say(format!("{at}: the path {path} is the view {}'s too", g.id));
        }
        if !caps.contains(&Cap::ViewOf(id.clone())) {
            out.say(format!("{at}: the view {id} needs the ask view:{id}"));
        }
        got.push(View {
            id,
            label,
            path,
            open,
        });
    }
    got
}

/// `needs`: at most [`NEEDS_MAX`] other plugins, each asked as
/// `needs:<name>`, each with the skills this plugin wants from it.
fn needs(
    v: &Field<Vec<Field<NeedWire>>>,
    caps: &[Cap],
    own: &str,
    out: &mut Refusals,
) -> Vec<Need> {
    let Some(list) = v.got.as_ref() else {
        out.say("needs: expected a list");
        return Vec::new();
    };
    if list.len() > NEEDS_MAX {
        out.say(format!("needs: at most {NEEDS_MAX}"));
    }
    let mut got: Vec<Need> = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let at = format!("needs[{i}]");
        let Some(n) = v.got.as_ref() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(&n.rest, &format!("{at}."), out);
        let name = match &n.name {
            None => {
                out.say(format!("{at}.name is missing"));
                None
            }
            Some(f) if f.got.as_deref().is_some_and(is_name) => f.got.clone(),
            Some(_) => {
                out.say(format!(
                    "{at}.name: expected a plugin's name, 1 to 12 characters of a-z and 0-9, a letter first"
                ));
                None
            }
        };
        let skills: Option<Vec<String>> = match &n.skills {
            None => {
                out.say(format!("{at}.skills is missing"));
                None
            }
            Some(f) => {
                let ids: Option<Vec<String>> = f.got.as_ref().and_then(|l| {
                    l.iter()
                        .map(|s| {
                            s.got
                                .as_deref()
                                .filter(|s| is_action_id(s))
                                .map(String::from)
                        })
                        .collect()
                });
                match ids {
                    Some(ids)
                        if !ids.is_empty()
                            && ids.iter().enumerate().all(|(i, s)| !ids[..i].contains(s)) =>
                    {
                        Some(ids)
                    }
                    _ => {
                        out.say(format!(
                            "{at}.skills: expected one or more skill ids, each once"
                        ));
                        None
                    }
                }
            }
        };
        let Some(name) = name else { continue };
        if name == own {
            out.say(format!("{at}: a plugin does not need itself"));
        }
        if got.iter().any(|g| g.name == name) {
            out.say(format!("{at}: the plugin {name} is there twice"));
        }
        if !caps.contains(&Cap::Needs(name.clone())) {
            out.say(format!(
                "{at}: the plugin {name} needs the ask needs:{name}"
            ));
        }
        if let Some(skills) = skills {
            got.push(Need { name, skills });
        }
    }
    got
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map, Value};

    // The cases are written as the JSON documents a plugin's author
    // writes, edited by `json!` and handed to `read` as bytes: the byte
    // seam is the reader's interface, and the helpers below build those
    // documents as `Value`, which they name in the open.
    const FULL: &str = include_str!("../../../../api/plugin-manifest.full.json");
    const BARE: &str = include_str!("../../../../api/plugin-manifest.bare.json");
    const SKILLS: &str = include_str!("../../../../api/plugin-manifest.skills.json");
    const TOOLS: &str = include_str!("../../../../api/plugin-manifest.tools.json");
    const SCHEMA: &str = include_str!("../../../../api/plugin-manifest.schema.json");

    fn full() -> Value {
        serde_json::from_str(FULL).unwrap()
    }

    fn refusals(v: &Value) -> Vec<String> {
        read(v.to_string().as_bytes(), "floorplan").unwrap_err()
    }

    fn with(edit: impl Fn(&mut Map<String, Value>)) -> Value {
        let mut v = full();
        edit(v.as_object_mut().unwrap());
        v
    }

    fn text_only(edit: impl Fn(&mut Map<String, Value>)) -> Value {
        let mut v: Value = serde_json::from_str(SKILLS).unwrap();
        edit(v.as_object_mut().unwrap());
        v
    }

    fn refusals_of_specs(v: &Value) -> Vec<String> {
        read(v.to_string().as_bytes(), "specs").unwrap_err()
    }

    #[test]
    fn the_full_sample_is_read_whole() {
        let m = read(FULL.as_bytes(), "floorplan").unwrap();
        assert_eq!(
            m,
            Manifest {
                name: "floorplan".into(),
                label: "Floorplan".into(),
                license: Some("Apache-2.0".into()),
                build: Some("Dockerfile".into()),
                asks: [
                    "view",
                    "product",
                    "model",
                    "action:refresh",
                    "hook:after:change.offered"
                ]
                .map(String::from)
                .to_vec(),
                timeout: Some(3600),
                kit: true,
                view: Some("Floorplan".into()),
                views: vec![],
                actions: vec![Action {
                    id: "refresh".into(),
                    label: "Refresh floorplan".into()
                }],
                hook: Some(600),
                skills: vec![],
                tools: vec![],
                needs: vec![],
            }
        );
    }

    #[test]
    fn a_need_names_another_plugin_its_ask_and_the_skills_it_wants() {
        let needing = |needs: Value, asks: &[&str]| {
            with(|m| {
                for a in asks {
                    m["asks"].as_array_mut().unwrap().push(json!(a));
                }
                m.insert("needs".into(), needs.clone());
            })
        };
        let m = read(
            needing(
                json!([{"name": "specs", "skills": ["grill", "spec"]}]),
                &["needs:specs"],
            )
            .to_string()
            .as_bytes(),
            "floorplan",
        )
        .unwrap();
        assert_eq!(
            m.needs,
            [Need {
                name: "specs".into(),
                skills: vec!["grill".into(), "spec".into()]
            }]
        );
        assert_eq!(
            refusals(&needing(
                json!([{"name": "specs", "skills": ["grill"]}]),
                &[]
            )),
            ["needs[0]: the plugin specs needs the ask needs:specs"]
        );
        assert_eq!(
            refusals(&needing(
                json!([{"name": "floorplan", "skills": ["a"]}]),
                &["needs:floorplan"]
            )),
            ["needs[0]: a plugin does not need itself"]
        );
        assert_eq!(
            refusals(&needing(
                json!([{"name": "specs", "skills": ["a"]}, {"name": "specs", "skills": ["b"]}]),
                &["needs:specs"]
            )),
            ["needs[1]: the plugin specs is there twice"]
        );
        for (needs, why) in [
            (
                json!([{"name": "page-ticket", "skills": ["a"]}]),
                "needs[0].name: expected a plugin's name",
            ),
            (
                json!([{"name": "specs", "skills": []}]),
                "needs[0].skills: expected one or more",
            ),
            (json!([{"name": "specs"}]), "needs[0].skills is missing"),
            (
                json!([{"name": "specs", "skills": ["a"], "pin": 1}]),
                "needs[0].pin: no field",
            ),
            (json!({"name": "specs"}), "needs: expected a list"),
        ] {
            let got = refusals(&needing(needs.clone(), &["needs:specs"]));
            assert!(got.iter().any(|g| g.starts_with(why)), "{needs}: {got:?}");
        }
        assert_eq!(
            ask("needs:page-ticket").unwrap_err(),
            "needs:page-ticket: a plugin's name is 1 to 12 characters of a-z and 0-9, a letter first"
        );
        assert!(!Cap::Needs("specs".into()).needs_run());
    }

    #[test]
    fn the_bare_sample_holds_only_what_is_required() {
        let m = read(BARE.as_bytes(), "floorplan").unwrap();
        assert_eq!(m.asks, Vec::<String>::new());
        assert_eq!(
            (m.kit, m.view, m.hook, m.license),
            (false, None, None, None)
        );
        let bare: Map<String, Value> = serde_json::from_str(BARE).unwrap();
        let keys: Vec<&str> = bare.keys().map(String::as_str).collect();
        let mut want = [REQUIRED.as_slice(), BUILT.as_slice()].concat();
        want.sort_unstable();
        assert_eq!(keys, want);
    }

    #[test]
    fn what_is_not_a_json_object_is_refused() {
        let not = |b: &[u8]| read(b, "floorplan").unwrap_err();
        assert!(not(b"{")
            .first()
            .unwrap()
            .starts_with("the manifest is not JSON"));
        assert_eq!(not(b"[]"), ["the manifest is not a JSON object"]);
        assert_eq!(
            not(&vec![b' '; BYTES_MAX + 1]),
            ["the manifest is over 65536 bytes"]
        );
    }

    #[test]
    fn views_are_read_each_with_its_ask_its_own_path_and_where_it_opens() {
        let with_views = |views: Value, asks: &[&str]| {
            with(|m| {
                m.remove("view");
                let keep: Vec<Value> = m["asks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|a| *a != "view")
                    .cloned()
                    .collect();
                m["asks"] = json!(keep);
                for a in asks {
                    m["asks"].as_array_mut().unwrap().push(json!(a));
                }
                m.insert("views".into(), views.clone());
            })
        };
        let two = json!([
            {"id": "map", "label": "Floorplan", "path": "/", "open": "dock"},
            {"id": "health", "label": "Health", "path": "/health/", "open": "page"}
        ]);
        let m = read(
            with_views(two, &["view:map", "view:health"])
                .to_string()
                .as_bytes(),
            "floorplan",
        )
        .unwrap();
        assert_eq!(m.view, None);
        assert_eq!(
            m.views,
            [
                View {
                    id: "map".into(),
                    label: "Floorplan".into(),
                    path: "/".into(),
                    open: Open::Dock
                },
                View {
                    id: "health".into(),
                    label: "Health".into(),
                    path: "/health/".into(),
                    open: Open::Page
                }
            ]
        );
        let one = |v: Value| json!([v]);
        for (views, asks, why) in [
            (one(json!({"id": "map", "label": "M", "path": "/", "open": "dock"})), &[][..], "views[0]: the view map needs the ask view:map"),
            (one(json!({"id": "Map", "label": "M", "path": "/", "open": "dock"})), &["view:map"][..], "views[0].id: expected"),
            (one(json!({"id": "map", "label": "M", "path": "/a", "open": "dock"})), &["view:map"][..], "views[0].path: expected a folder"),
            (one(json!({"id": "map", "label": "M", "path": "/../", "open": "dock"})), &["view:map"][..], "views[0].path: expected a folder"),
            (one(json!({"id": "map", "label": "M", "path": "/", "open": "full"})), &["view:map"][..], "views[0].open: expected dock or page"),
            (one(json!({"id": "map", "label": "M", "path": "/", "open": "dock", "x": 1})), &["view:map"][..], "views[0].x: no field"),
            (json!([
                {"id": "a", "label": "A", "path": "/x/", "open": "dock"},
                {"id": "b", "label": "B", "path": "/x/", "open": "page"}
            ]), &["view:a", "view:b"][..], "views[1]: the path /x/ is the view a's too"),
            (json!([
                {"id": "a", "label": "A", "path": "/", "open": "dock"},
                {"id": "a", "label": "B", "path": "/b/", "open": "page"}
            ]), &["view:a"][..], "views[1]: the view a is there twice"),
            (json!((0..5).map(|i| json!({"id": format!("v{}", "x".repeat(i + 1)), "label": "V", "path": format!("/{i}/"), "open": "dock"})).collect::<Vec<_>>()), &[][..], "views: expected 1 to 4 views"),
        ] {
            let got = refusals(&with_views(views.clone(), asks));
            assert!(got.iter().any(|g| g.starts_with(why)), "{views}: {got:?}");
        }
        assert_eq!(ask("view:map"), Ok(Cap::ViewOf("map".into())));
        assert!(ask("view:Map").is_err());
    }

    #[test]
    fn a_field_it_does_not_know_is_refused() {
        let v = with(|m| {
            m.insert("secrets".into(), json!(["TOKEN"]));
        });
        assert_eq!(refusals(&v), ["secrets: no field of manifest version 1"]);
        // views is a field now, and it excludes view.
        let v = with(|m| {
            m.insert("views".into(), json!([]));
        });
        assert_eq!(
            refusals(&v),
            [
                "view and views: a manifest has one or the other",
                "views: expected 1 to 4 views"
            ]
        );
        for (field, inner) in [("ui", "ui.x"), ("view", "view.x")] {
            let v = with(|m| {
                m[field]
                    .as_object_mut()
                    .unwrap()
                    .insert("x".into(), json!(1));
            });
            assert_eq!(
                refusals(&v),
                [format!("{inner}: no field of manifest version 1")]
            );
        }
        let v = with(|m| {
            m["actions"][0]
                .as_object_mut()
                .unwrap()
                .insert("x".into(), json!(1));
            m["hooks"][0]
                .as_object_mut()
                .unwrap()
                .insert("x".into(), json!(1));
        });
        assert_eq!(
            refusals(&v),
            [
                "actions[0].x: no field of manifest version 1",
                "hooks[0].x: no field of manifest version 1"
            ]
        );
    }

    #[test]
    fn every_required_field_is_named_when_missing() {
        let got = read(b"{}", "floorplan").unwrap_err();
        let want: Vec<String> = REQUIRED.iter().map(|f| format!("{f} is missing")).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn the_version_is_1_and_the_kind_batch() {
        for v in [json!(2), json!("1"), json!(1.5), json!(0), json!(1.0)] {
            let got = refusals(&with(|m| {
                m.insert("plugin".into(), v.clone());
            }));
            assert_eq!(
                got,
                [format!(
                    "plugin is {v}: this tehas reads manifest version 1"
                )]
            );
        }
        let got = refusals(&with(|m| {
            m.insert("kind".into(), json!("service"));
        }));
        assert_eq!(got, ["kind service is reserved for plugins v2"]);
        let got = refusals(&with(|m| {
            m.insert("kind".into(), json!(1));
        }));
        assert_eq!(got, ["kind: expected a string"]);
    }

    #[test]
    fn the_name_is_the_one_the_settings_give() {
        let got = read(FULL.as_bytes(), "other").unwrap_err();
        assert_eq!(got, ["name is floorplan, and TEHAS_PLUGINS names it other"]);
        let got = refusals(&with(|m| {
            m.insert("name".into(), json!("Floorplan"));
        }));
        assert_eq!(
            got,
            ["name: expected 1 to 12 characters of a-z and 0-9, a letter first"]
        );
    }

    #[test]
    fn a_label_is_one_line_of_one_to_24_characters() {
        let ok = with(|m| {
            m.insert("label".into(), json!("Ääkköset ja pohjapiirros"));
        });
        assert!(read(ok.to_string().as_bytes(), "floorplan").is_ok());
        for (bad, why) in [
            (json!(""), "label: expected 1 to 24 characters"),
            (json!("x".repeat(25)), "label: expected 1 to 24 characters"),
            (json!("a\nb"), "label: a control character or a line break"),
            (
                json!("a\u{7}"),
                "label: a control character or a line break",
            ),
            (
                json!("a\u{2028}b"),
                "label: a control character or a line break",
            ),
            (json!(1), "label: expected a string"),
        ] {
            let got = refusals(&with(|m| {
                m.insert("label".into(), bad.clone());
            }));
            assert_eq!(got, [why], "{bad}");
        }
        let got = refusals(&with(|m| {
            m["view"]["label"] = json!("");
            m["actions"][0]["label"] = json!("x".repeat(25));
        }));
        assert_eq!(
            got,
            [
                "view.label: expected 1 to 24 characters",
                "actions[0].label: expected 1 to 24 characters"
            ]
        );
    }

    #[test]
    fn the_license_is_shown_and_only_its_characters_checked() {
        for good in [
            "proprietary",
            "MIT OR Apache-2.0",
            "(GPL-2.0-only WITH Classpath-exception-2.0)",
        ] {
            let v = with(|m| {
                m.insert("license".into(), json!(good));
            });
            assert!(
                read(v.to_string().as_bytes(), "floorplan").is_ok(),
                "{good}"
            );
        }
        for bad in [
            json!(""),
            json!("x".repeat(65)),
            json!("MIT\n"),
            json!("<b>"),
            json!(1),
        ] {
            let got = refusals(&with(|m| {
                m.insert("license".into(), bad.clone());
            }));
            assert_eq!(
                got,
                ["license: expected an SPDX expression or proprietary, 1 to 64 characters"]
            );
        }
    }

    #[test]
    fn the_build_is_a_path_inside_the_repository() {
        for good in [
            "Dockerfile",
            "docker/Dockerfile.plugin",
            ".tehas/Dockerfile",
            "a.",
        ] {
            assert!(is_build(good), "{good}");
        }
        for bad in [
            "",
            "/Dockerfile",
            "../Dockerfile",
            "a/../Dockerfile",
            "a/..",
            "a..b",
            "./Dockerfile",
            "a//b",
            "a/",
            "a b",
            "a\\b",
            &"x".repeat(256),
        ] {
            assert!(!is_build(bad), "{bad:?}");
        }
        let got = refusals(&with(|m| {
            m.insert("build".into(), json!("../x"));
        }));
        assert_eq!(got.len(), 1);
        assert!(got[0].starts_with("build: expected a path in the repository"));
    }

    #[test]
    fn an_ask_of_a_later_tehas_is_refused_naming_the_word() {
        for word in [
            "gate:verify",
            "hook:after:merge",
            "net:example.org",
            "factory-read",
            "factory-sessions",
        ] {
            let got = refusals(&with(|m| {
                m["asks"].as_array_mut().unwrap().push(json!(word));
            }));
            assert_eq!(got, [format!("asks: {word} is reserved for plugins v2")]);
        }
    }

    #[test]
    fn the_asks_are_capabilities_of_v1_each_once() {
        let got = refusals(&with(|m| {
            m.insert("asks".into(), json!(["view", "view", "nope", 1, "model:5"]));
            m.remove("view");
            m.remove("actions");
            m.remove("hooks");
        }));
        assert_eq!(
            got,
            [
                "asks: view is asked twice",
                "asks: nope is no capability of plugins v1",
                "asks: expected a list of capabilities",
                "asks: model:5: a manifest asks for model; the tokens are the grant's",
            ]
        );
        let many: Vec<String> = (0..=ASKS_MAX as u8)
            .map(|i| {
                format!(
                    "skill:{}{}",
                    (b'a' + i / 26) as char,
                    (b'a' + i % 26) as char
                )
            })
            .collect();
        let got = refusals(&with(|m| {
            m.insert("asks".into(), json!(many));
        }));
        assert_eq!(got[0], "asks: at most 32");
    }

    #[test]
    fn a_plugin_with_no_build_is_text_alone() {
        let m = read(SKILLS.as_bytes(), "specs").unwrap();
        assert_eq!((m.build, m.timeout, m.view), (None, None, None));
        assert_eq!(m.asks, ["skill:grill", "skill:spec"]);
        assert_eq!(
            m.skills[0],
            Skill {
                id: "grill".into(),
                about: "Interview the person until the ticket has no open decision".into(),
                path: "skills/grill.md".into(),
                chats: vec!["obeya".into(), "wizard".into()],
            }
        );
        let kind = text_only(|m| {
            m.insert("kind".into(), json!("batch"));
        });
        assert!(read(kind.to_string().as_bytes(), "specs").is_ok());
        let got = refusals_of_specs(&text_only(|m| {
            m.insert("timeout".into(), json!(60));
            m["asks"].as_array_mut().unwrap().push(json!("product"));
            m["asks"].as_array_mut().unwrap().push(json!("action:go"));
        }));
        assert_eq!(
            got,
            [
                "timeout needs a build: a plugin with no build never runs",
                "asks: product needs a build: a plugin with no build never runs",
                "asks: action:go needs a build: a plugin with no build never runs",
            ]
        );
        let got = refusals_of_specs(&text_only(|m| {
            m.insert("build".into(), json!("Dockerfile"));
        }));
        assert_eq!(got, ["kind is missing", "timeout is missing"]);
    }

    #[test]
    fn a_skill_is_a_markdown_file_for_named_chats_and_needs_its_ask() {
        let got = refusals_of_specs(&text_only(|m| {
            m["skills"][0]["id"] = json!("Grill");
            m["skills"][1]["about"] = json!("x".repeat(ABOUT_MAX + 1));
        }));
        assert_eq!(
            got,
            [
                "skills[0].id: expected 1 to 16 characters of a-z and -",
                "skills[1].about: expected 1 to 120 characters",
            ]
        );
        for path in [
            "skills/grill.txt",
            "../grill.md",
            ".md",
            "a/.md",
            "a..md",
            "/grill.md",
        ] {
            let got = refusals_of_specs(&text_only(|m| {
                m["skills"][0]["path"] = json!(path);
            }));
            assert_eq!(
                got,
                ["skills[0].path: expected a .md file in the repository, of letters, digits and ._/-, with no .."],
                "{path}"
            );
        }
        for chats in [
            json!([]),
            json!(["slack"]),
            json!(["obeya", "obeya"]),
            json!("obeya"),
        ] {
            let got = refusals_of_specs(&text_only(|m| {
                m["skills"][0]["chats"] = chats.clone();
            }));
            assert_eq!(
                got,
                ["skills[0].chats: expected one or more of obeya, wizard, assistant, plugin, each once"],
                "{chats}"
            );
        }
        let got = refusals_of_specs(&text_only(|m| {
            m["asks"] = json!(["skill:grill"]);
            m["skills"][1]["id"] = json!("grill");
            m["skills"][0].as_object_mut().unwrap().remove("about");
            m["skills"][0]
                .as_object_mut()
                .unwrap()
                .insert("x".into(), json!(1));
        }));
        assert_eq!(
            got,
            [
                "skills[0].x: no field of manifest version 1",
                "skills[0].about is missing",
                "skills[1]: the skill grill is there twice",
            ]
        );
        let got = refusals_of_specs(&text_only(|m| {
            m["asks"] = json!(["skill:grill"]);
        }));
        assert_eq!(got, ["skills[1]: the skill spec needs the ask skill:spec"]);
        let nine: Vec<Value> = (0..=SKILLS_MAX)
            .map(|i| json!({"id": format!("s{}", "x".repeat(i)), "about": "A", "path": "a.md", "chats": ["obeya"]}))
            .collect();
        let got = refusals_of_specs(&text_only(|m| {
            m["asks"] = json!((0..=SKILLS_MAX)
                .map(|i| format!("skill:s{}", "x".repeat(i)))
                .collect::<Vec<_>>());
            m["skills"] = json!(nine);
        }));
        assert_eq!(got, ["skills: at most 8"]);
    }

    fn tooled(edit: impl Fn(&mut Map<String, Value>)) -> Value {
        let mut v: Value = serde_json::from_str(TOOLS).unwrap();
        edit(v.as_object_mut().unwrap());
        v
    }

    fn refusals_of_docs(v: &Value) -> Vec<String> {
        read(v.to_string().as_bytes(), "docs").unwrap_err()
    }

    #[test]
    fn a_tool_is_read_with_its_schema_timeout_and_chats() {
        let m = read(TOOLS.as_bytes(), "docs").unwrap();
        assert_eq!(m.asks, ["tool:find-doc"]);
        assert_eq!(
            m.tools,
            [Tool {
                id: "find-doc".into(),
                about: "Find the product doc that answers a question".into(),
                params: json!({"type": "object", "properties": {"q": {"type": "string", "maxLength": 200}}, "required": ["q"], "additionalProperties": false}),
                timeout: 5,
                chats: vec!["obeya".into()],
            }]
        );
    }

    #[test]
    fn a_tool_that_breaks_a_rule_is_refused_with_why() {
        let got = refusals_of_docs(&tooled(|m| {
            m["tools"][0]["id"] = json!("Find");
            m["tools"][0]["about"] = json!("a\nb");
            m["tools"][0]["timeout"] = json!(11);
            m["tools"][0]["chats"] = json!(["slack"]);
            m["tools"][0]["x"] = json!(1);
        }));
        assert_eq!(
            got,
            [
                "tools[0].x: no field of manifest version 1",
                "tools[0].id: expected 1 to 16 characters of a-z and -",
                "tools[0].about: a control character or a line break",
                "tools[0].timeout: expected whole seconds from 1 to 10",
                "tools[0].chats: expected one or more of obeya, wizard, assistant, plugin, each once",
            ]
        );
        for t in [0, 11] {
            let got = refusals_of_docs(&tooled(|m| m["tools"][0]["timeout"] = json!(t)));
            assert_eq!(
                got,
                ["tools[0].timeout: expected whole seconds from 1 to 10"]
            );
        }
        let got = refusals_of_docs(&tooled(|m| m["tools"][0]["timeout"] = json!(5.0)));
        assert_eq!(
            got,
            ["tools[0].timeout: expected whole seconds from 1 to 10"]
        );
        let got = refusals_of_docs(&tooled(|m| {
            m["tools"][0]["params"]["properties"]["q"] = json!({"type": "string"});
            m["tools"][0]["params"]["$ref"] = json!("#/x");
        }));
        assert_eq!(
            got,
            [
                "tools[0].params: the top level: $ref is not allowed",
                "tools[0].params: properties.q: a string needs maxLength",
            ]
        );
        let got = refusals_of_docs(&tooled(|m| {
            m["tools"][0].as_object_mut().unwrap().remove("params");
        }));
        assert_eq!(got, ["tools[0].params is missing"]);
    }

    #[test]
    fn a_tool_needs_its_ask_a_build_and_an_id_of_its_own() {
        let got = refusals_of_docs(&tooled(|m| {
            m["asks"] = json!([]);
        }));
        assert_eq!(
            got,
            ["tools[0]: the tool find-doc needs the ask tool:find-doc"]
        );
        let got = refusals_of_docs(&tooled(|m| {
            let again = m["tools"][0].clone();
            m["tools"].as_array_mut().unwrap().push(again);
        }));
        assert_eq!(got, ["tools[1]: the tool find-doc is there twice"]);
        let got = refusals_of_docs(&tooled(|m| {
            for k in ["build", "kind", "timeout"] {
                m.remove(k);
            }
        }));
        assert_eq!(
            got,
            [
                "asks: tool:find-doc needs a build: a plugin with no build never runs",
                "tools need a build: a plugin with no build never runs",
                "tools[0]: the tool find-doc needs the ask tool:find-doc",
            ]
        );
        let nine: Vec<Value> = (0..=TOOLS_MAX)
            .map(|i| {
                let mut t: Value =
                    serde_json::from_str::<Value>(TOOLS).unwrap()["tools"][0].clone();
                t["id"] = json!(format!("t{}", "x".repeat(i)));
                t
            })
            .collect();
        let got = refusals_of_docs(&tooled(|m| {
            m["asks"] = json!(nine
                .iter()
                .map(|t| format!("tool:{}", t["id"].as_str().unwrap()))
                .collect::<Vec<_>>());
            m["tools"] = json!(nine);
        }));
        assert_eq!(got, ["tools: at most 8"]);
    }

    #[test]
    fn a_view_an_action_and_a_hook_each_need_their_ask() {
        let got = refusals(&with(|m| {
            m.insert("asks".into(), json!(["product"]));
        }));
        assert_eq!(
            got,
            [
                "view needs the ask view",
                "actions[0]: the action refresh needs the ask action:refresh",
                "hooks need the ask hook:after:change.offered",
            ]
        );
    }

    #[test]
    fn the_timeouts_are_within_their_bounds() {
        for (t, ok) in [(59, false), (60, true), (14400, true), (14401, false)] {
            let v = with(|m| {
                m.insert("timeout".into(), json!(t));
            });
            assert_eq!(
                read(v.to_string().as_bytes(), "floorplan").is_ok(),
                ok,
                "{t}"
            );
        }
        let got = refusals(&with(|m| {
            m.insert("timeout".into(), json!("3600"));
        }));
        assert_eq!(got, ["timeout: expected whole seconds from 60 to 14400"]);
        // A float is not whole seconds, not even one that is whole.
        let got = refusals(&with(|m| {
            m.insert("timeout".into(), json!(3600.0));
        }));
        assert_eq!(got, ["timeout: expected whole seconds from 60 to 14400"]);
        for (t, ok) in [(59, false), (60, true), (1800, true), (1801, false)] {
            let v = with(|m| {
                m["hooks"][0]["timeout"] = json!(t);
            });
            assert_eq!(
                read(v.to_string().as_bytes(), "floorplan").is_ok(),
                ok,
                "{t}"
            );
        }
        let got = refusals(&with(|m| {
            m["hooks"][0]["timeout"] = json!(600.0);
        }));
        assert_eq!(
            got,
            ["hooks[0].timeout: expected whole seconds from 60 to 1800"]
        );
    }

    #[test]
    fn the_ui_view_actions_and_hooks_keep_their_shapes() {
        let cases: Vec<(Value, &str)> = vec![
            (
                with(|m| {
                    m.insert("ui".into(), json!({"kit": 2}));
                }),
                "ui.kit is 2: this tehas serves kit 1",
            ),
            (
                with(|m| {
                    m.insert("ui".into(), json!({"kit": 1.0}));
                }),
                "ui.kit is 1.0: this tehas serves kit 1",
            ),
            (
                with(|m| {
                    m.insert("ui".into(), json!({}));
                }),
                "ui.kit is missing",
            ),
            (
                with(|m| {
                    m.insert("view".into(), json!([{"label": "A"}, {"label": "B"}]));
                }),
                "view: expected an object; more than one view is views",
            ),
            (
                with(|m| {
                    m.insert("view".into(), json!({}));
                }),
                "view.label is missing",
            ),
            (
                with(|m| {
                    m["actions"][0]["id"] = json!("Refresh");
                }),
                "actions[0].id: expected 1 to 16 characters of a-z and -",
            ),
            (
                with(|m| {
                    m["actions"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"id": "refresh", "label": "Again"}));
                }),
                "actions[1]: the action refresh is there twice",
            ),
            (
                with(|m| {
                    m["hooks"][0]["at"] = json!("before:work");
                }),
                "hooks[0].at: the hook point before:work is reserved for plugins v2",
            ),
            (
                with(|m| {
                    m["hooks"][0]["at"] = json!("after:work");
                }),
                "hooks[0].at: the hook point after:work is now after:change.offered",
            ),
            (
                with(|m| {
                    m["hooks"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"at": "after:change.offered", "timeout": 60}));
                }),
                "hooks: at most one, at after:change.offered",
            ),
        ];
        for (v, why) in cases {
            assert_eq!(refusals(&v), [why]);
        }
        let got = refusals(&with(|m| {
            m["actions"] = json!([
                {"id": "a", "label": "A"},
                {"id": "b", "label": "B"},
                {"id": "c", "label": "C"}
            ]);
        }));
        assert_eq!(got[0], "actions: at most 2");
    }

    // The reader and the schema must agree on each case below: the samples,
    // and each field broken one way. What the schema cannot say, such as an
    // action's ask of its own id, is the reader's alone.
    use crate::plugin::schema::{closed, fits};

    fn schema() -> Value {
        serde_json::from_str(SCHEMA).unwrap()
    }

    #[test]
    fn the_schema_is_json_schema_2020_12_and_closes_every_object() {
        let s = schema();
        assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
        assert_eq!(
            s["$id"],
            "https://github.com/tehas-app/tehas-factory/api/plugin-manifest.schema.json"
        );
        closed(&s);
    }

    #[test]
    fn the_schema_names_the_readers_fields_and_bounds() {
        let s = schema();
        let props: Vec<&String> = s["properties"].as_object().unwrap().keys().collect();
        let mut fields = FIELDS.to_vec();
        fields.sort_unstable();
        let mut props = props.into_iter().map(String::as_str).collect::<Vec<_>>();
        props.sort_unstable();
        assert_eq!(props, fields);
        let required: Vec<&str> = s["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        assert_eq!(required, REQUIRED);
        let p = &s["properties"];
        assert_eq!(p["timeout"]["minimum"], TIMEOUT.0);
        assert_eq!(p["timeout"]["maximum"], TIMEOUT.1);
        assert_eq!(
            p["hooks"]["items"]["properties"]["timeout"]["minimum"],
            HOOK_TIMEOUT.0
        );
        assert_eq!(
            p["hooks"]["items"]["properties"]["timeout"]["maximum"],
            HOOK_TIMEOUT.1
        );
        assert_eq!(p["label"]["maxLength"], LABEL_MAX);
        assert_eq!(p["license"]["maxLength"], LICENSE_MAX);
        assert_eq!(p["build"]["maxLength"], BUILD_MAX);
        assert_eq!(p["asks"]["maxItems"], ASKS_MAX);
        assert_eq!(p["actions"]["maxItems"], ACTIONS_MAX);
        assert_eq!(p["skills"]["maxItems"], SKILLS_MAX);
        assert_eq!(p["tools"]["maxItems"], TOOLS_MAX);
        let tool = &p["tools"]["items"]["properties"];
        assert_eq!(tool["about"]["maxLength"], ABOUT_MAX);
        assert_eq!(tool["timeout"]["minimum"], TOOL_TIMEOUT.0);
        assert_eq!(tool["timeout"]["maximum"], TOOL_TIMEOUT.1);
        assert_eq!(tool["chats"]["items"]["enum"], json!(CHATS));
        let skill = &p["skills"]["items"]["properties"];
        assert_eq!(skill["about"]["maxLength"], ABOUT_MAX);
        assert_eq!(skill["chats"]["items"]["enum"], json!(CHATS));
        assert_eq!(
            s["dependentSchemas"]["build"]["required"],
            json!(BUILT[..1].iter().chain(&BUILT[2..]).collect::<Vec<_>>())
        );
    }

    #[test]
    fn the_reader_and_the_schema_agree() {
        let s = schema();
        let mut cases: Vec<(String, Value)> = vec![
            ("full".into(), full()),
            ("bare".into(), serde_json::from_str(BARE).unwrap()),
            ("skills".into(), serde_json::from_str(SKILLS).unwrap()),
            ("tools".into(), serde_json::from_str(TOOLS).unwrap()),
        ];
        let views_case = |views: Value, keep_view: bool| {
            with(|m| {
                let mut asks: Vec<Value> = m["asks"].as_array().unwrap().clone();
                if !keep_view {
                    m.remove("view");
                    asks.retain(|a| a != "view");
                }
                asks.push(json!("view:map"));
                asks.push(json!("view:health"));
                m["asks"] = json!(asks);
                m.insert("views".into(), views.clone());
            })
        };
        let v = |id: &str, path: &str, open: &str| json!({"id": id, "label": "V", "path": path, "open": open});
        for (what, views, keep_view) in [
            (
                "views",
                json!([v("map", "/", "dock"), v("health", "/health/", "page")]),
                false,
            ),
            ("views and view", json!([v("map", "/", "dock")]), true),
            ("view open", json!([v("map", "/", "full")]), false),
            ("view path", json!([v("map", "/a", "dock")]), false),
            ("view path dots", json!([v("map", "/../", "dock")]), false),
            (
                "view path dot part",
                json!([v("map", "/.x/", "dock")]),
                false,
            ),
            (
                "five views",
                json!([
                    v("map", "/", "dock"),
                    v("health", "/a/", "dock"),
                    v("health", "/b/", "dock"),
                    v("map", "/c/", "dock"),
                    v("map", "/d/", "dock")
                ]),
                false,
            ),
        ] {
            cases.push((what.into(), views_case(views, keep_view)));
        }
        let tool_cases: Vec<(&str, &str, Value)> = vec![
            ("tool id", "id", json!("Find")),
            ("tool about", "about", json!("x".repeat(121))),
            ("tool timeout 0", "timeout", json!(0)),
            ("tool timeout 10", "timeout", json!(10)),
            ("tool timeout 11", "timeout", json!(11)),
            ("tool no chat", "chats", json!([])),
            ("tool chat twice", "chats", json!(["obeya", "obeya"])),
            ("tool field", "x", json!(1)),
        ];
        for (what, key, v) in tool_cases {
            cases.push((
                what.into(),
                tooled(|m| {
                    m["tools"][0][key] = v.clone();
                }),
            ));
        }
        cases.push((
            "tools without a build".into(),
            tooled(|m| {
                for k in ["build", "kind", "timeout"] {
                    m.remove(k);
                }
                m["asks"] = json!([]);
            }),
        ));
        cases.push((
            "tool ask".into(),
            tooled(|m| {
                m["asks"] = json!(["tool:Find"]);
                m.remove("tools");
            }),
        ));
        for (what, needs) in [
            ("needs", json!([{"name": "specs", "skills": ["grill"]}])),
            ("need name", json!([{"name": "Specs", "skills": ["grill"]}])),
            ("need no skill", json!([{"name": "specs", "skills": []}])),
            (
                "need skill twice",
                json!([{"name": "specs", "skills": ["a", "a"]}]),
            ),
            (
                "need field",
                json!([{"name": "specs", "skills": ["a"], "pin": "x"}]),
            ),
            (
                "three needs",
                json!([
                    {"name": "specs", "skills": ["a"]},
                    {"name": "specs", "skills": ["a"]},
                    {"name": "specs", "skills": ["a"]}
                ]),
            ),
        ] {
            cases.push((
                what.into(),
                with(|m| {
                    m["asks"].as_array_mut().unwrap().push(json!("needs:specs"));
                    m.insert("needs".into(), needs.clone());
                }),
            ));
        }
        let text = |m: &mut Map<String, Value>| {
            let t: Map<String, Value> = serde_json::from_str(SKILLS).unwrap();
            *m = t;
        };
        let skill_cases: Vec<(&str, Value)> = vec![
            ("skill path", json!("grill.txt")),
            ("skill dots", json!("a..md")),
            ("skill dir", json!("a./b.md")),
            ("skill stem", json!(".md")),
        ];
        for (what, path) in skill_cases {
            cases.push((
                what.into(),
                with(|m| {
                    text(m);
                    m["skills"][0]["path"] = path.clone();
                }),
            ));
        }
        for (what, chats) in [
            ("no chat", json!([])),
            ("chat", json!(["slack"])),
            ("chat twice", json!(["obeya", "obeya"])),
        ] {
            cases.push((
                what.into(),
                with(|m| {
                    text(m);
                    m["skills"][0]["chats"] = chats.clone();
                }),
            ));
        }
        cases.push((
            "about".into(),
            with(|m| {
                text(m);
                m["skills"][0]["about"] = json!("x".repeat(121));
            }),
        ));
        cases.push((
            "text with kind".into(),
            with(|m| {
                text(m);
                m.insert("kind".into(), json!("batch"));
            }),
        ));
        cases.push((
            "text with timeout".into(),
            with(|m| {
                text(m);
                m.insert("timeout".into(), json!(60));
            }),
        ));
        cases.push((
            "build without timeout".into(),
            with(|m| {
                m.remove("timeout");
            }),
        ));
        cases.push((
            "build without kind".into(),
            with(|m| {
                m.remove("kind");
            }),
        ));
        cases.push(("full with a skill".into(), with(|m| {
            m["asks"].as_array_mut().unwrap().push(json!("skill:map"));
            m.insert("skills".into(), json!([{"id": "map", "about": "Read the map", "path": "map.md", "chats": ["assistant"]}]));
        })));
        let mut case = |what: &str, edit: &dyn Fn(&mut Map<String, Value>)| {
            cases.push((what.to_string(), with(edit)));
        };
        case("unknown field", &|m| {
            m.insert("x".into(), json!(1));
        });
        case("plugin 2", &|m| {
            m.insert("plugin".into(), json!(2));
        });
        case("kind", &|m| {
            m.insert("kind".into(), json!("service"));
        });
        case("name", &|m| {
            m.insert("name".into(), json!("1a"));
        });
        case("long name", &|m| {
            m.insert("name".into(), json!("abcdefghijklm"));
        });
        case("empty label", &|m| {
            m.insert("label".into(), json!(""));
        });
        case("long label", &|m| {
            m.insert("label".into(), json!("x".repeat(25)));
        });
        case("24 characters", &|m| {
            m.insert("label".into(), json!("ä".repeat(24)));
        });
        case("line break", &|m| {
            m.insert("label".into(), json!("a\nb"));
        });
        case("C1 control", &|m| {
            m.insert("label".into(), json!("a\u{85}b"));
        });
        case("line separator", &|m| {
            m.insert("label".into(), json!("a\u{2029}b"));
        });
        case("license", &|m| {
            m.insert("license".into(), json!("<b>"));
        });
        case("no license", &|m| {
            m.remove("license");
        });
        for build in [
            "../x",
            "a/../b",
            "/x",
            "a/",
            "./x",
            "a..b",
            "a//b",
            ".tehas/Dockerfile",
            "a.",
        ] {
            case(build, &|m| {
                m.insert("build".into(), json!(build));
            });
        }
        case("ask v2", &|m| {
            m["asks"].as_array_mut().unwrap().push(json!("gate:verify"));
        });
        case("ask twice", &|m| {
            m["asks"].as_array_mut().unwrap().push(json!("product"));
        });
        case("model tokens", &|m| {
            m["asks"][2] = json!("model:5");
        });
        case("seven asks", &|m| {
            m["asks"] = json!([
                "view",
                "product",
                "model",
                "hook:after:change.offered",
                "action:a",
                "action:b",
                "action:refresh"
            ]);
        });
        for t in [59, 60, 14400, 14401] {
            case("timeout", &|m| {
                m.insert("timeout".into(), json!(t));
            });
        }
        case("kit 2", &|m| {
            m["ui"]["kit"] = json!(2);
        });
        case("no kit", &|m| {
            m.insert("ui".into(), json!({}));
        });
        case("two views", &|m| {
            m.insert("view".into(), json!([{"label": "A"}]));
        });
        case("view without its ask", &|m| {
            m["asks"] = json!([
                "product",
                "model",
                "action:refresh",
                "hook:after:change.offered"
            ]);
        });
        case("no view label", &|m| {
            m.insert("view".into(), json!({}));
        });
        case("action id", &|m| {
            m["actions"][0]["id"] = json!("a_b");
        });
        case("three actions", &|m| {
            m["actions"] = json!([{"id": "a", "label": "A"}, {"id": "b", "label": "B"}, {"id": "refresh", "label": "C"}]);
        });
        case("hook point", &|m| {
            m["hooks"][0]["at"] = json!("after:merge");
        });
        case("two hooks", &|m| {
            m["hooks"]
                .as_array_mut()
                .unwrap()
                .push(json!({"at": "after:change.offered", "timeout": 60}));
        });
        case("hook without its ask", &|m| {
            m["asks"] = json!(["view", "product", "model", "action:refresh"]);
        });
        for t in [59, 1801] {
            case("hook timeout", &|m| {
                m["hooks"][0]["timeout"] = json!(t);
            });
        }
        for (what, v) in &cases {
            let reader = read(v.to_string().as_bytes(), v["name"].as_str().unwrap_or("x"));
            assert_eq!(
                reader.is_ok(),
                fits(v, &s),
                "{what}: the reader says {reader:?}"
            );
        }
        assert!(cases.iter().filter(|(_, v)| fits(v, &s)).count() >= 6);
    }
}
