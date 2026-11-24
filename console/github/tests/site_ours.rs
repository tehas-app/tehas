//! Which accounts a site's handles answer `Ours`: an account GitHub types a
//! bot that carries the bot's user id the site was given, and no other.
//! Every case reads the account on each path the adapter builds accounts
//! on: REST's issue author, label event actor and assignee, GraphQL's
//! timeline actor, close, assigned event's assignee and list author, the
//! code host's change author, the board's change author, a blueprint's
//! author, body editor, comment author and comment editor, and
//! `Identity::me`.

use std::num::NonZeroU64;
use std::sync::Arc;

use tehas_core::forge::GhRunner;
use tehas_github::{GitHub, GitHubSite, MakeRunner};
use tehas_ports::{
    Account, AccountKind, Actor, BlueprintNo, Bound, Caller, ChangeAsk, ChangeFilter, ChangeStates,
    Closer, Depth, Filter, Happened, HappenedKind, Identity, Scope, Site, TicketKey, Tickets,
    Window, ALL_MAX,
};

const CALLER: Caller = Caller::new("test.ours");
/// The bot's login as the factory keeps it, a line of a file.
const LOGIN: &str = "tehas-bot[bot]\n";
/// The user id of the bot's account.
const BOT_ID: u64 = 900;

/// Answers each call from the list in turn; a call past it fails.
struct Stub(Vec<String>);

impl GhRunner for Stub {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        if self.0.is_empty() {
            return Err("no fixture".to_string());
        }
        Ok(self.0.remove(0))
    }
}

/// An account as a case gives it: its type word, login and id, each of
/// which may be missing.
#[derive(Clone, Copy)]
struct Who {
    is: Option<&'static str>,
    login: &'static str,
    id: Option<u64>,
}

impl Who {
    fn json(&self, type_key: &str, id_key: &str) -> String {
        let mut fields = vec![format!(r#""login":"{}""#, self.login)];
        if let Some(is) = self.is {
            fields.push(format!(r#""{type_key}":"{is}""#));
        }
        if let Some(id) = self.id {
            fields.push(format!(r#""{id_key}":{id}"#));
        }
        format!("{{{}}}", fields.join(","))
    }

    /// REST's spelling of the account.
    fn rest(&self) -> String {
        self.json("type", "id")
    }

    /// GraphQL's spelling of the account.
    fn graphql(&self) -> String {
        self.json("__typename", "databaseId")
    }
}

fn who(is: Option<&'static str>, login: &'static str, id: Option<u64>) -> Who {
    Who { is, login, id }
}

fn site_of(bot_login: &str, bot_id: Option<u64>, answers: Vec<String>) -> GitHubSite {
    let make: MakeRunner =
        Arc::new(move |_: &Actor| Ok(Box::new(Stub(answers.clone())) as Box<dyn GhRunner>));
    GitHubSite::new("acme/widget", bot_login, bot_id, make)
}

fn key(n: u64) -> TicketKey {
    TicketKey::Number(NonZeroU64::new(n).unwrap())
}

fn rest_issue(author: &str, assignee: &str) -> String {
    format!(
        r#"{{"number":7,"title":"t","state":"open","state_reason":null,"labels":[],
"body":"","closed_at":null,"user":{author},"assignees":[{assignee}],
"created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-03T00:00:00Z",
"html_url":"https://github.com/acme/widget/issues/7"}}"#
    )
}

/// The question `Tickets::ticket` asks beside the REST read, of an open
/// issue with nothing beside it.
const BESIDE: &str = r#"{"data":{"repository":{"issue":{"number":7,"closes":{"nodes":[]},
"blockedBy":{"totalCount":0,"nodes":[]},
"subIssues":{"totalCount":0,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[]},
"mentions":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[]}}}}}"#;

fn rest_events(actor: &str) -> String {
    format!(
        r#"[{{"event":"labeled","label":{{"name":"tehas:pause"}},"actor":{actor},
"created_at":"2026-01-02T03:04:05Z"}}]"#
    )
}

