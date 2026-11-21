//! A build from its stage to `<home>/wiki/<id>/`: the pages read back from the
//! directory the model wrote, checked, rendered and moved into place, then
//! `current` pointed at it and the builds past retention removed. A build
//! that fails its checks changes nothing that is served.

use super::check::{self, Pages};
use super::collect::Head;
use super::render;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::Path;

/// The builds kept beside the last of each release.
pub const KEEP: usize = 10;

/// The layout and stylesheet of every page, and the schema a first build
/// starts from.
pub const LAYOUT: &str = include_str!("../../../host/wiki/layout.html");
pub const CSS: &str = include_str!("../../../host/wiki/wiki.css");
pub const SCHEMA: &str = include_str!("../../../host/wiki/SCHEMA.md");

/// The pages under `dir`, by their paths. A link, a file that is not a page's
/// Markdown, or a directory deeper than a page may be, is a reason the build
/// fails: nothing of a build is read but what the check can hold.
pub fn read(dir: &Path) -> Result<Pages, Vec<String>> {
    read_in(dir, false)
}

/// The Markdown of a published build, past what publishing added beside it.
pub fn read_built(dir: &Path) -> Result<Pages, Vec<String>> {
    read_in(dir, true)
}

fn read_in(dir: &Path, built: bool) -> Result<Pages, Vec<String>> {
    let mut pages = Pages::new();
    let mut why = Vec::new();
    let mut total = 0usize;
    walk(dir, "", 0, built, &mut pages, &mut why, &mut total);
    if why.is_empty() {
        Ok(pages)
    } else {
        Err(why)
    }
}

fn walk(
    dir: &Path,
    at: &str,
    depth: usize,
    built: bool,
    pages: &mut Pages,
    why: &mut Vec<String>,
    total: &mut usize,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => return why.push(format!("{}: {e}", dir.display())),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if at.is_empty() { name.clone() } else { format!("{at}/{name}") };
        let Ok(kind) = entry.file_type() else {
            why.push(format!("{path}: cannot be read"));
            continue;
        };
        let added = built && (name.ends_with(".html") || path == "wiki.css" || path == "build.json");
        if added && kind.is_file() {
            continue;
        }
        if kind.is_symlink() {
            why.push(format!("{path}: a link"));
        } else if kind.is_dir() && depth < 3 {
            walk(&entry.path(), &path, depth + 1, built, pages, why, total);
        } else if kind.is_file() && check::name_ok(&path) {
            match fs::read(entry.path()) {
                Ok(bytes) if bytes.len() > check::MAX_PAGE => {
                    why.push(format!("{path}: {} bytes, more than {}", bytes.len(), check::MAX_PAGE))
                }
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => {
                        *total += text.len();
                        if pages.len() >= check::MAX_PAGES || *total > check::MAX_TOTAL {
                            why.push("more pages or bytes than a build may hold".to_string());
                            return;
                        }
                        pages.insert(path, text);
                    }
                    Err(_) => why.push(format!("{path}: not UTF-8")),
                },
                Err(e) => why.push(format!("{path}: {e}")),
            }
        } else {
            why.push(format!("{path}: not a page of the wiki"));
        }
    }
}

/// Writes `pages` as Markdown under `dir`, for the model to start from.
pub fn seed(dir: &Path, pages: &Pages) -> io::Result<()> {
    for (path, text) in pages {
        let file = dir.join(path);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file, text)?;
    }
    Ok(())
}

/// The build's own record, `build.json`.
pub fn record(id: &str, head: &Head, built_at: &str, pages: usize, changed: usize) -> Value {
    json!({
        "id": id,
        "commit": head.commit,
        "commitDate": head.date,
        "describe": head.describe,
        "builtAt": built_at,
        "pages": pages,
        "changed": changed,
        "url": format!("/wiki/{id}/"),
    })
}

/// The pages a build changed against the build before.
pub fn changed(pages: &Pages, before: &Pages) -> usize {
    let gone = before.keys().filter(|k| !pages.contains_key(*k)).count();
    pages.iter().filter(|(k, v)| before.get(*k) != Some(*v)).count() + gone
}

/// Renders every page into `dir`, beside its Markdown, with the stylesheet.
pub fn render_all(dir: &Path, id: &str, pages: &Pages) -> io::Result<()> {
    for (path, text) in pages {
        let (title, verified, body) = match check::front(text) {
            Ok((f, body)) => (f.title, f.verified_at, body),
            Err(_) => ("Log".to_string(), String::new(), text.as_str()),
        };
        let html = render::page(
            LAYOUT,
            &render::Fill {
                title: &title,
                body: &render::body(body),
                build: id,
                verified: &verified,
                up: &render::up(path),
            },
        );
        fs::write(dir.join(render::html_name(path)), html)?;
    }
    fs::write(dir.join("wiki.css"), CSS)
}

/// The tag a build's describe names: `v1.2.0` of `v1.2.0-5-g68ffab1d`, or
/// the whole word when it is a tag itself; None for a bare commit.
pub fn tag_of(describe: &str) -> Option<&str> {
    let parts: Vec<&str> = describe.rsplitn(3, '-').collect();
    if let [g, n, tag] = parts[..] {
        if g.starts_with('g') && n.bytes().all(|b| b.is_ascii_digit()) && !n.is_empty() {
            return Some(tag);
        }
    }
    (describe.contains('.') || describe.starts_with('v')).then_some(describe)
}

