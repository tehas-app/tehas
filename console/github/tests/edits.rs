//! `Blueprints::body_edits`, `comment_edits` and their plurals as the
//! adapter answers them, replayed from its recordings of the fixture
//! project under `tests/recorded/edits/<state>/`, made by hand in the shape
//! a read-only answer of a public repository gave. A state holds what
//! differs from `site/`: `issue-<n>.json` and `node-<id>.json` are an
//! item's part of a GraphQL answer, `.silent` a null part, `.null` a null
//! part GitHub says it cannot find, `.forbidden` one it refuses; an item
//! with no recording is one GitHub cannot find. `bodies.*` and `nodes.*`
//! answer a whole question; `comment-<id>.*` is a comment's REST read.
//! `tests/synthetic/edits/garbled/` holds the answers that are not JSON.

mod answers;

use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tehas_core::forge::{Failed, GhRunner};
use tehas_github::GitHub;
use tehas_ports::laws;
use tehas_ports::{
    At, BlueprintNo, Blueprints, Bound, CommentId, Edit, Got, Marker, Outcome, Passing, Pool, Sent,
    Unread, Whole, ALL_MAX,
};

const PROJECT: &str = "acme/widget";
const EDITED: u64 = 2_501_240_001;
const PLAIN: u64 = 2_501_240_002;
const EDITED_NODE: &str = "IC_kwDOAbCdEf6VEd0001";

type Asked = Arc<Mutex<Vec<String>>>;

/// The recording `name` of the state, else of `site/`, by extension.
fn recording(state: &str, name: &str) -> Option<(String, String)> {
    for dir in [state, "site"] {
        for ext in ["json", "silent", "null", "forbidden", "fail"] {
            if let Ok(text) =
                std::fs::read_to_string(answers::at("edits", dir, &format!("{name}.{ext}")))
            {
                return Some((ext.to_string(), text));
            }
        }
    }
    None
}

/// The words of `text` after each `open`, up to the next `close`.
fn all_between<'a>(text: &'a str, open: &str, close: &str) -> Vec<&'a str> {
    text.match_indices(open)
        .filter_map(|(i, _)| {
            let rest = &text[i + open.len()..];
            rest.find(close).map(|j| &rest[..j])
        })
        .collect()
}

struct Replay {
    state: String,
    asked: Asked,
}

impl Replay {
    /// An answer composed of the items' parts, with GitHub's error for
    /// each it does not give.
    /// `items` are each part's path, as a JSON list, and its recording.
    fn compose(&self, items: Vec<(String, String)>, first: usize, data: &str) -> String {
        let mut data: Value = serde_json::from_str(data).unwrap();
        let mut errors = Vec::new();
        for (path, name) in items {
            let path: Vec<Value> = serde_json::from_str(&path).unwrap();
            let part = match recording(&self.state, &name) {
                Some((ext, text)) if ext == "json" => {
                    let mut v: Value = serde_json::from_str(&text).unwrap();
                    if let Some(n) = v.pointer_mut("/userContentEdits/nodes") {
                        n.as_array_mut().unwrap().truncate(first);
                    }
                    v
                }
                Some((ext, _)) if ext == "silent" => Value::Null,
                Some((ext, _)) if ext == "forbidden" => {
                    errors.push(json!({"type": "FORBIDDEN", "path": path, "message": "refused"}));
                    Value::Null
                }
                _ => {
                    errors.push(
                        json!({"type": "NOT_FOUND", "path": path, "message": "Could not resolve"}),
                    );
                    Value::Null
                }
            };
            let mut at = &mut data;
            for step in &path[..path.len() - 1] {
                at = &mut at[step.as_str().unwrap()];
            }
            match &path[path.len() - 1] {
                Value::String(k) => at[k.as_str()] = part,
                Value::Number(i) => at[i.as_u64().unwrap() as usize] = part,
                _ => unreachable!(),
            }
        }
        let mut answer = json!({ "data": data });
        if !errors.is_empty() {
            answer["errors"] = Value::Array(errors);
        }
        answer.to_string()
    }

