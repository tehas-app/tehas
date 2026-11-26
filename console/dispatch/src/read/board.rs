//! The seam between the board port's answer and the shapes the snapshot
//! reads today: `OpenWork` as the queue's page, each `WorkItem` as one open
//! issue of it, each `ChangeDigest` as a pull request of it. Pure. No read
//! moves here: the poll still reads the page, and each read moves onto the
//! ports in a pull request of its own, through this mapping.
//!
//! What the board does not answer is given beside it (the open pull
//! requests, the owner's issue types) or stays read apart: the closed issues
//! that still carry the running mark and the repository's marks with their
//! ids (`Tickets::list`, `Tickets::marks`).

use super::github::{Open, Queue, CREATED, TIMELINE};
use serde_json::json;
use tehas_core::time::iso;
use tehas_ports::{
    At, BlockerState, ChangeDigest, ChangeState, Got, Happened, Kind, Mark, OpenWork, Outcome, Ref,
    Refusal, Ticket, TicketKey, WorkItem,
};

/// How the site spells what today's shapes name by text.
pub struct Words<'a> {
    /// The repository, `owner/name`: a change not from a fork has its head
    /// here, which is what `own_pull` asks of a pull request.
    pub repo: &'a str,
    /// A mark's label: the site's `Site::mark_name`, `""` for a mark it
    /// cannot name.
    pub mark: &'a dyn Fn(&Mark) -> String,
}

/// The board's answer in today's shapes.
pub struct Mapped {
    /// The open issues, in the board's order, and the open pull requests
    /// when they were given.
    pub queue: Queue,
    /// The open tickets this seam cannot give as an issue, with why: the
    /// ticket's own read failed, its key is no number, or it carries a mark
    /// the site cannot name. Each may be running and may be blocked, so it
    /// holds a slot and is never started.
    pub unread: Vec<(TicketKey, Outcome)>,
    /// The control ticket as an issue of the page, if it was asked. Its
    /// failure fails it alone.
    pub control: Option<Got<Open>>,
}

/// What this seam cannot carry: a key that is no number, a mark with no
/// label.
const UNSUPPORTED: Outcome = Outcome::Refused(Refusal::Unsupported);

/// An account as the page names one, or null for none.
macro_rules! account {
    ($a:expr) => {
        match $a {
            Some(a) => json!({"login": a.login.as_str(), "databaseId": a.id.as_ref().map(|id| {
                id.as_str().parse::<u64>().map_or_else(|_| json!(id.as_str()), |n| json!(n))
            })}),
            None => json!(null),
        }
    };
}

/// The open work, the open pull requests (`pulls`, None when they were not
/// read) and the owner's issue types (`kinds`, None when not asked).
pub fn queue(
    work: &OpenWork,
    pulls: Option<&[ChangeDigest]>,
    kinds: Option<&[Kind]>,
    words: &Words,
) -> Mapped {
    let mut open = Vec::new();
    let mut unread = Vec::new();
    for (key, item) in work.items.items() {
        match item
            .as_ref()
            .map_err(Outcome::clone)
            .and_then(|i| issue(i, words))
        {
            Ok(o) => open.push(o),
            Err(why) => unread.push((key.clone(), why)),
        }
    }
    let open_pulls = pulls.map(|all| {
        json!(all
            .iter()
            .map(|c| {
                json!({"number": c.no.get(), "body": c.body, "isCrossRepository": c.from_fork,
                    "author": account!(&c.author),
                    "headRepository": {"nameWithOwner": head_repo(c, words)}})
            })
            .collect::<Vec<_>>())
    });
    Mapped {
        queue: Queue {
            repo: Some(words.repo.to_string()),
            open,
            closed_running: Vec::new(),
            open_pulls,
            issue_types: kinds.map(|k| k.iter().filter_map(kind_name).collect()),
            labels: None,
        },
        unread,
        control: work.control.as_ref().map(|t| {
            t.as_ref()
                .map_err(Outcome::clone)
                .and_then(|t| ticket(t, words))
        }),
    }
}

