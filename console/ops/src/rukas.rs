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
//!   layer          the tehas.layer label of that image: the sum of the
//!                  instance layer it was built from
//!   instanceLayer  the sum of the instance's sandbox.Dockerfile now, as
//!                  `console sandbox layer` gives it (src/sandbox.rs)
//!   layerMoved     the image read and the sum known, and the two not the
//!                  same, as `build.sh --check` compares them
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
//!   console rukas fetch <clone> <commit>
//!     Nothing when the clone holds the commit. Otherwise it fetches the tags,
//!     then the commit by its id: a release pins a commit, and once upstream
//!     re-points the tag it came from no tag or branch may hold it. Exit 1
//!     with one line on stderr when neither brings it.
//!
//! Exit 64 for any other words.

use crate::store::bounded;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tehas_core::call;
use tehas_core::file::{write_atomic, Temp};
use tehas_core::jq::{obj, J};
use tehas_core::rukas;
use tehas_core::time;

const FETCH_EVERY: i64 = 3600;
const LIMIT: Duration = Duration::from_secs(60);

/// What the file is read from, outside this host's files.
pub trait Outside {
    /// `git ls-remote` with these arguments.
    fn ls_remote(&mut self, args: &[&str]) -> Option<String>;
    /// The id of `image` and its tehas.rukas and tehas.layer labels, each
    /// empty when it has none.
    fn image(&mut self, image: &str) -> Option<(String, String, String)>;
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

    fn image(&mut self, image: &str) -> Option<(String, String, String)> {
        let format = "{{.Id}} {{index .Config.Labels \"tehas.rukas\"}} {{index .Config.Labels \"tehas.layer\"}}";
        let out = ran("docker", &["image", "inspect", "--format", format, image])?;
        Some(labels(out.lines().next()?)).filter(|(id, _, _)| !id.is_empty())
    }

