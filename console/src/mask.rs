//! Masks tokens, database passwords and private network addresses.
//!
//! An agent can print its environment and the console file is served by the
//! dashboard, which may be on the internet. The mask is a net for accidents,
//! not a wall: an agent that means to can print a secret in a form no pattern
//! knows.

use regex::Regex;
use std::sync::LazyLock;

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a pattern of the mask is not a regular expression")
}

static GITHUB_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?-u:\b)(gh[pousr]_|github_pat_)[A-Za-z0-9_]{16,}"));
static POSTGRES_PASSWORD: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(postgres(?:ql)?://[^:/\s]+:)[^@\s]+@"));
static SECRET_ASSIGNMENT: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?-u:\b)([A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|API_KEY)[A-Z0-9_]*=)\S+"));
// The private IPv4 ranges, the tailnet range and link-local. What stands
// before and after a match is checked in private_addresses.
static PRIVATE_ADDRESS: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?:10\.[0-9]{1,3}|192\.168|172\.(?:1[6-9]|2[0-9]|3[01])|169\.254|100\.(?:6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7]))\.[0-9]{1,3}\.[0-9]{1,3}",
    )
});
static TAILNET_NAME: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?-u:\b)[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.ts\.net(?-u:\b)"));

/// A private network address: an agent can read the container's hosts file,
/// and a connection error names the address it failed to reach. An address
/// counts only when it does not continue a longer number, so a version such
/// as 1.10.2.3 is left alone.
fn private_addresses(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut copied = 0;
    for m in PRIVATE_ADDRESS.find_iter(s) {
        let before = m.start().checked_sub(1).map(|i| bytes[i]);
        let after = bytes.get(m.end()).copied();
        if before.is_some_and(|b| b.is_ascii_digit() || b == b'.')
            || after.is_some_and(|b| b.is_ascii_digit())
        {
            continue;
        }
        out.push_str(&s[copied..m.start()]);
        out.push_str("[private address]");
        copied = m.end();
    }
    out.push_str(&s[copied..]);
    out
}

pub fn mask(s: &str) -> String {
    let s = GITHUB_TOKEN.replace_all(s, "${1}***");
    let s = POSTGRES_PASSWORD.replace_all(&s, "${1}***@");
    let s = SECRET_ASSIGNMENT.replace_all(&s, "${1}***");
    let s = private_addresses(&s);
    TAILNET_NAME.replace_all(&s, "[private name]").into_owned()
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
