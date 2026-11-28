//! Decided 116's containment, at every write that carries a text: each
//! `Write` variant, sent by the ports' `Writer` to this adapter as all
//! three handles, over the attack read's corpus of #3077 (a marker after
//! a table under a paragraph, in indented code, in deeply nested lists,
//! after an escaped `<!--`, in a link's destination or title, in a quote),
//! taken from the reader's recorded oracle corpus.
//!
//! For each write and each text, one of two holds:
//! - a caller's text that `markers::foreign_text` refuses (our wrapper,
//!   opening or closing, or a marker the reader finds, bare too) is refused
//!   at the writer, nothing sent;
//! - else every body sent reads exactly the markers the write asked for:
//!   a marker in the caller's text is dead in the composed text, and a
//!   restraint the factory wrote after it reads live.
//!
//! Every variant of `WriteKind` is listed in [`writes`], by the names its
//! own serde gives: a new variant fails here until it is listed.

mod oracle_corpus;
mod restraints;

use oracle_corpus::{rows, M};
use restraints::restraints;
use serde::de::{self, Deserialize, Visitor};
use std::fmt;
use std::sync::{Arc, Mutex};
use tehas_core::forge::GhRunner;
use tehas_github::markers::{foreign_text, kept, read};
use tehas_github::GitHub;
use tehas_ports::{
    At, BlueprintNo, Body, Branch, ChangeNo, CommentId, CommitId, Got, Marker, MarkerEdit,
    NewBlueprint, NewChange, NewRelease, NewTicket, NoBlobs, Outcome, Priority, Proposal, Receipt,
    RepoPath, Revision, Sha256, Status, StatusState, Sum, TagName, Takeover, TargetEdit, TargetId,
    TicketKey, Write, WriteKind, Writer, Writes,
};

const PROJECT: &str = "acme/widget";

fn sample(name: &str) -> String {
    let at = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples");
    std::fs::read_to_string(at.join(name)).unwrap()
}

/// The bodies each write posted, from the standard input of every call.
type Sent = Arc<Mutex<Vec<String>>>;

/// Answers the reads a write makes first: the repository's name, issue 7,
/// pull request 8, and no open pull request of a head. A call with input is recorded and answered
/// 422, so nothing is settled after it.
struct Site {
    sent: Sent,
    issue: String,
    pull: String,
    pulls: String,
    refused: String,
}

impl GhRunner for Site {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        let path = args.get(1).copied().unwrap_or_default();
        // The question of the repository's name, before a write.
        if path == "graphql"
            && args
                .last()
                .is_some_and(|q| q.ends_with("{ databaseId nameWithOwner } }"))
        {
            return Ok(
                r#"{"data":{"repository":{"databaseId":7001,"nameWithOwner":"acme/widget"}}}"#
                    .into(),
            );
        }
        if path == format!("repos/{PROJECT}/issues/7") {
            Ok(self.issue.clone())
        } else if path == format!("repos/{PROJECT}/pulls/8") {
            Ok(self.pull.clone())
        } else if path.starts_with(&format!("repos/{PROJECT}/pulls?")) {
            Ok(self.pulls.clone())
        } else {
            Err("no answer".into())
        }
    }

    fn run_input(&mut self, _: &[&str], input: &str) -> Result<String, String> {
        self.sent.lock().unwrap().push(input.to_string());
        Err(self.refused.clone())
    }
}

/// The samples a site answers with, read once.
fn answers() -> &'static [String; 4] {
    static ANSWERS: std::sync::OnceLock<[String; 4]> = std::sync::OnceLock::new();
    ANSWERS.get_or_init(|| {
        [
            sample("ticket-writes/issue-7.json"),
            sample("code-host-writes/pull-8-open.json"),
            sample("open-change/pulls-of-head-none.json"),
            tehas_core::forge::why(&sample("ticket-writes/validation.stderr"), Some(1)),
        ]
    })
}

fn site(sent: &Sent) -> Box<GitHub> {
    let [issue, pull, pulls, refused] = answers().clone();
    let s = Site {
        sent: Arc::clone(sent),
        issue,
        pull,
        pulls,
        refused,
    };
    Box::new(GitHub::new(PROJECT, Box::new(s)).on_clock(|| 1_795_000_000))
}

fn writer(sent: &Sent) -> Writer {
    Writer {
        tickets: site(sent),
        blueprints: site(sent),
        code_host: site(sent),
    }
}

