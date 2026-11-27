//! The branch's check, through the code host port: the head commit of the
//! branch and the checks of that name on it, one read, as the adapter
//! folds them.
//! The fold is the adapter's alone; this reads what it adds up to.

use crate::snapshot::Check;
use tehas_ports::{Branch, CodeHost, Outcome, Overall};

/// Where the factory's setup page says what the code host's account needs to
/// read the checks: the page and the anchor of its heading, which a test
/// pins.
pub const PERMISSION_DOC: &str = "host/docs/github-app.md#the-permission-to-read-the-checks";

/// The line the poll says once for `branch` when the code host refuses to
/// say its checks (`Forbidden`), in tehas's words: the factory's account may
/// lack a permission, and the setup page says which.
pub fn refused(branch: &str) -> String {
    format!(
        "the code host refused to say the checks of {branch}; the factory's account may lack a permission ({PERMISSION_DOC})"
    )
}

/// The state of the check `name` on the head of `branch`. A read that
/// failed, was cut or could not be read is `Unreadable`, never a pass. A
/// read the code host refused (`Forbidden`) leaves [`refused`]'s line in
/// `said`, once for the branch; any other failure leaves none, and keeps
/// the line of its own the poll says for it.
pub fn read(
    host: Option<&mut dyn CodeHost>,
    branch: &str,
    name: &str,
    said: &mut Vec<String>,
) -> Check {
    let mut seen = |failed: Option<&Outcome>| {
        if matches!(failed, Some(Outcome::Forbidden)) && !said.contains(&refused(branch)) {
            said.push(refused(branch));
        }
    };
    let (Some(host), Ok(at)) = (host, Branch::new(branch)) else {
        return Check::Unreadable;
    };
    let read = host.branch_checks(&at, Some(name));
    seen(read.as_ref().err());
    let Ok(Some((head, checks))) = read else {
        return Check::Unreadable;
    };
    let Some(short) = head.as_str().get(..12) else {
        return Check::Unreadable;
    };
    let short = short.to_string();
    match checks.overall {
        Overall::Pass => Check::Done(short),
        Overall::Fail => Check::Red(short),
        Overall::Pending => Check::Pending(short),
        Overall::None => Check::None,
        Overall::Unknown => Check::Unknown(short),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::Snapshot;
    use serde_json::json;
    use tehas_ports::fake::{CodeHostOp, FakeSite, Fault, Faulty, Op, World};
    use tehas_ports::{Actor, Caller, Check as Run, CheckStatus, Conclusion, Scope, Site};

    fn done(c: Conclusion) -> CheckStatus {
        CheckStatus::Done(c)
    }

    /// A world whose branch `main` has one commit, with these runs of the
    /// check on it, each by the App `actions` in its own suite (workflow).
    fn world(runs: &[(&str, &str, CheckStatus)]) -> World {
        let world = World::new();
        let head = world.push("main", "work", "alice");
        runs.iter().fold(world, |w, (suite, name, status)| {
            w.check_in("actions", suite, &head, Run::new(name.to_string(), *status))
        })
    }

    fn read_of(site: &dyn Site) -> Check {
        let mut host = site
            .code_host(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap();
        read(Some(host.as_mut()), "main", "verify", &mut Vec::new())
    }

    fn head(world: &World) -> String {
        world.head("main").unwrap().as_str()[..12].to_string()
    }

    /// Two workflows' jobs of one name are both held: the failed one is not
    /// hidden behind the other's pass, and the snapshot reads it back red.
    #[test]
    fn two_workflows_jobs_of_one_name_are_both_held() {
        let w = world(&[
            ("nightly", "verify", done(Conclusion::Failed)),
            ("ci", "verify", done(Conclusion::Passed)),
        ]);
        let check = read_of(&FakeSite::new(&w));
        assert_eq!(check, Check::Red(head(&w)));
        let snap =
            Snapshot::parse(&json!({"checks": {"state": check.word(), "head": check.head()}}));
        assert_eq!(snap.checks, Some(Check::Red(head(&w))));
    }

    #[test]
    fn what_the_checks_add_up_to_is_the_state() {
        let cases = [
            (vec![("ci", "verify", done(Conclusion::Passed))], "done"),
            (vec![("ci", "verify", done(Conclusion::Skipped))], "done"),
            (vec![("ci", "verify", CheckStatus::Running)], "pending"),
            (
                vec![("ci", "verify", done(Conclusion::Cancelled))],
                "unknown",
            ),
            (vec![("ci", "other", done(Conclusion::Failed))], "none"),
            (vec![], "none"),
        ];
        for (runs, word) in cases {
            let w = world(&runs);
            let check = read_of(&FakeSite::new(&w));
            assert_eq!(check.word(), word, "{runs:?}");
            let snap =
                Snapshot::parse(&json!({"checks": {"state": check.word(), "head": check.head()}}));
            assert_eq!(snap.checks, Some(check));
        }
    }

    /// A read the adapter could not give whole, or could not read, reaches
    /// the snapshot as unreadable, never as a pass.
    #[test]
    fn a_cut_or_malformed_read_is_unreadable_never_green() {
        let w = world(&[("ci", "verify", done(Conclusion::Passed))]);
        for fault in [Fault::Cut, Fault::Malformed, Fault::Unreachable] {
            let site = Faulty::new(FakeSite::new(&w)).on(Op::CodeHost(CodeHostOp::Checks), fault);
            let check = read_of(&site);
            assert_eq!(check, Check::Unreadable, "{fault:?}");
            let snap =
                Snapshot::parse(&json!({"checks": {"state": check.word(), "head": check.head()}}));
            assert_eq!(snap.checks, Some(Check::Unreadable));
        }
    }

    #[test]
    fn no_handle_no_branch_or_a_failed_head_read_is_unreadable() {
        assert_eq!(
            read(None, "main", "verify", &mut Vec::new()),
            Check::Unreadable
        );
        let w = world(&[("ci", "verify", done(Conclusion::Passed))]);
        let site = FakeSite::new(&w);
        let mut host = site
            .code_host(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap();
        assert_eq!(
            read(Some(host.as_mut()), "gone", "verify", &mut Vec::new()),
            Check::Unreadable
        );
        let site =
            Faulty::new(FakeSite::new(&w)).on(Op::CodeHost(CodeHostOp::Branch), Fault::Unreachable);
        assert_eq!(read_of(&site), Check::Unreadable);
    }

    /// A read the code host refused, of the head or of the checks, is
    /// unreadable, never a pass, and leaves the one line for the branch,
    /// however often it is read in the poll. The setup page's anchor in the
    /// line is a heading of that page.
    #[test]
    fn a_refused_read_is_unreadable_and_said_once_for_the_branch() {
        let w = world(&[("ci", "verify", done(Conclusion::Passed))]);
        for op in [CodeHostOp::Checks, CodeHostOp::Branch] {
            let site = Faulty::new(FakeSite::new(&w)).on(Op::CodeHost(op), Fault::Forbidden);
            let mut host = site
                .code_host(&Actor::Ours(Scope::Full), Caller::new("test"))
                .unwrap();
            let mut said = Vec::new();
            for _ in 0..2 {
                let check = read(Some(host.as_mut()), "main", "verify", &mut said);
                assert_eq!(check, Check::Unreadable, "{op:?}");
            }
            assert_eq!(said, [refused("main")], "{op:?}");
        }
        assert_eq!(
            refused("main"),
            "the code host refused to say the checks of main; the factory's account may lack a permission (host/docs/github-app.md#the-permission-to-read-the-checks)"
        );
    }

    /// Only a refusal says the line: a read cut, malformed, unreachable or
    /// out of time is unreadable with no line of it, and keeps its own.
    #[test]
    fn no_other_failure_says_the_refusal() {
        let w = world(&[("ci", "verify", done(Conclusion::Passed))]);
        for fault in [
            Fault::Cut,
            Fault::Malformed,
            Fault::Unreachable,
            Fault::Timeout,
        ] {
            for op in [CodeHostOp::Checks, CodeHostOp::Branch] {
                let site = Faulty::new(FakeSite::new(&w)).on(Op::CodeHost(op), fault);
                let mut host = site
                    .code_host(&Actor::Ours(Scope::Full), Caller::new("test"))
                    .unwrap();
                let mut said = Vec::new();
                let check = read(Some(host.as_mut()), "main", "verify", &mut said);
                assert_eq!(check, Check::Unreadable, "{fault:?} {op:?}");
                assert!(said.is_empty(), "{fault:?} {op:?}: {said:?}");
            }
        }
    }

    /// The anchor the line names is a `## ` heading of the setup page, by
    /// the docs server's rule: lowercase, spaces to dashes, no other
    /// punctuation (test/manual_test.sh's t_slug).
    #[test]
    fn the_line_names_a_heading_of_the_setup_page() {
        let (page, anchor) = PERMISSION_DOC.split_once('#').unwrap();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(page);
        let text = std::fs::read_to_string(&path).unwrap();
        let slug = |h: &str| -> String {
            h.to_lowercase()
                .replace(' ', "-")
                .chars()
                .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
                .collect()
        };
        assert!(
            text.lines()
                .filter_map(|l| l.strip_prefix("## ").or_else(|| l.strip_prefix("### ")))
                .any(|h| slug(h) == anchor),
            "{page} has no heading #{anchor}"
        );
    }

    /// The snapshot reads only a state it knows, with a head of 12 hex
    /// digits where the state has one.
    #[test]
    fn the_snapshot_reads_no_state_it_does_not_know() {
        for v in [
            json!({"state": "red", "head": "<b>"}),
            json!({"state": "red"}),
            json!({"state": "green", "head": "0123456789ab"}),
            json!({"failed": true}),
            json!("not json"),
        ] {
            assert_eq!(
                Snapshot::parse(&json!({"checks": v})).checks,
                Some(Check::Unreadable),
                "{v}"
            );
        }
    }
}
