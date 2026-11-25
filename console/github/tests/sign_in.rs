//! A person's sign-in through the factory's client for people:
//! `Identity::sign_in_link` and `signed_in` on a handle of the factory's
//! account at read scope (Decided 76). The code is traded through a test's
//! GitHub in place of curl; no test calls GitHub, and no handle made for a
//! sign-in asks the wiring for a runner of gh.

use std::sync::{Arc, Mutex};
use tehas_core::access::{self, Permission, Person};
use tehas_github::{GitHubSite, MakeRunner, PeopleClient};
use tehas_ports::{
    AccountKind, Actor, Caller, Got, Outcome, Passing, Refusal, Scope, Secret, Sent, SignInCode,
    SignInState, Site, Unread, WebLink,
};

const CALLER: Caller = Caller::new("test.sign-in");
const READ: Actor = Actor::Ours(Scope::Read);

/// A wiring that counts the runners it was asked for and makes none.
fn maker() -> (MakeRunner, Arc<Mutex<u32>>) {
    let asked = Arc::new(Mutex::new(0));
    let count = Arc::clone(&asked);
    let make: MakeRunner = Arc::new(move |_: &Actor| {
        *count.lock().unwrap() += 1;
        Err(Outcome::Refused(Refusal::Unsupported))
    });
    (make, asked)
}

fn client() -> PeopleClient {
    PeopleClient {
        id: "Iv1.client".into(),
        secret: Secret::new("s3cret"),
        callback: WebLink::new("https://plugins.example.org/_auth/callback").unwrap(),
    }
}

/// GitHub trading `code` for `token`, which is the person 4021 `Carol`.
struct Trade {
    took: Mutex<Vec<String>>,
    token: Got<String>,
    user: Got<Person>,
}

impl access::GitHub for Trade {
    fn token(&self, code: &str) -> Got<String> {
        self.took.lock().unwrap().push(format!("code {code}"));
        self.token.clone()
    }
    fn user(&self, token: &str) -> Got<Person> {
        self.took.lock().unwrap().push(format!("user {token}"));
        self.user.clone()
    }
    fn permission(&self, _: &str) -> Got<Permission> {
        panic!("a sign-in reads no permission")
    }
    fn public(&self) -> Got<bool> {
        panic!("a sign-in reads no repository")
    }
}

fn carol() -> Trade {
    Trade {
        took: Mutex::new(Vec::new()),
        token: Ok("gho_person".into()),
        user: Ok(Person {
            id: access::AccountId::new("4021"),
            login: "Carol".into(),
        }),
    }
}

fn site_with(trade: Trade) -> (GitHubSite, Arc<Trade>, Arc<Mutex<u32>>) {
    let (make, asked) = maker();
    let trade = Arc::new(trade);
    let site = GitHubSite::new("acme/widget", "tehas-bot[bot]", Some(900), make)
        .with_people_trading(client(), Arc::clone(&trade) as Arc<dyn access::GitHub>);
    (site, trade, asked)
}

#[test]
fn the_link_is_githubs_page_for_the_client_carrying_the_callback_and_the_state() {
    let (site, trade, asked) = site_with(carol());
    let id = site.identity(&READ, CALLER).unwrap();
    let link = id.sign_in_link(&SignInState::new("st/ate 1")).unwrap();
    assert_eq!(
        link.as_str(),
        "https://github.com/login/oauth/authorize?client_id=Iv1.client\
         &redirect_uri=https%3A%2F%2Fplugins.example.org%2F_auth%2Fcallback\
         &state=st%2Fate%201&allow_signup=false"
    );
    assert!(
        trade.took.lock().unwrap().is_empty(),
        "a link trades nothing"
    );
    assert_eq!(*asked.lock().unwrap(), 0, "a sign-in's handle runs no gh");
}

#[test]
fn a_code_is_traded_for_the_person_and_their_token() {
    let (site, trade, asked) = site_with(carol());
    let mut id = site.identity(&READ, CALLER).unwrap();
    let (who, token) = id.signed_in(&SignInCode::new("c0de")).unwrap();
    assert_eq!(who.login.as_str(), "carol");
    assert_eq!(who.id.as_ref().map(|i| i.as_str()), Some("4021"));
    assert_eq!(who.kind, AccountKind::Person);
    assert_eq!(token.expose(), "gho_person");
    assert_eq!(
        *trade.took.lock().unwrap(),
        ["code c0de", "user gho_person"]
    );
    assert_eq!(*asked.lock().unwrap(), 0);
}

