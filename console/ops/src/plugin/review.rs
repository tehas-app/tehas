//! `tehas plugin review <name> [<commit>] [--json]`: a commit of a plugin
//! fetched and read for a person to grant (docs/specs/2026-11-21-plugins.md,
//! "Commands"; the security part, "Granting"). It writes nothing but the
//! fetched objects and the base images it pulls to see their ONBUILD
//! triggers. api/plugin-review.schema.json is its JSON.

use super::exec::Exec;
use super::prepare::{self, Reading};
use serde_json::{json, Value};
use std::path::Path;
use tehas_core::models::ModelFile;
use tehas_core::plugin::review::{
    base_changes, changes, does, grants_line, is_here, url_host, warnings,
};
use tehas_core::plugin::{ask, Grants, Pin};

/// Where the model of the factory runs, for a plugin that asks for it.
#[derive(Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub provider: String,
    pub host: Option<String>,
    pub here: bool,
}

/// What a review found. A commit whose manifest could not be read at all is
/// no review but a failure, exit 1.
#[derive(Debug)]
pub struct Review {
    pub pin: Pin,
    pub commit: String,
    pub reading: Reading,
    /// Each base image and its ONBUILD triggers.
    pub bases: Vec<(String, Vec<String>)>,
    pub grants: Option<Grants>,
    /// What changed since the pin; None when the commit is the pin, or the
    /// pin's manifest could not be read.
    pub changes: Option<Vec<String>>,
    pub model: Vec<Endpoint>,
}

/// The things a review needs of the host, so a test gives its own.
pub struct Given<'a> {
    pub home: &'a Path,
    pub get: &'a dyn Fn(&str) -> String,
    pub models: &'a ModelFile,
    /// The host's own names.
    pub names: &'a [String],
}

impl Review {
    /// Each rule the commit breaks: its manifest, its skills, its
    /// Dockerfile and its base images.
    pub fn refusals(&self) -> Vec<String> {
        let mut out = self.reading.refusals.clone();
        out.extend(prepare::onbuild_refusals(&self.bases));
        out
    }

    fn asks(&self) -> &[String] {
        self.reading
            .manifest
            .as_ref()
            .map_or(&[], |m| m.asks.as_slice())
    }

    /// Whether the line grants `word` at this commit.
    fn granted(&self, word: &str) -> bool {
        let Ok(cap) = ask(word) else { return false };
        self.grants
            .as_ref()
            .is_some_and(|g| g.commit == self.commit && g.caps.contains(&cap))
    }

    /// Grants of the line that this commit does not ask for, which give
    /// nothing.
    fn unasked(&self) -> Vec<String> {
        let Some(g) = self.grants.as_ref().filter(|g| g.commit == self.commit) else {
            return Vec::new();
        };
        let asked: Vec<_> = self.asks().iter().filter_map(|a| ask(a).ok()).collect();
        g.caps
            .iter()
            .zip(&g.words)
            .filter(|(c, _)| !asked.contains(c))
            .map(|(_, w)| w.clone())
            .collect()
    }

    fn lines(&self) -> Vec<String> {
        let mut out = vec![grants_line(
            &self.pin.name,
            &self.commit,
            self.asks(),
            self.grants.as_ref(),
        )];
        if self.commit != self.pin.commit {
            out.insert(
                0,
                format!(
                    "TEHAS_PLUGINS: {}={}@{}",
                    self.pin.name, self.pin.url, self.commit
                ),
            );
        }
        out
    }

