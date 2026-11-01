//! The five minute run, and what it shares with the sweep: the settings, the
//! lock, the calls to Docker and GitHub, and the free space.

use super::verdict::{self, Pull};
use super::{choose, list, read_pages, Choice, Verdict};
use crate::{disk, edge, notice};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

pub const NET: &str = "tehas-previews";
/// Names the default branch's preview where a pull request's is named by its
/// number. It is not a number, so everything that matches is_number passes it
/// by; where it is meant, it is asked for by name.
pub const MAIN: &str = "main";
/// The open pull requests are read in pages, newest first, and 100 is what
/// GitHub answers with at most. Beyond PAGES of them the run has not read the
/// list, and it does not remove anything on a list it has not read.
const PAGE_SIZE: usize = 100;
const PAGES: usize = 10;
const SWEEP_WAIT: Duration = Duration::from_secs(1800);

/// The settings of one run, and what it has measured.
pub struct Preview {
    pub repo: String,
    pub domain: String,
    pub max: u64,
    pub min_kb: u64,
    pub settle: String,
    pub home: PathBuf,
    pub root: PathBuf,
    pub bare: PathBuf,
    pub live: PathBuf,
    pub logs: PathBuf,
    pub work_repo: PathBuf,
    pub free: Option<u64>,
    pub low: bool,
}

/// A line for the run's log. A log that cannot be written stops nothing: that
/// is when the disk is full, which this command has to get through.
pub fn say(line: fmt::Arguments) {
    let _ = writeln!(io::stdout(), "{line}");
}

/// A pull request's number, wherever one is matched here: one to nine digits,
/// no zero in front.
pub fn is_number(word: &str) -> bool {
    (1..=9).contains(&word.len())
        && !word.starts_with('0')
        && word.bytes().all(|b| b.is_ascii_digit())
}

/// GitHub takes a repository's name in any capitals.
pub fn same_repo(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// TEHAS_PREVIEW_SETTLE is the seconds between a container's start and the
/// question whether it still runs. It is there for the tests, so that they do
/// not wait. It is not documented as a setting, but a line in instance.env
/// reaches it all the same, through env.sh. So it can only shorten the wait: a
/// whole number below the default is taken, and anything else is the default.
fn settle_of(asked: &str) -> String {
    match asked {
        "0" | "1" | "2" | "3" | "4" => asked.to_string(),
        _ => "5".to_string(),
    }
}

/// `%.1f` of the figure in gigabytes.
pub fn gb(kb: u64) -> String {
    format!("{:.1} GB", kb as f64 / 1_048_576.0)
}

/// The branch name goes into a git refspec, so it is checked.
fn branch_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._/-".contains(c))
}

/// TEHAS_DOMAIN as lib/config.sh takes it. A preview's address is made by
/// putting uat-<n>. in front of it, and goes into pull requests.
fn domain_name(name: &str) -> bool {
    let label = |word: &str| {
        !word.starts_with('-')
            && !word.ends_with('-')
            && !word.is_empty()
            && word
                .bytes()
                .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'))
    };
    name.contains('.') && name.split('.').all(label)
}

