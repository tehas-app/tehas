//! The link from a ticket to its blueprints: `Links`, which the caller's
//! store answers, and `SameItem` for setups where ticket and blueprint are
//! one item.

use std::num::NonZeroU64;

use crate::{BlueprintNo, Got, TicketKey};

/// Which blueprints a ticket's approval covers, as tehas recorded it. Read
/// only for the adapters; the store alone writes the links.
pub trait Links: Send + Sync {
    /// The blueprints linked to a ticket.
    fn blueprints_of(&self, ticket: &TicketKey) -> Got<Vec<BlueprintNo>>;
    /// The ticket a blueprint is linked to, if any.
    fn ticket_of(&self, blueprint: BlueprintNo) -> Got<Option<TicketKey>>;
}

/// Where ticket and blueprint are one item: the same number, both ways. A
/// ticket's blueprint is `TicketKey::same_item`, so a ticket named by a key
/// has none here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SameItem;

impl Links for SameItem {
    fn blueprints_of(&self, ticket: &TicketKey) -> Got<Vec<BlueprintNo>> {
        Ok(ticket.same_item().into_iter().collect())
    }

    fn ticket_of(&self, blueprint: BlueprintNo) -> Got<Option<TicketKey>> {
        Ok(NonZeroU64::new(blueprint.get()).map(TicketKey::Number))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KeyText;

    #[test]
    fn one_item_is_its_own_blueprint_both_ways() {
        let n = NonZeroU64::new(7).unwrap();
        let no = BlueprintNo::new(7).unwrap();
        assert_eq!(SameItem.blueprints_of(&TicketKey::Number(n)), Ok(vec![no]));
        assert_eq!(SameItem.ticket_of(no), Ok(Some(TicketKey::Number(n))));
        let key = TicketKey::Key(KeyText::new("ABC-7").unwrap());
        assert_eq!(SameItem.blueprints_of(&key), Ok(vec![]));
    }
}
