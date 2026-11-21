//! A plugin's commit fetched, read and built
//! (docs/specs/2026-11-21-plugins-security.md, "The build" and "Granting"):
//! the commit into `~/tehas/plugins/<name>/repo.git`, the manifest, the skill
//! files and the Dockerfile read from it and held to their rules, the base
//! images pulled by digest and refused with an ONBUILD trigger, and the image
//! built from `git archive` with no network. `review` reads; `update` builds
//! and keeps what it came to in `state.json`.

use super::exec::{Cmd, Exec, Git};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tehas_core::file::{write_atomic, Temp};
use tehas_core::plugin::build::{self, DOCKERFILE_MAX, MANIFEST, PULL_LIMIT};
use tehas_core::plugin::manifest::{self, Manifest};
use tehas_core::plugin::state::{State, Word};
use tehas_core::plugin::{compare, dockerfile, skill, Grants, Pin};

pub(super) fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `~/tehas/plugins`.
pub fn root(home: &Path) -> PathBuf {
    home.join("plugins")
}

/// The directory of one plugin.
pub fn dir(home: &Path, name: &str) -> PathBuf {
    root(home).join(name)
}

fn repo(home: &Path, name: &str) -> String {
    dir(home, name).join("repo.git").display().to_string()
}

const STATE: &str = "state.json";

/// What `state.json` of a plugin holds. None: no file, or none this writes.
pub fn read_state(home: &Path, name: &str) -> Option<State> {
    let text = fs::read_to_string(dir(home, name).join(STATE)).ok()?;
    State::read(&text)
}

/// `state.json` written whole, by rename.
pub fn write_state(home: &Path, name: &str, s: &State) -> std::io::Result<()> {
    let d = dir(home, name);
    fs::create_dir_all(&d)?;
    let text = format!("{}\n", s.json());
    write_atomic(&d.join(STATE), text.as_bytes(), 0o644, &Temp::Unique)
}

fn git_local(repo: &str, args: &[&str]) -> Cmd {
    Cmd::git(Git::Local, build::git(repo, args))
}

/// The commit in `repo.git`, fetched when it is not there yet. Err: why not.
pub fn fetched(x: &mut dyn Exec, home: &Path, pin: &Pin, commit: &str) -> Result<(), String> {
    let repo = repo(home, &pin.name);
    let has = |x: &mut dyn Exec| {
        x.run(&git_local(
            &repo,
            &["cat-file", "-e", &format!("{commit}^{{commit}}")],
        ))
        .ok
    };
    if !Path::new(&repo).join("HEAD").is_file() {
        fs::create_dir_all(&repo).map_err(|e| format!("cannot make {repo}: {e}"))?;
        let init = x.run(&Cmd::git(
            Git::Local,
            vec![
                "init".into(),
                "--quiet".into(),
                "--bare".into(),
                repo.clone(),
            ],
        ));
        if !init.ok {
            return Err(format!("git init of {repo} failed: {}", init.err));
        }
    } else if has(x) {
        return Ok(());
    }
    // `timeout` runs git, so the command is given git's environment.
    let f = x.run(&Cmd {
        git: Git::Fetch,
        ..Cmd::new("timeout", build::fetch(&repo, &pin.url, commit))
    });
    if !f.ok {
        return Err(format!("the fetch of {commit} failed: {}", f.err));
    }
    if !has(x) {
        return Err(format!("the fetch did not bring the commit {commit}"));
    }
    Ok(())
}

/// A file of the commit, at most `max` bytes. Ok(None): no such file. Err:
/// one larger, or a read that failed.
fn blob(
    x: &mut dyn Exec,
    repo: &str,
    commit: &str,
    path: &str,
    max: u64,
) -> Result<Option<Vec<u8>>, String> {
    let spec = format!("{commit}:{path}");
    let kind = x.run(&git_local(repo, &["cat-file", "-t", &spec]));
    if !kind.ok {
        return Ok(None);
    }
    if kind.text() != "blob" {
        return Err(format!("{path} is not a file"));
    }
    let size = x.run(&git_local(repo, &["cat-file", "-s", &spec]));
    let n: u64 = size
        .text()
        .parse()
        .map_err(|_| format!("the size of {path} could not be read"))?;
    if n > max {
        return Err(format!("{path} is over {max} bytes"));
    }
    let got = x.run(&git_local(repo, &["cat-file", "blob", &spec]));
    if !got.ok {
        return Err(format!("{path} could not be read: {}", got.err));
    }
    Ok(Some(got.out))
}

