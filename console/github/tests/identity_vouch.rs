//! The vouch over `Identity::me`: a listed login that does not come with its
//! recorded user id is no longer that login, and one that does keeps it,
//! in either of GitHub's spellings of an App's account. One test alone in
//! this file, since the vouch reads the process's environment.

use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::Identity;

/// The viewer answer given, then the account read of its id: a bot for an
/// App's id, else a user.
struct Viewer(&'static str);

impl GhRunner for Viewer {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        match args {
            ["api", path] => {
                let id = path.strip_prefix("user/").expect("an account read");
                let kind = if id.starts_with("90") { "Bot" } else { "User" };
                Ok(format!(r#"{{"login":"x","id":{id},"type":"{kind}"}}"#))
            }
            _ => Ok(self.0.to_string()),
        }
    }
}

fn me(answer: &'static str) -> String {
    let mut gh = GitHub::new("acme/widget", Box::new(Viewer(answer)));
    gh.me().unwrap().login.as_str().to_string()
}

#[test]
fn a_listed_login_without_its_recorded_id_is_not_that_login() {
    let home = std::env::temp_dir().join(format!("tehas-github-me-{}", std::process::id()));
    let ids = home.join(tehas_core::accounts::FILE);
    std::fs::create_dir_all(ids.parent().unwrap()).unwrap();
    std::fs::write(&ids, "alice 999\ntehas-bot 900\n").unwrap();
    std::env::set_var("TEHAS_ALLOWED_LABELERS", "alice tehas-bot");
    std::env::set_var("TEHAS_MILLWRIGHT_LOGIN", "");
    std::env::set_var("TEHAS_HOME", &home);

    let alice = me(r#"{"data":{"viewer":{"login":"Alice","databaseId":101}}}"#);
    let rest = me(r#"{"data":{"viewer":{"login":"tehas-bot[bot]","databaseId":900}}}"#);
    let graphql = me(r#"{"data":{"viewer":{"login":"tehas-bot","databaseId":900}}}"#);
    let other = me(r#"{"data":{"viewer":{"login":"tehas-bot[bot]","databaseId":901}}}"#);
    std::fs::remove_dir_all(&home).unwrap();

    assert!(alice.starts_with("alice#"), "{alice}");
    assert_eq!(rest, "tehas-bot");
    assert_eq!(graphql, "tehas-bot");
    assert!(other.starts_with("tehas-bot#"), "{other}");
}
