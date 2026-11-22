//! The mesh read of the 2.1 `feed:mesh`: one sample of what a viiwork model
//! mesh draws, cut to the fields a view may show.
//!
//! The host asks the mesh for its cluster snapshot (GET /v1/cluster) and keeps
//! only the machines that are alive and serve a model the factory uses, and
//! only those models. Per machine, the draw now, the kWh of the last 24 hours
//! and 30 days and host RAM; per model, the slots, how many are busy and what
//! serves it; and the mesh's draw over the last 30 minutes, one sample per 15
//! seconds, with the machines' running token counts where the mesh reports
//! one.
//!
//! The feed can be on the internet, so what is copied is a list: [`snapshot`]
//! names every field. An engine version is copied only when it is a plain
//! version. Addresses, node ids and GPUs are not.
//!
//! The mesh is the one the factory's subagent provider is on, asked with that
//! endpoint's token, the endpoint the dispatcher settled on first. A cluster
//! URL given instead is asked with no token.
//!
//!   one answers with a cluster snapshot        the feed is rewritten
//!   else one gives no answer, a timeout, a     the feed is left as it is
//!   server error (5xx) or a busy answer
//!   (408, 429)
//!   any other answers                          the feed is removed: not viiwork
//!
//! Until 2.0 this was `tehasbot energy`, which wrote live/energy.json. The unit
//! that samples and writes the feed comes with the feed in 2.1.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tehas_core::jq::{obj, J};
use tehas_core::models::{auth, ModelFile};
use tehas_core::time;

/// Seconds between samples.
pub const PERIOD: i64 = 15;
/// Seconds of draw a sample keeps.
pub const WINDOW: f64 = 1800.0;

fn read_lossy(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The one word the last poll settled on, from live/dispatch.json; anything
/// else is no word at all.
pub fn settled(live: &Path) -> Option<&'static str> {
    let text = read_lossy(&live.join("dispatch.json"))?;
    match J::parse(&text)?.get("endpoint").as_str()? {
        "main" => Some("main"),
        "backup" => Some("backup"),
        _ => None,
    }
}

/// `<base>/v1/cluster` of an endpoint's base url.
pub fn cluster_url(base: &str) -> String {
    let base = base.strip_suffix('/').unwrap_or(base);
    format!("{}/v1/cluster", base.strip_suffix("/v1").unwrap_or(base))
}

/// Who to ask, each url with its key. A cluster URL is asked alone and with
/// no key. Else the provider's endpoints in `models`, the settled `word` alone
/// when there is one. With neither there is nothing to ask by, and no product
/// is named in place of one.
pub fn asks(
    url: &str,
    provider: &str,
    models: &ModelFile,
    word: Option<&'static str>,
) -> Result<Vec<(String, String)>, String> {
    if !url.is_empty() {
        return Ok(vec![(url.to_string(), String::new())]);
    }
    if provider.is_empty() {
        return Err("no subagent provider is set".into());
    }
    Ok(ends(word)
        .iter()
        .filter_map(|end| {
            let base = models.url(provider, end);
            (!base.is_empty()).then(|| (cluster_url(&base), models.key(provider, end)))
        })
        .collect())
}