fn commit_id(word: &str) -> bool {
    word.len() == 40 && word.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The answer of `gh api`, its errors going nowhere.
pub fn gh(path: &str) -> Option<String> {
    let out = Command::new("gh")
        .args(["api", path])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What Docker printed, whatever came of it: a listing that failed lists
/// nothing.
pub fn docker_lines(args: &[&str]) -> String {
    let out = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    out.map_or(String::new(), |out| {
        String::from_utf8_lossy(&out.stdout).into_owned()
    })
}

/// Whether Docker did it. Nothing it prints is anyone's.
pub fn docker_quiet(args: &[&str]) -> bool {
    Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn docker_answers() -> bool {
    docker_quiet(&["info", "--format", "{{.DockerRootDir}}"])
}

impl Preview {
    /// The settings lib/config.sh has checked. A command fails closed all the
    /// same: what is missing or malformed here is why, and nothing was asked.
    fn of_this_run() -> Result<Preview, String> {
        let get = |name: &str| std::env::var(name).unwrap_or_default();
        let number = |name: &str| {
            get(name)
                .parse::<u64>()
                .map_err(|_| format!("{name} is not a whole number"))
        };
        let (home, repo) = (get("TEHAS_HOME"), get("TEHAS_REPO"));
        if home.is_empty() || repo.is_empty() {
            return Err("TEHAS_HOME or TEHAS_REPO is not set".to_string());
        }
        if get("TEHAS_PREVIEWS") != "1" {
            return Err("TEHAS_PREVIEWS is not 1: this instance has no previews".to_string());
        }
        if !domain_name(&get("TEHAS_DOMAIN")) {
            return Err(
                "TEHAS_DOMAIN is not set or not a domain: a preview has no address without it"
                    .to_string(),
            );
        }
        let work_repo = match get("TEHAS_WORK_REPO") {
            dir if dir.is_empty() => "/nonexistent".to_string(),
            dir => dir,
        };
        let mut preview = Preview::under(PathBuf::from(home), repo, get("TEHAS_DOMAIN"));
        preview.max = number("TEHAS_PREVIEW_MAX")?;
        preview.min_kb = number("TEHAS_PREVIEW_MIN_FREE_MB")?.saturating_mul(1024);
        preview.settle = settle_of(&get("TEHAS_PREVIEW_SETTLE"));
        preview.work_repo = PathBuf::from(work_repo);
        Ok(preview)
    }

    fn under(home: PathBuf, repo: String, domain: String) -> Preview {
        let root = home.join("preview");
        Preview {
            repo,
            domain,
            max: 0,
            min_kb: 0,
            settle: settle_of(""),
            bare: root.join("repo.git"),
            live: home.join("live"),
            logs: home.join("logs"),
            root,
            home,
            work_repo: PathBuf::from("/nonexistent"),
            free: None,
            low: false,
        }
    }

    /// Where a preview answers. door in protolab.rs does the same for a ticket.
    pub(super) fn address(&self, name: &str) -> String {
        format!("https://uat-{name}.{}/", self.domain)
    }

    /// Set "free" and "low" for this run. Space that cannot be read is low:
    /// nothing is built on a guess.
    pub(super) fn measure(&mut self) {
        self.free = disk::free_kb(&self.home.to_string_lossy());
        if self.free.is_none_or(|kb| kb < self.min_kb) {
            self.low = true;
        }
    }

    pub(super) fn stop(&self, n: &str) {
        // A container goes with -v: a volume Docker made for it goes too. No
        // preview should have one, and nothing is left on the disk if one has.
        docker_quiet(&["rm", "-f", "-v", &format!("tehas-preview-{n}")]);
        docker_quiet(&["image", "rm", "-f", &format!("tehas-preview:{n}")]);
    }

    /// Everything of one pull request.
    pub(super) fn purge(&self, n: &str) {
        self.stop(n);
        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.bare)
            .args(["update-ref", "-d", &format!("refs/preview/{n}")])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = edge::remove(&self.root.join(n));
        let _ = fs::remove_file(self.logs.join(format!("preview-{n}.log")));
    }

    fn serving(&self, n: &str, sha: &str) -> bool {
        let name = format!("name=^tehas-preview-{n}$");
        let label = format!("label=tehas.sha={sha}");
        let filters = ["ps", "--quiet", "--filter", &name, "--filter", &label];
        !docker_lines(&filters).is_empty()
    }

    fn any_container(&self, n: &str) -> bool {
        let name = format!("name=^tehas-preview-{n}$");
        !docker_lines(&["ps", "-a", "--quiet", "--filter", &name]).is_empty()
    }

    /// Nothing is built, and the reason has the figures.
    fn no_space(&self, n: &str, sha: &str) {
        let reason = match self.free {
            Some(free) => format!(
                "The host is low on disk: {} is free and {} is needed.",
                gb(free),
                gb(self.min_kb)
            ),
            None => "The free space of the host could not be read.".to_string(),
        };
        self.mark(n, sha, "no-space", &reason);
    }

    /// Make the preview serve that commit, unless it already does or the
    /// commit was tried and failed. A new commit is tried again.
    fn bring(&self, n: &str, sha: &str, main_branch: &str) {
        let tried = || {
            self.field(n, "sha") == sha
                && matches!(
                    self.field(n, "state").as_str(),
                    "build-failed" | "no-recipe"
                )
                && !self.root.join(n).join(".retry").exists()
        };
        if self.serving(n, sha) {
            self.mark(n, sha, "serving", "");
        } else if !tried() {
            if self.low {
                self.no_space(n, sha);
            } else {
                self.build(n, main_branch);
            }
        }
    }

    /// The default branch and the commit at its head, as GitHub says now.
    fn default_head(&self) -> Option<(String, String)> {
        let path = format!("repos/{}", self.repo);
        let info: Value = serde_json::from_str(&gh(&path)?).ok()?;
        let branch = info.get("default_branch")?.as_str()?.to_string();
        if !branch_name(&branch) {
            return None;
        }
        let info: Value = serde_json::from_str(&gh(&format!("{path}/branches/{branch}"))?).ok()?;
        let head = info.pointer("/commit/sha")?.as_str()?.to_string();
        commit_id(&head).then_some((branch, head))
    }

    /// The head of the default branch. Asked of GitHub at every run. An answer
    /// that is missing or not clear changes nothing: what runs keeps running.
    fn main_preview(&self) {
        let Some((branch, head)) = self.default_head() else {
            say(format_args!(
                "{MAIN}: GitHub did not say what the default branch is at, left as it is"
            ));
            return;
        };
        if fs::create_dir_all(self.root.join(MAIN)).is_ok() {
            self.bring(MAIN, &head, &branch);
        }
    }

    /// One page as GitHub answered it, and how many pull requests it holds. An
    /// answer that is not an array is no page: read as a page of none it would
    /// make every preview look closed and build nothing of ours.
    fn page(&self, number: usize) -> Option<(String, usize)> {
        let answer = gh(&format!(
            "repos/{}/pulls?state=open&sort=created&direction=desc&per_page={PAGE_SIZE}&page={number}",
            self.repo
        ))?;
        match serde_json::from_str(&answer).ok()? {
            Value::Array(page) => Some((answer, page.len())),
            _ => None,
        }
    }

    /// All the open pull requests, kept as GitHub answered them. Only the
    /// newest page was asked for at first: a hundred newer pull requests from
    /// forks push every one of ours off it, and the run then removes every
    /// preview that serves until they close. The pages are read until one is
    /// short, where GitHub ends. None when the list was not read: a run does not
    /// build or remove on a list it did not read.
    fn read_list(&self) -> Option<String> {
        let mut kept = String::new();
        let mut pages = 0;
        loop {
            let (page, mut count) = self.page(pages + 1)?;
            kept.push_str(&page);
            kept.push('\n');
            pages += 1;
            // The last page this run may ask for that comes back full is not
            // proof the list ended there: one more answer tells them apart. Its
            // bytes are kept too, so the whole list is read as GitHub gave it:
            // an empty page means the list was read, and a full one is more
            // than the run may build on.
            if count == PAGE_SIZE && pages >= PAGES {
                let (probe, more) = self.page(pages + 1)?;
                kept.push_str(&probe);
                kept.push('\n');
                if more != 0 {
                    say(format_args!("the open pull requests are more than {}: the list was not read, nothing was built or removed", PAGES * PAGE_SIZE));
                    return None;
                }
                count = more;
            }
            if count < PAGE_SIZE {
                return Some(kept);
            }
        }
    }

    /// What is on the host and no longer open: by directory and by container.
    /// Only a container whose label names this repository gives a number, in
    /// any capitals, as in the sweep: another's is not this run's to speak of.
    fn on_host(&self) -> BTreeSet<u64> {
        let format = "{{.ID}} {{.Label \"tehas.preview\"}} {{.Label \"tehas.repo\"}}";
        let listing = [
            "ps",
            "-a",
            "--filter",
            "label=tehas.preview",
            "--format",
            format,
        ];
        let mut words = edge::numbers(&self.root);
        words.extend(self.container_numbers(&listing));
        words
            .iter()
            .filter(|word| is_number(word))
            .filter_map(|word| word.parse().ok())
            .collect()
    }

    /// The labels of number of the containers of this repository.
    pub(super) fn container_numbers(&self, listing: &[&str]) -> Vec<String> {
        docker_lines(listing)
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace().skip(1);
                let (number, owner) = (fields.next()?, fields.next()?);
                same_repo(owner, &self.repo).then(|| number.to_string())
            })
            .collect()
    }

    fn remove_closed(&self, open: &BTreeSet<u64>) {
        for n in self.on_host().difference(open) {
            // Membership in the list is not enough to remove something. The
            // pages are asked for one at a time: when the list moves while they
            // are read a pull request can fall between two of them and be in
            // neither, and one that is open would then look closed. So the run
            // asks about each number it means to remove, as the sweep does, and
            // keeps what GitHub says is open of ours.
            let said = match verdict::ask(*n, &self.repo) {
                Pull::Closed | Pull::Gone => "pull request no longer open, removed".to_string(),
                Pull::Elsewhere => format!("its branch is not in {}, removed", self.repo),
                Pull::Outside => format!("it was not opened by someone of {}, removed", self.repo),
                Pull::Keep => {
                    say(format_args!(
                        "#{n}: open though the list did not say so, kept"
                    ));
                    continue;
                }
                Pull::Unclear => {
                    say(format_args!("#{n}: no answer from GitHub, kept"));
                    continue;
                }
            };
            self.purge(&n.to_string());
            say(format_args!("#{n}: {said}"));
        }
    }

    fn tell(&self, n: u64, seen: &str) {
        let number = n.to_string();
        let address = self.address(&number);
        notice::tell(&number, seen, &self.repo, &self.root, &address);
    }

    /// The state of a pull request the console chose past the limit. Said once,
    /// not at every run after that: nothing is done when the state already says
    /// so and no container of the name is listed.
    fn over_limit(&self, n: &str, sha: &str) {
        let settled = self.field(n, "sha") == sha
            && self.field(n, "state") == "over-limit"
            && !self.any_container(n);
        if !settled {
            self.stop(n);
        }
        let reason = format!("More than {} pull requests are open.", self.max);
        self.mark(n, sha, "over-limit", &reason);
    }

    /// The chosen pull requests, newest first: their title and state, and the
    /// ones past the limit dealt with. False when a directory or a file of one
    /// could not be written.
    fn take_chosen(&self, chosen: &[Verdict], pulls: &[Value]) -> bool {
        for row in chosen {
            let (Verdict::Build { number, sha, seen } | Verdict::OverLimit { number, sha, seen }) =
                row
            else {
                continue;
            };
            let n = number.to_string();
            let dir = self.root.join(&n);
            if fs::create_dir_all(&dir).is_err() {
                return false;
            }
            let Some(pull) = list::pull_file(pulls, *number) else {
                return false;
            };
            if edge::put(&dir.join("pull.json"), pull.as_bytes()).is_err() {
                return false;
            }
            self.retitle(&n);
            if matches!(row, Verdict::OverLimit { .. }) {
                self.over_limit(&n, sha);
                self.tell(*number, seen);
            }
        }
        true
    }

    pub(super) fn run(&mut self) -> u8 {
        // A run that stops here or at the network still writes the list. Its
        // time is what the dashboard shows and what tehasbot health goes by,
        // and a list that stopped changing would look like a timer that
        // stopped. Without Docker the free space is not measured: the list has
        // no figure and says low, as for any space that cannot be read, so that
        // it does not read as a host where all is well.
        if !docker_answers() {
            say(format_args!(
                "Docker does not answer; nothing was built or removed"
            ));
            self.free = None;
            self.low = true;
            self.write_list();
            return 0;
        }
        self.measure();
        // A preview runs the pull request's own code. Nothing is built unless
        // the network keeps it from the host: console/src/edge.rs says what
        // that takes.
        if let Some(why) = edge::network_fault(NET) {
            say(format_args!(
                "the network {NET} does not keep the host out ({why}); nothing was built"
            ));
            self.write_list();
            return 0;
        }

        // Before the pull requests, so that a list that cannot be read does
        // not keep the default branch from being served.
        self.main_preview();

        let Some(kept) = self.read_list() else {
            self.write_list();
            return 0;
        };
        // A list that cannot be read, or one that does not say who opened one
        // of ours, builds and removes nothing: reading an unclear answer as "an
        // outsider's" would take a preview from a pull request that serves,
        // where the sweep keeps.
        let pulls = match read_pages(&kept) {
            Ok(pulls) => pulls,
            Err(why) => {
                let _ = writeln!(io::stderr(), "console preview choose: {why}");
                say(format_args!("the console chose no previews: the list was not read, nothing was built or removed"));
                self.write_list();
                return 0;
            }
        };
        let (chosen, elsewhere, outside) = match choose(&pulls, &self.repo, self.max) {
            Choice::Incomplete => {
                say(format_args!("the list of open pull requests does not say who opened each; nothing was built or removed"));
                self.write_list();
                return 0;
            }
            Choice::Chosen {
                chosen,
                elsewhere,
                outside,
            } => (chosen, elsewhere, outside),
        };
        say_skipped(
            elsewhere,
            "pull request from elsewhere",
            "pull requests from elsewhere",
        );
        say_skipped(
            outside,
            "pull request opened by someone outside the repository",
            "pull requests opened by someone outside the repository",
        );

        if !self.take_chosen(&chosen, &pulls) {
            self.write_list();
            return 1;
        }
        // The rows are the whole choice, so their numbers are what is not
        // removed.
        let open = chosen
            .iter()
            .filter_map(|row| match row {
                Verdict::Build { number, .. } | Verdict::OverLimit { number, .. } => Some(*number),
                _ => None,
            })
            .collect();
        self.remove_closed(&open);

        for row in &chosen {
            if let Verdict::Build { number, sha, seen } = row {
                self.bring(&number.to_string(), sha, "");
                self.tell(*number, seen);
            }
        }
        u8::from(!self.write_list())
    }
}

