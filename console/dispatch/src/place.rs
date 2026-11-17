//! Where a start runs, under the outsource feature (still being built): on
//! this host, or on a contractor. In-house unless the feature is on, the
//! repository is in TEHAS_OUTSOURCE_REPOS, the issue has lost fewer jobs than
//! TEHAS_OUTSOURCE_RETRIES, and a contractor fits. A contractor fits when it
//! was heard from within TEHAS_OUTSOURCE_TIMEOUT, is not revoked, draining or
//! resting, holds no open job and was given none this poll, and its last
//! report names this host's image digest and the room a cycle needs. Of those,
//! the one with the most memory free, then the most disk, then the first name.
//!
//! What a poll could not read is no fit: a read that failed keeps the cycle
//! in-house, never sends it out on a guess.
//!
//! The snapshot's `outsource` object, present only with the feature on:
//!   digest       the sandbox image's id on this host, empty when unread
//!   contractors  the rows of `store contractor-list`, or {"failed": true};
//!                a row's `report` is {digest, rukas, mem_mb, disk_kb, at},
//!                what the contractor last said it has and the second the
//!                factory took it; a report older than the timeout is no fit
//!   busy         the contractors of the running cycles' open jobs, or
//!                {"failed": true}
//!   lost         by issue, the jobs it lost, from `store job-attempts`; an
//!                issue it does not name was not read

use serde_json::Value;
use std::collections::BTreeMap;

/// The settings of the feature, held only when it is on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    /// TEHAS_OUTSOURCE_REPOS: owner/name, one space apart.
    pub repos: Vec<String>,
    /// TEHAS_OUTSOURCE_RETRIES: the lost jobs after which an issue runs here.
    pub retries: u64,
    /// TEHAS_OUTSOURCE_TIMEOUT: a contractor not heard from for longer is
    /// silent.
    pub timeout: u64,
}

impl Rules {
    /// The rules out of the settings, or None when the feature is off. `get`
    /// reads one setting; the registry decides whether the feature is on.
    pub fn of(get: &dyn Fn(&str) -> String) -> Option<Rules> {
        if !tehas_core::features::on_in("outsource", get) {
            return None;
        }
        let n = |k: &str, d: u64| get(k).trim().parse().unwrap_or(d);
        Some(Rules {
            repos: get("TEHAS_OUTSOURCE_REPOS")
                .split_whitespace()
                .map(str::to_string)
                .collect(),
            retries: n("TEHAS_OUTSOURCE_RETRIES", 2),
            timeout: n("TEHAS_OUTSOURCE_TIMEOUT", 600),
        })
    }

    /// GitHub names a repository in any case.
    fn allows(&self, repo: &str) -> bool {
        self.repos.iter().any(|r| r.eq_ignore_ascii_case(repo))
    }
}

/// What a contractor last said it has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub digest: String,
    pub mem_mb: u64,
    pub disk_kb: u64,
    /// When the factory took it. A report that does not say is taken as of
    /// the contractor's last call.
    pub at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contractor {
    pub name: String,
    pub revoked: bool,
    pub draining: bool,
    pub resting: bool,
    pub last_seen_at: Option<u64>,
    pub report: Option<Report>,
}

impl Contractor {
    /// One row of `store contractor-list`, which the store writes as a string
    /// of JSON; a row without a name is none.
    fn parse(v: &Value) -> Option<Contractor> {
        if let Value::String(text) = v {
            return Contractor::parse(&serde_json::from_str(text).ok()?);
        }
        let flag = |k: &str| v.get(k).and_then(Value::as_bool) != Some(false);
        let report = v.get("report").and_then(|r| {
            Some(Report {
                digest: r.get("digest")?.as_str()?.to_string(),
                mem_mb: r.get("mem_mb")?.as_u64()?,
                disk_kb: r.get("disk_kb")?.as_u64()?,
                at: r.get("at").and_then(Value::as_u64),
            })
        });
        Some(Contractor {
            name: v
                .get("name")?
                .as_str()
                .filter(|s| !s.is_empty())?
                .to_string(),
            // A flag the row does not say plainly is taken as set.
            revoked: flag("revoked"),
            draining: flag("draining"),
            resting: flag("resting"),
            last_seen_at: v.get("last_seen_at").and_then(Value::as_u64),
            report,
        })
    }
}

