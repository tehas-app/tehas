//! The tickets port, `Tickets` and `TicketWrites`, answered by [`GitHub`]
//! over the forge's terms. What the port's callers ask today is answered:
//! a ticket's digest (`several`), its marks going on and off (`history`),
//! the factory's labels made (`prepare`), a project's own label
//! (`ensure_mark`), and a mark put on or taken off. Every other method
//! answers `Refused(Unsupported)` before any request, until the step of the
//! ports' order that moves its first caller.

use std::num::{NonZeroU32, NonZeroU64};

use crate::forge::uri;
use crate::{table, GitHub};
use tehas_core::forge::Forge;
use tehas_ports::{
    Applied, At, Blocker, Bound, ChangeToken, Cite, Cites, CloseReason, ClosedTicket, Comment,
    CommentId, Cursor, Depth, Did, Edit, Filed, Filter, Got, HappenedKind, Item, Kind, Mark,
    MarkSpec, MarkerEdit, MarkerKey, MarkerKind, NewTicket, Outcome, Part, Posted, Priority, Ref,
    Refusal, Report, Reports, Revision, Step, StepPart, Target, TargetEdit, TargetId, Ticket,
    TicketDelta, TicketDigest, TicketEvent, TicketKey, TicketWrites, Tickets, Took, WebLink, Whole,
    Window, Within,
};

/// The answer of a method no caller asks of this adapter yet.
fn unsupported<T>() -> Got<T> {
    Err(Outcome::Refused(Refusal::Unsupported))
}

/// The number of a ticket that keeps its rule, or `Refused(Invalid)`.
fn number(key: &TicketKey) -> Got<u64> {
    match key {
        TicketKey::Number(n) => Ok(n.get()),
        TicketKey::Key(_) => Err(Outcome::Refused(Refusal::Invalid)),
    }
}

/// The label of a mark this adapter writes, or `Refused(Invalid)` for one
/// no label can carry.
fn label_of(mark: &Mark) -> Got<String> {
    table::label(mark)
        .filter(|l| table::is_label(l))
        .ok_or(Outcome::Refused(Refusal::Invalid))
}

impl Reports for GitHub {
    /// The adapter keeps no report of its own yet: a failed call answers its
    /// outcome to the caller.
    fn take_reports(&mut self) -> Vec<Report> {
        Vec::new()
    }
}

impl Cites for GitHub {
    fn cite(&self, what: &Cite) -> String {
        fn named<T>(r: &Ref<T>, here: impl Fn(&T) -> String) -> String {
            match r {
                Ref::Here(t) => format!("#{}", here(t)),
                Ref::Elsewhere { project, item } => {
                    format!("{}#{}", project.as_str(), item.as_str())
                }
            }
        }
        match what {
            Cite::Ticket(r) => named(r, |k| match k {
                TicketKey::Number(n) => n.get().to_string(),
                TicketKey::Key(k) => k.as_str().to_string(),
            }),
            Cite::Blueprint(r) => named(r, |n| n.get().to_string()),
            Cite::Change(r) => named(r, |n| n.get().to_string()),
            Cite::Account(login) => format!("@{}", login.as_str()),
        }
    }
}

impl Tickets for GitHub {
    fn ticket(&mut self, _: &TicketKey, _: Depth, _: Option<Window>) -> Got<Ticket> {
        unsupported()
    }

    fn what_is(&mut self, _: NonZeroU64) -> Got<Item> {
        unsupported()
    }

    fn what_are(&mut self, numbers: &[NonZeroU64]) -> Vec<(NonZeroU64, Got<Item>)> {
        numbers.iter().map(|n| (*n, unsupported())).collect()
    }

    fn list(&mut self, _: &Filter, _: Bound) -> Got<Whole<TicketDigest>> {
        unsupported()
    }

    /// One REST read of each issue, in the order asked.
    fn several(&mut self, keys: &[TicketKey]) -> Vec<(TicketKey, Got<TicketDigest>)> {
        keys.iter().map(|k| (k.clone(), self.digest(k))).collect()
    }

    fn history(
        &mut self,
        key: &TicketKey,
        window: Window,
        only: Option<&[HappenedKind]>,
    ) -> Got<Took<TicketEvent>> {
        GitHub::history(self, key, window, only)
    }

    fn notes(&mut self, _: &TicketKey, _: Window) -> Got<Took<Comment>> {
        unsupported()
    }

    fn note_edits(&mut self, _: &CommentId, _: Bound) -> Got<Whole<Edit>> {
        unsupported()
    }

    fn blockers(&mut self, _: &TicketKey) -> Got<Whole<Blocker>> {
        unsupported()
    }

    fn parts(&mut self, _: &TicketKey) -> Got<Whole<Part>> {
        unsupported()
    }

    fn closed_since(&mut self, _: At, _: Bound) -> Got<Whole<ClosedTicket>> {
        unsupported()
    }

    fn search(
        &mut self,
        _: &str,
        _: Within,
        _: Option<bool>,
        _: NonZeroU32,
    ) -> Got<Took<TicketDigest>> {
        unsupported()
    }

    fn filed_by(&mut self, _: &tehas_ports::AccountId, _: At, _: Bound) -> Got<Whole<Filed>> {
        unsupported()
    }

    fn find_marked(
        &mut self,
        _: &Mark,
        _: &MarkerKind,
        _: &MarkerKey,
        _: Option<bool>,
    ) -> Got<Whole<TicketKey>> {
        unsupported()
    }

