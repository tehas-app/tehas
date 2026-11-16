//! Rules 1 to 6 of the model proxy, in the process: one end of a socket pair
//! holds the contractor's request, `serve::one` answers the proxy's half on a
//! thread of its own, and a stub endpoint waits on a port of its own. The
//! listener's own port, the counts of the requests and the answers it gives
//! before it reads the records are `listener.rs`. No network beyond a port of
//! 127.0.0.1 and no database: the outside world is what these tests make.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::Builder;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tehas_proxy::jobs::Issuer;
use tehas_proxy::log::Sink;
use tehas_proxy::records::Rows;
use tehas_proxy::request::State;
use tehas_proxy::serve::{self, Serve};

mod common;
use common::{chunk, pair, prompt, read_some, request, rows_of, token_of, Endpoint, Plan, Records};

/// The records of a case, and the tokens this process issued for the rows, so
/// a case never has to name a token twice.
/// The records of a case, and the tokens this process issued for its rows, so
/// a case never has to name a token twice.
fn records_of(rows: Rows, issuer: &Issuer) -> Records {
    let records = Records::empty();
    records.load(rows, token_of);
    let _ = issuer;
    records
}

/// The proxy of a test: the parts the unit is started with, as fields, so a
/// case can say what was counted, what was answered and what was written down.
struct Proxy {
    endpoint: Endpoint,
    key: String,
    issuer: Issuer,
    factory: Records,
    lines: Arc<Mutex<Vec<String>>>,
    idle: Duration,
}

impl Proxy {
    /// A proxy with the records of the jobs a case lists. The endpoint's key is
    /// unmistakably not a token, so a case can tell one from the other.
    fn new(endpoint: Endpoint, jobs: &[(&str, &str, State)]) -> Proxy {
        let issuer = Issuer::new();
        let rows = rows_of(jobs);
        let factory = records_of(rows, &issuer);
        Proxy {
            endpoint,
            key: "sk-the-factory-s-own-key".to_string(),
            issuer,
            factory,
            lines: Arc::new(Mutex::new(Vec::new())),
            idle: Duration::from_secs(5),
        }
    }

    /// A proxy whose factory reads nothing: an instance of #2272's day before.
    fn of_no_records(endpoint: Endpoint) -> Proxy {
        Proxy {
            factory: Records::empty(),
            ..Proxy::new(endpoint, &[])
        }
    }

    /// One request through the proxy, and the answer as the contractor read it.
    fn ask(&self, token: &str, path: &str, body: &str) -> (u16, String) {
        self.request(&request(token, path, body))
    }

    /// One request of one job of the case, by its number.
    fn job(&self, job: i64, path: &str, body: &str) -> (u16, String) {
        self.ask(&token_of(job), path, body)
    }

    /// One thing through the proxy, the bytes of a request exactly as a case
    /// wrote them, which is how a request of no shape is sent.
    fn request(&self, bytes: &str) -> (u16, String) {
        self.request_of(bytes, self.idle)
    }

    fn request_of(&self, bytes: &str, idle: Duration) -> (u16, String) {
        let (mut proxy_half, client) = pair();
        write(&client, bytes);
        let _ = self.answer(&mut proxy_half, idle);
        read_some(client)
    }

    /// The proxy's half of one request, on the calling thread.
    fn answer(&self, client: &mut TcpStream, idle: Duration) -> tehas_proxy::serve::Outcome {
        let kept = JournalSink(Arc::clone(&self.lines));
        let serve = Serve {
            endpoint: self.endpoint.upstream(),
            key: self.key.clone(),
            records: &self.factory,
            sink: &kept,
            idle,
        };
        tehas_proxy::serve::one(client, &serve)
    }

