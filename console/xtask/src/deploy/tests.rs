//! The deploy's decisions with git and ssh stood in for. test/tag_release_test.sh
//! runs the script against a bare remote and a stub ssh.

use super::*;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|a| a.to_string()).collect()
}

fn args(v: &[&str]) -> Args {
    match parse(&s(v)).unwrap() {
        Parsed::Run(a) => a,
        Parsed::Help => panic!("help"),
    }
}

const HEAD: &str = "1111111111111111111111111111111111111111";
const OTHER: &str = "2222222222222222222222222222222222222222";

/// A remote whose main is at HEAD with v0.1.0 on `tag_at`, and a host that
/// answers each ssh command with a status, and rev-parse with `host_at`.
struct Fake {
    fetch_ok: bool,
    ls_ok: bool,
    tag_at: String,
    reach: i32,
    update: i32,
    host_at: Option<String>,
    calls: Vec<String>,
    said: Vec<String>,
}

impl Fake {
    fn new() -> Fake {
        Fake {
            fetch_ok: true,
            ls_ok: true,
            tag_at: HEAD.into(),
            reach: 0,
            update: 0,
            host_at: Some(HEAD.into()),
            calls: Vec::new(),
            said: Vec::new(),
        }
    }

    fn ssh_calls(&self) -> Vec<&str> {
        self.calls
            .iter()
            .filter_map(|c| c.strip_prefix("ssh "))
            .collect()
    }

    fn said(&self) -> String {
        self.said.join("\n")
    }
}

impl Sys for Fake {
    fn git(&mut self, a: &[&str]) -> (String, bool) {
        self.calls.push(format!("git {}", a.join(" ")));
        match a[0] {
            "fetch" => (String::new(), self.fetch_ok),
            "ls-remote" => (
                format!("ref: refs/heads/main\tHEAD\n{HEAD}\tHEAD"),
                self.ls_ok,
            ),
            "tag" => ("v0.10.0-rc1\nv0.1.0\nv0.0.9".into(), true),
            "rev-parse" if a[3] == "v0.1.0^{commit}" => (self.tag_at.clone(), true),
            _ => (String::new(), false),
        }
    }

    fn ssh(&mut self, dest: &str, command: &str) -> i32 {
        self.calls.push(format!("ssh {dest} {command}"));
        if command.starts_with("test -r") {
            self.reach
        } else {
            self.update
        }
    }

    fn ssh_out(&mut self, dest: &str, command: &str) -> (String, i32) {
        self.calls.push(format!("ssh {dest} {command}"));
        match &self.host_at {
            Some(at) => (at.clone(), 0),
            None => (String::new(), 255),
        }
    }

    fn say(&mut self, line: &str) {
        self.said.push(line.to_string());
    }
}

fn deploy(f: &mut Fake, v: &[&str]) -> u8 {
    run(&args(v), f)
}

#[test]
fn the_arguments_are_read_in_order() {
    assert_eq!(
        args(&["tehas-tehas"]),
        Args {
            check: false,
            remote: "origin".into(),
            dest: "tehas-tehas".into()
        }
    );
    let a = args(&["--remote", "up", "h", "--check"]);
    assert!(a.check);
    assert_eq!((a.remote.as_str(), a.dest.as_str()), ("up", "h"));
    assert_eq!(
        parse(&s(&["--remote"])),
        Err("--remote needs a name".into())
    );
    assert_eq!(parse(&s(&["-x", "h"])), Err("unknown argument: -x".into()));
    assert_eq!(parse(&s(&["a", "b"])), Err("one destination only".into()));
    assert_eq!(
        parse(&s(&["--check"])),
        Err("name the host, e.g. scripts/tag-deploy.sh tehas-tehas".into())
    );
    assert_eq!(parse(&s(&["--help", "--bogus"])), Ok(Parsed::Help));
    assert_eq!(
        parse(&s(&["a", "b", "-h"])),
        Err("one destination only".into())
    );
    assert_eq!(parse(&s(&["-h"])), Ok(Parsed::Help));
}

#[test]
fn the_ssh_command_is_tehas_ssh_split_on_white_space() {
    assert_eq!(ssh_command(None), s(&["ssh"]));
    assert_eq!(ssh_command(Some("")), s(&["ssh"]));
    assert_eq!(ssh_command(Some("  ")), s(&["ssh"]));
    assert_eq!(
        ssh_command(Some(" ssh  -o BatchMode=yes ")),
        s(&["ssh", "-o", "BatchMode=yes"])
    );
}

