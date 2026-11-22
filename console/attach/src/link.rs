//! The one link rule: which addresses tehas ever fetches for an issue. The
//! attachment checker, the dispatcher and the issue feed all read links by it.

use regex::Regex;
use std::sync::LazyLock;

/// Only GitHub's own storage: nothing else is ever fetched for an issue.
///
/// No part of the path past the folder is `.` or `..`, nor holds an escaped
/// `.`, `/` or `\`, so no link names a place outside the folder, however a
/// client or GitHub reads it. A link that does ends before that part.
pub const LINK_RULE: &str = r"https://(?:github\.com/user-attachments/(?:assets|files)/|github\.com/\.*[A-Za-z0-9_-][A-Za-z0-9_.-]*/\.*[A-Za-z0-9_-][A-Za-z0-9_.-]*/(?:assets|files)/[0-9]+/|user-images\.githubusercontent\.com/)\.*(?:[A-Za-z0-9_~&=+-]|%(?:[013-46-9A-Fa-f][0-9A-Fa-f]|2[0-9A-Da-d]|5[0-9ABabD-Fd-f]))(?:[A-Za-z0-9._~?&=+-]|%(?:[013-46-9A-Fa-f][0-9A-Fa-f]|2[0-9A-Da-d]|5[0-9ABabD-Fd-f]))*(?:/\.*(?:[A-Za-z0-9_~&=+-]|%(?:[013-46-9A-Fa-f][0-9A-Fa-f]|2[0-9A-Da-d]|5[0-9ABabD-Fd-f]))(?:[A-Za-z0-9._~?&=+-]|%(?:[013-46-9A-Fa-f][0-9A-Fa-f]|2[0-9A-Da-d]|5[0-9ABabD-Fd-f]))*)*";

static LINK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(LINK_RULE).unwrap());

const GITHUB: &str = "https://github.com/";
const IMAGES: &str = "https://user-images.githubusercontent.com/";

/// One whole link of GitHub's storage and nothing else.
pub fn is_link(s: &str) -> bool {
    LINK_RE
        .find(s)
        .is_some_and(|m| m.start() == 0 && m.end() == s.len())
}

/// The links of a text, each once, in the order written.
pub fn links(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in LINK_RE.find_iter(text) {
        if !out.iter().any(|have| have == m.as_str()) {
            out.push(m.as_str().to_string());
        }
    }
    out
}

/// Wider than [`LINK_RULE`], so no link it takes is missed: the check of a
/// caller that cannot run the attachment checker. The text may be JSON, which
/// may escape a slash.
pub fn may_link(text: &str) -> bool {
    let text = text.replace('\\', "");
    let after = text.find(GITHUB).map(|at| &text[at + GITHUB.len()..]);
    after.is_some_and(|rest| rest.contains("/assets/") || rest.contains("/files/"))
        || text.contains(IMAGES)
}

/// Whether a fetch of `link` carries the token: only to a whole link of an
/// attachment folder on github.com, never to GitHub's storage it redirects to.
pub fn sends_token(link: &str, token: &str) -> bool {
    link.starts_with(GITHUB) && is_link(link) && !token.is_empty()
}

/// Whether `link` is an address outside GitHub's storage that tehas may fetch
/// with a sha256 beside it: https, a host that is one of `hosts` (domains one
/// space apart) exactly, with no port and no account, and a path of plain
/// characters. As in [`LINK_RULE`], no
/// part of the path is `.` or `..` or holds an escaped `.`, `/` or `\`. No
/// fragment. An empty `hosts` allows nothing.
pub fn is_external(link: &str, hosts: &str) -> bool {
    let Some(rest) = link.strip_prefix("https://") else {
        return false;
    };
    let Some((host, tail)) = rest.split_once('/') else {
        return false;
    };
    let (path, query) = tail.split_once('?').unwrap_or((tail, ""));
    !host.is_empty()
        && hosts.split(' ').any(|h| h == host)
        && !path.is_empty()
        && path.split('/').all(plain_part)
        && query.bytes().all(|b| plain(b) || b == b'%' || b == b'/')
}

fn plain(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"._~&=+-".contains(&b)
}

