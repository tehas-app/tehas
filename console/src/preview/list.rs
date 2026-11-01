//! The state of one preview, preview/<number>/state.json, and the list for the
//! dashboard, live/previews.json, made from all of them.

use super::run::{say, Preview, MAIN};
use crate::edge;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;
use tehas_core::time;

fn stamp() -> String {
    time::iso(time::now())
}

fn pretty(value: &Value) -> Vec<u8> {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    text.into_bytes()
}

/// Every value of a file of JSON, none for an empty one, and None when it
/// cannot be read as JSON.
fn values(file: &Path) -> Option<Vec<Value>> {
    let text = fs::read_to_string(file).ok()?;
    serde_json::Deserializer::from_str(&text)
        .into_iter::<Value>()
        .collect::<Result<_, _>>()
        .ok()
}

/// A file's first value, null when it has none.
fn first_value(file: &Path) -> Option<Value> {
    let first = values(file)?.into_iter().next();
    Some(first.unwrap_or(Value::Null))
}

/// `$a // $b` of jq: null and false are not there.
fn given(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !matches!(v, Value::Null | Value::Bool(false)))
}

/// A value as `jq -r` prints it into a command substitution.
fn word(value: Option<&Value>) -> String {
    let text = match given(value) {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    text.trim_end_matches('\n').to_string()
}

/// The title and the address of pull.json, as they go into a state. None when
/// the file holds something else than an object or nothing.
fn title_and_url(pull: &Value) -> Option<(Value, Value)> {
    if !(pull.is_object() || pull.is_null()) {
        return None;
    }
    let title = given(pull.get("title")).cloned().unwrap_or(json!(""));
    let url = given(pull.get("url")).cloned().unwrap_or(Value::Null);
    Some((title, url))
}

/// What pull.json holds for one pull request of the list. None for a title
/// that is not text: nothing else of a list is read as a title.
pub(super) fn pull_file(pulls: &[Value], n: u64) -> Option<String> {
    let pull = pulls
        .iter()
        .find(|pull| pull.get("number").and_then(Value::as_u64) == Some(n));
    let title = match pull.and_then(|pull| given(pull.get("title"))) {
        Some(Value::String(title)) => title.chars().take(180).collect(),
        None => String::new(),
        Some(_) => return None,
    };
    let url = pull
        .and_then(|pull| given(pull.get("html_url")))
        .cloned()
        .unwrap_or(Value::Null);
    Some(format!("{}\n", json!({"title": title, "url": url})))
}

impl Preview {
    fn state_file(&self, n: &str) -> std::path::PathBuf {
        self.root.join(n).join("state.json")
    }

    pub(super) fn field(&self, n: &str, key: &str) -> String {
        first_value(&self.state_file(n)).map_or(String::new(), |state| word(state.get(key)))
    }

    /// Written only when it differs, so "time" is when the state began.
    pub(super) fn mark(&self, n: &str, sha: &str, state: &str, reason: &str) -> bool {
        if self.field(n, "sha") == sha && self.field(n, "state") == state {
            return true;
        }
        let dir = self.root.join(n);
        if fs::create_dir_all(&dir).is_err() {
            return false;
        }
        let pull = dir.join("pull.json");
        if !pull.is_file() {
            let _ = fs::write(&pull, "{}\n");
        }
        let Some((title, url)) = first_value(&pull).as_ref().and_then(title_and_url) else {
            return false;
        };
        let digits = n.bytes().all(|b| b.is_ascii_digit());
        let mut row = json!({
            "number": if digits { n.parse().ok() } else { None::<u64> },
            "sha": sha, "state": state, "reason": reason, "time": stamp(),
            "title": title, "url": url,
        });
        if !digits {
            row["name"] = json!(n);
        }
        if edge::put(&self.state_file(n), &pretty(&row)).is_err() {
            return false;
        }
        let who = if n == MAIN {
            n.to_string()
        } else {
            format!("#{n}")
        };
        if reason.is_empty() {
            say(format_args!("{who}: {state}"));
        } else {
            say(format_args!("{who}: {state} ({reason})"));
        }
        true
    }

    /// A pull request that got a new title has it in its state, whatever the
    /// state is. No state began, so "time" stays and no line is written. A
    /// state that cannot be read is left for mark.
    pub(super) fn retitle(&self, n: &str) {
        let file = self.state_file(n);
        if !file.is_file() {
            return;
        }
        let pull = first_value(&self.root.join(n).join("pull.json"));
        let new = pull
            .as_ref()
            .map_or(String::new(), |p| word(p.get("title")));
        if self.field(n, "title") == new {
            return;
        }
        let Some((title, url)) = pull.as_ref().and_then(title_and_url) else {
            return;
        };
        let Some(Value::Object(mut state)) = first_value(&file) else {
            return;
        };
        state.insert("title".to_string(), title);
        state.insert("url".to_string(), url);
        let _ = edge::put(&file, &pretty(&Value::Object(state)));
    }

    /// Every state of a directory that holds one, in the order of their names.
    /// An empty file is no state: a list that stops at one would stay stale
    /// through the runs that only refresh it.
    fn states(&self) -> Option<Vec<Map<String, Value>>> {
        let mut names: Vec<String> = fs::read_dir(&self.root)
            .ok()?
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| !name.starts_with('.') && self.state_file(name).is_file())
            .collect();
        names.sort();
        let mut rows = Vec::new();
        for name in &names {
            for value in values(&self.state_file(name))? {
                match value {
                    Value::Object(state) => rows.push(state),
                    _ => return None,
                }
            }
        }
        Some(rows)
    }

    fn list(&self) -> Option<Value> {
        let sweep = self.root.join("sweep.json");
        let sweep = if sweep.is_file() {
            first_value(&sweep)?
        } else {
            Value::Null
        };
        let (main, mut pulls): (Vec<_>, Vec<_>) = self
            .states()?
            .into_iter()
            .partition(|state| state.get("name").and_then(Value::as_str) == Some(MAIN));
        // A row of neither kind has no number to be sorted by, and the list is
        // not written.
        let of = |state: &Map<String, Value>| state.get("number").and_then(Value::as_f64);
        if pulls.iter().any(|state| of(state).is_none()) {
            return None;
        }
        pulls.sort_by(|a, b| {
            of(b)
                .partial_cmp(&of(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let previews: Vec<Value> = main
            .into_iter()
            .chain(pulls)
            .map(|mut state| {
                let address =
                    (state.get("state").and_then(Value::as_str) == Some("serving")).then(|| {
                        let name = match state.get("name").and_then(Value::as_str) {
                            Some(MAIN) => MAIN.to_string(),
                            _ => word(state.get("number")),
                        };
                        self.address(&name)
                    });
                state.insert("address".to_string(), json!(address));
                Value::Object(state)
            })
            .collect();
        Some(json!({
            "generatedAt": stamp(),
            "disk": {
                "freeBytes": self.free.map(|kb| kb.saturating_mul(1024)),
                "minFreeBytes": self.min_kb.saturating_mul(1024),
                "low": self.low,
            },
            "lastSweep": given(Some(&sweep)),
            "previews": previews,
        }))
    }

    pub(super) fn write_list(&self) -> bool {
        let Some(list) = self.list() else {
            return false;
        };
        edge::put(&self.live.join("previews.json"), &pretty(&list)).is_ok()
    }

    pub(super) fn write_sweep(&self, removed: u64) -> bool {
        let row = json!({"time": stamp(), "removed": removed});
        edge::put(&self.root.join("sweep.json"), &pretty(&row)).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::super::run::scratch;
    use super::*;

    fn state(preview: &Preview, n: &str) -> Value {
        first_value(&preview.state_file(n)).unwrap()
    }

    #[test]
    fn a_mark_writes_the_state_and_keeps_the_time_it_began() {
        let preview = scratch("mark");
        assert!(preview.mark("12", "abc", "building", ""));
        let first = state(&preview, "12");
        assert_eq!(first["number"], 12);
        assert_eq!(first["sha"], "abc");
        assert_eq!(first["state"], "building");
        assert_eq!(first["reason"], "");
        assert_eq!(first["title"], "");
        assert_eq!(first["url"], Value::Null);
        assert!(first.get("name").is_none());

        let mut older = first.clone();
        older["time"] = json!("then");
        fs::write(preview.state_file("12"), older.to_string()).unwrap();
        assert!(preview.mark("12", "abc", "building", "another reason"));
        assert_eq!(
            state(&preview, "12"),
            older,
            "the same state is not written"
        );
        assert!(preview.mark("12", "abc", "serving", ""));
        assert_ne!(state(&preview, "12")["time"], older["time"]);
    }

    #[test]
    fn a_mark_carries_the_title_and_main_has_a_name_and_no_number() {
        let preview = scratch("mark-main");
        fs::create_dir_all(preview.root.join("main")).unwrap();
        fs::write(
            preview.root.join("main/pull.json"),
            r#"{"title":"T","url":"u"}"#,
        )
        .unwrap();
        assert!(preview.mark("main", "abc", "no-recipe", "No file."));
        let row = state(&preview, "main");
        assert_eq!(row["number"], Value::Null);
        assert_eq!(row["name"], "main");
        assert_eq!(row["title"], "T");
        assert_eq!(row["url"], "u");
        assert_eq!(row["reason"], "No file.");
    }

    #[test]
    fn a_pull_file_that_cannot_be_read_marks_nothing() {
        let preview = scratch("mark-bad");
        fs::create_dir_all(preview.root.join("12")).unwrap();
        for bad in ["not json", "[]", "\"text\"", "12"] {
            fs::write(preview.root.join("12/pull.json"), bad).unwrap();
            assert!(!preview.mark("12", "abc", "serving", ""), "{bad}");
        }
        assert!(!preview.state_file("12").exists());
        fs::write(preview.root.join("12/pull.json"), "").unwrap();
        assert!(
            preview.mark("12", "abc", "serving", ""),
            "empty is no title"
        );
    }

    #[test]
    fn a_new_title_changes_the_title_and_the_address_and_nothing_else() {
        let preview = scratch("retitle");
        assert!(preview.mark("12", "abc", "serving", ""));
        let was = state(&preview, "12");
        fs::write(
            preview.root.join("12/pull.json"),
            r#"{"title":"New","url":"https://x/12"}"#,
        )
        .unwrap();
        preview.retitle("12");
        let now = state(&preview, "12");
        assert_eq!(now["title"], "New");
        assert_eq!(now["url"], "https://x/12");
        assert_eq!(now["time"], was["time"]);
        assert_eq!(now["state"], was["state"]);
    }

    #[test]
    fn a_retitle_leaves_what_it_cannot_read() {
        let preview = scratch("retitle-bad");
        preview.retitle("12");
        assert!(!preview.state_file("12").exists(), "no state, none made");
        assert!(preview.mark("12", "abc", "serving", ""));
        fs::write(preview.root.join("12/pull.json"), r#"{"title":"New"}"#).unwrap();
        fs::write(preview.state_file("12"), "not json").unwrap();
        preview.retitle("12");
        assert_eq!(
            fs::read_to_string(preview.state_file("12")).unwrap(),
            "not json"
        );
    }

    #[test]
    fn a_title_is_cut_at_180_characters_and_must_be_text() {
        let pulls = [
            json!({"number": 12, "title": "ä".repeat(200), "html_url": "u"}),
            json!({"number": 13, "html_url": null}),
            json!({"number": 14, "title": 7}),
        ];
        let file: Value = serde_json::from_str(&pull_file(&pulls, 12).unwrap()).unwrap();
        assert_eq!(file["title"].as_str().unwrap().chars().count(), 180);
        assert_eq!(file["url"], "u");
        assert_eq!(
            pull_file(&pulls, 13).unwrap(),
            "{\"title\":\"\",\"url\":null}\n"
        );
        assert_eq!(
            pull_file(&pulls, 99).unwrap(),
            "{\"title\":\"\",\"url\":null}\n",
            "not in the list is no title"
        );
        assert_eq!(pull_file(&pulls, 14), None);
    }

    fn list_of(preview: &Preview) -> Value {
        assert!(preview.write_list());
        first_value(&preview.live.join("previews.json")).unwrap()
    }

    #[test]
    fn the_list_has_main_first_then_the_newest_and_an_address_only_when_serving() {
        let mut preview = scratch("list");
        for (n, state) in [
            ("9", "serving"),
            ("100", "build-failed"),
            ("12", "serving"),
            ("main", "serving"),
        ] {
            assert!(preview.mark(n, "abc", state, ""));
        }
        preview.free = Some(2);
        preview.low = false;
        let list = list_of(&preview);
        let rows: Vec<(Value, Value)> = list["previews"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["number"].clone(), row["address"].clone()))
            .collect();
        assert_eq!(
            rows,
            [
                (Value::Null, json!("https://uat-main.example.org/")),
                (json!(100), Value::Null),
                (json!(12), json!("https://uat-12.example.org/")),
                (json!(9), json!("https://uat-9.example.org/")),
            ]
        );
        assert_eq!(list["disk"]["freeBytes"], 2048);
        assert_eq!(list["disk"]["minFreeBytes"], 1024 * 1024);
        assert_eq!(list["disk"]["low"], false);
        assert_eq!(list["lastSweep"], Value::Null);
        assert!(list["generatedAt"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn no_figure_is_null_and_low() {
        let mut preview = scratch("list-low");
        preview.free = None;
        preview.low = true;
        let list = list_of(&preview);
        assert_eq!(list["disk"]["freeBytes"], Value::Null);
        assert_eq!(list["disk"]["low"], true);
        assert_eq!(list["previews"], json!([]));
    }

    #[test]
    fn the_last_sweep_is_in_the_list() {
        let preview = scratch("list-sweep");
        assert!(preview.write_sweep(3));
        assert_eq!(list_of(&preview)["lastSweep"]["removed"], 3);
    }

    #[test]
    fn a_state_or_a_sweep_that_cannot_be_read_writes_no_list() {
        let preview = scratch("list-bad");
        fs::write(preview.live.join("previews.json"), "before").unwrap();
        assert!(preview.mark("12", "abc", "serving", ""));
        fs::write(preview.state_file("12"), "not json").unwrap();
        assert!(!preview.write_list());
        fs::write(preview.state_file("12"), r#"{"name":"odd"}"#).unwrap();
        assert!(
            !preview.write_list(),
            "a row with no number cannot be sorted"
        );
        fs::remove_file(preview.state_file("12")).unwrap();
        fs::write(preview.root.join("sweep.json"), "not json").unwrap();
        assert!(!preview.write_list());
        assert_eq!(
            fs::read_to_string(preview.live.join("previews.json")).unwrap(),
            "before"
        );
    }

    #[test]
    fn an_empty_state_is_no_row_and_the_list_is_still_written() {
        let preview = scratch("list-empty");
        assert!(preview.mark("12", "abc", "serving", ""));
        assert!(preview.mark("13", "abc", "serving", ""));
        for empty in ["", " \n"] {
            fs::write(preview.state_file("13"), empty).unwrap();
            let rows = list_of(&preview)["previews"].as_array().unwrap().clone();
            assert_eq!(rows.len(), 1, "{empty:?}");
            assert_eq!(rows[0]["number"], 12);
        }
    }

    #[test]
    fn a_state_of_two_objects_is_two_rows_as_jq_read_it() {
        let preview = scratch("list-two");
        fs::create_dir_all(preview.root.join("12")).unwrap();
        fs::write(
            preview.state_file("12"),
            "{\"number\":12}\n{\"number\":13}\n",
        )
        .unwrap();
        let numbers: Vec<Value> = list_of(&preview)["previews"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["number"].clone())
            .collect();
        assert_eq!(numbers, [json!(13), json!(12)]);
    }

    #[test]
    fn a_state_that_is_a_null_or_not_an_object_writes_no_list() {
        let preview = scratch("list-null");
        assert!(preview.mark("12", "abc", "serving", ""));
        for bad in ["null", "[]", "12", "\"text\""] {
            fs::write(preview.state_file("12"), bad).unwrap();
            assert!(!preview.write_list(), "{bad}");
        }
    }

    #[test]
    fn a_directory_without_a_state_and_a_hidden_one_are_not_rows() {
        let preview = scratch("list-dirs");
        fs::create_dir_all(preview.root.join("repo.git")).unwrap();
        fs::create_dir_all(preview.root.join(".old")).unwrap();
        fs::write(preview.root.join(".old/state.json"), "not json").unwrap();
        assert_eq!(list_of(&preview)["previews"], json!([]));
    }
}
