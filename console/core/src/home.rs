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
    /// The analytics program's database.
    AnalyticsDb,
    /// The store's backups.
    Backups,
    /// The login of the app the factory acts as.
    BotLogin,
    /// When the word list was last read.
    Buzzwords,
    /// The last result of `tehas check-product`.
    CheckProduct,
    /// Held while `tehas clean` runs.
    CleanLock,
    /// The factory's code index, `index/`.
    CodeIndex,
    /// The tip the indexer last failed on.
    CodeIndexFailed,
    /// Held while the indexer runs.
    CodeIndexLock,
    /// The commit and indexer image the code index was made from.
    CodeIndexState,
    /// The memory of the cycles, `memory/cycles`.
    CycleMemory,
    /// The ask a poll is started on.
    DispatchAgain,
    /// Held while a poll reads.
    DispatchLock,
    /// Held while one start makes the egress proxy.
    EgressLock,
    /// The mark of a host's first install.
    FirstInstall,
    /// The recent failures of GitHub calls.
    GithubFailures,
    /// The log of the writes made to GitHub.
    GithubWrites,
    /// The messages `tehas host-message` keeps.
    HostMessages,
    /// The images a contractor runs, kept by the proxy.
    Images,
    /// The pins of the state labels to their node ids.
    LabelPins,
    /// The limit `tehas limit` sets.
    Limit,
    /// Held while the MCP server's checkout is fetched.
    McpFetchLock,
    /// A contractor's files of the job it runs.
    Outsource,
    /// The proxy's index, bundles and jobs' files for contractors.
    OutsourceDownloads,
    /// The scratch of the stamp of an outsourced cycle's heads.
    OutsourceStamp,
    /// The panels `tehas panel` keeps.
    Panels,
    /// The flag `tehas pause` sets.
    Pause,
    /// The files of the plugins' tickets, kept for the cycles given them.
    PluginFiles,
    /// When the platform was last fetched.
    PlatformFetch,
    /// The newest release, as last fetched.
    PlatformNewest,
    /// Held while the store publishes the live views.
    PublishLock,
    /// The record of a sandbox that cannot push.
    PushFault,
    /// The local records of starts a read-only host made.
    ReadOnly,
    /// The writes a read-only host did not make.
    ReadOnlyLog,
    /// The ledger of the refinements of issues.
    Refined,
    /// The releases the host has run.
    ReleaseHistory,
    /// The pi-rukas commit of the sandbox image, as last read.
    RukasImage,
    /// The pi-rukas commit last asked of upstream.
    RukasTarget,
    /// The ledger of the sizes of the issues started.
    Sizes,
    /// The record of the starts.
    Starts,
    /// The state store.
    StoreDb,
    /// The hold that stops an update.
    UpdateHold,
    /// Held while `tehas update` runs.
    UpdateLock,
    /// The follow-ups of an update not yet done.
    UpdatePending,
    /// The cleaner's record of its last upkeep.
    Upkeep,
}

impl State {
    /// The path relative to the home, for the consts that pin it.
    pub const fn relative(self) -> &'static str {
        match self {
            State::AccountIds => "state/account-ids",
            State::AnalyticsDb => "state/analytics.db",
            State::Backups => "state/backup",
            State::BotLogin => "state/github/login",
            State::Buzzwords => "state/buzzwords.at",
            State::CheckProduct => "state/check-product.json",
            State::CleanLock => "state/clean.lock",
            State::CodeIndex => "state/index",
            State::CodeIndexFailed => "state/index.failed",
            State::CodeIndexLock => "state/index.lock",
            State::CodeIndexState => "state/index.json",
            State::CycleMemory => "state/memory/cycles",
            State::DispatchAgain => "state/dispatch.again",
            State::DispatchLock => "state/dispatch.lock",
            State::EgressLock => "state/egress.lock",
            State::FirstInstall => "state/first-install",
            State::GithubFailures => "state/github-failures",
            State::GithubWrites => "state/github-writes.log",
            State::HostMessages => "state/host-messages.json",
            State::Images => "state/images",
            State::LabelPins => "state/labels",
            State::Limit => "state/limit.json",
            State::McpFetchLock => "state/mcp-fetch.lock",
            State::Outsource => "state/outsource",
            State::OutsourceDownloads => "state/outsource/downloads",
            State::OutsourceStamp => "state/outsource/stamp",
            State::Panels => "state/panels.json",
            State::Pause => "state/pause.json",
            State::PluginFiles => "state/plugin-files",
            State::PlatformFetch => "state/platform-fetch",
            State::PlatformNewest => "state/platform-newest",
            State::PublishLock => "state/publish.lock",
            State::PushFault => "state/push-fault.json",
            State::ReadOnly => "state/read-only",
            State::ReadOnlyLog => "state/read-only.log",
            State::Refined => "state/refined.jsonl",
            State::ReleaseHistory => "state/release-history",
            State::RukasImage => "state/rukas-image",
            State::RukasTarget => "state/rukas-target",
            State::Sizes => "state/sizes.jsonl",
            State::Starts => "state/starts.json",
            State::StoreDb => "state/tehas.db",
            State::UpdateHold => "state/update-hold",
            State::UpdateLock => "state/update.lock",
            State::UpdatePending => "state/update-pending",
            State::Upkeep => "state/upkeep.json",
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

/// A view the factory publishes under `live/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Live {
    /// The contractors of outsource.
    Contractors,
    /// What waits for a start, and why.
    Demand,
    /// The disk the factory uses.
    Disk,
    /// The dispatcher's last decision.
    Dispatch,
    /// The ten last ends of cycles.
    Endings,
    /// The epics and their issues.
    Epics,
    /// The checks of the host's health.
    Health,
    /// The panels the front page shows.
    Panels,
    /// The platform's release, as last fetched.
    Platform,
    /// The plugins granted a menu, and their views.
    Plugins,
    /// The previews and the disk they use.
    Previews,
    /// The pi-rukas the sandbox runs, and upstream.
    Rukas,
    /// The cycles running now.
    Running,
    /// The queue and the cycles of the product.
    Status,
    /// The host's load, as `tehas top` publishes it.
    Top,
}

impl Live {
    /// The file's name in `live/`, for a writer that joins it to the directory.
    pub const fn name(self) -> &'static str {
        match self {
            Live::Contractors => "contractors.json",
            Live::Demand => "demand.json",
            Live::Disk => "disk.json",
            Live::Dispatch => "dispatch.json",
            Live::Endings => "endings.json",
            Live::Epics => "epics.json",
            Live::Health => "health.json",
            Live::Panels => "panels.json",
            Live::Platform => "platform.json",
            Live::Plugins => "plugins.json",
            Live::Previews => "previews.json",
            Live::Rukas => "rukas.json",
            Live::Running => "running.json",
            Live::Status => "status.json",
            Live::Top => "top.json",
        }
    }
}

