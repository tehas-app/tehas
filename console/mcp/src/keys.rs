//! The API keys of the two settings, and the check of a request's bearer
//! token.
//!
//! [`Setting`] names the two settings, so the words of the environment, the
//! words of an error and the scope a key of the setting is given stand in one
//! place. `TEHAS_MCP_KEYS` holds the full keys, which are given every tool.
//! `TEHAS_MCP_DASHBOARD_KEYS` holds the keys of a program that must not reach
//! the sessions and their logs: such a key is served only the four of
//! [`crate::tools::DASHBOARD`], beside the catalog of the tools. A key in both
//! lists is refused, so no key's scope is a guess.
//!
//! The shape a key must have is the rule of [`listed`] alone; the check in
//! `console/src/mcp.rs` of the same two settings is a fast reading of it before the
//! container is started, and says nothing the server does not enforce.
//!
//! The server is still given `~/tehas/mcp` and `~/tehas/logs`: the scope
//! lives in the program, not in the mounts.

use crate::tools::{self, Entry, Scope};
use sha2::{Digest, Sha256};

const MIN_LEN: usize = 32;

/// One of the two settings that hold keys: its name in the environment, the
/// word of an error, and the scope a key of it is given. A third list would
/// be one more value here, and nothing else would change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    /// `TEHAS_MCP_KEYS`: every tool.
    Full,
    /// `TEHAS_MCP_DASHBOARD_KEYS`: the four of the dashboard.
    Dashboard,
}

impl Setting {
    /// The name of the environment variable the list is read from, which is
    /// also the name an error gives it.
    pub fn name(self) -> &'static str {
        match self {
            Setting::Full => "TEHAS_MCP_KEYS",
            Setting::Dashboard => "TEHAS_MCP_DASHBOARD_KEYS",
        }
    }

    /// The scope a key of this setting is given.
    pub fn scope(self) -> Scope {
        match self {
            Setting::Full => Scope::Full,
            Setting::Dashboard => Scope::Dashboard,
        }
    }

    /// The two settings, the full one first: the order the lists are read in
    /// and the order an error names them.
    pub const ALL: [Setting; 2] = [Setting::Full, Setting::Dashboard];
}

/// The keys of both settings, each with its scope.
pub struct Keys(Vec<Entry>);

// Never the keys themselves, not even in a test's failure message.
impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let dashboard = self.0.iter().filter(|e| e.is_dashboard()).count();
        write!(
            f,
            "Keys({} full, {} dashboard)",
            self.0.len() - dashboard,
            dashboard
        )
    }
}