/// The snapshot's `outsource` object, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outsourcing {
    pub digest: String,
    pub contractors: Option<Vec<Contractor>>,
    pub busy: Option<Vec<String>>,
    pub lost: BTreeMap<u64, u64>,
}

impl Outsourcing {
    pub fn parse(v: &Value) -> Outsourcing {
        let failed = |x: &Value| x.get("failed").and_then(Value::as_bool) == Some(true);
        let list = |k: &str| v.get(k).filter(|x| !failed(x)).and_then(Value::as_array);
        Outsourcing {
            digest: v
                .get("digest")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            contractors: list("contractors")
                .map(|a| a.iter().filter_map(Contractor::parse).collect()),
            busy: list("busy").map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            }),
            lost: v
                .get("lost")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, n)| Some((k.parse().ok()?, n.as_u64()?)))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// What one start needs of a contractor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Need {
    pub repo: String,
    pub issue: u64,
    /// The memory a cycle's container may use, in MiB.
    pub mem_mb: u64,
    /// The disk a cycle must leave free, in KiB.
    pub disk_kb: u64,
    pub now: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// On this host, and why, in the words the start's log line ends with.
    InHouse(String),
    /// On this contractor.
    Out(String),
}

/// Where the start of `need` runs. `taken` holds the contractors this poll
/// already gave a start.
pub fn place(rules: &Rules, o: &Outsourcing, need: &Need, taken: &[String]) -> Place {
    let here = |why: String| Place::InHouse(why);
    if !rules.allows(&need.repo) {
        return here(format!("TEHAS_OUTSOURCE_REPOS does not name {}", need.repo));
    }
    let Some(&lost) = o.lost.get(&need.issue) else {
        return here("its outsourced attempts could not be read".into());
    };
    if lost >= rules.retries {
        return here(format!(
            "it lost {lost} outsourced jobs, TEHAS_OUTSOURCE_RETRIES is {}",
            rules.retries
        ));
    }
    let (Some(all), Some(busy)) = (&o.contractors, &o.busy) else {
        return here("the contractors could not be read".into());
    };
    let fits = |c: &&Contractor| {
        let fresh = c
            .last_seen_at
            .is_some_and(|at| at <= need.now && need.now - at <= rules.timeout);
        let free = !c.revoked && !c.draining && !c.resting;
        let idle = !busy.contains(&c.name) && !taken.contains(&c.name);
        let room = c.report.as_ref().is_some_and(|r| {
            let current =
                r.at.is_none_or(|at| at <= need.now && need.now - at <= rules.timeout);
            current
                && !o.digest.is_empty()
                && r.digest == o.digest
                && r.mem_mb >= need.mem_mb
                && r.disk_kb >= need.disk_kb
        });
        fresh && free && idle && room
    };
    let best = all.iter().filter(fits).max_by(|a, b| {
        let (ra, rb) = (a.report.as_ref(), b.report.as_ref());
        let key = |r: Option<&Report>| r.map(|r| (r.mem_mb, r.disk_kb)).unwrap_or_default();
        key(ra).cmp(&key(rb)).then_with(|| b.name.cmp(&a.name))
    });
    match best {
        Some(c) => Place::Out(c.name.clone()),
        None => here("no contractor fits".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: u64 = 1_000_000;
    const DIGEST: &str = "sha256:aaaa";

    fn rules() -> Rules {
        Rules {
            repos: vec!["acme/widget".into()],
            retries: 2,
            timeout: 600,
        }
    }

    fn row(name: &str, mem_mb: u64) -> Value {
        json!({"name": name, "revoked": false, "draining": false, "resting": false,
            "last_seen_at": NOW - 10,
            "report": {"digest": DIGEST, "mem_mb": mem_mb, "disk_kb": 50_000_000}})
    }

    fn world(rows: Vec<Value>) -> Outsourcing {
        Outsourcing::parse(&json!({"digest": DIGEST, "contractors": rows, "busy": [],
            "lost": {"7": 0}}))
    }

    fn need() -> Need {
        Need {
            repo: "acme/widget".into(),
            issue: 7,
            mem_mb: 4096,
            disk_kb: 10_485_760,
            now: NOW,
        }
    }

    fn out(name: &str) -> Place {
        Place::Out(name.into())
    }

    fn inhouse(o: &Outsourcing, need: &Need) -> String {
        match place(&rules(), o, need, &[]) {
            Place::InHouse(why) => why,
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn the_rules_are_none_with_the_feature_off_whatever_its_settings_say() {
        let get = |k: &str| match k {
            "TEHAS_OUTSOURCE" => "on".to_string(),
            "TEHAS_OUTSOURCE_REPOS" => "acme/widget".to_string(),
            _ => String::new(),
        };
        assert_eq!(Rules::of(&get), None);
    }

    #[test]
    fn the_rules_follow_the_settings_when_a_test_build_turns_the_feature_on() {
        let get = |k: &str| match k {
            "TEHAS_OUTSOURCE" => "on".to_string(),
            "TEHAS_TEST_FEATURES" => "outsource".to_string(),
            "TEHAS_OUTSOURCE_REPOS" => "acme/widget  acme/other".to_string(),
            "TEHAS_OUTSOURCE_RETRIES" => "3".to_string(),
            _ => String::new(),
        };
        assert_eq!(
            Rules::of(&get),
            Some(Rules {
                repos: vec!["acme/widget".into(), "acme/other".into()],
                retries: 3,
                timeout: 600,
            })
        );
        let off = |k: &str| match k {
            "TEHAS_OUTSOURCE" => "off".to_string(),
            "TEHAS_TEST_FEATURES" => "outsource".to_string(),
            _ => String::new(),
        };
        assert_eq!(Rules::of(&off), None);
    }

    #[test]
    fn a_repository_the_setting_does_not_name_runs_here() {
        let o = world(vec![row("box-1", 8192)]);
        let mut r = rules();
        r.repos.clear();
        assert_eq!(
            place(&r, &o, &need(), &[]),
            Place::InHouse("TEHAS_OUTSOURCE_REPOS does not name acme/widget".into())
        );
        r.repos = vec!["ACME/Widget".into()];
        assert_eq!(place(&r, &o, &need(), &[]), out("box-1"));
    }

    #[test]
    fn the_one_with_the_most_room_wins_then_the_first_name() {
        let o = world(vec![
            row("box-1", 8192),
            row("box-2", 16384),
            row("box-3", 16384),
        ]);
        assert_eq!(place(&rules(), &o, &need(), &[]), out("box-2"));
        let taken = vec!["box-2".to_string()];
        assert_eq!(place(&rules(), &o, &need(), &taken), out("box-3"));
    }

    #[test]
    fn a_contractor_that_does_not_fit_is_passed_over() {
        let mut bad = Vec::new();
        let mut silent = row("silent", 8192);
        silent["last_seen_at"] = json!(NOW - 601);
        bad.push(silent);
        let mut never = row("never", 8192);
        never["last_seen_at"] = Value::Null;
        bad.push(never);
        for flag in ["revoked", "draining", "resting"] {
            let mut r = row(flag, 8192);
            r[flag] = json!(true);
            bad.push(r);
        }
        let mut unsaid = row("unsaid", 8192);
        unsaid.as_object_mut().unwrap().remove("revoked");
        bad.push(unsaid);
        let mut image = row("image", 8192);
        image["report"]["digest"] = json!("sha256:bbbb");
        bad.push(image);
        bad.push(row("small", 2048));
        let mut disk = row("disk", 8192);
        disk["report"]["disk_kb"] = json!(1024);
        bad.push(disk);
        let mut text = row("text", 8192);
        text["revoked"] = json!(true);
        bad.push(json!(text.to_string()));
        let mut quiet = row("quiet", 8192);
        quiet.as_object_mut().unwrap().remove("report");
        bad.push(quiet);
        bad.push(row("busy", 8192));
        let mut o = world(bad);
        o.busy = Some(vec!["busy".into()]);
        assert_eq!(inhouse(&o, &need()), "no contractor fits");
        o.contractors.as_mut().unwrap().push(Contractor {
            name: "fit".into(),
            revoked: false,
            draining: false,
            resting: false,
            last_seen_at: Some(NOW),
            report: Some(Report {
                digest: DIGEST.into(),
                mem_mb: 4096,
                disk_kb: 10_485_760,
                at: Some(NOW),
            }),
        });
        assert_eq!(place(&rules(), &o, &need(), &[]), out("fit"));
        let as_text = world(vec![json!(row("box-1", 8192).to_string())]);
        assert_eq!(place(&rules(), &as_text, &need(), &[]), out("box-1"));
    }

    /// A row as `store contractor-list` writes it: a string of JSON whose
    /// report the store took from the contractor's hello.
    fn stored(name: &str, report: Value) -> Value {
        json!(json!({"name": name, "job_issue": null, "job_state": null,
            "revoked": false, "draining": false, "resting": false,
            "resting_until": null, "last_seen_at": NOW - 10,
            "jobs_done": 0, "jobs_lost": 0, "report": report})
        .to_string())
    }

    #[test]
    fn a_contractor_whose_stored_report_has_the_room_fits() {
        let report = json!({"digest": DIGEST, "rukas": "v0.14.50", "mem_mb": 8192,
            "disk_kb": 50_000_000, "at": NOW - 10});
        let o = world(vec![stored("box-1", report.clone())]);
        assert_eq!(place(&rules(), &o, &need(), &[]), out("box-1"));
        // A report of no image, no report at all, or one older than the
        // timeout, fits nothing.
        let mut no_image = report.clone();
        no_image["digest"] = Value::Null;
        let mut old = report.clone();
        old["at"] = json!(NOW - 601);
        for r in [no_image, Value::Null, old] {
            let o = world(vec![stored("box-1", r.clone())]);
            assert_eq!(inhouse(&o, &need()), "no contractor fits", "{r}");
        }
    }

    #[test]
    fn no_digest_of_this_host_is_no_fit() {
        let mut o = world(vec![row("box-1", 8192)]);
        o.digest.clear();
        assert_eq!(inhouse(&o, &need()), "no contractor fits");
    }

    #[test]
    fn an_issue_that_spent_its_retries_runs_here() {
        let mut o = world(vec![row("box-1", 8192)]);
        o.lost.insert(7, 1);
        assert_eq!(place(&rules(), &o, &need(), &[]), out("box-1"));
        o.lost.insert(7, 2);
        assert_eq!(
            inhouse(&o, &need()),
            "it lost 2 outsourced jobs, TEHAS_OUTSOURCE_RETRIES is 2"
        );
    }

    #[test]
    fn what_could_not_be_read_keeps_the_cycle_here() {
        let mut o = world(vec![row("box-1", 8192)]);
        o.lost.clear();
        assert_eq!(
            inhouse(&o, &need()),
            "its outsourced attempts could not be read"
        );
        let failed = Outsourcing::parse(&json!({"digest": DIGEST,
            "contractors": {"failed": true}, "busy": [], "lost": {"7": 0}}));
        assert_eq!(
            inhouse(&failed, &need()),
            "the contractors could not be read"
        );
        let busy = Outsourcing::parse(&json!({"digest": DIGEST,
            "contractors": [row("box-1", 8192)], "busy": {"failed": true}, "lost": {"7": 0}}));
        assert_eq!(inhouse(&busy, &need()), "the contractors could not be read");
        assert_eq!(
            inhouse(&Outsourcing::parse(&json!({})), &need()),
            "its outsourced attempts could not be read"
        );
    }
}
