//! The listener: what a contractor's cycle calls, and what it answers.
//!
//! One request, four steps, and each of them owns a rule: read the head, judge
//! it ([`crate::request::judge`]), put one question to the endpoint
//! ([`crate::forward::ask`]), and carry the answer back
//! ([`crate::forward::pipe`]). The order is the rule that costs least first, so
//! a request for a path the proxy does not serve is refused with no token read
//! and no endpoint called, and a dead job is refused before the endpoint is
//! asked anything.
//!
//! The contract is plain HTTP/1.1 with `Connection: close`, one request per
//! connection, which is what Pi's own client does of a provider: no chunked
//! transfer, no keep-alive, no pipeline. The proxy adds nothing to an answer it
//! can carry (rule 7); it answers with a head of its own only where there is
//! nothing to carry, and says so in one log line.

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::Builder;
use std::time::{Duration, Instant};

use crate::channel::{self, Asked, Channel};
use crate::count::{self, Row};
use crate::forward::{self, Answer, Upstream};

use crate::log::{self, Line, Sink};

use crate::request::{self, Ask, Live, Records, Refused};

/// How long a contractor has to send its head. A cycle sends a line and a
/// prompt; a connection that sends neither is cut, which keeps the proxy from
/// holding a socket for a host that will never ask anything.
pub const HEAD_WAIT: Duration = Duration::from_secs(20);
/// How long one read of a contractor's body may take. The body is copied to the
/// endpoint as it arrives, so a prompt that is sent slowly costs the proxy
/// nothing; a connection that stops sending at all is cut at this bound.
pub const BODY_WAIT: Duration = Duration::from_secs(120);
/// How long the proxy's answer may take to be written to the contractor. A
/// contractor that stops reading a stream is cut at this bound, and its job
/// goes to a check-in timeout rather than to a stuck proxy.
pub const WRITE_WAIT: Duration = Duration::from_secs(120);
/// The longest request head, fields included: a prompt is a body and never a
/// field, so a head of more than this is no request a client means.
pub const REQUEST_MAX: usize = 64 * 1024;
/// How many requests the proxy answers at once. A contractor runs one cycle and
/// one stream at a time, so this is a bound on a host's misbehaviour rather than
/// a limit on honest work.
pub const HANDS: usize = 32;

/// What the proxy needs of the factory, given once for the listener: where the
/// requests go, what the records and this process's own tokens say, and where
/// the lines of it all are written. The records are asked on every request, so
/// a job that ended or a contractor revoked mid-work is refused by the next one
/// rather than remembered wrongly from the last.
pub struct Serve<'a> {
    /// The endpoint of the provider the instance named.
    pub endpoint: Upstream,
    /// Its key, already resolved. It goes into one request and is dropped:
    /// never logged, never counted, never given to a contractor.
    pub key: String,
    /// What the factory's records and this process's own tokens say, which is
    /// what a request is judged against and never a field the contractor sent.
    pub records: &'a (dyn Records + Sync),
    pub sink: &'a (dyn Sink + Sync),
    /// How long a model stream may go silent: the instance's own bound, which
    /// `tehas_load` refuses when it is no longer than the check-in timeout.
    pub idle: Duration,
    /// The job channel on the same listener ([`crate::channel`]). None
    /// answers its paths as any path that is not a model path.
    pub channel: Option<&'a Channel<'a>>,
}

/// What one request is worth: whose job it was, the status the contractor was
/// answered with, and what the endpoint said of its cost.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub status: u16,
    /// The job and contractor of the token, None for a request that named no
    /// token this proxy issued.
    pub live: Option<Live>,
    /// The endpoint's own figures, of the bytes the contractor was given. None
    /// when the answer carried none, or when the contractor could not be
    /// written to the end: an answer no one read is no answer to price. The
    /// proxy never guesses a token count.
    pub usage: Option<count::Usage>,
    /// Whether the answer reached the contractor to its end. A stream cut by a
    /// contractor that stopped reading is said in the log, not hidden.
    pub complete: bool,
}

