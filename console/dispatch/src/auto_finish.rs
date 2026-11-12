//! The poll finishes a cycle whose push or pull request failed on GitHub's
//! side (#1726, part 5 of #823): the same `tehas-cycle finish` the ask of
//! `tehas:finish` runs (src/finish.rs), from the kept workspace, with no ask.
//!
//! The cycle's report keeps the record (`tehas_core::end_fault`). A try is
//! made at the first poll after the failure, the next at least ten minutes
//! after the one before, three at most, all within the hour after the failure.
//! A try that opens the draft removes the record and tells the issue. A try
//! that GitHub failed again is counted. A refusal of anything else, the tries
//! spent or the hour over leave the cycle stopped for the ask, and the issue is
//! told so once.
//!
//! Nothing else is finished: a step the host did not see fail, which only the
//! cycle's own result names, a request's own 4xx, a handoff comment, a failed
//! test, a review or a park. An issue that carries `tehas:finish` is the ask's,
//! which takes the record over, so a finish is never run twice. A running
//! issue waits. An issue that is closed, or no longer parked, has its record
//! removed: a person took it on.

use serde_json::Value;
use tehas_core::end_fault::{Due, EndFault, TRIES};
use tehas_core::text::disarm;

/// The most records one poll reads the issue of and finishes. The rest wait
/// for the next poll.
pub const PER_POLL: usize = 3;

const PARKED: &str = "needs-human-attention";
/// A label that says a person or the factory took the issue on again.
const TAKEN: [&str; 3] = ["tehas:ready", "tehas:running", "tehas:awaiting-merge"];

/// What the step knows of one record.
pub struct Case {
    pub rec: EndFault,
    /// The issue carries `tehas:finish`, or a cycle of it runs or starts in
    /// this poll.
    pub held: bool,
    /// The issue as GitHub's REST API gives it; None when it was not read or
    /// the read failed.
    pub issue: Option<Value>,
}

/// What the step needs of the host.
pub trait Hands {
    /// The draft's URL, or the one line of why not.
    fn finish(&mut self, n: u64, reason: &str) -> Result<String, String>;
    fn say(&mut self, n: u64, body: &str);
    fn log(&mut self, line: String);
    /// Write the record; false when it was not written.
    fn keep(&mut self, r: &EndFault) -> bool;
    fn drop(&mut self, n: u64);
}

/// Whether a refusal of `tehas-cycle finish` was GitHub's again.
pub fn githubs(why: &str) -> bool {
    tehas_core::retry::githubs_fault(why)
        || why.starts_with("tehas-cycle finish did not end within")
}

