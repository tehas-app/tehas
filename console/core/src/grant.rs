//! The grant decision: whether a label's timeline holds an application that
//! authorises work, and whose it is. Authorisation is who applied the label.
//!
//! The poll takes the decision in steps over what it reads, and each step
//! asks the rule here: `accounts::Accounts::vouch` renames an account that
//! does not come with its recorded id (`doubt`, `counted`), the pins
//! (tehas_dispatch::pins) give a refused application the unknown actor
//! (`account_of`), and the rules ask who made the label's newest event when
//! that event is an application (`events::standing_actor`, `standing`) and
//! whether that account is allowed (`events::allowed_in`). `grant` is those
//! steps in one, over a timeline in order. The poll does not call it: the
//! proofs below are about `grant`, about the steps the poll takes, and that
//! the two agree. A label taken off since it was applied grants nothing,
//! whatever the issue's own labels or the guard of a page
//! (read::github::dropped_events) say.
//!
//! `last_application` (`events::label_actor`) is who applied the label last,
//! taken off since or not. It answers a question about the past, such as who
//! asked for a plan whose label the bot took off; it grants nothing alone.
//!
//! The types are generic over the login and the label so the proofs can use
//! small numbers for them; the poll uses text.

use crate::accounts::Doubt;
use crate::events::UNKNOWN_ACTOR;
use std::borrow::Cow;

/// Who an event names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Actor<L> {
    pub login: L,
    /// The id `state/account-ids` records for the login.
    pub recorded: Option<u64>,
    /// The id the event's object carried.
    pub carried: Option<u64>,
}

impl<L> Actor<L> {
    /// An account the poll has vouched for already: what its login says.
    pub fn vouched(login: L) -> Actor<L> {
        Actor {
            login,
            recorded: None,
            carried: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Applied,
    Removed,
}

/// One application or removal of a label.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabelEvent<L, B> {
    pub actor: Actor<L>,
    pub label: B,
    /// The label is the pinned one, or the pins do not judge it.
    pub pinned: bool,
    pub change: Change,
}

/// The account an event counts for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Name<L> {
    /// What its login says.
    Login(L),
    /// A listed login without its recorded id, with the id it carried.
    Doubted(L, Option<u64>),
    /// Nobody: an application the pins refuse.
    Unknown,
}

impl Name<&str> {
    /// The login the poll writes for it: the login, `<login>#<id>`,
    /// `<login>#` or `unknown`. No login of a list holds `#`.
    pub fn text(&self) -> Cow<'_, str> {
        match *self {
            Name::Login(l) => Cow::Borrowed(l),
            Name::Doubted(l, id) => Cow::Owned(doubted(l, id)),
            Name::Unknown => Cow::Borrowed(UNKNOWN_ACTOR),
        }
    }
}

/// `<login>#<id>`, or `<login>#` with no id.
fn doubted(login: &str, id: Option<u64>) -> String {
    let mark = id.map(|i| i.to_string()).unwrap_or_default();
    format!("{login}#{mark}")
}

/// Why an account that came with `carried` does not count as the listed
/// login it names, or None when it does. A login of no list is no claim.
pub fn doubt(listed: bool, recorded: Option<u64>, carried: Option<u64>) -> Option<Doubt> {
    if !listed {
        return None;
    }
    match (recorded, carried) {
        (_, None) => Some(Doubt::NoId),
        (None, _) => Some(Doubt::Unrecorded),
        (Some(r), Some(i)) if r == i => None,
        (Some(r), Some(_)) => Some(Doubt::Other { recorded: r }),
    }
}

/// The account an actor counts as: its login, or doubted.
pub fn counted<L: Copy>(a: &Actor<L>, listed: bool) -> Name<L> {
    match doubt(listed, a.recorded, a.carried) {
        None => Name::Login(a.login),
        Some(_) => Name::Doubted(a.login, a.carried),
    }
}