fn timeline(actor: &str) -> String {
    format!(
        r#"{{"data":{{"repository":{{"issue":{{"number":7,"milestone":null,"events":{{"pageInfo":{{"hasNextPage":false,"endCursor":null}},"nodes":[
{{"__typename":"LabeledEvent","createdAt":"2026-01-01T00:00:01Z","actor":{actor},"label":{{"name":"bug"}}}},
{{"__typename":"AssignedEvent","createdAt":"2026-01-01T00:00:02Z","actor":null,"assignee":{actor}}},
{{"__typename":"ClosedEvent","createdAt":"2026-01-01T00:00:03Z","actor":{actor},"stateReason":"COMPLETED","closer":null}}]}}}}}}}}}}"#
    )
}

fn list(author: &str) -> String {
    format!(
        r#"{{"data":{{"repository":{{"issues":{{"pageInfo":{{"hasNextPage":false,"endCursor":null}},"nodes":[
{{"number":7,"title":"t","body":"","state":"OPEN","stateReason":null,"url":"https://github.com/acme/widget/issues/7",
"createdAt":"2026-01-02T03:04:05Z","updatedAt":"2026-01-03T00:00:00Z","author":{author},
"issueType":null,"milestone":null,"labels":{{"totalCount":0,"nodes":[]}}}}]}}}}}}}}"#
    )
}

/// Answers every call with the same text.
struct Always(String);

impl GhRunner for Always {
    fn run(&mut self, _: &[&str]) -> Result<String, String> {
        Ok(self.0.clone())
    }
}

fn always_site(bot_id: Option<u64>, answer: String) -> GitHubSite {
    let make: MakeRunner =
        Arc::new(move |_: &Actor| Ok(Box::new(Always(answer.clone())) as Box<dyn GhRunner>));
    GitHubSite::new("acme/widget", LOGIN, bot_id, make)
}

/// A recorded page of pull requests, every account in it `actor`.
fn pulls(file: &str, actor: &str) -> String {
    let at = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/recorded")
        .join(file);
    let mut page: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(at).unwrap()).unwrap();
    let actor: serde_json::Value = serde_json::from_str(actor).unwrap();
    let nodes = page["data"]["repository"]["pullRequests"]["nodes"]
        .as_array_mut()
        .unwrap();
    for n in nodes {
        for k in ["author", "editor", "mergedBy"] {
            if n.get(k).is_some_and(|v| !v.is_null()) {
                n[k] = actor.clone();
            }
        }
    }
    page.to_string()
}

/// A blueprint's REST read: issue 7 by `author`, touched since it was
/// opened, with one comment.
fn bp_issue(author: &str) -> String {
    format!(
        r#"{{"number":7,"title":"t","state":"open","body":"","user":{author},"comments":1,
"created_at":"2026-01-02T03:04:05Z","updated_at":"2026-01-03T00:00:00Z",
"html_url":"https://github.com/acme/widget/issues/7"}}"#
    )
}

/// GraphQL's answer of the body's last edit, by `editor`.
fn bp_body_edit(editor: &str) -> String {
    format!(
        r#"{{"data":{{"repository":{{"issue":{{"lastEditedAt":"2026-01-02T05:00:00Z","editor":{editor}}}}}}}}}"#
    )
}

/// One page of comments: one by `author`, edited since.
fn bp_comments(author: &str) -> String {
    format!(
        r#"[{{"id":11,"node_id":"IC_kwDOAbCdEf11","html_url":"https://github.com/acme/widget/issues/7#issuecomment-11",
"body":"A note.","user":{author},"created_at":"2026-01-02T04:00:00Z","updated_at":"2026-01-02T06:00:00Z"}}]"#
    )
}

/// GraphQL's answer of the comment's last edit, by `editor`.
fn bp_comment_edit(editor: &str) -> String {
    format!(
        r#"{{"data":{{"repository":{{"id":"R_1"}},"nodes":[{{"__typename":"IssueComment","id":"IC_kwDOAbCdEf11","lastEditedAt":"2026-01-02T06:00:00Z","editor":{editor}}}]}}}}"#
    )
}

fn bp_no() -> BlueprintNo {
    BlueprintNo::new(7).unwrap()
}

