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
    crate::settings::rules::is_repo(repo)
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

/// The repository's name before 2.0. GitHub redirects it to the new one until
/// the old name is reused, so an instance that still names it is read as it is.
pub const FORMER_REPO: &str = "tehas-app/tehas";

/// What an update says when its releases come from the former name.
pub fn moved(repo: &str) -> Option<String> {
    repo.eq_ignore_ascii_case(FORMER_REPO).then(|| {
        format!(
            "the repository {FORMER_REPO} is now {}; TEHAS_RELEASE_REPO still names the old one, which works only while GitHub redirects it: remove the line or name the new one",
            pinned_repo()
        )
    })
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
    fn the_built_in_repository_is_tehas_factory_and_only_the_former_name_is_said_to_have_moved() {
        assert_eq!(pinned_repo(), "tehas-app/tehas-factory");
        assert_eq!(FORMER_REPO, "tehas-app/tehas");
        assert!(repo_ok(FORMER_REPO));
        let said = moved("Tehas-App/tehas").unwrap();
        assert!(
            said.starts_with("the repository tehas-app/tehas is now tehas-app/tehas-factory;"),
            "{said}"
        );
        for other in [
            "tehas-app/tehas-factory",
            "tehas-app/tehas-industry",
            "someone/tehas",
            "",
        ] {
            assert_eq!(moved(other), None, "{other}");
        }
    }

    #[test]
    fn a_repository_is_owner_and_name() {
        for good in ["a/b", "tehas-app/tehas-factory", "A_1/b.c-d"] {
            assert!(repo_ok(good), "{good}");
        }
        for bad in ["", "a", "a/", "/b", "a/b/c", "a b/c", "a/b;c"] {
            assert!(!repo_ok(bad), "{bad}");
        }
    }
}
