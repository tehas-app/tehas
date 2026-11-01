//! What each setting of an instance may hold, its default, and when a load
//! asks: one row per setting. No regex here: a load runs before every command,
//! and compiling these patterns on each one costs more than the checks.
//!
//! The reasons are the words an operator reads and tests match, so a change
//! to one is a change to behaviour.

/// The ceiling of a planning turn. The unit of a turn gets RuntimeMaxSec = the
/// limit + 900 (dispatch/src/apply/turn.rs), so 7200 is 2h15, inside the unit's
/// own bound. The default of TEHAS_PLAN_TIMEOUT is this ceiling.
pub const PLAN_TIMEOUT_MAX: u64 = 7200;
/// The bound of one child of a cycle: pi-rukas's own two hours. tehas work
/// passes it to the container as PI_ENSEMBLE_SPAWN_TIMEOUT_MS.
pub const CHILD_TIMEOUT_MAX: u64 = 7200;
/// tehas-cycle@.service stops a cycle after 8 hours (RuntimeMaxSec) and a unit
/// cannot read an instance's settings; the hour between is for the stop and
/// the report.
pub const WORK_TIMEOUT_MAX: u64 = 25200;

/// The roles an instance may give a model of their own, as the suffix of
/// PI_ENSEMBLE_MODEL_<ROLE> and PI_ENSEMBLE_PROVIDER_<ROLE>, checked when the
/// settings load, before any image is asked. They are the members pi-rukas
/// v0.12.80 and later list for that family in --capabilities. tehas work
/// checks a cycle's names against the members its image lists, so a role
/// pi-rukas drops is refused there; one it adds is usable once it is added
/// here. All six lens reviewers are one role, CODE_REVIEW_SPECIALIST. The
/// project manager is the session: its model is the product's
/// .pi/settings.json.
pub const ROLES: [&str; 5] = [
    "DEVELOPER",
    "OPS",
    "EXPLORE",
    "ADVERSARIAL_DEVELOPER",
    "CODE_REVIEW_SPECIALIST",
];

/// The directory names tehasbot clean removes when the instance names none:
/// the build output of the toolchains tehas hosts, none of which a product
/// keeps in source. instances/example and the manual quote them, and
/// test/config_test.sh pins the copies against this.
pub const CLEAN_DIRS: &str =
    "target node_modules __pycache__ .venv .pytest_cache .mypy_cache .next .turbo .gradle";

const THEMES: &str = "expected pixel, paper, beesknees or beesknees26";
const SWITCH: Rule = Rule::OneOf(&["0", "1"], "expected 0 or 1");
const LOGIN_MILLWRIGHT: &str =
    "expected one GitHub login, such as tehas-millwright or tehas-millwright[bot]";
const LOGIN_BOT: &str = "expected one GitHub login, such as tehas-app-bot[bot]";

