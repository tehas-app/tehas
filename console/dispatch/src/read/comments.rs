//! The comment reads on the tickets port: each issue's notes as the
//! comment answer the rules read today, and the edited comments' histories
//! on `Tickets::note_edits` (the shape `fill_comment_edits` put back). The
//! mapping is the poll's one reader of the port's typed comment: it spells
//! the page's words, and no rule sees the port. Where the ticket carries no
//! notes and its key names the blueprint that is the same item, the
//! comments are the blueprint's and the blueprints port answers them (the
//! answer the tickets port promises to be the same).

use serde_json::{json, Value};
use tehas_core::time::iso;
use tehas_ports::{
    At, Blueprints, Comment, Marker, Outcome, TicketKey, Tickets, Unread, Window, ALL_MAX,
};

/// A comment answer no read failed: the failed read of a handle the wiring
/// could not make, which no gh call failed for and no rule may read as an
/// empty list.
const NO_HANDLE: &str = "ERROR: the poll has no handle to read the comments by";

/// The page's field that names when a comment was made, spelled once here
/// and read by its name everywhere else.
pub(crate) const CREATED_AT: &str = "createdAt";

/// One issue's comment answer as the rules read it: the page's shape, with
/// its comment nodes or its failed read. The poll's one reader writes it
/// and the snapshot holds it; no rule sees the port.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    page: serde_json::Value,
}

impl std::ops::Deref for Answer {
    type Target = serde_json::Value;

    fn deref(&self) -> &Self::Target {
        &self.page
    }
}

impl std::fmt::Display for Answer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.page.fmt(f)
    }
}

/// The account a page names, or null for none. The same spelling as
/// `read::board`: an id no number is named by its site word.
macro_rules! account {
    ($a:expr) => {
        match $a {
            Some(a) => json!({"login": a.login.as_str(), "databaseId": a.id.as_ref().map(|id| {
                id.as_str().parse::<u64>().map_or_else(|_| json!(id.as_str()), |n| json!(n))
            })}),
            None => json!(null),
        }
    };
}

/// How the site spells what the typed answers name by word.
pub struct Words<'a> {
    /// A marker as the site writes it: the site's `Site::marker_text`,
    /// from the program's wiring. Our markers come back into a comment's
    /// body with it, as the line they stand on: the rules read a comment's
    /// markers from its text, and a comment of the factory's names its
    /// kind on its own line.
    pub marker: &'a dyn Fn(&Marker) -> String,
}

