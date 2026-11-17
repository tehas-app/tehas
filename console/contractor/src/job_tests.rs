//! A job's life against a world of the test's making: a clock that moves only
//! when the job pauses, a cycle that ends when the case says, and a factory
//! that answers what the case scripts.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use serde_json::{json, Value};

use crate::http::Reply;
use crate::job::{run, run_job, End, Job, Owner, Work, World};

/// The cycle of a case: when it ends on its own, and how long it takes to end
/// after its stop signal (None: it never does, and is killed).
#[derive(Clone)]
struct Cycle {
    ends_at: Option<u64>,
    stops_in: Option<u64>,
    writes_result: bool,
}

struct Fake {
    now: Cell<u64>,
    replies: RefCell<HashMap<String, VecDeque<Reply>>>,
    /// The reply of a call once its script is empty.
    otherwise: RefCell<HashMap<String, Reply>>,
    events: Rc<RefCell<Vec<String>>>,
    cycle: Cycle,
    stopped_at: Rc<Cell<Option<u64>>>,
    result_written: Rc<Cell<bool>>,
    drain_at: Option<u64>,
    jobs: RefCell<VecDeque<Value>>,
}

impl Fake {
    fn new(cycle: Cycle) -> Fake {
        Fake {
            now: Cell::new(1000),
            replies: RefCell::new(HashMap::new()),
            otherwise: RefCell::new(HashMap::new()),
            events: Rc::new(RefCell::new(Vec::new())),
            cycle,
            stopped_at: Rc::new(Cell::new(None)),
            result_written: Rc::new(Cell::new(false)),
            drain_at: None,
            jobs: RefCell::new(VecDeque::new()),
        }
    }

    fn script(&self, call: &str, replies: Vec<Reply>) {
        self.replies
            .borrow_mut()
            .insert(call.to_string(), replies.into());
    }

    fn always(&self, call: &str, reply: Reply) {
        self.otherwise.borrow_mut().insert(call.to_string(), reply);
    }

    fn events(&self) -> Vec<String> {
        self.events.borrow().clone()
    }

    fn calls(&self, call: &str) -> usize {
        self.events().iter().filter(|e| *e == call).count()
    }
}

struct FakeWork {
    now: Rc<Cell<u64>>,
    cycle: Cycle,
    stopped_at: Rc<Cell<Option<u64>>>,
    killed: bool,
    events: Rc<RefCell<Vec<String>>>,
    result_written: Rc<Cell<bool>>,
}

impl Work for FakeWork {
    fn done(&mut self) -> Option<i32> {
        if self.killed {
            return Some(137);
        }
        let now = self.now.get();
        if self.cycle.ends_at.is_some_and(|t| now >= t) {
            self.result_written.set(self.cycle.writes_result);
            return Some(0);
        }
        match (self.stopped_at.get(), self.cycle.stops_in) {
            (Some(at), Some(d)) if now >= at + d => {
                self.result_written.set(self.cycle.writes_result);
                Some(21)
            }
            _ => None,
        }
    }
    fn stop(&mut self) {
        self.events.borrow_mut().push("TERM".to_string());
        self.stopped_at.set(Some(self.now.get()));
    }
    fn kill(&mut self) {
        self.events.borrow_mut().push("KILL".to_string());
        self.killed = true;
    }
}

/// The clock the fake world and its cycle share.
struct Shared(Rc<Cell<u64>>);

