//! The rules of an outside page: its id and its URL. `tehas host-message`
//! checks its links with them. They are the rules of 1.x's `tehas panel`,
//! kept for the 2.1 `frame` surface
//! (docs/specs/2026-11-22-plugin-surfaces-2.1.md, section 4).

const ID_MAX: usize = 32;
const URL_MAX: usize = 512;

/// The words of the sandbox an outside page is framed in, as the dock of
/// `host/fun/dock.js` frames one.
pub const SANDBOX: &str =
    "allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox";

/// An id is 1 to 32 characters of `a-z`, `0-9` and `-`.
pub fn id_ok(id: &str) -> bool {
    (1..=ID_MAX).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The host of an `http://` or `https://` URL with no user or password, in
/// lower case and with no trailing dot, or why the URL is refused.
pub fn url_host(url: &str) -> Result<String, &'static str> {
    if url.len() > URL_MAX {
        return Err("the url is longer than 512 characters");
    }
    if !url.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("the url holds a space, a control character or a character outside ASCII");
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or("the url starts with neither https:// nor http://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err("the url holds a user or a password");
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.contains(']') => (h, Some(p)),
        _ => (authority, None),
    };
    if port.is_some_and(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return Err("the url's port is not a number");
    }
    let named = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && host.split('.').any(|l| !l.is_empty());
    let literal = host.len() > 2
        && host.starts_with('[')
        && host.ends_with(']')
        && host[1..host.len() - 1]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.');
    if !named && !literal {
        return Err("the url names no host");
    }
    Ok(host.trim_end_matches('.').to_ascii_lowercase())
}

/// Whether a host is the factory's own domain, which the dashboard is served
/// from: a sibling name is another origin, so it is allowed.
pub fn is_own(host: &str, domain: &str) -> bool {
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    !domain.is_empty() && host == domain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_lower_case_digits_and_hyphens() {
        for good in ["a", "support", "a-1", &"x".repeat(32)] {
            assert!(id_ok(good), "{good}");
        }
        for bad in ["", "A", "a_b", "a b", "é", &"x".repeat(33)] {
            assert!(!id_ok(bad), "{bad}");
        }
    }

    #[test]
    fn a_url_is_http_or_https_with_a_host_and_no_user() {
        assert_eq!(
            url_host("https://Support.Example.org./f/acme"),
            Ok("support.example.org".into())
        );
        assert_eq!(
            url_host("http://example.org:8080?x=1"),
            Ok("example.org".into())
        );
        assert_eq!(url_host("https://[::1]:443/"), Ok("[::1]".into()));
        for bad in [
            "",
            "ftp://example.org/",
            "javascript:alert(1)",
            "HTTPS://example.org/",
            "https://",
            "https:///x",
            "https://user@example.org/",
            "https://u:p@example.org/",
            "https://example.org:x/",
            "https://exa mple.org/",
            "https://evil.org\\@example.org/",
            "https://example.org/\n",
            "https://ä.org/",
        ] {
            assert!(url_host(bad).is_err(), "{bad:?}");
        }
        let long = format!("https://example.org/{}", "x".repeat(493));
        assert_eq!(long.len(), 513);
        assert!(url_host(&long).is_err());
        assert!(url_host(&long[..512]).is_ok());
    }

    #[test]
    fn the_own_domain_is_refused_and_a_sibling_is_not() {
        assert!(is_own("example.org", "example.org"));
        assert!(is_own("example.org", "Example.org."));
        assert!(!is_own("support.example.org", "example.org"));
        assert!(!is_own("example.org", ""));
    }

    #[test]
    fn the_sandbox_lets_the_page_run_but_never_move_this_one() {
        assert!(SANDBOX.contains("allow-scripts"));
        assert!(!SANDBOX.contains("allow-top-navigation"));
        let dock = include_str!("../../../host/fun/dock.js");
        assert!(
            dock.contains(&format!("\"{SANDBOX}\"")),
            "dock.js frames otherwise"
        );
    }
}
