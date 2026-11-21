//! The socket against a stub endpoint on 127.0.0.1, through the real curl,
//! and a client over the unix socket.

use super::*;
use crate::day::{FileDay, MemoryDay};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use tehas_core::testdir::TestDir;

const ID: &str = "plugin-model";
const OWN: &str = "endpoint-model";

fn fixed() -> i64 {
    20_000
}

/// What the stub endpoint was sent.
#[derive(Debug, Clone)]
struct Seen {
    head: String,
    body: Vec<u8>,
}

impl Seen {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

type Reply = Box<dyn Fn() -> (u16, Vec<u8>) + Send + Sync>;

struct Stub {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    most: Arc<AtomicUsize>,
}

impl Stub {
    fn seen(&self) -> Vec<Seen> {
        lock(&self.seen).clone()
    }
}

fn completion(usage: bool) -> Value {
    let mut v = json!({
        "id": "c1",
        "object": "chat.completion",
        "model": OWN,
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": "ok" }, "finish_reason": "stop" }]
    });
    if usage {
        v["usage"] = json!({ "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15 });
    }
    v
}

fn answers(v: Value) -> Reply {
    let body = v.to_string().into_bytes();
    Box::new(move || (200, body.clone()))
}

/// An endpoint that answers every request with `reply`, after `delay`.
fn stub(reply: Reply, delay: Duration) -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let most = Arc::new(AtomicUsize::new(0));
    let now = Arc::new(AtomicUsize::new(0));
    let reply = Arc::new(reply);
    let (s, m) = (Arc::clone(&seen), Arc::clone(&most));
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { continue };
            let (seen, most, now, reply) = (
                Arc::clone(&s),
                Arc::clone(&m),
                Arc::clone(&now),
                Arc::clone(&reply),
            );
            std::thread::spawn(move || {
                let n = now.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(n, Ordering::SeqCst);
                let got = read_request(&mut conn);
                lock(&seen).push(got);
                std::thread::sleep(delay);
                let (status, body) = reply();
                now.fetch_sub(1, Ordering::SeqCst);
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = conn.write_all(head.as_bytes());
                let _ = conn.write_all(&body);
            });
        }
    });
    Stub { url, seen, most }
}

fn read_request(conn: &mut TcpStream) -> Seen {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 65536];
    let end = loop {
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break end;
        }
        let n = conn.read(&mut chunk).unwrap();
        assert!(n > 0, "the request ended in its head");
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8(buf[..end].to_vec()).unwrap();
    let len: usize = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse().unwrap())
        })
        .unwrap_or(0);
    let mut body = buf[end + 4..].to_vec();
    while body.len() < len {
        let n = conn.read(&mut chunk).unwrap();
        assert!(n > 0);
        body.extend_from_slice(&chunk[..n]);
    }
    Seen { head, body }
}

fn run(url: &str) -> Run {
    Run {
        id: ID.into(),
        tokens: 10_000_000,
        day_limit: 100_000_000,
        day: Arc::new(MemoryDay::new(fixed)),
        upstream: Upstream {
            url: url.into(),
            key: "unused".into(),
            model: OWN.into(),
        },
        curl: "curl".into(),
        content: Content::Text,
    }
}

/// The run's directory: a test directory, or one under /tmp when that path
/// is too long for a socket (108 bytes of `sun_path`).
enum SockDir {
    Test(TestDir),
    Short(PathBuf),
}

