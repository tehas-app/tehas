//! The tickets port's reads, `Tickets`. What the port's callers ask today
//! is answered: a ticket's digest (`several`) and its marks going on and
//! off (`history`).

use std::num::{NonZeroU32, NonZeroU64};

use super::unsupported;
use crate::forge::uri;
use crate::GitHub;
use tehas_ports::{
    At, Blocker, Bound, ChangeToken, ClosedTicket, Comment, CommentId, Cursor, Depth, Edit, Filed,
    Filter, Got, HappenedKind, Item, Kind, Mark, MarkId, MarkerKey, MarkerKind, Part, Target,
    TargetId, Ticket, TicketDelta, TicketDigest, TicketEvent, TicketKey, Tickets, Took, WebLink,
    Whole, Window, WithCloser, Within,
};

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

    fn several_with_closers(&mut self, keys: &[TicketKey]) -> Vec<(TicketKey, Got<WithCloser>)> {
        keys.iter().map(|k| (k.clone(), unsupported())).collect()
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

    fn marks(&mut self, _: Bound) -> Got<Whole<(Mark, MarkId)>> {
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