fn handle(bot_id: Option<u64>, answers: Vec<String>) -> Box<dyn Tickets + Send> {
    site_of(LOGIN, bot_id, answers)
        .tickets(&Actor::Ours(Scope::Full), CALLER)
        .unwrap()
}

/// The kind of the account `w` on every path, each named, read through a
/// site given `bot_id`.
fn kinds(bot_id: Option<u64>, w: Who) -> Vec<(&'static str, Option<AccountKind>)> {
    let (rest, gql) = (w.rest(), w.graphql());
    let kind = |a: Option<&Account>| a.map(|a| a.kind);
    let mut out = Vec::new();

    let mut h = handle(bot_id, vec![rest_issue(&rest, &rest)]);
    let (_, d) = h.several(&[key(7)]).pop().unwrap();
    out.push(("rest author", kind(d.unwrap().author.as_ref())));

    let mut h = handle(bot_id, vec![rest_events(&rest)]);
    let took = h
        .history(
            &key(7),
            Window::Whole(ALL_MAX),
            Some(&[HappenedKind::Marked]),
        )
        .unwrap();
    out.push(("rest event actor", kind(took.items[0].by.as_ref())));

    let mut h = handle(bot_id, vec![rest_issue(&rest, &rest), BESIDE.to_string()]);
    let t = h.ticket(&key(7), Depth::Cut { notes: 0 }, None).unwrap();
    out.push(("rest assignee", kind(t.assignees.first())));

    let mut h = handle(bot_id, vec![timeline(&gql)]);
    let took = h.history(&key(7), Window::Whole(ALL_MAX), None).unwrap();
    out.push(("graphql event actor", kind(took.items[0].by.as_ref())));
    let assignee = match &took.items[1].what {
        Happened::Assigned(a) => Some(a.kind),
        _ => None,
    };
    out.push(("graphql assignee", assignee));
    let closer = match &took.items[2].what {
        Happened::Closed {
            closer: Some(Closer::Account(a)),
            ..
        } => Some(a.kind),
        _ => None,
    };
    out.push(("graphql closer", closer));

    let mut h = handle(bot_id, vec![list(&gql)]);
    let all = h.list(&Filter::default(), Bound::new(10).unwrap()).unwrap();
    out.push(("graphql list author", kind(all.items()[0].author.as_ref())));

    let page = pulls("code-host/widget/changes-OPEN_CLOSED_MERGED.json", &gql);
    let mut h = always_site(bot_id, page)
        .code_host(&Actor::Ours(Scope::Full), CALLER)
        .unwrap();
    let every = ChangeFilter {
        states: ChangeStates::All,
        ..ChangeFilter::default()
    };
    let all = h.changes(&every, Bound::new(50).unwrap()).unwrap();
    out.push((
        "code host change author",
        kind(all.items()[0].author.as_ref()),
    ));

    let page = pulls("board/three-open-bare/pulls-first.json", &gql);
    let mut h = always_site(bot_id, page)
        .board(&Actor::Ours(Scope::Full), CALLER)
        .unwrap();
    let ask = ChangeAsk {
        filter: ChangeFilter::default(),
        files: None,
        closing: false,
        siblings: None,
        activity: None,
    };
    let all = h.open_changes(&ask, Bound::new(50).unwrap()).unwrap();
    out.push((
        "board change author",
        kind(all.items()[0].change.author.as_ref()),
    ));

    let answers = vec![
        bp_issue(&rest),
        bp_body_edit(&gql),
        bp_comments(&rest),
        bp_comment_edit(&gql),
    ];
    let mut h = site_of(LOGIN, bot_id, answers)
        .blueprints(&Actor::Ours(Scope::Full), CALLER)
        .unwrap();
    let b = h.blueprint(bp_no(), Depth::All).unwrap();
    out.push(("blueprint author", kind(b.author.as_ref())));
    out.push(("blueprint body editor", kind(b.editor.as_ref())));
    let c = &b.comments.items[0];
    out.push(("blueprint comment author", kind(c.author.as_ref())));
    out.push(("blueprint comment editor", kind(c.editor.as_ref())));
    out
}

