//! `tehas prototype file`: open a ticket for the Protolab with the caller's
//! own GitHub login. console/src/usage/prototype.txt is the text a person
//! reads.
//!
//!   tehas prototype file --title TEXT [--body-file FILE|-]
//!                        [--zip ADDRESS --zip-sha256 HEX]
//!                        [--spec LINK --spec-sha256 HEX]
//!
//! The Protolab takes a ticket only from an allowed account: the label, the
//! text its links are in, and a `Zip:` line all count only when an account of
//! TEHAS_ALLOWED_LABELERS wrote them (src/protolab.rs). So every call here is
//! the person's gh, never the factory's shim and never the app's token, and a
//! gh signed in as the bot is refused before anything is written.
//!
//! Still being built: with prototype-file off (TEHAS_PROTOTYPE_FILE) every
//! form but the help text is 78. A zip on a host outside GitHub needs
//! protolab-zip-url on as well.
//!
//! Exit: 0 filed and marked; 1 a read or a call of GitHub failed; 3 the gh
//! login is the bot's, or not an allowed account, so nothing is filed; 64 a
//! bad argument, which files nothing; 78 the feature is off, or the settings
//! are missing or malformed.

use crate::forge::{repo, repo_shaped, runnable, CALL_BOUND};
use attach::link::{is_external, is_link};
use serde_json::Value;
use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use tehas_core::accounts::{self, Accounts};
use tehas_core::events::allowed_in;
use tehas_core::forge::{Bounded, Forge, GhRunner, GitHub};
use tehas_core::grammar::{is_hex, spec_mark, SpecMark, SPEC_MARK};
use tehas_core::kinds::{self, Kind};

/// The feature of this command.
pub const FEATURE: &str = "prototype-file";
/// The feature a zip outside GitHub needs.
const ZIP_URL: &str = "protolab-zip-url";

const USAGE: &str = "usage: tehas prototype file --title TEXT [--body-file FILE|-] [--zip ADDRESS --zip-sha256 HEX] [--spec LINK --spec-sha256 HEX]";

/// The line that names a zip by address, as the Protolab reads it.
const ZIP_MARK: &str = "Zip:";

/// GitHub's own bounds of an issue's title and text, in characters.
const TITLE_MAX: usize = 256;
const BODY_MAX: usize = 65_536;

/// The names a gh reads a token from. The person's gh must use the login it
/// was signed in with, so none of them reaches it.
const TOKEN_NAMES: [&str; 5] = [
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
    "TEHAS_GH_TOKEN",
];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Asked {
    title: String,
    body_file: Option<String>,
    zip: Option<(String, String)>,
    spec: Option<SpecMark>,
}

fn bad(why: &str) -> u8 {
    eprintln!("tehas prototype: {why}");
    eprintln!("{USAGE}");
    64
}

/// The arguments after `prototype`, or 64 with the usage.
pub fn words(given: &[String]) -> Result<Asked, u8> {
    let Some((verb, rest)) = given.split_first() else {
        return Err(bad("which verb? file"));
    };
    if verb != "file" {
        return Err(bad(&format!("unknown verb: {verb}")));
    }
    let mut a = Asked::default();
    let (mut title, mut zip, mut zip_sum, mut spec, mut spec_sum) = (None, None, None, None, None);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let slot = match flag.as_str() {
            "--title" => &mut title,
            "--body-file" => &mut a.body_file,
            "--zip" => &mut zip,
            "--zip-sha256" => &mut zip_sum,
            "--spec" => &mut spec,
            "--spec-sha256" => &mut spec_sum,
            other => return Err(bad(&format!("unknown argument: {other}"))),
        };
        let Some(value) = it.next() else {
            return Err(bad(&format!("{flag} needs a value")));
        };
        if slot.replace(value.clone()).is_some() {
            return Err(bad(&format!("{flag} is given twice")));
        }
    }
    let title = title.unwrap_or_default();
    if title.trim().is_empty() || title.contains(['\n', '\r']) {
        return Err(bad("--title is one line of text"));
    }
    if title.chars().count() > TITLE_MAX {
        return Err(bad(&format!("the title is over {TITLE_MAX} characters")));
    }
    a.title = title.trim().to_string();
    if a.body_file.as_deref() == Some("") {
        return Err(bad("--body-file names a file, or - for standard input"));
    }
    a.zip = pair("--zip", zip, zip_sum)?;
    a.spec = pair("--spec", spec, spec_sum)?.map(|(link, sha256)| SpecMark { link, sha256 });
    if let Some(m) = &a.spec {
        if !is_link(&m.link) {
            return Err(bad(&format!(
                "--spec is a file attached on GitHub, not {}",
                m.link
            )));
        }
    }
    Ok(a)
}

