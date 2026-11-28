//! snapshot(): what one poll would read, in the JSON shape Snapshot::parse
//! takes. GitHub in one paged query and the per-issue reads the rules make of
//! the issues they decide about, the store's read verbs, and this host's files
//! and locks. Nothing is written.

use super::run::{Calls, GhFail};
use super::{board, comments, github, host, models, store};
use crate::apply::{said_file, SAID_MAX};
use crate::decide::has;
use crate::settings::Settings;
use crate::text;
use crate::text::closing_numbers;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::path::Path;
use tehas_core::events::{pause_holds, Events};
use tehas_core::home::{Home, State};
use tehas_ports::{BlockerState, BlueprintNo, Board, Bound, Outcome, Unread};

/// Where the snapshot reads from, and with what.
pub struct Sources<'a> {
    pub home: &'a Path,
    pub calls: Calls,
    pub systemctl: String,
    pub now: i64,
    pub models: models::Models,
    /// The subagent provider's endpoint, url and key, as the probe found it.
    pub endpoint: (String, String),
    /// The code host's read handle, from the program's wiring: None when
    /// it made none, and a read through it is then unreadable. The queue
    /// reads its open pull requests through it.
    pub code_host: Option<Box<dyn tehas_ports::CodeHost>>,
    /// The blueprints port's read handle, from the same wiring: None when
    /// it made none, and an edit history read through it is then a failed
    /// read.
    pub blueprints: Option<Box<dyn tehas_ports::Blueprints>>,
    /// The board handle, from the program's wiring: the queue read asks it
    /// for the open work, and the blockers' states are read through it.
    /// None when it made none: the poll fails closed on the queue, and no
    /// blocker's state is readable.
    pub board: Option<Box<dyn tehas_ports::Board>>,
    /// The tickets handle: the queue reads the closed issues still carrying
    /// the running mark, the repository's marks and, when asked, the
    /// owner's issue types through it.
    pub tickets: Option<Box<dyn tehas_ports::Tickets>>,
    /// A mark as the site spells its label word: the queue's mapping names
    /// the marks of the open work with it, from the program's wiring.
    pub mark_name: crate::apply::MarkName,
    /// A marker as the site writes it: the comment's mapping names its
    /// markers with it, from the program's wiring.
    pub marker_text: crate::apply::MarkerText,
    /// Lines the reads leave for the poll to say once, each once: the code
    /// host's refusal of the branch's checks (`read::checks::refused`).
    pub said: Vec<String>,
    /// The kind of the queue read's failure, as the health record names it,
    /// set by the read that failed: the ports know the class of a failure
    /// without words, and the poll records the class rather than class the
    /// line it logs. Empty when the failed read was not the queue's.
    pub faults: Vec<&'static str>,
}

/// The facts a decision names in needs that are read only when it asks:
/// the sandbox image's and the model's answers to images, and whether the
/// image takes --continue-pr. Returns whether any was read.
pub fn fill(src: &Sources, snap: &mut Value, needs: &[&str]) -> bool {
    let mut read = false;
    if needs.contains(&"images") && snap.get("images").is_none() {
        snap["images"] = models::images(&src.models, &src.endpoint.0, &src.endpoint.1);
        read = true;
    }
    if needs.contains(&"continues") && snap.get("continues").is_none() {
        snap["continues"] = json!(models::continues());
        read = true;
    }
    read
}

/// An open issue of the issue type `name` carries `label` from here on, as
/// though GitHub had given it: every rule that holds an issue on the label
/// holds it on the type too. The label itself still counts.
fn typed_as_label(open: &mut [github::Open], name: &str, label: &str) {
    for o in open {
        if o.node.pointer("/issueType/name").and_then(Value::as_str) == Some(name)
            && !has(&o.labels, label)
        {
            o.labels.push(label.to_string());
        }
    }
}

fn listed(open: &[github::Open], label: &str) -> Value {
    Value::Array(
        open.iter()
            .filter(|o| has(&o.labels, label))
            .map(|o| {
                json!({"number": o.number,
                    "labels": o.labels.iter().map(|l| json!({"name": l})).collect::<Vec<_>>()})
            })
            .collect(),
    )
}

/// The queue as the ports answer it: the open work, with the history of its
/// marks and retitles, the changes that close each issue and the control
/// ticket, on `Board::open_work`; the open pull requests with their bodies
/// on the code host; and the closed issues still carrying the running mark,
/// the repository's marks with their ids and, when the settings name an
/// improvement type, the owner's issue types, on the tickets port. All of
/// it mapped into the page's shapes by `read::board`. A handle the wiring
/// could not make, or a read the site refused, ends the poll as a failed gh
/// read did: the rules never decide from a queue that could not be read.
/// The numbers of the open tickets this seam cannot give as issues come
/// back beside it: they hold a slot and are never started.
fn queue(src: &mut Sources, c: &Settings) -> Result<(github::Queue, Vec<u64>), GhFail> {
    use std::num::{NonZeroU32, NonZeroU64};
    use tehas_ports::{
        BoardAsk, Bound, ChangeFilter, ChangeStates, Depth, Filter, HappenedKind, Mark, Outcome,
        State, TicketKey, Window, ALL_MAX,
    };
    let faults = &mut src.faults;
    let mut fail = |what: &str, o: Outcome| {
        faults.push(health_line(&o));
        GhFail {
            line: format!(
                "ERROR: the {what} of the queue could not be read: {}",
                o.line()
            ),
            ran: None,
        }
    };
    // Every open issue, every closed running issue and every mark: the
    // queue takes them all, and one past the bound ends the poll as a
    // failed read would.
    let bound = Bound::new(u32::MAX).expect("u32::MAX is a bound");
    let control = NonZeroU64::try_from(c.control).map_err(|_| GhFail {
        line: format!(
            "ERROR: the control issue is numbered {0}, which is no ticket",
            c.control
        ),
        ran: None,
    })?;
    // The ask the queue makes of the board: no notes, every mark's events
    // whole beside the newest retitle, the open changes that close each
    // issue, and the control ticket beside the rest.
    let mut ask = BoardAsk::new(Depth::Cut { notes: 0 });
    ask.history_by_kind = vec![
        (HappenedKind::Marked, Window::Whole(ALL_MAX)),
        (HappenedKind::Unmarked, Window::Whole(ALL_MAX)),
        (HappenedKind::Retitled, Window::Last(NonZeroU32::MIN)),
    ];
    ask.changes = Some((
        ChangeStates::Open,
        Window::Last(NonZeroU32::new(20).expect("20 is a window")),
    ));
    ask.control = Some(TicketKey::Number(control));
    // The running mark is the bot's own: every running cycle carries it,
    // the bot applies it alone, and the rules read the mark and the lock
    // rather than its events. The page answers it without, and the board
    // reads nothing apart for it.
    ask.marks_not_read = vec![tehas_ports::Mark::State(tehas_ports::State::Running)];
    let work = {
        let board = src.board.as_mut().ok_or_else(|| GhFail {
            line: "ERROR: the poll has no board handle to read the queue by".to_string(),
            ran: None,
        })?;
        read(|| board.open_work(&ask, bound)).map_err(|o| fail("open work", o))?
    };
    let mark_name = src.mark_name;
    let (closed_running, labels) = {
        let tickets = src.tickets.as_mut().ok_or_else(|| GhFail {
            line: "ERROR: the poll has no tickets handle to read the queue by".to_string(),
            ran: None,
        })?;
        // The closed issues that still carry the running mark, as the page
        // gave their numbers. A key that is no number names no issue.
        let closed_running: Vec<u64> = (read(|| {
            tickets.list(
                &Filter {
                    open: Some(false),
                    carrying: vec![Mark::State(State::Running)],
                    ..Default::default()
                },
                bound,
            )
        })
        .map_err(|o| fail("closed running issues", o))?
        .into_items())
        .into_iter()
        .filter_map(|d| match d.key {
            TicketKey::Number(n) => Some(n.get()),
            TicketKey::Key(_) => None,
        })
        .collect();
        let labels: Vec<Value> = (read(|| tickets.marks(bound))
            .map_err(|o| fail("repository's marks", o))?
            .into_items())
        .into_iter()
        .filter_map(|h| {
            let name = mark_name(&h.mark);
            (!name.is_empty() && name.starts_with("tehas:"))
                .then(|| json!({"id": h.id.as_str(), "name": name}))
        })
        .collect();
        (closed_running, labels)
    };
    let kinds = c
        .improvement_type
        .as_ref()
        .map(|_| {
            let tickets = src.tickets.as_mut().ok_or_else(|| GhFail {
                line: "ERROR: the poll has no tickets handle to read the queue by".to_string(),
                ran: None,
            })?;
            read(|| tickets.kinds())
                .map(|w| w.into_items())
                .map_err(|o| fail("the owner's issue types", o))
        })
        .transpose()?;
    let pulls = src
        .code_host
        .as_mut()
        .map(|h| {
            read(|| {
                h.changes(
                    &ChangeFilter {
                        bodies: true,
                        ..Default::default()
                    },
                    bound,
                )
            })
            .map(|w| w.into_items())
        })
        .transpose()
        .map_err(|o| fail("open pull requests", o))?;
    let words = board::Words {
        repo: &c.repo,
        mark: &mark_name,
    };
    let mut m = board::queue(&work, pulls.as_deref(), kinds.as_deref(), &words);
    let unread = m
        .unread
        .iter()
        .filter_map(|(k, _)| match k {
            TicketKey::Number(n) => Some(n.get()),
            TicketKey::Key(_) => None,
        })
        .collect();
    m.queue.closed_running = closed_running;
    // The marks were read: the list is the answer, the empty one included.
    // A read that was never made is absent elsewhere, never a short list
    // here: the page that made the read always named `tehasLabels`, and the
    // pins of state/labels keep the difference the two make.
    m.queue.labels = Some(Value::Array(labels));
    Ok((m.queue, unread))
}

/// The kind of a port failure as the host's health record names it: the
/// class the old gh words carried, read off the outcome instead.
fn health_line(o: &tehas_ports::Outcome) -> &'static str {
    use tehas_core::retry::Fault;
    use tehas_ports::{Outcome as O, Passing as P};
    let fault = match o {
        O::Spent { .. } => Fault::Limit,
        O::Unavailable {
            why: P::Timeout, ..
        } => Fault::Slow,
        O::Unavailable {
            why: P::Unreachable,
            ..
        } => Fault::Down,
        O::Unavailable { .. } => Fault::Server,
        O::Forbidden | O::Refused(_) => Fault::Refused,
        _ => Fault::Unread,
    };
    tehas_core::github_fails::line(fault)
}

/// A read the queue owes a retry to: the ports' words for the bad minute
/// the page read retried once, and never for a refusal.
fn owed(o: &tehas_ports::Outcome) -> bool {
    matches!(
        o,
        tehas_ports::Outcome::Unavailable {
            why: tehas_ports::Passing::Server | tehas_ports::Passing::Timeout,
            ..
        }
    )
}

/// A read of the queue, asked again on a bad minute, once. The attempts go
/// through the handle's counter, so the poll's cost counts them.
fn read<T>(mut ask: impl FnMut() -> tehas_ports::Got<T>) -> tehas_ports::Got<T> {
    let first = ask();
    match &first {
        Err(o) if owed(o) => ask(),
        _ => first,
    }
}

