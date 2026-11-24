//! xtask: the development tasks of this repository, behind the scripts in
//! scripts/ that exec it.
//!
//! Usage:
//!   xtask select-tests --root <dir> <path>...
//!   xtask collect-changelog --script <path> <argument>...
//!   xtask release-notes --script <path> <argument>...
//!   xtask pack-release --script <path> <argument>...
//!   xtask tag-release --script <path> <argument>...
//!   xtask tag-deploy --script <path> <argument>...
//!   xtask polyfills --script <path> <argument>...
//!   xtask test --root <dir> [path...]
//!   xtask kani-shard <kani-list.json> <index> <count>
//!   xtask lean-test --root <dir> <file>
//!   xtask lean-roster --root <dir> --report <file>
//!
//! select-tests is scripts/select-tests.sh: src/select.rs says what it prints.
//! pack-release is scripts/pack-release.sh, whose header (the --script) is its
//! usage: src/pack.rs. tag-release is scripts/tag-release.sh, the same way:
//! src/tag.rs; it works in the checkout two levels above the --script.
//! tag-deploy is scripts/tag-deploy.sh, the same way: src/deploy.rs.
//! collect-changelog is scripts/collect-changelog.sh, the same way:
//! src/changelog.rs; its default changelog is the CHANGELOG.md two levels
//! above the --script. polyfills is scripts/polyfills.sh, the same way:
//! src/polyfills.rs; its default tree is the checkout two levels above the
//! --script. test
//! is scripts/test.sh: src/run.rs. Each check of its pool runs as
//! `xtask test-task`, a process of this program's own. kani-shard is the
//! release gate's proofs job: src/kani.rs. lean-test and lean-roster are the
//! test commands of .pi/work.json: src/lean.rs.
//!
//! Exit: what the task says; 64 a task or an argument it does not know;
//! kani-shard 65 a list or a shard it refuses.

use std::path::PathBuf;
use std::process::ExitCode;

mod changelog;
mod deploy;
mod kani;
mod lean;
mod notes;
mod pack;
mod polyfills;
mod run;
mod select;
mod tag;

fn main() -> ExitCode {
    // A task's arguments are paths, taken as bytes.
    let os: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if os.first().is_some_and(|a| a == "test-task") {
        return ExitCode::from(run::task(&os[1..]));
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("select-tests") => select_tests(&args[1..]),
        Some("pack-release") => pack_release(&args[1..]),
        Some("tag-release") => tag_release(&args[1..]),
        Some("tag-deploy") => tag_deploy(&args[1..]),
        Some("collect-changelog") => collect_changelog(&args[1..]),
        Some("release-notes") => release_notes(&args[1..]),
        Some("polyfills") => polyfills(&args[1..]),
        Some("test") => test(&args[1..]),
        Some("kani-shard") => kani_shard(&args[1..]),
        Some("lean-test") => lean_test(&args[1..]),
        Some("lean-roster") => lean_roster(&args[1..]),
        Some("config-check") => config_check(&args[1..]),
        _ => {
            eprintln!("xtask: usage: xtask select-tests --root <dir> <path>...");
            eprintln!("       xtask pack-release --script <path> <argument>...");
            eprintln!("       xtask tag-release --script <path> <argument>...");
            eprintln!("       xtask tag-deploy --script <path> <argument>...");
            eprintln!("       xtask collect-changelog --script <path> <argument>...");
            eprintln!("       xtask release-notes --script <path> <argument>...");
            eprintln!("       xtask polyfills --script <path> <argument>...");
            eprintln!("       xtask test --root <dir> [path...]");
            eprintln!("       xtask kani-shard <kani-list.json> <index> <count>");
            eprintln!("       xtask lean-test --root <dir> <file>");
            eprintln!("       xtask lean-roster --root <dir> --report <file>");
            eprintln!("       xtask config-check --settings <instance dir> [--env <file>]");
            ExitCode::from(64)
        }
    }
}

fn workspace(root: &std::path::Path) -> Option<Vec<lean::Target>> {
    let all = select::test_targets(&select::cargo_metadata(root)?)?;
    Some(
        all.into_iter()
            .map(|(package, target, file)| lean::Target {
                package,
                target,
                file,
            })
            .collect(),
    )
}

