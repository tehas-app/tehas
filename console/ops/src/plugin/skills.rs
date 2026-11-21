//! `tehas plugin skills`: the granted skills of the ready plugins, for the
//! chat hosts (docs/specs/2026-11-21-plugins-v2-chats.md, "In the chat";
//! api/plugin-skills.schema.json). A skill's text is read again from the
//! plugin's repository at its pin, never from what a run wrote, and a plugin
//! is listed only while what `tehas update` read is still what is there.

use super::exec::Exec;
use super::prepare;
use std::path::Path;
use tehas_core::jq::{obj, J};
use tehas_core::plugin::manifest::CHATS;
use tehas_core::plugin::state::Word;
use tehas_core::plugin::{self, Cap};

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
    for row in super::rows(get, &|name| prepare::read_state(home, name)) {
        if row.state.state != Word::Ready {
            continue;
        }
        let name = row.pin.name.clone();
        match skills_of(x, home, get, &row, chat) {
            Ok(entries) => {
                let bytes: usize = entries.iter().map(|e| e.text.len()).sum();
                if total + bytes > TOTAL_MAX {
                    out.left_out.push((
                        name,
                        format!("its skills would pass {TOTAL_MAX} bytes of skills in all"),
                    ));
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

/// One ready plugin's granted skills for `chat`, or why none are listed.
fn skills_of(
    x: &mut dyn Exec,
    home: &Path,
    get: &dyn Fn(&str) -> String,
    row: &super::Row,
    chat: Option<&str>,
) -> Result<Vec<Entry>, String> {
    let pin = &row.pin;
    let grants = plugin::grants(&get(&plugin::grants_setting(&pin.name)))
        .ok()
        .filter(|g| g.commit == pin.commit)
        .ok_or("the grants are not for the pin")?;
    let r = prepare::read(x, home, &pin.name, &pin.commit)?;
    if r.manifest_sha256_differs(row.state.manifest_sha256.as_deref()) {
        return Err("the manifest is not the one tehas update read: run tehas update".into());
    }
    if !r.refusals.is_empty() {
        return Err(r.refusals.join("; "));
    }
    let Some(m) = r.manifest else {
        return Err("the manifest could not be read".into());
    };
    let mut entries = Vec::new();
    for s in &m.skills {
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
    use tehas_core::plugin::skill;
    use tehas_core::plugin::state::State;
    use tehas_core::testdir::TestDir;

    const GRILL: &str = "### Interview\n\nOne question at a time.\n";
    const SPEC: &str = "### Specify\n\nProblem, solution, criteria.\n";

    fn manifest() -> String {
        r#"{"plugin": 1, "name": "floorplan", "label": "Spec interview", "asks": ["skill:grill", "skill:spec"],
            "skills": [
              {"id": "grill", "about": "Interview the person", "path": "skills/grill.md", "chats": ["obeya", "wizard"]},
              {"id": "spec", "about": "Write the specification", "path": "skills/spec.md", "chats": ["assistant"]}
            ]}"#
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

    fn ids(l: &Listing) -> Vec<&str> {
        l.entries.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn the_granted_skills_are_read_again_from_the_pin_for_each_chat() {
        let m = manifest();
        let files = [("skills/grill.md", GRILL), ("skills/spec.md", SPEC)];
        let (home, mut x) = ready("skills-list", &m, &files);
        let get = settings("skill:grill skill:spec");
        let all = list(&mut x, &home, &get, None);
        assert_eq!(ids(&all), ["grill", "spec"]);
        assert_eq!(all.left_out, []);
        let e = &all.entries[0];
        assert_eq!(
            (
                e.plugin.as_str(),
                e.label.as_str(),
                e.commit.as_str(),
                e.text.as_str()
            ),
            ("floorplan", "Spec interview", C, GRILL)
        );
        assert_eq!(e.sha256, prepare::sha256(GRILL.as_bytes()));
        assert!(x.ran_any(&format!("cat-file blob {C}:skills/grill.md")));
        assert_eq!(ids(&list(&mut x, &home, &get, Some("wizard"))), ["grill"]);
        assert_eq!(ids(&list(&mut x, &home, &get, Some("assistant"))), ["spec"]);
        let only = settings("skill:grill");
        assert_eq!(ids(&list(&mut x, &home, &only, None)), ["grill"]);
    }

    #[test]
    fn a_plugin_is_left_out_when_what_update_read_is_not_there_now() {
        let m = manifest();
        let files = [("skills/grill.md", GRILL), ("skills/spec.md", SPEC)];
        let get = settings("skill:grill skill:spec");

        let (home, mut x) = ready("skills-moved", &m, &files);
        let mut s = State::new(Word::Ready, C);
        s.manifest_sha256 = Some("0".repeat(64));
        prepare::write_state(&home, "floorplan", &s).unwrap();
        let l = list(&mut x, &home, &get, None);
        assert_eq!(ids(&l), Vec::<&str>::new());
        assert_eq!(
            l.left_out,
            [(
                "floorplan".to_string(),
                "the manifest is not the one tehas update read: run tehas update".to_string()
            )]
        );

        let bad = [("skills/grill.md", "a\u{1b}[2Jb"), ("skills/spec.md", SPEC)];
        let (home, mut x) = ready("skills-bad", &m, &bad);
        let l = list(&mut x, &home, &get, None);
        assert_eq!(l.entries, []);
        assert!(l.left_out[0].1.contains("skills/grill.md"), "{l:?}");

        let (home, mut x) = ready("skills-unready", &m, &files);
        prepare::write_state(&home, "floorplan", &State::new(Word::Building, C)).unwrap();
        assert_eq!(list(&mut x, &home, &get, None), Listing::default());
    }

    /// A manifest of `n` skills `a`, `b`, ... at `<id>.md`, and its grants.
    fn many(n: u8) -> (String, String) {
        let ids: Vec<String> = (0..n).map(|i| ((b'a' + i) as char).to_string()).collect();
        let skills: Vec<String> = ids
            .iter()
            .map(|i| {
                format!(r#"{{"id": "{i}", "about": "A", "path": "{i}.md", "chats": ["obeya"]}}"#)
            })
            .collect();
        let asks: Vec<String> = ids.iter().map(|i| format!(r#""skill:{i}""#)).collect();
        let m = format!(
            r#"{{"plugin": 1, "name": "floorplan", "label": "Many", "asks": [{}], "skills": [{}]}}"#,
            asks.join(", "),
            skills.join(", ")
        );
        let grants = ids
            .iter()
            .map(|i| format!("skill:{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        (m, grants)
    }

    #[test]
    fn skills_past_the_total_leave_their_plugin_out_whole() {
        let full = "x".repeat(skill::BYTES_MAX);
        let names: Vec<String> = (0..5u8)
            .map(|i| format!("{}.md", (b'a' + i) as char))
            .collect();
        let files: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), full.as_str())).collect();

        let (m, g) = many(4);
        let (home, mut x) = ready("skills-total", &m, &files[..4]);
        assert_eq!(list(&mut x, &home, &settings(&g), None).entries.len(), 4);

        let (m, g) = many(5);
        let (home, mut x) = ready("skills-over", &m, &files);
        let l = list(&mut x, &home, &settings(&g), None);
        assert_eq!(l.entries, []);
        assert!(l.left_out[0].1.contains("8192"), "{l:?}");
    }

    #[test]
    fn the_json_has_the_fields_of_the_api_sample() {
        let m = manifest();
        let files = [("skills/grill.md", GRILL), ("skills/spec.md", SPEC)];
        let (home, mut x) = ready("skills-json", &m, &files);
        let l = list(&mut x, &home, &settings("skill:grill"), Some("wizard"));
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
