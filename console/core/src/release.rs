//! Where releases of tehas come from and the key that signs them, built into
//! the program from host/versions.env and host/release/minisign.pub. A host
//! never takes either from what an earlier release left on it: a versions.env
//! or a key file there is written by the release it describes, so trusting it
//! would let one release choose every later one. The instance may name another
//! repository with TEHAS_RELEASE_REPO; the key stays the built-in one.

const VERSIONS: &str = include_str!("../../../host/versions.env");

/// The minisign public key every release is signed with.
pub const KEY: &str = include_str!("../../../host/release/minisign.pub");

/// owner/name on GitHub, each half of safe characters.
pub fn repo_ok(repo: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    };
    repo.split_once('/')
        .is_some_and(|(a, b)| part(a) && part(b))
}

/// The repository built in: the last TEHAS_RELEASE_REPO line of host/versions.env.
pub fn pinned_repo() -> &'static str {
    VERSIONS
        .lines()
        .filter_map(|l| l.strip_prefix("TEHAS_RELEASE_REPO="))
        .next_back()
        .unwrap_or("")
}

/// The instance's setting when it has one, else the built-in repository.
pub fn repo_of(setting: &str) -> &str {
    if setting.is_empty() {
        pinned_repo()
    } else {
        setting
    }
}

/// The repository releases are read from on this run: TEHAS_RELEASE_REPO, else
/// the built-in one. None when the setting is not owner/name.
pub fn repo() -> Option<String> {
    let setting = std::env::var("TEHAS_RELEASE_REPO").unwrap_or_default();
    let repo = repo_of(&setting);
    repo_ok(repo).then(|| repo.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_repository_is_owner_and_name() {
        assert!(repo_ok(pinned_repo()), "{}", pinned_repo());
    }

    #[test]
    fn the_built_in_key_is_a_minisign_public_key() {
        let mut lines = KEY.lines();
        assert!(lines
            .next()
            .is_some_and(|l| l.starts_with("untrusted comment: minisign public key")));
        assert!(lines.next().is_some_and(|l| l.starts_with("RW")));
    }

    #[test]
    fn the_instance_setting_wins_and_nothing_else_is_read() {
        assert_eq!(repo_of("someone/fork"), "someone/fork");
        assert_eq!(repo_of(""), pinned_repo());
    }

    #[test]
    fn a_repository_is_owner_and_name() {
        for good in ["a/b", "tehas-app/tehas", "A_1/b.c-d"] {
            assert!(repo_ok(good), "{good}");
        }
        for bad in ["", "a", "a/", "/b", "a/b/c", "a b/c", "a/b;c"] {
            assert!(!repo_ok(bad), "{bad}");
        }
    }
}
