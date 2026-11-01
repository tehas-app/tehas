//! Which pi-rukas commit a host follows, and the base image named for it.
//!
//! `lib/rukas.sh` keeps the same reading for what runs before a console
//! exists, bootstrap.sh and scripts/tag-release.sh. test/rukas_test.sh runs
//! the two side by side, so they cannot drift apart.
//!
//! The call upstream is the caller's: this holds the rules, and a call that
//! gets no answer is `None`.

/// The commit `follow` names in `repo`, and what it is, in the words of
/// `tehas_rukas_target`. `ls_remote` is given the arguments of `git ls-remote`.
/// `release` takes the newest vX.Y.Z tag, `release:<line>` the newest of that
/// line, `branch:<name>` the head of that branch, and `commit:<40 hex>` that
/// commit with no call upstream. `None` when upstream does not answer, has no
/// such tag or branch, or `follow` is none of these.
pub fn target(
    follow: &str,
    repo: &str,
    mut ls_remote: impl FnMut(&[&str]) -> Option<String>,
) -> Option<(String, String)> {
    let (commit, what) = if follow == "release" || follow.starts_with("release:") {
        let line = follow.strip_prefix("release:");
        if !line.is_none_or(line_shaped) {
            return None;
        }
        release(&ls_remote(&["--tags", repo])?, line.unwrap_or(""))?
    } else if let Some(name) = follow.strip_prefix("branch:").filter(|n| !n.is_empty()) {
        let refs = ls_remote(&[repo, &format!("refs/heads/{name}")])?;
        let commits: Vec<&str> = refs
            .lines()
            .map(|l| l.split('\t').next().unwrap_or(""))
            .collect();
        (one(commits)?, format!("the head of {name}"))
    } else if let Some(commit) = follow.strip_prefix("commit:") {
        // Not looked up: a commit upstream never published has no image, and
        // the build says so.
        (commit.to_string(), "pinned".to_string())
    } else {
        return None;
    };
    commit_shaped(&commit).then_some((commit, what))
}

/// The base image of the sandbox build, from the text of versions.env: the
/// image upstream publishes for the commit the host follows, tagged by its
/// first twelve digits, or the digest `PI_RUKAS_IMAGE_DIGEST` names, which
/// is what a release pins its base to. An instance's own line is not read.
/// `None` when the pins name no base or the lookup failed: a name built from
/// nothing would read in the log as an image that was never pulled.
pub fn base_image(
    versions: &str,
    ls_remote: impl FnMut(&[&str]) -> Option<String>,
) -> Option<String> {
    let follow = pin(versions, "PI_RUKAS_FOLLOW", path_shaped);
    let digest = pin(versions, "PI_RUKAS_IMAGE_DIGEST", digest_shaped);
    let repo = pin(versions, "PI_RUKAS_REPO", path_shaped);
    let path = repo
        .strip_prefix("https://github.com/")
        .filter(|p| p.contains('/'))?;
    if !digest.is_empty() && !digest.starts_with("sha256:") {
        return None;
    }
    if follow.is_empty() && digest.is_empty() {
        return None;
    }
    let path = path.strip_suffix(".git").unwrap_or(path);
    let path = path.to_ascii_lowercase();
    if !digest.is_empty() {
        return Some(format!("ghcr.io/{path}@{digest}"));
    }
    let (commit, _) = target(follow, repo, ls_remote)?;
    Some(format!("ghcr.io/{path}:sha-{}", &commit[..12]))
}

/// The value of the last line of `name` whose value has the right shape,
/// empty when there is none: a value of another shape names an image that
/// cannot exist.
fn pin<'a>(versions: &'a str, name: &str, shaped: fn(&str) -> bool) -> &'a str {
    versions
        .split('\n')
        .filter_map(|l| l.strip_prefix(name)?.strip_prefix('='))
        .rfind(|v| shaped(v))
        .unwrap_or("")
}

fn path_shaped(v: &str) -> bool {
    v.bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'.' | b'/' | b'-'))
}

