//! `Cites::link` as the adapter answers it: with no request, the page each
//! item's own read gives, and nothing for an account, from the recordings under `tests/recorded/` and
//! the pull request sample of `tests/samples/change-read/`.

mod replay;

use replay::{adapter_of, PROJECT};
use std::num::NonZeroU64;
use std::path::PathBuf;
use tehas_ports::{
    BlueprintNo, ChangeNo, Cite, Cites, Depth, ItemName, KeyText, Login, Mark, ProjectName, Ref,
    TicketKey, Tickets, WebLink,
};

fn ticket(n: u64) -> Cite {
    Cite::Ticket(Ref::Here(TicketKey::Number(NonZeroU64::new(n).unwrap())))
}

fn blueprint(n: u64) -> Cite {
    Cite::Blueprint(Ref::Here(BlueprintNo::new(n).unwrap()))
}

fn change(n: u64) -> Cite {
    Cite::Change(Ref::Here(ChangeNo::new(n).unwrap()))
}

/// The `html_url` of a recording or sample, as GitHub gave it.
fn html_url(rel: &str) -> WebLink {
    let at = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(rel);
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(at).unwrap()).unwrap();
    WebLink::new(doc["html_url"].as_str().unwrap()).unwrap()
}

#[test]
fn a_ticket_and_a_blueprint_link_to_the_issue_its_read_gives_and_ask_nothing() {
    let (mut gh, asked) = adapter_of(PROJECT);
    let linked = [gh.link(&ticket(20)), gh.link(&blueprint(20))];
    assert_eq!(asked.get(), 0, "a link asks GitHub nothing");
    let read = gh
        .ticket(
            &TicketKey::Number(NonZeroU64::new(20).unwrap()),
            Depth::Cut { notes: 0 },
            None,
        )
        .unwrap();
    assert_eq!(
        linked,
        [Some(read.digest.link.clone()), Some(read.digest.link)]
    );
}

#[test]
fn a_change_links_to_its_pull_request() {
    let (gh, asked) = adapter_of(PROJECT);
    assert_eq!(
        gh.link(&change(9)),
        Some(html_url("samples/change-read/pull-9.json"))
    );
    assert_eq!(asked.get(), 0);
}

#[test]
fn what_is_not_an_item_of_this_repository_links_nothing() {
    let (gh, _) = adapter_of(PROJECT);
    let elsewhere = Cite::Ticket(Ref::Elsewhere {
        project: ProjectName::new("acme/other"),
        item: ItemName::new("3"),
    });
    let key = Cite::Ticket(Ref::Here(TicketKey::Key(KeyText::new("WID-7").unwrap())));
    // A login says not whether it is a person's or an App's, whose page is
    // another: a person, and a bot by its slug or its full login.
    let accounts = ["carol", "tehas-bot", "tehas-bot[bot]"].map(|l| Cite::Account(Login::new(l)));
    for what in [elsewhere, key, Cite::Mark(Mark::Epic)]
        .into_iter()
        .chain(accounts)
    {
        assert_eq!(gh.link(&what), None, "{what:?}");
    }
}

#[test]
fn a_repository_that_is_not_owner_and_name_links_nothing() {
    for repo in ["widget", "acme/widget/extra", "acme/..", "-acme/widget"] {
        let (gh, _) = adapter_of(repo);
        for what in [ticket(20), blueprint(20), change(9)] {
            assert_eq!(gh.link(&what), None, "{repo}: {what:?}");
        }
    }
}
