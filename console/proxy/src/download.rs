//! The downloads of the job channel: what a contractor fetches from the
//! factory instead of building or fetching it itself.
//!
//! | Path | Who calls | What it serves |
//! | --- | --- | --- |
//! | `image?id=<id>` | a contractor, by its key | the factory's sandbox image, `docker save` output |
//! | `index` | a job, by its job token | the code index the factory built, for its image |
//! | `bundle` | a job, by its job token | a git bundle of the product checkout's tip |
//! | `attachments` | a job, by its job token | the issue's images and files with their manifest |
//!
//! Each is a GET under [`crate::channel::PREFIX`]. The image comes before a
//! job, so it is the contractor's, by the key `hello` takes; the others take
//! the job scope and the job's state as the channel's `check-in` does. A
//! model token on them is 403 `scope`.
//!
//! Each file is made once, on the first request that needs it, by a rename,
//! and kept for the next request of the same thing: an image per image id, an
//! index per (tip, image id), a bundle per tip, a job's files per job. An
//! answer names what it is in fields of its own (`x-tehas-*`), with the
//! sha256 of the file, and takes a `Range` so a broken download resumes.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Condvar, Mutex};

use sha2::{Digest, Sha256};

/// One file to serve and the fields that say what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    pub path: PathBuf,
    pub fields: Vec<(String, String)>,
}

/// Why a download is not served: the status and the word of the refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Miss {
    pub status: u16,
    pub word: String,
}

impl Miss {
    pub fn new(status: u16, word: &str) -> Miss {
        Miss {
            status,
            word: word.to_string(),
        }
    }
}

/// What the downloads need of the factory. The host's is [`OfHome`]; a test
/// gives its own.
pub trait Downloads {
    /// The id of the factory's current sandbox image, as `docker image
    /// inspect` gives it. None when it cannot be read.
    fn image_id(&self) -> Option<String>;
    /// The image of this id, saved and compressed.
    fn image(&self, id: &str) -> Result<Pack, Miss>;
    /// The code index for the checkout's tip, built by the current image.
    fn index(&self) -> Result<Pack, Miss>;
    /// A git bundle of the checkout's tip.
    fn bundle(&self) -> Result<Pack, Miss>;
    /// The issue's images and files for this job, fetched and checked as an
    /// in-house start does. 404 when the instance gives a cycle none.
    fn attachments(&self, job: i64, issue: i64) -> Result<Pack, Miss>;
    /// The job ended: its files go.
    fn forget(&self, job: i64);
}

/// How many image downloads run at once. A third waits for one to end.
pub const IMAGES_AT_ONCE: usize = 2;

/// A count of places, taken by blocking.
pub struct Gate {
    free: Mutex<usize>,
    freed: Condvar,
}

/// One place of a [`Gate`], given back when dropped.
pub struct Held<'a>(&'a Gate);

impl Gate {
    pub const fn new(places: usize) -> Gate {
        Gate {
            free: Mutex::new(places),
            freed: Condvar::new(),
        }
    }

    pub fn take(&self) -> Held<'_> {
        let mut free = self.free.lock().unwrap_or_else(|e| e.into_inner());
        while *free == 0 {
            free = self.freed.wait(free).unwrap_or_else(|e| e.into_inner());
        }
        *free -= 1;
        Held(self)
    }
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        self.0.freed.notify_one();
    }
}

/// What a `Range` field asks of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    /// No range: the whole file.
    Whole,
    /// The bytes from the first to the last, both included.
    Part(u64, u64),
    /// A range that cannot be served (416).
    Unserved,
}

/// The bytes a `Range` field asks of a file of `size` bytes. Only one range
/// of the form `bytes=<from>-` or `bytes=<from>-<to>` is taken.
pub fn range(field: Option<&str>, size: u64) -> Span {
    let Some(field) = field else {
        return Span::Whole;
    };
    let part = || -> Option<(u64, u64)> {
        let (from, to) = field.trim().strip_prefix("bytes=")?.split_once('-')?;
        let from: u64 = from.trim().parse().ok()?;
        let to = match to.trim() {
            "" => size.checked_sub(1)?,
            t => t.parse::<u64>().ok()?.min(size.saturating_sub(1)),
        };
        (from < size && to >= from).then_some((from, to))
    };
    part().map_or(Span::Unserved, |(from, to)| Span::Part(from, to))
}

