//! Masks tokens, database passwords and private network addresses.
//!
//! An agent can print its environment and the console file is served by the
//! dashboard, which may be on the internet. The mask is a net for accidents,
//! not a wall: an agent that means to can print a secret in a form no pattern
//! knows.

use regex::{Captures, Regex};
use std::sync::{LazyLock, OnceLock};

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a pattern of the mask is not a regular expression")
}

// A word begins where JavaScript's \b says it does, by ASCII letters only:
// the patterns came from a script, and a token that follows a letter such as
// "ä" is still a token.
static GITHUB_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?-u:\b)(gh[pousr]_|github_pat_)[A-Za-z0-9_]{16,}"));
static POSTGRES_PASSWORD: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(postgres(?:ql)?://[^:/\s]+:)[^@\s]+@"));
static SECRET_ASSIGNMENT: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?-u:\b)([A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|API_KEY)[A-Z0-9_]*=)\S+"));
// The private IPv4 ranges, the tailnet range and link-local, and the IPv6
// unique local range. The groups at either end take what would make the
// address part of something longer, see private_address.
static PRIVATE_IPV4: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"([0-9.])?(?:10\.[0-9]{1,3}|192\.168|172\.(?:1[6-9]|2[0-9]|3[01])|169\.254|100\.(?:6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7]))\.[0-9]{1,3}\.[0-9]{1,3}([0-9])?",
    )
});
static PRIVATE_IPV6: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)([0-9a-f:])?f[cd][0-9a-f]{2}(?::[0-9a-f]{0,4}){2,}"));
static TAILNET_NAME: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?-u:\b)[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.ts\.net(?-u:\b)"));

/// A private network address: an agent can read the container's hosts file,
/// and a connection error names the address it failed to reach. An address
/// counts only when it does not continue a longer number, so a version such
/// as 1.10.2.3 is left alone: a match that took a neighbour into one of its
/// groups is written back as it was.
fn private_address(found: &Captures) -> String {
    if found.iter().skip(1).any(|group| group.is_some()) {
        found[0].to_string()
    } else {
        "[private address]".to_string()
    }
}

pub fn mask(s: &str) -> String {
    let s = GITHUB_TOKEN.replace_all(s, "${1}***");
    let s = POSTGRES_PASSWORD.replace_all(&s, "${1}***@");
    let s = SECRET_ASSIGNMENT.replace_all(&s, "${1}***");
    let s = PRIVATE_IPV4.replace_all(&s, private_address);
    let s = PRIVATE_IPV6.replace_all(&s, private_address);
    let s = TAILNET_NAME.replace_all(&s, "[private name]");
    hide(PRIVATE.get().and_then(Option::as_ref), &s)
}

// Names the host knows and an agent can print: the host aliases, the model
// host, the bot's email. tehas work gives them with --private.
static PRIVATE: OnceLock<Option<Regex>> = OnceLock::new();

// A shorter name would mask ordinary words.
const SHORTEST_PRIVATE: usize = 4;

/// One pattern of the names, matched as text in any case, the longest first
/// so that a name is not masked in part. None when no name is kept.
fn private_names(names: &[String]) -> Result<Option<Regex>, String> {
    let own = "[private address] [private name] ***";
    let mut kept: Vec<&str> = names
        .iter()
        .map(|n| n.trim())
        .filter(|n| n.chars().count() >= SHORTEST_PRIVATE)
        // A name in the mask's own words would mask what the mask wrote.
        .filter(|n| !own.to_lowercase().contains(&n.to_lowercase()))
        .collect();
    if kept.is_empty() {
        return Ok(None);
    }
    kept.sort_by_key(|n| std::cmp::Reverse(n.len()));
    let alternation: Vec<String> = kept.iter().map(|n| regex::escape(n)).collect();
    Regex::new(&format!("(?i){}", alternation.join("|")))
        .map(Some)
        .map_err(|e| format!("console: --private: {e}"))
}

/// Sets the names for the rest of the run. Once: the first call wins.
pub fn set_private(names: &[String]) -> Result<(), String> {
    let _ = PRIVATE.set(private_names(names)?);
    Ok(())
}

fn hide(names: Option<&Regex>, s: &str) -> String {
    match names {
        Some(names) => names.replace_all(s, "[private name]").into_owned(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::mask;

    // Built here, not written out: scripts/check-generic.sh reads this file too.
    fn ip(parts: [u8; 4]) -> String {
        parts.map(|p| p.to_string()).join(".")
    }

    #[test]
    fn masks_a_fine_grained_github_token() {
        let out = mask("token is github_pat_ABC123DEF456GHI789JOK012 and done");
        assert_eq!(out, "token is github_pat_*** and done");
    }

    #[test]
    fn masks_classic_github_tokens() {
        for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
            assert_eq!(
                mask(&format!("x={prefix}ABCDEFGHIJKLMNOPQRSTUVWXYZ")),
                format!("x={prefix}***")
            );
        }
    }

    #[test]
    fn does_not_mask_a_short_or_incomplete_github_token_prefix() {
        // Fewer than 16 secret characters: the prefix is not a real token.
        assert_eq!(mask("github_pat_short123"), "github_pat_short123");
    }

    #[test]
    fn masks_the_password_in_a_postgres_url() {
        assert_eq!(
            mask("DB=postgres://tehas:supersecretvalue@db:5432/prod"),
            "DB=postgres://tehas:***@db:5432/prod"
        );
        assert_eq!(
            mask("DB=postgresql://user:hunter2@host:5432/db"),
            "DB=postgresql://user:***@host:5432/db"
        );
    }

    #[test]
    fn masks_key_value_secrets_for_token_like_names() {
        assert_eq!(mask("FOO_TOKEN=abcdef123456"), "FOO_TOKEN=***");
        assert_eq!(
            mask("DATABASE_PASSWORD=hunter2 x"),
            "DATABASE_PASSWORD=*** x"
        );
        assert_eq!(mask("GH_TOKEN=ghp_whatever"), "GH_TOKEN=***");
        assert_eq!(mask("CF_API_KEY=s3cr3t"), "CF_API_KEY=***");
    }

    #[test]
    fn a_secret_ends_at_white_space_of_any_kind() {
        assert_eq!(mask("A_TOKEN=abc\tnext"), "A_TOKEN=***\tnext");
        assert_eq!(mask("A_TOKEN=abc\u{a0}next"), "A_TOKEN=***\u{a0}next");
        assert_eq!(mask("A_SECRET=päivää and"), "A_SECRET=*** and");
    }

    #[test]
    fn leaves_ordinary_text_untouched() {
        let plain = "no secrets here, just a postgres://user@host:5432/db url";
        assert_eq!(mask(plain), plain);
    }

    #[test]
    fn masks_private_network_addresses() {
        assert_eq!(
            mask(&format!("models {} ok", ip([10, 1, 2, 3]))),
            "models [private address] ok"
        );
        assert_eq!(
            mask(&format!("{}:8086", ip([192, 168, 1, 10]))),
            "[private address]:8086"
        );
        assert_eq!(
            mask(&format!("at {}.", ip([172, 16, 0, 5]))),
            "at [private address]."
        );
        assert_eq!(mask(&ip([172, 31, 255, 1])), "[private address]");
        assert_eq!(
            mask(&format!("host_{}", ip([100, 100, 2, 3]))),
            "host_[private address]"
        );
        assert_eq!(mask(&ip([169, 254, 1, 1])), "[private address]");
        assert_eq!(
            mask(&format!(
                "{}:8086",
                ["node", "tail1234", "ts", "net"].join(".")
            )),
            "[private name]:8086"
        );
    }

    #[test]
    fn masks_unique_local_ipv6_addresses() {
        let address = ["fd7a", "115c", "a1e0", "", "1"].join(":");
        assert_eq!(
            mask(&format!("at [{address}]:80")),
            "at [[private address]]:80"
        );
        assert_eq!(mask(&address.to_uppercase()), "[private address]");
        // Not the end of a longer run of hexadecimal, and not a time.
        for s in [
            format!("0{address}"),
            format!("a:{address}"),
            ["fd12", "30"].join(":"),
        ] {
            assert_eq!(mask(&s), s);
        }
    }

    #[test]
    fn an_address_beside_another_or_inside_a_number_is_judged_alone() {
        let (a, b) = (ip([10, 0, 0, 1]), ip([10, 0, 0, 2]));
        assert_eq!(
            mask(&format!("{a} {b}")),
            "[private address] [private address]"
        );
        assert_eq!(
            mask(&format!("{a}:{b}")),
            "[private address]:[private address]"
        );
        for s in [format!("1.{a}"), format!("5{a}"), format!(".{a}")] {
            assert_eq!(mask(&s), s);
        }
        assert_eq!(mask(&format!("{a}.{b}")), format!("[private address].{b}"));
        assert_eq!(mask(&format!("ä{a}ä")), "ä[private address]ä");
    }

    #[test]
    fn masks_every_address_on_a_line() {
        let line = format!(
            "{} then {},{}",
            ip([10, 0, 0, 1]),
            ip([10, 0, 0, 2]),
            ip([192, 168, 0, 3])
        );
        assert_eq!(
            mask(&line),
            "[private address] then [private address],[private address]"
        );
    }

    #[test]
    fn leaves_public_and_documentation_addresses_and_versions_alone() {
        for s in [
            "192.0.2.7",
            "8.8.8.8",
            "172.32.0.1",
            "100.128.0.1",
            "version 1.10.2.3",
            "210.1.2.3",
            "127.0.0.1",
        ] {
            assert_eq!(mask(s), s);
        }
        // An address that runs on into more digits is a longer number.
        let long = format!("{}456", ip([10, 1, 2, 3]));
        assert_eq!(mask(&long), long);
    }
}

#[cfg(test)]
mod private_tests {
    use super::{hide, private_names};

    fn names(given: &[&str]) -> Vec<String> {
        given.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_name_the_host_knows_is_masked_wherever_it_is() {
        let known = private_names(&names(&["models.lan", "bot@example.com"])).unwrap();
        assert_eq!(
            hide(
                known.as_ref(),
                "curl http://models.lan:8086/v1 by bot@example.com."
            ),
            "curl http://[private name]:8086/v1 by [private name]."
        );
    }

    #[test]
    fn a_name_is_masked_in_any_case_and_as_text_not_a_pattern() {
        let known = private_names(&names(&["a.b-c"])).unwrap();
        assert_eq!(hide(known.as_ref(), "A.B-C axb-c"), "[private name] axb-c");
    }

    #[test]
    fn the_longer_of_two_names_is_masked_whole() {
        let known = private_names(&names(&["gpu1", "gpu1.lan"])).unwrap();
        assert_eq!(hide(known.as_ref(), "at gpu1.lan"), "at [private name]");
    }

    #[test]
    fn short_and_empty_names_are_left_out() {
        assert!(private_names(&names(&["", "abc", "  "])).unwrap().is_none());
        let known = private_names(&names(&["abc", "host1"])).unwrap();
        assert_eq!(hide(known.as_ref(), "abc host1"), "abc [private name]");
    }

    #[test]
    fn a_name_in_the_mask_s_own_words_is_left_out() {
        let known = private_names(&names(&["private", "name"])).unwrap();
        assert!(known.is_none());
    }

    #[test]
    fn without_names_nothing_changes() {
        assert_eq!(hide(None, "models.lan"), "models.lan");
    }
}
