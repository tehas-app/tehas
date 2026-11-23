//! How much a read takes: `Bound`, `Window`, `Depth`, and where a search
//! looks, `Within`.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

/// The most items a caller takes. More than this is `Unread(Cut)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Bound(NonZeroU32);

impl Bound {
    /// The bound, or None for 0.
    pub fn new(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self)
    }

    /// The most items.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl From<NonZeroU32> for Bound {
    fn from(n: NonZeroU32) -> Self {
        Self(n)
    }
}

/// How much of a list of events a caller takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Window {
    /// Every event, paged whole, or `Unread(Cut)` past the bound.
    Whole(Bound),
    /// The newest n, oldest first, each whole, answered as `Took` with
    /// `more`. More events than n is not a fault: a stale read wants the
    /// last 50 of a busy change, a queue read the last 100 of a ticket's
    /// marks.
    Last(NonZeroU32),
}

/// How much of a ticket or a blueprint a read takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Depth {
    /// The last `notes` notes, text cut to the adapter's bound.
    Cut {
        /// How many notes.
        notes: u8,
    },
    /// The newest `notes` notes, oldest first, each with its text whole,
    /// answered as `Took` with `more`. A note too large to give whole makes
    /// the read `Unread(TooLarge)`, never a cut text.
    Last {
        /// How many notes.
        notes: NonZeroU32,
    },
    /// Every note and event, paged whole or `Unread(Cut)`.
    All,
}

/// Where a search looks for its words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Within {
    /// The titles alone.
    Titles,
    /// The titles and the text.
    Everything,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bound_is_never_zero() {
        assert_eq!(Bound::new(0), None);
        assert_eq!(Bound::new(50).map(Bound::get), Some(50));
    }
}
