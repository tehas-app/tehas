//! tehasbot energy: the dashboard's energy file, $TEHAS_HOME/live/energy.json.
//!
//! One sample of what the model mesh draws, for the Energy use section of the
//! game view. It works only when the model backend is a viiwork mesh: it asks
//! the mesh for its cluster snapshot (GET /v1/cluster) and keeps only the
//! machines that are alive and serve a model the factory uses, and only those
//! models. Per machine, the draw now, the kWh of the last 24 hours and 30 days
//! and host RAM; per model, the slots, how many are busy and what serves it;
//! and the mesh's draw over the last 30 minutes, one sample per 15 seconds,
//! with the machines' running token counts where the mesh reports one.
//!
//! The dashboard can be on the internet, and so can this file, so what is
//! copied is a list: `snapshot` names every field. An engine version is copied
//! only when it is a plain version. Addresses, node ids and GPUs are not.
//!
//! The mesh is the one the factory's subagent provider is on. The dispatcher
//! writes the word main or backup in live/dispatch.json; the sampler asks only
//! that endpoint, with its token. With no word there it asks main and then
//! backup, so an update of the dispatcher never blanks the floor.
//! TEHAS_ENERGY_URL names a cluster URL instead, asked with no token.
//! TEHAS_ENERGY=off removes the file.
//!
//!   one answers with a cluster snapshot        the file is rewritten
//!   else one gives no answer, a timeout, a     the file is left as it is
//!   server error (5xx) or a busy answer
//!   (408, 429)
//!   any other answers                          the file is removed: not viiwork
//!
//! Loads no settings: what it needs is in the unit's environment.
//!
//! Usage: console energy               one sample
//!        console energy --every       one sample at every 15-second mark, for ever
//!
//! Prints nothing. Exit: 0 unless the file cannot be written; 64 usage; 78
//! there is no subagent provider to ask, which also ends --every.

use crate::jq::{obj, J};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tehas_core::models::{auth, host_path, ModelFile};
use tehas_core::time;

const PERIOD: i64 = 15;
const WINDOW: f64 = 1800.0;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

pub fn main(given: &[String]) -> u8 {
    let home = match env("TEHAS_HOME") {
        h if h.is_empty() => format!("{}/tehas", env("HOME")),
        h => h,
    };
    match given.first().map(String::as_str) {
        None | Some("") => sample(Path::new(&home)),
        Some("--every") => every(&home),
        _ => {
            eprintln!("usage: tehasbot energy [--every]");
            64
        }
    }
}

// A child per mark, by the path of the installed console, so an update reaches
// the loop: the unit is not restarted on a console change, and current_exe
// reads "(deleted)" once an install has replaced the file.
fn every(home: &str) -> u8 {
    let console = format!("{home}/libexec/console");
    loop {
        let done = Command::new(&console).arg("energy").status();
        if done.is_ok_and(|s| s.code() == Some(78)) {
            return 78;
        }
        std::thread::sleep(Duration::from_secs(to_mark(time::now())));
    }
}

fn to_mark(now: i64) -> u64 {
    (PERIOD - now.rem_euclid(PERIOD)) as u64
}

fn sample(home: &Path) -> u8 {
    let live = home.join("live");
    let file = live.join("energy.json");
    if env("TEHAS_ENERGY") == "off" {
        let _ = std::fs::create_dir_all(&live);
        let _ = std::fs::remove_file(&file);
        return 0;
    }
    let asks = match asks(&live) {
        Ok(asks) => asks,
        Err(word) => {
            eprintln!("tehas: energy: {word}");
            return 78;
        }
    };
    let _ = std::fs::create_dir_all(&live);
    if asks.is_empty() {
        let _ = std::fs::remove_file(&file);
        return 0;
    }
    let mut later = false;
    let mut body = None;
    for (url, key) in &asks {
        match ask(url, key) {
            Answer::Later => later = true,
            Answer::Other => {}
            Answer::Snapshot(j) => {
                body = Some(j);
                break;
            }
        }
    }
    let Some(body) = body else {
        if !later {
            let _ = std::fs::remove_file(&file);
        }
        return 0;
    };
    let prev = read_lossy(&file)
        .and_then(|t| J::parse_first(&t))
        .unwrap_or(J::Null);
    let Some(now) = clock() else { return 1 };
    write(&file, &snapshot(&body, &prev, &now, &factory_models()))
}