/// `config-check --settings <dir> [--env <file>]`: an instance's settings
/// held against this version's load (tehas_ops::settings::config_check),
/// with `<file>` standing for the host's ~/.config/tehas/env. 78 before
/// anything is read when the directory is not given or is not there, or the
/// file is given and is not there; then 0 on a pass and 78 on a fail.
fn config_check(args: &[String]) -> ExitCode {
    const USAGE: &str = "xtask: usage: xtask config-check --settings <instance dir> [--env <file>]";
    let (mut dir, mut env) = (None, None);
    for pair in args.chunks(2) {
        let slot = match pair {
            [flag, _] if flag == "--settings" => &mut dir,
            [flag, _] if flag == "--env" => &mut env,
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(78);
            }
        };
        if slot.replace(pair[1].as_str()).is_some() {
            eprintln!("{USAGE}");
            return ExitCode::from(78);
        }
    }
    let Some(dir) = dir else {
        eprintln!("{USAGE}");
        return ExitCode::from(78);
    };
    if !std::path::Path::new(dir).is_dir() {
        eprintln!("xtask: config-check: no instance directory at {dir}");
        return ExitCode::from(78);
    }
    if let Some(file) = env.filter(|f| !std::path::Path::new(f).is_file()) {
        eprintln!("xtask: config-check: no env file at {file}");
        return ExitCode::from(78);
    }
    let (lines, pass) = tehas_ops::settings::config_check(dir, env, "bash");
    for line in lines {
        tehas_core::outln!("{line}");
    }
    ExitCode::from(if pass { 0 } else { 78 })
}

fn lean_test(args: &[String]) -> ExitCode {
    let [flag, root, file] = args else {
        eprintln!("xtask: usage: xtask lean-test --root <dir> <file>");
        return ExitCode::from(64);
    };
    if flag != "--root" {
        eprintln!("xtask: usage: xtask lean-test --root <dir> <file>");
        return ExitCode::from(64);
    }
    let root = PathBuf::from(root);
    let Some(all) = workspace(&root) else {
        eprintln!(
            "xtask: lean-test: cargo metadata failed in {}",
            root.display()
        );
        return ExitCode::from(1);
    };
    let prefix = format!("{}/", root.display());
    let file = file.strip_prefix(&prefix).unwrap_or(file);
    ExitCode::from(lean::run_test(&root, &all, file))
}

fn lean_roster(args: &[String]) -> ExitCode {
    let [f1, root, f2, report] = args else {
        eprintln!("xtask: usage: xtask lean-roster --root <dir> --report <file>");
        return ExitCode::from(64);
    };
    if f1 != "--root" || f2 != "--report" {
        eprintln!("xtask: usage: xtask lean-roster --root <dir> --report <file>");
        return ExitCode::from(64);
    }
    let root = PathBuf::from(root);
    let Some(all) = workspace(&root) else {
        eprintln!(
            "xtask: lean-roster: cargo metadata failed in {}",
            root.display()
        );
        return ExitCode::from(2);
    };
    ExitCode::from(lean::run_roster(&root, &all, std::path::Path::new(report)))
}

fn select_tests(args: &[String]) -> ExitCode {
    let (root, paths) = match args {
        [flag, root, paths @ ..] if flag == "--root" => (PathBuf::from(root), paths),
        _ => {
            eprintln!("xtask: select-tests needs --root <dir>");
            return ExitCode::from(64);
        }
    };
    if paths.is_empty() {
        eprintln!("usage: scripts/select-tests.sh <path>...");
        return ExitCode::from(64);
    }
    let metadata = || select::cargo_metadata(&root);
    for line in select::select(&root, paths, metadata) {
        tehas_core::outln!("{line}");
    }
    ExitCode::SUCCESS
}