    /// One request of one job of the case, answered on a thread of its own, so
    /// the case reads the other end of the pair while the answer is still
    /// coming. The `Serve` is made for the thread, which is what lets a case
    /// change the records or the bound between two requests.
    fn answering(
        &self,
        client: TcpStream,
        idle: Duration,
    ) -> JoinHandle<tehas_proxy::serve::Outcome> {
        let (proxy, client) = (
            Proxy {
                endpoint: self.endpoint.again(),
                key: self.key.clone(),
                issuer: self.issuer.shared(),
                factory: self.factory.clone(),
                lines: Arc::clone(&self.lines),
                idle: self.idle,
            },
            client,
        );
        Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || {
                let mut client = client;
                let kept = JournalSink(Arc::clone(&proxy.lines));
                let serve = Serve {
                    endpoint: proxy.endpoint.upstream(),
                    key: proxy.key.clone(),
                    records: &proxy.factory,
                    sink: &kept,
                    idle,
                };
                tehas_proxy::serve::one(&mut client, &serve)
            })
            .expect("a thread for the proxy's half")
    }

    /// The lines the proxy said, in order.
    fn said(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// What the proxy counted of the requests this process answered, per
    /// contractor and per job.
    fn counted(&self) -> Vec<(String, i64, u64)> {
        self.issuer.take_counts()
    }
}

/// Write a contractor's bytes and keep the socket open, which is what a
/// contractor does while it waits for the answer.
fn write(stream: &TcpStream, bytes: &str) {
    let mut asked = stream.try_clone().unwrap();
    asked.write_all(bytes.as_bytes()).unwrap();
    asked.flush().unwrap();
}

/// The sink the proxy of a test writes to: the same journal the unit's lines
/// go to, kept where the case can read it back.
struct JournalSink(Arc<Mutex<Vec<String>>>);

impl Sink for JournalSink {
    fn line(&self, line: &tehas_proxy::log::Line) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line.0.clone());
    }
}

/// The figures of an answer, as the contractor read them.
fn usage_of(said: &str) -> Option<tehas_proxy::count::Usage> {
    let body = said.split_once("\r\n\r\n")?.1;
    tehas_proxy::count::of_answer(body.as_bytes())
}

// --- the rules ---------------------------------------------------------------

