//! The checks a build passes before it is published: what the model wrote is
//! held to the wiki's schema (docs/specs/2026-11-21-wiki.md, "The run").
//! Pure: a build is the map of its pages' paths to their text.

use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

pub type Pages = BTreeMap<String, String>;

/// The most pages a build may hold, and the most bytes of one page and of all.
pub const MAX_PAGES: usize = 500;
pub const MAX_PAGE: usize = 64 * 1024;
pub const MAX_TOTAL: usize = 8 * 1024 * 1024;

/// The three pages every build has.
pub const FIXED: [&str; 3] = ["SCHEMA.md", "index.md", "log.md"];

/// A page's front matter.
#[derive(Debug, PartialEq)]
pub struct Front {
    pub title: String,
    pub sources: Vec<String>,
    pub verified_at: String,
}

/// A page's name: up to three directories of `a-z`, `0-9` and `-`, then a
/// file of the same and `.md`; or one of the three fixed pages.
pub fn name_ok(path: &str) -> bool {
    if FIXED.contains(&path) {
        return true;
    }
    let parts: Vec<&str> = path.split('/').collect();
    let word = |p: &str| {
        !p.is_empty()
            && p.len() <= 64
            && p.bytes()
                .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'))
            && !p.starts_with('-')
    };
    let Some((file, dirs)) = parts.split_last() else {
        return false;
    };
    dirs.len() <= 3 && dirs.iter().all(|d| word(d)) && file.strip_suffix(".md").is_some_and(word)
}

/// The front matter of a page and the text after it, or why there is none.
pub fn front(text: &str) -> Result<(Front, &str), String> {
    let rest = text
        .strip_prefix("---\n")
        .ok_or("no front matter: the page does not start with ---")?;
    let end = rest
        .find("\n---\n")
        .ok_or("the front matter has no closing ---")?;
    let (head, body) = (&rest[..end], &rest[end + 5..]);
    let mut title = None;
    let mut sources = Vec::new();
    let mut verified = None;
    for line in head.lines() {
        let Some((key, value)) = line.split_once(':') else {
            return Err(format!("a front matter line is not key: value: {line}"));
        };
        let value = value.trim();
        match key.trim() {
            "title" => title = Some(value.trim_matches('"').to_string()),
            "verified_at" => verified = Some(value.to_string()),
            "sources" => {
                let list = value
                    .strip_prefix('[')
                    .and_then(|v| v.strip_suffix(']'))
                    .ok_or("sources is not a [list]")?;
                sources = list
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            other => return Err(format!("an unknown front matter key: {other}")),
        }
    }
    let title = title
        .filter(|t| !t.is_empty() && t.len() <= 200)
        .ok_or("no title")?;
    let verified_at = verified
        .filter(|v| {
            v.len() >= 8
                && v.len() <= 40
                && v.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
        .ok_or("verified_at is not a commit's hex")?;
    Ok((
        Front {
            title,
            sources,
            verified_at,
        },
        body,
    ))
}

/// The targets of a page's links and images, as the renderer reads them.
pub fn links(text: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Parser, Tag};
    Parser::new_ext(text, super::render::OPTIONS)
        .filter_map(|e| match e {
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => {
                Some(dest_url.to_string())
            }
            _ => None,
        })
        .collect()
}

/// Where a link of `from` points inside the build, or None for a link that
/// leaves it; Err for a link a page may not hold.
pub fn target(from: &str, link: &str) -> Result<Option<String>, String> {
    if link.starts_with("https://") {
        return Ok(None);
    }
    if link.starts_with('#') || link.is_empty() {
        return Ok(None);
    }
    if link.contains(':') || link.starts_with('/') || link.contains('\\') {
        return Err(format!(
            "a link that is not https or to a page of the wiki: {link}"
        ));
    }
    let path = link.split('#').next().unwrap_or("");
    let mut at: Vec<&str> = from.split('/').collect();
    at.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                at.pop()
                    .ok_or_else(|| format!("a link out of the wiki: {link}"))?;
            }
            p => at.push(p),
        }
    }
    Ok(Some(at.join("/")))
}

static SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}|AKIA[0-9A-Z]{16}",
        r"|-----BEGIN [A-Z ]*PRIVATE KEY-----|xox[abprs]-[A-Za-z0-9-]{10,}|sk-[A-Za-z0-9_-]{20,}"
    ))
    .unwrap()
});

/// Whether a text holds what looks like a credential.
pub fn secret(text: &str) -> bool {
    SECRET.is_match(text)
}