/// The account an event counts for: an application the pins refuse is
/// nobody's; any other is its actor's, as `counted` gives it.
pub fn account_of<L: Copy, B>(e: &LabelEvent<L, B>, listed: bool) -> Name<L> {
    if e.change == Change::Applied && !e.pinned {
        Name::Unknown
    } else {
        counted(&e.actor, listed)
    }
}

/// The newest application of `label`: who applied it.
pub fn last_application<L, B: PartialEq>(
    timeline: &[LabelEvent<L, B>],
    label: &B,
) -> Option<usize> {
    timeline
        .iter()
        .rposition(|e| e.change == Change::Applied && e.label == *label)
}

/// The newest application or removal of `label`.
pub fn latest<L, B: PartialEq>(timeline: &[LabelEvent<L, B>], label: &B) -> Option<usize> {
    timeline.iter().rposition(|e| e.label == *label)
}

/// The application of `label` that stands: its newest event, when that is an
/// application. None when the label was taken off since, or never applied.
pub fn standing<L, B: PartialEq>(timeline: &[LabelEvent<L, B>], label: &B) -> Option<usize> {
    latest(timeline, label).filter(|&i| timeline[i].change == Change::Applied)
}

/// The application of `label` that grants, by its place in the timeline: the
/// newest event of the label is an application, and the account it counts
/// for is allowed. `listed` says whether `state/account-ids` vouches for a
/// login; `allowed` is TEHAS_ALLOWED_LABELERS.
pub fn grant<L: Copy, B: PartialEq>(
    timeline: &[LabelEvent<L, B>],
    label: &B,
    listed: &dyn Fn(L) -> bool,
    allowed: &dyn Fn(Name<L>) -> bool,
) -> Option<usize> {
    let i = latest(timeline, label)?;
    let e = &timeline[i];
    let applied = last_application(timeline, label) == Some(i);
    (applied && allowed(account_of(e, listed(e.actor.login)))).then_some(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::allowed_in;

    fn ev(
        login: &'static str,
        carried: Option<u64>,
        pinned: bool,
        change: Change,
    ) -> LabelEvent<&'static str, &'static str> {
        LabelEvent {
            actor: Actor {
                login,
                recorded: (login == "alice").then_some(1001),
                carried,
            },
            label: "tehas:ready",
            pinned,
            change,
        }
    }

    fn decide(tl: &[LabelEvent<&str, &str>]) -> Option<usize> {
        let listed = |l: &str| l == "alice" || l == "bob";
        let allowed = |n: Name<&str>| allowed_in("alice bob", &n.text());
        grant(tl, &"tehas:ready", &listed, &allowed)
    }

    #[test]
    fn an_allowed_account_with_its_recorded_id_grants() {
        let tl = [ev("alice", Some(1001), true, Change::Applied)];
        assert_eq!(decide(&tl), Some(0));
    }

    #[test]
    fn what_does_not_count_grants_nothing() {
        use Change::*;
        for tl in [
            vec![ev("alice", Some(66), true, Applied)],
            vec![ev("alice", None, true, Applied)],
            vec![ev("bob", Some(1002), true, Applied)],
            vec![ev("alice", Some(1001), false, Applied)],
            vec![ev("mallory", None, true, Applied)],
            vec![
                ev("alice", Some(1001), true, Applied),
                ev("mallory", None, true, Removed),
            ],
            vec![
                ev("alice", Some(1001), true, Applied),
                ev("mallory", None, true, Applied),
            ],
        ] {
            assert_eq!(decide(&tl), None, "{tl:?}");
        }
    }

    #[test]
    fn only_an_application_nothing_took_off_stands() {
        use Change::*;
        let applied = ev("alice", Some(1001), true, Applied);
        let removed = ev("alice", Some(1001), true, Removed);
        let label = &"tehas:ready";
        assert_eq!(standing(&[applied], label), Some(0));
        assert_eq!(standing(&[applied, removed], label), None);
        assert_eq!(last_application(&[applied, removed], label), Some(0));
        assert_eq!(standing(&[applied, removed, applied], label), Some(2));
        assert_eq!(standing(&[removed], label), None);
        assert_eq!(standing::<&str, &str>(&[], label), None);
    }

    #[test]
    fn the_names_are_the_ones_vouch_and_the_pins_write() {
        let a = Actor {
            login: "alice",
            recorded: Some(1001),
            carried: Some(66),
        };
        assert_eq!(counted(&a, true).text(), "alice#66");
        assert_eq!(counted(&a, false).text(), "alice");
        let none = Actor { carried: None, ..a };
        assert_eq!(counted(&none, true).text(), "alice#");
        assert_eq!(Name::<&str>::Unknown.text(), UNKNOWN_ACTOR);
        for i in [0, 7, 10, 99, 100, 1001, u64::MAX] {
            assert_eq!(doubted("a", Some(i)), format!("a#{i}"));
        }
    }

    #[test]
    fn a_doubted_name_is_on_no_list_of_logins() {
        use crate::settings::rules::is_login;
        for login in ["alice", "unknown", ""] {
            for id in [None, Some(0), Some(1001), Some(u64::MAX)] {
                let name = Name::Doubted(login, id).text().into_owned();
                assert!(!is_login(&name), "{name}");
                assert!(!allowed_in("alice unknown", &name), "{name}");
            }
        }
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;
    use crate::events::allowed_in;

    /// Logins are numbers: 0 alice, 1 alice[bot], 2 bob, 3 mallory, 4 the
    /// GitHub account `unknown`, 5 the empty login.
    type Login = u8;
    const LOGINS: u8 = 6;
    const UNKNOWN_LOGIN: Login = 4;
    const EMPTY: Login = 5;
    /// The account-ids key of each login, as `accounts::account` gives it.
    const KEY: [u8; 6] = [0, 0, 1, 2, 3, 4];
    const READY: u8 = 0;
    const MAX: usize = 6;

    /// The settings and the recorded ids. `allowed` is a set of logins, a
    /// bit each; the empty login is never one, as `allowed_in` has it. The
    /// listed keys are those of the allowed logins and maybe more: the
    /// millwright's.
    struct World {
        allowed: u8,
        listed: u8,
        ids: [Option<u64>; 5],
    }

    fn world() -> World {
        let allowed: u8 = kani::any();
        let extra: u8 = kani::any();
        kani::assume((allowed & !((1 << EMPTY) - 1)) == 0);
        let keys = (0..LOGINS - 1)
            .filter(|l| allowed & (1 << l) != 0)
            .fold(0u8, |k, l| k | (1 << KEY[l as usize]));
        let ids: [Option<u64>; 5] = kani::any();
        World {
            allowed,
            listed: keys | (extra & 0b1_1111),
            ids,
        }
    }

    impl World {
        fn listed(&self, l: Login) -> bool {
            self.listed & (1 << KEY[l as usize]) != 0
        }
        fn allows_login(&self, l: Login) -> bool {
            l != EMPTY && self.allowed & (1 << l) != 0
        }
        /// `allowed_in(list, name.text())`: a doubted name holds `#`, which
        /// no login of the list does (`Rule::Logins`; the test
        /// `a_doubted_name_is_on_no_list_of_logins`), and the unknown actor's
        /// name is the login `unknown`. Text is too dear for the proofs.
        fn allows(&self, n: Name<Login>) -> bool {
            match n {
                Name::Login(l) => self.allows_login(l),
                Name::Doubted(..) => false,
                Name::Unknown => self.allows_login(UNKNOWN_LOGIN),
            }
        }
    }

    fn event(w: &World) -> LabelEvent<Login, u8> {
        let (login, label): (Login, u8) = (kani::any(), kani::any());
        kani::assume(login < LOGINS && label < 2);
        LabelEvent {
            actor: Actor {
                login,
                recorded: w.ids[KEY[login as usize] as usize],
                carried: kani::any(),
            },
            label,
            pinned: kani::any(),
            change: if kani::any() {
                Change::Applied
            } else {
                Change::Removed
            },
        }
    }

    fn timeline(w: &World) -> ([LabelEvent<Login, u8>; MAX], usize) {
        let n: usize = kani::any();
        kani::assume(n <= MAX);
        (core::array::from_fn(|_| event(w)), n)
    }

    fn decide(w: &World, tl: &[LabelEvent<Login, u8>]) -> Option<usize> {
        grant(tl, &READY, &|l| w.listed(l), &|n| w.allows(n))
    }

    /// A grant counts only when the application that stands came from an
    /// allowed account whose id is the recorded one, on the pinned label.
    #[kani::proof]
    #[kani::unwind(8)]
    fn a_grant_is_an_allowed_recorded_pinned_application() {
        let w = world();
        kani::assume(!w.allows_login(UNKNOWN_LOGIN));
        let (tl, n) = timeline(&w);
        let tl = &tl[..n];
        let got = decide(&w, tl);
        kani::cover!(got.is_some() && n == MAX, "a full timeline grants");
        kani::cover!(got.is_none() && n == MAX, "a full timeline does not");
        if let Some(i) = got {
            let e = &tl[i];
            assert!(e.label == READY && e.change == Change::Applied && e.pinned);
            assert!(tl[i + 1..].iter().all(|x| x.label != READY));
            assert!(e.actor.recorded.is_some() && e.actor.carried == e.actor.recorded);
            assert!(w.allows_login(e.actor.login));
        }
    }

    /// A login whose object does not carry the recorded id never matches the
    /// allowed list, and an application of it never grants.
    #[kani::proof]
    #[kani::unwind(8)]
    fn a_doubted_login_never_grants() {
        let w = world();
        kani::assume(!w.allows_login(UNKNOWN_LOGIN));
        let (tl, n) = timeline(&w);
        let tl = &tl[..n];
        if let Some(i) = last_application(tl, &READY) {
            let a = &tl[i].actor;
            let doubted = w.listed(a.login) && (a.carried.is_none() || a.carried != a.recorded);
            kani::cover!(
                doubted && w.allows_login(a.login),
                "an allowed login is doubted"
            );
            if doubted {
                assert!(!w.allows(counted(a, true)));
                assert!(decide(&w, tl).is_none());
            }
        }
    }

    /// An application with the unknown actor never grants: refused by the
    /// pins, or naming nobody. Unless TEHAS_ALLOWED_LABELERS names the
    /// GitHub account `unknown`: see `the_account_unknown_takes_refused_applications`.
    #[kani::proof]
    #[kani::unwind(8)]
    fn the_unknown_actor_never_grants() {
        let w = world();
        kani::assume(!w.allows_login(UNKNOWN_LOGIN));
        let (tl, n) = timeline(&w);
        let tl = &tl[..n];
        if let Some(i) = last_application(tl, &READY) {
            let e = &tl[i];
            let unknown = !e.pinned || e.actor.login == UNKNOWN_LOGIN || e.actor.login == EMPTY;
            kani::cover!(unknown && n == MAX, "the newest application is nobody's");
            if unknown {
                assert!(decide(&w, tl).is_none());
            }
        }
    }

    /// With the account `unknown` on the list, an application the pins
    /// refused grants: the pins' name for nobody is that account's login.
    #[kani::proof]
    #[kani::should_panic]
    #[kani::unwind(8)]
    fn the_account_unknown_takes_refused_applications() {
        let w = world();
        let e = event(&w);
        assert!(!(e.change == Change::Applied && !e.pinned && decide(&w, &[e]).is_some()));
    }

    /// `last_application` is the newest application of the label: who
    /// `events::label_actor` names, taken off since or not.
    #[kani::proof]
    #[kani::unwind(8)]
    fn the_last_application_is_the_newest() {
        let w = world();
        let (tl, n) = timeline(&w);
        let tl = &tl[..n];
        let applies = |e: &LabelEvent<Login, u8>| e.label == READY && e.change == Change::Applied;
        let got = last_application(tl, &READY);
        kani::cover!(
            got.is_some_and(|i| tl[..i].iter().any(applies)),
            "an older application is passed over"
        );
        match got {
            Some(i) => assert!(applies(&tl[i]) && !tl[i + 1..].iter().any(applies)),
            None => assert!(!tl.iter().any(applies)),
        }
    }

    /// `latest` is the newest event of the label: what `events::label_state`
    /// reads as applied or taken off.
    #[kani::proof]
    #[kani::unwind(8)]
    fn the_latest_is_the_newest_event() {
        let w = world();
        let (tl, n) = timeline(&w);
        let tl = &tl[..n];
        let got = latest(tl, &READY);
        kani::cover!(
            got.is_some_and(|i| tl[..i].iter().any(|e| e.label == READY)),
            "an older event is passed over"
        );
        match got {
            Some(i) => {
                assert!(tl[i].label == READY && tl[i + 1..].iter().all(|e| e.label != READY))
            }
            None => assert!(tl.iter().all(|e| e.label != READY)),
        }
    }

    /// `standing` is the newest event of the label when that is an
    /// application, and nothing when the label was taken off since: what
    /// `events::standing_actor` names, and so who the poll credits.
    #[kani::proof]
    #[kani::unwind(8)]
    fn the_standing_application_is_the_newest_event() {
        let w = world();
        let (tl, n) = timeline(&w);
        let tl = &tl[..n];
        let got = standing(tl, &READY);
        let applied_once = tl
            .iter()
            .any(|e| e.label == READY && e.change == Change::Applied);
        kani::cover!(
            got.is_none() && applied_once,
            "an application taken off since stands for nobody"
        );
        kani::cover!(
            got.is_some_and(|i| tl[..i]
                .iter()
                .any(|e| e.label == READY && e.change == Change::Removed)),
            "an application after a removal stands"
        );
        match got {
            Some(i) => {
                assert!(tl[i].label == READY && tl[i].change == Change::Applied);
                assert!(tl[i + 1..].iter().all(|e| e.label != READY));
            }
            None => assert!(latest(tl, &READY).is_none_or(|i| tl[i].change == Change::Removed)),
        }
    }

    /// The poll's steps (the application that stands, then whether its
    /// account is allowed) decide as `grant` does, on every timeline: a
    /// removal the issue's labels do not show yet included.
    #[kani::proof]
    #[kani::unwind(8)]
    fn the_polls_steps_agree_with_grant() {
        let w = world();
        let (tl, n) = timeline(&w);
        let tl = &tl[..n];
        let ends_applied = latest(tl, &READY).is_some_and(|i| tl[i].change == Change::Applied);
        kani::cover!(
            ends_applied && n == MAX,
            "a full timeline ends in an application"
        );
        kani::cover!(
            !ends_applied && last_application(tl, &READY).is_some(),
            "a timeline ends in a removal after an application"
        );
        let steps = standing(tl, &READY)
            .filter(|&i| w.allows(account_of(&tl[i], w.listed(tl[i].actor.login))));
        assert_eq!(steps, decide(&w, tl));
    }

    /// A removal after an application leaves no grant, until the label is
    /// applied again.
    #[kani::proof]
    #[kani::unwind(8)]
    fn a_removal_after_an_application_leaves_no_grant() {
        let w = world();
        let (tl, n) = timeline(&w);
        let tl = &tl[..n];
        let removed = latest(tl, &READY).is_some_and(|i| tl[i].change == Change::Removed);
        let taken_off = removed
            && last_application(tl, &READY).is_some_and(|i| {
                let e = &tl[i];
                e.pinned && w.allows_login(e.actor.login) && e.actor.carried == e.actor.recorded
            });
        kani::cover!(taken_off, "an allowed application was taken off");
        if removed {
            assert!(standing(tl, &READY).is_none());
            assert!(decide(&w, tl).is_none());
        }
    }
}