impl Outcome {
    /// The row of the store this request adds: one row per request the endpoint
    /// priced, keyed by contractor and job so #2277 can total them. None when
    /// there is nothing to write, which is every refusal and every answer that
    /// named no figures.
    pub fn row(&self) -> Option<Row> {
        let live = self.live.as_ref()?;
        Some(Row {
            contractor: live.contractor.clone(),
            job: live.job,
            usage: self.usage?,
        })
    }
}

/// One request of one contractor, from the head to the last byte of the answer.
pub fn one(client: &mut TcpStream, serve: &Serve) -> Outcome {
    let began = Instant::now();
    // What the records and this process's tokens say for the request, which is
    // what the rules are judged against, and no further until the next one.
    let read = serve.records;
    let _ = client.set_read_timeout(Some(HEAD_WAIT));
    let _ = client.set_write_timeout(Some(WRITE_WAIT));
    let _ = client.set_nodelay(true);
    let Some((method, path, head, length)) = read_request(client) else {
        serve.sink.line(&Line(NO_REQUEST.to_string()));
        return refuse(client, 400, None);
    };
    let (method, path) = (method.as_str(), path.as_str());
    if let (Some(call), Some(chan)) = (channel::Call::of(path), serve.channel) {
        return channel_one(client, chan, call, method, &head, length, serve);
    }
    let ask = Ask::new(path);
    // The path first, before the token is read: a probe of a path the proxy
    // does not serve is refused whatever it carried (rule 3).
    if let Some(why) = ask.refused() {
        let why = Refused::Path(why);
        serve.sink.line(&Line(why.line(None, None)));
        return refused(client, &why, None);
    }
    let token = request::bearer(&String::from_utf8_lossy(&head));
    let live = match request::judge(token.as_deref(), &ask, read) {
        Ok(live) => live,
        Err(why) => {
            // Who was asked, whether or not the answer is a refusal: the line
            // names the job and the contractor the token belongs to.
            let live = token.as_deref().and_then(|t| read.live(t));
            serve.sink.line(&Line(why.line(
                live.as_ref().map(|l| l.job),
                live.as_ref().map(|l| l.contractor.as_str()),
            )));
            // A request of a token this process issued counts for its job,
            // even a refused one: a contractor refused for the state of its
            // job asked the factory all the same, and the count of rule 5 is
            // of asks, not of answers. The ask is counted once: a token of a
            // job the proxy still holds was already counted by the reading
            // that named who was refused, so only the reading that named no
            // job says it here.
            if live.is_none() {
                if let Some(token) = token.as_deref() {
                    read.asked(token);
                }
            }
            return refused(client, &why, live);
        }
    };
    // The ask of a stream is the one word of the body the proxy reads, and only
    // to know how the answer must be carried back. Nothing else of a body is
    // looked at, kept or written (rule 6).
    let head_end = forward::head_end(&head).unwrap_or(head.len());
    let (fields, body) = head.split_at(head_end);
    let streaming = asks_for_a_stream(body);
    let asked = forward::Ask::new(method, ask.path, fields, body, length, streaming);
    let answer = match forward::ask(&serve.endpoint, &serve.key, serve.idle, &asked, client) {
        Ok(answer) => answer,
        Err(why) => {
            // The endpoint could not be reached, or answered nothing it meant:
            // the one case a 502 of the proxy's own is right. The words of the
            // failure go to the log, and the contractor is told only that the
            // factory could not ask.
            serve.sink.line(&Line(format!(
                "{method} {} 502 job {} contractor {}: {why}",
                log::shown(ask.path),
                live.job,
                live.contractor
            )));
            // The factory was asked and the endpoint was silent, which is a
            // request of the job all the same: the count is of asks. The
            // reading that named the job counted it already, so the ask is
            // said again only when no reading named a job, which is a token
            // of none.
            if token.is_none() {
                read.asked("");
            }
            return refuse(client, 502, Some(live));
        }
    };
    let status = answer.status;
    let (usage, complete) = hand_on(client, answer, serve, &live);
    serve.sink.line(&Line::answered(
        method,
        &log::shown(ask.path),
        status,
        began.elapsed().as_millis(),
        live.job,
        &live.contractor,
    ));
    Outcome {
        status,
        live: Some(live),
        usage,
        complete,
    }
}

