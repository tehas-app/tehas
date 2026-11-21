//! A hook's verdict, `/out/_tehas/verdict.json`
//! (docs/specs/2026-11-21-plugins-hooks.md, "The verdict"), read after the tar
//! reader took it: api/plugin-verdict.schema.json. A verdict that breaks it is
//! a hook that did not finish.

use crate::grammar::commit_id;
use serde_json::{json, Map, Value};

/// The most bytes of the file.
pub const BYTES_MAX: u64 = 64 << 10;
pub const SUMMARY_MAX: usize = 140;
pub const FINDINGS_MAX: usize = 20;
pub const TEXT_MAX: usize = 300;
const FILE_MAX: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Word {
    Pass,
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub file: Option<String>,
    pub line: Option<u64>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub verdict: Word,
    pub summary: String,
    pub findings: Vec<Finding>,
}

fn one_line(s: &str, max: usize) -> bool {
    !s.is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
}

fn only(o: &Map<String, Value>, keys: &[&str], at: &str) -> Result<(), String> {
    match o.keys().find(|k| !keys.contains(&k.as_str())) {
        Some(k) => Err(format!("{at}: {k} is no field of a verdict")),
        None => Ok(()),
    }
}

fn finding(v: &Value, at: &str) -> Result<Finding, String> {
    let o = v.as_object().ok_or(format!("{at}: not an object"))?;
    only(o, &["file", "line", "text"], at)?;
    let file = match o.get("file") {
        None => None,
        Some(Value::String(f))
            if one_line(f, FILE_MAX) && !f.starts_with('/') && !f.split('/').any(|p| p == "..") =>
        {
            Some(f.clone())
        }
        Some(_) => return Err(format!("{at}: file is no path in the product")),
    };
    let line = match o.get("line") {
        None => None,
        Some(l) => Some(
            l.as_u64()
                .filter(|n| *n >= 1)
                .ok_or(format!("{at}: line is 1 or more"))?,
        ),
    };
    let text = match o.get("text") {
        Some(Value::String(t)) if one_line(t, TEXT_MAX) => t.clone(),
        _ => {
            return Err(format!(
                "{at}: text is one line of 1 to {TEXT_MAX} characters"
            ))
        }
    };
    Ok(Finding { file, line, text })
}

/// The file's bytes as a verdict, or why it breaks the schema.
pub fn read(bytes: &[u8]) -> Result<Verdict, String> {
    if bytes.len() as u64 > BYTES_MAX {
        return Err(format!("more than {BYTES_MAX} bytes"));
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "not JSON".to_string())?;
    let o = v.as_object().ok_or("not an object")?;
    only(o, &["verdict", "summary", "findings"], "verdict.json")?;
    let verdict = match o.get("verdict").and_then(Value::as_str) {
        Some("pass") => Word::Pass,
        // fail is v2.
        Some("note" | "fail") => Word::Note,
        _ => return Err("verdict is pass or note".to_string()),
    };
    let summary = match o.get("summary") {
        Some(Value::String(s)) if one_line(s, SUMMARY_MAX) => s.clone(),
        _ => {
            return Err(format!(
                "summary is one line of 1 to {SUMMARY_MAX} characters"
            ))
        }
    };
    let findings = match o.get("findings") {
        None => Vec::new(),
        Some(Value::Array(list)) if list.len() <= FINDINGS_MAX => list
            .iter()
            .enumerate()
            .map(|(i, f)| finding(f, &format!("findings[{i}]")))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(format!("findings are a list of at most {FINDINGS_MAX}")),
    };
    Ok(Verdict {
        verdict,
        summary,
        findings,
    })
}

impl Verdict {
    pub fn json(&self) -> Value {
        let word = match self.verdict {
            Word::Pass => "pass",
            Word::Note => "note",
        };
        let findings: Vec<Value> = self
            .findings
            .iter()
            .map(|f| {
                let mut o = json!({"text": f.text});
                if let Some(file) = &f.file {
                    o["file"] = json!(file);
                }
                if let Some(line) = f.line {
                    o["line"] = json!(line);
                }
                o
            })
            .collect();
        json!({"verdict": word, "summary": self.summary, "findings": findings})
    }
}

/// `hooks/<pr>.verdict.json`: the end of a hook, which the runner writes and
/// the gate's page reads. It holds a verdict only when `result` is `done`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct End {
    pub pr: u64,
    pub head: String,
    /// `done`, `failed` or `timeout`.
    pub result: String,
    pub verdict: Option<Verdict>,
}

impl End {
    pub fn json(&self) -> Value {
        json!({
            "api": 1,
            "pr": self.pr,
            "head": self.head,
            "result": self.result,
            "verdict": self.finished().map(Verdict::json),
        })
    }

