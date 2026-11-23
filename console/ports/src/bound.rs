//! How much a read takes: `Bound`, `Window`, `Depth`, and where a search
//! looks, `Within`.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

/// The most items a caller takes. More than this is `Unread(Cut)`, but
/// for a delta (`Tickets::delta`, `Blueprints::delta`, `CodeHost::delta`):
/// past the bound a delta answers a page of its first changes, oldest
/// first, at most `bound` and never none, with `more` and a `next` that
/// resumes after them.
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

/// The most notes or events `Depth::All` takes. More is `Unread(Cut)`.
pub const ALL_MAX: Bound = Bound(NonZeroU32::new(1000).unwrap());

/// How much of a ticket or a blueprint a read takes: its notes, and a
/// ticket's history (`Depth::notes`, `Depth::history`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Depth {
    /// The last `notes` notes, text cut to the adapter's bound. No history:
    /// a ticket's is empty, with `more`.
    Cut {
        /// How many notes.
        notes: u8,
    },
    /// The newest `notes` notes, oldest first, each with its text whole,
    /// answered as `Took` with `more`. A note too large to give whole makes
    /// the read `Unread(TooLarge)`, never a cut text. A ticket's history is
    /// its newest `notes` events, with `more`.
    Last {
        /// How many notes.
        notes: NonZeroU32,
    },
    /// Every note and event, paged whole, or `Unread(Cut)` past `ALL_MAX`.
    All,
}

impl Depth {
    /// The window of notes this depth takes; None takes no note.
    pub fn notes(self) -> Option<Window> {
        match self {
            Depth::Cut { notes } => NonZeroU32::new(u32::from(notes)).map(Window::Last),
            Depth::Last { notes } => Some(Window::Last(notes)),
            Depth::All => Some(Window::Whole(ALL_MAX)),
        }
    }

    /// The window of a ticket's history this depth takes. None takes none,
    /// answered as an empty history with `more`: possibly cut.
    pub fn history(self) -> Option<Window> {
        match self {
            Depth::Cut { .. } => None,
            Depth::Last { notes } => Some(Window::Last(notes)),
            Depth::All => Some(Window::Whole(ALL_MAX)),
        }
    }
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

    #[test]
    fn a_depth_names_its_windows_of_notes_and_history() {
        let five = NonZeroU32::new(5).unwrap();
        let last = Depth::Last { notes: five };
        assert_eq!(last.history(), Some(Window::Last(five)));
        assert_eq!(last.notes(), Some(Window::Last(five)));
        assert_eq!(Depth::All.history(), Some(Window::Whole(ALL_MAX)));
        assert_eq!(Depth::All.notes(), Some(Window::Whole(ALL_MAX)));
        assert_eq!(Depth::Cut { notes: 5 }.history(), None);
        assert_eq!(Depth::Cut { notes: 5 }.notes(), Some(Window::Last(five)));
        assert_eq!(Depth::Cut { notes: 0 }.notes(), None);
        assert_eq!(ALL_MAX.get(), 1000);
    }
}
