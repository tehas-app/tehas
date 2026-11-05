//! Files: a host file read by the store's rule, a database's side files, and
//! a file's bytes as base64.

use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The 1 MiB bound of a read: a file, a stream or a line one program writes
/// and another reads. Both sides of a seam name this one constant, so a change
/// to it cannot leave a writer producing what its reader refuses. A bound that
/// is deliberately different keeps its own constant and says why.
pub const READ_BOUND: u64 = 1 << 20;
/// `READ_BOUND` for a bound counted in a `usize`.
pub const READ_BOUND_LEN: usize = READ_BOUND as usize;

#[derive(Debug)]
pub enum ReadError {
    /// It could not be looked at, opened or read.
    Io(io::Error),
    /// A link, a directory, a device or a fifo.
    NotRegular,
    /// Another file was in its place when it was opened.
    Replaced,
    /// Over the bound.
    TooLarge,
}

/// The bytes of a regular file of at most `bound` bytes, and the metadata of
/// the file that was read. It is not read through a link, and it is the same
/// file after it is opened.
pub fn read_regular(path: &Path, bound: u64) -> Result<(Vec<u8>, Metadata), ReadError> {
    let before = std::fs::symlink_metadata(path).map_err(ReadError::Io)?;
    if !before.file_type().is_file() {
        return Err(ReadError::NotRegular);
    }
    let f = File::open(path).map_err(ReadError::Io)?;
    let m = f.metadata().map_err(ReadError::Io)?;
    if m.dev() != before.dev() || m.ino() != before.ino() || !m.is_file() {
        return Err(ReadError::Replaced);
    }
    if m.len() > bound {
        return Err(ReadError::TooLarge);
    }
    let mut bytes = Vec::new();
    f.take(bound + 1)
        .read_to_end(&mut bytes)
        .map_err(ReadError::Io)?;
    if bytes.len() as u64 > bound {
        return Err(ReadError::TooLarge);
    }
    Ok((bytes, m))
}

// SQLite names a database's side files by appending to its name.
pub fn side_file(file: &Path, side: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(side);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdir::TestDir;

    #[test]
    fn a_regular_file_is_read_whole() {
        let d = TestDir::new("whole");
        let p = d.join("f");
        std::fs::write(&p, "four").unwrap();
        let (bytes, m) = read_regular(&p, 4).unwrap();
        assert_eq!(bytes, b"four");
        assert_eq!(m.len(), 4);
    }

    #[test]
    fn a_file_over_the_bound_is_refused() {
        let d = TestDir::new("bound");
        let p = d.join("f");
        std::fs::write(&p, "five!").unwrap();
        assert!(matches!(read_regular(&p, 4), Err(ReadError::TooLarge)));
    }

    #[test]
    fn a_link_a_directory_and_a_missing_file_are_refused() {
        let d = TestDir::new("refused");
        let p = d.join("f");
        std::fs::write(&p, "x").unwrap();
        let link = d.join("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        assert!(matches!(read_regular(&link, 9), Err(ReadError::NotRegular)));
        assert!(matches!(
            read_regular(d.path(), 9),
            Err(ReadError::NotRegular)
        ));
        assert!(matches!(
            read_regular(&d.join("gone"), 9),
            Err(ReadError::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn a_side_file_is_named_after_its_database() {
        let f = side_file(Path::new("/x/state/tehas.db"), "-wal");
        assert_eq!(f, Path::new("/x/state/tehas.db-wal"));
    }

    #[test]
    fn the_read_bound_is_one_mib_in_both_counts() {
        assert_eq!(READ_BOUND, 1_048_576);
        assert_eq!(READ_BOUND_LEN as u64, READ_BOUND);
    }

    #[test]
    fn a_file_of_the_read_bound_is_read_and_one_byte_more_is_not() {
        let d = TestDir::new("read-bound");
        let p = d.join("f");
        std::fs::write(&p, vec![b'x'; READ_BOUND_LEN]).unwrap();
        assert_eq!(
            read_regular(&p, READ_BOUND).unwrap().0.len(),
            READ_BOUND_LEN
        );
        std::fs::write(&p, vec![b'x'; READ_BOUND_LEN + 1]).unwrap();
        assert!(matches!(
            read_regular(&p, READ_BOUND),
            Err(ReadError::TooLarge)
        ));
    }
}
