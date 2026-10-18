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

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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
        result["issues"][0]["failure"] = json!({"kind": "provider",
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
            "state/cycles/7.approved.json",
            &json!({"at": iso(started - 90), "by": "someone", "how": "label"}).to_string(),
        );
        self.put("state/cycles/7.rukas", "abcdef1\n");
        self.put("docs/version.txt", "v0.7.22+1234abc\n");
        self.put(
            "state/cycles/7.history.jsonl",
            &format!(
                "{}\n",
                json!({"at": iso(started + 30), "started": started, "rc": 10, "status": "handoff",
                       "parked": true, "pr": "https://github.com/o/r/pull/3"})
            ),
        );
        for args in [vec!["run", "7"], vec!["event", "7", "ready"]] {
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
            "{{\"api\":1,\"schema\":1,\"from\":{started},\"to\":{},\"runs\":1}}\n",
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
    assert_eq!(tables.len(), 6, "{tables:?}");
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
