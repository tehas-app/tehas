//! The readers of a container's docker words, shared by the Rust tests of
//! the containers an agent runs in: the cycle's (console/cycle/tests/work/)
//! and the manual session's (console/tests/dev.rs). The documented security
//! options have one home in the Rust tests: the literal below, as
//! test/docker_args.sh is theirs in the shell. It is not read from
//! console/core/src/sandbox.rs, which is the code under test.

/// The options of the slice limits, by name.
#[allow(dead_code)] // each test file uses a part of this module
pub const LIMITS: &str =
    "--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares";

/// The options of the slice limits and of the security list, by name: the
/// allow-list of unknown_options for every suite of a cycle's container.
#[allow(dead_code)] // each test file uses a part of this module
pub const DOCKER_OPTS: &str = "--cgroup-parent --memory --memory-swap --memory-reservation --cpus --pids-limit --cpu-shares --cap-drop --cap-add --security-opt --tmpfs";

/// The documented security options of a container, for the ceiling the
/// stubbed slice gives it (--memory 2048m): /tmp is half of it. A literal, not
/// read from console/core/src/sandbox.rs, which is the code under test.
#[allow(dead_code)] // each test file uses a part of this module
pub const SECURITY_OPTS: &str = "--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777";

#[allow(dead_code)] // each test file uses a part of this module
/// The mount of the tmpfs and its options: the word after --tmpfs.
pub fn security_tmpfs() -> String {
    let w: Vec<&str> = SECURITY_OPTS.split(' ').collect();
    let i = w.iter().position(|x| *x == "--tmpfs").unwrap();
    w[i + 1].to_string()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The words after each `flag`, as docker reads them.
pub fn security_of(args: &[String], flag: &str) -> Vec<String> {
    args.windows(2)
        .filter(|p| p[0] == flag)
        .map(|p| p[1].clone())
        .collect()
}

#[allow(dead_code)] // each test file uses a part of this module
/// The documented options in order, each followed by a space, from their
/// first full run in `args`; "" when they are not there in one run.
pub fn security_line(args: &[String]) -> String {
    let doc: Vec<&str> = SECURITY_OPTS.split(' ').collect();
    args.windows(doc.len())
        .find(|w| w.iter().zip(&doc).all(|(a, d)| a == d))
        .map(|w| w.iter().map(|a| format!("{a} ")).collect())
        .unwrap_or_default()
}

#[allow(dead_code)] // each test file uses a part of this module
/// How many times an option stands in `args`, `--flag=<value>` among them.
pub fn count_of(args: &[String], flag: &str) -> usize {
    args.iter()
        .filter(|a| a.split_once('=').map_or(a.as_str(), |(t, _)| t) == flag)
        .count()
}

#[allow(dead_code)] // each test file uses a part of this module
/// Every option of a docker run that is not one the command is known to
/// pass, up to the image: `valued` names the options that take a value,
/// `plain` those that take none, each list words split by spaces.
pub fn unknown_options(args: &[String], image: &str, valued: &str, plain: &str) -> Vec<String> {
    let valued: Vec<&str> = valued.split_whitespace().collect();
    let plain: Vec<&str> = plain.split_whitespace().collect();
    let host = regex::Regex::new(r"^--add-host=[A-Za-z0-9.-]+:[0-9a-fA-F.:]+$").unwrap();
    let mut out = vec![];
    let mut skip = false;
    for (i, a) in args.iter().enumerate() {
        if i == 0 || (i == 1 && a == "--pull=never") {
            continue;
        }
        if skip {
            skip = false;
            continue;
        }
        if a == image {
            break;
        }
        let t = a.split_once('=').map_or(a.as_str(), |(t, _)| t);
        if valued.contains(&t) {
            skip = t == a;
            continue;
        }
        if plain.contains(&t) || host.is_match(a) {
            continue;
        }
        out.push(a.clone());
    }
    out
}
