//! The `GitHub` type: the forge's terms answered from GitHub's labels and
//! their events, through a [`GhRunner`], and the values they answer with.
//! The tickets port is answered over these terms (`tickets.rs`).

use crate::gh;
use crate::table::{is_label, APPROVAL_LABEL};
use tehas_core::forge::GhRunner;

/// A term the forge could not answer, as a value rather than a panic.
///
/// `call` names the term that stopped, so one line says what was attempted,
/// and `why` is gh's reason, which the runner made from its fixed list of
/// error words and never from the whole of its answer: a caller does not hold
/// what may name a host or carry a token. An error is never an empty answer,
/// and an empty answer is never an error.
#[derive(Clone, Debug)]
pub struct ForgeError {
    pub call: String,
    /// The reason, without its `": "`: the line adds that. Empty when the
    /// call gave none.
    pub why: String,
}

impl ForgeError {
    /// The error of one call, with its reason, which may be empty. A reason is
    /// what a runner said, not gh's own words: only the fixed list of them
    /// belongs in a log line.
    pub fn new(call: &str, why: impl Into<String>) -> ForgeError {
        ForgeError {
            call: call.to_string(),
            why: why.into(),
        }
    }

    /// The line a caller logs, shaped as the poll's lines are: what failed,
    /// and gh's own words when it had any. The separator is made here, so a
    /// reason never has to carry it.
    pub fn line(&self) -> String {
        if self.why.is_empty() {
            format!("ERROR: {} failed", self.call)
        } else {
            format!("ERROR: {} failed: {}", self.call, self.why)
        }
    }
}

impl std::fmt::Display for ForgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.line())
    }
}

impl std::error::Error for ForgeError {}

/// Who applied a label, and when, as the approval term answers. `None` is the
/// answer of an issue nothing applied; an empty `actor` inside `Some` is a fact
/// and not a refusal, the event's account being gone. The caller compares the
/// account with the allowed list, which is what keeps authorisation where the
/// site put it (AGENTS.md rule 5): the adapter answers who, and never whether
/// that account may start anything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Approval {
    /// The account that applied the label, empty when the event names none.
    pub actor: String,
    /// The application's stamp, empty when it has none.
    pub at: String,
}

/// The latest `labeled` or `unlabeled` event of one label: its kind, the
/// account behind it, and its time. A field is empty when the event names
/// none, which is what GitHub answers for an event whose actor is gone; an
/// event with no time is read as older than any stamped one.
///
/// An empty `actor` is a fact, not a refusal: the caller compares it with the
/// allowed list and decides that nobody was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelEvent {
    /// `labeled` or `unlabeled`.
    pub kind: String,
    /// The account that changed the label, empty when the event names none.
    pub actor: String,
    /// The event's stamp, empty when it has none.
    pub at: String,
}

/// gh's answer to the events call as [`tehas_core::events`] reads events. Why not
/// when it says nothing of them: an empty answer included.
pub fn read_events(out: &str) -> Result<Vec<tehas_core::events::Event>, String> {
    Ok(gh::parse_events(out)?.into_iter().map(public).collect())
}

fn public(e: gh::Event) -> tehas_core::events::Event {
    tehas_core::events::Event {
        event: e.event,
        label: e.label,
        actor: e.actor,
        at: e.at,
    }
}

/// The GitHub adapter: every term backed by labels, as the bash backs them
/// today. It holds the repository it asks and the runner that asks GitHub, and
/// reads nothing else.
///
/// The ordering of the events that answers [`GitHub::label_state`] and
/// [`GitHub::approval`] is the rules of [`gh`].
pub struct GitHub {
    /// `owner/name`, as `TEHAS_REPO` gives it.
    pub repo: String,
    runner: Box<dyn GhRunner>,
    /// The clock a spent budget's time is read from.
    pub(crate) now: fn() -> i64,
    /// Mark names the site gave that no mark can have, not yet taken.
    pub(crate) left_out: Vec<String>,
    /// The factory's own account, given by the site that made this handle:
    /// None for a handle made with [`GitHub::new`], which answers no account
    /// `Ours`.
    pub(crate) bot: Option<crate::tickets::Bot>,
    /// The node id and the issue of each comment a read of comments found
    /// edited, by its REST id: an edit history is asked by node id.
    pub(crate) comment_nodes: std::collections::HashMap<u64, (String, tehas_ports::BlueprintNo)>,
}

