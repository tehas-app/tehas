//! The tickets port's digest, marks and history, answered over the forge's
//! reads in the ports' types. The traits are answered in `port/`; the rest
//! of the tickets port comes with the moves that need it.

use crate::{gh, table, ForgeError, GitHub};
use serde_json::Value;
use tehas_ports::{
    Account, AccountId, AccountKind, At, CloseReason, Got, Happened, HappenedKind, Kind, Login,
    Mark, MarkId, MarkName, Outcome, Passing, Pool, Realm, Refusal, Revision, Sent, TargetId,
    TargetName, TicketDigest, TicketEvent, TicketKey, TicketState, Took, Unread, WebLink, Window,
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

    /// A ticket's digest, from the issue's REST read and one question for
    /// the ticket it is a part of (`GitHub::parent`): the answer of
    /// `Tickets::several` for one key. GitHub has no priority: every ticket
    /// is `Normal`. The revision is the sum of the title and the body as
    /// GitHub holds them, markers in (`gh::text_revision`), the one rule of
    /// both ports' revisions; `updated` is the time it last changed. An
    /// accepted edge: the title's sum drops its CRs and LFs, so a title
    /// changed by those alone keeps its revision.
    pub(crate) fn digest(&mut self, key: &TicketKey) -> Got<TicketDigest> {
        let issue = number(key)?;
        let out = self.issue_answer(issue).map_err(|e| self.outcome(&e))?;
        let mut digest = self.digest_of(key, &out)?;
        digest.parent = self.parent(key)?;
        Ok(digest)
    }

    /// The digest of an issue's REST read. A pull request answers that read
    /// too, and is no ticket: `NotFound`.
    pub(crate) fn digest_of(&mut self, key: &TicketKey, out: &str) -> Got<TicketDigest> {
        if gh::is_pull(out) {
            return Err(Outcome::NotFound);
        }
        let labels = gh::parse_labels(out).ok_or(Outcome::Unread(Unread::Malformed))?;
        let marks = self.read_marks(&labels).into_iter().flatten().collect();
        let mut digest = read_digest(out, key.clone(), marks, self.bot.as_ref())
            .ok_or(Outcome::Unread(Unread::Malformed))?;
        digest.labels = labels;
        Ok(digest)
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
        take(self.mark_events(issue, only)?, window)
    }
}

impl GitHub {
    /// Every event of the kinds `only` names, `Marked` and `Unmarked`
    /// among them, on the issue, oldest first, from the label events' REST
    /// read, whole.
    pub(crate) fn mark_events(
        &mut self,
        issue: u64,
        only: &[HappenedKind],
    ) -> Got<Vec<TicketEvent>> {
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
            event.by = account(&e.actor, &facts, self.bot.as_ref());
            event.at = at;
            event.mark_id = e.label_id.map(MarkId::new);
            items.push(event);
        }
        Ok(items)
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
fn read_digest(
    out: &str,
    key: TicketKey,
    marks: Vec<Mark>,
    bot: Option<&Bot>,
) -> Option<TicketDigest> {
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
    let kind = v
        .pointer("/type/name")
        .and_then(Value::as_str)
        .map(Kind::named);
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
        account(u.get("login")?.as_str()?, &facts, bot)
    });
    let mut digest = TicketDigest::new(
        key,
        state,
        time("created_at")?,
        time("updated_at")?,
        Revision::new(gh::text_revision(out)?),
        WebLink::new(text("html_url")?).ok()?,
    );
    digest.title = text("title")?.to_string();
    digest.kind = kind;
    digest.marks = marks;
    digest.target = target;
    digest.author = author;
    Some(digest)
}

/// The factory's own account, as a site gives it to the handles it makes:
/// the user id of its bot account. A handle made without a site, or by a
/// site given no id or a login that names nobody, answers no account
/// `Ours`.
#[derive(Clone, Debug)]
pub(crate) struct Bot {
    user_id: Option<u64>,
}

impl Bot {
    /// The bot of `login`, as the caller read it, and its account's user
    /// id. The login is trimmed, so a line read from a file names it; a
    /// login that is then empty, holds whitespace, or is only the App's
    /// marks names nobody, and the bot matches no account. GitHub gives no
    /// account the id 0: given it, the bot has no id and matches none.
    pub(crate) fn new(login: &str, user_id: Option<u64>) -> Bot {
        let login = login.trim();
        let named = !login.is_empty()
            && !login.contains(char::is_whitespace)
            && !matches!(tehas_core::accounts::account(login), "" | "[bot]" | "app/");
        Bot {
            user_id: user_id.filter(|id| named && *id > 0),
        }
    }

