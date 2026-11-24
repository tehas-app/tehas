//! What the fetch and the build of a plugin came to, kept in
//! `~/tehas/plugins/<name>/state.json` and read by `tehas plugin`: the state,
//! why it was refused, the commit it is of, the local image id, the
//! manifest's sha256 and the base images by digest.

use serde::Serialize;
use serde_json::Value;

/// `state.json` as it is written.
#[derive(Serialize)]
struct StateFile<'a> {
    api: u8,
    state: &'a str,
    refusal: Option<&'a str>,
    commit: &'a str,
    image: Option<&'a str>,
    manifest_sha256: Option<&'a str>,
    bases: &'a [String],
}

/// The states of a plugin. `unbuilt` has no file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Word {
    Unbuilt,
    Building,
    Ready,
    Refused,
}

impl Word {
    pub fn word(self) -> &'static str {
        match self {
            Word::Unbuilt => "unbuilt",
            Word::Building => "building",
            Word::Ready => "ready",
            Word::Refused => "refused",
        }
    }

    fn of(s: &str) -> Option<Word> {
        [Word::Unbuilt, Word::Building, Word::Ready, Word::Refused]
            .into_iter()
            .find(|w| w.word() == s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub state: Word,
    pub refusal: Option<String>,
    pub commit: String,
    /// The image's id, for a plugin built. None for one with no build.
    pub image: Option<String>,
    pub manifest_sha256: Option<String>,
    /// The base images, each by digest.
    pub bases: Vec<String>,
}

impl State {
    pub fn new(state: Word, commit: &str) -> State {
        State {
            state,
            refusal: None,
            commit: commit.to_string(),
            image: None,
            manifest_sha256: None,
            bases: Vec::new(),
        }
    }

    pub fn refused(commit: &str, why: impl Into<String>) -> State {
        State {
            refusal: Some(why.into()),
            ..State::new(Word::Refused, commit)
        }
    }

    /// The file's text.
    pub fn json(&self) -> String {
        let file = StateFile {
            api: 1,
            state: self.state.word(),
            refusal: self.refusal.as_deref(),
            commit: &self.commit,
            image: self.image.as_deref(),
            manifest_sha256: self.manifest_sha256.as_deref(),
            bases: &self.bases,
        };
        serde_json::to_string(&file).unwrap_or_default()
    }

    /// The file's text. None: not a state this writes, which a reader takes
    /// for none.
    pub fn read(text: &str) -> Option<State> {
        let v: Value = serde_json::from_str(text).ok()?;
        if v.get("api")? != 1 {
            return None;
        }
        let text_or_null = |k: &str| -> Option<Option<String>> {
            match v.get(k)? {
                Value::Null => Some(None),
                Value::String(s) => Some(Some(s.clone())),
                _ => None,
            }
        };
        let commit = v.get("commit")?.as_str()?;
        if !crate::grammar::commit_id(commit) {
            return None;
        }
        let bases = v
            .get("bases")?
            .as_array()?
            .iter()
            .map(|b| b.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()?;
        Some(State {
            state: Word::of(v.get("state")?.as_str()?)?,
            refusal: text_or_null("refusal")?,
            commit: commit.to_string(),
            image: text_or_null("image")?,
            manifest_sha256: text_or_null("manifest_sha256")?,
            bases,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn a_state_reads_back_as_it_was_written() {
        let mut s = State::new(Word::Ready, C);
        s.image = Some(format!("sha256:{}", "a".repeat(64)));
        s.manifest_sha256 = Some("b".repeat(64));
        s.bases = vec![format!("node@sha256:{}", "c".repeat(64))];
        assert_eq!(State::read(&s.json()), Some(s));
        let r = State::refused(C, "the fetch failed");
        assert_eq!(State::read(&r.json()), Some(r));
    }

    #[test]
    fn a_file_of_another_shape_is_no_state() {
        for bad in [
            "",
            "{}",
            r#"{"api":2,"state":"ready","refusal":null,"commit":"x","image":null,"manifest_sha256":null,"bases":[]}"#,
            &format!(
                r#"{{"api":1,"state":"done","refusal":null,"commit":"{C}","image":null,"manifest_sha256":null,"bases":[]}}"#
            ),
            &format!(
                r#"{{"api":1,"state":"ready","refusal":1,"commit":"{C}","image":null,"manifest_sha256":null,"bases":[]}}"#
            ),
        ] {
            assert_eq!(State::read(bad), None, "{bad}");
        }
    }
}