fn pack_release(args: &[String]) -> ExitCode {
    let (script, rest) = match args {
        [flag, script, rest @ ..] if flag == "--script" => (script, rest),
        _ => {
            eprintln!("xtask: pack-release needs --script <path>");
            return ExitCode::from(64);
        }
    };
    let parsed = match pack::parse(rest) {
        Ok(p) => p,
        Err(f) => return refuse(f),
    };
    let a = match parsed {
        pack::Parsed::Help => {
            let text = std::fs::read_to_string(script).unwrap_or_default();
            tehas_core::out!("{}", pack::help(&text));
            return ExitCode::SUCCESS;
        }
        pack::Parsed::Run(a) => a,
    };
    let root = std::env::var("TEHAS_PACK_ROOT").ok();
    match pack::pack(&a, root.as_deref(), &pwd()) {
        Ok(p) => {
            tehas_core::outln!("{}", p.tarball);
            tehas_core::outln!("{}", p.sha_line);
            ExitCode::SUCCESS
        }
        Err(f) => refuse(f),
    }
}

fn tag_release(args: &[String]) -> ExitCode {
    let (script, rest) = match args {
        [flag, script, rest @ ..] if flag == "--script" => (PathBuf::from(script), rest),
        _ => {
            eprintln!("xtask: tag-release needs --script <path>");
            return ExitCode::from(64);
        }
    };
    let a = match tag::parse(rest, &pwd()) {
        Ok(tag::Parsed::Run(a)) => a,
        Ok(tag::Parsed::Help) => {
            let text = std::fs::read_to_string(&script).unwrap_or_default();
            tehas_core::out!("{}", pack::help(&text));
            return ExitCode::SUCCESS;
        }
        Err(msg) => {
            eprintln!("tag-release: {msg}");
            return ExitCode::from(64);
        }
    };
    let root = pack::logical(&pwd(), &script.join("../.."));
    let hosts = tag::Hosts {
        project_of: &tehas_github::project_of,
        site: &github,
    };
    ExitCode::from(tag::run(&a, &root, &hosts))
}

/// GitHub as the code host of `project`, asked as the operator through the
/// gh of the operator's shell, each call in the forge's bound. tag-release
/// acts for no factory, so the site knows no factory's account.
fn github(project: &str) -> std::sync::Arc<dyn tehas_ports::Site> {
    let make: tehas_github::MakeRunner = std::sync::Arc::new(|_| {
        Ok(Box::new(tehas_core::forge::Bounded::new(
            "gh".to_string(),
            tehas_ops::forge::CALL_BOUND,
        )) as Box<dyn tehas_core::forge::GhRunner>)
    });
    std::sync::Arc::new(tehas_github::GitHubSite::new(project, "", None, make))
}

fn tag_deploy(args: &[String]) -> ExitCode {
    let (script, rest) = match args {
        [flag, script, rest @ ..] if flag == "--script" => (PathBuf::from(script), rest),
        _ => {
            eprintln!("xtask: tag-deploy needs --script <path>");
            return ExitCode::from(64);
        }
    };
    let a = match deploy::parse(rest) {
        Ok(deploy::Parsed::Run(a)) => a,
        Ok(deploy::Parsed::Help) => {
            let text = std::fs::read_to_string(&script).unwrap_or_default();
            tehas_core::out!("{}", pack::help(&text));
            return ExitCode::SUCCESS;
        }
        Err(msg) => {
            eprintln!("tag-deploy: {msg}");
            return ExitCode::from(64);
        }
    };
    let root = pack::logical(&pwd(), &script.join("../.."));
    let ssh = deploy::ssh_command(std::env::var("TEHAS_SSH").ok().as_deref());
    ExitCode::from(deploy::run(&a, &mut deploy::Real::new(&root, ssh)))
}

fn collect_changelog(args: &[String]) -> ExitCode {
    let (script, rest) = match args {
        [flag, script, rest @ ..] if flag == "--script" => (PathBuf::from(script), rest),
        _ => {
            eprintln!("xtask: collect-changelog needs --script <path>");
            return ExitCode::from(64);
        }
    };
    let a = match changelog::parse(rest) {
        Ok(changelog::Parsed::Run(a)) => a,
        Ok(changelog::Parsed::Help) => {
            let text = std::fs::read_to_string(&script).unwrap_or_default();
            tehas_core::out!("{}", pack::help(&text));
            return ExitCode::SUCCESS;
        }
        Err(msg) => {
            eprintln!("collect-changelog: {msg}");
            return ExitCode::from(64);
        }
    };
    let default = pack::logical(&pwd(), &script.join("../../CHANGELOG.md"));
    ExitCode::from(changelog::run(&a, &default))
}

