//! One run of a plugin as data: what it is given, which
//! [`crate::sandbox::plugin_run`] turns into its container's arguments and
//! [`context`] into `/run/context.json`
//! (docs/specs/2026-11-21-plugins-security.md, "The container"). A
//! capability is given when the manifest asks for it and the grants for the
//! pin's commit grant it, and not otherwise. And the kill of a run's
//! container past its time, by the id docker wrote in its cid file.

use super::manifest::Manifest;
use super::{compare, is_name, Cap, Grants, Limits};
use crate::grammar::{commit_id, is_image};
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Stdio};

/// The container's own uid and gid, which own the image's /work, /out and
/// /state.
pub const UID: u32 = 10001;

/// The kind of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Refresh,
    Hook,
    /// One call of a tool, attached, kept nowhere.
    Call,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Refresh => "refresh",
            Kind::Hook => "hook",
            Kind::Call => "call",
        }
    }
}

/// What started a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// The tick saw the default branch move.
    Timer,
    /// A person on the host: `tehas plugin run` or `tehas plugin hook`.
    Host,
    /// A press of the view's button.
    Action,
    /// A cycle left a pull request.
    Cycle,
    /// A chat called a tool.
    Chat,
    /// An outside agent called a tool through the MCP server.
    Mcp,
}

impl Trigger {
    pub fn word(self) -> &'static str {
        match self {
            Trigger::Timer => "timer",
            Trigger::Host => "host",
            Trigger::Action => "action",
            Trigger::Cycle => "cycle",
            Trigger::Chat => "chat",
            Trigger::Mcp => "mcp",
        }
    }
}

/// The pull request of a hook: its number and the commits of its base and
/// head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pr {
    pub number: u64,
    pub base: String,
    pub head: String,
}

/// One run, before it starts.
pub struct Run<'a> {
    pub manifest: &'a Manifest,
    pub grants: &'a Grants,
    /// The plugin's commit, the pin's.
    pub commit: &'a str,
    pub kind: Kind,
    pub trigger: Trigger,
    /// The run's row in the state store.
    pub id: u64,
    /// The run's directory on the host: the cid file, the context, the
    /// product export and the model socket's directory are in it.
    pub dir: &'a str,
    /// The image, by id.
    pub image: &'a str,
    pub limits: Limits,
    /// The commit of `/product`.
    pub head: &'a str,
    /// The model's id as the run's socket answers to it.
    pub model: &'a str,
    /// A hook's pull request.
    pub pr: Option<Pr>,
    /// A call's tool, by id.
    pub tool: Option<&'a str>,
}

/// What a run is given beyond what every run has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Given {
    pub product: bool,
    /// The tokens of a run, with `model`.
    pub tokens: Option<u64>,
}

/// A host path that goes into a `--mount` whole: absolute, of letters,
/// digits and `._-/`, no `..`, so no comma or `=` adds an option.
fn is_dir(s: &str) -> bool {
    s.starts_with('/')
        && !s.contains("..")
        && !s.ends_with('/')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
}