/// The comments of every asked issue, in the order asked: each one issue's
/// answer, `{"data":{"repository":{"issue":{"comments":{"nodes":[...]}}}}}`,
/// or the failed read of the issue when a read of it failed. The edited
/// comments have their histories read and put back, so `edit_holds` and
/// `edit_history_unread` see what the chunked reads gave.
pub fn all(
    mut tickets: Option<&mut (dyn Tickets + '_)>,
    mut blueprints: Option<&mut (dyn Blueprints + '_)>,
    words: &Words,
    numbers: &[u64],
) -> Vec<Answer> {
    numbers
        .iter()
        .map(|n| {
            one(
                tickets.as_deref_mut(),
                blueprints.as_deref_mut(),
                words,
                *n,
                true,
            )
        })
        .collect()
}

/// The comments of one issue, with no history read of an edited comment:
/// for a reader that judges no comment's edits (the finish asks, which read
/// only `lastEditedAt` and the editor), which so pays no history read.
pub fn unjudged(
    tickets: Option<&mut (dyn Tickets + '_)>,
    blueprints: Option<&mut (dyn Blueprints + '_)>,
    words: &Words,
    n: u64,
) -> Answer {
    one(tickets, blueprints, words, n, false)
}

/// One issue's answer. `histories` says whether the edited comments' edit
/// histories are read.
fn one(
    tickets: Option<&mut (dyn Tickets + '_)>,
    blueprints: Option<&mut (dyn Blueprints + '_)>,
    words: &Words,
    n: u64,
    histories: bool,
) -> Answer {
    let fail = |o: &Outcome| Answer {
        page: json!({"failed": true, "line": o.line()}),
    };
    let Some(number) = std::num::NonZeroU64::new(n) else {
        return Answer {
            page: json!({"failed": true, "line": NO_HANDLE}),
        };
    };
    let key = TicketKey::Number(number);
    let Some(t) = tickets else {
        return Answer {
            page: json!({"failed": true, "line": NO_HANDLE}),
        };
    };
    let mut comments = match t.notes(&key, Window::Whole(ALL_MAX)) {
        Ok(took) => took.items,
        Err(o) => return fail(&o),
    };
    if comments.is_empty() {
        // Where ticket and blueprint are one item and the ticket answers no
        // notes, the item's comments are the blueprint's, as the port says.
        if let (Some(no), Some(b)) = (key.same_item(), blueprints) {
            match b.comments(no, ALL_MAX) {
                Ok(whole) => comments = whole.into_items(),
                Err(o) => return fail(&o),
            }
        }
    }
    let mut nodes = Vec::with_capacity(comments.len());
    for c in &comments {
        // The edit history of the comment as the port carried it: the
        // versions newest first, the text as created last, each its editor,
        // and the count of them, so `edit_holds` and `edit_history_unread`
        // see what the old read did. A history the port says is cut or does
        // not agree with itself is `{"unjudged": true}`: `edit_holds` holds
        // on it as on one by nobody, and the poll goes on. Any other
        // failure is the read's own failed marker, which ends the poll as
        // unread when a rule needs the history.
        let history = match (histories, c.edited) {
            (true, Some(_)) => Some(match t.note_edits(&c.id, ALL_MAX) {
                Ok(whole) => {
                    let edits = whole.into_items();
                    let mut versions: Vec<Value> = edits
                        .iter()
                        .rev()
                        .map(|e| json!({"editor": account!(&e.by)}))
                        .collect();
                    versions.push(json!({"editor": account!(&c.author)}));
                    json!({"totalCount": edits.len() + 1, "nodes": versions})
                }
                Err(Outcome::Unread(Unread::Cut | Unread::Inconsistent)) => {
                    json!({"unjudged": true})
                }
                Err(o) => json!({"failed": true, "line": o.line()}),
            }),
            (true, None) => Some(json!({"totalCount": 0, "nodes": []})),
            (false, _) => None,
        };
        let mut node = json!({
            "databaseId": c.id.as_str().parse::<u64>().ok(),
            "author": account!(&c.author),
            "body": body(c, words),
            (CREATED_AT): iso(c.created.secs()),
            "lastEditedAt": c.edited.map(stamp),
            "editor": account!(&c.editor),
        });
        if let Some(h) = history {
            node["userContentEdits"] = h;
        }
        nodes.push(node);
    }
    Answer {
        page: json!({"data": {"repository": {"issue": {"comments": {"nodes": nodes}}}}}),
    }
}

/// A comment's text as the read gave it: the port's body without our
/// markers, and every marker the site can write back on its own line - a
/// question's at the text's end, as the factory writes the comments it
/// asks, the rest at its start. A marker the site cannot name (one that
/// does not read, a kind this copy does not know) holds its words in the
/// body it was read from; `Bare` ones stay there already.
fn body(c: &Comment, words: &Words) -> String {
    if let Some(raw) = &c.raw {
        return raw.clone();
    }
    let (mut head, mut tail): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for m in &c.markers {
        if matches!(m, Marker::Bare { .. }) {
            continue;
        }
        let line = (words.marker)(m);
        if line.is_empty() {
            continue;
        }
        if matches!(m, Marker::Question { .. }) {
            tail.push(line);
        } else {
            head.push(line);
        }
    }
    [head.join("\n"), c.body.clone(), tail.join("\n")]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<String>>()
        .join("\n\n")
}

/// A time as the page writes it.
fn stamp(at: At) -> String {
    iso(at.secs())
}

#[cfg(test)]
mod tests {
    use super::{all, unjudged, Answer, Words, CREATED_AT, NO_HANDLE};
    use serde_json::{json, Value};
    use std::num::NonZeroU32;
    use tehas_core::events::{edit_history_unread, edit_holds};
    use tehas_ports::fake::{marker_text, FakeSite, Fault, Faulty, Op, TicketsOp, World};
    use tehas_ports::{
        Actor, Caller, CommentId, Marker, MarkerKind, QuestionKind, Ref, Scope, Site, TicketKey,
    };

    fn marker(m: &Marker) -> String {
        marker_text(m)
    }

    fn words() -> Words<'static> {
        Words { marker: &marker }
    }

    /// The comment of a case-world by the spec: its id, when the world has
    /// held one comment alone.
    fn only_id() -> CommentId {
        CommentId::new("comment-1")
    }

    /// The comment nodes of one answer, for the cases' assertions.
    #[derive(Clone, Debug)]
    struct Nodes {
        items: Vec<Value>,
    }

    impl std::ops::Deref for Nodes {
        type Target = Vec<Value>;

        fn deref(&self) -> &Self::Target {
            &self.items
        }
    }

    fn read(world: &World, n: u64) -> Nodes {
        let site = FakeSite::new(world);
        let (by, caller) = (Actor::Ours(Scope::Full), Caller::new("test"));
        let mut tickets = site.tickets(&by, caller).unwrap();
        let mut blueprints = site.blueprints(&by, caller).unwrap();
        let mut answers = all(
            Some(tickets.as_mut()),
            Some(blueprints.as_mut()),
            &words(),
            &[n],
        );
        nodes(answers.pop().unwrap())
    }

    fn nodes(answer: Answer) -> Nodes {
        assert!(answer["failed"].is_null(), "{answer}");
        Nodes {
            items: answer["data"]["repository"]["issue"]["comments"]["nodes"]
                .as_array()
                .unwrap()
                .clone(),
        }
    }

    #[test]
    fn a_comment_reads_as_the_page_answered_it() {
        let world = World::new().ticket(7, |t| t.comment("alice", "plain text"));
        let at = tehas_core::time::iso(world.now().secs());
        let ns = read(&world, 7);
        assert_eq!(ns.len(), 1);
        assert_eq!(ns[0]["author"]["login"], "alice");
        assert_eq!(ns[0]["body"], "plain text");
        assert_eq!(ns[0][CREATED_AT], json!(at));
        assert_eq!(ns[0]["lastEditedAt"], json!(null));
        assert_eq!(ns[0]["editor"], json!(null));
        // An unedited comment carries an empty history, as the fill put it.
        assert_eq!(
            ns[0]["userContentEdits"],
            json!({"totalCount": 0, "nodes": []})
        );
        // The unjudged read carries no history at all.
        let site = FakeSite::new(&world);
        let (by, caller) = (Actor::Ours(Scope::Full), Caller::new("test"));
        let mut tickets = site.tickets(&by, caller).unwrap();
        let mut blueprints = site.blueprints(&by, caller).unwrap();
        let u = nodes(unjudged(
            Some(tickets.as_mut()),
            Some(blueprints.as_mut()),
            &words(),
            7,
        ));
        assert!(u[0].get("userContentEdits").is_none(), "{u:?}");
    }

    #[test]
    fn our_markers_come_back_on_the_body_as_the_site_writes_them() {
        // The blocked marker opens the comment, the question's closes it,
        // as the factory writes the comments it asks: the rules read the
        // kind of a comment from its text, as they did from the page's.
        let world = World::new()
            .ticket(7, |t| {
                t.comment_marked(
                    "tehas-bot",
                    "Waiting.",
                    vec![Marker::Blocked {
                        by: vec![Ref::Here(TicketKey::Number(11.try_into().unwrap()))],
                    }],
                )
            })
            .ticket(8, |t| {
                t.comment_marked(
                    "tehas-bot",
                    "Tick one box.",
                    vec![Marker::Question {
                        kind: QuestionKind::Park,
                        blueprint: tehas_ports::BlueprintNo::new(8).unwrap(),
                        turn: NonZeroU32::new(1).unwrap(),
                    }],
                )
            });
        let ns = read(&world, 7);
        let blocked = marker(&Marker::Blocked {
            by: vec![Ref::Here(TicketKey::Number(11.try_into().unwrap()))],
        });
        assert_eq!(ns[0]["body"], format!("{blocked}\n\nWaiting."));
        let ns = read(&world, 8);
        let ask = marker(&Marker::Question {
            kind: QuestionKind::Park,
            blueprint: tehas_ports::BlueprintNo::new(8).unwrap(),
            turn: NonZeroU32::new(1).unwrap(),
        });
        assert_eq!(ns[0]["body"], format!("Tick one box.\n\n{ask}"));
    }

    /// The world's one marker comment of the bot, edited by an outside
    /// account: the read back gives `edit_holds` the history it judged
    /// before, and a read that fails holds and waits, as the fill's did.
    #[test]
    fn an_outside_edit_of_a_marker_comment_still_holds_after_the_history_is_put_back() {
        let ours = |w: &str| w == "tehas-bot";
        let park = Marker::Park(park_marker());
        let world = World::new().ticket(7, |t| {
            t.comment_marked("tehas-bot", "Parked.", vec![park.clone()])
        });
        world.edit_comment_elsewhere(&only_id(), "Parked. Edited.", "mallory");
        let since = tehas_core::time::iso(world.now().secs() - 60);
        let ns = read(&world, 7);
        let c = &ns[0];
        assert_eq!(c["editor"]["login"], "mallory");
        assert_eq!(
            c["userContentEdits"],
            json!({"totalCount": 2, "nodes": [
                {"editor": {"login": "mallory", "databaseId": "id-mallory"}},
                {"editor": {"login": "tehas-bot", "databaseId": "id-tehas-bot"}}]})
        );
        assert_eq!(
            edit_holds(c, &since, "tehas-bot", &ours).as_deref(),
            Some("mallory")
        );
        // A history cut short holds too, as before: the count says more
        // than the nodes, and the outside editor of the one held stands.
        let cut = |c: &Value| {
            let mut c = c.clone();
            c["userContentEdits"] = json!({"totalCount": 101,
                "nodes": [{"editor": {"login": "tehas-bot"}}]});
            c
        };
        assert!(edit_holds(&cut(c), &since, "tehas-bot", &ours).is_some());
        // A history read that failed holds too, and ends the poll as unread
        // when the comment's own editor is allowed: the poll waits for the
        // history rather than refusing on a comment it cannot prove.
        let ours_edited =
            World::new().ticket(7, |t| t.comment_marked("tehas-bot", "Parked.", vec![park]));
        ours_edited.edit_comment_elsewhere(&only_id(), "Parked. Edited.", "tehas-bot");
        let ours_since = tehas_core::time::iso(ours_edited.now().secs() - 60);
        let faulty = Faulty::new(FakeSite::new(&ours_edited))
            .on(Op::Tickets(TicketsOp::NoteEdits), Fault::Unreachable);
        let mut tickets = faulty
            .tickets(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap();
        let mut blueprints = faulty
            .blueprints(&Actor::Ours(Scope::Full), Caller::new("test"))
            .unwrap();
        let ns = nodes(
            all(
                Some(tickets.as_mut()),
                Some(blueprints.as_mut()),
                &words(),
                &[7],
            )
            .pop()
            .unwrap(),
        );
        assert_eq!(ns[0]["userContentEdits"]["failed"], json!(true));
        assert!(ns[0]["userContentEdits"]["line"].is_string(), "{ns:?}");
        assert!(edit_holds(&ns[0], &ours_since, "tehas-bot", &ours).is_some());
        assert!(edit_history_unread(&ns[0], &ours));
    }

    #[test]
    fn a_history_the_port_cuts_holds_without_the_poll_waiting() {
        let world = World::new().ticket(7, |t| t.comment("tehas-bot", "Parked."));
        let id = only_id();
        for _ in 0..1001 {
            world.edit_comment_elsewhere(&id, "Parked.", "mallory");
        }
        let ns = read(&world, 7);
        assert_eq!(ns[0]["userContentEdits"], json!({"unjudged": true}));
        let ours = |w: &str| w == "tehas-bot";
        assert!(edit_holds(&ns[0], "", "tehas-bot", &ours).is_some());
        assert!(
            !edit_history_unread(&ns[0], &ours),
            "the poll does not wait"
        );
    }

    #[test]
    fn a_poll_without_the_handle_reads_nothing_and_fails_closed() {
        let a = all(None, None, &words(), &[7, 8]);
        assert_eq!(a.len(), 2);
        for v in a {
            assert_eq!(v["failed"], json!(true), "{v}");
            assert_eq!(v["line"], json!(NO_HANDLE));
        }
        assert_eq!(unjudged(None, None, &words(), 7)["failed"], json!(true));
    }

    fn park_marker() -> tehas_ports::ParkMarker {
        // A park as the rules spell one: the words of the marker are the
        // site's, and the read only asks that its kind comes back.
        serde_json::from_value(json!({
            "blueprint": 7, "step": "develop", "kind": "plain", "provider": false,
            "sig": "0".repeat(64)
        }))
        .unwrap()
    }

    #[test]
    fn a_note_kind_the_site_cannot_name_holds_its_words() {
        // An unknown marker names no text: its words stay in the body the
        // port gave, and the read asks no line for it. The body holds the
        // note as the site wrote it; the read never parses the body, so
        // the words are the comment's own, in a form no site names.
        let note = "an <!-- unknown --> note";
        let world = World::new().ticket(7, |t| {
            t.comment_marked("alice", note, vec![Marker::Unknown])
        });
        let ns = read(&world, 7);
        assert_eq!(ns[0]["body"], note);
        let _ = MarkerKind::Park;
    }
}
