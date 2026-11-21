//! The gate's own pages: the sign-in, a refusal, the findings of a hook.
//! Every text in them is escaped, and each carries the policy that
//! [`super::csp`] works out from its own bytes.

use serde_json::Value;

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

/// The findings of a kept verdict (docs/specs/2026-11-21-plugins-hooks.md,
/// "The verdict"). None: not a verdict.
pub fn findings(name: &str, pr: u64, verdict: &[u8]) -> Option<Vec<u8>> {
    let v: Value = serde_json::from_slice(verdict).ok()?;
    let word = v["verdict"].as_str()?;
    let summary = v["summary"].as_str().unwrap_or("");
    let mut body = format!(
        "<p>Verdict: <strong>{}</strong></p><p>{}</p>",
        escape(word),
        escape(summary)
    );
    let list = v["findings"].as_array().map_or(&[][..], |a| a.as_slice());
    if list.is_empty() {
        body.push_str("<p>No findings.</p>");
    } else {
        body.push_str("<ul>");
        for f in list {
            let place = match (f["file"].as_str(), f["line"].as_u64()) {
                (Some(file), Some(line)) => format!("<code>{}:{line}</code> ", escape(file)),
                (Some(file), None) => format!("<code>{}</code> ", escape(file)),
                _ => String::new(),
            };
            let text = escape(f["text"].as_str().unwrap_or(""));
            body.push_str(&format!("<li>{place}{text}</li>"));
        }
        body.push_str("</ul>");
    }
    Some(page(&format!("{name} on #{pr}"), &body, None))
}
