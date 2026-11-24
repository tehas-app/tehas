//! `CodeHost::remote`: where git fetches and pushes, built from the
//! configured repository with no request, as 1.x's finish builds it
//! (`console/cycle/src/finish/plan.rs`, `Plan::remote`):
//! `https://github.com/<owner>/<name>.git`.
//!
//! The remote carries no credential, in its text or beside it: the adapter
//! obtains and keeps none, and git authenticates a push as it does today,
//! through the credential its host or container is given. `with` is None
//! for both kinds of access.

use crate::GitHub;
use tehas_ports::{Access, GitRemote, Got, Outcome, Refusal, Remote, Unread};

impl GitHub {
    /// The https remote of this repository, for either access. A name that
    /// is not exactly `owner/name` of GitHub's characters is
    /// `Refused(Invalid)`.
    pub(super) fn git_remote(&self, access: Access) -> Got<Remote> {
        match access {
            Access::Fetch | Access::Push => {}
        }
        let (owner, name) = parts(&self.repo).ok_or(Outcome::Refused(Refusal::Invalid))?;
        let url = GitRemote::new(format!("https://github.com/{owner}/{name}.git"))
            .map_err(|_| Outcome::Unread(Unread::Malformed))?;
        Ok(Remote::new(url))
    }
}

/// The project `owner/name` a git remote of GitHub names, with no request:
/// `https://github.com/<owner>/<name>`, `git@github.com:<owner>/<name>` or
/// `ssh://git@github.com/<owner>/<name>`, each with or without `.git` and a
/// trailing `/`, the host in any case. None for any other remote: another
/// host, a path, a port, a remote that carries a credential, or a name
/// GitHub could not hold. The caller stops on None; nothing is guessed.
pub fn project_of(remote: &str) -> Option<String> {
    let rest = remote.trim();
    let path = if let Some(r) = strip_folded(rest, "https://github.com/") {
        r
    } else if let Some(r) = strip_folded(rest, "git@github.com:") {
        r
    } else {
        strip_folded(rest, "ssh://git@github.com/")?
    };
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = match path.len().checked_sub(4) {
        Some(at) if path.is_char_boundary(at) && path[at..].eq_ignore_ascii_case(".git") => {
            &path[..at]
        }
        _ => path,
    };
    let (owner, name) = parts(path)?;
    Some(format!("{owner}/{name}"))
}

/// `text` after `prefix`, the prefix's ASCII case folded.
fn strip_folded<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// `owner` and `name` of a repository GitHub could hold: one `/`; an owner
/// of letters, digits and inner single dashes, at most 39; a name of
/// letters, digits, `-`, `_` and `.`, at most 100, not `.` or `..`, not
/// ending in `.git`; neither starting with a dash.
pub(in crate::port) fn parts(repo: &str) -> Option<(&str, &str)> {
    let (owner, name) = repo.split_once('/')?;
    let owner_ok = !owner.is_empty()
        && owner.len() <= 39
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && !owner.contains("--");
    let name_ok = !name.is_empty()
        && name.len() <= 100
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !name.starts_with('-')
        && name != "."
        && name != ".."
        && !name.to_ascii_lowercase().ends_with(".git");
    (owner_ok && name_ok).then_some((owner, name))
}
