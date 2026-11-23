//! What the 2.1 `feed:host` keeps of the snapshot of `tehas top`, and the loop
//! that writes it every five seconds.
//!
//! The feed can be on the internet, so what is copied is a list: [`published`]
//! keeps the keys of [`PUBLISHED`] and nothing else. A key added to the
//! snapshot later stays out of the feed until someone names it here. What is
//! inside a listed key is fixed by the code that builds it, in `tehas top`.
//!
//! [`every_mark`] runs one child per 5-second mark, each under a bound: a hung
//! child is one missed mark. 78 from a child ends the loop, and any other
//! failure tries again at the next mark.
//!
//! Until 2.0 this was `tehasbot top`, which wrote live/top.json. The unit that
//! samples and writes the feed comes with the feed in 2.1.

use std::path::Path;
use std::time::Duration;
use tehas_core::file::Temp;
use tehas_core::jq::J;
use tehas_core::time;

/// The keys of a snapshot that may be published, and nothing else.
pub const PUBLISHED: [&str; 6] = ["generatedAt", "factory", "queue", "cycle", "host", "units"];

/// Seconds between the marks.
pub const PERIOD: i64 = 5;

/// Seconds a child is given to write one mark.
pub const MARK_WAIT_SECS: u64 = 30;

/// The part of a snapshot that may be published, one line. Nothing for what is
/// no JSON object.
pub fn published(snapshot: &str) -> Option<String> {
    match J::parse(snapshot)? {
        J::Obj(fields) => {
            let kept = fields
                .into_iter()
                .filter(|(key, _)| PUBLISHED.contains(&key.as_str()))
                .collect();
            Some(J::Obj(kept).dump())
        }
        _ => None,
    }
}

/// Writes a published line to `file` atomically, readable by the web server.
/// A file that cannot be written leaves the old one in place.
pub fn write(file: &Path, line: &str) -> std::io::Result<()> {
    tehas_core::file::write_atomic(
        file,
        format!("{line}\n").as_bytes(),
        0o644,
        &Temp::Predictable,
    )
}

/// Seconds from `now` to the next multiple of [`PERIOD`].
pub fn to_mark(now: i64) -> u64 {
    (PERIOD - now.rem_euclid(PERIOD)) as u64
}

/// Runs `program` once per mark, each run stopped at `bound` (TERM, then KILL
/// after `grace`), until one exits 78. Returns 78.
pub fn every_mark(program: &str, args: &[String], bound: Duration, grace: Duration) -> u8 {
    loop {
        if tehas_core::call::run_grace(program, args, None, bound, grace) == Some(78) {
            return 78;
        }
        std::thread::sleep(Duration::from_secs(to_mark(time::now())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn only_the_listed_keys_are_published_in_the_order_they_came() {
        let snapshot = r#"{"token":"ghp_secret","host":{"load":[0.10]},"reading":{"clock":"1"},
            "console":{"lines":["x"]},"generatedAt":"t","factory":null,"queue":1,"cycle":[],
            "units":[],"energy":null,"draft":{"note":"no"}}"#;
        assert_eq!(
            published(snapshot).unwrap(),
            r#"{"host":{"load":[0.10]},"generatedAt":"t","factory":null,"queue":1,"cycle":[],"units":[]}"#
        );
    }

    #[test]
    fn a_cut_answers_nothing_for_what_is_no_snapshot() {
        for text in ["", "{not json}", "[]", "1"] {
            assert_eq!(published(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_write_replaces_the_file_and_a_failed_one_keeps_it() {
        let d = tehas_core::testdir::TestDir::new("host-feed-write");
        let file = d.join("host.json");
        write(&file, "{\"queue\":1}").unwrap();
        write(&file, "{\"queue\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{\"queue\":2}\n");
        // A directory where the temporary file goes: the write fails.
        std::fs::create_dir(d.join("host.json.new")).unwrap();
        assert!(write(&file, "{\"queue\":3}").is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{\"queue\":2}\n");
    }

    #[test]
    fn the_next_mark_is_the_next_multiple_of_the_period() {
        assert_eq!(to_mark(100), 5);
        assert_eq!(to_mark(101), 4);
        assert_eq!(to_mark(104), 1);
    }

    /// A child that never ends is stopped at the bound and costs one mark, and
    /// the 78 of the next ends the loop. The child sleeps far past the bound,
    /// so a bound that went away would take this case with it.
    #[test]
    fn the_loop_bounds_its_child_and_ends_only_on_78() {
        let d = tehas_core::testdir::TestDir::new("host-feed-bound");
        let child = d.join("child");
        tehas_core::testing::write_exe(
            &child,
            "#!/bin/sh\nif [ -f \"$1\" ]; then exit 78; fi\n: >\"$1\"\nexec sleep 600\n",
        )
        .unwrap();
        let mark = d.join("mark").to_string_lossy().into_owned();
        let began = Instant::now();
        let ended = every_mark(
            &child.to_string_lossy(),
            &[mark],
            Duration::from_secs(MARK_WAIT_SECS),
            Duration::from_secs(1),
        );
        assert_eq!(
            ended, 78,
            "the loop goes on past a hung child and ends on 78"
        );
        assert!(
            began.elapsed() < Duration::from_secs(70),
            "the hung child was stopped at its bound, not when its sleep did: {}s",
            began.elapsed().as_secs()
        );
    }
}