/// The shape of a value.
#[derive(Clone, Copy)]
pub enum Rule {
    Repo,
    /// The counts refuse `auto` by name until cycles can use it.
    Count,
    CountOrAuto,
    SliceContainers,
    /// The dispatcher waits for the gate while it holds state/dispatch.lock,
    /// so these seconds delay every later poll too: that is the ceiling.
    LowGateWait,
    /// Exactly one space between logins and none around them: the dispatcher
    /// matches " login " inside " list ", and a stray space would make an
    /// empty name match.
    Logins,
    Login(&'static str),
    AppId,
    OneOf(&'static [&'static str], &'static str),
    /// Both analytics settings end up in an HTML attribute of every page, so
    /// no quote, space or angle bracket may get through.
    ScriptAddress,
    SiteName,
    /// The shape lib/slice.sh reads at every start: no leading zero, at most
    /// twelve digits of bytes or nine with a unit, and a size Docker takes:
    /// it refuses a memory value under 6m.
    ContainerSize,
    HostAddress,
    /// The dashboard puts a ticket's number straight after it.
    LabAddress,
    /// A name is made by putting uat-<n>. or lab-<n>. in front of it.
    Domain,
    PreviewMax,
    FreeDisk,
    Workarounds,
    RefreshSlots,
    RefreshEvery,
    /// Hours: docker's `until` and find's `-newermt`.
    CleanAfter,
    /// An idle poll's budget for `store poll-idle --budget`: the three figures
    /// in this order, any left out; another order would bound the wrong
    /// figure. Each figure as the store's count reads it, no leading 0.
    PollBudget,
    /// The two bounds of core/src/size.rs, each holding what the other leaves
    /// unbounded: a body of one huge line, a body of many short ones.
    MeasureBound,
    ScratchDir,
    /// Names, not paths: the cleaner matches them with find -name.
    CleanDirs,
    Follow,
    /// A check run's name goes into a URL and into the log.
    BranchCheck,
    /// Run by the dispatcher as one word, with no shell between. Every segment
    /// is named: //, /./ and /../ would let one file be named another way,
    /// which is how a check on where a path sits is defeated.
    LowGate,
    OneWord,
}

/// When a load checks a setting.
#[derive(Clone, Copy, PartialEq)]
pub enum When {
    /// Every load: it has a default, so it always holds something.
    Always,
    /// Every load that gives it a value.
    Given,
    /// Every load that names it, even as an empty line.
    Named,
    /// Only when a command requires it: tehas_require reads the load's
    /// `_tehas_bad`, so a login shell is not refused for it.
    Required,
}

pub struct Setting {
    pub name: &'static str,
    pub rule: Rule,
    pub when: When,
    /// What the load exports when the instance left it out or empty. None:
    /// it stays unset, or the load works it out (load.rs).
    pub default: Option<&'static str>,
}

const fn row(name: &'static str, rule: Rule, when: When, default: Option<&'static str>) -> Setting {
    Setting {
        name,
        rule,
        when,
        default,
    }
}
const fn always(name: &'static str, rule: Rule, default: &'static str) -> Setting {
    row(name, rule, When::Always, Some(default))
}
const fn given(name: &'static str, rule: Rule) -> Setting {
    row(name, rule, When::Given, None)
}
const fn given_or(name: &'static str, rule: Rule, default: &'static str) -> Setting {
    row(name, rule, When::Given, Some(default))
}
const fn required(name: &'static str, rule: Rule) -> Setting {
    row(name, rule, When::Required, None)
}

/// In the order a load checks them, which picks the setting an operator is
/// told about first. An optional setting left out is checked only when it is
/// there, as TEHAS_BUILD_TIMEOUT is: its default belongs to install.sh.
pub const TABLE: &[Setting] = &[
    always(
        "TEHAS_THEME",
        Rule::OneOf(&["pixel", "paper", "beesknees", "beesknees26"], THEMES),
        "pixel",
    ),
    always(
        "TEHAS_LAYOUT",
        Rule::OneOf(&["floor", "business"], "expected floor or business"),
        "floor",
    ),
    always("TEHAS_INDEXING", SWITCH, "0"),
    always("TEHAS_WORK_IMAGES", SWITCH, "0"),
    always("TEHAS_WORK_DESCRIBE", SWITCH, "0"),
    always("TEHAS_WORK_FILES", SWITCH, "0"),
    always("TEHAS_WORK_COMMENTS", SWITCH, "0"),
    always("TEHAS_WORK_ZIPS", SWITCH, "0"),
    always(
        "TEHAS_ANALYTICS",
        Rule::OneOf(&["on", "off"], "expected on or off"),
        "on",
    ),
    always("TEHAS_ANALYTICS_COOKIE", SWITCH, "0"),
    always("TEHAS_TOP", SWITCH, "0"),
    always("TEHAS_EASY_MERGE", SWITCH, "0"),
    always("TEHAS_IMPROVE_RUKAS_SKIP", SWITCH, "0"),
    always("TEHAS_REVIEW", SWITCH, "0"),
    always("TEHAS_CONCURRENCY", Rule::Count, "1"),
    row(
        "TEHAS_TICKET_CONCURRENCY",
        Rule::CountOrAuto,
        When::Always,
        None,
    ),
    always("TEHAS_HOST_ADDR", Rule::HostAddress, "127.0.0.1"),
    row("TEHAS_MAX_AWAITING", Rule::Count, When::Always, None),
    always("TEHAS_WORK_TIMEOUT", Rule::Count, "21600"),
    always("TEHAS_WORK_STALL", Rule::Count, "3600"),
    always("TEHAS_VERIFY_TIMEOUT", Rule::Count, "1800"),
    row("TEHAS_CHILD_TIMEOUT", Rule::Count, When::Always, None),
    always("TEHAS_PLAN_MAX_TURNS", Rule::Count, "20"),
    row("TEHAS_PLAN_TIMEOUT", Rule::Count, When::Always, None),
    always("TEHAS_MILLWRIGHT_MAX_RETRIES", Rule::Count, "2"),
    always("TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS", Rule::Count, "3"),
    always("TEHAS_MILLWRIGHT_FEED", SWITCH, "0"),
    always("TEHAS_MILLWRIGHT_MAX_FEEDS", Rule::Count, "2"),
    always("TEHAS_REFRESH_CONFLICTS", SWITCH, "0"),
    always("TEHAS_REFRESH_MAX", Rule::Count, "2"),
    always("TEHAS_REFRESH_EVERY", Rule::RefreshEvery, "10"),
    always("TEHAS_REFRESH_SLOTS", Rule::RefreshSlots, "1"),
    always("TEHAS_PREVIEWS", SWITCH, "0"),
    always("TEHAS_PREVIEW_MAX", Rule::PreviewMax, "5"),
    always("TEHAS_PREVIEW_MIN_FREE", Rule::FreeDisk, "5g"),
    always("TEHAS_CYCLE_MIN_FREE", Rule::FreeDisk, "10g"),
    always("TEHAS_CLEAN_MIN_FREE", Rule::FreeDisk, "30g"),
    always("TEHAS_CYCLE_RESERVE", Rule::ContainerSize, "2g"),
    always("TEHAS_SLICE_CONTAINERS", Rule::SliceContainers, "4"),
    always("TEHAS_LOW_GATE_WAIT", Rule::LowGateWait, "60"),
    always("TEHAS_CLEAN_AFTER", Rule::CleanAfter, "24"),
    always(
        "TEHAS_POLL_BUDGET",
        Rule::PollBudget,
        "cpu=5,wall=30,writes=0",
    ),
    always("TEHAS_SIZE_MAX_BYTES", Rule::MeasureBound, "131072"),
    always("TEHAS_SIZE_MAX_LINES", Rule::MeasureBound, "1000"),
    given("TEHAS_BUILD_TIMEOUT", Rule::Count),
    row("PI_RUKAS_FOLLOW", Rule::Follow, When::Named, None),
    given("TEHAS_BRANCH", Rule::OneWord),
    given_or("TEHAS_DOMAIN", Rule::Domain, ""),
    given("TEHAS_PROTOLAB_URL", Rule::LabAddress),
    given(
        "TEHAS_UPDATE_FROM",
        Rule::OneOf(&["git", "release"], "expected git or release"),
    ),
    given("TEHAS_RELEASE_REPO", Rule::Repo),
    given_or("TEHAS_SIZE_MAX_FILES", Rule::Count, ""),
    given(
        "TEHAS_CYCLE_REPORT",
        Rule::OneOf(&["bash", "rust"], "expected bash or rust"),
    ),
    given("TEHAS_SCRATCH_DIR", Rule::ScratchDir),
    given_or("TEHAS_ANALYTICS_SRC", Rule::ScriptAddress, ""),
    given_or("TEHAS_ANALYTICS_SITE", Rule::SiteName, ""),
    given_or("TEHAS_IMPROVE_REPO", Rule::Repo, ""),
    given_or("TEHAS_IMPROVE_RUKAS_REPO", Rule::Repo, ""),
    given_or("TEHAS_IMPROVE_MAX_OPEN", Rule::Count, "10"),
    given_or("TEHAS_MILLWRIGHT_LOGIN", Rule::Login(LOGIN_MILLWRIGHT), ""),
    given_or("TEHAS_GITHUB_APP_ID", Rule::AppId, ""),
    given_or("TEHAS_GITHUB_APP_INSTALLATION", Rule::AppId, ""),
    given_or("TEHAS_BOT_LOGIN", Rule::Login(LOGIN_BOT), ""),
    given_or("TEHAS_RUKAS_WORKAROUNDS", Rule::Workarounds, ""),
    given_or("TEHAS_BRANCH_CHECK", Rule::BranchCheck, ""),
    given_or("TEHAS_LOW_GATE", Rule::LowGate, ""),
    given_or("TEHAS_CLEAN_DIRS", Rule::CleanDirs, CLEAN_DIRS),
    required("TEHAS_REPO", Rule::Repo),
    required("TEHAS_CONTROL_ISSUE", Rule::Count),
    required("TEHAS_ALLOWED_LABELERS", Rule::Logins),
    required("PI_ENSEMBLE_SUBAGENT_MODEL", Rule::OneWord),
    required("PI_ENSEMBLE_SUBAGENT_PROVIDER", Rule::OneWord),
    required("TEHAS_CONTAINER_MEMORY", Rule::ContainerSize),
    row("TEHAS_REVIEW_REPO", Rule::Repo, When::Required, Some("")),
];

/// The trees a merged pull request writes: a gate may not sit under one.
#[derive(Default)]
pub struct Trees<'a> {
    pub home: &'a str,
    pub instance: &'a str,
    pub work_repo: &'a str,
}