/// One open work item as an issue of the page: the ticket, its edit record
/// from the blueprint that is the same item, and the changes that close it.
pub fn issue(item: &WorkItem, words: &Words) -> Got<Open> {
    let mut o = ticket(&item.ticket, words)?;
    let same = (item.ticket.digest.key.same_item())
        .and_then(|no| item.blueprints.iter().find(|b| b.no == no));
    if let Some(b) = same {
        o.node["body"] = json!(b.body);
        o.node["lastEditedAt"] = json!(b.edited.map(stamp));
        o.node["editor"] = account!(&b.editor);
    }
    // Open first, then the newest ended, as the board orders them: past its
    // window an open change makes the item unread, so what `more` leaves out
    // is ended changes alone.
    let prs: Vec<_> = (item.changes.items.iter())
        .map(|c| {
            let state = match c.state {
                ChangeState::Open => "OPEN",
                ChangeState::Merged { .. } => "MERGED",
                ChangeState::Closed { .. } => "CLOSED",
            };
            json!({"url": c.link.as_str(), "state": state, "isCrossRepository": c.from_fork,
                "author": account!(&c.author),
                "headRepository": {"nameWithOwner": head_repo(c, words)}})
        })
        .collect();
    o.node["closedByPullRequestsReferences"] = json!({ "nodes": prs });
    Ok(o)
}

/// A ticket as an issue of the page, without what its blueprints and
/// changes give: the control ticket's whole answer.
fn ticket(t: &Ticket, words: &Words) -> Got<Open> {
    let d = &t.digest;
    let TicketKey::Number(number) = &d.key else {
        return Err(UNSUPPORTED);
    };
    let labels = (d.marks.iter())
        .map(|m| Some((words.mark)(m)).filter(|l| !l.is_empty()))
        .collect::<Option<Vec<String>>>()
        .ok_or(UNSUPPORTED)?;
    // An event of a mark with no label authorises nothing and is left out.
    let events: Vec<_> = (t.history.items.iter())
        .filter_map(|e| {
            let (kind, mark) = match &e.what {
                Happened::Marked(m) => ("LabeledEvent", Some(m)),
                Happened::Unmarked(m) => ("UnlabeledEvent", Some(m)),
                Happened::Retitled { .. } => ("RenamedTitleEvent", None),
                _ => return None,
            };
            let mut node = json!({"__typename": kind, CREATED: e.at.map(stamp),
                "actor": account!(&e.by)});
            if let Some(m) = mark {
                let name = (words.mark)(m);
                if name.is_empty() {
                    return None;
                }
                let id = e.mark_id.as_ref().map(|i| i.as_str());
                node["label"] = json!({"id": id, "name": name});
            }
            Some(node)
        })
        .collect();
    let parts = t.parts.len();
    let done = t.parts.iter().filter(|p| !p.open).count();
    let blocked_by = (t.blockers.iter())
        .filter(|b| b.state == BlockerState::Open)
        .count();
    let parent = match &d.parent {
        Some(Ref::Here(TicketKey::Number(p))) => json!({"number": p.get()}),
        _ => json!(null),
    };
    let mut node = json!({
        "number": number.get(), "title": d.title, "body": t.text,
        CREATED: stamp(d.created), "author": account!(&d.author),
        "lastEditedAt": null, "editor": null,
        "labels": {"nodes": labels.iter().map(|l| json!({"name": l})).collect::<Vec<_>>()},
        "subIssuesSummary": {"total": parts, "completed": done},
        "issueDependenciesSummary": {"blockedBy": blocked_by},
        "parent": parent,
        "closedByPullRequestsReferences": {"nodes": []},
    });
    // A history the port says may be cut is left out, as a page that dropped
    // it: the guard then reads it whole by name.
    if !t.history.more {
        node[TIMELINE] = json!({ "nodes": events });
    }
    if let Some(name) = d.kind.as_ref().and_then(kind_name) {
        node["issueType"] = json!({ "name": name });
    }
    Ok(Open {
        number: number.get(),
        labels,
        node,
    })
}

/// A time as the page writes it.
fn stamp(at: At) -> String {
    iso(at.secs())
}

/// The repository a change's head is in, as far as this project knows: its
/// own, or none for a fork's.
fn head_repo<'a>(c: &ChangeDigest, words: &Words<'a>) -> Option<&'a str> {
    (!c.from_fork).then_some(words.repo)
}

