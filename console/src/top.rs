//! The host section of `tehas top` (lib/top.sh): one reading of /proc, and
//! the section drawn from two of them.
//!
//!   console top reading <dir>
//!     One reading of the tree at <dir> (/proc on a host) as one line of JSON:
//!     the clock in seconds (uptime), the CPUs, the load averages, memory in
//!     KiB, the aggregate cpu line of stat, and one row per process: [pid,
//!     comm, CPU ticks, start ticks, resident pages]. Exit 1 when the tree
//!     cannot be read. The clock and the load averages are the text of the
//!     files, so a reading says what the host said.
//!   console top host <later reading> <earlier reading>
//!     The section: CPU use is what changed between the two readings, never
//!     the average since a process started. A process counts only with the
//!     same pid and start time in both, so a reused pid is not one long
//!     process. Two readings at the same instant measure nothing: their CPU
//!     figures are null. Prints null when either reading is no object.
//!
//! A process is named by its short command name only. The command line, the
//! environment and the user are never opened.

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

// How many processes the section keeps.
const ROWS: usize = 15;

pub fn main(given: &[String]) -> u8 {
    match given {
        [verb, dir] if verb == "reading" => match reading(Path::new(dir)) {
            Some(text) => {
                println!("{text}");
                0
            }
            None => 1,
        },
        [verb, later, earlier] if verb == "host" => {
            println!("{}", host_text(later, earlier));
            0
        }
        [verb, live, lines, max_age, reading, earlier] if verb == "snapshot" => {
            match (lines.parse(), max_age.parse()) {
                (Ok(lines), Ok(max_age)) => {
                    println!(
                        "{}",
                        crate::snapshot::snapshot(
                            Path::new(live),
                            lines,
                            max_age,
                            reading,
                            earlier
                        )
                    );
                    0
                }
                _ => {
                    eprintln!("console top: snapshot: <lines> and <max age> are whole numbers");
                    2
                }
            }
        }
        _ => {
            eprintln!(
                "console top: usage: reading <dir> | host <later> <earlier> | snapshot <live> <lines> <max age> <reading> <earlier>"
            );
            2
        }
    }
}

/// The section from two readings, as text: null when either is no object.
pub fn host_text(later: &str, earlier: &str) -> String {
    let parse = |s: &str| serde_json::from_str::<Value>(s).unwrap_or(Value::Null);
    let section = host(&parse(later), &parse(earlier), page_size()).to_string();
    // The load averages as the reading wrote them: 0.10 stays 0.10.
    match raw_load(later) {
        Some(raw) => section.replacen(LOAD, &raw, 1),
        None => section.replacen(LOAD, "null", 1),
    }
}

/// A decimal as the file wrote it, digits and dots only.
fn decimal(s: &str) -> Option<&str> {
    (!s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && s.parse::<f64>().is_ok())
    .then_some(s)
}

fn digits(s: &str) -> Option<u64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

