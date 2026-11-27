//! The stub log of a failed case names only the stubs that did not finish,
//! and gives each finished one its exit status and standard error.

mod common;

use common::World;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

fn call(w: &World, name: &str) -> std::process::ExitStatus {
    Command::new(w.t().join("bin").join(name))
        .env("T", w.t())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
}

#[test]
fn only_a_killed_stub_is_reported_as_not_finished() {
    let w = World::new("stub-log");
    w.stub("done", "echo said >&2\nexit 3");
    w.stub("signalled", "kill -TERM \"$BASHPID\"");
    w.stub("killed", "echo half >&2\nkill -KILL \"$$\"");

    assert_eq!(call(&w, "done").code(), Some(3));
    assert_eq!(call(&w, "signalled").code(), Some(128 + 15));
    assert_eq!(call(&w, "killed").signal(), Some(9));

    let log = std::fs::read_to_string(w.t().join("stub.log")).unwrap();
    assert_eq!(log, "done rc=3\nsaid\nsignalled rc=143\n");
    let unfinished = w.unfinished();
    assert_eq!(unfinished.len(), 1, "{unfinished:?}");
    let (stub, err) = &unfinished[0];
    assert!(stub.starts_with("killed."), "{stub}");
    assert_eq!(err, "half\n");
}