fn digest_shaped(v: &str) -> bool {
    v.len() == 71
        && v.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b':')
}

fn commit_shaped(c: &str) -> bool {
    c.len() == 40 && c.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn digits(s: &str) -> Option<u32> {
    let shaped = (1..=6).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit());
    shaped.then(|| s.parse().ok())?
}

/// `0`, or `0.14`: a major version, or a major and a minor.
fn line_shaped(line: &str) -> bool {
    let mut parts = line.split('.');
    let major = parts.next().and_then(digits);
    let minor = parts.next().is_none_or(|m| digits(m).is_some());
    major.is_some() && minor && parts.next().is_none()
}

/// The three numbers of a tag named vX.Y.Z and nothing else: a tag with a
/// suffix, such as v1.0.0-rc1, is not a release.
fn version(tag: &str) -> Option<[u32; 3]> {
    let mut parts = tag.strip_prefix('v')?.split('.');
    let found = [
        digits(parts.next()?)?,
        digits(parts.next()?)?,
        digits(parts.next()?)?,
    ];
    parts.next().is_none().then_some(found)
}

/// The commit and the words of the newest release in the lines of
/// `git ls-remote --tags`, of `line` when it is not empty.
fn release(refs: &str, line: &str) -> Option<(String, String)> {
    let fields: Vec<(&str, &str)> = refs
        .lines()
        .map(|l| {
            let mut f = l.split_whitespace();
            (f.next().unwrap_or(""), f.next().unwrap_or(""))
        })
        .collect();
    let prefix = format!("v{line}.");
    let newest = fields
        .iter()
        .filter_map(|(_, name)| name.strip_prefix("refs/tags/"))
        .filter(|tag| line.is_empty() || tag.starts_with(&prefix))
        .filter_map(|tag| Some((version(tag)?, tag)))
        .max()?
        .1;
    let of = |name: &str| -> Vec<&str> {
        fields
            .iter()
            .filter(|(_, n)| *n == name)
            .map(|(c, _)| *c)
            .collect()
    };
    let name = format!("refs/tags/{newest}");
    // An annotated tag is listed twice, and its ^{} line is the commit.
    let peeled = of(&format!("{name}^{{}}"));
    let commits = if peeled.is_empty() { of(&name) } else { peeled };
    Some((one(commits)?, format!("release {newest}")))
}

