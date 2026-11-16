//! The proxy as the unit runs it: a listener on a port of its own, the store
//! calls it makes for the counts of rule 5, and what the feature's settings
//! decide. The rules of one request are `rules.rs`; this file is the port, the
//! counting and the switch, with a stub endpoint behind and no database.

use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use tehas_proxy::jobs::Issuer;
use tehas_proxy::log::{Line, Sink};
use tehas_proxy::request::State;
use tehas_proxy::serve::{self, Serve};

mod common;
use common::{prompt, read_some, request, rows_of, token_of, Endpoint, Plan, Records, BIND_WAIT};

/// The proxy of a case: the unit's own hands, on a port of its own, and the
/// three things a request is judged against: the factory's rows, the tokens of
/// live jobs, and the journal of the lines.
///
/// The rows name the token of each job, as a test mints it, and the proxy is
/// the one holder of the tokens: the two are made together, in one table, which
/// is what a revocation and the end of a job empty. A case asks the proxy
/// through [`Proxy::job`] and [`Proxy::ask`], which is the contractor's side of
/// the port.
struct Proxy {
    at: String,
    /// The tokens the proxy answers with, and the count of the requests that
    /// carried them. A case takes one back for the job that ended.
    issuer: Issuer,
    /// The rows the proxy reads, held by the case too so it can say what they
    /// hold between two requests.
    factory: Records,
    /// The journal, where the case reads back the one line of each request.
    lines: Arc<Mutex<Vec<String>>>,
    /// How many requests of the case the proxy holds at the moment: the ones it
    /// has taken in and not yet answered. The case ends only when it is none, so
    /// no case leaves a request of its own being answered behind it.
    busy: Arc<AtomicUsize>,
    /// Whether the thread that accepts is to end. It ends with the backlog it
    /// holds, and the port goes with the backlog.
    stop: Arc<AtomicBool>,
}

impl Proxy {
    /// A proxy that answers what the stub answers, of the jobs a case lists.
    /// The port is bound before the thread that accepts starts, so the first
    /// request of a case finds the accept loop in its accept rather than a port
    /// nobody listens on yet.
    fn open(endpoint: &Endpoint, jobs: &[(&str, &str, State)]) -> Proxy {
        let (at, backlog) = bind();
        let issuer = Issuer::new();
        let rows = rows_of(jobs, token_of);
        for row in &rows.jobs {
            issuer.put(&row.token, row.job, row.attempt, &row.contractor);
        }
        let proxy = Proxy {
            at: at.to_string(),
            issuer: issuer.clone(),
            factory: Records::of(rows, &issuer),
            lines: Arc::new(Mutex::new(Vec::new())),
            busy: Arc::new(AtomicUsize::new(0)),
            stop: Arc::new(AtomicBool::new(false)),
        };
        let serve: &'static Serve<'static> = Box::leak(Box::new(Serve {
            endpoint: endpoint.upstream(),
            key: "sk-the-factory-s-own-key".to_string(),
            records: Box::leak(Box::new(proxy.factory.clone())),
            sink: Box::leak(Box::new(Kept(Arc::clone(&proxy.lines)))),
            idle: Duration::from_secs(5),
        }));
        // The unit's hands, the same bound of streams and the same one request
        // per connection, on a thread of the case's. The case's Drop ends the
        // thread, which ends the accept loop with the backlog it holds and gives
        // the port back to the next case of the suite.
        let (busy, stop) = (Arc::clone(&proxy.busy), Arc::clone(&proxy.stop));
        thread::spawn(move || {
            let hands = Arc::new(AtomicUsize::new(serve::HANDS));
            for accepted in backlog.incoming() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = accepted else { continue };
                let Some(ticket) = serve::Ticket::take(&hands) else {
                    drop(stream);
                    continue;
                };
                busy.fetch_add(1, Ordering::SeqCst);
                let busy = Arc::clone(&busy);
                thread::Builder::new()
                    .stack_size(256 * 1024)
                    .spawn(move || {
                        let mut stream = stream;
                        tehas_proxy::serve::one(&mut stream, serve);
                        drop(ticket);
                        busy.fetch_sub(1, Ordering::SeqCst);
                    })
                    .ok();
            }
        });
        proxy
    }

    /// What the proxy has counted of the requests it answered, per contractor
    /// and per job: what the store would be given, and what a case waits on.
    fn counted(&self) -> Vec<(String, i64, u64)> {
        self.issuer.take_counts()
    }

    /// The token of a job of the case: the value the job's row names and the
    /// proxy's table holds, which is what the contractor was given for the job.
    fn token(&self, job: i64) -> String {
        self.factory
            .snapshot()
            .jobs
            .iter()
            .find(|j| j.job == job)
            .map(|j| j.token.clone())
            .unwrap_or_else(|| panic!("job {job} of the case's records"))
    }

    /// One request on the port, and the answer as the contractor read it.
    fn ask(&self, token: &str, path: &str, body: &str) -> (u16, String) {
        self.raw(&request(token, path, body))
    }

    /// One request of one job of the case, by its number.
    fn job(&self, job: i64, path: &str, body: &str) -> (u16, String) {
        self.ask(&self.token(job), path, body)
    }

    fn raw(&self, bytes: &str) -> (u16, String) {
        self.busy.fetch_add(1, Ordering::SeqCst);
        let said = ask_of_the_port(&self.at, bytes);
        self.busy.fetch_sub(1, Ordering::SeqCst);
        said
    }

    /// The lines the proxy said, in order.
    fn said(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Wait until the proxy has said `of` lines, which is one per request it has
    /// finished, and give back what it had said then.
    fn waited_for(&self, of: usize) -> Vec<String> {
        let deadline = Instant::now() + BIND_WAIT;
        while self.said().len() < of && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        self.said()
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        // The requests of the case are answered before the case ends, and then
        // the thread that accepts is ended with the backlog it holds, which
        // gives the port back to the next case of the suite.
        let deadline = Instant::now() + Duration::from_secs(35);
        while self.busy.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        self.stop.store(true, Ordering::SeqCst);
        // The accept loop ends only when a connect wakes it out of its accept.
        let _ = TcpStream::connect(&self.at);
    }
}