/// The named endpoint alone, so an endpoint the cycles are not on cannot
/// answer for the factory's draw. With none named, both.
pub fn ends(word: Option<&'static str>) -> Vec<&'static str> {
    word.map_or(vec!["main", "backup"], |w| vec![w])
}

/// What an ask gave.
pub enum Answer {
    /// A cluster snapshot: the feed is rewritten.
    Snapshot(J),
    /// No answer, a server error or a busy answer: the feed stays.
    Later,
    /// Any other answer: the feed is removed.
    Other,
}

/// Each ask in turn, until one gives a snapshot. With none, [`Answer::Later`]
/// when any said later, else [`Answer::Other`].
pub fn read(asks: &[(String, String)]) -> Answer {
    let mut later = false;
    for (url, key) in asks {
        match ask(url, key) {
            Answer::Snapshot(j) => return Answer::Snapshot(j),
            Answer::Later => later = true,
            Answer::Other => {}
        }
    }
    if later {
        Answer::Later
    } else {
        Answer::Other
    }
}

/// One ask of a cluster URL by curl, the key on its standard input.
pub fn ask(url: &str, key: &str) -> Answer {
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

/// The answer an HTTP status and body mean.
pub fn judge(code: &str, body: &str) -> Answer {
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

/// The models the factory uses: PI_ENSEMBLE_SUBAGENT_MODEL and each
/// PI_ENSEMBLE_MODEL_<ROLE>, sorted, each once.
pub fn factory_models() -> Vec<String> {
    let mut models: Vec<String> = std::env::vars_os()
        .filter(|(k, _)| k.to_str().is_some_and(names_a_model))
        .map(|(_, v)| v.to_string_lossy().into_owned())
        .filter(|v| !v.is_empty())
        .collect();
    models.sort();
    models.dedup();
    models
}

pub fn names_a_model(name: &str) -> bool {
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

/// The samples of the last feed that stay: one per slot, the new one taking
/// its slot, none older than the window or from a clock that has gone back.
pub fn kept_samples(prev: &J, now: f64) -> Vec<J> {
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

/// The address a view may link as the source of the numbers: only an
/// `https://` one. A value that is no address is not published.
pub fn source_url(given: &str) -> Option<String> {
    let given = given.trim();
    given.starts_with("https://").then(|| given.to_string())
}

/// The feed from a cluster snapshot and the feed before it. Every field that
/// reaches the feed is named here: it can be on the internet.
pub fn snapshot(cluster: &J, prev: &J, now: &J, factory: &[String], source: Option<&str>) -> J {
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

    let mut fields = vec![
        ("generatedAt", J::str(time::iso(at.floor() as i64))),
        ("source", J::str("viiwork")),
    ];
    if let Some(url) = source {
        fields.push(("sourceUrl", J::str(url)));
    }
    fields.extend(vec![
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
    ]);
    obj(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> J {
        J::parse(text).unwrap()
    }

    fn file(cluster: &str, prev: &str, now: i64, factory: &[&str]) -> J {
        let factory: Vec<String> = factory.iter().map(|s| s.to_string()).collect();
        // The address is the host's to give: no case of the file gives one.
        snapshot(
            &parse(cluster),
            &parse(prev),
            &J::num(now as f64),
            &factory,
            None,
        )
    }

    #[test]
    fn only_an_https_setting_gives_the_file_a_source_address() {
        // The address is a host setting, and the file carries only an https
        // one. A value the sampler turns down reaches the file not at all: the
        // LEAKMARK of these values shows it.
        let cluster = one_machine(r#"{"models": [{"name": "m", "slots": 1}]}"#);
        let with = |given: &str| {
            let source = source_url(given);
            snapshot(
                &parse(&cluster),
                &J::Null,
                &J::num(1800.0),
                &["m".to_string()],
                source.as_deref(),
            )
        };
        assert_eq!(
            with(" https://example.com/mesh ").at(&["sourceUrl"]).dump(),
            "\"https://example.com/mesh\""
        );
        for weak in [
            "",
            "   ",
            "http://LEAKMARK.example",
            "https:/LEAKMARK",
            "HTTPS://LEAKMARK",
        ] {
            let got = with(weak);
            assert_eq!(got.at(&["sourceUrl"]).dump(), "null", "{weak:?}");
            assert!(!got.dump().contains("LEAKMARK"), "{weak:?}");
        }
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
                r#""periodSeconds":15,"#,
                r#""factoryModels":["m"],"totals":{"watts":null,"kWh24h":null,"kWh30d":null,"#,
                r#""hosts":1,"reporting":0,"energyHosts":0},"#,
                r#""hosts":[{"name":"n","source":null,"watts":null,"kWh24h":null,"kWh30d":null,"#,
                r#""ramUsedMb":null,"ramTotalMb":null}],"#,
                r#""models":[{"name":"m","hosts":["n"],"engines":[],"slots":1,"busy":0,"queued":0}],"#,
                r#""samples":[{"t":1800,"w":{},"k":{"n":9}}]}"#
            )
        );
        // With an address the key follows `source` and nothing else moves.
        let cluster =
            one_machine(r#"{"models": [{"name": "m", "slots": 1, "tokens_total": 9.7}]}"#);
        let with = snapshot(
            &parse(&cluster),
            &J::Null,
            &J::num(1800.0),
            &["m".to_string()],
            Some("https://example.com/mesh"),
        );
        assert_eq!(
            with.dump(),
            concat!(
                r#"{"generatedAt":"1970-01-01T00:30:00Z","source":"viiwork","sourceUrl":"https://example.com/mesh","periodSeconds":15,"#,
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
    fn a_cluster_url_is_asked_alone_and_no_provider_is_nothing_to_ask_by() {
        let none = ModelFile::read("/nonexistent/models.json");
        let url = "http://mesh.example:8086/v1/cluster";
        assert_eq!(
            asks(url, "", &none, Some("main")).unwrap(),
            [(url.to_string(), String::new())]
        );
        assert!(asks("", "", &none, None).is_err());
        assert!(asks("", "p", &none, None).unwrap().is_empty());
    }

    #[test]
    fn only_main_or_backup_is_a_settled_word() {
        let dir = std::env::temp_dir().join(format!("mesh-feed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (text, want) in [
            (r#"{"endpoint": "backup"}"#, Some("backup")),
            (r#"{"endpoint": "main"}"#, Some("main")),
            (r#"{"endpoint": "other"}"#, None),
            ("not json", None),
        ] {
            std::fs::write(dir.join("dispatch.json"), text).unwrap();
            assert_eq!(settled(&dir), want, "{text}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(settled(&dir), None);
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
