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
