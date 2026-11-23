//! What a write sends: `Body`, `Sum`, and what a body names, `Cite`.

use serde::{Deserialize, Serialize};

use crate::{BlueprintNo, ChangeNo, Login, Ref, TicketKey};

// T4.1b: `Blobs`, the trait a write that sends files finds their bytes
// through, and `NoBlobs`, its answer for a caller that sends none.
// T4.1b: `Cites`, the trait of the read handles that writes a `Cite` in a
// site's text.

/// A body or a file named by its sum: sha-256 in hex, and its length.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sum {
    /// The sha-256 of the bytes, in hex.
    pub sha256: String,
    /// The number of bytes.
    pub len: u64,
}

/// What a body names: an item here or elsewhere, or an account.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cite {
    /// A ticket.
    Ticket(Ref<TicketKey>),
    /// A blueprint.
    Blueprint(Ref<BlueprintNo>),
    /// A change.
    Change(Ref<ChangeNo>),
    /// An account.
    Account(Login),
}

/// A body as a write takes it, and as its journal row keeps it: the text
/// when sent, its sum in the row, so the store keeps no body. A port method
/// given `Summed` answers `Refused(Invalid)` before any request. A body never
/// holds our markers: they travel beside it, and its sum is over the body
/// alone.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Body {
    /// The text, as it is sent.
    Text(String),
    /// The text's sum, as a journal row keeps it.
    Summed(Sum),
}