impl SockDir {
    fn new() -> SockDir {
        let t = TestDir::new("sock");
        if t.join(SOCKET).as_os_str().len() < 100 {
            return SockDir::Test(t);
        }
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let p = PathBuf::from(format!("/tmp/tehas-sock-{}-{n}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        SockDir::Short(p)
    }

    fn path(&self) -> &Path {
        match self {
            SockDir::Test(t) => t.path(),
            SockDir::Short(p) => p,
        }
    }
}

impl Drop for SockDir {
    fn drop(&mut self) {
        if let SockDir::Short(p) = self {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

fn start(run: Run) -> (SockDir, Server) {
    let dir = SockDir::new();
    let server = Server::start(dir.path(), run).unwrap();
    (dir, server)
}

/// The status and JSON of the answer to `bytes`, sent whole.
fn raw(server: &Server, bytes: &[u8]) -> (u16, Value) {
    let mut c = UnixStream::connect(server.path()).unwrap();
    c.write_all(bytes).unwrap();
    let mut got = Vec::new();
    c.read_to_end(&mut got).unwrap();
    let end = got.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(got[..end].to_vec()).unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    let body = serde_json::from_slice(&got[end + 4..]).unwrap_or(Value::Null);
    (status, body)
}

fn call(server: &Server, method: &str, path: &str, body: &[u8]) -> (u16, Value) {
    let mut bytes = format!(
        "{method} {path} HTTP/1.1\r\nHost: tehas\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    raw(server, &bytes)
}

fn chat(server: &Server, req: &Value) -> (u16, Value) {
    call(
        server,
        "POST",
        "/v1/chat/completions",
        req.to_string().as_bytes(),
    )
}

fn ask() -> Value {
    json!({
        "model": ID,
        "max_tokens": 100,
        "messages": [{ "role": "user", "content": "hello" }]
    })
}

fn with(field: &str, value: Value) -> Value {
    let mut r = ask();
    r[field] = value;
    r
}

fn says(answer: &(u16, Value)) -> String {
    answer.1["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// The reserve of [`ask`]: the bytes sent, the endpoint's model in them.
fn reserve() -> u64 {
    with("model", json!(OWN)).to_string().len() as u64 / 3 + 100
}

fn reserve_of(seen: &Seen) -> u64 {
    seen.body.len() as u64 / 3 + seen.json()["max_tokens"].as_u64().unwrap()
}

#[test]
fn the_socket_is_open_to_any_uid_and_gone_when_stopped() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let path = server.path().to_path_buf();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o666);
    assert_eq!(chat(&server, &ask()).0, 200);
    assert_eq!(
        server.stop(),
        Counts {
            requests: 1,
            tokens: 15
        }
    );
    assert!(!path.exists());
    assert!(UnixStream::connect(&path).is_err());
}

#[test]
fn the_models_list_names_the_context_s_id_alone() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let (status, body) = call(&server, "GET", "/v1/models", b"");
    assert_eq!(status, 200);
    let ids: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [ID]);
    assert!(up.seen().is_empty());
}

#[test]
fn any_other_path_or_method_is_404_and_not_sent() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let body = ask().to_string();
    for (method, path) in [
        ("GET", "/"),
        ("GET", "/v1/chat/completions"),
        ("POST", "/v1/models"),
        ("POST", "/v1/completions"),
        ("POST", "/v1/embeddings"),
        ("POST", "/v1/chat/completions/"),
        ("POST", "/v1/chat/completions?x=1"),
        ("GET", "/v1/models?x=1"),
        ("DELETE", "/v1/models"),
    ] {
        assert_eq!(
            call(&server, method, path, body.as_bytes()).0,
            404,
            "{method} {path}"
        );
    }
    assert!(up.seen().is_empty());
}

#[test]
fn a_request_goes_to_the_endpoint_as_its_own_model_with_the_key_from_the_environment() {
    std::env::set_var("TEHAS_MODEL_KEY_PLUGINSOCKET", "the-key");
    let up = stub(answers(completion(true)), Duration::ZERO);
    let mut r = run(&up.url);
    r.upstream.key = "$TEHAS_MODEL_KEY_PLUGINSOCKET".into();
    let (_dir, server) = start(r);
    let (status, body) = chat(&server, &ask());
    assert_eq!(status, 200);
    assert_eq!(body, completion(true));
    let seen = up.seen();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0]
            .head
            .starts_with("POST /v1/chat/completions HTTP/1.1\r\n"),
        "{}",
        seen[0].head
    );
    assert!(seen[0].head.contains("Authorization: Bearer the-key\r\n"));
    assert!(seen[0].head.contains("Content-Type: application/json\r\n"));
    assert_eq!(
        seen[0].json(),
        json!({ "model": OWN, "max_tokens": 100, "messages": [{ "role": "user", "content": "hello" }] })
    );
}

#[test]
fn a_word_key_sends_no_authorization() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    assert_eq!(chat(&server, &ask()).0, 200);
    assert!(!up.seen()[0]
        .head
        .to_ascii_lowercase()
        .contains("authorization"));
}

#[test]
fn the_body_must_be_a_json_object() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    for body in [&b"not json"[..], b"[]", b"\"text\"", b""] {
        assert_eq!(call(&server, "POST", "/v1/chat/completions", body).0, 400);
    }
    assert!(up.seen().is_empty());
}