fn reading(root: &Path) -> Option<String> {
    let uptime = fs::read_to_string(root.join("uptime")).ok()?;
    let clock = decimal(uptime.lines().next()?.split(' ').next()?)?.to_string();

    // The sum over every cpu: user nice system idle iowait irq softirq steal.
    // Idle and iowait are not use.
    let stat = fs::read_to_string(root.join("stat")).ok()?;
    let f: Vec<&str> = stat.lines().next()?.split_whitespace().collect();
    if f.first() != Some(&"cpu") || f.len() < 9 {
        return None;
    }
    let ticks: Vec<u64> = f[1..9].iter().map(|v| digits(v)).collect::<Option<_>>()?;
    let total: u64 = ticks.iter().sum();
    let idle = ticks[3] + ticks[4];

    let mut cpus = fs::read_to_string(root.join("cpuinfo"))
        .map(|s| s.lines().filter(|l| l.starts_with("processor")).count())
        .unwrap_or(0);
    if cpus == 0 {
        cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    }

    // The three load averages and memory: either can be absent, and is then
    // said as nothing rather than guessed at.
    let load: Vec<String> = fs::read_to_string(root.join("loadavg"))
        .ok()
        .and_then(|s| {
            let words: Vec<&str> = s.lines().next()?.split_whitespace().take(3).collect();
            let load: Option<Vec<String>> = words
                .iter()
                .map(|w| decimal(w).map(str::to_string))
                .collect();
            load.filter(|l| l.len() == 3)
        })
        .unwrap_or_default();
    let (mut total_kb, mut avail_kb) = (None, None);
    if let Ok(meminfo) = fs::read_to_string(root.join("meminfo")) {
        for line in meminfo.lines() {
            let mut w = line.split_whitespace();
            match (w.next(), w.next()) {
                (Some("MemTotal:"), Some(v)) => total_kb = digits(v),
                (Some("MemAvailable:"), Some(v)) => avail_kb = digits(v),
                _ => {}
            }
        }
    }
    let memory = match (total_kb, avail_kb) {
        (Some(t), Some(a)) => format!("{{\"totalKb\":{t},\"availKb\":{a}}}"),
        _ => "null".to_string(),
    };

    let mut pids: Vec<(u64, std::path::PathBuf)> = fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| Some((digits(e.file_name().to_str()?)?, e.path())))
                .collect()
        })
        .unwrap_or_default();
    pids.sort();
    let procs: Vec<Value> = pids
        .iter()
        .filter_map(|(pid, dir)| row(*pid, dir))
        .collect();

    Some(format!(
        "{{\"clock\":{clock},\"cpus\":{cpus},\"load\":[{}],\"memory\":{memory},\"cpuAll\":{total},\"cpuIdle\":{idle},\"procs\":{}}}",
        load.join(","),
        Value::Array(procs)
    ))
}

/// [pid, comm, utime + stime, starttime, rss] from <dir>/stat. The comm is
/// what lies between the first ( and the last ), and may hold either; the
/// fields after it are counted from there. A file that cannot be read, or
/// names too few fields, gives no row.
fn row(pid: u64, dir: &Path) -> Option<Value> {
    let text = fs::read_to_string(dir.join("stat")).ok()?;
    let text: String = text
        .lines()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let (open, close) = (text.find('(')?, text.rfind(')')?);
    if close < open {
        return None;
    }
    let comm = &text[open + 1..close];
    let f: Vec<&str> = text[close + 1..].split_whitespace().collect();
    if f.len() < 22 {
        return None;
    }
    let (utime, stime) = (digits(f[11])?, digits(f[12])?);
    let (start, rss) = (digits(f[19])?, digits(f[21])?);
    Some(json!([pid, comm, utime + stime, start, rss]))
}

fn page_size() -> f64 {
    Command::new("getconf")
        .arg("PAGE_SIZE")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| digits(s.trim()))
        .map_or(4096.0, |p| p as f64)
}

// Where the load averages go in a section, for main to fill in as text.
const LOAD: &str = "\"\\u0001load\"";

/// The three load averages of a reading as its text wrote them.
fn raw_load(reading: &str) -> Option<String> {
    let at = reading.find("\"load\":")? + "\"load\":".len();
    let rest = reading[at..].trim_start().strip_prefix('[')?;
    let words: Vec<&str> = rest[..rest.find(']')?].split(',').map(str::trim).collect();
    (words.len() == 3 && words.iter().all(|w| w.parse::<f64>().is_ok()))
        .then(|| format!("[{}]", words.join(",")))
}

/// A number that is not negative, else nothing.
fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|n| *n >= 0.0)
}

/// A figure as jq prints it: a whole number without a fraction.
fn figure(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9e15 {
        Value::from(n as i64)
    } else {
        Value::from(n)
    }
}

/// Rounded to one decimal, within the bounds.
fn one(n: f64, hi: f64) -> Value {
    figure((n.clamp(0.0, hi) * 10.0).round() / 10.0)
}

