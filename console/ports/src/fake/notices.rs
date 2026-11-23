//! `Notices`: a sink that keeps what it was told, so a test asserts one
//! report.

use std::sync::Mutex;

use super::state::lock;
use crate::{Health, Notice};

/// A `Notices` sink that keeps every notice, in the order told.
#[derive(Debug, Default)]
pub struct Notices {
    told: Mutex<Vec<Notice>>,
}

impl Notices {
    /// A sink that has been told nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every notice it was told, oldest first.
    pub fn all(&self) -> Vec<Notice> {
        lock(&self.told).clone()
    }

    /// How many notices said `what`.
    pub fn count(&self, what: &Health) -> usize {
        lock(&self.told).iter().filter(|n| n.what == *what).count()
    }

    /// How many notices of each kind of `Health` it was told, ignoring
    /// their content: `Spent` and `Unavailable` count as one kind each.
    pub fn count_like(&self, what: &Health) -> usize {
        lock(&self.told)
            .iter()
            .filter(|n| std::mem::discriminant(&n.what) == std::mem::discriminant(what))
            .count()
    }
}

impl crate::Notices for Notices {
    fn notice(&self, n: Notice) {
        lock(&self.told).push(n);
    }
}