/// The hex sha256 of a file, read once.
pub fn sha256_of(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The sha256 kept beside a made file, or made now and kept.
fn kept_sha(path: &Path) -> Result<String, Miss> {
    let side = PathBuf::from(format!("{}.sha256", path.display()));
    if let Ok(t) = fs::read_to_string(&side) {
        let t = t.trim();
        if t.len() == 64 {
            return Ok(t.to_string());
        }
    }
    let sha = sha256_of(path).map_err(|_| Miss::new(503, "the file could not be read"))?;
    let _ = fs::write(&side, format!("{sha}\n"));
    Ok(sha)
}

/// The factory's downloads on a host, from its own files under `home`.
pub struct OfHome {
    pub home: PathBuf,
    /// The product checkout (TEHAS_WORK_REPO).
    pub checkout: PathBuf,
    /// The branch work lands on, whose `origin/<branch>` is the tip.
    pub branch: String,
    /// The sandbox image a cycle runs (PI_ENSEMBLE_IMAGE).
    pub image: String,
    /// The docker program.
    pub docker: PathBuf,
    /// Whether files are compressed with zstd. Off, a tar is served as it is.
    pub zstd: bool,
    /// Whether the instance gives a cycle the issue's images or files
    /// (TEHAS_WORK_IMAGES or TEHAS_WORK_FILES).
    pub gives_files: bool,
    /// How the issue's files of a job are made: the in-house fetch
    /// ([`tehas_cycle::gives::for_job`]) on a host.
    pub fetch: fn(&Path, u64, &Path) -> Result<(), String>,
    /// One file of each kind is made at a time, so concurrent requests make
    /// it once, and a long image save holds up no bundle.
    pub making: Making,
}

/// The locks a file of each kind is made under.
#[derive(Default)]
pub struct Making {
    image: Mutex<()>,
    index: Mutex<()>,
    bundle: Mutex<()>,
    files: Mutex<()>,
}

fn held(m: &Mutex<()>) -> std::sync::MutexGuard<'_, ()> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl OfHome {
    /// Where the images are kept: `state/images`, made on the first request.
    pub fn images(&self) -> PathBuf {
        self.home.join("state/images")
    }

    /// Where the index, the bundles and the jobs' files are kept.
    pub fn kept(&self) -> PathBuf {
        self.home.join("state/outsource/downloads")
    }

    fn pack_word(&self) -> &'static str {
        if self.zstd {
            "zstd"
        } else {
            "tar"
        }
    }

    fn ext(&self) -> &'static str {
        if self.zstd {
            "tar.zst"
        } else {
            "tar"
        }
    }

    fn git(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(&self.checkout)
            .args(["-c", "core.hooksPath=/dev/null"])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }

    /// The commit `origin/<branch>` of the checkout names.
    pub fn tip(&self) -> Option<String> {
        let out = self
            .git()
            .args(["rev-parse", "-q", "--verify"])
            .arg(format!("refs/remotes/origin/{}^{{commit}}", self.branch))
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        let tip = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (tip.len() >= 40 && tip.bytes().all(|b| b.is_ascii_hexdigit())).then_some(tip)
    }

    /// `from`'s stdout written to `file` through zstd, or as it is.
    fn compressed(&self, mut from: Command, file: &Path) -> Result<(), String> {
        let out = File::create(file).map_err(|e| format!("{}: {e}", file.display()))?;
        if !self.zstd {
            let status = from
                .stdout(out)
                .stderr(Stdio::null())
                .status()
                .map_err(|e| e.to_string())?;
            return status
                .success()
                .then_some(())
                .ok_or_else(|| format!("exit {status}"));
        }
        let mut child = from
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let pipe = child.stdout.take().ok_or("no output")?;
        let zstd = Command::new("zstd")
            .args(["-q", "-T0", "-c"])
            .stdin(pipe)
            .stdout(out)
            .stderr(Stdio::null())
            .status();
        let made = child.wait().map_err(|e| e.to_string())?;
        let zstd = zstd.map_err(|e| format!("zstd: {e}"))?;
        (made.success() && zstd.success())
            .then_some(())
            .ok_or_else(|| format!("exit {made}, zstd {zstd}"))
    }

    /// A tar of `dir` at `file`, made by a rename; the leftovers of an index
    /// run are not in it.
    fn pack_dir(&self, dir: &Path, file: &Path) -> Result<(), Miss> {
        let new = file.with_extension("new");
        let mut tar = Command::new("tar");
        tar.arg("-C").arg(dir).args([
            "--exclude=./logs",
            "--exclude=*.tmp",
            "--exclude=*.db.stage.*",
            "-cf",
            "-",
            ".",
        ]);
        if let Err(why) = self.compressed(tar, &new) {
            let _ = fs::remove_file(&new);
            return Err(Miss::new(503, &format!("the pack was not made: {why}")));
        }
        fs::rename(&new, file).map_err(|_| Miss::new(503, "the pack was not kept"))
    }

    /// Remove the files of `dir` whose names start with `prefix`, but those
    /// of `keep` and the sha256 kept beside them.
    fn prune(dir: &Path, prefix: &str, keep: &[&Path]) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            let kept = keep.iter().any(|k| path == *k || sidecar(k, &path));
            if e.file_name().to_string_lossy().starts_with(prefix) && !kept {
                let _ = fs::remove_file(&path);
            }
        }
    }

    /// The uid this process runs as, which the image was built for.
    fn uid() -> String {
        fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("Uid:"))
                    .and_then(|l| l.split_whitespace().next().map(str::to_string))
            })
            .unwrap_or_default()
    }
}