/// A skill's file of the fetched commit, read as [`read`] reads it and held
/// to the skill's rules.
pub fn skill_text(
    x: &mut dyn Exec,
    home: &Path,
    name: &str,
    commit: &str,
    path: &str,
) -> Result<String, String> {
    let bytes = blob(x, &repo(home, name), commit, path, skill::BYTES_MAX as u64)?
        .ok_or_else(|| format!("{path}: not in the commit"))?;
    skill::text(&bytes)
        .map(str::to_string)
        .map_err(|why| format!("{path}: {why}"))
}

/// A commit's manifest and Dockerfile as read.
#[derive(Debug, Default)]
pub struct Reading {
    pub manifest: Option<Manifest>,
    /// The manifest as JSON, for what changed since another commit.
    pub raw: Value,
    pub sha256: String,
    pub bases: Vec<String>,
    /// Each rule the commit breaks.
    pub refusals: Vec<String>,
}

impl Reading {
    /// Whether the manifest read now is not the one `kept` records.
    pub fn manifest_sha256_differs(&self, kept: Option<&str>) -> bool {
        kept != Some(self.sha256.as_str())
    }
}

/// The manifest, its skills and its Dockerfile, read from the fetched
/// commit. Err: the manifest could not be read at all.
pub fn read(x: &mut dyn Exec, home: &Path, name: &str, commit: &str) -> Result<Reading, String> {
    let repo = repo(home, name);
    let mut file = |path: &str, max: u64| blob(x, &repo, commit, path, max);
    reading(
        &mut file,
        name,
        &format!("the commit {commit}"),
        "the commit",
    )
}

/// A file of a plugin by its path, at most `max` bytes: Ok(None) when it is
/// not there, Err when it is there and cannot be read or is too large.
pub type File<'a> = dyn FnMut(&str, u64) -> Result<Option<Vec<u8>>, String> + 'a;

/// The manifest, its skills and its Dockerfile, read through `file`, from a
/// fetched commit or a folder: `whole` names it where the manifest is
/// missing, `place` where another file is. Err: no manifest.
pub fn reading(file: &mut File, name: &str, whole: &str, place: &str) -> Result<Reading, String> {
    let bytes = file(MANIFEST, manifest::BYTES_MAX as u64)?
        .ok_or_else(|| format!("{whole} has no {MANIFEST}"))?;
    let mut r = Reading {
        raw: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        sha256: sha256(&bytes),
        ..Reading::default()
    };
    let m = match manifest::read(&bytes, name) {
        Ok(m) => m,
        Err(why) => {
            r.refusals = why
                .into_iter()
                .map(|w| format!("{MANIFEST}: {w}"))
                .collect();
            return Ok(r);
        }
    };
    for s in &m.skills {
        let said = match file(&s.path, skill::BYTES_MAX as u64) {
            Ok(Some(b)) => skill::text(&b).err(),
            Ok(None) => Some(format!("not in {place}")),
            Err(why) => Some(why),
        };
        if let Some(why) = said {
            r.refusals
                .push(format!("skill {}: {}: {why}", s.id, s.path));
        }
    }
    if let Some(build) = &m.build {
        match file(build, DOCKERFILE_MAX) {
            Ok(Some(b)) => {
                let d = dockerfile::read(&b);
                r.refusals
                    .extend(d.refusals.into_iter().map(|w| format!("{build}: {w}")));
                // A name docker would read as a flag.
                for b in d
                    .bases
                    .iter()
                    .filter(|b| !b.starts_with(|c: char| c.is_ascii_alphanumeric()))
                {
                    r.refusals
                        .push(format!("{build}: FROM {b}: not an image's name"));
                }
                r.bases = d.bases;
            }
            Ok(None) => r.refusals.push(format!("{build}: not in {place}")),
            Err(why) => r.refusals.push(why),
        }
    }
    r.manifest = Some(m);
    Ok(r)
}