impl Run<'_> {
    /// What the run is given, or why it may not start.
    pub fn given(&self) -> Result<Given, String> {
        let name = &self.manifest.name;
        if !is_name(name) {
            return Err(format!("{name:?} is no plugin name"));
        }
        let c = compare(&self.manifest.asks, Some(self.grants), self.commit);
        if !c.granted() {
            return Err(c.refusals().join("; "));
        }
        if !is_image(self.image) {
            return Err(format!("{}: expected an image id", self.image));
        }
        if !is_dir(self.dir) {
            return Err(format!("{}: expected a plain absolute path", self.dir));
        }
        if !commit_id(self.commit) || !commit_id(self.head) {
            return Err("expected whole commits".to_string());
        }
        let has =
            |cap: Cap| self.grants.caps.contains(&cap) && self.manifest.asks.contains(&cap.ask());
        let triggers: &[Trigger] = match self.kind {
            Kind::Refresh => &[Trigger::Timer, Trigger::Host, Trigger::Action],
            Kind::Hook => &[Trigger::Cycle, Trigger::Host],
            Kind::Call => &[Trigger::Chat, Trigger::Mcp, Trigger::Host],
        };
        if !triggers.contains(&self.trigger) {
            return Err(format!(
                "a {} is not started by {}",
                self.kind.word(),
                self.trigger.word()
            ));
        }
        match (&self.pr, self.kind) {
            (None, Kind::Hook) => return Err("a hook without its pull request".to_string()),
            (Some(_), Kind::Refresh | Kind::Call) => {
                return Err(format!("a {} with a pull request", self.kind.word()))
            }
            (Some(pr), _) if !commit_id(&pr.base) || !commit_id(&pr.head) => {
                return Err("a pull request without whole commits".to_string())
            }
            _ => {}
        }
        if self.kind == Kind::Hook && !has(Cap::HookAfterWork) {
            return Err("a hook without hook:after:work".to_string());
        }
        match (self.tool, self.kind) {
            (None, Kind::Call) => return Err("a call without its tool".to_string()),
            (Some(_), Kind::Refresh | Kind::Hook) => {
                return Err(format!("a {} with a tool", self.kind.word()))
            }
            (Some(id), _) if !self.manifest.tools.iter().any(|t| t.id == id) => {
                return Err(format!("the manifest has no tool {id}"))
            }
            (Some(id), _) if !has(Cap::Tool(id.to_string())) => {
                return Err(format!("a call of {id} without tool:{id}"))
            }
            _ => {}
        }
        // A hook has no model in v1 (docs/specs/2026-11-21-plugins-hooks.md,
        // "The run"), whatever is granted.
        let tokens = if has(Cap::Model) && self.kind == Kind::Refresh {
            Some(
                self.grants
                    .tokens
                    .ok_or("model granted without its tokens")?,
            )
        } else {
            None
        };
        // A call has no product and no model, whatever is granted: it answers
        // a chat from its arguments and /state.
        Ok(Given {
            product: has(Cap::Product) && self.kind != Kind::Call,
            tokens,
        })
    }

    /// The container's name, which `docker kill` and `docker rm` use too.
    pub fn container(&self) -> String {
        format!("tehas-plugin-{}-{}", self.manifest.name, self.id)
    }

    /// Where the run's files are on the host: `cid`, `context.json`,
    /// `product`, `model`, the model socket's directory, and a call's
    /// `call.json`.
    pub fn path(&self, file: &str) -> String {
        format!("{}/{file}", self.dir)
    }
}

