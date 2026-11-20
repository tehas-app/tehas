//! The files install.sh copies, and the stale ones it sweeps.
//!
//!   console update files [--check | --preflight] [--no-steps <source>]... [--from <dir>] [--sync <name>]... <root> <host>
//!   console update files write <path>
//!
//! The first form reads the file table on standard input, one `<source>|<path>|<mode>`
//! a line. A source is a path of the checkout: absolute as it is, under `bin/`,
//! `lib/` or `libexec/` of `<root>`, anything else under `<host>`. A file whose
//! content and mode match is left alone. A file whose content matches and whose
//! mode does not is given the mode in place. Any other file is copied beside its
//! path as `<path>.new` and renamed over it, so a cycle that starts meanwhile
//! finds the old file or the new one, never a part of one. The old one is kept as
//! `<path>.prev`. Each `--sync <name>` then copies every file of the directory
//! `<name>` stands in for into `$TEHAS_HOME/<name>` as 644 and removes what is
//! there that the directory no longer has, its subdirectories in the same way
//! (the kit in `fun/kit/`). The pages that name hashed files
//! (src/update/assets.rs) are copied after every other file and the sweep comes
//! last, so a run that fails partway leaves pages that name files the home has.
//! A file in these directories is tehas's: one the directory does not have is
//! removed. A subdirectory of the home that the directory does not have is
//! kept and said so on standard error, since no install has a record of having
//! made it. A symbolic link on either side, where a sync would read, write or
//! sweep, is refused before anything is copied: a link in the home would carry
//! the copies and the sweep into the directory it points at. So is a name that
//! is a directory on one side and a file on the other. `--preflight` makes that
//! check alone and reads no table, which install.sh asks before it installs the
//! programs, so a refusal leaves the host as it was. The directory is
//! `<host>/<name>`, or
//! `<dir>/<name>` with `--from <dir>`, which is how install.sh installs the
//! dashboard's pages from the stage `console update assets` made (see
//! src/update/assets.rs): the sweep then takes back a hashed name of older
//! content along with any page that went away.
//!
//! Before each copy the steps its source calls for go into the record of steps
//! owed (src/update/steps.rs), so a run that dies partway still owes them. A
//! `--no-steps` source is copied without that.
//!
//! It prints `installed:`, `mode:` and `removed:` lines, or with --check, which
//! changes nothing, `differs:`, `missing:`, `mode:` and `stale:` lines. Each file
//! that differs is also a line `changed <source>`, for install.sh to count and to
//! name the steps by. Exit: 0 done, 1 a file could not be read, copied or recorded
//! (said on standard error, after the lines of what was done), 64 usage, 78
//! TEHAS_HOME unset.
//!
//! `write` puts standard input into `<path>` by the same rename, keeping the mode
//! of the file it replaces, 644 for a new one. Exit: 0 written, 1 not, 64 usage.

