//! The host's filing of a drafted plan, on a tick of "File as a new issue".
//! No container runs for it: the planner's container holds a token that
//! reads issues alone, so a confirm inside it was refused by the site. The
//! host opens the plan as the factory, through the blueprints port: the
//! title and the spec the runner showed, byte for byte, its session's line
//! in it. A spec is a blueprint in the ports' terms, so a site that keeps
//! tickets and blueprints apart files it there too. It carries no marker of
//! the issue it was planned on: the GitHub adapter opens no blueprint with
//! a marker yet, and the dispatcher ties the filed issue to it, as it did
//! the one a confirm filed.
//!
//! A try whose answer says the plan may have been filed leaves `filing` in
//! <keep>. The next try looks for it before it files again, by the settle
//! rule of an opened blueprint: a search of titles, its hits read whole, one
//! of the factory's own with the same title and the session's line is the
//! one: the line holds the session's id, which no other session has. A search that holds more than it gave, or one
//! that fails, cannot tell: nothing is filed until a try can.

use serde_json::Value;
use std::fs;
use std::num::NonZeroU32;
use std::path::Path;
use tehas_ports::{
    AccountKind, BlueprintWrites, Blueprints, Body, Depth, NewBlueprint, Outcome, Sent, WebLink,
    Within,
};

/// The record of a try that may have filed the plan.
const FILING: &str = "filing";
/// How many hits of the title search are read whole.
const LOOKED: u32 = 20;

/// What a tick of "File as a new issue" did.
#[derive(Debug, PartialEq)]
pub(super) enum Filed {
    /// The plan is filed there, by this try or an earlier one.
    At { link: WebLink, title: String },
    /// Nothing was filed, or it may have been and the next try looks first:
    /// why, for the person.
    Not(String),
}

/// Files the plan kept in `keep` by the session `session`, reading through
/// `read` and opening through `write`.
pub(super) fn file(
    keep: &Path,
    session: &str,
    read: &mut dyn Blueprints,
    write: &mut dyn BlueprintWrites,
) -> Filed {
    let Some((title, spec)) = shown(keep) else {
        return Filed::Not(
            "no plan the runner showed is kept: answer to have the plan drafted again".into(),
        );
    };
    let line = format!("plan-session={session}");
    let record = keep.join(FILING);
    if record.exists() {
        match look(read, &title, &line) {
            Look::There(link) => {
                let _ = fs::remove_file(&record);
                return Filed::At { link, title };
            }
            Look::Unknown(why) => {
                return Filed::Not(format!(
                    "an earlier try may have filed the plan and {why}, so nothing was filed; the next try looks again"
                ))
            }
            Look::NotThere => {}
        }
    }
    // Recorded before it is sent: a try cut off after this may have filed.
    if fs::write(&record, format!("{session}\n")).is_err() {
        return Filed::Not("the try could not be recorded, so nothing was filed".into());
    }
    let new = NewBlueprint {
        title: title.clone(),
        body: Body::Text(spec),
        markers: Vec::new(),
    };
    match write.open(&new) {
        Ok(posted) => {
            let _ = fs::remove_file(&record);
            Filed::At {
                link: posted.link,
                title,
            }
        }
        Err(o) if maybe(&o) => Filed::Not(format!(
            "the plan may have been filed ({}); the next try looks for it before it files again",
            o.line()
        )),
        Err(o) => {
            let _ = fs::remove_file(&record);
            Filed::Not(format!("the plan was not filed: {}", o.line()))
        }
    }
}

/// An answer that may have filed it: one sent with no answer, or one that
/// could not be read.
fn maybe(o: &Outcome) -> bool {
    matches!(
        o,
        Outcome::Unavailable {
            sent: Sent::Maybe,
            ..
        } | Outcome::Unread(_)
    )
}

