//! How a whole log is packed for upload. Pieces are split at line boundaries
//! and compressed one by one, so each opens on its own. The number of pieces
//! doubles until every piece fits. gzip and split do the work, so the bytes are
//! what they always were.

use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Stdio};

fn gzip(from: &Path, to: &Path) -> Option<u64> {
    let out = File::create(to).ok()?;
    let status = Command::new("gzip")
        .args(["-9", "-c"])
        .arg(from)
        .stdout(out)
        .status()
        .ok()?;
    status
        .success()
        .then(|| fs::metadata(to).ok())?
        .map(|m| m.len())
}

fn remove_pieces(dir: &Path, name: &str) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let start = format!("{name}.");
    for e in entries.flatten() {
        let file = e.file_name().to_string_lossy().into_owned();
        if file.starts_with(&start) && file.ends_with(".gz") {
            let _ = fs::remove_file(e.path());
        }
    }
}

/// One round: the file in `n` pieces. The names when each is at most `limit`.
fn pieces(file: &Path, dir: &Path, name: &str, limit: u64, n: u64) -> Option<Option<Vec<String>>> {
    let parts = dir.join(".parts");
    let _ = fs::remove_dir_all(&parts);
    remove_pieces(dir, name);
    fs::create_dir_all(&parts).ok()?;
    let split = Command::new("split")
        .args(["-n", &format!("l/{n}"), "-a", "5", "-d"])
        .arg(file)
        .arg(parts.join("p."))
        .stdin(Stdio::null())
        .status()
        .ok()?;
    if !split.success() {
        return None;
    }
    let mut made: Vec<_> = fs::read_dir(&parts)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    made.sort();
    let mut names = Vec::new();
    let mut big = false;
    for part in &made {
        if fs::metadata(part).map_or(true, |m| m.len() == 0) {
            continue;
        }
        let piece = format!("{name}.{}.gz", names.len() + 1);
        big |= gzip(part, &dir.join(&piece)).is_none_or(|size| size > limit);
        names.push(piece);
    }
    let _ = fs::remove_dir_all(&parts);
    Some((!big).then_some(names))
}

/// Gzip `file` into `dir` as `<name>.gz`, or in numbered pieces when that would
/// be over `limit` bytes. The names written, the pieces in order. None when it
/// cannot be done, and then no piece is left.
pub fn pack(file: &Path, dir: &Path, name: &str, limit: u64) -> Option<Vec<String>> {
    fs::create_dir_all(dir).ok()?;
    let whole = format!("{name}.gz");
    let size = gzip(file, &dir.join(&whole))?;
    if size <= limit {
        return Some(vec![whole]);
    }
    let _ = fs::remove_file(dir.join(&whole));
    let lines = fs::read(file).ok()?.iter().filter(|b| **b == b'\n').count() as u64;
    if lines >= 2 {
        let mut n = size / limit + 1;
        loop {
            n = n.min(lines);
            match pieces(file, dir, name, limit, n) {
                Some(Some(names)) => return Some(names),
                Some(None) if n < lines => n *= 2,
                _ => break,
            }
        }
    }
    let _ = fs::remove_dir_all(dir.join(".parts"));
    remove_pieces(dir, name);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("improve-pack-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Dir(d)
    }

    // 300 lines that compress badly, so a small limit forces pieces.
    fn big_log() -> Vec<u8> {
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut out = Vec::new();
        for _ in 0..300 {
            for _ in 0..8 {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                out.extend_from_slice(format!("{x:016x}").as_bytes());
            }
            out.push(b'\n');
        }
        out
    }

    fn unzip(file: &Path) -> Vec<u8> {
        let out = Command::new("gzip").arg("-dc").arg(file).output().unwrap();
        assert!(out.status.success());
        out.stdout
    }

    #[test]
    fn a_log_under_the_limit_is_one_file() {
        let d = dir("one");
        let log = d.0.join("log");
        fs::write(&log, big_log()).unwrap();
        let gz = d.0.join("gz");
        let names = pack(&log, &gz, "ticket-7.log", 1_000_000).unwrap();
        assert_eq!(names, ["ticket-7.log.gz"]);
        assert_eq!(unzip(&gz.join("ticket-7.log.gz")), big_log());
    }

    #[test]
    fn a_log_over_the_limit_is_numbered_pieces_that_join_back_to_it() {
        let d = dir("pieces");
        let log = d.0.join("log");
        fs::write(&log, big_log()).unwrap();
        let gz = d.0.join("gz");
        let names = pack(&log, &gz, "ticket-7.log", 3000).unwrap();
        assert!(names.len() >= 2, "{names:?}");
        let mut joined = Vec::new();
        for (i, name) in names.iter().enumerate() {
            assert_eq!(*name, format!("ticket-7.log.{}.gz", i + 1));
            let file = gz.join(name);
            assert!(fs::metadata(&file).unwrap().len() <= 3000);
            joined.extend(unzip(&file));
        }
        assert_eq!(joined, big_log());
        assert_eq!(fs::read_dir(&gz).unwrap().count(), names.len());
    }

    #[test]
    fn a_line_too_long_for_one_piece_leaves_no_pieces() {
        let d = dir("long");
        let log = d.0.join("log");
        let mut text: Vec<u8> = big_log().into_iter().filter(|b| *b != b'\n').collect();
        text.extend_from_slice(b"\nsecond line\n");
        fs::write(&log, text).unwrap();
        let gz = d.0.join("gz");
        assert_eq!(pack(&log, &gz, "ticket-7.log", 3000), None);
        assert_eq!(fs::read_dir(&gz).unwrap().count(), 0);
    }

    #[test]
    fn a_file_that_cannot_be_read_packs_nothing() {
        let d = dir("gone");
        assert_eq!(pack(&d.0.join("none"), &d.0.join("gz"), "x", 3000), None);
    }
}
