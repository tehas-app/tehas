//! The `GitHub` type: the forge's terms answered from GitHub's labels and
//! their events, through a [`GhRunner`].

use crate::gh;
use crate::table::{is_label, APPROVAL_LABEL};
use tehas_core::forge::{Approval, Forge, ForgeError, GhRunner, LabelEvent};

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
/// The ordering of the events that answers [`Forge::label_state`] and
/// [`Forge::approval`] is the rules of [`gh`].
pub struct GitHub {
    /// `owner/name`, as `TEHAS_REPO` gives it.
    pub repo: String,
    runner: Box<dyn GhRunner>,
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
        }
    }

    /// Ask gh, keeping the reason a failed call gave. `args` are the words
    /// after gh; `call` is what a [`ForgeError`] names.
    fn call(&mut self, call: &str, args: &[&str]) -> Result<String, ForgeError> {
        match self.runner.run(args) {
            Ok(out) => Ok(out),
            Err(why) => Err(ForgeError::new(call, why)),
        }
    }

    /// The reason of a read that is no answer, named after the read the
    /// adapter was making: the reader's words, kept whole.
    fn reason_of(why: String) -> ForgeError {
        ForgeError::new("the issue's label events", why)
    }

    /// The `-F` fields of a GraphQL question of the repository.
    fn owner_name(&self) -> (String, String) {
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

    /// gh's answer to the events call, every page of it.
    pub(crate) fn event_pages(&mut self, issue: u64) -> Result<String, ForgeError> {
        let path = format!("repos/{}/issues/{issue}/events?per_page=100", self.repo);
        self.call("the issue's label events", &["api", &path, "--paginate"])
    }
}

impl Forge for GitHub {
    fn labels(&mut self, issue: u64) -> Result<Vec<String>, ForgeError> {
        let out = self.issue_answer(issue)?;
        // The answer is read as an issue with labels or not at all: an answer
        // that is no issue is not an issue with no labels.
        gh::parse_labels(&out).ok_or_else(|| ForgeError::new("the issue", "no labels to read"))
    }

    fn label_state(&mut self, issue: u64, label: &str) -> Result<Option<LabelEvent>, ForgeError> {
        Ok(gh::latest_event(&self.events(issue)?, label))
    }

    fn label_events(&mut self, issue: u64) -> Result<Vec<tehas_core::events::Event>, ForgeError> {
        Ok(self.events(issue)?.into_iter().map(public).collect())
    }

    fn applied_by(&mut self, issue: u64, label: &str) -> Result<Option<Approval>, ForgeError> {
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

    fn approval(&mut self, issue: u64) -> Result<Option<Approval>, ForgeError> {
        self.applied_by(issue, APPROVAL_LABEL)
    }

    fn issue_types(&mut self) -> Result<Vec<String>, ForgeError> {
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

    fn typed_by(&mut self, issue: u64, type_name: &str) -> Result<Option<Approval>, ForgeError> {
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

    fn add_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError> {
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

    fn remove_label(&mut self, issue: u64, label: &str) -> Result<(), ForgeError> {
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
fn uri(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
