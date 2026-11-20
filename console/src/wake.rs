//! `tehas wake [--json]`: ask for a poll now and return at once, the ask of
//! the factory API that `tehas dispatch --soon` makes for a person. Both write
//! `state/dispatch.again`, which `tehas-dispatch.path` starts a poll on. While
//! a poll runs the file waits, and the next poll starts when that one ends.
//!
//! Exit: 0 the poll is asked for; 1 the file could not be written; 64 an
//! argument other than `--json`; 78 the settings are missing or malformed.

use std::env;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: tehas wake [--json]";

/// Whether the arguments ask for `--json`, or 64 with the usage.
pub fn words(given: &[String]) -> Result<bool, u8> {
    match given {
        [] => Ok(false),
        [flag] if flag == "--json" => Ok(true),
        _ => {
            eprintln!("{USAGE}");
            Err(64)
        }
    }
}

/// Writes the file a poll is started on, and names it.
pub fn ask(home: &Path) -> Result<PathBuf, (PathBuf, io::Error)> {
    let again = home.join("state/dispatch.again");
    match File::create(&again) {
        Ok(_) => Ok(again),
        Err(e) => Err((again, e)),
    }
}

/// The object `--json` prints.
fn object(asked: bool) -> String {
    format!("{{\"api\":1,\"asked\":{asked}}}")
}

pub fn main(given: &[String]) -> u8 {
    let json = match words(given) {
        Ok(j) => j,
        Err(rc) => return rc,
    };
    let home = PathBuf::from(env::var_os("TEHAS_HOME").unwrap_or_default());
    let asked = ask(&home);
    match &asked {
        Ok(again) if !json => {
            tehas_core::outln!("tehas wake: a poll is asked for ({})", again.display())
        }
        Err((again, e)) => eprintln!("tehas wake: could not write {}: {e}", again.display()),
        Ok(_) => {}
    }
    if json {
        tehas_core::outln!("{}", object(asked.is_ok()));
    }
    if asked.is_ok() {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn it_takes_json_alone() {
        assert_eq!(words(&[]), Ok(false));
        assert_eq!(words(&w(&["--json"])), Ok(true));
        for bad in [&["--soon"][..], &["--json", "--json"], &[""], &["now"]] {
            assert_eq!(words(&w(bad)), Err(64), "{bad:?}");
        }
    }

    #[test]
    fn the_ask_writes_the_file_the_path_unit_starts_a_poll_on() {
        let home = std::env::temp_dir().join(format!("tehas-wake-{}", std::process::id()));
        std::fs::create_dir_all(home.join("state")).unwrap();
        let again = ask(&home).unwrap();
        assert_eq!(again, home.join("state/dispatch.again"));
        assert!(again.is_file());
        // A second ask while the first waits is the same one ask.
        assert_eq!(ask(&home).unwrap(), again);
        std::fs::remove_dir_all(&home).unwrap();
        assert!(ask(&home).is_err());
    }

    #[test]
    fn the_object_is_the_sample() {
        let sample: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api/wake.sample.json"
        )))
        .unwrap();
        let printed: serde_json::Value = serde_json::from_str(&object(true)).unwrap();
        assert_eq!(printed, sample);
        assert_eq!(object(false), "{\"api\":1,\"asked\":false}");
    }
}