/// The builds to keep of `(id, builtAt, describe)`: the last [`KEEP`] by
/// when they were built, and the last of each release.
pub fn keep(builds: &[(String, String, String)]) -> BTreeSet<String> {
    let mut by_time: Vec<&(String, String, String)> = builds.iter().collect();
    by_time.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
    let mut kept: BTreeSet<String> = by_time.iter().take(KEEP).map(|b| b.0.clone()).collect();
    let mut tags = BTreeSet::new();
    for b in by_time {
        if let Some(tag) = tag_of(&b.2) {
            if tags.insert(tag.to_string()) {
                kept.insert(b.0.clone());
            }
        }
    }
    kept
}

/// Moves the rendered stage into place as `<wiki>/<id>`, points `current` at
/// it and removes the builds past retention and their raw sources.
pub fn put(wiki: &Path, stage: &Path, id: &str) -> io::Result<()> {
    let to = wiki.join(id);
    if to.exists() {
        let old = wiki.join(format!("{id}.old"));
        let _ = fs::remove_dir_all(&old);
        fs::rename(&to, &old)?;
        fs::rename(stage, &to)?;
        let _ = fs::remove_dir_all(&old);
    } else {
        fs::rename(stage, &to)?;
    }
    let link = wiki.join("current.new");
    let _ = fs::remove_file(&link);
    symlink(id, &link)?;
    fs::rename(&link, wiki.join("current"))?;
    let mut builds = Vec::new();
    for e in fs::read_dir(wiki)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !super::is_id(&name) {
            continue;
        }
        let rec = fs::read_to_string(e.path().join("build.json"))
            .ok()
            .and_then(|t| super::build_of(&t))
            .unwrap_or(Value::Null);
        let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
        builds.push((name, s("builtAt"), s("describe")));
    }
    let kept = keep(&builds);
    for (name, _, _) in builds.iter().filter(|b| !kept.contains(&b.0) && b.0 != id) {
        let _ = fs::remove_dir_all(wiki.join(name));
        let _ = fs::remove_dir_all(wiki.join(format!("{name}.raw")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    #[test]
    fn a_stage_with_a_link_or_a_stray_file_is_refused() {
        let d = TestDir::new("wiki-read");
        let dir = d.join("stage");
        fs::create_dir_all(dir.join("modules")).unwrap();
        fs::write(dir.join("index.md"), "x").unwrap();
        fs::write(dir.join("modules/a.md"), "y").unwrap();
        assert_eq!(read(&dir).unwrap().len(), 2);
        fs::write(dir.join("run.sh"), "z").unwrap();
        symlink("/etc/passwd", dir.join("modules/b.md")).unwrap();
        let why = read(&dir).unwrap_err();
        assert!(why.contains(&"run.sh: not a page of the wiki".to_string()), "{why:?}");
        assert!(why.contains(&"modules/b.md: a link".to_string()), "{why:?}");
    }

    #[test]
    fn a_release_s_last_build_and_the_last_ten_are_kept() {
        let mut builds = Vec::new();
        for day in 1..=14 {
            let id = format!("2026-11-{day:02}-0000000{}", day % 10);
            let describe = if day <= 3 { format!("v1.0.0-{day}-gabc") } else { format!("v1.1.0-{day}-gabc") };
            builds.push((id, format!("2026-11-{day:02}T03:00:00Z"), describe));
        }
        let kept = keep(&builds);
        assert_eq!(kept.len(), KEEP + 1);
        assert!(kept.contains("2026-11-03-00000003"), "the last of v1.0.0");
        assert!(!kept.contains("2026-11-02-00000002"));
        assert!(kept.contains("2026-11-14-00000004"));
        assert_eq!(tag_of("v1.2.0-5-g68ffab1d"), Some("v1.2.0"));
        assert_eq!(tag_of("v1.2.0-beta.2-5-g68ffab1d"), Some("v1.2.0-beta.2"));
        assert_eq!(tag_of("v1.2.0"), Some("v1.2.0"));
        assert_eq!(tag_of("68ffab1d"), None);
    }

    #[test]
    fn put_moves_the_build_in_and_points_current_at_it() {
        let d = TestDir::new("wiki-put");
        let wiki = d.join("wiki");
        fs::create_dir_all(&wiki).unwrap();
        let id = "2026-11-21-68ffab1d";
        for round in 0..2 {
            let stage = wiki.join(format!("{id}.new"));
            fs::create_dir_all(&stage).unwrap();
            fs::write(stage.join("index.html"), format!("round {round}")).unwrap();
            put(&wiki, &stage, id).unwrap();
            assert_eq!(fs::read_to_string(wiki.join("current/index.html")).unwrap(), format!("round {round}"));
            assert!(!stage.exists());
        }
        assert_eq!(fs::read_link(wiki.join("current")).unwrap(), Path::new(id));
    }

    #[test]
    fn every_page_is_rendered_with_its_title_and_the_stylesheet() {
        let d = TestDir::new("wiki-render");
        let dir = d.join("b");
        fs::create_dir_all(dir.join("modules")).unwrap();
        let mut pages = Pages::new();
        pages.insert("index.md".into(), "---\ntitle: Index\nverified_at: 68ffab1d\n---\n[d](modules/d.md)\n".into());
        pages.insert("modules/d.md".into(), "---\ntitle: <D>\nverified_at: 68ffab1d\n---\nText\n".into());
        pages.insert("log.md".into(), "# Log\n".into());
        render_all(&dir, "2026-11-21-68ffab1d", &pages).unwrap();
        let index = fs::read_to_string(dir.join("index.html")).unwrap();
        assert!(index.contains("href=\"modules/d.html\""), "{index}");
        let d = fs::read_to_string(dir.join("modules/d.html")).unwrap();
        assert!(d.contains("&lt;D&gt;") && d.contains("../wiki.css"), "{d}");
        assert!(dir.join("log.html").exists() && dir.join("wiki.css").exists());
    }
}
