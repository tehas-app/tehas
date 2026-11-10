//! The factory's new GitHub App: its manifest, the page that hands the
//! manifest to GitHub in the owner's browser, and the code GitHub sends back.

use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// The app the factory needs: what a cycle and the host write and read, no
/// workflows, no events and no webhook. Private: one account installs it.
pub fn json(name: &str, owner_url: &str, redirect: &str) -> String {
    json!({
        "name": name,
        "url": owner_url,
        "redirect_url": redirect,
        "public": false,
        "hook_attributes": {"url": owner_url, "active": false},
        "default_events": [],
        "default_permissions": {
            "contents": "write",
            "issues": "write",
            "pull_requests": "write",
            "metadata": "read",
            "actions": "read",
            "checks": "read",
            "statuses": "read"
        }
    })
    .to_string()
}

/// Where GitHub takes a manifest for an organisation's app or a user's.
pub fn form_url(owner: &str, is_org: bool) -> String {
    if is_org {
        format!("https://github.com/organizations/{owner}/settings/apps/new")
    } else {
        "https://github.com/settings/apps/new".to_string()
    }
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A page that posts the manifest to GitHub as soon as it is open. The state
/// goes in the address, where GitHub takes it and sends it back.
pub fn page(manifest: &str, action: &str, state: &str) -> String {
    format!(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<title>New GitHub App</title>\n\
<form method=\"post\" action=\"{}?state={}\">\n\
<input type=\"hidden\" name=\"manifest\" value=\"{}\">\n\
<button type=\"submit\">Create the GitHub App</button>\n</form>\n\
<script>document.forms[0].submit()</script>\n",
        escaped(action),
        escaped(state),
        escaped(manifest)
    )
}

fn base64(data: &[u8]) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let at = |i: usize| chunk.get(i).copied().unwrap_or(0) as u32;
        let n = at(0) << 16 | at(1) << 8 | at(2);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(LETTERS[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The page as one link to paste into a browser's address bar.
pub fn data_link(manifest: &str, action: &str, state: &str) -> String {
    format!(
        "data:text/html;base64,{}",
        base64(page(manifest, action, state).as_bytes())
    )
}

fn is_code(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= 100
        && word
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

/// The value of `name` in the query of a URL, if it has one.
fn query_value<'a>(url: &'a str, name: &str) -> Option<&'a str> {
    let query = url.split_once('?')?.1;
    let query = query.split('#').next().unwrap_or_default();
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
}

fn is_url(pasted: &str) -> bool {
    pasted.contains("://") || pasted.contains('?')
}

/// The code in what the owner pasted: the address GitHub sent the browser
/// to, or the code alone.
pub fn code_of(pasted: &str) -> Option<String> {
    let pasted = pasted.trim();
    let code = if is_url(pasted) {
        query_value(pasted, "code")?
    } else {
        pasted
    };
    is_code(code).then(|| code.to_string())
}

/// The code, when a pasted address carries this run's state. A code alone is
/// taken: the owner copied it from the address.
pub fn checked_code(pasted: &str, state: &str) -> Result<String, &'static str> {
    let pasted = pasted.trim();
    let code = code_of(pasted).ok_or("that is not an address with a code, nor a code")?;
    if is_url(pasted) && query_value(pasted, "state") != Some(state) {
        return Err("the address does not carry the state this run sent to GitHub");
    }
    Ok(code)
}

/// 48 hex digits from the system's random source.
pub fn new_state() -> io::Result<String> {
    let mut bytes = [0u8; 24];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// What is kept of GitHub's answer to the conversion. The client and webhook
/// secrets are not kept. No Debug: the key is in it.
pub struct Made {
    pub id: String,
    pub slug: String,
    pub pem: String,
}

pub fn made(answer: &[u8]) -> Option<Made> {
    let answer: Value = serde_json::from_slice(answer).ok()?;
    let id = answer["id"].as_u64()?.to_string();
    let slug = answer["slug"].as_str()?;
    let good_slug = slug.starts_with(|c: char| c.is_ascii_alphanumeric())
        && slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let pem = answer["pem"].as_str()?;
    (good_slug && pem.starts_with("-----BEGIN")).then(|| Made {
        id,
        slug: slug.to_string(),
        pem: pem.to_string(),
    })
}

/// A directory that is a directory, not a link to one.
fn real_parent(file: &Path) -> io::Result<()> {
    let parent = file.parent().unwrap_or(Path::new("."));
    let meta = fs::symlink_metadata(parent)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(io::Error::other(format!(
            "{} is not a plain directory",
            parent.display()
        )));
    }
    Ok(())
}

/// Whether `write_key` can write there: a plain directory and nothing at the
/// path, not even a link. Asked before a code is spent.
pub fn key_can_go(file: &Path) -> io::Result<()> {
    real_parent(file)?;
    match fs::symlink_metadata(file) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} is there", file.display()),
        )),
    }
}

