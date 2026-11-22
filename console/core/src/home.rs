//! The layout of a tehas home, `$TEHAS_HOME`: what is under `state/`,
//! `live/` and `run/`. This is the one place that writes it. Home only hands
//! out paths: it reads no file, makes none and locks none.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A tehas home, the directory that `TEHAS_HOME` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    root: PathBuf,
}

/// A file or directory of a fixed name under `state/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The ids of the accounts the lists name.
    AccountIds,
    /// The login of the app the factory acts as.
    BotLogin,
    /// The factory's code index, `index/`.
    CodeIndex,
    /// The commit and indexer image the code index was made from.
    CodeIndexState,
    /// Held while one start makes the egress proxy.
    EgressLock,
    /// The recent failures of GitHub calls.
    GithubFailures,
    /// The pins of the state labels to their node ids.
    LabelPins,
    /// The images a contractor runs, kept by the proxy.
    Images,
    /// The limit `tehas limit` sets.
    Limit,
    /// A contractor's files of the job it runs.
    Outsource,
    /// The proxy's index, bundles and jobs' files for contractors.
    OutsourceDownloads,
    /// The flag `tehas pause` sets.
    Pause,
    /// The record of a sandbox that cannot push.
    PushFault,
    /// The local records of starts a read-only host made.
    ReadOnly,
    /// The writes a read-only host did not make.
    ReadOnlyLog,
    /// The record of the starts.
    Starts,
    /// The state store.
    StoreDb,
    /// Held while `tehas update` runs.
    UpdateLock,
}

impl State {
    /// The path relative to the home, for the consts that pin it.
    pub const fn relative(self) -> &'static str {
        match self {
            State::AccountIds => "state/account-ids",
            State::BotLogin => "state/github/login",
            State::CodeIndex => "state/index",
            State::CodeIndexState => "state/index.json",
            State::EgressLock => "state/egress.lock",
            State::GithubFailures => "state/github-failures",
            State::LabelPins => "state/labels",
            State::Images => "state/images",
            State::Limit => "state/limit.json",
            State::Outsource => "state/outsource",
            State::OutsourceDownloads => "state/outsource/downloads",
            State::Pause => "state/pause.json",
            State::PushFault => "state/push-fault.json",
            State::ReadOnly => "state/read-only",
            State::ReadOnlyLog => "state/read-only.log",
            State::Starts => "state/starts.json",
            State::StoreDb => "state/tehas.db",
            State::UpdateLock => "state/update.lock",
        }
    }
}

/// A directory of a GitHub App token under `state/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenDir {
    /// The app's own token, which goes to no container.
    App,
    /// A cycle's token, narrowed to the product repository.
    Cycle,
    /// The token of a planning turn and of a session.
    Session,
}

impl TokenDir {
    pub const ALL: [TokenDir; 3] = [TokenDir::App, TokenDir::Cycle, TokenDir::Session];

    /// The path relative to the home, for the consts and tests that pin it.
    pub const fn relative(self) -> &'static str {
        match self {
            TokenDir::App => "state/github",
            TokenDir::Cycle => "state/github-cycle",
            TokenDir::Session => "state/github-session",
        }
    }
}

impl Home {
    pub fn new(root: impl Into<PathBuf>) -> Home {
        Home { root: root.into() }
    }

    /// TEHAS_HOME as `var_os` reads it; None when unset or empty.
    pub fn from_env() -> Option<Home> {
        from_var(std::env::var_os("TEHAS_HOME"))
    }

    /// The home itself, for what is not part of this layout.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `state/`, for code that makes, mounts or sweeps the directory. A name
    /// in it is a `State`.
    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn state(&self, s: State) -> PathBuf {
        self.root.join(s.relative())
    }

    /// `state/cycles`, where a cycle's notes are.
    pub fn cycles_dir(&self) -> PathBuf {
        self.root.join("state/cycles")
    }

    /// `state/cycles/<issue>.<note>`.
    pub fn cycle_note(&self, issue: u64, note: &str) -> PathBuf {
        self.cycles_dir().join(format!("{issue}.{note}"))
    }

    pub fn token_dir(&self, t: TokenDir) -> PathBuf {
        self.root.join(t.relative())
    }

    /// `live/`, the views the factory publishes, for the mount and the MCP
    /// server's `live:`.
    pub fn live_dir(&self) -> PathBuf {
        self.root.join("live")
    }
}

fn from_var(v: Option<OsString>) -> Option<Home> {
    v.filter(|v| !v.is_empty()).map(Home::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h() -> Home {
        Home::new("/h")
    }

    fn at(p: PathBuf) -> String {
        p.display().to_string()
    }

    #[test]
    fn the_home_is_its_root() {
        assert_eq!(h().root(), Path::new("/h"));
        assert_eq!(at(h().state_dir()), "/h/state");
        assert_eq!(at(Home::new("").state_dir()), "state");
        assert_eq!(at(h().live_dir()), "/h/live");
    }

    #[test]
    fn an_unset_or_empty_variable_is_no_home() {
        assert_eq!(from_var(None), None);
        assert_eq!(from_var(Some(OsString::new())), None);
        assert_eq!(from_var(Some("/h".into())), Some(h()));
    }

    #[test]
    fn each_state_name_is_its_file() {
        let cases = [
            (State::AccountIds, "/h/state/account-ids"),
            (State::BotLogin, "/h/state/github/login"),
            (State::CodeIndex, "/h/state/index"),
            (State::CodeIndexState, "/h/state/index.json"),
            (State::EgressLock, "/h/state/egress.lock"),
            (State::GithubFailures, "/h/state/github-failures"),
            (State::LabelPins, "/h/state/labels"),
            (State::Images, "/h/state/images"),
            (State::Limit, "/h/state/limit.json"),
            (State::Outsource, "/h/state/outsource"),
            (State::OutsourceDownloads, "/h/state/outsource/downloads"),
            (State::Pause, "/h/state/pause.json"),
            (State::PushFault, "/h/state/push-fault.json"),
            (State::ReadOnly, "/h/state/read-only"),
            (State::ReadOnlyLog, "/h/state/read-only.log"),
            (State::Starts, "/h/state/starts.json"),
            (State::StoreDb, "/h/state/tehas.db"),
            (State::UpdateLock, "/h/state/update.lock"),
        ];
        for (s, want) in cases {
            assert_eq!(at(h().state(s)), want, "{s:?}");
        }
    }

    #[test]
    fn a_cycle_note_is_beside_its_number() {
        assert_eq!(at(h().cycles_dir()), "/h/state/cycles");
        assert_eq!(at(h().cycle_note(7, "lock")), "/h/state/cycles/7.lock");
        assert_eq!(
            at(h().cycle_note(7, "end-failed.json")),
            "/h/state/cycles/7.end-failed.json"
        );
    }

    #[test]
    fn each_token_dir_is_its_directory() {
        let cases = [
            (TokenDir::App, "/h/state/github"),
            (TokenDir::Cycle, "/h/state/github-cycle"),
            (TokenDir::Session, "/h/state/github-session"),
        ];
        for (t, want) in cases {
            assert_eq!(at(h().token_dir(t)), want, "{t:?}");
        }
        assert_eq!(
            TokenDir::ALL.map(|t| t.relative()),
            ["state/github", "state/github-cycle", "state/github-session"]
        );
    }
}
