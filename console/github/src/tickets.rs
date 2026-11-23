//! The tickets port's digest, marks and history, answered over the forge's
//! reads in the ports' types. The traits are answered in `port/`; the rest
//! of the tickets port comes with the moves that need it.

use crate::{gh, table, ForgeError, GitHub};
use serde_json::Value;
use tehas_ports::{
    Account, AccountId, AccountKind, At, CloseReason, Got, Happened, HappenedKind, Kind, Login,
    Mark, MarkName, Outcome, Passing, Pool, Realm, Refusal, Revision, Sent, TargetId, TargetName,
    TicketDigest, TicketEvent, TicketKey, TicketState, Took, Unread, WebLink, Window,
};

impl GitHub {
    /// The marks a ticket carries now, in the site's order: the
    /// `digest.marks` of `Tickets::ticket`. A name no mark can have is left
    /// out, and kept for [`GitHub::take_left_out`].
    pub fn marks(&mut self, key: &TicketKey) -> Got<Vec<Mark>> {
        let issue = number(key)?;
        let out = self.issue_answer(issue).map_err(|e| self.outcome(&e))?;
        let labels = gh::parse_labels(&out).ok_or(Outcome::Unread(Unread::Malformed))?;
        Ok(self.read_marks(&labels).into_iter().flatten().collect())
    }

    /// A ticket's digest, from the issue's REST read: the answer of
    /// `Tickets::several` for one key. GitHub has no priority: every ticket
    /// is `Normal`. The revision is the time GitHub last saw it change.
    pub(crate) fn digest(&mut self, key: &TicketKey) -> Got<TicketDigest> {
        let issue = number(key)?;
        let out = self.issue_answer(issue).map_err(|e| self.outcome(&e))?;
        self.digest_of(key, &out)
    }

    /// The digest of an issue's REST read. A pull request answers that read
    /// too, and is no ticket: `NotFound`.
    pub(crate) fn digest_of(&mut self, key: &TicketKey, out: &str) -> Got<TicketDigest> {
        if gh::is_pull(out) {
            return Err(Outcome::NotFound);
        }
        let labels = gh::parse_labels(out).ok_or(Outcome::Unread(Unread::Malformed))?;
        let marks = self.read_marks(&labels).into_iter().flatten().collect();
        read_digest(out, key.clone(), marks).ok_or(Outcome::Unread(Unread::Malformed))
    }

    /// A ticket's history, oldest first, as `Tickets::history` answers. The
    /// kinds `Marked` and `Unmarked` alone are the label events' REST read;
    /// any other kind, or every kind (`only` None), is the issue's timeline
    /// (`ticket.rs`).
    pub fn history(
        &mut self,
        key: &TicketKey,
        window: Window,
        only: Option<&[HappenedKind]>,
    ) -> Got<Took<TicketEvent>> {
        let issue = number(key)?;
        let marks = [HappenedKind::Marked, HappenedKind::Unmarked];
        let only = match only {
            Some(kinds) if kinds.iter().all(|k| marks.contains(k)) => kinds,
            _ => return self.timeline(key, window, only),
        };
        let out = self.event_pages(issue).map_err(|e| self.outcome(&e))?;
        let mut read =
            gh::parse_with_facts(&out).map_err(|_| Outcome::Unread(Unread::Malformed))?;
        read.retain(|(e, _)| e.event == "labeled" || e.event == "unlabeled");
        read.sort_by(|a, b| gh::event_order(&&a.0, &&b.0));
        let labels: Vec<String> = read.iter().map(|(e, _)| e.label.clone()).collect();
        let marks = self.read_marks(&labels);
        let mut items = Vec::new();
        for ((e, facts), mark) in read.into_iter().zip(marks) {
            let Some(mark) = mark else {
                continue;
            };
            let what = if e.event == "labeled" {
                Happened::Marked(mark)
            } else {
                Happened::Unmarked(mark)
            };
            if !only.contains(&what.kind()) {
                continue;
            }
            let at = match &e.at {
                None => None,
                Some(t) => Some(At::new(
                    tehas_core::time::from_iso(t).ok_or(Outcome::Unread(Unread::Malformed))?,
                )),
            };
            let mut event = TicketEvent::new(what);
            event.by = account(&e.actor, &facts);
            event.at = at;
            items.push(event);
        }
        take(items, window)
    }
}

impl GitHub {
    /// The mark of each label, in order: a word of the table, or the
    /// project's own mark, read with `MarkName::read_list`. None for a name
    /// no mark can have, which is kept once for the caller to report.
    pub(crate) fn read_marks(&mut self, labels: &[String]) -> Vec<Option<Mark>> {
        let theirs = labels.iter().filter(|l| table::word(l).is_none()).cloned();
        let read = MarkName::read_list(theirs);
        for name in read.left_out {
            if !self.left_out.contains(&name) {
                self.left_out.push(name);
            }
        }
        labels
            .iter()
            .map(|l| {
                table::word(l).or_else(|| {
                    read.kept
                        .iter()
                        .find(|k| k.as_str() == l)
                        .cloned()
                        .map(Mark::Project)
                })
            })
            .collect()
    }

    /// The named outcome of a call that failed, read from the runner's
    /// reason. gh's reason holds no time a spent budget resets at, so
    /// `Spent` is until now, by the adapter's clock, and the caller waits by
    /// its own rule.
    pub(crate) fn outcome(&self, e: &ForgeError) -> Outcome {
        outcome(e, (self.now)())
    }
}