/// `w` is read on every path, and is `Ours` on none.
fn never_ours(bot_id: Option<u64>, w: Who) {
    for (path, kind) in kinds(bot_id, w) {
        assert!(kind.is_some(), "{path}: no account read");
        assert_ne!(kind, Some(AccountKind::Ours), "{path}");
    }
}

/// `w` is `Ours` on every path.
fn always_ours(bot_id: Option<u64>, w: Who) {
    for (path, kind) in kinds(bot_id, w) {
        assert_eq!(kind, Some(AccountKind::Ours), "{path}");
    }
}

#[test]
fn the_factorys_bot_account_is_ours_when_its_id_matches() {
    // REST spells the App `<slug>[bot]`, GraphQL `<slug>`: the same id.
    always_ours(
        Some(BOT_ID),
        who(Some("Bot"), "tehas-bot[bot]", Some(BOT_ID)),
    );
    always_ours(Some(BOT_ID), who(Some("Bot"), "tehas-bot", Some(BOT_ID)));
}

#[test]
fn a_bot_with_the_bots_id_and_another_login_is_ours() {
    always_ours(
        Some(BOT_ID),
        who(Some("Bot"), "renamed-factory", Some(BOT_ID)),
    );
}

#[test]
fn a_bot_with_the_slug_and_another_id_is_not_ours() {
    never_ours(Some(BOT_ID), who(Some("Bot"), "tehas-bot", Some(7)));
    never_ours(Some(BOT_ID), who(Some("Bot"), "tehas-bot[bot]", Some(7)));
}

#[test]
fn a_bot_with_the_slug_and_no_id_is_not_ours() {
    never_ours(Some(BOT_ID), who(Some("Bot"), "tehas-bot", None));
}

#[test]
fn a_user_holding_the_slug_is_not_ours() {
    never_ours(Some(BOT_ID), who(Some("User"), "tehas-bot", Some(BOT_ID)));
    never_ours(Some(BOT_ID), who(Some("User"), "tehas-bot", Some(7)));
}

#[test]
fn an_organization_holding_the_slug_is_not_ours() {
    never_ours(
        Some(BOT_ID),
        who(Some("Organization"), "tehas-bot", Some(BOT_ID)),
    );
    never_ours(Some(BOT_ID), who(Some("Organization"), "tehas-bot", None));
}

#[test]
fn a_mannequin_holding_the_slug_is_not_ours() {
    never_ours(
        Some(BOT_ID),
        who(Some("Mannequin"), "tehas-bot", Some(BOT_ID)),
    );
    never_ours(Some(BOT_ID), who(Some("Mannequin"), "tehas-bot", None));
}

#[test]
fn an_enterprise_user_account_holding_the_slug_is_not_ours() {
    never_ours(
        Some(BOT_ID),
        who(Some("EnterpriseUserAccount"), "tehas-bot", Some(BOT_ID)),
    );
}

#[test]
fn an_untyped_account_holding_the_slug_is_not_ours() {
    never_ours(Some(BOT_ID), who(None, "tehas-bot", Some(BOT_ID)));
    never_ours(Some(BOT_ID), who(None, "tehas-bot[bot]", Some(BOT_ID)));
}

#[test]
fn with_no_bot_id_given_to_the_site_nothing_is_ours() {
    never_ours(None, who(Some("Bot"), "tehas-bot[bot]", Some(BOT_ID)));
    never_ours(None, who(Some("Bot"), "tehas-bot", Some(BOT_ID)));
}

#[test]
fn with_a_bot_id_of_zero_nothing_is_ours() {
    never_ours(Some(0), who(Some("Bot"), "x", Some(0)));
    never_ours(Some(0), who(Some("Bot"), "tehas-bot[bot]", Some(0)));
}