/// What the model wrote against the build before it: every reason the build
/// fails, empty when it passes. `head` is the commit the build reads.
pub fn check(pages: &Pages, before: &Pages, head: &str) -> Vec<String> {
    let mut why = Vec::new();
    if pages.len() > MAX_PAGES {
        why.push(format!("{} pages, more than {MAX_PAGES}", pages.len()));
    }
    let total: usize = pages.values().map(String::len).sum();
    if total > MAX_TOTAL {
        why.push(format!("{total} bytes in all, more than {MAX_TOTAL}"));
    }
    for fixed in FIXED {
        if !pages.contains_key(fixed) {
            why.push(format!("no {fixed}"));
        }
    }
    let mut linked: BTreeSet<String> = BTreeSet::new();
    for (path, text) in pages {
        if !name_ok(path) {
            why.push(format!("{path}: not a page's name"));
            continue;
        }
        if text.len() > MAX_PAGE {
            why.push(format!(
                "{path}: {} bytes, more than {MAX_PAGE}",
                text.len()
            ));
        }
        if secret(text) {
            why.push(format!("{path}: holds what looks like a credential"));
        }
        if path != "log.md" {
            match front(text) {
                Ok((f, _)) => {
                    let changed = before.get(path) != Some(text);
                    if changed && !head.starts_with(&f.verified_at) {
                        why.push(format!(
                            "{path}: changed, but verified_at is {} and not the head",
                            f.verified_at
                        ));
                    }
                }
                Err(e) => why.push(format!("{path}: {e}")),
            }
        }
        for link in links(text) {
            match target(path, &link) {
                Ok(Some(to)) if pages.contains_key(&to) => {
                    if path == "index.md" {
                        linked.insert(to);
                    }
                }
                Ok(Some(to)) => why.push(format!("{path}: a link to {to}, which is no page")),
                Ok(None) => {}
                Err(e) => why.push(format!("{path}: {e}")),
            }
        }
    }
    for path in pages
        .keys()
        .filter(|p| p.as_str() != "index.md" && name_ok(p))
    {
        if !linked.contains(path) {
            why.push(format!("index.md does not list {path}"));
        }
    }
    let old = before.get("log.md").map(String::as_str).unwrap_or("");
    match pages.get("log.md") {
        Some(log) if !log.starts_with(old) => {
            why.push("log.md: the entries before this build were changed".to_string())
        }
        Some(log) => {
            let entries = log[old.len()..]
                .lines()
                .filter(|l| l.starts_with("## "))
                .count();
            if entries != 1 {
                why.push(format!("log.md: {entries} new entries, not one"));
            }
        }
        None => {}
    }
    why
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "68ffab1d0123456789abcdef0123456789abcdef";

    fn page(title: &str, body: &str) -> String {
        format!(
            "---\ntitle: {title}\nsources: [console/dispatch/]\nverified_at: 68ffab1d\n---\n{body}"
        )
    }

    fn good() -> Pages {
        let mut p = Pages::new();
        p.insert(
            "SCHEMA.md".into(),
            page("Schema", "How this wiki is kept.\n"),
        );
        p.insert(
            "index.md".into(),
            page(
                "Index",
                "- [Schema](SCHEMA.md)\n- [Log](log.md)\n- [Dispatcher](modules/dispatch.md)\n",
            ),
        );
        p.insert(
            "log.md".into(),
            "# Log\n\n## 2026-11-21 68ffab1d\n\nFirst build.\n".into(),
        );
        p.insert(
            "modules/dispatch.md".into(),
            page("The dispatcher", "See [the index](../index.md).\n"),
        );
        p
    }

    #[test]
    fn a_whole_first_build_passes() {
        assert_eq!(check(&good(), &Pages::new(), HEAD), Vec::<String>::new());
    }

    #[test]
    fn names_are_lower_case_words_at_most_three_deep() {
        for ok in [
            "index.md",
            "SCHEMA.md",
            "a.md",
            "modules/dispatch.md",
            "a/b/c/d.md",
            "releases/v1-2-0.md",
        ] {
            assert!(name_ok(ok), "{ok}");
        }
        for bad in [
            "A.md",
            "a/b/c/d/e.md",
            "../x.md",
            "x.html",
            "a/.md",
            "-a.md",
            "a b.md",
            "Schema.md",
            "",
            "a/",
        ] {
            assert!(!name_ok(bad), "{bad}");
        }
    }

    #[test]
    fn front_matter_takes_its_three_keys() {
        let text = page("T", "body\n");
        let (f, body) = front(&text).unwrap();
        assert_eq!(f.title, "T");
        assert_eq!(f.sources, ["console/dispatch/"]);
        assert_eq!(f.verified_at, "68ffab1d");
        assert_eq!(body, "body\n");
        for bad in [
            "no front",
            "---\ntitle: T\n",
            "---\ntitle: T\nverified_at: zz\n---\n",
            "---\nverified_at: 68ffab1d\n---\n",
            "---\ntitle: T\nverified_at: 68ffab1d\nowner: me\n---\n",
            "---\ntitle: T\nsources: a, b\nverified_at: 68ffab1d\n---\n",
        ] {
            assert!(front(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_link_leaves_only_by_https_and_stays_inside_the_wiki() {
        assert_eq!(
            target("a/b.md", "../c.md").unwrap().as_deref(),
            Some("c.md")
        );
        assert_eq!(
            target("a/b.md", "d.md#part").unwrap().as_deref(),
            Some("a/d.md")
        );
        assert_eq!(target("a.md", "https://example.org/").unwrap(), None);
        assert_eq!(target("a.md", "#here").unwrap(), None);
        for bad in [
            "javascript:alert(1)",
            "http://x/",
            "/etc/passwd",
            "../../x.md",
            "data:text/html,x",
            "file:///x",
        ] {
            assert!(target("a.md", bad).is_err(), "{bad}");
        }
        assert_eq!(
            links(
                "[a](x.md) and [b](<y z.md>) and [c](z.md \"t\") and [d][r]\n\n[r]: javascript:x\n"
            ),
            ["x.md", "y z.md", "z.md", "javascript:x"]
        );
    }

    #[test]
    fn every_page_is_in_the_index_and_every_link_lands() {
        let mut p = good();
        p.insert(
            "concepts/lone.md".into(),
            page("Lone", "Nobody links me.\n"),
        );
        p.insert("concepts/bad.md".into(), page("Bad", "[x](missing.md)\n"));
        let why = check(&p, &Pages::new(), HEAD);
        assert!(
            why.contains(&"index.md does not list concepts/lone.md".to_string()),
            "{why:?}"
        );
        assert!(
            why.contains(
                &"concepts/bad.md: a link to concepts/missing.md, which is no page".to_string()
            ),
            "{why:?}"
        );
    }

    #[test]
    fn a_changed_page_names_the_head_and_an_unchanged_one_may_keep_its_commit() {
        let before = good();
        let mut p = good();
        p.insert(
            "modules/dispatch.md".into(),
            page("The dispatcher", "New text. [i](../index.md)\n").replace("68ffab1d", "0000aaaa"),
        );
        let log = format!("{}\n## 2026-11-22 7c1e09a2\n\nOne page.\n", p["log.md"]);
        p.insert("log.md".into(), log);
        let why = check(&p, &before, "7c1e09a20123456789abcdef0123456789abcdef");
        assert_eq!(
            why,
            ["modules/dispatch.md: changed, but verified_at is 0000aaaa and not the head"]
        );
    }

    #[test]
    fn the_log_only_grows_and_by_one_entry() {
        let before = good();
        let mut p = good();
        p.insert("log.md".into(), "# Log\n\n## rewritten\n".into());
        assert!(check(&p, &before, HEAD)
            .contains(&"log.md: the entries before this build were changed".to_string()));
        let mut p = good();
        p.insert("log.md".into(), format!("{}## a\n## b\n", before["log.md"]));
        assert!(check(&p, &before, HEAD).contains(&"log.md: 2 new entries, not one".to_string()));
        assert!(
            check(&good(), &before, HEAD).contains(&"log.md: 0 new entries, not one".to_string())
        );
    }

    #[test]
    fn a_credential_a_missing_fixed_page_and_a_bad_name_fail() {
        let mut p = good();
        p.remove("SCHEMA.md");
        p.insert("Bad.md".into(), "x".into());
        let token = format!("ghp_{}", "a".repeat(36));
        p.insert(
            "modules/dispatch.md".into(),
            page("D", &format!("{token} [i](../index.md)\n")),
        );
        let why = check(&p, &Pages::new(), HEAD);
        for want in [
            "no SCHEMA.md",
            "Bad.md: not a page's name",
            "modules/dispatch.md: holds what looks like a credential",
        ] {
            assert!(why.contains(&want.to_string()), "{want}: {why:?}");
        }
        assert!(secret("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(!secret("a plain sentence about keys"));
    }
}