/// The issue a ticket is. A GitHub ticket is always its number: a key is no
/// ticket here.
pub(crate) fn number(key: &TicketKey) -> Got<u64> {
    match key {
        TicketKey::Number(n) => Ok(n.get()),
        TicketKey::Key(_) => Err(Outcome::Refused(Refusal::Invalid)),
    }
}

/// The digest of an issue's REST read, its marks read already. None for an
/// answer that lacks a field every issue has.
fn read_digest(out: &str, key: TicketKey, marks: Vec<Mark>) -> Option<TicketDigest> {
    let mut v: Value = serde_json::from_str(out.trim()).ok()?;
    tehas_core::accounts::vouch(&mut v);
    let text = |k: &str| v.get(k).and_then(Value::as_str);
    let time = |k: &str| text(k).and_then(tehas_core::time::from_iso).map(At::new);
    let state = match (text("state")?, text("state_reason")) {
        ("open", _) => TicketState::Open,
        // The REST read names no original of a duplicate: a reader that
        // needs it reads the timeline.
        ("closed", Some("not_planned" | "duplicate")) => {
            TicketState::Closed(CloseReason::NotPlanned)
        }
        ("closed", _) => TicketState::Closed(CloseReason::Done),
        _ => return None,
    };
    let kind = v.pointer("/type/name").and_then(Value::as_str).map(kind_of);
    let target = match v.get("milestone").filter(|m| !m.is_null()) {
        None => None,
        Some(m) => Some(TargetName::new(
            TargetId::new(m.get("number")?.as_u64()?.to_string()),
            m.get("title")?.as_str()?.to_string(),
        )),
    };
    let author = v.get("user").and_then(|u| {
        let facts = gh::Facts {
            id: u.get("id").and_then(Value::as_u64),
            kind: u.get("type").and_then(Value::as_str).map(str::to_string),
        };
        account(u.get("login")?.as_str()?, &facts)
    });
    let mut digest = TicketDigest::new(
        key,
        state,
        time("created_at")?,
        time("updated_at")?,
        Revision::new(text("updated_at")?),
        WebLink::new(text("html_url")?).ok()?,
    );
    digest.title = text("title")?.to_string();
    digest.kind = kind;
    digest.marks = marks;
    digest.target = target;
    digest.author = author;
    Some(digest)
}

/// The kind an issue type of GitHub's is, by its name.
pub(crate) fn kind_of(name: &str) -> Kind {
    match name {
        "Feature" => Kind::Feature,
        "Bug" => Kind::Bug,
        "Task" => Kind::Task,
        "Epic" => Kind::Epic,
        other => MarkName::new(other).map_or(Kind::Unknown, Kind::Other),
    }
}

/// The account of an event, None where it names nobody. The login is folded
/// to lower case, as GitHub compares logins, and an app's spellings
/// (`x[bot]` in REST, `x` in GraphQL, `app/x`) to one, as the spec's `Login`
/// says.
pub(crate) fn account(login: &str, facts: &gh::Facts) -> Option<Account> {
    if login.is_empty() {
        return None;
    }
    let kind = match facts.kind.as_deref() {
        Some("User") => AccountKind::Person,
        Some("Bot") => AccountKind::Bot,
        _ => AccountKind::Unknown,
    };
    let login = Login::new(tehas_core::accounts::folded(tehas_core::accounts::account(
        login,
    )));
    let mut account = Account::new(Realm::CodeHost, login, kind);
    account.id = facts.id.map(|id| AccountId::new(id.to_string()));
    Some(account)
}

/// What a window takes of a list, oldest first.
pub(crate) fn take<T>(mut items: Vec<T>, window: Window) -> Got<Took<T>> {
    match window {
        Window::Whole(bound) if items.len() > bound.get() as usize => {
            Err(Outcome::Unread(Unread::Cut))
        }
        Window::Whole(_) => {
            let mut took = Took::default();
            took.items = items;
            Ok(took)
        }
        Window::Last(n) => {
            let skip = items.len().saturating_sub(n.get() as usize);
            let mut took = Took::default();
            took.items = items.split_off(skip);
            took.more = skip > 0;
            Ok(took)
        }
    }
}

/// [`GitHub::outcome`] at the time `now`. A 410 is an issue deleted or
/// issues turned off: `NotFound`, as the ports spec's table says.
fn outcome(e: &ForgeError, now: i64) -> Outcome {
    if gh::says_not_found(&e.why) {
        return Outcome::NotFound;
    }
    let why = e.why.trim_start_matches(": ");
    let maybe = |why| Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    };
    match why.strip_prefix("HTTP ").and_then(|s| s.get(..3)) {
        Some("401") => Outcome::SignedOut,
        Some("410") => Outcome::NotFound,
        Some("429") => Outcome::Spent {
            pool: Pool::Calls,
            until: At::new(now),
        },
        Some("403") => Outcome::Forbidden,
        Some(s) if s.starts_with('5') => maybe(Passing::Server),
        _ if why.contains("rate limit") => Outcome::Spent {
            pool: Pool::Calls,
            until: At::new(now),
        },
        _ if why.contains("timed out") || why.contains("timeout") || why.contains("bound") => {
            maybe(Passing::Timeout)
        }
        _ if why.contains("connection refused") || why.contains("no such host") => {
            Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No,
            }
        }
        _ if why.contains("connection reset") || why.contains("unexpected EOF") => {
            maybe(Passing::Unreachable)
        }
        _ if why.contains("could not be run") => Outcome::Unavailable {
            why: Passing::Unreachable,
            sent: Sent::No,
        },
        _ => maybe(Passing::Server),
    }
}