/// The snapshot, or the line the poll logs when a read it cannot do
/// without failed.
pub fn snapshot(src: &mut Sources, c: &Settings) -> Result<Value, GhFail> {
    let home = src.home;
    let mut s = json!({"now": src.now, "hostname": host::hostname()});
    s["pause_flag"] = host::pause(home);
    s["limit"] = host::limit(home, src.now);
    s["last_why"] = json!(host::last_why(home));
    // As $(head -c 1000) leaves it: no final newlines.
    s["branch_note"] = json!(host::head(&Home::new(home).state(State::BranchCheck), 1000)
        .trim_end_matches('\n')
        .to_string());
    s["refresh_held"] = json!(host::head(&Home::new(home).state(State::RefreshHeld), 300));
    s["push_fault"] = host::push_fault(home);
    if c.read_only {
        s["read_only_ran"] = json!(tehas_core::read_only::recorded(home));
    }
    s["refine_brief"] = json!(Path::new(&c.libexec)
        .join("../lib/refine-brief.md")
        .metadata()
        .is_ok_and(|m| m.len() > 0));

    s["order"] = store::order(&mut src.calls);
    let (mut q, unread) = queue(src, c)?;
    let calls = &mut src.calls;
    // The board answers by a repository's id, which a rename leaves
    // standing: the poll still asks that the name GitHub gives the
    // repository agree with the setting, as the page itself reported it.
    s["repo"] = json!(calls
        .gh(&["api", &format!("repos/{}", c.repo)])
        .ok()
        .and_then(|v| {
            v.get("nameWithOwner")
                .and_then(Value::as_str)
                .map(str::to_string)
        }));
    if let Some(labels) = q.labels.take() {
        s["tehas_labels"] = labels;
    }
    // Which issues the page cut short is read of the labels as GitHub gave
    // them, before an issue of the improvement type is given the label below.
    let dropped_of_page: Vec<u64> = q
        .open
        .iter()
        .filter(|o| github::dropped_events(o, &[github::RUNNING]))
        .map(|o| o.number)
        .collect();
    if let Some(t) = &c.improvement_type {
        s["issue_types"] = json!(q.issue_types);
        typed_as_label(&mut q.open, t, crate::decide::IMPROVEMENT);
    }
    s["user"] = json!({"login": c.bot_login});
    // A file that cannot be read gives no id: the bot is then known by its
    // login, as before the first record. Its questions authorise nothing.
    s["bot_id"] = json!(tehas_core::accounts::id_of(
        &tehas_core::accounts::read(home),
        &tehas_core::accounts::bot_key(&c.bot_login)
    ));
    // What rule 6 holds an issue on out of an open pull request's body:
    // every open pull request of `own_pull` that names it with a closing
    // keyword, by number, whatever base it targets. GitHub's own links are
    // kept to the same pull requests.
    if let Some(pulls) = &q.open_pulls {
        names_from_bodies(&mut s, pulls, &q.open, c);
    }
    // The open tickets the mapping could not give as issues hold a slot
    // too: their locks keep their records open, and nothing starts them.
    let open: BTreeSet<u64> = q.open.iter().map(|o| o.number).chain(unread).collect();
    let mut issues = Map::new();
    for o in &q.open {
        let mut i = github::issue(o);
        if let Some(prs) = i.get_mut("prs").and_then(Value::as_array_mut) {
            prs.retain(|p| own_pull(p, c));
        }
        issues.insert(o.number.to_string(), Value::Object(i));
    }
    // Those the page left out are read as the bash reads them, side by side.
    // The bot's `RUNNING` mark is the one label the queue does not ask about:
    // every running cycle carries it, the bot applies it alone, and the rules
    // read the mark and the lock file rather than its events. The control
    // issue is asked about by name below, with no label exempt.
    // The open issues the poll labels `epic`. Whether a person took the
    // label off is read of their events: a page whose timeline holds no
    // event of the label may have cut a removal off, so it is read whole.
    let epic_titled: Vec<&github::Open> = q
        .open
        .iter()
        .filter(|o| o.number != c.control && github::epic_titled(o))
        .collect();
    s["epic_titled"] = epic_titled.iter().map(|o| json!(o.number)).collect();
    let dropped: Vec<u64> = q
        .open
        .iter()
        .filter(|o| {
            dropped_of_page.contains(&o.number)
                || (epic_titled.iter().any(|e| e.number == o.number)
                    && !github::has_event_of(o, text::EPIC))
        })
        .map(|o| o.number)
        .collect();
    for (n, r) in dropped.iter().zip(events_all(calls, &c.repo, &dropped)) {
        issues[&n.to_string()]["events"] = r;
    }
    match q.open.iter().find(|o| o.number == c.control) {
        Some(o) => {
            // The control issue asks about every label it carries, so the
            // mark's exemption cannot reach it: the page's timeline is kept
            // only when the guard vouches for it, and one cut short of the
            // last `tehas:pause` event is read again rather than decide the
            // pause. A page that left the control issue's events out entirely
            // is already in `dropped` and holds its REST read. The guard runs
            // again here, with nothing exempt, and its answer for the queue is
            // kept beside the issue: the two differ only for an issue whose
            // labels are the mark alone, and reading the timeline once and
            // sharing it would give the control issue the queue's exemption.
            let events = if github::dropped_events(o, &[]) {
                issues
                    .get(&o.number.to_string())
                    .and_then(|i| i.get("events"))
                    .cloned()
                    .unwrap_or_else(|| github::events(&o.node))
            } else {
                github::events(&o.node)
            };
            s["control"] = json!({"labels": o.labels.iter().map(|l| json!({"name": l})).collect::<Vec<_>>(),
            "events": events})
        }
        None => {
            let n = c.control.to_string();
            let labels = calls
                .gh(&["issue", "view", &n, "--repo", &c.repo, "--json", "labels"])
                .ok()
                .and_then(|v| v.get("labels").cloned())
                .unwrap_or_else(|| json!({"failed": true}));
            s["control"] =
                json!({"labels": labels, "events": rest_events(calls, &c.repo, c.control)});
        }
    }
    for (key, label) in [
        ("ready", "tehas:ready"),
        ("blocked", "tehas:blocked"),
        ("asking", "tehas:question"),
        ("plans", "tehas:plan"),
        ("refines", "tehas:refine"),
    ] {
        s[key] = listed(&q.open, label);
    }
    s["awaiting"] = Value::Array(
        q.open
            .iter()
            .filter(|o| has(&o.labels, "tehas:awaiting-merge"))
            .map(|o| json!({"number": o.number}))
            .collect(),
    );
    let mut marked: Vec<Value> = q
        .open
        .iter()
        .filter(|o| has(&o.labels, github::RUNNING))
        .map(|o| json!({"number": o.number, "state": "OPEN"}))
        .collect();
    marked.extend(
        q.closed_running
            .iter()
            .map(|n| json!({"number": n, "state": "CLOSED"})),
    );
    s["marked"] = Value::Array(marked);
    s["planning"] = q
        .open
        .iter()
        .filter(|o| has(&o.labels, "tehas:planning"))
        .map(|o| json!(o.number))
        .collect();

    // The store and the locks.
    let sessions = store::sessions(calls);
    if let Some(list) = sessions.as_array() {
        let list: Vec<Value> = list
            .iter()
            .map(|x| {
                let mut x = x.clone();
                if x.get("running") == Some(&json!(true)) {
                    let n = x["issue"].as_u64().unwrap_or(0);
                    let (flight, rc) = host::turn(home, n, &src.systemctl);
                    x["in_flight"] = json!(flight);
                    x["rc"] = json!(rc);
                }
                x
            })
            .collect();
        s["plan_open"] = Value::Array(list);
    } else {
        s["plan_open"] = sessions;
    }
    // A listing that fails is no list of none: the rules end the poll on it.
    let locks = host::cycles(home).unwrap_or_else(|| {
        s["running_unlisted"] = json!(true);
        Vec::new()
    });
    let mut running: Vec<Value> = locks
        .iter()
        .map(|n| json!({"issue": n, "how": ""}))
        .collect();
    let mut refresh_rows: Vec<u64> = Vec::new();
    // The open records no process holds the lock of: cycles that died without
    // a word, which the poll ends as lost (T11.3). Asked on every poll, as a
    // lost cycle holds no lock to list. A failure with no lock held stops
    // nothing: the next poll asks again.
    let records = store::cycles(calls);
    if let Ok(r) = &records {
        let lost: Vec<Value> = r
            .iter()
            .filter(|x| x["lost"] == json!(true) && x["attempt"].as_u64().is_some_and(|a| a > 0))
            .map(|x| json!({"issue": x["issue"], "attempt": x["attempt"]}))
            .collect();
        s["lost"] = Value::Array(lost);
    }
    if !locks.is_empty() {
        match records {
            Ok(records) => {
                // took_how holds every row the store lists, held or not.
                refresh_rows = records
                    .iter()
                    .filter(|r| r["how"] == "refresh")
                    .filter_map(|r| r["issue"].as_u64())
                    .collect();
                for r in running.iter_mut() {
                    if let Some(rec) = records.iter().find(|x| x["issue"] == r["issue"]) {
                        *r = rec.clone();
                    }
                }
            }
            Err(e) => s["cycles_failed"] = e,
        }
        let authors = [bot(&s)];
        let lists: Vec<Value> = authors
            .iter()
            .map(|author| {
                github::list(
                    calls,
                    &[
                        "pr",
                        "list",
                        "--repo",
                        &c.repo,
                        "--author",
                        author,
                        "--state",
                        "all",
                        "--json",
                        "url,state,headRefName,createdAt",
                        "--limit",
                        "100",
                    ],
                )
            })
            .collect();
        s["bot_prs"] = joined_lists(lists);
    }
    for n in &locks {
        if !open.contains(n) {
            closed(&mut issues, *n);
        }
    }
    s["running"] = Value::Array(running.clone());
    s["slice"] = host::slice(home, &locks);
    let (endpoint, url, key) = models::endpoint(&src.models);
    s["endpoint"] = endpoint;
    src.endpoint = (url, key);
    let work_dir = std::env::var("TEHAS_WORK_DIR").unwrap_or_default();
    let user_home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
    s["leftovers"] = host::leftovers(home, &work_dir, &user_home, src.now, c.keep_days);
    // The queue is every open issue, so one it does not hold is closed: a
    // closed issue never resumes, and what it left goes at once.
    for l in s["leftovers"].as_array_mut().into_iter().flatten() {
        let n = l["issue"].as_u64().unwrap_or(0);
        l["closed"] = json!(!open.contains(&n));
    }
    s["disk_kb"] = host::disk(home);

    // The per-issue reads the snapshot makes, gathered before any of them is
    // asked: the rules name the issues they ask the store about, and all of
    // those names are known here from the queue, the locks and the requests. One
    // run of `store batch` answers the whole list (#941), where the poll used to
    // start the store once per question -- 49 processes for one idle poll. The
    // list is not deduped here: `gather` sorts and dedups it before it asks.
    let mut per_issue: Vec<store::Read> = Vec::new();
    // A function rather than a closure: the list is whatever the caller has, and
    // a closure cannot name its type parameter.
    fn want<I: IntoIterator<Item = store::Read>>(per_issue: &mut Vec<store::Read>, reads: I) {
        per_issue.extend(reads);
    }

    // Refreshes: the history, the marks, the requests and what they read.
    let history = store::refreshed(calls);
    let mut from: Vec<u64> = history
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    from.extend(&refresh_rows);
    from.sort_unstable();
    from.dedup();
    want(
        &mut per_issue,
        from.iter().copied().map(store::Read::Approval),
    );
    // A refresh row with no lock counts as the bash's took_how does: in the
    // issues a refresh of this host may own.
    s["refresh_history"] = match history {
        Value::Array(_) => json!(from),
        e => e,
    };
    // The requests are read before the list: the clean-up of the marks runs
    // while a request waits, as well as after a refresh on record.
    let (requests, refresh_warn) = host::requests(home);
    if !refresh_warn.is_empty() {
        s["refresh_warn"] = json!(refresh_warn);
    }
    if !from.is_empty() || !requests.is_empty() {
        s["refreshing"] = github::list(
            calls,
            &[
                "pr",
                "list",
                "--repo",
                &c.repo,
                "--state",
                "open",
                "--label",
                "tehas:refreshing",
                "--limit",
                "1000",
                "--json",
                "number,labels",
            ],
        );
    }
    // The events of the refreshing pull requests are read side by side, 8 at a
    // time, as the issues that lost the mark are read above (#1423).
    let refreshing = numbers(&s["refreshing"]);
    for (pr, r) in refreshing
        .iter()
        .zip(events_all(calls, &c.repo, &refreshing))
    {
        entry(&mut issues, *pr)["events"] = r;
    }
    want(
        &mut per_issue,
        requests.iter().map(|(n, _)| store::Read::Approval(*n)),
    );
    // The asking issues, the candidates of rule 6, and what each owes.
    let asking = numbers(&s["asking"]);
    want(
        &mut per_issue,
        asking
            .iter()
            .flat_map(|n| [store::Read::Question(*n), store::Read::Park(*n)]),
    );
    let mut candidates: BTreeSet<u64> = numbers(&s["ready"]).into_iter().collect();
    candidates.extend(numbers(&s["blocked"]));
    // The issues `tehas start` approved are candidates as the labelled ones
    // are, each open one with its labels. A paused poll asks nothing of them.
    if !paused(&s, c) {
        s["host_starts"] = match store::hosts(calls) {
            Value::Array(list) => list
                .into_iter()
                .filter_map(|mut h| {
                    let n = h.get("issue").and_then(Value::as_u64)?;
                    let o = q.open.iter().find(|o| o.number == n)?;
                    h["labels"] = o.labels.iter().map(|l| json!({"name": l})).collect();
                    candidates.insert(n);
                    Some(h)
                })
                .collect(),
            failed => failed,
        };
    }
    let candidates: Vec<u64> = candidates.into_iter().collect();
    for n in &candidates {
        if let Some(record) = host::plan_start(home, *n) {
            entry(&mut issues, *n)["plan_start"] = record;
        }
    }
    want(
        &mut per_issue,
        candidates
            .iter()
            .flat_map(|n| [store::Read::Park(*n), store::Read::Approval(*n)]),
    );
    // The driver of a retry is on its record, whichever rule starts it. Only an
    // issue that carries a label that chooses the driver costs a read.
    let read = crate::decide::driver_labels(c.driver_default);
    let lean: Vec<u64> = q
        .open
        .iter()
        .filter(|o| read.iter().any(|l| has(&o.labels, l)))
        .map(|o| o.number)
        .collect();
    want(
        &mut per_issue,
        lean.iter().copied().map(store::Read::Approval),
    );
    // One run of the store answers all of them, and the answers are read below
    // as the single calls were read. A poll that ends at the pause below pays
    // this batch for the refresh reads above the pause and nothing else: the
    // rules after it are not read at all.
    let mut asked = store::Asked::empty();
    store::gather(calls, &mut asked, &per_issue);

    for n in &from {
        let (_, current) = store::approval(&asked, *n);
        entry(&mut issues, *n)["current"] = current;
    }
    for n in lean {
        entry(&mut issues, n)["approval"] = store::approval(&asked, n).0;
    }
    s["refresh_requests"] = json!(requests.iter().map(|(n, _)| n).collect::<Vec<_>>());
    for (n, req) in &requests {
        if !open.contains(n) {
            closed(&mut issues, *n);
        }
        let i = entry(&mut issues, *n);
        i["request"] = req.clone();
        i["refresh_said"] = json!(host::head(&said_file(home, *n), SAID_MAX));
        let (shaped, _) = store::approval(&asked, *n);
        let i = entry(&mut issues, *n);
        i["approval"] = shaped;
        i["history"] = store::history(calls, *n);
        if let Some(pr) = req.get("pr").and_then(Value::as_u64) {
            let p = github::pull(calls, &c.repo, pr);
            s["pulls"][pr.to_string()] = p;
        }
    }

    // A paused poll ends after its clean-up: what only the rules after the
    // pause read is not read.
    if paused(&s, c) {
        s["issues"] = Value::Object(issues);
        return Ok(s);
    }

    // The questions waiting for an answer, the blocked candidates and the
    // candidates whose park may lift read their comments in one read of the
    // whole list rather than of one issue at a time: one aliased query a chunk
    // answers every asking issue, every blocked candidate the question read does
    // not cover (#1198), and every park that may lift (#1773), and one call
    // reads the states of the blockers the whole queue names (#1161).
    let blocked = blocked_list(&q, &candidates, &asking);
    // The list is built from the park answers the batch above already holds, so
    // it is planned after that run and before this one.
    let lifting = lifting_parks(&asked, &q, &candidates, &asking);
    let wanted: Vec<u64> = asking
        .iter()
        .chain(&blocked)
        .chain(&lifting)
        .copied()
        .collect();
    // The comments come through the tickets port: each issue's notes, the
    // blueprint's comments where the item answers none, and the edited
    // comments' histories on `note_edits`, mapped by `read::comments`.
    let words = comments::Words {
        marker: &src.marker_text,
    };
    let mut read = comments::all(
        src.tickets.as_deref_mut(),
        src.blueprints.as_deref_mut(),
        &words,
        &wanted,
    )
    .into_iter();
    let mut asked_of: BTreeMap<u64, Value> = BTreeMap::new();
    for (n, comments) in asking.iter().zip(read.by_ref()) {
        let question = store::question(&asked, *n);
        let i = entry(&mut issues, *n);
        i["question"] = question;
        i["comments"] = comments.clone();
        asked_of.insert(*n, comments);
    }
    let comments: BTreeMap<u64, Value> = blocked.into_iter().zip(read.by_ref()).collect();
    // What the batch answered of the parks that may lift, taken in the order
    // `lifting` holds them: the batch's iterator still has exactly that many
    // answers left, and an issue the question read answered first keeps that
    // answer rather than a second one for the same number.
    let lifted: BTreeMap<u64, Value> = lifting.into_iter().zip(read).collect();
    let board = src.board.as_mut().map(|b| b.as_mut() as &mut dyn Board);
    let states = blocker_reads(board, &q, &candidates, &comments, &asked_of);

    // The candidates of rule 6: what their start rests on. Membership of the
    // blocked batch is decided once, in `blocked_list`, and of the lifting
    // batch once, in `lifting_parks`, and the rule's own tests are those two
    // predicates, so the batches and the rule cannot disagree about an issue.
    for &n in &candidates {
        let park = store::park(&asked, n);
        let (shaped, _) = store::approval(&asked, n);
        let i = entry(&mut issues, n);
        i["park"] = park;
        i["approval"] = shaped;
        if !is_blocked(&q, n) {
            if i.get("comments").is_none() {
                if let Some(list) = lifted.get(&n) {
                    i["comments"] = list.clone();
                }
            }
            continue;
        }
        // The comments are the batch's, or the question read's when that read
        // answered the issue first; the one read paid for both.
        let read = comments.get(&n).cloned();
        let held = issues
            .get(&n.to_string())
            .and_then(|i| i.get("comments").cloned());
        let answer = read.as_ref().or(held.as_ref()).unwrap_or_else(|| {
            comments
                .get(&n)
                .expect("the batch answered every blocked candidate")
        });
        let own = crate::decide::named_blockers(answer);
        let i = entry(&mut issues, n);
        i["comments"] = read.or(held).expect("a blocked issue's comments were read");
        // A number GitHub gave no state for is absent, which is what decide
        // reads as "the state of #b could not be read": not the same as naming
        // no blocker at all, which would let the issue start.
        i["blockers"] = Value::Object(
            own.iter()
                .filter_map(|b| {
                    states
                        .get(b)
                        .map(|s| (b.to_string(), Value::String(s.clone())))
                })
                .collect(),
        );
    }
    // A page cut right after the label the last start used still ends on an
    // application of it, which the guard above cannot tell from a whole one.
    let behind: Vec<u64> = q
        .open
        .iter()
        .filter(|o| candidates.contains(&o.number) && !dropped.contains(&o.number))
        .filter(|o| behind_last_start(&issues[&o.number.to_string()], &o.labels))
        .map(|o| o.number)
        .collect();
    for (n, r) in behind.iter().zip(events_all(calls, &c.repo, &behind)) {
        issues[&n.to_string()]["events"] = r;
    }

    // The parks owed a retry or a refine, and what they read of the issue.
    s["owed"] = store::owed(calls, "--automatic");
    s["refines_owed"] = store::owed(calls, "--refine");
    for key in ["owed", "refines_owed"] {
        for n in s[key]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|p| p["issue"].as_u64())
        {
            if !open.contains(&n) {
                closed(&mut issues, n);
            }
        }
    }

    if !c.branch_check.is_empty() {
        let host = src
            .code_host
            .as_mut()
            .map(|h| h.as_mut() as &mut dyn tehas_ports::CodeHost);
        let check = super::checks::read(host, &c.branch, &c.branch_check, &mut src.said);
        s["checks"] = json!({"state": check.word(), "head": check.head()});
    }
    let wanted = edit_wanted(&s, &q, &candidates);
    edit_histories(src.blueprints.as_deref_mut(), &mut issues, &wanted);
    s["issues"] = Value::Object(issues);
    Ok(s)
}