    fn graphql(&self, query: &str) -> Result<String, Failed> {
        let first = all_between(query, "userContentEdits(first: ", ")")
            .first()
            .map(|n| n.parse().unwrap())
            .unwrap_or(100);
        let (whole, answer) = if query.contains("nodes(ids:") {
            let ids: Vec<String> = all_between(query, "nodes(ids: [", "]")[0]
                .split(", ")
                .map(|i| i.trim_matches('"').to_string())
                .collect();
            let items = ids
                .iter()
                .enumerate()
                .map(|(i, id)| (json!(["nodes", i]).to_string(), format!("node-{id}")))
                .collect();
            let data = json!({"repository": {"id": "R_kgDOAbCdEf"}, "nodes": vec![Value::Null; ids.len()]});
            let mut answer: Value =
                serde_json::from_str(&self.compose(items, first, &data.to_string())).unwrap();
            if !query.contains("userContentEdits") {
                // The editors' question of a read of comments.
                for n in answer["data"]["nodes"].as_array_mut().unwrap() {
                    let editor = n["userContentEdits"]["nodes"][0]["editor"].clone();
                    let kept = json!({"__typename": n["__typename"], "id": n["id"], "lastEditedAt": n["lastEditedAt"], "editor": editor});
                    *n = kept;
                }
            }
            ("nodes", answer.to_string())
        } else {
            let items = all_between(query, ": issue(number: ", ")")
                .into_iter()
                .map(|n| {
                    (
                        json!(["repository", format!("i{n}")]).to_string(),
                        format!("issue-{n}"),
                    )
                })
                .collect();
            (
                "bodies",
                self.compose(items, first, r#"{"repository": {}}"#),
            )
        };
        let read = |ext: &str| {
            std::fs::read_to_string(answers::at("edits", &self.state, &format!("{whole}.{ext}")))
        };
        if let Ok(out) = read("json") {
            return self.exit(out);
        }
        if let Ok(why) = read("fail") {
            return Err(Failed {
                why: why.trim_end().to_string(),
                answer: None,
            });
        }
        self.exit(answer)
    }

    /// gh's exit: 1 on any GraphQL `errors`, with the answer kept.
    fn exit(&self, out: String) -> Result<String, Failed> {
        let errors = serde_json::from_str::<Value>(&out)
            .ok()
            .is_some_and(|v| v.get("errors").is_some());
        if errors {
            return Err(Failed {
                why: "gh: Could not resolve".to_string(),
                answer: Some(out),
            });
        }
        Ok(out)
    }

    fn rest(&self, path: &str) -> Result<String, Failed> {
        let rest = path.strip_prefix("repos/acme/widget/issues/").unwrap();
        let name = if let Some(id) = rest.strip_prefix("comments/") {
            format!("comment-{id}")
        } else if let Some(n) = rest.strip_suffix("/comments?per_page=100&page=1") {
            format!("comments-{n}")
        } else {
            format!("rest-issue-{rest}")
        };
        let failed = |why: &str| Failed {
            why: why.trim_end().to_string(),
            answer: None,
        };
        match recording(&self.state, &name) {
            Some((ext, text)) if ext == "json" => Ok(text),
            Some((ext, why)) if ext == "fail" => Err(failed(&why)),
            _ => Err(failed("gh: Not Found (HTTP 404)")),
        }
    }
}

impl GhRunner for Replay {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        self.run_answer(args).map_err(|f| f.why)
    }

    fn run_answer(&mut self, args: &[&str]) -> Result<String, Failed> {
        self.asked.lock().unwrap().push(args.join(" "));
        match args {
            ["api", "graphql", "-F", "owner=acme", "-F", "name=widget", "-f", query] => {
                self.graphql(query.strip_prefix("query=").unwrap())
            }
            ["api", path] => self.rest(path),
            _ => panic!("no recording of {args:?}"),
        }
    }
}

fn replay_now() -> i64 {
    1_767_225_600
}

fn adapter(state: &str) -> (GitHub, Asked) {
    let asked = Asked::default();
    let replay = Replay {
        state: state.to_string(),
        asked: Arc::clone(&asked),
    };
    (
        GitHub::new(PROJECT, Box::new(replay)).on_clock(replay_now),
        asked,
    )
}