/// The base images pulled by digest, and each one's ONBUILD triggers. Err:
/// a pull or an inspect that failed.
pub fn bases(x: &mut dyn Exec, bases: &[String]) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut out = Vec::new();
    for b in bases {
        let pull = x.run(&Cmd::new(
            "timeout",
            vec![
                PULL_LIMIT.to_string(),
                "docker".into(),
                "pull".into(),
                "--quiet".into(),
                b.clone(),
            ],
        ));
        if !pull.ok {
            return Err(format!("the pull of {b} failed: {}", pull.err));
        }
        let inspect = x.run(&Cmd::new(
            "docker",
            vec![
                "image".into(),
                "inspect".into(),
                "--format".into(),
                "{{json .Config.OnBuild}}".into(),
                b.clone(),
            ],
        ));
        let triggers = inspect
            .ok
            .then(|| build::onbuild(&inspect.text()))
            .flatten()
            .ok_or_else(|| format!("docker could not say what {b} holds: {}", inspect.err))?;
        out.push((b.clone(), triggers));
    }
    Ok(out)
}

/// The refusal of a base image with an ONBUILD trigger, which would run in
/// the build with no line of the plugin's Dockerfile to show it.
pub fn onbuild_refusals(bases: &[(String, Vec<String>)]) -> Vec<String> {
    bases
        .iter()
        .filter(|(_, t)| !t.is_empty())
        .map(|(b, t)| {
            format!(
                "the base image {b} has ONBUILD triggers, which would run in the build: {}",
                t.join("; ")
            )
        })
        .collect()
}

/// The id of a local image, if Docker has it.
fn image_id(x: &mut dyn Exec, image: &str) -> Option<String> {
    let d = x.run(&Cmd::new(
        "docker",
        vec![
            "image".into(),
            "inspect".into(),
            "--format".into(),
            "{{.Id}}".into(),
            image.into(),
        ],
    ));
    let id = d.text();
    (d.ok && tehas_core::grammar::is_image(&id)).then_some(id)
}

/// The image built from the commit. Ok: its id.
fn built(
    x: &mut dyn Exec,
    home: &Path,
    name: &str,
    commit: &str,
    file: &str,
) -> Result<String, String> {
    let repo = repo(home, name);
    let archive = git_local(&repo, &["archive", "--format=tar", commit]);
    let mut b = Cmd::new("timeout", build::build(name, commit, file));
    b.buildkit = true;
    let done = x.pipe(&archive, &b);
    if !done.ok {
        return Err(format!("the build failed: {}", done.err));
    }
    image_id(x, &build::tag(name, commit)).ok_or_else(|| "the build left no image".to_string())
}

/// The other images of the plugin, which the image of `keep` replaces.
fn drop_others(x: &mut dyn Exec, name: &str, keep: Option<&str>) {
    let listed = x.run(&Cmd::new(
        "docker",
        vec![
            "image".into(),
            "ls".into(),
            "--format".into(),
            "{{.Repository}}:{{.Tag}}".into(),
            build::image_repo(name),
        ],
    ));
    if !listed.ok {
        return;
    }
    let keep = keep.map(|c| build::tag(name, c));
    for image in listed.text().lines() {
        if Some(image) != keep.as_deref() && image.starts_with(&build::image_repo(name)) {
            x.run(&Cmd::new("docker", vec!["rmi".into(), image.into()]));
        }
    }
}

/// The pin fetched, read, held to its grants and built, as `tehas update`
/// does it: what it came to, also written to `state.json`. An image of the
/// commit already built is kept, not built again.
pub fn prepare(x: &mut dyn Exec, home: &Path, pin: &Pin, grants: Option<&Grants>) -> State {
    prepare_view(x, home, pin, grants).0
}

/// [`prepare`], with the label of the manifest's view at the pin when the
/// plugin is ready and its manifest has one.
pub fn prepare_view(
    x: &mut dyn Exec,
    home: &Path,
    pin: &Pin,
    grants: Option<&Grants>,
) -> (State, Option<String>) {
    let mut view = None;
    let s = prepared(x, home, pin, grants, &mut view);
    if let Err(e) = write_state(home, &pin.name, &s) {
        let why = format!("state.json could not be written: {e}");
        return (State::refused(&pin.commit, why), None);
    }
    let view = view.filter(|_| s.state == Word::Ready);
    (s, view)
}