fn rule_of(name: &str) -> Option<Rule> {
    if name.starts_with("PI_ENSEMBLE_MODEL_") || name.starts_with("PI_ENSEMBLE_PROVIDER_") {
        return Some(Rule::OneWord);
    }
    TABLE.iter().find(|s| s.name == name).map(|s| s.rule)
}

/// Why `value` is no value of `name`. A name with no rule takes anything.
pub fn check(name: &str, value: &str, trees: &Trees) -> Result<(), String> {
    match rule_of(name) {
        Some(rule) => verdict(rule, value, trees),
        None => Ok(()),
    }
}

fn verdict(rule: Rule, v: &str, trees: &Trees) -> Result<(), String> {
    let refuse = |ok: bool, why: &str| if ok { Ok(()) } else { Err(why.to_string()) };
    match rule {
        Rule::Repo => refuse(is_repo(v), "expected owner/name"),
        Rule::Count => count(v),
        Rule::CountOrAuto => {
            refuse(v != "auto", AUTO)?;
            count(v)
        }
        Rule::SliceContainers => {
            refuse(v != "auto", AUTO)?;
            let n = if leading(v, 2) { v.parse().ok() } else { None };
            refuse(
                matches!(n, Some(2..=16)),
                "expected a whole number from 2 to 16",
            )
        }
        Rule::LowGateWait => {
            refuse(leading(v, usize::MAX), POSITIVE)?;
            refuse(
                v.len() <= 3 && v.parse::<u32>().is_ok_and(|n| n <= 600),
                "expected at most 600 seconds",
            )
        }
        Rule::Logins => refuse(
            v.split(' ').all(is_login),
            "expected GitHub logins separated by single spaces",
        ),
        Rule::Login(why) => refuse(is_login(v.strip_suffix("[bot]").unwrap_or(v)), why),
        Rule::AppId => refuse(leading(v, 12), "expected the ID GitHub shows, a whole number"),
        Rule::OneOf(words, why) => refuse(words.contains(&v), why),
        Rule::ScriptAddress => {
            let path = v
                .strip_prefix("https://")
                .and_then(|rest| rest.split_once('/'));
            refuse(
                path.is_some_and(|(host, path)| {
                    is_host(host) && path.chars().all(class("._/-"))
                }),
                "expected the https address of a script",
            )
        }
        Rule::SiteName => refuse(
            all(v, class("._-")),
            "expected one word of letters, digits, dots and dashes",
        ),
        Rule::ContainerSize => refuse(
            is_container_size(v),
            "expected a size of at least 6m, such as 512m or 2g",
        ),
        Rule::HostAddress => refuse(all(v, class(".:-")), "expected an address or host name"),
        Rule::LabAddress => {
            let rest = v
                .strip_prefix("https://")
                .or_else(|| v.strip_prefix("http://"));
            refuse(
                rest.is_some_and(is_host),
                "expected an address such as https://lab.example, with no path and no slash at the end",
            )
        }
        Rule::Domain => refuse(
            is_domain(v),
            "expected a domain such as example.org, in lower case, with no scheme and no path",
        ),
        Rule::PreviewMax => refuse(leading(v, 2), "expected a number from 1 to 99"),
        Rule::FreeDisk => refuse(
            mem_mb(v).is_some_and(|mb| mb.parse::<i64>().is_ok_and(|mb| mb > 0)),
            "expected a size such as 512m or 5g",
        ),
        Rule::Workarounds => refuse(
            v.split(' ').all(is_workaround),
            "expected NAME@VERSION entries separated by single spaces, such as PI_ENSEMBLE_COMMENT_RETENTION@0.12.80",
        ),
        Rule::RefreshSlots => refuse(
            matches!(v.as_bytes(), [b'0'..=b'8']),
            "expected a whole number from 0 to 8",
        ),
        Rule::RefreshEvery => refuse(
            leading(v, 4) && v.parse::<u32>().is_ok_and(|n| n <= 1440),
            "expected a whole number of minutes from 1 to 1440",
        ),
        Rule::CleanAfter => refuse(
            leading(v, 5),
            "expected a whole number of hours of at least 1",
        ),
        Rule::PollBudget => poll_budget(v),
        Rule::MeasureBound => refuse(leading(v, 8), "expected a whole number of at least 1"),
        Rule::ScratchDir => refuse(
            v.strip_prefix('/')
                .is_some_and(|rest| rest.split('/').all(|p| all(p, class("._-"))))
                && !v.contains(".."),
            "expected an absolute path such as /mnt/scratch",
        ),
        Rule::CleanDirs => clean_dirs(v),
        Rule::Follow => follow(v),
        Rule::BranchCheck => refuse(
            v.split(' ').all(|w| all(w, class("._()/-"))),
            "expected the name of a check, words of letters, digits and ._()/- separated by single spaces",
        ),
        Rule::LowGate => low_gate(v, trees),
        Rule::OneWord => refuse(
            !v.is_empty() && !v.bytes().any(is_space),
            "expected one word",
        ),
    }
}