/// One call of the job channel: its whole body read, at most
/// [`channel::BODY_MAX`], answered with one JSON object. The log line names
/// the call, the status and whose job or contractor it was, never a body or a
/// token.
fn channel_one(
    client: &mut TcpStream,
    chan: &Channel,
    call: channel::Call,
    method: &str,
    head: &[u8],
    length: usize,
    serve: &Serve,
) -> Outcome {
    let began = Instant::now();
    if method != "POST" || length > channel::BODY_MAX {
        serve.sink.line(&Line(format!(
            "{method} {}{} 400: no call of the job channel",
            channel::PREFIX,
            call.word()
        )));
        return refuse(client, 400, None);
    }
    let end = forward::head_end(head).unwrap_or(head.len());
    let mut body = head[end..].to_vec();
    let _ = client.set_read_timeout(Some(BODY_WAIT));
    let mut chunk = [0u8; 8192];
    while body.len() < length {
        match client.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(length);
    let fields = String::from_utf8_lossy(&head[..end]);
    let bearer = request::bearer(&fields);
    let name = header(&fields, channel::NAME_HEADER);
    let answer = chan.answer(&Asked {
        call,
        bearer: bearer.as_deref(),
        name: name.as_deref(),
        body: &body,
    });
    let live = bearer.as_deref().and_then(|t| chan.issuer.peek(t));
    let who = match (&live, &name) {
        (Some(l), _) => format!("job {} contractor {}", l.job, l.contractor),
        (None, Some(n)) => format!("contractor {}", log::shown(n)),
        _ => "no job".to_string(),
    };
    serve.sink.line(&Line(format!(
        "POST {}{} {} {} ms {who}{}",
        channel::PREFIX,
        call.word(),
        answer.status,
        began.elapsed().as_millis(),
        answer
            .refused()
            .map_or_else(String::new, |w| format!(": refused {w}"))
    )));
    let text = format!("{}\n", answer.body);
    let head = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        answer.status,
        reason(answer.status),
        text.len()
    );
    use std::io::Write;
    let complete = client
        .write_all(head.as_bytes())
        .and_then(|()| client.write_all(text.as_bytes()))
        .and_then(|()| client.flush())
        .is_ok();
    Outcome {
        status: answer.status,
        live,
        usage: None,
        complete,
    }
}