#[test]
fn only_the_allowed_fields_pass() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    for field in [
        "user",
        "logprobs",
        "frequency_penalty",
        "logit_bias",
        "stream_options",
        "functions",
        "Model",
    ] {
        let a = chat(&server, &with(field, json!(1)));
        assert_eq!(a.0, 400, "{field}");
        assert!(says(&a).contains("not allowed"), "{field}");
    }
    assert!(up.seen().is_empty());
    let all = json!({
        "model": ID,
        "max_tokens": 10,
        "messages": [{ "role": "user", "content": "hello" }],
        "temperature": 0.2,
        "top_p": 1,
        "stop": ["\n\n"],
        "tools": [{ "type": "function", "function": { "name": "read_file", "parameters": {} } }],
        "tool_choice": "auto",
        "response_format": { "type": "json_object" },
        "seed": 7
    });
    assert_eq!(chat(&server, &all).0, 200);
    let mut sent = up.seen()[0].json();
    assert_eq!(sent["model"], OWN);
    sent["model"] = json!(ID);
    assert_eq!(sent, all);
}

#[test]
fn an_allowed_field_of_the_wrong_kind_is_400() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    for (field, value) in [
        ("temperature", json!("hot")),
        ("top_p", json!(null)),
        ("stop", json!([1])),
        ("tools", json!({})),
        ("tools", json!(["read_file"])),
        ("tool_choice", json!(1)),
        ("response_format", json!("json")),
        ("seed", json!(1.5)),
    ] {
        assert_eq!(
            chat(&server, &with(field, value.clone())).0,
            400,
            "{field} {value}"
        );
    }
    assert!(up.seen().is_empty());
}

#[test]
fn a_stream_or_more_than_one_choice_is_400_and_their_defaults_are_dropped() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    for (field, value) in [
        ("stream", json!(true)),
        ("stream", json!("no")),
        ("n", json!(2)),
        ("n", json!(0)),
    ] {
        assert_eq!(
            chat(&server, &with(field, value.clone())).0,
            400,
            "{field} {value}"
        );
    }
    assert!(up.seen().is_empty());
    let mut r = with("stream", json!(false));
    r["n"] = json!(1);
    assert_eq!(chat(&server, &r).0, 200);
    let sent = up.seen()[0].json();
    assert!(
        sent.get("stream").is_none() && sent.get("n").is_none(),
        "{sent}"
    );
}

#[test]
fn the_model_must_be_the_context_s_id() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    for model in [json!(OWN), json!("other"), json!(null), json!(1)] {
        let a = chat(&server, &with("model", model.clone()));
        assert_eq!(a.0, 400, "{model}");
        assert_eq!(says(&a), format!("model must be {ID}"));
    }
    let mut r = ask();
    r.as_object_mut().unwrap().remove("model");
    assert_eq!(chat(&server, &r).0, 400);
    assert!(up.seen().is_empty());
}

#[test]
fn max_tokens_is_required_and_at_most_16384() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    for n in [
        json!(0),
        json!(16_385),
        json!("100"),
        json!(1.5),
        json!(-1),
        json!(null),
    ] {
        assert_eq!(chat(&server, &with("max_tokens", n.clone())).0, 400, "{n}");
    }
    let mut r = ask();
    r.as_object_mut().unwrap().remove("max_tokens");
    assert_eq!(chat(&server, &r).0, 400);
    assert!(up.seen().is_empty());
    assert_eq!(chat(&server, &with("max_tokens", json!(16_384))).0, 200);
}

