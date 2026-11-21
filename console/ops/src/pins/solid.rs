//! The kit's Solid: the pins of tools/vendor-solid/src/main.rs, each moved to
//! the newest release of its major on npm with its tarball and integrity,
//! when the Solid packages that release asks for are met by what the others
//! move to.

use super::text::{choose, Ver};
use super::tools::{npm_doc, npm_versions, World};
use serde_json::Value;

pub const MAIN_RS: &str = "tools/vendor-solid/src/main.rs";

/// One entry of `PINS`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub name: String,
    pub version: String,
    pub tarball: String,
    pub integrity: String,
}

/// The lines of an entry after its `Pin {`, in the order main.rs writes them.
const FIELDS: [&str; 4] = ["name", "version", "tarball", "integrity"];

/// The lines inside `const PINS: ... = [` and its `];`.
fn section(lines: &[&str]) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| l.starts_with("const PINS: "))?;
    let end = (start + 1..lines.len()).find(|&i| lines[i] == "];")?;
    Some((start + 1, end))
}

/// The string of a line `<name>: "<value>",`.
fn field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    line.trim()
        .strip_prefix(name)?
        .strip_prefix(": \"")?
        .strip_suffix("\",")
}

/// The entries of `PINS` in main.rs. Each is a `Pin {` line, its four fields
/// one a line, and `},`; anything else in the array is refused.
pub fn pins(text: &str) -> Result<Vec<Pin>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let (start, end) = section(&lines).ok_or_else(|| format!("{MAIN_RS} names no PINS"))?;
    let mut out = Vec::new();
    let mut i = start;
    while i < end {
        let get = |k: usize| {
            lines
                .get(i + 1 + k)
                .filter(|_| i + 1 + k < end)
                .and_then(|l| field(l, FIELDS[k]))
        };
        let closed = lines.get(i + 5).map(|l| l.trim()) == Some("},");
        match (lines[i].trim(), get(0), get(1), get(2), get(3)) {
            ("Pin {", Some(n), Some(v), Some(t), Some(g)) if closed => {
                out.push(Pin {
                    name: n.into(),
                    version: v.into(),
                    tarball: t.into(),
                    integrity: g.into(),
                });
                i += 6;
            }
            _ => {
                let line = i + 1;
                return Err(format!(
                    "{MAIN_RS}: line {line}, in PINS, is not a whole Pin"
                ));
            }
        }
    }
    if out.is_empty() {
        return Err(format!("{MAIN_RS} names no PINS"));
    }
    Ok(out)
}

/// main.rs with the entry of `pin.name` given `pin`'s version, tarball and
/// integrity. Nothing else of the text changes.
pub fn set(text: &str, pin: &Pin) -> Result<String, String> {
    pins(text)?;
    let values = [&pin.version, &pin.tarball, &pin.integrity];
    if values.iter().any(|v| {
        v.is_empty()
            || !v
                .bytes()
                .all(|b| b.is_ascii_graphic() && b != b'"' && b != b'\\')
    }) {
        return Err(format!(
            "{} {}: a value is not a plain string",
            pin.name, pin.version
        ));
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let bare: Vec<&str> = lines.iter().map(|l| l.trim_end_matches('\n')).collect();
    let (start, end) = section(&bare).ok_or_else(|| format!("{MAIN_RS} names no PINS"))?;
    let at = (start..end)
        .find(|&i| bare[i].trim() == "Pin {" && field(bare[i + 1], "name") == Some(&pin.name))
        .ok_or_else(|| format!("{MAIN_RS} has no pin of {}", pin.name))?;
    let mut out = String::with_capacity(text.len());
    for (i, line) in lines.iter().enumerate() {
        match i.checked_sub(at + 1) {
            Some(k @ 1..=3) => {
                let indent = &bare[i][..bare[i].len() - bare[i].trim_start().len()];
                out.push_str(&format!("{indent}{}: \"{}\",", FIELDS[k], values[k - 1]));
                if line.ends_with('\n') {
                    out.push('\n');
                }
            }
            _ => out.push_str(line),
        }
    }
    Ok(out)
}

/// A version of an npm range, each part a number or a wildcard.
type Partial = [Option<u64>; 3];

fn partial(s: &str) -> Option<Partial> {
    let mut out = [None; 3];
    let mut wild = false;
    let mut n = 0;
    for (k, part) in s.split('.').enumerate() {
        if k == 3 {
            return None;
        }
        n = k + 1;
        if matches!(part, "x" | "X" | "*") {
            wild = true;
        } else if wild || part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        } else {
            out[k] = Some(part.parse().ok()?);
        }
    }
    (n > 0).then_some(out)
}