/// Rule 1: a streamed reply reaches the contractor chunk by chunk. The stub
/// sends one part at once and the second a second later; a proxy that buffered
/// the answer would give the contractor nothing until the second had come.
#[test]
fn a_stream_reaches_the_contractor_chunk_by_chunk() {
    let endpoint = Endpoint::open(Plan::streaming(
        &chunk(r#"{"choices":[{"delta":{"content":"one"}}]}"#),
        &format!(
            "{}{}\n\n",
            chunk(
                r#"{"choices":[{"delta":{"content":"two"}}],"usage":{"prompt_tokens":4,"completion_tokens":6,"total_tokens":10}}"#
            ),
            "data: [DONE]"
        ),
        Duration::from_millis(700),
    ));
    let proxy = Proxy::new(endpoint, &[("7", "gpu1", State::Running)]);
    let body = r#"{"model":"main","stream":true,"messages":[{"role":"user","content":"read SRC-MARKER"}]}"#;
    let (proxy_half, client) = pair();
    write(
        &client,
        &request(&token_of(7), "/v1/chat/completions", body),
    );
    // The proxy's half, on a thread of its own: it ends only when the
    // contractor has had the endpoint's last byte.
    let answer = proxy.answering(proxy_half, Duration::from_secs(5));

    let mut reader = client.try_clone().unwrap();
    // The contractor's end, reading as the answer comes. The request went
    // before the proxy was given its half of the pair.
    let began = Instant::now();
    let _ = reader.set_read_timeout(Some(Duration::from_millis(600)));
    let mut first = Vec::new();
    let mut part = [0u8; 4096];
    while let Ok(n) = reader.read(&mut part) {
        if n == 0 {
            break;
        }
        first.extend_from_slice(&part[..n]);
        if String::from_utf8_lossy(&first).contains("\"one\"") {
            break;
        }
    }
    let waited = began.elapsed();
    let first = String::from_utf8_lossy(&first).into_owned();
    assert!(
        first.contains("\"one\""),
        "the first chunk never came to the contractor: {first:?}"
    );
    assert!(
        !first.contains("\"two\""),
        "both chunks came at once, which is a buffered answer: {first}"
    );
    assert!(
        waited < Duration::from_millis(650),
        "the first chunk took {waited:?}, which is the second's wait"
    );

    // The rest, to the end of the stream: the figures ride in the last chunk
    // that carries them, so they are known only once the contractor has had it.
    let _ = reader.set_read_timeout(Some(Duration::from_secs(5)));
    let mut rest = Vec::new();
    loop {
        match reader.read(&mut part) {
            Ok(0) | Err(_) => break,
            Ok(n) => rest.extend_from_slice(&part[..n]),
        }
    }
    let whole = format!("{first}{}", String::from_utf8_lossy(&rest));
    assert!(whole.contains("data: [DONE]"), "no end of stream: {whole}");

    let outcome = answer.join().expect("the proxy's half of the stream");
    assert_eq!(outcome.status, 200);
    assert!(
        outcome.complete,
        "the stream stopped on the way to the contractor"
    );
    assert_eq!(
        outcome.usage,
        Some(tehas_proxy::count::Usage {
            prompt: 4,
            completion: 6,
            total: 10
        }),
        "the figures the stream carried were not counted"
    );
    let lines = proxy.said().join("\n");
    assert!(lines.contains(" 200 "), "a stream of no line: {lines}");
    assert_eq!(proxy.endpoint.calls(), 1);
}

/// Rule 7 and rule 5: a busy endpoint's answer goes back as it came, its status
/// and its body, so the agents' retry of a busy model works unchanged.
#[test]
fn a_busy_endpoint_is_answered_as_it_answered() {
    let endpoint = Endpoint::open(Plan::text(
        429,
        r#"{"error":{"type":"rate_limit","message":"no slot for you"}}"#,
    ));
    let proxy = Proxy::new(endpoint, &[("7", "gpu1", State::Running)]);
    let (status, said) = proxy.job(7, "/v1/chat/completions", &prompt("SRC-MARKER"));
    assert_eq!(status, 429, "a busy endpoint was rewritten: {said}");
    assert!(
        said.contains("rate_limit") && said.contains("no slot for you"),
        "the endpoint's own reason was swallowed: {said}"
    );
    assert_eq!(proxy.endpoint.calls(), 1);
    let lines = proxy.said().join("\n");
    assert!(
        lines.contains(" 429 "),
        "the log says no busy status: {lines}"
    );
    // A busy answer prices nothing: the endpoint answered no completion.
    assert_eq!(usage_of(&said), None, "a busy answer was priced");
}

/// Rule 3: only the model paths a cycle needs are forwarded; the rest are
/// refused before the endpoint is called. The proof is the stub's call count.
#[test]
fn a_path_outside_the_list_is_refused_without_an_endpoint_call() {
    let endpoint = Endpoint::open(Plan::json(200, 1, 2));
    let proxy = Proxy::new(endpoint, &[("7", "gpu1", State::Running)]);
    for path in [
        "/v1/embeddings",
        "/v1/files",
        "/v1/responses",
        "/v1/messages",
        "/admin",
        "/",
        "/v1/chat",
        "/v1/models/secret",
        "/v1/admin",
    ] {
        let (status, _) = proxy.job(7, path, "{}");
        assert_eq!(status, 404, "{path}");
        assert_eq!(proxy.endpoint.calls(), 0, "{path} reached the endpoint");
    }
    // And a path a cycle needs reaches it.
    let (status, _) = proxy.job(7, "/v1/chat/completions", "{}");
    assert_eq!(status, 200);
    assert_eq!(proxy.endpoint.calls(), 1);
    // A path of the list under the endpoint's own base, and one of the names
    // the same endpoints go by.
    for path in [
        "/chat/completions",
        "/models",
        "/models/list",
        "/count-tokens",
        "/completions",
        "/health",
    ] {
        let (status, _) = proxy.job(7, path, "{}");
        assert_eq!(status, 200, "{path} of the list was refused");
    }
}

/// Rule 4: a job that ended, a contractor that is revoked and a token of no job
/// each end the request with one line and no endpoint call, judged from the
/// records and never from a field the contractor sent.
#[test]
fn a_dead_job_and_a_revoked_contractor_are_refused_with_one_line() {
    let endpoint = Endpoint::open(Plan::json(200, 1, 2));
    let proxy = Proxy::new(
        endpoint,
        &[
            ("7", "gpu1", State::Running),
            ("8", "gpu2", State::Running),
            ("9", "gpu3", State::Running),
        ],
    );

    for state in [State::Finished, State::Released, State::Lost] {
        proxy.factory.end(7, state);
        let (status, _) = proxy.job(7, "/v1/chat/completions", "{}");
        assert_eq!(status, 403, "{}", state.word());

        proxy.factory.end(7, State::Running);
    }
    // A job whose state the records do not know is a job the proxy cannot
    // vouch for, and is refused as one that ended.
    proxy.factory.end(8, State::Lost);
    let (status, _) = proxy.job(8, "/v1/chat/completions", "{}");
    assert_eq!(status, 403, "a job of no known state was answered");
    proxy.factory.revoke("gpu3");
    let (status, _) = proxy.job(9, "/v1/chat/completions", "{}");
    assert_eq!(status, 403, "a revoked contractor was answered");

    // A token of no job, and no token at all: the same answer, and no call.
    let (status, _) = proxy.ask("no-such-token", "/v1/chat/completions", "{}");
    assert_eq!(status, 401);
    let (status, _) =
        proxy.request("POST /v1/chat/completions HTTP/1.1\r\ncontent-length: 2\r\n\r\n{}");
    assert_eq!(status, 401);
    // A token the records name, in a request whose head holds it in another
    // field: what a contractor sends decides nothing.
    let (status, _) = proxy.request(&format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: factory:8090\r\nx-token: {}\r\ncontent-length: 2\r\n\r\n{{}}",
        token_of(7)
    ));
    assert_eq!(status, 401, "a token of another field was believed");

    assert_eq!(proxy.endpoint.calls(), 0, "a refusal called the endpoint");
    let lines = proxy.said();
    assert_eq!(lines.len(), 8, "one line per refusal: {lines:?}");
    assert!(
        lines.iter().all(|l| l.starts_with("refused ")),
        "a refusal that is no refusal: {lines:?}"
    );
    assert!(
        lines.iter().all(|l| !l.contains("tok-")),
        "a refusal that names a token: {lines:?}"
    );
    // A job that ended and a contractor revoked are named in the line, so the
    // host's journal says which of the two refused.
    assert!(
        lines
            .iter()
            .any(|l| l.contains("a job whose record says lost")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("job 9 of contractor gpu3")
            && l.contains("a contractor that is revoked or unknown")),
        "{lines:?}"
    );
}