/// A socket's directory under `run/`, one for each socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Socket {
    /// Where `tehas-mcp-fetch.socket` listens: a connection asks for a fetch
    /// of the MCP server's checkout.
    McpFetch,
    /// Where the broker of plugin calls keeps its socket.
    PluginCalls,
}

impl Socket {
    const fn relative(self) -> &'static str {
        match self {
            Socket::McpFetch => "run/mcp-fetch",
            Socket::PluginCalls => "run/plugin-calls",
        }
    }
}

impl Home {
    pub fn new(root: impl Into<PathBuf>) -> Home {
        Home { root: root.into() }
    }

    /// A home whose paths join as `format!("{home}/state/...")` does: an
    /// empty home still names `/state/...`, not a path under the cwd.
    pub fn of_text(home: &str) -> Home {
        Home::new(format!("{home}/"))
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
    pub fn cycle_note(&self, issue: impl Into<i128>, note: &str) -> PathBuf {
        let issue = issue.into();
        self.cycles_dir().join(format!("{issue}.{note}"))
    }

    /// `state/plan-<issue>.json<end>`, with `end` such as `""` or `".out"`.
    pub fn plan_file(&self, issue: u64, end: &str) -> PathBuf {
        self.state_dir().join(format!("plan-{issue}.json{end}"))
    }

    /// `state/dev/<name>.lock`, what `tehas dev` holds for a clone's session.
    pub fn dev_lock(&self, name: &str) -> PathBuf {
        self.state_dir().join("dev").join(format!("{name}.lock"))
    }

    pub fn token_dir(&self, t: TokenDir) -> PathBuf {
        self.root.join(t.relative())
    }

    /// `live/`, the views the factory publishes, for the mount and the MCP
    /// server's `live:`.
    pub fn live_dir(&self) -> PathBuf {
        self.root.join("live")
    }

    pub fn live(&self, l: Live) -> PathBuf {
        self.live_dir().join(l.name())
    }

    /// `live/console-<issue>.txt`, a cycle's console.
    pub fn live_console(&self, issue: u64) -> PathBuf {
        self.live_dir().join(format!("console-{issue}.txt"))
    }

    /// `live/console-<issue>.txt.now`, what the cycle's console shows now.
    pub fn live_console_now(&self, issue: u64) -> PathBuf {
        self.live_dir().join(format!("console-{issue}.txt.now"))
    }

    /// The host side of a socket's directory. The container side is the
    /// mount's, not the home's.
    pub fn socket_dir(&self, s: Socket) -> PathBuf {
        self.root.join(s.relative())
    }

    /// `run/`, where the cycles' copies of the code are made today, as
    /// `<kind>-<issue>-<pid>`.
    pub fn code_copies(&self) -> PathBuf {
        self.root.join("run")
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
    fn a_home_of_text_joins_as_text() {
        assert_eq!(at(Home::of_text("").state_dir()), "/state");
        assert_eq!(
            at(Home::of_text("").live(Live::Running)),
            "/live/running.json"
        );
        assert_eq!(
            at(Home::of_text("/h").state(State::Limit)),
            "/h/state/limit.json"
        );
        assert_eq!(at(Home::of_text("/h/").state_dir()), "/h//state");
        assert_eq!(
            at(Home::of_text("/h").cycle_note(9, "cid")),
            "/h/state/cycles/9.cid"
        );
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
            (State::AnalyticsDb, "/h/state/analytics.db"),
            (State::Backups, "/h/state/backup"),
            (State::BotLogin, "/h/state/github/login"),
            (State::Buzzwords, "/h/state/buzzwords.at"),
            (State::CheckProduct, "/h/state/check-product.json"),
            (State::CleanLock, "/h/state/clean.lock"),
            (State::CodeIndex, "/h/state/index"),
            (State::CodeIndexFailed, "/h/state/index.failed"),
            (State::CodeIndexLock, "/h/state/index.lock"),
            (State::CodeIndexState, "/h/state/index.json"),
            (State::CycleMemory, "/h/state/memory/cycles"),
            (State::DispatchAgain, "/h/state/dispatch.again"),
            (State::DispatchLock, "/h/state/dispatch.lock"),
            (State::EgressLock, "/h/state/egress.lock"),
            (State::FirstInstall, "/h/state/first-install"),
            (State::GithubFailures, "/h/state/github-failures"),
            (State::GithubWrites, "/h/state/github-writes.log"),
            (State::HostMessages, "/h/state/host-messages.json"),
            (State::Images, "/h/state/images"),
            (State::LabelPins, "/h/state/labels"),
            (State::Limit, "/h/state/limit.json"),
            (State::McpFetchLock, "/h/state/mcp-fetch.lock"),
            (State::Outsource, "/h/state/outsource"),
            (State::OutsourceDownloads, "/h/state/outsource/downloads"),
            (State::OutsourceStamp, "/h/state/outsource/stamp"),
            (State::Panels, "/h/state/panels.json"),
            (State::Pause, "/h/state/pause.json"),
            (State::PluginFiles, "/h/state/plugin-files"),
            (State::PlatformFetch, "/h/state/platform-fetch"),
            (State::PlatformNewest, "/h/state/platform-newest"),
            (State::PublishLock, "/h/state/publish.lock"),
            (State::PushFault, "/h/state/push-fault.json"),
            (State::ReadOnly, "/h/state/read-only"),
            (State::ReadOnlyLog, "/h/state/read-only.log"),
            (State::Refined, "/h/state/refined.jsonl"),
            (State::ReleaseHistory, "/h/state/release-history"),
            (State::RukasImage, "/h/state/rukas-image"),
            (State::RukasTarget, "/h/state/rukas-target"),
            (State::Sizes, "/h/state/sizes.jsonl"),
            (State::Starts, "/h/state/starts.json"),
            (State::StoreDb, "/h/state/tehas.db"),
            (State::UpdateHold, "/h/state/update-hold"),
            (State::UpdateLock, "/h/state/update.lock"),
            (State::UpdatePending, "/h/state/update-pending"),
            (State::Upkeep, "/h/state/upkeep.json"),
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
        assert_eq!(
            at(h().cycle_note(-1_i64, "lock")),
            "/h/state/cycles/-1.lock"
        );
    }

    #[test]
    fn a_plan_file_is_named_for_its_issue() {
        assert_eq!(at(h().plan_file(7, "")), "/h/state/plan-7.json");
        assert_eq!(at(h().plan_file(7, ".out")), "/h/state/plan-7.json.out");
    }

    #[test]
    fn each_live_view_is_its_file() {
        let cases = [
            (Live::Contractors, "/h/live/contractors.json"),
            (Live::Demand, "/h/live/demand.json"),
            (Live::Disk, "/h/live/disk.json"),
            (Live::Dispatch, "/h/live/dispatch.json"),
            (Live::Endings, "/h/live/endings.json"),
            (Live::Epics, "/h/live/epics.json"),
            (Live::Health, "/h/live/health.json"),
            (Live::Panels, "/h/live/panels.json"),
            (Live::Platform, "/h/live/platform.json"),
            (Live::Plugins, "/h/live/plugins.json"),
            (Live::Previews, "/h/live/previews.json"),
            (Live::Rukas, "/h/live/rukas.json"),
            (Live::Running, "/h/live/running.json"),
            (Live::Status, "/h/live/status.json"),
            (Live::Top, "/h/live/top.json"),
        ];
        for (l, want) in cases {
            assert_eq!(at(h().live(l)), want, "{l:?}");
        }
    }

    #[test]
    fn a_dev_lock_is_named_for_its_clone() {
        assert_eq!(at(h().dev_lock("frank")), "/h/state/dev/frank.lock");
    }

    #[test]
    fn a_console_is_named_for_its_issue() {
        assert_eq!(at(h().live_console(5)), "/h/live/console-5.txt");
        assert_eq!(at(h().live_console_now(5)), "/h/live/console-5.txt.now");
    }

    #[test]
    fn each_socket_has_its_directory() {
        assert_eq!(at(h().socket_dir(Socket::McpFetch)), "/h/run/mcp-fetch");
        assert_eq!(
            at(h().socket_dir(Socket::PluginCalls)),
            "/h/run/plugin-calls"
        );
        assert_eq!(at(h().code_copies()), "/h/run");
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
