//! The shape of a tehas version: a release vX.Y.Z, or a prerelease of one,
//! vX.Y.Z-alpha.N, vX.Y.Z-beta.N or vX.Y.Z-rc.N. The release workflow, the
//! tag step and the update read versions through this one parser.

/// The stage of a prerelease, in the order they come: an alpha before a beta
/// before a release candidate, and all of them before the release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Alpha,
    Beta,
    Rc,
}

impl Stage {
    pub const ALL: [Stage; 3] = [Stage::Alpha, Stage::Beta, Stage::Rc];

    pub fn name(self) -> &'static str {
        match self {
            Stage::Alpha => "alpha",
            Stage::Beta => "beta",
            Stage::Rc => "rc",
        }
    }

    pub fn parse(s: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|st| st.name() == s)
    }
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// The version before a prerelease suffix, its stage and its number N:
/// `v1.0.0-beta.2` is (`v1.0.0`, Beta, `2`). None without such a suffix.
/// The part before is not checked; [`is_tag`] checks the whole.
pub fn prerelease(version: &str) -> Option<(&str, Stage, &str)> {
    let (base, pre) = version.rsplit_once('-')?;
    let (stage, n) = pre.split_once('.')?;
    let stage = Stage::parse(stage)?;
    digits(n).then_some((base, stage, n))
}

/// vX.Y.Z, each part one or more digits.
pub fn is_release(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| digits(p))
}

/// A release, or a prerelease of one.
pub fn is_tag(tag: &str) -> bool {
    is_release(prerelease(tag).map_or(tag, |(base, _, _)| base))
}

/// A prerelease of a release: published as a GitHub prerelease, installed
/// only when it is named.
pub fn is_prerelease(tag: &str) -> bool {
    is_tag(tag) && prerelease(tag).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_a_release_or_one_of_three_prereleases_of_one() {
        for good in [
            "v0.7.6",
            "v10.0.123",
            "v1.0.0-alpha.1",
            "v1.0.0-beta.12",
            "v1.0.0-rc.3",
        ] {
            assert!(is_tag(good), "{good}");
        }
        for bad in [
            "",
            "v",
            "0.7.6",
            "V0.7.6",
            "v0.7",
            "v0.7.6.1",
            "v0..6",
            "v0.7.x",
            "v0.7.6-rc1",
            "v0.7.6-rc.",
            "v0.7.6-rc.x",
            "v0.7.6-rc.1.2",
            "v0.7.6-gamma.1",
            "v0.7.6-Alpha.1",
            "v0.7.6-alpha",
            "v0.7-alpha.1",
            "v0.7.6-x-alpha.1",
            "v0.7.6-",
        ] {
            assert!(!is_tag(bad), "{bad:?}");
        }
        assert!(is_release("v1.0.0"));
        assert!(!is_release("v1.0.0-alpha.1"));
        assert!(is_prerelease("v1.0.0-alpha.1"));
        assert!(is_prerelease("v1.0.0-beta.1"));
        assert!(is_prerelease("v1.0.0-rc.1"));
        assert!(!is_prerelease("v1.0.0"));
        assert!(!is_prerelease("v1.0-rc.1"));
    }

    #[test]
    fn a_prerelease_names_its_release_stage_and_number() {
        assert_eq!(
            prerelease("v1.0.0-beta.2"),
            Some(("v1.0.0", Stage::Beta, "2"))
        );
        assert_eq!(
            prerelease("1.0.0-alpha.10"),
            Some(("1.0.0", Stage::Alpha, "10"))
        );
        assert_eq!(prerelease("v1.0.0"), None);
        assert_eq!(prerelease("v1.0.0-rc"), None);
    }

    #[test]
    fn the_stages_come_alpha_beta_rc() {
        assert!(Stage::Alpha < Stage::Beta && Stage::Beta < Stage::Rc);
        for st in Stage::ALL {
            assert_eq!(Stage::parse(st.name()), Some(st));
        }
        assert_eq!(Stage::parse("gamma"), None);
    }
}