impl World for (Fake, Shared) {
    fn ask(&self, call: &str) -> Reply {
        self.0.events.borrow_mut().push(call.to_string());
        if call == "job" {
            return match self.0.jobs.borrow_mut().pop_front() {
                Some(j) => Reply::Ok(j),
                None => Reply::Ok(json!({ "job": null })),
            };
        }
        reply(self, call)
    }
    fn call(&self, call: &str, token: &str, _body: &Value) -> Reply {
        assert_eq!(token, "job-token");
        self.0.events.borrow_mut().push(call.to_string());
        reply(self, call)
    }
    fn start(&self, _job: &Job) -> Result<Box<dyn Work>, String> {
        self.0.events.borrow_mut().push("start".to_string());
        Ok(Box::new(FakeWork {
            now: Rc::clone(&self.1 .0),
            cycle: self.0.cycle.clone(),
            stopped_at: Rc::clone(&self.0.stopped_at),
            killed: false,
            events: Rc::clone(&self.0.events),
            result_written: Rc::clone(&self.0.result_written),
        }))
    }
    fn result(&self, _job: &Job) -> Option<(Value, i64)> {
        self.0
            .result_written
            .get()
            .then(|| (json!({ "status": "merged" }), 0))
    }
    fn salvage(&self, _job: &Job) -> bool {
        self.0.events.borrow_mut().push("push".to_string());
        true
    }
    fn wipe(&self) {
        self.0.events.borrow_mut().push("wipe".to_string());
    }
    fn draining(&self) -> bool {
        self.0.drain_at.is_some_and(|t| self.1 .0.get() >= t)
    }
    fn now(&self) -> u64 {
        self.1 .0.get()
    }
    fn pause(&self, seconds: u64) {
        self.1 .0.set(self.1 .0.get() + seconds.max(1));
    }
    fn say(&self, _line: &str) {}
}

fn reply(w: &(Fake, Shared), call: &str) -> Reply {
    if let Some(r) =
        w.0.replies
            .borrow_mut()
            .get_mut(call)
            .and_then(VecDeque::pop_front)
    {
        return r;
    }
    w.0.otherwise
        .borrow()
        .get(call)
        .cloned()
        .unwrap_or_else(|| Reply::Ok(json!({})))
}

fn world(cycle: Cycle) -> (Fake, Shared) {
    let fake = Fake::new(cycle);
    let clock = Rc::new(Cell::new(fake.now.get()));
    (fake, Shared(clock))
}

fn job() -> Job {
    Job::of(&json!({
        "job": 3, "issue": 7, "attempt": 1, "timeout": 60, "token": "job-token", "model_token": "model-token",
        "settings": { "TEHAS_REPO": "acme/widget" }, "models": {}
    }))
    .unwrap()
}

fn down() -> Reply {
    Reply::Down("connection refused".to_string())
}

fn ends_at(t: u64) -> Cycle {
    Cycle {
        ends_at: Some(1000 + t),
        stops_in: Some(10),
        writes_result: true,
    }
}

fn never_ends(stops_in: Option<u64>, writes_result: bool) -> Cycle {
    Cycle {
        ends_at: None,
        stops_in,
        writes_result,
    }
}

#[test]
fn a_job_is_read_from_its_answer_and_no_job_is_none() {
    let j = job();
    assert_eq!((j.id, j.issue, j.attempt, j.timeout), (3, 7, 1, 60));
    assert_eq!(j.every(), 15);
    assert_eq!(Job::of(&json!({ "job": null })), None);
    assert_eq!(
        Job::of(&json!({ "job": 1, "issue": 2, "attempt": 1, "timeout": 60, "token": "" })),
        None
    );
    // A job needs a model token of its own, apart from the job token.
    for model in [json!(null), json!(""), json!("job-token")] {
        let v = json!({ "job": 1, "issue": 2, "attempt": 1, "timeout": 60,
                        "token": "job-token", "model_token": model });
        assert_eq!(Job::of(&v), None, "{v}");
    }
    assert_eq!(j.token, "job-token");
    assert_eq!(j.model_token, "model-token");
    assert_eq!(j.owner, None);
}

#[test]
fn a_job_names_the_factory_s_account_with_both_numbers_or_none() {
    let with = |extra: Value| {
        let mut v = json!({ "job": 1, "issue": 2, "attempt": 1, "timeout": 60,
                            "token": "job-token", "model_token": "model-token" });
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        Job::of(&v).unwrap().owner
    };
    assert_eq!(
        with(json!({ "uid": 1001, "gid": 1002 })),
        Some(Owner {
            uid: 1001,
            gid: 1002
        })
    );
    assert_eq!(
        with(json!({ "uid": 0, "gid": 0 })),
        Some(Owner { uid: 0, gid: 0 })
    );
    for extra in [
        json!({ "uid": 1001 }),
        json!({ "gid": 1002 }),
        json!({ "uid": -1, "gid": 1002 }),
        json!({ "uid": 4_294_967_296_u64, "gid": 1002 }),
        json!({ "uid": "1001", "gid": "1002" }),
    ] {
        assert_eq!(with(extra.clone()), None, "{extra}");
    }
}

