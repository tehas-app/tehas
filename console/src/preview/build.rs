//! One build: fetch the head, build its recipe, replace the container. The
//! commit is the one fetched, which is the one built.

use super::run::{docker_quiet, say, Preview, MAIN, NET};
use crate::edge::docker;
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const RECIPE: &str = "preview/Dockerfile";
const BUILD_LIMIT: u64 = 900;
/// What a preview has in memory. A declared volume anywhere else is on the disk.
const IN_MEMORY: [&str; 3] = ["/tmp", "/config", "/data"];

/// The failed starts of a commit, as the retry marker holds them: the marker
/// is "<commit> <failed starts so far>".
fn tries_of(marker: &str, sha: &str) -> u32 {
    let Some((line, _)) = marker.split_once('\n') else {
        return 0;
    };
    let mut words = line.split_whitespace();
    match (words.next(), words.next(), words.next()) {
        (Some(mark), Some(tries), None) if mark == sha && tries.len() == 1 => {
            tries.parse().unwrap_or(0)
        }
        _ => 0,
    }
}

/// What an image says of its volumes, read from `{{json .Config.Volumes}}`.
#[derive(Debug, PartialEq)]
enum Volumes {
    InMemory,
    /// The first one elsewhere: the recipe's own text, without control
    /// characters and cut short.
    At(String),
    /// No answer, or one that is not a set of paths, is not a "none".
    Unread,
}

fn volumes(said: &str) -> Volumes {
    let mut answers = serde_json::Deserializer::from_str(said).into_iter::<Value>();
    let (Some(Ok(answer)), None) = (answers.next(), answers.next()) else {
        return Volumes::Unread;
    };
    match answer {
        Value::Null => Volumes::InMemory,
        Value::Object(paths) => match paths.keys().find(|p| !IN_MEMORY.contains(&p.as_str())) {
            None => Volumes::InMemory,
            Some(path) => Volumes::At(path.chars().filter(|c| !c.is_control()).take(80).collect()),
        },
        _ => Volumes::Unread,
    }
}

fn append(log: &Path) -> Option<File> {
    OpenOptions::new().append(true).create(true).open(log).ok()
}

/// The program with both its streams in the log, as `>>log 2>&1`.
fn logged(program: &mut Command, log: &Path) -> bool {
    let Some(out) = append(log) else {
        return false;
    };
    let Ok(err) = out.try_clone() else {
        return false;
    };
    program
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .status()
        .is_ok_and(|status| status.success())
}

/// The options of a preview's container. What it gets of the host is here and
/// nowhere else: test/preview_build_test.sh pins this list word for word.
fn run_args(n: &str, sha: &str, repo: &str) -> Vec<String> {
    let (name, image) = (format!("tehas-preview-{n}"), format!("tehas-preview:{n}"));
    let (preview, commit, of_repo) = (
        format!("tehas.preview={n}"),
        format!("tehas.sha={sha}"),
        format!("tehas.repo={repo}"),
    );
    [
        "run",
        "--detach",
        "--name",
        &name,
        "--network",
        NET,
        "--read-only",
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        "/config",
        "--tmpfs",
        "/data",
        "--cap-drop",
        "ALL",
        "--cap-add",
        "NET_BIND_SERVICE",
        "--security-opt",
        "no-new-privileges:true",
        "--memory",
        "256m",
        "--cpus",
        "1",
        "--pids-limit",
        "128",
        "--log-opt",
        "max-size=10m",
        "--log-opt",
        "max-file=2",
        "--restart",
        "unless-stopped",
        "--label",
        &preview,
        "--label",
        &commit,
        "--label",
        &of_repo,
        &image,
    ]
    .map(String::from)
    .to_vec()
}

fn build_args(n: &str, sha: &str, repo: &str) -> Vec<String> {
    let image = format!("tehas-preview:{n}");
    let (preview, commit, of_repo) = (
        format!("tehas.preview={n}"),
        format!("tehas.sha={sha}"),
        format!("tehas.repo={repo}"),
    );
    let limit = BUILD_LIMIT.to_string();
    [
        &limit, "docker", "build", "--quiet", "-f", RECIPE, "-t", &image, "--label", &preview,
        "--label", &commit, "--label", &of_repo, "-",
    ]
    .map(String::from)
    .to_vec()
}

