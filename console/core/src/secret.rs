//! Logs should hold no secret. This is for the day one does: what has the shape
//! of a token is not recorded and not uploaded. The store, the improvement loop
//! and the mask share the one list of shapes, so what the mask hides and what
//! the check refuses are the same tokens.

fn run_of(b: &[u8], from: usize, keep: impl Fn(u8) -> bool) -> usize {
    b.get(from..)
        .map_or(0, |t| t.iter().take_while(|&&c| keep(c)).count())
}

fn alnum(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}

fn base64url(c: u8) -> bool {
    alnum(c) || c == b'_' || c == b'-'
}

/// The end of a private key's header or footer whose words start at `from`,
/// just past its closing dashes: the words, then `PRIVATE KEY-----`.
fn key_line_end(b: &[u8], from: usize) -> Option<usize> {
    let end = from + run_of(b, from, |c| c.is_ascii_uppercase() || c == b' ');
    let key = b"PRIVATE KEY";
    (end - from >= key.len() && b[..end].ends_with(key) && b[end..].starts_with(b"-----"))
        .then_some(end + 5)
}

/// The token that starts at `i`: where its kept prefix ends and where it ends.
/// A GitHub token, a provider key (`sk-`), a Slack token, an AWS key id, a JWT
/// and a private key block. A token is a token whatever comes before it, as a
/// URL-encoded one follows a letter or a digit, except a provider key, whose
/// `sk-` also ends words such as task-.
fn at(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let t = &b[i..];
    let token = |keep: usize, n: usize, least: usize| (n >= least).then_some((keep, keep + n));
    if t.len() >= 4 && t.starts_with(b"gh") && b"pousr".contains(&t[2]) && t[3] == b'_' {
        return token(i + 4, run_of(b, i + 4, alnum), 16);
    }
    if t.starts_with(b"github_pat_") {
        return token(i + 11, run_of(b, i + 11, |c| alnum(c) || c == b'_'), 16);
    }
    if t.starts_with(b"sk-") && (i == 0 || !alnum(b[i - 1])) {
        return token(i + 3, run_of(b, i + 3, base64url), 20);
    }
    if t.len() >= 5 && t.starts_with(b"xox") && b"abprs".contains(&t[3]) && t[4] == b'-' {
        return token(i + 5, run_of(b, i + 5, |c| alnum(c) || c == b'-'), 10);
    }
    if t.starts_with(b"AKIA") {
        let n = run_of(b, i + 4, |c| c.is_ascii_digit() || c.is_ascii_uppercase());
        return token(i + 4, n, 16);
    }
    if t.starts_with(b"eyJ") {
        // A header and a payload, each a JSON object in base64url, and the
        // signature when there is one.
        let head = run_of(b, i + 3, base64url);
        let dot = i + 3 + head;
        if head < 8 || !b[dot..].starts_with(b".eyJ") {
            return None;
        }
        let body = run_of(b, dot + 4, base64url);
        if body < 8 {
            return None;
        }
        let mut end = dot + 4 + body;
        if b.get(end) == Some(&b'.') {
            end += 1 + run_of(b, end + 1, base64url);
        }
        return Some((i + 3, end));
    }
    if t.starts_with(b"-----BEGIN ") {
        // The block to its footer, or to the end of the text when the footer
        // is not in it.
        let keep = key_line_end(b, i + 11)?;
        let end = (keep..b.len())
            .find(|&j| b[j..].starts_with(b"-----END "))
            .and_then(|j| key_line_end(b, j + 9))
            .unwrap_or(b.len());
        return Some((keep, end));
    }
    None
}

/// Each token in `b`, in order and apart: where its kept prefix ends and where
/// it ends.
pub fn tokens(b: &[u8]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match at(b, i) {
            Some((keep, end)) => {
                found.push((keep, end));
                i = end.max(i + 1);
            }
            None => i += 1,
        }
    }
    found
}

/// Whether `b` holds a token of one of the shapes.
pub fn secret(b: &[u8]) -> bool {
    (0..b.len()).any(|i| at(b, i).is_some())
}

/// `s` with each token written as its prefix and `***`.
pub fn hide(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut from = 0;
    for (keep, end) in tokens(s.as_bytes()) {
        // Every shape starts and ends at an ASCII byte, so both are character
        // boundaries.
        out.push_str(&s[from..keep]);
        out.push_str("***");
        from = end;
    }
    out.push_str(&s[from..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJl";

    #[test]
    fn the_shapes_are_seen() {
        for shape in [
            format!("push with ghp_{} failed", "a".repeat(36)),
            format!("github_pat_{}", "a".repeat(22)),
            format!("{{\"apiKey\":\"sk-{}\"}}", "b".repeat(24)),
            format!("xoxb-{}", "1".repeat(10)),
            "AKIAABCDEFGH12345678".to_string(),
            "-----BEGIN OPENSSH PRIVATE KEY-----".to_string(),
            format!("jwt {JWT} expired"),
        ] {
            assert!(secret(shape.as_bytes()), "{shape}");
        }
    }

    #[test]
    fn a_word_at_the_start_of_a_line_counts_as_grep_counts_it() {
        assert!(secret(format!("x\nsk-{}", "b".repeat(20)).as_bytes()));
    }

    #[test]
    fn an_ordinary_log_is_not_taken_for_a_secret() {
        for line in [
            "the ghp_ prefix and sk-short are words here",
            "check disk-usage-of-the-container-first",
            r#"{"branch":"task-1234-add-the-login-page"}"#,
            "eyJhbGciOiJIUzI1NiJ9.e30.abc",
            "-----BEGIN PUBLIC KEY-----",
        ] {
            assert!(!secret(line.as_bytes()), "{line}");
        }
    }

    #[test]
    fn hide_keeps_the_prefix_and_the_text_around() {
        assert_eq!(
            hide("slack xoxb-1234-abcdefgh said no"),
            "slack xoxb-*** said no"
        );
        assert_eq!(
            hide("aws AKIAABCDEFGHIJKLMNOP denied"),
            "aws AKIA*** denied"
        );
        assert_eq!(hide(&format!("jwt {JWT} expired")), "jwt eyJ*** expired");
        assert_eq!(hide("é ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ é"), "é ghp_*** é");
    }

    #[test]
    fn hide_takes_a_private_key_block_whole() {
        let block =
            "key -----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJB\n-----END RSA PRIVATE KEY----- after";
        assert_eq!(hide(block), "key -----BEGIN RSA PRIVATE KEY-----*** after");
        let cut = "-----BEGIN PRIVATE KEY-----\nMIIBOgIBAAJB\nmore";
        assert_eq!(hide(cut), "-----BEGIN PRIVATE KEY-----***");
    }

    #[test]
    fn every_token_seen_is_hidden() {
        let text = format!(
            "a ghp_{} b sk-{} c AKIA{} d",
            "A".repeat(20),
            "x".repeat(20),
            "B".repeat(16)
        );
        assert!(secret(text.as_bytes()));
        let hidden = hide(&text);
        assert_eq!(hidden, "a ghp_*** b sk-*** c AKIA*** d");
        assert!(!secret(hidden.as_bytes()));
    }
}