#[test]
fn a_message_is_text_and_the_fields_of_a_tool_turn() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let call_ = json!({ "id": "c1", "type": "function", "function": { "name": "read_file", "arguments": "{}" } });
    let good = [
        json!([{ "role": "system", "content": "be brief" }, { "role": "user", "content": "hi" }]),
        json!([{ "role": "user", "content": [{ "type": "text", "text": "hi" }] }]),
        json!([
            { "role": "user", "content": "read it" },
            { "role": "assistant", "content": null, "tool_calls": [call_] },
            { "role": "tool", "tool_call_id": "c1", "name": "read_file", "content": "text" }
        ]),
        json!([{ "role": "assistant", "tool_calls": [call_] }]),
        json!([{ "role": "assistant", "content": "done", "tool_calls": [call_] }]),
    ];
    for messages in &good {
        assert_eq!(
            chat(&server, &with("messages", messages.clone())).0,
            200,
            "{messages}"
        );
    }
    let bad = [
        json!([]),
        json!("hi"),
        json!(["hi"]),
        json!([{ "role": "user" }]),
        json!([{ "role": "user", "content": null }]),
        json!([{ "role": "assistant", "content": null }]),
        json!([{ "role": "user", "content": 1 }]),
        json!([{ "role": "developer", "content": "hi" }]),
        json!([{ "content": "hi" }]),
        json!([{ "role": "user", "content": "hi", "images": [] }]),
        json!([{ "role": "user", "content": [{ "type": "image_url", "image_url": { "url": "http://h/x.png" } }] }]),
        json!([{ "role": "user", "content": [{ "type": "file", "file": { "file_id": "f" } }] }]),
        json!([{ "role": "user", "content": [{ "type": "input_audio", "input_audio": {} }] }]),
        json!([{ "role": "user", "content": [{ "type": "text", "text": "hi", "image_url": "http://h" }] }]),
        json!([{ "role": "user", "content": [{ "type": "text", "text": 1 }] }]),
        json!([{ "role": "user", "content": "hi", "tool_calls": [call_] }]),
        json!([{ "role": "assistant", "content": null, "tool_calls": [] }]),
        json!([{ "role": "assistant", "content": null, "tool_calls": [{ "id": "c1", "type": "function" }] }]),
        json!([{ "role": "assistant", "content": null, "tool_calls": [{ "id": "c1", "type": "web", "function": { "name": "f", "arguments": "{}" } }] }]),
        json!([{ "role": "user", "content": "hi", "tool_call_id": "c1" }]),
        json!([{ "role": "tool", "content": "hi", "tool_call_id": 1 }]),
        json!([{ "role": "tool", "content": "hi", "name": 1 }]),
    ];
    let sent = up.seen().len();
    for messages in &bad {
        assert_eq!(
            chat(&server, &with("messages", messages.clone())).0,
            400,
            "{messages}"
        );
    }
    assert_eq!(up.seen().len(), sent);
}

#[test]
fn an_answer_with_usage_charges_its_tokens() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let day = Arc::new(MemoryDay::new(fixed));
    let mut r = run(&up.url);
    r.day = day.clone();
    let (_dir, server) = start(r);
    assert_eq!(chat(&server, &ask()).0, 200);
    assert_eq!(chat(&server, &ask()).0, 200);
    assert_eq!(
        server.counts(),
        Counts {
            requests: 2,
            tokens: 30
        }
    );
    assert_eq!(day.used(), 30);
}

#[test]
fn an_answer_without_usage_keeps_the_reserve() {
    let up = stub(answers(completion(false)), Duration::ZERO);
    let day = Arc::new(MemoryDay::new(fixed));
    let mut r = run(&up.url);
    r.day = day.clone();
    let (_dir, server) = start(r);
    assert_eq!(chat(&server, &ask()).0, 200);
    let reserve = reserve_of(&up.seen()[0]);
    assert_eq!(reserve, self::reserve());
    assert_eq!(
        server.counts(),
        Counts {
            requests: 1,
            tokens: reserve
        }
    );
    assert_eq!(day.used(), reserve);
}

#[test]
fn a_request_past_the_run_s_budget_is_429_and_not_sent() {
    let up = stub(answers(completion(false)), Duration::ZERO);
    let probe = {
        let (_dir, server) = start(run(&up.url));
        chat(&server, &ask());
        reserve_of(&up.seen()[0])
    };
    let up = stub(answers(completion(false)), Duration::ZERO);
    let day = Arc::new(MemoryDay::new(fixed));
    let mut r = run(&up.url);
    r.tokens = 2 * probe - 1;
    r.day = day.clone();
    let (_dir, server) = start(r);
    assert_eq!(chat(&server, &ask()).0, 200);
    let a = chat(&server, &ask());
    assert_eq!(a.0, 429);
    assert!(says(&a).contains("run's"));
    assert_eq!(up.seen().len(), 1);
    assert_eq!(
        server.counts(),
        Counts {
            requests: 1,
            tokens: probe
        }
    );
    assert_eq!(day.used(), probe);
}

#[test]
fn a_reserve_the_run_cannot_hold_is_refused_before_anything_is_sent() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let mut r = run(&up.url);
    r.tokens = 100;
    let (_dir, server) = start(r);
    assert_eq!(chat(&server, &ask()).0, 429);
    assert!(up.seen().is_empty());
    assert_eq!(server.counts(), Counts::default());
}

