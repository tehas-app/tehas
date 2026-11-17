//! How often the dispatcher timer polls: TEHAS_POLL_SECS. The unit file
//! host/systemd/tehas-dispatch.timer keeps the default, two minutes. Another
//! value is a drop-in beside it that sets all three of the unit's times, so a
//! failed poll still arms the timer again as the unit file says why. tehas
//! setup and tehas update write it, or remove it when the setting is back at
//! the default, and leave it alone when it already says what the setting says.

use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tehas_core::file::{write_atomic, Temp};
use tehas_core::settings::rules::{self, Trees};

pub const SETTING: &str = "TEHAS_POLL_SECS";
pub const DEFAULT: u64 = 120;
pub const TIMER: &str = "tehas-dispatch.timer";

/// What a sync did to the drop-in.
#[derive(Debug, PartialEq)]
pub enum Change {
    Written,
    Removed,
    Same,
}

impl Change {
    /// Whether systemd has to reload and the timer to restart.
    pub fn changed(&self) -> bool {
        *self != Change::Same
    }
}

/// Where install.sh puts the user units.
pub fn user_units() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(|h| PathBuf::from(h).join(".config/systemd/user"))
}

pub fn drop_in(units: &Path) -> PathBuf {
    units.join(format!("{TIMER}.d/poll.conf"))
}

/// The seconds a value of the setting gives. Unset or empty is the default,
/// as the load gives it.
pub fn parse(value: Option<&str>) -> Result<u64, String> {
    let value = value.unwrap_or_default();
    if value.is_empty() {
        return Ok(DEFAULT);
    }
    rules::check(SETTING, value, &Trees::default())
        .map_err(|why| format!("{SETTING} is not valid: {why}"))?;
    value
        .parse()
        .map_err(|_| format!("{SETTING} is not valid: {value} is no number"))
}

/// Each time reset first: a drop-in adds to the times of the unit file
/// otherwise, and the timer would fire at both.
fn text(secs: u64) -> String {
    let mut text = format!("# Written by tehas from {SETTING}={secs}.\n[Timer]\n");
    for key in ["OnBootSec", "OnUnitInactiveSec", "OnUnitActiveSec"] {
        text.push_str(&format!("{key}=\n{key}={secs}s\n"));
    }
    text
}

/// Bring the drop-in under `units` in line with `value`. A malformed value
/// writes and removes nothing.
pub fn sync(units: &Path, value: Option<&str>) -> Result<Change, String> {
    let secs = parse(value)?;
    let file = drop_in(units);
    let fail = |e: std::io::Error| format!("{}: {e}", file.display());
    if secs == DEFAULT {
        return match fs::remove_file(&file) {
            Ok(()) => {
                if let Some(dir) = file.parent() {
                    let _ = fs::remove_dir(dir);
                }
                Ok(Change::Removed)
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Change::Same),
            Err(e) => Err(fail(e)),
        };
    }
    let want = text(secs);
    if fs::read_to_string(&file).is_ok_and(|have| have == want) {
        return Ok(Change::Same);
    }
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(fail)?;
    }
    write_atomic(&file, want.as_bytes(), 0o644, &Temp::Pid).map_err(fail)?;
    Ok(Change::Written)
}

/// The seconds the drop-in under `units` sets, for `tehas status`. None when
/// there is none or it is not one this module wrote.
pub fn written(units: &Path) -> Option<u64> {
    let have = fs::read_to_string(drop_in(units)).ok()?;
    let secs = have
        .lines()
        .filter_map(|l| l.strip_prefix("OnUnitActiveSec="))
        .filter_map(|v| v.strip_suffix('s'))
        .find_map(|v| v.parse().ok())?;
    (have == text(secs)).then_some(secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    #[test]
    fn the_default_writes_nothing() {
        let d = TestDir::new("poll-default");
        for value in [None, Some(""), Some("120")] {
            assert_eq!(sync(&d.join("units"), value), Ok(Change::Same));
        }
        assert!(!d.join("units").exists());
    }

    #[test]
    fn another_value_resets_and_sets_all_three_times() {
        let d = TestDir::new("poll-write");
        let units = d.join("units");
        assert_eq!(sync(&units, Some("300")), Ok(Change::Written));
        assert_eq!(
            fs::read_to_string(drop_in(&units)).unwrap(),
            "# Written by tehas from TEHAS_POLL_SECS=300.\n[Timer]\n\
             OnBootSec=\nOnBootSec=300s\n\
             OnUnitInactiveSec=\nOnUnitInactiveSec=300s\n\
             OnUnitActiveSec=\nOnUnitActiveSec=300s\n"
        );
        assert_eq!(written(&units), Some(300));
    }

    #[test]
    fn the_same_value_again_is_left_alone() {
        let d = TestDir::new("poll-same");
        let units = d.join("units");
        assert_eq!(sync(&units, Some("600")), Ok(Change::Written));
        assert_eq!(sync(&units, Some("600")), Ok(Change::Same));
        assert_eq!(sync(&units, Some("900")), Ok(Change::Written));
        assert_eq!(written(&units), Some(900));
    }

    #[test]
    fn the_default_again_removes_the_drop_in() {
        let d = TestDir::new("poll-remove");
        let units = d.join("units");
        assert_eq!(sync(&units, Some("60")), Ok(Change::Written));
        assert_eq!(sync(&units, None), Ok(Change::Removed));
        assert!(!drop_in(&units).exists());
        assert!(!units.join("tehas-dispatch.timer.d").exists());
        assert_eq!(written(&units), None);
        assert_eq!(sync(&units, Some("120")), Ok(Change::Same));
    }

    #[test]
    fn a_malformed_value_touches_nothing() {
        let d = TestDir::new("poll-bad");
        let units = d.join("units");
        assert_eq!(sync(&units, Some("300")), Ok(Change::Written));
        for bad in ["29", "3601", "2min", "0300"] {
            assert_eq!(
                sync(&units, Some(bad)),
                Err(
                    "TEHAS_POLL_SECS is not valid: expected a whole number of seconds from 30 to 3600"
                        .to_string()
                )
            );
        }
        assert_eq!(written(&units), Some(300));
    }

    #[test]
    fn a_drop_in_written_by_hand_is_not_read_as_ours() {
        let d = TestDir::new("poll-hand");
        let units = d.join("units");
        fs::create_dir_all(drop_in(&units).parent().unwrap()).unwrap();
        fs::write(drop_in(&units), "[Timer]\nOnUnitActiveSec=5min\n").unwrap();
        assert_eq!(written(&units), None);
        assert_eq!(sync(&units, Some("300")), Ok(Change::Written));
        assert_eq!(written(&units), Some(300));
    }
}
