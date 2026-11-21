//! One build of the wiki (docs/specs/2026-11-21-wiki.md, "The run"): plan on
//! the host, let the writer change the pages, check, render and publish. The
//! writer is the model's container; a test gives its own. A build that fails
//! leaves the current one served and says why in `<home>/wiki/failure.json`.

use super::check::{self, Pages};
use super::collect::{self, Head};
use super::publish;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

/// What a build is asked to do and where.
pub struct Ask<'a> {
    pub home: &'a Path,
    pub checkout: &'a Path,
    pub branch: &'a str,
    /// Read the whole history and let every page change.
    pub full: bool,
    /// Today's date, `YYYY-MM-DD`, and the time now, RFC 3339.
    pub date: &'a str,
    pub now: &'a str,
}

/// What the writer is given: the stage to change, the raw sources, the commit.
pub struct Work<'a> {
    pub stage: &'a Path,
    pub raw: &'a Path,
    pub head: &'a Head,
}

#[derive(Debug, PartialEq)]
pub enum Done {
    /// Main has not moved since the current build.
    UpToDate(String),
    Built { id: String, changed: usize },
}

fn current(wiki: &Path) -> (Option<String>, Pages) {
    let rec = fs::read_to_string(wiki.join("current/build.json"))
        .ok()
        .and_then(|t| super::build_of(&t));
    let commit = rec.and_then(|r| r["commit"].as_str().map(str::to_string));
    let pages = publish::read_built(&wiki.join("current")).unwrap_or_default();
    (commit, pages)
}

fn fail(wiki: &Path, now: &str, id: &str, why: &[String]) -> String {
    let rec = json!({"at": now, "id": id, "why": why});
    let _ = fs::write(wiki.join("failure.json"), format!("{rec}\n"));
    why.join("; ")
}