    /// Whether the account of `facts` is this bot: GitHub types it a bot
    /// and it carries the bot's user id. Nothing else decides: not the
    /// login, which a renamed App changes and another account may hold.
    /// With no id known, no account is the bot.
    pub(crate) fn is(&self, facts: &gh::Facts) -> bool {
        self.user_id.is_some_and(|ours| facts.id == Some(ours))
            && facts.kind.as_deref() == Some("Bot")
    }
}

/// The account of an event, None where it names nobody. The login is folded
/// to lower case, as GitHub compares logins, and an app's spellings
/// (`x[bot]` in REST, `x` in GraphQL, `app/x`) to one, as the spec's `Login`
/// says. It is `Ours` where it is the site's `bot`.
pub(crate) fn account(login: &str, facts: &gh::Facts, bot: Option<&Bot>) -> Option<Account> {
    if login.is_empty() {
        return None;
    }
    let kind = match facts.kind.as_deref() {
        _ if bot.is_some_and(|b| b.is(facts)) => AccountKind::Ours,
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

/// The reason a runner gives when its time ran out before it started gh,
/// exactly: a runner's own words are never read.
pub(crate) const NOT_STARTED: &str = "not started: out of time";

/// The reason every runner gives for a gh that exited 4, gh's documented
/// "authentication required": no credential for the host, and no request
/// made. `Bounded` says `: gh exited 4`, the relay's runner `gh exited 4`.
const GH_NO_CREDENTIAL: &str = "gh exited 4";

/// [`GitHub::outcome`] at the time `now`. A 410 is an issue deleted or
/// issues turned off: `NotFound`, as the ports spec's table says. The
/// status is found anywhere in the reason: `Bounded` gives `: HTTP 403`,
/// the relay's runner `gh exited 1: HTTP 403`, and its `gh exited 124` is a
/// gh it stopped at its bound.
///
/// [`NOT_STARTED`] is the one form any runner returns when it gives up
/// before it starts gh because its time is up: nothing was sent.
fn outcome(e: &ForgeError, now: i64) -> Outcome {
    if e.why == NOT_STARTED {
        return Outcome::Unavailable {
            why: Passing::Timeout,
            sent: Sent::No,
        };
    }
    if gh::says_not_found(&e.why) {
        return Outcome::NotFound;
    }
    let why = e.why.trim_start_matches(": ");
    // gh's exit 4 is its "authentication required": it holds no credential
    // for the host and asked nothing. `SignedOut`, so a caller knows nothing
    // was taken, never `Unavailable { sent: Maybe }` (tehas-68, 2026-11-25).
    if why == GH_NO_CREDENTIAL {
        return Outcome::SignedOut;
    }
    let maybe = |why| Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    };
    let status = why.match_indices("HTTP ").find_map(|(i, _)| {
        why.get(i + 5..i + 8)
            .filter(|d| d.bytes().all(|b| b.is_ascii_digit()))
    });
    match status {
        Some("401") => Outcome::SignedOut,
        Some("410") => Outcome::NotFound,
        Some("429") => Outcome::Spent {
            pool: Pool::Calls,
            until: At::new(now),
        },
        Some("403") => Outcome::Forbidden,
        // GitHub refusing the input: nothing was made.
        Some("422") => Outcome::Refused(Refusal::Invalid),
        Some(s) if s.starts_with('5') => maybe(Passing::Server),
        _ if why.contains("rate limit") => Outcome::Spent {
            pool: Pool::Calls,
            until: At::new(now),
        },
        _ if why.contains("timed out")
            || why.contains("timeout")
            || why.contains("bound")
            || e.why.starts_with("gh exited 124") =>
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn of(why: &str) -> Outcome {
        outcome(&ForgeError::new("gh api", why), 0)
    }

    #[test]
    fn a_gh_with_no_credential_is_signed_out() {
        // gh exits 4 with "To get started with GitHub CLI, please run: gh
        // auth login" and asks nothing: both runners' forms of it.
        assert_eq!(of(": gh exited 4"), Outcome::SignedOut);
        assert_eq!(of("gh exited 4"), Outcome::SignedOut);
    }

    #[test]
    fn a_401_is_still_signed_out() {
        assert_eq!(of(": HTTP 401"), Outcome::SignedOut);
        assert_eq!(of("gh exited 1: HTTP 401"), Outcome::SignedOut);
    }

    #[test]
    fn another_exit_stays_a_server_failure_that_may_have_been_sent() {
        let maybe = Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::Maybe,
        };
        for why in [": gh exited 1", "gh exited 1", ": gh exited 40"] {
            assert_eq!(of(why), maybe, "{why}");
        }
    }
}