/// The revision a read of issue 7 gives, so an edit at it is sent.
fn since() -> Revision {
    let sent = Sent::default();
    Revision::new(site(&sent).text_revision(7).unwrap())
}

/// The names `WriteKind`'s serde knows: every variant, `Unknown` too.
fn kinds() -> &'static [&'static str] {
    struct Names;
    /// What a visitor makes: serde's, not JSON.
    type Out<'de, V> = <V as Visitor<'de>>::Value;
    #[derive(Debug)]
    struct Got(&'static [&'static str]);
    impl fmt::Display for Got {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl std::error::Error for Got {}
    impl de::Error for Got {
        fn custom<T: fmt::Display>(_: T) -> Self {
            Got(&[])
        }
    }
    impl<'de> de::Deserializer<'de> for Names {
        type Error = Got;
        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<Out<'de, V>, Got> {
            Err(Got(&[]))
        }
        fn deserialize_enum<V: Visitor<'de>>(
            self,
            _: &'static str,
            variants: &'static [&'static str],
            _: V,
        ) -> Result<Out<'de, V>, Got> {
            Err(Got(variants))
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map struct identifier ignored_any
        }
    }
    let names = WriteKind::deserialize(Names).unwrap_err().0;
    assert!(names.len() > 1, "WriteKind's serde names its variants");
    names
}

fn no() -> BlueprintNo {
    BlueprintNo::new(7).unwrap()
}

fn ticket() -> TicketKey {
    TicketKey::Number(std::num::NonZeroU64::new(7).unwrap())
}

fn change() -> ChangeNo {
    ChangeNo::new(8).unwrap()
}

fn new_change(title: &str, body: &str) -> NewChange {
    NewChange {
        head: Branch::new("work").unwrap(),
        base: Branch::new("main").unwrap(),
        title: title.into(),
        body: Body::Text(body.into()),
        draft: true,
        closes: vec![no()],
    }
}

/// The writes of the variant named `kind` that put `text` where a caller's
/// text goes (a body, and a title where it has one), each with the markers
/// it asks for: `markers` wherever the variant carries markers. None for a
/// variant that carries no caller's text: a mark, a state, a number, a
/// file by sum, a label's own words from the factory's table. An unlisted
/// name panics.
fn writes(kind: &str, text: &str, markers: &[Marker], since: &Revision) -> Option<Vec<Write>> {
    let body = |t: &str| Body::Text(t.into());
    let set = MarkerEdit {
        set: markers.to_vec(),
        ..Default::default()
    };
    let m = markers.to_vec();
    Some(match kind {
        "Mark" | "Unmark" | "Close" | "Reopen" | "SetKind" | "SetPriority" | "WaitOn"
        | "StopWaiting" | "Assign" | "AddPart" | "RemovePart" | "SetTarget" | "Prepare"
        | "EnsureMark" | "CloseBlueprint" | "MarkChange" | "UnmarkChange" | "CloseChange"
        | "Merge" | "Tag" | "Upload" | "PublishRelease" | "Unknown" => return None,
        "TicketNote" => vec![Write::TicketNote {
            ticket: ticket(),
            body: body(text),
            markers: m,
        }],
        "FileTicket" => [(text, "plain"), ("plain", text)]
            .map(|(title, t)| {
                Write::FileTicket(NewTicket {
                    title: title.into(),
                    text: body(t),
                    markers: m.clone(),
                    marks: vec![],
                    kind: None,
                    priority: Priority::Normal,
                    target: None,
                    parent: None,
                })
            })
            .to_vec(),
        "EditTicket" => [(None, Some(text)), (Some(text), None)]
            .map(|(title, t)| Write::EditTicket {
                ticket: ticket(),
                title: title.map(str::to_string),
                text: t.map(body),
                markers: set.clone(),
                since: since.clone(),
            })
            .to_vec(),
        "NewTarget" => vec![Write::NewTarget {
            title: text.into(),
            due: Some(At::new(1)),
        }],
        "EditTarget" => vec![Write::EditTarget {
            id: TargetId::new("t"),
            edit: TargetEdit::Retitle(text.into()),
        }],
        "OpenBlueprint" => [(text, "plain"), ("plain", text)]
            .map(|(title, t)| {
                Write::OpenBlueprint(NewBlueprint {
                    title: title.into(),
                    body: body(t),
                    markers: m.clone(),
                })
            })
            .to_vec(),
        "EditBlueprint" => [(None, Some(text)), (Some(text), None)]
            .map(|(title, t)| Write::EditBlueprint {
                no: no(),
                title: title.map(str::to_string),
                body: t.map(body),
                markers: set.clone(),
                since: since.clone(),
            })
            .to_vec(),
        "Comment" => vec![Write::Comment {
            on: no(),
            body: body(text),
            markers: m,
        }],
        "EditComment" => vec![Write::EditComment {
            id: CommentId::new("12"),
            body: body(text),
            markers: set,
            since: since.clone(),
        }],
        "OpenChange" => vec![
            Write::OpenChange(new_change("plain", text)),
            Write::OpenChange(new_change(text, "plain")),
        ],
        "EditChange" => [(None, Some(text)), (Some(text), None)]
            .map(|(title, t)| Write::EditChange {
                no: change(),
                title: title.map(str::to_string),
                body: t.map(body),
                markers: set.clone(),
            })
            .to_vec(),
        "CommentChange" => vec![Write::CommentChange {
            no: change(),
            body: body(text),
            markers: m,
        }],
        "Propose" => vec![Write::Propose(Proposal {
            base: CommitId::new("abc1234").unwrap(),
            branch: Branch::new("work").unwrap(),
            files: vec![(
                RepoPath::new("VERSION").unwrap(),
                Sum {
                    sha256: Sha256::new("d".repeat(64)).unwrap(),
                    len: 9,
                },
            )],
            message: text.into(),
            change: new_change("plain", text),
            takeover: Takeover::Never,
        })],
        "SetStatus" => vec![Write::SetStatus {
            at: CommitId::new("abc1234").unwrap(),
            status: Status {
                name: "tehas".into(),
                state: StatusState::Passed,
                about: text.into(),
                link: None,
            },
        }],
        "NewRelease" => [(text, "plain"), ("plain", text)]
            .map(|(title, notes)| {
                Write::NewRelease(NewRelease {
                    tag: TagName::new("v2.0.0").unwrap(),
                    title: title.into(),
                    notes: body(notes),
                    draft: true,
                    prerelease: false,
                })
            })
            .to_vec(),
        other => panic!("WriteKind::{other} is not listed: say whether it carries a text"),
    })
}