/// An address and its sha256, both or neither.
fn pair(
    flag: &str,
    link: Option<String>,
    sum: Option<String>,
) -> Result<Option<(String, String)>, u8> {
    match (link, sum) {
        (None, None) => Ok(None),
        (Some(link), Some(sum)) => {
            if link.is_empty() || link.contains(char::is_whitespace) {
                return Err(bad(&format!("{flag} is one address")));
            }
            if !is_hex(&sum, 64, 64) {
                return Err(bad(&format!("{flag}-sha256 is 64 lower-case hex digits")));
            }
            Ok(Some((link, sum)))
        }
        _ => Err(bad(&format!("{flag} and {flag}-sha256 go together"))),
    }
}

/// Where a zip may come from on this instance: a file attached on GitHub
/// always, an address on a host of `hosts` only with protolab-zip-url on.
/// Err is the status and the line.
fn zip_allowed(link: &str, zips: bool, hosts: &str) -> Result<(), (u8, String)> {
    if is_link(link) {
        return Ok(());
    }
    if !zips {
        return Err((
            78,
            format!("a zip outside GitHub needs {ZIP_URL}, which is off on this instance"),
        ));
    }
    if is_external(link, hosts) {
        return Ok(());
    }
    Err((
        64,
        format!("{link} is not a file attached on GitHub, nor an https address on a host of TEHAS_PROTOLAB_ZIP_HOSTS"),
    ))
}

/// The line as a reader of marks sees it: without the space and the quote
/// marks before it.
fn bare(line: &str) -> &str {
    line.trim_start_matches(|c: char| c == '>' || c.is_whitespace())
}

/// The issue's text: the person's, then a line for the zip and one for the
/// spec, the spec's as tehas_core::grammar writes it. Err says why the text
/// cannot carry them: it has such a line of its own, or leaves a code fence
/// open, which would hide the lines after it. The dispatcher reads the spec
/// back with spec_mark, so the text is held to read back as the spec given.
fn body(
    text: &str,
    zip: Option<&(String, String)>,
    spec: Option<&SpecMark>,
) -> Result<String, String> {
    let mut fence: Option<&str> = None;
    for line in text.lines() {
        let t = line.trim();
        match fence {
            Some(open) if t.starts_with(open) => fence = None,
            Some(_) => {}
            None if t.starts_with("```") || t.starts_with("~~~") => fence = Some(&t[..3]),
            None => {}
        }
        for (mark, given) in [(ZIP_MARK, zip.is_some()), (SPEC_MARK, spec.is_some())] {
            if given && bare(line).starts_with(mark) {
                return Err(format!("the text has a `{mark}` line of its own"));
            }
        }
    }
    if fence.is_some() && (zip.is_some() || spec.is_some()) {
        return Err("the text leaves a code fence open".to_string());
    }
    let mut out = text.trim_end().to_string();
    let lines: Vec<String> = zip
        .map(|(link, sum)| format!("{ZIP_MARK} {link} sha256:{sum}"))
        .into_iter()
        .chain(spec.map(SpecMark::line))
        .collect();
    if !lines.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&lines.join("\n"));
    }
    out.push('\n');
    if out.chars().count() > BODY_MAX {
        return Err(format!("the text is over {BODY_MAX} characters"));
    }
    if let Some(m) = spec {
        match spec_mark(&out) {
            Ok(Some(read)) if &read == m => {}
            Ok(_) => {
                return Err(format!(
                    "the text does not read back as its `{SPEC_MARK}` line"
                ))
            }
            Err(why) => return Err(format!("the text would not be read: {why}")),
        }
    }
    Ok(out)
}

