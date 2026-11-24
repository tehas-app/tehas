//! The branch's check, through the code host port: the head commit of the
//! branch, then the checks of that name on it, as the adapter folds them,
//! the newest of each App, suite and name (Decided 129). The fold is the
//! adapter's alone; this reads what it adds up to.

use crate::snapshot::Check;
use tehas_ports::{Branch, CodeHost, Overall};

/// The state of the check `name` on the head of `branch`. A read that
/// failed, was cut or could not be read is `Unreadable`, never a pass.
pub fn read(host: Option<&mut dyn CodeHost>, branch: &str, name: &str) -> Check {
    let (Some(host), Ok(branch)) = (host, Branch::new(branch)) else {
        return Check::Unreadable;
    };
    let Ok(Some(head)) = host.branch(&branch) else {
        return Check::Unreadable;
    };
    let (Ok(checks), Some(short)) = (host.checks(&head, Some(name)), head.as_str().get(..12))
    else {
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
        read(Some(host.as_mut()), "main", "verify")
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
        assert_eq!(read(None, "main", "verify"), Check::Unreadable);
        let w = world(&[("ci", "verify", done(Conclusion::Passed))]);
        let site = FakeSite::new(&w);
        let mut host = site
            .code_host(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap();
        assert_eq!(
            read(Some(host.as_mut()), "gone", "verify"),
            Check::Unreadable
        );
        let site =
            Faulty::new(FakeSite::new(&w)).on(Op::CodeHost(CodeHostOp::Branch), Fault::Unreachable);
        assert_eq!(read_of(&site), Check::Unreadable);
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
