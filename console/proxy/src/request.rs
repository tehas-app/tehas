//! What one request of a contractor asks for, and whether this proxy answers
//! it. The decision is made of three readings and no guess: the path, against
//! the list below; the token, against the tokens this process issued; and the
//! job and its contractor, against the factory's own records.

/// The paths a cycle's container sends to a model endpoint, and the reason
/// each is here. Everything else is refused with 404 before an upstream call
/// is made: a contractor holds no model key, so what it may ask the factory's
/// model for is decided here and not by what it sends.
///
/// The list is the shape Pi asks of an `openai-completions` provider, which
/// is what `models.json` of an instance names, and the suffixes are matched
/// after any base path the endpoint's `baseUrl` carries. A versioned path is
/// the same ask under another name.
pub const PATHS: [(&str, &str); 6] = [
    // The chat completion itself, streamed or not: the whole reason a
    // contractor calls the factory.
    ("chat/completions", "the completion a cycle's agents run on"),
    // Pi asks what an endpoint offers before it picks a model of a role, and
    // tehas_core::models::ask does the same when a cycle starts.
    (
        "models",
        "the models the endpoint offers, for a role's pick",
    ),
    // The same list under the name some gateways answer.
    ("list", "the model list under another name"),
    // The count an agent checks a budget against before it sends a long turn.
    ("count-tokens", "the token count of a prompt"),
    // A small turn that asks the model to name the next step, the last shape
    // a cycle reaches for when it ends a workstream.
    ("completions", "a completion that is not a chat"),
    // The health of the endpoint, which the contractor's doctor shows a
    // person. It carries no prompt and no source.
    ("health", "whether the endpoint answers at all"),
];

/// The path of a request line, with any query taken off, and the provider it
/// names when the path carries one. `/<base>/v1/chat/completions` is one ask
/// of the model, whatever the endpoint's own base path is.
#[derive(Debug, PartialEq, Eq)]
pub struct Ask<'a> {
    /// The path as it was sent, without its query.
    pub path: &'a str,
    /// Whether the request asks for a stream. Read from the body only for the
    /// log, so this is always false at the decision: the proxy never reads a
    /// body to decide, which is rule 6.
    pub stream: bool,
}

impl<'a> Ask<'a> {
    pub fn new(path: &'a str) -> Ask<'a> {
        Ask {
            path: path.split_once('?').map_or(path, |(p, _)| p),
            stream: false,
        }
    }

    /// The rule that answers this ask: the entry of [`PATHS`] whose suffix
    /// the path ends with, with the reason a cycle needs it. None is a 404
    /// and no upstream call.
    pub fn rule(&self) -> Option<(&'static str, &'static str)> {
        let tail = self.path.trim_end_matches('/');
        // A segment match is what the entry names: /v1/chat/completions is
        // the chat entry, since the whole of its name matches. A bare
        // completions entry matches nothing above /v1/completions.
        PATHS
            .iter()
            .find(|(suffix, _)| tail == *suffix || tail.ends_with(&format!("/{suffix}")))
            .copied()
    }

    /// Why this ask is refused, or None for one this proxy forwards. The
    /// reason names the path, cut to what one line holds, and never the
    /// token, which is rule 6's: a log line is read by a person, and a path
    /// is what a probe chooses.
    pub fn refused(&self) -> Option<String> {
        self.rule().map_or_else(
            || {
                Some(format!(
                    "{} is not a model path of a cycle",
                    crate::log::shown(self.path)
                ))
            },
            |_| None,
        )
    }
}

/// Why a request was refused, in the words of one log line.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// The token is none, or not one this process issued.
    UnknownToken,
    /// The token was issued for a job that is no longer going. The word is
    /// the state the factory's record holds.
    JobOver(&'static str),
    /// The job's record names another attempt or another contractor than the
    /// token was issued for.
    Elsewhere,
    /// The contractor of the job is revoked, or has no record at all.
    Revoked,
    /// The path is outside [`PATHS`].
    Path(String),
    /// The token is a job's token of the job scope, which the model paths
    /// never take.
    Scope,
}

impl Refused {
    /// The status the contractor is answered with. A refusal of the token or
    /// of the job is 401 or 403, so a contractor tells it apart from a model
    /// that is busy; a path outside the list is 404, so a contractor cannot
    /// read it as a model that refused.
    pub fn status(&self) -> u16 {
        match self {
            Refused::UnknownToken => 401,
            Refused::JobOver(_) | Refused::Elsewhere | Refused::Revoked | Refused::Scope => 403,
            Refused::Path(_) => 404,
        }
    }

