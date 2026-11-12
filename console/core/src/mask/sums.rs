//! The secret values of the host, for a program that is given their sums and
//! not the values: the MCP server's container. A value is hidden wherever its
//! bytes stand in the text, as the host's mask hides it.
//!
//! The line the host gives is the salt, then one word per value:
//! `<bytes>:<filter>:<sum>`. The sum is SHA-256 of the salt and the value. The
//! filter is 16 bits of a rolling hash of the value, so the text is read once
//! per length and only a window that passes the filter is summed.

use sha2::{Digest, Sha256};
use std::collections::HashSet;

const SALT_BYTES: usize = 16;
// More values than a host has settings and model keys; the line stays short.
const MOST_SUMS: usize = 512;
const LONGEST_VALUE: usize = 4096;

pub struct Sums {
    salt: Vec<u8>,
    base: u64,
    // The longest first, so a value that holds another is hidden whole.
    lengths: Vec<Length>,
}

struct Length {
    bytes: usize,
    filter: Box<[u64; 1024]>,
    sums: HashSet<[u8; 32]>,
}

impl Length {
    fn may_hold(&self, fp: u16) -> bool {
        self.filter[usize::from(fp >> 6)] & (1 << (fp & 63)) != 0
    }
}

// The base of the rolling hash comes from the salt, so the filter of a value
// differs from one start of the server to the next. Odd, so no byte is lost.
fn base_of(salt: &[u8]) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&salt[..8]);
    u64::from_le_bytes(b) | 1
}

fn roll(base: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |h, b| {
        h.wrapping_mul(base).wrapping_add(u64::from(*b))
    })
}

fn filter(h: u64) -> u16 {
    (h >> 48) as u16
}