fn say_skipped(count: usize, one: &str, many: &str) {
    match count {
        0 => {}
        1 => say(format_args!("1 {one}, not built")),
        _ => say(format_args!("{count} {many}, not built")),
    }
}

/// The command of a timer. The sweep waits for a run to end; a run does not
/// wait for anything.
pub fn main(sweep: bool) -> u8 {
    let mut preview = match Preview::of_this_run() {
        Ok(preview) => preview,
        Err(why) => {
            let _ = writeln!(io::stderr(), "console preview: {why}");
            return 78;
        }
    };
    let scratch = preview.home.join("tmp");
    for dir in [&preview.root, &preview.live, &preview.logs, &scratch] {
        if let Err(why) = fs::create_dir_all(dir) {
            let dir = dir.display();
            let _ = writeln!(io::stderr(), "console preview: cannot create {dir}: {why}");
            return 1;
        }
    }
    let wait = sweep.then_some(SWEEP_WAIT);
    let Some(_lock) = edge::lock(&preview.home, "preview", wait) else {
        if sweep {
            say(format_args!(
                "the lock was not free in 30 minutes, nothing was swept"
            ));
        } else {
            say(format_args!("another tehasbot preview is running"));
        }
        return 0;
    };
    if sweep {
        preview.sweep()
    } else {
        preview.run()
    }
}