/// The person's gh: the real one the host installs in `home`/bin, else the
/// first of `path`, never the factory's shim, which acts as the app.
fn persons_gh(home: &str, path: &str) -> Option<String> {
    let home = Path::new(home);
    let installed = home.join("bin").join("gh");
    if !home.as_os_str().is_empty() && runnable(&installed) {
        return Some(installed.display().to_string());
    }
    let shim = home.join("shim");
    path.split(':')
        .filter(|d| !d.is_empty() && (home.as_os_str().is_empty() || Path::new(d) != shim))
        .map(|d| Path::new(d).join("gh"))
        .find(|p| runnable(p))
        .map(|p| p.display().to_string())
}

/// Why `login` may not file a prototype here, or None when it may. `bot` is
/// the factory's login; `allowed` the list; `doubt` what the recorded ids say.
fn refused(login: &str, bot: &str, allowed: &str, doubt: Option<String>) -> Option<String> {
    if login.ends_with("[bot]") || accounts::factorys(login, bot) {
        return Some(format!(
            "gh is signed in as {login}, an app: a prototype is filed with a person's own login (gh auth login)"
        ));
    }
    if !allowed_in(allowed, login) {
        return Some(format!(
            "{login} is not in TEHAS_ALLOWED_LABELERS, so the Protolab would not take its label: nothing filed"
        ));
    }
    doubt
}

fn read_text(file: Option<&str>) -> Result<String, String> {
    let mut bytes = Vec::new();
    let bound = (BODY_MAX * 4 + 1) as u64;
    let got = match file {
        None => return Ok(String::new()),
        Some("-") => io::stdin().take(bound).read_to_end(&mut bytes),
        Some(f) => File::open(f).and_then(|h| h.take(bound).read_to_end(&mut bytes)),
    };
    got.map_err(|e| format!("cannot read {}: {e}", file.unwrap_or_default()))?;
    String::from_utf8(bytes).map_err(|_| "the text is not UTF-8".to_string())
}

fn json_of(out: &str, what: &str) -> Result<Value, String> {
    serde_json::from_str::<Value>(out)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| format!("GitHub's answer for {what} could not be read"))
}

pub fn main(given: &[String]) -> u8 {
    if !tehas_core::features::on(FEATURE) {
        eprintln!("tehas prototype: {FEATURE} is off (TEHAS_PROTOTYPE_FILE) and still being built in this release");
        return 78;
    }
    let asked = match words(given) {
        Ok(a) => a,
        Err(rc) => return rc,
    };
    let var = |k: &str| env::var(k).unwrap_or_default();
    let Some(repo) = repo().filter(|r| repo_shaped(r)) else {
        eprintln!("tehas prototype: TEHAS_REPO is not owner/name");
        return 78;
    };
    if let Some((link, _)) = &asked.zip {
        let zips = tehas_core::features::on(ZIP_URL);
        if let Err((rc, why)) = zip_allowed(link, zips, &var("TEHAS_PROTOLAB_ZIP_HOSTS")) {
            eprintln!("tehas prototype: {why}");
            return rc;
        }
    }
    let text = match read_text(asked.body_file.as_deref()) {
        Ok(t) => t,
        Err(why) => {
            eprintln!("tehas prototype: {why}");
            return 1;
        }
    };
    let body = match body(&text, asked.zip.as_ref(), asked.spec.as_ref()) {
        Ok(b) => b,
        Err(why) => return bad(&why),
    };
    let Some(program) = persons_gh(&var("TEHAS_HOME"), &var("PATH")) else {
        eprintln!("tehas prototype: no gh of your own to run (bootstrap.sh installs it)");
        return 1;
    };
    for name in TOKEN_NAMES {
        env::remove_var(name);
    }
    file(&repo, &program, &asked.title, &body)
}

