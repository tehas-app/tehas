//! The frame of `tehas top`, drawn from its snapshot (src/snapshot.rs) and
//! from nothing else: no file, no command, and no clock but when the snapshot
//! holds no time.
//!
//!   console top frame <width> <height> <snapshot
//!
//! Every line is cut to <width>, and the frame never has more lines than
//! <height>. To fit, host process rows go first, then console lines and unit
//! rows, the later row first, then the queue's issue rows. The factory's
//! lines and the cycle lines are those of `tehas status` (src/status.rs).

use crate::jq::{first, J};
use crate::status;

#[derive(Clone)]
enum Kind {
    Text(String),
    State { health: J, stale: bool },
    Cycle(J),
    Host(J),
    Process(J),
    Issue(J),
    Unit(J),
    Console(J),
}

struct Item {
    // Which rows go first when the frame is too tall: 1 first, 9 never.
    p: u8,
    // How many lines the item counts for.
    n: usize,
    kind: Kind,
}

fn item(p: u8, kind: Kind) -> Item {
    Item { p, n: 1, kind }
}

fn text(t: String) -> Item {
    item(9, Kind::Text(t))
}

/// `(. // "") | tostring`, cut to $w characters.
fn cut(s: &str, w: usize) -> String {
    first(s, w)
}

/// Every string with the characters a terminal could act on taken out:
/// below 0x20 but tab, and 0x7f to 0x9f.
fn safe(v: J) -> J {
    match v {
        J::Str(s) => J::Str(
            s.chars()
                .filter(|c| matches!(*c as u32, 32..=126 | 9) || *c as u32 >= 160)
                .collect(),
        ),
        J::Arr(a) => J::Arr(a.into_iter().map(safe).collect()),
        J::Obj(o) => J::Obj(o.into_iter().map(|(k, v)| (k, safe(v))).collect()),
        other => other,
    }
}

fn or_q(v: &J) -> String {
    if v.is_null() {
        "?".to_string()
    } else {
        v.text()
    }
}

fn default<'a>(v: &'a J, d: &'a J) -> &'a J {
    v.or(d)
}

/// Right-justified in $w, or cut to it.
fn rj(v: &J, w: usize) -> String {
    let t = default(v, &J::str("?")).text();
    let n = t.chars().count();
    if n >= w {
        first(&t, w)
    } else {
        format!("{}{t}", " ".repeat(w - n))
    }
}

/// Padded with spaces to $w, or cut to it.
fn lj(t: &str, w: usize) -> String {
    let n = t.chars().count();
    if n >= w {
        first(t, w)
    } else {
        format!("{t}{}", " ".repeat(w - n))
    }
}

fn host_line(h: &J) -> String {
    let load = match h.get("load") {
        J::Arr(a) if a.len() == 3 => a.iter().map(or_q).collect::<Vec<_>>().join(" "),
        _ => "?".to_string(),
    };
    let mb = |k: &str| default(h.at(&["memory", k]), &J::str("?")).text();
    format!(
        "host      {} cpus, load {load}, cpu {}%, mem {} of {} MB",
        or_q(h.get("cpus")),
        or_q(h.get("cpuPercent")),
        mb("usedMb"),
        mb("totalMb")
    )
}

fn process_line(p: &J) -> String {
    format!(
        "  {}  {}  {}%  {}%  {} s",
        rj(p.get("pid"), 7),
        lj(&default(p.get("name"), &J::str("")).text(), 15),
        rj(p.get("cpuPercent"), 5),
        rj(p.get("memPercent"), 5),
        rj(p.get("upSeconds"), 8)
    )
}

fn issue_line(i: &J) -> String {
    format!(
        "  {}  {}  {}",
        lj(&i.get("number").text(), 5),
        lj(&default(i.get("state"), &J::str("")).text(), 9),
        default(i.get("title"), &J::str("")).text()
    )
}

fn unit_line(u: &J) -> String {
    let file_state = default(u.get("unitFileState"), &J::str("-")).text();
    let next = default(u.get("next"), &J::str("")).text();
    format!(
        "  {}  {}  {}{}",
        lj(&default(u.get("id"), &J::str("")).text(), 26),
        if file_state.is_empty() {
            "-"
        } else {
            &file_state
        },
        default(u.get("activeState"), &J::str("-")).text(),
        if next.is_empty() {
            String::new()
        } else {
            format!("  next {next}")
        }
    )
}

/// " (<age>)" of a time, or nothing when it cannot be read.
fn stamp(t: &J, now: i64) -> String {
    match t.as_str().and_then(crate::flags::epoch) {
        Some(e) => format!(" ({})", status::ago(e, now)),
        None => String::new(),
    }
}

