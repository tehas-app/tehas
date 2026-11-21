//! One run of a plugin as data: what it is given, which
//! [`crate::sandbox::plugin_run`] turns into its container's arguments and
//! [`context`] into `/run/context.json`
//! (docs/specs/2026-11-21-plugins-security.md, "The container"). A
//! capability is given when the manifest asks for it and the grants for the
//! pin's commit grant it, and not otherwise.

use super::manifest::Manifest;
use super::{compare, is_name, Cap, Grants, Limits};
use crate::grammar::{commit_id, is_image};
use serde_json::{json, Value};

/// The container's own uid and gid, which own the image's /work, /out and
/// /state.
pub const UID: u32 = 10001;

/// The kind of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Refresh,
    Hook,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Refresh => "refresh",
            Kind::Hook => "hook",
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
}

impl Trigger {
    pub fn word(self) -> &'static str {
        match self {
            Trigger::Timer => "timer",
            Trigger::Host => "host",
            Trigger::Action => "action",
            Trigger::Cycle => "cycle",
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
            (Some(_), Kind::Refresh) => return Err("a refresh with a pull request".to_string()),
            (Some(pr), _) if !commit_id(&pr.base) || !commit_id(&pr.head) => {
                return Err("a pull request without whole commits".to_string())
            }
            _ => {}
        }
        if self.kind == Kind::Hook && !has(Cap::HookAfterWork) {
            return Err("a hook without hook:after:work".to_string());
        }
        let tokens = if has(Cap::Model) {
            Some(
                self.grants
                    .tokens
                    .ok_or("model granted without its tokens")?,
            )
        } else {
            None
        };
        Ok(Given {
            product: has(Cap::Product),
            tokens,
        })
    }

    /// The container's name, which `docker kill` and `docker rm` use too.
    pub fn container(&self) -> String {
        format!("tehas-plugin-{}-{}", self.manifest.name, self.id)
    }

    /// Where the run's files are on the host: `cid`, `context.json`,
    /// `product` and `model`, the model socket's directory.
    pub fn path(&self, file: &str) -> String {
        format!("{}/{file}", self.dir)
    }
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
    Ok(c)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::plugin::grants;

    pub const C: &str = "0123456789abcdef0123456789abcdef01234567";
    pub const H: &str = "89abcdef0123456789abcdef0123456789abcdef";
    pub const B: &str = "fedcba9876543210fedcba9876543210fedcba98";
    pub const IMAGE: &str =
        "sha256:1111111111111111111111111111111111111111111111111111111111111111";

    /// A manifest that asks for `asks`, as a plugin named demo.
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
            actions: vec![],
            hook: asks.contains(&"hook:after:work").then_some(300),
            skills: vec![],
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
        }
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
                "model": {"id": "plugin", "tokens": 2000000},
                "pr": {"number": 42, "base": B, "head": H},
            })
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
        for name in ["refresh", "hook"] {
            assert!(
                fits(&api(&format!("plugin-context.{name}.json")), &s),
                "{name}"
            );
        }
        let sets: [(&[&str], &str); 4] = [
            (&[], ""),
            (&["product"], "product"),
            (&["model"], "model:5"),
            (
                &["product", "model", "hook:after:work"],
                "product model:5 hook:after:work",
            ),
        ];
        for (asks, words) in sets {
            let m = manifest(asks);
            let g = granted(words);
            for kind in [Kind::Refresh, Kind::Hook] {
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
    }
}