/// The value of one field of a head, by its name in any case.
fn header(fields: &str, name: &str) -> Option<String> {
    fields.lines().skip(1).find_map(|line| {
        let (n, v) = line.split_once(':')?;
        n.trim()
            .eq_ignore_ascii_case(name)
            .then(|| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

/// The endpoint's answer, on the contractor's socket: its head and what came
/// with it first, then the rest as the endpoint sends it. A stream was read no
/// further than its head, so the two cases are carried alike.
fn hand_on(
    client: &mut TcpStream,
    answer: Answer,
    serve: &Serve,
    live: &Live,
) -> (Option<count::Usage>, bool) {
    let Answer {
        mut stream,
        got,
        body,
        ..
    } = answer;
    match forward::pipe(&mut stream, client, &got, body) {
        // Only the tail of a stream is kept, for the endpoint's own figures. A
        // completion of a length was read whole before it was forwarded.
        Ok(tail) => {
            let usage = count::of_answer(&tail).or_else(|| count::of_answer(&got[body..]));
            (usage.with_total(), true)
        }
        Err(e) => {
            // The contractor stopped reading. The figures of an answer the
            // proxy could not carry are not counted.
            serve.sink.line(&Line(format!(
                "job {} contractor {} stopped reading: {e}",
                live.job, live.contractor
            )));
            (None, false)
        }
    }
}

/// A refusal of the proxy's own head, with the status the rule asks for.
fn refused(client: &mut TcpStream, why: &Refused, live: Option<Live>) -> Outcome {
    let status = why.status();
    Outcome {
        status,
        live,
        usage: None,
        complete: refuse_now(client, status).is_ok(),
    }
}

fn refuse(client: &mut TcpStream, status: u16, live: Option<Live>) -> Outcome {
    Outcome {
        status,
        live,
        usage: None,
        complete: refuse_now(client, status).is_ok(),
    }
}

/// The head of the proxy's own, and one short object of text. The words say
/// only what the status says: never a token, never the endpoint, never a body.
fn refuse_now(client: &mut TcpStream, status: u16) -> std::io::Result<()> {
    // The words say what the status says and nothing more: no token, no
    // endpoint, no byte of a body.
    let said = message(status);
    let text = format!("{{\"error\":{{\"type\":\"tehas_proxy\",\"message\":\"{said}\"}}}}\n");
    let head = format!(
        "HTTP/1.1 {status} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-tehas-proxy: {}\r\nconnection: close\r\n\r\n",
        reason(status),
        text.len(),
        message(status)
    );
    use std::io::Write;
    client.write_all(head.as_bytes())?;
    client.write_all(text.as_bytes())?;
    client.flush()
}

/// The one word of each answer of the proxy's own, so a contractor tells a
/// refusal of the proxy from an answer of the endpoint.
fn message(status: u16) -> &'static str {
    match status {
        400 => NO_REQUEST,
        401 => "no token of a job of this factory",
        403 => "no job of this factory",
        404 => "no model path of a cycle",
        502 => "no answer from the model endpoint",
        _ => "refused",
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        429 => "Too Many Requests",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        200 => "OK",
        _ => "Error",
    }
}

/// The request line, the head, and everything of it that arrived: None for a
/// head that never ends, is over the bound, or carries no request line the
/// proxy can read.
fn read_request(client: &mut TcpStream) -> Option<(String, String, Vec<u8>, usize)> {
    let mut head = Vec::new();
    let mut one = [0u8; 1];
    loop {
        if forward::head_end(&head).is_some() {
            break;
        }
        if head.len() > REQUEST_MAX {
            return None;
        }
        match client.read(&mut one) {
            Ok(0) | Err(_) => return None,
            Ok(_) => head.push(one[0]),
        }
    }
    let (method, path) = {
        let (m, p) = request_line(&head)?;
        (m.to_string(), p.to_string())
    };
    // The body, of which the proxy keeps only the part that had already come.
    // The rest goes from the contractor's socket to the endpoint's in
    // [`forward::ask`], as the contractor sends it: a prompt of a cycle is a
    // body of no bound, and a proxy that read it whole before it asked the
    // endpoint would be a proxy that an endpoint which reads its body first
    // never answers.
    let head_end = forward::head_end(&head).unwrap_or(0);
    let want = forward::content_length(&head[..head_end]).unwrap_or(0);
    let _ = client.set_read_timeout(Some(BODY_WAIT));
    let mut chunk = [0u8; 8192];
    while head.len() < head_end + want.min(STORED_BODY) {
        match client.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => head.extend_from_slice(&chunk[..n]),
        }
    }
    Some((method, path, head, want))
}

/// The method and the path of a request line, the only part of a head the proxy
/// reads. The fields go on as they came, less the ones the proxy owns: see
/// [`crate::forward::client_head`]. None is a request of no shape, which is 400
/// and no endpoint call.
fn request_line(head: &[u8]) -> Option<(&str, &str)> {
    let text = std::str::from_utf8(head).ok()?;
    let mut words = text.lines().next()?.split_whitespace();
    let method = words.next()?;
    let path = words.next()?;
    let version = words.next()?;
    if !METHODS.contains(&method)
        || !version.starts_with("HTTP/")
        || !path.starts_with('/')
        || path.bytes().any(|b| !(0x21..=0x7e).contains(&b))
    {
        return None;
    }
    Some((method, path))
}

/// The methods of a model ask. A proxy that forwarded any word would let a
/// contractor ask an endpoint to delete its models.
const METHODS: [&str; 4] = ["GET", "POST", "PUT", "PATCH"];

/// How much of a contractor's body the proxy keeps to itself before it asks the
/// endpoint. A prompt is copied on as it arrives; this is only the part read in
/// the same breath as the head, which is the whole of a small ask and nothing
/// more than this of a large one.
const STORED_BODY: usize = 64 * 1024;

/// Whether what the contractor sent asks for a stream. Only the word of the ask
/// is read; a body that is not JSON asks for nothing, and the endpoint's answer
/// is passed on as it comes either way.
fn asks_for_a_stream(body: &[u8]) -> bool {
    const KEY: &[u8] = b"\"stream\"";
    body.windows(KEY.len())
        .enumerate()
        .filter(|(_, w)| *w == KEY)
        .any(|(i, _)| {
            let rest = &body[i + KEY.len()..];
            let rest = rest.trim_ascii_start();
            rest.strip_prefix(b":")
                .is_some_and(|v| v.trim_ascii_start().starts_with(b"true"))
        })
}

/// An endpoint that gives a total alone still gives the two halves of it, and a
/// row of no figures is the record that the job asked at all.
trait WithTotal {
    fn with_total(self) -> Option<count::Usage>;
}

impl WithTotal for Option<count::Usage> {
    fn with_total(self) -> Option<count::Usage> {
        self.map(|mut u| {
            if u.total == 0 {
                u.total = u.prompt + u.completion;
            }
            u
        })
    }
}

/// The listener: answer until the thread that called this ends, which on a host
/// is the process. A request over [`HANDS`] is ended rather than queued, which
/// is what a cycle retries as it retries a busy model; the count is one atomic
/// and holds no lock over a request.
pub fn listen(serve: &'static Serve<'static>, at: &str) -> Result<TcpListener, String> {
    let listener =
        TcpListener::bind(at).map_err(|e| format!("the model proxy cannot listen on {at}: {e}"))?;
    let hands = Arc::new(AtomicUsize::new(HANDS));
    for accepted in listener.incoming() {
        let Ok(stream) = accepted else { continue };
        let Some(ticket) = Ticket::take(&hands) else {
            drop(stream);
            continue;
        };
        Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || {
                let mut stream = stream;
                one(&mut stream, serve);
                drop(ticket);
            })
            .ok();
    }
    Ok(listener)
}

/// One place among the hands of the listener.
pub struct Ticket(Arc<AtomicUsize>);

impl Ticket {
    pub fn take(hands: &Arc<AtomicUsize>) -> Option<Ticket> {
        hands
            .try_update(Ordering::Acquire, Ordering::Relaxed, |n| n.checked_sub(1))
            .ok()
            .map(|_| Ticket(Arc::clone(hands)))
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Release);
    }
}