    /// The one line of the log: what was refused and why, with the job and
    /// the contractor the token names. Never a body, never a token, never the
    /// text of a prompt.
    pub fn line(&self, job: Option<i64>, contractor: Option<&str>) -> String {
        let who = match (job, contractor) {
            (Some(job), Some(name)) => format!("job {job} of contractor {name}"),
            (Some(job), None) => format!("job {job}"),
            _ => "no job".to_string(),
        };
        let what = match self {
            Refused::UnknownToken => "a token this factory issued for no job".to_string(),
            Refused::JobOver(state) => format!("a job whose record says {state}"),
            Refused::Elsewhere => {
                "a job whose record names another attempt or contractor".to_string()
            }
            Refused::Revoked => "a contractor that is revoked or unknown".to_string(),
            Refused::Path(path) => format!("the path {path}"),
            Refused::Scope => "a token of the job channel on a model path".to_string(),
        };
        format!("refused {who}: {what}")
    }
}

/// The three readings of one request, in the order that costs least: the
/// path, which needs nothing; the token, which reads this process's own
/// table; and the records, which read the factory. `asked` is what the
/// contractor sent, and nothing of what it sent decides any of them.
pub fn judge(token: Option<&str>, ask: &Ask<'_>, records: &dyn Records) -> Result<Live, Refused> {
    if let Some(why) = ask.refused() {
        return Err(Refused::Path(why));
    }
    let live = match token.and_then(|t| records.live(t)) {
        Some(live) => live,
        None if token.is_some_and(|t| records.wrong_scope(t)) => return Err(Refused::Scope),
        None => return Err(Refused::UnknownToken),
    };
    match records.state_of(live.job) {
        Some(state) if state.ends() => return Err(Refused::JobOver(state.word())),
        None => return Err(Refused::JobOver("absent")),
        _ => {}
    }
    if records.holder(live.job) != Some((live.attempt, live.contractor.clone())) {
        return Err(Refused::Elsewhere);
    }
    if !records.allowed(&live.contractor) {
        return Err(Refused::Revoked);
    }
    Ok(live)
}

/// What a token names: the job it was issued for, and the contractor that
/// was given it. Both come from this process's own table, never from a field
/// of the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Live {
    pub job: i64,
    pub attempt: i64,
    pub contractor: String,
}

/// What the proxy reads of the factory's records: the jobs, their states, and
/// who is allowed to work. The proxy decides nothing from what a contractor
/// sends: `contractor` and `attempt` of [`Live`] are what the issuing process
/// wrote, and `state_of` and `allowed` read the factory's own records again
/// on every request, so a job that ended or a contractor revoked mid-stream
/// is refused by the next request and not remembered wrongly.
/// A request is counted once. [`Records::asked`] is the ask of rule 5, and a
/// request that names a live job was already counted by the reading that
/// named it ([`Records::live`]), so it is said again only for a request whose
/// token names no job at all.
pub trait Records {
    /// The job and contractor this token was issued for, None for a token
    /// this process never issued or one whose job has been cleaned up.
    fn live(&self, token: &str) -> Option<Live>;
    /// Whether the token names a live job in the job scope: a token of the
    /// contractor program, refused here with 403 rather than as unknown.
    fn wrong_scope(&self, _token: &str) -> bool {
        false
    }
    /// The state of a job as the factory's record has it. None for a job the
    /// records do not know: the proxy refuses it as one that is over.
    fn state_of(&self, job: i64) -> Option<State>;
    /// The attempt and the contractor the job's record names. A token issued
    /// for another attempt or another contractor is refused.
    fn holder(&self, job: i64) -> Option<(i64, String)>;
    /// Whether the contractor may work at all: not revoked, and known.
    fn allowed(&self, contractor: &str) -> bool;
    /// One request carried this token, counted for its job: the ask of a
    /// request whose token names no job, since a request that names a live job
    /// was counted by [`Self::live`] and is never counted twice. A token of no
    /// job counts for nobody.
    fn asked(&self, token: &str);
}