// TEHAS_ENERGY_NOW holds the sample's time for a test. What is no number
// stops the sample before the file is touched.
fn clock() -> Option<J> {
    match env("TEHAS_ENERGY_NOW") {
        n if n.is_empty() => Some(J::num(time::now() as f64)),
        n => J::parse(&n).filter(|j| matches!(j, J::Num(_))),
    }
}

fn read_lossy(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The one word the last poll settled on; anything else is no word at all.
fn settled(live: &Path) -> Option<&'static str> {
    let text = read_lossy(&live.join("dispatch.json"))?;
    match J::parse(&text)?.get("endpoint").as_str()? {
        "main" => Some("main"),
        "backup" => Some("backup"),
        _ => None,
    }
}

/// `<base>/v1/cluster` of an endpoint's base url.
fn cluster_url(base: &str) -> String {
    let base = base.strip_suffix('/').unwrap_or(base);
    format!("{}/v1/cluster", base.strip_suffix("/v1").unwrap_or(base))
}

/// Who to ask, each url with its key. A sampler with no url and no provider
/// has no setting to ask by, and names no product in place of one.
fn asks(live: &Path) -> Result<Vec<(String, String)>, String> {
    let url = env("TEHAS_ENERGY_URL");
    if !url.is_empty() {
        return Ok(vec![(url, String::new())]);
    }
    let provider = env("PI_ENSEMBLE_SUBAGENT_PROVIDER");
    if provider.is_empty() {
        return Err("PI_ENSEMBLE_SUBAGENT_PROVIDER is not set".into());
    }
    let path = match env("TEHAS_MODELS_FILE") {
        p if p.is_empty() => host_path(),
        p => p,
    };
    let models = ModelFile::read(&path);
    Ok(ends(settled(live))
        .iter()
        .filter_map(|end| {
            let base = models.url(&provider, end);
            (!base.is_empty()).then(|| (cluster_url(&base), models.key(&provider, end)))
        })
        .collect())
}