/// One thing to the port of a case: the answer read to its end, and the socket
/// of the ask closed with it. The proxy answers one request per connection, so a
/// socket a case kept open would hold a request of nobody.
fn ask_of_the_port(at: &str, bytes: &str) -> (u16, String) {
    let client = TcpStream::connect(at).expect("the proxy's port");
    client.set_nodelay(true).unwrap();
    let mut writer = client.try_clone().expect("a second handle of the socket");
    writer.write_all(bytes.as_bytes()).unwrap();
    writer.flush().unwrap();
    drop(writer);
    read_some(client)
}

/// A port of this host, bound and ready to answer: what the unit's listener
/// does for itself, given to the test so the port is the test's to give back.
fn bind() -> (SocketAddr, TcpListener) {
    let backlog = TcpListener::bind("127.0.0.1:0").expect("a port of this host");
    let at = backlog.local_addr().unwrap();
    (at, backlog)
}

/// A port of this host that nothing listens on, for the case of the feature
/// that is off: a port a case did not ask for, free to bind and dead to connect.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// The sink of the unit's lines, kept where the case reads them back.
struct Kept(Arc<Mutex<Vec<String>>>);

impl Sink for Kept {
    fn line(&self, line: &Line) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line.0.clone());
    }
}

// --- the listener ------------------------------------------------------------

/// The proxy as a service: a port of its own, one request answered and closed,
/// the whole of the prompt reaching the endpoint and the whole of its answer
/// reaching the contractor, and one line of the journal for it.
#[test]
fn a_request_on_the_port_is_answered_and_the_prompt_reaches_the_endpoint() {
    let endpoint = Endpoint::open(Plan::json(200, 12, 30));
    let proxy = Proxy::open(&endpoint, &[("7", "gpu1", State::Running)]);
    let (status, said) = proxy.job(7, "/v1/chat/completions", &prompt("SRC-MARKER"));
    assert_eq!(status, 200, "the answer was lost: {said}");
    assert!(
        said.contains("the answer"),
        "the endpoint's answer did not reach the contractor: {said}"
    );
    assert!(
        said.to_ascii_lowercase().contains("connection: close"),
        "a listener that keeps a contractor: {said}"
    );
    let asked = endpoint.asked_text();
    assert!(
        asked.contains("SRC-MARKER"),
        "the prompt never reached the model: {asked}"
    );
    assert!(
        asked.contains("Authorization: Bearer sk-the-factory-s-own-key"),
        "the endpoint was not given its own key: {asked}"
    );
    let lines = proxy.waited_for(1);
    assert_eq!(lines.len(), 1, "one line for one answer: {lines:?}");
    assert!(
        lines[0].starts_with("POST /v1/chat/completions 200 ")
            && lines[0].ends_with("ms job 7 contractor gpu1"),
        "{}\n",
        lines[0]
    );
}