#[test]
fn a_cycle_that_ends_reports_its_result_and_the_workspace_is_wiped() {
    let w = world(ends_at(100));
    assert_eq!(run_job(&w, &job(), 30), End::Finished);
    let ev = w.0.events();
    assert_eq!(ev.first().map(String::as_str), Some("start"));
    assert!(w.0.calls("check-in") >= 5, "{ev:?}");
    assert_eq!(w.0.calls("result"), 1);
    assert_eq!(ev.last().map(String::as_str), Some("wipe"));
    assert!(!ev.contains(&"TERM".to_string()) && !ev.contains(&"push".to_string()));
}

#[test]
fn a_quiet_cycle_is_kept_alive_by_the_contractor_s_own_timer() {
    // Nothing printed for an hour: the check-ins keep coming, the job stays.
    let w = world(ends_at(3600));
    assert_eq!(run_job(&w, &job(), 30), End::Finished);
    assert!(w.0.calls("check-in") >= 3600 / 15 - 1);
}

#[test]
fn a_network_blip_shorter_than_the_timeout_costs_nothing() {
    let w = world(ends_at(200));
    w.0.script("check-in", vec![down(), down(), down()]);
    assert_eq!(run_job(&w, &job(), 30), End::Finished);
    assert!(!w.0.events().contains(&"TERM".to_string()));
}

#[test]
fn a_factory_gone_for_the_timeout_stops_the_cycle_salvages_and_reports() {
    let w = world(never_ends(Some(10), false));
    w.0.always("check-in", down());
    assert_eq!(run_job(&w, &job(), 30), End::Offline);
    let ev = w.0.events();
    let at = |e: &str| {
        ev.iter()
            .position(|x| x == e)
            .unwrap_or_else(|| panic!("{e}: {ev:?}"))
    };
    assert!(at("TERM") < at("push"));
    assert!(at("push") < at("stopped"));
    assert!(at("stopped") < ev.len() - 1);
    assert!(!ev.contains(&"KILL".to_string()));
    assert_eq!(ev.last().map(String::as_str), Some("wipe"));
}

#[test]
fn the_network_dropping_between_salvage_and_report_retries_the_report() {
    let w = world(never_ends(Some(10), false));
    w.0.always("check-in", down());
    w.0.script("stopped", vec![down(), down(), Reply::Ok(json!({}))]);
    assert_eq!(run_job(&w, &job(), 30), End::Offline);
    assert_eq!(w.0.calls("stopped"), 3);
    assert_eq!(w.0.calls("push"), 1);
}

#[test]
fn a_report_that_never_gets_through_is_given_up_after_the_timeout() {
    let w = world(never_ends(Some(10), false));
    w.0.always("check-in", down());
    w.0.always("stopped", down());
    assert_eq!(run_job(&w, &job(), 30), End::Offline);
    let tries = w.0.calls("stopped");
    assert!((2..=13).contains(&tries), "{tries}");
    assert_eq!(w.0.events().last().map(String::as_str), Some("wipe"));
}

#[test]
fn a_revoked_job_is_stopped_and_pushes_and_reports_nothing() {
    let w = world(never_ends(Some(10), true));
    w.0.script("check-in", vec![Reply::No(403, "revoked".to_string())]);
    assert_eq!(run_job(&w, &job(), 30), End::Revoked);
    let ev = w.0.events();
    assert!(ev.contains(&"TERM".to_string()));
    assert!(!ev.contains(&"push".to_string()), "{ev:?}");
    assert_eq!(w.0.calls("stopped") + w.0.calls("result"), 0);
    assert_eq!(ev.last().map(String::as_str), Some("wipe"));
}