/// One part of an external path: not empty, not `.` or `..`, plain characters
/// and escapes of anything but `.`, `/` and `\`.
fn plain_part(part: &str) -> bool {
    if part.is_empty() || part.bytes().all(|b| b == b'.') {
        return false;
    }
    let b = part.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let Some(hex) = part.get(i + 1..i + 3) else {
                return false;
            };
            match u8::from_str_radix(hex, 16) {
                Ok(c) if hex.bytes().all(|h| h.is_ascii_hexdigit()) && !b"./\\".contains(&c) => {}
                _ => return false,
            }
            i += 3;
        } else if plain(b[i]) {
            i += 1;
        } else {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn an_external_link_is_https_on_a_listed_host_with_a_plain_path() {
        let hosts = "files.example.org cdn.example.net";
        for link in [
            "https://files.example.org/p/proto.zip",
            "https://cdn.example.net/a/b-c_d.e~f.zip",
            "https://files.example.org/proto%20one.zip",
            "https://files.example.org/proto.zip?sig=a%2Fb&exp=1",
        ] {
            assert!(is_external(link, hosts), "{link}");
        }
        for link in [
            "http://files.example.org/p.zip",
            "https://files.example.org",
            "https://files.example.org/",
            "https://other.example.org/p.zip",
            "https://example.org/p.zip",
            "https://sub.files.example.org/p.zip",
            "https://files.example.org.evil.example/p.zip",
            "https://Files.example.org/p.zip",
            "https://files.example.org:8443/p.zip",
            "https://user@files.example.org/p.zip",
            "https://evil.example@files.example.org/p.zip",
            "https://files.example.org/../p.zip",
            "https://files.example.org/a/./p.zip",
            "https://files.example.org/a//p.zip",
            "https://files.example.org/%2e%2e/p.zip",
            "https://files.example.org/a%2Fb.zip",
            "https://files.example.org/a%5cb.zip",
            "https://files.example.org/a%2",
            "https://files.example.org/a%zz.zip",
            "https://files.example.org/p.zip#x",
            "https://files.example.org/p zip",
            "https://files.example.org/p\"x.zip",
            "https://files.example.org/p.zip?a=<b>",
        ] {
            assert!(!is_external(link, hosts), "{link}");
        }
        assert!(!is_external("https://files.example.org/p.zip", ""));
        assert!(!is_external("https:///p.zip", " "));
    }

    #[test]
    fn a_text_gives_each_link_once_in_its_order() {
        let text = "a https://github.com/user-attachments/files/2/b.md, then https://user-images.githubusercontent.com/1/x.png and https://github.com/user-attachments/files/2/b.md again https://github.com/o/r/assets/9/c.png";
        assert_eq!(
            links(text),
            [
                "https://github.com/user-attachments/files/2/b.md",
                "https://user-images.githubusercontent.com/1/x.png",
                "https://github.com/o/r/assets/9/c.png",
            ]
        );
    }

    #[test]
    fn only_a_whole_link_is_a_link() {
        assert!(is_link("https://github.com/user-attachments/assets/1"));
        assert!(!is_link(" https://github.com/user-attachments/assets/1"));
        assert!(!is_link("https://github.com/user-attachments/assets/1 x"));
        assert!(!is_link("https://example.com/user-attachments/assets/1"));
        assert!(!is_link("http://github.com/user-attachments/assets/1"));
    }

    #[test]
    fn the_wide_check_takes_every_link_the_rule_takes() {
        for text in [
            "https://github.com/user-attachments/assets/1",
            "https://github.com/o/r/files/1/a.md",
            "https://user-images.githubusercontent.com/1/x.png",
            r#"{"body":"https:\/\/github.com\/user-attachments\/assets\/x"}"#,
        ] {
            assert!(may_link(text), "{text}");
        }
        for text in ["", "no link", "https://example.com/assets/1"] {
            assert!(!may_link(text), "{text}");
        }
    }

    #[test]
    fn the_token_goes_only_to_a_whole_link_on_github() {
        let link = "https://github.com/user-attachments/files/1/a.md";
        assert!(sends_token(link, "t"));
        assert!(!sends_token(link, ""));
        assert!(!sends_token(
            "https://user-images.githubusercontent.com/1/x.png",
            "t"
        ));
        assert!(!sends_token(&format!("{link} x"), "t"));
    }

    /// The rule is written once in the workspace: every reader of links asks
    /// this module.
    #[test]
    fn the_rule_is_written_once() {
        fn walk(dir: &Path, found: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                let name = entry.file_name();
                if path.is_dir() {
                    if name != "target" && name != "node_modules" {
                        walk(&path, found);
                    }
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = fs::read_to_string(&path).unwrap_or_default();
                    if text.contains(r"user-images\.githubusercontent\.com/)") {
                        found.push(path.display().to_string());
                    }
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut found = Vec::new();
        walk(&root, &mut found);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].ends_with("attach/src/link.rs"), "{found:?}");
    }

    /// The links of `text` as jq reads them with the rule of
    /// console/attach/src/link.rs. The rule is read from that file when the
    /// test runs, so this compares two readings and not two copies of one. With
    /// no jq on the host there is nothing to compare, and the Rust side is
    /// pinned by every other case of links here.
    fn links_of_the_rule(text: &str) -> Vec<String> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/link.rs");
        let source = fs::read_to_string(path).expect("console/attach/src/link.rs is there");
        let rule = source
            .lines()
            .find_map(|line| line.strip_prefix("pub const LINK_RULE: &str = r\""))
            .and_then(|tail| tail.strip_suffix("\";"))
            .expect("the rule of the links is in console/attach/src/link.rs");
        let program = format!("def links: [scan({})];", serde_json::Value::from(rule));
        let Ok(answered) = std::process::Command::new("jq")
            .args(["-Rs", "--arg", "t", text])
            .arg(format!("{program} $t | links"))
            .output()
        else {
            return Vec::new();
        };
        assert!(
            answered.status.success(),
            "jq: {}",
            String::from_utf8_lossy(&answered.stderr)
        );
        serde_json::from_slice(&answered.stdout).expect("jq answers a list of links")
    }

    #[test]
    fn only_githubs_own_storage_is_a_link_and_nothing_else() {
        let text = format!(
            "{} {} {} {} {} {}",
            "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555",
            "https://github.com/user-attachments/files/21/proto.html",
            "https://github.com/acme/widget/files/22/style.css",
            "https://acme.github.io/widget/files/23/elsewhere.css",
            "https://user-images.githubusercontent.com/12345/98765-abcd-ef01-2345-6789abcdef01",
            "https://example.com/not-ours.png",
        );
        assert_eq!(
            links(&text),
            [
                "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555",
                "https://github.com/user-attachments/files/21/proto.html",
                "https://github.com/acme/widget/files/22/style.css",
                "https://user-images.githubusercontent.com/12345/98765-abcd-ef01-2345-6789abcdef01",
            ]
        );
        for nothing in [
            "https://github.com/",
            "https://github.com/a/b/other/1/x.html",
            "https://github.com/a/b/files/x/y.html",
            "https://github.com/a/b",
            "https://githubuser-attachments/assets/1",
            "https://example.com/assets/1",
            "https://github.com/a/b/files/1a/x.html",
        ] {
            let none: Vec<String> = Vec::new();
            assert_eq!(links(nothing), none, "{nothing}");
        }
    }

    #[test]
    fn a_name_of_two_parts_may_hold_dots_hyphens_and_underscores() {
        assert_eq!(
            links("https://github.com/ac-me.my_org/w-id.get/files/7/x.js"),
            ["https://github.com/ac-me.my_org/w-id.get/files/7/x.js"]
        );
    }

    #[test]
    fn a_link_ends_before_a_part_that_would_leave_the_folder() {
        let cases = [
            "https://github.com/o/r/files/1/../../raw/main/a.json",
            "https://github.com/o/r/files/1/./a.json",
            "https://github.com/o/r/files/1/%2e%2e/raw/a.json",
            "https://github.com/o/r/files/1/.%2E/raw/a.json",
            "https://github.com/o/r/files/1/..?x=1",
            "https://github.com/o/../files/1/a.json",
            "https://github.com/user-attachments/files/../../o/r/raw/a.json",
            "https://user-images.githubusercontent.com/../x.png",
            "https://github.com/user-attachments/files/1/../../o/r/raw/a.json",
            "https://github.com/o/r/files/1/a/../../../raw/a.json",
            "https://github.com/o/r/files/1/a%2f..%2fb.json",
            "https://github.com/o/r/files/1/a%5C..%5cb.json",
            "https://github.com/o/r/files/1/.hidden.json",
            "https://github.com/o/r/files/1/a..b/...c.json",
            "https://github.com/o.r/r.s/files/1/a%20b.json?x=../y",
        ];
        let text = cases.join(" ");
        let found = links(&text);
        assert_eq!(
            found,
            [
                "https://github.com/user-attachments/files/1",
                "https://github.com/o/r/files/1/a",
                "https://github.com/o/r/files/1/.hidden.json",
                "https://github.com/o/r/files/1/a..b/...c.json",
                "https://github.com/o.r/r.s/files/1/a%20b.json?x=../y",
            ]
        );
        for link in &cases[..12] {
            assert!(!is_link(link), "{link}");
        }
        // jq's scan lists a link each time it is found.
        let mut said = links_of_the_rule(&text);
        said.dedup();
        assert!(said.is_empty() || said == found, "{said:?}");
    }

    #[test]
    fn a_link_ends_where_the_address_does() {
        // The bytes of the link end where a URL's do: ',' and ' ' and ')'
        // stand outside it, a fragment is no part of the address.
        assert_eq!(
            links("x https://github.com/user-attachments/files/1/a.png?foo=bar&x=1,baz, https://github.com/a/b/files/2/c.js#frag y"),
            [
                "https://github.com/user-attachments/files/1/a.png?foo=bar&x=1",
                "https://github.com/a/b/files/2/c.js",
            ]
        );
        // The same cut with jq's own reading of the rule, so the two cannot
        // drift: the line is taken from the file every time this test runs.
        let cases = "x https://github.com/user-attachments/files/1/a.png?foo=bar&x=1,baz, https://github.com/a/b/files/2/c.js#frag y";
        let said = links_of_the_rule(cases);
        assert_eq!(said, links(cases));
        // Markdown's bracket is no URL character, and a link may stand alone.
        assert_eq!(
            links("![shot](https://github.com/user-attachments/files/2/a.png)"),
            ["https://github.com/user-attachments/files/2/a.png"]
        );
    }
}