fn host(later: &Value, earlier: &Value, page: f64) -> Value {
    let (Some(a), Some(b)) = (later.as_object(), earlier.as_object()) else {
        return Value::Null;
    };
    let (c2, c1) = (num(a.get("clock")), num(b.get("clock")));
    // The window in ticks, a hundredth of a second.
    let dt = match (c2, c1) {
        (Some(c2), Some(c1)) if c2 > c1 => Some(((c2 - c1) * 100.0).round()),
        _ => None,
    };
    let rows = |r: &Map<String, Value>| -> Vec<Vec<Value>> {
        r.get("procs")
            .and_then(Value::as_array)
            .map(|p| {
                p.iter()
                    .filter_map(|row| row.as_array().filter(|r| r.len() == 5).cloned())
                    .collect()
            })
            .unwrap_or_default()
    };
    let key = |v: &Value| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let prev: HashMap<String, Vec<Value>> = rows(b).into_iter().map(|r| (key(&r[0]), r)).collect();
    let dall = num(a.get("cpuAll"))
        .zip(num(b.get("cpuAll")))
        .map(|(x, y)| x - y);
    let didle = num(a.get("cpuIdle"))
        .zip(num(b.get("cpuIdle")))
        .map(|(x, y)| x - y);
    let total_kb = a.get("memory").and_then(|m| num(m.get("totalKb")));
    let avail_kb = a.get("memory").and_then(|m| num(m.get("availKb")));
    let mem_total = total_kb.filter(|t| *t > 0.0);

    let cpu_percent = match (dt, dall, didle) {
        (Some(_), Some(dall), Some(didle)) if dall > 0.0 => {
            one((dall - didle) / dall * 100.0, 100.0)
        }
        _ => Value::Null,
    };
    let memory = match (mem_total, avail_kb) {
        (Some(t), Some(av)) => json!({
            "totalMb": figure((t / 1024.0).round()),
            "usedMb": figure(((t - av) / 1024.0).round()),
            "percent": one((t - av) / t * 100.0, 100.0),
        }),
        _ => Value::Null,
    };
    let load = if a
        .get("load")
        .and_then(Value::as_array)
        .is_some_and(|l| l.len() == 3)
    {
        json!("\u{1}load")
    } else {
        Value::Null
    };

    let mut processes: Vec<(f64, f64, Value)> = rows(a)
        .into_iter()
        .filter_map(|r| {
            let p = prev.get(&key(&r[0]))?;
            if num(p.get(3)) != num(r.get(3)) {
                return None;
            }
            let cpu = match (dt, num(r.get(2)), num(p.get(2))) {
                (Some(dt), Some(now), Some(was)) if dt > 0.0 => one((now - was) / dt * 100.0, 99999.0),
                _ => Value::Null,
            };
            let mem = match (mem_total, num(r.get(4))) {
                (Some(t), Some(rss)) => one(rss * page / 1024.0 / t * 100.0, 100.0),
                _ => Value::Null,
            };
            let up = match (c2, num(r.get(3))) {
                (Some(c2), Some(start)) => {
                    figure((((c2 * 100.0).round() - start) / 100.0).floor().clamp(0.0, 999_999_999.0))
                }
                _ => Value::Null,
            };
            let order = cpu.as_f64().unwrap_or(-1.0);
            let pid = r[0].as_f64().unwrap_or(0.0);
            let name = if r[1].is_null() { json!("") } else { r[1].clone() };
            Some((
                order,
                pid,
                json!({"pid": r[0], "name": name, "cpuPercent": cpu, "memPercent": mem, "upSeconds": up}),
            ))
        })
        .collect();
    // The most CPU in the window first, then by pid.
    processes.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.total_cmp(&y.1)));
    let processes: Vec<Value> = processes.into_iter().take(ROWS).map(|p| p.2).collect();

    json!({
        "cpus": a.get("cpus").and_then(|c| num(Some(c))).map_or(Value::Null, figure),
        "load": load,
        "cpuPercent": cpu_percent,
        "memory": memory,
        "processes": processes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(clock: f64, all: u64, idle: u64, procs: Value) -> Value {
        json!({"clock": clock, "cpus": 2, "load": [0.1, 0.2, 0.3],
               "memory": {"totalKb": 65536, "availKb": 32768},
               "cpuAll": all, "cpuIdle": idle, "procs": procs})
    }

    #[test]
    fn use_is_the_difference_between_two_readings() {
        let was = reading(
            1000.0,
            100_000,
            90_000,
            json!([
                [100, "sleepy", 100000, 100, 400],
                [200, "busy", 100, 200, 400]
            ]),
        );
        let now = reading(
            1100.0,
            200_000,
            180_000,
            json!([
                [100, "sleepy", 100100, 100, 400],
                [200, "busy", 4100, 200, 400],
                [700, "new", 50, 700, 10]
            ]),
        );
        let h = host(&now, &was, 4096.0);
        assert_eq!(h["cpuPercent"], json!(10));
        assert_eq!(
            h["memory"],
            json!({"totalMb": 64, "usedMb": 32, "percent": 50})
        );
        let names: Vec<&str> = h["processes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["busy", "sleepy"]);
        assert_eq!(h["processes"][0]["cpuPercent"], json!(40));
        assert_eq!(h["processes"][1]["cpuPercent"], json!(1));
        assert_eq!(h["processes"][0]["upSeconds"], json!(1098));
        assert_eq!(h["processes"][0]["memPercent"], json!(2.4));
    }

    #[test]
    fn the_same_instant_measures_nothing_and_a_reused_pid_is_not_one_process() {
        let was = reading(1000.0, 1000, 900, json!([[100, "cycle", 5000, 500, 400]]));
        let h = host(&was, &was, 4096.0);
        assert_eq!(h["cpuPercent"], Value::Null);
        assert_eq!(h["processes"][0]["cpuPercent"], Value::Null);
        let now = reading(1010.0, 2000, 1900, json!([[100, "cycle", 50, 50000, 400]]));
        assert_eq!(host(&now, &was, 4096.0)["processes"], json!([]));
        assert_eq!(host(&now, &json!("x"), 4096.0), Value::Null);
    }

    #[test]
    fn a_tree_is_read_as_the_files_say() {
        let dir = std::env::temp_dir().join(format!("top-reading-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("42")).unwrap();
        fs::create_dir_all(dir.join("notapid")).unwrap();
        fs::write(dir.join("uptime"), "1000.00 5.0\n").unwrap();
        fs::write(dir.join("stat"), "cpu  1 2 3 4 5 6 7 8 9 10\ncpu0 1\n").unwrap();
        fs::write(dir.join("cpuinfo"), "processor\t: 0\nprocessor\t: 1\n").unwrap();
        fs::write(dir.join("loadavg"), "0.10 0.20 0.30 1/2 3\n").unwrap();
        fs::write(
            dir.join("meminfo"),
            "MemTotal: 64 kB\nMemAvailable: 32 kB\n",
        )
        .unwrap();
        fs::write(
            dir.join("42/stat"),
            "42 (a (b) c) S 1 1 1 0 -1 1 0 0 0 0 7 3 0 0 20 0 1 0 900 1048576 10 0\n",
        )
        .unwrap();
        fs::write(
            dir.join("notapid/stat"),
            "7 (x) S 1 1 1 0 -1 1 0 0 0 0 1 1 0 0 20 0 1 0 900 1 1 0\n",
        )
        .unwrap();
        let text = super::reading(&dir).unwrap();
        assert_eq!(
            text,
            r#"{"clock":1000.00,"cpus":2,"load":[0.10,0.20,0.30],"memory":{"totalKb":64,"availKb":32},"cpuAll":36,"cpuIdle":9,"procs":[[42,"a (b) c",10,900,10]]}"#
        );
        fs::remove_file(dir.join("uptime")).unwrap();
        assert!(super::reading(&dir).is_none());
        fs::remove_dir_all(&dir).unwrap();
    }
}