/// The states of a job that end it: the two of the spec's machine that are
/// not a failure, and the two that are. Any of them refuses the job's token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Offered,
    Running,
    Stopping,
    Finished,
    Released,
    Lost,
}

impl State {
    /// Whether a token of a job in this state is refused. `offered`,
    /// `running` and `stopping` are the states a live job is in: a contractor
    /// that is ending a job still needs its model to finish its notes.
    pub fn ends(&self) -> bool {
        matches!(self, State::Finished | State::Released | State::Lost)
    }

    /// The word the factory's record holds, for the log.
    pub fn word(&self) -> &'static str {
        match self {
            State::Offered => "offered",
            State::Running => "running",
            State::Stopping => "stopping",
            State::Finished => "finished",
            State::Released => "released",
            State::Lost => "lost",
        }
    }

    pub fn of(word: &str) -> Option<State> {
        Some(match word {
            "offered" => State::Offered,
            "running" => State::Running,
            "stopping" => State::Stopping,
            "finished" => State::Finished,
            "released" => State::Released,
            "lost" => State::Lost,
            _ => return None,
        })
    }
}

/// The token of a request head: `Authorization: Bearer <token>`, the form Pi
/// sends an `apiKey` of a provider in. Nothing else is a token: a contractor
/// that sends its own key in another field is answered 401, not guessed at.
pub fn bearer(head: &str) -> Option<String> {
    head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.trim().eq_ignore_ascii_case("authorization") {
            return None;
        }
        let value = value.trim();
        let (word, token) = value.split_once(' ')?;
        if !word.eq_ignore_ascii_case("Bearer") {
            return None;
        }
        let token = token.trim();
        (!token.is_empty() && !token.contains(char::is_whitespace)).then(|| token.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// The records of a test: the tokens issued, the states of the jobs, and
    /// who may work. Nothing here reads a file or a database.
    pub struct Table {
        pub issued: HashMap<String, Live>,
        pub states: HashMap<i64, State>,
        pub holders: HashMap<i64, (i64, String)>,
        pub allowed: Vec<String>,
    }

    impl Table {
        pub fn new() -> Table {
            Table {
                issued: HashMap::new(),
                states: HashMap::new(),
                holders: HashMap::new(),
                allowed: Vec::new(),
            }
        }
        pub fn job(mut self, token: &str, job: i64, contractor: &str, state: State) -> Table {
            self.issued.insert(
                token.to_string(),
                Live {
                    job,
                    attempt: 1,
                    contractor: contractor.to_string(),
                },
            );
            self.states.insert(job, state);
            self.holders.insert(job, (1, contractor.to_string()));
            self
        }
        pub fn allow(mut self, name: &str) -> Table {
            self.allowed.push(name.to_string());
            self
        }
    }

    impl Records for Table {
        fn live(&self, token: &str) -> Option<Live> {
            self.issued.get(token).cloned()
        }
        fn state_of(&self, job: i64) -> Option<State> {
            self.states.get(&job).copied()
        }
        fn holder(&self, job: i64) -> Option<(i64, String)> {
            self.holders.get(&job).cloned()
        }
        fn allowed(&self, contractor: &str) -> bool {
            self.allowed.iter().any(|c| c == contractor)
        }
        fn asked(&self, _token: &str) {}
    }

    fn ask(path: &str) -> Ask<'_> {
        Ask::new(path)
    }

    fn ready() -> Table {
        Table::new()
            .job("tok-1", 7, "gpu1", State::Running)
            .allow("gpu1")
    }

    #[test]
    fn a_model_path_of_a_cycle_is_forwarded_and_the_rest_is_not() {
        for path in [
            "/v1/chat/completions",
            "/chat/completions",
            "http://127.0.0.1:8090/v1/chat/completions"
                .strip_prefix("http://127.0.0.1:8090")
                .unwrap(),
            "/v1/models",
            "/models",
            "/v1/models/list",
            "/v1/count-tokens",
            "/v1/completions",
            "/health",
            "/v1/chat/completions/",
        ] {
            let a = ask(path);
            assert!(a.rule().is_some(), "{path} is a path a cycle needs");
            assert_eq!(
                judge(Some("tok-1"), &a, &ready()),
                Ok(Live {
                    job: 7,
                    attempt: 1,
                    contractor: "gpu1".to_string()
                })
            );
        }
        for path in [
            "/v1/embeddings",
            "/v1/files",
            "/v1/messages",
            "/v1/responses",
            "/v1beta/models/gemini:generateContent",
            "/",
            "",
            "/v1/chat",
            "/v1/chat/completions/extra",
            "/admin",
            "/v1/models/secret",
        ] {
            let a = ask(path);
            assert_eq!(
                judge(Some("tok-1"), &a, &ready()),
                Err(Refused::Path(format!(
                    "{path} is not a model path of a cycle"
                ))),
                "{path}"
            );
            assert_eq!(Refused::Path(String::new()).status(), 404);
        }
    }

    #[test]
    fn every_path_of_the_list_is_named_once_and_says_why_it_is_here() {
        let mut seen: Vec<&str> = PATHS.iter().map(|(p, _)| *p).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before, "a path is listed twice");
        assert!(
            PATHS.iter().all(|(_, why)| !why.is_empty()),
            "a path names no reason"
        );
        assert!(
            PATHS.iter().any(|(p, _)| *p == "chat/completions"),
            "the completion itself is not forwarded"
        );
    }

    #[test]
    fn a_query_is_no_part_of_the_path() {
        assert_eq!(
            ask("/v1/chat/completions?x=1&stream=true")
                .rule()
                .map(|p| p.0),
            Some("chat/completions")
        );
        assert_eq!(
            ask("/v1/chat/completions?stream=true").refused(),
            None,
            "a query would decide the ask"
        );
    }

    #[test]
    fn a_token_of_no_job_is_401_and_names_no_path_as_the_reason() {
        let a = ask("/v1/chat/completions");
        assert_eq!(judge(None, &a, &ready()), Err(Refused::UnknownToken));
        assert_eq!(judge(Some(""), &a, &ready()), Err(Refused::UnknownToken));
        assert_eq!(
            judge(Some("tok-2"), &a, &ready()),
            Err(Refused::UnknownToken)
        );
        assert_eq!(Refused::UnknownToken.status(), 401);
        assert_eq!(
            Refused::UnknownToken.line(Some(7), Some("gpu1")),
            "refused job 7 of contractor gpu1: a token this factory issued for no job"
        );
    }

    #[test]
    fn a_path_outside_the_list_is_refused_before_the_token_is_asked() {
        // No token at all: a request for a path the proxy does not serve is
        // refused whatever it carried, so a probe cannot tell a live token
        // from a dead one by the status it is answered with.
        assert_eq!(
            judge(None, &ask("/v1/embeddings"), &ready()),
            Err(Refused::Path(
                "/v1/embeddings is not a model path of a cycle".to_string()
            ))
        );
    }

    #[test]
    fn a_job_that_has_ended_is_refused_and_its_state_is_the_reason() {
        for (state, word) in [
            (State::Finished, "finished"),
            (State::Released, "released"),
            (State::Lost, "lost"),
        ] {
            let table = Table::new().job("tok-1", 7, "gpu1", state).allow("gpu1");
            assert_eq!(
                judge(Some("tok-1"), &ask("/v1/chat/completions"), &table),
                Err(Refused::JobOver(word)),
                "{word}"
            );
            assert_eq!(Refused::JobOver(word).status(), 403);
            assert_eq!(
                Refused::JobOver(word).line(Some(7), Some("gpu1")),
                format!("refused job 7 of contractor gpu1: a job whose record says {word}")
            );
        }
    }

    #[test]
    fn a_job_that_is_still_going_is_answered() {
        for state in [State::Offered, State::Running, State::Stopping] {
            let table = Table::new().job("tok-1", 7, "gpu1", state).allow("gpu1");
            assert!(
                judge(Some("tok-1"), &ask("/v1/chat/completions"), &table).is_ok(),
                "{}",
                state.word()
            );
        }
    }

    #[test]
    fn a_job_the_records_do_not_know_is_over() {
        let table = Table::new();
        assert_eq!(
            judge(Some("tok-1"), &ask("/v1/chat/completions"), &table),
            Err(Refused::UnknownToken),
            "an unknown token is refused before the records are read"
        );
        let mut table = Table::new()
            .job("tok-1", 7, "gpu1", State::Running)
            .allow("gpu1");
        table.states.remove(&7);
        assert_eq!(
            judge(Some("tok-1"), &ask("/v1/chat/completions"), &table),
            Err(Refused::JobOver("absent"))
        );
    }

    #[test]
    fn a_token_of_another_attempt_or_contractor_is_refused() {
        for holder in [(2, "gpu1"), (1, "gpu2")] {
            let mut table = ready();
            table.holders.insert(7, (holder.0, holder.1.to_string()));
            assert_eq!(
                judge(Some("tok-1"), &ask("/v1/chat/completions"), &table),
                Err(Refused::Elsewhere),
                "{holder:?}"
            );
            assert_eq!(Refused::Elsewhere.status(), 403);
        }
        assert!(judge(Some("tok-1"), &ask("/v1/chat/completions"), &ready()).is_ok());
    }

    #[test]
    fn a_revoked_contractor_is_refused_and_so_is_an_unknown_one() {
        let table = Table::new().job("tok-1", 7, "gpu1", State::Running);
        assert_eq!(
            judge(Some("tok-1"), &ask("/v1/chat/completions"), &table),
            Err(Refused::Revoked)
        );
        assert_eq!(Refused::Revoked.status(), 403);
        assert_eq!(
            Refused::Revoked.line(Some(7), Some("gpu1")),
            "refused job 7 of contractor gpu1: a contractor that is revoked or unknown"
        );
    }

    #[test]
    fn the_state_words_are_the_words_of_the_job_machine() {
        for word in [
            "offered", "running", "stopping", "finished", "released", "lost",
        ] {
            let state = State::of(word).unwrap_or_else(|| panic!("{word} is no state"));
            assert_eq!(state.word(), word);
            assert_eq!(
                state.ends(),
                matches!(word, "finished" | "released" | "lost")
            );
        }
        assert_eq!(State::of("cancelled"), None);
        assert_eq!(State::of(""), None);
    }

    #[test]
    fn a_log_line_never_holds_a_token_a_body_or_a_path_of_another_host() {
        let line = Refused::JobOver("lost").line(Some(9), Some("gpu1"));
        assert_eq!(
            line,
            "refused job 9 of contractor gpu1: a job whose record says lost"
        );
        for secret in ["tok", "Bearer", "secret"] {
            assert!(!line.contains(secret), "{line}");
        }
        // A refusal of a path names the path, which is public, and nothing
        // else: no query, no head, no body.
        let line = Refused::Path("/v1/embeddings is not a model path of a cycle".to_string())
            .line(None, None);
        assert_eq!(
            line,
            "refused no job: the path /v1/embeddings is not a model path of a cycle"
        );
    }

    #[test]
    fn the_bearer_word_is_the_token_and_nothing_else_is() {
        let head =
            "POST /v1/chat/completions HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer tok-1\r\n\r\n";
        assert_eq!(bearer(head).as_deref(), Some("tok-1"));
        // The name and the word are read in any case; the value is not.
        assert_eq!(
            bearer("authorization: bearer tok-2\r\n").as_deref(),
            Some("tok-2")
        );
        for head in [
            "",
            "POST / HTTP/1.1\r\n\r\n",
            "Authorization: tok\r\n",
            "Authorization: Basic tok\r\n",
            "Authorization: Bearer \r\n",
            "Authorization: Bearer a b\r\n",
            "X-Authorization: Bearer tok\r\n",
            "Proxy-Authorization: Bearer tok\r\n",
        ] {
            assert_eq!(bearer(head), None, "{head:?}");
        }
    }
}