/// The new app's key, mode 600, in a file that was not there.
pub fn write_key(file: &Path, pem: &str) -> io::Result<()> {
    real_parent(file)?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(file)?;
    f.write_all(pem.as_bytes())?;
    f.sync_all()
}

/// The page, mode 600, never written through a link.
pub fn write_page(file: &Path, html: &str) -> io::Result<()> {
    real_parent(file)?;
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(file)?;
    use std::os::unix::fs::PermissionsExt;
    f.set_permissions(fs::Permissions::from_mode(0o600))?;
    f.write_all(html.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn decoded(link: &str) -> String {
        let b64 = link
            .strip_prefix("data:text/html;base64,")
            .expect("a base64 data link");
        let out = std::process::Command::new("base64")
            .arg("-d")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                use std::io::Write;
                c.stdin.take().unwrap().write_all(b64.as_bytes())?;
                c.wait_with_output()
            })
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    }

    fn unescaped(html: &str) -> String {
        html.replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }

    #[test]
    fn manifest_has_the_permissions_and_no_hook() {
        let m: Value = serde_json::from_str(&json(
            "widget-factory",
            "https://github.com/acme",
            "https://github.com/acme",
        ))
        .unwrap();
        assert_eq!(m["name"], "widget-factory");
        assert_eq!(m["url"], "https://github.com/acme");
        assert_eq!(m["redirect_url"], "https://github.com/acme");
        assert_eq!(m["hook_attributes"]["active"], false);
        assert_eq!(m["default_events"], serde_json::json!([]));
        assert_eq!(m["public"], false);
        assert_eq!(
            m["default_permissions"],
            serde_json::json!({
                "contents": "write",
                "issues": "write",
                "pull_requests": "write",
                "metadata": "read",
                "actions": "read",
                "checks": "read",
                "statuses": "read"
            })
        );
        assert!(m["default_permissions"].get("workflows").is_none());
    }

    #[test]
    fn org_and_user_form_urls() {
        assert_eq!(
            form_url("acme", true),
            "https://github.com/organizations/acme/settings/apps/new"
        );
        assert_eq!(
            form_url("alice", false),
            "https://github.com/settings/apps/new"
        );
    }

    #[test]
    fn data_link_posts_on_load() {
        let manifest = json(
            "a \"b\" <c> & 'd'",
            "https://github.com/acme",
            "https://github.com/acme",
        );
        let action = "https://github.com/organizations/acme/settings/apps/new";
        let html = decoded(&data_link(&manifest, action, "s123"));
        assert_eq!(html, page(&manifest, action, "s123"));
        assert!(html.contains(r#"method="post""#), "{html}");
        assert!(
            html.contains(&format!(r#"action="{action}?state=s123""#)),
            "{html}"
        );
        let value = html
            .split(r#"name="manifest" value=""#)
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("a manifest field");
        assert_eq!(unescaped(value), manifest);
        assert!(html.contains("submit()"), "{html}");
    }

    #[test]
    fn code_from_url_or_bare() {
        assert_eq!(
            code_of("https://github.com/acme?code=abc123&state=s").as_deref(),
            Some("abc123")
        );
        assert_eq!(code_of("abc123").as_deref(), Some("abc123"));
        assert_eq!(code_of("  abc123\n").as_deref(), Some("abc123"));
        assert_eq!(code_of("https://x?state=s"), None);
        assert_eq!(code_of("abc 123"), None);
        assert_eq!(code_of("abc;rm"), None);
        assert_eq!(code_of(""), None);
        assert_eq!(code_of(&"a".repeat(101)), None);
        assert_eq!(code_of(&"a".repeat(100)).map(|c| c.len()), Some(100));
        assert_eq!(code_of("https://github.com/acme?code=a%2Fb&state=s"), None);
    }

    #[test]
    fn a_pasted_url_must_carry_this_runs_state() {
        let s = "s".repeat(40);
        let url = format!("https://github.com/acme?code=abc123&state={s}");
        assert_eq!(checked_code(&url, &s), Ok("abc123".to_string()));
        assert_eq!(checked_code("abc123", &s), Ok("abc123".to_string()));
        assert!(checked_code(&url, &"t".repeat(40)).is_err());
        assert!(checked_code("https://github.com/acme?code=abc123", &s).is_err());
        assert!(checked_code("https://github.com/acme?state=x", &s).is_err());
        assert!(checked_code("", &s).is_err());
        let fragment = format!("https://github.com/acme?state={s}&code=abc123#top");
        assert_eq!(checked_code(&fragment, &s), Ok("abc123".to_string()));
    }

    #[test]
    fn each_run_has_its_own_long_state() {
        let a = new_state().unwrap();
        let b = new_state().unwrap();
        assert!(a.len() >= 32, "{a}");
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn a_conversion_answer_gives_the_id_slug_and_key() {
        let m = made(br#"{"id":9,"slug":"acme-factory","pem":"-----BEGIN RSA PRIVATE KEY-----\nx\n-----END RSA PRIVATE KEY-----\n","client_secret":"cs","webhook_secret":"ws"}"#).unwrap();
        assert_eq!(m.id, "9");
        assert_eq!(m.slug, "acme-factory");
        assert!(m.pem.starts_with("-----BEGIN"));
        assert!(made(br#"{"id":9,"slug":"acme-factory"}"#).is_none());
        assert!(made(br#"{"id":9,"slug":"a b","pem":"-----BEGIN x"}"#).is_none());
        assert!(made(br#"{"slug":"a","pem":"-----BEGIN x"}"#).is_none());
        assert!(made(br#"{"id":9,"slug":"a","pem":"not a key"}"#).is_none());
    }

    #[test]
    fn the_key_is_new_and_600() {
        let d = tehas_core::testdir::TestDir::new("manifest-key");
        let f = d.join("github-app.pem");
        write_key(&f, "-----BEGIN x\n").unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "-----BEGIN x\n");
        assert_eq!(
            fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(write_key(&f, "-----BEGIN y\n").is_err(), "never over a key");
        assert!(key_can_go(&f).is_err());
        assert!(key_can_go(&d.join("other.pem")).is_ok());
        std::os::unix::fs::symlink(d.join("nowhere"), d.join("dangling.pem")).unwrap();
        assert!(key_can_go(&d.join("dangling.pem")).is_err());
        assert_eq!(fs::read_to_string(&f).unwrap(), "-----BEGIN x\n");
    }

    #[test]
    fn the_page_is_600_and_never_through_a_link() {
        let d = tehas_core::testdir::TestDir::new("manifest-page");
        let f = d.join("new-app.html");
        write_page(&f, "<p>one</p>").unwrap();
        write_page(&f, "<p>two</p>").unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "<p>two</p>");
        assert_eq!(
            fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let target = d.join("elsewhere");
        fs::write(&target, "kept").unwrap();
        let link = d.join("linked.html");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(write_page(&link, "<p>x</p>").is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "kept");
        let real = d.join("real");
        fs::create_dir(&real).unwrap();
        let dir_link = d.join("dirlink");
        std::os::unix::fs::symlink(&real, &dir_link).unwrap();
        assert!(write_page(&dir_link.join("new-app.html"), "<p>x</p>").is_err());
        assert!(!real.join("new-app.html").exists());
    }
}
