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
    let ids = home.join(tehas_core::accounts::FILE);
    std::fs::create_dir_all(ids.parent().unwrap()).unwrap();
    // janit is listed with another id than the recordings carry.
    std::fs::write(&ids, "janit 999\n").unwrap();
    std::env::set_var("TEHAS_ALLOWED_LABELERS", "janit");
    std::env::set_var("TEHAS_MILLWRIGHT_LOGIN", "");
    std::env::set_var("TEHAS_HOME", &home);

    let key = TicketKey::Number(NonZeroU64::new(27).unwrap());
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
    // The recorded world's one account, janit, acted with the id 198965
    // and the file said 999: it reads tagged, in every place, and no
    // read says plain `janit` anymore.
    assert!(!logins.iter().any(|l| l == "janit"), "{logins:?}");
    assert!(logins.iter().all(|l| l == "janit#198965"), "{logins:?}");
}