    /// The file's bytes as an end, or None.
    pub fn read(bytes: &[u8]) -> Option<End> {
        let v: Value = serde_json::from_slice(bytes).ok()?;
        if v.get("api")? != 1 {
            return None;
        }
        let head = v.get("head")?.as_str().filter(|c| commit_id(c))?;
        let result = v.get("result")?.as_str()?;
        if !matches!(result, "done" | "failed" | "timeout") {
            return None;
        }
        let verdict = match v.get("verdict")? {
            Value::Null => None,
            o => Some(read(o.to_string().as_bytes()).ok()?),
        };
        Some(End {
            pr: v.get("pr")?.as_u64().filter(|n| *n > 0)?,
            head: head.to_string(),
            result: result.to_string(),
            verdict: verdict.filter(|_| result == "done"),
        })
    }

    /// The verdict, when the hook finished with one.
    pub fn finished(&self) -> Option<&Verdict> {
        self.verdict.as_ref().filter(|_| self.result == "done")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::schema::{api, closed, fits};
    use serde_json::json;

    fn reads(v: &Value) -> Result<Verdict, String> {
        read(v.to_string().as_bytes())
    }

    #[test]
    fn the_samples_fit_the_schema_and_read() {
        let s = api("plugin-verdict.schema.json");
        assert_eq!(
            s["$id"],
            "https://github.com/tehas-app/tehas/api/plugin-verdict.schema.json"
        );
        closed(&s);
        for name in ["note", "pass"] {
            let v = api(&format!("plugin-verdict.{name}.json"));
            assert!(fits(&v, &s), "{name}");
            assert!(reads(&v).is_ok(), "{name}");
        }
        let note = reads(&api("plugin-verdict.note.json")).unwrap();
        assert_eq!(note.verdict, Word::Note);
        assert_eq!(
            note.findings,
            [Finding {
                file: Some("server/routes/refund.js".into()),
                line: Some(42),
                text: "POST /refund answers without a session".into()
            }]
        );
    }

    #[test]
    fn fail_is_read_as_note() {
        let v = json!({"verdict": "fail", "summary": "x"});
        assert!(fits(&v, &api("plugin-verdict.schema.json")));
        assert_eq!(reads(&v).unwrap().verdict, Word::Note);
    }

    #[test]
    fn what_breaks_the_schema_breaks_the_reader_too() {
        let s = api("plugin-verdict.schema.json");
        let long = "x".repeat(141);
        let bad = [
            json!({"verdict": "ok", "summary": "x"}),
            json!({"verdict": "pass"}),
            json!({"verdict": "pass", "summary": ""}),
            json!({"verdict": "pass", "summary": long}),
            json!({"verdict": "pass", "summary": "two\nlines"}),
            json!({"verdict": "pass", "summary": "x", "status": "error"}),
            json!({"verdict": "pass", "summary": "x", "findings": [{"file": "a"}]}),
            json!({"verdict": "pass", "summary": "x", "findings": [{"text": "t", "line": 0}]}),
            json!({"verdict": "pass", "summary": "x", "findings": [{"text": "t", "file": "/etc/passwd"}]}),
            json!({"verdict": "pass", "summary": "x", "findings": [{"text": "t", "url": "u"}]}),
            json!({"verdict": "pass", "summary": "x", "findings": vec![json!({"text": "t"}); 21]}),
            json!([]),
        ];
        for v in bad {
            assert!(!fits(&v, &s), "the schema takes {v}");
            assert!(reads(&v).is_err(), "the reader takes {v}");
        }
        // A path out of the product is the reader's alone.
        let up = json!({"verdict": "pass", "summary": "x", "findings": [{"text": "t", "file": "a/../../b"}]});
        assert!(reads(&up).is_err());
        assert!(read(&vec![b' '; (BYTES_MAX + 1) as usize]).is_err());
        assert_eq!(read(b"{"), Err("not JSON".to_string()));
    }

    const H: &str = "89abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn an_end_reads_back_as_it_was_written() {
        let v = reads(&api("plugin-verdict.note.json")).unwrap();
        let end = End {
            pr: 42,
            head: H.into(),
            result: "done".into(),
            verdict: Some(v.clone()),
        };
        assert_eq!(
            End::read(end.json().to_string().as_bytes()),
            Some(end.clone())
        );
        assert_eq!(end.finished(), Some(&v));
        let failed = End {
            result: "failed".into(),
            verdict: None,
            ..end.clone()
        };
        assert_eq!(
            End::read(failed.json().to_string().as_bytes()),
            Some(failed.clone())
        );
        assert_eq!(failed.finished(), None);
        for bad in [
            json!({"api": 1, "pr": 42, "head": "x", "result": "done", "verdict": null}),
            json!({"api": 1, "pr": 42, "head": H, "result": "lost", "verdict": null}),
            json!({"api": 1, "pr": 42, "head": H, "result": "done", "verdict": {"verdict": "ok"}}),
            json!({"verdict": "note", "summary": "flat", "findings": []}),
        ] {
            assert_eq!(End::read(bad.to_string().as_bytes()), None, "{bad}");
        }
    }
}