fn draw(item: &Item, now: i64) -> Vec<String> {
    match &item.kind {
        Kind::Text(t) => vec![t.clone()],
        Kind::State { health, stale } => status::state_lines(health, now, *stale, ""),
        Kind::Cycle(c) => vec![status::cycle_line(c, now)],
        Kind::Host(h) => vec![host_line(h)],
        Kind::Process(p) => vec![process_line(p)],
        Kind::Issue(i) => vec![issue_line(i)],
        Kind::Unit(u) => vec![unit_line(u)],
        Kind::Console(l) => vec![if l.truthy() { l.text() } else { String::new() }],
    }
}

/// Drops rows, the most expendable first, until the frame fits in $h lines.
fn fit(mut items: Vec<Item>, h: usize) -> Vec<Item> {
    let total = |items: &[Item]| items.iter().map(|i| i.n).sum::<usize>();
    for p in 1..5 {
        let have = total(&items);
        if have <= h {
            break;
        }
        let mut need = have - h;
        let mut keep = vec![true; items.len()];
        for (i, it) in items.iter().enumerate().rev() {
            if need == 0 {
                break;
            }
            if it.p == p {
                keep[i] = false;
                need -= 1;
            }
        }
        let mut k = keep.into_iter();
        items.retain(|_| k.next().unwrap_or(true));
    }
    items
}

