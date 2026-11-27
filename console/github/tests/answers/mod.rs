//! Where a replayed answer is: `tests/recorded/<area>/<state>/` holds
//! GitHub's answers, recorded or made by hand in their shape, and
//! `tests/synthetic/<area>/<state>/` the answers GitHub cannot give, made
//! by hand (its README says each file's source and change).

use std::path::{Path, PathBuf};

/// The file `name` of a state: the synthetic one where there is one, else
/// the recording.
// Each test binary compiles its own copy; the state-shaped readers that
// call this live in other binaries than the ones including replay.
#[allow(dead_code)]
pub fn at(area: &str, state: &str, name: &str) -> PathBuf {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    pick(
        &tests.join("synthetic").join(area).join(state).join(name),
        &tests.join("recorded").join(area).join(state).join(name),
    )
}

/// The file `name` under `dir`, of either tree: None when neither has it.
// Reached only through the replay runner, which the stub-based binaries
// include but never call.
#[allow(dead_code)]
pub fn find(dir: &str, name: &str) -> Option<PathBuf> {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    pick_opt(
        &tests.join("synthetic").join(dir).join(name),
        &tests.join("recorded").join(dir).join(name),
    )
}

/// As [`pick`], with no third answer for neither tree.
fn pick_opt(synthetic: &Path, recorded: &Path) -> Option<PathBuf> {
    match (synthetic.exists(), recorded.exists()) {
        (true, true) => panic!(
            "{} and {} are both there: a synthetic answer may not shadow a recording",
            synthetic.display(),
            recorded.display()
        ),
        (true, false) => Some(synthetic.to_path_buf()),
        (false, false) => None,
        (false, true) => Some(recorded.to_path_buf()),
    }
}

/// Panics when both exist, so a stub never hides a recording.
fn pick(synthetic: &Path, recorded: &Path) -> PathBuf {
    pick_opt(synthetic, recorded).unwrap_or_else(|| recorded.to_path_buf())
}

#[test]
fn a_name_in_both_trees_panics() {
    let root = std::env::temp_dir().join(format!("tehas-answers-{}", std::process::id()));
    let (synthetic, recorded) = (root.join("synthetic"), root.join("recorded"));
    std::fs::create_dir_all(&root).unwrap();
    assert_eq!(pick(&synthetic, &recorded), recorded);
    std::fs::write(&synthetic, "").unwrap();
    assert_eq!(pick(&synthetic, &recorded), synthetic);
    std::fs::write(&recorded, "").unwrap();
    let both = std::panic::catch_unwind(|| pick(&synthetic, &recorded));
    std::fs::remove_dir_all(&root).unwrap();
    let why = both.expect_err("a name in both trees must panic");
    let why = why.downcast_ref::<String>().unwrap();
    assert!(why.contains("may not shadow a recording"), "{why}");
}
