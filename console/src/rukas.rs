//! What pi-rukas this host's cycles run, in live/rukas.json, so health can say
//! when the sandbox image is behind the commit the host follows. tehasbot
//! status writes it at every run, beside live/platform.json, and tehasbot
//! health reads it as `checks.rukas`.
//!
//!   follow         PI_RUKAS_FOLLOW as loaded: instance.env over versions.env
//!   target         the commit it names now, as `build.sh --check` reads it
//!   targetVersion  the release of that commit, for a host that follows one
//!   image          the tehas.rukas label of PI_ENSEMBLE_IMAGE: what cycles run
//!   imageVersion   the version that image's pi-rukas-headless --capabilities
//!                  answers, as build.sh reads it
//!   clone          HEAD of <home>/repos/pi-rukas, so a clone that moved
//!                  without a rebuild shows
//!   behind         image and target both known, not the same commit, and the
//!                  image not a later release on the line followed: an image
//!                  ahead of its target is no drift
//!
//! Upstream is asked at most once an hour (state/rukas-target keeps the last
//! answer), and again at once when the image is on another commit than the
//! answer kept and was not asked for yet: a build from a release newer than
//! the answer gets the answer it was built from at the next run, not an hour
//! later. An image's --capabilities is asked once per image id
//! (state/rukas-image).
//! A read that gets no answer is null, and decides nothing.
//!
//! The same module answers the sandbox build, bootstrap.sh and
//! scripts/tag-release.sh, by the rules of tehas_core::rukas:
//!
//!   console rukas target <PI_RUKAS_FOLLOW> <PI_RUKAS_REPO>
//!     "<commit> <what it is>" on one line. Exit 1 with nothing printed when
//!     upstream does not answer, has no such tag or branch, or the value is
//!     none of release, release:<line>, branch:<name> or commit:<40 hex>.
//!   console rukas image <PI_RUKAS_REPO> <commit>
//!     ghcr.io/<owner>/<name>:sha-<first 12>, lower case. Exit 1 with nothing
//!     printed when the repository is not on GitHub or the commit is not 40
//!     hex digits.
//!
//! Exit 64 for any other words.

use crate::store::bounded;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tehas_core::call;
use tehas_core::jq::{obj, J};
use tehas_core::rukas;
use tehas_core::time;

const FETCH_EVERY: i64 = 3600;
const LIMIT: Duration = Duration::from_secs(60);

/// What the file is read from, outside this host's files.
pub trait Outside {
    /// `git ls-remote` with these arguments.
    fn ls_remote(&mut self, args: &[&str]) -> Option<String>;
    /// The id of `image` and its tehas.rukas label, empty when it has none.
    fn image(&mut self, image: &str) -> Option<(String, String)>;
    /// The --capabilities answer of the pi-rukas in `image`.
    fn capabilities(&mut self, image: &str) -> Option<String>;
    /// The commit the clone in `dir` is at.
    fn head(&mut self, dir: &Path) -> Option<String>;
}

struct Real;

fn ran(program: &str, args: &[&str]) -> Option<String> {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let r = call::run(program, &args, LIMIT);
    r.ok().then_some(r.out)
}

impl Outside for Real {
    fn ls_remote(&mut self, args: &[&str]) -> Option<String> {
        let mut all = vec!["ls-remote"];
        all.extend_from_slice(args);
        ran("git", &all)
    }

    fn image(&mut self, image: &str) -> Option<(String, String)> {
        let format = "{{.Id}} {{index .Config.Labels \"tehas.rukas\"}}";
        let out = ran("docker", &["image", "inspect", "--format", format, image])?;
        let line = out.lines().next()?.trim();
        let (id, label) = line.split_once(' ').unwrap_or((line, ""));
        (!id.is_empty()).then(|| (id.to_string(), label.trim().to_string()))
    }

    fn capabilities(&mut self, image: &str) -> Option<String> {
        ran(
            "docker",
            &[
                "run",
                "--rm",
                "--init",
                "--network",
                "none",
                image,
                "/opt/pi-rukas/bin/pi-rukas-headless",
                "--capabilities",
            ],
        )
    }

