//! Whether the app's cycle token can push: the proof `tehas update` and
//! `install.sh --check` make before a factory starts work on a new version.
//!
//! A cycle's container pushes with the app's narrowed token, which
//! `tehasbot github-token --cycle` makes. A token that has never pushed can
//! still be refused, and then every start stops at the sandbox's push check.
//! So the update asks it once: a dry-run push from the product checkout to
//! the origin a cycle's tree is cloned with, to the branch the sandbox's own
//! check names. A dry run needs the same credentials a real push does and
//! creates nothing.
//!
//! The token reaches git by its environment through a credential helper of
//! this command's own, never on a command line. Every other helper is dropped,
//! so neither the host's gh nor a stored credential answers in its place.
//!
//! Nothing is proven on a host with no product checkout or no tehasbot: that
//! is no proof and no fault, and the callers print nothing for it.

use super::timed;
use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tehas_core::mask::mask;

/// One answer of tehasbot github-token.
const TOKEN_LIMIT: u32 = 60;
/// The dry-run push.
const PUSH_LIMIT: u32 = 60;
/// The ref the sandbox's push check names (tehas_core::sandbox::push_check).
const REFSPEC: &str = "HEAD:refs/heads/tehas-preflight";
/// The token is read from this variable by the helper below.
const TOKEN_VAR: &str = "GH_TOKEN";
/// git's credential helper for the proof: the token from the environment.
const HELPER: &str = "credential.helper=!f() { test \"$1\" = get || exit 0; echo username=x-access-token; echo \"password=$GH_TOKEN\"; }; f";
/// The lines of what git or tehasbot said that a finding shows.
const SHOWN: usize = 3;

/// What the proof found.
#[derive(Debug, PartialEq)]
pub(crate) enum Proof {
    /// The account the token acts as can push.
    Pushes(String),
    /// tehasbot made no cycle token; what it said.
    NoToken(Vec<String>),
    /// The account, and what git said when the dry-run push was refused.
    Refused(String, Vec<String>),
    /// The account, on a read-only instance: its cycle token may not push, so
    /// no push is asked of it.
    ReadOnly(String),
}

impl Proof {
    /// Whether a factory may start work on it: the token pushes, or the
    /// instance is read-only and its token is not meant to.
    pub fn starts(&self) -> bool {
        matches!(self, Proof::Pushes(_) | Proof::ReadOnly(_))
    }
}

/// How a program is run: its exit status, its standard output and its
/// standard error, apart, so a token on the one never reaches the other.
pub(crate) trait Shell {
    fn run(&mut self, cmd: &mut Command) -> (i32, String, String);
}

pub(crate) struct Real;

impl Shell for Real {
    fn run(&mut self, cmd: &mut Command) -> (i32, String, String) {
        match cmd.stdin(Stdio::null()).output() {
            Ok(o) => (
                super::code(o.status),
                String::from_utf8_lossy(&o.stdout).into_owned(),
                String::from_utf8_lossy(&o.stderr).into_owned(),
            ),
            Err(e) => (127, String::new(), e.to_string()),
        }
    }
}

fn tehasbot(home: &Path) -> PathBuf {
    home.join("bin/tehasbot")
}

/// The product checkout the proof pushes from, when this host has one and the
/// tehasbot that makes the token: TEHAS_WORK_REPO, as the settings gave it.
pub(crate) fn product(home: &Path) -> Option<PathBuf> {
    let tree = PathBuf::from(env::var_os("TEHAS_WORK_REPO").filter(|t| !t.is_empty())?);
    let bot = std::fs::metadata(tehasbot(home));
    let runs = bot.is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    (runs && tree.join(".git").exists()).then_some(tree)
}

/// The last lines of what a program said, masked, with a line of its own when
/// it said nothing.
fn last(said: &str, what: &str) -> Vec<String> {
    let lines: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return vec![format!("{what} said nothing")];
    }
    lines[lines.len().saturating_sub(SHOWN)..]
        .iter()
        .map(|l| mask(l.trim_end()))
        .collect()
}

