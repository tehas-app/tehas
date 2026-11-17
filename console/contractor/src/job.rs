//! One job from its offer to its end, and the loop that asks for jobs: the
//! spec's "How it works", "Durability" and "Dying gracefully".
//!
//! A running job checks in on its own timer, whatever the cycle prints, so a
//! long quiet model turn does not lose it. It ends one of six ways (stale is
//! an end of `Lost`):
//!
//! | End | When | Stop the cycle | Push its branch | Report |
//! | --- | --- | --- | --- | --- |
//! | finished | the cycle ended on its own | no | no, the cycle pushed | the result |
//! | offline | no check-in answered for the timeout | yes, with the grace | yes | stopped, or the result |
//! | lost | the factory refused a check-in for the job's state | yes | yes | stopped, or the result |
//! | stale | a newer attempt of the issue replaced this one (`stale`) | yes | no | nothing |
//! | revoked | the factory said `revoked` | yes | no | nothing |
//! | drained | `tehas-contractor drain`, and the grace passed | yes | yes | stopped: a release |
//!
//! The cycle is never killed outright: it is sent its stop signal first and
//! given the grace to finish its step and write its notes, and killed only
//! after that. Whatever the end, the workspace and the job's tokens are wiped.

use serde_json::{json, Map, Value};

use crate::http::Reply;

/// The factory's word for an attempt a newer one of the same issue replaced.
pub const STALE: &str = "stale";

/// What the factory gave: one cycle for a limited time.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: i64,
    pub issue: i64,
    pub attempt: i64,
    /// Seconds without an answered check-in before the job is given up.
    pub timeout: u64,
    /// The job's token of the job scope, for the job channel and nothing
    /// else. This program holds it; the container never sees it. Dies with
    /// the job.
    pub token: String,
    /// The job's token of the model scope, for the container's models.json:
    /// the model paths and nothing else. Dies with the job.
    pub model_token: String,
    /// The factory's settings that leave with the job.
    pub settings: Map<String, Value>,
    /// The models.json of the cycle, every provider behind the factory's proxy.
    pub models: Value,
}

impl Job {
    /// The job of a `job` answer, None for an answer of no job.
    pub fn of(v: &Value) -> Option<Job> {
        Some(Job {
            id: v["job"].as_i64()?,
            issue: v["issue"].as_i64()?,
            attempt: v["attempt"].as_i64()?,
            timeout: v["timeout"].as_u64().filter(|t| *t > 0)?,
            token: v["token"].as_str().filter(|t| !t.is_empty())?.to_string(),
            model_token: v["model_token"]
                .as_str()
                .filter(|t| !t.is_empty() && Some(*t) != v["token"].as_str())?
                .to_string(),
            settings: v["settings"].as_object().cloned().unwrap_or_default(),
            models: v["models"].clone(),
        })
    }

    /// The seconds between two check-ins: a quarter of the timeout, so three
    /// may be lost before the job is, between 5 and 60.
    pub fn every(&self) -> u64 {
        (self.timeout / 4).clamp(5, 60)
    }
}

/// The cycle a job runs.
pub trait Work {
    /// Its exit status once it has ended.
    fn done(&mut self) -> Option<i32>;
    /// The stop signal: the cycle finishes its step and writes its notes.
    fn stop(&mut self);
    fn kill(&mut self);
}

/// What a contractor needs of the world. The host's is [`crate::host`]'s; a
/// test gives its own.
pub trait World {
    /// A call made with the contractor's key: `hello` or `job`.
    fn ask(&self, call: &str) -> Reply;
    /// A call made with a job's token.
    fn call(&self, call: &str, token: &str, body: &Value) -> Reply;
    /// Prepare the job's workspace and start its cycle.
    fn start(&self, job: &Job) -> Result<Box<dyn Work>, String>;
    /// The result the cycle wrote and its exit status, if it wrote one.
    fn result(&self, job: &Job) -> Option<(Value, i64)>;
    /// Push the cycle's branch, so a replacement can continue it.
    fn salvage(&self, job: &Job) -> bool;
    /// Remove the workspace, the job's files and its tokens.
    fn wipe(&self);
    /// Whether `tehas-contractor drain` asked this contractor to stop.
    fn draining(&self) -> bool;
    fn now(&self) -> u64;
    fn pause(&self, seconds: u64);
    fn say(&self, line: &str);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    Finished,
    Offline,
    Lost(String),
    Revoked,
    Drained,
    NotStarted(String),
}

/// How long a report of a job's end is retried while the factory does not
/// answer: the job's own timeout, after which the factory has given it up.
fn report(world: &dyn World, job: &Job, call: &str, body: &Value) -> Reply {
    let until = world.now() + job.timeout;
    loop {
        let r = world.call(call, &job.token, body);
        match &r {
            Reply::Down(why) if world.now() < until => {
                world.say(&format!(
                    "job {}: the {call} report got no answer ({why}), again in 5s",
                    job.id
                ));
                world.pause(5);
            }
            _ => return r,
        }
    }
}