fn release_notes(args: &[String]) -> ExitCode {
    let (script, rest) = match args {
        [flag, script, rest @ ..] if flag == "--script" => (PathBuf::from(script), rest),
        _ => {
            eprintln!("xtask: release-notes needs --script <path>");
            return ExitCode::from(64);
        }
    };
    let a = match notes::parse(rest) {
        Ok(notes::Parsed::Run(a)) => a,
        Ok(notes::Parsed::Help) => {
            let text = std::fs::read_to_string(&script).unwrap_or_default();
            tehas_core::out!("{}", pack::help(&text));
            return ExitCode::SUCCESS;
        }
        Err(msg) => {
            eprintln!("release-notes: {msg}");
            return ExitCode::from(64);
        }
    };
    let default = pack::logical(&pwd(), &script.join("../../CHANGELOG.md"));
    ExitCode::from(notes::run(&a, &default))
}

fn polyfills(args: &[String]) -> ExitCode {
    let (script, rest) = match args {
        [flag, script, rest @ ..] if flag == "--script" => (PathBuf::from(script), rest),
        _ => {
            eprintln!("xtask: polyfills needs --script <path>");
            return ExitCode::from(64);
        }
    };
    let usage = || pack::help(&std::fs::read_to_string(&script).unwrap_or_default());
    let a = match polyfills::parse(rest) {
        Ok(polyfills::Parsed::Run(a)) => a,
        Ok(polyfills::Parsed::Help) => {
            tehas_core::out!("{}", usage());
            return ExitCode::SUCCESS;
        }
        Err(polyfills::Bad::Usage) => {
            eprint!("{}", usage());
            return ExitCode::from(64);
        }
        Err(polyfills::Bad::Say(msg)) => {
            eprintln!("polyfills: {msg}");
            return ExitCode::from(64);
        }
    };
    let root = match &a.root {
        Some(r) => pack::logical(&pwd(), r),
        None => pack::logical(&pwd(), &script.join("../..")),
    };
    ExitCode::from(polyfills::run(&a, &root))
}

fn test(args: &[String]) -> ExitCode {
    let (root, paths) = match args {
        [flag, root, paths @ ..] if flag == "--root" => (PathBuf::from(root), paths),
        _ => {
            eprintln!("xtask: test needs --root <dir>");
            return ExitCode::from(64);
        }
    };
    let Ok(me) = std::env::current_exe() else {
        eprintln!("test.sh: cannot name the program that runs the checks");
        return ExitCode::from(2);
    };
    ExitCode::from(run::main(&root, paths, &me))
}

fn kani_shard(args: &[String]) -> ExitCode {
    let usage = || {
        eprintln!("xtask: usage: xtask kani-shard <kani-list.json> <index> <count>");
        ExitCode::from(64)
    };
    let [list, index, count] = args else {
        return usage();
    };
    let (Ok(index), Ok(count)) = (index.parse(), count.parse()) else {
        return usage();
    };
    let text = match std::fs::read_to_string(list) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("kani-shard: {list}: {e}");
            return ExitCode::from(65);
        }
    };
    match kani::shard(&text, index, count) {
        Ok(names) => {
            for n in names {
                tehas_core::outln!("{n}");
            }
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("kani-shard: {msg}");
            ExitCode::from(65)
        }
    }
}

fn refuse(f: pack::Fail) -> ExitCode {
    eprintln!("pack-release: {}", f.msg);
    ExitCode::from(f.code)
}

/// The working directory as the shell names it: $PWD when that is this
/// directory, as `cd` left it, else the one the kernel gives.
fn pwd() -> PathBuf {
    let real = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match std::env::var_os("PWD").map(PathBuf::from) {
        Some(p) if p.is_absolute() && same_dir(&p, &real) => p,
        _ => real,
    }
}

fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}
