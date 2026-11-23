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

impl Sum {
    /// The sum of these bytes.
    pub(crate) fn of(bytes: &[u8]) -> Sum {
        let hex: String = sha256(bytes).iter().map(|b| format!("{b:02x}")).collect();
        Sum {
            sha256: Sha256::new(hex).expect("64 lower-case hex digits"),
            len: bytes.len() as u64,
        }
    }
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

impl Body {
    /// The same body as `Summed`: what a journal row keeps.
    pub(crate) fn summed(&self) -> Body {
        match self {
            Body::Text(text) => Body::Summed(Sum::of(text.as_bytes())),
            Body::Summed(sum) => Body::Summed(sum.clone()),
        }
    }
}

/// The round constants of sha-256.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// The sha-256 of `bytes`, by FIPS 180-4. The crate takes serde alone, so
/// it has its own.
fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = bytes.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((bytes.len() as u64).wrapping_mul(8)).to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *chunk = word.to_be_bytes();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        Sum::of(bytes).sha256.as_str().to_string()
    }

    #[test]
    fn sha256_keeps_the_standards_vectors() {
        assert_eq!(
            hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hex(&[b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        assert_eq!(Sum::of(b"abc").len, 3);
    }

    #[test]
    fn a_summed_body_keeps_its_sum() {
        let text = Body::Text("abc".into());
        let summed = text.summed();
        assert_eq!(summed, Body::Summed(Sum::of(b"abc")));
        assert_eq!(summed.summed(), summed);
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
