//! The control issue is never work: what of it the poll does not read. The
//! rules are in tests/fixtures/control.json.

mod common;

use common::{count_lines, has, World};

#[test]
fn a_plan_on_the_control_issue_does_not_read_its_text() {
    let w = World::new("control");
    w.fx("ready.json", "[]");
    w.fx(
        "plans.json",
        r#"[{"number":1,"labels":[{"name":"tehas:plan"}]}]"#,
    );
    w.rm_fx("issue-1.json");
    let r = w.poll();
    assert_eq!(
        count_lines(&w.gh(), "issue view 1 --repo acme/widget --json title,body"),
        0,
        "{}",
        w.gh()
    );
    has(
        &r.out,
        "skip #1: the control issue is never worked on (`tehas:plan`)",
    );
}