    pub fn json(&self) -> Value {
        let m = self.reading.manifest.as_ref();
        let asks: Vec<Value> = self
            .asks()
            .iter()
            .map(|a| {
                json!({
                    "ask": a,
                    "does": ask(a).map(|c| does(&c)).unwrap_or(""),
                    "granted": self.granted(a),
                })
            })
            .collect();
        let bases: Vec<Value> = self
            .bases
            .iter()
            .map(|(b, t)| json!({"image": b, "onbuild": t}))
            .collect();
        let model: Vec<Value> = self
            .model
            .iter()
            .map(|e| json!({"provider": e.provider, "host": e.host, "here": e.here}))
            .collect();
        let grants_commit = self.grants.as_ref().map(|g| g.commit.clone());
        let lines = self.lines();
        json!({
            "api": 1,
            "name": self.pin.name,
            "url": self.pin.url,
            "commit": self.commit,
            "pin": self.pin.commit,
            "manifest_sha256": self.reading.sha256,
            "label": m.map(|m| m.label.clone()),
            "license": m.and_then(|m| m.license.clone()),
            "build": m.and_then(|m| m.build.clone()),
            "asks": asks,
            "unasked": self.unasked(),
            "grants_commit": grants_commit,
            "bases": bases,
            "model": model,
            "warnings": warnings(self.asks()),
            "changes": self.changes,
            "refusals": self.refusals(),
            "settings": lines,
        })
    }

    pub fn text(&self) -> Vec<String> {
        let m = self.reading.manifest.as_ref();
        let mut out = vec![format!(
            "{} {}@{}{}",
            self.pin.name,
            self.pin.url,
            self.commit,
            if self.commit == self.pin.commit {
                " (the pin)"
            } else {
                " (not the pin)"
            }
        )];
        if let Some(m) = m {
            out.push(format!(
                "label: {}; license: {}",
                m.label,
                m.license.as_deref().unwrap_or("not given")
            ));
        }
        let refusals = self.refusals();
        if !refusals.is_empty() {
            out.push("refused:".to_string());
            out.extend(refusals.iter().map(|r| format!("  {r}")));
        }
        if let Some(g) = self.grants.as_ref().filter(|g| g.commit != self.commit) {
            out.push(format!(
                "the grants line is for {}, so it grants this commit nothing",
                g.commit
            ));
        }
        out.push("asks:".to_string());
        if self.asks().is_empty() {
            out.push("  nothing".to_string());
        }
        for a in self.asks() {
            let what = ask(a).map(|c| does(&c)).unwrap_or("");
            let state = if self.granted(a) {
                "granted"
            } else {
                "not granted"
            };
            out.push(format!("  {a} ({state}): {what}"));
        }
        for u in self.unasked() {
            out.push(format!(
                "  {u} is granted and not asked for: it gives nothing"
            ));
        }
        match m.and_then(|m| m.build.as_deref()) {
            None => out.push("build: none, so no image and no runs".to_string()),
            Some(file) => {
                out.push(format!("build: {file}, base images:"));
                for (b, t) in &self.bases {
                    if t.is_empty() {
                        out.push(format!("  {b}"));
                    } else {
                        out.push(format!("  {b}, ONBUILD: {}", t.join("; ")));
                    }
                }
            }
        }
        for e in &self.model {
            let host = e.host.as_deref().unwrap_or("no address in models.json");
            let place = if e.here {
                "on this host"
            } else {
                "on another host"
            };
            out.push(format!("model: provider {} at {host}, {place}", e.provider));
        }
        for w in warnings(self.asks()) {
            out.push(format!("warning: {w}"));
        }
        match &self.changes {
            Some(c) if c.is_empty() => out.push("since the pin: nothing changed".to_string()),
            Some(c) => {
                out.push("since the pin:".to_string());
                out.extend(c.iter().map(|l| format!("  {l}")));
            }
            None if self.commit != self.pin.commit => {
                out.push("since the pin: the pin's manifest could not be read".to_string());
            }
            None => {}
        }
        out.push("to grant it, write:".to_string());
        out.extend(self.lines().iter().map(|l| format!("  {l}")));
        out
    }
}

/// The endpoints of the models the factory uses, for a plugin that asks
/// for `model`.
fn endpoints(g: &Given) -> Vec<Endpoint> {
    let mut providers: Vec<String> = Vec::new();
    for (p, _) in tehas_core::models::used_from(g.get) {
        if !providers.contains(&p) {
            providers.push(p);
        }
    }
    providers
        .into_iter()
        .map(|provider| {
            let host = url_host(&g.models.url(&provider, "main"));
            let here = host.as_deref().is_some_and(|h| is_here(h, g.names));
            Endpoint {
                provider,
                host,
                here,
            }
        })
        .collect()
}