/// Who the person is, the issue, then its mark: the label, or the issue type
/// TEHAS_ISSUE_TYPES maps the kind to, set as the issue is opened.
fn file(repo: &str, program: &str, title: &str, body: &str) -> u8 {
    let mut gh = Bounded::new(program, CALL_BOUND);
    let fail = |why: String| {
        eprintln!("tehas prototype: {why}");
        1
    };
    let user = match gh
        .run(&["api", "user"])
        .and_then(|o| json_of(&o, "your account"))
    {
        Ok(u) => u,
        Err(why) => {
            return fail(format!(
                "your gh could not say who you are ({why}); run gh auth login"
            ))
        }
    };
    let login = user.get("login").and_then(Value::as_str).unwrap_or("");
    if login.is_empty() {
        return fail("GitHub's answer for your account names no login".to_string());
    }
    let var = |k: &str| env::var(k).unwrap_or_default();
    let doubt = Accounts::from_env()
        .doubt(login, user.get("id").and_then(Value::as_u64))
        .map(|d| accounts::says(login, user.get("id").and_then(Value::as_u64), &d));
    if let Some(why) = refused(
        login,
        &var("TEHAS_BOT_LOGIN"),
        &var("TEHAS_ALLOWED_LABELERS"),
        doubt,
    ) {
        eprintln!("tehas prototype: {why}");
        return 3;
    }
    let kind_type = kinds::type_from_env(Kind::Prototype);
    let path = format!("repos/{repo}/issues");
    let (title_f, body_f) = (format!("title={title}"), format!("body={body}"));
    let mut args = vec!["api", "-X", "POST", &path, "-f", &title_f, "-f", &body_f];
    let type_f = kind_type.as_ref().map(|t| format!("type={t}"));
    if let Some(t) = &type_f {
        args.extend(["-f", t]);
    }
    let issue = match gh.run(&args).and_then(|o| json_of(&o, "the new issue")) {
        Ok(i) => i,
        Err(why) => return fail(format!("the issue was not opened: {why}")),
    };
    let Some(n) = issue.get("number").and_then(Value::as_u64) else {
        return fail("GitHub's answer for the new issue has no number".to_string());
    };
    let url = issue.get("html_url").and_then(Value::as_str).unwrap_or("");
    let marked = match &kind_type {
        Some(t) => {
            let got = issue.pointer("/type/name").and_then(Value::as_str);
            if got == Some(t.as_str()) {
                Ok(format!("the type {t}"))
            } else {
                Err(format!("GitHub did not give it the type {t}"))
            }
        }
        None => GitHub::new(repo.to_string(), Box::new(gh))
            .add_label(n, Kind::Prototype.label())
            .map(|()| Kind::Prototype.label().to_string())
            .map_err(|e| e.to_string()),
    };
    match marked {
        Ok(mark) => {
            tehas_core::outln!("filed #{n} as {login} with {mark}: {url}");
            0
        }
        Err(why) => fail(format!(
            "#{n} was opened ({url}) but not marked: {why}; mark it by hand"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn w(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    fn sum(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn mark(link: &str, sha256: String) -> SpecMark {
        SpecMark {
            link: link.into(),
            sha256,
        }
    }

    const ATTACHED: &str = "https://github.com/user-attachments/files/7/proto.zip";
    const SPEC: &str = "https://github.com/user-attachments/files/8/spec.md";

    #[test]
    fn the_words_give_a_title_a_text_a_zip_and_a_spec() {
        let (z, s) = (sum('a'), sum('b'));
        let got = words(&w(&[
            "file",
            "--title",
            " A prototype ",
            "--body-file",
            "-",
            "--zip",
            ATTACHED,
            "--zip-sha256",
            &z,
            "--spec",
            SPEC,
            "--spec-sha256",
            &s,
        ]));
        assert_eq!(
            got,
            Ok(Asked {
                title: "A prototype".into(),
                body_file: Some("-".into()),
                zip: Some((ATTACHED.into(), z)),
                spec: Some(mark(SPEC, s)),
            })
        );
        assert_eq!(
            words(&w(&["file", "--title", "T"])).map(|a| a.zip.is_none() && a.spec.is_none()),
            Ok(true)
        );
    }

    #[test]
    fn a_bad_word_is_64() {
        let z = sum('a');
        let long = "x".repeat(TITLE_MAX + 1);
        for bad in [
            &[][..],
            &["open"],
            &["file"],
            &["file", "--title"],
            &["file", "--title", ""],
            &["file", "--title", "  "],
            &["file", "--title", "a\nb"],
            &["file", "--title", &long],
            &["file", "--title", "T", "--title", "U"],
            &["file", "--title", "T", "--bogus", "x"],
            &["file", "--title", "T", "--body-file", ""],
            &["file", "--title", "T", "--zip", ATTACHED],
            &["file", "--title", "T", "--zip-sha256", &z],
            &[
                "file",
                "--title",
                "T",
                "--zip",
                ATTACHED,
                "--zip-sha256",
                "ABC",
            ],
            &["file", "--title", "T", "--zip", "a b", "--zip-sha256", &z],
            &["file", "--title", "T", "--spec", SPEC],
            &[
                "file",
                "--title",
                "T",
                "--spec",
                "https://example.org/s.md",
                "--spec-sha256",
                &z,
            ],
            &[
                "file",
                "--title",
                "T",
                "--spec",
                SPEC,
                "--spec-sha256",
                &z.to_uppercase(),
            ],
        ] {
            assert_eq!(words(&w(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn a_zip_outside_github_needs_the_zip_url_feature_and_a_listed_host() {
        let ext = "https://files.example.org/p/proto.zip";
        let hosts = "files.example.org";
        assert_eq!(zip_allowed(ATTACHED, false, ""), Ok(()));
        assert_eq!(zip_allowed(ATTACHED, true, ""), Ok(()));
        assert_eq!(zip_allowed(ext, true, hosts), Ok(()));
        assert_eq!(zip_allowed(ext, false, hosts).map_err(|e| e.0), Err(78));
        assert_eq!(zip_allowed(ext, true, "").map_err(|e| e.0), Err(64));
        assert_eq!(
            zip_allowed("http://files.example.org/p.zip", true, hosts).map_err(|e| e.0),
            Err(64)
        );
    }

    #[test]
    fn the_text_takes_a_zip_line_and_a_spec_line_after_it() {
        let z = (ATTACHED.to_string(), sum('a'));
        let s = mark(SPEC, sum('b'));
        assert_eq!(
            body("Make it fast.\n\n", Some(&z), Some(&s)),
            Ok(format!(
                "Make it fast.\n\nZip: {ATTACHED} sha256:{}\nSpec: {SPEC} sha256:{}\n",
                sum('a'),
                sum('b')
            ))
        );
        assert_eq!(
            body("", None, Some(&s)),
            Ok(format!("Spec: {SPEC} sha256:{}\n", sum('b')))
        );
        assert_eq!(body("Words.", None, None), Ok("Words.\n".into()));
        // A fence closed before the lines hides nothing.
        assert!(body("```\nZip: x\n```\n", None, Some(&s)).is_ok());
    }

    #[test]
    fn the_spec_line_is_the_one_grammar_of_a_spec_mark() {
        let s = mark(SPEC, sum('c'));
        let text = body("Words.", None, Some(&s)).unwrap();
        assert_eq!(text.lines().last(), Some(s.line().as_str()));
        assert_eq!(spec_mark(&text), Ok(Some(s.clone())));
        assert_eq!(tehas_dispatch::text::spec_mark(&text), Ok(Some(s)));
    }

    #[test]
    fn a_text_that_would_hide_or_double_a_line_is_refused() {
        let z = (ATTACHED.to_string(), sum('a'));
        let s = mark(SPEC, sum('b'));
        assert!(body("Zip: mine", Some(&z), None).is_err());
        assert!(body("> Zip: quoted", Some(&z), None).is_err());
        assert!(body("  Spec: x", None, Some(&s)).is_err());
        assert!(body("```\nopen", Some(&z), None).is_err());
        assert!(body("~~~\nopen", None, Some(&s)).is_err());
        // Without the flag, the person's own line is the person's text.
        assert!(body("Zip: mine", None, Some(&s)).is_ok());
        assert!(body("```\nopen", None, None).is_ok());
        assert!(body(&"x".repeat(BODY_MAX), None, None).is_err());
    }

    #[test]
    fn the_bot_and_an_account_off_the_list_are_refused() {
        let bot = "acme-app[bot]";
        assert!(refused("acme-app[bot]", bot, "alice", None)
            .unwrap()
            .contains("an app"));
        assert!(refused("acme-app", bot, "acme-app alice", None)
            .unwrap()
            .contains("an app"));
        assert!(refused("other[bot]", bot, "other[bot]", None).is_some());
        assert!(refused("mallory", bot, "alice", None)
            .unwrap()
            .contains("not in TEHAS_ALLOWED_LABELERS"));
        assert_eq!(refused("alice", bot, "bob alice", None), None);
        assert_eq!(
            refused("alice", bot, "alice", Some("renamed".into())),
            Some("renamed".into())
        );
        // No bot login known yet: an app still names itself.
        assert!(refused("acme-app[bot]", "", "acme-app[bot]", None).is_some());
    }

    /// The dispatcher's poll over one issue, #7, with `labels` put on by
    /// alice, an allowed account, and `text` its body.
    fn poll_over(text: &str, labels: &[&str]) -> tehas_dispatch::Decision {
        use serde_json::json;
        use tehas_dispatch::{decide, Settings, Snapshot};
        let settings = json!({
            "TEHAS_REPO": "acme/widget", "TEHAS_CONTROL_ISSUE": 1,
            "TEHAS_ALLOWED_LABELERS": "alice", "TEHAS_MAX_CYCLES": 1,
            "TEHAS_BOT_LOGIN": "tehas-bot",
            "TEHAS_SPEC_PIPE": "1", "TEHAS_TEST_FEATURES": "spec-pipe",
        });
        let gib: u64 = 1 << 30;
        let events: Vec<Value> = labels
            .iter()
            .enumerate()
            .map(|(k, l)| {
                json!({"event": "labeled", "label": {"name": l}, "actor": {"login": "alice"},
                       "created_at": format!("2026-10-01T00:0{k}:00Z")})
            })
            .collect();
        let names: Vec<Value> = labels.iter().map(|l| json!({"name": l})).collect();
        let pressure = "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        let snapshot = json!({
            "repo": "acme/widget",
            "control": {"labels": [], "events": []},
            "plan_open": [], "running": [], "blocked": [], "awaiting": [],
            "ready": [{"number": 7, "labels": names}],
            "limit": {"state": "none"},
            "slice": {
                "show": {"FragmentPath": "/etc/systemd/system/tehas.slice", "ActiveState": "active",
                         "MemoryMax": (20 * gib).to_string()},
                "docker": "systemd 2",
                "controllers": "cpuset cpu io memory hugetlb pids rdma misc",
                "cpus": "8", "root": "/sys/fs/cgroup", "scopes": {}, "pressure": pressure,
            },
            "endpoint": {},
            "disk_kb": 40u64 * 1_048_576,
            "issues": {"7": {
                "events": events,
                "work": {"number": 7, "state": "open", "labels": names},
                "edit": {"body": text, "lastEditedAt": null, "editor": null},
                "prs": [], "comments": {"failed": true}, "park": null, "approval": null,
                "current": null, "refresh_said": "",
            }},
            "now": 1_793_404_800u64,
            "hostname": "tehas-test",
            "bot_prs": [], "marked": [], "planning": [], "refresh_history": [],
            "leftovers": [], "asking": [], "owed": [], "plans": [], "refines": [],
            "refines_owed": [],
        });
        decide(
            &Snapshot::parse(&snapshot),
            &Settings::parse(&settings),
            false,
        )
    }

    #[test]
    fn a_filed_ticket_labelled_ready_starts_its_cycle_with_its_spec() {
        let z = (ATTACHED.to_string(), sum('a'));
        let s = mark(SPEC, sum('b'));
        let text = body("Make it fast.", Some(&z), Some(&s)).unwrap();
        let d = poll_over(&text, &["tehas:prototype", "tehas:ready"]);
        assert_eq!(d.picked, [7], "{:?} {:?}", d.log, d.actions);
        let start = d
            .actions
            .iter()
            .find(|a| a.get("start").and_then(Value::as_u64) == Some(7))
            .expect("a start of #7");
        assert_eq!(
            start["spec"],
            serde_json::json!({"link": SPEC, "sha256": sum('b'), "off": ["tehas:ready"]})
        );
    }

    #[test]
    fn the_persons_gh_is_never_the_shim() {
        let d = tehas_core::testdir::TestDir::new("prototype-gh");
        let home = d.join("tehas");
        let (shim, bin, other) = (home.join("shim"), home.join("bin"), d.join("usr"));
        for dir in [&shim, &other] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join("gh"), "#!/bin/sh\n").unwrap();
            fs::set_permissions(dir.join("gh"), fs::Permissions::from_mode(0o755)).unwrap();
        }
        let (h, s, o) = (
            home.to_str().unwrap(),
            shim.to_str().unwrap(),
            other.to_str().unwrap(),
        );
        let path = format!("{s}:{o}");
        assert_eq!(persons_gh(h, &path), Some(format!("{o}/gh")));
        assert_eq!(persons_gh(h, s), None);
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("gh"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(bin.join("gh"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(persons_gh(h, &path), Some(format!("{}/gh", bin.display())));
    }
}