const AUTO: &str = "expected a whole number (auto is not usable yet)";
const POSITIVE: &str = "expected a positive whole number";
const GATE_PATH: &str =
    "expected an absolute path of /, letters, digits and ._-, with no empty or dot segment";

fn all(s: &str, ok: impl Fn(char) -> bool) -> bool {
    !s.is_empty() && s.chars().all(ok)
}
/// A letter or digit of ASCII, or one of `more`.
fn class(more: &'static str) -> impl Fn(char) -> bool + Copy {
    move |c| c.is_ascii_alphanumeric() || more.contains(c)
}
fn digits(s: &str) -> bool {
    all(s, |c| c.is_ascii_digit())
}
/// `[1-9][0-9]*`, at most `max` digits.
fn leading(s: &str, max: usize) -> bool {
    digits(s) && !s.starts_with('0') && s.len() <= max
}
/// The space class of a POSIX pattern, in ASCII.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}
fn is_login(s: &str) -> bool {
    all(s, class("-")) && !s.starts_with('-')
}
fn is_repo(s: &str) -> bool {
    matches!(s.split_once('/'), Some((o, n)) if all(o, class("_.-")) && all(n, class("_.-")))
}
/// A host name or address, then `:<port>` if there is one.
fn is_host(s: &str) -> bool {
    let (name, port) = match s.split_once(':') {
        Some((name, port)) => (name, Some(port)),
        None => (s, None),
    };
    all(name, class(".-")) && port.is_none_or(digits)
}
fn is_domain(s: &str) -> bool {
    let label = |l: &str| {
        all(l, |c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
        }) && !l.starts_with('-')
            && !l.ends_with('-')
    };
    s.contains('.') && s.split('.').all(label)
}
fn is_workaround(entry: &str) -> bool {
    let Some((name, version)) = entry.split_once('@') else {
        return false;
    };
    let family = name
        .strip_prefix("PI_ENSEMBLE_")
        .or_else(|| name.strip_prefix("TEHAS_"));
    let parts = version.split('.').count();
    family.is_some_and(|rest| {
        all(rest, |c| {
            c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'
        })
    }) && (2..=4).contains(&parts)
        && version.split('.').all(|p| digits(p) && p.len() <= 6)
}
fn is_container_size(v: &str) -> bool {
    let (n, unit) = split_unit(v);
    let shape = match unit {
        None | Some('b' | 'B') => leading(n, 12),
        Some(_) => leading(n, 9),
    };
    shape && mem_mb(v).is_some_and(|mb| mb.parse::<i64>().is_ok_and(|mb| mb >= 6))
}

/// A count of the store's own grammar (verbs::count): at most nine digits, and
/// the only one beginning with 0 is 0 itself.
pub fn is_count(s: &str) -> bool {
    digits(s) && s.len() <= 9 && (s == "0" || !s.starts_with('0'))
}

fn count(v: &str) -> Result<(), String> {
    if !leading(v, usize::MAX) {
        return Err(POSITIVE.to_string());
    }
    if v.len() > 9 {
        return Err("expected at most 9 digits".to_string());
    }
    Ok(())
}

fn split_unit(v: &str) -> (&str, Option<char>) {
    match v.chars().last() {
        Some(c @ ('b' | 'k' | 'm' | 'g' | 'B' | 'K' | 'M' | 'G')) => (&v[..v.len() - 1], Some(c)),
        _ => (v, None),
    }
}

/// A number as shell arithmetic reads it: a leading 0 makes it octal, and past
/// 64 bits it wraps. None for a digit octal has not.
fn arithmetic(n: &str) -> Option<i64> {
    let base = if n.starts_with('0') { 8 } else { 10 };
    n.bytes().try_fold(0i64, |sum, b| {
        let digit = i64::from(b - b'0');
        (digit < base).then(|| sum.wrapping_mul(base).wrapping_add(digit))
    })
}

