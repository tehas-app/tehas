//! The site takes the factory's login from its caller alone: a home whose
//! state names another bot, and the variables that once named one, change
//! nothing. A file of its own, since it sets the process's variables.

use std::num::NonZeroU64;
use std::sync::Arc;

use tehas_core::forge::GhRunner;
use tehas_core::home::{Home, State};
use tehas_github::{GitHubSite, MakeRunner};
use tehas_ports::{AccountKind, Actor, Caller, Scope, Site, TicketKey};

struct Stub(Vec<String>);

impl GhRunner for Stub {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        Ok(self.0.remove(0))
    }
}

fn issue(login: &str) -> String {
    format!(
        r#"{{"number":7,"title":"t","state":"open","labels":[],
"user":{{"login":"{login}","id":42,"type":"Bot"}},
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

fn site(answer: String) -> GitHubSite {
    let make: MakeRunner =
        Arc::new(move |_: &Actor| Ok(Box::new(Stub(vec![answer.clone()])) as Box<dyn GhRunner>));
    GitHubSite::new("acme/widget", "tehas-bot[bot]", make)
}

#[test]
fn the_bot_login_comes_from_the_caller_not_from_disk_or_the_environment() {
    let home = std::env::temp_dir().join(format!("tehas-github-site-env-{}", std::process::id()));
    let login = Home::new(&home).state(State::BotLogin);
    std::fs::create_dir_all(login.parent().unwrap()).unwrap();
    std::fs::write(&login, "impostor[bot]\n").unwrap();
    std::env::set_var("TEHAS_HOME", &home);
    std::env::set_var("TEHAS_BOT_LOGIN", "impostor[bot]");
    std::env::set_var("TEHAS_CYCLE_LOGIN", "impostor[bot]");
    let given = author_kind(&site(issue("tehas-bot[bot]")));
    let kept = author_kind(&site(issue("impostor[bot]")));
    std::fs::remove_dir_all(&home).unwrap();
    assert_eq!(given, AccountKind::Ours);
    assert_eq!(kept, AccountKind::Bot);
}