/// Fetch and read `commit` of `pin`, and the pin's own commit to say what
/// changed. Err: the fetch or the read failed, with why.
pub fn review(x: &mut dyn Exec, g: &Given, pin: &Pin, commit: &str) -> Result<Review, String> {
    prepare::fetched(x, g.home, pin, commit)?;
    let reading = prepare::read(x, g.home, &pin.name, commit)?;
    let bases = if reading.refusals.is_empty() {
        prepare::bases(x, &reading.bases)?
    } else {
        reading
            .bases
            .iter()
            .map(|b| (b.clone(), Vec::new()))
            .collect()
    };
    let changes = (commit != pin.commit)
        .then(|| {
            prepare::fetched(x, g.home, pin, &pin.commit).ok()?;
            let old = prepare::read(x, g.home, &pin.name, &pin.commit).ok()?;
            let mut c = changes(&old.raw, &reading.raw);
            c.extend(base_changes(&old.bases, &reading.bases));
            Some(c)
        })
        .flatten();
    let grants =
        tehas_core::plugin::grants(&(g.get)(&tehas_core::plugin::grants_setting(&pin.name))).ok();
    let model = if reading
        .manifest
        .as_ref()
        .is_some_and(|m| m.asks.iter().any(|a| a == "model"))
    {
        endpoints(g)
    } else {
        Vec::new()
    };
    Ok(Review {
        pin: pin.clone(),
        commit: commit.to_string(),
        reading,
        bases,
        grants,
        changes,
        model,
    })
}

#[cfg(test)]
mod tests {
    use super::super::exec::fake::{fails, ok};
    use super::super::prepare::tests::{dockerfile, good, manifest, pin, C, NODE};
    use super::*;
    use std::collections::BTreeMap;

    const D: &str = "89abcdef0123456789abcdef0123456789abcdef";

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> String {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| map.get(name).cloned().unwrap_or_default()
    }

    fn models() -> ModelFile {
        ModelFile {
            json: json!({"providers": {"local": {"baseUrl": "http://127.0.0.1:8080/v1"}, "far": {"baseUrl": "https://llm.example/v1"}}}),
        }
    }

