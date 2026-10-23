//! The cycle lock probe: whether `state/cycles/<issue>.lock` is held, as
//! `flock -n` would find it. It never creates the file and never reads or
//! writes `cycle_locks`.

use crate::verbs::{j, obj};
use std::fs::{self, File, TryLockError};
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub struct Lock {
    /// 0 free, 1 held, 2 could not tell.
    pub rc: u8,
    pub file: bool,
    pub why: Option<String>,
}

impl Lock {
    fn free(file: bool) -> Lock {
        Lock {
            rc: 0,
            file,
            why: None,
        }
    }

    fn unknown(file: bool, why: String) -> Lock {
        Lock {
            rc: 2,
            file,
            why: Some(why),
        }
    }

    pub fn state(&self) -> &'static str {
        match self.rc {
            0 => "free",
            1 => "held",
            _ => "unknown",
        }
    }

    pub fn json(&self, now: i64) -> String {
        let mut pairs = vec![
            ("state", j(self.state())),
            ("probe_rc", j(self.rc)),
            ("file", j(self.file)),
            ("seen_at", j(now)),
        ];
        if let Some(why) = &self.why {
            pairs.push(("why", j(why.as_str())));
        }
        obj(&pairs)
    }
}

pub fn dir(home: &Path) -> PathBuf {
    home.join("state").join("cycles")
}

pub fn path(home: &Path, issue: i64) -> PathBuf {
    dir(home).join(format!("{issue}.lock"))
}

/// `open_row`: the issue has a cycle row with no `ended_at`. Without a lock
/// file such a row cannot be told apart from a cycle whose file was removed.
pub fn probe(home: &Path, issue: i64, open_row: bool) -> Lock {
    let absent = || {
        if open_row {
            Lock::unknown(
                false,
                "no lock file for a cycle on record as running".to_string(),
            )
        } else {
            Lock::free(false)
        }
    };
    match fs::symlink_metadata(dir(home)) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Lock::unknown(false, "state/cycles is not a directory".to_string()),
        Err(e) if e.kind() == ErrorKind::NotFound => return absent(),
        Err(e) => return Lock::unknown(false, format!("state/cycles: {e}")),
    }
    let file = path(home, issue);
    let before = match fs::symlink_metadata(&file) {
        Ok(m) if m.file_type().is_file() => m,
        Ok(_) => return Lock::unknown(true, format!("{issue}.lock is not a regular file")),
        Err(e) if e.kind() == ErrorKind::NotFound => return absent(),
        Err(e) => return Lock::unknown(true, format!("{issue}.lock: {e}")),
    };
    let f = match File::open(&file) {
        Ok(f) => f,
        Err(e) => return Lock::unknown(true, format!("{issue}.lock: {e}")),
    };
    match f.metadata() {
        Ok(m) if m.dev() == before.dev() && m.ino() == before.ino() => {}
        Ok(_) => return Lock::unknown(true, format!("{issue}.lock was replaced")),
        Err(e) => return Lock::unknown(true, format!("{issue}.lock: {e}")),
    }
    match f.try_lock() {
        Ok(()) => {
            let _ = f.unlock();
            Lock::free(true)
        }
        Err(TryLockError::WouldBlock) => Lock {
            rc: 1,
            file: true,
            why: None,
        },
        Err(TryLockError::Error(e)) => Lock::unknown(true, format!("{issue}.lock: {e}")),
    }
}

/// The issues named by lock files in `state/cycles`. A missing directory lists
/// none; one that cannot be listed, or an entry that cannot be read, is the
/// error.
pub fn listed(home: &Path) -> Result<Vec<i64>, String> {
    let d = dir(home);
    match fs::symlink_metadata(&d) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err("state/cycles is not a directory".to_string()),
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("state/cycles: {e}")),
    }
    let entries = fs::read_dir(&d).map_err(|e| format!("state/cycles: {e}"))?;
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("state/cycles: {e}"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(n) = name.strip_suffix(".lock").and_then(crate::verbs::number) {
            found.push(n);
        }
    }
    found.sort_unstable();
    Ok(found)
}
