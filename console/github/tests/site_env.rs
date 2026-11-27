//! The site takes the factory's bot login and user id from its caller
//! alone: a home whose state names another bot and its id, and the
//! variables that once named one, change nothing. A file of its own, since it sets the process's variables.

use std::num::NonZeroU64;
use std::sync::Arc;

use tehas_core::forge::GhRunner;
use tehas_core::home::{Home, State};
use tehas_core::testdir::TestDir;
use tehas_github::{GitHubSite, MakeRunner};
use tehas_ports::{AccountKind, Actor, Caller, Scope, Site, TicketKey};

struct Stub(Vec<String>);

impl GhRunner for Stub {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        Ok(self.0.remove(0))
    }
}

fn issue(login: &str, id: u64) -> String {
    format!(
        r#"{{"number":7,"title":"t","state":"open","labels":[],
"user":{{"login":"{login}","id":{id},"type":"Bot"}},
"created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-03T00:00:00Z",
"html_url":"https://github.com/acme/widget/issues/7"}}"#
    )
}

fn author_kind(site: &GitHubSite) -> AccountKind {
    let mut h = site
        .tickets(&Actor::Ours(Scope::Full), Caller::new("test.env"))
        .unwrap();
    let key = TicketKey::Number(NonZeroU64::new(7).unwrap());
    let (_, d) = h.several(&[key]).pop().unwrap();
    d.unwrap().author.unwrap().kind
}

/// The answer of the question of the ticket #7 is a part of: none.
const NO_PARENT: &str = r#"{"data":{"repository":{"n7":{"number":7,"parent":null}}}}"#;

fn site(answer: String) -> GitHubSite {
    let make: MakeRunner = Arc::new(move |_: &Actor| {
        Ok(Box::new(Stub(vec![answer.clone(), NO_PARENT.to_string()])) as Box<dyn GhRunner>)
    });
    GitHubSite::new("acme/widget", "tehas-bot[bot]", Some(42), make)
}

#[test]
fn the_bot_login_comes_from_the_caller_not_from_disk_or_the_environment() {
    let dir = TestDir::new("github-site-env");
    let home = dir.to_path_buf();
    let login = Home::new(&home).state(State::BotLogin);
    std::fs::create_dir_all(login.parent().unwrap()).unwrap();
    std::fs::write(&login, "impostor[bot]\n").unwrap();
    std::fs::write(Home::new(&home).state(State::AccountIds), "impostor 43\n").unwrap();
    std::env::set_var("TEHAS_HOME", &home);
    std::env::set_var("TEHAS_BOT_LOGIN", "impostor[bot]");
    std::env::set_var("TEHAS_CYCLE_LOGIN", "impostor[bot]");
    let given = author_kind(&site(issue("tehas-bot[bot]", 42)));
    let kept = author_kind(&site(issue("impostor[bot]", 43)));
    assert_eq!(given, AccountKind::Ours);
    assert_eq!(kept, AccountKind::Bot);
}
