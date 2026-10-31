//! The API keys of the two settings, and the check of a request's bearer
//! token.
//!
//! `TEHAS_MCP_KEYS` holds the full keys, which are given every tool.
//! `TEHAS_MCP_DASHBOARD_KEYS` holds the keys of a program that must not reach
//! the sessions and their logs: such a key is served only the tools of
//! `crate::tools::DASHBOARD`, the four that copy from `live/`. A key in both
//! lists is refused, so no key's scope is a guess.
//!
//! The server is still given `~/tehas/mcp` and `~/tehas/logs`: the scope
//! lives in the program, not in the mounts.

use crate::tools::DASHBOARD;
use sha2::{Digest, Sha256};

const MIN_LEN: usize = 32;

/// What a key's holder may call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every tool the server has.
    Full,
    /// The four of `DASHBOARD`, and no other.
    Dashboard,
}

/// The server's own word for a key's scope, for the access log.
impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Full => "full",
            Scope::Dashboard => "dashboard",
        }
    }
}

/// The keys of both settings, each with its scope.
pub struct Keys(Vec<(String, Scope)>);

// Never the keys themselves, not even in a test's failure message.
impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let dashboard = self
            .0
            .iter()
            .filter(|(_, s)| *s == Scope::Dashboard)
            .count();
        write!(
            f,
            "Keys({} full, {} dashboard)",
            self.0.len() - dashboard,
            dashboard
        )
    }
}

/// The keys of both settings: the raw value of `TEHAS_MCP_KEYS` and that of
/// `TEHAS_MCP_DASHBOARD_KEYS`, either of which may be empty. At least one of
/// the two must hold a key. The error names the setting and a key by its
/// position, never by its value.
pub fn parse_keys(full: &str, dashboard: &str) -> Result<Keys, String> {
    let full = listed(full, "TEHAS_MCP_KEYS")?;
    let dashboard = listed(dashboard, "TEHAS_MCP_DASHBOARD_KEYS")?;
    if full.is_empty() && dashboard.is_empty() {
        return Err(
            "TEHAS_MCP_KEYS and TEHAS_MCP_DASHBOARD_KEYS are both empty: \
                    give at least one key in either"
                .into(),
        );
    }
    // The whole of both lists is compared, so the time taken says nothing
    // about which list a key is in. The message names the settings and the
    // positions, never the key.
    for (i, k) in full.iter().enumerate() {
        for (j, other) in dashboard.iter().enumerate() {
            if same(k.as_bytes(), other.as_bytes()) {
                return Err(format!(
                    "TEHAS_MCP_KEYS: key {} is TEHAS_MCP_DASHBOARD_KEYS: key {}: \
                     a key is in one list or the other",
                    i + 1,
                    j + 1
                ));
            }
        }
    }
    let mut keys: Vec<(String, Scope)> = full
        .into_iter()
        .map(|k| (k, Scope::Full))
        .chain(dashboard.into_iter().map(|k| (k, Scope::Dashboard)))
        .collect();
    // The order of the entries is the server's, never the settings': a request
    // for a key of one scope is not answered sooner than one for a key of the
    // other, whichever list its owner wrote first.
    keys.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    Ok(Keys(keys))
}

/// One list of keys, each checked. The name is the setting's, for an error.
fn listed(raw: &str, name: &str) -> Result<Vec<String>, String> {
    let keys: Vec<String> = raw.split_whitespace().map(str::to_string).collect();
    for (i, k) in keys.iter().enumerate() {
        let ok = k.len() >= MIN_LEN
            && k.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            return Err(format!(
                "{name}: key {} is not at least {MIN_LEN} characters of A-Za-z0-9_-",
                i + 1
            ));
        }
    }
    Ok(keys)
}

impl Keys {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The fingerprint of the key an Authorization header carries, with the
    /// scope that key was listed under, or None.
    pub fn check(&self, authorization: Option<&str>) -> Option<(String, Scope)> {
        let h = authorization?;
        let (scheme, token) = h.split_at_checked(7)?;
        if !scheme.eq_ignore_ascii_case("bearer ") {
            return None;
        }
        // Every key is compared in full, so the time taken does not say which
        // key or how much of one matched.
        let mut found = None;
        for (k, scope) in &self.0 {
            if same(k.as_bytes(), token.as_bytes()) {
                found = Some((fingerprint(k), *scope));
            }
        }
        found
    }

    /// Whether a key of this scope is served the tool.
    pub fn serves(scope: Scope, name: &str) -> bool {
        scope == Scope::Full || DASHBOARD.contains(&name)
    }
}

fn same(a: &[u8], b: &[u8]) -> bool {
    let mut diff = (a.len() != b.len()) as u8;
    for (i, x) in a.iter().enumerate() {
        diff |= x ^ b.get(i).copied().unwrap_or(0);
    }
    diff == 0
}