#[test]
fn the_tag_at_the_head_is_deployed_and_checked() {
    let mut f = Fake::new();
    assert_eq!(deploy(&mut f, &["tehas-tehas"]), 0);
    assert_eq!(
        f.ssh_calls(),
        [
            "tehas-tehas test -r ~/tehas/env.sh",
            "tehas-tehas . ~/tehas/env.sh && tehas update",
            "tehas-tehas git -C ~/tehas/platform rev-parse HEAD",
        ]
    );
    assert_eq!(
        f.said(),
        "tag-deploy: v0.1.0 to tehas-tehas\ntag-deploy: tehas-tehas is at v0.1.0"
    );
    assert_eq!(f.calls[0], "git fetch --quiet --tags --force origin");
}

#[test]
fn a_tag_that_is_not_the_head_calls_no_host() {
    let mut f = Fake::new();
    f.tag_at = OTHER.into();
    assert_eq!(deploy(&mut f, &["h"]), 1);
    assert_eq!(
        f.said(),
        "tag-deploy: main at 1111111 is past v0.1.0; run scripts/tag-release.sh first"
    );
    assert!(f.ssh_calls().is_empty());
}

#[test]
fn a_remote_that_cannot_be_read_calls_no_host() {
    let mut f = Fake::new();
    f.fetch_ok = false;
    assert_eq!(deploy(&mut f, &["--remote", "up", "h"]), 1);
    assert_eq!(
        f.said(),
        "tag-deploy: git fetch --tags --force from up failed"
    );
    let mut f = Fake::new();
    f.ls_ok = false;
    assert_eq!(deploy(&mut f, &["h"]), 1);
    assert_eq!(f.said(), "tag-deploy: cannot read HEAD of origin");
    assert!(f.ssh_calls().is_empty());
}

#[test]
fn an_unreachable_host_or_a_user_without_tehas_updates_nothing() {
    let mut f = Fake::new();
    f.reach = 255;
    assert_eq!(deploy(&mut f, &["h"]), 1);
    assert_eq!(f.said(), "tag-deploy: cannot reach h over ssh");
    assert_eq!(f.ssh_calls(), ["h test -r ~/tehas/env.sh"]);
    let mut f = Fake::new();
    f.reach = 1;
    assert_eq!(deploy(&mut f, &["h"]), 1);
    assert!(f
        .said()
        .starts_with("tag-deploy: h has no ~/tehas/env.sh for the ssh user"));
    assert_eq!(f.ssh_calls().len(), 1);
}

#[test]
fn a_failed_update_passes_its_status_and_checks_no_further() {
    let mut f = Fake::new();
    f.update = 3;
    assert_eq!(deploy(&mut f, &["h"]), 3);
    assert!(f.said().ends_with("tag-deploy: tehas update on h exited 3"));
    assert_eq!(f.ssh_calls().len(), 2);
}

#[test]
fn a_host_not_at_the_tag_fails() {
    let mut f = Fake::new();
    f.host_at = Some(OTHER.into());
    assert_eq!(deploy(&mut f, &["h"]), 1);
    assert!(f
        .said()
        .ends_with("tag-deploy: h is at 2222222, not v0.1.0 (1111111)"));
    let mut f = Fake::new();
    f.host_at = None;
    assert_eq!(deploy(&mut f, &["h"]), 1);
    assert!(f
        .said()
        .ends_with("tag-deploy: cannot read the checkout on h"));
}

#[test]
fn check_runs_the_updates_check_and_nothing_else() {
    let mut f = Fake::new();
    assert_eq!(deploy(&mut f, &["--check", "h"]), 0);
    assert_eq!(
        f.ssh_calls(),
        [
            "h test -r ~/tehas/env.sh",
            "h . ~/tehas/env.sh && tehas update --check"
        ]
    );
    assert_eq!(f.said(), "tag-deploy: v0.1.0 to h (check only)");
    let mut f = Fake::new();
    f.update = 1;
    assert_eq!(deploy(&mut f, &["--check", "h"]), 1);
    assert!(f.said().ends_with("exited 1"));
}

#[test]
fn a_remote_without_a_release_tag_calls_no_host() {
    struct NoTags(Fake);
    impl Sys for NoTags {
        fn git(&mut self, a: &[&str]) -> (String, bool) {
            if a[0] == "tag" {
                return ("v1\nv0.2.0-rc1".into(), true);
            }
            self.0.git(a)
        }
        fn ssh(&mut self, d: &str, c: &str) -> i32 {
            self.0.ssh(d, c)
        }
        fn ssh_out(&mut self, d: &str, c: &str) -> (String, i32) {
            self.0.ssh_out(d, c)
        }
        fn say(&mut self, l: &str) {
            self.0.say(l)
        }
    }
    let mut f = NoTags(Fake::new());
    assert_eq!(run(&args(&["h"]), &mut f), 1);
    assert_eq!(
        f.0.said(),
        "tag-deploy: no default branch or no vX.Y.Z tag on origin"
    );
    assert!(f.0.ssh_calls().is_empty());
}
