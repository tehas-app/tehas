//! Every write and its answer, as the journal records it: `Write`,
//! `WriteKind`, `Receipt`, `Posted`, `Writes`.

use serde::{Deserialize, Serialize};

use crate::WebLink;

/// A write's answer: what it made, and where a person opens it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Posted<T> {
    /// What it made.
    pub id: T,
    /// Where a person opens it.
    pub link: WebLink,
}
