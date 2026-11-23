//! The tickets port's writes, `TicketWrites`. Answered: the factory's
//! labels made (`prepare`), a project's own label (`ensure_mark`), and a
//! mark put on or taken off.

use super::{label_of, number, unsupported};
use crate::{table, GitHub};
use tehas_ports::{
    Applied, At, CloseReason, CommentId, Did, Got, Item, Kind, Mark, MarkSpec, MarkerEdit,
    NewTicket, Outcome, Posted, Priority, Refusal, Revision, Step, StepPart, TargetEdit, TargetId,
    TicketKey, TicketWrites, Unprepared, WebLink,
};

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
        let mut applied = Applied::default();
        applied.stopped = first.map(|p| (p, Outcome::Refused(Refusal::Unsupported)));
        (applied, None)
    }

    /// Each label of the factory's table, made or brought back to its colour
    /// and text, one call each.
    fn prepare(&mut self) -> Result<(), Unprepared> {
        let mut first = Ok(());
        for spec in table::specs() {
            let made = self.ensure_mark(&spec);
            if let (Ok(()), Err(why)) = (&first, made) {
                let mut unprepared = Unprepared::new(why);
                unprepared.mark = Some(spec.name.clone());
                first = Err(unprepared);
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