#[test]
fn a_request_past_the_day_s_total_is_429_and_not_sent() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let day = Arc::new(MemoryDay::new(fixed));
    assert!(day.reserve(950, 1000));
    let mut r = run(&up.url);
    r.day_limit = 1000;
    r.day = day.clone();
    let (_dir, server) = start(r);
    let a = chat(&server, &ask());
    assert_eq!(a.0, 429);
    assert!(says(&a).contains("day's"));
    assert!(up.seen().is_empty());
    assert_eq!(day.used(), 950);
    assert_eq!(server.counts(), Counts::default());
}

#[test]
fn the_day_s_total_in_a_file_is_shared_by_two_runs() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let dir = TestDir::new("model-day");
    let file = dir.join("model-day");
    let probe = reserve();
    let mut a = run(&up.url);
    a.day = Arc::new(FileDay::new(&file, fixed));
    a.day_limit = 15 + probe;
    let (_da, first) = start(a);
    assert_eq!(chat(&first, &ask()).0, 200);
    assert_eq!(FileDay::new(&file, fixed).used(), Some(15));
    let mut b = run(&up.url);
    b.day = Arc::new(FileDay::new(&file, fixed));
    b.day_limit = 15 + probe - 1;
    let (_db, second) = start(b);
    assert_eq!(chat(&second, &ask()).0, 429);
    assert_eq!(up.seen().len(), 1);
}

#[test]
fn one_request_is_sent_at_a_time_and_a_second_waits() {
    let up = stub(answers(completion(true)), Duration::from_millis(300));
    let (_dir, server) = start(run(&up.url));
    let server = Arc::new(server);
    let clients: Vec<_> = (0..3)
        .map(|_| {
            let s = Arc::clone(&server);
            std::thread::spawn(move || chat(&s, &ask()).0)
        })
        .collect();
    for c in clients {
        assert_eq!(c.join().unwrap(), 200);
    }
    assert_eq!(up.most.load(Ordering::SeqCst), 1);
    assert_eq!(
        server.counts(),
        Counts {
            requests: 3,
            tokens: 45
        }
    );
}

#[test]
fn a_body_past_1_mib_is_413_and_not_read() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let head = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        MAX_BODY + 1
    );
    assert_eq!(raw(&server, head.as_bytes()).0, 413);
    assert!(up.seen().is_empty());
}

#[test]
fn a_body_of_1_mib_is_sent() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let short = ask().to_string().len();
    let text = "a".repeat(MAX_BODY - short + "hello".len());
    let r = with("messages", json!([{ "role": "user", "content": text }]));
    assert_eq!(r.to_string().len(), MAX_BODY);
    assert_eq!(chat(&server, &r).0, 200);
    assert_eq!(
        up.seen()[0].json()["messages"][0]["content"]
            .as_str()
            .unwrap()
            .len(),
        text.len()
    );
}

#[test]
fn a_body_needs_its_length() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let body = ask().to_string();
    let none = format!("POST /v1/chat/completions HTTP/1.1\r\n\r\n{body}");
    assert_eq!(raw(&server, none.as_bytes()).0, 411);
    let chunked = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    assert_eq!(raw(&server, chunked.as_bytes()).0, 411);
    assert!(up.seen().is_empty());
}

#[test]
fn a_head_that_is_not_http_or_too_long_is_refused() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    assert_eq!(raw(&server, b"hello\r\n\r\n").0, 400);
    let long = format!(
        "GET /v1/models HTTP/1.1\r\nX: {}\r\n\r\n",
        "a".repeat(MAX_HEAD)
    );
    assert_eq!(raw(&server, long.as_bytes()).0, 431);
}

