//! The pins of the granting `tehas:` labels: `$TEHAS_HOME/state/labels`, one
//! `<name>\t<GitHub node id>` line each. The dispatcher judges each label
//! application against them (tehas_dispatch::pins), and `tehas labels` pins
//! the labels it made (`pin_made`).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::file::ReadError;

/// The labels whose application the dispatcher takes as a person's word to
/// start or plan work, the ones `tehas labels` pins. A lone `tehas:blocked` starts an issue once its
/// blockers close, so it is one of them.
pub const GRANTING: [&str; 7] = [
    "tehas:ready",
    "tehas:plan",
    "tehas:refine",
    "tehas:finish",
    "tehas:blocked",
    "tehas:lean",
    "tehas:classic",
];

/// The prefix of the labels of plugin phases. No core label takes it, and
/// none of them grants: core neither pins nor acts on one.
pub const PLUGIN_PHASE: &str = "tehas:p-";

/// Where the pins are kept, under TEHAS_HOME.
pub const FILE: &str = crate::home::State::LabelPins.relative();

/// Label name to node id.
pub type Pins = BTreeMap<String, String>;

/// What [`load`] found where the pins are kept.
#[derive(Debug, PartialEq)]
pub enum Record {
    /// No file: a fresh install, or a host from before the pins.
    Absent,
    /// The pins on record.
    Kept(Pins),
}

impl Record {
    /// The pins, none when there is no file.
    pub fn pins(self) -> Pins {
        match self {
            Record::Absent => Pins::new(),
            Record::Kept(p) => p,
        }
    }
}

/// The pins file is there and cannot be trusted: why, in a few words. No
/// granting label counts while it is, and nothing writes over it.
#[derive(Debug, PartialEq)]
pub struct Unreadable(pub String);

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the label pins in {FILE} cannot be read ({}): no granting label counts until they can. Make {FILE} a regular file, readable by tehas, of <name><tab><id> lines; or, after checking the tehas labels on GitHub, remove it and run tehas labels to pin them again",
            self.0
        )
    }
}

/// The bound of the file: seven pins take well under 1 KiB.
const BOUND: u64 = 64 * 1024;

/// The pins on record. Only a file that is not there is [`Record::Absent`].
/// Anything else that is not a regular file of `<name>\t<id>` lines, each
/// name once and no whitespace in either, is [`Unreadable`]: a link, a
/// directory, a file tehas may not read, one over the bound, not UTF-8, or a
/// line of another shape.
pub fn load(home: &Path) -> Result<Record, Unreadable> {
    let path = home.join(FILE);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Record::Absent),
        Err(e) => return Err(Unreadable(e.to_string())),
        Ok(_) => {}
    }
    let (bytes, _) = crate::file::read_regular(&path, BOUND).map_err(|e| {
        Unreadable(match e {
            ReadError::Io(e) => e.to_string(),
            ReadError::NotRegular => "it is not a regular file".into(),
            ReadError::Replaced => "it was replaced while it was read".into(),
            ReadError::TooLarge => "it is over 64 KiB".into(),
        })
    })?;
    let text = String::from_utf8(bytes).map_err(|_| Unreadable("it is not UTF-8".into()))?;
    parse(&text).map(Record::Kept)
}

fn parse(text: &str) -> Result<Pins, Unreadable> {
    let mut pins = Pins::new();
    for (at, l) in text.lines().enumerate() {
        let bare = |w: &str| !w.is_empty() && !w.contains(char::is_whitespace);
        let Some((name, id)) = l.split_once('\t').filter(|(n, id)| bare(n) && bare(id)) else {
            return Err(Unreadable(format!(
                "line {} is not <name><tab><id>",
                at + 1
            )));
        };
        if pins.insert(name.to_string(), id.to_string()).is_some() {
            return Err(Unreadable(format!("line {} pins {name} again", at + 1)));
        }
    }
    Ok(pins)
}

pub fn save(home: &Path, pins: &Pins) -> io::Result<()> {
    let file = home.join(FILE);
    let new = file.with_extension("new");
    let _ = fs::remove_file(&new);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&new)?;
    for (name, id) in pins {
        writeln!(f, "{name}\t{id}")?;
    }
    f.sync_all()?;
    fs::rename(&new, &file)
}

/// Whether `name` is a granting label.
pub fn granting(name: &str) -> bool {
    GRANTING.contains(&name)
}

/// A label of [`GRANTING`] as `tehas labels` saw it: its id before it made the
/// labels and after, None where GitHub had no such label, and when GitHub
/// says the label of `after` was created, in Unix seconds.
pub struct Seen<'a> {
    pub name: &'a str,
    pub before: Option<String>,
    pub after: Option<String>,
    pub created: Option<i64>,
}

