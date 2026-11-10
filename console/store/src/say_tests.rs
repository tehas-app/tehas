//! --say: the lines install.sh prints, from the answers of init, migrate and
//! import, and the refusals of the dry run.

use crate::import::day;
use crate::say::{say, strip, Said};
use crate::verbs_tests::*;
use crate::{db, execute, Settings, Verb};
use std::fs::{self, File};
use std::path::Path;

fn settings() -> Settings {
    Settings {
        version: "v0.10.2".to_string(),
        commit: "cb6912a".to_string(),
        instance: "tehas-tehas".to_string(),
        repo: "example-owner/example-product".to_string(),
    }
}

fn host(name: &str) -> TestDir {
    let h = home(name);
    fs::write(h.join(".tehas-home"), "tehas\n").unwrap();
    h
}

fn run(home: &Path, verb: Verb) -> Said {
    say(home, &verb, execute(home, &verb, NOW), NOW)
}

fn dry(home: &Path) -> Said {
    run(home, Verb::Import { dry_run: true })
}

fn state(home: &Path) -> String {
    home.join("state").display().to_string()
}

#[test]
fn the_flag_is_taken_from_the_three_verbs_only() {
    let s = strings;
    assert_eq!(
        strip(&s(&["import", "--say", "--dry-run"])),
        (s(&["import", "--dry-run"]), true)
    );
    assert_eq!(
        strip(&s(&["migrate", "--backup", "/x", "--say"])),
        (s(&["migrate", "--backup", "/x"]), true)
    );
    assert!(strip(&s(&["init", "--say"])).1);
    assert_eq!(
        strip(&s(&["check", "--say"])),
        (s(&["check", "--say"]), false)
    );
    assert_eq!(strip(&s(&["import"])), (s(&["import"]), false));
    // Twice: the second is left for the verb, which refuses it.
    let (rest, said) = strip(&s(&["import", "--say", "--say"]));
    assert!(said);
    assert_eq!(crate::parse(&rest), None);
}

#[test]
fn init_says_it_made_the_database() {
    let h = scratch("say-init");
    let said = run(
        &h,
        Verb::Init {
            force: false,
            settings: settings(),
        },
    );
    let file = db::path(&h);
    assert_eq!(
        said,
        Said {
            code: 0,
            out: vec![format!("store: made {}", file.display())],
            err: vec![],
        }
    );
    // A second init is refused, with the store's own line and status.
    let said = run(
        &h,
        Verb::Init {
            force: false,
            settings: settings(),
        },
    );
    assert_ne!(said.code, 0);
    assert!(said.out.is_empty());
    assert_eq!(said.err.len(), 1);
    assert!(said.err[0].starts_with("store: "), "{:?}", said.err);
}

#[test]
fn migrate_says_its_schema_and_backup() {
    let h = home("say-migrate");
    let backup = h.join("state/backup");
    let said = run(
        &h,
        Verb::Migrate {
            backup: backup.clone(),
            settings: settings(),
        },
    );
    assert_eq!(said.code, 0, "{said:?}");
    assert_eq!(said.out.len(), 1);
    let prefix = format!(
        "store: {} at schema {}, backup {}/tehas-{}-",
        db::path(&h).display(),
        db::SCHEMA_VERSION,
        backup.display(),
        db::SCHEMA_VERSION
    );
    assert!(said.out[0].starts_with(&prefix), "{:?}", said.out);
    // A file that is not a database: the store's word, its status, no line out.
    fs::write(db::path(&h), "not a database\n").unwrap();
    let said = run(
        &h,
        Verb::Migrate {
            backup,
            settings: settings(),
        },
    );
    assert_eq!(said.code, 78);
    assert!(said.out.is_empty());
    assert!(said.err[0].starts_with("store: corrupt"), "{:?}", said.err);
}

#[test]
fn an_answer_with_no_schema_or_backup_says_so() {
    let h = scratch("say-shown");
    let verb = Verb::Migrate {
        backup: h.join("state/backup"),
        settings: settings(),
    };
    let said = say(&h, &verb, Ok("{}".to_string()), NOW);
    assert_eq!(
        said.out,
        vec![format!(
            "store: {} at schema ?, backup ?",
            db::path(&h).display()
        )]
    );
}

#[test]
fn import_says_what_it_moved_and_nothing_when_it_moved_none() {
    let h = host("say-import");
    let said = run(&h, Verb::Import { dry_run: false });
    assert_eq!(said, Said::default());
    let answer = r#"{"api":1,"verb":"import","ok":true,"imported":{"cycles":1,"approvals":1,"parks":0,"questions":0,"cycle_settings":0,"cycle_history":1,"planning":1,"planning_answers":0,"planning_filed":0,"planning_notes":0,"plan_history":0},"moved":5,"held":[],"dry_run":false}"#;
    let said = say(
        &h,
        &Verb::Import { dry_run: false },
        Ok(answer.to_string()),
        NOW,
    );
    assert_eq!(
        said.out,
        vec![format!(
            "store: imported cycles 1, approvals 1, cycle_history 1, planning 1; moved 5 files to {}/imported-{}/",
            state(&h),
            day(NOW)
        )]
    );
    // From a real import: a start file and its account.
    fs::create_dir_all(h.join("state/cycles")).unwrap();
    fs::write(h.join("state/cycles/7.taken"), "").unwrap();
    fs::write(h.join("state/cycles/7.by"), "alice\n").unwrap();
    let said = run(&h, Verb::Import { dry_run: false });
    assert_eq!(said.code, 0, "{said:?}");
    assert_eq!(
        said.out,
        vec![format!(
            "store: imported cycles 1; moved 2 files to {}/imported-{}/",
            state(&h),
            day(NOW)
        )]
    );
    assert!(h
        .join(format!("state/imported-{}/cycles/7.taken", day(NOW)))
        .exists());
}