/// Rule 6: a body is never logged, because it holds source code. The marker is
/// in the prompt the contractor sent and in the answer the endpoint sent, and
/// appears nowhere in what the proxy wrote.
#[test]
fn a_body_is_never_logged() {
    let endpoint = Endpoint::open(Plan::text(
        200,
        r#"{"choices":[{"message":{"content":"the answer holds SRC-MARKER-OUT-OF-A-BODY"}}]}"#,
    ));
    let proxy = Proxy::new(endpoint, &[("7", "gpu1", State::Running)]);
    let (status, said) = proxy.ask(
        &token_of(7),
        "/v1/chat/completions",
        &prompt("SRC-MARKER-IN-A-BODY"),
    );
    assert_eq!(status, 200, "the answer itself was lost: {said}");
    // The body did go to the endpoint, which is the proof that there was a body
    // to be logged: the stub's own record of the ask holds it.
    let sent = proxy.endpoint.asked_text();
    assert!(
        sent.contains("SRC-MARKER-IN-A-BODY"),
        "the body never went to the endpoint: {sent}"
    );
    let lines = proxy.said();
    assert_eq!(lines.len(), 1, "one line for one answer: {lines:?}");
    for line in &lines {
        assert!(!line.contains("SRC-MARKER"), "a body was logged: {line}");
        assert!(!line.contains("tok-"), "a token was logged: {line}");
        assert!(
            !line.contains("sk-the-factory"),
            "the endpoint's key was logged: {line}"
        );
        assert!(!line.contains('\n'), "a line that is two lines: {line:?}");
    }
    assert!(
        lines[0].starts_with("POST /v1/chat/completions 200 ")
            && lines[0].ends_with("ms job 7 contractor gpu1"),
        "the line of an answer names the ask, the status and the job: {}",
        lines[0]
    );
    // A path too long for one line is cut, and holds no line end: a body is
    // never a path, but a path must not become two lines either.
    let long = format!("/v1/models/{}", "x".repeat(300));
    let proxy = Proxy::new(
        Endpoint::open(Plan::json(200, 1, 1)),
        &[("7", "gpu1", State::Running)],
    );
    let (status, _) = proxy.job(7, &long, "{}");
    assert_eq!(status, 404);
    let line = &proxy.said()[0];
    // The path of the line is cut at the bound of one line, so a probe cannot
    // make the proxy say a paragraph.
    let said = line
        .split("the path ")
        .nth(1)
        .unwrap()
        .split(" is not a model path")
        .next()
        .unwrap();
    assert_eq!(said.len(), 200, "a path of no bound: {said}");
    assert!(
        said.ends_with(&"x".repeat(100)),
        "a cut path of no use: {said}"
    );
}