/// Whether `path` is the sha256 kept beside `of`.
fn sidecar(of: &Path, path: &Path) -> bool {
    path == PathBuf::from(format!("{}.sha256", of.display()))
}

impl Downloads for OfHome {
    fn image_id(&self) -> Option<String> {
        if self.image.is_empty() {
            return None;
        }
        let out = Command::new("timeout")
            .args(["-k", "5", "30"])
            .arg(&self.docker)
            .args(["image", "inspect", "--format", "{{.Id}}", &self.image])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        store::outsource::image_id(String::from_utf8_lossy(&out.stdout).trim())
    }

    fn image(&self, id: &str) -> Result<Pack, Miss> {
        let Some(hex) = store::outsource::image_id(id).map(|i| i[7..].to_string()) else {
            return Err(Miss::new(400, "an image download names an image id"));
        };
        let _making = held(&self.making.image);
        let file = self.images().join(format!("{hex}.{}", self.ext()));
        if !file.is_file() {
            if self.image_id().as_deref() != Some(id) {
                return Err(Miss::new(404, "not the factory's image"));
            }
            fs::create_dir_all(self.images())
                .map_err(|_| Miss::new(503, "the images directory could not be made"))?;
            let new = file.with_extension("new");
            let mut save = Command::new(&self.docker);
            save.args(["save", &self.image]).stdin(Stdio::null());
            if let Err(why) = self.compressed(save, &new) {
                let _ = fs::remove_file(&new);
                return Err(Miss::new(503, &format!("the image was not saved: {why}")));
            }
            fs::rename(&new, &file).map_err(|_| Miss::new(503, "the image was not kept"))?;
            // The image before this one stays for a contractor still loading
            // it; older ones go.
            let previous = newest_other(&self.images(), &file);
            let mut keep = vec![file.as_path()];
            if let Some(p) = previous.as_deref() {
                keep.push(p);
            }
            OfHome::prune(&self.images(), "", &keep);
        }
        let sha = kept_sha(&file)?;
        Ok(Pack {
            path: file,
            fields: vec![
                ("x-tehas-image".into(), id.to_string()),
                ("x-tehas-uid".into(), OfHome::uid()),
                ("x-tehas-sha256".into(), sha),
                ("x-tehas-pack".into(), self.pack_word().into()),
            ],
        })
    }

