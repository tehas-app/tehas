//! The marker reader's recorded corpus, `samples/markers-oracle.tsv`:
//! each body with the classes comrak gave its markers. Its layout is
//! known here alone; `markers_oracle.rs` records and checks it, and
//! `restraint_containment.rs` feeds its bodies to every writer.

use std::path::PathBuf;

/// The marker the corpus writes for `M`.
pub const M: &str = "<!-- tehas:finish issue=7 result=opened -->";

/// A body as the corpus file writes it: `M` the marker, and `\\`, `\n`,
/// `\r`, `\t`, `\u00a0` and `\ufeff` escaped. A backslash before any other
/// character stands for itself.
pub fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == 'M' {
            out.push_str(M);
            continue;
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.clone().next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.clone().skip(1).take(4).collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
                chars.nth(3);
            }
            _ => {
                out.push('\\');
                continue;
            }
        }
        chars.next();
    }
    out
}

/// The corpus file.
pub fn file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples/markers-oracle.tsv")
}

/// Each recorded body, as the class words of its markers and the body.
pub fn rows() -> Vec<(Vec<String>, String)> {
    std::fs::read_to_string(file())
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let (classes, body) = l.split_once('\t').unwrap();
            let classes = classes
                .split(',')
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect();
            (classes, unescape(body))
        })
        .collect()
}