/// The open issues a rule may start or finish on an approval, and so whose
/// body edits after it are judged whole: the candidates and their epics, the
/// plans, the refines, the questions a ticked Retry starts, the finishes and
/// the parks owed a cycle. Only one that was ever edited, as one with no edit
/// has no history to read.
fn edit_wanted(s: &Value, q: &github::Queue, candidates: &[u64]) -> Vec<u64> {
    let mut want: BTreeSet<u64> = candidates.iter().copied().collect();
    for key in ["plans", "refines", "asking"] {
        want.extend(numbers(&s[key]));
    }
    for key in ["owed", "refines_owed"] {
        want.extend(
            s[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p["issue"].as_u64()),
        );
    }
    for o in &q.open {
        if has(&o.labels, crate::finish::LABEL) {
            want.insert(o.number);
        }
        if candidates.contains(&o.number) {
            if let Some(p) = o.node.pointer("/parent/number").and_then(Value::as_u64) {
                want.insert(p);
            }
        }
    }
    q.open
        .iter()
        .filter(|o| want.contains(&o.number))
        .filter(|o| {
            o.node
                .get("lastEditedAt")
                .and_then(Value::as_str)
                .is_some_and(|at| !at.is_empty())
        })
        .map(|o| o.number)
        .collect()
}

/// The most edits of a body the poll reads, the text as created besides:
/// a body edited more often holds as one whose history is cut.
const EDITS_MAX: u32 = 99;