/// An issue type's name as the site writes it, as `Kind::named` reads it.
fn kind_name(k: &Kind) -> Option<String> {
    let name = match k {
        Kind::Feature => "Feature",
        Kind::Bug => "Bug",
        Kind::Task => "Task",
        Kind::Epic => "Epic",
        Kind::Other(n) => n.as_str(),
        _ => return None,
    };
    Some(name.to_string())
}

/// The page of a world of the ports' fake, for the tests of the reads: the
/// world through the fake's board and this mapping, each mark spelt as
/// `host/labels.tsv` names it, as GitHub's site spells it.
#[cfg(test)]
pub(crate) mod fake {
    use super::{queue, Mapped, Open, Words};
    use std::num::NonZeroU32;
    use tehas_ports::fake::{FakeSite, World};
    use tehas_ports::{
        Actor, BoardAsk, Bound, Caller, ChangeDigest, ChangeFilter, ChangeStates, Depth,
        HappenedKind, Mark, OpenWork, Scope, Site, Window, ALL_MAX,
    };

    const THE_TABLE: &str = include_str!("../../../../host/labels.tsv");

    /// The repository the tests' worlds are.
    pub(crate) const REPO: &str = "acme/widget";

    /// The label whose latest column is the port word `word`. The one
    /// lookup the tests of this crate share: they hold a word, not a mark.
    pub(crate) fn label_of_word(word: &str) -> Option<String> {
        THE_TABLE.lines().find_map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            (cols.last() == Some(&word)).then(|| cols[0].to_string())
        })
    }

    /// A mark's label as `host/labels.tsv` names it, a project's own mark
    /// by its name.
    pub(crate) fn label(m: &Mark) -> String {
        let word = match m {
            Mark::Act(a) => format!("Act::{a:?}"),
            Mark::State(s) => format!("State::{s:?}"),
            Mark::Choice(c) => format!("Choice::{c:?}"),
            Mark::Epic => "Mark::Epic".to_string(),
            Mark::Project(name) => return name.as_str().to_string(),
            _ => return String::new(),
        };
        label_of_word(&word).unwrap_or_default()
    }

    /// The open work of `world` as the queue asks the board for it.
    pub(crate) fn work(world: &World) -> OpenWork {
        let site = FakeSite::new(world);
        let mut board = site
            .board(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap();
        let mut ask = BoardAsk::new(Depth::Cut { notes: 0 });
        ask.history_by_kind = vec![
            (HappenedKind::Marked, Window::Whole(ALL_MAX)),
            (HappenedKind::Unmarked, Window::Whole(ALL_MAX)),
            (HappenedKind::Retitled, Window::Whole(ALL_MAX)),
        ];
        ask.changes = Some((
            ChangeStates::All,
            Window::Last(NonZeroU32::new(20).unwrap()),
        ));
        board.open_work(&ask, Bound::new(100).unwrap()).unwrap()
    }

    /// The open changes of `world` with their bodies, as rule 6 asks them.
    pub(crate) fn pulls(world: &World) -> Vec<ChangeDigest> {
        let site = FakeSite::new(world);
        let mut host = site
            .code_host(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap();
        let filter = ChangeFilter {
            bodies: true,
            ..Default::default()
        };
        host.changes(&filter, Bound::new(100).unwrap())
            .unwrap()
            .into_items()
    }

    /// The board's answer and the open changes mapped, each mark spelt by the
    /// table.
    pub(crate) fn mapped(work: &OpenWork, pulls: Option<&[ChangeDigest]>) -> Mapped {
        let words = Words {
            repo: REPO,
            mark: &label,
        };
        queue(work, pulls, None, &words)
    }

    /// The open issues of the work, as the queue's page gives them. Every
    /// item maps: a test world holds none the seam refuses.
    pub(crate) fn page_of(work: &OpenWork) -> Vec<Open> {
        let m = mapped(work, None);
        assert!(m.unread.is_empty(), "{:?}", m.unread);
        m.queue.open
    }

    /// The open issues of `world` in the board's order, as the queue's page
    /// gives them.
    pub(crate) fn page(world: &World) -> Vec<Open> {
        page_of(&work(world))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::github;
    use std::num::NonZeroU32;
    use tehas_ports::fake::{FakeSite, World};
    use tehas_ports::{
        Act, Actor, BoardAsk, Bound, Caller, ChangeStates, Depth, HappenedKind, Item, Scope, Site,
        Unread, Window, ALL_MAX,
    };

    const REPO: &str = "acme/widget";

    /// The ask the queue makes of the board: no notes, every mark's events
    /// whole and the newest renames, the changes that close each item, and
    /// the control ticket.
    fn ask(history: Option<Window>) -> BoardAsk {
        let mut ask = BoardAsk::new(Depth::Cut { notes: 0 });
        match history {
            Some(window) => ask.history = Some(window),
            None => {
                ask.history_by_kind = vec![
                    (HappenedKind::Marked, Window::Whole(ALL_MAX)),
                    (HappenedKind::Unmarked, Window::Whole(ALL_MAX)),
                    (HappenedKind::Retitled, Window::Last(NonZeroU32::MIN)),
                ];
            }
        }
        ask.changes = Some((
            ChangeStates::All,
            Window::Last(NonZeroU32::new(20).unwrap()),
        ));
        ask.control = Some(TicketKey::Number(1.try_into().unwrap()));
        ask
    }

    /// The world through the fake's board, mapped, with the fake's spelling
    /// of a mark as the site's.
    fn mapped(world: &World, history: Option<Window>) -> Mapped {
        let site = FakeSite::new(world);
        let mut board = site
            .board(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap();
        let work = board
            .open_work(&ask(history), Bound::new(100).unwrap())
            .unwrap();
        let name = |m: &Mark| site.mark_name(m);
        queue(
            &work,
            None,
            None,
            &Words {
                repo: REPO,
                mark: &name,
            },
        )
    }

    fn numbers(m: &Mapped) -> Vec<u64> {
        m.queue.open.iter().map(|o| o.number).collect()
    }

    /// The blocked world of `read::build`'s poll tests: #21, #22 and #23,
    /// each "issue n" with "a body", filed by alice and blocked by her, and
    /// the control issue #1.
    fn blocked() -> World {
        let world = World::new().ticket(1, |t| t.title("control"));
        [21, 22, 23].into_iter().fold(world, |w, n| {
            w.ticket(n, |t| {
                t.title(&format!("issue {n}"))
                    .text("a body")
                    .author("alice")
                    .act(Act::Block, "alice")
            })
        })
    }

    /// `TicketDigest.labels` is for display alone, never a decision input
    /// (ports Decided 130): this seam is the dispatcher's one reader of a
    /// digest, and `decide` reads only what it maps. A digest whose label
    /// names say what its marks do not maps the same as one with none.
    #[test]
    fn the_label_names_of_a_digest_are_never_read() {
        let world = blocked();
        let site = FakeSite::new(&world);
        let mut board = site
            .board(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap();
        let work = board
            .open_work(&ask(None), Bound::new(100).unwrap())
            .unwrap();
        let name = |m: &Mark| site.mark_name(m);
        let words = Words {
            repo: REPO,
            mark: &name,
        };
        let with_labels = |labels: &[&str]| {
            let items = (work.items.items().iter().cloned())
                .map(|(k, item)| {
                    let item = item.map(|mut i| {
                        i.ticket.digest.labels = labels.iter().map(|l| l.to_string()).collect();
                        i
                    });
                    (k, item)
                })
                .collect();
            let mut w = work.clone();
            w.items = tehas_ports::Whole::new(items);
            if let Some(Ok(t)) = w.control.as_mut() {
                t.digest.labels = labels.iter().map(|l| l.to_string()).collect();
            }
            queue(&w, None, None, &words)
        };
        let page = |m: &Mapped| {
            let control = m.control.as_ref().and_then(|c| c.as_ref().ok());
            (m.queue.open.iter().chain(control))
                .map(|o| (o.number, o.labels.clone(), o.node.clone()))
                .collect::<Vec<_>>()
        };
        let none = with_labels(&[]);
        let named = with_labels(&["act:approve", "state:running", "anything"]);
        assert_eq!(numbers(&none), [1, 21, 22, 23]);
        assert_eq!(page(&named), page(&none));
        assert_eq!(named.unread.len(), none.unread.len());
    }

    #[test]
    fn each_open_ticket_is_an_issue_of_the_page_as_the_snapshot_reads_it() {
        let world = blocked();
        let at = iso(world.now().secs());
        let m = mapped(&world, None);
        assert_eq!(numbers(&m), [1, 21, 22, 23], "the board's order");
        assert!(m.unread.is_empty());
        assert_eq!(m.queue.repo.as_deref(), Some(REPO));
        let o = &m.queue.open[1];
        assert_eq!(o.labels, ["act:block"]);
        let i = github::issue(o);
        assert_eq!(
            i["work"],
            json!({"number": 21, "state": "open", "title": "issue 21", "body": "a body",
                "labels": [{"name": "act:block"}],
                "sub_issues_summary": {"total": 0, "completed": 0},
                "issue_dependencies_summary": {"blocked_by": 0}})
        );
        assert_eq!(
            i["view"],
            json!({"title": "issue 21", "body": "a body", "state": "OPEN",
                "labels": [{"name": "act:block"}]})
        );
        let alice = json!({"login": "alice", "databaseId": "id-alice"});
        assert_eq!(
            i["edit"],
            json!({"lastEditedAt": null, "editor": null, "body": "a body", "title": "issue 21",
                CREATED: at, "author": alice})
        );
        assert_eq!(
            i["events"],
            json!([{"event": "labeled", "label": {"id": "mark-1", "name": "act:block"},
                "actor": {"login": "alice"}, "created_at": at}])
        );
        assert_eq!(i["prs"], json!([]));
        assert!(!github::dropped_events(o, &[]), "the guard vouches for it");
        assert!(github::has_event_of(o, "act:block"));
    }

    #[test]
    fn the_control_ticket_is_mapped_alone() {
        let m = mapped(&blocked(), None);
        let control = m.control.unwrap().unwrap();
        assert_eq!(control.number, 1);
        assert_eq!(github::issue(&control)["work"]["title"], "control");
    }

    #[test]
    fn a_history_the_port_may_have_cut_is_one_the_guard_reads_again() {
        let world = blocked().ticket(21, |t| t.act(Act::Approve, "alice"));
        let m = mapped(&world, Some(Window::Last(NonZeroU32::MIN)));
        let o = &m.queue.open[1];
        assert_eq!(o.labels, ["act:block", "act:approve"]);
        assert!(o.node.get(TIMELINE).is_none());
        assert!(github::dropped_events(o, &[]));
    }

    #[test]
    fn a_ticket_the_seam_cannot_give_holds_and_is_never_an_issue() {
        // A split world's keys are no numbers.
        let split = World::split().ticket(3, |t| t.title("keyed"));
        let m = mapped(&split, None);
        assert!(m.queue.open.is_empty());
        let unread: Vec<_> = m.unread.iter().map(|(_, why)| why.clone()).collect();
        assert_eq!(unread, [UNSUPPORTED]);
        // A mark the site cannot name.
        let world = blocked().ticket(22, |t| t.mark(Mark::Unknown));
        let m = mapped(&world, None);
        assert_eq!(numbers(&m), [1, 21, 23]);
        assert_eq!(m.unread.len(), 1);
        assert_eq!(m.unread[0].0, TicketKey::Number(22.try_into().unwrap()));
    }

    #[test]
    fn a_failed_item_is_unread_with_its_outcome() {
        let mut work = OpenWork::default();
        let key = TicketKey::Number(9.try_into().unwrap());
        let cut = Outcome::Unread(Unread::Cut);
        work.items = tehas_ports::Whole::new(vec![(key.clone(), Err(cut.clone()))]);
        let name = |_: &Mark| String::new();
        let m = queue(
            &work,
            None,
            None,
            &Words {
                repo: REPO,
                mark: &name,
            },
        );
        assert!(m.queue.open.is_empty());
        assert_eq!(m.unread, [(key, cut)]);
        assert!(m.control.is_none());
    }

    #[test]
    fn parts_blockers_parent_and_type_are_the_page_s_summaries() {
        let world = World::new()
            .ticket(11, |t| t.title("open blocker"))
            .ticket(12, |t| t.closed(tehas_ports::CloseReason::Done))
            .ticket(13, |t| t)
            .ticket(14, |t| t.closed(tehas_ports::CloseReason::Done))
            .ticket(7, |t| {
                t.blocker(Item::Ticket(TicketKey::Number(11.try_into().unwrap())))
                    .blocker(Item::Ticket(TicketKey::Number(12.try_into().unwrap())))
                    .part(TicketKey::Number(13.try_into().unwrap()))
                    .part(TicketKey::Number(14.try_into().unwrap()))
                    .kind(Kind::named("Improvement"))
            });
        let m = mapped(&world, None);
        let o = m.queue.open.iter().find(|o| o.number == 7).unwrap();
        let work = &github::issue(o)["work"];
        assert_eq!(work["issue_dependencies_summary"], json!({"blocked_by": 1}));
        assert_eq!(
            work["sub_issues_summary"],
            json!({"total": 2, "completed": 1})
        );
        assert_eq!(
            o.node.pointer("/issueType/name"),
            Some(&json!("Improvement"))
        );
        let part = m.queue.open.iter().find(|o| o.number == 13).unwrap();
        assert_eq!(github::issue(part)["work"]["parent"], json!(7));
    }

    #[test]
    fn an_edited_body_is_the_edit_record_of_the_same_item() {
        let world = blocked();
        world.advance(60);
        world.edit_elsewhere(&world.key(21), "a new body", "bob");
        let at = iso(world.now().secs());
        let m = mapped(&world, None);
        let edit = &github::issue(&m.queue.open[1])["edit"];
        assert_eq!(edit["body"], "a new body");
        assert_eq!(edit["lastEditedAt"], json!(at));
        assert_eq!(edit["editor"]["login"], "bob");
    }

    #[test]
    fn the_changes_that_close_an_issue_are_its_pull_requests_and_own_pull_reads_them() {
        let world = blocked()
            .change(30, "work-21", |c| c.closes(21).author("tehas-bot"))
            .change(31, "other-21", |c| {
                c.closes(21)
                    .author("mallory")
                    .state(ChangeState::Closed { at: At::new(1) })
            });
        let m = mapped(&world, None);
        let prs = github::issue(&m.queue.open[1])["prs"].clone();
        assert_eq!(
            prs[0],
            json!({"url": "https://fake.test/fake/project/c/30", "state": "OPEN",
                "isCrossRepository": false,
                "author": {"login": "tehas-bot", "databaseId": "id-tehas-bot"},
                "headRepository": {"nameWithOwner": REPO}})
        );
        assert_eq!(prs[1]["state"], "CLOSED");
        let own = |p: &serde_json::Value| {
            tehas_core::accounts::own_pull(p, REPO, "tehas-bot", |a| a == "alice")
        };
        assert!(own(&prs[0]));
        assert!(!own(&prs[1]), "another author's stays out, in dispatch");
    }

    #[test]
    fn the_open_pull_requests_are_rule_six_s_nodes_and_a_fork_s_head_is_not_ours() {
        let world = blocked().change(30, "work", |c| c.body("Fixes #21").author("tehas-bot"));
        let site = FakeSite::new(&world);
        let mut host = site
            .code_host(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap();
        let filter = tehas_ports::ChangeFilter {
            bodies: true,
            ..Default::default()
        };
        let mut pulls = host
            .changes(&filter, Bound::new(100).unwrap())
            .unwrap()
            .into_items();
        let mut fork = pulls[0].clone();
        fork.from_fork = true;
        pulls.push(fork);
        let name = |_: &Mark| String::new();
        let words = Words {
            repo: REPO,
            mark: &name,
        };
        let m = queue(&OpenWork::default(), Some(&pulls), None, &words);
        let nodes = m.queue.open_pulls.unwrap();
        assert_eq!(
            nodes[0],
            json!({"number": 30, "body": "Fixes #21", "isCrossRepository": false,
                "author": {"login": "tehas-bot", "databaseId": "id-tehas-bot"},
                "headRepository": {"nameWithOwner": REPO}})
        );
        assert_eq!(nodes[1]["headRepository"]["nameWithOwner"], json!(null));
        assert!(queue(&OpenWork::default(), None, None, &words)
            .queue
            .open_pulls
            .is_none());
    }

    #[test]
    fn the_owner_s_types_are_named_as_the_site_writes_them() {
        let name = |_: &Mark| String::new();
        let words = Words {
            repo: REPO,
            mark: &name,
        };
        let kinds = [Kind::Task, Kind::named("Improvement"), Kind::Unknown];
        let m = queue(&OpenWork::default(), None, Some(&kinds), &words);
        assert_eq!(m.queue.issue_types.unwrap(), ["Task", "Improvement"]);
    }
}
