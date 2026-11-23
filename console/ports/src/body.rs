//! What a write sends: `Body`, `Sum`, `Blobs`, `NoBlobs`, and citing,
//! `Cite` and `Cites`.

use serde::{Deserialize, Serialize};

use crate::{BlueprintNo, ChangeNo, Login, Ref, Sha256, TicketKey};

/// A body or a file named by its sum: sha-256 in hex, and its length.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sum {
    /// The sha-256 of the bytes.
    pub sha256: Sha256,
    /// The number of bytes.
    pub len: u64,
}

/// Where a write that sends files finds their bytes, by sum. The caller
/// holds the files; a write row names them by sum alone. A sum it does not
/// hold, or bytes that do not match it, is `Refused(Invalid)` before any
/// request.
pub trait Blobs {
    /// The bytes of the file with this sum, or None if the caller holds none.
    fn open(&self, sum: &Sum) -> Option<Box<dyn std::io::Read + '_>>;
}

/// For a caller whose writes send no files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoBlobs;

impl Blobs for NoBlobs {
    fn open(&self, _: &Sum) -> Option<Box<dyn std::io::Read + '_>> {
        None
    }
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

/// `Tickets`, `Blueprints` and `CodeHost` have it. The text that names
/// `what` in a body written through this handle, as its site links or
/// mentions it. No request.
pub trait Cites {
    /// The text that names `what` in a body written through this handle.
    fn cite(&self, what: &Cite) -> String;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_blobs_holds_no_file() {
        let sum = Sum {
            sha256: Sha256::new("a".repeat(64)).unwrap(),
            len: 1,
        };
        assert!(NoBlobs.open(&sum).is_none());
    }
}