    fn sample(name: &str) -> Value {
        let file = format!(
            "{}/../../api/plugin-review.{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap()
    }

    #[test]
    fn a_review_of_the_pin_says_each_ask_and_the_line_to_write() {
        let m = manifest(r#""view", "product", "model", "action:refresh""#).replace(
            r#""timeout": 3600"#,
            r#""timeout": 3600, "license": "Apache-2.0", "view": {"label": "Floorplan"}, "actions": [{"id": "refresh", "label": "Refresh"}]"#,
        );
        let d = dockerfile();
        let (home, mut x) = good(
            "review-pin",
            &[("tehas-plugin.json", &m), ("Dockerfile", &d)],
        );
        let get = vars(&[
            (
                "TEHAS_PLUGIN_GRANTS_FLOORPLAN",
                &format!("{C} product model:2000000 hook:after:work"),
            ),
            ("PI_ENSEMBLE_SUBAGENT_PROVIDER", "local"),
            ("PI_ENSEMBLE_SUBAGENT_MODEL", "qwen"),
        ]);
        let models = models();
        let names = vec!["tehas-tehas".to_string()];
        let g = Given {
            home: &home,
            get: &get,
            models: &models,
            names: &names,
        };
        let r = review(&mut x, &g, &pin(C), C).unwrap();
        assert!(r.refusals().is_empty(), "{:?}", r.refusals());
        let mut j = r.json();
        j["manifest_sha256"] = json!("0".repeat(64));
        assert_eq!(j, sample("sample"));
        let text = r.text();
        assert!(text.contains(&"  view (not granted): its /out is served behind the sign-in, and a panel in the dock: everyone the sign-in lets in sees it".to_string()), "{text:#?}");
        assert!(text.contains(
            &"  hook:after:work is granted and not asked for: it gives nothing".to_string()
        ));
        assert!(text.contains(&"model: provider local at 127.0.0.1, on this host".to_string()));
        assert!(text.contains(&format!("  {NODE}")));
        assert!(text.contains(&format!(
            "  TEHAS_PLUGIN_GRANTS_FLOORPLAN=\"{C} view product model:2000000 action:refresh\""
        )));
        assert!(text
            .iter()
            .any(|l| l.starts_with("warning: view and product together")));
        assert!(!x.ran_any("docker build"));
    }

    #[test]
    fn a_review_of_another_commit_says_what_changed_since_the_pin() {
        let (home, mut x) = good("review-other", &[]);
        let old = manifest(r#""product""#);
        let new = manifest(r#""product", "model""#);
        let d = dockerfile();
        for (c, m) in [(C, &old), (D, &new)] {
            for (path, text) in [
                ("tehas-plugin.json", m.as_str()),
                ("Dockerfile", d.as_str()),
            ] {
                x.on(&format!("cat-file -t {c}:{path}"), ok("blob\n"));
                x.on(
                    &format!("cat-file -s {c}:{path}"),
                    ok(&format!("{}\n", text.len())),
                );
                x.on(&format!("cat-file blob {c}:{path}"), ok(text));
            }
        }
        let get = vars(&[("TEHAS_PLUGIN_GRANTS_FLOORPLAN", &format!("{C} product"))]);
        let models = ModelFile { json: Value::Null };
        let g = Given {
            home: &home,
            get: &get,
            models: &models,
            names: &[],
        };
        let r = review(&mut x, &g, &pin(C), D).unwrap();
        assert_eq!(
            r.changes.as_deref(),
            Some(&["asks for model, which the pin does not".to_string()][..])
        );
        let text = r.text();
        assert!(text.contains(&format!(
            "the grants line is for {C}, so it grants this commit nothing"
        )));
        assert!(text.contains(&format!(
            "  TEHAS_PLUGINS: floorplan=https://github.com/tehas-app/tehas-plugin-floorplan@{D}"
        )));
        assert!(text.contains(&format!(
            "  TEHAS_PLUGIN_GRANTS_FLOORPLAN=\"{D} product model:<tokens>\""
        )));
        assert!(r.json()["asks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["granted"] == false));
    }

    #[test]
    fn a_refused_commit_names_why_and_an_onbuild_base_is_named() {
        let m = manifest(r#""product""#);
        let d = dockerfile();
        let (home, mut x) = good(
            "review-onbuild",
            &[("tehas-plugin.json", &m), ("Dockerfile", &d)],
        );
        x.on("{{json .Config.OnBuild}}", ok("[\"RUN make\"]"));
        let get = vars(&[]);
        let models = ModelFile { json: Value::Null };
        let g = Given {
            home: &home,
            get: &get,
            models: &models,
            names: &[],
        };
        let r = review(&mut x, &g, &pin(C), C).unwrap();
        assert_eq!(
            r.refusals(),
            [format!("the base image {NODE} has ONBUILD triggers, which would run in the build: RUN make")]
        );
        assert_eq!(r.json()["bases"][0]["onbuild"], json!(["RUN make"]));
        assert_eq!(
            sample("refused")["refusals"].as_array().map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn a_fetch_or_a_pull_that_fails_is_no_review() {
        let (home, mut x) = good("review-fail", &[]);
        x.on("cat-file -e", fails("missing"));
        x.on("fetch", fails("offline"));
        let get = vars(&[]);
        let models = ModelFile { json: Value::Null };
        let g = Given {
            home: &home,
            get: &get,
            models: &models,
            names: &[],
        };
        let why = review(&mut x, &g, &pin(C), C).unwrap_err();
        assert_eq!(why, format!("the fetch of {C} failed: offline"));
    }
}