/// The named endpoint alone, so an endpoint the cycles are not on cannot
/// answer for the factory's draw. With none named, both.
fn ends(word: Option<&'static str>) -> Vec<&'static str> {
    word.map_or(vec!["main", "backup"], |w| vec![w])
}

enum Answer {
    Snapshot(J),
    /// No answer, a server error or a busy answer: the file stays.
    Later,
    Other,
}

fn ask(url: &str, key: &str) -> Answer {
    let spawned = Command::new("curl")
        .args(["-sS", "-K", "-", "-m", "5", "-w", r"\n%{http_code}", url])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = spawned else {
        return Answer::Later;
    };
    if let Some(mut stdin) = child.stdin.take() {
        // A curl that left without reading gives its own exit status.
        let _ = stdin.write_all(auth(key).as_bytes());
    }
    let Ok(out) = child.wait_with_output() else {
        return Answer::Later;
    };
    if !out.status.success() {
        return Answer::Later;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let text = text.trim_end_matches('\n');
    let (body, code) = text.rsplit_once('\n').unwrap_or((text, text));
    judge(code, body)
}

fn judge(code: &str, body: &str) -> Answer {
    if code.starts_with('5') || code == "408" || code == "429" {
        return Answer::Later;
    }
    if code != "200" {
        return Answer::Other;
    }
    match J::parse(body) {
        Some(j) if matches!(j.get("members"), J::Arr(_)) => Answer::Snapshot(j),
        _ => Answer::Other,
    }
}

fn write(file: &Path, value: &J) -> u8 {
    let mut temp = file.as_os_str().to_owned();
    temp.push(format!(".{}.new", std::process::id()));
    let temp = PathBuf::from(temp);
    let done = std::fs::write(&temp, format!("{}\n", value.pretty()))
        .and_then(|()| std::fs::rename(&temp, file));
    if done.is_err() {
        let _ = std::fs::remove_file(&temp);
        return 1;
    }
    0
}

/// The models the factory uses: PI_ENSEMBLE_SUBAGENT_MODEL and each
/// PI_ENSEMBLE_MODEL_<ROLE>, sorted, each once.
fn factory_models() -> Vec<String> {
    let mut models: Vec<String> = std::env::vars_os()
        .filter(|(k, _)| k.to_str().is_some_and(names_a_model))
        .map(|(_, v)| v.to_string_lossy().into_owned())
        .filter(|v| !v.is_empty())
        .collect();
    models.sort();
    models.dedup();
    models
}

fn names_a_model(name: &str) -> bool {
    name == "PI_ENSEMBLE_SUBAGENT_MODEL"
        || name.strip_prefix("PI_ENSEMBLE_MODEL_").is_some_and(|r| {
            !r.is_empty()
                && r.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        })
}

fn r2(n: f64) -> f64 {
    (n * 100.0).round() / 100.0
}

/// `if type == "number" and . > 0 then r2 else null`
fn positive(v: &J) -> Option<f64> {
    v.as_f64().filter(|n| *n > 0.0).map(r2)
}

fn num(v: &J) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// Non-empty text.
fn text(v: &J) -> Option<&str> {
    v.as_str().filter(|s| !s.is_empty())
}

/// What an engine prints after its number can be a path or a host name.
fn plain(v: &J) -> Option<&str> {
    text(v).filter(|s| {
        s.len() <= 40
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'_' | b'-'))
    })
}

/// `.[]?`: the items of an array, the values of an object, else nothing.
fn each(v: &J) -> Vec<&J> {
    match v {
        J::Arr(a) => a.iter().collect(),
        J::Obj(o) => o.iter().map(|(_, v)| v).collect(),
        _ => Vec::new(),
    }
}

fn opt_num(v: Option<f64>) -> J {
    v.map_or(J::Null, J::num)
}

fn opt_str(v: Option<&str>) -> J {
    v.map_or(J::Null, J::str)
}

/// A number the mesh sent, or null.
fn copied(v: &J) -> J {
    if matches!(v, J::Num(_)) {
        v.clone()
    } else {
        J::Null
    }
}

/// A map's entry, the later value in the earlier place, as from_entries does.
fn put(map: &mut Vec<(String, J)>, key: &str, value: J) {
    match map.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value,
        None => map.push((key.to_string(), value)),
    }
}

struct Host {
    name: String,
    source: Option<String>,
    watts: Option<f64>,
    kwh24: Option<f64>,
    kwh30: Option<f64>,
    ram_used: J,
    ram_total: J,
}

fn host(node: &str, s: &J) -> Host {
    let p = s.get("power");
    let w = p.get("watts");
    let reads = p.get("available").is_true() && w.as_f64().is_some_and(|n| n >= 0.0);
    Host {
        name: node.to_string(),
        source: p.get("source").as_str().filter(|_| reads).map(String::from),
        watts: w.as_f64().filter(|_| reads).map(f64::round),
        kwh24: positive(s.get("energy_kwh_24h")),
        kwh30: positive(s.get("energy_kwh_30d")),
        ram_used: copied(s.get("host_mem_used_mb")),
        ram_total: copied(s.get("host_mem_total_mb")),
    }
}

impl Host {
    fn json(&self) -> J {
        obj(vec![
            ("name", J::str(&self.name)),
            ("source", opt_str(self.source.as_deref())),
            ("watts", opt_num(self.watts)),
            ("kWh24h", opt_num(self.kwh24)),
            ("kWh30d", opt_num(self.kwh30)),
            ("ramUsedMb", self.ram_used.clone()),
            ("ramTotalMb", self.ram_total.clone()),
        ])
    }
}