fn fingerprint(key: &str) -> String {
    Sha256::digest(key.as_bytes())[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const K1: &str = "abcdefghijklmnopqrstuvwxyz012345";
    const K2: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ-_6789";
    const D1: &str = "0123456789abcdefghijklmnopqrstuvwxyz";
    // 33 characters, so a list holding it is refused: the shortest key is 32.
    const SHORT: &str = "0123456789abcdefghijklmnopqrstuvwxyz+";

    #[test]
    fn no_key_in_either_list_is_refused_by_name() {
        for (full, dashboard) in [("", ""), ("  \n", ""), ("", " \t")] {
            let err = parse_keys(full, dashboard).unwrap_err();
            assert!(err.contains("TEHAS_MCP_KEYS"), "{err}");
            assert!(err.contains("TEHAS_MCP_DASHBOARD_KEYS"), "{err}");
        }
    }

    #[test]
    fn a_dashboard_key_alone_is_enough() {
        let keys = parse_keys("", D1).unwrap();
        assert_eq!(keys.len(), 1);
        let (fp, scope) = keys.check(Some(&format!("Bearer {D1}"))).unwrap();
        assert_eq!(scope, Scope::Dashboard);
        assert_eq!(fp.len(), 8);
        assert!(keys.check(Some(&format!("Bearer {K1}"))).is_none());
    }

    #[test]
    fn a_full_key_alone_is_enough() {
        let keys = parse_keys(K1, "").unwrap();
        assert_eq!(
            keys.check(Some(&format!("Bearer {K1}"))).unwrap().1,
            Scope::Full
        );
    }

    #[test]
    fn each_list_keeps_its_own_scope() {
        let keys = parse_keys(&format!("{K1} {K2}"), D1).unwrap();
        assert_eq!(keys.len(), 3);
        assert_eq!(
            keys.check(Some(&format!("Bearer {K2}"))).unwrap().1,
            Scope::Full
        );
        assert_eq!(
            keys.check(Some(&format!("Bearer {D1}"))).unwrap().1,
            Scope::Dashboard
        );
    }

    #[test]
    fn a_key_in_both_lists_is_refused_by_setting_and_position() {
        let err = parse_keys(&format!("{K1} {D1}"), &format!("{D1} {K2}")).unwrap_err();
        assert!(err.contains("TEHAS_MCP_KEYS"), "{err}");
        assert!(err.contains("TEHAS_MCP_DASHBOARD_KEYS"), "{err}");
        // The second full key and the first dashboard one: those are the two
        // positions named, and no byte of the key is.
        assert!(err.contains("key 2"), "{err}");
        assert!(err.contains("key 1"), "{err}");
        assert!(!err.contains(D1), "{err}");
    }

    #[test]
    fn a_short_key_of_either_setting_is_refused() {
        assert!(parse_keys(&K1[..31], "")
            .unwrap_err()
            .contains("TEHAS_MCP_KEYS"));
        let err = parse_keys("", &format!("{D1} {SHORT}")).unwrap_err();
        assert!(err.contains("TEHAS_MCP_DASHBOARD_KEYS"), "{err}");
        assert!(err.contains("key 2"), "{err}");
    }

    #[test]
    fn a_key_with_other_characters_is_refused_without_its_value() {
        let bad = "abcdefghijklmnopqrstuvwxyz01234+";
        let err = parse_keys(&format!("{K1} {bad}"), "").unwrap_err();
        assert!(!err.contains(bad));
        assert!(err.contains("TEHAS_MCP_KEYS"));
        let err = parse_keys("", &format!("{D1} {bad}")).unwrap_err();
        assert!(!err.contains(bad));
        assert!(err.contains("TEHAS_MCP_DASHBOARD_KEYS"));
    }

    #[test]
    fn keys_are_split_on_any_whitespace() {
        assert_eq!(
            parse_keys(&format!("{K1}\t{K2}\n"), &format!("  {D1}  "))
                .unwrap()
                .len(),
            3
        );
        assert_eq!(parse_keys("", &format!("{D1}\n{K2}")).unwrap().len(), 2);
    }

    #[test]
    fn a_listed_key_is_named_by_a_short_fingerprint() {
        let keys = parse_keys(&format!("{K1} {K2}"), "").unwrap();
        let (fp, _) = keys.check(Some(&format!("Bearer {K1}"))).unwrap();
        assert_eq!(fp.len(), 8);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(
            keys.check(Some(&format!("bearer {K1}"))),
            Some((fp, Scope::Full))
        );
        assert!(keys.check(Some(&format!("Bearer {K2}"))).is_some());
    }

    #[test]
    fn anything_else_is_no_key() {
        let keys = parse_keys(K1, "").unwrap();
        for h in [
            format!("Basic {K1}"),
            format!("Bearer {K1} "),
            format!("Bearer  {K1}"),
            format!("Bearer {K1}x"),
            format!("Bearer {}", &K1[..31]),
            "Bearer".to_string(),
        ] {
            assert_eq!(keys.check(Some(&h)), None, "{h}");
        }
        assert_eq!(keys.check(None), None);
    }

    #[test]
    fn a_dashboard_key_is_served_only_its_four_tools() {
        for name in DASHBOARD {
            assert!(Keys::serves(Scope::Dashboard, name), "{name}");
            // A full key is unchanged: it is served every tool of the server.
            assert!(Keys::serves(Scope::Full, name), "{name}");
        }
        // A name the server has not is answered the same to either key; that
        // is the dispatch's guard, `known`, and tests/rpc.rs pins the answer.
        for scope in [Scope::Full, Scope::Dashboard] {
            assert!(!super::super::tools::known("factory_delete"), "{scope:?}");
        }
        for name in [
            "factory_console",
            "factory_issue",
            "factory_log",
            "factory_search",
            "factory_sessions",
            "factory_session",
            "factory_events",
            "factory_failures",
            "factory_outcomes",
            "factory_park",
            "factory_decisions",
            "factory_status_x",
            "",
            "Factory_Status",
        ] {
            assert!(!Keys::serves(Scope::Dashboard, name), "{name}");
        }
    }

    #[test]
    fn the_debug_line_names_no_key() {
        let keys = parse_keys(K1, D1).unwrap();
        let line = format!("{keys:?}");
        assert!(!line.contains(K1) && !line.contains(D1), "{line}");
        assert!(
            line.contains("1 full") && line.contains("1 dashboard"),
            "{line}"
        );
    }
}
