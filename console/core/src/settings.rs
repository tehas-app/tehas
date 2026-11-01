//! The settings a program needs of the instance, read from the environment
//! `tehas_load` leaves. The load has checked each value it holds, so all that
//! is left of `tehas_require` is whether the name is set: a command that needs
//! a setting stops with 78 before it does anything else.

use std::env;

/// A name set to nothing, or to spaces only, as `tehas_require` counts it.
fn first_unset<'a>(names: &[&'a str], get: impl Fn(&str) -> String) -> Option<&'a str> {
    names
        .iter()
        .copied()
        .find(|name| get(name).trim_matches(' ').is_empty())
}

/// Every name set, or the status to exit with, 78, the name said on standard
/// error in the words of `tehas_require`.
pub fn require(names: &[&str]) -> Result<(), u8> {
    let get = |name: &str| env::var_os(name).map_or(String::new(), |v| v.to_string_lossy().into());
    match first_unset(names, get) {
        Some(name) => {
            let dir = get("TEHAS_INSTANCE_DIR");
            let dir = if dir.is_empty() { "?" } else { &dir };
            eprintln!("tehas: {name} is not set (see {dir}/instance.env)");
            Err(78)
        }
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(name: &str) -> String {
        match name {
            "TEHAS_REPO" => "acme/widget".to_string(),
            "SPACES" => "   ".to_string(),
            _ => String::new(),
        }
    }

    #[test]
    fn every_name_set_is_no_refusal() {
        assert_eq!(first_unset(&["TEHAS_REPO"], given), None);
        assert_eq!(first_unset(&[], given), None);
    }

    #[test]
    fn the_first_name_empty_or_only_spaces_is_the_one_said() {
        assert_eq!(
            first_unset(&["TEHAS_REPO", "EMPTY", "SPACES"], given),
            Some("EMPTY")
        );
        assert_eq!(
            first_unset(&["SPACES", "TEHAS_REPO"], given),
            Some("SPACES")
        );
    }

    #[test]
    fn a_name_not_set_is_78() {
        assert_eq!(
            require(&["TEHAS_SETTINGS_TEST_NO_SUCH_NAME"]),
            Err(78),
            "exit 78, no fallback"
        );
    }
}