fn bp(n: u64) -> BlueprintNo {
    BlueprintNo::new(n).unwrap()
}

fn cid(n: u64) -> CommentId {
    CommentId::new(n.to_string())
}

fn one() -> Bound {
    Bound::new(1).unwrap()
}

fn questions(asked: &Asked) -> Vec<String> {
    asked
        .lock()
        .unwrap()
        .iter()
        .filter(|a| a.starts_with("api graphql"))
        .cloned()
        .collect()
}

fn rest_reads(asked: &Asked) -> usize {
    asked
        .lock()
        .unwrap()
        .iter()
        .filter(|a| a.starts_with("api repos/acme/widget/issues/comments/"))
        .count()
}

fn texts(got: &Got<Whole<Edit>>) -> Vec<String> {
    got.as_ref()
        .unwrap()
        .items()
        .iter()
        .map(|e| e.after.clone())
        .collect()
}

fn logins(got: &Got<Whole<Edit>>) -> Vec<String> {
    got.as_ref()
        .unwrap()
        .items()
        .iter()
        .map(|e| e.by.as_ref().unwrap().login.as_str().to_string())
        .collect()
}

fn inconsistent<T>() -> Got<T> {
    Err(Outcome::Unread(Unread::Inconsistent))
}

#[test]
fn several_edits_are_each_ones_at_every_bound() {
    let nos = [bp(7), bp(8), bp(9999), bp(7), bp(9)];
    let ids = [
        cid(EDITED),
        cid(PLAIN),
        CommentId::new("no-such-comment"),
        cid(2_501_240_003),
        cid(EDITED),
        cid(2_501_240_004),
    ];
    for bound in [ALL_MAX, one()] {
        let (mut gh, _) = adapter("site");
        laws::blueprints::several_edits_are_each_ones(&mut gh, &nos, &ids, bound).unwrap();
    }
}

#[test]
fn a_body_history_is_its_edits_oldest_first_the_text_as_created_left_out() {
    let (mut gh, asked) = adapter("site");
    let got = gh.body_edits(bp(7), ALL_MAX);
    assert_eq!(
        texts(&got),
        [
            "Do it now.",
            "Do it today.\r\n\r\nThe page shows the defaults."
        ]
    );
    assert_eq!(logins(&got), ["tehas-bot", "octo-dev"]);
    let at: Vec<At> = got.unwrap().items().iter().map(|e| e.at).collect();
    assert_eq!(
        at,
        [
            At::new(tehas_core::time::from_iso("2026-11-20T12:00:00Z").unwrap()),
            At::new(tehas_core::time::from_iso("2026-11-21T08:00:00Z").unwrap()),
        ]
    );
    assert_eq!(gh.body_edits(bp(8), ALL_MAX), Ok(Whole::default()));
    // An account GitHub no longer names is its ghost.
    assert_eq!(logins(&gh.body_edits(bp(9), ALL_MAX)), ["ghost"]);
    let q = questions(&asked);
    assert_eq!(q.len(), 3);
    assert!(q[0].contains("i7: issue(number: 7) { number createdAt lastEditedAt userContentEdits(first: 100) { totalCount nodes { editedAt deletedAt diff editor"));
}

#[test]
fn several_bodies_are_one_question_each_number_once() {
    let (mut gh, asked) = adapter("site");
    let got = gh.body_edits_of(&[bp(7), bp(8), bp(9999), bp(7)], ALL_MAX);
    let nos: Vec<u64> = got.iter().map(|(n, _)| n.get()).collect();
    assert_eq!(nos, [7, 8, 9999, 7]);
    assert_eq!(texts(&got[0].1).len(), 2);
    assert_eq!(got[1].1, Ok(Whole::default()));
    assert_eq!(got[2].1, Err(Outcome::NotFound));
    assert_eq!(got[3].1, got[0].1);
    let q = questions(&asked);
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].matches(": issue(number: 7)").count(), 1);
}

