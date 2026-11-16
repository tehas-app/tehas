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
use tehas_proxy::records::Rows;
use tehas_proxy::request::State;
use tehas_proxy::serve::{self, Serve};

mod common;
use common::{prompt, read_some, request, rows_of, token_of, Endpoint, Plan, Records, BIND_WAIT};

/// A free port of this host, for a listener of a case. The listener a case
/// makes is dropped as soon as it answers one request, so the port is free
/// again before the case that asked for it ends.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// The proxy of a case, on a port of its own, answering requests until told to
/// stop. One thread accepts and one answers each request, which is what the
/// unit's process does. The parts the listener needs are made once, as they
/// are when the unit starts: a case that changes what the records say writes
/// them in place, since the proxy reads them again for every request.
struct Proxy {
    at: String,
    issuer: Issuer,
    /// The rows the proxy reads, held by the case too so it can say what they
    /// hold between two requests.
    factory: Records,
    lines: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    busy: Arc<AtomicUsize>,
}

impl Proxy {
    /// A proxy that answers what the stub answers, of the jobs a case lists.
    /// The parts of the unit's listener are the unit's own; the accepting
    /// thread is the test's, since it must be able to give the port back and
    /// `serve::listen` never returns while a proxy runs.
    fn open(endpoint: &Endpoint, jobs: &[(&str, &str, State)]) -> Proxy {
        let (at, backlog) = bind();
        let proxy = Proxy {
            at: at.to_string(),
            issuer: Issuer::new(),
            factory: Records::empty(),
            lines: Arc::new(Mutex::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
            busy: Arc::new(AtomicUsize::new(0)),
        };
        // The rows are the case's and the proxy's to read, and the tokens of
        // those rows are this process's own, as they are on a host where the
        // proxy minted them when the job was placed. The case and the threads
        // that answer hold the one table, so what the case forgets is forgotten
        // by the proxy that is asked.
        proxy.load(rows_of(jobs), token_of);
        let serve: &'static Serve<'static> = Box::leak(Box::new(Serve {
            endpoint: endpoint.upstream(),
            key: "sk-the-factory-s-own-key".to_string(),
            records: Box::leak(Box::new(proxy.factory.clone())),
            sink: Box::leak(Box::new(Kept(Arc::clone(&proxy.lines)))),
            idle: Duration::from_secs(5),
        }));
        // The unit's hands, and one thread to answer each request, which is
        // what `serve::listen` does for the process.
        let hands = Arc::new(AtomicUsize::new(serve::HANDS));
        let (stop, busy) = (Arc::clone(&proxy.stop), Arc::clone(&proxy.busy));
        thread::spawn(move || {
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
                let serve: &'static Serve<'static> = serve;
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

    /// Say what the factory's records hold, to the case and to the proxy that
    /// reads them for every request.
    fn load(&self, rows: Rows, mint: impl Fn(i64) -> String) {
        self.factory.load(rows, mint);
    }

    /// One request on the port, and the answer as the contractor read it.
    fn ask(&self, token: &str, path: &str, body: &str) -> (u16, String) {
        self.raw(&request(token, path, body))
    }

    fn raw(&self, bytes: &str) -> (u16, String) {
        let mut client = TcpStream::connect(&self.at).expect("the proxy's port");
        client.set_nodelay(true).unwrap();
        client.write_all(bytes.as_bytes()).unwrap();
        client.flush().unwrap();
        read_some(client)
    }

    /// The lines the proxy said, in order.
    fn said(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Wait for the requests a case has sent to be answered: the log is what
    /// says so, one line per request.
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
        // The accept thread is holding the port until a connect wakes it; a
        // request still being answered holds it a little longer.
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.at);
        let deadline = Instant::now() + Duration::from_secs(35);
        while self.busy.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// A port of this host, bound and ready to answer: what the unit's listener
/// does for itself, given to the test so the port is the test's to give back.
fn bind() -> (SocketAddr, TcpListener) {
    let backlog = TcpListener::bind("127.0.0.1:0").expect("a port of this host");
    let at = backlog.local_addr().unwrap();
    (at, backlog)
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
    let (status, said) = proxy.ask(&token_of(7), "/v1/chat/completions", &prompt("SRC-MARKER"));
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
    let one = request(&token_of(7), "/v1/chat/completions", "{}");
    let mut client = TcpStream::connect(&proxy.at).unwrap();
    client.write_all(one.as_bytes()).unwrap();
    client.flush().unwrap();
    let (status, _) = read_some(client.try_clone().unwrap());
    assert_eq!(status, 200);

    // The same connection again: the proxy ended it, so the second ask of it
    // is answered with nothing rather than a second forwarding.
    client
        .write_all(one.as_bytes())
        .expect("a socket that is closed for reading still takes bytes");
    let (again, _) = read_some(client);
    assert_eq!(again, 0, "a pipelined request was answered");
    assert_eq!(endpoint.calls(), 1, "the pipeline reached the model");

    // A new connection goes, which is what the agents do anyway.
    let (status, _) = proxy.ask(&token_of(7), "/v1/chat/completions", "{}");
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
        let (status, _) = proxy.ask(&token_of(job), "/v1/chat/completions", "{}");
        assert_eq!(status, 200, "job {job}");
    }
    let (status, _) = proxy.ask("a-token-of-no-job", "/v1/chat/completions", "{}");
    assert_eq!(status, 401);
    proxy.waited_for(4);

    // What the proxy would hand the store, one call per contractor and job.
    let rows = serve::request_rows(&proxy.issuer.take_counts());
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
    assert!(proxy.issuer.take_counts().is_empty(), "a count of twice");
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
    let (status, _) = proxy.ask(&token_of(7), "/v1/chat/completions", "{}");
    assert_eq!(status, 200);

    // The job is over: its token goes, which is what the proxy does when it is
    // told, and the request that carries it is refused without a call.
    eprintln!("holds before: {}", proxy.issuer.holds(&token_of(7)));
    assert!(proxy.issuer.forget(&token_of(7)));
    eprintln!(
        "holds after: {}, rows {:?}",
        proxy.issuer.holds(&token_of(7)),
        proxy
            .factory
            .snapshot()
            .jobs
            .iter()
            .map(|j| (j.job, j.token.clone()))
            .collect::<Vec<_>>()
    );
    let (status, _) = proxy.ask(&token_of(7), "/v1/chat/completions", "{}");
    assert_eq!(status, 401, "a forgotten token was answered");
    // The records say the same, and the log line of the two says which.
    let lines = proxy.waited_for(2);
    assert!(
        lines[1].starts_with("refused ") && lines[1].contains("job 7 of contractor gpu1"),
        "{}\n",
        lines[1]
    );
    // The other job of the same contractor is untouched.
    let (status, _) = proxy.ask(&token_of(8), "/v1/chat/completions", "{}");
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
    assert_eq!(proxy.issuer.forget_contractor("gpu1"), 2);
    assert_eq!(proxy.issuer.forget_jobs(&[9]), 1);
    for job in [7, 8, 9] {
        let (status, _) = proxy.ask(&token_of(job), "/v1/chat/completions", "{}");
        assert_eq!(status, 401, "the token of job {job} outlived its job");
    }
    assert_eq!(endpoint.calls(), 0, "a dead token reached the model");
    let lines = proxy.waited_for(3);
    assert!(lines.iter().all(|l| l.starts_with("refused ")), "{lines:?}");
}

/// A factory whose records read nothing, or whose read fails, forwards nothing.
/// The fail-closed half of the proxy as a service: the state of every instance
/// the day before #2272's tables.
#[test]
fn a_factory_that_reads_nothing_forwards_nothing_to_the_endpoint() {
    let endpoint = Endpoint::open(Plan::json(200, 1, 1));
    let (at, backlog) = bind();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let serve: &'static Serve<'static> = Box::leak(Box::new(Serve {
        endpoint: endpoint.upstream(),
        key: "sk-the-factory-s-own-key".to_string(),
        // The records of an instance the day before #2272: nothing to read, so
        // no contractor may work and no job is going.
        records: Box::leak(Box::new(Records::empty())),
        sink: Box::leak(Box::new(Kept(Arc::clone(&lines)))),
        idle: Duration::from_secs(5),
    }));
    let at = at.to_string();
    thread::spawn(move || {
        for accepted in backlog.incoming() {
            if let Ok(mut stream) = accepted {
                thread::spawn(move || tehas_proxy::serve::one(&mut stream, serve));
            }
        }
    });
    // Give the accepting thread a moment to reach its first accept, so the
    // request below is not the one that finds nobody listening.
    thread::sleep(Duration::from_millis(50));
    let mut client = TcpStream::connect(&at).expect("the port of an empty factory");
    client
        .write_all(request(&token_of(7), "/v1/chat/completions", "{}").as_bytes())
        .unwrap();
    client.flush().unwrap();
    let (status, _) = read_some(client);
    assert_eq!(status, 401, "a model was reached with no records");
    assert_eq!(endpoint.calls(), 0);
    // The refusal was said, and named a job of no records.
    let deadline = Instant::now() + BIND_WAIT;
    while lines
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_empty()
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(20));
    }
    let said = lines
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .join("\n");
    assert!(said.starts_with("refused "), "{said}");
    assert!(!said.contains("tok-"), "a refusal of a token: {said}");
}

/// The lines of the proxy hold no token and no byte of a body, on the port as
/// in the process: what the unit's journal keeps.
#[test]
fn the_journal_of_the_port_holds_no_token_and_no_body() {
    let endpoint = Endpoint::open(Plan::json(200, 5, 6));
    let proxy = Proxy::open(&endpoint, &[("7", "gpu1", State::Running)]);
    proxy.ask(
        &token_of(7),
        "/v1/chat/completions",
        &prompt("SRC-MARKER-ON-THE-PORT"),
    );
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
