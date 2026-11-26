//! Where a replayed answer is: `tests/recorded/<area>/<state>/` holds
//! GitHub's answers, recorded or made by hand in their shape, and
//! `tests/synthetic/<area>/<state>/` the answers GitHub cannot give, made
//! by hand (its README says each file's source and change).

use std::path::PathBuf;

/// The file `name` of a state: the synthetic one where there is one, else
/// the recording.
pub fn at(area: &str, state: &str, name: &str) -> PathBuf {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let synthetic = tests.join("synthetic").join(area).join(state).join(name);
    match synthetic.exists() {
        true => synthetic,
        false => tests.join("recorded").join(area).join(state).join(name),
    }
}