impl GitHub {
    /// The adapter of one repository, asking GitHub through `runner`. The
    /// runner is the one that runs gh: on a host,
    /// [`tehas_core::forge::Bounded`], built with the
    /// path of the instance's `host/shim/gh`, which takes the bot's token from
    /// `TEHAS_GH_TOKEN`. Where the program, its token and the bound come from
    /// is the caller's setting to read; the next issues of #800 say which
    /// command owns it.
    pub fn new(repo: impl Into<String>, runner: Box<dyn GhRunner>) -> GitHub {
        GitHub {
            repo: repo.into(),
            runner,
            now: tehas_core::time::now,
            left_out: Vec::new(),
            bot: None,
            comment_nodes: std::collections::HashMap::new(),
        }
    }

    /// The same adapter reading the time from `now`, so a replay answers a
    /// fixed time.
    pub fn on_clock(self, now: fn() -> i64) -> GitHub {
        GitHub { now, ..self }
    }

    /// The names the site gave that no mark can have, each once, since the
    /// last take: for the caller to report (Decided 61).
    pub fn take_left_out(&mut self) -> Vec<String> {
        std::mem::take(&mut self.left_out)
    }

    /// Ask gh, keeping the reason a failed call gave. `args` are the words
    /// after gh; `call` is what a [`ForgeError`] names.
    pub(crate) fn call(&mut self, call: &str, args: &[&str]) -> Result<String, ForgeError> {
        match self.runner.run(args) {
            Ok(out) => Ok(out),
            Err(why) => Err(ForgeError::new(call, why)),
        }
    }

    /// [`GitHub::call`], keeping what gh answered when it failed: gh exits
    /// 1 on any GraphQL `errors`, so a question of several items reads
    /// which of them failed from the answer.
    pub(crate) fn call_answer(
        &mut self,
        args: &[&str],
    ) -> Result<String, tehas_core::forge::Failed> {
        self.runner.run_answer(args)
    }

    /// [`GitHub::call`] with `input` on gh's standard input, the way a write
    /// sends its text: never in an argument.
    pub(crate) fn call_input(
        &mut self,
        call: &str,
        args: &[&str],
        input: &str,
    ) -> Result<String, ForgeError> {
        self.runner
            .run_input(args, input)
            .map_err(|why| ForgeError::new(call, why))
    }

    /// The reason of a read that is no answer, named after the read the
    /// adapter was making: the reader's words, kept whole.
    fn reason_of(why: String) -> ForgeError {
        ForgeError::new("the issue's label events", why)
    }

    /// The `-F` fields of a GraphQL question of the repository.
    pub(crate) fn owner_name(&self) -> (String, String) {
        let (o, n) = self.repo.split_once('/').unwrap_or((&self.repo, ""));
        (format!("owner={o}"), format!("name={n}"))
    }

    /// The issue's label events, whole: the REST list with `--paginate`, so an
    /// issue with more events than one page holds is read to the end rather
    /// than to its first hundred.
    fn events(&mut self, issue: u64) -> Result<Vec<gh::Event>, ForgeError> {
        let out = self.event_pages(issue)?;
        gh::parse_events(&out).map_err(Self::reason_of)
    }

    /// gh's answer to the issue's REST read.
    pub(crate) fn issue_answer(&mut self, issue: u64) -> Result<String, ForgeError> {
        let path = format!("repos/{}/issues/{issue}", self.repo);
        self.call("the issue", &["api", &path])
    }

    /// The revision of an issue's title and body: equal answers mean neither
    /// changed. Labels and comments do not move it. The sums are the ones a
    /// start holds its text to (`tehas_core::approval`).
    pub fn text_revision(&mut self, issue: u64) -> Result<String, ForgeError> {
        let out = self.issue_answer(issue)?;
        gh::text_revision(&out).ok_or_else(|| ForgeError::new("the issue", "no title to read"))
    }

    /// gh's answer to the events call, every page of it.
    pub(crate) fn event_pages(&mut self, issue: u64) -> Result<String, ForgeError> {
        let path = format!("repos/{}/issues/{issue}/events?per_page=100", self.repo);
        self.call("the issue's label events", &["api", &path, "--paginate"])
    }
}

/// The forge's terms. `issue` is the issue's number in every term. Every
/// term makes its own call and answers a value or a [`ForgeError`]: an empty
/// value is the site's answer, and an answer that could not be read is an
/// error, never an empty list.
impl GitHub {
    /// The labels the issue carries, in the order the site gave them.
    pub fn labels(&mut self, issue: u64) -> Result<Vec<String>, ForgeError> {
        let out = self.issue_answer(issue)?;
        // The answer is read as an issue with labels or not at all: an answer
        // that is no issue is not an issue with no labels.
        gh::parse_labels(&out).ok_or_else(|| ForgeError::new("the issue", "no labels to read"))
    }

    /// The newest `labeled` or `unlabeled` event of `label`, or None.
    pub fn label_state(
        &mut self,
        issue: u64,
        label: &str,
    ) -> Result<Option<LabelEvent>, ForgeError> {
        Ok(gh::latest_event(&self.events(issue)?, label))
    }