#[cfg(test)]
impl Drop for Preview {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

#[cfg(test)]
pub fn scratch(name: &str) -> Preview {
    let home = std::env::temp_dir().join(format!("preview-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let mut preview = Preview::under(home, "acme/widget".into(), "example.org".into());
    preview.min_kb = 1024;
    for dir in [&preview.root, &preview.live, &preview.logs] {
        fs::create_dir_all(dir).unwrap();
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_one_to_nine_digits_with_no_zero_in_front() {
        for word in ["1", "12", "999999999"] {
            assert!(is_number(word), "{word}");
        }
        for word in ["", "0", "012", "1234567890", "12a", "-1", "١٢"] {
            assert!(!is_number(word), "{word}");
        }
    }

    #[test]
    fn the_wait_can_only_be_shortened() {
        for (asked, waited) in [
            ("0", "0"),
            ("4", "4"),
            ("5", "5"),
            ("6", "5"),
            ("60", "5"),
            ("00", "5"),
            ("-1", "5"),
            ("0.5", "5"),
            ("soon", "5"),
            ("", "5"),
            ("0\n", "5"),
        ] {
            assert_eq!(settle_of(asked), waited, "{asked:?}");
        }
    }

    #[test]
    fn gigabytes_read_as_printf_prints_them() {
        for (kb, said) in [
            (0, "0.0 GB"),
            (1000, "0.0 GB"),
            (262144, "0.2 GB"),
            (786432, "0.8 GB"),
            (5242880, "5.0 GB"),
            (41943040, "40.0 GB"),
        ] {
            assert_eq!(gb(kb), said, "{kb}");
        }
    }

    #[test]
    fn a_branch_name_is_safe_for_a_refspec() {
        for name in ["main", "release/1.2", "a_b-c.d"] {
            assert!(branch_name(name), "{name}");
        }
        for name in ["", "-x", ".x", "/x", "a b", "a:b", "a\nb", "a\n", "ä"] {
            assert!(!branch_name(name), "{name:?}");
        }
    }

    #[test]
    fn a_domain_reads_as_lib_config_sh_reads_it() {
        for name in ["example.org", "a.b.c", "x-1.example.org", "1.2"] {
            assert!(domain_name(name), "{name}");
        }
        for name in [
            "",
            " ",
            "example",
            "Example.org",
            "-a.example.org",
            "a-.example.org",
            "a..org",
            ".example.org",
            "example.org.",
            "example.org/x",
            "https://example.org",
            "example.org\n",
            "ä.example.org",
        ] {
            assert!(!domain_name(name), "{name:?}");
        }
    }

    #[test]
    fn a_commit_id_is_forty_lowercase_hex() {
        assert!(commit_id(&"a".repeat(40)));
        for word in [
            "abc".to_string(),
            "A".repeat(40),
            "g".repeat(40),
            "a".repeat(41),
        ] {
            assert!(!commit_id(&word), "{word}");
        }
    }

    #[test]
    fn a_measure_that_has_no_figure_is_low() {
        let mut preview = scratch("measure");
        let home = std::mem::take(&mut preview.home);
        preview.measure();
        preview.home = home;
        assert_eq!(preview.free, None);
        assert!(preview.low);
    }
}