/// A Docker memory value (512m, 8g, a bare number of bytes) in MiB, as text: a
/// value in MiB stays as it was written, leading zeros and all, because it was
/// never a number to the shell either.
pub fn mem_mb(v: &str) -> Option<String> {
    let (n, unit) = split_unit(v);
    if !digits(n) {
        return None;
    }
    match unit {
        Some('m' | 'M') => Some(n.to_string()),
        Some('g' | 'G') => Some(arithmetic(n)?.wrapping_mul(1024).to_string()),
        Some('k' | 'K') => Some((arithmetic(n)? / 1024).to_string()),
        _ => Some((arithmetic(n)? / 1_048_576).to_string()),
    }
}

fn poll_budget(v: &str) -> Result<(), String> {
    let parts: Option<Vec<(&str, &str)>> = v.split(',').map(|p| p.split_once('=')).collect();
    let parts = parts.unwrap_or_default();
    let names: Vec<&str> = parts.iter().map(|(name, _)| *name).collect();
    let order = matches!(
        names.as_slice(),
        ["cpu"]
            | ["cpu", "wall"]
            | ["cpu", "wall", "writes"]
            | ["wall"]
            | ["wall", "writes"]
            | ["writes"]
    );
    if !order || !parts.iter().all(|(_, n)| digits(n) && n.len() <= 9) {
        return Err("expected cpu=<s>,wall=<s>,writes=<n>, any part left out".to_string());
    }
    if !parts.iter().all(|(_, n)| is_count(n)) {
        return Err("expected cpu=<s>,wall=<s>,writes=<n>, and no leading 0".to_string());
    }
    Ok(())
}

/// The pattern admits no slash, and these are the dot-names a workspace keeps,
/// in any spelling.
fn clean_dirs(v: &str) -> Result<(), String> {
    if !v.split(' ').all(|d| all(d, class("._-"))) {
        return Err(
            "expected directory names of letters, digits and ._-, separated by single spaces"
                .to_string(),
        );
    }
    for dir in v.split(' ') {
        let low = dir.to_ascii_lowercase();
        if dir == "." || dir == ".." || low == ".git" || low == ".pi" || low.starts_with(".pi.") {
            return Err(format!("refuses {dir}: that is not build output"));
        }
    }
    Ok(())
}

fn follow(v: &str) -> Result<(), String> {
    if v == "release" {
        return Ok(());
    }
    if let Some(line) = v.strip_prefix("release:") {
        let parts = line.split('.').count();
        if parts <= 2 && line.split('.').all(|p| digits(p) && p.len() <= 6) {
            return Ok(());
        }
        return Err("expected release:<line>, a major version or major.minor such as 0.14".into());
    }
    if let Some(name) = v.strip_prefix("branch:") {
        if all(name, class("._/-")) {
            return Ok(());
        }
        return Err("expected branch:<name>, a name of letters, digits and ._/-".into());
    }
    if let Some(sha) = v.strip_prefix("commit:") {
        // The whole commit: an abbreviation could name another one later.
        if sha.len() == 40 && all(sha, |c| matches!(c, '0'..='9' | 'a'..='f')) {
            return Ok(());
        }
        return Err("expected commit: and 40 lower-case hex digits".into());
    }
    Err("expected release, release:<line>, branch:<name> or commit:<40 hex>".into())
}

