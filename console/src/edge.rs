//! What tehasbot protolab and tehasbot preview share: a numbered set under
//! the home, and the previews' network.
//!
//!   console preview network check|create|ensure <network>
//!
//! A preview runs a pull request's own code, so its network has to keep it
//! from the host. What does that is checked, not a name: the network is
//! internal (no way out), a bridge (the option below means nothing to another
//! driver), without IPv6 (the option is about IPv4 only, and with IPv6 the
//! host has an address of that kind on the bridge), and made with
//! gateway_mode_ipv4 "isolated" (the host has no IPv4 address on the bridge)
//! by a Docker that knows that option. Docker before 27.1 does not know it.
//! What is known: the bridge driver of Docker 29.1.3 was seen to accept an
//! option it does not know and to show it when asked. That an older Docker
//! takes this one and ignores it was not tried. The version is checked so
//! that it does not matter, and an old Docker is not asked to make anything.
//!
//! `check` is install.sh's: it changes nothing. `create` makes the network.
//! `ensure` is what tehasbot preview does: it makes a network Docker does not
//! have, then checks.
//!
//! Exit, with the reason on standard output for 1, 2 and 3: 0 the network
//! keeps the host out, or `create` made it; 1 it does not, or `create` failed;
//! 2 Docker shows no such network; 3 this Docker cannot keep the host out, and
//! the network was not asked about; 64 usage.

use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const OPTION: &str = "com.docker.network.bridge.gateway_mode_ipv4";