/// Whether `v` meets one comparator of a range: an operator and a version.
fn comparator(c: &str, v: Ver) -> bool {
    let (op, rest) = ["<=", ">=", "<", ">", "=", "^", "~"]
        .iter()
        .find_map(|op| c.strip_prefix(op).map(|r| (*op, r)))
        .unwrap_or(("", c));
    // Against a release, a pre-release in a bound holds as its X.Y.Z, but
    // an exact pre-release is never met.
    let (core, pre) = match rest.split_once('-') {
        Some((core, _)) => (core, true),
        None => (rest, false),
    };
    let Some(p) = partial(core) else {
        return false;
    };
    let full = p.iter().all(Option::is_some);
    let base = Ver(p[0].unwrap_or(0), p[1].unwrap_or(0), p[2].unwrap_or(0));
    let parts = [v.0, v.1, v.2];
    match op {
        "" | "=" => {
            !(pre && full)
                && p.iter()
                    .zip(parts)
                    .all(|(want, got)| want.is_none_or(|w| w == got))
        }
        ">=" => v >= base,
        "<" => v < base,
        ">" if full => {
            if pre {
                v >= base
            } else {
                v > base
            }
        }
        "<=" if full => {
            if pre {
                v < base
            } else {
                v <= base
            }
        }
        "^" => match p {
            [Some(a), None, _] => v.0 == a,
            [Some(0), Some(0), None] => v.0 == 0 && v.1 == 0,
            [Some(0), Some(0), Some(_)] => v == base,
            [Some(0), Some(b), _] => v >= base && v.0 == 0 && v.1 == b,
            [Some(a), Some(_), _] => v >= base && v.0 == a,
            _ => false,
        },
        "~" => match p {
            [Some(a), None, _] => v.0 == a,
            [Some(a), Some(b), _] => v >= base && v.0 == a && v.1 == b,
            _ => false,
        },
        _ => false,
    }
}

/// Whether the release `v` meets an npm range. A form this does not read,
/// a hyphen range, a tag or a protocol, is not met.
pub fn satisfies(range: &str, v: Ver) -> bool {
    range
        .split("||")
        .any(|alt| alt.split_whitespace().all(|c| comparator(c, v)))
}

/// What a Solid pin moves to.
pub struct Choice {
    pub name: String,
    pub current: Ver,
    pub pin: Ver,
    pub later: Option<Ver>,
    /// Why a newer release of the major was not taken.
    pub kept: Option<String>,
    doc: Value,
}

impl Choice {
    /// The pin at the version chosen, its tarball and integrity from the
    /// registry.
    pub fn moved(&self) -> Result<Pin, String> {
        let version = self.pin.to_string();
        let dist = &self.doc["versions"][&version]["dist"];
        let at = format!("https://registry.npmjs.org/{}/-/", self.name);
        let plain = |s: &&str| {
            s.bytes()
                .all(|b| b.is_ascii_graphic() && b != b'"' && b != b'\\')
        };
        let tarball = dist["tarball"]
            .as_str()
            .filter(|t| t.starts_with(&at) && t.ends_with(".tgz") && plain(t))
            .ok_or_else(|| {
                format!(
                    "{} {version}: the registry names no tarball under {at}",
                    self.name
                )
            })?;
        let integrity = dist["integrity"]
            .as_str()
            .filter(|i| {
                i.strip_prefix("sha512-").is_some_and(|b| {
                    b.len() == 88
                        && b.bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"+/=".contains(&c))
                })
            })
            .ok_or_else(|| {
                format!(
                    "{} {version}: the registry names no sha512 integrity",
                    self.name
                )
            })?;
        Ok(Pin {
            name: self.name.clone(),
            version,
            tarball: tarball.into(),
            integrity: integrity.into(),
        })
    }
}

/// Each pin of main.rs and the newest release of its major that npm would
/// install, kept where that release asks, in its peer dependencies or its
/// dependencies, for a Solid package at a version the others do not move to.
pub fn plan(text: &str, world: &dyn World) -> Result<Vec<Choice>, String> {
    let mut out = Vec::new();
    for p in pins(text)? {
        let current = Ver::parse(&p.version)
            .ok_or_else(|| format!("{MAIN_RS} pins {} {}, not X.Y.Z", p.name, p.version))?;
        let doc = npm_doc(world, &p.name).map_err(|e| format!("{}: {e}", p.name))?;
        let found = npm_versions(&doc, world);
        if found.is_empty() {
            return Err(format!("{} lists no release", p.name));
        }
        let (pin, later) = choose(current, &found);
        out.push(Choice {
            name: p.name,
            current,
            pin,
            later,
            kept: None,
            doc,
        });
    }
    // Keeping one pin can leave another unmet: until nothing changes.
    loop {
        let mut changed = false;
        for i in 0..out.len() {
            let c = &out[i];
            if c.pin == c.current {
                continue;
            }
            let entry = &c.doc["versions"][c.pin.to_string()];
            let unmet = ["peerDependencies", "dependencies"]
                .iter()
                .flat_map(|k| entry[k].as_object().into_iter().flatten())
                .find_map(|(dep, range)| {
                    let other = out.iter().find(|o| o.name == *dep)?;
                    let range = range.as_str().unwrap_or("?");
                    (!satisfies(range, other.pin)).then(|| {
                        format!(
                            "{} asks for {dep} {range}, and {dep} would be {}",
                            c.pin, other.pin
                        )
                    })
                });
            if let Some(why) = unmet {
                out[i].kept = Some(why);
                out[i].pin = out[i].current;
                changed = true;
            }
        }
        if !changed {
            return Ok(out);
        }
    }
}