/// The frame as its lines.
pub fn frame(snapshot: &J, cols: usize, rows: usize) -> Vec<String> {
    let s = safe(snapshot.clone());
    let now = s
        .get("generatedAt")
        .as_str()
        .and_then(crate::flags::epoch)
        .unwrap_or_else(tehas_core::time::now);
    let objects = |v: &J| -> Vec<J> {
        v.items()
            .iter()
            .filter(|i| matches!(i, J::Obj(_)))
            .cloned()
            .collect()
    };
    let generated = default(s.get("generatedAt"), &J::str(""))
        .text()
        .replacen('T', " ", 1)
        .replacen('Z', " UTC", 1);
    let mut items = vec![text(format!("tehas top   snapshot {generated}"))];

    // 1. The factory and the queue counts, in the words tehas status uses.
    let factory = s.get("factory");
    if factory.is_null() {
        items.push(text("factory   no health file yet".to_string()));
    } else {
        let health = J::Obj(vec![
            ("status".to_string(), factory.get("status").clone()),
            ("time".to_string(), factory.get("time").clone()),
            (
                "output".to_string(),
                default(factory.get("output"), &J::str("")).clone(),
            ),
            (
                "checks".to_string(),
                J::Obj(vec![(
                    "queue".to_string(),
                    s.at(&["queue", "counts"]).or(&J::Null).clone(),
                )]),
            ),
        ]);
        let stale = factory.get("stale").truthy();
        items.push(Item {
            p: 9,
            n: 2,
            kind: Kind::State { health, stale },
        });
    }

    // 2. The issues behind the counts.
    for i in objects(s.at(&["queue", "issues"])) {
        items.push(item(3, Kind::Issue(i)));
    }

    // 3. The cycles.
    let cycle = s.get("cycle");
    if cycle.is_null() {
        items.push(text("cycle     no cycle written yet".to_string()));
    } else {
        let all = cycle.get("cycles");
        let listed = if !all.is_empty() {
            objects(all)
        } else {
            objects(&J::Arr(vec![cycle.get("last").clone()]))
        };
        if listed.is_empty() {
            items.push(item(9, Kind::Cycle(J::Null)));
        }
        for c in listed {
            items.push(item(9, Kind::Cycle(c)));
        }
    }

    // 4. The host, then its processes: the most expendable rows of all.
    let host = s.get("host");
    if host.is_null() {
        items.push(text("host      no reading of /proc".to_string()));
    } else {
        items.push(item(9, Kind::Host(host.clone())));
        for p in objects(host.get("processes")) {
            items.push(item(1, Kind::Process(p)));
        }
    }

    // 5. The units of the factory.
    match s.get("units") {
        J::Null => items.push(text(
            "units     systemctl --user gave no answer".to_string(),
        )),
        u if u.is_empty() => items.push(text("units     no tehas units".to_string())),
        u => {
            let active = u
                .items()
                .iter()
                .filter(|x| x.get("activeState").as_str() == Some("active"))
                .count();
            items.push(text(format!(
                "units     {} units, {active} active",
                u.len()
            )));
            for x in objects(u) {
                items.push(item(2, Kind::Unit(x)));
            }
        }
    }

    // 6. What the model mesh draws.
    let e = s.get("energy");
    items.push(text(if e.is_null() {
        "energy    no sample".to_string()
    } else {
        let q = |k: &str| default(e.get(k), &J::str("?")).text();
        format!(
            "energy    {} W now, {} of {} slots busy{}",
            q("watts"),
            q("busy"),
            q("slots"),
            stamp(e.get("time"), now)
        )
    }));

    // 7. The console of a cycle, in the rows that are left.
    let c = s.get("console");
    if c.is_null() {
        items.push(text("console   no console file".to_string()));
    } else {
        items.push(text(format!(
            "console   #{}  {}  {}",
            default(c.get("issue"), &J::str("?")).text(),
            c.get("name").text(),
            if c.get("ended").truthy() {
                "ended"
            } else {
                "live"
            }
        )));
        for l in c.get("lines").items() {
            items.push(item(2, Kind::Console(l.clone())));
        }
    }

    fit(items, rows)
        .iter()
        .flat_map(|i| draw(i, now))
        .map(|l| cut(&l, cols))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(text: &str) -> J {
        J::parse(text).unwrap()
    }

    const BASE: &str = r#"{"generatedAt":"2026-10-17T14:00:20Z",
      "factory":{"status":"pass","output":"Working on #12.","time":"2026-10-17T14:00:00Z","stale":false},
      "queue":{"counts":{"inLine":1,"building":1,"review":1,"stopped":1,"time":"2026-10-17T14:00:00Z"},
               "issues":[{"number":12,"state":"running","title":"Two cycles side by side"}]},
      "cycle":{"cycles":[{"issue":12,"running":true,"step":"develop","startedAt":"2026-10-17T13:50:20Z"}]},
      "host":{"cpus":2,"load":[0.10,0.20,0.30],"cpuPercent":33.3,"memory":{"usedMb":32,"totalMb":64},
              "processes":[{"pid":100,"name":"systemd","cpuPercent":1,"memPercent":0.5,"upSeconds":900}]},
      "units":[{"id":"tehas-dispatch.timer","unitFileState":"enabled","activeState":"active","next":"10:00:00"}],
      "energy":{"watts":312.5,"busy":4,"slots":8,"time":"2026-10-17T14:00:10Z"},
      "console":{"issue":12,"name":"console-12.txt","ended":false,"lines":["one","two"]}}"#;

    #[test]
    fn every_section_in_order() {
        assert_eq!(
            frame(&snap(BASE), 80, 24),
            [
                "tehas top   snapshot 2026-10-17 14:00:20 UTC",
                "factory   pass: Working on #12. (20 s ago)",
                "queue     1 in line, 1 building, 1 in review, 1 stopped (20 s ago)",
                "  12     running    Two cycles side by side",
                "cycle     #12 running, step develop, started 10 min ago",
                "host      2 cpus, load 0.10 0.20 0.30, cpu 33.3%, mem 32 of 64 MB",
                "      100  systemd              1%    0.5%       900 s",
                "units     1 units, 1 active",
                "  tehas-dispatch.timer        enabled  active  next 10:00:00",
                "energy    312.5 W now, 4 of 8 slots busy (10 s ago)",
                "console   #12  console-12.txt  live",
                "one",
                "two",
            ]
        );
    }

    #[test]
    fn a_short_terminal_drops_processes_then_the_later_console_lines() {
        let f = frame(&snap(BASE), 80, 11);
        assert_eq!(f.len(), 11);
        assert!(!f.iter().any(|l| l.contains("systemd")));
        assert_eq!(f[10], "one");
        let f = frame(&snap(BASE), 80, 9);
        assert!(!f
            .iter()
            .any(|l| l == "one" || l.starts_with("  tehas-dispatch")));
        assert!(f.iter().any(|l| l.contains("Two cycles")));
    }

    #[test]
    fn lines_are_cut_and_controls_taken_out() {
        let s = snap(
            r#"{"generatedAt":"2026-10-17T14:00:20Z","console":{"issue":1,"name":"c\u001b[2J","lines":["a\u0007b"]}}"#,
        );
        let f = frame(&s, 20, 24);
        assert!(f.iter().all(|l| l.chars().count() <= 20));
        assert!(f.contains(&"console   #1  c[2J  ".to_string()));
        assert!(f.contains(&"ab".to_string()));
    }

    #[test]
    fn missing_sections_are_one_line_each() {
        let f = frame(
            &snap(r#"{"generatedAt":"2026-10-17T14:00:20Z","units":[]}"#),
            80,
            24,
        );
        assert_eq!(
            f[1..],
            [
                "factory   no health file yet",
                "cycle     no cycle written yet",
                "host      no reading of /proc",
                "units     no tehas units",
                "energy    no sample",
                "console   no console file",
            ]
        );
    }

    #[test]
    fn a_cycle_record_with_no_cycle() {
        let f = frame(
            &snap(r#"{"generatedAt":"2026-10-17T14:00:20Z","cycle":{"cycles":[],"last":null}}"#),
            80,
            24,
        );
        assert!(f.contains(&"cycle     none yet".to_string()));
    }
}