#[test]
fn twenty_items_a_question() {
    let (mut gh, asked) = adapter("site");
    let nos: Vec<BlueprintNo> = (1..=45).map(bp).collect();
    let got = gh.body_edits_of(&nos, ALL_MAX);
    assert_eq!(got.len(), 45);
    for (no, read) in &got {
        match no.get() {
            7..=9 => assert!(read.is_ok(), "{no:?} {read:?}"),
            _ => assert_eq!(read, &Err(Outcome::NotFound)),
        }
    }
    let sizes: Vec<usize> = questions(&asked)
        .iter()
        .map(|q| q.matches(": issue(number: ").count())
        .collect();
    assert_eq!(sizes, [20, 20, 5]);
}

#[test]
fn the_bound_holds_for_each_item_not_for_the_read() {
    let (mut gh, asked) = adapter("site");
    let got = gh.body_edits_of(&[bp(7), bp(8), bp(9)], one());
    assert_eq!(got[0].1, Err(Outcome::Unread(Unread::Cut)));
    assert_eq!(got[1].1, Ok(Whole::default()));
    assert_eq!(logins(&got[2].1), ["ghost"]);
    // The bound's edits and the text as created.
    assert!(questions(&asked)[0].contains("userContentEdits(first: 2)"));
    let two = Bound::new(2).unwrap();
    assert_eq!(texts(&gh.body_edits(bp(7), two)).len(), 2);
}

#[test]
fn a_comment_history_reads_its_markers_apart() {
    let (mut gh, asked) = adapter("site");
    let got = gh.comment_edits(&cid(EDITED), ALL_MAX);
    assert_eq!(
        texts(&got),
        ["Which page?", "Which page, the settings or the help?"]
    );
    for e in got.unwrap().items() {
        assert!(
            matches!(e.markers.as_slice(), [Marker::Question { .. }]),
            "{e:?}"
        );
    }
    assert_eq!(gh.comment_edits(&cid(PLAIN), ALL_MAX), Ok(Whole::default()));
    assert_eq!(rest_reads(&asked), 2);
    // A node id read once is kept.
    gh.comment_edits_of(&[cid(EDITED), cid(PLAIN)], ALL_MAX);
    assert_eq!(rest_reads(&asked), 2);
    let q = questions(&asked);
    assert_eq!(q.len(), 3);
    assert!(q[2].contains(&format!(
        "nodes(ids: [\"{EDITED_NODE}\", \"IC_kwDOAbCdEf6VEd0002\"])"
    )));
}

#[test]
fn a_read_of_comments_keeps_the_edited_ones_node_ids() {
    let (mut gh, asked) = adapter("site");
    let comments = gh.comments(bp(7), ALL_MAX).unwrap();
    assert_eq!(comments.items().len(), 2);
    let before = questions(&asked).len();
    let got = gh.comment_edits_of(&[cid(EDITED)], ALL_MAX);
    assert_eq!(texts(&got[0].1).len(), 2);
    assert_eq!(rest_reads(&asked), 0);
    assert_eq!(questions(&asked).len(), before + 1);
}

#[test]
fn a_comment_that_is_not_on_an_issue_here_is_not_found_or_untrusted() {
    let (mut gh, asked) = adapter("site");
    let got = gh.comment_edits_of(
        &[
            CommentId::new("no-such-comment"),
            CommentId::new("+2501240001"),
            cid(2_501_240_003),
            cid(2_501_240_004),
            cid(2_501_240_005),
        ],
        ALL_MAX,
    );
    let outcomes: Vec<_> = got.into_iter().map(|(_, g)| g.map(drop)).collect();
    assert_eq!(
        outcomes,
        [
            Err(Outcome::NotFound),
            Err(Outcome::NotFound),
            Err(Outcome::NotFound),
            Err(Outcome::NotFound),
            inconsistent(),
        ]
    );
    // No id that is no number is asked; none is left to ask GitHub's GraphQL.
    assert_eq!(rest_reads(&asked), 3);
    assert!(questions(&asked).is_empty());
}