    /// Every label event of the issue, for a rule of the caller's to read.
    pub fn label_events(
        &mut self,
        issue: u64,
    ) -> Result<Vec<tehas_core::events::Event>, ForgeError> {
        Ok(self.events(issue)?.into_iter().map(public).collect())
    }

    /// The newest application of `label`, or None when no event applied it.
    /// A removal never answers it: authorisation is who applied the label.
    pub fn applied_by(&mut self, issue: u64, label: &str) -> Result<Option<Approval>, ForgeError> {
        // The applications alone, as `tehas_label_actor` reads them: the
        // newest event of the label is not the newest application of it, and
        // a removal would hand the caller the remover's account to compare
        // with the allowed list.
        Ok(
            gh::latest_applied(&self.events(issue)?, label).map(|e| Approval {
                actor: e.actor,
                at: e.at,
            }),
        )
    }

    /// Who applied the label that approves work most recently, and when.
    pub fn approval(&mut self, issue: u64) -> Result<Option<Approval>, ForgeError> {
        self.applied_by(issue, APPROVAL_LABEL)
    }

    /// The names of the issue types an issue of the repository can be given.
    pub fn issue_types(&mut self) -> Result<Vec<String>, ForgeError> {
        let (owner, name) = self.owner_name();
        let out = self.call(
            "the issue types",
            &[
                "api",
                "graphql",
                "-F",
                &owner,
                "-F",
                &name,
                "-f",
                &format!("query={}", gh::ISSUE_TYPES),
            ],
        )?;
        gh::parse_issue_types(&out).map_err(|why| ForgeError::new("the issue types", why))
    }

    /// Who gave the issue the type it holds now, when that is `type_name`.
    pub fn typed_by(
        &mut self,
        issue: u64,
        type_name: &str,
    ) -> Result<Option<Approval>, ForgeError> {
        let (owner, name) = self.owner_name();
        let out = self.call(
            "the issue's type events",
            &[
                "api",
                "graphql",
                "-F",
                &owner,
                "-F",
                &name,
                "-F",
                &format!("number={issue}"),
                "-f",
                &format!("query={}", gh::TYPE_EVENTS),
            ],
        )?;
        gh::parse_typed_by(&out, type_name)
            .map_err(|why| ForgeError::new("the issue's type events", why))
    }

    /// Put `label` on the issue. A name no label can have is refused
    /// without a call.
    pub fn add_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError> {
        if !is_label(label) {
            return Err(ForgeError::new(
                "the label change",
                format!("{label} is no label name to write: an empty one, a long one, or one outside the characters tehas's labels use"),
            ));
        }
        let path = format!("repos/{}/issues/{issue}/labels", self.repo);
        // The label goes as a field of the request, as the bash and the
        // dispatcher's writer send it, and not into a path: it keeps the
        // characters of the site's answer. The REST list endpoint, not
        // `gh issue edit`, which asks GitHub a
        // GraphQL question a bot's token may not answer (issue #120): a mark
        // made that way fails on every issue. Adding a label GitHub already
        // has is not an error, so this is safe to ask twice. The whole call is
        // one argv, so the words here are the words the poll's write log
        // records, and the action journal of #800 records the same call.
        self.call(
            "the label change",
            &[
                "api",
                "-X",
                "POST",
                &path,
                "-f",
                &format!("labels[]={label}"),
            ],
        )?;
        Ok(())
    }

    /// Take `label` off the issue. One that is not there is the state
    /// wanted. A name no label can have is refused without a call.
    pub fn remove_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError> {
        if !is_label(label) {
            return Err(ForgeError::new(
                "the label change",
                format!("{label} is no label name to write: an empty one, a long one, or one outside the characters tehas's labels use"),
            ));
        }
        // The name is encoded, so a character outside the tehas vocabulary -
        // a `:`, a space - stays inside the last segment of the path and moves
        // the DELETE nowhere else. The repository is the settings' own
        // `owner/name`, as every reader of it passes it to this path.
        let path = format!("repos/{}/issues/{issue}/labels/{}", self.repo, uri(label));
        // A label that is not on the issue answers 404, which is the state
        // wanted: as the poll's refresh_unmark reads that answer, it is
        // success. Any other failure is the caller's to log.
        match self.call("the label change", &["api", "-X", "DELETE", &path]) {
            Ok(_) => Ok(()),
            Err(e) if gh::says_not_found(&e.why) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// `s` as jq's `@uri` writes it, so a value cannot carry a path or a query of
/// its own past the segment it sits in. Applied to the one part of a path the
/// site's own answer fills: the label of a removal. The dispatcher's
/// `read::build::uri` holds the same function, for the branch of a query.
pub(crate) fn uri(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