    fn head(&mut self, dir: &Path) -> Option<String> {
        let dir = dir.to_str()?;
        ran("git", &["-C", dir, "rev-parse", "--verify", "HEAD"])
    }
}

/// Write live/rukas.json from this host.
pub fn write(home: &Path, live: &Path) {
    let versions = fs::read_to_string(home.join("versions.env")).unwrap_or_default();
    let setting = |name: &str| {
        env::var(name)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| rukas::pinned(&versions, name).to_string())
    };
    let given = Given {
        follow: setting("PI_RUKAS_FOLLOW"),
        repo: setting("PI_RUKAS_REPO"),
        image: env::var("PI_ENSEMBLE_IMAGE").unwrap_or_default(),
    };
    let file = read(home, &given, &mut Real, time::now());
    let to = live.join("rukas.json");
    match file {
        Some(f) => put(&to, &f),
        None => {
            let _ = fs::remove_file(&to);
        }
    }
}

pub struct Given {
    pub follow: String,
    pub repo: String,
    pub image: String,
}

fn commit_shaped(c: &str) -> bool {
    c.len() == 40 && c.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A version read from a host, null when there was none to read. Not
/// `J::text_or_null`, which turns an empty text to null: an empty text here is
/// a version of sorts, and a missing one is the null.
fn text_or_null(s: Option<String>) -> J {
    s.map_or(J::Null, J::Str)
}

/// The file, or None on a host that follows no pi-rukas and has no image or
/// clone of it.
pub fn read(home: &Path, g: &Given, out: &mut dyn Outside, now: i64) -> Option<J> {
    let (image, image_version) = if g.image.is_empty() {
        (None, None)
    } else {
        match out.image(&g.image) {
            Some((id, label)) => {
                let label = commit_shaped(&label).then_some(label);
                (label, image_version(home, &g.image, &id, out, now))
            }
            None => (None, None),
        }
    };
    let clone = out
        .head(&home.join("repos/pi-rukas"))
        .map(|h| h.trim().to_string())
        .filter(|h| commit_shaped(h));
    if g.follow.is_empty() && image.is_none() && clone.is_none() {
        return None;
    }
    let target = if g.follow.is_empty() {
        None
    } else {
        target(home, g, image.as_deref(), out, now)
    };
    let (target, target_version) = match target {
        Some((commit, what)) => (
            Some(commit),
            what.strip_prefix("release ").map(str::to_string),
        ),
        None => (None, None),
    };
    let behind = match (&image, &target) {
        (Some(i), Some(t)) => {
            i != t
                && !ahead(
                    &g.follow,
                    image_version.as_deref(),
                    target_version.as_deref(),
                )
        }
        _ => false,
    };
    Some(obj(vec![
        ("generatedAt", J::str(time::iso(now))),
        (
            "follow",
            if g.follow.is_empty() {
                J::Null
            } else {
                J::str(g.follow.clone())
            },
        ),
        ("target", text_or_null(target)),
        ("targetVersion", text_or_null(target_version)),
        ("image", text_or_null(image)),
        ("imageVersion", text_or_null(image_version)),
        ("clone", text_or_null(clone)),
        ("behind", J::Bool(behind)),
    ]))
}

/// Whether the image is a later release than the target, on the line the host
/// follows (`release` or `release:<line>`). An image built from a release that
/// came out after the target was asked is ahead of it, not behind.
fn ahead(follow: &str, image: Option<&str>, target: Option<&str>) -> bool {
    let line = match follow.strip_prefix("release") {
        Some("") => "",
        Some(rest) => match rest.strip_prefix(':') {
            Some(line) => line,
            None => return false,
        },
        None => return false,
    };
    let (Some(image), Some(target)) = (image, target) else {
        return false;
    };
    let numbers = |v: &str| -> Option<Vec<u64>> {
        let v = v.strip_prefix('v')?;
        if !version_shaped(v) {
            return None;
        }
        v.split('.').map(|p| p.parse().ok()).collect()
    };
    let on_line = |v: &str| line.is_empty() || v.starts_with(&format!("v{line}."));
    match (numbers(image), numbers(target)) {
        (Some(i), Some(t)) => on_line(image) && on_line(target) && i > t,
        _ => false,
    }
}

/// The commit the host follows and what it is, asked upstream at most once an
/// hour, and once more for an image on another commit than the answer kept
/// (`image` in the file is the image the last question was asked for). An
/// answer kept for the same follow and repository stands while a new one
/// cannot be had.
fn target(
    home: &Path,
    g: &Given,
    image: Option<&str>,
    out: &mut dyn Outside,
    now: i64,
) -> Option<(String, String)> {
    let file = home.join("state/rukas-target");
    let kept = fs::read_to_string(&file)
        .ok()
        .and_then(|t| J::parse(&t))
        .filter(|k| {
            k.get("follow").as_str() == Some(g.follow.as_str())
                && k.get("repo").as_str() == Some(g.repo.as_str())
        });
    let kept_answer = kept.as_ref().and_then(|k| {
        let c = k.get("commit").as_str()?;
        let w = k.get("what").as_str()?;
        commit_shaped(c).then(|| (c.to_string(), w.to_string()))
    });
    let at = kept
        .as_ref()
        .and_then(|k| k.get("at").as_f64())
        .unwrap_or(0.0) as i64;
    // The image moved off the answer kept since upstream was last asked: a
    // build from a release the answer does not know yet.
    let image_moved = kept.as_ref().is_some_and(|k| {
        image.is_some_and(|i| {
            kept_answer.as_ref().map(|(c, _)| c.as_str()) != Some(i)
                && k.get("image").as_str() != Some(i)
        })
    });
    if now - at < FETCH_EVERY && kept.is_some() && !image_moved {
        return kept_answer;
    }
    let answer = rukas::target(&g.follow, &g.repo, |args| out.ls_remote(args)).or(kept_answer);
    let (commit, what) = match &answer {
        Some((c, w)) => (J::str(c.clone()), J::str(w.clone())),
        None => (J::Null, J::Null),
    };
    put(
        &file,
        &obj(vec![
            ("at", J::num(now as f64)),
            ("follow", J::str(g.follow.clone())),
            ("repo", J::str(g.repo.clone())),
            ("commit", commit),
            ("what", what),
            ("image", text_or_null(image.map(str::to_string))),
        ]),
    );
    answer
}

/// v<version> of what the image's pi-rukas answers, asked again when the image
/// is another one, or an hour after a question that got no answer.
fn image_version(
    home: &Path,
    image: &str,
    id: &str,
    out: &mut dyn Outside,
    now: i64,
) -> Option<String> {
    let file = home.join("state/rukas-image");
    let kept = fs::read_to_string(&file)
        .ok()
        .and_then(|t| J::parse(&t))
        .filter(|k| k.get("id").as_str() == Some(id));
    if let Some(k) = &kept {
        if let Some(v) = k.get("version").as_str() {
            return Some(v.to_string());
        }
        let at = k.get("at").as_f64().unwrap_or(0.0) as i64;
        if now - at < FETCH_EVERY {
            return None;
        }
    }
    let version = out
        .capabilities(image)
        .and_then(|a| J::parse(a.trim()))
        .and_then(|a| a.get("version").as_str().map(str::to_string))
        .filter(|v| version_shaped(v))
        .map(|v| format!("v{v}"));
    put(
        &file,
        &obj(vec![
            ("at", J::num(now as f64)),
            ("id", J::str(id)),
            ("version", text_or_null(version.clone())),
        ]),
    );
    version
}

/// 0.14.54: two to four groups of up to six digits, as build.sh takes them.
fn version_shaped(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    (2..=4).contains(&parts.len())
        && parts
            .iter()
            .all(|p| (1..=6).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

fn put(to: &Path, file: &J) {
    let Some(dir) = to.parent() else { return };
    let _ = fs::create_dir_all(dir);
    let name = to.file_name().and_then(|n| n.to_str()).unwrap_or("rukas");
    let new = dir.join(format!("{name}.{}.new", std::process::id()));
    let written =
        fs::write(&new, format!("{}\n", file.pretty())).and_then(|()| fs::rename(&new, to));
    if written.is_err() {
        let _ = fs::remove_file(&new);
    }
}

const USAGE: &str = "usage: console rukas target <PI_RUKAS_FOLLOW> <PI_RUKAS_REPO>\n       console rukas image <PI_RUKAS_REPO> <commit>";

pub fn main(given: &[String]) -> u8 {
    let said = match given {
        [verb, follow, repo] if verb == "target" => {
            rukas::target(follow, repo, ls_remote).map(|(commit, what)| format!("{commit} {what}"))
        }
        [verb, repo, commit] if verb == "image" => rukas::image(repo, commit),
        _ => {
            eprintln!("{USAGE}");
            return 64;
        }
    };
    match said {
        Some(line) => {
            tehas_core::outln!("{line}");
            0
        }
        None => 1,
    }
}

/// `git ls-remote`, bounded: the cleaner runs unattended on a timer, and no
/// caller may wait on a network that does not answer.
pub fn ls_remote(args: &[&str]) -> Option<String> {
    let said = bounded("5", "30", "git")
        .arg("ls-remote")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    said.status
        .success()
        .then(|| String::from_utf8_lossy(&said.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    const NOW: i64 = 1_792_245_620;

    fn hex(c: char) -> String {
        c.to_string().repeat(40)
    }

    #[derive(Default)]
    struct Fake {
        tags: Option<String>,
        image: Option<(String, String)>,
        caps: Option<String>,
        head: Option<String>,
        asked: Vec<String>,
    }

    impl Outside for Fake {
        fn ls_remote(&mut self, args: &[&str]) -> Option<String> {
            self.asked.push(format!("ls-remote {}", args.join(" ")));
            self.tags.clone()
        }
        fn image(&mut self, _: &str) -> Option<(String, String)> {
            self.image.clone()
        }
        fn capabilities(&mut self, image: &str) -> Option<String> {
            self.asked.push(format!("caps {image}"));
            self.caps.clone()
        }
        fn head(&mut self, _: &Path) -> Option<String> {
            self.head.clone()
        }
    }

    fn given(follow: &str) -> Given {
        Given {
            follow: follow.to_string(),
            repo: "https://github.com/up/pi-rukas.git".to_string(),
            image: "tehas/pi-rukas:uid1000".to_string(),
        }
    }

    fn fake(image: &str) -> Fake {
        Fake {
            tags: Some(format!(
                "{}\trefs/tags/v0.14.53\n{}\trefs/tags/v0.14.54\n",
                hex('a'),
                hex('b')
            )),
            image: Some(("sha256:1".to_string(), image.to_string())),
            caps: Some(r#"{"version":"0.14.53"}"#.to_string()),
            head: Some(format!("{}\n", hex('b'))),
            asked: vec![],
        }
    }

    #[test]
    fn an_image_on_an_older_release_is_behind_and_says_both_versions() {
        let d = TestDir::new("rukas-behind");
        let mut f = fake(&hex('a'));
        let got = read(&d, &given("release:0"), &mut f, NOW).unwrap();
        assert_eq!(got.get("follow").text(), "release:0");
        assert_eq!(got.get("target").text(), hex('b'));
        assert_eq!(got.get("targetVersion").text(), "v0.14.54");
        assert_eq!(got.get("image").text(), hex('a'));
        assert_eq!(got.get("imageVersion").text(), "v0.14.53");
        assert_eq!(got.get("clone").text(), hex('b'));
        assert_eq!(got.get("behind"), &J::Bool(true));
    }

    #[test]
    fn an_image_on_the_target_is_not_behind() {
        let d = TestDir::new("rukas-current");
        let mut f = fake(&hex('b'));
        let got = read(&d, &given("release:0"), &mut f, NOW).unwrap();
        assert_eq!(got.get("behind"), &J::Bool(false));
    }

    #[test]
    fn upstream_is_asked_once_an_hour_and_capabilities_once_per_image() {
        let d = TestDir::new("rukas-cache");
        let mut f = fake(&hex('a'));
        read(&d, &given("release:0"), &mut f, NOW).unwrap();
        read(&d, &given("release:0"), &mut f, NOW + 60).unwrap();
        assert_eq!(f.asked.len(), 2, "{:?}", f.asked);
        // Upstream is silent an hour later: the answer kept still stands.
        f.tags = None;
        let got = read(&d, &given("release:0"), &mut f, NOW + FETCH_EVERY).unwrap();
        assert_eq!(f.asked.len(), 3, "{:?}", f.asked);
        assert_eq!(got.get("target").text(), hex('b'));
        // Another image is asked again.
        f.image = Some(("sha256:2".to_string(), hex('b')));
        f.caps = Some(r#"{"version":"0.14.54"}"#.to_string());
        let got = read(&d, &given("release:0"), &mut f, NOW + FETCH_EVERY).unwrap();
        assert_eq!(got.get("imageVersion").text(), "v0.14.54");
        assert_eq!(got.get("behind"), &J::Bool(false));
    }

    /// Upstream as it answers after v0.14.55 came out.
    fn released(f: &mut Fake) {
        f.tags = Some(format!(
            "{}\trefs/tags/v0.14.53\n{}\trefs/tags/v0.14.54\n{}\trefs/tags/v0.14.55\n",
            hex('a'),
            hex('b'),
            hex('c')
        ));
    }

    fn asked_upstream(f: &Fake) -> usize {
        f.asked
            .iter()
            .filter(|a| a.starts_with("ls-remote"))
            .count()
    }

    #[test]
    fn a_build_from_a_release_after_the_cached_target_is_current() {
        let d = TestDir::new("rukas-race");
        // The target is cached at v0.14.54, with the image on it.
        let mut f = fake(&hex('b'));
        f.caps = Some(r#"{"version":"0.14.54"}"#.to_string());
        read(&d, &given("release:0"), &mut f, NOW).unwrap();
        // v0.14.55 comes out, and a build within the hour builds from it.
        released(&mut f);
        f.image = Some(("sha256:2".to_string(), hex('c')));
        f.caps = Some(r#"{"version":"0.14.55"}"#.to_string());
        let got = read(&d, &given("release:0"), &mut f, NOW + 60).unwrap();
        assert_eq!(asked_upstream(&f), 2, "{:?}", f.asked);
        assert_eq!(got.get("target").text(), hex('c'));
        assert_eq!(got.get("targetVersion").text(), "v0.14.55");
        assert_eq!(got.get("imageVersion").text(), "v0.14.55");
        assert_eq!(got.get("behind"), &J::Bool(false));
        // The answer agrees with the image now: not asked again in the hour.
        read(&d, &given("release:0"), &mut f, NOW + 120).unwrap();
        assert_eq!(asked_upstream(&f), 2, "{:?}", f.asked);
    }

    #[test]
    fn an_image_on_a_later_release_of_the_line_is_ahead_not_behind() {
        let d = TestDir::new("rukas-ahead");
        let mut f = fake(&hex('b'));
        read(&d, &given("release:0"), &mut f, NOW).unwrap();
        // Upstream does not answer, so the cached v0.14.54 stands.
        f.tags = None;
        f.image = Some(("sha256:2".to_string(), hex('c')));
        f.caps = Some(r#"{"version":"0.14.55"}"#.to_string());
        let got = read(&d, &given("release:0"), &mut f, NOW + 60).unwrap();
        assert_eq!(got.get("target").text(), hex('b'));
        assert_eq!(got.get("behind"), &J::Bool(false));
    }

    #[test]
    fn an_image_on_an_earlier_release_stays_behind_and_asks_once() {
        let d = TestDir::new("rukas-earlier");
        let mut f = fake(&hex('b'));
        f.caps = Some(r#"{"version":"0.14.54"}"#.to_string());
        read(&d, &given("release:0"), &mut f, NOW).unwrap();
        // The image goes back to v0.14.53 with the target at v0.14.54.
        f.image = Some(("sha256:2".to_string(), hex('a')));
        f.caps = Some(r#"{"version":"0.14.53"}"#.to_string());
        let got = read(&d, &given("release:0"), &mut f, NOW + 60).unwrap();
        assert_eq!(got.get("targetVersion").text(), "v0.14.54");
        assert_eq!(got.get("imageVersion").text(), "v0.14.53");
        assert_eq!(got.get("behind"), &J::Bool(true));
        assert_eq!(asked_upstream(&f), 2, "{:?}", f.asked);
        // Asked once for that image, then the hour again.
        let got = read(&d, &given("release:0"), &mut f, NOW + 120).unwrap();
        assert_eq!(got.get("behind"), &J::Bool(true));
        assert_eq!(asked_upstream(&f), 2, "{:?}", f.asked);
    }

    #[test]
    fn ahead_is_only_a_later_release_on_the_line_followed() {
        let (later, now) = (Some("v0.14.55"), Some("v0.14.54"));
        assert!(ahead("release", later, now));
        assert!(ahead("release:0", later, now));
        assert!(ahead("release:0.14", Some("v0.14.100"), now));
        assert!(!ahead("release:0.14", Some("v0.15.0"), now));
        assert!(!ahead("release", now, later));
        assert!(!ahead("release", now, now));
        assert!(!ahead("releases", later, now));
        assert!(!ahead(&format!("commit:{}", hex('c')), later, now));
        assert!(!ahead("branch:main", later, now));
        assert!(!ahead("release", None, now));
        assert!(!ahead("release", Some("v0.14.55-dirty"), now));
    }

    #[test]
    fn another_follow_asks_upstream_again() {
        let d = TestDir::new("rukas-refollow");
        let mut f = fake(&hex('a'));
        read(&d, &given("release:0"), &mut f, NOW).unwrap();
        let got = read(&d, &given(&format!("commit:{}", hex('c'))), &mut f, NOW + 1).unwrap();
        assert_eq!(got.get("target").text(), hex('c'));
        assert!(got.get("targetVersion").is_null());
        assert_eq!(got.get("behind"), &J::Bool(true));
    }

    #[test]
    fn nothing_known_decides_nothing() {
        let d = TestDir::new("rukas-unknown");
        let mut f = Fake::default();
        let got = read(&d, &given("release:0"), &mut f, NOW).unwrap();
        assert!(got.get("target").is_null());
        assert!(got.get("image").is_null());
        assert_eq!(got.get("behind"), &J::Bool(false));
        // An image with no label is no commit.
        f.image = Some(("sha256:1".to_string(), String::new()));
        f.tags = fake("").tags;
        let got = read(&d, &given("release:0"), &mut f, NOW + FETCH_EVERY).unwrap();
        assert!(got.get("image").is_null());
        assert_eq!(got.get("behind"), &J::Bool(false));
    }

    #[test]
    fn a_host_with_no_pi_rukas_has_no_file() {
        let d = TestDir::new("rukas-none");
        let mut f = Fake::default();
        assert!(read(&d, &given(""), &mut f, NOW).is_none());
        assert!(f.asked.is_empty());
    }

    #[test]
    fn a_capabilities_answer_of_another_shape_is_no_version() {
        let d = TestDir::new("rukas-caps");
        let mut f = fake(&hex('a'));
        f.caps = Some(r#"{"version":"0.14.53-dirty"}"#.to_string());
        let got = read(&d, &given("release:0"), &mut f, NOW).unwrap();
        assert!(got.get("imageVersion").is_null());
        // Not asked again within the hour.
        read(&d, &given("release:0"), &mut f, NOW + 60).unwrap();
        let caps = f.asked.iter().filter(|a| a.starts_with("caps")).count();
        assert_eq!(caps, 1);
    }
}
