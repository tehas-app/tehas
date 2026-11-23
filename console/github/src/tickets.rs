//! The tickets port's marks and history, answered over the forge's reads in
//! the ports' types. The rest of the tickets port comes with the moves that
//! need it.

use crate::{gh, table, GitHub};
use tehas_core::forge::ForgeError;
use tehas_ports::{
    Account, AccountId, AccountKind, At, Got, Happened, HappenedKind, Login, Mark, MarkName,
    Outcome, Passing, Pool, Realm, Refusal, Sent, TicketEvent, TicketKey, Took, Unread, Window,
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

    /// A ticket's marks going on and off, oldest first, as `Tickets::history`
    /// answers for the kinds `Marked` and `Unmarked`. Any other kind, or
    /// every kind (`only` None), is `Refused(Unsupported)` before any
    /// request: this read holds the label events alone.
    pub fn history(
        &mut self,
        key: &TicketKey,
        window: Window,
        only: Option<&[HappenedKind]>,
    ) -> Got<Took<TicketEvent>> {
        let issue = number(key)?;
        let marks = [HappenedKind::Marked, HappenedKind::Unmarked];
        let only = only
            .filter(|kinds| kinds.iter().all(|k| marks.contains(k)))
            .ok_or(Outcome::Refused(Refusal::Unsupported))?;
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
            items.push(TicketEvent {
                what,
                by: account(&e.actor, &facts),
                at,
            });
        }
        take(items, window)
    }
}

impl GitHub {
    /// The mark of each label, in order: a word of the table, or the
    /// project's own mark, read with `MarkName::read_list`. None for a name
    /// no mark can have, which is kept once for the caller to report.
    fn read_marks(&mut self, labels: &[String]) -> Vec<Option<Mark>> {
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
fn number(key: &TicketKey) -> Got<u64> {
    match key {
        TicketKey::Number(n) => Ok(n.get()),
        TicketKey::Key(_) => Err(Outcome::Refused(Refusal::Invalid)),
    }
}

/// The account of an event, None where it names nobody. The login is folded
/// to lower case, as GitHub compares logins.
fn account(login: &str, facts: &gh::Facts) -> Option<Account> {
    if login.is_empty() {
        return None;
    }
    Some(Account {
        realm: Realm::CodeHost,
        login: Login::new(login.to_ascii_lowercase()),
        id: facts.id.map(|id| AccountId::new(id.to_string())),
        kind: match facts.kind.as_deref() {
            Some("User") => AccountKind::Person,
            Some("Bot") => AccountKind::Bot,
            _ => AccountKind::Unknown,
        },
    })
}

/// What a window takes of a list, oldest first.
fn take<T>(mut items: Vec<T>, window: Window) -> Got<Took<T>> {
    match window {
        Window::Whole(bound) if items.len() > bound.get() as usize => {
            Err(Outcome::Unread(Unread::Cut))
        }
        Window::Whole(_) => Ok(Took { items, more: false }),
        Window::Last(n) => {
            let skip = items.len().saturating_sub(n.get() as usize);
            Ok(Took {
                more: skip > 0,
                items: items.split_off(skip),
            })
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