/// The one commit of a listing. Two of them are no answer.
fn one(commits: Vec<&str>) -> Option<String> {
    match commits[..] {
        [commit] => Some(commit.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "https://github.com/Up-Owner/pi-rukas.git";

    fn hex(c: char) -> String {
        c.to_string().repeat(40)
    }

    fn tags(pairs: &[(&String, &str)]) -> String {
        pairs
            .iter()
            .map(|(c, n)| format!("{c}\trefs/tags/{n}\n"))
            .collect()
    }

    fn follow(what: &str, refs: &str) -> Option<(String, String)> {
        target(what, REPO, |_| Some(refs.to_string()))
    }

    fn silent(what: &str) -> Option<(String, String)> {
        target(what, REPO, |_| panic!("asked upstream"))
    }

    #[test]
    fn release_takes_the_newest_tag_by_version_not_by_name() {
        let (a, b, c) = (hex('a'), hex('b'), hex('c'));
        let refs = tags(&[(&a, "v0.12.9"), (&b, "v0.12.80"), (&c, "v0.12.8")]);
        let got = follow("release", &refs).unwrap();
        assert_eq!(got, (b, "release v0.12.80".to_string()));
    }

    #[test]
    fn release_asks_for_the_tags_of_the_repository() {
        let mut asked = Vec::new();
        target("release", REPO, |args| {
            asked = args.iter().map(|a| a.to_string()).collect();
            None
        });
        assert_eq!(asked, ["--tags", REPO]);
    }

    #[test]
    fn release_passes_over_a_suffix_and_a_tag_that_is_not_a_version() {
        let (a, b, c) = (hex('a'), hex('b'), hex('c'));
        let refs = tags(&[(&a, "v0.12.80"), (&b, "v0.13.0-rc1"), (&c, "nightly")]);
        assert_eq!(follow("release", &refs).unwrap().0, a);
    }

    #[test]
    fn an_annotated_tag_is_the_commit_it_points_at() {
        let (a, b, c) = (hex('a'), hex('b'), hex('c'));
        let refs = tags(&[(&a, "v0.12.79"), (&b, "v0.12.80"), (&c, "v0.12.80^{}")]);
        assert_eq!(follow("release", &refs).unwrap().0, c);
    }

    #[test]
    fn release_with_no_version_tag_gives_nothing() {
        let a = hex('a');
        assert_eq!(follow("release", &tags(&[(&a, "nightly")])), None);
    }

    #[test]
    fn a_line_takes_the_newest_tag_of_that_line() {
        let (a, b, c, d) = (hex('a'), hex('b'), hex('c'), hex('d'));
        let refs = tags(&[
            (&a, "v0.14.0"),
            (&b, "v0.14.1"),
            (&c, "v0.15.0"),
            (&d, "v10.14.9"),
        ]);
        assert_eq!(follow("release:0.14", &refs).unwrap().0, b);
        assert_eq!(follow("release:0.15", &refs).unwrap().0, c);
        assert_eq!(follow("release:0.16", &refs), None);
    }

    #[test]
    fn a_major_takes_every_minor_of_it_and_no_other_major() {
        let (a, b, c, d) = (hex('a'), hex('b'), hex('c'), hex('d'));
        let refs = tags(&[
            (&a, "v2.3.0"),
            (&b, "v2.45.12"),
            (&c, "v3.0.0"),
            (&d, "v12.0.0"),
        ]);
        assert_eq!(follow("release:2", &refs).unwrap().0, b);
    }

    #[test]
    fn equal_numbers_fall_to_the_name_as_sort_v_does() {
        let (a, b) = (hex('a'), hex('b'));
        let refs = tags(&[(&a, "v0.1.1"), (&b, "v0.1.01")]);
        assert_eq!(follow("release", &refs).unwrap().0, a);
    }

    #[test]
    fn a_tag_listed_twice_is_no_answer() {
        let (a, b) = (hex('a'), hex('b'));
        assert_eq!(
            follow("release", &tags(&[(&a, "v1.0.0"), (&b, "v1.0.0")])),
            None
        );
    }

    #[test]
    fn a_branch_is_its_head() {
        let d = hex('d');
        let refs = format!("{d}\trefs/heads/main\n");
        let mut asked = Vec::new();
        let got = target("branch:main", REPO, |args| {
            asked = args.iter().map(|a| a.to_string()).collect();
            Some(refs.clone())
        });
        assert_eq!(got, Some((d, "the head of main".to_string())));
        assert_eq!(asked, [REPO, "refs/heads/main"]);
    }

    #[test]
    fn a_branch_no_one_has_gives_nothing() {
        assert_eq!(follow("branch:gone", ""), None);
    }

    #[test]
    fn upstream_that_does_not_answer_gives_nothing() {
        assert_eq!(target("release", REPO, |_| None), None);
        assert_eq!(target("branch:main", REPO, |_| None), None);
    }

    #[test]
    fn a_value_of_another_shape_is_refused_without_asking_upstream() {
        for v in [
            "main",
            "branch:",
            "",
            "latest",
            "release:",
            "release:x",
            "release:0.14.1",
            "release:v0",
            "release:1234567",
        ] {
            assert_eq!(silent(v), None, "{v}");
        }
    }

    #[test]
    fn a_commit_is_that_commit_and_asks_upstream_nothing() {
        let a = hex('a');
        let got = silent(&format!("commit:{a}"));
        assert_eq!(got, Some((a, "pinned".to_string())));
    }

    #[test]
    fn a_commit_that_is_not_40_lower_case_hex_is_refused() {
        let a = hex('a');
        for v in [
            format!("commit:{a}-"),
            format!("commit:{}", &a[..39]),
            "commit:".to_string(),
            format!("commit:{}", "A".repeat(40)),
            format!("commit:{}", "g".repeat(40)),
        ] {
            assert_eq!(silent(&v), None, "{v}");
        }
    }

    fn base(versions: &str) -> Option<String> {
        base_image(versions, |_| panic!("asked upstream"))
    }

    #[test]
    fn the_base_of_a_pinned_commit_is_named_by_twelve_digits_lower_case() {
        let got = base(&format!(
            "PI_RUKAS_REPO={REPO}\nPI_RUKAS_FOLLOW=commit:{}\n",
            hex('a')
        ));
        assert_eq!(
            got.as_deref(),
            Some("ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa")
        );
    }

    #[test]
    fn a_digest_is_the_name_and_nothing_is_looked_up() {
        let digest = format!("sha256:{}", "e".repeat(64));
        let got = base(&format!(
            "PI_RUKAS_REPO={REPO}\nPI_RUKAS_FOLLOW=release\nPI_RUKAS_IMAGE_DIGEST={digest}\n"
        ));
        assert_eq!(got, Some(format!("ghcr.io/up-owner/pi-rukas@{digest}")));
    }

    #[test]
    fn a_followed_release_is_looked_up_for_its_commit() {
        let refs = tags(&[(&hex('b'), "v1.2.3")]);
        let got = base_image(
            &format!("PI_RUKAS_REPO={REPO}\nPI_RUKAS_FOLLOW=release\n"),
            |_| Some(refs.clone()),
        );
        assert_eq!(
            got.as_deref(),
            Some("ghcr.io/up-owner/pi-rukas:sha-bbbbbbbbbbbb")
        );
    }

    #[test]
    fn a_lookup_that_fails_names_no_base() {
        let versions = format!("PI_RUKAS_REPO={REPO}\nPI_RUKAS_FOLLOW=release\n");
        assert_eq!(base_image(&versions, |_| None), None);
        assert_eq!(base_image(&versions, |_| Some(String::new())), None);
    }

    #[test]
    fn the_last_line_of_the_right_shape_is_the_pin() {
        let (a, b) = (hex('a'), hex('b'));
        let got = base(&format!(
            "PI_RUKAS_REPO={REPO}\nPI_RUKAS_FOLLOW=commit:{a}\nPI_RUKAS_FOLLOW=commit:{b}\nPI_RUKAS_FOLLOW=not quoted right\n"
        ));
        assert_eq!(
            got.as_deref(),
            Some("ghcr.io/up-owner/pi-rukas:sha-bbbbbbbbbbbb")
        );
    }

    #[test]
    fn pins_that_name_no_base_are_none() {
        let pinned = format!("PI_RUKAS_FOLLOW=commit:{}\n", hex('a'));
        for versions in [
            String::new(),
            format!("PI_RUKAS_REPO={REPO}\n"),
            pinned.clone(),
            format!("PI_RUKAS_REPO=https://example.org/o/n.git\n{pinned}"),
            format!("PI_RUKAS_REPO=https://github.com/o\n{pinned}"),
            format!(
                "PI_RUKAS_REPO={REPO}\nPI_RUKAS_IMAGE_DIGEST=md5:{}\n{pinned}",
                "e".repeat(67)
            ),
        ] {
            assert_eq!(base(&versions), None, "{versions}");
        }
    }

    #[test]
    fn a_digest_of_the_wrong_length_is_no_pin_and_the_commit_names_the_base() {
        let got = base(&format!(
            "PI_RUKAS_REPO={REPO}\nPI_RUKAS_IMAGE_DIGEST=sha256:abc\nPI_RUKAS_FOLLOW=commit:{}\n",
            hex('a')
        ));
        assert_eq!(
            got.as_deref(),
            Some("ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa")
        );
    }
}