/// The dry-run push, with the token in its environment and no helper but this
/// command's.
fn push(tree: &Path, token: &str) -> Command {
    let mut git = timed(PUSH_LIMIT, "git");
    git.arg("-C")
        .arg(tree)
        .args(["-c", "credential.helper=", "-c", HELPER])
        .args(["push", "--dry-run", "origin", REFSPEC])
        .env(TOKEN_VAR, token)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_ASKPASS")
        .env_remove("SSH_ASKPASS")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_PARAMETERS");
    git
}

/// Make the cycle token and push with it once.
pub(crate) fn prove(home: &Path, tree: &Path, sh: &mut impl Shell) -> Proof {
    prove_of(home, tree, sh, tehas_core::read_only::on())
}

/// [`prove`] with the read-only setting given, not read, so a test asks for
/// each without changing the process's environment. A read-only instance's
/// token is still made, and then not pushed with.
fn prove_of(home: &Path, tree: &Path, sh: &mut impl Shell, read_only: bool) -> Proof {
    let bot = tehasbot(home);
    let (rc, login, _) = sh.run(timed(TOKEN_LIMIT, &bot).args(["github-token", "--login"]));
    let login = login.trim();
    let account = if rc == 0 && !login.is_empty() {
        login.to_string()
    } else {
        "the app (its login could not be read)".to_string()
    };
    let (rc, token, said) = sh.run(timed(TOKEN_LIMIT, &bot).args(["github-token", "--cycle"]));
    let token = token.trim();
    if rc != 0 || token.is_empty() {
        let what = format!("tehasbot github-token --cycle (exit {rc})");
        return Proof::NoToken(last(&said, &what));
    }
    if read_only {
        return Proof::ReadOnly(account);
    }
    let (rc, out, err) = sh.run(&mut push(tree, token));
    if rc == 0 {
        return Proof::Pushes(account);
    }
    // The token itself, wherever git repeats it.
    let mut said = format!("{out}\n{err}").replace(token, "***");
    if rc == 124 || rc == 137 {
        said.push_str(&format!(
            "\nthe dry-run push gave no answer in {PUSH_LIMIT}s"
        ));
    }
    Proof::Refused(
        account,
        last(&said, &format!("git push --dry-run (exit {rc})")),
    )
}

/// What the app has to be given for the token to push.
fn fix(repo: &str) -> Vec<String> {
    vec![
        format!("  The cycle token is the GitHub App's, narrowed to {repo} with Contents and Pull requests write and Issues read."),
        format!("  Install the app on {repo} with those permissions, under the account that owns {repo}, and accept the permissions it asks for; a ruleset must let it push tehas/ branches."),
        "  tehasbot github-token --cycle --check says what the token reaches; then run tehas update --check again.".to_string(),
    ]
}

/// The report's lines for a proof: `Cycle token:` and a word, then why.
pub(crate) fn lines(proof: &Proof, repo: &str) -> Vec<String> {
    let mut lines = match proof {
        Proof::Pushes(account) => {
            return vec![format!("Cycle token:  OK ({account} can push to origin)")]
        }
        Proof::ReadOnly(account) => {
            return vec![format!(
                "Cycle token:  OK (read-only: {account} may not push, so no push was asked)"
            )]
        }
        Proof::NoToken(said) => {
            let mut l = vec!["Cycle token:  CANNOT PUSH (the app made no cycle token)".to_string()];
            l.extend(said.iter().map(|s| format!("    {s}")));
            l
        }
        Proof::Refused(account, said) => {
            let mut l = vec![format!(
                "Cycle token:  CANNOT PUSH ({account} was refused a dry-run push to origin)"
            )];
            l.extend(said.iter().map(|s| format!("    {s}")));
            l
        }
    };
    lines.extend(fix(repo));
    lines
}

