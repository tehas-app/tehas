//! The reader of `tehas-plugin.json`, a plugin's manifest: bytes in, a typed
//! manifest or every reason it is refused out. A field it does not know is
//! refused, so a manifest made for a later tehas never runs half read.
//! api/plugin-manifest.schema.json holds the same rules for a plugin's
//! author; the tests below hold the two to each other.

use super::needs::Need;
use super::{ask, is_action_id, is_name, params, Cap};
use serde_json::{Map, Value};

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
pub const FIELDS: [&str; 15] = [
    "plugin", "kind", "name", "label", "license", "build", "asks", "timeout", "ui", "view",
    "actions", "hooks", "skills", "tools", "needs",
];
pub const REQUIRED: [&str; 4] = ["plugin", "name", "label", "asks"];
/// Required of a plugin with an image. One with none is text alone.
pub const BUILT: [&str; 3] = ["kind", "build", "timeout"];

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
    /// The panel's label, when the manifest has a view.
    pub view: Option<String>,
    pub actions: Vec<Action>,
    /// The timeout of the hook at `after:work`, when it has one.
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
                "{field}: expected whole seconds from {} to {}",
                bounds.0, bounds.1
            ));
            None
        }
    }
}

/// The keys of an object that are not `known`, each said.
fn closed(m: &Map<String, Value>, known: &[&str], field: &str, out: &mut Refusals) {
    for key in m.keys().filter(|k| !known.contains(&k.as_str())) {
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
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(e) => return Err(vec![format!("the manifest is not JSON: {e}")]),
    };
    let Some(m) = value.as_object() else {
        return Err(vec!["the manifest is not a JSON object".to_string()]);
    };
    let mut out = Refusals(Vec::new());
    for key in m.keys() {
        if key == "views" {
            out.say("views: more than one view is reserved for plugins v2");
        } else if !FIELDS.contains(&key.as_str()) {
            out.say(format!("{key}: no field of manifest version 1"));
        }
    }
    for field in REQUIRED.iter().filter(|f| !m.contains_key(**f)) {
        out.say(format!("{field} is missing"));
    }
    let built = m.contains_key("build");
    if built {
        for field in BUILT.iter().filter(|f| !m.contains_key(**f)) {
            out.say(format!("{field} is missing"));
        }
    } else if m.contains_key("timeout") {
        out.say("timeout needs a build: a plugin with no build never runs");
    }
    let field = |key: &str| m.get(key);

    if let Some(v) = field("plugin") {
        if v.as_u64() != Some(1) {
            out.say(format!(
                "plugin is {v}: this tehas reads manifest version 1"
            ));
        }
    }
    if let Some(v) = field("kind") {
        match v.as_str() {
            Some("batch") => {}
            Some(kind) => out.say(format!("kind {kind} is reserved for plugins v2")),
            None => out.say("kind: expected a string"),
        }
    }
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
    let license = match field("license") {
        None => None,
        Some(v) => match v.as_str() {
            Some(s) if is_license(s) => Some(s.to_string()),
            _ => {
                out.say(format!(
                    "license: expected an SPDX expression or proprietary, 1 to {LICENSE_MAX} characters"
                ));
                None
            }
        },
    };
    let build = field("build").and_then(|v| match v.as_str() {
        Some(s) if is_build(s) => Some(s.to_string()),
        _ => {
            out.say("build: expected a path in the repository, of letters, digits and ._/-, with no .. and no / at either end");
            None
        }
    });

    let mut asks: Vec<String> = Vec::new();
    let mut caps: Vec<Cap> = Vec::new();
    match field("asks").map(|v| v.as_array()) {
        None => {}
        Some(None) => out.say("asks: expected a list of capabilities"),
        Some(Some(list)) => {
            if list.len() > ASKS_MAX {
                out.say(format!("asks: at most {ASKS_MAX}"));
            }
            for v in list {
                let Some(word) = v.as_str() else {
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
    let timeout = field("timeout").and_then(|v| seconds(v, "timeout", TIMEOUT, &mut out));

    let mut kit = false;
    if let Some(v) = field("ui") {
        match v.as_object() {
            Some(ui) => {
                closed(ui, &["kit"], "ui.", &mut out);
                match ui.get("kit") {
                    Some(k) if k.as_u64() == Some(1) => kit = true,
                    Some(k) => out.say(format!("ui.kit is {k}: this tehas serves kit 1")),
                    None => out.say("ui.kit is missing"),
                }
            }
            None => out.say("ui: expected an object"),
        }
    }

    let view = match field("view") {
        None => None,
        Some(Value::Array(_)) => {
            out.say("view: more than one view is reserved for plugins v2");
            None
        }
        Some(Value::Object(v)) => {
            closed(v, &["label"], "view.", &mut out);
            if !caps.contains(&Cap::View) {
                out.say("view needs the ask view");
            }
            match v.get("label") {
                Some(l) => label(l, "view.label", &mut out),
                None => {
                    out.say("view.label is missing");
                    None
                }
            }
        }
        Some(_) => {
            out.say("view: expected an object");
            None
        }
    };

    let mut actions: Vec<Action> = Vec::new();
    match field("actions").map(|v| v.as_array()) {
        None => {}
        Some(None) => out.say("actions: expected a list"),
        Some(Some(list)) => {
            if list.len() > ACTIONS_MAX {
                out.say(format!("actions: at most {ACTIONS_MAX}"));
            }
            for (i, v) in list.iter().enumerate() {
                let at = format!("actions[{i}]");
                let Some(a) = v.as_object() else {
                    out.say(format!("{at}: expected an object"));
                    continue;
                };
                closed(a, &["id", "label"], &format!("{at}."), &mut out);
                let id = match a.get("id").map(|v| v.as_str()) {
                    Some(Some(id)) if is_action_id(id) => Some(id.to_string()),
                    Some(_) => {
                        out.say(format!("{at}.id: expected 1 to 16 characters of a-z and -"));
                        None
                    }
                    None => {
                        out.say(format!("{at}.id is missing"));
                        None
                    }
                };
                let l = match a.get("label") {
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
    match field("hooks").map(|v| v.as_array()) {
        None => {}
        Some(None) => out.say("hooks: expected a list"),
        Some(Some(list)) => {
            if list.len() > 1 {
                out.say("hooks: at most one, at after:work");
            }
            if !caps.contains(&Cap::HookAfterWork) {
                out.say("hooks need the ask hook:after:work");
            }
            for (i, v) in list.iter().enumerate() {
                let at = format!("hooks[{i}]");
                let Some(h) = v.as_object() else {
                    out.say(format!("{at}: expected an object"));
                    continue;
                };
                closed(h, &["at", "timeout"], &format!("{at}."), &mut out);
                match h.get("at").map(|v| v.as_str()) {
                    Some(Some("after:work")) => {}
                    Some(Some(point)) => out.say(format!(
                        "{at}.at: the hook point {point} is reserved for plugins v2"
                    )),
                    Some(None) => out.say(format!("{at}.at: expected after:work")),
                    None => out.say(format!("{at}.at is missing")),
                }
                match h.get("timeout") {
                    Some(t) => hook = seconds(t, &format!("{at}.timeout"), HOOK_TIMEOUT, &mut out),
                    None => out.say(format!("{at}.timeout is missing")),
                }
            }
        }
    }

    let skills = match field("skills") {
        None => Vec::new(),
        Some(v) => skills(v, &caps, &mut out),
    };
    let tools = match field("tools") {
        None => Vec::new(),
        Some(v) => {
            if !built {
                out.say("tools need a build: a plugin with no build never runs");
            }
            tools(v, &caps, &mut out)
        }
    };
    let needs = match field("needs") {
        None => Vec::new(),
        Some(v) => needs(v, &caps, name, &mut out),
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
fn skills(v: &Value, caps: &[Cap], out: &mut Refusals) -> Vec<Skill> {
    let Some(list) = v.as_array() else {
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
        let Some(sk) = v.as_object() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(
            sk,
            &["id", "about", "path", "chats"],
            &format!("{at}."),
            out,
        );
        let mut need = |key: &str| {
            let v = sk.get(key);
            if v.is_none() {
                out.say(format!("{at}.{key} is missing"));
            }
            v
        };
        let (id, about, path, chats) = (need("id"), need("about"), need("path"), need("chats"));
        let id = id.and_then(|v| match v.as_str() {
            Some(id) if is_action_id(id) => Some(id.to_string()),
            _ => {
                out.say(format!("{at}.id: expected 1 to 16 characters of a-z and -"));
                None
            }
        });
        let about = about.and_then(|v| line(v, &format!("{at}.about"), ABOUT_MAX, out));
        let path = path.and_then(|v| match v.as_str() {
            Some(p) if is_markdown(p) => Some(p.to_string()),
            _ => {
                out.say(format!(
                    "{at}.path: expected a .md file in the repository, of letters, digits and ._/-, with no .."
                ));
                None
            }
        });
        let chats = chats.and_then(|v| chats_of(v, &at, out));
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
fn chats_of(v: &Value, at: &str, out: &mut Refusals) -> Option<Vec<String>> {
    let names: Option<Vec<String>> = v.as_array().and_then(|l| {
        l.iter()
            .map(|c| c.as_str().filter(|c| CHATS.contains(c)).map(String::from))
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
fn tools(v: &Value, caps: &[Cap], out: &mut Refusals) -> Vec<Tool> {
    let Some(list) = v.as_array() else {
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
        let Some(t) = v.as_object() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(
            t,
            &["id", "about", "params", "timeout", "chats"],
            &format!("{at}."),
            out,
        );
        let mut need = |key: &str| {
            let v = t.get(key);
            if v.is_none() {
                out.say(format!("{at}.{key} is missing"));
            }
            v
        };
        let (id, about, schema, timeout, chats) = (
            need("id"),
            need("about"),
            need("params"),
            need("timeout"),
            need("chats"),
        );
        let id = id.and_then(|v| match v.as_str() {
            Some(id) if is_action_id(id) => Some(id.to_string()),
            _ => {
                out.say(format!("{at}.id: expected 1 to 16 characters of a-z and -"));
                None
            }
        });
        let about = about.and_then(|v| line(v, &format!("{at}.about"), ABOUT_MAX, out));
        let schema = schema.and_then(|v| {
            let why = params::rules(v);
            for w in &why {
                out.say(format!("{at}.params: {w}"));
            }
            why.is_empty().then(|| v.clone())
        });
        let timeout = timeout.and_then(|v| seconds(v, &format!("{at}.timeout"), TOOL_TIMEOUT, out));
        let chats = chats.and_then(|v| chats_of(v, &at, out));
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

/// `needs`: at most [`NEEDS_MAX`] other plugins, each asked as
/// `needs:<name>`, each with the skills this plugin wants from it.
fn needs(v: &Value, caps: &[Cap], own: &str, out: &mut Refusals) -> Vec<Need> {
    let Some(list) = v.as_array() else {
        out.say("needs: expected a list");
        return Vec::new();
    };
    if list.len() > NEEDS_MAX {
        out.say(format!("needs: at most {NEEDS_MAX}"));
    }
    let mut got: Vec<Need> = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let at = format!("needs[{i}]");
        let Some(n) = v.as_object() else {
            out.say(format!("{at}: expected an object"));
            continue;
        };
        closed(n, &["name", "skills"], &format!("{at}."), out);
        let name = match n.get("name").map(|v| v.as_str()) {
            None => {
                out.say(format!("{at}.name is missing"));
                None
            }
            Some(Some(s)) if is_name(s) => Some(s.to_string()),
            Some(_) => {
                out.say(format!(
                    "{at}.name: expected a plugin's name, 1 to 12 characters of a-z and 0-9, a letter first"
                ));
                None
            }
        };
        let skills: Option<Vec<String>> = match n.get("skills") {
            None => {
                out.say(format!("{at}.skills is missing"));
                None
            }
            Some(v) => {
                let ids: Option<Vec<String>> = v.as_array().and_then(|l| {
                    l.iter()
                        .map(|s| s.as_str().filter(|s| is_action_id(s)).map(String::from))
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
    use serde_json::json;

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
                    "hook:after:work"
                ]
                .map(String::from)
                .to_vec(),
                timeout: Some(3600),
                kit: true,
                view: Some("Floorplan".into()),
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
    fn a_field_it_does_not_know_is_refused() {
        let v = with(|m| {
            m.insert("secrets".into(), json!(["TOKEN"]));
        });
        assert_eq!(refusals(&v), ["secrets: no field of manifest version 1"]);
        let v = with(|m| {
            m.insert("views".into(), json!([]));
        });
        assert_eq!(
            refusals(&v),
            ["views: more than one view is reserved for plugins v2"]
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
        for v in [json!(2), json!("1"), json!(1.5), json!(0)] {
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
                "hooks need the ask hook:after:work",
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
                    m.insert("ui".into(), json!({}));
                }),
                "ui.kit is missing",
            ),
            (
                with(|m| {
                    m.insert("view".into(), json!([{"label": "A"}, {"label": "B"}]));
                }),
                "view: more than one view is reserved for plugins v2",
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
                    m["hooks"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"at": "after:work", "timeout": 60}));
                }),
                "hooks: at most one, at after:work",
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
            "https://github.com/tehas-app/tehas/api/plugin-manifest.schema.json"
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
                "hook:after:work",
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
            m["asks"] = json!(["product", "model", "action:refresh", "hook:after:work"]);
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
                .push(json!({"at": "after:work", "timeout": 60}));
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
