//! --say: the lines install.sh prints, from the answers of init and migrate.

use crate::say::{say, strip, Said};
use crate::verbs_tests::*;
use crate::{db, execute, Settings, Verb};
use std::fs;
use std::path::Path;

fn settings() -> Settings {
    Settings {
        version: "v0.10.2".to_string(),
        commit: "cb6912a".to_string(),
        instance: "tehas-tehas".to_string(),
        repo: "example-owner/example-product".to_string(),
    }
}

fn run(home: &Path, verb: Verb) -> Said {
    say(home, &verb, execute(home, &verb, NOW))
}

#[test]
fn the_flag_is_taken_from_the_two_verbs_only() {
    let s = strings;
    assert_eq!(
        strip(&s(&["migrate", "--backup", "/x", "--say"])),
        (s(&["migrate", "--backup", "/x"]), true)
    );
    assert!(strip(&s(&["init", "--say"])).1);
    assert_eq!(
        strip(&s(&["check", "--say"])),
        (s(&["check", "--say"]), false)
    );
    // `import` is gone with the stores older than v0.30.
    assert_eq!(
        strip(&s(&["import", "--say"])),
        (s(&["import", "--say"]), false)
    );
    assert_eq!(crate::parse(&s(&["import"])), None);
    // Twice: the second is left for the verb, which refuses it.
    let (rest, said) = strip(&s(&["init", "--say", "--say"]));
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
            outsource: false,
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
            outsource: false,
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
            outsource: false,
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
            outsource: false,
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
        outsource: false,
    };
    let said = say(&h, &verb, Ok("{}".to_string()));
    assert_eq!(
        said.out,
        vec![format!(
            "store: {} at schema ?, backup ?",
            db::path(&h).display()
        )]
    );
}