fn prepared(
    x: &mut dyn Exec,
    home: &Path,
    pin: &Pin,
    grants: Option<&Grants>,
    view: &mut Option<String>,
) -> State {
    let c = &pin.commit;
    if let Err(why) = fetched(x, home, pin, c) {
        return State::refused(c, why);
    }
    let r = match read(x, home, &pin.name, c) {
        Ok(r) => r,
        Err(why) => return State::refused(c, why),
    };
    let mut refusals = r.refusals.clone();
    let Some(m) = &r.manifest else {
        return refused(c, &r, refusals);
    };
    let compared = compare(&m.asks, grants, c);
    if !compared.granted() {
        refusals.extend(compared.refusals());
    }
    if !refusals.is_empty() {
        return refused(c, &r, refusals);
    }
    let mut s = State::new(Word::Ready, c);
    view.clone_from(&m.view);
    s.manifest_sha256 = Some(r.sha256.clone());
    s.bases.clone_from(&r.bases);
    let Some(file) = &m.build else {
        return s;
    };
    if let Some(id) = image_id(x, &build::tag(&pin.name, c)) {
        s.image = Some(id);
        return s;
    }
    let mut building = s.clone();
    building.state = Word::Building;
    if let Err(e) = write_state(home, &pin.name, &building) {
        return State::refused(c, format!("state.json could not be written: {e}"));
    }
    let pulled = match bases(x, &r.bases) {
        Ok(p) => p,
        Err(why) => return refused(c, &r, vec![why]),
    };
    let onbuild = onbuild_refusals(&pulled);
    if !onbuild.is_empty() {
        return refused(c, &r, onbuild);
    }
    match built(x, home, &pin.name, c, file) {
        Ok(id) => {
            drop_others(x, &pin.name, Some(c));
            s.image = Some(id);
            s
        }
        Err(why) => refused(c, &r, vec![why]),
    }
}

fn refused(commit: &str, r: &Reading, why: Vec<String>) -> State {
    let mut s = State::refused(commit, why.join("; "));
    s.manifest_sha256 = (!r.sha256.is_empty()).then(|| r.sha256.clone());
    s.bases.clone_from(&r.bases);
    s
}

/// The refs of `repo.git` but the pin's taken out, so what a review fetched
/// goes, then `git gc`. Nothing to do when there is no repository yet.
pub fn prune(x: &mut dyn Exec, home: &Path, pin: &Pin) -> Result<(), String> {
    let repo = repo(home, &pin.name);
    if !Path::new(&repo).join("HEAD").is_file() {
        return Ok(());
    }
    let listed = x.run(&git_local(
        &repo,
        &["for-each-ref", "--format=%(refname)", "refs/tehas/"],
    ));
    if !listed.ok {
        return Err(format!("the refs could not be listed: {}", listed.err));
    }
    let keep = build::keep_ref(&pin.commit);
    for r in listed.text().lines().filter(|r| *r != keep) {
        let d = x.run(&git_local(&repo, &["update-ref", "-d", r]));
        if !d.ok {
            return Err(format!("{r} could not be removed: {}", d.err));
        }
    }
    let gc = x.run(&git_local(&repo, &["gc", "--prune=now", "--quiet"]));
    if !gc.ok {
        return Err(format!("git gc failed: {}", gc.err));
    }
    Ok(())
}