#[test]
fn an_import_that_holds_a_file_says_the_store_s_lines() {
    let h = host("say-import-held");
    let target = h.join("elsewhere");
    fs::write(&target, "alice\n").unwrap();
    fs::create_dir_all(h.join("state/cycles")).unwrap();
    fs::write(h.join("state/cycles/7.taken"), "").unwrap();
    std::os::unix::fs::symlink(&target, h.join("state/cycles/7.by")).unwrap();
    let said = run(&h, Verb::Import { dry_run: false });
    assert_eq!(said.code, 1);
    assert!(said.out.is_empty());
    assert!(
        said.err[0].starts_with("store: held 2 files"),
        "{:?}",
        said.err
    );
}

#[test]
fn the_dry_run_says_nothing_when_the_install_may_go_on() {
    let h = host("say-dry");
    assert_eq!(dry(&h), Said::default());
    // Old files and no run: still nothing to say, and nothing moved.
    fs::create_dir_all(h.join("state/cycles")).unwrap();
    fs::write(h.join("state/cycles/7.taken"), "").unwrap();
    assert_eq!(dry(&h), Said::default());
    assert!(h.join("state/cycles/7.taken").exists());
    // A fresh host has no database yet.
    fs::remove_file(db::path(&h)).unwrap();
    assert_eq!(dry(&h), Said::default());
}

#[test]
fn the_dry_run_refuses_a_cycle_or_planning_turn_that_runs() {
    let h = host("say-dry-live");
    fs::create_dir_all(h.join("state/cycles")).unwrap();
    fs::write(h.join("state/cycles/7.taken"), "").unwrap();
    let lock = hold(&h, 9);
    assert_eq!(
        dry(&h),
        Said {
            code: 1,
            out: vec![],
            err: vec!["tehas: a cycle or planning turn runs (#9); run tehas drain --wait and let it end, then install again; nothing was installed".to_string()],
        }
    );
    drop(lock);
    let f = File::create(h.join("state/plan-8.json.lock")).unwrap();
    f.lock().unwrap();
    let f = Held(f);
    let said = dry(&h);
    assert_eq!(said.code, 1);
    assert!(said.err[0].contains("runs (#8);"), "{:?}", said.err);
    drop(f);
}

#[test]
fn the_dry_run_refuses_a_file_the_import_would_hold() {
    let h = host("say-dry-held");
    let target = h.join("elsewhere");
    fs::write(&target, "alice\n").unwrap();
    fs::create_dir_all(h.join("state/cycles")).unwrap();
    fs::write(h.join("state/cycles/7.taken"), "").unwrap();
    std::os::unix::fs::symlink(&target, h.join("state/cycles/7.by")).unwrap();
    let said = dry(&h);
    assert_eq!(said.code, 1);
    assert!(said.out.is_empty());
    assert_eq!(
        said.err,
        vec![
            "store: held 2 files stay where they are, 0 would move".to_string(),
            "  state/cycles/7.by: is not a regular file".to_string(),
            "  state/cycles/7.taken: held with cycles/7.by".to_string(),
            format!(
                "tehas: the old records listed above cannot be imported; move each one out of {} or mend it, then install again; nothing was installed",
                state(&h)
            ),
        ]
    );
    assert!(h.join("state/cycles/7.taken").exists());
    assert_eq!(fs::read_to_string(&target).unwrap(), "alice\n");
}

#[test]
fn a_dry_run_that_cannot_judge_says_so_and_exits_1() {
    // A home without the marker is 78 to the verb, and a refusal of the install.
    let h = home("say-dry-unmarked");
    let said = dry(&h);
    assert_eq!(said.code, 1);
    assert_eq!(said.err.len(), 2, "{:?}", said.err);
    assert!(
        said.err[0].starts_with("store: unmarked "),
        "{:?}",
        said.err
    );
    assert_eq!(
        said.err[1],
        format!(
            "tehas: the store could not judge the old records under {}; nothing was installed",
            state(&h)
        )
    );
    // Only the last five lines of a long failure are kept.
    let long = (1..=8)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let said = say(&h, &Verb::Import { dry_run: true }, Err((1, long)), NOW);
    assert_eq!(said.err.len(), 6);
    assert_eq!(said.err[0], "line 4");
}
