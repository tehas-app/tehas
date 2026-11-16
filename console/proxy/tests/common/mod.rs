//! The parts both integration files of the proxy need: a stub endpoint that
//! answers what a case plans and holds the bytes of the ask, the factory's
//! records as rows a case says what they hold, and the journal the proxy's
//! lines go to.
//!
//! Nothing here reads a file, a database or the network beyond a port of
//! 127.0.0.1 that the test itself bound.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use tehas_proxy::forward::Upstream;
use tehas_proxy::jobs::Issuer;
use tehas_proxy::records::{Job, Rows};
use tehas_proxy::request::State;

/// How long a case waits for a listener to bind: long enough that a wait that
/// ends is a bug's timeout and not this one.
pub const BIND_WAIT: Duration = Duration::from_secs(25);

/// What the stub answers with: its status, and the body in two parts, the
/// second after `late` of silence. The shape the answer goes back in is the
/// shape the contractor asked for, which the stub reads out of the ask: a prompt
/// that asks for a stream is answered with an event stream whose parts go out as
/// they are made, and one that does not with a completion that names its own
/// length. The parts are what says whether a proxy passed them on as they came
/// or kept them to its end.
pub struct Plan {
    pub status: u16,
    pub first: String,
    pub second: String,
    pub late: Duration,
}

impl Plan {
    /// The two parts of an answer, the second after `late` of silence.
    pub fn of(status: u16, first: &str, second: &str, late: Duration) -> Plan {
        Plan {
            status,
            first: first.to_string(),
            second: second.to_string(),
            late,
        }
    }

    /// An answer of one part, with the endpoint's own figures for a completion.
    pub fn json(status: u16, prompt: u64, completion: u64) -> Plan {
        Plan::text(
            status,
            &format!(
                r#"{{"choices":[{{"message":{{"content":"the answer"}}}}],"usage":{{"prompt_tokens":{prompt},"completion_tokens":{completion},"total_tokens":{}}}}}"#,
                prompt + completion
            ),
        )
    }

    /// An answer of one part, of no figures.
    pub fn text(status: u16, body: &str) -> Plan {
        Plan::of(status, body, "", Duration::ZERO)
    }

    /// An endpoint that accepts the ask and then answers nothing, which the
    /// instance's idle bound ends: the silence is the case, and the proxy's own
    /// 502 is what comes of it.
    pub fn silent() -> Plan {
        Plan::of(200, "", "", Duration::ZERO)
    }
}

/// The stub endpoint: a port of its own that answers the plan and holds the
/// whole of the ask, so a refusal that made no call is proved by the count and
/// a body that was forwarded is proved by the bytes.
pub struct Endpoint {
    port: u16,
    calls: Arc<AtomicUsize>,
    asked: Arc<Mutex<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    handling: Arc<AtomicBool>,
}

/// What one connection of the stub needs, so the accept thread hands one value
/// to the thread that answers.
struct Stub {
    plan: Arc<Mutex<Plan>>,
    calls: Arc<AtomicUsize>,
    asked: Arc<Mutex<Vec<u8>>>,
    stop: Arc<AtomicBool>,
}

