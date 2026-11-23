//! The dispatcher: listeners registered at a program's wiring, called in
//! the raiser's thread, in order of priority, until one stops the event.

use std::panic::{catch_unwind, AssertUnwindSafe};

use super::{Envelope, Name, Payload};

/// A listener's answer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Heard {
    /// Call the next listener.
    Go,
    /// Call no later listener for this raise. It undoes nothing.
    Stop,
    /// The listener failed, and says why. The next listener is called.
    Failed(String),
}

/// A listener that failed or panicked.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Fault {
    /// The listener's id.
    pub listener: &'static str,
    /// Why: its `Failed`, or its panic's message.
    pub why: String,
}

/// What a raise did: who stopped it, if one did, and the faults, for the
/// raiser to log. A raise never fails the raiser.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Raised {
    /// The listener that answered `Stop`.
    pub stopped_by: Option<&'static str>,
    /// The listeners that failed or panicked, in the order called.
    pub faults: Vec<Fault>,
}

type Listen = Box<dyn Fn(&Envelope) -> Heard + Send + Sync>;

struct Entry {
    // None for a listener of every name.
    name: Option<Name>,
    listener: &'static str,
    priority: i16,
    f: Listen,
}

/// Listeners by name and priority, made at a program's wiring.
#[derive(Default)]
pub struct Builder {
    entries: Vec<Entry>,
}

impl Builder {
    /// No listener yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Calls `f` for every event of `P`'s name. Higher `priority` runs
    /// first; equal priorities run in the order registered.
    pub fn on<P: Payload>(
        &mut self,
        listener: &'static str,
        priority: i16,
        f: impl Fn(&P) -> Heard + Send + Sync + 'static,
    ) -> &mut Self {
        self.entries.push(Entry {
            name: Some(P::NAME),
            listener,
            priority,
            f: Box::new(move |e: &Envelope| P::peek(&e.event).map_or(Heard::Go, &f)),
        });
        self
    }

    /// Calls `f` for every event, as its envelope: for the bridges that
    /// take every event as JSON, the log, plugins, a page's stream.
    pub fn on_any(
        &mut self,
        listener: &'static str,
        priority: i16,
        f: impl Fn(&Envelope) -> Heard + Send + Sync + 'static,
    ) -> &mut Self {
        self.entries.push(Entry {
            name: None,
            listener,
            priority,
            f: Box::new(f),
        });
        self
    }

    /// The dispatcher, read only from here on.
    pub fn build(self) -> Dispatcher {
        let mut entries = self.entries;
        // A stable sort keeps the order registered among equal priorities.
        entries.sort_by_key(|e| std::cmp::Reverse(e.priority));
        Dispatcher { entries }
    }
}

/// Built once at a program's wiring, then read only: `Send + Sync`. No
/// listener has a handle on it, so no raise happens inside a raise.
pub struct Dispatcher {
    entries: Vec<Entry>,
}

impl Dispatcher {
    /// Calls the listeners of `p`'s name, and those of every name, in order
    /// until one stops it. A listener's failure or panic is a fault, and
    /// the next listener is called.
    pub fn raise<P: Payload>(&self, p: P) -> Raised {
        let envelope = Envelope::new(p);
        let mut raised = Raised::default();
        for e in self
            .entries
            .iter()
            .filter(|e| e.name.is_none_or(|n| n == P::NAME))
        {
            match catch_unwind(AssertUnwindSafe(|| (e.f)(&envelope))) {
                Ok(Heard::Go) => {}
                Ok(Heard::Stop) => {
                    raised.stopped_by = Some(e.listener);
                    break;
                }
                Ok(Heard::Failed(why)) => raised.faults.push(Fault {
                    listener: e.listener,
                    why,
                }),
                Err(panic) => raised.faults.push(Fault {
                    listener: e.listener,
                    why: panic_text(panic.as_ref()),
                }),
            }
        }
        raised
    }
}

fn panic_text(panic: &(dyn std::any::Any + Send)) -> String {
    let text = panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned());
    format!("panicked: {}", text.as_deref().unwrap_or("no message"))
}