#[test]
fn a_lost_job_takes_the_salvage_path_and_a_late_result_is_sent() {
    let w = world(never_ends(Some(10), true));
    w.0.script("check-in", vec![Reply::No(403, "lost".to_string())]);
    assert_eq!(run_job(&w, &job(), 30), End::Lost("lost".to_string()));
    assert_eq!(w.0.calls("push"), 1);
    // The cycle wrote its result while it stopped: that goes, not "stopped".
    assert_eq!((w.0.calls("result"), w.0.calls("stopped")), (1, 0));
}

#[test]
fn a_job_a_newer_attempt_replaced_is_stopped_and_pushes_and_reports_nothing() {
    let w = world(never_ends(Some(10), true));
    w.0.script("check-in", vec![Reply::No(409, "stale".to_string())]);
    assert_eq!(run_job(&w, &job(), 30), End::Lost("stale".to_string()));
    let ev = w.0.events();
    assert!(ev.contains(&"TERM".to_string()));
    assert!(!ev.contains(&"push".to_string()), "{ev:?}");
    assert_eq!(w.0.calls("stopped") + w.0.calls("result"), 0);
    assert_eq!(ev.last().map(String::as_str), Some("wipe"));
}

#[test]
fn a_cycle_that_outlives_its_grace_is_killed_and_never_before() {
    let w = world(never_ends(None, false));
    w.0.script("check-in", vec![Reply::No(403, "lost".to_string())]);
    let began = w.1 .0.get();
    run_job(&w, &job(), 30);
    let ev = w.0.events();
    assert!(ev.contains(&"KILL".to_string()));
    let stopped = w.0.stopped_at.get().unwrap();
    assert!(stopped >= began);
    assert!(w.1 .0.get() >= stopped + 30);
}

#[test]
fn drain_lets_the_cycle_end_within_the_grace_otherwise_hands_it_back() {
    // Ends within the grace: a normal finish, nothing pushed.
    let mut w = world(ends_at(40));
    w.0.drain_at = Some(1020);
    assert_eq!(run_job(&w, &job(), 60), End::Finished);
    assert_eq!(w.0.calls("push"), 0);
    // Does not: stopped, pushed, handed back with "stopped".
    w = world(never_ends(Some(5), false));
    w.0.drain_at = Some(1020);
    assert_eq!(run_job(&w, &job(), 60), End::Drained);
    let ev = w.0.events();
    assert_eq!((w.0.calls("push"), w.0.calls("stopped")), (1, 1), "{ev:?}");
}

#[test]
fn drain_versus_crash_the_crash_reports_the_cycle_s_own_end() {
    // A cycle that dies on its own with no result is reported as stopped and
    // never pushed by the contractor: its branch is whatever it pushed.
    let w = world(Cycle {
        ends_at: Some(1010),
        stops_in: None,
        writes_result: false,
    });
    assert_eq!(run_job(&w, &job(), 30), End::Finished);
    assert_eq!((w.0.calls("stopped"), w.0.calls("push")), (1, 0));
}

#[test]
fn the_loop_wipes_first_runs_a_job_and_ends_when_drained() {
    let mut w = world(ends_at(30));
    w.0.jobs.borrow_mut().push_back(json!({
        "job": 3, "issue": 7, "attempt": 1, "timeout": 60, "token": "job-token", "model_token": "model-token"
    }));
    w.0.drain_at = Some(1200);
    assert_eq!(run(&w, 30, 60), 0);
    let ev = w.0.events();
    assert_eq!(ev.first().map(String::as_str), Some("wipe"));
    assert_eq!(w.0.calls("start"), 1);
    assert_eq!(w.0.calls("result"), 1);
    // A revoked contractor stops asking at once.
    w = world(ends_at(30));
    w.0.script("hello", vec![Reply::No(403, "revoked".to_string())]);
    assert_eq!(run(&w, 30, 60), 1);
    assert_eq!(w.0.calls("job"), 0);
}