#[test]
fn an_answer_past_4_mib_is_502_and_keeps_the_reserve() {
    let big = json!({ "pad": "a".repeat(MAX_ANSWER) });
    let up = stub(answers(big), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let a = chat(&server, &ask());
    assert_eq!(a.0, 502);
    assert!(says(&a).contains("4 MiB"));
    let reserve = reserve_of(&up.seen()[0]);
    assert_eq!(
        server.counts(),
        Counts {
            requests: 1,
            tokens: reserve
        }
    );
}

#[test]
fn an_answer_of_4_mib_is_passed_back() {
    let pad = MAX_ANSWER - json!({ "pad": "" }).to_string().len();
    let big = json!({ "pad": "a".repeat(pad) });
    let up = stub(answers(big.clone()), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let (status, body) = chat(&server, &ask());
    assert_eq!(status, 200);
    assert_eq!(body, big);
}

#[test]
fn an_answer_that_is_not_json_is_502() {
    let up = stub(
        Box::new(|| (200, b"<html>busy</html>".to_vec())),
        Duration::ZERO,
    );
    let (_dir, server) = start(run(&up.url));
    let a = chat(&server, &ask());
    assert_eq!(a.0, 502);
    assert!(says(&a).contains("not JSON"));
}

#[test]
fn an_endpoint_s_error_is_passed_back_with_its_status() {
    let busy = json!({ "error": { "message": "busy" } });
    let body = busy.to_string().into_bytes();
    let up = stub(Box::new(move || (429, body.clone())), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    assert_eq!(chat(&server, &ask()), (429, busy));
}

#[test]
fn an_endpoint_that_does_not_answer_is_502_and_keeps_the_reserve() {
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", closed.local_addr().unwrap());
    drop(closed);
    let (_dir, server) = start(run(&url));
    let a = chat(&server, &ask());
    assert_eq!(a.0, 502);
    assert_eq!(says(&a), "the model could not be asked");
    assert_eq!(
        server.counts(),
        Counts {
            requests: 1,
            tokens: reserve()
        }
    );
}

#[test]
fn a_curl_that_cannot_be_run_is_502() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let mut r = run(&up.url);
    r.curl = "/nonexistent/curl".into();
    let (_dir, server) = start(r);
    assert_eq!(chat(&server, &ask()).0, 502);
    assert!(up.seen().is_empty());
}

#[test]
fn the_key_and_body_go_to_curl_on_its_standard_input() {
    std::env::set_var("TEHAS_MODEL_KEY_PLUGINCONFIG", "k\"ey");
    let body = json!({ "model": OWN, "messages": [{ "content": "a \"quote\" and \\ and \n" }] })
        .to_string();
    let c = config("$TEHAS_MODEL_KEY_PLUGINCONFIG", &body);
    let lines: Vec<&str> = c.lines().collect();
    assert_eq!(lines[0], "header = \"Authorization: Bearer k\\\"ey\"");
    assert_eq!(lines[1], "header = \"Content-Type: application/json\"");
    assert_eq!(lines[2], "header = \"Expect:\"");
    assert!(lines[3].starts_with("data-binary = \"{"));
    assert_eq!(lines.len(), 4);
}

#[test]
fn more_connections_than_the_limit_are_503() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let idle: Vec<UnixStream> = (0..MAX_OPEN)
        .map(|_| UnixStream::connect(server.path()).unwrap())
        .collect();
    std::thread::sleep(Duration::from_millis(200));
    // Refused before its request is read, so this client sends none.
    let mut c = UnixStream::connect(server.path()).unwrap();
    let mut got = String::new();
    c.read_to_string(&mut got).unwrap();
    assert!(got.starts_with("HTTP/1.1 503 "), "{got}");
    drop(idle);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(call(&server, "GET", "/v1/models", b"").0, 200);
}

#[test]
fn a_request_after_the_stop_is_not_sent() {
    let up = stub(answers(completion(true)), Duration::ZERO);
    let (_dir, server) = start(run(&up.url));
    let path = server.path().to_path_buf();
    assert_eq!(server.stop(), Counts::default());
    assert!(UnixStream::connect(&path).is_err());
    assert!(up.seen().is_empty());
}

fn b64_encode(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\0\x10JFIF\0";
const WEBP: &[u8] = b"RIFF\x24\0\0\0WEBPVP8 ";

fn image(kind: &str, bytes: &[u8]) -> Value {
    json!({"type": "image_url", "image_url": {"url": format!("data:{kind};base64,{}", b64_encode(bytes))}})
}

fn asking(parts: Vec<Value>) -> Vec<u8> {
    let mut content = vec![json!({"type": "text", "text": "what differs?"})];
    content.extend(parts);
    with("messages", json!([{ "role": "user", "content": content }]))
        .to_string()
        .into_bytes()
}

fn checked(body: &[u8], content: Content) -> Result<(), String> {
    check(body, ID, OWN, content).map(|_| ())
}

#[test]
fn base64_reads_back_its_length_and_its_first_bytes() {
    for bytes in [&b"a"[..], b"ab", b"abc", PNG, WEBP] {
        let s = b64_encode(bytes);
        assert_eq!(base64_len(&s), Some(bytes.len()), "{s}");
        let n = bytes.len().min(12);
        assert_eq!(base64_head(&s).unwrap()[..n], bytes[..n], "{s}");
    }
    for bad in ["", "abc", "ab=c", "a===", "ab!d"] {
        assert_eq!(base64_len(bad), None, "{bad}");
    }
}

#[test]
fn with_model_images_a_request_carries_png_jpeg_or_webp_in_data_urls() {
    let parts = vec![
        image("image/png", PNG),
        image("image/jpeg", JPEG),
        image("image/webp", WEBP),
    ];
    assert_eq!(
        checked(&asking(parts.clone()), Content::TextAndImages),
        Ok(())
    );
    let mut detail = image("image/png", PNG);
    detail["image_url"]["detail"] = json!("low");
    assert_eq!(
        checked(&asking(vec![detail]), Content::TextAndImages),
        Ok(())
    );
    // Without the grant the same request is refused, as before.
    assert_eq!(
        checked(&asking(parts), Content::Text).unwrap_err(),
        "content parts must be of type text"
    );
}

#[test]
fn an_image_is_never_an_address_and_is_what_its_type_says() {
    let url = |u: &str| json!({"type": "image_url", "image_url": {"url": u}});
    let mut bad_detail = image("image/png", PNG);
    bad_detail["image_url"]["detail"] = json!("max");
    let mut extra = image("image/png", PNG);
    extra["image_url"]["name"] = json!("x");
    let cases: Vec<(Value, &str)> = vec![
        (
            url("https://example.com/a.png"),
            "an image is a data: URL, never an address",
        ),
        (
            url("http://127.0.0.1/a.png"),
            "an image is a data: URL, never an address",
        ),
        (
            url("data:image/png,raw"),
            "an image is base64 in a data: URL",
        ),
        (
            image("image/gif", b"GIF89a"),
            "an image is image/png, image/jpeg or image/webp",
        ),
        (
            image("image/png", JPEG),
            "the image's bytes are not image/png",
        ),
        (
            image("image/webp", b"RIFF\0\0\0\0AVI LIST"),
            "the image's bytes are not image/webp",
        ),
        (
            url("data:image/png;base64,iVBO!"),
            "an image's data is not base64",
        ),
        (bad_detail, "detail must be auto, low or high"),
        (extra, "the field name is not allowed"),
        (
            json!({"type": "file", "file": {}}),
            "a content part must be text or an image_url",
        ),
    ];
    for (part, why) in cases {
        assert_eq!(
            checked(&asking(vec![part.clone()]), Content::TextAndImages).unwrap_err(),
            why,
            "{part}"
        );
    }
}

#[test]
fn images_are_at_most_four_of_four_mib_and_only_they_open_the_larger_body() {
    let five = vec![image("image/png", PNG); MAX_IMAGES + 1];
    assert_eq!(
        checked(&asking(five), Content::TextAndImages).unwrap_err(),
        format!("at most {MAX_IMAGES} image parts in a request")
    );
    let mut big = PNG.to_vec();
    big.resize(MAX_IMAGE + 1, 0);
    assert_eq!(
        checked(
            &asking(vec![image("image/png", &big)]),
            Content::TextAndImages
        )
        .unwrap_err(),
        format!("an image is at most {MAX_IMAGE} bytes")
    );
    // An image of 3 MiB makes a body over 1 MiB, which the grant allows.
    let mut large = PNG.to_vec();
    large.resize(3 << 20, 0);
    let body = asking(vec![image("image/png", &large)]);
    assert!(body.len() > MAX_BODY);
    assert_eq!(checked(&body, Content::TextAndImages), Ok(()));
    // Text alone over 1 MiB is refused even with the grant.
    let text = with(
        "messages",
        json!([{ "role": "user", "content": "x".repeat(MAX_BODY) }]),
    )
    .to_string()
    .into_bytes();
    assert_eq!(
        checked(&text, Content::TextAndImages).unwrap_err(),
        format!("a request with no image part is at most {MAX_BODY} bytes")
    );
    assert_eq!(Content::TextAndImages.max_body(), MAX_BODY_IMAGES);
    assert_eq!(Content::Text.max_body(), MAX_BODY);
}