impl Endpoint {
    pub fn open(plan: Plan) -> Endpoint {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (plan, calls, asked, stop, handling) = (
            Arc::new(Mutex::new(plan)),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        {
            let (plan, calls, asked, stop, handling) = (
                Arc::clone(&plan),
                Arc::clone(&calls),
                Arc::clone(&asked),
                Arc::clone(&stop),
                Arc::clone(&handling),
            );
            thread::spawn(move || {
                let _ = listener.set_nonblocking(true);
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let held = Arc::clone(&handling);
                            held.store(true, Ordering::SeqCst);
                            let stub = Stub {
                                plan: Arc::clone(&plan),
                                calls: Arc::clone(&calls),
                                asked: Arc::clone(&asked),
                                stop: Arc::clone(&stop),
                            };
                            thread::spawn(move || {
                                serve_stub(stream, &stub);
                                held.store(false, Ordering::SeqCst);
                            });
                        }
                        Err(_) => thread::sleep(Duration::from_millis(20)),
                    }
                }
            });
        }
        Endpoint {
            port,
            calls,
            asked,
            stop,
            handling,
        }
    }

    /// The same stub for a thread of a case: the handle of a port that is
    /// already open, so a case can give a thread its endpoint without a second
    /// stub that a case would have to answer twice.
    pub fn at(port: u16) -> Endpoint {
        Endpoint {
            port,
            calls: Arc::new(AtomicUsize::new(0)),
            asked: Arc::new(Mutex::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(true)),
            handling: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The port the stub answers on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The endpoint of the instance's settings: this port, under `/v1`.
    pub fn upstream(&self) -> Upstream {
        Upstream {
            host: "127.0.0.1".into(),
            port: self.port,
            base: "/v1".into(),
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// What the proxy sent: its own head and the body it forwarded.
    pub fn asked(&self) -> Vec<u8> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The ask as text: a stub's body is a case's own words.
    pub fn asked_text(&self) -> String {
        String::from_utf8_lossy(&self.asked()).into_owned()
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // A connection the stub is still holding keeps its thread, and the
        // port with it, until the socket dies.
        let deadline = Instant::now() + Duration::from_secs(35);
        while self.handling.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// The endpoint's half of one request. It reads the whole of what the proxy
/// sent, the head to its end and then the body the head's content-length
/// promises, and holds it for the case; then it sends the head, the first part
/// of the body, and the rest after the plan's wait.
fn serve_stub(mut stream: TcpStream, stub: &Stub) {
    let (plan, calls, asked, stop) = (&stub.plan, &stub.calls, &stub.asked, &stub.stop);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut got = Vec::new();
    let mut chunk = [0u8; 4096];
    while tehas_proxy::forward::head_end(&got).is_none() {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => got.extend_from_slice(&chunk[..n]),
            Err(_) => return,
        }
    }
    // The whole of the request: the proxy's head, the body of the length that
    // head's own content-length names, and the blank line that ends it. The
    // field is read from the whole, since the head the proxy writes puts its
    // own fields first.
    let want = tehas_proxy::forward::head_end(&got).unwrap_or(0)
        + tehas_proxy::forward::content_length(&got).unwrap_or(0)
        + 2;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
    while got.len() < want {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => got.extend_from_slice(&chunk[..n]),
        }
    }
    {
        let mut held = asked.lock().unwrap_or_else(PoisonError::into_inner);
        *held = got.clone();
    }
    calls.fetch_add(1, Ordering::SeqCst);

    // Whether the contractor asked for a stream, out of the ask the proxy
    // forwarded: the stub answers the ask it was given, as an endpoint does,
    // which is what lets a case ask for a stream in its prompt alone.
    let wants_stream = String::from_utf8_lossy(&got)
        .to_ascii_lowercase()
        .replace(' ', "")
        .contains("\"stream\":true");
    let (status, first, second, late) = {
        let plan = plan.lock().unwrap_or_else(PoisonError::into_inner);
        (
            plan.status,
            plan.first.clone(),
            plan.second.clone(),
            plan.late,
        )
    };
    if first.is_empty() && second.is_empty() {
        // An endpoint that answers nothing at all: what ends the request is the
        // instance's idle bound, and the words of that are the proxy's.
        return;
    }
    // A stream is sent as an event stream is: no length, the parts written as
    // they are made. A completion of a length names its own.
    let head = if wants_stream {
        format!(
            "HTTP/1.1 {status} OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n"
        )
    } else {
        format!(
            "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            first.len() + second.len()
        )
    };
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(first.as_bytes());
    let _ = stream.flush();
    if !late.is_zero() {
        // The first part now and the rest after the wait: the bound of a stream
        // is counted from the last byte, so this is a stream gone silent.
        thread::sleep(late);
    }
    let _ = stream.write_all(second.as_bytes());
    let _ = stream.flush();
    // The answer is whole, so the stub ends the connection, as an endpoint of a
    // length and a stream of `connection: close` both do. The waiting is the
    // proxy's own: a stub that held the socket after its last byte would hold the
    // proxy's thread past an answer nobody is left to read.
    let _ = stream.shutdown(std::net::Shutdown::Both);
    let _ = stop.load(Ordering::SeqCst);
}

/// The records of a case: the rows the proxy reads, behind a lock, and the one
/// table of the tokens the proxy holds. A case changes the rows between two
/// requests, which is how a job that ends mid-work and a contractor revoked
/// mid-stream are tested: the proxy asks the factory again for every request, so
/// the next one is refused rather than remembered wrongly from the last.
#[derive(Clone)]
pub struct Records {
    rows: Arc<Mutex<Rows>>,
    issuer: Issuer,
}

impl Records {
    /// The records of a case: the rows it says the factory holds, and the tokens
    /// of the proxy that answers them. A case never hands a token to the rows of
    /// its own, and never holds a token of its own either: the proxy is their one
    /// holder, as on a host, which is what a revocation and the end of a job
    /// empty.
    pub fn of(rows: Rows, issuer: &Issuer) -> Records {
        Records {
            rows: Arc::new(Mutex::new(rows)),
            issuer: issuer.shared(),
        }
    }

    /// Records that read nothing at all, of a proxy that holds no token: the
    /// state of an instance whose outsource tables are not there yet, which is
    /// every instance until #2272's land, and the state of a read that failed.
    pub fn empty() -> Records {
        Records::of(Rows::default(), &Issuer::new())
    }

    /// What a job's record says now.
    pub fn end(&self, job: i64, state: State) {
        self.edit(|rows| {
            if let Some(row) = rows.jobs.iter_mut().find(|j| j.job == job) {
                row.state = state.word().to_string();
            }
        });
    }

    /// A contractor that may no longer work. The proxy asks only who may, so
    /// the row goes: a job of it is then refused as the job of a contractor
    /// that is revoked or unknown.
    pub fn revoke(&self, contractor: &str) {
        self.edit(|rows| rows.contractors.retain(|c| c != contractor));
    }

    /// Change what the rows say, in place.
    pub fn edit(&self, what: impl FnOnce(&mut Rows)) {
        what(&mut self.rows.lock().unwrap_or_else(PoisonError::into_inner));
    }

    /// What the rows say now.
    pub fn snapshot(&self) -> Rows {
        self.rows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl tehas_proxy::records::Reader for Records {
    fn rows(&self) -> Rows {
        self.snapshot()
    }
}

impl tehas_proxy::request::Records for Records {
    /// The job and contractor of a token the proxy holds. A token it never
    /// issued, or one it took back when the job ended or its contractor was
    /// revoked, names no job: the rows of the factory name a token and are never
    /// what answers for one.
    fn live(&self, token: &str) -> Option<tehas_proxy::request::Live> {
        self.issuer.look(token)
    }
    fn state_of(&self, job: i64) -> Option<State> {
        self.snapshot()
            .jobs
            .iter()
            .find(|j| j.job == job)
            .and_then(|j| State::of(&j.state))
    }
    fn allowed(&self, contractor: &str) -> bool {
        self.snapshot().contractors.iter().any(|c| c == contractor)
    }
    /// The ask of a request whose token names no job: a request of a token of a
    /// job was counted by [`Issuer::look`] above and is never counted twice.
    fn asked(&self, token: &str) {
        self.issuer.asked(token);
    }
}

/// The token of a job of a case, fixed so a case can name it in a request of its
/// own bytes and read it back out of a line it must not appear in. The proxy
/// holds the same tokens, in its own table.
pub fn token_of(job: i64) -> String {
    format!("tok-{job}")
}

/// The rows of a case, each a job: its number, its contractor and what its own
/// record says. The token of a row is the one `mint` gives for the job, so the
/// rows and the proxy's own table agree on which token is whose without either
/// naming a token of the other's.
pub fn rows_of(jobs: &[(&str, &str, State)], mint: impl Fn(i64) -> String) -> Rows {
    let mut rows = Rows::default();
    for (job, contractor, state) in jobs {
        let (job, contractor) = (job.parse().unwrap(), (*contractor).to_string());
        if !rows.contractors.iter().any(|c| c == &contractor) {
            rows.contractors.push(contractor.clone());
        }
        rows.jobs.push(Job {
            job,
            attempt: 1,
            contractor,
            state: state.word().to_string(),
            token: mint(job),
        });
    }
    rows
}

/// Read from a socket until the answer is whole, with a wait that cannot hang
/// the suite: a content-length head is whole when its body has arrived, and a
/// stream is whole when the endpoint's end-of-stream marker has.
pub fn read_some(mut stream: TcpStream) -> (u16, String) {
    let mut said = Vec::new();
    let mut chunk = [0u8; 4096];
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                said.extend_from_slice(&chunk[..n]);
                if said.windows(10).any(|w| w == b"data: [DONE]") {
                    break;
                }
                if let Some(end) = tehas_proxy::forward::head_end(&said) {
                    if let Some(want) = tehas_proxy::forward::content_length(&said) {
                        if said.len() >= end + want {
                            break;
                        }
                    }
                }
            }
        }
    }
    let text = String::from_utf8_lossy(&said).into_owned();
    let status = text
        .strip_prefix("HTTP/1.1 ")
        .and_then(|s| s.split(' ').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, text)
}

/// A contractor's request: the token it was given, the path it asks for, and
/// the body it sends with it. The Host and the token are the contractor's own,
/// which is what the proxy is to drop.
pub fn request(token: &str, path: &str, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: factory:8090\r\nAuthorization: Bearer {token}\r\n\
         content-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// A prompt that holds the marker a case looks for, and the source it stands
/// for: a body is a prompt and nothing else, which is why none of it is logged.
pub fn prompt(marker: &str) -> String {
    format!(
        r#"{{"model":"main","messages":[{{"role":"user","content":"read {marker} and finish the file"}}]}}"#
    )
}
