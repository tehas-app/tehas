//! The grants a plugin may ask for, and nothing else.
//!
//! The standard's §5 says it plainly (`tehas-standard` `v3/plugins.md:214-216`):
//! "A plugin **asks** by name in `asks`; a grant gives a value only on a
//! host. Names are `name` or `name:<value>`. A host refuses a name it does
//! not know." Its conformance rule (`:20-23`) reads a plain statement of what
//! a host does as a MUST, so refusing an unknown name is required, not
//! courteous: a manifest that asks `weather:` must fail here rather than
//! install and be granted nothing in silence.
//!
//! What a name *earns* is decided elsewhere. Nothing is granted from a
//! manifest (`docs/specs/2026-11-25-plugins-2.0.0-operator.md:841-848`): for
//! our own plugins the signed vetted entry is the grant, and for a
//! third-party plugin a person approves each destination by name. This module
//! only knows the words, so that a plugin's author learns the difference
//! before the host runs anything.
//!
//! The list is the standard's §5, common and admin-given, plus the two the
//! operator's own specs add. §5 `:228` says a host may add its own and lists
//! them in its spec; these are ours, and the provenance of every name is the
//! comment beside it. `view:<id>` and `skill:<id>` are host additions, not
//! §5 names: `docs/specs/2026-11-21-plugins-v2.md:65,68` and
//! `docs/specs/2026-11-21-plugins-v2-chats.md:49`. The standard lists
//! neither, which it is wrong about and has been told.

/// The most characters a parameterised name's value may hold.
const VALUE_MAX: usize = 64;

/// The names with no value on them, each §5's at the line noted.
const PLAIN: [&str; 18] = [
    "product",         // :217
    "history",         // :217
    "files",           // :217
    "cycles",          // :217
    "issues",          // :217
    "pages",           // :217
    "keep",            // :219, a private /state
    "model:images",    // :220-221, a name of its own, not a token count
    "feed:advisories", // :219, and named again below by its shape
    "industry:logs",   // :219
    "view",            // :221
    "menu",            // :221
    "record",          // :222
    "issue:draft",     // :222
    "issue:advisory",  // :235-239
    "net:", // :223, written so by the standard; in 2.1 only an adapter reaches the network (operator :872)
    "http", // :227, an adapter's alone; the manifest's kind decides
    "viewer:login", // :224-227
];

/// Whether `word` is a grant 2.0 knows. The reason says what a near miss
/// wanted, so `feed:` alone and `feed:mesh` do not come back the same.
pub fn check(word: &str) -> Result<(), String> {
    if PLAIN.contains(&word) {
        return Ok(());
    }
    let Some((front, value)) = word.split_once(':') else {
        return Err(not_a_grant());
    };
    if value.is_empty() {
        return Err(format!("{front}: wants a value after the colon"));
    }
    if value.len() > VALUE_MAX {
        return Err(format!("{front}: its value is over {VALUE_MAX} characters"));
    }
    match front {
        // :220. A token budget for one run.
        "model" if is_number(value) => Ok(()),
        // :221 and 6.3, and this reader's pairing of an entry to its id.
        // `skill:` and `view:` are the host's own, from its v2 specs.
        "action" | "tool" | "view" | "skill" if super::is_action_id(value) => Ok(()),
        // :218. Which feeds exist is the host's and the vetted entry's, so
        // only that the name is a word is checked here.
        "feed" | "live" if is_value(value) => Ok(()),
        // :219.
        "industry" if is_value(value) => Ok(()),
        // :221. No interval grammar exists in either document, so the shape
        // is all the reader can ask for.
        "every" if is_value(value) => Ok(()),
        // :221-222. The one spelled example is `clock:mon-fri@06:00`, so the
        // value may hold an `@` and a `:`.
        "clock" if is_schedule(value) => Ok(()),
        // :222 and :231. The policy an admin attaches to a gate
        // (`hook:before:<event>=<policy>`) is the grant's, never a
        // manifest's, so a `=` in an ask names nothing.
        "hook" if is_hook(value) => Ok(()),
        // :233.
        "phase" if super::is_action_id(value) => Ok(()),
        // :234.
        "answer" if is_value(value) => Ok(()),
        // :648, §14.
        "device" if is_value(value) => Ok(()),
        _ => Err(not_a_grant()),
    }
}

/// Why a word is not a grant. The list is not shown: it is the standard's
/// §5 and the host's own, and a plugin's author reads those, not a host's
/// error text.
fn not_a_grant() -> String {
    "is not a grant 2.0 knows, of the standard's §5 and the host's own list".to_string()
}

