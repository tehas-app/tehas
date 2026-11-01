//! Logs should hold no secret. This is for the day one does: what has the shape
//! of a token is not recorded and not uploaded. The store and the improvement
//! loop share the one check.

fn run_of(b: &[u8], from: usize, keep: impl Fn(u8) -> bool) -> usize {
    b.get(from..)
        .map_or(0, |t| t.iter().take_while(|&&c| keep(c)).count())
}

/// The six shapes of a token.
pub fn secret(b: &[u8]) -> bool {
    let alnum = |c: u8| c.is_ascii_alphanumeric();
    for i in 0..b.len() {
        let t = &b[i..];
        if t.len() >= 4
            && t.starts_with(b"gh")
            && b"pousr".contains(&t[2])
            && t[3] == b'_'
            && run_of(b, i + 4, alnum) >= 36
        {
            return true;
        }
        if t.starts_with(b"github_pat_") && run_of(b, i + 11, |c| alnum(c) || c == b'_') >= 22 {
            return true;
        }
        if t.starts_with(b"sk-")
            && (i == 0 || !alnum(b[i - 1]))
            && run_of(b, i + 3, |c| alnum(c) || c == b'_' || c == b'-') >= 20
        {
            return true;
        }
        if t.len() >= 5
            && t.starts_with(b"xox")
            && b"abprs".contains(&t[3])
            && t[4] == b'-'
            && run_of(b, i + 5, |c| alnum(c) || c == b'-') >= 10
        {
            return true;
        }
        if t.starts_with(b"AKIA")
            && run_of(b, i + 4, |c| c.is_ascii_digit() || c.is_ascii_uppercase()) >= 16
        {
            return true;
        }
        if t.starts_with(b"-----BEGIN ") {
            let from = i + 11;
            let end = from + run_of(b, from, |c| c.is_ascii_uppercase() || c == b' ');
            let key = b"PRIVATE KEY";
            if end - from >= key.len() && b[..end].ends_with(key) && b[end..].starts_with(b"-----")
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_six_shapes_are_seen() {
        for shape in [
            format!("push with ghp_{} failed", "a".repeat(36)),
            format!("github_pat_{}", "a".repeat(22)),
            format!("{{\"apiKey\":\"sk-{}\"}}", "b".repeat(24)),
            format!("xoxb-{}", "1".repeat(10)),
            "AKIAABCDEFGH12345678".to_string(),
            "-----BEGIN OPENSSH PRIVATE KEY-----".to_string(),
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
        ] {
            assert!(!secret(line.as_bytes()), "{line}");
        }
    }
}