/// The markers a write asks for.
fn asked(w: &Write) -> Vec<Marker> {
    match w {
        Write::TicketNote { markers, .. }
        | Write::Comment { markers, .. }
        | Write::CommentChange { markers, .. }
        | Write::FileTicket(NewTicket { markers, .. })
        | Write::OpenBlueprint(NewBlueprint { markers, .. }) => markers.clone(),
        Write::EditTicket { markers, .. }
        | Write::EditBlueprint { markers, .. }
        | Write::EditComment { markers, .. }
        | Write::EditChange { markers, .. } => markers.set.clone(),
        _ => vec![],
    }
}

/// The text of each body field a write sent: `body` of an issue, a pull
/// request or a comment.
fn bodies(sent: &[String]) -> Vec<String> {
    sent.iter()
        .filter_map(|s| {
            let v: serde_json::Value = serde_json::from_str(s).unwrap();
            v.get("body").and_then(|b| b.as_str()).map(str::to_string)
        })
        .collect()
}

/// One write of `text`: refused at the writer when the text is foreign, or
/// each body it sent reads exactly the markers it asked for. Ok with the
/// count of bodies sent; a failure names the write, the text and what went
/// wrong.
fn check(w: &Write, text: &str) -> Result<usize, String> {
    let sent = Sent::default();
    let got: Got<Receipt> = writer(&sent).send(w, &NoBlobs);
    let sent = sent.lock().unwrap().clone();
    if foreign_text(text) {
        return match (&got, sent.is_empty()) {
            (Err(Outcome::Refused(_)), true) => Ok(0),
            _ => Err(format!(
                "{:?} not refused at the writer for {text:?}: {got:?}, sent {sent:?}",
                w.kind()
            )),
        };
    }
    let mut want: Vec<String> = asked(w).iter().map(|m| format!("{m:?}")).collect();
    want.sort();
    let sent = bodies(&sent);
    for b in &sent {
        let mut read: Vec<String> = read(b, None)
            .markers
            .iter()
            .map(|m| format!("{m:?}"))
            .collect();
        read.sort();
        if read != want {
            return Err(format!(
                "{:?} posted {b:?} for {text:?}: read {read:?}, asked {want:?}",
                w.kind()
            ));
        }
    }
    Ok(sent.len())
}