/// A value of lower-case letters, digits and `._-`.
fn is_value(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}

/// A token count, as `model:<tokens>` wants.
fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// A schedule: `daily@06:00`, `mon-fri@06:00`.
fn is_schedule(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"@:_-.".contains(&b))
}

/// `<when>:<event>`, as `hook:` wants. Which points exist is part 5's list
/// and the gate's knowledge, so only the shape is here.
fn is_hook(s: &str) -> bool {
    match s.split_once(':') {
        Some((when, event)) => is_value(when) && is_value(event),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{check, PLAIN};

    fn ok(word: &str) {
        assert_eq!(check(word), Ok(()), "{word}");
    }

    fn refused(word: &str) -> String {
        match check(word) {
            Ok(()) => panic!("{word} was accepted"),
            Err(why) => why,
        }
    }

    /// Every name of the list is accepted. The list is the reader's whole
    /// knowledge of what a plugin may ask, so it is checked in full rather
    /// than by samples.
    #[test]
    fn every_name_of_the_list_is_a_grant() {
        for word in PLAIN {
            ok(word);
        }
        for word in [
            "model:2000",
            "action:refresh",
            "tool:find",
            "view:energy",
            "skill:grill",
            "feed:mesh",
            "feed:host",
            "live:cycles",
            "every:1m",
            "clock:daily@06:00",
            "clock:mon-fri@06:00",
            "hook:after:change.offered",
            "hook:before:change.offered",
            "phase:review",
            "answer:review",
            "device:camera",
        ] {
            ok(word);
        }
    }

    /// A name the list does not hold is refused. `feed` with no colon and
    /// `viewer` with no `:login` are the two a plugin's author is likeliest
    /// to write, so they are named here and not left to the general case.
    #[test]
    fn a_name_the_host_does_not_know_is_refused() {
        for word in [
            "weather",
            "wheather",
            "feed",
            "viewer",
            "gate:verify",
            "needs:specs",
        ] {
            assert!(
                refused(word).starts_with("is not a grant"),
                "{word}: {}",
                refused(word)
            );
        }
    }

    /// The near miss that matters: a parameterised name with nothing on it.
    /// `feed:notes` is a grant and `feed:` is not, so a manifest cannot ask
    /// its way to every feed by naming the front alone. The counter-assertion
    /// is in the same test: the bare front is refused where the named feed is
    /// accepted.
    #[test]
    fn a_parameterised_name_needs_its_value() {
        for front in [
            "feed", "live", "every", "model", "action", "skill", "view", "clock",
        ] {
            let bare = format!("{front}:");
            assert!(
                refused(&bare).starts_with(&format!("{front}: wants a value")),
                "{bare}: {}",
                refused(&bare)
            );
        }
        ok("feed:notes");
        ok("model:1000");
        ok("skill:notes");
    }

    /// The value of each parameterised name keeps to its shape, so a stray
    /// word does not become a grant by landing after a colon.
    #[test]
    fn a_parameterised_name_wants_its_value_of_the_right_shape() {
        let not = [
            "model:many",
            "action:Refresh",
            "skill:grill/me",
            "feed:MEsh",
            "phase:1",
        ];
        for word in not {
            assert!(
                refused(word).starts_with("is not a grant"),
                "{word}: {}",
                refused(word)
            );
        }
        let long = format!("every:{}1", "1".repeat(super::VALUE_MAX));
        assert!(refused(&long).starts_with("every: its value"));
    }

    /// A gate's policy belongs to the admin's grant, not to a manifest, and a
    /// point needs both its sides.
    #[test]
    fn a_hook_names_its_point_and_not_its_policy() {
        ok("hook:before:change.offered");
        for word in [
            "hook:before:change.offered=hold",
            "hook:after",
            "hook::change.offered",
            "hook:after:",
        ] {
            assert!(
                refused(word).starts_with("is not a grant"),
                "{word}: {}",
                refused(word)
            );
        }
    }

    /// The two host additions, which the standard omits and the registry
    /// depends on: without them a skills-only plugin asks nothing the host
    /// knows, and a view of its own cannot be asked for at all.
    #[test]
    fn the_host_adds_its_own_names() {
        ok("skill:course");
        ok("skill:levels");
        ok("view:host");
        for word in ["skills:course", "views:host"] {
            assert!(refused(word).starts_with("is not a grant"), "{word}");
        }
    }
}