/// `total(f)`: the sum of what is there, or null.
fn total(values: impl Iterator<Item = Option<f64>>) -> J {
    let there: Vec<f64> = values.flatten().collect();
    if there.is_empty() {
        J::Null
    } else {
        J::num(r2(there.iter().sum()))
    }
}

/// What serves a model on one machine: in the order `unique` sorts the object
/// by, its keys sorted, with null before text.
type Engine = (
    Option<String>, // engine
    Option<String>, // engineName
    Option<String>, // engineVersion
    Option<String>, // version
);

struct Row {
    host: String,
    engine: Engine,
    slots: f64,
    busy: f64,
    queued: f64,
}

/// One model's machines: `node` and its version, each model of the status.
fn model_rows<'a>(alive: &[(&'a str, &'a J)], factory: &[String]) -> BTreeMap<&'a str, Vec<Row>> {
    let mut rows: BTreeMap<&str, Vec<Row>> = BTreeMap::new();
    for (node, s) in alive {
        let version = text(s.get("ver"))
            .map(|v| v.strip_prefix('v').unwrap_or(v))
            .filter(|v| !v.is_empty());
        for m in each(s.get("models")) {
            let Some(name) = m.get("name").as_str() else {
                continue;
            };
            if !is_factory(m, factory) {
                continue;
            }
            rows.entry(name).or_default().push(Row {
                host: node.to_string(),
                engine: (
                    text(m.get("engine")).map(String::from),
                    text(m.get("engine_name")).map(String::from),
                    plain(m.get("engine_version")).map(String::from),
                    version.map(String::from),
                ),
                slots: num(m.get("slots")),
                busy: num(m.get("busy")),
                queued: num(m.get("queued")),
            });
        }
    }
    rows
}

