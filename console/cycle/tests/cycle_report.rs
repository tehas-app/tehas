//! Who reports a cycle: tehas-cycle, in the process that ran it. What the
//! report says for each end is the other cycle_* tests; the console in front
//! of it is test/cycle_front_test.sh.

mod common;

use common::{has, twin, World};
use tehas_core::testing::write_exe;

#[test]
fn tehas_cycle_reports_in_the_process_of_the_cycle_nothing_is_left_open_in_the_journal_and_the_cycle_is_on_record(
) {
    let w = World::new("report-process");
    // A tehas-cycle where the cycle would find one, noting each verb asked of
    // it: the report is no second program.
    write_exe(
        w.home.join("libexec/tehas-cycle"),
        "#!/usr/bin/env bash\necho \"$1\" >>\"$T/calls/verbs\"\nexit 1\n",
    )
    .unwrap();
    w.ended("aborted", "develop", "", "");
    let r = w.run7();
    assert_eq!(r.rc, 0);
    assert_eq!(w.calls("verbs"), "");
    has(&r.out, "done #7 rc=20 status=aborted step=develop");
    assert!(!w.row(&["cycle-show", "7"])["cycle"]["ended_at"].is_null());
    assert_eq!(
        w.calls("systemctl"),
        "--user start --no-block tehas-dispatch.service\n"
    );
    assert_eq!(w.calls("fake-status").lines().count(), 1);
    // Its writes, in process on the fake: the park and its question, and no
    // row of the journal left open.
    let w = World::new("report-process-writes");
    w.ended("aborted", "develop", "", "");
    has(
        &w.report_said(),
        "note: Tehas cycle stopped without a pull request",
    );
    assert_eq!(
        w.row(&["journal-open"])["actions"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn a_report_goes_by_the_settings_the_cycle_started_with_whatever_instance_env_says_by_then() {
    for change in [
        r#"echo TEHAS_SITE_INDEXING=yes >>"$TEHAS_INSTANCE_DIR/instance.env""#,
        r#"echo "this is ( not shell" >>"$TEHAS_INSTANCE_DIR/instance.env""#,
        r#"echo TEHAS_REPO=other/place >>"$TEHAS_INSTANCE_DIR/instance.env""#,
        r#"echo TEHAS_CYCLE_REPORT=bash >>"$TEHAS_INSTANCE_DIR/instance.env""#,
        r#"rm "$TEHAS_INSTANCE_DIR/instance.env""#,
    ] {
        let (w, r, said) = twin("report-settings", |w| {
            w.ended("aborted", "develop", "", "");
            w.work(change);
        });
        assert_eq!(r.rc, 0, "{change}");
        has(&r.out, "done #7 rc=20 status=aborted step=develop");
        let gh = w.gh();
        has(&said, "note: Tehas cycle stopped without a pull request");
        // The calls name the repository the cycle started with, none another.
        has(&gh, "acme/widget");
        assert_eq!(gh.lines().filter(|l| l.contains("other/place")).count(), 0);
        has(&said, "unmark State(Running)");
        assert!(!w.row(&["cycle-show", "7"])["cycle"]["ended_at"].is_null());
    }
}

#[test]
fn a_store_gone_by_the_end_still_gets_the_issue_its_comment_and_its_label_off_70() {
    let (w, r, said) = twin("report-store-gone", |w| {
        w.ended("aborted", "develop", "", "");
        w.work(r#"rm "$TEHAS_HOME/libexec/store""#);
    });
    assert_eq!(r.rc, 70);
    assert!(!w.home.join("libexec/store").exists());
    has(&said, "note: Tehas cycle stopped without a pull request");
    has(&said, "unmark State(Running)");
}
