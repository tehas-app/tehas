//! #2729 AC1: `tehas status --json` says whether the factory's analytics are
//! on, in its own field, from TEHAS_ANALYTICS of the instance.
//!
//! Today the command is a verb with no settings (console/src/front.rs:129,
//! `console/src/status.rs:77`), so the object holds no analytics field and a
//! manager can only learn the setting by running `tehas analytics copy`, which
//! ends 78 with its words (console/src/commands.rs:128).

mod common;

use common::*;
use std::fs;
use tehas_core::time::{iso, now};

/// `tehas status --json` with a healthy factory whose instance says this of
/// its analytics. The health file is fresh, so the exit is 0.
fn said(w: &World, analytics: &str) -> String {
    fs::create_dir_all(&w.live).unwrap();
    fs::write(
        w.live_file("health.json"),
        format!(
            "{{\"status\":\"pass\",\"output\":\"Idle, taking work.\",\
             \"time\":\"{}\",\"checks\":{{}}}}\n",
            iso(now() - 20),
        ),
    )
    .unwrap();
    if analytics.is_empty() {
        w.minimal(&[]);
    } else {
        w.minimal(&[&format!("TEHAS_ANALYTICS={analytics}")]);
    }
    let r = w.summary(&["--json"]);
    assert_eq!(r.rc, 0, "{}: {}", w.status().out, r.out);
    r.out
}

/// The instance's value of the switch reaches the object as it stands there,
/// not as an environment variable: tehas_load drops a TEHAS_ANALYTICS it is
/// given in the environment and takes the file's line instead
/// (test/config_test.sh:837).
fn named(out: &str, want: &str) {
    assert_eq!(
        jq(".analytics", out),
        format!("\"{want}\""),
        "the object says otherwise:\n{out}"
    );
}

#[test]
fn ac1_status_json_says_analytics_is_off_when_the_instance_says_off() {
    let w = World::new("ac1-off");
    named(&said(&w, "off"), "off");
}

#[test]
fn ac1_status_json_says_analytics_is_on_when_the_instance_says_on() {
    let w = World::new("ac1-on");
    named(&said(&w, "on"), "on");
}

/// The switch has a default, on, and an instance that never mentions it is an
/// instance with analytics on (console/core/src/settings/rules.rs:344).
#[test]
fn ac1_the_default_of_a_setting_the_instance_never_wrote_is_on() {
    let w = World::new("ac1-default");
    named(&said(&w, ""), "on");
}

/// The load exports a switch in the form of its default, so 1 and 0 are the
/// same two words as on and off (console/core/src/settings/load.rs:602).
#[test]
fn ac1_a_switch_written_as_1_or_0_is_the_same_answer() {
    for (given, want) in [("1", "on"), ("0", "off")] {
        let w = World::new(&format!("ac1-{given}"));
        named(&said(&w, given), want);
    }
}