#[test]
fn an_untrusted_body_history_fails_that_item_alone() {
    let cut = Err(Outcome::Unread(Unread::Cut));
    let malformed = Err(Outcome::Unread(Unread::Malformed));
    let cases = [
        ("wrong-number", 7, inconsistent()),
        ("count-off", 7, inconsistent()),
        ("null-version", 7, inconsistent()),
        ("out-of-order", 7, inconsistent()),
        ("creation-stamp", 7, inconsistent()),
        ("last-stamp", 7, inconsistent()),
        ("deleted-version", 7, inconsistent()),
        ("never-edited", 7, inconsistent()),
        ("edited-no-history", 8, inconsistent()),
        ("null-issue", 7, inconsistent()),
        ("item-error", 7, inconsistent()),
        ("no-history", 7, malformed.clone()),
        ("no-login", 7, malformed),
        ("too-many", 7, cut),
    ];
    for (state, n, want) in cases {
        let (mut gh, _) = adapter(state);
        let got = gh.body_edits_of(&[bp(n), bp(9)], ALL_MAX);
        assert_eq!(got[0].1.clone().map(drop), want, "{state}");
        assert_eq!(logins(&got[1].1), ["ghost"], "{state}");
    }
}

#[test]
fn an_untrusted_comment_history_fails_that_item_alone() {
    let cases = [
        ("wrong-id", inconsistent()),
        ("missing-id", inconsistent()),
        ("wrong-database-id", inconsistent()),
        ("not-a-comment", inconsistent()),
        ("other-issue", inconsistent()),
        ("null-comment", inconsistent()),
        ("deleted-comment", Err(Outcome::NotFound)),
        (
            "rest-fails",
            Err(Outcome::Unavailable {
                why: Passing::Server,
                sent: Sent::Maybe,
            }),
        ),
    ];
    for (state, want) in cases {
        let (mut gh, _) = adapter(state);
        let got = gh.comment_edits_of(&[cid(EDITED), cid(PLAIN)], ALL_MAX);
        assert_eq!(got[0].1.clone().map(drop), want, "{state}");
        assert_eq!(got[1].1, Ok(Whole::default()), "{state}");
    }
}

#[test]
fn a_failed_question_fails_each_of_its_items() {
    let maybe = |why| Outcome::Unavailable {
        why,
        sent: Sent::Maybe,
    };
    let cases = [
        (
            "rate-limited",
            Outcome::Spent {
                pool: Pool::Priced,
                until: At::new(replay_now()),
            },
        ),
        ("error-no-path", Outcome::Unread(Unread::Inconsistent)),
        ("garbled", Outcome::Unread(Unread::Malformed)),
        ("no-list", Outcome::Unread(Unread::Malformed)),
        ("server-error", maybe(Passing::Server)),
        ("timeout", maybe(Passing::Timeout)),
        ("forbidden", Outcome::Forbidden),
        // gh said not found and kept no answer: which item, it cannot say.
        ("unresolved", Outcome::Unread(Unread::Inconsistent)),
    ];
    for (state, want) in cases {
        let (mut gh, _) = adapter(state);
        for (_, got) in gh.body_edits_of(&[bp(7), bp(8)], ALL_MAX) {
            assert_eq!(got, Err(want.clone()), "{state} bodies");
        }
        for (_, got) in gh.comment_edits_of(&[cid(EDITED), cid(PLAIN)], ALL_MAX) {
            assert_eq!(got, Err(want.clone()), "{state} comments");
        }
    }
}

#[test]
fn a_failed_question_fails_the_bodies_read_together_whole() {
    for state in ["error-no-path", "unresolved", "garbled", "server-error"] {
        let (mut gh, _) = adapter(state);
        let of = gh.body_edits_of(&[bp(7), bp(8)], ALL_MAX);
        let (mut gh, _) = adapter(state);
        let together = gh.body_edits_together(&[bp(7), bp(8)], ALL_MAX);
        assert_eq!(together, Err(of[0].1.clone().unwrap_err()), "{state}");
    }
    // An item's own failure is that item's, the others read beside it.
    for state in ["item-error", "deleted-version", "null-issue", "too-many"] {
        let (mut gh, _) = adapter(state);
        let of = gh.body_edits_of(&[bp(7), bp(9)], ALL_MAX);
        let (mut gh, _) = adapter(state);
        let together = gh.body_edits_together(&[bp(7), bp(9)], ALL_MAX);
        assert_eq!(together, Ok(of), "{state}");
    }
}
