//! The vouch over the reads of `ticket.rs`: a listed login that does not
//! come with its recorded user id is no longer that login, in every place
//! a read names an account. One test alone in this file, since the vouch
//! reads the process's environment.

mod replay;

use std::num::NonZeroU64;
use tehas_ports::{Bound, Depth, Filter, Happened, TicketKey, Tickets, Window, ALL_MAX};

#[test]
fn a_listed_login_without_its_recorded_id_is_vouched_for_nowhere() {
    let home = std::env::temp_dir().join(format!("tehas-github-vouch-{}", std::process::id()));
    std::fs::create_dir_all(home.join("state")).unwrap();
    // alice is listed with another id than the recordings carry; bob with
    // his own.
    std::fs::write(home.join("state/account-ids"), "alice 999\nbob 102\n").unwrap();
    std::env::set_var("TEHAS_ALLOWED_LABELERS", "Alice bob");
    std::env::set_var("TEHAS_MILLWRIGHT_LOGIN", "");
    std::env::set_var("TEHAS_HOME", &home);

    let key = TicketKey::Number(NonZeroU64::new(20).unwrap());
    let mut gh = replay::adapter_of(replay::PROJECT).0;
    let ticket = gh.ticket(&key, Depth::All, None).unwrap();
    let history = gh.history(&key, Window::Whole(ALL_MAX), None).unwrap();
    let list = gh
        .list(&Filter::default(), Bound::new(50).unwrap())
        .unwrap();
    std::fs::remove_dir_all(&home).unwrap();

    let mut logins: Vec<String> = Vec::new();
    let mut seen = |a: &Option<tehas_ports::Account>| {
        logins.extend(a.iter().map(|a| a.login.as_str().to_string()));
    };
    seen(&ticket.digest.author);
    ticket.assignees.iter().for_each(|a| seen(&Some(a.clone())));
    for e in ticket.history.items.iter().chain(&history.items) {
        seen(&e.by);
        if let Happened::Assigned(a) = &e.what {
            seen(&Some(a.clone()));
        }
    }
    list.items().iter().for_each(|d| seen(&d.author));
    assert!(logins.len() > 20, "{logins:?}");
    assert!(!logins.iter().any(|l| l == "alice"), "{logins:?}");
    assert!(logins.iter().any(|l| l.starts_with("alice#")), "{logins:?}");
    // bob came with his recorded id, and is bob.
    assert!(logins.iter().any(|l| l == "bob"), "{logins:?}");
}