/// Runs one build. Err is why it failed, in one line.
pub fn run(ask: &Ask, write: impl FnOnce(&Work) -> Result<(), String>) -> Result<Done, String> {
    let wiki = ask.home.join("wiki");
    fs::create_dir_all(&wiki).map_err(|e| format!("{}: {e}", wiki.display()))?;
    let main = format!("refs/remotes/origin/{}", ask.branch);
    let head = collect::head(ask.checkout, &main).ok_or_else(|| format!("no {main} in {}", ask.checkout.display()))?;
    let (built, before) = current(&wiki);
    if built.as_deref() == Some(head.commit.as_str()) && !ask.full {
        return Ok(Done::UpToDate(head.commit));
    }
    let id = super::id(ask.date, &head.commit).ok_or("the date or the commit is not of its form")?;
    let after = built
        .as_deref()
        .filter(|b| !ask.full && collect::follows(ask.checkout, b, &head.commit));
    let stage: PathBuf = wiki.join(format!("{id}.new"));
    let raw = wiki.join(format!("{id}.raw"));
    let _ = fs::remove_dir_all(&stage);
    let _ = fs::remove_dir_all(&raw);
    let io = |e: std::io::Error| fail(&wiki, ask.now, &id, &[e.to_string()]);
    fs::create_dir_all(&stage).map_err(io)?;
    let mut start = before.clone();
    start.entry("SCHEMA.md".into()).or_insert_with(|| publish::SCHEMA.to_string());
    publish::seed(&stage, &start).map_err(io)?;
    let dropped = collect::write(ask.checkout, &raw, &head, after, &before).map_err(io)?;
    for hash in &dropped {
        eprintln!("tehasbot wiki: commit {hash} left out of the raw sources: its text looks like a credential");
    }
    let wrote = write(&Work {
        stage: &stage,
        raw: &raw,
        head: &head,
    });
    let result = wrote
        .map_err(|e| vec![e])
        .and_then(|()| publish::read(&stage))
        .and_then(|pages| {
            // Against the stage as it was given, so the schema tehas seeded is
            // no page the model changed.
            let why = check::check(&pages, &start, &head.commit);
            if why.is_empty() {
                Ok(pages)
            } else {
                Err(why)
            }
        });
    let pages = match result {
        Ok(p) => p,
        Err(why) => {
            let _ = fs::remove_dir_all(&stage);
            return Err(fail(&wiki, ask.now, &id, &why));
        }
    };
    let changed = publish::changed(&pages, &before);
    let rec = publish::record(&id, &head, ask.now, pages.len(), changed);
    publish::render_all(&stage, &id, &pages)
        .and_then(|()| fs::write(stage.join("build.json"), format!("{rec}\n")))
        .and_then(|()| publish::put(&wiki, &stage, &id))
        .map_err(io)?;
    let _ = fs::remove_file(wiki.join("failure.json"));
    Ok(Done::Built { id, changed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tehas_core::testdir::TestDir;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo(d: &TestDir) -> PathBuf {
        let r = d.join("repo");
        fs::create_dir_all(&r).unwrap();
        git(&r, &["init", "-q", "-b", "main"]);
        git(&r, &["commit", "-q", "--allow-empty", "-m", "One"]);
        git(&r, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        r
    }

    fn head8(r: &Path) -> String {
        collect::head(r, "HEAD").unwrap().commit[..8].to_string()
    }

    /// A writer that does what the schema asks: a page, the index, one entry.
    fn writer(text: &'static str) -> impl FnOnce(&Work) -> Result<(), String> {
        move |w: &Work| {
            let v = &w.head.commit[..8];
            let page = |t: &str, b: &str| format!("---\ntitle: {t}\nverified_at: {v}\n---\n{b}");
            fs::create_dir_all(w.stage.join("modules")).unwrap();
            fs::write(w.stage.join("modules/a.md"), page("A", text)).unwrap();
            fs::write(
                w.stage.join("index.md"),
                page("Index", "- [schema](SCHEMA.md)\n- [log](log.md)\n- [a](modules/a.md)\n"),
            )
            .unwrap();
            let log = fs::read_to_string(w.stage.join("log.md")).unwrap_or_else(|_| "# Log\n".into());
            fs::write(w.stage.join("log.md"), format!("{log}\n## build {v}\n")).unwrap();
            fs::write(w.stage.join("SCHEMA.md"), page("Schema", "Kept.\n")).unwrap();
            assert!(w.raw.join("log.jsonl").exists());
            Ok(())
        }
    }

    fn ask<'a>(home: &'a Path, repo: &'a Path, date: &'a str) -> Ask<'a> {
        Ask {
            home,
            checkout: repo,
            branch: "main",
            full: false,
            date,
            now: "2026-11-21T03:00:00Z",
        }
    }

    #[test]
    fn a_first_build_is_published_and_a_second_on_the_same_commit_is_up_to_date() {
        let d = TestDir::new("wiki-build");
        let r = repo(&d);
        let home = d.join("home");
        let got = run(&ask(&home, &r, "2026-11-21"), writer("First.\n")).unwrap();
        let id = format!("2026-11-21-{}", head8(&r));
        assert_eq!(got, Done::Built { id: id.clone(), changed: 4 });
        let index = fs::read_to_string(home.join("wiki/current/index.html")).unwrap();
        assert!(index.contains("href=\"modules/a.html\""), "{index}");
        let rec: serde_json::Value = serde_json::from_str(&fs::read_to_string(home.join("wiki/current/build.json")).unwrap()).unwrap();
        assert_eq!(rec["url"], format!("/wiki/{id}/"));
        assert!(super::super::build_of(&rec.to_string()).is_some());
        assert!(matches!(run(&ask(&home, &r, "2026-11-22"), writer("x")).unwrap(), Done::UpToDate(_)));
    }

    #[test]
    fn a_failed_check_keeps_the_current_build_and_says_why() {
        let d = TestDir::new("wiki-fail");
        let r = repo(&d);
        let home = d.join("home");
        run(&ask(&home, &r, "2026-11-21"), writer("First.\n")).unwrap();
        git(&r, &["commit", "-q", "--allow-empty", "-m", "Two"]);
        git(&r, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let bad = |w: &Work| -> Result<(), String> {
            fs::write(w.stage.join("stray.md"), "no front matter").unwrap();
            Ok(())
        };
        let why = run(&ask(&home, &r, "2026-11-22"), bad).unwrap_err();
        assert!(why.contains("stray.md"), "{why}");
        let still = fs::read_link(home.join("wiki/current")).unwrap();
        assert!(still.to_string_lossy().starts_with("2026-11-21-"));
        assert!(fs::read_to_string(home.join("wiki/failure.json")).unwrap().contains("stray.md"));
        assert!(!home.join(format!("wiki/2026-11-22-{}.new", head8(&r))).exists());
        let next = run(&ask(&home, &r, "2026-11-22"), writer("Second.\n")).unwrap();
        assert!(matches!(next, Done::Built { changed: 4, .. }), "{next:?}");
        assert!(!home.join("wiki/failure.json").exists());
    }

    #[test]
    fn a_writer_that_fails_publishes_nothing() {
        let d = TestDir::new("wiki-writer");
        let r = repo(&d);
        let home = d.join("home");
        let why = run(&ask(&home, &r, "2026-11-21"), |_: &Work| Err("the model stopped".to_string())).unwrap_err();
        assert_eq!(why, "the model stopped");
        assert!(!home.join("wiki/current").exists());
    }
}
