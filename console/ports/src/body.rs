//! What a write sends: `Body`, `Sum`, `Blobs`, `NoBlobs`, and citing,
//! `Cite` and `Cites`.

use serde::{Deserialize, Serialize};

use crate::{BlueprintNo, ChangeNo, Login, Marker, Ref, Sha256, TicketKey};

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

/// What a body names: an item here or elsewhere, or an account. It may
/// grow. It has no `Unknown`: it is what a caller asks `Cites::cite` for,
/// never a record read back.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Cite {
    /// A ticket.
    Ticket(Ref<TicketKey>),
    /// A blueprint.
    Blueprint(Ref<BlueprintNo>),
    /// A change.
    Change(Ref<ChangeNo>),
    /// An account.
    Account(Login),
    /// The text a person types to make this marker, quoted as the site
    /// shows code, so it is no live marker: what a reply tells a person to
    /// write. A marker that is not [`Marker::writable`] cites as `""`.
    Marker(Marker),
}

/// `Tickets`, `Blueprints` and `CodeHost` have it. The text that names
/// `what` in a body written through this handle, as its site links or
/// mentions it. No request. A body that holds a cited marker holds no
/// marker (`laws::markers::a_cited_marker_is_no_marker`).
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

impl Body {
    /// The same body as `Summed`: what a journal row keeps. The ports never
    /// hash: `sha256` gives the digest of a text, and None for a text whose
    /// digest the caller does not give, which makes this None.
    pub(crate) fn summed(&self, sha256: impl FnOnce(&str) -> Option<Sha256>) -> Option<Body> {
        match self {
            Body::Text(text) => Some(Body::Summed(Sum {
                sha256: sha256(text)?,
                len: text.len() as u64,
            })),
            Body::Summed(sum) => Some(Body::Summed(sum.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    /// The standard digests of "abc" and of "", and no other.
    fn known(text: &str) -> Option<Sha256> {
        match text {
            "abc" => Sha256::new(ABC).ok(),
            "" => Sha256::new(EMPTY).ok(),
            _ => None,
        }
    }

    #[test]
    fn a_summed_body_takes_the_callers_digest() {
        let summed = Body::Text("abc".into()).summed(known).unwrap();
        let want = Body::Summed(Sum {
            sha256: Sha256::new(ABC).unwrap(),
            len: 3,
        });
        assert_eq!(summed, want);
        let empty = Body::Text(String::new()).summed(known).unwrap();
        assert_eq!(
            empty,
            Body::Summed(Sum {
                sha256: Sha256::new(EMPTY).unwrap(),
                len: 0,
            })
        );
    }

    #[test]
    fn a_summed_body_keeps_its_sum_and_asks_for_none() {
        let summed = Body::Text("abc".into()).summed(known).unwrap();
        let again = summed.summed(|_| panic!("a summed body asks for no digest"));
        assert_eq!(again, Some(summed));
    }

    #[test]
    fn a_text_without_a_given_digest_is_not_summed() {
        assert_eq!(Body::Text("abd".into()).summed(known), None);
        assert_eq!(Body::Text("abc".into()).summed(|_| None), None);
    }

    #[test]
    fn no_blobs_holds_no_file() {
        let sum = Sum {
            sha256: Sha256::new("a".repeat(64)).unwrap(),
            len: 1,
        };
        assert!(NoBlobs.open(&sum).is_none());
    }
}