#[test]
fn a_bot_login_is_trimmed_and_one_holding_whitespace_names_nobody() {
    let bot = who(Some("Bot"), "tehas-bot[bot]", Some(BOT_ID)).rest();
    let author = |login: &str| {
        let s = site_of(login, Some(BOT_ID), vec![rest_issue(&bot, &bot)]);
        let mut h = s.tickets(&Actor::Ours(Scope::Full), CALLER).unwrap();
        let (_, d) = h.several(&[key(7)]).pop().unwrap();
        d.unwrap().author.unwrap().kind
    };
    assert_eq!(author("tehas-bot[bot]\n"), AccountKind::Ours);
    assert_eq!(author("  tehas-bot[bot]\r\n"), AccountKind::Ours);
    for login in [
        "tehas bot[bot]",
        "tehas-bot[bot]\nother[bot]",
        "",
        "\n",
        "[bot]",
        "app/",
    ] {
        assert_eq!(author(login), AccountKind::Bot, "{login:?}");
    }
}

#[test]
fn a_handle_made_without_a_site_never_answers_ours() {
    let bot = who(Some("Bot"), "tehas-bot[bot]", Some(BOT_ID)).rest();
    let mut gh = GitHub::new(
        "acme/widget",
        Box::new(Stub(vec![rest_issue(&bot, &bot), rest_events(&bot)])),
    );
    let (_, d) = gh.several(&[key(7)]).pop().unwrap();
    assert_eq!(d.unwrap().author.unwrap().kind, AccountKind::Bot);
    let took = Tickets::history(
        &mut gh,
        &key(7),
        Window::Whole(ALL_MAX),
        Some(&[HappenedKind::Marked]),
    )
    .unwrap();
    assert_eq!(took.items[0].by.as_ref().unwrap().kind, AccountKind::Bot);
}

/// What `me` answers through the site's identity handle.
fn me(bot_id: Option<u64>, viewer: &str, user: &str) -> AccountKind {
    let s = site_of(LOGIN, bot_id, vec![viewer.to_string(), user.to_string()]);
    let mut h = s.identity(&Actor::Ours(Scope::Full), CALLER).unwrap();
    h.me().unwrap().kind
}

const VIEWER_BOT: &str = r#"{"data":{"viewer":{"login":"tehas-bot","databaseId":900}}}"#;
const USER_BOT: &str = r#"{"login":"tehas-bot[bot]","id":900,"type":"Bot"}"#;

#[test]
fn me_answers_the_factorys_bot_account_ours_when_its_id_matches() {
    assert_eq!(me(Some(BOT_ID), VIEWER_BOT, USER_BOT), AccountKind::Ours);
}

#[test]
fn me_with_another_id_or_no_bot_id_given_is_not_ours() {
    assert_eq!(me(Some(7), VIEWER_BOT, USER_BOT), AccountKind::Bot);
    assert_eq!(me(None, VIEWER_BOT, USER_BOT), AccountKind::Bot);
    let viewer = r#"{"data":{"viewer":{"login":"tehas-bot","databaseId":900}}}"#;
    let user = r#"{"login":"tehas-bot","id":900,"type":"User"}"#;
    assert_eq!(me(Some(BOT_ID), viewer, user), AccountKind::Person);
}

#[test]
fn me_through_a_handle_made_without_a_site_is_never_ours() {
    let mut gh = GitHub::new(
        "acme/widget",
        Box::new(Stub(vec![VIEWER_BOT.to_string(), USER_BOT.to_string()])),
    );
    assert_eq!(Identity::me(&mut gh).unwrap().kind, AccountKind::Bot);
}

#[test]
fn an_edit_by_a_deleted_account_is_never_ours() {
    // GraphQL names no editor of an edited text: GitHub's `ghost`, with no
    // id, so the bot rule never matches it.
    let bot = who(Some("Bot"), "tehas-bot[bot]", Some(BOT_ID));
    let answers = vec![bp_issue(&bot.rest()), bp_body_edit("null")];
    let mut h = site_of(LOGIN, Some(BOT_ID), answers)
        .blueprints(&Actor::Ours(Scope::Full), CALLER)
        .unwrap();
    let b = h.blueprint(bp_no(), Depth::Cut { notes: 0 }).unwrap();
    assert_eq!(b.author.unwrap().kind, AccountKind::Ours);
    let editor = b.editor.unwrap();
    assert_eq!(editor.login.as_str(), "ghost");
    assert_eq!(editor.kind, AccountKind::Unknown);
    assert!(b.edited.is_some());
}