/// The sink of a test: the lines the proxy would have written, read back.
pub fn kept(lines: &Arc<std::sync::Mutex<Vec<String>>>) -> impl Sink + Sync {
    let lines = Arc::clone(lines);
    move |line: &Line| {
        lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(line.0.clone())
    }
}

/// The word of an instance that has the feature off: nothing is listening, and
/// the command that would have started it ends 0 having said this.
pub const OFF: &str = "outsource is off: no model proxy is listening";

/// What the proxy says of a request it cannot read at all: no head, no request
/// line, nothing. The contractor is answered 400 with no endpoint call.
pub const NO_REQUEST: &str = "a request of no shape";

/// Why the proxy will not listen, in the words a person reads. The feature being
/// off is the first of them, and it is the case in which nothing is listening,
/// nothing is in `live/`, and `tehas status` says no word about the feature.
pub fn declined(
    on: bool,
    listen: &str,
    endpoint: &str,
    idle: Duration,
    timeout: u64,
) -> Option<String> {
    if !on {
        return Some(OFF.to_string());
    }
    if !listens(listen) {
        return Some(format!(
            "TEHAS_OUTSOURCE_PROXY_ADDR is no address and port to listen on: {listen}"
        ));
    }
    if Upstream::of(endpoint).is_none() {
        return Some(format!(
            "the model endpoint of models.json is no http address: {endpoint}"
        ));
    }
    if idle.as_secs() <= timeout {
        return Some(format!(
            "TEHAS_OUTSOURCE_PROXY_IDLE of {} s is no longer than the check-in timeout of {timeout} s",
            idle.as_secs()
        ));
    }
    None
}