fn sum(salt: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut s = Sha256::new();
    s.update(salt);
    s.update(bytes);
    s.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// The line of the values for a program that must not hold them. A value over
/// the longest is left out, as is any past the most.
pub fn line(values: &[String], salt: &[u8]) -> String {
    let base = base_of(salt);
    let mut words = vec![hex(salt)];
    for v in values
        .iter()
        .filter(|v| !v.is_empty() && v.len() <= LONGEST_VALUE)
        .take(MOST_SUMS)
    {
        let b = v.as_bytes();
        words.push(format!(
            "{}:{:04x}:{}",
            b.len(),
            filter(roll(base, b)),
            hex(&sum(salt, b))
        ));
    }
    words.join(" ")
}

/// The line read back. It names what is wrong and never echoes a word.
pub fn parse(line: &str) -> Result<Sums, String> {
    let mut words = line.split(' ');
    let salt = words
        .next()
        .and_then(unhex)
        .filter(|s| s.len() == SALT_BYTES)
        .ok_or("the salt is not 32 hex digits")?;
    let base = base_of(&salt);
    let mut lengths: Vec<Length> = Vec::new();
    for (i, word) in words.enumerate() {
        if i == MOST_SUMS {
            return Err(format!("more than {MOST_SUMS} values"));
        }
        let bad = || format!("value {} is not <bytes>:<filter>:<sum>", i + 1);
        let mut parts = word.split(':');
        let (Some(n), Some(fp), Some(s), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        let bytes = n
            .parse::<usize>()
            .ok()
            .filter(|n| (1..=LONGEST_VALUE).contains(n))
            .ok_or_else(bad)?;
        let fp = Some(fp)
            .filter(|f| f.len() == 4)
            .and_then(|f| u16::from_str_radix(f, 16).ok())
            .ok_or_else(bad)?;
        let s: [u8; 32] = unhex(s).and_then(|s| s.try_into().ok()).ok_or_else(bad)?;
        let at = match lengths.iter().position(|l| l.bytes == bytes) {
            Some(at) => at,
            None => {
                lengths.push(Length {
                    bytes,
                    filter: Box::new([0; 1024]),
                    sums: HashSet::new(),
                });
                lengths.len() - 1
            }
        };
        let l = &mut lengths[at];
        l.filter[usize::from(fp >> 6)] |= 1 << (fp & 63);
        l.sums.insert(s);
    }
    lengths.sort_by_key(|l| std::cmp::Reverse(l.bytes));
    Ok(Sums {
        salt,
        base,
        lengths,
    })
}

impl Sums {
    /// The text with every value whose sum it was given as `***`.
    pub fn hide(&self, text: &str) -> String {
        let mut text = text.to_string();
        for l in &self.lengths {
            text = self.hide_length(l, &text);
        }
        text
    }

    fn hide_length(&self, l: &Length, text: &str) -> String {
        let b = text.as_bytes();
        let n = l.bytes;
        if b.len() < n {
            return text.to_string();
        }
        let top = self.base.wrapping_pow((n - 1) as u32);
        let mut h = roll(self.base, &b[..n]);
        let mut out = String::with_capacity(text.len());
        let mut kept = 0;
        let mut i = 0;
        loop {
            // The bytes of a value are whole characters, so a window that
            // starts or ends inside one is no value; the check keeps a slice
            // of the text from panicking all the same.
            if i >= kept
                && l.may_hold(filter(h))
                && text.is_char_boundary(i)
                && text.is_char_boundary(i + n)
                && l.sums.contains(&sum(&self.salt, &b[i..i + n]))
            {
                out.push_str(&text[kept..i]);
                out.push_str("***");
                kept = i + n;
            }
            if i + n >= b.len() {
                break;
            }
            h = h
                .wrapping_sub(u64::from(b[i]).wrapping_mul(top))
                .wrapping_mul(self.base)
                .wrapping_add(u64::from(b[i + n]));
            i += 1;
        }
        out.push_str(&text[kept..]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{line, parse};

    const SALT: [u8; 16] = *b"0123456789abcdef";

    fn values(given: &[&str]) -> Vec<String> {
        given.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn a_value_is_hidden_wherever_it_stands_and_its_line_does_not_hold_it() {
        let value = "Zq9xLm2Pw8RtYb4K";
        let given = line(&values(&[value]), &SALT);
        assert!(!given.contains(value));
        let sums = parse(&given).unwrap();
        assert_eq!(sums.hide(value), "***");
        assert_eq!(
            sums.hide(&format!("key {value}\tx{value}y {value}{value}")),
            "key ***\tx***y ******"
        );
        assert_eq!(sums.hide("Zq9xLm2Pw8RtYb4"), "Zq9xLm2Pw8RtYb4");
        assert_eq!(sums.hide(""), "");
        assert_eq!(sums.hide("é ü Zq9xLm2Pw8RtYb4K é"), "é ü *** é");
    }

    #[test]
    fn the_longer_of_two_values_is_hidden_whole() {
        let sums = parse(&line(&values(&["abcdefgh", "abcdefghijkl"]), &SALT)).unwrap();
        assert_eq!(sums.hide("<abcdefghijkl> <abcdefgh>"), "<***> <***>");
    }

    #[test]
    fn a_line_of_another_salt_hides_nothing() {
        let mut other = SALT;
        other[0] ^= 1;
        let given = line(&values(&["Zq9xLm2Pw8RtYb4K"]), &other);
        let words: Vec<&str> = given.split(' ').collect();
        let forged = format!("{} {}", super::hex(&SALT), words[1]);
        let sums = parse(&forged).unwrap();
        assert_eq!(sums.hide("Zq9xLm2Pw8RtYb4K"), "Zq9xLm2Pw8RtYb4K");
    }

    #[test]
    fn no_values_is_a_salt_alone_and_hides_nothing() {
        let given = line(&[], &SALT);
        assert_eq!(given.len(), 32);
        assert_eq!(parse(&given).unwrap().hide("text"), "text");
    }

    #[test]
    fn a_bad_line_is_refused_and_named_without_its_words() {
        let good = line(&values(&["Zq9xLm2Pw8RtYb4K"]), &SALT);
        let (salt, word) = good.split_once(' ').unwrap();
        assert!(parse("").is_err());
        assert!(parse("abc").is_err());
        assert!(parse(&salt[..30]).is_err());
        for bad in [
            "x".to_string(),
            "16:zzzz:00".to_string(),
            format!("0{}", &word[2..]),
            format!("{word}:1"),
            format!("99999:{}", word.split_once(':').unwrap().1),
        ] {
            let said = parse(&format!("{salt} {bad}")).map(|_| ()).unwrap_err();
            assert_eq!(said, "value 1 is not <bytes>:<filter>:<sum>");
        }
        let many = vec![word; 513].join(" ");
        assert!(parse(&format!("{salt} {many}")).is_err());
    }
}