/// Rule 5: the endpoint's own figures are counted per contractor and per job,
/// and the row that goes to the store holds no token, no path and no body. The
/// proxy never guesses a number: an answer that named none is no row at all.
#[test]
fn the_figures_of_an_answer_are_counted_per_contractor_and_job() {
    let endpoint = Endpoint::open(Plan::json(200, 120, 340));
    let proxy = Proxy::new(
        endpoint,
        &[
            ("7", "gpu1", State::Running),
            ("8", "gpu1", State::Running),
            ("9", "gpu2", State::Running),
        ],
    );
    let mut rows: Vec<String> = Vec::new();
    for token in [7i64, 8, 9] {
        let (status, said) = proxy.job(token, "/v1/chat/completions", "{}");
        assert_eq!(status, 200, "job {token}");
        let usage = usage_of(&said).expect("the endpoint priced its own answer");
        assert_eq!(
            usage,
            tehas_proxy::count::Usage {
                prompt: 120,
                completion: 340,
                total: 460
            },
            "a figure of the proxy's own"
        );
        // The row is the one the request's own outcome gives, which is what
        // keeps a count of a number the proxy did not read.
        let outcome = tehas_proxy::serve::Outcome {
            status,
            live: proxy.issuer.look(&token_of(token)),
            usage: Some(usage),
            complete: true,
        };
        let row = outcome.row().expect("a priced answer has a row");
        rows.push(serve::usage_args(&row).join(" "));
    }
    let all = rows.join("\n");
    assert!(all.contains("--contractor gpu1 --job 7"), "{all}");
    assert!(all.contains("--contractor gpu1 --job 8"), "{all}");
    assert!(all.contains("--contractor gpu2 --job 9"), "{all}");
    assert!(
        all.contains("--prompt 120 --completion 340 --total 460"),
        "{all}"
    );
    assert!(!all.contains("tok-"), "a row that holds a token: {all}");
    assert!(!all.contains("chat"), "a row that names a path: {all}");

    // The requests themselves are counted per contractor and per job, from the
    // table of tokens this process issued.
    let counts = serve::request_rows(&proxy.counted());
    assert_eq!(
        counts.len(),
        3,
        "one row per contractor and job: {counts:?}"
    );
    let said = counts
        .iter()
        .map(|c| c.join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("--contractor gpu1 --job 7 --requests 1"),
        "{said}"
    );
    assert!(
        said.contains("--contractor gpu1 --job 8 --requests 1"),
        "{said}"
    );
    assert!(
        said.contains("--contractor gpu2 --job 9 --requests 1"),
        "{said}"
    );
}

/// An endpoint that answers nothing is a 502 of the proxy's own, in one line,
/// and never an answer invented for it. The bound that ends it is the
/// instance's, and it is longer than the check-in timeout.
#[test]
fn an_endpoint_that_answers_nothing_is_a_502_and_no_invented_answer() {
    let endpoint = Endpoint::open(Plan::silent());
    let proxy = Proxy::new(endpoint, &[("7", "gpu1", State::Running)]);
    let (status, said) = proxy.job(7, "/v1/chat/completions", "{}");
    assert_eq!(
        status, 502,
        "an endpoint of no answer was answered for it: {said}"
    );
    assert!(said.contains("Bad Gateway"), "{said}");
    let lines = proxy.said().join("\n");
    assert!(lines.contains(" 502 "), "{lines}");
    // The words of the failure name the host and not the key it was to send.
    assert!(
        !lines.contains("sk-the-factory"),
        "a line of the key: {lines}"
    );
    // The ask is counted, since the factory was asked; the answer that never
    // arrived is priced by nothing.
    assert_eq!(proxy.counted(), vec![("gpu1".to_string(), 7, 1)]);
    // The connection was ended rather than held: the contractor's read ends.
    assert!(
        said.ends_with("\r\n\r\n") || said.contains("connection: close"),
        "a 502 that holds the socket open: {said}"
    );
}

