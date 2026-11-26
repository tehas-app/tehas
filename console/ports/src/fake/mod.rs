//! The fakes every caller's tests run on, in tehas's words: `World`, an
//! in-memory site with a clock the test moves; `FakeSite`, a `Site` over a
//! `World`; `Faulty`, which wraps any site and answers chosen calls with an
//! outcome; `NoticeLog`, a sink that keeps what it was told; `MemoryLedger`
//! and `MemoryTokens`, a `Ledger` and a `TokenStore` in memory.
//!
//! Compiled always, std only, no effects: the clock is the test's, and no
//! answer depends on the machine it runs on.

mod blueprints;
mod code_host;
mod faulty;
mod ledger;
mod notices;
mod site;
mod spec;
mod state;
mod tickets;
mod tokens;
mod world;

pub use faulty::*;
pub use ledger::*;
pub use notices::*;
pub use site::*;
pub use spec::*;
pub use tokens::*;
pub use world::*;

/// One label name as the site compares them: whatever the case.
fn same_label(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

// Here, not in `state`, so that every port of the fake may call them
// and nothing outside the fake may.
impl state::ItemRec {
    /// Whether the ticket holds a label of this name, by a mark or by
    /// name alone. The site holds a name once.
    fn holds_label(&self, name: &str) -> bool {
        let marks = self.marks.iter().map(site::mark_name);
        (self.labels.iter().map(|(_, n)| n.clone()))
            .chain(marks)
            .any(|n| same_label(&n, name))
    }

    /// Put a label on by name alone, after the marks the ticket carries
    /// now. An empty name or one held already changes nothing.
    fn label(&mut self, name: &str) {
        if !name.is_empty() && !self.holds_label(name) {
            self.labels.push((self.marks.len(), name.to_string()));
        }
    }

    /// Put a mark on, once: whether it was not on before. A label of its
    /// name put on by name alone becomes the mark, where it stood.
    fn put_mark(&mut self, m: &crate::Mark) -> bool {
        if self.marks.contains(m) {
            return false;
        }
        let name = site::mark_name(m);
        let raw = (self.labels.iter()).position(|(_, n)| !name.is_empty() && same_label(n, &name));
        let Some(r) = raw else {
            self.marks.push(m.clone());
            return true;
        };
        let (k, _) = self.labels.remove(r);
        self.marks.insert(k, m.clone());
        for (i, (after, _)) in self.labels.iter_mut().enumerate() {
            if *after > k || (*after == k && i >= r) {
                *after += 1;
            }
        }
        true
    }

    /// Take a mark off: whether it was on. The labels put on by name
    /// alone keep their places.
    fn take_mark(&mut self, m: &crate::Mark) -> bool {
        let Some(i) = self.marks.iter().position(|x| x == m) else {
            return false;
        };
        self.marks.remove(i);
        for (after, _) in &mut self.labels {
            if *after > i {
                *after -= 1;
            }
        }
        true
    }
}
