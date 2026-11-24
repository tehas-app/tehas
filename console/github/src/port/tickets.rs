//! The tickets port's reads, `Tickets`. Answered: one ticket to a depth
//! (`ticket`), the tickets a filter takes (`list`), a ticket's digest
//! (`several`), its history of every kind (`history`), its parts
//! (`parts`), what it waits on (`blockers`), what a number names
//! (`what_is`, `what_are`), the site's marks with their ids (`marks`)
//! and its types of ticket (`kinds`).

use std::num::{NonZeroU32, NonZeroU64};

use super::unsupported;
use crate::forge::uri;
use crate::GitHub;
use tehas_ports::{
    At, Blocker, Bound, ChangeToken, ClosedTicket, Comment, CommentId, Cursor, Depth, Edit, Filed,
    Filter, Got, HappenedKind, Item, Kind, Mark, MarkHeld, MarkerKey, MarkerKind, Outcome, Part,
    Target, TargetId, Ticket, TicketDelta, TicketDigest, TicketEvent, TicketKey, Tickets, Took,
    Unread, WebLink, Whole, Window, WithCloser, Within,
};

const INCONSISTENT: Outcome = Outcome::Unread(Unread::Inconsistent);

impl Tickets for GitHub {
    /// The issue's REST read, and one question for the rest of it with
    /// the first page of its history.
    fn ticket(&mut self, key: &TicketKey, depth: Depth, history: Option<Window>) -> Got<Ticket> {
        GitHub::ticket(self, key, depth, history)
    }

    /// `what_are` of the one number.
    fn what_is(&mut self, number: NonZeroU64) -> Got<Item> {
        let mut answers = GitHub::what_are(self, &[number]);
        answers.pop().map_or(Err(INCONSISTENT), |(_, a)| a)
    }

    /// One question of the repository, an alias a number, for up to a
    /// hundred numbers.
    fn what_are(&mut self, numbers: &[NonZeroU64]) -> Vec<(NonZeroU64, Got<Item>)> {
        GitHub::what_are(self, numbers)
    }

    /// The repository's issues, a question a page.
    fn list(&mut self, filter: &Filter, bound: Bound) -> Got<Whole<TicketDigest>> {
        GitHub::list(self, filter, bound)
    }

    fn recent(&mut self, _: &Filter, _: NonZeroU32) -> Got<Took<TicketDigest>> {
        unsupported()
    }

    fn recently_closed(&mut self, _: NonZeroU32) -> Got<Took<ClosedTicket>> {
        unsupported()
    }

    fn count(&mut self, _: &Filter) -> Got<u64> {
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

    /// The issue's blocked-by links, one question.
    fn blockers(&mut self, key: &TicketKey) -> Got<Whole<Blocker>> {
        GitHub::blockers(self, key)
    }

    fn parts(&mut self, key: &TicketKey) -> Got<Whole<Part>> {
        GitHub::parts(self, key)
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

    /// The owner's issue types, a question a page.
    fn kinds(&mut self) -> Got<Whole<Kind>> {
        self.kinds_held()
    }

    /// The repository's labels with their node ids, a question a page.
    fn marks(&mut self, bound: Bound) -> Got<Whole<MarkHeld>> {
        self.marks_held(bound)
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