/// How close in time two labels' creation must be for one `tehas labels`
/// run to have made both.
pub const SAME_RUN_SECS: i64 = 60;

/// `tehas labels`' step, after it made the labels. With no file yet (a fresh
/// install, or a host from before the pins) every label of [`GRANTING`] found
/// is pinned, as the poll would pin it. Once the file exists, only a label this
/// run made, one there was none of before, is pinned: a label that was there
/// with no pin may be another label renamed to the name, and its name cannot
/// tell, so it is said and left refused for a person to pin by hand, unless
/// GitHub says it was created within [`SAME_RUN_SECS`] of a label that is
/// pinned and still that label: the run that made the pinned one made it too.
/// A label renamed keeps its creation time, so another label renamed to the
/// name is still refused. That is how a label added to [`GRANTING`] after a host
/// pinned the others gets its pin. A pin is never changed here. The file is written even with no pin in it, so the poll
/// pins nothing after. The lines to print, and whether every label of
/// [`GRANTING`] now has its pin. A file that is there and cannot be read is
/// [`Unreadable`], and nothing is written over it.
pub fn pin_made(home: &Path, seen: &[Seen]) -> Result<(Vec<String>, bool), Unreadable> {
    let record = load(home)?;
    let kept = record != Record::Absent;
    let mut pins = record.pins();
    let anchors: Vec<i64> = seen
        .iter()
        .filter(|s| s.after.is_some() && pins.get(s.name) == s.after.as_ref())
        .filter_map(|s| s.created)
        .collect();
    let same_run = |s: &Seen| {
        s.before == s.after
            && s.created
                .is_some_and(|c| anchors.iter().any(|a| (c - a).abs() <= SAME_RUN_SECS))
    };
    let mut said = Vec::new();
    let mut whole = true;
    for s in seen {
        let name = s.name;
        if !GRANTING.contains(&name) {
            continue;
        }
        match (pins.get(name), &s.after) {
            (Some(p), Some(id)) if p != id => {
                whole = false;
                said.push(format!(
                    "not pinned: the label {name} is {id}, and {FILE} pins {p}: no application of it counts until the pin and the label agree"
                ));
            }
            (Some(_), _) => {}
            (None, Some(id)) if !kept || s.before.is_none() => {
                pins.insert(name.to_string(), id.clone());
                said.push(format!("pinned the label {name} as {id} in {FILE}"));
            }
            (None, Some(id)) if same_run(s) => {
                pins.insert(name.to_string(), id.clone());
                said.push(format!(
                    "pinned the label {name} as {id} in {FILE}: it was made with the pinned labels"
                ));
            }
            (None, Some(id)) => {
                whole = false;
                said.push(format!(
                    "not pinned: the label {name} ({id}) was there before with no pin, and its name cannot tell whether it is the label it should be: if it is, add the line \"{name}<tab>{id}\" to {FILE}; if not, rename or delete it and run tehas labels again"
                ));
            }
            (None, None) => {
                whole = false;
                said.push(format!("not pinned: the label {name} was not found"));
            }
        }
    }
    if let Some(dir) = home.join(FILE).parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Err(e) = save(home, &pins) {
        said.push(format!("the label pins could not be kept in {FILE}: {e}"));
        whole = false;
    }
    Ok((said, whole))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::Home;
    use crate::testdir::TestDir;

    fn pins(home: &Path) -> Pins {
        load(home).unwrap().pins()
    }

    fn seen<'a>(name: &'a str, before: Option<&str>, after: Option<&str>) -> Seen<'a> {
        made(name, before, after, None)
    }

    fn made<'a>(
        name: &'a str,
        before: Option<&str>,
        after: Option<&str>,
        created: Option<i64>,
    ) -> Seen<'a> {
        Seen {
            name,
            before: before.map(str::to_string),
            after: after.map(str::to_string),
            created,
        }
    }

    #[test]
    fn with_no_file_every_label_found_is_pinned() {
        let d = TestDir::new("pins-made-first");
        let (said, whole) = pin_made(
            d.path(),
            &[
                seen("tehas:ready", Some("LA_r"), Some("LA_r")),
                seen("tehas:plan", None, None),
                seen("tehas:running", None, Some("LA_x")),
                seen("tehas:lean", None, Some("LA_l")),
            ],
        )
        .unwrap();
        assert!(!whole);
        assert_eq!(
            said[0],
            "pinned the label tehas:ready as LA_r in state/labels"
        );
        assert_eq!(said[1], "not pinned: the label tehas:plan was not found");
        assert_eq!(
            said[2],
            "pinned the label tehas:lean as LA_l in state/labels"
        );
        assert_eq!(said.len(), 3);
        assert_eq!(
            pins(d.path()),
            Pins::from([
                ("tehas:ready".into(), "LA_r".into()),
                ("tehas:lean".into(), "LA_l".into())
            ])
        );
    }

    // tehas:plan has no pin and a label of that name is there: it may be a
    // person's `bug` a container renamed, so it is not pinned.
    #[test]
    fn once_the_file_exists_only_a_label_this_run_made_is_pinned() {
        let d = TestDir::new("pins-made-kept");
        fs::create_dir_all(Home::new(d.path()).state_dir()).unwrap();
        fs::write(d.path().join(FILE), "tehas:ready\tLA_r\n").unwrap();
        let (said, whole) = pin_made(
            d.path(),
            &[
                seen("tehas:ready", Some("LA_other"), Some("LA_other")),
                seen("tehas:plan", Some("LA_bug"), Some("LA_bug")),
                seen("tehas:lean", None, Some("LA_lean")),
            ],
        )
        .unwrap();
        assert!(!whole);
        assert!(said[0].starts_with(
            "not pinned: the label tehas:ready is LA_other, and state/labels pins LA_r"
        ));
        assert!(said[1]
            .starts_with("not pinned: the label tehas:plan (LA_bug) was there before with no pin"));
        assert_eq!(
            said[2],
            "pinned the label tehas:lean as LA_lean in state/labels"
        );
        let pins = pins(d.path());
        assert_eq!(pins["tehas:ready"], "LA_r");
        assert!(!pins.contains_key("tehas:plan"));
        let (said, whole) = pin_made(
            d.path(),
            &[
                seen("tehas:ready", Some("LA_r"), Some("LA_r")),
                seen("tehas:lean", Some("LA_lean"), Some("LA_lean")),
            ],
        )
        .unwrap();
        assert!(whole && said.is_empty());
    }

    // A host pinned the labels before tehas:blocked joined GRANTING. The run
    // that made tehas:ready made tehas:blocked 8 s later, so it is adopted.
    #[test]
    fn a_label_made_with_a_pinned_one_is_pinned() {
        let d = TestDir::new("pins-same-run");
        fs::create_dir_all(Home::new(d.path()).state_dir()).unwrap();
        fs::write(d.path().join(FILE), "tehas:ready\tLA_r\n").unwrap();
        let (said, whole) = pin_made(
            d.path(),
            &[
                made("tehas:ready", Some("LA_r"), Some("LA_r"), Some(1_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_b"), Some(1_008)),
            ],
        )
        .unwrap();
        assert!(whole);
        assert_eq!(
            said,
            ["pinned the label tehas:blocked as LA_b in state/labels: it was made with the pinned labels"]
        );
        assert_eq!(pins(d.path())["tehas:blocked"], "LA_b");
    }

    // Each is refused: made long before or after the pinned label, of no
    // known time, beside a pinned label that is not the label of its pin,
    // or a different label at the end of the run than at its start.
    #[test]
    fn a_label_not_made_with_a_pinned_one_is_still_refused() {
        let cases: [(&str, Seen, Seen); 5] = [
            (
                "made a day before",
                made("tehas:ready", Some("LA_r"), Some("LA_r"), Some(90_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_b"), Some(3_600)),
            ),
            (
                "made just past the bound",
                made("tehas:ready", Some("LA_r"), Some("LA_r"), Some(1_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_b"), Some(1_061)),
            ),
            (
                "no time",
                made("tehas:ready", Some("LA_r"), Some("LA_r"), Some(1_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_b"), None),
            ),
            (
                "the pinned name is another label now",
                made("tehas:ready", Some("LA_x"), Some("LA_x"), Some(1_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_b"), Some(1_000)),
            ),
            (
                "another label by the end of the run",
                made("tehas:ready", Some("LA_r"), Some("LA_r"), Some(1_000)),
                made("tehas:blocked", Some("LA_b"), Some("LA_c"), Some(1_000)),
            ),
        ];
        for (why, ready, blocked) in cases {
            let d = TestDir::new("pins-not-same-run");
            fs::create_dir_all(Home::new(d.path()).state_dir()).unwrap();
            fs::write(d.path().join(FILE), "tehas:ready\tLA_r\n").unwrap();
            let (said, whole) = pin_made(d.path(), &[ready, blocked]).unwrap();
            assert!(!whole, "{why}");
            assert!(
                said.iter()
                    .all(|l| !l.starts_with("pinned the label tehas:blocked")),
                "{why}: {said:?}"
            );
            assert!(!pins(d.path()).contains_key("tehas:blocked"), "{why}");
        }
    }

    /// Each way the file can be there and not be read: its name, and the
    /// state dir of `d` made so. Permission denied is left out when the
    /// tests can read a file of mode 000 (root).
    const READY: &str = GRANTING[0];

    type Case = (&'static str, Box<dyn Fn(&Path)>);

    fn unreadable_cases() -> Vec<Case> {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let mut cases: Vec<Case> = vec![
            (
                "a dangling link",
                Box::new(|h: &Path| symlink(h.join("gone"), h.join(FILE)).unwrap()),
            ),
            (
                "a live link",
                Box::new(|h: &Path| {
                    fs::write(h.join("elsewhere"), format!("{READY}\tLA_r\n")).unwrap();
                    symlink(h.join("elsewhere"), h.join(FILE)).unwrap();
                }),
            ),
            (
                "a directory",
                Box::new(|h: &Path| fs::create_dir(h.join(FILE)).unwrap()),
            ),
            (
                "a line of another shape",
                Box::new(|h: &Path| {
                    fs::write(h.join(FILE), format!("{READY}\tLA_r\n{READY}x LA_p\n")).unwrap()
                }),
            ),
            (
                "a name pinned twice",
                Box::new(|h: &Path| {
                    fs::write(h.join(FILE), format!("{READY}\tLA_r\n{READY}\tLA_x\n")).unwrap()
                }),
            ),
            (
                "not UTF-8",
                Box::new(|h: &Path| {
                    fs::write(h.join(FILE), [READY.as_bytes(), b"\tLA_\xff\n"].concat()).unwrap()
                }),
            ),
            (
                "over the bound",
                Box::new(|h: &Path| {
                    fs::write(h.join(FILE), format!("{READY}\tLA_r\n").repeat(5000)).unwrap()
                }),
            ),
        ];
        let probe = TestDir::new("pins-probe");
        let f = probe.path().join("f");
        fs::write(&f, "x").unwrap();
        fs::set_permissions(&f, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&f).is_err() {
            cases.push((
                "permission denied",
                Box::new(|h: &Path| {
                    fs::write(h.join(FILE), format!("{READY}\tLA_r\n")).unwrap();
                    fs::set_permissions(h.join(FILE), fs::Permissions::from_mode(0o000)).unwrap();
                }),
            ));
        }
        cases
    }

    /// What is at FILE, as bytes, without following a link: the link's
    /// target path, a directory's mark, or the file's bytes.
    fn as_is(home: &Path) -> Vec<u8> {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::PermissionsExt;
        let path = home.join(FILE);
        let m = fs::symlink_metadata(&path).unwrap();
        if m.file_type().is_symlink() {
            return fs::read_link(&path)
                .unwrap()
                .as_os_str()
                .as_bytes()
                .to_vec();
        }
        if m.is_dir() {
            return b"<dir>".to_vec();
        }
        let mode = m.permissions().mode();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let bytes = fs::read(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        bytes
    }

    #[test]
    fn only_a_missing_file_is_absent() {
        let d = TestDir::new("pins-absent");
        assert_eq!(load(d.path()), Ok(Record::Absent));
        fs::create_dir_all(Home::new(d.path()).state_dir()).unwrap();
        assert_eq!(load(d.path()), Ok(Record::Absent));
        fs::write(d.path().join(FILE), "").unwrap();
        assert_eq!(load(d.path()), Ok(Record::Kept(Pins::new())));
        fs::write(
            d.path().join(FILE),
            format!("{READY}\tLA_r\n{PLUGIN_PHASE}x\tLA_x\n"),
        )
        .unwrap();
        assert_eq!(pins(d.path())[READY], "LA_r");
    }

    #[test]
    fn a_file_that_is_there_and_cannot_be_read_is_unreadable() {
        for (why, make) in unreadable_cases() {
            let d = TestDir::new("pins-unreadable");
            fs::create_dir_all(Home::new(d.path()).state_dir()).unwrap();
            make(d.path());
            let Err(u) = load(d.path()) else {
                panic!("{why}: read");
            };
            let line = u.to_string();
            assert!(
                line.starts_with(&format!("the label pins in {FILE} cannot be read (")),
                "{why}: {line}"
            );
            assert!(line.contains("run tehas labels"), "{why}: {line}");
        }
    }

    #[test]
    fn tehas_labels_writes_nothing_over_a_file_it_cannot_read() {
        for (why, make) in unreadable_cases() {
            let d = TestDir::new("pins-made-unreadable");
            fs::create_dir_all(Home::new(d.path()).state_dir()).unwrap();
            make(d.path());
            let was = as_is(d.path());
            let got = pin_made(d.path(), &[seen(READY, None, Some("LA_new"))]);
            assert!(got.is_err(), "{why}: {got:?}");
            assert_eq!(as_is(d.path()), was, "{why}");
            assert!(!d.path().join(FILE).with_extension("new").exists(), "{why}");
        }
    }
}