/// `docker kill` of the container whose id `cidfile` holds, when it holds
/// one. Nothing is said: a container already gone is what the kill wants.
pub fn kill_by_cidfile(cidfile: &Path) {
    let text = std::fs::read_to_string(cidfile).unwrap_or_default();
    if let Some(cid) = cid_of(&text) {
        let _ = Command::new("docker")
            .args(["kill", cid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// The id of a cid file: hex digits only, so it is never an option.
fn cid_of(text: &str) -> Option<&str> {
    let cid = text.trim();
    (!cid.is_empty() && cid.bytes().all(|b| b.is_ascii_hexdigit())).then_some(cid)
}

/// `/run/context.json` of a run, api/plugin-context.schema.json. Nothing of
/// a person, and nothing of a capability the run is not given.
pub fn context(run: &Run) -> Result<Value, String> {
    let given = run.given()?;
    let mut c = json!({
        "kind": run.kind.word(),
        "trigger": run.trigger.word(),
        "commit": run.commit,
    });
    if given.product {
        c["head"] = json!(run.head);
    }
    if let Some(tokens) = given.tokens {
        c["model"] = json!({"id": run.model, "tokens": tokens});
    }
    if let Some(pr) = &run.pr {
        c["pr"] = json!({"number": pr.number, "base": pr.base, "head": pr.head});
    }
    if let Some(tool) = run.tool {
        c["tool"] = json!(tool);
    }
    Ok(c)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::plugin::grants;
    use crate::plugin::manifest::Tool;

    pub const C: &str = "0123456789abcdef0123456789abcdef01234567";
    pub const H: &str = "89abcdef0123456789abcdef0123456789abcdef";
    pub const B: &str = "fedcba9876543210fedcba9876543210fedcba98";
    pub const IMAGE: &str =
        "sha256:1111111111111111111111111111111111111111111111111111111111111111";

    /// A manifest that asks for `asks`, as a plugin named demo, with a tool
    /// for each `tool:<id>`.
    pub fn manifest(asks: &[&str]) -> Manifest {
        Manifest {
            name: "demo".into(),
            label: "Demo".into(),
            license: None,
            build: Some("Dockerfile".into()),
            asks: asks.iter().map(|a| a.to_string()).collect(),
            timeout: Some(600),
            kit: false,
            view: None,
            views: vec![],
            actions: vec![],
            hook: asks.contains(&"hook:after:work").then_some(300),
            skills: vec![],
            tools: asks
                .iter()
                .filter_map(|a| a.strip_prefix("tool:"))
                .map(|id| Tool {
                    id: id.into(),
                    about: "Find a doc".into(),
                    params: json!({"type": "object"}),
                    timeout: 5,
                    chats: vec!["obeya".into()],
                })
                .collect(),
            needs: vec![],
        }
    }

    /// The grants line of `words` for the commit C.
    pub fn granted(words: &str) -> Grants {
        grants(format!("{C} {words}").trim_end()).unwrap()
    }

    pub fn run<'a>(m: &'a Manifest, g: &'a Grants, kind: Kind) -> Run<'a> {
        Run {
            manifest: m,
            grants: g,
            commit: C,
            kind,
            trigger: match kind {
                Kind::Refresh => Trigger::Timer,
                Kind::Hook => Trigger::Cycle,
                Kind::Call => Trigger::Chat,
            },
            id: 7,
            dir: "/home/tehas/tehas/plugins/demo/run-7",
            image: IMAGE,
            limits: Limits {
                memory: 2,
                cpus: 2,
                disk: 4,
            },
            head: H,
            model: "plugin",
            pr: (kind == Kind::Hook).then(|| Pr {
                number: 42,
                base: B.into(),
                head: H.into(),
            }),
            tool: (kind == Kind::Call).then_some("find-doc"),
        }
    }

    #[test]
    fn a_cid_file_is_killed_only_by_its_hex_id() {
        assert_eq!(cid_of("4f2a9c\n"), Some("4f2a9c"));
        for bad in ["", " \n", "--all", "4f2a 9c", "4f2a;rm"] {
            assert_eq!(cid_of(bad), None, "{bad:?}");
        }
        // No file: nothing to kill, and nothing is run.
        kill_by_cidfile(Path::new("/nonexistent/tehas/cid"));
    }

    #[test]
    fn a_capability_is_given_when_asked_and_granted() {
        let m = manifest(&["product", "model"]);
        let g = granted("product model:2000");
        assert_eq!(
            run(&m, &g, Kind::Refresh).given(),
            Ok(Given {
                product: true,
                tokens: Some(2000)
            })
        );
        // A grant not asked for gives nothing.
        let m = manifest(&[]);
        assert_eq!(
            run(&m, &g, Kind::Refresh).given(),
            Ok(Given {
                product: false,
                tokens: None
            })
        );
    }

    #[test]
    fn a_run_that_is_not_granted_does_not_start() {
        let m = manifest(&["product"]);
        let g = granted("");
        assert_eq!(
            run(&m, &g, Kind::Refresh).given().unwrap_err(),
            "product is asked and not granted"
        );
        let g = grants(&format!("{H} product")).unwrap();
        assert_eq!(
            run(&m, &g, Kind::Refresh).given().unwrap_err(),
            "the grants are for another commit; product is asked and not granted"
        );
    }

    #[test]
    fn a_hook_needs_its_grant_and_its_pull_request() {
        let m = manifest(&[]);
        let g = granted("");
        assert_eq!(
            run(&m, &g, Kind::Hook).given().unwrap_err(),
            "a hook without hook:after:work"
        );
        let m = manifest(&["hook:after:work"]);
        let g = granted("hook:after:work");
        let mut r = run(&m, &g, Kind::Hook);
        assert!(r.given().is_ok());
        r.pr = None;
        assert_eq!(r.given().unwrap_err(), "a hook without its pull request");
        let mut r = run(&m, &g, Kind::Hook);
        r.trigger = Trigger::Timer;
        assert_eq!(r.given().unwrap_err(), "a hook is not started by timer");
        let mut r = run(&m, &g, Kind::Refresh);
        r.pr = run(&m, &g, Kind::Hook).pr;
        assert_eq!(r.given().unwrap_err(), "a refresh with a pull request");
    }

    #[test]
    fn a_path_or_an_image_that_could_add_an_option_is_refused() {
        let m = manifest(&[]);
        let g = granted("");
        for dir in ["relative", "/a,dst=/etc", "/a/../b", "/a/", "/a b", "/a=b"] {
            let mut r = run(&m, &g, Kind::Refresh);
            r.dir = dir;
            assert!(r.given().is_err(), "{dir}");
        }
        let mut r = run(&m, &g, Kind::Refresh);
        r.image = "node:22";
        assert!(r.given().is_err());
    }

    #[test]
    fn the_context_names_only_what_the_run_is_given() {
        let m = manifest(&[]);
        let g = granted("");
        assert_eq!(
            context(&run(&m, &g, Kind::Refresh)).unwrap(),
            json!({"kind": "refresh", "trigger": "timer", "commit": C})
        );
        let m = manifest(&[
            "view",
            "product",
            "model",
            "action:refresh",
            "hook:after:work",
        ]);
        let g = granted("view product model:2000000 action:refresh hook:after:work");
        assert_eq!(
            context(&run(&m, &g, Kind::Hook)).unwrap(),
            json!({
                "kind": "hook",
                "trigger": "cycle",
                "commit": C,
                "head": H,
                "pr": {"number": 42, "base": B, "head": H},
            })
        );
        assert_eq!(
            context(&run(&m, &g, Kind::Refresh)).unwrap()["model"],
            json!({"id": "plugin", "tokens": 2000000})
        );
    }

    #[test]
    fn a_hook_has_no_model_whatever_is_granted() {
        let m = manifest(&["model", "hook:after:work"]);
        let g = granted("model:2000 hook:after:work");
        assert_eq!(run(&m, &g, Kind::Hook).given().unwrap().tokens, None);
        assert_eq!(
            run(&m, &g, Kind::Refresh).given().unwrap().tokens,
            Some(2000)
        );
        let w = crate::sandbox::plugin_run(&run(&m, &g, Kind::Hook)).unwrap();
        assert!(!w.iter().any(|a| a.contains("dst=/run/tehas")), "{w:?}");
    }

    #[test]
    fn a_call_needs_its_tool_asked_granted_and_in_the_manifest() {
        let m = manifest(&["tool:find-doc"]);
        let g = granted("tool:find-doc");
        assert_eq!(
            run(&m, &g, Kind::Call).given(),
            Ok(Given {
                product: false,
                tokens: None
            })
        );
        let mut r = run(&m, &g, Kind::Call);
        r.tool = None;
        assert_eq!(r.given().unwrap_err(), "a call without its tool");
        let mut r = run(&m, &g, Kind::Call);
        r.tool = Some("grep");
        assert_eq!(r.given().unwrap_err(), "the manifest has no tool grep");
        let mut r = run(&m, &g, Kind::Refresh);
        r.tool = Some("find-doc");
        assert_eq!(r.given().unwrap_err(), "a refresh with a tool");
        let mut r = run(&m, &g, Kind::Call);
        r.pr = run(&m, &g, Kind::Hook).pr;
        assert_eq!(r.given().unwrap_err(), "a call with a pull request");
        // Asked and in the manifest, and granted only at another commit.
        let other = grants(&format!("{H} tool:find-doc")).unwrap();
        assert!(run(&m, &other, Kind::Call).given().is_err());
        // In the manifest and granted, and not asked.
        let mut unasked = manifest(&["tool:find-doc"]);
        unasked.asks.clear();
        assert_eq!(
            run(&unasked, &g, Kind::Call).given().unwrap_err(),
            "a call of find-doc without tool:find-doc"
        );
    }

    #[test]
    fn a_call_is_started_by_a_chat_the_mcp_server_or_the_host() {
        let m = manifest(&["tool:find-doc"]);
        let g = granted("tool:find-doc");
        for t in [Trigger::Chat, Trigger::Mcp, Trigger::Host] {
            let mut r = run(&m, &g, Kind::Call);
            r.trigger = t;
            assert!(r.given().is_ok(), "{t:?}");
        }
        for t in [Trigger::Timer, Trigger::Action, Trigger::Cycle] {
            let mut r = run(&m, &g, Kind::Call);
            r.trigger = t;
            assert_eq!(
                r.given().unwrap_err(),
                format!("a call is not started by {}", t.word())
            );
        }
        for kind in [Kind::Refresh, Kind::Hook] {
            for t in [Trigger::Chat, Trigger::Mcp] {
                let m = manifest(&["hook:after:work"]);
                let g = granted("hook:after:work");
                let mut r = run(&m, &g, kind);
                r.trigger = t;
                assert!(r.given().is_err(), "{kind:?} {t:?}");
            }
        }
    }

    #[test]
    fn a_call_has_no_product_and_no_model_whatever_is_granted() {
        let m = manifest(&["product", "model", "tool:find-doc"]);
        let g = granted("product model:2000 tool:find-doc");
        assert_eq!(
            run(&m, &g, Kind::Call).given(),
            Ok(Given {
                product: false,
                tokens: None
            })
        );
        assert_eq!(
            context(&run(&m, &g, Kind::Call)).unwrap(),
            json!({"kind": "call", "trigger": "chat", "commit": C, "tool": "find-doc"})
        );
    }

    #[test]
    fn every_context_fits_its_schema() {
        use crate::plugin::schema::{api, closed, fits};
        let s = api("plugin-context.schema.json");
        assert_eq!(
            s["$id"],
            "https://github.com/tehas-app/tehas/api/plugin-context.schema.json"
        );
        closed(&s);
        for name in ["refresh", "hook", "call"] {
            assert!(
                fits(&api(&format!("plugin-context.{name}.json")), &s),
                "{name}"
            );
        }
        let sets: [(&[&str], &str); 5] = [
            (&[], ""),
            (&["product"], "product"),
            (&["model"], "model:5"),
            (
                &["product", "model", "hook:after:work"],
                "product model:5 hook:after:work",
            ),
            (
                &["product", "model", "tool:find-doc"],
                "product model:5 tool:find-doc",
            ),
        ];
        for (asks, words) in sets {
            let m = manifest(asks);
            let g = granted(words);
            for kind in [Kind::Refresh, Kind::Hook, Kind::Call] {
                if let Ok(c) = context(&run(&m, &g, kind)) {
                    assert!(fits(&c, &s), "{c}");
                }
            }
        }
        let mut person = api("plugin-context.refresh.json");
        person["login"] = json!("someone");
        assert!(!fits(&person, &s));
        let mut refresh = api("plugin-context.hook.json");
        refresh["kind"] = json!("refresh");
        assert!(!fits(&refresh, &s), "only a hook names a pull request");
        let mut model = api("plugin-context.hook.json");
        model["model"] = json!({"id": "plugin", "tokens": 5});
        assert!(!fits(&model, &s), "a hook has no model");
        let mut tool = api("plugin-context.refresh.json");
        tool["tool"] = json!("find-doc");
        assert!(!fits(&tool, &s), "only a call names a tool");
        let mut model = api("plugin-context.call.json");
        model["model"] = json!({"id": "plugin", "tokens": 5});
        assert!(!fits(&model, &s), "a call has no model");
    }
}
