//! The gate's own pages: the sign-in, a refusal, the findings of a hook.
//! Every text in them is escaped, and each carries the policy that
//! [`super::csp`] works out from its own bytes.

use tehas_core::plugin::verdict::{End, Word};

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

const STYLE: &str = "body{font:16px/1.5 system-ui,sans-serif;max-width:40em;margin:2em auto;padding:0 16px}li{margin:.5em 0}code{font-size:.9em}";

/// A page of `title` and `body`, which is HTML the gate made. `script` is
/// the gate's own.
pub fn page(title: &str, body: &str, script: Option<&str>) -> Vec<u8> {
    let script = script.map_or(String::new(), |s| format!("<script>{s}</script>"));
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{STYLE}</style></head><body><h1>{}</h1>{body}{script}</body></html>\n",
        escape(title),
        escape(title)
    )
    .into_bytes()
}

/// Polls `/_session` every 3 seconds and reloads once it is signed in, as a
/// frame of the dock waits for the sign-in in another tab.
const POLL: &str = "setInterval(function(){fetch('/_session',{credentials:'same-origin'}).then(function(r){if(r.ok)location.reload()})},3000)";

/// The sign-in page of a plugin's name: a link that opens the sign-in in a
/// tab of its own, `start`.
pub fn sign_in(label: &str, start: &str) -> Vec<u8> {
    let body = format!(
        "<p>Sign in with GitHub to see {}.</p><p><a href=\"{}\" target=\"_blank\" rel=\"noopener\">Sign in with GitHub</a></p><p>The sign-in opens in a new tab. This page reloads when it is done.</p>",
        escape(label),
        escape(start)
    );
    page("Sign in", &body, Some(POLL))
}

pub fn refusal(title: &str, why: &str) -> Vec<u8> {
    page(title, &format!("<p>{}</p>", escape(why)), None)
}

/// The page of a hook's end, `hooks/<pr>.verdict.json`
/// (docs/specs/2026-11-21-plugins-hooks.md, "The verdict"): the head it read
/// and its findings, or that it did not finish. None: no end of `pr`.
pub fn findings(name: &str, pr: u64, file: &[u8]) -> Option<Vec<u8>> {
    let end = End::read(file).filter(|e| e.pr == pr)?;
    let short = &end.head[..12];
    let mut body = format!(
        "<p>Head: <code title=\"{}\">{}</code></p>",
        escape(&end.head),
        escape(short)
    );
    let Some(v) = end.finished() else {
        body.push_str(&format!(
            "<p>The hook did not finish: it ended {}.</p>",
            escape(&end.result)
        ));
        return Some(page(&format!("{name} on #{pr}"), &body, None));
    };
    let word = match v.verdict {
        Word::Pass => "pass",
        Word::Note => "note",
    };
    body.push_str(&format!(
        "<p>Verdict: <strong>{word}</strong></p><p>{}</p>",
        escape(&v.summary)
    ));
    if v.findings.is_empty() {
        body.push_str("<p>No findings.</p>");
    } else {
        body.push_str("<ul>");
        for f in &v.findings {
            let place = match (&f.file, f.line) {
                (Some(file), Some(line)) => format!("<code>{}:{line}</code> ", escape(file)),
                (Some(file), None) => format!("<code>{}</code> ", escape(file)),
                _ => String::new(),
            };
            body.push_str(&format!("<li>{place}{}</li>", escape(&f.text)));
        }
        body.push_str("</ul>");
    }
    Some(page(&format!("{name} on #{pr}"), &body, None))
}
