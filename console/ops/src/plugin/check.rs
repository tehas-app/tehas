//! `tehas plugin check <dir>`: the rules `review` applies to a fetched commit,
//! applied to a plugin's folder on this machine (plugin specification 1.0.0,
//! "Commands"): the manifest, its skills' text and the Dockerfile's rules,
//! each ask known and not reserved. It reads no settings, runs no git or
//! docker and reaches no network, so it runs on a laptop as well as a host.
//! What it cannot see without the network, the bases' ONBUILD triggers, it
//! names as left to `review` and `tehas update`.

use super::prepare::{self, Reading};
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path};
use tehas_core::plugin::build::MANIFEST;
use tehas_core::plugin::is_name;

/// What a check cannot see, said with every answer.
pub const LEFT: &str = "the bases' ONBUILD triggers: tehas plugin review and tehas update pull the bases and check them";

#[derive(Debug)]
pub struct Checked {
    pub dir: String,
    pub reading: Reading,
}

impl Checked {
    pub fn refusals(&self) -> &[String] {
        &self.reading.refusals
    }

    pub fn json(&self) -> Value {
        let m = self.reading.manifest.as_ref();
        json!({
            "api": 1,
            "dir": self.dir,
            "name": m.map(|m| m.name.clone()),
            "asks": m.map(|m| m.asks.clone()).unwrap_or_default(),
            "bases": self.reading.bases,
            "manifest_sha256": self.reading.sha256,
            "refusals": self.reading.refusals,
            "left": [LEFT],
        })
    }

    pub fn text(&self) -> Vec<String> {
        let mut out = Vec::new();
        match &self.reading.manifest {
            Some(m) => out.push(format!("{}: {} ({})", self.dir, m.name, m.label)),
            None => out.push(format!("{}: the manifest could not be read", self.dir)),
        }
        if let Some(m) = &self.reading.manifest {
            out.push(format!(
                "  asks: {}",
                if m.asks.is_empty() {
                    "nothing".to_string()
                } else {
                    m.asks.join(" ")
                }
            ));
        }
        for b in &self.reading.bases {
            out.push(format!("  base: {b}"));
        }
        if self.reading.refusals.is_empty() {
            out.push("ok: the manifest, the skills and the Dockerfile keep the rules".to_string());
        } else {
            out.push("refused:".to_string());
            out.extend(self.reading.refusals.iter().map(|r| format!("  {r}")));
        }
        out.push(format!("not checked here: {LEFT}"));
        out
    }
}

/// A file of the folder by its path in the repository: a regular file,
/// reached through no link, of at most `max` bytes.
fn file_of(dir: &Path, path: &str, max: u64) -> Result<Option<Vec<u8>>, String> {
    let rel = Path::new(path);
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("{path}: not a path in the repository"));
    }
    // Each folder on the way, and the file, must be what it says, not a link.
    let mut at = dir.to_path_buf();
    for part in rel.components() {
        at.push(part);
        match fs::symlink_metadata(&at) {
            Err(_) => return Ok(None),
            Ok(m) if m.file_type().is_symlink() => {
                return Err(format!("{path}: a link, which a commit's file is not"))
            }
            Ok(_) => {}
        }
    }
    let mut f = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&at)
        .map_err(|e| format!("{path}: {e}"))?;
    let meta = f.metadata().map_err(|e| format!("{path}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{path} is not a file"));
    }
    if meta.len() > max {
        return Err(format!("{path} is over {max} bytes"));
    }
    let mut bytes = Vec::new();
    f.by_ref()
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{path}: {e}"))?;
    if bytes.len() as u64 > max {
        return Err(format!("{path} is over {max} bytes"));
    }
    Ok(Some(bytes))
}