/// Whether a setting names an address and a port to bind: a host or address, a
/// colon, and a port that is no word. The dashboard's own address is refused by
/// the caller, which knows what it listens on.
pub fn listens(value: &str) -> bool {
    let Some((host, port)) = value.rsplit_once(':') else {
        return false;
    };
    !host.is_empty()
        && !host.contains('/')
        && !host.contains(' ')
        && !host.contains('@')
        && port.parse::<u16>().is_ok_and(|p| p != 0)
}

/// What the proxy writes when it starts.
pub fn started(at: &str, idle: Duration) -> String {
    format!(
        "tehas-proxy: listening on {at}; a model stream of {} s silent is dropped",
        idle.as_secs()
    )
}

/// The name of the file a job's token is handed over in. It sits in the job's
/// own directory in the state, is written once, and is read only by the process
/// that starts the contractor's container: #2274 reads it to write the
/// container's `models.json`. The token is never in a command line, a log line
/// or the store.
pub const TOKEN_FILE: &str = "job-token";

/// Write a job's token where the process that starts its container can read it.
/// The directory is 0700 and the file is made 0600, so no other user can read
/// the token at any moment. A token of an earlier attempt is removed first.
pub fn write_token(dir: &std::path::Path, token: &str) -> std::io::Result<std::path::PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let path = dir.join(TOKEN_FILE);
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    file.write_all(token.as_bytes())?;
    Ok(path)
}

/// What the proxy has counted since it was last asked, per contractor and per
/// job (rule 5): the arguments of one store call per row, of no token, no path
/// and no byte of a body.
pub fn request_rows(counts: &[(String, i64, u64)]) -> Vec<Vec<String>> {
    counts
        .iter()
        .map(|(contractor, job, requests)| {
            vec![
                "outsource-requests".to_string(),
                "--contractor".to_string(),
                contractor.clone(),
                "--job".to_string(),
                job.to_string(),
                "--requests".to_string(),
                requests.to_string(),
            ]
        })
        .collect()
}

/// The arguments of the store verb that keeps one answer's figures.
pub fn usage_args(row: &Row) -> Vec<String> {
    count::args(row)
}

/// The proxy's answer to one connection, ended after the one request.
impl Serve<'_> {
    pub fn answer(&self, mut client: TcpStream) -> Outcome {
        one(&mut client, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ask_of_a_stream_is_read_whatever_the_spacing() {
        for body in [
            r#"{"model":"main","stream":true}"#,
            r#"{"model":"main","stream": true}"#,
            r#"{"stream" : true,"model":"main"}"#,
            "{\"stream\":\n  true}",
        ] {
            assert!(asks_for_a_stream(body.as_bytes()), "{body}");
        }
        for body in [
            "",
            "not json",
            r#"{"model":"main"}"#,
            r#"{"stream":false}"#,
            r#"{"stream_options":{"include_usage":true}}"#,
            r#"{"content":"stream true"}"#,
        ] {
            assert!(!asks_for_a_stream(body.as_bytes()), "{body}");
        }
    }
}