/// The keys of both settings: each one's raw value, either of which may be
/// empty, paired with the setting it was read from, in either order. At least
/// one of the two must hold a key. The error names the setting and a key by
/// its position, never by its value.
pub fn parse_keys(of: [(&'static Setting, &str); 2]) -> Result<Keys, String> {
    let [first, second] = of;
    if first.0 == second.0 {
        return Err(format!(
            "{} is given twice: a list goes with its own setting",
            first.0.name()
        ));
    }
    // By the setting, never by the place: a swapped pair must not give the
    // dashboard list the scope of the full one.
    let (full, dashboard) = if *first.0 == Setting::Full {
        (first.1, second.1)
    } else {
        (second.1, first.1)
    };
    let full_list = listed(full, Setting::Full)?;
    let dashboard_list = listed(dashboard, Setting::Dashboard)?;
    if full_list.is_empty() && dashboard_list.is_empty() {
        return Err(format!(
            "{} and {} are both empty: give at least one key in either",
            Setting::Full.name(),
            Setting::Dashboard.name(),
        ));
    }
    // Every key of one list is compared with every key of the other, so the
    // time taken says nothing about which list a key is in. The two lists are
    // the operator's own file and this runs once, at start-up, before the
    // container and before any request: the constant-time comparison of the
    // request path, where a key is looked for among the whole store, is
    // `Keys::check`. The message names the settings and the positions, never
    // the key.
    let full = &full_list;
    let dashboard = &dashboard_list;
    for (i, k) in full.iter().enumerate() {
        for (j, other) in dashboard.iter().enumerate() {
            if same(k.as_bytes(), other.as_bytes()) {
                return Err(format!(
                    "{}: key {} is {}: key {}: \
                     a key is in one list or the other",
                    Setting::Full.name(),
                    i + 1,
                    Setting::Dashboard.name(),
                    j + 1,
                ));
            }
        }
    }
    let mut keys: Vec<Entry> = Setting::ALL
        .into_iter()
        .zip([full_list, dashboard_list])
        .flat_map(|(setting, list)| {
            list.into_iter().map(move |key| Entry {
                key,
                scope: setting.scope(),
            })
        })
        .collect();
    // The order of the entries is the server's, never the settings', and it
    // is taken from the fingerprint rather than the key's own bytes, so no
    // comparison of a request's token with a stored key reads a key of one
    // list differently from one of the other.
    keys.sort_unstable_by_key(|e| (fingerprint(&e.key), e.key.len()));
    Ok(Keys(keys))
}

/// One list of keys, each checked: at least [`MIN_LEN`] characters of
/// `A-Za-z0-9_-`. `console/src/mcp.rs` reads the same rule for the same two
/// settings before it starts the container; this is the rule that stands. The error
/// names the setting and the key's place in it, never the key.
fn listed(raw: &str, of: Setting) -> Result<Vec<String>, String> {
    let keys: Vec<String> = raw.split_whitespace().map(str::to_string).collect();
    for (i, k) in keys.iter().enumerate() {
        let ok = k.len() >= MIN_LEN
            && k.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            return Err(format!(
                "{}: key {} is not at least {MIN_LEN} characters of A-Za-z0-9_-",
                of.name(),
                i + 1,
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

    /// The key an Authorization header carries, with its fingerprint and the
    /// scope it was listed under, or None.
    pub fn check(&self, authorization: Option<&str>) -> Option<Authenticated> {
        let h = authorization?;
        let (scheme, token) = h.split_at_checked(7)?;
        if !scheme.eq_ignore_ascii_case("bearer ") {
            return None;
        }
        // Every key is compared in full, so the time taken does not say which
        // key or how much of one matched.
        let mut found = None;
        for e in &self.0 {
            if same(e.key.as_bytes(), token.as_bytes()) {
                found = Some(Authenticated {
                    fingerprint: fingerprint(&e.key),
                    scope: e.scope,
                });
            }
        }
        found
    }
}

/// A request's key, as the key store knows it. Built only by [`Keys::check`],
/// so a scope carried by one of these is a scope a listed key was given: no
/// caller can name a scope the settings do not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authenticated {
    fingerprint: String,
    scope: Scope,
}

impl Authenticated {
    /// The short name of the key for the access log: eight hex characters of
    /// its hash, never the key.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// The scope the key store read this key under.
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Whether this key is served the tool: the rule of `tools::serves`
    /// over the scope of the key that made the request.
    pub fn may_call(&self, name: &str) -> bool {
        tools::serves(self.scope, name)
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
    // 31 characters of legal ones, so a list holding it is refused for its
    // length alone.
    const SHORT: &str = "abcdefghijklmnopqrstuvwxyz01234";
    // 33 characters with one that is not legal, so it is refused for its
    // characters alone.
    const ODD_CHAR: &str = "0123456789abcdefghijklmnopqrstuvwxyz+";

    // The two raw lists of a call to `parse_keys`, so the pair is written once
    // and a swap of the two is a swap of the two settings, not of two strings.
    fn of<'a>(full: &'a str, dashboard: &'a str) -> [(&'static Setting, &'a str); 2] {
        [(&Setting::Full, full), (&Setting::Dashboard, dashboard)]
    }

    // The holder of a key of one setting, read from the store itself so that
    // `may_call` is tested of a value the key store actually issues.
    fn holder(setting: Setting) -> Authenticated {
        let list = match setting {
            Setting::Full => K1,
            Setting::Dashboard => D1,
        };
        parse_keys(of(K1, D1))
            .expect("two lists of one key each")
            .check(Some(&format!("Bearer {list}")))
            .expect("a listed key")
    }

    #[test]
    fn no_key_in_either_list_is_refused_by_name() {
        for (full, dashboard) in [("", ""), ("  \n", ""), ("", " \t")] {
            let err = parse_keys(of(full, dashboard)).unwrap_err();
            assert!(err.contains("TEHAS_MCP_KEYS"), "{err}");
            assert!(err.contains("TEHAS_MCP_DASHBOARD_KEYS"), "{err}");
        }
    }

    #[test]
    fn a_dashboard_key_alone_is_enough() {
        let keys = parse_keys(of("", D1)).unwrap();
        assert_eq!(keys.len(), 1);
        let key = keys.check(Some(&format!("Bearer {D1}"))).unwrap();
        assert_eq!(key.scope(), Scope::Dashboard);
        assert_eq!(key.fingerprint().len(), 8);
        assert!(keys.check(Some(&format!("Bearer {K1}"))).is_none());
    }

    #[test]
    fn a_full_key_alone_is_enough() {
        let keys = parse_keys(of(K1, "")).unwrap();
        assert_eq!(
            keys.check(Some(&format!("Bearer {K1}"))).unwrap().scope(),
            Scope::Full
        );
    }

    #[test]
    fn each_list_keeps_its_own_scope() {
        let keys = parse_keys(of(&format!("{K1} {K2}"), D1)).unwrap();
        assert_eq!(keys.len(), 3);
        assert_eq!(
            keys.check(Some(&format!("Bearer {K2}"))).unwrap().scope(),
            Scope::Full
        );
        assert_eq!(
            keys.check(Some(&format!("Bearer {D1}"))).unwrap().scope(),
            Scope::Dashboard
        );
    }

    // The setting a list is paired with decides its keys' scope, not the place
    // of the pair: a swap must not give a dashboard key every tool.
    #[test]
    fn a_swapped_pair_keeps_each_list_its_own_scope() {
        let keys = parse_keys([(&Setting::Dashboard, D1), (&Setting::Full, K1)]).unwrap();
        assert_eq!(
            keys.check(Some(&format!("Bearer {D1}"))).unwrap().scope(),
            Scope::Dashboard
        );
        assert_eq!(
            keys.check(Some(&format!("Bearer {K1}"))).unwrap().scope(),
            Scope::Full
        );
    }

    #[test]
    fn a_setting_given_twice_is_refused() {
        for setting in &Setting::ALL {
            let err = parse_keys([(setting, K1), (setting, D1)]).unwrap_err();
            assert!(err.contains(setting.name()), "{err}");
            assert!(!err.contains(K1) && !err.contains(D1), "{err}");
        }
    }

    #[test]
    fn a_key_in_both_lists_is_refused_by_setting_and_position() {
        let err = parse_keys(of(&format!("{K1} {D1}"), &format!("{D1} {K2}"))).unwrap_err();
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
        assert!(parse_keys(of(&K1[..31], ""))
            .unwrap_err()
            .contains("TEHAS_MCP_KEYS"));
        // Refused for its length alone: every character of it is legal.
        let err = parse_keys(of("", &format!("{D1} {SHORT}"))).unwrap_err();
        assert!(err.contains("TEHAS_MCP_DASHBOARD_KEYS"), "{err}");
        assert!(err.contains("key 2"), "{err}");
        assert!(!err.contains(SHORT), "{err}");
    }

    #[test]
    fn a_key_with_other_characters_is_refused_without_its_value() {
        // 33 characters, so it is refused for its characters alone.
        let err = parse_keys(of(&format!("{K1} {ODD_CHAR}"), "")).unwrap_err();
        assert!(!err.contains(ODD_CHAR));
        assert!(err.contains("TEHAS_MCP_KEYS"));
        let err = parse_keys(of("", &format!("{D1} {ODD_CHAR}"))).unwrap_err();
        assert!(!err.contains(ODD_CHAR));
        assert!(err.contains("TEHAS_MCP_DASHBOARD_KEYS"));
        let bad = "abcdefghijklmnopqrstuvwxyz01234+";
        let err = parse_keys(of(&format!("{K1} {bad}"), "")).unwrap_err();
        assert!(!err.contains(bad));
        assert!(err.contains("TEHAS_MCP_KEYS"));
    }

    #[test]
    fn keys_are_split_on_any_whitespace() {
        assert_eq!(
            parse_keys(of(&format!("{K1}\t{K2}\n"), &format!("  {D1}  ")))
                .unwrap()
                .len(),
            3
        );
        assert_eq!(parse_keys(of("", &format!("{D1}\n{K2}"))).unwrap().len(), 2);
    }

    #[test]
    fn a_listed_key_is_named_by_a_short_fingerprint() {
        let keys = parse_keys(of(&format!("{K1} {K2}"), "")).unwrap();
        let key = keys.check(Some(&format!("Bearer {K1}"))).unwrap();
        assert_eq!(key.fingerprint().len(), 8);
        assert!(key.fingerprint().chars().all(|c| c.is_ascii_hexdigit()));
        // The same key by the other spelling of the scheme: the same holder,
        // the same fingerprint, and the scope of the list it stands in.
        let lower = keys.check(Some(&format!("bearer {K1}"))).expect("a scheme");
        assert_eq!(lower.fingerprint(), key.fingerprint());
        assert_eq!(lower.scope(), Scope::Full);
        assert!(keys.check(Some(&format!("Bearer {K2}"))).is_some());
    }

    #[test]
    fn anything_else_is_no_key() {
        let keys = parse_keys(of(K1, "")).unwrap();
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
        for name in tools::DASHBOARD {
            let dashboard = holder(Setting::Dashboard);
            let full = holder(Setting::Full);
            assert!(dashboard.may_call(name), "{name}");
            // A full key is unchanged: it is served every tool of the server.
            assert!(full.may_call(name), "{name}");
        }
        // A name the server has not is served to neither key; tests/rpc.rs
        // pins the answer.
        for setting in Setting::ALL {
            assert!(!holder(setting).may_call("factory_delete"), "{setting:?}");
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
            assert!(!holder(Setting::Dashboard).may_call(name), "{name}");
        }
    }

    #[test]
    fn the_debug_line_names_no_key() {
        let keys = parse_keys(of(K1, D1)).unwrap();
        let line = format!("{keys:?}");
        assert!(!line.contains(K1) && !line.contains(D1), "{line}");
        assert!(
            line.contains("1 full") && line.contains("1 dashboard"),
            "{line}"
        );
    }
}
