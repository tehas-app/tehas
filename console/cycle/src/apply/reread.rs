//! A write of the report that GitHub may have taken, read back before it is
//! sent again (#1754). Nothing decides a cycle's report again, and a settled
//! row is never replayed, so the report reads it itself: a write that is not
//! there is sent again, one that is there is done.

use serde_json::Value;
use tehas_core::journal::summed;

/// What GitHub shows of one write.
#[derive(Debug, PartialEq)]
pub(super) enum Seen {
    /// It is there: a posted comment's id, or nothing.
    There(String),
    Missing,
    /// The read failed, or no read settles a write of this kind.
    Unread,
}

/// The words of a gh call that follow `flag`, each time it is given.
fn after<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == flag)
        .map(|w| w[1].as_str())
        .collect()
}

fn names(v: &Value) -> Vec<&str> {
    v.get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|l| l.get("name").and_then(Value::as_str))
        .collect()
}

fn sum_of(text: &str) -> Option<String> {
    summed(&[], Some(text))?
        .get("sha256")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Whether the write `kind` on issue `n` is on GitHub. `since` is when it was
/// sent; a comment posted a minute before it still counts, for the clocks of
/// the host and GitHub. `read` asks gh for a path, every page when told to.
pub(super) fn seen(
    repo: &str,
    n: u64,
    kind: &str,
    args: &[String],
    input: Option<&str>,
    since: i64,
    read: &mut dyn FnMut(&str, bool) -> Option<Value>,
) -> Seen {
    let want = summed(args, input).and_then(|s| s.get("sha256")?.as_str().map(str::to_string));
    let issue = format!("repos/{repo}/issues/{n}");
    match kind {
        "comment-post" => {
            let Some(want) = want else {
                return Seen::Unread;
            };
            let Some(v) = read(&format!("{issue}/comments?per_page=100"), true) else {
                return Seen::Unread;
            };
            let from = tehas_core::time::iso(since.saturating_sub(60));
            let found = v.as_array().into_iter().flatten().find(|c| {
                c.get("created_at")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t >= from.as_str())
                    && sum_of(c.get("body").and_then(Value::as_str).unwrap_or(""))
                        .is_some_and(|s| s == want)
            });
            match found {
                Some(c) => Seen::There(c.get("id").map(Value::to_string).unwrap_or_default()),
                None => Seen::Missing,
            }
        }
        "label-add" | "label-remove" | "refresh-end" => {
            let Some(v) = read(&issue, false) else {
                return Seen::Unread;
            };
            let have = names(&v);
            let (on, off) = if kind == "refresh-end" {
                (vec![], vec!["tehas:refreshing"])
            } else {
                (after(args, "--add-label"), after(args, "--remove-label"))
            };
            let done = on.iter().all(|l| have.contains(l)) && off.iter().all(|l| !have.contains(l));
            if done {
                Seen::There(String::new())
            } else {
                Seen::Missing
            }
        }
        _ => Seen::Unread,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(s: &str) -> Vec<String> {
        s.split(' ').map(str::to_string).collect()
    }

    fn with(answer: Option<Value>) -> impl FnMut(&str, bool) -> Option<Value> {
        move |_, _| answer.clone()
    }

    const NOW: i64 = 2_000_000_000;

    #[test]
    fn a_comment_is_found_by_its_text_since_it_was_sent() {
        let a = args("issue comment 7 --repo a/b --body-file -");
        let at = tehas_core::time::iso(NOW - 30);
        let old = tehas_core::time::iso(NOW - 600);
        let v = json!([
            {"id": 1, "body": "Hello.", "created_at": old},
            {"id": 2, "body": "Other.", "created_at": at},
            {"id": 3, "body": "Hello.", "created_at": at},
        ]);
        let got = seen(
            "a/b",
            7,
            "comment-post",
            &a,
            Some("Hello."),
            NOW,
            &mut with(Some(v)),
        );
        assert_eq!(got, Seen::There("3".into()));
    }

    #[test]
    fn a_comment_given_as_json_is_found_by_its_body() {
        let a = args("api repos/a/b/issues/7/comments --input - --jq .id");
        let at = tehas_core::time::iso(NOW);
        let v = json!([{"id": 9, "body": "Q?", "created_at": at}]);
        let input = json!({"body": "Q?"}).to_string();
        let got = seen(
            "a/b",
            7,
            "comment-post",
            &a,
            Some(&input),
            NOW,
            &mut with(Some(v)),
        );
        assert_eq!(got, Seen::There("9".into()));
    }

    #[test]
    fn an_earlier_comment_with_the_same_text_is_not_the_write() {
        let a = args("issue comment 7 --repo a/b --body-file -");
        let v = json!([{"id": 1, "body": "Hello.", "created_at": tehas_core::time::iso(NOW - 61)}]);
        let got = seen(
            "a/b",
            7,
            "comment-post",
            &a,
            Some("Hello."),
            NOW,
            &mut with(Some(v)),
        );
        assert_eq!(got, Seen::Missing);
    }

    #[test]
    fn labels_are_there_when_each_added_is_on_and_each_removed_is_off() {
        let a = args("issue edit 7 --repo a/b --remove-label tehas:running --add-label needs-human-attention");
        let on = json!({"labels": [{"name": "needs-human-attention"}]});
        let both =
            json!({"labels": [{"name": "needs-human-attention"}, {"name": "tehas:running"}]});
        let r = |v: Value| seen("a/b", 7, "label-add", &a, None, NOW, &mut with(Some(v)));
        assert_eq!(r(on), Seen::There(String::new()));
        assert_eq!(r(both), Seen::Missing);
        assert_eq!(r(json!({"labels": []})), Seen::Missing);
    }

    #[test]
    fn a_refresh_mark_is_off_when_the_label_is_gone() {
        let a = args("api -X DELETE repos/a/b/issues/9/labels/tehas:refreshing");
        let r = |v: Value| seen("a/b", 9, "refresh-end", &a, None, NOW, &mut with(Some(v)));
        assert_eq!(r(json!({"labels": []})), Seen::There(String::new()));
        assert_eq!(
            r(json!({"labels": [{"name": "tehas:refreshing"}]})),
            Seen::Missing
        );
    }

    #[test]
    fn a_failed_read_or_another_kind_settles_nothing() {
        let a = args("issue comment 7 --repo a/b --body-file -");
        assert_eq!(
            seen(
                "a/b",
                7,
                "comment-post",
                &a,
                Some("x"),
                NOW,
                &mut with(None)
            ),
            Seen::Unread
        );
        let v = Some(json!({"labels": []}));
        assert_eq!(
            seen("a/b", 7, "issue-edit", &a, Some("x"), NOW, &mut with(v)),
            Seen::Unread
        );
    }
}