/// The body edit history of each issue of `ns`, read through the
/// blueprints port in one ask and put beside its last edit
/// (`board::history`). With no handle, it is a failed read.
fn edit_histories(
    blueprints: Option<&mut (dyn tehas_ports::Blueprints + 'static)>,
    issues: &mut Map<String, Value>,
    ns: &[u64],
) {
    if ns.is_empty() {
        return;
    }
    let nos: Vec<BlueprintNo> = ns.iter().filter_map(|&n| BlueprintNo::new(n)).collect();
    let bound = Bound::new(EDITS_MAX).expect("a bound of no 0");
    let read = match blueprints {
        Some(b) => b.body_edits_together(&nos, bound),
        None => Err(Outcome::Unread(Unread::Malformed)),
    };
    for n in ns {
        let h = board::history(&read, *n);
        let i = entry(issues, *n);
        if i["edit"].is_object() {
            i["edit"]["userContentEdits"] = h;
        }
    }
}

/// Whether `n` is held by `tehas:blocked` on the queue's page. The page is the
/// one source of the label, so an issue it did not answer for is not blocked:
/// it is read as no other rule reads it, rather than written as blocked with
/// nothing behind it.
fn is_blocked(q: &github::Queue, n: u64) -> bool {
    q.open
        .iter()
        .any(|o| o.number == n && has(&o.labels, crate::decide::BLOCKED))
}

/// The blocked candidates of rule 6 whose comments the poll reads beside the
/// questions': each blocked one the question read does not already cover.
///
/// Membership is decided once, by `is_blocked`, so the batch and the rule that
/// consumes it cannot disagree about which issue is blocked. The page is the
/// one source of labels; an issue it did not answer for is not a blocked
/// candidate. An asking issue is read once, by the question read, but it stays
/// a candidate of rule 6: the rule writes its comments and its blockers from
/// the answer it already has.
fn blocked_list(q: &github::Queue, candidates: &[u64], asking: &[u64]) -> Vec<u64> {
    candidates
        .iter()
        .copied()
        .filter(|n| is_blocked(q, *n) && !asking.contains(n))
        .collect()
}

/// Whether a Retry ticked in the park's question may lift `park`'s hold: the test rule 6 applies to a candidate, held here so the
/// batch of comment reads and the rule ask the same issues (#1773). The read
/// says nothing either way, so a park whose read failed is no park to lift on.
fn park_lifts(park: &Value) -> bool {
    park.is_object()
        && park.get("failed").is_none()
        && park.get("question").is_some_and(|q| !q.is_null())
}

/// The unblocked candidates of rule 6 whose comments the poll reads beside the
/// questions': each unblocked one the question read does not already cover, and
/// whose park may lift. Membership is decided once, by `park_lifts`, so the
/// batch and the rule that consumes it cannot disagree about which issue's park
/// may lift. An asking issue is read once, by the question read.
fn lifting_parks(
    asked: &store::Asked,
    q: &github::Queue,
    candidates: &[u64],
    asking: &[u64],
) -> Vec<u64> {
    candidates
        .iter()
        .copied()
        .filter(|n| {
            !is_blocked(q, *n) && !asking.contains(n) && park_lifts(&store::park(asked, *n))
        })
        .collect()
}

/// The states of the blockers the blocked candidates' comments name, in one
/// read of the board for the union of them (#1161), in the words decide
/// reads: OPEN, CLOSED or MERGED.
///
/// The comment read answers every number it was given, in order, so a missing
/// answer is a broken contract rather than a value to paper over. The union
/// covers what the whole queue names, including the issues whose comments the
/// question read gave. A queue that names no blocker is a list of none, and
/// the poll pays no call for it. A number the board did not answer, and every
/// number where the wiring made no board, is absent, which is what decide
/// reads as "unreadable". More than `ALL_MAX` (1000) distinct blockers are
/// all `Unread(Cut)` at the board, so every issue they hold stays held.
fn blocker_reads(
    board: Option<&mut dyn Board>,
    q: &github::Queue,
    candidates: &[u64],
    comments: &BTreeMap<u64, Value>,
    asked_of: &BTreeMap<u64, Value>,
) -> BTreeMap<u64, String> {
    let mut union: Vec<u64> = Vec::new();
    for n in candidates {
        if !is_blocked(q, *n) {
            continue;
        }
        let Some(answer) = comments.get(n).or_else(|| asked_of.get(n)) else {
            continue;
        };
        union.extend(crate::decide::named_blockers(answer));
    }
    union.sort_unstable();
    union.dedup();
    let numbers: Vec<NonZeroU64> = union.into_iter().filter_map(NonZeroU64::new).collect();
    let Some(board) = board.filter(|_| !numbers.is_empty()) else {
        return BTreeMap::new();
    };
    board
        .as_blockers(&numbers)
        .into_iter()
        .filter_map(|(n, b)| {
            let word = match b.ok()?.state {
                BlockerState::Open => "OPEN",
                BlockerState::Closed => "CLOSED",
                BlockerState::Merged => "MERGED",
            };
            Some((n.get(), word.to_string()))
        })
        .collect()
}

/// Whether the poll stops at the pause, as decide's pause_flag and
/// pause_label find it. A read that failed is not a pause: decide ends the
/// poll on it.
/// Whether the poll may start a cycle at all: not paused, and an issue ready
/// or approved by `tehas start`. A poll that can start nothing has no start for
/// the push probe to free, and asks the sandbox nothing.
pub fn may_start(s: &Value, c: &Settings) -> bool {
    let waiting = |k: &str| s[k].as_array().is_some_and(|a| !a.is_empty());
    !paused(s, c) && (waiting("ready") || waiting("host_starts"))
}

fn paused(s: &Value, c: &Settings) -> bool {
    if !s["pause_flag"].is_null() {
        return true;
    }
    let control = &s["control"];
    if control["labels"]
        .as_array()
        .is_some_and(|l| l.iter().any(|x| x["name"] == "tehas:pause"))
    {
        return true;
    }
    match Events::parse(&control["events"]) {
        Events::Read(events) => pause_holds(&events, &|l| c.allowed(l)).is_some(),
        _ => false,
    }
}

/// An issue the rules ask about that is not open: closed, as far as the poll
/// can tell without asking.
fn closed(issues: &mut Map<String, Value>, n: u64) {
    let i = entry(issues, n);
    let defaults = json!({"work": {"number": n, "state": "closed"}, "view": {"state": "CLOSED", "labels": []},
        "edit": {"lastEditedAt": null, "editor": null}, "events": [], "prs": []});
    for (k, v) in defaults.as_object().into_iter().flatten() {
        if i.get(k).is_none() {
            i[k.as_str()] = v.clone();
        }
    }
}

/// Whether a pull request may hold an issue: its branch is in the product
/// repository and the factory or an allowed account opened it. Anyone can open
/// a pull request from a fork with any body, so no other one holds an issue.
/// tehas_core holds the rule, which `tehas-cycle finish` reads too.
fn own_pull(p: &Value, c: &Settings) -> bool {
    tehas_core::accounts::own_pull(p, &c.repo, &c.bot_login, |a| c.allowed(a))
}

/// Which open pull requests name an open issue with a closing keyword in their
/// own description, written to `pulls_name` of the snapshot by issue number.
///
/// A body is read once per keyword it holds, not once per open issue: a page of
/// 25 issues over 100 bodies is 2500 matches otherwise, and most descriptions
/// hold no keyword at all. The nodes are what the page answered: a list of no
/// shape a rule reads holds nothing, and no issue is held on a body no poll read.
fn names_from_bodies(s: &mut Value, pulls: &Value, open: &[github::Open], c: &Settings) {
    let Some(nodes) = pulls.as_array() else {
        return;
    };
    let mentions: BTreeMap<u64, Vec<u64>> = nodes
        .iter()
        .filter(|p| own_pull(p, c))
        .filter_map(|p| {
            let pr = p.get("number").and_then(Value::as_u64)?;
            // A body of no shape a rule reads, none or absent, is a description
            // that closes nothing.
            let body = p.get("body").and_then(Value::as_str).unwrap_or("");
            Some((pr, closing_numbers(body)))
        })
        .collect();
    let mut names: Map<String, Value> = Map::new();
    for o in open {
        let hits: Vec<u64> = mentions
            .iter()
            .filter(|(pr, hits)| **pr != o.number && hits.contains(&o.number))
            .map(|(pr, _)| *pr)
            .collect();
        if !hits.is_empty() {
            names.insert(o.number.to_string(), json!(hits));
        }
    }
    if !names.is_empty() {
        s["pulls_name"] = Value::Object(names);
    }
}

/// The label events of an issue or pull request as the bash reads them:
/// the REST events, all pages; {"failed": true} when the call failed.
pub fn rest_events(calls: &mut Calls, repo: &str, n: u64) -> Value {
    let path = format!("repos/{repo}/issues/{n}/events?per_page=100");
    calls
        .gh(&["api", &path, "--paginate"])
        .unwrap_or_else(|_| json!({"failed": true}))
}

/// Whether the newest application of a start label the issue carries is no
/// newer than the approval its last start used. That start took the label off,
/// so the label on the issue was applied again since, and the timeline that
/// does not show it was cut.
fn behind_last_start(issue: &Value, labels: &[String]) -> bool {
    let Some(used) = issue
        .pointer("/approval/at")
        .and_then(Value::as_str)
        .and_then(tehas_core::time::from_iso)
    else {
        return false;
    };
    let Events::Read(ev) = Events::parse(&issue["events"]) else {
        return false;
    };
    ["tehas:ready", crate::decide::BLOCKED]
        .iter()
        .filter(|l| has(labels, l))
        .any(|l| {
            let (_, at) = tehas_core::events::label_actor(&ev, l);
            tehas_core::time::from_iso(&at).is_some_and(|t| t <= used)
        })
}

/// `rest_events` for several issues or pull requests, read side by side through
/// `Calls::gh_all`: one answer per number, in the order given.
fn events_all(calls: &mut Calls, repo: &str, ns: &[u64]) -> Vec<Value> {
    let reads: Vec<Vec<String>> = ns
        .iter()
        .map(|n| {
            vec![
                "api".into(),
                format!("repos/{repo}/issues/{n}/events?per_page=100"),
                "--paginate".into(),
            ]
        })
        .collect();
    calls
        .gh_all(&reads)
        .into_iter()
        .map(|r| r.unwrap_or_else(|_| json!({"failed": true})))
        .collect()
}

fn bot(s: &Value) -> String {
    s.pointer("/user/login")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn numbers(list: &Value) -> Vec<u64> {
    list.as_array()
        .map(|a| a.iter().filter_map(|i| i["number"].as_u64()).collect())
        .unwrap_or_default()
}

fn entry(issues: &mut Map<String, Value>, n: u64) -> &mut Value {
    issues.entry(n.to_string()).or_insert_with(|| json!({}))
}

/// The lists of each author as one, or the first that failed or is no list.
fn joined_lists(lists: Vec<Value>) -> Value {
    let mut all = Vec::new();
    for l in lists {
        match l {
            Value::Array(a) => {
                for p in a {
                    let url = p.get("url");
                    if url.is_none() || !all.iter().any(|q: &Value| q.get("url") == url) {
                        all.push(p);
                    }
                }
            }
            other => return other,
        }
    }
    Value::Array(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::board;
    use crate::read::models::Models;
    use crate::read::run;
    use std::time::Duration;
    use tehas_ports::fake::World;
    use tehas_ports::TicketKey;

    #[test]
    fn the_bots_pull_requests_are_the_lists_of_both_authors() {
        let a = json!({"url": "https://github.com/a/b/pull/1"});
        let b = json!({"url": "https://github.com/a/b/pull/2"});
        assert_eq!(
            joined_lists(vec![json!([a.clone()]), json!([b.clone(), a.clone()])]),
            json!([a, b])
        );
        let failed = json!({"failed": true});
        assert_eq!(joined_lists(vec![json!([a]), failed.clone()]), failed);
    }

    /// The edit history is read only of an edited issue a rule may start,
    /// a question whose Retry box may be ticked among them.
    #[test]
    fn the_edit_history_is_asked_of_the_edited_issues_that_may_start() {
        use tehas_ports::{Act, Mark, MarkName, State};
        let marks = [
            (3, Mark::Act(Act::Approve), true),
            (4, Mark::Act(Act::Approve), false),
            (5, Mark::Project(MarkName::new("bug").unwrap()), true),
            (6, Mark::Act(Act::Plan), true),
            (7, Mark::Act(Act::Finish), true),
            (8, Mark::State(State::NeedsPerson), true),
            (9, Mark::Epic, true),
            (10, Mark::State(State::AwaitingMerge), true),
            (11, Mark::State(State::Question), true),
        ];
        let mut world = World::new();
        for (n, mark, _) in &marks {
            world = world.ticket(*n, |t| t.mark(mark.clone()));
        }
        // #9 is the parent of #3.
        let world = world.ticket(9, |t| t.part(TicketKey::Number(3.try_into().unwrap())));
        world.advance(60);
        for (n, _, edited) in &marks {
            if *edited {
                world.edit_elsewhere(&world.key(*n), "edited", "alice");
            }
        }
        let q = github::Queue {
            open: board::fake::page(&world),
            closed_running: Vec::new(),
            open_pulls: None,
            issue_types: None,
            labels: None,
        };
        let s = json!({"plans": [{"number": 6}], "refines": [],
            "owed": [{"issue": 8}], "refines_owed": [], "asking": [{"number": 11}]});
        assert_eq!(edit_wanted(&s, &q, &[3, 4]), [3, 6, 7, 8, 9, 11]);
    }

    /// With no board from the wiring, a blocker the comments name has no
    /// state, which decide reads as unreadable and holds. The same reads on
    /// a board answer it.
    #[test]
    fn no_board_leaves_every_blocker_unread() {
        let q = github::Queue {
            open: vec![github::Open {
                number: 21,
                labels: vec![crate::decide::BLOCKED.to_string()],
                node: Value::Null,
            }],
            closed_running: Vec::new(),
            open_pulls: None,
            issue_types: None,
            labels: None,
        };
        // The blocked world's comment of #21, which names #11.
        let comments = BTreeMap::from([(21, world()["comments"]["21"].clone())]);
        let none = BTreeMap::new();
        assert!(blocker_reads(None, &q, &[21], &comments, &none).is_empty());
        let mut board = Board::of(&World::new(), &json!({"11": "OPEN"}));
        assert_eq!(
            blocker_reads(Some(&mut board), &q, &[21], &comments, &none),
            BTreeMap::from([(11, "OPEN".to_string())])
        );
    }

    #[test]
    fn a_merged_change_is_read_as_merged() {
        let q = github::Queue {
            open: vec![github::Open {
                number: 21,
                labels: vec![crate::decide::BLOCKED.to_string()],
                node: Value::Null,
            }],
            closed_running: Vec::new(),
            open_pulls: None,
            issue_types: None,
            labels: None,
        };
        // The blocked world's comment of #21, which names #11.
        let comments = BTreeMap::from([(21, world()["comments"]["21"].clone())]);
        let mut board = Board::of(&World::new(), &json!({"11": "MERGED"}));
        assert_eq!(
            blocker_reads(Some(&mut board), &q, &[21], &comments, &BTreeMap::new()),
            BTreeMap::from([(11, "MERGED".to_string())])
        );
    }

    /// The edit histories are read through the blueprints port, in one ask
    /// for the issues wanted, and put beside each one's last edit; a read
    /// the port fails, or one with no handle, is a failed history.
    #[test]
    fn the_edit_histories_are_read_through_the_blueprints_port() {
        use tehas_ports::fake::{BlueprintsOp, FakeSite, Fault, Faulty, Op};
        use tehas_ports::{Actor, Blueprints, Caller, Depth, MarkerEdit, Scope, Site};
        let world = World::new().ticket(7, |t| t.text("a body"));
        let site = FakeSite::new(&world);
        let by = Actor::Ours(Scope::Full);
        let mut writes = site.blueprint_writes(&by, Caller::new("test")).unwrap();
        let no = BlueprintNo::new(7).unwrap();
        world.advance(60);
        let rev = writes
            .blueprint(no, Depth::Cut { notes: 0 })
            .unwrap()
            .revision;
        writes
            .edit(no, None, Some("edited"), &MarkerEdit::default(), &rev)
            .unwrap();
        let read = |b: Option<&mut (dyn Blueprints + 'static)>| {
            let mut issues = Map::new();
            for n in [7, 8] {
                entry(&mut issues, n)["edit"] = json!({});
            }
            edit_histories(b, &mut issues, &[7]);
            assert_eq!(issues["8"]["edit"], json!({}), "only the issues asked");
            issues["7"]["edit"]["userContentEdits"].clone()
        };
        let mut handle = site.blueprints(&by, Caller::new("test")).unwrap();
        let h = read(Some(handle.as_mut()));
        assert_eq!(h["totalCount"], json!(1), "{h}");
        assert_eq!(
            h["nodes"][0]["editedAt"],
            json!(tehas_core::time::iso(world.now().secs()))
        );
        assert!(h["nodes"][0]["editor"]["login"].is_string(), "{h}");
        let faulty = Faulty::new(FakeSite::new(&world)).on(
            Op::Blueprints(BlueprintsOp::BodyEditsOf),
            Fault::Unreachable,
        );
        let mut handle = faulty.blueprints(&by, Caller::new("test")).unwrap();
        assert_eq!(read(Some(handle.as_mut()))["failed"], json!(true));
        assert_eq!(read(None)["failed"], json!(true));
    }
    use crate::snapshot::{ClosingPr, Snapshot};

    /// What separates two calls in a stub's record: a word of nothing that no
    /// argument of a real gh or store call contains, named once so that neither
    /// the body of a stub nor the reader of the record spells an escape.
    const RECORD_SEP: &str = "»tehas-record«";
    /// Separates the verb `batch` from the request it was given on standard
    /// input, inside one record of the store's calls.
    const REQUEST_SEP: &str = "«tehas-request»";
    /// Ends the request of a run of the batch, where the verb of the call whose
    /// record follows the run begins.
    const RUST_SEP: &str = "«tehas-rust»";

    /// The board of a case on the ports' fake: its queue reads come from
    /// `queue`, and its tickets the blockers of `states` (a number and OPEN
    /// or CLOSED, or MERGED for a merged change), each `as_blockers` it is
    /// asked kept in `asked`.
    struct Board {
        queue: Box<dyn tehas_ports::Board + Send>,
        blockers: Box<dyn tehas_ports::Board + Send>,
        asked: std::sync::Arc<std::sync::Mutex<Vec<Vec<u64>>>>,
    }

    /// The board of a case: the queue's, and the blockers' states of the
    /// `states` answer `host` wrote beside the programs.
    fn test_board(queue: &World, dir: &Path) -> Board {
        let states = std::fs::read_to_string(dir.join(".states"))
            .ok()
            .map(|s| serde_json::from_str(&s).unwrap())
            .unwrap_or(Value::Null);
        Board::of(queue, &states)
    }

    impl Board {
        fn of(queue: &World, states: &Value) -> Self {
            use tehas_ports::{Actor, Caller, CloseReason, Scope, Site};
            let blocker_world =
                states
                    .as_object()
                    .into_iter()
                    .flatten()
                    .fold(World::new(), |w, (n, state)| {
                        let n = n.parse().unwrap();
                        match state.as_str() {
                            Some("MERGED") => w.change(n, &format!("change-{n}"), |c| {
                                c.state(tehas_ports::ChangeState::Merged {
                                    at: tehas_ports::At::new(1_767_225_600),
                                    by: None,
                                    commit: None,
                                })
                            }),
                            Some("CLOSED") => w.ticket(n, |t| t.closed(CloseReason::Done)),
                            _ => w.ticket(n, |t| t),
                        }
                    });
            let (by, caller) = (Actor::Ours(Scope::Full), Caller::new("test"));
            Self {
                queue: tehas_ports::fake::FakeSite::new(queue)
                    .board(&by, caller)
                    .unwrap(),
                blockers: tehas_ports::fake::FakeSite::new(&blocker_world)
                    .board(&by, caller)
                    .unwrap(),
                asked: Default::default(),
            }
        }
    }

    impl tehas_ports::Reports for Board {
        fn take_reports(&mut self) -> Vec<tehas_ports::Report> {
            let mut r = self.queue.take_reports();
            r.extend(self.blockers.take_reports());
            r
        }
    }

    impl tehas_ports::Board for Board {
        fn as_blockers(
            &mut self,
            numbers: &[NonZeroU64],
        ) -> Vec<(NonZeroU64, tehas_ports::Got<tehas_ports::Blocker>)> {
            let asked = numbers.iter().map(|n| n.get()).collect();
            self.asked.lock().unwrap().push(asked);
            self.blockers.as_blockers(numbers)
        }

        fn open_work(
            &mut self,
            ask: &tehas_ports::BoardAsk,
            bound: tehas_ports::Bound,
        ) -> tehas_ports::Got<tehas_ports::OpenWork> {
            self.queue.open_work(ask, bound)
        }

        fn work_item(
            &mut self,
            key: &TicketKey,
            ask: &tehas_ports::BoardAsk,
        ) -> tehas_ports::Got<tehas_ports::WorkItem> {
            self.queue.work_item(key, ask)
        }

        fn closed_work(
            &mut self,
            ask: &tehas_ports::BoardAsk,
            since: tehas_ports::At,
            bound: tehas_ports::Bound,
        ) -> tehas_ports::Got<
            tehas_ports::Whole<(TicketKey, tehas_ports::Got<tehas_ports::WorkItem>)>,
        > {
            self.queue.closed_work(ask, since, bound)
        }

        fn target_work(
            &mut self,
            ask: &tehas_ports::BoardAsk,
            target: &tehas_ports::TargetId,
            bound: tehas_ports::Bound,
        ) -> tehas_ports::Got<
            tehas_ports::Whole<(TicketKey, tehas_ports::Got<tehas_ports::WorkItem>)>,
        > {
            self.queue.target_work(ask, target, bound)
        }

        fn open_changes(
            &mut self,
            ask: &tehas_ports::ChangeAsk,
            bound: tehas_ports::Bound,
        ) -> tehas_ports::Got<tehas_ports::Whole<tehas_ports::ChangeItem>> {
            self.queue.open_changes(ask, bound)
        }
    }

    /// The tickets of the blocked world on the ports' fake: #21, #22 and #23,
    /// filed by alice and blocked by her. The board's answer carries the
    /// label's event, so the poll trusts it: no read by name is paid for
    /// these issues. This world is the queue: what a case adds to it is on
    /// the queue, and what it leaves out is not.
    fn blocked() -> World {
        blocked_of(&[21, 22, 23])
    }

    /// `blocked` with the given issues alone.
    fn blocked_of(numbers: &[u64]) -> World {
        use tehas_ports::{Act, At};
        let blocked = World::new();
        blocked.set_now(At::new(
            tehas_core::time::from_iso("2026-10-02T00:00:00Z").unwrap(),
        ));
        numbers.iter().fold(blocked, |w, n| {
            let n = *n;
            w.ticket(n, |t| {
                t.title(&format!("issue {n}"))
                    .text("a body")
                    .author("alice")
                    .act(Act::Block, "alice")
            })
        })
    }

    /// The world of a poll beside its queue: issues #21, #22 and #23 are on
    /// the queue in `blocked`, all carrying `tehas:blocked`, and their newest
    /// comment holds each one here. #21 names #11, #22 names #11 and #12, and
    /// #23 names #13 alone: a blocker no other issue names, so a per-issue
    /// blocker read costs calls that a merged one does not. The board's answer
    /// gives the label's event, so the poll trusts it and pays no read by name
    /// for these issues. The control issue is nowhere in the queue, so its
    /// labels and its events are read by name, and nothing of it holds the
    /// pause.
    fn world() -> Value {
        let comment = |body: &str| {
            json!({"data": {"repository": {"issue": {"comments": {"nodes": [
                {"databaseId": 500, "author": {"login": "tehas-bot"}, "body": body,
                 "createdAt": "2026-10-03T00:00:00Z", "lastEditedAt": null, "editor": null,
                 "userContentEdits": {"totalCount": 0, "nodes": []}}
            ]}}}}})
        };
        let labeled = || {
            json!([{"event": "labeled", "label": {"name": crate::decide::BLOCKED},
                    "actor": {"login": "alice"}, "created_at": "2026-10-02T00:00:00Z"}])
        };
        json!({
            // The three issues are the candidates of rule 6.
            "blocked": [{"number": 21, "labels": [{"name": crate::decide::BLOCKED}]},
                        {"number": 22, "labels": [{"name": crate::decide::BLOCKED}]},
                        {"number": 23, "labels": [{"name": crate::decide::BLOCKED}]}],
            "control": {"labels": []},
            "events": {"21": labeled(), "22": labeled(), "23": labeled()},
            "comments": {"21": comment("<!-- tehas:blocked by=11 -->"),
                         "22": comment("<!-- tehas:blocked by=11,12 -->"),
                         "23": comment("<!-- tehas:blocked by=13 -->")},
            // The blockers' states, as the board's tickets.
            "states": {"11": "OPEN", "12": "CLOSED", "13": "OPEN"}
        })
    }

    /// One answer of the world, as the file the stub reads for it.
    fn fixture(dir: &Path, name: &str, v: &Value) {
        std::fs::write(dir.join(format!(".{name}")), v.to_string()).unwrap();
    }

    /// The reads a poll of the blocked world makes, in the order the stub asks
    /// them. A `gh` read is told apart by the word only its own arguments carry,
    /// and the first word a call holds picks the answer, so the order of this
    /// table is the disambiguation. `fixture` holds either the bytes of the
    /// answer, or the name of a file beside the programs to read; a name ending
    /// in `-` takes the number out of the call, because the comment read and the
    /// events read answer one issue at a time. A read whose `fails` is set is
    /// answered as a call GitHub refused. A new read of the poll costs one entry
    /// here and nothing else.
    struct Read {
        /// The word a call's arguments must hold for this entry to answer it.
        word: &'static str,
        fixture: &'static str,
        fails: bool,
    }

    const READS: &[Read] = &[
        Read {
            word: "check-runs",
            fixture: r#"{"check_runs":[]}"#,
            fails: false,
        },
        Read {
            word: "events?per_page=100",
            fixture: "events-",
            fails: false,
        },
        Read {
            word: "issue view",
            fixture: "control",
            fails: false,
        },
        Read {
            word: "tehas:refreshing",
            fixture: "refreshing",
            fails: false,
        },
        // Last: every other REST path holds this word too, and the first
        // word a call's arguments hold picks the answer.
        Read {
            word: "repos/acme/widget",
            fixture: r#"{"nameWithOwner":"acme/widget"}"#,
            fails: false,
        },
    ];

    /// Whether a table entry names a file to read rather than holding its bytes.
    fn is_fixture(answer: &str) -> bool {
        !answer.starts_with('{')
    }

    /// The home of a case. `broken` names issues whose comments the site
    /// refuses to give; the queue is told of them in `seed`, not here.
    fn host(world: &Value) -> tehas_core::testdir::TestDir {
        let dir = tehas_core::testdir::TestDir::new("build-poll");
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::create_dir_all(Home::new(&*dir).cycles_dir()).unwrap();
        std::fs::create_dir_all(Home::new(&*dir).state(State::Refreshes)).unwrap();
        std::fs::create_dir_all(dir.join("logs")).unwrap();

        // The stub's own list of words is held in w-10.. and asked of the table
        // by `tr`: the first word a call's arguments hold picks the answer, so
        // the order of the table is the disambiguation, and a word may hold a
        // space (`issue view`) or a `?`, so the shell sees the list as one word
        // per line and never splits it further.
        let words: Vec<&'static str> = READS.iter().map(|r| r.word).collect();
        assert_eq!(words.len(), 5, "the stub's list holds one word a read");
        // One word a line; the stub reads it with IFS set to the newline alone,
        // so a word that holds a space stays one word.
        std::fs::write(dir.join("words"), words.join("\n")).unwrap();
        std::fs::write(
            dir.join("store-words"),
            ["plan-show", "history-show", "park-show"].join("\n"),
        )
        .unwrap();
        // `approvals`: the store's answer of `approval-current <n>`, as its line.
        for (n, said) in world["approvals"].as_object().into_iter().flatten() {
            std::fs::write(
                dir.join(format!(".approval-current-{n}")),
                said.as_str().unwrap_or(""),
            )
            .unwrap();
        }
        for (n, r) in READS.iter().enumerate() {
            let answer = format!(
                "{}{}",
                if r.fails { "!" } else { "" },
                if is_fixture(r.fixture) {
                    format!(".{}", r.fixture)
                } else {
                    r.fixture.to_string()
                }
            );
            std::fs::write(dir.join(format!("w-{}", n + 1)), answer).unwrap();
        }
        fixture(&dir, "states", &world["states"]);
        fixture(&dir, "control", &world["control"]);
        if let Some(r) = world.get("refreshing") {
            fixture(&dir, "refreshing", r);
        }
        // The events reads named in `together` wait, each, until every one of
        // them has started: they answer only when they run side by side, and
        // a read of them one at a time gives up and is refused. The wait is
        // for the count, bounded well inside the call's own bound.
        for n in world["together"].as_array().into_iter().flatten() {
            std::fs::write(dir.join(format!("together-{n}")), "").unwrap();
        }
        for (n, events) in world["events"].as_object().into_iter().flatten() {
            fixture(&dir, &format!("events-{n}"), events);
        }

        // `answer` takes the status its caller leaves: 0 for an answer, 1 for
        // the fixture that asked to be refused.
        stub(
            &dir.join("bin"),
            "gh",
            r#"i=0
OLDIFS=$IFS
IFS=$(printf '\nX')
IFS=${IFS%X}
for word in $(cat "$here/../words"); do
  i=$((i + 1))
  a=$(cat "$here/../w-$i")
  case "$line" in
    *"$word"*)
      case "$a" in
        .*) key=$(printf '%s' "$line" | sed -n 's|.*/issues/\([0-9]*\)/events.*|\1|p')
            [ -n "$key" ] || key=$(printf '%s' "$line" | sed -n 's/.*number=\([0-9]*\).*/\1/p')
            f="$here/../$a$key"
            if [ -n "$key" ] && [ -f "$here/../together-$key" ]; then
              : >"$here/../started-$key"
              want=$(ls "$here/.." | grep -c '^together-')
              tries=0
              while [ "$(ls "$here/.." | grep -c '^started-')" -lt "$want" ]; do
                tries=$((tries + 1))
                [ "$tries" -le 100 ] || { printf 'alone' >&2; printf '{}' >"$out"; answer 1; }
                sleep 0.05
              done
            fi
            [ -f "$f" ] || { printf '{}' >"$out"; answer 1; }
            cat "$f" >"$out" ;;
        *) printf '%s' "$a" >"$out" ;;
      esac
      answer 0 ;;
  esac
done
IFS=$OLDIFS
printf '{}' >"$out""#,
        );
        // The store says it holds nothing. `batch` answers its own request the
        // way the store does: the shape of that answer is the store's contract,
        // tested beside the store, so here a quiet host's store is enough. The
        // words the store is asked by are held in `store-words` so that a case
        // can widen them without a second place to edit.
        let store_body = String::from(
            r#"# `batch` is the one verb whose request is on standard input rather than in
# the arguments, as `store batch` reads it, so the request is taken from there
# and the words of the call are just the verb.
case "$line" in
  batch*) req=$(mktemp "$here/../store-req.XXXXXX")
    # The request arrives on the poll's standard input, after the verb: read whole
    # here, appended to the record of the call the header opened, and kept for the
    # answer below. The separators are substituted by `stub`, so the answer of a
    # run cannot write one of them itself.
    read -r batch_request
    printf '%s' "{sep}{req}$batch_request{rust}" >>"$here/{name}-calls"
    printf '%s' "$batch_request" >"$req"
    acc=[]
    # One answer per call of the request, in its order. A record a case wrote
    # beside the programs, named for the verb and its number, answers that call
    # with the very bytes the single call would have printed; a call with no
    # record of its own gets the quiet answer this host has always given. The
    # line goes into the answer as text, as `store batch` puts a verb's line in
    # `out` -- which is why the record's bytes are handed to `jq` as an argument
    # rather than read by it, and are not parsed on the way.
    n=$(jq 'length' "$req")
    i=0
    while [ "$i" -lt "$n" ]; do
      verb=$(jq -r ".[$i][0] // \"\"" "$req")
      num=$(jq -r ".[$i][1] // \"\"" "$req")
      f="$here/../.$verb-$num"
      if [ -f "$f" ]; then
        said=$(cat "$f")
      else
        case "$verb" in
          plan-show) said='{"api":1,"ok":true,"verb":"plan-show","planning":null}' ;;
          history-show) said='{"api":1,"ok":true,"verb":"history-show","rows":[]}' ;;
          approval-current) said='{"api":1,"ok":true,"verb":"approval-current","current":null}' ;;
          park-show) said='{"api":1,"ok":true,"verb":"park-show","park":null}' ;;
          question-known) said='{"api":1,"ok":true,"verb":"question-known","known":true}' ;;
          *) said='{"api":1,"ok":true,"verb":"'"$verb"'"}' ;;
        esac
      fi
      acc=$(printf '%s' "$acc" | jq -c --arg out "$said" '. + [{rc: 0, out: $out}]')
      i=$((i + 1))
    done
    rm -f "$req"
    printf '{"api":1,"ok":true,"verb":"batch","answers":%s}' "$acc" >"$out"
    answer 0 ;;
  $(sed -n 1p "$here/../store-words")*) printf '%s' '{"api":1,"ok":true,"verb":"plan-show","sessions":[]}' >"$out" ;;
  cycle-show*) printf '%s' '{"api":1,"ok":true,"verb":"cycle-show","cycles":[]}' >"$out" ;;
  $(sed -n 2p "$here/../store-words")*) printf '%s' '{"api":1,"ok":true,"verb":"history-show","rows":[]}' >"$out" ;;
  $(sed -n 3p "$here/../store-words")*) printf '%s' '{"api":1,"ok":true,"verb":"park-show","parks":[]}' >"$out" ;;
  approval-current*) printf '%s' '{"api":1,"ok":true,"verb":"approval-current","current":null}' >"$out" ;;
  # The approvals of `tehas start` a case wrote beside the programs, or none.
  approval-hosts*) if [ -f "$here/../.approval-hosts" ]; then cat "$here/../.approval-hosts" >"$out"
    else printf '%s' '{"api":1,"ok":true,"verb":"approval-hosts","waiting":[]}' >"$out"; fi ;;
  # The answer of any other verb is the one line of its own: a store of this
  # host has nothing to say. The leading dot is spelled out so that the body
  # file `tehas_core::testing` leaves beside the program cannot answer for a
  # read of one issue, whose name begins the same way.
  .store*.body | *.body) printf '{}' >"$out" ;;
  *) printf '%s' "{\"api\":1,\"ok\":true,\"verb\":\"$1\"}" >"$out" ;;
