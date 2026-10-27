//! `tehas analytics`, libexec/analytics-cmd, against this program: the copy a
//! person or a program fetches holds no token, no URL with a host and no
//! address, and the command calls nothing outside the host.

use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const ANALYTICS: &str = env!("CARGO_BIN_EXE_analytics");

/// The schema the copy of this database is made under, read from the program
/// rather than written here, so the version is named in one place: an export
/// of an empty home prints `PRAGMA user_version` of a database it just made.
fn schema_version() -> i64 {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("command-schema");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("state")).unwrap();
    let copy = dir.join("copy.db");
    let out = Command::new(ANALYTICS)
        .args(["export", copy.to_str().unwrap(), "--json"])
        .env("TEHAS_HOME", &dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let line: Value = serde_json::from_slice(&out.stdout).unwrap();
    line["schema"].as_i64().expect("the copy names its schema")
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// One `verify-completed` line of the cycle's stream, as the runner ends a run
// of the product's gate: the tier the command named, whether it passed and its
// own milliseconds.
fn gate(at: i64, command: &str, tier: &str, ok: bool) -> String {
    json!({"v": 1, "kind": "verify-completed", "issue": 7,
           "event": {"kind": "verify-completed", "step": "develop", "command": command,
                      "target.kind": tier, "ok": ok, "ms": 300_000, "at": at}})
    .to_string()
}

// An address written out here, so no private one stands in the source.
fn ip(octets: [u8; 4]) -> String {
    octets.map(|o| o.to_string()).join(".")
}

fn iso(t: i64) -> String {
    let out = Command::new("date")
        .args(["-u", "-d", &format!("@{t}"), "+%FT%TZ"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

struct Host {
    dir: PathBuf,
    home: PathBuf,
    instance: PathBuf,
    bin: PathBuf,
}

impl Host {
    // A scratch host: TEHAS_HOME with this program as libexec/analytics, an
    // instance, and gh, docker, git and curl that record a call and fail.
    fn new(name: &str) -> Host {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("command-{name}"));
        let _ = fs::remove_dir_all(&dir);
        let home = dir.join("tehas");
        for sub in ["state/cycles", "logs", "docs", "libexec"] {
            fs::create_dir_all(home.join(sub)).unwrap();
        }
        symlink(ANALYTICS, home.join("libexec/analytics")).unwrap();
        let instance = dir.join("instance");
        fs::create_dir_all(&instance).unwrap();
        fs::set_permissions(&instance, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            instance.join("instance.env"),
            "TEHAS_REPO=acme/widget\nTEHAS_CONTROL_ISSUE=1\nTEHAS_ALLOWED_LABELERS=alice\n\
             PI_ENSEMBLE_SUBAGENT_MODEL=test-model\nPI_ENSEMBLE_SUBAGENT_PROVIDER=test-provider\n",
        )
        .unwrap();
        let bin = dir.join("bin");
        fs::create_dir_all(&bin).unwrap();
        for tool in ["gh", "docker", "git", "curl"] {
            let stub = bin.join(tool);
            fs::write(
                &stub,
                format!(
                    "#!/usr/bin/env bash\necho \"{tool} $*\" >>\"{}\"\nexit 1\n",
                    dir.join("outside").display()
                ),
            )
            .unwrap();
            fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
        }
        Host {
            dir,
            home,
            instance,
            bin,
        }
    }

    fn put(&self, name: &str, text: &str) {
        fs::write(self.home.join(name), text).unwrap();
    }

    // The person's command, with the environment a login gives it and no other.
    fn tehas(&self, args: &[&str]) -> (i32, String, String) {
        let path = format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap());
        let out = Command::new("bash")
            .arg(root().join("libexec/analytics-cmd"))
            .args(args)
            .current_dir(&self.dir)
            .env_clear()
            .env("PATH", path)
            .env("HOME", &self.dir)
            .env("TEHAS_HOME", &self.home)
            .env("TEHAS_INSTANCE_DIR", &self.instance)
            .output()
            .unwrap();
        (
            out.status.code().unwrap(),
            String::from_utf8(out.stdout).unwrap(),
            String::from_utf8(out.stderr).unwrap(),
        )
    }

    fn outside(&self) -> String {
        fs::read_to_string(self.dir.join("outside")).unwrap_or_default()
    }

    // A cycle of issue 7 that ended an hour ago, its failure cause full of
    // what must not be fetched, recorded as libexec/cycle records it.
    fn record_a_run(&self) -> i64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let started = now - 3600;
        let fixture = root().join("test/fixtures/headless-316134e");
        let mut result: Value =
            serde_json::from_str(&fs::read_to_string(fixture.join("result.json")).unwrap())
                .unwrap();
        result["issues"][0]["failedStep"] = json!("handoff");
        result["issues"][0]["cap"] = json!("explore-bodies-empty");
        result["issues"][0]["parkReason"] = json!("cap");
        result["issues"][0]["failure"] = json!({"kind": "provider", "step": "handoff",
            "cause": format!("GET https://user:hunter2@models.example/v1 from {} and {} with Bearer abcdefghijklmnopqrstuvwxyz; \
                connect ECONNREFUSED models.example:8000; git@host.tld:org/repo; token=ghp_abcdefghijklmnopqrstuvwxyz123456; \
                Incorrect API key provided: sk-abc123def456ghi789jkl0; dial fe80::1%eth0 and 2001:db8::7",
                ip([10, 0, 0, 7]), ip([203, 0, 113, 9]))});
        self.put("state/cycles/7.result.json", &result.to_string());
        fs::copy(
            fixture.join("progress.jsonl"),
            self.home.join("logs/work-7-261015-183834.jsonl"),
        )
        .unwrap();
        self.put(
            "state/cycles/7.progress.json",
            &json!({"roles": [{"role": "ops", "dispatches": 1, "turns": 3, "input": 100,
                               "output": 10, "cacheRead": 5},
                              {"role": "explore", "dispatches": 1, "turns": 2, "input": 50,
                               "output": 5, "cacheRead": 0}],
                    "reason": "", "failure": null, "reviewHeld": false})
            .to_string(),
        );
        self.put(
            "logs/work-7-261015-183834.jsonl.preflight",
            "Everything up-to-date\n",
        );
        // The live files of a poll that was held.
        fs::create_dir_all(self.home.join("live")).unwrap();
        self.put(
            "live/status.json",
            &json!({"generatedAt": iso(now), "paused": false, "queued": [{"number": 8}],
                    "running": [], "awaiting": [], "stopped": [], "planning": 0})
            .to_string(),
        );
        self.put(
            "live/dispatch.json",
            &json!({"time": iso(now), "why": "resources", "count": 0, "free": 2}).to_string(),
        );
        self.put(
            "live/energy.json",
            &json!({"generatedAt": iso(now), "totals": {"watts": 250.0}}).to_string(),
        );
        self.put("docs/version.txt", "v0.7.22+1234abc\n");
        // The run's facts as lib/analytics.sh gives them: the history line,
        // the approval, the settings snapshot as lib/improve.sh writes it (a
        // model with dots, a provider word, and a host value it withheld) and
        // the image's commit.
        fs::create_dir_all(self.home.join("tmp")).unwrap();
        self.put(
            "tmp/facts.json",
            &json!({"line": {"at": iso(started + 30), "started": started, "rc": 10,
                             "status": "handoff", "parked": true,
                             "pr": "https://github.com/o/r/pull/3"},
                    "attempt": 1,
                    "approval": {"at": iso(started - 90), "how": "label"},
                    "settings": {"container": {"PI_ENSEMBLE_SUBAGENT_MODEL": "qwen3.5-coder-32b",
                                               "PI_ENSEMBLE_SUBAGENT_PROVIDER": "fleet",
                                               "PI_ENSEMBLE_MODEL_OPS": "[withheld]"},
                                 "host": {"TEHAS_WORK_TIMEOUT": "21600"},
                                 "concurrentCycles": 2},
                    "rukas": "abcdef1"})
            .to_string(),
        );
        let facts = self.home.join("tmp/facts.json");
        // The gate's two runs, appended to the stream the copy is read from:
        // the rows of run_gate_runs and the gate columns of the run hold what
        // these lines say, and the scan of the copy below reaches every text
        // column of them, so a command of the gate is checked for a host as
        // the cause is. The fixture's own lines hold no secret, so it is these
        // appended lines the scan of the gate's columns has to find a host in.
        let stream = fs::read_to_string(self.home.join("logs/work-7-261015-183834.jsonl")).unwrap();
        fs::write(
            self.home.join("logs/work-7-261015-183834.jsonl"),
            format!(
                "{stream}{}\n{}\n",
                gate(1792318700000, "scripts/verify.sh --fast", "fast", true),
                gate(1792318760000, "scripts/verify.sh", "full", false),
            ),
        )
        .unwrap();
        let image = format!("sha256:{}", "ab".repeat(32));
        for args in [
            vec![
                "run",
                "7",
                "--facts",
                facts.to_str().unwrap(),
                "--image",
                image.as_str(),
                "--oom",
                "0",
                "--forks",
                "0",
            ],
            vec!["event", "7", "ready"],
            vec!["sample"],
        ] {
            let out = Command::new(ANALYTICS)
                .args(&args)
                .env("TEHAS_HOME", &self.home)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        }
        started
    }
}

#[test]
fn an_export_through_tehas_analytics_holds_no_token_host_or_address() {
    let host = Host::new("export");
    let started = host.record_a_run();
    let copy = host.dir.join("copy.db");
    let (code, out, err) = host.tehas(&["export", copy.to_str().unwrap(), "--json"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(err, "");
    assert_eq!(
        out,
        format!(
            "{{\"api\":1,\"schema\":{},\"from\":{started},\"to\":{},\"runs\":1}}\n",
            schema_version(),
            started + 30
        )
    );
    assert_eq!(
        fs::metadata(&copy).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(host.outside(), "");

    // Every text value of every table: no token, no URL with a host, no
    // address. The cause was there, masked.
    let token = regex::Regex::new(
        r"gh[pousr]_[A-Za-z0-9]{8}|github_pat_|(?i)bearer [A-Za-z0-9]{8}|:[^@/\s]+@|sk-[A-Za-z0-9]{6}|hunter2|@",
    )
    .unwrap();
    let url =
        regex::Regex::new(r"(?i)[a-z]+://[A-Za-z0-9.-]+|models\.example|host\.tld|github\.com")
            .unwrap();
    let address =
        regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b|(?i)[0-9a-f]*::[0-9a-f]+")
            .unwrap();
    let db = Connection::open_with_flags(&copy, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let tables: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(tables.len(), 7, "{tables:?}");
    // The gate was recorded: two runs of it, of two tiers, and the run's own
    // total of them.
    let gate: (i64, i64, i64) = db
        .query_row(
            "SELECT (SELECT count(*) FROM run_gate_runs), coalesce(sum(tier IS NOT NULL), 0),
                    (SELECT gate_ms FROM runs) FROM run_gate_runs",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(gate, (2, 2, 600_000));
    // Every text column holds a value, so every one is scanned.
    for t in &tables {
        let cols: Vec<String> = db
            .prepare(&format!(
                "SELECT name FROM pragma_table_info('{t}') WHERE type = 'TEXT'"
            ))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(!cols.is_empty(), "{t}");
        for c in cols {
            let n: i64 = db
                .query_row(
                    &format!("SELECT count(*) FROM {t} WHERE {c} IS NOT NULL"),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(n > 0, "{t}.{c} is never set");
        }
    }
    let mut seen = 0;
    for t in &tables {
        let mut q = db.prepare(&format!("SELECT * FROM {t}")).unwrap();
        let n = q.column_count();
        let mut rows = q.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            for i in 0..n {
                if let Ok(Some(v)) = row.get::<_, Option<String>>(i) {
                    seen += 1;
                    for re in [&token, &url, &address] {
                        assert!(!re.is_match(&v), "{t}: {v}");
                    }
                }
            }
        }
    }
    assert!(seen > 10);
    let cause: String = db
        .query_row("SELECT failure_cause FROM runs", [], |r| r.get(0))
        .unwrap();
    assert!(cause.contains("ECONNREFUSED"), "{cause}");

    // The same file twice is refused, and the copy stays as it was.
    let before = fs::read(&copy).unwrap();
    let (code, out, _) = host.tehas(&["export", copy.to_str().unwrap(), "--json"]);
    assert_eq!((code, out.as_str()), (73, ""));
    assert_eq!(fs::read(&copy).unwrap(), before);
    assert_eq!(host.outside(), "");
}

#[test]
fn tehas_analytics_report_prints_the_ten_days_for_a_person() {
    let host = Host::new("report");
    host.record_a_run();
    let (code, out, err) = host.tehas(&["report"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("Last 10 days, "), "{out}");
    assert!(out.contains("\nRuns               1: "), "{out}");
    assert!(!out.contains("ECONNREFUSED"), "{out}");
    assert_eq!(host.outside(), "");
}

#[test]
fn with_analytics_off_tehas_analytics_is_69_and_touches_nothing() {
    let host = Host::new("off");
    let env = host.instance.join("instance.env");
    let mut settings = fs::read_to_string(&env).unwrap();
    settings.push_str("TEHAS_ANALYTICS=off\n");
    fs::write(&env, settings).unwrap();
    let copy = host.dir.join("copy.db");
    for args in [vec!["report"], vec!["export", copy.to_str().unwrap()]] {
        let (code, out, err) = host.tehas(&args);
        assert_eq!((code, out.as_str()), (69, ""), "{args:?}");
        assert!(err.contains("TEHAS_ANALYTICS=off"), "{err}");
    }
    assert!(!copy.exists());
    assert!(!host.home.join("state/analytics.db").exists());
}