    fn index(&self) -> Result<Pack, Miss> {
        let dir = self.home.join("state/index");
        let record: serde_json::Value = fs::read_to_string(self.home.join("state/index.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .ok_or_else(|| Miss::new(404, "no index"))?;
        let commit = record["commit"].as_str().unwrap_or_default();
        let built = record["indexer_image"].as_str().unwrap_or_default();
        let has_db = fs::read_dir(&dir).is_ok_and(|d| {
            d.flatten()
                .any(|e| e.file_name().to_string_lossy().ends_with(".db"))
        });
        if commit.is_empty() || !commit.bytes().all(|b| b.is_ascii_hexdigit()) || !has_db {
            return Err(Miss::new(404, "no index"));
        }
        let Some(hex) = store::outsource::image_id(built).map(|i| i[7..].to_string()) else {
            return Err(Miss::new(404, "no index"));
        };
        if self.image_id().as_deref() != Some(built) {
            return Err(Miss::new(404, "an index of another image"));
        }
        let _making = held(&self.making.index);
        let kept = self.kept();
        fs::create_dir_all(&kept).map_err(|_| Miss::new(503, "no downloads directory"))?;
        let name = format!("index-{commit}-{}", &hex[..12]);
        let file = kept.join(format!("{name}.{}", self.ext()));
        if !file.is_file() {
            self.pack_dir(&dir, &file)?;
            OfHome::prune(&kept, "index-", &[file.as_path()]);
        }
        let sha = kept_sha(&file)?;
        Ok(Pack {
            path: file,
            fields: vec![
                ("x-tehas-tip".into(), commit.to_string()),
                ("x-tehas-image".into(), built.to_string()),
                ("x-tehas-sha256".into(), sha),
                ("x-tehas-pack".into(), self.pack_word().into()),
            ],
        })
    }

    fn bundle(&self) -> Result<Pack, Miss> {
        if self.branch.is_empty() {
            return Err(Miss::new(404, "no branch"));
        }
        let Some(tip) = self.tip() else {
            return Err(Miss::new(404, "no tip in the checkout"));
        };
        let _making = held(&self.making.bundle);
        let kept = self.kept();
        fs::create_dir_all(&kept).map_err(|_| Miss::new(503, "no downloads directory"))?;
        let file = kept.join(format!("bundle-{tip}.bundle"));
        if !file.is_file() {
            let new = kept.join(format!("bundle-{tip}.new"));
            let _ = fs::remove_file(&new);
            // The one ref of the tip, as the checkout names it; a ref that
            // moved meanwhile makes a bundle of another tip, which is refused.
            let made = self
                .git()
                .args(["bundle", "create", "-q"])
                .arg(&new)
                .arg(format!("refs/remotes/origin/{}", self.branch))
                .stdout(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            let heads = Command::new("git")
                .args(["bundle", "list-heads"])
                .arg(&new)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default();
            if !made || !heads.starts_with(&tip) || heads.lines().count() != 1 {
                let _ = fs::remove_file(&new);
                return Err(Miss::new(503, "the bundle was not made"));
            }
            fs::rename(&new, &file).map_err(|_| Miss::new(503, "the bundle was not kept"))?;
            OfHome::prune(&kept, "bundle-", &[file.as_path()]);
        }
        let sha = kept_sha(&file)?;
        Ok(Pack {
            path: file,
            fields: vec![
                ("x-tehas-tip".into(), tip),
                ("x-tehas-branch".into(), self.branch.clone()),
                ("x-tehas-sha256".into(), sha),
                ("x-tehas-pack".into(), "bundle".into()),
            ],
        })
    }

    fn attachments(&self, job: i64, issue: i64) -> Result<Pack, Miss> {
        if !self.gives_files {
            return Err(Miss::new(404, "no files given"));
        }
        let Ok(number) = u64::try_from(issue) else {
            return Err(Miss::new(404, "no files given"));
        };
        let _making = held(&self.making.files);
        let at = self.kept().join(format!("job-{job}"));
        let file = at.join(format!("files.{}", self.ext()));
        if !file.is_file() {
            let dir = at.join("files");
            fs::create_dir_all(&at).map_err(|_| Miss::new(503, "no downloads directory"))?;
            if let Err(why) = (self.fetch)(&self.home, number, &dir) {
                let _ = fs::remove_dir_all(&at);
                return Err(Miss::new(409, &why));
            }
            let packed = self.pack_dir(&dir, &file);
            let _ = fs::remove_dir_all(&dir);
            packed?;
        }
        let sha = kept_sha(&file)?;
        Ok(Pack {
            path: file,
            fields: vec![
                ("x-tehas-issue".into(), issue.to_string()),
                ("x-tehas-sha256".into(), sha),
                ("x-tehas-pack".into(), self.pack_word().into()),
            ],
        })
    }

    fn forget(&self, job: i64) {
        let _ = fs::remove_dir_all(self.kept().join(format!("job-{job}")));
    }
}

/// The newest file of `dir` other than `but` and the sha256 files.
fn newest_other(dir: &Path, but: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let p = e.path();
            p != but && !p.to_string_lossy().ends_with(".sha256")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .max()
        .map(|(_, p)| p)
}

/// Copy `len` bytes of `file` from `from` to `to`.
pub fn send(file: &Path, from: u64, len: u64, to: &mut dyn Write) -> io::Result<u64> {
    use std::io::{Seek, SeekFrom};
    let mut f = File::open(file)?;
    f.seek(SeekFrom::Start(from))?;
    io::copy(&mut f.take(len), to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_is_one_span_of_the_file_or_refused() {
        assert_eq!(range(None, 10), Span::Whole);
        assert_eq!(range(Some("bytes=0-"), 10), Span::Part(0, 9));
        assert_eq!(range(Some("bytes=4-6"), 10), Span::Part(4, 6));
        assert_eq!(range(Some("bytes=4-60"), 10), Span::Part(4, 9));
        for bad in [
            "bytes=10-",
            "bytes=6-4",
            "bytes=-3",
            "items=0-1",
            "bytes=0-1,4-5",
        ] {
            assert_eq!(range(Some(bad), 10), Span::Unserved, "{bad}");
        }
        assert_eq!(range(Some("bytes=0-"), 0), Span::Unserved);
    }

    #[test]
    fn a_gate_lets_two_through_and_the_third_waits() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let gate = Arc::new(Gate::new(IMAGES_AT_ONCE));
        let inside = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let threads: Vec<_> = (0..5)
            .map(|_| {
                let (gate, inside, most) = (gate.clone(), inside.clone(), most.clone());
                std::thread::spawn(move || {
                    let _held = gate.take();
                    let n = inside.fetch_add(1, Ordering::SeqCst) + 1;
                    most.fetch_max(n, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    inside.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(most.load(Ordering::SeqCst), IMAGES_AT_ONCE);
    }
}
