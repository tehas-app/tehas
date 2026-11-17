//! The settings a contractor's job carries from the factory: the ones a cycle
//! is shaped by and the ones every load requires, never a host's address,
//! path or secret. The model's endpoint and key are not here: the models.json
//! a job carries puts every provider behind the factory's proxy. The list is
//! part of the release, not a setting (proxy/src/channel.rs sends it).

use super::rules::ROLES;

pub const CARRIED: &[&str] = &[
    "TEHAS_REPO",
    // Required by every load, so the contractor's `tehas work` loads: a
    // number, public logins and the App's public id, none of them a secret.
    "TEHAS_CONTROL_ISSUE",
    "TEHAS_ALLOWED_LABELERS",
    "TEHAS_GITHUB_APP_ID",
    "TEHAS_BRANCH",
    "TEHAS_DRIVER",
    "TEHAS_READ_ONLY",
    "TEHAS_WORK_TIMEOUT",
    "TEHAS_WORK_STALL",
    "TEHAS_CHILD_TIMEOUT",
    "TEHAS_WORKSTREAMS",
    "TEHAS_VERIFY_TIMEOUT",
    "TEHAS_BUILD_TIMEOUT",
    "TEHAS_RUKAS_WORKAROUNDS",
    "TEHAS_OUTSOURCE",
    "PI_ENSEMBLE_SUBAGENT_PROVIDER",
    "PI_ENSEMBLE_SUBAGENT_MODEL",
];

/// Whether a setting may leave the factory with a job: one of [`CARRIED`], or
/// a role's own model or provider.
pub fn carried(name: &str) -> bool {
    CARRIED.contains(&name)
        || ROLES.iter().any(|role| {
            name == format!("PI_ENSEMBLE_MODEL_{role}")
                || name == format!("PI_ENSEMBLE_PROVIDER_{role}")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_carried_name_is_a_ruled_setting_and_none_names_a_secret_or_a_host() {
        for name in CARRIED {
            assert!(
                super::super::rules::TABLE.iter().any(|s| s.name == *name),
                "{name} has no row in the settings table"
            );
            for word in ["KEY", "TOKEN", "SECRET", "URL", "ADDR", "HOST", "DIR"] {
                assert!(!name.contains(word), "{name}");
            }
        }
        assert!(carried("PI_ENSEMBLE_MODEL_DEVELOPER"));
        assert!(!carried("PI_ENSEMBLE_API_KEY_MAIN"));
        assert!(!carried("TEHAS_HOST_ADDR"));
    }
}