/// The product repository the fix names.
pub(crate) fn repo() -> String {
    env::var("TEHAS_REPO")
        .ok()
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "the product repository".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// The answers a test gives, in order, and the calls it was asked.
    #[derive(Default)]
    struct Fake {
        answers: VecDeque<(i32, String, String)>,
        asked: Vec<(Vec<String>, Option<String>)>,
    }

    impl Fake {
        fn answering(answers: &[(i32, &str, &str)]) -> Fake {
            Fake {
                answers: answers
                    .iter()
                    .map(|(rc, o, e)| (*rc, o.to_string(), e.to_string()))
                    .collect(),
                ..Fake::default()
            }
        }
    }

    impl Shell for Fake {
        fn run(&mut self, cmd: &mut Command) -> (i32, String, String) {
            let words = std::iter::once(cmd.get_program())
                .chain(cmd.get_args())
                .map(|w| w.to_string_lossy().into_owned())
                .collect();
            let token = cmd
                .get_envs()
                .find(|(k, _)| *k == TOKEN_VAR)
                .and_then(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()));
            self.asked.push((words, token));
            self.answers
                .pop_front()
                .unwrap_or((127, String::new(), String::new()))
        }
    }

    #[test]
    fn a_read_only_instance_makes_its_token_and_asks_no_push() {
        let mut sh = Fake::answering(&[(0, "acme-factory[bot]\n", ""), (0, "ghs_cycle\n", "")]);
        let got = prove_of(Path::new("/h"), Path::new("/t"), &mut sh, true);
        assert_eq!(got, Proof::ReadOnly("acme-factory[bot]".into()));
        assert!(got.starts());
        assert_eq!(sh.asked.len(), 2, "{:?}", sh.asked);
        assert!(sh
            .asked
            .iter()
            .all(|(words, _)| !words.contains(&"push".to_string())));
        assert_eq!(
            lines(&got, "acme/widget"),
            ["Cycle token:  OK (read-only: acme-factory[bot] may not push, so no push was asked)"]
        );
    }

    #[test]
    fn a_read_only_instance_with_no_token_is_still_a_fault() {
        let mut sh = Fake::answering(&[(0, "acme-factory[bot]\n", ""), (1, "", "tehas: no key\n")]);
        let got = prove_of(Path::new("/h"), Path::new("/t"), &mut sh, true);
        assert!(matches!(got, Proof::NoToken(_)), "{got:?}");
        assert!(!got.starts());
    }

    const PUSH_403: &str = "remote: Write access to repository not granted.\nfatal: unable to access 'https://github.com/acme/widget.git/': The requested URL returned error: 403";

    #[test]
    fn the_cycle_token_is_made_then_pushed_with_by_its_environment_alone() {
        let mut sh = Fake::answering(&[
            (0, "acme-factory[bot]\n", ""),
            (0, "ghs_cycle\n", ""),
            (
                0,
                "",
                "To https://github.com/acme/widget.git\n * [new branch] HEAD -> tehas-preflight",
            ),
        ]);
        let got = prove_of(
            Path::new("/h"),
            Path::new("/h/repos/widget"),
            &mut sh,
            false,
        );
        assert_eq!(got, Proof::Pushes("acme-factory[bot]".into()));
        assert!(got.starts());
        let calls: Vec<&[String]> = sh.asked.iter().map(|(w, _)| &w[4..]).collect();
        assert_eq!(calls[0], ["/h/bin/tehasbot", "github-token", "--login"]);
        assert_eq!(calls[1], ["/h/bin/tehasbot", "github-token", "--cycle"]);
        assert_eq!(
            calls[2],
            [
                "git",
                "-C",
                "/h/repos/widget",
                "-c",
                "credential.helper=",
                "-c",
                HELPER,
                "push",
                "--dry-run",
                "origin",
                REFSPEC
            ]
        );
        // Each call is bounded, and the token is in no word, only in git's
        // environment.
        for (words, _) in &sh.asked {
            assert_eq!(words[0], "timeout");
            assert!(!words.iter().any(|w| w.contains("ghs_cycle")));
        }
        assert_eq!(sh.asked[2].1.as_deref(), Some("ghs_cycle"));
        assert_eq!(sh.asked[0].1, None);
    }

    #[test]
    fn the_helper_answers_only_a_get_with_the_token_of_its_environment() {
        let script = HELPER.strip_prefix("credential.helper=!").unwrap();
        // git runs the helper as `<script> <verb>` in a shell.
        let run = |verb: &str| {
            let out = Command::new("sh")
                .args(["-c", &format!("{script} {verb}")])
                .env(TOKEN_VAR, "ghs_x")
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap()
        };
        assert_eq!(run("get"), "username=x-access-token\npassword=ghs_x\n");
        assert_eq!(run("store"), "");
        assert_eq!(run("erase"), "");
    }

    #[test]
    fn a_refused_push_names_the_account_and_gits_last_lines_masked() {
        let mut sh = Fake::answering(&[
            (0, "acme-factory[bot]", ""),
            (0, "ghs_cycle", ""),
            (128, "", PUSH_403),
        ]);
        let got = prove_of(Path::new("/h"), Path::new("/t"), &mut sh, false);
        let Proof::Refused(account, said) = &got else {
            panic!("{got:?}");
        };
        assert_eq!(account, "acme-factory[bot]");
        assert_eq!(said[0], "remote: Write access to repository not granted.");
        assert!(said[1].ends_with("returned error: 403"));
        assert!(!got.starts());
        let lines = lines(&got, "acme/widget");
        assert_eq!(
            lines[0],
            "Cycle token:  CANNOT PUSH (acme-factory[bot] was refused a dry-run push to origin)"
        );
        assert_eq!(
            lines[1],
            "    remote: Write access to repository not granted."
        );
        assert!(lines
            .iter()
            .any(|l| l.contains("Install the app on acme/widget")));
        assert!(lines.last().unwrap().contains("tehas update --check"));
    }

    #[test]
    fn a_token_git_echoes_is_masked() {
        let mut sh = Fake::answering(&[
            (0, "a[bot]", ""),
            (0, "ghs_cycle", ""),
            (
                128,
                "",
                "fatal: https://x-access-token:ghs_abcdefghijklmnopqrstuvwxyz0123456789@github.com/a/b denied",
            ),
        ]);
        let Proof::Refused(_, said) = prove_of(Path::new("/h"), Path::new("/t"), &mut sh, false)
        else {
            panic!("not refused");
        };
        assert!(
            !said[0].contains("ghs_abcdefghijklmnopqrstuvwxyz0123456789"),
            "{said:?}"
        );
    }

    #[test]
    fn no_cycle_token_is_said_with_tehasbots_words_and_nothing_is_pushed() {
        let mut sh = Fake::answering(&[
            (0, "a[bot]", ""),
            (
                1,
                "",
                "tehas: GitHub's answer for the app's token held no token and expiry\n",
            ),
        ]);
        let got = prove_of(Path::new("/h"), Path::new("/t"), &mut sh, false);
        assert_eq!(
            got,
            Proof::NoToken(vec![
                "tehas: GitHub's answer for the app's token held no token and expiry".into()
            ])
        );
        assert_eq!(sh.asked.len(), 2);
        assert_eq!(
            lines(&got, "a/b")[..2],
            [
                "Cycle token:  CANNOT PUSH (the app made no cycle token)".to_string(),
                "    tehas: GitHub's answer for the app's token held no token and expiry".into()
            ]
        );
        // A token program that exits 0 with nothing is no token either.
        let mut sh = Fake::answering(&[(1, "", ""), (0, "\n", "")]);
        let got = prove_of(Path::new("/h"), Path::new("/t"), &mut sh, false);
        assert_eq!(
            got,
            Proof::NoToken(vec![
                "tehasbot github-token --cycle (exit 0) said nothing".into()
            ])
        );
    }

    #[test]
    fn a_push_that_never_answers_is_refused_with_the_wait_and_an_unread_login_is_said() {
        let mut sh = Fake::answering(&[(1, "", ""), (0, "ghs_cycle", ""), (124, "", "")]);
        let got = prove_of(Path::new("/h"), Path::new("/t"), &mut sh, false);
        assert_eq!(
            got,
            Proof::Refused(
                "the app (its login could not be read)".into(),
                vec![format!("the dry-run push gave no answer in {PUSH_LIMIT}s")]
            )
        );
    }

    #[test]
    fn a_push_is_ok_in_one_line() {
        assert_eq!(
            lines(&Proof::Pushes("a[bot]".into()), "a/b"),
            ["Cycle token:  OK (a[bot] can push to origin)"]
        );
    }
}