/// A second request on a new connection goes, so a cycle's agents may ask as
/// often as they like; a second request on the connection the proxy already
/// answered does not, since it ends each one (`Connection: close`).
#[test]
fn a_connection_answers_one_request_and_a_new_connection_answers_the_next() {
    let endpoint = Endpoint::open(Plan::json(200, 1, 1));
    let proxy = Proxy::open(&endpoint, &[("7", "gpu1", State::Running)]);
    let one = request(&proxy.token(7), "/v1/chat/completions", "{}");
    let client = TcpStream::connect(&proxy.at).unwrap();
    client.set_nodelay(true).unwrap();
    let mut writer = client.try_clone().unwrap();
    writer.write_all(one.as_bytes()).unwrap();
    writer.flush().unwrap();
    let (status, _) = read_some(client.try_clone().unwrap());
    assert_eq!(status, 200);

    // The same connection again: the proxy ended it, so the second ask of it
    // is answered with nothing rather than a second forwarding. The socket of
    // the ask is closed with the answer, as a contractor's is.
    writer
        .write_all(one.as_bytes())
        .expect("a socket that is closed for reading still takes bytes");
    writer.flush().unwrap();
    let (again, _) = read_some(client);
    assert_eq!(again, 0, "a pipelined request was answered");
    assert_eq!(endpoint.calls(), 1, "the pipeline reached the model");

    // A new connection goes, which is what the agents do anyway.
    let (status, _) = proxy.job(7, "/v1/chat/completions", "{}");
    assert_eq!(status, 200);
    assert_eq!(endpoint.calls(), 2);
}