esac"#,
        );
        stub(&dir.join("bin"), "store", &store_body);
        dir
    }

    /// A stub program of `name` in `bin`. It records the arguments it is given
    /// and then runs `body`, which reads them from `$line` and writes its answer
    /// to a file of its own that only `answer` reads. The poll runs its calls
    /// side by side, so nothing of a call is shared: `mktemp` gives the arguments
    /// and the answer each a path of their own, no call waits for another --
    /// which is the point of the batching this file is about -- and no call
    /// reads what another left behind. `RECORD_SEP` separates the calls in the
    /// record, held in a name so that neither this program nor its reader spells
    /// an escape.
    fn stub(bin: &Path, name: &str, body: &str) {
        let mut lines: Vec<String> = [
            "#!/bin/sh".to_string(),
            format!("here='{}'", bin.display()),
            "args=$(mktemp \"$here/{name}-args.XXXXXX\")".to_string(),
            "out=$(mktemp \"$here/{name}-out.XXXXXX\")".to_string(),
            // The running list of answers the stub of the store builds, one
            // element per call of the batch's request. The request of a run of
            // the batch is read here, where the poll's standard input is still
            // open: it answers the calls below and is recorded beside the verb,
            // so a case sees what each run asked.
            "acc=[]".to_string(),
            // The record of the call is the whole call as it was asked. `batch`
            // carries its request on standard input, as the store reads it, and
            // the request is recorded beside the verb: a case counts the runs of
            // the batch and sees the calls each one asked.
            format!(
                "printf '%s{sep}' \"$*\" >>\"$here/{name}-calls\"; printf '%s' \"$*\" >\"$args\"",
                sep = RECORD_SEP,
            ),
            "line=$(cat \"$args\")".to_string(),
            "rm -f \"$args\"".to_string(),
            // The body writes `$out` and leaves through `answer`, which prints
            // it and exits with the status it is given: 0 for an answer, 1 for
            // a read that must look refused. A body that finds nothing writes
            // `{}` and answers that.
            "answer() {".to_string(),
            "  cat \"$out\"".to_string(),
            "  rm -f \"$out\"".to_string(),
            "  exit \"${1:-0}\"".to_string(),
            "}".to_string(),
        ]
        .to_vec();
        lines.extend(body.lines().map(str::to_string));
        lines.push("answer 0".to_string());
        // A call is one record: the arguments, and for `batch` the request the
        // poll wrote on its standard input, which the body reads. They are joined
        // with the separator that keeps a record of its own, so `calls_of` reads
        // one entry a call and sees the request beside the verb that asked it.
        let script = lines
            .join("\n")
            .replace("{name}", name)
            .replace("{sep}", RECORD_SEP)
            .replace("{req}", REQUEST_SEP)
            .replace("{rust}", RUST_SEP)
            + "\n";
        tehas_core::testing::write_exe(bin.join(name), script).unwrap();
    }

    /// The blocked world widened to `count` issues from #21, each on the
    /// queue, labelled blocked and held by #11: enough of them to fill more
    /// than one chunk of the comment read. The world and its queue.
    fn wide_world(count: u64) -> (Value, World) {
        let mut w = world();
        let comment = w["comments"]["21"].clone();
        let labeled = w["events"]["21"].clone();
        let numbers: Vec<u64> = (21..21 + count).collect();
        w["blocked"] = numbers
            .iter()
            .map(|n| json!({"number": n, "labels": [{"name": crate::decide::BLOCKED}]}))
            .collect();
        w["events"] = numbers
            .iter()
            .map(|n| (n.to_string(), labeled.clone()))
            .collect::<serde_json::Map<String, Value>>()
            .into();
        w["comments"] = numbers
            .iter()
            .map(|n| (n.to_string(), comment.clone()))
            .collect::<serde_json::Map<String, Value>>()
            .into();
        (w, blocked_of(&numbers))
    }

    /// A world whose blocked issues all could start: the control issue's events
    /// are read, and every blocker is closed.
    fn startable(mut w: Value) -> Value {
        w["events"]["1"] = json!([]);
        // The markers are an allowed account's, so the rule trusts them.
        for (_, c) in w["comments"].as_object_mut().into_iter().flatten() {
            c["data"]["repository"]["issue"]["comments"]["nodes"][0]["author"]["login"] =
                json!("alice");
        }
        w["states"] = json!({"11": "CLOSED", "12": "CLOSED", "13": "CLOSED"});
        w
    }

    /// A snapshot given the room of a quiet host, as the fixtures' world has it:
    /// a slice with nothing in it and disk to spare, so what the rules decide
    /// rests on the issues alone.
    fn roomy(mut snap: Value) -> Value {
        const GIB: u64 = 1_073_741_824;
        snap["slice"] = json!({
            "show": {"FragmentPath": "/etc/systemd/system/tehas.slice", "ActiveState": "active",
                     "MemoryMax": (20 * GIB).to_string()},
            "docker": "systemd 2",
            "controllers": "cpuset cpu io memory hugetlb pids rdma misc",
            "cpus": "8",
            "root": "/sys/fs/cgroup",
            "scopes": {},
            "pressure": "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0",
        });
        snap["disk_kb"] = json!(40 * 1_048_576u64);
        snap
    }

    /// The calls the stub of `name` recorded, in the order it wrote them.
    fn calls_of(dir: &Path, name: &str) -> Vec<String> {
        std::fs::read_to_string(dir.join("bin").join(format!("{name}-calls")))
            .unwrap_or_default()
            .split(RECORD_SEP)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// The runs of `store batch` the poll asked, as one call per run: the verb,
    /// the separator of the request, and the request itself. A `park-show` of the
    /// lists owed a run of its own is not a run of the batch.
    fn batch_runs(store: &[String]) -> Vec<String> {
        // The record of a run of the batch holds the verb, the separator, and the
        // request: `batch [..]` is the call, and its request is what a case reads.
        store
            .iter()
            .filter(|c| c.contains(REQUEST_SEP))
            .cloned()
            .collect()
    }

    /// The calls the runs of `store batch` asked, as one list: each element a
    /// `[verb, argument]` pair, in the order the store was asked for them.
    fn batched_calls(store: &[String]) -> Vec<Vec<String>> {
        batch_runs(store)
            .iter()
            .flat_map(|c| {
                // `batch«tehas-request»[..]park-show --automatic`: the request is
                // the JSON array between the separator and the verb of the call
                // whose record follows it.
                let after_verb = c.split_once(REQUEST_SEP).map_or("", |(_, r)| r);
                let request = after_verb
                    .split_once(RUST_SEP)
                    .map_or(after_verb, |(r, _)| r);
                let Ok(Value::Array(calls)) = serde_json::from_str(request) else {
                    return Vec::new();
                };
                calls
                    .into_iter()
                    .filter_map(|c| {
                        let pair = c.as_array()?.clone();
                        (pair.len() == 2).then(|| {
                            pair.iter()
                                .map(|a| a.as_str().unwrap_or("").to_string())
                                .collect()
                        })
                    })
                    .collect()
            })
            .collect()
    }

    /// The whole poll of a world: the snapshot, the calls the stubs recorded, what
    /// the poll counted of them, and what it asked the store. The host lives only
    /// as long as this call, so a case cannot leave it behind. The clock of
    /// `run.rs` is held for the whole of it: the poll's reads are measured against
    /// that clock, and a retry case of `run.rs` that set it while one of them ran
    /// would be the reason the read gave up.
    fn poll(world: &Value, broken: &[u64]) -> (Value, Vec<String>, u32, Vec<String>) {
        poll_with(world, broken, &settings())
    }

    /// `poll`, under settings of the case's own.
    fn poll_with(
        world: &Value,
        broken: &[u64],
        c: &Settings,
    ) -> (Value, Vec<String>, u32, Vec<String>) {
        poll_q(&blocked(), world, broken, c)
    }

    /// `poll`, with the queue as the ports' fake holds it: the open work,
    /// the closed running issues, the marks, the types and the open pull
    /// requests all come from `queue`.
    fn poll_q(
        queue: &World,
        world: &Value,
        broken: &[u64],
        c: &Settings,
    ) -> (Value, Vec<String>, u32, Vec<String>) {
        let (snap, gh_calls, gh, store, _) = poll_h_asked(queue, world, broken, c, true);
        (snap, gh_calls, gh, store)
    }

    /// `poll_q`, and without the code host handle at all when `pulls` is
    /// false: a wiring that made none, where the open pull requests are
    /// nothing known rather than none open. The fifth part is the numbers
    /// of each read of the blockers' states.
    fn poll_h_asked(
        queue: &World,
        world: &Value,
        broken: &[u64],
        c: &Settings,
        pulls: bool,
    ) -> (Value, Vec<String>, u32, Vec<String>, Vec<Vec<u64>>) {
        let _clock = crate::read::run::no_case_clock();
        let dir = host(world);
        seed(queue, world, broken);
        let fault = world["note-edits-fault"].as_bool().unwrap_or(false);
        let mut src = sources_of(&dir, queue, pulls, fault);
        let board = test_board(queue, &dir);
        let asked = std::sync::Arc::clone(&board.asked);
        src.board = Some(Box::new(board));
        let snap = match snapshot(&mut src, c) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.line),
        };
        let gh = src.calls.gh_count;
        let asked = asked.lock().unwrap().clone();
        (
            snap,
            calls_of(&dir, "gh"),
            gh,
            calls_of(&dir, "store"),
            asked,
        )
    }

    fn sources(dir: &Path) -> Sources<'_> {
        sources_of(dir, &blocked(), true, false)
    }

    /// `sources` with the queue's handles of the ports' fake: the board,
    /// the tickets and, when `pulls`, the code host, all of one world, as
    /// one wiring makes them, the marks spelt as the table spells them.
    fn sources_of<'a>(
        dir: &'a Path,
        queue: &World,
        pulls: bool,
        note_edits_fault: bool,
    ) -> Sources<'a> {
        use tehas_ports::fake::{FakeSite, Fault, Faulty, Op, TicketsOp};
        use tehas_ports::{Actor, Caller, Scope, Site};
        let plain = FakeSite::new(queue);
        let faulty = || {
            Faulty::new(FakeSite::new(queue))
                .on(Op::Tickets(TicketsOp::NoteEdits), Fault::Unreachable)
        };
        let (by, caller) = (Actor::Ours(Scope::Full), Caller::new("test"));
        let tickets: Box<dyn tehas_ports::Tickets> = if note_edits_fault {
            faulty().tickets(&by, caller).unwrap()
        } else {
            plain.tickets(&by, caller).unwrap()
        };
        let blueprints: Box<dyn tehas_ports::Blueprints> = if note_edits_fault {
            faulty().blueprints(&by, caller).unwrap()
        } else {
            plain.blueprints(&by, caller).unwrap()
        };
        Sources {
            home: dir,
            calls: run::Calls {
                home: Some(dir.to_string_lossy().into_owned()),
                bound: Duration::from_secs(10),
                store_bound: Duration::from_secs(10),
                gh: dir.join("bin").join("gh").to_string_lossy().into_owned(),
                store: dir.join("bin").join("store").to_string_lossy().into_owned(),
                gh_count: 0,
                gh_token: None,
                retry: run::RetryPolicy::default(),
                run_gh: run::gh_process,
            },
            systemctl: "true".into(),
            now: 1_793_404_800,
            models: Models {
                file: tehas_core::models::ModelFile { json: Value::Null },
                used: Vec::new(),
                provider: String::new(),
                model: String::new(),
            },
            endpoint: (String::new(), String::new()),
            code_host: pulls
                .then(|| {
                    plain
                        .code_host(&by, caller)
                        .map(|h| h as Box<dyn tehas_ports::CodeHost>)
                })
                .transpose()
                .unwrap(),
            board: Some(Box::new(test_board(queue, dir))),
            tickets: Some(tickets),
            mark_name: board::fake::label,
            marker_text: |m| tehas_ports::fake::marker_text(m),
            blueprints: Some(blueprints),
            said: Vec::new(),
            faults: Vec::new(),
        }
    }

    /// Seed the case's comments onto the queue: the same world read back
    /// through the tickets port. The texts are kept as given, and an
    /// edited comment gets the history of its case answer: the versions
    /// newest first, the last edit first below the text as created. Each
    /// poll seeds anew, so a case that widens its world between two polls
    /// reads what it holds then. `broken` names issues whose comments the
    /// site refuses.
    fn seed(queue: &World, world: &Value, broken: &[u64]) {
        queue.clear_broken_comments();
        for n in broken {
            queue.fail_comments(*n);
        }
        for (k, answer) in world["comments"].as_object().into_iter().flatten() {
            let Ok(n) = k.parse::<u64>() else { continue };
            let nodes = answer["data"]["repository"]["issue"]["comments"]["nodes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            queue.clear_comments(n);
            for node in &nodes {
                let author = node["author"]["login"].as_str().unwrap_or("");
                let body = node["body"].as_str().unwrap_or("");
                let Some(id) = queue.add_comment(n, author, body) else {
                    continue;
                };
                if node["lastEditedAt"].is_string() {
                    let mut hist = node["userContentEdits"]["nodes"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    if hist.is_empty() {
                        // A case that names an editor and no versions: the
                        // text was edited once, by them.
                        hist = vec![
                            json!({"editor": node["editor"].clone()}),
                            json!({"editor": node["author"].clone()}),
                        ];
                    }
                    // Newest first, the text as created last: the world
                    // keeps edits oldest first.
                    for v in hist.iter().rev().skip(1) {
                        let by = v["editor"]["login"].as_str().unwrap_or("nobody");
                        queue.edit_comment_elsewhere(&id, body, by);
                    }
                }
            }
        }
    }

    /// The comment nodes of issue `n`'s snapshot answer.
    fn nodes_of(snap: &Value, n: u64) -> Vec<Value> {
        snap["issues"][n.to_string()]["comments"]["data"]["repository"]["issue"]["comments"]
            ["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// The line of the refused read of comments, in the outcome's words.
    const FAILED_NOTES: &str = "the site failed; it may have been sent";

    #[test]
    fn a_poll_without_the_handles_of_the_queue_read_fails_closed_on_it() {
        let _clock = crate::read::run::no_case_clock();
        let dir = host(&world());
        let mut src = sources(&dir);
        src.board = None;
        let Err(f) = snapshot(&mut src, &settings()) else {
            panic!("a poll with no board handle must end on the queue read");
        };
        assert_eq!(
            f.line,
            "ERROR: the poll has no board handle to read the queue by"
        );
        let mut src = sources(&dir);
        src.tickets = None;
        let Err(f) = snapshot(&mut src, &settings()) else {
            panic!("a poll with no tickets handle must end on the queue read");
        };
        assert_eq!(
            f.line,
            "ERROR: the poll has no tickets handle to read the queue by"
        );
    }

    /// The states of the blockers a case expects, as `Issue::blockers` holds
    /// them.
    fn states(want: &[(u64, &str)]) -> BTreeMap<u64, String> {
        want.iter().map(|(n, s)| (*n, (*s).to_string())).collect()
    }

    fn settings() -> Settings {
        Settings::parse(&json!({
            "TEHAS_REPO": "acme/widget",
            "TEHAS_CONTROL_ISSUE": 1,
            "TEHAS_BOT_LOGIN": "tehas-bot",
            "TEHAS_ALLOWED_LABELERS": "alice",
        }))
    }

    /// Two blocked issues that share a blocker read their comments through
    /// the tickets port and their blockers once: no gh call reads a comment,
    /// one call answers the states of every blocker the queue names, and each
    /// issue is given only the numbers its own comment names (#1161).
    ///
    /// The contract is asserted against the shape of the calls and against
    /// `Snapshot::parse`, the type the rules actually read: a read that went
    /// back to a gh process per issue, a call for a number no rule asked
    /// about, or an answer whose shape `parse` cannot type, all fail here.
    #[test]
    fn the_blocked_issues_read_their_comments_side_by_side_and_their_blockers_once() {
        let w = world();
        let blocked_len = w["blocked"].as_array().unwrap().len();
        let (snap, calls, gh_count, store, blocker_reads) =
            poll_h_asked(&blocked(), &w, &[], &settings(), true);

        // No gh call reads a comment: the notes come through the tickets
        // port, and the queries a comment used to pay are gone. The one
        // call that remains per issue is its events.
        assert!(
            calls
                .iter()
                .all(|c| !c.contains("comments(last") && !c.contains("nodes(ids:")),
            "no gh call reads a comment: {calls:?}"
        );
        let _ = blocked_len;
        // The states of every blocker are one read of the board. #23 names
        // #13, which no other issue names, so a read of the same list one
        // issue at a time costs three reads here, not one.
        assert_eq!(
            blocker_reads,
            [vec![11, 12, 13]],
            "the states of the blockers are one read of each blocker once"
        );
        // The poll's own record of what it paid agrees with the stub's.
        assert_eq!(
            gh_count as usize,
            calls.len(),
            "the poll counted every gh call it made"
        );

        // Each issue holds its own numbers, from that one answer, and the shape
        // is what the reader's types say it is.
        let parsed = Snapshot::parse(&snap);
        assert_eq!(parsed.issues[&21].blockers, states(&[(11, "OPEN")]));
        assert_eq!(
            parsed.issues[&22].blockers,
            states(&[(11, "OPEN"), (12, "CLOSED")])
        );
        assert_eq!(parsed.issues[&23].blockers, states(&[(13, "OPEN")]));
        for n in 21..=23 {
            assert!(
                parsed.issues[&n].comments.is_some(),
                "issue {n} has the comments the poll read"
            );
            let nodes = nodes_of(&snap, n);
            assert_eq!(nodes.len(), 1, "issue {n} keeps its own comment");
            assert_eq!(
                nodes[0]["body"],
                w["comments"][n.to_string()]["data"]["repository"]["issue"]["comments"]["nodes"][0]
                    ["body"],
                "issue {n} keeps its own comment"
            );
            assert_eq!(nodes[0]["author"]["login"], "tehas-bot");
            assert_eq!(
                nodes[0]["userContentEdits"],
                json!({"totalCount": 0, "nodes": []})
            );
        }
        // The store is still asked once for the per-issue reads of the three
        // issues: batching the two reads of rule 6 did not turn one run of
        // `store batch` into three, and every park and approval of the list is
        // asked in one of its requests rather than by a call of its own.
        let runs = batch_runs(&store);
        let asked = batched_calls(&store);
        for n in 21..=23 {
            assert!(
                asked
                    .iter()
                    .any(|c| c == &["park-show".to_string(), n.to_string()]),
                "issue {n}'s park is asked in a run of the batch: {store:?}"
            );
            assert!(
                asked
                    .iter()
                    .any(|c| c == &["approval-current".to_string(), n.to_string()]),
                "issue {n}'s approval is asked in a run of the batch: {store:?}"
            );
        }
        // The three issues are two reads apiece: the runs that asked them are
        // fewer than the calls, so the list went in batches and not one apiece.
        assert!(
            runs.len() < asked.len(),
            "{} runs of the store answered the {asked:?} of the batch",
            runs.len()
        );
    }

    /// With TEHAS_ISSUE_TYPES mapping improvement, the queue asks the owner's
    /// types and an issue of the type carries `tehas:improvement` in every
    /// read the rules take of it, with no read of its events by name.
    /// Without the setting the queue asks neither: no ticket read is paid for
    /// a type no rule maps.
    #[test]
    fn an_issue_of_the_improvement_type_carries_the_label_and_costs_no_read() {
        let w = world();
        let typed = blocked()
            .kind(tehas_ports::Kind::Task)
            .kind(tehas_ports::Kind::named("Improvement"))
            .ticket(40, |t| {
                t.title("issue 40")
                    .kind(tehas_ports::Kind::named("Improvement"))
            })
            .ticket(41, |t| t.title("issue 41").kind(tehas_ports::Kind::Task));
        let mut c = settings();
        c.improvement_type = Some("Improvement".into());
        let (snap, calls, _, _) = poll_q(&typed, &w, &[], &c);
        // The fake's types beside the two the world names.
        let types = snap["issue_types"].as_array().unwrap();
        assert!(
            types.contains(&json!("Task")) && types.contains(&json!("Improvement")),
            "{types:?}"
        );
        let improvement = json!([{"name": crate::decide::IMPROVEMENT}]);
        assert_eq!(snap["issues"]["40"]["work"]["labels"], improvement);
        assert_eq!(snap["issues"]["40"]["view"]["labels"], improvement);
        assert_eq!(snap["issues"]["41"]["work"]["labels"], json!([]));
        assert!(!calls.iter().any(|c| c.contains("issues/40/events")));

        let (snap, _, _, _) = poll_q(&typed, &w, &[], &settings());
        assert_eq!(snap["issues"]["40"]["work"]["labels"], json!([]));
        assert!(snap.get("issue_types").is_none());
    }

    /// An open issue titled with the epic marker that lacks `epic` is listed for
    /// the label. Its events are read whole when the page holds none of the
    /// label, so a removal the page cut off is still seen; one the page does
    /// show, and an issue that carries the label, cost no read.
    #[test]
    fn an_epic_titled_issue_is_listed_and_its_events_read_whole_when_the_page_shows_none() {
        use tehas_ports::{At, Happened, Mark};
        let mut w = world();
        let at = Some(At::new(
            tehas_core::time::from_iso("2026-10-02T00:00:00Z").unwrap(),
        ));
        let epic = "EPIC: Billing rework";
        let epics = blocked()
            .ticket(30, |t| t.title(epic))
            .ticket(31, |t| {
                t.title(epic)
                    .event(Happened::Unmarked(Mark::Epic), Some("alice"), at)
            })
            .ticket(32, |t| t.title(epic).marked(Mark::Epic, "alice"))
            .ticket(33, |t| t.title("feat: a part of epic 30"));
        w["events"]["30"] = json!([{"event": "unlabeled", "label": {"name": "epic"},
            "actor": {"login": "bob"}, "created_at": "2026-09-01T00:00:00Z"}]);
        let (snap, calls, _, _) = poll_q(&epics, &w, &[], &settings());
        assert_eq!(snap["epic_titled"], json!([30, 31]));
        for n in 30..=33 {
            let read = calls
                .iter()
                .filter(|c| c.contains(&format!("issues/{n}/events?")))
                .count();
            assert_eq!(read, usize::from(n == 30), "#{n}: {calls:?}");
        }
        assert_eq!(snap["issues"]["30"]["events"], w["events"]["30"]);
    }

    /// A ready issue whose page timeline was cut right after the `tehas:ready`
    /// its last cycle started on: the newest application on the page is that
    /// used approval, so the guard sees nothing wrong, and every start would be
    /// refused as an approval that started a cycle already. The events are read
    /// whole, and the person's newer label is what the rules see. An issue whose
    /// page shows a label newer than its last start costs no read.
    #[test]
    fn a_ready_label_no_newer_than_the_last_start_reads_the_events_whole() {
        use tehas_ports::{Act, At, Happened, Mark, State};
        let mut w = world();
        let ready = Mark::Act(Act::Approve);
        let question = Mark::State(State::Question);
        let whole = [
            (true, "2026-10-02T22:13:03Z", &ready, "alice"),
            (false, "2026-10-03T02:30:05Z", &ready, "tehas-bot"),
            (true, "2026-10-03T03:00:07Z", &question, "tehas-bot"),
            (false, "2026-10-03T12:26:07Z", &question, "alice"),
            (true, "2026-10-03T12:26:07Z", &ready, "alice"),
        ];
        let at = |iso: &str| Some(At::new(tehas_core::time::from_iso(iso).unwrap()));
        let issue = |t: tehas_ports::fake::TicketSpec, n: u64, events: usize| {
            whole[..events].iter().fold(
                t.title(&format!("issue {n}"))
                    .text("Words.")
                    .mark(ready.clone()),
                |t, (on, iso, mark, who)| {
                    let what = match on {
                        true => Happened::Marked((*mark).clone()),
                        false => Happened::Unmarked((*mark).clone()),
                    };
                    t.event(what, Some(who), at(iso))
                },
            )
        };
        // #50 is cut after its first event, #51 is whole.
        let ready_world = blocked()
            .ticket(50, |t| issue(t, 50, 1))
            .ticket(51, |t| issue(t, 51, whole.len()));
        let full: Vec<Value> = whole
            .iter()
            .map(|(on, iso, mark, who)| {
                json!({"event": if *on { "labeled" } else { "unlabeled" },
                    "label": {"name": board::fake::label(mark)},
                    "actor": {"login": who}, "created_at": iso})
            })
            .collect();
        let sum = tehas_core::approval::body_sha256(&json!({"body": "Words."}));
        let at = tehas_core::time::from_iso("2026-10-02T22:13:03Z").unwrap();
        let said = format!(
            "{{\"api\":1,\"ok\":true,\"verb\":\"approval-current\",\"approval\":null,\"current\":{{\"at\":{at},\"by\":\"alice\",\"how\":\"approved\",\"body_sha256\":\"{sum}\",\"input\":{{}}}},\"rests_on\":null}}"
        );
        w["approvals"] = json!({"50": said, "51": said});
        w["events"]["50"] = json!(full);
        w["events"]["51"] = json!(full);
        let (snap, calls, _, _) = poll_q(&ready_world, &w, &[], &settings());
        for n in [50, 51] {
            let read = calls
                .iter()
                .filter(|c| c.contains(&format!("issues/{n}/events?")))
                .count();
            assert_eq!(read, usize::from(n == 50), "#{n}: {calls:?}");
        }
        assert_eq!(snap["issues"]["50"]["events"], json!(full));
        assert_eq!(
            snap["issues"]["50"]["approval"]["at"],
            json!("2026-10-02T22:13:03Z")
        );
    }

    /// The refreshing sweep reads the events of its pull requests side by side,
    /// as the read of the issues that lost the mark does (#1423): the stub
    /// answers the events of #31, #32 and #33 only once all three have started,
    /// so a sweep that reads them one at a time is refused, and each pull
    /// request keeps its own answer. A refused read is `{"failed": true}`.
    #[test]
    fn the_refreshing_sweep_reads_the_events_of_its_pull_requests_side_by_side() {
        let mut w = world();
        let labeled = |n: u64| {
            json!([{"event": "labeled", "label": {"name": "tehas:refreshing"},
                    "actor": {"login": "tehas-bot"}, "created_at": format!("2026-10-0{}T00:00:00Z", n - 30)}])
        };
        w["refreshing"] = json!([31, 32, 33, 34].map(|n| json!({"number": n,
            "labels": [{"name": "tehas:refreshing"}]})));
        for n in 31..=33u64 {
            w["events"][n.to_string()] = labeled(n);
        }
        w["together"] = json!([31, 32, 33]);
        // #34 has no events to give, so its read is refused. A waiting
        // request is what has the list read.
        let _clock = crate::read::run::no_case_clock();
        let dir = host(&w);
        std::fs::write(Home::new(&*dir).refresh_request(40), r#"{"asked":"pr"}"#).unwrap();
        let mut src = sources(&dir);
        let snap = match snapshot(&mut src, &settings()) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.line),
        };
        let (calls, gh_count) = (calls_of(&dir, "gh"), src.calls.gh_count);

        for n in 31..=33u64 {
            assert_eq!(
                snap["issues"][n.to_string()]["events"],
                labeled(n),
                "pull request {n} keeps its own events: {calls:?}"
            );
        }
        assert_eq!(snap["issues"]["34"]["events"], json!({"failed": true}));
        for n in 31..=34u64 {
            let path = format!("repos/acme/widget/issues/{n}/events?per_page=100");
            assert!(
                calls.iter().any(|c| c.contains(&path)),
                "pull request {n} has its events read: {calls:?}"
            );
        }
        assert_eq!(
            gh_count as usize,
            calls.len(),
            "the poll counted every gh call it made"
        );
    }

    /// With no refresh on record, the pull requests marked tehas:refreshing
    /// are listed while a request waits, whoever wrote it, and not when none
    /// does.
    #[test]
    fn the_refreshing_list_is_read_while_a_request_waits() {
        for (body, read) in [
            (Some(r#"{"asked":"pr"}"#), true),
            (Some(r#"{"at":"x"}"#), true),
            (None, false),
        ] {
            let _clock = crate::read::run::no_case_clock();
            let mut w = world();
            w["refreshing"] = json!([]);
            let dir = host(&w);
            if let Some(body) = body {
                std::fs::write(Home::new(&*dir).refresh_request(40), body).unwrap();
            }
            let mut src = sources(&dir);
            let s = settings();
            let snap = match snapshot(&mut src, &s) {
                Ok(s) => s,
                Err(f) => panic!("the stubs answer: {}", f.line),
            };
            let calls = calls_of(&dir, "gh");
            assert_eq!(
                calls.iter().any(|c| c.contains("tehas:refreshing")),
                read,
                "{body:?}: {calls:?}"
            );
            assert_eq!(snap.get("refreshing").is_some(), read, "{body:?}");
            let waiting = if body.is_some() {
                json!([40])
            } else {
                json!([])
            };
            assert_eq!(snap["refresh_requests"], waiting);
        }
    }

    /// The record that a hold was said is read at the path and the length its
    /// writer in apply.rs uses, so a key it kept is recognised as said.
    #[test]
    fn the_said_record_is_read_where_and_as_far_as_its_writer_keeps_it() {
        let _clock = crate::read::run::no_case_clock();
        let dir = host(&world());
        std::fs::write(Home::new(&*dir).refresh_request(40), r#"{"at":"x"}"#).unwrap();
        let key = "k".repeat(SAID_MAX + 50);
        std::fs::write(said_file(&dir, 40), &key).unwrap();
        let mut src = sources(&dir);
        let snap = match snapshot(&mut src, &settings()) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.line),
        };
        assert_eq!(
            snap["issues"]["40"]["refresh_said"],
            json!(&key[..SAID_MAX])
        );
    }

    /// The record of a plan's start is read for the issues the rule can start,
    /// each its own, and it reaches the type the rule reads.
    #[test]
    fn a_candidate_is_given_the_record_of_its_plan_and_no_other_is() {
        // The clock of `run.rs` stands for the reads of this host, as it does for
        // every poll of these cases.
        let _clock = crate::read::run::no_case_clock();
        let dir = host(&world());
        std::fs::create_dir_all(Home::new(&*dir).plan_starts()).unwrap();
        for n in [21, 99] {
            let body = format!(r#"{{"by":"alice","sum":"{n}"}}"#);
            std::fs::write(host::plan_start_file(&dir, n), body).unwrap();
        }
        let mut src = sources(&dir);
        let snap = match snapshot(&mut src, &settings()) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.line),
        };
        let parsed = Snapshot::parse(&snap);
        assert_eq!(
            parsed.issues[&21].plan_start,
            Some(crate::snapshot::PlanStart::Record {
                by: "alice".into(),
                sum: Some("21".into())
            })
        );
        assert_eq!(parsed.issues[&22].plan_start, None);
        assert!(!parsed.issues.contains_key(&99), "no candidate, no read");
    }

    /// The last poll's two notes are read where the writer leaves them.
    #[test]
    fn the_notes_of_the_last_poll_are_read_from_their_files() {
        let _clock = crate::read::run::no_case_clock();
        let dir = host(&world());
        std::fs::write(
            Home::new(&*dir).state(State::BranchCheck),
            "main is gone\n\n",
        )
        .unwrap();
        std::fs::write(Home::new(&*dir).state(State::RefreshHeld), "no continue-pr").unwrap();
        let mut src = sources(&dir);
        let snap = match snapshot(&mut src, &settings()) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.line),
        };
        assert_eq!(snap["branch_note"], json!("main is gone"));
        assert_eq!(snap["refresh_held"], json!("no continue-pr"));
    }

    /// The host's approvals reach the rules only for the issues open on the
    /// queue's page: an approval of a closed issue, or of a number the page
    /// does not hold, is dropped, so nothing of it is started.
    #[test]
    fn a_host_approval_of_an_issue_not_open_is_dropped() {
        let _clock = crate::read::run::no_case_clock();
        let w = world();
        // #40 is closed and still carries the running mark: the queue names
        // it in `marked` as a closed one, and nothing else.
        let queue = blocked().ticket(40, |t| {
            t.title("issue 40")
                .mark(tehas_ports::Mark::State(tehas_ports::State::Running))
                .closed(tehas_ports::CloseReason::Done)
        });
        let dir = host(&w);
        let waiting = |n: u64| json!({"issue": n, "at": 1_793_404_000, "body_sha256": "x", "title_sha256": null});
        fixture(
            &dir,
            "approval-hosts",
            &json!({"api": 1, "ok": true, "verb": "approval-hosts",
                    "waiting": [waiting(21), waiting(40), waiting(77)]}),
        );
        let mut src = sources_of(&dir, &queue, true, false);
        let snap = match snapshot(&mut src, &settings()) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.line),
        };
        let starts: Vec<u64> = snap["host_starts"]
            .as_array()
            .unwrap_or_else(|| panic!("the approvals were read: {}", snap["host_starts"]))
            .iter()
            .filter_map(|h| h["issue"].as_u64())
            .collect();
        assert_eq!(starts, vec![21], "only the open issue's approval stands");
        let parsed = Snapshot::parse(&snap);
        let hosts: Vec<u64> = parsed
            .host_starts
            .unwrap_or_default()
            .iter()
            .filter_map(|h| h.issue)
            .collect();
        assert_eq!(hosts, vec![21]);
    }

    /// A comment that names nothing asks nothing of the blockers: the poll pays
    /// no call for a list of none, and the issue's blockers are empty rather
    /// than unreadable.
    #[test]
    fn a_blocked_issue_with_nothing_to_name_asks_nothing_of_the_blockers() {
        let mut world = world();
        // The one blocked issue of the queue, and its newest comment names no
        // blocker at all.
        let queue = blocked_of(&[21]);
        world["events"] = json!({"21": world["events"]["21"]});
        // An unedited comment, with the empty history GitHub gives one.
        world["comments"] = json!({"21": {"data": {"repository": {"issue": {"comments":
                {"nodes": [{"body": "no marker here",
                            "userContentEdits": {"totalCount": 0, "nodes": []}}]}}}}}});
        let (snap, _, _, _, blocker_reads) = poll_h_asked(&queue, &world, &[], &settings(), true);
        assert!(
            blocker_reads.is_empty(),
            "no blocker is named, so none is asked: {blocker_reads:?}"
        );
        let parsed = Snapshot::parse(&snap);
        assert!(parsed.issues[&21].blockers.is_empty());
        assert!(parsed.issues[&21].comments.is_some());
        let nodes = nodes_of(&snap, 21);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["body"], "no marker here");
        assert_eq!(
            nodes[0]["userContentEdits"],
            json!({"totalCount": 0, "nodes": []})
        );
    }

    /// An edited comment's history is read on `Tickets::note_edits` and put
    /// back on it, as the comment read itself gave it before: an outside
    /// account's edit of a comment the factory's own account saved last still
    /// holds, every edit counting. An unedited comment asks for no history:
    /// with the histories themselves refused, its answer stands whole.
    #[test]
    fn an_edited_comment_carries_the_history_read_by_its_id() {
        let mut world = world();
        let queue = blocked_of(&[21]);
        world["events"] = json!({"21": world["events"]["21"]});
        world["comments"] = json!({"21": {"data": {"repository": {"issue": {"comments":
        {"nodes": [
            {"id": "IC_kwDOplain", "body": "never edited",
             "lastEditedAt": null, "editor": null},
            {"id": "IC_kwDOedited", "body": "<!-- tehas:retry -->",
             "author": {"login": "tehas-bot"},
             "lastEditedAt": "2026-10-04T00:00:00Z",
             "editor": {"login": "mallory"}}
        ]}}}}}});
        let _ = ();
        let (snap, _, _, _) = poll_q(&queue, &world, &[], &settings());
        let nodes = nodes_of(&snap, 21);
        assert_eq!(nodes.len(), 2);
        assert_eq!(
            nodes[0]["userContentEdits"],
            json!({"totalCount": 0, "nodes": []}),
            "the unedited comment asks no history"
        );
        let h = &nodes[1]["userContentEdits"];
        assert_eq!(h["totalCount"], json!(2), "{h}");
        assert_eq!(h["nodes"][0]["editor"]["login"], json!("mallory"));
        assert_eq!(h["nodes"][1]["editor"]["login"], json!("tehas-bot"));
        let ours = |w: &str| w == "tehas-bot";
        assert_eq!(
            tehas_core::events::edit_holds(&nodes[1], "", "", &ours).as_deref(),
            Some("mallory"),
            "the outside edit still holds"
        );

        // With no edited comment, no history is read: refuse them all and
        // the answer of the unedited comment stands.
        world["comments"]["21"]["data"]["repository"]["issue"]["comments"]["nodes"] =
            json!([world["comments"]["21"]["data"]["repository"]["issue"]["comments"]["nodes"][0]]);
        world["note-edits-fault"] = json!(true);
        let (snap, _, _, _) = poll_q(&queue, &world, &[], &settings());
        let nodes = nodes_of(&snap, 21);
        assert_eq!(
            nodes[0]["userContentEdits"],
            json!({"totalCount": 0, "nodes": []}),
            "no history is asked of an unedited comment: {nodes:?}"
        );
    }

    /// A history the answer leaves out is a read that failed, which holds: the
    /// comment cannot be shown to have only allowed editors.
    #[test]
    fn an_edited_comment_whose_history_is_not_answered_holds() {
        let mut world = world();
        let queue = blocked_of(&[21]);
        world["events"] = json!({"21": world["events"]["21"]});
        world["comments"] = json!({"21": {"data": {"repository": {"issue": {"comments":
        {"nodes": [
            {"id": "IC_kwDOedited", "body": "<!-- tehas:retry -->",
             "author": {"login": "tehas-bot"},
             "lastEditedAt": "2026-10-04T00:00:00Z",
             "editor": {"login": "tehas-bot"}}
        ]}}}}}});
        world["note-edits-fault"] = json!(true);
        let (snap, _, _, _) = poll_q(&queue, &world, &[], &settings());
        let c = &nodes_of(&snap, 21)[0];
        assert_eq!(c["userContentEdits"]["failed"], json!(true), "{c}");
        let ours = |w: &str| w == "tehas-bot";
        assert!(tehas_core::events::edit_holds(c, "", "", &ours).is_some());
        assert!(tehas_core::events::edit_history_unread(c, &ours));
    }

    /// A comment read the site refused is a failed read for that issue, not
    /// an empty one: the snapshot holds `{"failed": true}` and the outcome's
    /// line, which `Snapshot::parse` turns into no comments at all, and
    /// decide ends the poll on it rather than reading an issue as one that
    /// names no blocker, so a hold stays a hold and the held issue starts
    /// nothing. The other issues keep their answers: the read is per issue
    /// now, and the refused read of one is no longer the failure of all.
    #[test]
    fn a_failed_comment_read_holds_its_issue_and_starts_nothing() {
        let w = startable(world());
        // With every read answered, the poll goes as far as a start.
        let (snap, _, _, _) = poll(&w, &[]);
        let d = crate::decide::decide(&Snapshot::parse(&roomy(snap)), &settings(), true);
        assert!(
            !d.picked.is_empty(),
            "the answered world starts: {:?}",
            d.log
        );
        // #22's comments are refused; #21 and #23 stand.
        let (snap, _, _, _) = poll(&w, &[22]);
        let parsed = Snapshot::parse(&snap);
        assert_eq!(
            snap["issues"]["22"]["comments"],
            json!({"failed": true, "line": FAILED_NOTES}),
            "#22's read is refused"
        );
        assert!(
            parsed.issues[&22].comments.is_none(),
            "#22 has no comments to decide on"
        );
        for n in [21, 23] {
            assert!(
                parsed.issues[&n].comments.is_some(),
                "issue {n} keeps the comments of its own read"
            );
        }
        let d = crate::decide::decide(&Snapshot::parse(&roomy(snap.clone())), &settings(), true);
        assert_ne!(d.exit, 0, "the poll ends on the failed read: {:?}", d.log);
        assert!(
            !d.picked.contains(&22),
            "the held issue starts nothing: {:?}",
            d.picked
        );
        assert!(
            d.log
                .iter()
                .any(|l| l.starts_with("ERROR: unreadable comments of #22")
                    && l.ends_with(&format!(": {FAILED_NOTES}"))),
            "the poll names the read and the outcome's words: {:?}",
            d.log
        );
    }

    /// The comment read is one read of the tickets port an issue: twelve
    /// blocked issues all keep their own comment, and a read the site
    /// refused holds that issue alone and spoils none of the others'
    /// answers. A refused read still ends the whole poll: a hold the poll
    /// cannot read stays a hold, and nothing starts.
    #[test]
    fn the_comment_read_is_one_an_issue_and_a_failed_read_spoils_only_its_own() {
        let (w, q) = wide_world(12);
        let w = startable(w);
        let (snap, _, _, _) = poll_q(&q, &w, &[], &settings());
        for n in 21..=32 {
            let nodes = nodes_of(&snap, n);
            assert_eq!(nodes.len(), 1, "issue {n} is read once, whole");
            assert_eq!(
                nodes[0]["body"],
                w["comments"][n.to_string()]["data"]["repository"]["issue"]["comments"]["nodes"][0]
                    ["body"],
                "issue {n} keeps its own comment"
            );
        }

        // #32 is refused; its neighbours keep their answers.
        let (snap, _, _, _) = poll_q(&q, &w, &[32], &settings());
        for n in 21..=31 {
            assert!(
                snap["issues"][n.to_string()]["comments"]
                    .get("failed")
                    .is_none(),
                "issue {n} keeps the answer of its own read"
            );
        }
        assert_eq!(
            snap["issues"]["32"]["comments"],
            json!({"failed": true, "line": FAILED_NOTES})
        );
        // The refused issue is held; the poll goes on and the rest start.
        let d = crate::decide::decide(&Snapshot::parse(&roomy(snap.clone())), &settings(), true);
        assert!(
            !d.picked.contains(&32),
            "the refused issue is held: {:?}",
            d.picked
        );

        // #21 is refused: its hold ends the poll, and nothing starts.
        let (snap, _, _, _) = poll_q(&q, &w, &[21], &settings());
        assert_eq!(
            snap["issues"]["21"]["comments"],
            json!({"failed": true, "line": FAILED_NOTES}),
            "#21's read is refused"
        );
        for n in 22..=32 {
            assert!(
                snap["issues"][n.to_string()]["comments"]
                    .get("failed")
                    .is_none(),
                "issue {n} keeps the answer of its own read"
            );
        }
        let d = crate::decide::decide(&Snapshot::parse(&roomy(snap.clone())), &settings(), true);
        assert!(d.picked.is_empty(), "nothing starts: {:?}", d.picked);
        assert_ne!(d.exit, 0, "the poll ends on the failed read: {:?}", d.log);
    }

    /// A merged blocker call that answers nothing leaves every blocked issue
    /// with no state at all, which is what decide reads as "the state of #b
    /// could not be read" -- and not as "this issue names no blocker", which
    /// would let it start.
    #[test]
    fn a_blocker_the_merged_call_did_not_answer_is_left_unread() {
        let mut w = world();
        // The merged read answers #11 and #12 but not #13, which #23 names.
        w["states"] = json!({"11": "OPEN", "12": "CLOSED"});
        let (snap, _, _, _) = poll(&w, &[]);
        let parsed = Snapshot::parse(&snap);
        assert_eq!(parsed.issues[&23].blockers, states(&[]));
        assert_eq!(
            snap["issues"]["23"]["blockers"],
            json!({}),
            "#23 was asked of the call and answered nothing"
        );
        // The two issues whose blockers were answered keep both states.
        assert_eq!(
            parsed.issues[&22].blockers,
            states(&[(11, "OPEN"), (12, "CLOSED")])
        );
    }

    /// A blocked issue that is also asking has its comments read once: the
    /// numbers the two reads want are one list, and the issue publishes the
    /// answer that one read gave.
    #[test]
    fn a_blocked_issue_the_question_read_answered_pays_no_second_comment_read() {
        let mut w = world();
        // #22 carries both labels, so the question read gives its comments.
        let queue = blocked().ticket(22, |t| {
            t.mark(tehas_ports::Mark::State(tehas_ports::State::Question))
        });
        w["asking"] = json!([{"number": 22, "labels": [{"name": "tehas:question"}]}]);
        let (snap, _, _, _) = poll_q(&queue, &w, &[], &settings());
        // The asking issue is one of the numbers asked for, not asked for
        // twice: its answer carries its own comment once.
        let nodes = nodes_of(&snap, 22);
        assert_eq!(nodes.len(), 1, "the double-labelled issue is read once");
        assert_eq!(
            nodes[0]["body"],
            w["comments"]["22"]["data"]["repository"]["issue"]["comments"]["nodes"][0]["body"],
            "the issue keeps the answer the question read gave"
        );
        let parsed = Snapshot::parse(&snap);
        assert!(parsed.issues[&22].comments.is_some());
        assert_eq!(
            parsed.issues[&22].blockers,
            states(&[(11, "OPEN"), (12, "CLOSED")])
        );
    }

    /// An issue the blocked list carries but the queue's page left out is not a
    /// blocked candidate: the batch and rule 6 test the same predicate, so an
    /// issue one of them holds is never written as blocked by the other. The
    /// page, not the list, is what makes an issue blocked.
    #[test]
    fn an_issue_off_the_page_is_not_batched_and_not_written_as_blocked() {
        let w = world();
        // #23 stays a candidate of rule 6 and keeps the label in the blocked
        // list, but the queue did not answer for it.
        let queue = blocked_of(&[21, 22]);
        let (snap, _, _, _, blocker_reads) = poll_h_asked(&queue, &w, &[], &settings(), true);
        // The two issues on the page are read; #23, off the page, is not.
        assert_eq!(nodes_of(&snap, 21).len(), 1);
        assert_eq!(nodes_of(&snap, 22).len(), 1);
        assert_eq!(
            snap["issues"].get("23"),
            None,
            "an issue off the page is not asked"
        );
        // Its blockers are not asked of the merged read either: the union holds
        // only the blockers the two issues on the page name.
        assert_eq!(
            blocker_reads,
            [vec![11, 12]],
            "#23 is not blocked, so the blocker only it names is not asked"
        );
        let parsed = Snapshot::parse(&snap);
        assert_eq!(parsed.issues[&21].blockers, states(&[(11, "OPEN")]));
        // Rule 6 wrote no comments and no blockers for it: the page holds the
        // label, and the page does not hold the issue, so nothing of it is read
        // and it is not in the snapshot at all.
        assert!(
            !parsed.issues.contains_key(&23),
            "an issue off the page is not written as blocked: {:?}",
            parsed.issues.keys().collect::<Vec<_>>()
        );
        assert_eq!(snap["issues"].get("23"), None);
    }

    /// A pull request that is not the repository's own names nothing: one from
    /// a fork, one of an account neither the factory nor allowed. Each closes
    /// #21 and #22 in its body, and neither issue is held. A shape GitHub
    /// answers that the port cannot carry (no author, no head) is the
    /// adapter's problem to answer, not the read's.
    #[test]
    fn a_pull_request_not_of_the_repository_names_nothing() {
        let cases: [(&str, &str, bool); 3] = [
            ("an outsider's fork", "mallory", true),
            ("an allowed account's fork", "alice", true),
            (
                "an outsider on a branch of the repository",
                "mallory",
                false,
            ),
        ];
        for (what, author, fork) in cases {
            let w = world();
            let queue = blocked().change(90, "feature/other-90", |c| {
                c.body("Closes #21, #22").author(author).fork(fork)
            });
            let (snap, _, _, _) = poll_q(&queue, &w, &[], &settings());
            assert_eq!(snap.get("pulls_name"), None, "{what} names nothing");
            assert!(
                Snapshot::parse(&snap).pulls_name.is_empty(),
                "{what} holds nothing"
            );
        }
    }

    /// The factory's own pull request and an allowed account's, both on a
    /// branch of the repository, name what their bodies close.
    #[test]
    fn the_factorys_and_an_allowed_accounts_pull_requests_name_their_issues() {
        let w = world();
        let queue = blocked()
            .change(90, "feature/other-90", |c| {
                c.body("Closes #21").author("app/tehas-bot")
            })
            .change(91, "feature/other-91", |c| {
                c.body("Fixes #22").author("alice")
            });
        let (snap, _, _, _) = poll_q(&queue, &w, &[], &settings());
        assert_eq!(snap["pulls_name"], json!({"21": [90], "22": [91]}));
    }

    /// GitHub links a fork's pull request that closes an issue too: the linked
    /// list keeps only the repository's own, so an outsider's holds nothing.
    #[test]
    fn a_linked_pull_request_not_of_the_repository_is_left_out() {
        let w = world();
        let fake = blocked()
            .change(90, "work-21", |c| c.closes(21).author("tehas-bot"))
            .change(91, "fork-21", |c| {
                c.closes(21).closes(22).author("mallory").fork(true)
            });
        // The link the fake gives the repository's own change, for the
        // `ClosingPr` the snapshot is held to.
        let page = board::fake::page_of(&board::fake::work(&fake));
        let own = page[0].node["closedByPullRequestsReferences"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["isCrossRepository"] == json!(false))
            .unwrap()["url"]
            .clone();
        let (snap, _, _, _) = poll_q(&fake, &w, &[], &settings());
        let parsed = Snapshot::parse(&snap);
        let own = ClosingPr {
            url: own.as_str().unwrap().to_string(),
            open: true,
        };
        assert_eq!(parsed.issues[&21].prs, Some(vec![own]));
        assert_eq!(parsed.issues[&22].prs, Some(vec![]));
    }

    /// The queue's own bodies are what hold an issue a pull request into
    /// another branch closes: GitHub records a closing reference only for a
    /// pull request into the default branch, so an issue whose
    /// `closedByPullRequestsReferences` is empty is held by the body alone,
    /// and `Snapshot::parse` carries the numbers to `pulls_name` for rule 6.
    #[test]
    fn an_open_pull_request_naming_the_issue_in_its_body_is_written_as_holding_it() {
        let w = world();
        // A body that names no keyword, one that names a number with a colon
        // but is not one of the closing words, one that names a longer
        // number, and none at all: none holds anything.
        let queue = blocked()
            .change(90, "feature/other-90", |c| {
                c.body("Closes #21. Fixes #22.").author("alice")
            })
            .change(91, "feature/other-91", |c| {
                c.body("See #21 and issue 22 for background")
                    .author("alice")
            })
            .change(92, "feature/other-92", |c| {
                c.body("Resolves #212, which is not #21").author("alice")
            })
            .change(93, "x", |c| c.author("alice"));
        let (snap, _, _, _) = poll_q(&queue, &w, &[], &settings());
        assert_eq!(
            snap["pulls_name"],
            json!({"21": [90], "22": [90]}),
            "the bodies name #21 and #22, and no other number"
        );
        let parsed = Snapshot::parse(&snap);
        assert_eq!(parsed.pulls_name[&21], vec![90]);
        assert_eq!(parsed.pulls_name[&22], vec![90]);
        assert!(
            !parsed.pulls_name.contains_key(&23),
            "an issue no body names is absent: {:?}",
            parsed.pulls_name
        );
    }

    /// The hold reads only the open pull requests the poll could ask of the
    /// code host: a wiring that made no code host handle writes no names, so
    /// no issue is held by a body no poll read.
    #[test]
    fn a_page_that_answered_no_open_pull_requests_writes_no_names() {
        let w = world();
        let (snap, _, _, _, _) = poll_h_asked(&blocked(), &w, &[], &settings(), false);
        assert_eq!(snap.get("pulls_name"), None);
        assert!(Snapshot::parse(&snap).pulls_name.is_empty());
    }

    /// The numbers of `pulls_name` are read the way every other number of the
    /// snapshot is: a key or a hit of a numeric string reads as the number it
    /// is, and one of no shape a reader takes is dropped with its own entry,
    /// never counted as a pull request that holds the issue.
    #[test]
    fn the_numbers_of_pulls_name_read_as_the_snapshot_reads_a_number() {
        let snap = json!({"pulls_name": {
            "7": [90.5, "91", 92],
            "8": ["93"],
            "9": [],
            "x": [94],
            "10": "93",
            "11": ["not a number"],
        }});
        let parsed = Snapshot::parse(&snap);
        assert_eq!(
            parsed.pulls_name[&7],
            vec![91, 92],
            "a hit that is no number is dropped, the rest of the entry is read"
        );
        assert_eq!(parsed.pulls_name[&8], vec![93], "a hit of a string");
        assert_eq!(
            parsed.pulls_name.get(&9),
            None,
            "an entry of no pull requests holds an issue on nothing"
        );
        assert_eq!(parsed.pulls_name.get(&10), None, "hits of no list");
        assert_eq!(
            parsed.pulls_name.get(&11),
            None,
            "an entry whose every hit is unreadable"
        );
        assert!(
            !parsed.pulls_name.contains_key(&0),
            "a key of no number names no issue: {:?}",
            parsed.pulls_name
        );
    }

    /// state/cycles that cannot be listed is no list of none: the snapshot
    /// says so, and lists no running cycle it cannot see.
    #[test]
    fn a_cycles_directory_that_cannot_be_listed_is_said_in_the_snapshot() {
        let _clock = crate::read::run::no_case_clock();
        let w = world();
        let dir = host(&w);
        let mut src = sources(&dir);
        let snap = match snapshot(&mut src, &settings()) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.line),
        };
        assert_eq!(snap.get("running_unlisted"), None, "a listed directory");
        std::fs::remove_dir_all(Home::new(&*dir).cycles_dir()).unwrap();
        std::fs::write(Home::new(&*dir).cycles_dir(), "not a directory").unwrap();
        let mut src = sources(&dir);
        let snap = match snapshot(&mut src, &settings()) {
            Ok(s) => s,
            Err(f) => panic!("the stubs answer: {}", f.line),
        };
        assert_eq!(snap["running_unlisted"], json!(true));
        assert_eq!(snap["running"], json!([]));
        assert!(Snapshot::parse(&snap).running_unlisted);
    }
}