/// A factory whose records read nothing forwards nothing: the fail-closed half
/// of the proxy, and the state of every instance until #2272's tables are read.
#[test]
fn a_factory_of_no_records_answers_only_refusals() {
    let endpoint = Endpoint::open(Plan::json(200, 1, 1));
    let proxy = Proxy::of_no_records(endpoint);
    for token in [token_of(7), "any-word-at-all".to_string()] {
        let (status, _) = proxy.ask(&token, "/v1/chat/completions", "{}");
        assert_eq!(status, 401, "a token of no job was answered");
    }
    // A path outside the list is refused before the token is even read.
    let (status, _) = proxy.job(7, "/v1/embeddings", "{}");
    assert_eq!(status, 404);
    assert_eq!(
        proxy.endpoint.calls(),
        0,
        "the model was reached with no records"
    );
    let lines = proxy.said();
    assert_eq!(lines.len(), 3, "one line per refusal: {lines:?}");
    assert!(lines.iter().all(|l| l.starts_with("refused ")), "{lines:?}");
}

/// The endpoint's own key goes on the request, and the contractor's token and
/// its own head do not: the factory's key never leaves the factory, and the
/// job token never reaches the endpoint.
#[test]
fn the_endpoint_sees_its_own_key_and_never_the_job_s_token() {
    let endpoint = Endpoint::open(Plan::json(200, 1, 1));
    let proxy = Proxy::new(endpoint, &[("7", "gpu1", State::Running)]);
    let (status, _) = proxy.job(7, "/v1/chat/completions", "{}");
    assert_eq!(status, 200);
    let sent = proxy.endpoint.asked_text();
    assert!(
        sent.contains("Authorization: Bearer sk-the-factory-s-own-key"),
        "the endpoint was not given its own key: {sent}"
    );
    assert!(
        !sent.contains("tok-"),
        "the job token reached the endpoint: {sent}"
    );
    assert!(
        !sent.contains("Host: factory"),
        "the contractor's Host went on: {sent}"
    );
    assert!(
        sent.contains("Host: 127.0.0.1"),
        "the endpoint was not told who it is: {sent}"
    );
    // One authorization field, so the contractor's cannot ride beside it.
    assert_eq!(
        sent.to_ascii_lowercase().matches("authorization:").count(),
        1,
        "two authorization fields: {sent}"
    );
    // The prompt went, so the ask is the contractor's own.
    assert!(sent.ends_with("{}"), "no body went with it: {sent}");
}

/// A request of no shape is refused with 400 and no endpoint call: the proxy
/// answers only what it can read.
#[test]
fn a_request_of_no_shape_is_refused_and_the_endpoint_never_hears_of_it() {
    let endpoint = Endpoint::open(Plan::json(200, 1, 1));
    let proxy = Proxy::new(endpoint, &[("7", "gpu1", State::Running)]);
    for bad in [
        "",
        // A request line of the shape, of a method the proxy does not serve:
        // the head is a head, so it is the path and the method that refuse it.
        "BREW /v1/chat/completions HTTP/1.1\r\nHost: factory:8090\r\n\r\n",
        "POST chat/completions HTTP/1.1\r\nHost: factory:8090\r\n\r\n",
        "POST /v1/chat/completions\r\nHost: factory:8090\r\n\r\n",
        // A version of no shape the proxy reads.
        "POST /v1/chat/completions 9.9\r\nHost: factory:8090\r\n\r\n",
    ] {
        let (status, _) = proxy.request(bad);
        assert!(
            status == 400 || status == 405,
            "a request of no shape was answered {status}: {bad:?}"
        );
    }
    assert_eq!(proxy.endpoint.calls(), 0, "a bad request reached the model");
    let lines = proxy.said().join("\n");
    assert!(!lines.contains("tok-"), "a refusal of a token: {lines}");
}