/// Run one job to its end. `grace` is the seconds the cycle has after its
/// stop signal.
pub fn run_job(world: &dyn World, job: &Job, grace: u64) -> End {
    world.say(&format!(
        "job {} of issue #{}, attempt {}",
        job.id, job.issue, job.attempt
    ));
    let mut work = match world.start(job) {
        Ok(w) => w,
        Err(why) => {
            world.say(&format!("job {}: the cycle did not start: {why}", job.id));
            let _ = report(world, job, "stopped", &json!({ "why": why }));
            world.wipe();
            return End::NotStarted(why);
        }
    };
    let mut last_ok = world.now();
    let mut next = last_ok + job.every();
    let mut drain_until: Option<u64> = None;
    loop {
        if let Some(code) = work.done() {
            return finished(world, job, code);
        }
        let now = world.now();
        if drain_until.is_none() && world.draining() {
            world.say(&format!(
                "job {}: draining, the cycle has {grace}s to end",
                job.id
            ));
            drain_until = Some(now + grace);
        }
        if drain_until.is_some_and(|until| now >= until) {
            return die(world, job, work.as_mut(), grace, End::Drained);
        }
        if now >= next {
            next = now + job.every();
            match world.call("check-in", &job.token, &json!({})) {
                Reply::Ok(_) => last_ok = now,
                r if r.revoked() => return die(world, job, work.as_mut(), grace, End::Revoked),
                Reply::No(_, word) => {
                    return die(world, job, work.as_mut(), grace, End::Lost(word));
                }
                Reply::Down(why) => {
                    if now.saturating_sub(last_ok) >= job.timeout {
                        world.say(&format!("job {}: the factory is gone: {why}", job.id));
                        return die(world, job, work.as_mut(), grace, End::Offline);
                    }
                }
            }
        }
        world.pause(1);
    }
}

fn finished(world: &dyn World, job: &Job, code: i32) -> End {
    let said = match world.result(job) {
        Some((result, exit)) => report(
            world,
            job,
            "result",
            &json!({ "result": result, "exit": exit }),
        ),
        None => report(
            world,
            job,
            "stopped",
            &json!({ "why": format!("the cycle ended with {code} and wrote no result") }),
        ),
    };
    if !matches!(said, Reply::Ok(_)) {
        world.say(&format!(
            "job {}: the factory did not take its end: {said:?}",
            job.id
        ));
    }
    world.wipe();
    End::Finished
}

/// The orderly end of a job the contractor stops: the stop signal, the grace,
/// the salvage push and the report while the job is still current, and the wipe.
fn die(world: &dyn World, job: &Job, work: &mut dyn Work, grace: u64, end: End) -> End {
    let why = match &end {
        End::Offline => "stopped: contractor offline".to_string(),
        End::Lost(word) => format!("stopped: the factory says the job is {word}"),
        End::Revoked => "stopped: contractor revoked".to_string(),
        End::Drained => "stopped: contractor draining".to_string(),
        End::Finished | End::NotStarted(_) => "stopped".to_string(),
    };
    world.say(&format!("job {}: {why}", job.id));
    work.stop();
    let until = world.now() + grace;
    while work.done().is_none() {
        if world.now() >= until {
            world.say(&format!(
                "job {}: the cycle outlived its grace, killed",
                job.id
            ));
            work.kill();
            break;
        }
        world.pause(1);
    }
    // A revoked contractor pushes nothing, and neither does an attempt that a
    // newer one replaced: its push would move the branch the replacement works on.
    let current = !matches!(&end, End::Revoked) && end != End::Lost(STALE.to_string());
    if current {
        if !world.salvage(job) {
            world.say(&format!("job {}: its branch could not be pushed", job.id));
        }
        // A result the cycle wrote while it stopped is still taken while its
        // attempt is current; otherwise the job is handed back.
        let _ = match world.result(job) {
            Some((result, exit)) => report(
                world,
                job,
                "result",
                &json!({ "result": result, "exit": exit }),
            ),
            None => report(world, job, "stopped", &json!({ "why": why })),
        };
    }
    world.wipe();
    end
}

/// Ask for jobs and run them until drained or revoked. The workspace is wiped
/// first. Exit: 0 drained; 1 revoked, or the factory does not know this
/// contractor.
pub fn run(world: &dyn World, grace: u64, poll: u64) -> i32 {
    world.wipe();
    loop {
        if world.draining() {
            world.say("drained: no job taken, none running");
            return 0;
        }
        match world.ask("hello") {
            Reply::Ok(_) => {}
            r if r.revoked() => {
                world.say("the factory revoked this contractor: it takes no more work");
                return 1;
            }
            Reply::No(_, word) => {
                world.say(&format!("the factory refused this contractor: {word}"));
                return 1;
            }
            Reply::Down(why) => {
                world.say(&format!("the factory did not answer: {why}"));
                world.pause(poll);
                continue;
            }
        }
        match world.ask("job") {
            Reply::Ok(v) => match Job::of(&v) {
                Some(job) => {
                    let end = run_job(world, &job, grace);
                    world.say(&format!("job {}: {end:?}", job.id));
                }
                None => world.pause(poll),
            },
            r if r.revoked() => {
                world.say("the factory revoked this contractor: it takes no more work");
                return 1;
            }
            _ => world.pause(poll),
        }
    }
}