fn low_gate(v: &str, trees: &Trees) -> Result<(), String> {
    let path = v.strip_prefix('/').is_some_and(|rest| {
        rest.split('/')
            .all(|p| all(p, class("._-")) && p != "." && p != "..")
    });
    if !path {
        return Err(GATE_PATH.to_string());
    }
    // The gate is run as the tehas user by every poll. A merged pull request
    // writes the trees under the host's tehas home and the instance directory,
    // and install.sh puts a new bin/ or libexec/ file there from the same
    // change, so a gate named by an instance setting must be a program no pull
    // request can put in place.
    for base in [trees.home, trees.instance, trees.work_repo] {
        let base = base.strip_suffix('/').unwrap_or(base);
        if !base.is_empty() && format!("{v}/").starts_with(&format!("{base}/")) {
            return Err(format!(
                "expected a command outside {base}, which a merged pull request can change"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn why(name: &str, v: &str) -> Option<String> {
        check(name, v, &Trees::default()).err()
    }
    fn takes(name: &str, v: &str) {
        assert_eq!(why(name, v), None, "{name}={v}");
    }
    fn refuses(name: &str, v: &str, reason: &str) {
        assert_eq!(why(name, v).as_deref(), Some(reason), "{name}={v}");
    }

    #[test]
    fn a_repository_is_owner_and_name() {
        takes("TEHAS_REPO", "acme/widget.js_2");
        refuses("TEHAS_REPO", "acme", "expected owner/name");
        refuses("TEHAS_REPO", "a/b/c", "expected owner/name");
        refuses("TEHAS_REPO", "/b", "expected owner/name");
    }

    #[test]
    fn a_count_has_no_leading_zero_and_nine_digits() {
        takes("TEHAS_WORK_TIMEOUT", "999999999");
        refuses("TEHAS_WORK_TIMEOUT", "0", POSITIVE);
        refuses("TEHAS_WORK_TIMEOUT", "01", POSITIVE);
        refuses(
            "TEHAS_WORK_TIMEOUT",
            "1000000000",
            "expected at most 9 digits",
        );
    }

    #[test]
    fn auto_is_named_only_where_the_poll_reads_it() {
        takes("TEHAS_TICKET_CONCURRENCY", "3");
        refuses("TEHAS_TICKET_CONCURRENCY", "auto", AUTO);
        refuses("TEHAS_TICKET_CONCURRENCY", "AUTO", POSITIVE);
        refuses("TEHAS_CONCURRENCY", "auto", POSITIVE);
    }

    #[test]
    fn the_slice_holds_two_to_sixteen() {
        takes("TEHAS_SLICE_CONTAINERS", "2");
        takes("TEHAS_SLICE_CONTAINERS", "16");
        refuses("TEHAS_SLICE_CONTAINERS", "auto", AUTO);
        for bad in ["1", "17", "06", "1.5", "99999999999999999999"] {
            refuses(
                "TEHAS_SLICE_CONTAINERS",
                bad,
                "expected a whole number from 2 to 16",
            );
        }
    }

    #[test]
    fn the_wait_for_a_low_gate_is_ten_minutes_at_most() {
        takes("TEHAS_LOW_GATE_WAIT", "600");
        refuses("TEHAS_LOW_GATE_WAIT", "soon", POSITIVE);
        for bad in ["601", "9999", "99999999999999999999"] {
            refuses("TEHAS_LOW_GATE_WAIT", bad, "expected at most 600 seconds");
        }
    }

    #[test]
    fn logins_are_separated_by_single_spaces() {
        takes("TEHAS_ALLOWED_LABELERS", "alice bob-2 carol");
        let why = "expected GitHub logins separated by single spaces";
        for bad in ["alice  bob", "alice ", " alice", "alice;rm", "-a", ""] {
            refuses("TEHAS_ALLOWED_LABELERS", bad, why);
        }
    }

    #[test]
    fn a_login_may_end_in_one_bot() {
        takes("TEHAS_MILLWRIGHT_LOGIN", "acme-mill");
        takes("TEHAS_BOT_LOGIN", "acme-app[bot]");
        for bad in ["a[bot][bot]", "[bot]", "a[BOT]", "a[bot]x", "a b", "-a"] {
            refuses("TEHAS_MILLWRIGHT_LOGIN", bad, LOGIN_MILLWRIGHT);
            refuses("TEHAS_BOT_LOGIN", bad, LOGIN_BOT);
        }
    }

    #[test]
    fn an_app_id_is_twelve_digits_at_most() {
        takes("TEHAS_GITHUB_APP_ID", "123456789012");
        let why = "expected the ID GitHub shows, a whole number";
        refuses("TEHAS_GITHUB_APP_ID", "1234567890123", why);
        refuses("TEHAS_GITHUB_APP_INSTALLATION", "0", why);
    }

    #[test]
    fn a_word_setting_takes_its_words() {
        takes("TEHAS_THEME", "beesknees26");
        refuses("TEHAS_THEME", "../pixel", THEMES);
        takes("TEHAS_LAYOUT", "business");
        refuses("TEHAS_LAYOUT", "game", "expected floor or business");
        takes("TEHAS_ANALYTICS", "off");
        refuses("TEHAS_ANALYTICS", "On", "expected on or off");
        takes("TEHAS_UPDATE_FROM", "git");
        refuses("TEHAS_UPDATE_FROM", "tag", "expected git or release");
        takes("TEHAS_CYCLE_REPORT", "rust");
        refuses("TEHAS_CYCLE_REPORT", "perl", "expected bash or rust");
    }

    #[test]
    fn a_switch_is_0_or_1() {
        for name in ["TEHAS_INDEXING", "TEHAS_PREVIEWS", "TEHAS_MILLWRIGHT_FEED"] {
            takes(name, "1");
            refuses(name, "2", "expected 0 or 1");
        }
    }

    #[test]
    fn a_script_address_is_https_with_a_path() {
        takes("TEHAS_ANALYTICS_SRC", "https://a.example:8443/x/ea.js");
        takes("TEHAS_ANALYTICS_SRC", "https://a.example/");
        let why = "expected the https address of a script";
        for bad in [
            "http://a.example/x.js",
            "https://a.example",
            "https://a.example/x.js\" onload=\"x",
            "https://a.example:/x.js",
            "https:///x.js",
        ] {
            refuses("TEHAS_ANALYTICS_SRC", bad, why);
        }
    }

    #[test]
    fn a_site_is_one_word() {
        takes("TEHAS_ANALYTICS_SITE", "a.b_c-d");
        refuses(
            "TEHAS_ANALYTICS_SITE",
            "two words",
            "expected one word of letters, digits, dots and dashes",
        );
    }

    #[test]
    fn a_container_size_is_at_least_6m_with_no_leading_zero() {
        takes("TEHAS_CYCLE_RESERVE", "6m");
        takes("TEHAS_CONTAINER_MEMORY", "6291456");
        let why = "expected a size of at least 6m, such as 512m or 2g";
        for bad in [
            "lots",
            "2gb",
            "0",
            "5m",
            "6000k",
            "-1g",
            "1.5g",
            "02g",
            "1000000000g",
        ] {
            refuses("TEHAS_CYCLE_RESERVE", bad, why);
        }
    }

    #[test]
    fn a_host_address_is_a_name_or_an_address() {
        takes("TEHAS_HOST_ADDR", "fe80::1");
        refuses(
            "TEHAS_HOST_ADDR",
            "not an address",
            "expected an address or host name",
        );
    }

    #[test]
    fn the_lab_address_has_no_path() {
        takes("TEHAS_PROTOLAB_URL", "http://lab.example:8080");
        let why =
            "expected an address such as https://lab.example, with no path and no slash at the end";
        refuses("TEHAS_PROTOLAB_URL", "https://lab.example/", why);
        refuses("TEHAS_PROTOLAB_URL", "lab.example", why);
    }

    #[test]
    fn a_domain_has_two_labels_in_lower_case() {
        takes("TEHAS_DOMAIN", "a-b.example.org");
        let why =
            "expected a domain such as example.org, in lower case, with no scheme and no path";
        for bad in [
            "localhost",
            "Example.org",
            "-a.example.org",
            "a.b-",
            "a..b",
            "example.org/",
        ] {
            refuses("TEHAS_DOMAIN", bad, why);
        }
    }

    #[test]
    fn previews_are_one_to_ninety_nine() {
        takes("TEHAS_PREVIEW_MAX", "99");
        refuses("TEHAS_PREVIEW_MAX", "100", "expected a number from 1 to 99");
        refuses("TEHAS_PREVIEW_MAX", "0", "expected a number from 1 to 99");
    }

    #[test]
    fn free_disk_is_a_size_above_nothing() {
        takes("TEHAS_CYCLE_MIN_FREE", "1048576b");
        takes("TEHAS_CLEAN_MIN_FREE", "08m");
        takes("TEHAS_PREVIEW_MIN_FREE", "010g");
        let why = "expected a size such as 512m or 5g";
        for bad in ["lots", "0", "0m", "1k", "1024b", "9223372036854775808m"] {
            refuses("TEHAS_CYCLE_MIN_FREE", bad, why);
        }
    }

    #[test]
    fn a_size_shell_arithmetic_could_not_read_is_refused() {
        // In bash this ended tehas_load with no word and status 0.
        for bad in ["09g", "08", "09k"] {
            refuses(
                "TEHAS_CYCLE_MIN_FREE",
                bad,
                "expected a size such as 512m or 5g",
            );
        }
    }

    #[test]
    fn a_size_in_mib_is_the_figure_as_arithmetic_reads_it() {
        assert_eq!(mem_mb("2g").as_deref(), Some("2048"));
        assert_eq!(mem_mb("512M").as_deref(), Some("512"));
        assert_eq!(mem_mb("08m").as_deref(), Some("08"));
        assert_eq!(mem_mb("010g").as_deref(), Some("8192"));
        assert_eq!(mem_mb("1048576k").as_deref(), Some("1024"));
        assert_eq!(mem_mb("2097152").as_deref(), Some("2"));
        assert_eq!(
            mem_mb("99999999999999999999g").as_deref(),
            Some("2123646838278978560")
        );
        assert_eq!(mem_mb("1.5g"), None);
        assert_eq!(mem_mb(""), None);
    }

    #[test]
    fn a_workaround_is_a_name_and_a_version() {
        takes(
            "TEHAS_RUKAS_WORKAROUNDS",
            "PI_ENSEMBLE_X@0.12.80 TEHAS_Y_2@1.2.3.4",
        );
        let why = "expected NAME@VERSION entries separated by single spaces, such as PI_ENSEMBLE_COMMENT_RETENTION@0.12.80";
        for bad in [
            "PI_ENSEMBLE_X",
            "PI_ENSEMBLE_X@soon",
            "PATH@1.2",
            "PI_ENSEMBLE_X@1",
            "PI_ENSEMBLE_X@1.2.3.4.5",
            "PI_ENSEMBLE_X@1.1234567",
            "PI_ENSEMBLE_X@1.2  TEHAS_TOP@1.3",
            "TEHAS_@1.2",
            "TEHAS_a@1.2",
        ] {
            refuses("TEHAS_RUKAS_WORKAROUNDS", bad, why);
        }
    }

    #[test]
    fn the_refresh_lanes_are_one_digit() {
        takes("TEHAS_REFRESH_SLOTS", "0");
        takes("TEHAS_REFRESH_SLOTS", "8");
        for bad in ["9", "01", "-1", "one", "é"] {
            refuses(
                "TEHAS_REFRESH_SLOTS",
                bad,
                "expected a whole number from 0 to 8",
            );
        }
    }

    #[test]
    fn the_refresh_minutes_are_a_day_at_most() {
        takes("TEHAS_REFRESH_EVERY", "1440");
        let why = "expected a whole number of minutes from 1 to 1440";
        for bad in ["0", "1441", "010", "10m", "99999"] {
            refuses("TEHAS_REFRESH_EVERY", bad, why);
        }
    }

    #[test]
    fn the_cleaners_hours_are_five_digits() {
        takes("TEHAS_CLEAN_AFTER", "99999");
        refuses(
            "TEHAS_CLEAN_AFTER",
            "123456",
            "expected a whole number of hours of at least 1",
        );
    }

    #[test]
    fn a_budget_is_in_the_stores_order_and_grammar() {
        for good in [
            "cpu=5",
            "wall=0",
            "cpu=5,wall=30",
            "cpu=0,wall=0,writes=0",
            "wall=1,writes=2",
        ] {
            takes("TEHAS_POLL_BUDGET", good);
        }
        let shape = "expected cpu=<s>,wall=<s>,writes=<n>, any part left out";
        for bad in [
            "cpu=",
            "cpu",
            "cpu=1,writes=2",
            "wall=1,cpu=1",
            "cpu=1,cpu=2",
            "cpu=1000000000",
            " cpu=1",
        ] {
            refuses("TEHAS_POLL_BUDGET", bad, shape);
        }
        refuses(
            "TEHAS_POLL_BUDGET",
            "cpu=5,wall=030",
            "expected cpu=<s>,wall=<s>,writes=<n>, and no leading 0",
        );
    }

    #[test]
    fn a_measure_bound_is_eight_digits() {
        takes("TEHAS_SIZE_MAX_BYTES", "99999999");
        for bad in ["0", "100000000", "1e3"] {
            refuses(
                "TEHAS_SIZE_MAX_LINES",
                bad,
                "expected a whole number of at least 1",
            );
        }
    }

    #[test]
    fn a_scratch_directory_is_absolute() {
        takes("TEHAS_SCRATCH_DIR", "/mnt/scratch");
        let why = "expected an absolute path such as /mnt/scratch";
        for bad in ["scratch", "/", "/a/", "/a//b", "/a/../b", "/a/b..c", "/a b"] {
            refuses("TEHAS_SCRATCH_DIR", bad, why);
        }
    }

    #[test]
    fn the_cleaner_takes_names_and_none_a_workspace_keeps() {
        takes("TEHAS_CLEAN_DIRS", CLEAN_DIRS);
        takes("TEHAS_CLEAN_DIRS", ".pip .gitx");
        let shape =
            "expected directory names of letters, digits and ._-, separated by single spaces";
        for bad in ["a/target", "target *", "two  spaces", " a"] {
            refuses("TEHAS_CLEAN_DIRS", bad, shape);
        }
        for dir in [".", "..", ".git", ".GIT", ".pi", ".Pi", ".pi.bak"] {
            let why = format!("refuses {dir}: that is not build output");
            refuses("TEHAS_CLEAN_DIRS", &format!("target {dir}"), &why);
        }
    }

    #[test]
    fn follow_is_a_release_a_branch_or_a_whole_commit() {
        let sha = "a".repeat(40);
        for good in [
            "release".to_string(),
            "release:0.14".to_string(),
            "release:2".to_string(),
            "branch:feature/one".to_string(),
            format!("commit:{sha}"),
        ] {
            takes("PI_RUKAS_FOLLOW", &good);
        }
        let line = "expected release:<line>, a major version or major.minor such as 0.14";
        refuses("PI_RUKAS_FOLLOW", "release:", line);
        refuses("PI_RUKAS_FOLLOW", "release:0.14.1", line);
        refuses("PI_RUKAS_FOLLOW", "release:v0", line);
        refuses(
            "PI_RUKAS_FOLLOW",
            "branch:two words",
            "expected branch:<name>, a name of letters, digits and ._/-",
        );
        let hex = "expected commit: and 40 lower-case hex digits";
        refuses("PI_RUKAS_FOLLOW", "commit:abc", hex);
        refuses(
            "PI_RUKAS_FOLLOW",
            &format!("commit:{}", "A".repeat(40)),
            hex,
        );
        let any = "expected release, release:<line>, branch:<name> or commit:<40 hex>";
        refuses("PI_RUKAS_FOLLOW", "main", any);
        refuses("PI_RUKAS_FOLLOW", "", any);
    }

    #[test]
    fn a_check_is_words_a_url_can_hold() {
        takes("TEHAS_BRANCH_CHECK", "build / verify (linux)");
        let why = "expected the name of a check, words of letters, digits and ._()/- separated by single spaces";
        for bad in ["verify&x=1", "two  spaces", " verify", "a\"b"] {
            refuses("TEHAS_BRANCH_CHECK", bad, why);
        }
    }

    #[test]
    fn a_low_gate_is_a_named_absolute_path() {
        takes("TEHAS_LOW_GATE", "/x/gate");
        for bad in [
            "gate",
            "bin/gate",
            "//gate",
            "/x/gate/",
            "/.",
            "/..",
            "/x/./gate",
            "/x/../gate",
            "/x//gate",
            "/x/gate ",
        ] {
            refuses("TEHAS_LOW_GATE", bad, GATE_PATH);
        }
    }

    #[test]
    fn a_low_gate_sits_outside_the_trees_a_pull_request_writes() {
        let trees = Trees {
            home: "/h/tehas/",
            instance: "/etc/instance",
            work_repo: "/h/tehas/repos/w",
        };
        let inside = |v| check("TEHAS_LOW_GATE", v, &trees).unwrap_err();
        let home = "expected a command outside /h/tehas, which a merged pull request can change";
        assert_eq!(inside("/h/tehas/bin/gate"), home);
        assert_eq!(inside("/h/tehas"), home);
        assert_eq!(
            inside("/etc/instance/gate"),
            "expected a command outside /etc/instance, which a merged pull request can change"
        );
        assert_eq!(check("TEHAS_LOW_GATE", "/h/tehas2/gate", &trees), Ok(()));
    }

    #[test]
    fn a_model_is_one_word() {
        for name in [
            "PI_ENSEMBLE_SUBAGENT_MODEL",
            "PI_ENSEMBLE_SUBAGENT_PROVIDER",
            "PI_ENSEMBLE_MODEL_OPS",
            "PI_ENSEMBLE_PROVIDER_ANYTHING",
            "TEHAS_BRANCH",
        ] {
            takes(name, "qwen3.8-flash");
            refuses(name, "two words", "expected one word");
            refuses(name, "tab\tbed", "expected one word");
            refuses(name, "", "expected one word");
        }
    }

    #[test]
    fn a_name_with_no_rule_takes_anything() {
        takes("TEHAS_CYCLE_CPUS", "any thing");
        takes("PI_ENSEMBLE_HOST_ALIASES", "");
    }

    #[test]
    fn every_name_has_one_row() {
        let mut names: Vec<_> = TABLE.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), TABLE.len());
    }
}