use std::fs::{self, File, Permissions};
use std::io::{self, ErrorKind, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tehas_core::file::{write_atomic, Temp};

use super::{fault, say, steps, tell};

const USAGE: &str = "console update files: usage: [--check | --preflight] [--no-steps <source>]... [--from <dir>] [--sync <name>]... <root> <host> | write <path>";

/// One line of the file table.
#[derive(Debug, PartialEq)]
struct Entry {
    src: String,
    to: PathBuf,
    mode: u32,
    /// The mode as the table wrote it, for the lines that name it.
    mode_text: String,
}

impl Entry {
    fn parse(line: &str) -> Result<Entry, String> {
        let mut fields = line.splitn(3, '|');
        let (Some(src), Some(to), Some(mode_text)) = (fields.next(), fields.next(), fields.next())
        else {
            return Err(format!(
                "the file table line {line} is not <source>|<path>|<mode>"
            ));
        };
        if src.is_empty() || to.is_empty() {
            return Err(format!(
                "the file table line {line} is not <source>|<path>|<mode>"
            ));
        }
        let mode = Some(mode_text)
            .filter(|m| (3..=4).contains(&m.len()) && m.bytes().all(|b| (b'0'..=b'7').contains(&b)))
            .and_then(|m| u32::from_str_radix(m, 8).ok())
            .ok_or_else(|| format!("the mode {mode_text} for {src} is not a number"))?;
        Ok(Entry {
            src: src.to_string(),
            to: PathBuf::from(to),
            mode,
            mode_text: mode_text.to_string(),
        })
    }
}

/// A run of the copies: where the sources are, the home the steps are recorded
/// in, and the lines said so far.
struct Copier {
    root: PathBuf,
    host: PathBuf,
    /// Where the directories of `--sync` are read from: the host, or the stage
    /// install.sh made.
    from: PathBuf,
    home: PathBuf,
    check: bool,
    no_steps: Vec<String>,
    lines: Vec<String>,
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Whether two files hold the same bytes. A file that cannot be read differs,
/// as `cmp -s` answers, so it is copied over.
fn same_content(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::metadata(a), fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    match (fs::read(a), fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// The names in a directory, without the hidden ones, in byte order: what a
/// shell's `*` gives.
fn names(dir: &Path) -> io::Result<Vec<String>> {
    let mut out: Vec<String> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    out.sort();
    Ok(out)
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

fn is_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

/// What stops a sync of `from` into `to` before it copies anything: a symbolic
/// link among the two directories, the names under `from` and the names under
/// `to` that the sweep reads, and a name of the home that is a directory where
/// the checkout has a file or the other way round. A link is not looked
/// through.
fn blockers(from: &Path, to: &Path, found: &mut Vec<String>) {
    let mut stop = false;
    for dir in [from, to] {
        if is_link(dir) {
            found.push(format!("{} is a symbolic link", dir.display()));
            stop = true;
        }
    }
    if stop {
        return;
    }
    for name in names(from).unwrap_or_default() {
        let (source, target) = (from.join(&name), to.join(&name));
        let there = fs::symlink_metadata(&target).is_ok();
        if is_dir(&source) {
            if there && !is_dir(&target) && !is_link(&target) {
                found.push(format!(
                    "{} is a file and the checkout has a directory there",
                    target.display()
                ));
            } else {
                blockers(&source, &target, found);
            }
        } else if is_link(&source) {
            found.push(format!("{} is a symbolic link", source.display()));
        } else if is_link(&target) {
            found.push(format!("{} is a symbolic link", target.display()));
        } else if is_dir(&target) {
            found.push(format!(
                "{} is a directory and the checkout has a file there",
                target.display()
            ));
        }
    }
    for name in names(to).unwrap_or_default() {
        let target = to.join(&name);
        if !from.join(&name).exists() && is_link(&target) {
            found.push(format!("{} is a symbolic link", target.display()));
        }
    }
}

/// `cp -p`: the bytes, the mode and the time of change.
fn keep_prev(to: &Path) -> io::Result<()> {
    let prev = with_suffix(to, ".prev");
    match fs::remove_file(&prev) {
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    fs::copy(to, &prev)?;
    let modified = fs::metadata(to)?.modified()?;
    File::options()
        .write(true)
        .open(&prev)?
        .set_modified(modified)
}

impl Copier {
    fn say(&mut self, line: String) {
        self.lines.push(line);
    }

    fn changed(&mut self, src: &str) {
        self.lines.push(format!("changed {src}"));
    }

    fn source_of(&self, src: &str) -> PathBuf {
        if src.starts_with('/') {
            PathBuf::from(src)
        } else if ["bin/", "libexec/", "lib/"]
            .iter()
            .any(|p| src.starts_with(p))
        {
            self.root.join(src)
        } else {
            self.host.join(src)
        }
    }

    /// Install one file if its content or its mode differs. `source` is the
    /// file copied: the checkout's for a line of the file table, and the stage's
    /// for a file of a `--sync`.
    fn put_from(&mut self, e: &Entry, source: &Path) -> Result<(), String> {
        let from = source;
        if !from.is_file() {
            return Err(format!("{} is missing from this checkout", from.display()));
        }
        let to = e.to.display().to_string();
        // A symlink is judged by the file it points at.
        let there = fs::metadata(&e.to).ok().filter(|m| m.is_file());
        if let Some(meta) = &there {
            if same_content(from, &e.to) {
                // 0644 and 644 are the same mode written two ways.
                let on_disk = meta.permissions().mode() & 0o7777;
                if on_disk == e.mode {
                    return Ok(());
                }
                self.changed(&e.src);
                if self.check {
                    self.say(format!("mode: {to} is {on_disk:o}, wants {}", e.mode_text));
                    return Ok(());
                }
                // The mode is owed, not the copy: no .new, no rename and no
                // .prev beside a file whose content is already this one's.
                fs::set_permissions(&e.to, Permissions::from_mode(e.mode))
                    .map_err(|_| format!("could not set mode {} on {to}", e.mode_text))?;
                self.say(format!("mode: {to} -> {}", e.mode_text));
                return Ok(());
            }
        }
        self.changed(&e.src);
        if self.check {
            let word = if there.is_some() {
                "differs"
            } else {
                "missing"
            };
            self.say(format!("{word}: {to}"));
            return Ok(());
        }
        // Owed before it is copied, not after: a copy that fails halfway leaves
        // behind a record of what the copies so far asked for.
        if !self.no_steps.contains(&e.src) {
            steps::record_lines(&self.home, &steps::steps_for(std::slice::from_ref(&e.src)))
                .map_err(|()| {
                    format!(
                        "could not record the follow-ups of {}; the copies so far are installed",
                        e.src
                    )
                })?;
        }
        if let Some(dir) = e.to.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir).map_err(|_| format!("could not install {to}"))?;
        }
        if there.is_some() {
            keep_prev(&e.to).map_err(|_| format!("could not keep {to} as {to}.prev"))?;
        }
        let bytes = fs::read(from).map_err(|_| format!("could not install {to}"))?;
        write_atomic(&e.to, &bytes, e.mode, &Temp::Predictable)
            .map_err(|_| format!("could not install {to}"))?;
        self.say(format!("installed: {to}"));
        Ok(())
    }

    /// `<from>/<name>`/* into $TEHAS_HOME/<name>/: the pages that name hashed
    /// files when `pages` is set, everything else when it is not. The pages go
    /// last, so a run that fails partway leaves the old pages naming the old
    /// files, which the sweep has not yet taken.
    fn copy_tree(&mut self, name: &str, pages: bool) -> Result<(), String> {
        let from_dir = self.from.join(name);
        let to_dir = self.home.join(name);
        let sources = names(&from_dir)
            .map_err(|_| format!("{} is missing from this checkout", from_dir.display()))?;
        for file in &sources {
            let src = format!("{name}/{file}");
            if from_dir.join(file).is_dir() {
                self.copy_tree(&src, pages)?;
                continue;
            }
            if super::assets::PAGES.contains(&src.as_str()) != pages {
                continue;
            }
            self.put_from(
                &Entry {
                    src: format!("{name}/{file}"),
                    to: to_dir.join(file),
                    mode: 0o644,
                    mode_text: "644".to_string(),
                },
                &from_dir.join(file),
            )?;
        }
        Ok(())
    }

    /// What $TEHAS_HOME/<name>/ holds that `<from>/<name>` no longer has, taken
    /// away. Pages come and go, and so does a hashed name whose content changed.
    fn sweep(&mut self, name: &str) {
        let from_dir = self.from.join(name);
        let to_dir = self.home.join(name);
        for file in names(&from_dir).unwrap_or_default() {
            if from_dir.join(&file).is_dir() {
                self.sweep(&format!("{name}/{file}"));
            }
        }
        if !to_dir.is_dir() {
            return;
        }
        for file in names(&to_dir).unwrap_or_default() {
            let to = to_dir.join(&file);
            if !to.exists() {
                continue;
            }
            let is_dir = fs::symlink_metadata(&to).is_ok_and(|m| m.is_dir());
            if is_dir && !from_dir.join(&file).exists() {
                fault(&format!(
                    "kept {}: this checkout has no such directory, and an install removes none: if a release dropped it, remove it by hand",
                    to.display()
                ));
                continue;
            }
            // A file an earlier run left beside the pages and did not rename
            // over is removed with the rest.
            let leftover = file.ends_with(".new");
            if !leftover
                && (file.ends_with(".prev")
                    || file == "version.txt"
                    || file == "rukas-version.txt"
                    || from_dir.join(&file).exists())
            {
                continue;
            }
            self.changed(&format!("{name}/{file}"));
            if self.check {
                self.say(format!("stale: {}", to.display()));
                continue;
            }
            let mut gone = fs::remove_file(&to);
            if !leftover {
                gone = gone.and_then(|()| match fs::remove_file(with_suffix(&to, ".prev")) {
                    Err(e) if e.kind() != ErrorKind::NotFound => Err(e),
                    _ => Ok(()),
                });
            }
            match gone {
                Ok(()) => self.say(format!("removed: {}", to.display())),
                Err(_) => fault(&format!("could not remove {}", to.display())),
            }
        }
    }

    /// The syncs' blockers as the refusal says them: before the programs with
    /// `--preflight`, when nothing is installed yet, or else after them.
    fn refuse(&self, syncs: &[String], preflight: bool) -> Result<(), String> {
        let mut found = Vec::new();
        for name in syncs {
            blockers(&self.from.join(name), &self.home.join(name), &mut found);
        }
        if found.is_empty() {
            return Ok(());
        }
        let done = if preflight {
            "nothing was installed"
        } else {
            "the files of this step were not installed, and the programs an earlier step installed stay"
        };
        Err(format!(
            "{}; {done}. An install does not copy through a symbolic link or over a file of the other kind: move each away, or replace a link with a file or directory of its own, and run again",
            found.join(", ")
        ))
    }

    fn run(&mut self, table: &str, syncs: &[String]) -> Result<(), String> {
        self.refuse(syncs, false)?;
        let entries = table
            .lines()
            .filter(|l| !l.is_empty())
            .map(Entry::parse)
            .collect::<Result<Vec<_>, _>>()?;
        for e in &entries {
            let from = self.source_of(&e.src);
            self.put_from(e, &from)?;
        }
        for pages in [false, true] {
            for name in syncs {
                self.copy_tree(name, pages)?;
            }
        }
        for name in syncs {
            self.sweep(name);
        }
        Ok(())
    }
}

/// `files write <path>`: standard input into the path.
fn write(path: &Path) -> u8 {
    let mut bytes = Vec::new();
    if io::stdin().read_to_end(&mut bytes).is_err() {
        fault(&format!("could not write {}", path.display()));
        return 1;
    }
    let mode = fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o7777)
        .unwrap_or(0o644);
    if write_atomic(path, &bytes, mode, &Temp::Predictable).is_err() {
        fault(&format!("could not write {}", path.display()));
        return 1;
    }
    0
}

pub fn verb(given: &[String]) -> u8 {
    if let [w, path] = given {
        if w == "write" {
            return write(Path::new(path));
        }
    }
    let mut check = false;
    let mut preflight = false;
    let mut no_steps = Vec::new();
    let mut syncs = Vec::new();
    let mut from = None;
    let mut places = Vec::new();
    let mut words = given.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--check" => check = true,
            "--preflight" => preflight = true,
            "--no-steps" | "--sync" | "--from" => {
                let Some(value) = words.next() else {
                    tell(USAGE);
                    return 64;
                };
                match word.as_str() {
                    "--sync" => syncs.push(value.clone()),
                    "--from" => from = Some(PathBuf::from(value)),
                    _ => no_steps.push(value.clone()),
                }
            }
            w if w.starts_with("--") => {
                tell(USAGE);
                return 64;
            }
            _ => places.push(word.clone()),
        }
    }
    let [root, host_dir] = &places[..] else {
        tell(USAGE);
        return 64;
    };
    let host = match super::host_or_refuse() {
        Ok(host) => host,
        Err(rc) => return rc,
    };
    let mut table = String::new();
    if !preflight && io::stdin().read_to_string(&mut table).is_err() {
        fault("could not read the file table");
        return 1;
    }
    let mut copier = Copier {
        root: PathBuf::from(root),
        host: PathBuf::from(host_dir),
        from: from.unwrap_or_else(|| PathBuf::from(host_dir)),
        home: host.home,
        check,
        no_steps,
        lines: Vec::new(),
    };
    let done = if preflight {
        copier.refuse(&syncs, true)
    } else {
        copier.run(&table, &syncs)
    };
    for line in &copier.lines {
        say(line);
    }
    match done {
        Ok(()) => 0,
        Err(why) => {
            fault(&why);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{dir, Dir};
    use super::*;

    fn copier(d: &Dir, check: bool) -> Copier {
        Copier {
            root: d.0.join("root"),
            host: d.0.join("root/host"),
            from: d.0.join("root/host"),
            home: d.0.join("home"),
            check,
            no_steps: Vec::new(),
            lines: Vec::new(),
        }
    }

    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    fn set_mode(path: &Path, mode: u32) {
        fs::set_permissions(path, Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn a_table_line_parses_and_a_bad_mode_is_refused() {
        let e = Entry::parse("lib/config.sh|/h/lib/config.sh|0644").unwrap();
        assert_eq!(e.mode, 0o644);
        assert_eq!(e.mode_text, "0644");
        assert_eq!(
            Entry::parse("x|/h/x|64a").unwrap_err(),
            "the mode 64a for x is not a number"
        );
        assert!(Entry::parse("x|/h/x|07755").is_err());
        assert!(Entry::parse("x|/h/x").is_err());
    }

    #[test]
    fn sources_are_found_in_the_root_or_the_host() {
        let d = dir("files-source");
        let c = copier(&d, false);
        assert_eq!(c.source_of("/abs/console"), PathBuf::from("/abs/console"));
        assert_eq!(c.source_of("bin/tehas"), d.0.join("root/bin/tehas"));
        assert_eq!(c.source_of("lib/x.sh"), d.0.join("root/lib/x.sh"));
        assert_eq!(c.source_of("env.sh"), d.0.join("root/host/env.sh"));
    }

    #[test]
    fn a_missing_file_is_named_then_installed_and_its_steps_recorded() {
        let d = dir("files-missing");
        d.file("root/host/edge/docs.Caddyfile", "new\n");
        let to = d.0.join("home/edge/docs.Caddyfile");
        let table = format!("edge/docs.Caddyfile|{}|644\n", to.display());

        let mut c = copier(&d, true);
        c.run(&table, &[]).unwrap();
        assert_eq!(
            c.lines,
            vec![
                "changed edge/docs.Caddyfile".to_string(),
                format!("missing: {}", to.display())
            ]
        );
        assert!(!to.exists());

        let mut c = copier(&d, false);
        c.run(&table, &[]).unwrap();
        assert_eq!(c.lines[1], format!("installed: {}", to.display()));
        assert_eq!(fs::read_to_string(&to).unwrap(), "new\n");
        assert_eq!(mode_of(&to), 0o644);
        assert!(!with_suffix(&to, ".new").exists());
        assert_eq!(
            steps::read_record(&d.0.join("home")).unwrap(),
            steps::steps_for(&["edge/docs.Caddyfile".to_string()])
        );

        let mut c = copier(&d, true);
        c.run(&table, &[]).unwrap();
        assert!(c.lines.is_empty());
    }

    #[test]
    fn content_drift_is_copied_and_the_old_file_kept() {
        let d = dir("files-content");
        d.file("root/bin/tehas", "new\n");
        let to = d.file("home/bin/tehas", "old\n");
        set_mode(&to, 0o755);
        let table = format!("bin/tehas|{}|755", to.display());

        let mut c = copier(&d, true);
        c.run(&table, &[]).unwrap();
        assert_eq!(c.lines[1], format!("differs: {}", to.display()));

        let mut c = copier(&d, false);
        c.no_steps.push("bin/tehas".to_string());
        c.run(&table, &[]).unwrap();
        assert_eq!(fs::read_to_string(&to).unwrap(), "new\n");
        assert_eq!(mode_of(&to), 0o755);
        let prev = with_suffix(&to, ".prev");
        assert_eq!(fs::read_to_string(&prev).unwrap(), "old\n");
        assert_eq!(mode_of(&prev), 0o755);
        // --no-steps: copied without a record.
        assert!(steps::read_record(&d.0.join("home")).unwrap().is_empty());
    }

    #[test]
    fn mode_drift_is_set_in_place_and_0644_equals_644() {
        let d = dir("files-mode");
        d.file("root/host/env.sh", "same\n");
        let to = d.file("home/env.sh", "same\n");
        set_mode(&to, 0o664);
        let table = format!("env.sh|{}|644", to.display());

        let mut c = copier(&d, true);
        c.run(&table, &[]).unwrap();
        assert_eq!(
            c.lines,
            vec![
                "changed env.sh".to_string(),
                format!("mode: {} is 664, wants 644", to.display())
            ]
        );
        assert_eq!(mode_of(&to), 0o664);

        let mut c = copier(&d, false);
        c.run(&table, &[]).unwrap();
        assert_eq!(c.lines[1], format!("mode: {} -> 644", to.display()));
        assert_eq!(mode_of(&to), 0o644);
        assert!(!with_suffix(&to, ".prev").exists());
        assert!(steps::read_record(&d.0.join("home")).unwrap().is_empty());

        let mut c = copier(&d, true);
        c.run(&format!("env.sh|{}|0644", to.display()), &[])
            .unwrap();
        assert!(c.lines.is_empty());
    }

    #[test]
    fn a_missing_source_stops_the_run_after_the_copies_before_it() {
        let d = dir("files-nosource");
        d.file("root/host/a", "a\n");
        let a = d.0.join("home/a");
        let b = d.0.join("home/b");
        let table = format!("a|{}|644\nb|{}|644\n", a.display(), b.display());
        let mut c = copier(&d, false);
        let why = c.run(&table, &[]).unwrap_err();
        assert_eq!(
            why,
            format!(
                "{} is missing from this checkout",
                d.0.join("root/host/b").display()
            )
        );
        assert!(a.exists());
        assert!(!b.exists());
    }

    #[test]
    fn a_failed_copy_leaves_no_new_file() {
        let d = dir("files-fail");
        d.file("root/host/f", "new\n");
        // The path is a directory, so the rename over it fails.
        let to = d.0.join("home/f");
        fs::create_dir_all(to.join("inside")).unwrap();
        let table = format!("f|{}|644", to.display());
        let mut c = copier(&d, false);
        let why = c.run(&table, &[]).unwrap_err();
        assert_eq!(why, format!("could not install {}", to.display()));
        assert!(!with_suffix(&to, ".new").exists());
    }

    #[test]
    fn a_stale_page_is_named_then_removed_and_kept_files_stay() {
        let d = dir("files-stale");
        d.file("root/host/docs/a.md", "a\n");
        let home = d.0.join("home/docs");
        d.file("home/docs/a.md", "a\n");
        d.file("home/docs/gone.md", "x\n");
        d.file("home/docs/gone.md.prev", "x\n");
        d.file("home/docs/a.md.new", "half\n");
        d.file("home/docs/version.txt", "v1\n");
        d.file("home/docs/rukas-version.txt", "v1\n");
        set_mode(&home.join("a.md"), 0o644);
        let syncs = vec!["docs".to_string()];

        let mut c = copier(&d, true);
        c.run("", &syncs).unwrap();
        assert_eq!(
            c.lines,
            vec![
                "changed docs/a.md.new".to_string(),
                format!("stale: {}", home.join("a.md.new").display()),
                "changed docs/gone.md".to_string(),
                format!("stale: {}", home.join("gone.md").display()),
            ]
        );

        let mut c = copier(&d, false);
        c.run("", &syncs).unwrap();
        assert!(c
            .lines
            .contains(&format!("removed: {}", home.join("gone.md").display())));
        assert!(c
            .lines
            .contains(&format!("removed: {}", home.join("a.md.new").display())));
        assert!(!home.join("gone.md").exists());
        assert!(!home.join("gone.md.prev").exists());
        assert!(home.join("a.md").exists());
        assert!(home.join("version.txt").exists());
        assert!(home.join("rukas-version.txt").exists());
    }

    #[test]
    fn a_new_page_is_synced_in_as_644() {
        let d = dir("files-sync");
        d.file("root/host/fun/index.html", "hi\n");
        let mut c = copier(&d, false);
        c.run("", &["fun".to_string()]).unwrap();
        let to = d.0.join("home/fun/index.html");
        assert_eq!(fs::read_to_string(&to).unwrap(), "hi\n");
        assert_eq!(mode_of(&to), 0o644);
        assert_eq!(c.lines[0], "changed fun/index.html");
    }

    #[test]
    fn a_sync_copies_subdirectories_and_sweeps_them_too() {
        let d = dir("files-subdirs");
        d.file("root/host/fun/index.html", "hi\n");
        d.file("root/host/fun/kit/island.js", "// island\n");
        d.file("root/host/fun/kit/vendor/solid.js", "// solid\n");
        let syncs = ["fun".to_string()];
        let mut c = copier(&d, false);
        c.run("", &syncs).unwrap();
        let home = d.0.join("home/fun");
        assert_eq!(
            fs::read_to_string(home.join("kit/vendor/solid.js")).unwrap(),
            "// solid\n"
        );
        assert_eq!(mode_of(&home.join("kit/island.js")), 0o644);
        assert!(c
            .lines
            .contains(&"changed fun/kit/vendor/solid.js".to_string()));

        // A second run changes nothing.
        let mut c = copier(&d, true);
        c.run("", &syncs).unwrap();
        assert_eq!(c.lines, Vec::<String>::new());

        // A file the checkout no longer has is stale. A directory it does not
        // have is kept: it may be a person's own, so it is never named stale.
        d.file("home/fun/kit/gone.js", "x\n");
        d.file("home/fun/old/a.js", "x\n");
        let mut c = copier(&d, true);
        c.run("", &syncs).unwrap();
        assert_eq!(
            c.lines,
            vec![
                "changed fun/kit/gone.js".to_string(),
                format!("stale: {}", home.join("kit/gone.js").display()),
            ]
        );
        let mut c = copier(&d, false);
        c.run("", &syncs).unwrap();
        assert!(!home.join("kit/gone.js").exists());
        assert_eq!(fs::read_to_string(home.join("old/a.js")).unwrap(), "x\n");
        assert!(home.join("kit/vendor/solid.js").is_file());
        assert!(!c.lines.iter().any(|l| l.contains("old")), "{:?}", c.lines);
    }

    /// Every file under `dir` with its bytes, at any depth, links not followed.
    fn tree(dir: &Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for name in names(dir).unwrap() {
            let path = dir.join(&name);
            if fs::symlink_metadata(&path).unwrap().is_dir() {
                for (inner, text) in tree(&path) {
                    out.push((format!("{name}/{inner}"), text));
                }
            } else {
                out.push((name, fs::read_to_string(&path).unwrap_or_default()));
            }
        }
        out
    }

    #[test]
    fn a_symlinked_directory_in_the_home_is_refused_and_its_target_left_alone() {
        let d = dir("files-home-link");
        d.file("root/host/fun/index.html", "hi\n");
        d.file("root/host/fun/kit/island.js", "// island\n");
        // fun/kit in the home points outside it, at a directory of a person's
        // own: a file the checkout has, and one it lacks.
        d.file("elsewhere/island.js", "mine\n");
        d.file("elsewhere/notes.txt", "keep me\n");
        fs::create_dir_all(d.0.join("home/fun")).unwrap();
        std::os::unix::fs::symlink(d.0.join("elsewhere"), d.0.join("home/fun/kit")).unwrap();
        let before = tree(&d.0.join("elsewhere"));
        for check in [true, false] {
            let mut c = copier(&d, check);
            let why = c.run("", &["fun".to_string()]).unwrap_err();
            assert!(why.contains("home/fun/kit is a symbolic link"), "{why}");
            // update.rs reads this phrase as an install that changed nothing,
            // which a run after the programs is not.
            assert!(!why.contains("nothing was installed"), "{why}");
            assert_eq!(c.lines, Vec::<String>::new());
        }
        // The preflight install.sh asks before the programs says so.
        let why = copier(&d, false)
            .refuse(&["fun".to_string()], true)
            .unwrap_err();
        assert!(why.contains("home/fun/kit is a symbolic link"), "{why}");
        assert!(why.contains("nothing was installed"), "{why}");
        assert_eq!(tree(&d.0.join("elsewhere")), before);
        assert!(is_link(&d.0.join("home/fun/kit")));
        // Nothing else was copied either: the refusal comes first.
        assert!(!d.0.join("home/fun/index.html").exists());
    }

    #[test]
    fn a_symlink_in_the_checkout_or_the_stage_is_refused() {
        let d = dir("files-source-link");
        d.file("root/host/fun/index.html", "hi\n");
        d.file("outside/secret.txt", "not for the home\n");
        std::os::unix::fs::symlink(d.0.join("outside"), d.0.join("root/host/fun/kit")).unwrap();
        std::os::unix::fs::symlink(
            d.0.join("outside/secret.txt"),
            d.0.join("root/host/fun/page.js"),
        )
        .unwrap();
        let mut c = copier(&d, false);
        let why = c.run("", &["fun".to_string()]).unwrap_err();
        assert!(why.contains("root/host/fun/kit"), "{why}");
        assert!(why.contains("root/host/fun/page.js"), "{why}");
        assert!(!d.0.join("home/fun").exists());

        // A file of the home that is a link, under a name the sync writes or
        // sweeps, is refused too: neither is written through or removed.
        let d = dir("files-file-link");
        d.file("root/host/fun/index.html", "hi\n");
        d.file("outside/a.txt", "a\n");
        d.file("outside/b.txt", "b\n");
        fs::create_dir_all(d.0.join("home/fun")).unwrap();
        std::os::unix::fs::symlink(d.0.join("outside/a.txt"), d.0.join("home/fun/index.html"))
            .unwrap();
        std::os::unix::fs::symlink(d.0.join("outside/b.txt"), d.0.join("home/fun/gone.js"))
            .unwrap();
        let mut c = copier(&d, false);
        let why = c.run("", &["fun".to_string()]).unwrap_err();
        assert!(why.contains("home/fun/index.html"), "{why}");
        assert!(why.contains("home/fun/gone.js"), "{why}");
        assert!(is_link(&d.0.join("home/fun/index.html")));
        assert!(is_link(&d.0.join("home/fun/gone.js")));
        assert_eq!(
            fs::read_to_string(d.0.join("outside/a.txt")).unwrap(),
            "a\n"
        );

        // A home whose fun directory is itself a link is refused the same way.
        let d = dir("files-top-link");
        d.file("root/host/fun/index.html", "hi\n");
        fs::create_dir_all(d.0.join("outside")).unwrap();
        fs::create_dir_all(d.0.join("home")).unwrap();
        std::os::unix::fs::symlink(d.0.join("outside"), d.0.join("home/fun")).unwrap();
        let mut c = copier(&d, false);
        assert!(c.run("", &["fun".to_string()]).is_err());
        assert!(!d.0.join("outside/index.html").exists());
    }

    #[test]
    fn a_name_of_the_other_kind_in_the_home_is_refused_before_any_copy() {
        let d = dir("files-kinds");
        d.file("root/host/fun/index.html", "hi\n");
        d.file("root/host/fun/extra/a.js", "// a\n");
        d.file("root/host/fun/zz", "a file\n");
        // The home has a file where the checkout has a directory, and a
        // directory, holding a person's file, where it has a file.
        d.file("home/fun/extra", "mine\n");
        d.file("home/fun/zz/keep.txt", "keep me\n");
        for check in [true, false] {
            let mut c = copier(&d, check);
            let why = c.run("", &["fun".to_string()]).unwrap_err();
            assert!(
                why.contains(&format!(
                    "{} is a file and the checkout has a directory there",
                    d.0.join("home/fun/extra").display()
                )),
                "{why}"
            );
            assert!(
                why.contains(&format!(
                    "{} is a directory and the checkout has a file there",
                    d.0.join("home/fun/zz").display()
                )),
                "{why}"
            );
            assert!(why.contains("move each away"), "{why}");
            assert_eq!(c.lines, Vec::<String>::new());
        }
        assert!(copier(&d, false)
            .refuse(&["fun".to_string()], true)
            .is_err());
        assert!(!d.0.join("home/fun/index.html").exists());
        assert_eq!(
            fs::read_to_string(d.0.join("home/fun/zz/keep.txt")).unwrap(),
            "keep me\n"
        );
        // Moved away, the next run installs.
        fs::remove_file(d.0.join("home/fun/extra")).unwrap();
        fs::remove_dir_all(d.0.join("home/fun/zz")).unwrap();
        assert!(copier(&d, false).refuse(&["fun".to_string()], true).is_ok());
        copier(&d, false).run("", &["fun".to_string()]).unwrap();
        assert!(d.0.join("home/fun/extra/a.js").is_file());
        assert!(d.0.join("home/fun/zz").is_file());
    }

    #[test]
    fn the_pages_are_copied_after_the_files_they_name() {
        // A copy that fails partway leaves the old pages, which name files the
        // home still has, and sweeps nothing.
        let d = dir("files-pages-last");
        d.file("stage/docs/dashboard.frag", "names kit.bbb.css\n");
        d.file("stage/docs/a.md", "a\n");
        d.file("stage/fun/index.html", "names kit.bbb.css\n");
        d.file("stage/fun/kit/kit.bbb.css", "new\n");
        d.file("home/docs/dashboard.frag", "names kit.aaa.css\n");
        d.file("home/fun/index.html", "names kit.aaa.css\n");
        d.file("home/fun/kit/kit.aaa.css", "old\n");
        let syncs = ["docs".to_string(), "fun".to_string()];
        set_mode(&d.0.join("home/fun/kit"), 0o555);
        let mut c = copier(&d, false);
        c.from = d.0.join("stage");
        let failed = c.run("", &syncs);
        set_mode(&d.0.join("home/fun/kit"), 0o755);
        assert!(failed.is_err());
        assert!(d.0.join("home/docs/a.md").is_file());
        for page in ["docs/dashboard.frag", "fun/index.html"] {
            assert_eq!(
                fs::read_to_string(d.0.join("home").join(page)).unwrap(),
                "names kit.aaa.css\n",
                "{page}"
            );
        }
        assert!(d.0.join("home/fun/kit/kit.aaa.css").is_file());

        let mut c = copier(&d, false);
        c.from = d.0.join("stage");
        c.run("", &syncs).unwrap();
        let installed: Vec<&String> = c
            .lines
            .iter()
            .filter(|l| l.starts_with("installed: "))
            .collect();
        let at = |name: &str| installed.iter().position(|l| l.ends_with(name)).unwrap();
        assert!(at("kit.bbb.css") < at("dashboard.frag"), "{installed:?}");
        assert!(at("kit.bbb.css") < at("index.html"), "{installed:?}");
        assert!(!d.0.join("home/fun/kit/kit.aaa.css").exists());
    }

    #[test]
    fn a_sync_reads_the_stage_and_sweeps_what_only_the_stage_holds() {
        // --from is how install.sh installs the dashboard's pages from the stage
        // `console update assets` made: the stage's own file is the one that
        // comes, and a hashed name the stage no longer has is stale.
        let d = dir("files-from");
        d.file("root/host/fun/index.html", "the checkout's page\n");
        d.file("stage/fun/index.html", "the staged page\n");
        d.file("stage/fun/panels.ab12cd34ef56.js", "// the panel\n");
        let mut c = copier(&d, false);
        c.from = d.0.join("stage");
        c.run("", &["fun".to_string()]).unwrap();
        assert_eq!(
            fs::read_to_string(d.0.join("home/fun/index.html")).unwrap(),
            "the staged page\n"
        );
        assert!(d.0.join("home/fun/panels.ab12cd34ef56.js").is_file());

        // The content changed, so the stage names a different hash: the old one
        // is named as stale and taken away with its .prev.
        fs::write(d.0.join("home/fun/panels.ab12cd34ef56.js"), "// old\n").unwrap();
        fs::write(
            d.0.join("home/fun/panels.ab12cd34ef56.js.prev"),
            "// older\n",
        )
        .unwrap();
        fs::remove_file(d.0.join("stage/fun/panels.ab12cd34ef56.js")).unwrap();
        d.file("stage/fun/panels.998877665544.js", "// new\n");
        let mut c = copier(&d, true);
        c.from = d.0.join("stage");
        c.run("", &["fun".to_string()]).unwrap();
        assert!(
            c.lines
                .iter()
                .any(|l| l.starts_with("stale: ") && l.ends_with("panels.ab12cd34ef56.js")),
            "{:?}",
            c.lines
        );
        let mut c = copier(&d, false);
        c.from = d.0.join("stage");
        c.run("", &["fun".to_string()]).unwrap();
        assert!(!d.0.join("home/fun/panels.ab12cd34ef56.js").exists());
        assert!(!d.0.join("home/fun/panels.ab12cd34ef56.js.prev").exists());
        assert!(d.0.join("home/fun/panels.998877665544.js").is_file());
    }
}
