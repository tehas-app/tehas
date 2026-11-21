//! A plugin's filed files, copied for a cycle (plugin specification 1.0.0,
//! the v3 part, "Drafted issues, and the prototype reaching a cycle"). The
//! folder tehas kept when a person filed the issue is copied beside the
//! issue's attachments: regular files only, reached through no link, under
//! the attachments' count and size, and hashed as copied, so what the cycle
//! holds is what the hash on the record says, or the cycle does not start.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

/// The most files of one filed folder, as an issue's attached files.
pub const MOST_FILES: usize = 16;
/// The most bytes of them together, as an issue's attached files.
pub const MOST_BYTES: u64 = 4 * 1024 * 1024;
/// The deepest folder of one filed folder.
const MOST_DEPTH: usize = 4;

/// One file copied: its path in the folder, its size and its sha256.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Copied {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

/// The sha256 of a folder: of one line `<sha256>  <path>` for each file, in
/// the order of the paths, as `sha256sum` writes them. The same files make the
/// same hash wherever and whenever they are read.
pub fn tree_sha256(files: &[Copied]) -> String {
    let mut sorted: Vec<&Copied> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut h = Sha256::new();
    for f in sorted {
        h.update(format!("{}  {}\n", f.sha256, f.path).as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// A name of a folder's entry: printable, no slash, not `.` or `..`.
fn plain(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
}

/// Copy the folder `src` into `dst`, which must not exist yet. Err, with why,
/// for a link or anything that is not a folder or a regular file, a name that
/// is not plain, more files or bytes than an issue's attached files, or a
/// file that changed while it was read.
pub fn copy(src: &Path, dst: &Path) -> Result<Vec<Copied>, String> {
    let top = fs::symlink_metadata(src).map_err(|_| "the filed files are not there".to_string())?;
    if !top.is_dir() {
        return Err("the filed files are not a folder".to_string());
    }
    let mut out = Vec::new();
    let mut used = 0u64;
    walk(src, dst, "", 0, &mut out, &mut used)?;
    Ok(out)
}

fn walk(
    src: &Path,
    dst: &Path,
    rel: &str,
    depth: usize,
    out: &mut Vec<Copied>,
    used: &mut u64,
) -> Result<(), String> {
    if depth > MOST_DEPTH {
        return Err(format!(
            "the filed files are deeper than {MOST_DEPTH} folders"
        ));
    }
    fs::create_dir_all(dst)
        .map_err(|_| "the folder for the filed files could not be made".to_string())?;
    fs::set_permissions(dst, fs::Permissions::from_mode(0o755))
        .map_err(|_| "the folder for the filed files could not be made".to_string())?;
    let mut names: Vec<String> = Vec::new();
    for e in fs::read_dir(src).map_err(|_| "the filed files could not be read".to_string())? {
        let e = e.map_err(|_| "the filed files could not be read".to_string())?;
        let name = e
            .file_name()
            .into_string()
            .map_err(|_| "a filed file's name is not text".to_string())?;
        names.push(name);
    }
    names.sort();
    for name in names {
        let path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if !plain(&name) {
            return Err(format!("a filed file's name is not plain: {path:?}"));
        }
        let from = src.join(&name);
        let meta = fs::symlink_metadata(&from).map_err(|_| format!("{path} could not be read"))?;
        let kind = meta.file_type();
        if kind.is_symlink() {
            return Err(format!("{path} is a link"));
        }
        if kind.is_dir() {
            walk(&from, &dst.join(&name), &path, depth + 1, out, used)?;
            // The folder read is the one checked, not a link put in its place.
            let read =
                fs::metadata(&from).map_err(|_| format!("{path} changed while it was copied"))?;
            if read.dev() != meta.dev() || read.ino() != meta.ino() {
                return Err(format!("{path} changed while it was copied"));
            }
            continue;
        }
        if !kind.is_file() {
            return Err(format!("{path} is not a regular file"));
        }
        if out.len() >= MOST_FILES {
            return Err(format!("more than {MOST_FILES} filed files"));
        }
        if *used + meta.len() > MOST_BYTES {
            return Err(format!("the filed files pass {MOST_BYTES} bytes"));
        }
        let copied = copy_file(&from, &dst.join(&name), &meta, &path)?;
        *used += copied.bytes;
        out.push(copied);
    }
    Ok(())
}

/// One file, opened and checked to be the very file `meta` described, so a
/// link put in its place after the look is never followed into.
fn copy_file(from: &Path, to: &Path, meta: &fs::Metadata, path: &str) -> Result<Copied, String> {
    let changed = || format!("{path} changed while it was copied");
    let mut f = fs::File::open(from).map_err(|_| format!("{path} could not be read"))?;
    let opened = f.metadata().map_err(|_| changed())?;
    if !opened.is_file() || opened.dev() != meta.dev() || opened.ino() != meta.ino() {
        return Err(changed());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut f)
        .take(MOST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("{path} could not be read"))?;
    if bytes.len() as u64 != opened.len() {
        return Err(changed());
    }
    let mut w = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(|_| format!("{path} could not be written"))?;
    w.write_all(&bytes)
        .map_err(|_| format!("{path} could not be written"))?;
    fs::set_permissions(to, fs::Permissions::from_mode(0o644))
        .map_err(|_| format!("{path} could not be written"))?;
    Ok(Copied {
        path: path.to_string(),
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tehas_core::testdir::TestDir;

    fn filed(name: &str, files: &[(&str, &str)]) -> TestDir {
        let d = TestDir::new(&format!("plugin-files-{name}"));
        for (path, text) in files {
            let p = d.join("src").join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        d
    }

    #[test]
    fn a_folder_is_copied_whole_and_hashed_as_sha256sum_lists_it() {
        let d = filed(
            "ok",
            &[
                ("v2.html", "<p>b</p>"),
                ("notes.md", "# n"),
                ("img/a.png", "png"),
            ],
        );
        let got = copy(&d.join("src"), &d.join("dst")).unwrap();
        let paths: Vec<&str> = got.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["img/a.png", "notes.md", "v2.html"]);
        assert_eq!(fs::read_to_string(d.join("dst/img/a.png")).unwrap(), "png");
        let mode = fs::metadata(d.join("dst/notes.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o644);
        // sha256sum's lines, sorted by path, hashed.
        let lines: String = got
            .iter()
            .map(|c| format!("{}  {}\n", c.sha256, c.path))
            .collect();
        let want: String = Sha256::digest(lines.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(tree_sha256(&got), want);
        let mut reversed = got.clone();
        reversed.reverse();
        assert_eq!(tree_sha256(&reversed), want);
    }

    #[test]
    fn a_link_or_a_strange_file_is_refused_and_nothing_follows_it() {
        let d = filed("link", &[("a.html", "x")]);
        fs::write(d.join("secret"), "s").unwrap();
        symlink(d.join("secret"), d.join("src/b.html")).unwrap();
        assert_eq!(
            copy(&d.join("src"), &d.join("dst")).unwrap_err(),
            "b.html is a link"
        );
        assert!(!d.join("dst/b.html").exists());

        let d = filed("dirlink", &[("a.html", "x")]);
        symlink("/etc", d.join("src/etc")).unwrap();
        assert_eq!(
            copy(&d.join("src"), &d.join("dst")).unwrap_err(),
            "etc is a link"
        );

        let d = filed("top", &[]);
        symlink("/etc", d.join("link")).unwrap();
        assert!(copy(&d.join("link"), &d.join("dst")).is_err());
        assert!(copy(&d.join("missing"), &d.join("dst")).is_err());
    }

    #[test]
    fn more_files_or_bytes_than_an_issues_attachments_are_refused() {
        let many: Vec<(String, String)> = (0..=MOST_FILES)
            .map(|i| (format!("f{i:02}"), "x".into()))
            .collect();
        let refs: Vec<(&str, &str)> = many.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let d = filed("many", &refs);
        assert_eq!(
            copy(&d.join("src"), &d.join("dst")).unwrap_err(),
            format!("more than {MOST_FILES} filed files")
        );
        let big = "x".repeat(MOST_BYTES as usize / 2 + 1);
        let d = filed("big", &[("a", &big), ("b", &big)]);
        assert_eq!(
            copy(&d.join("src"), &d.join("dst")).unwrap_err(),
            format!("the filed files pass {MOST_BYTES} bytes")
        );
        let d = filed("deep", &[("a/b/c/d/e/f.html", "x")]);
        assert!(copy(&d.join("src"), &d.join("dst"))
            .unwrap_err()
            .contains("deeper than"));
    }
}
