//! `tehas plugin skills`: the granted skills of the ready plugins, for the
//! chat hosts (docs/specs/2026-11-21-plugins-v2-chats.md, "In the chat";
//! api/plugin-skills.schema.json). A skill's text is read again from the
//! plugin's repository at its pin, never from what a run wrote, and a plugin
//! is listed only while what `tehas update` read is still what is there.

use super::exec::Exec;
use super::prepare;
use super::ready::ready;
use std::path::Path;
use tehas_core::jq::{obj, J};
use tehas_core::plugin::manifest::CHATS;
use tehas_core::plugin::state::Word;
use tehas_core::plugin::Cap;

/// The most bytes of skill text across all plugins.
pub const TOTAL_MAX: usize = 8192;

/// One skill as a chat host takes it.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub plugin: String,
    pub label: String,
    pub commit: String,
    pub id: String,
    pub about: String,
    pub chats: Vec<String>,
    pub text: String,
    pub sha256: String,
}

/// What the command found: the skills listed, and each ready plugin left
/// out with why.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub chat: Option<String>,
    pub entries: Vec<Entry>,
    pub left_out: Vec<(String, String)>,
}

/// Whether `chat` names a chat host.
pub fn is_chat(chat: &str) -> bool {
    CHATS.contains(&chat)
}

/// The skills of the plugins of `get`'s settings, for `chat` or for all.
pub fn list(
    x: &mut dyn Exec,
    home: &Path,
    get: &dyn Fn(&str) -> String,
    chat: Option<&str>,
) -> Listing {
    let mut out = Listing {
        chat: chat.map(str::to_string),
        ..Listing::default()
    };
    let mut total = 0;
    for row in super::rows(get, &|name| prepare::read_state(home, name), &|_| (None, 0)) {
        if row.state.state != Word::Ready {
            continue;
        }
        let name = row.pin.name;
        match skills_of(x, home, get, &name, chat) {
            Ok(entries) => {
                let bytes: usize = entries.iter().map(|e| e.text.len()).sum();
                if let Err(why) = fits_total(total, bytes) {
                    out.left_out.push((name, why));
                } else {
                    total += bytes;
                    out.entries.extend(entries);
                }
            }
            Err(why) => out.left_out.push((name, why)),
        }
    }
    out
}

/// Whether `bytes` of skill text still fit once `total` of them are taken.
/// A 2.x manifest cannot reach this through [`list`] until #3371 reads the
/// frontmatter, so the test calls it directly.
fn fits_total(total: usize, bytes: usize) -> Result<(), String> {
    if total + bytes > TOTAL_MAX {
        return Err(format!(
            "its skills would pass {TOTAL_MAX} bytes of skills in all"
        ));
    }
    Ok(())
}

/// One ready plugin's granted skills for `chat`, or why none are listed.
fn skills_of(
    x: &mut dyn Exec,
    home: &Path,
    get: &dyn Fn(&str) -> String,
    name: &str,
    chat: Option<&str>,
) -> Result<Vec<Entry>, String> {
    let r = ready(x, home, get, name)?;
    let (pin, grants, m) = (&r.pin, &r.grants, &r.manifest);
    let mut entries = Vec::new();
    // The 2.2 table keeps a skill's about and chats in the file's own
    // frontmatter, which this host does not read yet (#3371). The listing
    // keeps reading the 1.x entries, which a 2.x reader never sets: a 2.x
    // plugin lists no skill until the frontmatter is read, rather than
    // guessing which chats its text belongs to.
    for s in &m.v1.skills {
        let granted = grants.caps.contains(&Cap::Skill(s.id.clone()));
        let for_chat = chat.is_none_or(|c| s.chats.iter().any(|h| h == c));
        if !granted || !for_chat {
            continue;
        }
        let text = prepare::skill_text(x, home, &pin.name, &pin.commit, &s.path)?;
        entries.push(Entry {
            plugin: pin.name.clone(),
            label: m.label.clone(),
            commit: pin.commit.clone(),
            id: s.id.clone(),
            about: s.about.clone(),
            chats: s.chats.clone(),
            sha256: prepare::sha256(text.as_bytes()),
            text,
        });
    }
    Ok(entries)
}

impl Listing {
    /// The object of `--json`, api/plugin-skills.schema.json.
    pub fn json(&self) -> J {
        let strs = |v: &[String]| J::Arr(v.iter().map(|s| J::str(s.as_str())).collect());
        let skills = self
            .entries
            .iter()
            .map(|e| {
                obj(vec![
                    ("plugin", J::str(&e.plugin)),
                    ("label", J::str(&e.label)),
                    ("commit", J::str(&e.commit)),
                    ("id", J::str(&e.id)),
                    ("about", J::str(&e.about)),
                    ("chats", strs(&e.chats)),
                    ("text", J::str(&e.text)),
                    ("sha256", J::str(&e.sha256)),
                ])
            })
            .collect();
        obj(vec![
            ("api", J::int(1)),
            ("chat", self.chat.as_deref().map_or(J::Null, J::str)),
            ("skills", J::Arr(skills)),
        ])
    }