/// Check the plugin in `dir`. Err: the folder holds no manifest that can be
/// read at all, with why.
pub fn check(dir: &Path) -> Result<Checked, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }
    // The name the manifest gives itself: a folder has no TEHAS_PLUGINS line.
    let name = file_of(
        dir,
        MANIFEST,
        tehas_core::plugin::manifest::BYTES_MAX as u64,
    )?
    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    .and_then(|v| v.get("name").and_then(Value::as_str).map(String::from))
    .filter(|n| is_name(n))
    .unwrap_or_else(|| "plugin".to_string());
    let mut file = |path: &str, max: u64| file_of(dir, path, max);
    let shown = dir.display().to_string();
    let reading = prepare::reading(&mut file, &name, &shown, "the folder")?;
    Ok(Checked {
        dir: shown,
        reading,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tehas_core::testdir::TestDir;

    const NODE: &str = "FROM node:22-bookworm@sha256:e13726ca604184d7a91fcacab1fc09eefd642dc28e79ac34e85540dd93b78cfb";

    fn folder(name: &str, files: &[(&str, &str)]) -> TestDir {
        let d = TestDir::new(&format!("plugin-check-{name}"));
        for (path, text) in files {
            let p = d.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        d
    }

    fn manifest(asks: &str) -> String {
        format!(
            r#"{{"plugin": 1, "kind": "batch", "name": "demo", "label": "Demo", "build": "Dockerfile", "asks": [{asks}], "timeout": 600}}"#
        )
    }

    fn dockerfile() -> String {
        format!("{NODE}\nUSER 10001:10001\nENTRYPOINT [\"node\", \"/plugin/main.mjs\"]\n")
    }

    #[test]
    fn a_folder_that_keeps_the_rules_is_ok_and_says_what_it_did_not_check() {
        let d = folder(
            "ok",
            &[
                ("tehas-plugin.json", &manifest(r#""product""#)),
                ("Dockerfile", &dockerfile()),
            ],
        );
        let c = check(&d).unwrap();
        assert!(c.refusals().is_empty(), "{:?}", c.refusals());
        let j = c.json();
        assert_eq!(j["name"], "demo");
        assert_eq!(j["asks"], json!(["product"]));
        assert_eq!(j["bases"].as_array().unwrap().len(), 1);
        assert_eq!(j["left"], json!([LEFT]));
        let text = c.text();
        assert!(text.iter().any(|l| l.starts_with("ok:")), "{text:?}");
        assert!(text.last().unwrap().starts_with("not checked here:"));
    }

    #[test]
    fn each_rule_a_folder_breaks_is_said() {
        let d = folder(
            "refused",
            &[
                (
                    "tehas-plugin.json",
                    &manifest(r#""product", "gate:before:work""#),
                ),
                ("Dockerfile", "FROM node:22\nADD https://example.com/x /x\n"),
            ],
        );
        let r = check(&d).unwrap();
        let joined = r.refusals().join("\n");
        assert!(joined.contains("gate:before:work"), "{joined}");
        assert_eq!(r.text().iter().filter(|l| *l == "refused:").count(), 1);

        let d = folder(
            "dockerfile",
            &[
                ("tehas-plugin.json", &manifest(r#""product""#)),
                ("Dockerfile", "FROM node:22\n"),
            ],
        );
        let r = check(&d).unwrap();
        assert!(
            r.refusals().iter().all(|w| w.starts_with("Dockerfile: ")),
            "{:?}",
            r.refusals()
        );
        assert!(!r.refusals().is_empty());

        let d = folder(
            "no-docker",
            &[("tehas-plugin.json", &manifest(r#""product""#))],
        );
        assert_eq!(
            check(&d).unwrap().refusals(),
            ["Dockerfile: not in the folder"]
        );
    }

    #[test]
    fn a_folder_without_a_manifest_or_with_a_linked_file_is_refused() {
        let d = folder("empty", &[("README.md", "x")]);
        assert!(check(&d).unwrap_err().ends_with("has no tehas-plugin.json"));
        assert!(check(&d.join("README.md"))
            .unwrap_err()
            .ends_with("is not a folder"));

        let d = folder(
            "link",
            &[
                ("tehas-plugin.json", &manifest(r#""product""#)),
                ("real", &dockerfile()),
            ],
        );
        symlink(d.join("real"), d.join("Dockerfile")).unwrap();
        let r = check(&d).unwrap();
        assert!(
            r.refusals().iter().any(|w| w.contains("a link")),
            "{:?}",
            r.refusals()
        );
    }

    #[test]
    fn a_path_out_of_the_folder_is_never_read() {
        let d = folder("out", &[]);
        assert!(file_of(&d, "../etc/passwd", 10).is_err());
        assert!(file_of(&d, "/etc/passwd", 10).is_err());
        assert_eq!(file_of(&d, "missing.md", 10), Ok(None));
        fs::write(d.join("big"), "0123456789x").unwrap();
        assert_eq!(file_of(&d, "big", 10).unwrap_err(), "big is over 10 bytes");
    }
}