/// `$TEHAS_HOME/state/<name>.lock`, held until the file is dropped. Without a
/// wait, another holder is an answer at once. None when it is held or cannot
/// be opened. The file closes on exec, so a child cannot keep the lock.
pub fn lock(home: &Path, name: &str, wait: Option<Duration>) -> Option<File> {
    let state = home.join("state");
    fs::create_dir_all(&state).ok()?;
    let file = File::create(state.join(format!("{name}.lock"))).ok()?;
    let deadline = Instant::now() + wait.unwrap_or_default();
    loop {
        if file.try_lock().is_ok() {
            return Some(file);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// The bytes become the file. A server may read it at any moment, so they are
/// written beside it and renamed. A failure leaves the file as it was.
pub fn put(file: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut beside = file.as_os_str().to_owned();
    beside.push(".new");
    fs::write(&beside, bytes)
        .and_then(|()| fs::rename(&beside, file))
        .inspect_err(|_| {
            let _ = fs::remove_file(&beside);
        })
}

/// The new directory takes the place by rename. The old one is moved aside
/// first and removed afterwards, so there is no moment with a half-copied one.
pub fn place(new: &Path, place: &Path) -> io::Result<()> {
    let old = place.parent().unwrap_or(Path::new(".")).join(".old");
    remove(&old)?;
    if place.is_dir() {
        fs::rename(place, &old)?;
    }
    fs::rename(new, place)?;
    remove(&old)
}

pub fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotADirectory => fs::remove_file(path),
        done => done,
    }
}

/// The subdirectories whose name is a number, in the order of their names.
/// A directory that cannot be read has none.
pub fn numbers(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    found.sort();
    found
}

/// Docker's major and minor, from what it says of its server. Anything but
/// `<1-4 digits>.<1-4 digits>` and a suffix of `.+~-` and what follows is none.
fn version(said: &str) -> Option<(u32, u32)> {
    let digits = |text: &str| {
        let end = text
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len());
        (1..=4).contains(&end).then_some(end)
    };
    let (major, rest) = said.split_once('.')?;
    if digits(major)? != major.len() {
        return None;
    }
    let (minor, suffix) = rest.split_at(digits(rest)?);
    let plain = |c: char| c.is_ascii_alphanumeric() || ".+~-".contains(c);
    if !suffix.is_empty()
        && (!suffix.starts_with(['.', '+', '~', '-']) || !suffix.chars().all(plain))
    {
        return None;
    }
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// Why this Docker cannot keep the host out: older than 27.1, or no version
/// that can be read. `docker` answers with standard output, or None on failure.
fn docker_fault(docker: &mut impl FnMut(&[&str]) -> Option<String>) -> Option<String> {
    let said = docker(&["version", "--format", "{{.Server.Version}}"]).unwrap_or_default();
    let said = said.trim_end_matches('\n');
    match version(said) {
        None => Some("Docker's version could not be read".to_string()),
        Some(seen) if seen < (27, 1) => Some(format!("Docker {said} is older than 27.1")),
        Some(_) => None,
    }
}

/// The four words of Docker's answer, split as `read` does: on blanks in the
/// first line, what is left over staying in the last.
pub fn words(said: &str) -> [&str; 4] {
    let blank = [' ', '\t'];
    let mut rest = said.split('\n').next().unwrap_or("").trim_matches(blank);
    let mut found = [""; 4];
    for word in found.iter_mut().take(3) {
        let (first, after) = rest.split_once(blank).unwrap_or((rest, ""));
        *word = first;
        rest = after.trim_start_matches(blank);
    }
    found[3] = rest;
    found
}

/// The exit status and the reason for a network that is not as it should be.
/// It does not ask for Docker's version: a caller asks docker_fault first.
fn net_fault(
    docker: &mut impl FnMut(&[&str]) -> Option<String>,
    network: &str,
) -> Option<(u8, &'static str)> {
    let format = format!(
        "{{{{.Internal}}}} {{{{.Driver}}}} {{{{.EnableIPv6}}}} {{{{index .Options \"{OPTION}\"}}}}"
    );
    let Some(said) = docker(&["network", "inspect", "--format", &format, network]) else {
        return Some((2, "it could not be read"));
    };
    let [internal, driver, ipv6, mode] = words(&said);
    let why = if internal != "true" {
        "it is not internal"
    } else if driver != "bridge" {
        "its driver is not bridge"
    } else if ipv6 != "false" {
        "it has IPv6"
    } else if mode != "isolated" {
        "it is not isolated from the host"
    } else {
        return None;
    };
    Some((1, why))
}

fn net_create(docker: &mut impl FnMut(&[&str]) -> Option<String>, network: &str) -> bool {
    let option = format!("{OPTION}=isolated");
    let create = [
        "network",
        "create",
        "--internal",
        "--ipv6=false",
        "-o",
        &option,
        network,
    ];
    docker(&create).is_some()
}

fn check(
    docker: &mut impl FnMut(&[&str]) -> Option<String>,
    network: &str,
) -> (u8, Option<String>) {
    if let Some(why) = docker_fault(docker) {
        return (3, Some(why));
    }
    net_answer(docker, network)
}

fn ensure(
    docker: &mut impl FnMut(&[&str]) -> Option<String>,
    network: &str,
) -> (u8, Option<String>) {
    if let Some(why) = docker_fault(docker) {
        return (3, Some(why));
    }
    if docker(&["network", "inspect", network]).is_none() {
        net_create(docker, network);
    }
    net_answer(docker, network)
}

fn net_answer(
    docker: &mut impl FnMut(&[&str]) -> Option<String>,
    network: &str,
) -> (u8, Option<String>) {
    match net_fault(docker, network) {
        Some((status, why)) => (status, Some(why.to_string())),
        None => (0, None),
    }
}

/// Why the network does not keep the host out, as `ensure` finds it: a network
/// Docker does not have is made first.
pub fn network_fault(network: &str) -> Option<String> {
    let mut ask = docker;
    ensure(&mut ask, network).1
}

pub fn docker(args: &[&str]) -> Option<String> {
    let out = Command::new("docker")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `console preview network`: `preview` took its own word off the arguments.
pub fn network_main(given: &[String]) -> u8 {
    let [word, network] = given else {
        return usage();
    };
    let mut ask = docker;
    let (status, why) = match word.as_str() {
        "check" if !network.is_empty() => check(&mut ask, network),
        "ensure" if !network.is_empty() => ensure(&mut ask, network),
        "create" if !network.is_empty() => (u8::from(!net_create(&mut ask, network)), None),
        _ => return usage(),
    };
    if let Some(why) = why {
        println!("{why}");
    }
    status
}

fn usage() -> u8 {
    eprintln!("console preview network: usage: check|create|ensure <network>");
    64
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: &str = "version --format {{.Server.Version}}";
    const INSPECT: &str = "network inspect --format {{.Internal}} {{.Driver}} {{.EnableIPv6}} \
         {{index .Options \"com.docker.network.bridge.gateway_mode_ipv4\"}} net";
    const CREATE: &str = "network create --internal --ipv6=false \
         -o com.docker.network.bridge.gateway_mode_ipv4=isolated net";

    /// A Docker that answers from a list of (start of the call, answer), and
    /// what it was asked, in order.
    struct Fake {
        answers: Vec<(String, Option<String>)>,
        calls: Vec<String>,
    }

    impl Fake {
        fn new(answers: &[(&str, Option<&str>)]) -> Self {
            Fake {
                answers: answers
                    .iter()
                    .map(|(call, said)| (call.to_string(), said.map(str::to_string)))
                    .collect(),
                calls: Vec::new(),
            }
        }

        /// A new Docker with `answer` to the inspect that names the fields.
        fn network(answer: Option<&str>) -> Self {
            Fake::new(&[
                (VERSION, Some("29.1.3\n")),
                ("network inspect --format", answer),
            ])
        }

        fn ask(&mut self, args: &[&str]) -> Option<String> {
            let call = args.join(" ");
            let said = self
                .answers
                .iter()
                .find(|(start, _)| call.starts_with(start))
                .map_or(Some(String::new()), |(_, said)| said.clone());
            self.calls.push(call);
            said
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("edge-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn a_version_is_read_as_the_shell_read_it() {
        for (said, seen) in [
            ("29.1.3", (29, 1)),
            ("27.1.0-rc1", (27, 1)),
            ("27.10.0", (27, 10)),
            ("100.0", (100, 0)),
            ("19.03.15", (19, 3)),
            ("27.1+dfsg", (27, 1)),
            ("27.1~3", (27, 1)),
            ("9999.9999", (9999, 9999)),
        ] {
            assert_eq!(version(said), Some(seen), "{said}");
        }
        for said in [
            "",
            "latest",
            "27",
            "27.",
            ".1",
            "27.x.1",
            "v29.1.3",
            "29.1.3; id",
            "29.1.3 ",
            "29.1.3\nmore",
            "27.1_0",
            "12345.1",
            "1.12345",
            "27.1a",
            "٢٧.1",
        ] {
            assert_eq!(version(said), None, "{said:?}");
        }
    }

    #[test]
    fn a_docker_older_than_27_1_or_unread_has_a_fault() {
        for (said, why) in [
            (Some("26.1.4\n"), Some("Docker 26.1.4 is older than 27.1")),
            (Some("27.0.3\n"), Some("Docker 27.0.3 is older than 27.1")),
            (Some("27.0\n"), Some("Docker 27.0 is older than 27.1")),
            (
                Some("19.03.15\n"),
                Some("Docker 19.03.15 is older than 27.1"),
            ),
            (Some("\n"), Some("Docker's version could not be read")),
            (
                Some("29.1.3\nx\n"),
                Some("Docker's version could not be read"),
            ),
            (None, Some("Docker's version could not be read")),
            (Some("27.1.0\n"), None),
            (Some("28.0.1\n"), None),
        ] {
            let mut fake = Fake::new(&[(VERSION, said)]);
            assert_eq!(
                docker_fault(&mut |args: &[&str]| fake.ask(args)).as_deref(),
                why,
                "{said:?}"
            );
        }
    }

    #[test]
    fn each_fault_of_a_network_has_its_reason() {
        for (said, want) in [
            ("true bridge false isolated\n", None),
            ("  true\tbridge  false isolated  \n", None),
            (
                "false bridge false isolated\n",
                Some((1, "it is not internal")),
            ),
            ("\n", Some((1, "it is not internal"))),
            (
                "\ntrue bridge false isolated\n",
                Some((1, "it is not internal")),
            ),
            (
                "true ipvlan false isolated\n",
                Some((1, "its driver is not bridge")),
            ),
            ("true bridge true isolated\n", Some((1, "it has IPv6"))),
            ("true bridge  \n", Some((1, "it has IPv6"))),
            (
                "true bridge false \n",
                Some((1, "it is not isolated from the host")),
            ),
            (
                "true bridge false nat\n",
                Some((1, "it is not isolated from the host")),
            ),
            (
                "true bridge false isolated\r\n",
                Some((1, "it is not isolated from the host")),
            ),
            (
                "true bridge false isolated x\n",
                Some((1, "it is not isolated from the host")),
            ),
        ] {
            let mut fake = Fake::network(Some(said));
            assert_eq!(
                net_fault(&mut |args: &[&str]| fake.ask(args), "net"),
                want,
                "{said:?}"
            );
            assert_eq!(fake.calls, [INSPECT]);
        }
    }

    #[test]
    fn a_network_docker_does_not_show_is_status_2() {
        let mut fake = Fake::network(None);
        assert_eq!(
            net_fault(&mut |args: &[&str]| fake.ask(args), "net"),
            Some((2, "it could not be read"))
        );
    }

    #[test]
    fn check_asks_the_version_then_the_network_and_makes_nothing() {
        let mut fake = Fake::network(None);
        let answer = check(&mut |args: &[&str]| fake.ask(args), "net");
        assert_eq!(answer, (2, Some("it could not be read".to_string())));
        assert_eq!(fake.calls, [VERSION, INSPECT]);

        let mut fake = Fake::network(Some("true bridge false isolated\n"));
        assert_eq!(check(&mut |args: &[&str]| fake.ask(args), "net"), (0, None));
    }

    #[test]
    fn an_old_docker_is_asked_about_no_network() {
        let answer = Some("Docker 26.1.4 is older than 27.1".to_string());
        let mut fake = Fake::new(&[(VERSION, Some("26.1.4\n"))]);
        assert_eq!(
            check(&mut |args: &[&str]| fake.ask(args), "net"),
            (3, answer.clone())
        );
        assert_eq!(fake.calls, [VERSION]);
        let mut fake = Fake::new(&[(VERSION, Some("26.1.4\n"))]);
        assert_eq!(
            ensure(&mut |args: &[&str]| fake.ask(args), "net"),
            (3, answer)
        );
        assert_eq!(fake.calls, [VERSION]);
    }

    #[test]
    fn ensure_makes_a_missing_network_and_checks_what_it_made() {
        let mut fake = Fake::new(&[
            (VERSION, Some("29.1.3\n")),
            (
                "network inspect --format",
                Some("true bridge false isolated\n"),
            ),
            ("network inspect net", None),
        ]);
        assert_eq!(
            ensure(&mut |args: &[&str]| fake.ask(args), "net"),
            (0, None)
        );
        assert_eq!(
            fake.calls,
            [VERSION, "network inspect net", CREATE, INSPECT]
        );
    }

    #[test]
    fn ensure_leaves_a_network_that_is_there() {
        let mut fake = Fake::network(Some("true bridge false nat\n"));
        assert_eq!(
            ensure(&mut |args: &[&str]| fake.ask(args), "net"),
            (1, Some("it is not isolated from the host".to_string()))
        );
        assert_eq!(fake.calls, [VERSION, "network inspect net", INSPECT]);
    }

    #[test]
    fn create_says_whether_docker_made_it() {
        let mut fake = Fake::new(&[("network create", None)]);
        assert!(!net_create(&mut |args: &[&str]| fake.ask(args), "net"));
        assert_eq!(fake.calls, [CREATE]);
        let mut fake = Fake::new(&[]);
        assert!(net_create(&mut |args: &[&str]| fake.ask(args), "net"));
    }

    #[test]
    fn a_usage_is_64_so_it_is_no_status_of_the_network() {
        for given in [&["check"][..], &["open", "net"], &["check", ""], &[]] {
            let given: Vec<String> = given.iter().map(|w| w.to_string()).collect();
            assert_eq!(network_main(&given), 64);
        }
    }

    #[test]
    fn a_lock_is_held_by_one_at_a_time() {
        let home = scratch("lock");
        let held = lock(&home, "demo", None).expect("the first takes it");
        assert!(home.join("state/demo.lock").exists());
        assert!(lock(&home, "demo", None).is_none());
        assert!(lock(&home, "demo", Some(Duration::from_millis(150))).is_none());
        drop(held);
        // With a wait: a child another test is forking holds the file until it
        // has exec'd, and that outlives the drop.
        assert!(lock(&home, "demo", Some(Duration::from_secs(30))).is_some());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn put_writes_the_whole_file_or_leaves_the_old_one() {
        let root = scratch("put");
        let file = root.join("list.json");
        fs::write(&file, "old").unwrap();
        put(&file, b"new").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
        assert!(!root.join("list.json.new").exists());

        assert!(put(&root.join("nowhere/list.json"), b"x").is_err());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);

        // A rename that fails: the place is a directory that holds something.
        let taken = root.join("taken");
        fs::create_dir_all(taken.join("inside")).unwrap();
        assert!(put(&taken, b"x").is_err());
        assert!(!root.join("taken.new").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn place_puts_a_directory_in_place_of_the_old_one() {
        let root = scratch("place");
        let new = root.join(".work/7");
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("page"), "new").unwrap();
        let site = root.join("7");
        fs::create_dir_all(&site).unwrap();
        fs::write(site.join("page"), "old").unwrap();
        // What a run that died left beside it does not stop this one.
        fs::create_dir_all(root.join(".old/left")).unwrap();

        place(&new, &site).unwrap();
        assert_eq!(fs::read_to_string(site.join("page")).unwrap(), "new");
        assert!(!new.exists());
        assert!(!root.join(".old").exists());

        // The first time there is nothing to move aside, and a new one that is
        // not there leaves the old one out of the way, as the shell does.
        let first = root.join(".work/8");
        fs::create_dir_all(&first).unwrap();
        fs::write(root.join(".old"), "a file left there").unwrap();
        place(&first, &root.join("8")).unwrap();
        assert!(root.join("8").is_dir());
        assert!(place(&root.join("gone"), &root.join("9")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn numbers_are_the_numbered_directories_only() {
        let root = scratch("numbers");
        for name in ["7", "12", "007", "12a", ".1", "x"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        fs::write(root.join("9"), "a file").unwrap();
        std::os::unix::fs::symlink(root.join("7"), root.join("8")).unwrap();
        assert_eq!(numbers(&root), ["007", "12", "7", "8"]);
        assert!(numbers(&root.join("nowhere")).is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