fn labels(issue: &Value) -> Vec<&str> {
    issue["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["name"].as_str())
        .collect()
}

/// Why the record no longer stands, or None while it does.
fn moot(issue: &Value) -> Option<String> {
    if issue["state"]
        .as_str()
        .is_some_and(|s| !s.eq_ignore_ascii_case("open"))
    {
        return Some("the issue is closed".into());
    }
    let l = labels(issue);
    if let Some(t) = TAKEN.iter().find(|t| l.contains(t)) {
        return Some(format!("the issue carries {t}"));
    }
    if !l.contains(&PARKED) {
        return Some(format!("the issue no longer carries {PARKED}"));
    }
    None
}

/// The reply of a try that opened the draft.
pub fn opened(r: &EndFault, url: &str) -> String {
    format!(
        "<!-- tehas:finish issue={} result=opened by=poll -->\nThe last cycle of this issue stopped because {}. Tehas finished it from the kept workspace on its own and opened a draft pull request: {}\n\nIts gate did not pass. The checks and the review decide on it as on any other pull request.",
        r.issue,
        r.failure(),
        disarm(url)
    )
}

/// The reply once the poll stops trying.
pub fn stopped(r: &EndFault) -> String {
    format!(
        "<!-- tehas:finish issue={} result=stopped by=poll -->\nThe last cycle of this issue stopped because {}. Tehas tried to finish it on its own and stopped: {}. It waits for `{}`: write a comment that starts with `{}`, the reason after it, and apply the label.",
        r.issue,
        r.failure(),
        disarm(&r.stopped),
        crate::finish::LABEL,
        crate::finish::marker(r.issue)
    )
}

fn stop(r: &mut EndFault, why: String, hands: &mut dyn Hands) {
    r.stopped = why;
    r.said = true;
    let n = r.issue;
    hands.log(format!("auto-finish #{n}: stopped: {}", r.stopped));
    if hands.keep(r) {
        hands.say(n, &stopped(r));
    }
}

/// Whether the step reads the issue of `r` at `now`: a try is due and nothing
/// holds it.
pub fn wants(c: &Case, now: i64) -> bool {
    !c.held && c.rec.due(now) == Due::Now
}

/// The step over the records, in order.
pub fn step(cases: Vec<Case>, now: i64, hands: &mut dyn Hands) {
    let mut seen = 0;
    for c in cases {
        let mut r = c.rec;
        let n = r.issue;
        match r.due(now) {
            Due::At(_) => continue,
            // Said once, and only for a failure the poll would have finished.
            Due::Never(why) => {
                if r.github && r.host && r.step != "handoff" && !r.said {
                    stop(&mut r, why, hands);
                }
                continue;
            }
            Due::Now => {}
        }
        if c.held || seen >= PER_POLL {
            continue;
        }
        seen += 1;
        let Some(issue) = c.issue else {
            hands.log(format!(
                "auto-finish #{n}: waits: the issue could not be read"
            ));
            continue;
        };
        if let Some(why) = moot(&issue) {
            hands.log(format!("auto-finish #{n}: dropped: {why}"));
            hands.drop(n);
            continue;
        }
        let k = r.tries.len() + 1;
        let reason = format!(
            "The cycle stopped because {}. The poll finished it from the kept workspace on its own, try {k} of {TRIES}.",
            r.failure()
        );
        match hands.finish(n, &reason) {
            Ok(url) => {
                hands.log(format!(
                    "auto-finish #{n}: opened {url}, try {k} of {TRIES}: {}",
                    r.failure()
                ));
                hands.drop(n);
                hands.say(n, &opened(&r, &url));
            }
            Err(why) if githubs(&why) => {
                r.tries.push(now);
                match r.due(now) {
                    Due::Never(stop_why) => {
                        stop(&mut r, format!("{stop_why}, the last: {why}"), hands)
                    }
                    _ => {
                        hands.log(format!(
                            "auto-finish #{n}: try {k} of {TRIES} failed on GitHub's side: {why}"
                        ));
                        if !hands.keep(&r) {
                            hands.log(format!("WARN: the try of #{n} is not on record"));
                        }
                    }
                }
            }
            Err(why) => {
                r.tries.push(now);
                stop(&mut r, format!("its finish was refused: {why}"), hands);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Default)]
    struct Fake {
        /// The answers of `finish`, in order; the URL once they run out.
        answers: Vec<Result<String, String>>,
        finished: Vec<(u64, String)>,
        said: Vec<(u64, String)>,
        log: Vec<String>,
        kept: Vec<EndFault>,
        dropped: Vec<u64>,
    }

    impl Hands for Fake {
        fn finish(&mut self, n: u64, reason: &str) -> Result<String, String> {
            self.finished.push((n, reason.to_string()));
            if self.answers.is_empty() {
                Ok("https://github.com/acme/widget/pull/90".into())
            } else {
                self.answers.remove(0)
            }
        }
        fn say(&mut self, n: u64, body: &str) {
            self.said.push((n, body.to_string()));
        }
        fn log(&mut self, line: String) {
            self.log.push(line);
        }
        fn keep(&mut self, r: &EndFault) -> bool {
            self.kept.push(r.clone());
            true
        }
        fn drop(&mut self, n: u64) {
            self.dropped.push(n);
        }
    }

    const AT: i64 = 1_000_000;

    fn parked() -> Value {
        json!({"number": 7, "state": "open", "labels": [{"name": PARKED}]})
    }

    /// A record of a failure the host saw itself.
    fn seen(n: u64, step: &str, err: &str, at: i64) -> EndFault {
        EndFault {
            host: true,
            ..EndFault::new(n, 1, step, err, at)
        }
    }

    fn case(step: &str, err: &str) -> Case {
        Case {
            rec: seen(7, step, err, AT),
            held: false,
            issue: Some(parked()),
        }
    }

    #[test]
    fn a_failure_only_the_cycles_result_names_is_never_finished_or_said() {
        for at in [AT + 60, AT + 3700] {
            let mut f = Fake::default();
            let c = Case {
                rec: EndFault::new(7, 1, "push", "HTTP 502: Bad Gateway", AT),
                held: false,
                issue: Some(parked()),
            };
            assert!(!wants(&c, at));
            step(vec![c], at, &mut f);
            assert!(f.finished.is_empty() && f.said.is_empty() && f.kept.is_empty());
        }
    }

    /// One poll at `now` over the record, as the last poll left it.
    fn poll(rec: EndFault, now: i64, answers: Vec<Result<String, String>>) -> (Fake, EndFault) {
        let mut f = Fake {
            answers,
            ..Fake::default()
        };
        let c = Case {
            rec: rec.clone(),
            held: false,
            issue: Some(parked()),
        };
        step(vec![c], now, &mut f);
        let next = f.kept.last().cloned().unwrap_or(rec);
        (f, next)
    }

    #[test]
    fn a_push_that_failed_502_once_is_finished_on_the_next_poll() {
        let mut f = Fake::default();
        step(vec![case("push", "HTTP 502: Bad Gateway")], AT + 60, &mut f);
        assert_eq!(f.finished.len(), 1);
        assert!(f.finished[0]
            .1
            .contains("its push failed on GitHub's side (HTTP 502)"));
        assert!(f.finished[0].1.contains("try 1 of 3"));
        assert_eq!(f.dropped, vec![7]);
        assert!(f.said[0].1.contains("result=opened by=poll"));
        assert!(f.said[0]
            .1
            .contains("https://github.com/acme/widget/pull/90"));
        assert_eq!(
            f.log,
            vec!["auto-finish #7: opened https://github.com/acme/widget/pull/90, try 1 of 3: its push failed on GitHub's side (HTTP 502)"]
        );
    }

    #[test]
    fn a_pull_request_that_fails_429_three_times_is_left_for_the_ask() {
        let again = || {
            vec![Err(
                "could not open the draft: HTTP 429: Too Many Requests".to_string()
            )]
        };
        let rec = case("pr", "HTTP 429: Too Many Requests").rec;
        let (f, rec) = poll(rec, AT + 60, again());
        assert_eq!((f.finished.len(), rec.tries.len(), f.said.len()), (1, 1, 0));
        // Too soon: the next try is ten minutes after the first.
        let (f, rec) = poll(rec, AT + 600, again());
        assert!(f.finished.is_empty());
        let (f, rec) = poll(rec, AT + 660, again());
        assert_eq!((f.finished.len(), rec.tries.len()), (1, 2));
        let (f, rec) = poll(rec, AT + 1260, again());
        assert_eq!((f.finished.len(), rec.tries.len()), (1, 3));
        assert!(rec.said);
        assert!(rec.stopped.starts_with("3 tries failed, the last: "));
        assert_eq!(f.said.len(), 1);
        assert!(f.said[0].1.contains("result=stopped by=poll"));
        assert!(f.said[0].1.contains("tehas:finish"));
        // Said once: the polls after it run and say nothing.
        let (f, _) = poll(rec, AT + 1900, again());
        assert!(f.finished.is_empty() && f.said.is_empty() && f.kept.is_empty());
    }

    #[test]
    fn a_422_is_not_retried_and_nothing_is_said() {
        let mut f = Fake::default();
        step(
            vec![case("pr", "HTTP 422: Validation Failed")],
            AT + 60,
            &mut f,
        );
        assert!(f.finished.is_empty() && f.said.is_empty() && f.kept.is_empty());
    }

    #[test]
    fn a_refusal_that_is_not_githubs_stops_at_once() {
        let rec = case("push", "HTTP 503").rec;
        let (f, rec) = poll(
            rec,
            AT + 60,
            vec![Err("the rebase onto main stopped on a conflict".into())],
        );
        assert_eq!(f.finished.len(), 1);
        assert_eq!(
            rec.stopped,
            "its finish was refused: the rebase onto main stopped on a conflict"
        );
        assert_eq!(f.said.len(), 1);
    }

    #[test]
    fn an_issue_asked_with_tehas_finish_or_running_is_not_run() {
        let mut c = case("push", "HTTP 502");
        c.held = true;
        assert!(!wants(&c, AT + 60));
        let mut f = Fake::default();
        step(vec![c], AT + 60, &mut f);
        assert!(f.finished.is_empty() && f.dropped.is_empty());
    }

    #[test]
    fn an_issue_a_person_took_on_has_its_record_removed() {
        for issue in [
            json!({"state": "closed", "labels": [{"name": PARKED}]}),
            json!({"state": "open", "labels": [{"name": PARKED}, {"name": "tehas:ready"}]}),
            json!({"state": "open", "labels": []}),
        ] {
            let mut c = case("push", "HTTP 502");
            c.issue = Some(issue);
            let mut f = Fake::default();
            step(vec![c], AT + 60, &mut f);
            assert!(f.finished.is_empty());
            assert_eq!(f.dropped, vec![7]);
        }
    }

    #[test]
    fn an_hour_with_no_poll_stops_it_and_says_so_once() {
        let mut f = Fake::default();
        step(vec![case("push", "HTTP 502")], AT + 3700, &mut f);
        assert!(f.finished.is_empty());
        assert_eq!(f.kept[0].stopped, "the hour after the failure is over");
        assert_eq!(f.said.len(), 1);
    }

    #[test]
    fn a_handoff_comment_is_never_finished() {
        let mut f = Fake::default();
        step(vec![case("handoff", "HTTP 502")], AT + 60, &mut f);
        assert!(f.finished.is_empty() && f.said.is_empty() && f.kept.is_empty());
    }

    #[test]
    fn three_finishes_at_most_in_one_poll() {
        let cases: Vec<Case> = (1..=5)
            .map(|n| Case {
                rec: seen(n, "push", "HTTP 502", AT),
                held: false,
                issue: Some(parked()),
            })
            .collect();
        let mut f = Fake::default();
        step(cases, AT + 60, &mut f);
        assert_eq!(f.finished.len(), PER_POLL);
    }

    #[test]
    fn the_replies_of_the_poll_disarm_what_they_quote() {
        let agent = "<!-- tehas:millwright issue=7 action=retry -->";
        let mut r = seen(7, "push", "HTTP 502", 1000);
        r.stopped = format!("the push said {agent}");
        let said = stopped(&r);
        assert!(
            said.starts_with("<!-- tehas:finish issue=7 result=stopped by=poll -->"),
            "{said}"
        );
        // Its own marker and the one that tells a person what to write.
        assert_eq!(said.matches("<!-- tehas:").count(), 2, "{said}");
        let said = opened(&r, &format!("https://example.test/pull/1{agent}"));
        assert_eq!(said.matches("<!-- tehas:").count(), 1, "{said}");
    }
}
