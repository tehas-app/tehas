//! What the gate knows of each plugin, behind [`Catalog`] so the tests stub
//! it: whether its view is served, whether it asked for the kit, and the
//! actions granted. [`Installed`] reads it from the host: the settings' pins
//! and grants, `~/tehas/plugins/<name>/state.json` and the manifest at the
//! pinned commit in `repo.git`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::SystemTime;
use tehas_core::plugin::{self, Cap, Grants, Pin};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// `view` granted, and a good refresh's files there to serve.
    pub view: bool,
    /// `"ui": {"kit": 1}`.
    pub kit: bool,
    /// The ids of the actions asked for and granted.
    pub actions: Vec<String>,
    /// The folders of the plugin's views, each with whether it is granted:
    /// `/` for v1's `view`, each view's path of `views`.
    pub paths: Vec<(String, bool)>,
    /// Kept files to frame at `/_frame/`. `keep:` is not a capability yet,
    /// so no plugin has any.
    pub keep: bool,
}

pub trait Catalog: Send + Sync {
    /// The facts of the plugin `name` of the settings. None: no such plugin.
    fn facts(&self, name: &str) -> Option<Facts>;
}

/// The plugin's directory: `~/tehas/plugins/<name>`.
pub fn dir(home: &Path, name: &str) -> PathBuf {
    home.join("plugins").join(name)
}

/// The facts of a plugin from its manifest's asks, its kit and its actions,
/// against its grants at the pinned commit. Refused, it has none but its
/// name.
pub fn facts_of(
    asks: &[String],
    kit: bool,
    actions: &[String],
    grants: Option<&Grants>,
    pin: &str,
) -> Facts {
    if !plugin::compare(asks, grants, pin).granted() {
        return Facts::default();
    }
    let caps: &[Cap] = grants.map_or(&[], |g| &g.caps);
    Facts {
        view: caps.contains(&Cap::View),
        kit,
        actions: actions
            .iter()
            .filter(|id| caps.contains(&Cap::Action(id.to_string())))
            .cloned()
            .collect(),
        paths: Vec::new(),
        keep: false,
    }
}

/// Whether a path of the view, its segments, is served: the view with the
/// longest folder that holds it is granted. A shorter view's grant never
/// serves a path a longer view's folder holds.
pub fn covered(paths: &[(String, bool)], segs: &[String]) -> bool {
    let at = format!("/{}/", segs.join("/")).replace("//", "/");
    paths
        .iter()
        .filter(|(p, _)| at.starts_with(p.as_str()))
        .max_by_key(|(p, _)| p.len())
        .is_some_and(|(_, granted)| *granted)
}

/// Whether the text of `state.json` says the plugin is ready at `commit`.
/// It is read here until the state's reader of the build
/// (console/core/src/plugin/state.rs) can be used.
pub fn ready(text: &str, commit: &str) -> bool {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    v["api"] == 1 && v["state"] == "ready" && v["commit"] == commit
}

/// The host's catalog.
pub struct Installed {
    pub home: PathBuf,
    pub pins: Vec<Pin>,
    pub grants: HashMap<String, Grants>,
    // The facts of each plugin, with the state file's time they were read at.
    kept: Mutex<HashMap<String, (Option<SystemTime>, Facts)>>,
}

impl Installed {
    pub fn new(home: PathBuf, pins: Vec<Pin>, grants: HashMap<String, Grants>) -> Installed {
        Installed {
            home,
            pins,
            grants,
            kept: Mutex::new(HashMap::new()),
        }
    }

    fn manifest(&self, pin: &Pin) -> Option<plugin::manifest::Manifest> {
        let repo = dir(&self.home, &pin.name).join("repo.git");
        let out = Command::new("git")
            .arg("--git-dir")
            .arg(&repo)
            .args(["-c", "core.hooksPath=/dev/null", "show"])
            .arg(format!("{}:tehas-plugin.json", pin.commit))
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        plugin::manifest::read(&out.stdout, &pin.name).ok()
    }

    fn read(&self, pin: &Pin) -> Facts {
        let state = dir(&self.home, &pin.name).join("state.json");
        let ok = std::fs::read_to_string(&state).is_ok_and(|t| ready(&t, &pin.commit));
        let Some(m) = ok.then(|| self.manifest(pin)).flatten() else {
            return Facts::default();
        };
        let actions: Vec<String> = m.actions.iter().map(|a| a.id.clone()).collect();
        let mut f = facts_of(
            &m.asks,
            m.kit,
            &actions,
            self.grants.get(&pin.name),
            &pin.commit,
        );
        let grants = self.grants.get(&pin.name);
        if plugin::compare(&m.asks, grants, &pin.commit).granted() {
            let caps = grants.map_or(&[][..], |g| &g.caps);
            if m.view.is_some() {
                f.paths.push(("/".to_string(), f.view));
            }
            for v in &m.views {
                f.paths
                    .push((v.path.clone(), caps.contains(&Cap::ViewOf(v.id.clone()))));
            }
        }
        f.view = f.paths.iter().any(|(_, granted)| *granted);
        f
    }
}

impl Catalog for Installed {
    fn facts(&self, name: &str) -> Option<Facts> {
        let pin = self.pins.iter().find(|p| p.name == name)?;
        let state = dir(&self.home, name).join("state.json");
        let time = std::fs::metadata(&state).and_then(|m| m.modified()).ok();
        let mut kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((t, f)) = kept.get(name) {
            if *t == time && time.is_some() {
                return Some(f.clone());
            }
        }
        let f = self.read(pin);
        kept.insert(name.to_string(), (time, f.clone()));
        Some(f)
    }
}

/// The files of the kit's MANIFEST, `kit 1` and then `file <sha256> <size>
/// <path>` lines. None: no manifest of kit 1.
pub fn kit_files(manifest: &str) -> Option<Vec<String>> {
    let mut lines = manifest.lines();
    if lines.next()? != "kit 1" {
        return None;
    }
    let mut out = Vec::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let w: Vec<&str> = line.split(' ').collect();
        match w.as_slice() {
            ["file", _, _, path] => out.push(path.to_string()),
            _ => return None,
        }
    }
    Some(out)
}