/// A plugin no longer in the settings: its images, its state volume and its
/// directory. Err: what could not be removed.
pub fn remove(x: &mut dyn Exec, home: &Path, name: &str) -> Result<(), String> {
    drop_others(x, name, None);
    let volume = format!("tehas-plugin-{name}-state");
    let has = x.run(&Cmd::new(
        "docker",
        vec!["volume".into(), "inspect".into(), volume.clone()],
    ));
    if has.ok {
        let rm = x.run(&Cmd::new(
            "docker",
            vec!["volume".into(), "rm".into(), volume.clone()],
        ));
        if !rm.ok {
            return Err(format!("docker volume rm {volume} failed: {}", rm.err));
        }
    }
    let d = dir(home, name);
    match fs::symlink_metadata(&d) {
        Ok(m) if m.is_dir() => {
            fs::remove_dir_all(&d).map_err(|e| format!("cannot remove {}: {e}", d.display()))
        }
        Ok(_) => fs::remove_file(&d).map_err(|e| format!("cannot remove {}: {e}", d.display())),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::exec::fake::{fails, ok, Fake};
    use super::*;
    use tehas_core::testdir::TestDir;

    pub const C: &str = "0123456789abcdef0123456789abcdef01234567";
    pub const URL: &str = "https://github.com/tehas-app/tehas-plugin-floorplan";
    pub const NODE: &str =
        "node@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    pub const ID: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    pub fn pin(commit: &str) -> Pin {
        Pin {
            name: "floorplan".into(),
            url: URL.into(),
            commit: commit.into(),
        }
    }

    pub fn manifest(asks: &str) -> String {
        format!(
            r#"{{"plugin": 1, "kind": "batch", "name": "floorplan", "label": "Floorplan", "build": "Dockerfile", "asks": [{asks}], "timeout": 3600}}"#
        )
    }

    pub fn dockerfile() -> String {
        format!("FROM {NODE}\nCOPY . /app\n")
    }

    /// A home whose repo.git is there, so the init is not run, and git and
    /// docker answering as for a good commit: `files` are the commit's.
    pub fn good(name: &str, files: &[(&str, &str)]) -> (TestDir, Fake) {
        let home = TestDir::new(name);
        let repo = dir(&home, "floorplan").join("repo.git");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        let mut x = Fake::new();
        x.on("cat-file -e", ok(""));
        x.on("cat-file -t", fails("no such path"));
        for (path, text) in files {
            x.on(&format!("cat-file -t {C}:{path}"), ok("blob\n"));
            x.on(
                &format!("cat-file -s {C}:{path}"),
                ok(&format!("{}\n", text.len())),
            );
            x.on(&format!("cat-file blob {C}:{path}"), ok(text));
        }
        x.on("docker pull", ok("pulled\n"));
        x.on("{{json .Config.OnBuild}}", ok("null\n"));
        x.on("{{.Id}}", fails("No such image"));
        x.on("docker image ls", ok(""));
        x.on("docker build", ok(""));
        (home, x)
    }

    fn granted(words: &str) -> Grants {
        tehas_core::plugin::grants(&format!("{C} {words}")).unwrap()
    }

    #[test]
    fn a_good_pin_is_fetched_read_pulled_built_and_ready() {
        let m = manifest(r#""product""#);
        let d = dockerfile();
        let (home, mut x) = good(
            "plugin-good",
            &[("tehas-plugin.json", &m), ("Dockerfile", &d)],
        );
        x.times("cat-file -e", fails("missing"), 1);
        x.on("fetch --quiet", ok(""));
        x.on("{{.Id}}", ok(&format!("{ID}\n")));
        x.times("{{.Id}}", fails("No such image"), 1);
        let s = prepare(&mut x, &home, &pin(C), Some(&granted("product")));
        assert_eq!(s.state, Word::Ready, "{s:?}");
        assert_eq!(s.image.as_deref(), Some(ID));
        assert_eq!(s.bases, [NODE]);
        assert_eq!(s.manifest_sha256, Some(sha256(m.as_bytes())));
        assert_eq!(read_state(&home, "floorplan"), Some(s));
        assert!(x.ran_any("fetch --quiet"));
        assert!(x.ran_any(&format!("docker pull --quiet {NODE}")));
        let pipe = x.ran.iter().find(|l| l.contains(" | ")).unwrap();
        assert!(
            pipe.contains(&format!(
                "archive --format=tar {C} | timeout 1800 docker build --network none --cgroup-parent tehas.slice --no-cache --pull=false -f Dockerfile -t tehas-plugin-floorplan:{C} -"
            )),
            "{pipe}"
        );
    }

    #[test]
    fn a_commit_the_fetch_did_not_bring_is_refused() {
        let (home, mut x) = good("plugin-nofetch", &[]);
        x.on("cat-file -e", fails("missing"));
        x.on("fetch --quiet", ok(""));
        let s = prepare(&mut x, &home, &pin(C), None);
        assert_eq!(s.state, Word::Refused);
        assert_eq!(
            s.refusal.as_deref(),
            Some(format!("the fetch did not bring the commit {C}").as_str())
        );
        x.on("fetch --quiet", fails("fatal: unable to access"));
        let s = prepare(&mut x, &home, &pin(C), None);
        assert_eq!(
            s.refusal.as_deref(),
            Some(format!("the fetch of {C} failed: fatal: unable to access").as_str())
        );
        let fetch = x.ran.iter().find(|l| l.contains("fetch --quiet")).unwrap();
        assert!(fetch.starts_with("timeout 600 git -C "), "{fetch}");
        assert!(fetch.contains(
            "-c protocol.allow=never -c protocol.https.allow=always -c http.followRedirects=false"
        ));
        assert!(fetch.ends_with(&format!("{URL} +{C}:refs/tehas/{C}")));
    }

    #[test]
    fn a_first_fetch_makes_the_bare_repository() {
        let home = TestDir::new("plugin-init");
        let mut x = Fake::new();
        x.on("init --quiet --bare", ok(""));
        x.on("fetch", fails("offline"));
        let s = prepare(&mut x, &home, &pin(C), None);
        assert_eq!(s.state, Word::Refused);
        assert!(
            x.ran[0].starts_with("git init --quiet --bare "),
            "{:?}",
            x.ran
        );
        assert!(x.ran[0].ends_with("/plugins/floorplan/repo.git"));
        assert!(!x.ran_any("cat-file -e"), "{:?}", x.ran);
    }

    #[test]
    fn a_missing_manifest_is_a_read_that_failed() {
        let (home, mut x) = good("plugin-nomanifest", &[]);
        let s = prepare(&mut x, &home, &pin(C), None);
        assert_eq!(
            s.refusal.as_deref(),
            Some(format!("the commit {C} has no tehas-plugin.json").as_str())
        );
    }

    #[test]
    fn a_manifest_or_dockerfile_that_breaks_a_rule_refuses_and_builds_nothing() {
        let m = manifest(r#""product""#);
        let bad = "FROM node:22\n";
        let (home, mut x) = good(
            "plugin-rules",
            &[("tehas-plugin.json", &m), ("Dockerfile", bad)],
        );
        let s = prepare(&mut x, &home, &pin(C), Some(&granted("product")));
        assert_eq!(s.state, Word::Refused);
        assert_eq!(
            s.refusal.as_deref(),
            Some("Dockerfile: line 1: FROM node:22: not named by @sha256: digest")
        );
        assert!(!x.ran_any("docker pull"));
        assert!(!x.ran_any("docker build"));
        let flag = format!("FROM -q@sha256:{}\n", "a".repeat(64));
        let (home, mut x) = good(
            "plugin-rules-3",
            &[("tehas-plugin.json", &m), ("Dockerfile", &flag)],
        );
        let s = prepare(&mut x, &home, &pin(C), Some(&granted("product")));
        assert_eq!(
            s.refusal,
            Some(format!(
                "Dockerfile: FROM -q@sha256:{}: not an image's name",
                "a".repeat(64)
            ))
        );
        assert!(!x.ran_any("docker pull"));
        let (home, mut x) = good(
            "plugin-rules-2",
            &[("tehas-plugin.json", r#"{"plugin": 2}"#)],
        );
        let s = prepare(&mut x, &home, &pin(C), None);
        assert!(s.refusal.unwrap().starts_with("tehas-plugin.json: "));
    }

    #[test]
    fn an_ask_not_granted_refuses_the_plugin() {
        let m = manifest(r#""product", "view""#);
        let d = dockerfile();
        let (home, mut x) = good(
            "plugin-ungranted",
            &[("tehas-plugin.json", &m), ("Dockerfile", &d)],
        );
        let s = prepare(&mut x, &home, &pin(C), Some(&granted("product")));
        assert_eq!(s.refusal.as_deref(), Some("view is asked and not granted"));
        let other =
            tehas_core::plugin::grants(&format!("{} product view", "f".repeat(40))).unwrap();
        let s = prepare(&mut x, &home, &pin(C), Some(&other));
        assert_eq!(
            s.refusal.as_deref(),
            Some("the grants are for another commit; product is asked and not granted; view is asked and not granted")
        );
        assert!(!x.ran_any("docker build"));
    }

    #[test]
    fn a_base_with_onbuild_triggers_is_refused_and_named() {
        let m = manifest(r#""product""#);
        let d = dockerfile();
        let (home, mut x) = good(
            "plugin-onbuild",
            &[("tehas-plugin.json", &m), ("Dockerfile", &d)],
        );
        x.on("{{json .Config.OnBuild}}", ok("[\"RUN curl x | sh\"]\n"));
        let s = prepare(&mut x, &home, &pin(C), Some(&granted("product")));
        assert_eq!(
            s.refusal.as_deref(),
            Some(format!("the base image {NODE} has ONBUILD triggers, which would run in the build: RUN curl x | sh").as_str())
        );
        assert!(!x.ran_any("docker build"));
    }

    #[test]
    fn an_image_of_the_commit_already_built_is_kept() {
        let m = manifest(r#""product""#);
        let d = dockerfile();
        let (home, mut x) = good(
            "plugin-kept",
            &[("tehas-plugin.json", &m), ("Dockerfile", &d)],
        );
        x.on("{{.Id}}", ok(&format!("{ID}\n")));
        let s = prepare(&mut x, &home, &pin(C), Some(&granted("product")));
        assert_eq!(s.state, Word::Ready);
        assert_eq!(s.image.as_deref(), Some(ID));
        assert!(!x.ran_any("docker pull"));
        assert!(!x.ran_any("docker build"));
    }

    #[test]
    fn a_failed_build_refuses_and_keeps_the_bases() {
        let m = manifest(r#""product""#);
        let d = dockerfile();
        let (home, mut x) = good(
            "plugin-badbuild",
            &[("tehas-plugin.json", &m), ("Dockerfile", &d)],
        );
        x.on("docker build", fails("RUN npm ci: exit 1"));
        let s = prepare(&mut x, &home, &pin(C), Some(&granted("product")));
        assert_eq!(
            s.refusal.as_deref(),
            Some("the build failed: RUN npm ci: exit 1")
        );
        assert_eq!(s.bases, [NODE]);
        assert_eq!(read_state(&home, "floorplan"), Some(s));
    }

    #[test]
    fn a_plugin_with_no_build_reads_its_skills_and_is_ready_with_no_image() {
        let m = r#"{"plugin": 1, "name": "floorplan", "label": "Floorplan", "asks": ["skill:grill"], "skills": [{"id": "grill", "about": "Asks hard questions", "path": "skills/grill.md", "chats": ["obeya"]}]}"#;
        let (home, mut x) = good(
            "plugin-skills",
            &[
                ("tehas-plugin.json", m),
                ("skills/grill.md", "### Grill\n\nAsk why.\n"),
            ],
        );
        let s = prepare(&mut x, &home, &pin(C), Some(&granted("skill:grill")));
        assert_eq!(s.state, Word::Ready, "{s:?}");
        assert_eq!(s.image, None);
        assert!(!x.ran_any("docker"));
        let (home, mut x) = good(
            "plugin-skills-bad",
            &[
                ("tehas-plugin.json", m),
                ("skills/grill.md", "<!-- hidden -->\n"),
            ],
        );
        let s = prepare(&mut x, &home, &pin(C), Some(&granted("skill:grill")));
        assert_eq!(
            s.refusal.as_deref(),
            Some("skill grill: skills/grill.md: a line opens an HTML comment")
        );
        let (home, mut x) = good("plugin-skills-gone", &[("tehas-plugin.json", m)]);
        let s = prepare(&mut x, &home, &pin(C), Some(&granted("skill:grill")));
        assert_eq!(
            s.refusal.as_deref(),
            Some("skill grill: skills/grill.md: not in the commit")
        );
    }

    #[test]
    fn the_refs_but_the_pins_are_pruned_then_the_repository_collected() {
        let (home, mut x) = good("plugin-prune", &[]);
        let other = "f".repeat(40);
        x.on(
            "for-each-ref",
            ok(&format!("refs/tehas/{C}\nrefs/tehas/{other}\n")),
        );
        x.on("update-ref -d", ok(""));
        x.on("gc --prune=now --quiet", ok(""));
        prune(&mut x, &home, &pin(C)).unwrap();
        assert!(x.ran_any(&format!("update-ref -d refs/tehas/{other}")));
        assert!(!x.ran_any(&format!("update-ref -d refs/tehas/{C}")));
        let gc = x.ran.last().unwrap();
        assert!(
            gc.starts_with("git -C ") && gc.ends_with(" gc --prune=now --quiet"),
            "{gc}"
        );
        let fresh = TestDir::new("plugin-prune-none");
        let mut y = Fake::new();
        prune(&mut y, &fresh, &pin(C)).unwrap();
        assert!(y.ran.is_empty());
    }

    #[test]
    fn a_plugin_taken_out_loses_its_images_volume_and_directory() {
        let (home, mut x) = good("plugin-remove", &[]);
        x.on(
            "docker image ls",
            ok(&format!(
                "tehas-plugin-floorplan:{C}\ntehas-plugin-floorplan:{}\n",
                "f".repeat(40)
            )),
        );
        x.on("docker volume inspect", ok("[]"));
        x.on("docker volume rm", ok(""));
        x.on("docker rmi", ok(""));
        remove(&mut x, &home, "floorplan").unwrap();
        assert!(!dir(&home, "floorplan").exists());
        assert!(x.ran_any(&format!("docker rmi tehas-plugin-floorplan:{C}")));
        assert!(x.ran_any("docker volume rm tehas-plugin-floorplan-state"));
    }
}
