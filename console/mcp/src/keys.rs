//! The API keys in TEHAS_MCP_KEYS, and the check of a request's bearer token.

use sha2::{Digest, Sha256};

const MIN_LEN: usize = 32;

pub struct Keys(Vec<String>);

// Never the keys themselves, not even in a test's failure message.
impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Keys({} keys)", self.0.len())
    }
}

/// The keys from the raw setting. The error names the setting and a key by
/// its position, never by its value.
pub fn parse_keys(raw: &str) -> Result<Keys, String> {
    let keys: Vec<String> = raw.split_whitespace().map(str::to_string).collect();
    if keys.is_empty() {
        return Err("TEHAS_MCP_KEYS is empty: give at least one key".into());
    }
    for (i, k) in keys.iter().enumerate() {
        let ok = k.len() >= MIN_LEN
            && k.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            return Err(format!(
                "TEHAS_MCP_KEYS: key {} is not at least {MIN_LEN} characters of A-Za-z0-9_-",
                i + 1
            ));
        }
    }
    Ok(Keys(keys))
}

impl Keys {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The fingerprint of the key an Authorization header carries, or None.
    pub fn check(&self, authorization: Option<&str>) -> Option<String> {
        let h = authorization?;
        let (scheme, token) = h.split_at_checked(7)?;
        if !scheme.eq_ignore_ascii_case("bearer ") {
            return None;
        }
        // Every key is compared in full, so the time taken does not say which
        // key or how much of one matched.
        let mut found = None;
        for k in &self.0 {
            if same(k.as_bytes(), token.as_bytes()) {
                found = Some(fingerprint(k));
            }
        }
        found
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

    #[test]
    fn an_empty_list_is_refused_by_name() {
        for raw in ["", "  \n"] {
            assert!(parse_keys(raw).unwrap_err().contains("TEHAS_MCP_KEYS"));
        }
    }

    #[test]
    fn a_short_key_is_refused() {
        assert!(parse_keys(&K1[..31]).is_err());
    }

    #[test]
    fn a_key_with_other_characters_is_refused_without_its_value() {
        let bad = "abcdefghijklmnopqrstuvwxyz01234+";
        let err = parse_keys(&format!("{K1} {bad}")).unwrap_err();
        assert!(!err.contains(bad));
        assert!(err.contains("TEHAS_MCP_KEYS"));
    }

    #[test]
    fn keys_are_split_on_any_whitespace() {
        assert_eq!(parse_keys(&format!("{K1}\t{K2}\n")).unwrap().len(), 2);
    }

    #[test]
    fn a_listed_key_is_named_by_a_short_fingerprint() {
        let keys = parse_keys(&format!("{K1} {K2}")).unwrap();
        let fp = keys.check(Some(&format!("Bearer {K1}"))).unwrap();
        assert_eq!(fp.len(), 8);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(keys.check(Some(&format!("bearer {K1}"))), Some(fp));
        assert!(keys.check(Some(&format!("Bearer {K2}"))).is_some());
    }

    #[test]
    fn anything_else_is_no_key() {
        let keys = parse_keys(K1).unwrap();
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
}