/// The title and the spec of the plan the last turn showed, as the runner
/// gave them (`outcome` keeps them at exit 12).
fn shown(keep: &Path) -> Option<(String, String)> {
    let draft: Value =
        serde_json::from_str(&fs::read_to_string(keep.join("draft.json")).ok()?).ok()?;
    let title = draft.get("title")?.as_str()?.to_string();
    let spec = draft.get("shown")?.as_str()?.to_string();
    (!title.trim().is_empty() && !spec.trim().is_empty()).then_some((title, spec))
}

enum Look {
    There(WebLink),
    NotThere,
    /// The look could not tell: why.
    Unknown(String),
}

/// Whether the plan is filed already, by the settle rule of an opened
/// blueprint.
fn look(read: &mut dyn Blueprints, title: &str, line: &str) -> Look {
    let first = NonZeroU32::new(LOOKED).expect("20 is no zero");
    let took = match read.search(title, Within::Titles, None, first) {
        Ok(took) => took,
        Err(o) => return Look::Unknown(format!("the search failed ({})", o.line())),
    };
    let nos: Vec<_> = took
        .items
        .iter()
        .filter(|h| h.title == title)
        .map(|h| h.no)
        .collect();
    for (_, got) in read.several(&nos, Depth::Cut { notes: 0 }) {
        let Ok(b) = got else {
            return Look::Unknown("a hit of the search could not be read".into());
        };
        let ours = b.author.as_ref().map(|a| a.kind) == Some(AccountKind::Ours);
        if ours && b.title == title && b.body.contains(line) {
            return Look::There(b.link);
        }
    }
    if took.more {
        return Look::Unknown("the search holds more than it gave".into());
    }
    Look::NotThere
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;
    use tehas_ports::fake::{BlueprintsOp, FakeSite, Fault, Faulty, Op, World};
    use tehas_ports::{Actor, Blueprint, Caller, Scope, Site};

    const TITLE: &str = "Count the parts of a widget";
    const SESSION: &str = "3f2a9c1e-0000-4000-8000-000000000001";

    fn spec() -> String {
        format!(
            "## Spec\n\nCount them.\n\n<!-- pi-rukas:provenance v1 plan-session={SESSION} -->\n"
        )
    }

    /// <keep> with the plan exit 12 kept.
    fn keep(name: &str) -> TestDir {
        let d = TestDir::new(&format!("plan-file-{name}"));
        let draft = serde_json::json!({"title": TITLE, "spec": "Count them.", "shown": spec()});
        fs::write(d.join("draft.json"), draft.to_string()).unwrap();
        d
    }

    /// Issue 7, which the plan was made on.
    fn world() -> World {
        World::new().ticket(7, |t| t.title("Widgets"))
    }

    fn file_on(site: &impl Site, keep: &Path) -> Filed {
        let (by, c) = (Actor::Ours(Scope::Full), Caller::new("plan"));
        let mut read = site.blueprints(&by, c).unwrap();
        let mut write = site.blueprint_writes(&by, c).unwrap();
        file(keep, SESSION, &mut *read, &mut *write)
    }

    /// The blueprints titled as the plan, read whole.
    fn filed(world: &World) -> Vec<Blueprint> {
        let site = FakeSite::new(world);
        let mut b = site
            .blueprints(&Actor::Operator, Caller::new("test"))
            .unwrap();
        let first = NonZeroU32::new(50).unwrap();
        let hits = b.search(TITLE, Within::Titles, None, first).unwrap();
        let nos: Vec<_> = hits.items.iter().map(|h| h.no).collect();
        b.several(&nos, Depth::Cut { notes: 0 })
            .into_iter()
            .map(|(_, g)| g.unwrap())
            .collect()
    }

    #[test]
    fn the_shown_plan_is_filed_as_the_factory_byte_for_byte() {
        let (w, k) = (world(), keep("files"));
        let Filed::At { title, .. } = file_on(&FakeSite::new(&w), k.path()) else {
            panic!("not filed");
        };
        assert_eq!(title, TITLE);
        let filed = filed(&w);
        assert_eq!(filed.len(), 1);
        assert_eq!(filed[0].body, spec());
        assert!(filed[0].markers.is_empty());
        assert_eq!(
            filed[0].author.as_ref().map(|a| a.kind),
            Some(AccountKind::Ours)
        );
        assert!(!k.join(FILING).exists());
    }

    #[test]
    fn with_no_shown_plan_kept_nothing_is_filed() {
        let (w, k) = (world(), keep("none"));
        fs::write(k.join("draft.json"), r#"{"title": "T", "spec": "S"}"#).unwrap();
        assert!(matches!(
            file_on(&FakeSite::new(&w), k.path()),
            Filed::Not(_)
        ));
        assert!(filed(&w).is_empty());
    }

    /// A try with no answer that filed: the next one finds it and files
    /// nothing more.
    #[test]
    fn a_try_that_may_have_filed_is_looked_for_before_the_next_files() {
        let (w, k) = (world(), keep("maybe-filed"));
        let faulty = Faulty::new(FakeSite::new(&w))
            .on(
                Op::Blueprints(BlueprintsOp::Open),
                Fault::TimeoutMaybe { applied: true },
            )
            .times(1);
        let Filed::Not(why) = file_on(&faulty, k.path()) else {
            panic!("filed");
        };
        assert!(why.contains("may have been filed"), "{why}");
        assert!(k.join(FILING).exists());
        assert!(matches!(file_on(&faulty, k.path()), Filed::At { .. }));
        assert_eq!(filed(&w).len(), 1);
        assert!(!k.join(FILING).exists());
    }

    /// One that did not: the next try finds none and files once.
    #[test]
    fn a_try_that_did_not_file_is_filed_once_by_the_next() {
        let (w, k) = (world(), keep("maybe-not"));
        let faulty = Faulty::new(FakeSite::new(&w))
            .on(
                Op::Blueprints(BlueprintsOp::Open),
                Fault::TimeoutMaybe { applied: false },
            )
            .times(1);
        assert!(matches!(file_on(&faulty, k.path()), Filed::Not(_)));
        assert!(filed(&w).is_empty());
        assert!(matches!(file_on(&faulty, k.path()), Filed::At { .. }));
        assert_eq!(filed(&w).len(), 1);
    }

    /// A look that cannot tell files nothing, and the record stays.
    #[test]
    fn a_look_that_fails_files_nothing() {
        let (w, k) = (world(), keep("look-fails"));
        fs::write(k.join(FILING), SESSION).unwrap();
        let faulty =
            Faulty::new(FakeSite::new(&w)).on(Op::Blueprints(BlueprintsOp::Search), Fault::Timeout);
        let Filed::Not(why) = file_on(&faulty, k.path()) else {
            panic!("filed");
        };
        assert!(why.contains("the search failed"), "{why}");
        assert!(filed(&w).is_empty());
        assert!(k.join(FILING).exists());
    }

    /// A person's copy of the plan, its line in it, is not the factory's
    /// filing: the plan is filed.
    #[test]
    fn a_persons_copy_is_not_the_filing() {
        let w = world().blueprint(9, |b| b.title(TITLE).body(&spec()).author("alice"));
        let k = keep("copy");
        fs::write(k.join(FILING), SESSION).unwrap();
        assert!(matches!(
            file_on(&FakeSite::new(&w), k.path()),
            Filed::At { .. }
        ));
        assert_eq!(filed(&w).len(), 2);
    }

    /// A refusal files nothing and leaves no record: the next try files.
    #[test]
    fn a_refusal_leaves_no_record() {
        let (w, k) = (world(), keep("refused"));
        let faulty = Faulty::new(FakeSite::new(&w))
            .on(Op::Blueprints(BlueprintsOp::Open), Fault::Timeout)
            .times(1);
        assert!(matches!(file_on(&faulty, k.path()), Filed::Not(_)));
        assert!(!k.join(FILING).exists());
    }
}