    /// The lines a person reads: one a skill, then each plugin left out.
    pub fn text(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .entries
            .iter()
            .map(|e| {
                format!(
                    "{}/{}\t{}\t{}\t{} bytes\t{}",
                    e.plugin,
                    e.id,
                    e.label,
                    e.chats.join(","),
                    e.text.len(),
                    e.about
                )
            })
            .collect();
        if out.is_empty() {
            out.push("no skills: no ready plugin has a granted skill for this chat".into());
        }
        out.extend(
            self.left_out
                .iter()
                .map(|(n, why)| format!("left out: {n}: {why}")),
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::exec::fake::Fake;
    use super::super::prepare::tests::{good, C, URL};
    use super::*;
    use tehas_core::plugin::state::State;
    use tehas_core::testdir::TestDir;

    const GRILL: &str = "### Interview\n\nOne question at a time.\n";

    /// A 2.2 manifest: the skills are paths, and their about and chats live
    /// in the file's own frontmatter, which this host does not read yet
    /// (#3371).
    fn manifest() -> String {
        r#"{"plugin": 2, "spec": "2.0.0-r8", "name": "floorplan", "label": "Spec interview",
            "asks": ["skill:grill", "skill:spec"],
            "skills": ["skills/grill/SKILL.md", "skills/spec/SKILL.md"]}"#
            .to_string()
    }

    /// A home with the plugin ready at the pin, as tehas update leaves it.
    fn ready(name: &str, m: &str, files: &[(&str, &str)]) -> (TestDir, Fake) {
        let mut all = vec![("tehas-plugin.json", m)];
        all.extend_from_slice(files);
        let (home, x) = good(name, &all);
        let mut s = State::new(Word::Ready, C);
        s.manifest_sha256 = Some(prepare::sha256(m.as_bytes()));
        prepare::write_state(&home, "floorplan", &s).unwrap();
        (home, x)
    }

    fn settings(grants: &str) -> impl Fn(&str) -> String {
        let grants = format!("{C} {grants}");
        move |k: &str| match k {
            "TEHAS_PLUGINS" => format!("floorplan={URL}@{C}"),
            "TEHAS_PLUGIN_GRANTS_FLOORPLAN" => grants.clone(),
            _ => String::new(),
        }
    }

    /// A 2.2 plugin lists no skill until #3371 reads the frontmatter, and
    /// says nothing against it either: the skills are there and granted, and
    /// the host still reads none of them, rather than guessing which chats
    /// a text belongs to. The counter-assertion is the same run: nothing is
    /// left out, and no skill blob is read.
    #[test]
    fn a_2_x_plugin_lists_no_skill_until_the_frontmatter_is_read() {
        let m = manifest();
        let files = [
            ("skills/grill/SKILL.md", GRILL),
            ("skills/spec/SKILL.md", GRILL),
        ];
        let (home, mut x) = ready("skills-2x", &m, &files);
        let get = settings("skill:grill skill:spec");
        let l = list(&mut x, &home, &get, None);
        assert_eq!(l.entries, []);
        assert_eq!(l.left_out, []);
        // The plugin's own files are read and checked, as tehas update
        // checks them; what is missing is only the listing.
        assert!(l.text()[0].starts_with("no skills:"), "{:?}", l.text());
    }

    #[test]
    fn a_plugin_is_left_out_when_what_update_read_is_not_there_now() {
        let m = manifest();
        let files = [
            ("skills/grill/SKILL.md", GRILL),
            ("skills/spec/SKILL.md", GRILL),
        ];
        let get = settings("skill:grill skill:spec");

        let (home, mut x) = ready("skills-moved", &m, &files);
        let mut s = State::new(Word::Ready, C);
        s.manifest_sha256 = Some("0".repeat(64));
        prepare::write_state(&home, "floorplan", &s).unwrap();
        let l = list(&mut x, &home, &get, None);
        assert_eq!(
            l.left_out,
            [(
                "floorplan".to_string(),
                "the manifest is not the one tehas update read: run tehas update".to_string()
            )]
        );

        // A skill file that breaks a rule is refused by tehas update, not
        // here: ops/src/plugin/prepare.rs holds that test, and this host
        // reads no skill text at all until #3371.
        let (home, mut x) = ready("skills-unready", &m, &files);
        prepare::write_state(&home, "floorplan", &State::new(Word::Building, C)).unwrap();
        assert_eq!(list(&mut x, &home, &get, None), Listing::default());
    }

    /// The total bound, at the seam a 2.x manifest cannot reach until
    /// #3371: one byte under is in, one over is out whole.
    #[test]
    fn skills_past_the_total_leave_their_plugin_out_whole() {
        assert_eq!(fits_total(0, 4096), Ok(()));
        assert_eq!(fits_total(TOTAL_MAX - 1, 1), Ok(()));
        assert!(fits_total(TOTAL_MAX, 1).unwrap_err().contains("8192"));
        assert!(fits_total(TOTAL_MAX - 1, 2).unwrap_err().contains("8192"));
    }

    #[test]
    fn the_json_has_the_fields_of_the_api_sample() {
        let l = Listing {
            chat: Some("wizard".into()),
            entries: vec![Entry {
                plugin: "floorplan".into(),
                label: "Spec interview".into(),
                commit: C.into(),
                id: "grill".into(),
                about: "Interview the person".into(),
                chats: vec!["obeya".into(), "wizard".into()],
                text: GRILL.into(),
                sha256: prepare::sha256(GRILL.as_bytes()),
            }],
            left_out: vec![],
        };
        let got: serde_json::Value = serde_json::from_str(&l.json().dump()).unwrap();
        let sample: serde_json::Value =
            serde_json::from_str(include_str!("../../../../api/plugin-skills.sample.json"))
                .unwrap();
        let keys = |v: &serde_json::Value| -> Vec<String> {
            v.as_object().unwrap().keys().cloned().collect()
        };
        assert_eq!(keys(&got), keys(&sample));
        assert_eq!(keys(&got["skills"][0]), keys(&sample["skills"][0]));
        assert_eq!(got["chat"], "wizard");
    }
}