/// Rule 5, on the port: the requests the proxy answered are counted per
/// contractor and per job, and the arguments of the store call hold no token,
/// no path and no byte of a body. The store's own rows are #2277's.
#[test]
fn the_requests_of_the_port_are_counted_per_contractor_and_job() {
    let endpoint = Endpoint::open(Plan::json(200, 3, 4));
    let proxy = Proxy::open(
        &endpoint,
        &[
            ("7", "gpu1", State::Running),
            ("8", "gpu1", State::Running),
            ("9", "gpu2", State::Running),
        ],
    );
    for job in [7, 7, 8] {
        let (status, _) = proxy.job(job, "/v1/chat/completions", "{}");
        assert_eq!(status, 200, "job {job}");
    }
    let (status, _) = proxy.ask("a-token-of-no-job", "/v1/chat/completions", "{}");
    assert_eq!(status, 401);
    proxy.waited_for(4);

    // What the proxy would hand the store, one call per contractor and job.
    let rows = serve::request_rows(&proxy.counted());
    let said = rows
        .iter()
        .map(|r| r.join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(said.contains("outsource-requests"), "{said}");
    assert!(
        said.contains("--contractor gpu1 --job 7 --requests 2"),
        "{said}"
    );
    assert!(
        said.contains("--contractor gpu1 --job 8 --requests 1"),
        "{said}"
    );
    assert!(
        !said.contains("gpu2"),
        "a refusal of no job counted as a job of gpu2: {said}"
    );
    assert!(!said.contains("tok-"), "a row of a token: {said}");
    assert!(!said.contains("/v1"), "a row of a path: {said}");
    // The counts were spent, so the next call starts from nothing.
    assert!(proxy.counted().is_empty(), "a count of twice");
}

/// The feature off: no listener is made, nothing is bound, and the word that
/// says why is what the command gives back. `tehas-proxy` prints this and ends
/// 0, so a unit with the switch off is not a failing unit.
#[test]
fn the_feature_off_makes_no_listener_and_says_the_word_that_says_why() {
    let why = serve::declined(
        false,
        "127.0.0.1:8090",
        "http://127.0.0.1:3000/v1",
        Duration::from_secs(3600),
        900,
    )
    .expect("an instance with the feature off is declined");
    assert_eq!(why, serve::OFF, "the word is not the one of the manual");
    assert!(why.contains("off"), "{why}");
    assert!(why.contains("no model proxy is listening"), "{why}");
    // Nothing was bound, so a port a case did not ask for is free to bind, and
    // answers nothing until it is.
    let at = format!("127.0.0.1:{}", free_port());
    assert!(
        TcpStream::connect(&at).is_err(),
        "a port the off feature bound"
    );
    drop(TcpListener::bind(&at).expect("the port of an off feature"));
}

/// Rule 2 and the rest of the settings the proxy will not start on, each in
/// the words of the setting that is wrong. The idle bound of a stream is the
/// one rule: longer than the check-in timeout, or the proxy refuses to start.
#[test]
fn a_setting_the_proxy_will_not_start_on_is_named_by_its_own_name() {
    let idle = Duration::from_secs(3600);
    // The one case that is not a refusal.
    assert!(serve::declined(
        true,
        "127.0.0.1:8090",
        "http://127.0.0.1:3000/v1",
        idle,
        900
    )
    .is_none());
    for (listen, want) in [
        ("127.0.0.1", "TEHAS_OUTSOURCE_ADDR"),
        ("", "TEHAS_OUTSOURCE_ADDR"),
        ("http://127.0.0.1:8090", "TEHAS_OUTSOURCE_ADDR"),
        ("127.0.0.1:0", "TEHAS_OUTSOURCE_ADDR"),
        (":8090", "TEHAS_OUTSOURCE_ADDR"),
    ] {
        let why = serve::declined(true, listen, "http://127.0.0.1:3000/v1", idle, 900)
            .unwrap_or_else(|| panic!("{listen} was taken as a port"));
        assert!(why.contains(want), "{listen}: {why}");
    }
    for endpoint in ["", "https://models.example/v1", "127.0.0.1:3000", "http:/x"] {
        let why = serve::declined(true, "127.0.0.1:8090", endpoint, idle, 900)
            .unwrap_or_else(|| panic!("{endpoint} was taken as the endpoint"));
        assert!(why.contains("no http address"), "{endpoint}: {why}");
    }
    // The idle bound, and the check-in timeout it must outlast.
    for (idle, timeout) in [
        (Duration::from_secs(900), 900),
        (Duration::from_secs(60), 900),
        (Duration::ZERO, 900),
        (Duration::from_secs(30), 30),
    ] {
        let why = serve::declined(
            true,
            "127.0.0.1:8090",
            "http://127.0.0.1:3000/v1",
            idle,
            timeout,
        )
        .unwrap_or_else(|| panic!("an idle of {} s was taken", idle.as_secs()));
        assert!(why.contains("TEHAS_OUTSOURCE_IDLE"), "{why}");
        assert!(why.contains("900") || why.contains("30"), "{why}");
    }
    // A bound one second longer than the timeout is the least that is allowed.
    assert!(serve::declined(
        true,
        "127.0.0.1:8090",
        "http://127.0.0.1:3000/v1",
        Duration::from_secs(901),
        900
    )
    .is_none());
}

/// What the proxy says when it starts, and what `tehas status` shows of it:
/// the port and the bound, and no word about the model it forwards to.
#[test]
fn what_the_proxy_says_of_itself_names_the_port_and_the_bound_and_no_endpoint() {
    let said = serve::started("127.0.0.1:8090", Duration::from_secs(3600));
    assert!(said.contains("127.0.0.1:8090"), "{said}");
    assert!(said.contains("3600"), "{said}");
    assert!(!said.contains("models.json"), "{said}");
    assert!(!said.contains("http://"), "{said}");
}

/// A job's token is handed over in a file of the job's own directory, of mode
/// 0600, and is in no command line: #2274 reads that file to write the
/// container's `models.json`.
#[test]
fn a_job_s_token_goes_to_a_file_of_the_job_s_own_directory() {
    let dir = std::env::temp_dir().join(format!("tehas-proxy-token-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = serve::write_token(&dir.join("job-7"), "tok-7").expect("the token file");
    assert_eq!(path.file_name().unwrap(), serve::TOKEN_FILE);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "tok-7");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600,
        "a token file another user can read"
    );
    // A directory of nothing else, and no token anywhere but that file.
    let listed: Vec<String> = std::fs::read_dir(dir.join("job-7"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(listed, vec![serve::TOKEN_FILE.to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A token dies with its job: a request that carried it a moment ago is refused
/// once the job ends, and one of another job of the same contractor goes on.
#[test]
fn a_token_forgotten_is_refused_from_then_on() {
    let endpoint = Endpoint::open(Plan::json(200, 1, 1));
    let proxy = Proxy::open(
        &endpoint,
        &[("7", "gpu1", State::Running), ("8", "gpu1", State::Running)],
    );
    let (status, _) = proxy.job(7, "/v1/chat/completions", "{}");
    assert_eq!(status, 200);

    // The job is over: the token it was given goes, which is what the proxy is
    // told to do when a job ends. The proxy is the only holder of the token, so
    // forgetting it at the proxy is what ends it: the factory's rows still name
    // it, and a token is never looked up from what a contractor sends.
    let token = proxy.token(7);
    assert!(
        proxy.issuer.holds(&token),
        "the proxy never held its job's token"
    );
    assert!(proxy.issuer.forget(&token));
    assert!(!proxy.issuer.holds(&token), "a forgotten token is held");
    assert!(
        proxy
            .factory
            .snapshot()
            .jobs
            .iter()
            .any(|j| j.job == 7 && j.token == token),
        "the refused token is not the one the job's row names"
    );
    let (status, _) = proxy.ask(&token, "/v1/chat/completions", "{}");
    assert_eq!(status, 401, "a forgotten token was answered");
    // Each of the two leaves one line, and the second says why. The second
    // request is answered before the case opens a third connection, so the two
    // lines are there when the order of them is read.
    let lines = proxy.waited_for(2);
    assert!(
        lines[0].starts_with("POST "),
        "the line of the answered request: {}\n",
        lines[0]
    );
    assert!(
        lines[1].starts_with("refused ")
            && lines[1].contains("a token this factory issued for no job"),
        "{}\n",
        lines[1]
    );
    // The other job of the same contractor is untouched.
    let (status, _) = proxy.job(8, "/v1/chat/completions", "{}");
    assert_eq!(status, 200);
    assert_eq!(endpoint.calls(), 2, "a refusal called the endpoint");
}

/// Every token of a contractor goes at once when it is revoked, and every
/// token of a job that ended goes with it: what the proxy is told to forget.
#[test]
fn a_revoked_contractor_s_tokens_all_go_at_once() {
    let endpoint = Endpoint::open(Plan::json(200, 1, 1));
    let proxy = Proxy::open(
        &endpoint,
        &[
            ("7", "gpu1", State::Running),
            ("8", "gpu1", State::Running),
            ("9", "gpu2", State::Running),
        ],
    );
    // The proxy holds the tokens of the live jobs before it is told: the rows
    // and the tokens were made together when the case opened the proxy.
    for job in [7, 8, 9] {
        assert!(proxy.issuer.holds(&proxy.token(job)), "job {job}");
    }
    assert_eq!(proxy.issuer.forget_contractor("gpu1"), 2);
    assert_eq!(proxy.issuer.forget_jobs(&[9]), 1);
    assert!(proxy.issuer.is_empty(), "a token of the three left");
    for job in [7, 8, 9] {
        let (status, _) = proxy.job(job, "/v1/chat/completions", "{}");
        assert_eq!(status, 401, "the token of job {job} outlived its job");
    }
    assert_eq!(endpoint.calls(), 0, "a dead token reached the model");
    let lines = proxy.waited_for(3);
    assert!(lines.iter().all(|l| l.starts_with("refused ")), "{lines:?}");
}

/// A factory whose records read nothing, or whose read fails, forwards nothing.
/// The fail-closed half of the proxy as a service: the state of every instance
/// whose store cannot be read, or has the feature off.
#[test]
fn a_factory_that_reads_nothing_forwards_nothing_to_the_endpoint() {
    let endpoint = Endpoint::open(Plan::json(200, 1, 1));
    // The proxy of a factory with no outsource tables, on the unit's own
    // listener: there are rows to read and they say nothing, which is the state
    // of a store that cannot be read or has the feature off.
    let proxy = Proxy::open(&endpoint, &[]);
    let (status, _) = proxy.ask("a-token-of-no-job", "/v1/chat/completions", "{}");
    assert_eq!(status, 401, "a model was reached with no records");
    assert_eq!(endpoint.calls(), 0);
    // The refusal was said, and named no token: the ask of a token of no job is
    // the one thing that was said of it, and it was said once.
    let said = proxy.waited_for(1).join("\n");
    assert!(said.starts_with("refused "), "{said}");
    assert!(!said.contains("tok-"), "a refusal of a token: {said}");
}

/// The lines of the proxy hold no token and no byte of a body, on the port as
/// in the process: what the unit's journal keeps.
#[test]
fn the_journal_of_the_port_holds_no_token_and_no_body() {
    let endpoint = Endpoint::open(Plan::json(200, 5, 6));
    let proxy = Proxy::open(&endpoint, &[("7", "gpu1", State::Running)]);
    proxy.job(7, "/v1/chat/completions", &prompt("SRC-MARKER-ON-THE-PORT"));
    proxy.ask("a-token-of-no-job", "/v1/embeddings", "{}");
    let lines = proxy.waited_for(2);
    for line in &lines {
        assert!(
            !line.contains("SRC-MARKER"),
            "a body in the journal: {line}"
        );
        assert!(!line.contains("tok-"), "a token in the journal: {line}");
        assert!(
            !line.contains("sk-the-factory"),
            "the endpoint's key in the journal: {line}"
        );
        assert!(!line.contains('\n'), "a line that is two: {line:?}");
    }
}