/// Whether a corpus row, its classes and its body, is a case of an attack.
type Case = fn(&[String], &str) -> bool;

/// The attack read's cases of #3077, each a test of the recorded corpus.
const ATTACKS: &[(&str, Case)] = &[
    ("a marker after a table that follows a paragraph", |_, b| {
        (b.contains("p\n") || b.contains("p\r\n"))
            && ["-|-", ":-", "|---|"].iter().any(|d| b.contains(d))
    }),
    ("a marker in indented code", |c, b| {
        c.iter().any(|c| c == "code") && !b.contains('`') && !b.contains('~')
    }),
    ("a marker in deep nested lists", |_, b| {
        ["- - ", "> - >", "1. - ", "- \n     10) "]
            .iter()
            .any(|n| b.contains(n))
    }),
    ("a marker after `\\<!--`", |_, b| {
        b.contains(&format!("\\{M}"))
    }),
    ("a marker in a link's destination or title", |_, b| {
        b.contains("](") || b.contains("]:")
    }),
    ("a marker in a quote", |c, _| c.iter().any(|c| c == "quote")),
];

/// The most bodies taken of each case, spread over the corpus's order:
/// the whole corpus is the oracle's to read, and a write of each body
/// through every variant is slow.
const PER_ATTACK: usize = 60;

/// The corpus bodies of the attack read, at least one of each case.
fn corpus() -> Vec<String> {
    let rows = rows();
    let mut out = Vec::new();
    for (name, is) in ATTACKS {
        let of: Vec<&String> = rows
            .iter()
            .filter(|(c, b)| is(c, b))
            .map(|r| &r.1)
            .collect();
        assert!(!of.is_empty(), "the corpus holds no case of {name}");
        let step = of.len().div_ceil(PER_ATTACK);
        out.extend(of.into_iter().step_by(step).cloned());
    }
    // A hundred nested items read; one more is past the reader's cap, and
    // what is inside it is dead (`markers_dead.rs`).
    for depth in [100, 101] {
        out.push(format!("{}{M}", "- ".repeat(depth)));
    }
    out.sort();
    out.dedup();
    out
}

/// Each marker of a body in the three forms a caller's text may carry it:
/// wrapped, as its closing wrapper, and bare.
fn forms(body: &str) -> [String; 3] {
    let bare = M.trim_start_matches("<!-- ").trim_end_matches(" -->");
    [
        body.to_string(),
        body.replace(M, "<!-- /tehas:finish -->"),
        body.replace(M, bare),
    ]
}

#[test]
fn every_write_kind_is_listed() {
    let since = since();
    for kind in kinds() {
        let _ = writes(kind, "plain", &[], &since);
    }
    let texts = kinds()
        .iter()
        .filter(|k| writes(k, "plain", &[], &since).is_some())
        .count();
    assert_eq!(texts, 15, "the variants that carry a caller's text");
}

/// A marker a caller's text holds, in any place of the attack read, is
/// refused at the writer or dead in what is posted.
#[test]
fn a_marker_in_a_callers_text_is_refused_or_dead_in_every_write() {
    let since = since();
    let mut failed = Vec::new();
    for body in corpus() {
        for text in forms(&body) {
            for kind in kinds() {
                for w in writes(kind, &text, &[], &since).unwrap_or_default() {
                    if let Err(e) = check(&w, &text) {
                        failed.push(e);
                    }
                }
            }
        }
    }
    assert!(
        failed.is_empty(),
        "{} writes:\n{}",
        failed.len(),
        failed[..failed.len().min(20)].join("\n")
    );
}

/// A restraint the factory writes after the text before each attack's
/// marker reads live in what is posted, or the write is refused.
#[test]
fn a_restraint_after_each_attacks_text_reads_or_is_refused() {
    let since = since();
    let mut failed = Vec::new();
    let mut posted = 0;
    for body in corpus() {
        let text = kept(&body[..body.find(M).unwrap()]);
        for r in restraints() {
            for kind in kinds() {
                for w in writes(kind, text, std::slice::from_ref(&r), &since).unwrap_or_default() {
                    match check(&w, text) {
                        Ok(n) => posted += n,
                        Err(e) => failed.push(e),
                    }
                }
            }
        }
    }
    assert!(posted > 0, "some restraint was posted");
    assert!(
        failed.is_empty(),
        "{} writes:\n{}",
        failed.len(),
        failed[..failed.len().min(20)].join("\n")
    );
}