#[test]
fn a_code_github_does_not_take_is_forbidden_and_a_person_unread_is_unavailable() {
    let mut refused = carol();
    refused.token = Err(Outcome::Forbidden);
    let (site, trade, _) = site_with(refused);
    let mut id = site.identity(&READ, CALLER).unwrap();
    assert_eq!(
        id.signed_in(&SignInCode::new("old")).err(),
        Some(Outcome::Forbidden)
    );
    assert_eq!(*trade.took.lock().unwrap(), ["code old"]);

    let mut unread = carol();
    unread.user = Err(Outcome::Unavailable {
        why: Passing::Server,
        sent: Sent::Maybe,
    });
    let (site, _, _) = site_with(unread);
    let mut id = site.identity(&READ, CALLER).unwrap();
    assert_eq!(
        id.signed_in(&SignInCode::new("c0de")).err(),
        Some(Outcome::Unavailable {
            why: Passing::Server,
            sent: Sent::No,
        })
    );
}

#[test]
fn a_code_that_is_not_githubs_shape_is_invalid_and_trades_nothing() {
    let (site, trade, _) = site_with(carol());
    let mut id = site.identity(&READ, CALLER).unwrap();
    let long = "a".repeat(65);
    for code in [
        "",
        "a\"\nurl = \"https://evil\"",
        "ab\0cd",
        "a b",
        "a/b",
        "a%0A",
        long.as_str(),
    ] {
        assert_eq!(
            id.signed_in(&SignInCode::new(code)).err(),
            Some(Outcome::Refused(Refusal::Invalid)),
            "{code:?}"
        );
    }
    assert!(trade.took.lock().unwrap().is_empty());
    let longest = "a".repeat(64);
    for code in ["0123456789abcdef0123", "a_b-C", longest.as_str()] {
        assert!(id.signed_in(&SignInCode::new(code)).is_ok(), "{code:?}");
    }
}

#[test]
fn a_trade_that_failed_but_for_a_refusal_may_have_spent_the_code() {
    // What passed is kept; any other failure is GitHub's own. Each may
    // have spent the code.
    for (failed, why) in [
        (
            Outcome::Unavailable {
                why: Passing::Unreachable,
                sent: Sent::No,
            },
            Passing::Unreachable,
        ),
        (
            Outcome::Unavailable {
                why: Passing::Timeout,
                sent: Sent::Maybe,
            },
            Passing::Timeout,
        ),
        (Outcome::Unread(Unread::Malformed), Passing::Server),
        (Outcome::SignedOut, Passing::Server),
        (Outcome::NotFound, Passing::Server),
    ] {
        let mut lost = carol();
        lost.token = Err(failed.clone());
        let (site, _, _) = site_with(lost);
        let mut id = site.identity(&READ, CALLER).unwrap();
        assert_eq!(
            id.signed_in(&SignInCode::new("c0de")).err(),
            Some(Outcome::Unavailable {
                why,
                sent: Sent::Maybe,
            }),
            "{failed:?}"
        );
    }
}

#[test]
fn the_sign_in_handle_asks_nothing_as_the_factory() {
    let (site, _, asked) = site_with(carol());
    let mut id = site.identity(&READ, CALLER).unwrap();
    assert!(id.me().is_err());
    assert_eq!(*asked.lock().unwrap(), 0);
}

#[test]
fn read_scope_is_still_refused_for_every_other_port_and_for_a_write() {
    let (site, _, asked) = site_with(carol());
    let unsupported = Some(Outcome::Refused(Refusal::Unsupported));
    assert_eq!(site.tickets(&READ, CALLER).err(), unsupported);
    assert_eq!(site.code_host(&READ, CALLER).err(), unsupported);
    assert_eq!(site.board(&READ, CALLER).err(), unsupported);
    assert_eq!(*asked.lock().unwrap(), 0);
}

#[test]
fn a_site_given_no_client_signs_nobody_in() {
    let (make, _) = maker();
    let site = GitHubSite::new("acme/widget", "tehas-bot[bot]", Some(900), make);
    let unsupported = Some(Outcome::Refused(Refusal::Unsupported));
    assert_eq!(site.identity(&READ, CALLER).err(), unsupported);
}