    fn kinds(&mut self) -> Got<Whole<Kind>> {
        unsupported()
    }

    fn targets(&mut self, _: Bound) -> Got<Whole<Target>> {
        unsupported()
    }

    fn members(&mut self, _: &TargetId, _: Bound) -> Got<Whole<TicketDigest>> {
        unsupported()
    }

    fn changed(&mut self, _: Option<&ChangeToken>) -> Got<(bool, ChangeToken)> {
        unsupported()
    }

    fn delta(&mut self, _: Option<&Cursor>, _: Bound) -> Got<TicketDelta> {
        unsupported()
    }

    /// GitHub's page for a new issue, with the title and text filled in. A
    /// link too long for a `WebLink` leaves the text out, then the title.
    fn new_ticket_link(&self, title: &str, text: &str) -> WebLink {
        let page = format!("https://github.com/{}/issues/new", self.repo);
        [
            format!("{page}?title={}&body={}", uri(title), uri(text)),
            format!("{page}?title={}", uri(title)),
            page,
        ]
        .into_iter()
        .find_map(|l| WebLink::new(l).ok())
        .unwrap_or_else(|| WebLink::new("https://github.com").expect("a web link"))
    }
}

impl TicketWrites for GitHub {
    fn apply(
        &mut self,
        _: &TicketKey,
        step: &Step,
    ) -> (Applied<StepPart>, Option<Posted<CommentId>>) {
        let first = if step.note.is_some() || !step.markers.is_empty() {
            Some(StepPart::Note)
        } else if let Some(m) = step.unmark.first() {
            Some(StepPart::Unmark(m.clone()))
        } else if let Some(m) = step.mark.first() {
            Some(StepPart::Mark(m.clone()))
        } else if step.close.is_some() {
            Some(StepPart::Close)
        } else {
            None
        };
        let applied = Applied {
            done: Vec::new(),
            stopped: first.map(|p| (p, Outcome::Refused(Refusal::Unsupported))),
        };
        (applied, None)
    }

    /// Each label of the factory's table, made or brought back to its colour
    /// and text, one call each. A label that cannot be made stops none of
    /// the others; the first that failed is the answer.
    fn prepare(&mut self) -> Did {
        let mut first = Ok(());
        for spec in table::specs() {
            let made = self.ensure_mark(&spec);
            if first.is_ok() {
                first = made;
            }
        }
        first
    }

    /// `gh label create --force`: made, or its colour and text put back.
    fn ensure_mark(&mut self, spec: &MarkSpec) -> Did {
        let name = spec.name.as_str();
        if !table::is_label(name) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
        let repo = self.repo.clone();
        let mut args = vec!["label", "create", name, "--repo", &repo];
        if let Some(c) = &spec.colour {
            args.extend(["--color", c.as_str()]);
        }
        if let Some(about) = &spec.about {
            args.extend(["--description", about]);
        }
        args.push("--force");
        self.call("the label", &args)
            .map(|_| ())
            .map_err(|e| self.outcome(&e))
    }

    fn mark(&mut self, key: &TicketKey, mark: &Mark) -> Did {
        let (issue, label) = (number(key)?, label_of(mark)?);
        self.add_label(issue, &label).map_err(|e| self.outcome(&e))
    }

    fn unmark(&mut self, key: &TicketKey, mark: &Mark) -> Did {
        let (issue, label) = (number(key)?, label_of(mark)?);
        self.remove_label(issue, &label)
            .map_err(|e| self.outcome(&e))
    }

    fn close(&mut self, _: &TicketKey, _: &CloseReason) -> Did {
        unsupported()
    }

    fn note(
        &mut self,
        _: &TicketKey,
        _: &str,
        _: &[tehas_ports::Marker],
    ) -> Got<Posted<CommentId>> {
        unsupported()
    }

    fn file(&mut self, _: &NewTicket) -> Got<Posted<TicketKey>> {
        unsupported()
    }

    fn edit(
        &mut self,
        _: &TicketKey,
        _: Option<&str>,
        _: Option<&str>,
        _: &MarkerEdit,
        _: &Revision,
    ) -> Got<WebLink> {
        unsupported()
    }

    fn reopen(&mut self, _: &TicketKey) -> Did {
        unsupported()
    }

    fn set_kind(&mut self, _: &TicketKey, _: Option<&Kind>) -> Did {
        unsupported()
    }

    fn set_priority(&mut self, _: &TicketKey, _: Priority) -> Did {
        unsupported()
    }

    fn wait_on(&mut self, _: &TicketKey, _: &Item) -> Did {
        unsupported()
    }

    fn stop_waiting(&mut self, _: &TicketKey, _: &Item) -> Did {
        unsupported()
    }

    fn assign(&mut self, _: &TicketKey, _: &tehas_ports::AccountId) -> Did {
        unsupported()
    }

    fn add_part(&mut self, _: &TicketKey, _: &TicketKey) -> Did {
        unsupported()
    }

    fn remove_part(&mut self, _: &TicketKey, _: &TicketKey) -> Did {
        unsupported()
    }

    fn new_target(&mut self, _: &str, _: Option<At>) -> Got<TargetId> {
        unsupported()
    }

    fn edit_target(&mut self, _: &TargetId, _: &TargetEdit) -> Did {
        unsupported()
    }

    fn set_target(&mut self, _: &TicketKey, _: Option<&TargetId>) -> Did {
        unsupported()
    }
}