    fn capabilities(&mut self, image: &str) -> Option<String> {
        ran(
            "docker",
            &[
                "run",
                tehas_core::sandbox::NEVER_PULL,
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

/// The id and the two labels of a line `image` asked docker for. One space
/// apart, so a label that is not there leaves two spaces.
fn labels(line: &str) -> (String, String, String) {
    let mut words = line.trim_end().splitn(3, ' ').map(|w| w.trim().to_string());
    let mut next = || words.next().unwrap_or_default();
    (next(), next(), next())
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
        layer: env::var("TEHAS_INSTANCE_DIR")
            .ok()
            .filter(|d| !d.is_empty())
            .map(|d| Path::new(&d).join("sandbox.Dockerfile")),
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
    /// The instance's sandbox.Dockerfile, None when the instance directory
    /// is not known.
    pub layer: Option<PathBuf>,
}

fn text_or_null(s: Option<String>) -> J {
    s.map_or(J::Null, J::Str)
}

/// The file, or None on a host that follows no pi-rukas and has no image or
/// clone of it.
pub fn read(home: &Path, g: &Given, out: &mut dyn Outside, now: i64) -> Option<J> {
    let (image, image_version, layer) = if g.image.is_empty() {
        (None, None, None)
    } else {
        match out.image(&g.image) {
            Some((id, label, layer)) => {
                let label = tehas_core::grammar::commit_id(&label).then_some(label);
                (
                    label,
                    image_version(home, &g.image, &id, out, now),
                    Some(layer),
                )
            }
            None => (None, None, None),
        }
    };
    let clone = out
        .head(&home.join("repos/pi-rukas"))
        .map(|h| h.trim().to_string())
        .filter(|h| tehas_core::grammar::commit_id(h));
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
    // The sum is asked only of an image that was read: no image, no drift.
    let instance_layer = layer.as_ref().and(g.layer.as_deref()).and_then(layer_sum);
    let layer_moved = matches!((&layer, &instance_layer), (Some(h), Some(w)) if h != w);
    let some = |s: Option<String>| text_or_null(s.filter(|s| !s.is_empty()));
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
        ("layer", some(layer)),
        ("instanceLayer", some(instance_layer)),
        ("layerMoved", J::Bool(layer_moved)),
    ]))
}

/// The sum of the instance's layer, empty with no sandbox.Dockerfile, and
/// None when it cannot be read, which decides nothing.
fn layer_sum(file: &Path) -> Option<String> {
    if !file.exists() {
        return Some(String::new());
    }
    crate::sandbox::layer_sum(file).ok()
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
        tehas_core::grammar::commit_id(c).then(|| (c.to_string(), w.to_string()))
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
    let answer = rukas::target(&g.follow, &g.repo, |args| out.ls_remote(args))
        .map(|(c, what)| {
            // A release pins a commit: which release it is, when upstream says.
            let release = (what == "pinned")
                .then(|| out.ls_remote(&["--tags", &g.repo]))
                .flatten()
                .and_then(|refs| rukas::release_of(&refs, &c));
            (c, release.unwrap_or(what))
        })
        .or(kept_answer);
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
    let text = format!("{}\n", file.pretty());
    let _ = write_atomic(to, text.as_bytes(), 0o644, &Temp::Pid);
}

const USAGE: &str = "usage: console rukas target <PI_RUKAS_FOLLOW> <PI_RUKAS_REPO>\n       console rukas image <PI_RUKAS_REPO> <commit>\n       console rukas fetch <clone> <commit>";

/// The fetches are bounded as build.sh bounded its own: five minutes each.
const FETCH_LIMIT: &str = "300";

/// Bring `commit` into the clone in `dir`, or say why it is not there.
pub fn fetch(dir: &Path, commit: &str) -> Result<(), String> {
    if !tehas_core::grammar::commit_id(commit) {
        return Err(format!("{commit} is not a commit id"));
    }
    let want = format!("{commit}^{{commit}}");
    let holds = || {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["cat-file", "-e", &want])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if holds() {
        return Ok(());
    }
    // --force: a tag upstream re-pointed is taken, not refused.
    let mut why = String::new();
    for args in [
        &["fetch", "-q", "--tags", "--force", "origin"][..],
        &["fetch", "-q", "origin", commit],
    ] {
        why = match bounded("10", FETCH_LIMIT, "git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stdin(Stdio::null())
            .output()
        {
            Ok(o) => String::from_utf8_lossy(&o.stderr).trim().to_string(),
            Err(e) => e.to_string(),
        };
        if holds() {
            return Ok(());
        }
    }
    let why = why.lines().last().unwrap_or("").to_string();
    Err(format!(
        "the pi-rukas clone in {} does not hold {commit}, and a fetch of its tags and of the commit by id did not bring it{}",
        dir.display(),
        if why.is_empty() { String::new() } else { format!(": {why}") }
    ))
}

pub fn main(given: &[String]) -> u8 {
    if let [verb, dir, commit] = given {
        if verb == "fetch" {
            return match fetch(Path::new(dir), commit) {
                Ok(()) => 0,
                Err(why) => {
                    eprintln!("{why}");
                    1
                }
            };
        }
    }
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

    fn git(dir: &Path, args: &[&str]) -> String {
        let o = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("git runs");
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    /// An upstream that holds a commit no tag or branch reaches, served by id
    /// as GitHub serves it, and a clone of it made before.
    fn upstream_with_lost_commit(t: &TestDir) -> (PathBuf, String) {
        let work = t.join("work");
        let up = t.join("up.git");
        fs::create_dir_all(&work).unwrap();
        git(&work, &["init", "-q", "-b", "main"]);
        git(&work, &["commit", "-q", "--allow-empty", "-m", "one"]);
        git(&work, &["tag", "v0.1.0"]);
        git(&work, &["commit", "-q", "--allow-empty", "-m", "two"]);
        let lost = git(&work, &["rev-parse", "HEAD"]);
        git(t.path(), &["init", "-q", "--bare", "up.git"]);
        git(&up, &["config", "uploadpack.allowAnySHA1InWant", "true"]);
        let url = format!("file://{}", up.display());
        git(&work, &["push", "-q", &url, "main", "v0.1.0"]);
        git(&up, &["update-ref", "refs/heads/main", "refs/tags/v0.1.0"]);
        git(t.path(), &["clone", "-q", &url, "clone"]);
        (t.join("clone"), lost)
    }

    #[test]
    fn a_commit_on_no_tag_or_branch_is_fetched_by_its_id() {
        let t = TestDir::new("rukas-fetch");
        let (clone, lost) = upstream_with_lost_commit(&t);
        let has = |c: &str| {
            Command::new("git")
                .arg("-C")
                .arg(&clone)
                .args(["cat-file", "-e", &format!("{c}^{{commit}}")])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        };
        assert!(!has(&lost), "the clone starts without the commit");
        assert_eq!(fetch(&clone, &lost), Ok(()));
        assert!(has(&lost));
        // Held already: nothing to fetch, and still no error.
        assert_eq!(fetch(&clone, &lost), Ok(()));
    }

    #[test]
    fn a_commit_upstream_does_not_hold_is_refused_with_why() {
        let t = TestDir::new("rukas-fetch-none");
        let (clone, _) = upstream_with_lost_commit(&t);
        let why = fetch(&clone, &hex('d')).unwrap_err();
        assert!(
            why.starts_with(&format!(
                "the pi-rukas clone in {} does not hold {}, and a fetch",
                clone.display(),
                hex('d')
            )),
            "{why}"
        );
        assert_eq!(
            fetch(&clone, "main"),
            Err("main is not a commit id".to_string())
        );
    }

    #[derive(Default)]
    struct Fake {
        tags: Option<String>,
        image: Option<(String, String, String)>,
        caps: Option<String>,
        head: Option<String>,
        asked: Vec<String>,
    }

    impl Outside for Fake {
        fn ls_remote(&mut self, args: &[&str]) -> Option<String> {
            self.asked.push(format!("ls-remote {}", args.join(" ")));
            self.tags.clone()
        }
        fn image(&mut self, _: &str) -> Option<(String, String, String)> {
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
            layer: None,
        }
    }

    fn fake(image: &str) -> Fake {
        Fake {
            tags: Some(format!(
                "{}\trefs/tags/v0.14.53\n{}\trefs/tags/v0.14.54\n",
                hex('a'),
                hex('b')
            )),
            image: Some(("sha256:1".to_string(), image.to_string(), String::new())),
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
        f.image = Some(("sha256:2".to_string(), hex('b'), String::new()));
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
        f.image = Some(("sha256:2".to_string(), hex('c'), String::new()));
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
        f.image = Some(("sha256:2".to_string(), hex('c'), String::new()));
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
        f.image = Some(("sha256:2".to_string(), hex('a'), String::new()));
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
    fn a_pinned_commit_is_named_by_its_release() {
        let d = TestDir::new("rukas-pinned");
        let mut f = fake(&hex('b'));
        let got = read(&d, &given(&format!("commit:{}", hex('b'))), &mut f, NOW).unwrap();
        assert_eq!(got.get("target").text(), hex('b'));
        assert_eq!(got.get("targetVersion").text(), "v0.14.54");
        assert_eq!(got.get("behind"), &J::Bool(false));
        // Upstream not answering leaves the pin, named by nothing.
        let d = TestDir::new("rukas-pinned-quiet");
        f.tags = None;
        let got = read(&d, &given(&format!("commit:{}", hex('b'))), &mut f, NOW).unwrap();
        assert_eq!(got.get("target").text(), hex('b'));
        assert!(got.get("targetVersion").is_null());
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
        f.image = Some(("sha256:1".to_string(), String::new(), String::new()));
        f.tags = fake("").tags;
        let got = read(&d, &given("release:0"), &mut f, NOW + FETCH_EVERY).unwrap();
        assert!(got.get("image").is_null());
        assert_eq!(got.get("behind"), &J::Bool(false));
    }

    #[test]
    fn the_image_layer_is_compared_with_the_sum_of_sandbox_dockerfile() {
        let d = TestDir::new("rukas-layer");
        let file = d.join("instance/sandbox.Dockerfile");
        let mut g = given("release:0");
        g.layer = Some(file.clone());
        let mut f = fake(&hex('b'));
        // No sandbox.Dockerfile and no label: no layer, nothing moved.
        let got = read(&d, &g, &mut f, NOW).unwrap();
        assert!(got.get("layer").is_null());
        assert!(got.get("instanceLayer").is_null());
        assert_eq!(got.get("layerMoved"), &J::Bool(false));
        // A sandbox.Dockerfile the image was not built from moved.
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "FROM x\n").unwrap();
        let sum = crate::sandbox::layer_sum(&file).unwrap();
        let got = read(&d, &g, &mut f, NOW).unwrap();
        assert!(got.get("layer").is_null());
        assert_eq!(got.get("instanceLayer").text(), sum);
        assert_eq!(got.get("layerMoved"), &J::Bool(true));
        assert_eq!(got.get("behind"), &J::Bool(false));
        // The image built from it is current.
        f.image = Some(("sha256:1".to_string(), hex('b'), sum.clone()));
        let got = read(&d, &g, &mut f, NOW).unwrap();
        assert_eq!(got.get("layer").text(), sum);
        assert_eq!(got.get("layerMoved"), &J::Bool(false));
        // The file changed since the build.
        fs::write(&file, "FROM y\n").unwrap();
        let got = read(&d, &g, &mut f, NOW).unwrap();
        assert_eq!(got.get("layerMoved"), &J::Bool(true));
        // The layer taken away: the image has a layer the instance no longer has.
        fs::remove_file(&file).unwrap();
        let got = read(&d, &g, &mut f, NOW).unwrap();
        assert!(got.get("instanceLayer").is_null());
        assert_eq!(got.get("layerMoved"), &J::Bool(true));
    }

    #[test]
    fn a_layer_that_cannot_be_compared_decides_nothing() {
        let d = TestDir::new("rukas-layer-unknown");
        let mut f = fake(&hex('b'));
        f.image = Some(("sha256:1".to_string(), hex('b'), "sha256:x".to_string()));
        // No instance directory known.
        let got = read(&d, &given("release:0"), &mut f, NOW).unwrap();
        assert_eq!(got.get("layer").text(), "sha256:x");
        assert!(got.get("instanceLayer").is_null());
        assert_eq!(got.get("layerMoved"), &J::Bool(false));
        // No image read: the sum is not asked.
        let mut g = given("release:0");
        g.layer = Some(d.join("sandbox.Dockerfile"));
        fs::write(d.join("sandbox.Dockerfile"), "FROM x\n").unwrap();
        f.image = None;
        let got = read(&d, &g, &mut f, NOW).unwrap();
        assert!(got.get("layer").is_null());
        assert!(got.get("instanceLayer").is_null());
        assert_eq!(got.get("layerMoved"), &J::Bool(false));
        // A sandbox.Dockerfile that cannot be read.
        f.image = Some(("sha256:1".to_string(), hex('b'), "sha256:x".to_string()));
        fs::remove_file(d.join("sandbox.Dockerfile")).unwrap();
        fs::create_dir(d.join("sandbox.Dockerfile")).unwrap();
        let got = read(&d, &g, &mut f, NOW).unwrap();
        assert!(got.get("instanceLayer").is_null());
        assert_eq!(got.get("layerMoved"), &J::Bool(false));
    }

    #[test]
    fn the_labels_are_one_space_apart() {
        let l = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
        assert_eq!(
            labels("sha256:1 abc sha256:2\n"),
            l("sha256:1", "abc", "sha256:2")
        );
        assert_eq!(labels("sha256:1  sha256:2"), l("sha256:1", "", "sha256:2"));
        assert_eq!(labels("sha256:1 abc "), l("sha256:1", "abc", ""));
        assert_eq!(labels("sha256:1 abc"), l("sha256:1", "abc", ""));
        assert_eq!(labels("sha256:1"), l("sha256:1", "", ""));
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