fn model(name: &str, rows: &[Row]) -> Option<J> {
    let sum = |f: fn(&Row) -> f64| rows.iter().map(f).sum::<f64>();
    let slots = sum(|r| r.slots);
    if slots <= 0.0 {
        return None;
    }
    let serving = || rows.iter().filter(|r| r.slots > 0.0);
    let mut hosts: Vec<&str> = serving().map(|r| r.host.as_str()).collect();
    hosts.sort_unstable();
    hosts.dedup();
    // An engine version alone names nothing and is left out.
    let mut engines: Vec<&Engine> = serving()
        .map(|r| &r.engine)
        .filter(|(e, n, _, v)| e.is_some() || n.is_some() || v.is_some())
        .collect();
    engines.sort();
    engines.dedup();
    Some(obj(vec![
        ("name", J::str(name)),
        ("hosts", J::Arr(hosts.into_iter().map(J::str).collect())),
        (
            "engines",
            J::Arr(
                engines
                    .into_iter()
                    .map(|(engine, name, own, version)| {
                        obj(vec![
                            ("version", opt_str(version.as_deref())),
                            ("engine", opt_str(engine.as_deref())),
                            ("engineName", opt_str(name.as_deref())),
                            ("engineVersion", opt_str(own.as_deref())),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("slots", J::num(slots)),
        ("busy", J::num(sum(|r| r.busy))),
        ("queued", J::num(sum(|r| r.queued))),
    ]))
}

/// Whether an entry of a machine's models is one the factory uses.
fn is_factory(model: &J, factory: &[String]) -> bool {
    model
        .get("name")
        .as_str()
        .is_some_and(|n| factory.iter().any(|f| f == n))
}

/// The machine's running count for the factory's models, where it has one.
fn tokens(s: &J, factory: &[String]) -> Option<f64> {
    let counts: Vec<f64> = each(s.get("models"))
        .into_iter()
        .filter(|m| is_factory(m, factory))
        .filter_map(|m| m.get("tokens_total").as_f64())
        .filter(|n| *n >= 0.0)
        .collect();
    (!counts.is_empty()).then(|| counts.iter().sum::<f64>().floor())
}

fn slot(t: f64) -> f64 {
    (t / PERIOD as f64).floor()
}

/// The samples of the last file that stay: one per slot, the new one taking
/// its slot, none older than the window or from a clock that has gone back.
fn kept_samples(prev: &J, now: f64) -> Vec<J> {
    prev.get("samples")
        .items()
        .iter()
        .filter(|s| matches!(s.get("w"), J::Obj(_)))
        .filter(|s| {
            s.get("t")
                .as_f64()
                .is_some_and(|t| t > now - WINDOW && t <= now && slot(t) != slot(now))
        })
        .cloned()
        .collect()
}

/// The energy file from a cluster snapshot and the file before it. Every field
/// that reaches the file is named here: it can be on the internet.
fn snapshot(cluster: &J, prev: &J, now: &J, factory: &[String]) -> J {
    let at = now.as_f64().unwrap_or(0.0);
    let mut alive: Vec<(&str, &J)> = cluster
        .get("members")
        .items()
        .iter()
        .filter(|m| m.get("state").as_str() == Some("alive"))
        .filter(|m| matches!(m.get("status"), J::Obj(_)))
        .filter_map(|m| Some((m.get("node").as_str()?, m.get("status"))))
        .collect();
    alive.sort_by_key(|(node, _)| *node);

    let serves = |s: &J| {
        each(s.get("models"))
            .into_iter()
            .any(|m| num(m.get("slots")) > 0.0 && is_factory(m, factory))
    };
    let hosts: Vec<Host> = alive
        .iter()
        .filter(|(_, s)| serves(s))
        .map(|(node, s)| host(node, s))
        .collect();
    let models: Vec<J> = model_rows(&alive, factory)
        .iter()
        .filter_map(|(name, rows)| model(name, rows))
        .collect();

    let mut w = Vec::new();
    for h in &hosts {
        if let Some(watts) = h.watts {
            put(&mut w, &h.name, J::num(watts));
        }
    }
    let mut k = Vec::new();
    for (node, s) in &alive {
        if let Some(count) = tokens(s, factory) {
            put(&mut k, node, J::num(count));
        }
    }
    let mut new = vec![("t", now.clone()), ("w", J::Obj(w))];
    if !k.is_empty() {
        new.push(("k", J::Obj(k)));
    }
    let mut samples = kept_samples(prev, at);
    samples.push(obj(new));
    samples.sort_by(|a, b| a.get("t").cmp(b.get("t")));

    obj(vec![
        ("generatedAt", J::str(time::iso(at.floor() as i64))),
        ("source", J::str("viiwork")),
        ("sourceUrl", J::str("https://github.com/janit/viiwork")),
        ("periodSeconds", J::num(PERIOD as f64)),
        (
            "factoryModels",
            J::Arr(factory.iter().map(J::str).collect()),
        ),
        (
            "totals",
            obj(vec![
                ("watts", total(hosts.iter().map(|h| h.watts))),
                ("kWh24h", total(hosts.iter().map(|h| h.kwh24))),
                ("kWh30d", total(hosts.iter().map(|h| h.kwh30))),
                ("hosts", J::num(hosts.len() as f64)),
                (
                    "reporting",
                    J::num(hosts.iter().filter(|h| h.watts.is_some()).count() as f64),
                ),
                (
                    "energyHosts",
                    J::num(hosts.iter().filter(|h| h.kwh24.is_some()).count() as f64),
                ),
            ]),
        ),
        ("hosts", J::Arr(hosts.iter().map(Host::json).collect())),
        ("models", J::Arr(models)),
        ("samples", J::Arr(samples)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> J {
        J::parse(text).unwrap()
    }

    fn file(cluster: &str, prev: &str, now: i64, factory: &[&str]) -> J {
        let factory: Vec<String> = factory.iter().map(|s| s.to_string()).collect();
        snapshot(&parse(cluster), &parse(prev), &J::num(now as f64), &factory)
    }

    fn one_machine(status: &str) -> String {
        format!(r#"{{"members": [{{"node": "n", "state": "alive", "status": {status}}}]}}"#)
    }

    #[test]
    fn rounding_is_half_away_from_zero_to_two_places() {
        assert_eq!(r2(1.4449), 1.44);
        assert_eq!(r2(55.0826), 55.08);
        assert_eq!(r2(0.125), 0.13);
        assert_eq!(positive(&parse("0")), None);
        assert_eq!(positive(&parse("-1.5")), None);
        assert_eq!(positive(&parse(r#""7""#)), None);
    }

    #[test]
    fn watts_are_whole_and_print_without_a_fraction() {
        let got = file(
            &one_machine(
                r#"{"power": {"available": true, "watts": 2.5, "source": "rapl"},
                    "models": [{"name": "m", "slots": 1}]}"#,
            ),
            "null",
            1800,
            &["m"],
        );
        assert_eq!(got.at(&["totals", "watts"]).dump(), "3");
        assert_eq!(got.get("hosts").items()[0].get("watts").dump(), "3");
    }

    #[test]
    fn the_file_keeps_the_order_of_its_keys() {
        let got = file(
            &one_machine(r#"{"models": [{"name": "m", "slots": 1, "tokens_total": 9.7}]}"#),
            "null",
            1800,
            &["m"],
        );
        assert_eq!(
            got.dump(),
            concat!(
                r#"{"generatedAt":"1970-01-01T00:30:00Z","source":"viiwork","#,
                r#""sourceUrl":"https://github.com/janit/viiwork","periodSeconds":15,"#,
                r#""factoryModels":["m"],"totals":{"watts":null,"kWh24h":null,"kWh30d":null,"#,
                r#""hosts":1,"reporting":0,"energyHosts":0},"#,
                r#""hosts":[{"name":"n","source":null,"watts":null,"kWh24h":null,"kWh30d":null,"#,
                r#""ramUsedMb":null,"ramTotalMb":null}],"#,
                r#""models":[{"name":"m","hosts":["n"],"engines":[],"slots":1,"busy":0,"queued":0}],"#,
                r#""samples":[{"t":1800,"w":{},"k":{"n":9}}]}"#
            )
        );
    }

    #[test]
    fn one_sample_per_slot_within_the_window() {
        let now = 3600.0;
        let prev = parse(
            r#"{"samples": [
              {"t": 1800, "w": {}}, {"t": 1801, "w": {}}, {"t": 3599, "w": {}},
              {"t": 3600, "w": {}}, {"t": 3614, "w": {}}, {"t": 3615, "w": {}},
              {"t": "soon", "w": {}}, {"t": 2000}, 7]}"#,
        );
        let kept: Vec<String> = kept_samples(&prev, now)
            .iter()
            .map(|s| s.get("t").dump())
            .collect();
        assert_eq!(kept, ["1801", "3599"]);
        assert!(kept_samples(&parse(r#"{"samples": 5}"#), now).is_empty());
        assert!(kept_samples(&J::Null, now).is_empty());
    }

    #[test]
    fn a_kept_sample_is_copied_as_it_was() {
        let got = file(
            &one_machine("{}"),
            r#"{"samples": [{"t": 1785, "w": {"b": 1, "a": 2.50}, "k": {"b": 3}}]}"#,
            1800,
            &[],
        );
        assert_eq!(
            got.get("samples").items()[0].dump(),
            r#"{"t":1785,"w":{"b":1,"a":2.50},"k":{"b":3}}"#
        );
    }

    #[test]
    fn only_a_plain_version_is_copied() {
        for good in ["b11371", "0.31.0+cu12_x-1", &"b".repeat(40)] {
            assert_eq!(plain(&J::str(good)), Some(good));
        }
        for bad in ["", "b1 /opt/x", "x@host", "b1\n", "é", &"b".repeat(41)] {
            assert_eq!(plain(&J::str(bad)), None, "{bad:?}");
        }
        assert_eq!(plain(&parse("7")), None);
        assert_eq!(plain(&parse(r#"{"m": "x"}"#)), None);
    }

    #[test]
    fn engines_sort_as_jq_sorts_objects_and_null_comes_first() {
        let status = |ver: &str, engine: &str| {
            format!(
                r#"{{"ver": "{ver}", "models": [{{"name": "m", "slots": 1, "engine": "{engine}"}}]}}"#
            )
        };
        let cluster = format!(
            r#"{{"members": [
              {{"node": "a", "state": "alive", "status": {}}},
              {{"node": "b", "state": "alive", "status": {}}},
              {{"node": "c", "state": "alive", "status": {}}},
              {{"node": "d", "state": "alive", "status": {}}}]}}"#,
            status("v2.9.0", "b"),
            status("v2.10.0", "b"),
            status("v9", ""),
            status("v2.10.0", "b"),
        );
        let got = file(&cluster, "null", 1800, &["m"]);
        let engines = got.get("models").items()[0].get("engines").dump();
        assert_eq!(
            engines,
            concat!(
                r#"[{"version":"9","engine":null,"engineName":null,"engineVersion":null},"#,
                r#"{"version":"2.10.0","engine":"b","engineName":null,"engineVersion":null},"#,
                r#"{"version":"2.9.0","engine":"b","engineName":null,"engineVersion":null}]"#
            )
        );
    }

    #[test]
    fn a_machine_named_twice_keeps_the_later_reading_in_the_earlier_place() {
        let mut w = Vec::new();
        put(&mut w, "a", J::num(1.0));
        put(&mut w, "b", J::num(2.0));
        put(&mut w, "a", J::num(3.0));
        assert_eq!(J::Obj(w).dump(), r#"{"a":3,"b":2}"#);
    }

    #[test]
    fn the_wait_ends_on_a_mark() {
        assert_eq!(to_mark(1791526545), 15);
        assert_eq!(to_mark(1791526546), 14);
        assert_eq!(to_mark(1791526559), 1);
    }

    #[test]
    fn the_cluster_url_comes_from_the_base_url() {
        let want = "http://mesh.example:8086/v1/cluster";
        for base in [
            "http://mesh.example:8086",
            "http://mesh.example:8086/",
            "http://mesh.example:8086/v1",
            "http://mesh.example:8086/v1/",
        ] {
            assert_eq!(cluster_url(base), want);
        }
    }

    #[test]
    fn a_named_endpoint_is_asked_alone() {
        assert_eq!(ends(Some("backup")), ["backup"]);
        assert_eq!(ends(Some("main")), ["main"]);
        assert_eq!(ends(None), ["main", "backup"]);
    }

    #[test]
    fn what_an_answer_means() {
        let later = |c: &str, b: &str| matches!(judge(c, b), Answer::Later);
        let other = |c: &str, b: &str| matches!(judge(c, b), Answer::Other);
        for code in ["500", "502", "503", "408", "429"] {
            assert!(later(code, "{}"), "{code}");
        }
        assert!(other("404", r#"{"members": []}"#));
        for body in ["", "[]", "<html>", r#"{"data": []}"#, r#"{"members": 5}"#] {
            assert!(other("200", body), "{body}");
        }
        assert!(matches!(
            judge("200", r#"{"members": []}"#),
            Answer::Snapshot(_)
        ));
    }

    #[test]
    fn the_factory_names_a_model_by_two_kinds_of_setting() {
        assert!(names_a_model("PI_ENSEMBLE_SUBAGENT_MODEL"));
        assert!(names_a_model("PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST"));
        assert!(names_a_model("PI_ENSEMBLE_MODEL_R2"));
        assert!(!names_a_model("PI_ENSEMBLE_MODEL_"));
        assert!(!names_a_model("PI_ENSEMBLE_MODEL_lower"));
        assert!(!names_a_model("PI_ENSEMBLE_MAX_WORKSTREAMS"));
    }
}