impl Preview {
    fn git(&self) -> Command {
        let mut git = Command::new("git");
        git.arg("-C").arg(&self.bare).stdin(Stdio::null());
        git
    }

    /// The product checkout knows the address and how to sign in to it. Read
    /// on every build, so a repository that moved is followed here too.
    fn origin(&self) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.work_repo)
            .args(["config", "--get", "remote.origin.url"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        match out {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .trim_end_matches('\n')
                .to_string(),
            _ => format!("https://github.com/{}.git", self.repo),
        }
    }

    fn clone_follows(&self, url: &str) -> bool {
        let done = |git: &mut Command| git.status().is_ok_and(|status| status.success());
        if !self.bare.is_dir() {
            let init = Command::new("git")
                .args(["init", "-q", "--bare"])
                .arg(&self.bare)
                .stdin(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            return init && done(self.git().args(["remote", "add", "origin", url]));
        }
        let now = self
            .git()
            .args(["config", "--get", "remote.origin.url"])
            .stderr(Stdio::inherit())
            .output()
            .map(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .trim_end_matches('\n')
                    .to_string()
            })
            .unwrap_or_default();
        now == url || done(self.git().args(["remote", "set-url", "origin", url]))
    }

    fn head_of_ref(&self, n: &str) -> Option<String> {
        let wanted = format!("refs/preview/{n}^{{commit}}");
        let out = self
            .git()
            .args(["rev-parse", "--verify", "--quiet", &wanted])
            .stderr(Stdio::inherit())
            .output()
            .ok()?;
        out.status.success().then(|| {
            String::from_utf8_lossy(&out.stdout)
                .trim_end_matches('\n')
                .to_string()
        })
    }

    fn has_recipe(&self, sha: &str) -> bool {
        self.git()
            .args(["cat-file", "-e", &format!("{sha}:{RECIPE}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    /// `git archive | timeout docker build`: both have to go well, as with
    /// pipefail. git's own complaint goes to the log, as the build's does.
    fn built(&self, n: &str, sha: &str, log: &Path) -> bool {
        let (Some(git_err), Some(out)) = (append(log), append(log)) else {
            return false;
        };
        let Ok(err) = out.try_clone() else {
            return false;
        };
        let Ok(mut archive) = self
            .git()
            .args(["archive", sha])
            .stdout(Stdio::piped())
            .stderr(git_err)
            .spawn()
        else {
            return false;
        };
        let stdin = archive.stdout.take().map_or(Stdio::null(), Stdio::from);
        let build = Command::new("timeout")
            .args(build_args(n, sha, &self.repo))
            .stdin(stdin)
            .stdout(out)
            .stderr(err)
            .status();
        let archived = archive.wait();
        build.is_ok_and(|status| status.success()) && archived.is_ok_and(|status| status.success())
    }

    /// Says nothing when the image declares no volume outside what the
    /// container has in memory, and otherwise why not. For a path anywhere else
    /// Docker makes a volume on the host's disk: it can be written under a
    /// read-only root, it is outside the memory limit, and it stays when the
    /// container goes. The path is text of the recipe's own, so it goes in that
    /// pull request's build log. What is returned reaches state.json and the
    /// notice in the description, where the bot's own account would be quoting
    /// someone else's commit.
    fn volume_fault(&self, n: &str, log: &Path) -> Option<String> {
        let image = format!("tehas-preview:{n}");
        let said = docker(&[
            "image",
            "inspect",
            "--format",
            "{{json .Config.Volumes}}",
            &image,
        ]);
        match volumes(&said.unwrap_or_default()) {
            Volumes::InMemory => None,
            Volumes::At(path) => {
                if let Some(mut log) = append(log) {
                    let _ = writeln!(
                        log,
                        "The recipe declares a volume at {path}. A preview may write to /tmp, /config and /data only."
                    );
                }
                Some(format!(
                    "The recipe declares a volume outside /tmp, /config and /data. Its path is in logs/preview-{n}.log on the host."
                ))
            }
            Volumes::Unread => Some("The image's volumes could not be read.".to_string()),
        }
    }

    /// The image was built and no container of it stayed up. The host may have
    /// hiccuped, not the commit: the next runs try again. Three tries ride out
    /// a hiccup, and a recipe that cannot start does not loop.
    fn start_failed(&self, n: &str, sha: &str, tries: u32, reason: &str) {
        if tries < 3 {
            let marker = format!("{sha} {}\n", tries + 1);
            let _ = fs::write(self.root.join(n).join(".retry"), marker);
        }
        self.mark(n, sha, "build-failed", reason);
    }

    fn settled(&self, name: &str) -> bool {
        let answer = docker(&[
            "inspect",
            "--format",
            "{{.State.Running}} {{.RestartCount}}",
            name,
        ]);
        answer.unwrap_or_default().trim_end_matches('\n') == "true 0"
    }

    pub(super) fn build(&self, n: &str, main_branch: &str) {
        let log = self.logs.join(format!("preview-{n}.log"));
        let retry = self.root.join(n).join(".retry");
        let who = if n == MAIN {
            n.to_string()
        } else {
            format!("#{n}")
        };
        if !self.clone_follows(&self.origin()) {
            return;
        }
        let _ = File::create(&log);
        let from = if n == MAIN {
            format!("refs/heads/{main_branch}")
        } else {
            format!("refs/pull/{n}/head")
        };
        let refspec = format!("+{from}:refs/preview/{n}");
        let fetch = ["fetch", "--quiet", "--no-tags", "origin", &refspec];
        if !logged(self.git().args(fetch), &log) {
            // No state: a fetch that failed says nothing about the commit.
            say(format_args!(
                "{who}: the commit could not be fetched, tried again at the next run"
            ));
            return;
        }
        let Some(sha) = self.head_of_ref(n) else {
            return;
        };
        // Read here and cleared whatever this build turns out to be, so no
        // path keeps a stale one.
        let marker = fs::read_to_string(&retry).unwrap_or_default();
        let tries = tries_of(&marker, &sha);
        let _ = fs::remove_file(&retry);
        if !self.has_recipe(&sha) {
            self.stop(n);
            let reason = format!("The commit has no {RECIPE}.");
            self.mark(n, &sha, "no-recipe", &reason);
            return;
        }
        self.mark(n, &sha, "building", "");
        let (name, image) = (format!("tehas-preview-{n}"), format!("tehas-preview:{n}"));
        let image_id = || {
            let id = docker(&["image", "inspect", "--format", "{{.Id}}", &image]);
            id.map(|id| id.trim_end_matches('\n').to_string())
        };
        let old = image_id();
        if !self.built(n, &sha, &log) {
            let reason = format!(
                "The build failed or ran over {} minutes. Its output is in logs/preview-{n}.log on the host.",
                BUILD_LIMIT / 60
            );
            self.mark(n, &sha, "build-failed", &reason);
            return;
        }
        // Asked before the old container goes, so this is a failed build like
        // any other: the earlier preview keeps serving and the commit is not
        // tried again. The image is of no use and is removed.
        if let Some(fault) = self.volume_fault(n, &log) {
            docker_quiet(&["image", "rm", &image]);
            self.mark(n, &sha, "build-failed", &fault);
            return;
        }
        // The same name cannot run twice, so the old container goes first. It
        // is a few seconds of the router's 404.
        docker_quiet(&["rm", "-f", "-v", &name]);
        if !logged(
            Command::new("docker").args(run_args(n, &sha, &self.repo)),
            &log,
        ) {
            let reason =
                format!("The container did not start. See logs/preview-{n}.log on the host.");
            self.start_failed(n, &sha, tries, &reason);
            return;
        }
        // docker run answers once the process is started. One that stops at
        // once, because it writes outside its three directories or misses a
        // variable, would be called serving and be started again for ever. So
        // it is asked a moment later: still running, and never restarted.
        let _ = Command::new("sleep").arg(&self.settle).status();
        if !self.settled(&name) {
            if let Some(mut log) = append(&log) {
                let _ = writeln!(log, "docker logs of {name}, which started and stopped:");
            }
            let tail = ["logs", "--tail", "40", &name];
            logged(Command::new("docker").args(tail), &log);
            docker_quiet(&["rm", "-f", "-v", &name]);
            let reason = format!(
                "The container started and stopped. See \"docker logs\" in logs/preview-{n}.log on the host."
            );
            self.start_failed(n, &sha, tries, &reason);
            return;
        }
        let now = image_id();
        if let Some(old) = old.filter(|old| !old.is_empty() && Some(old) != now.as_ref()) {
            docker_quiet(&["image", "rm", &old]);
        }
        self.mark(n, &sha, "serving", "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_in_memory_or_none_are_fine() {
        for said in [
            "null\n",
            "{}\n",
            "{\"/tmp\":{}}\n",
            "{\"/config\":{},\"/data\":{}}\n",
        ] {
            assert_eq!(volumes(said), Volumes::InMemory, "{said}");
        }
    }

    #[test]
    fn a_volume_elsewhere_is_named() {
        for (said, at) in [
            ("{\"/srv\":{}}", "/srv"),
            ("{\"/tmp/\":{}}", "/tmp/"),
            ("{\"/data/x\":{}}", "/data/x"),
            ("{\"\":{}}", ""),
            ("{\"/\":{}}", "/"),
        ] {
            assert_eq!(volumes(said), Volumes::At(at.to_string()), "{said}");
        }
        assert_eq!(
            volumes("{\"/var/lib/docker\":{},\"/data\":{}}"),
            Volumes::At("/var/lib/docker".to_string())
        );
    }

    #[test]
    fn a_path_loses_its_control_characters_before_it_is_cut() {
        let said = format!(
            "{{\"/a\\u0007b\\u001b[31m\\u0085{}\":{{}}}}",
            "x".repeat(200)
        );
        let Volumes::At(path) = volumes(&said) else {
            panic!("a path");
        };
        assert_eq!(path, format!("/ab[31m{}", "x".repeat(73)));
        assert_eq!(path.chars().count(), 80);
    }

    #[test]
    fn an_answer_that_is_no_set_of_paths_is_unread() {
        for said in [
            "",
            "\n",
            "not json",
            "[]",
            "\"/srv\"",
            "12",
            "false",
            "null\nnull\n",
        ] {
            assert_eq!(volumes(said), Volumes::Unread, "{said:?}");
        }
    }

    #[test]
    fn the_retry_marker_counts_for_its_own_commit() {
        assert_eq!(tries_of("abc 2\n", "abc"), 2);
        assert_eq!(tries_of("  abc   2  \n", "abc"), 2);
        for marker in [
            "def 2\n",
            "abc\n",
            "abc 12\n",
            "abc x\n",
            "abc 2 3\n",
            "",
            "abc 2",
        ] {
            assert_eq!(tries_of(marker, "abc"), 0, "{marker:?}");
        }
    }

    #[test]
    fn the_container_is_started_with_exactly_these() {
        assert_eq!(
            run_args("12", "abc", "acme/widget").join(" "),
            "run --detach --name tehas-preview-12 --network tehas-previews --read-only \
             --tmpfs /tmp --tmpfs /config --tmpfs /data --cap-drop ALL \
             --cap-add NET_BIND_SERVICE --security-opt no-new-privileges:true --memory 256m \
             --cpus 1 --pids-limit 128 --log-opt max-size=10m --log-opt max-file=2 \
             --restart unless-stopped --label tehas.preview=12 --label tehas.sha=abc \
             --label tehas.repo=acme/widget tehas-preview:12"
        );
    }

    #[test]
    fn the_build_is_limited_and_reads_standard_input() {
        assert_eq!(
            build_args("12", "abc", "acme/widget").join(" "),
            "900 docker build --quiet -f preview/Dockerfile -t tehas-preview:12 \
             --label tehas.preview=12 --label tehas.sha=abc --label tehas.repo=acme/widget -"
        );
    }
}
