//! What install.sh finds wrong with the instance before it changes anything.
//!
//!   console update findings <euid> [<bash>]
//!
//! One line per finding on standard output, each opening with `tehas: `, and
//! exit 78 when there is one; exit 0 and nothing when there is none. What the
//! instance's own shell prints while its instance.env is sourced goes to
//! standard error, as `console settings load` leaves it. `<euid>` and `<bash>`
//! are what lib/config.sh gives `console settings load`.
//!
//! In order, and the first two stop at their first finding:
//!
//! 1. The instance directory, its instance.env and its sandbox.Dockerfile are
//!    the tehas user's own: owned by this account, writable by nobody else, and
//!    by a group only when it is the account's own primary group. A symlink is
//!    judged by the file it points at, and a file that is not there is the
//!    load's to name. instance.env is shell that every command runs, and the
//!    Dockerfile's bytes are built into the image every cycle runs, so a file
//!    another account can write is refused before it is sourced.
//! 2. The settings load (tehas_load), and every setting an instance has to set
//!    itself is set and valid (tehas_require_instance_settings).
//! 3. The instance's models.json is there and holds JSON. It lists every model
//!    the agents use (`console models used`) under the provider each uses, every key it
//!    names is a word or a set token (tehas_core::models::keys_ok), and it lists
//!    the session model the product's `.pi/settings.json` names, when it names
//!    one.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use serde_json::Value;
use tehas_core::models;
use tehas_core::settings::load::Vars;
use tehas_core::settings::INSTANCE_SETTINGS;

use super::{say, tell};
use crate::{account, settings};

/// Who may own and write the settings.
pub(crate) struct Me {
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub name: String,
}

pub fn verb(given: &[String]) -> u8 {
    let (uid, bash) = match given {
        [uid] => (uid.as_str(), "bash"),
        [uid, bash] => (uid.as_str(), bash.as_str()),
        _ => {
            tell("console update findings: usage: <euid> [<bash>]");
            return 64;
        }
    };
    let me = Me {
        uid: account::uid(),
        gid: account::gid(),
        name: account::name(),
    };
    let found = findings(settings::this_environment(), &me, |vars| {
        settings::load_into(vars, uid, bash).map_err(|line| format!("tehas: {line}"))
    });
    for line in &found {
        say(line);
    }
    if found.is_empty() {
        0
    } else {
        78
    }
}

/// Every finding about the instance `vars` names, in the order of the module's
/// list. `load` is tehas_load on the variables, Err its line.
pub(crate) fn findings(
    mut vars: Vars,
    me: &Me,
    load: impl FnOnce(&mut Vars) -> Result<(), String>,
) -> Vec<String> {
    let dir = settings::instance_dir(&vars);
    for file in [
        dir.clone(),
        format!("{dir}/instance.env"),
        format!("{dir}/sandbox.Dockerfile"),
    ] {
        if let Some(line) = not_ours(&file, me) {
            return vec![line];
        }
    }
    if let Err(line) = load(&mut vars) {
        return vec![line];
    }
    if let Some(line) = settings::refusal(&INSTANCE_SETTINGS, &vars) {
        return vec![line];
    }
    model_findings(&dir, &vars)
}

/// Why another account could write `file`, or None. A file that is not there
/// is not judged.
fn not_ours(file: &str, me: &Me) -> Option<String> {
    fs::symlink_metadata(file).ok()?;
    // A symlink is judged by the file it points at; one that points nowhere
    // has no owner that is this account.
    let (mode, owner, group) = match fs::metadata(file) {
        Ok(meta) => (meta.mode() & 0o7777, Some(meta.uid()), Some(meta.gid())),
        Err(_) => (0, None, None),
    };
    if owner.is_none() || owner != me.uid {
        Some(format!("tehas: {file} is not owned by {}", me.name))
    } else if mode & 0o002 != 0 {
        Some(format!("tehas: {file} is writable by everyone (chmod o-w)"))
    } else if mode & 0o020 != 0 && (group.is_none() || group != me.gid) {
        Some(format!(
            "tehas: {file} is writable by another group (chmod g-w)"
        ))
    } else {
        None
    }
}

/// The findings about `<dir>/models.json`, with the settings loaded in `vars`.
fn model_findings(dir: &str, vars: &Vars) -> Vec<String> {
    let file = format!("{dir}/models.json");
    let Ok(bytes) = fs::read(&file) else {
        return vec![format!("tehas: no models.json in {dir}")];
    };
    // jq -e . also refuses a file that is only null or false.
    let json = match serde_json::from_slice::<Value>(&bytes) {
        Ok(json) if !matches!(json, Value::Null | Value::Bool(false)) => json,
        _ => return vec![format!("tehas: {file} is not valid JSON")],
    };
    let mut found: Vec<String> = models::used_from(|name| vars.text(name))
        .into_iter()
        .filter(|(provider, model)| !lists(&json, provider, model))
        .map(|(provider, model)| {
            format!("tehas: models.json does not list model {model} under provider {provider}")
        })
        .collect();
    // A token is named, never written: models.json is in a repository.
    let set = |name: &str| !vars.text(name).is_empty();
    if let Err(why) = models::keys_ok(&json, &file, set) {
        found.push(format!("tehas: {why}"));
    }
    // The session's model is named by the product, not the instance.
    let product = format!("{}/.pi/settings.json", vars.text("TEHAS_WORK_REPO"));
    if let Some((provider, model)) = session_model(Path::new(&product)) {
        if !lists(&json, &provider, &model) {
            found.push(format!(
                "tehas: models.json does not list the product's session model {provider}/{model} (from {product})"
            ));
        }
    }
    found
}

/// Whether the provider's models hold one with this id.
fn lists(json: &Value, provider: &str, model: &str) -> bool {
    json.get("providers")
        .and_then(|p| p.get(provider))
        .and_then(|p| p.get("models"))
        .and_then(Value::as_array)
        .is_some_and(|list| {
            list.iter()
                .any(|m| m.get("id").and_then(Value::as_str) == Some(model))
        })
}

/// The defaultProvider and defaultModel a Pi settings file names, None when it
/// cannot be read or names neither.
fn session_model(file: &Path) -> Option<(String, String)> {
    let json: Value = serde_json::from_slice(&fs::read(file).ok()?).ok()?;
    let word = |key: &str| {
        json.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (provider, model) = (word("defaultProvider"), word("defaultModel"));
    (!provider.is_empty() || !model.is_empty()).then_some((provider, model))
}

#[cfg(test)]
mod tests {
    use super::super::testing::{dir, Dir};
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    const MODELS: &str = r#"{"providers": {"p": {"apiKey": "unused",
        "models": [{"id": "m"}, {"id": "session"}]}}}"#;

    fn me() -> Me {
        Me {
            uid: account::uid(),
            gid: account::gid(),
            name: account::name(),
        }
    }

    // An instance directory with these files, each mode 600, and the
    // variables that name it.
    fn instance(files: &[(&str, &str)]) -> (Dir, Vars) {
        let d = dir("findings");
        let inst = d.0.join("instance");
        fs::create_dir(&inst).unwrap();
        fs::set_permissions(&inst, fs::Permissions::from_mode(0o700)).unwrap();
        for (name, text) in files {
            let path = inst.join(name);
            fs::write(&path, text).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let vars = vars(&[
            ("TEHAS_INSTANCE_DIR", inst.to_str().unwrap()),
            ("HOME", d.0.path().to_str().unwrap()),
        ]);
        (d, vars)
    }

    fn vars(list: &[(&str, &str)]) -> Vars {
        Vars::new(
            list.iter()
                .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec())),
        )
    }

    // The settings an instance has to set, with the agents on p/m.
    const MINIMAL: [(&str, &str); 5] = [
        ("TEHAS_REPO", "acme/widget"),
        ("TEHAS_CONTROL_ISSUE", "1"),
        ("TEHAS_ALLOWED_LABELERS", "alice"),
        ("PI_ENSEMBLE_SUBAGENT_PROVIDER", "p"),
        ("PI_ENSEMBLE_SUBAGENT_MODEL", "m"),
    ];

    // A load that sets MINIMAL, then these, and succeeds.
    fn loads(set: &[(&str, &str)]) -> impl FnOnce(&mut Vars) -> Result<(), String> {
        let set: Vec<(String, String)> = MINIMAL
            .iter()
            .chain(set)
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |vars: &mut Vars| {
            for (k, v) in set {
                vars.set(&k, v);
            }
            Ok(())
        }
    }

    fn mode(d: &Dir, name: &str, mode: u32) {
        let path = d.0.join("instance").join(name);
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn inst(d: &Dir) -> String {
        d.0.join("instance").to_str().unwrap().to_string()
    }

    #[test]
    fn settings_anyone_can_write_are_refused_before_they_are_loaded() {
        let (d, vars) = instance(&[("instance.env", ""), ("models.json", MODELS)]);
        mode(&d, "instance.env", 0o666);
        let found = findings(vars, &me(), |_| panic!("the file was loaded"));
        assert_eq!(
            found,
            [format!(
                "tehas: {}/instance.env is writable by everyone (chmod o-w)",
                inst(&d)
            )]
        );
    }

    #[test]
    fn a_group_may_write_them_only_when_it_is_the_accounts_own() {
        let (d, vars) = instance(&[("sandbox.Dockerfile", "FROM x")]);
        mode(&d, "sandbox.Dockerfile", 0o660);
        let mut other = me();
        other.gid = other.gid.map(|g| g + 1);
        let line = format!(
            "tehas: {}/sandbox.Dockerfile is writable by another group (chmod g-w)",
            inst(&d)
        );
        let said = findings(vars, &other, |_| Err("loaded".into()));
        assert_eq!(said, [line]);
        let (d, vars) = instance(&[("sandbox.Dockerfile", "FROM x")]);
        mode(&d, "sandbox.Dockerfile", 0o660);
        assert_eq!(findings(vars, &me(), |_| Err("loaded".into())), ["loaded"]);
    }

    #[test]
    fn a_directory_of_another_account_is_named_with_this_one() {
        let (d, vars) = instance(&[]);
        let mut other = me();
        other.uid = other.uid.map(|u| u + 1);
        other.name = "tehas".into();
        assert_eq!(
            findings(vars, &other, |_| Err("loaded".into())),
            [format!("tehas: {} is not owned by tehas", inst(&d))]
        );
    }

    #[test]
    fn a_symlink_is_judged_by_the_file_it_points_at() {
        let (d, vars) = instance(&[]);
        let real = d.0.join("real.env");
        fs::write(&real, "").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&real, d.0.join("instance/instance.env")).unwrap();
        assert_eq!(findings(vars, &me(), |_| Err("loaded".into())), ["loaded"]);
        // One that points nowhere is no file of this account.
        let (d, vars) = instance(&[]);
        symlink(d.0.join("gone"), d.0.join("instance/instance.env")).unwrap();
        assert_eq!(
            findings(vars, &me(), |_| Err("loaded".into())),
            [format!(
                "tehas: {}/instance.env is not owned by {}",
                inst(&d),
                me().name
            )]
        );
    }

    #[test]
    fn a_load_that_is_refused_is_the_one_finding() {
        let (_d, vars) = instance(&[("models.json", "not json")]);
        let said = findings(vars, &me(), |_| Err("tehas: no instance.env in /x".into()));
        assert_eq!(said, ["tehas: no instance.env in /x"]);
    }

    #[test]
    fn a_required_setting_that_is_not_set_is_the_one_finding() {
        let (_d, vars) = instance(&[("models.json", "not json")]);
        // The load leaves TEHAS_REPO out.
        let said = findings(vars, &me(), |vars: &mut Vars| {
            loads(&[])(vars)?;
            vars.unset(b"TEHAS_REPO");
            Ok(())
        });
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].starts_with("tehas: TEHAS_REPO is not set (see "),
            "{said:?}"
        );
    }

    #[test]
    fn a_models_json_that_is_missing_or_no_json_is_named() {
        let (d, vars) = instance(&[]);
        assert_eq!(
            findings(vars, &me(), loads(&[])),
            [format!("tehas: no models.json in {}", inst(&d))]
        );
        for text in ["{", "", "null", "false"] {
            let (d, vars) = instance(&[("models.json", text)]);
            assert_eq!(
                findings(vars, &me(), loads(&[])),
                [format!("tehas: {}/models.json is not valid JSON", inst(&d))],
                "{text:?}"
            );
        }
    }

    #[test]
    fn every_model_the_agents_use_is_listed_under_its_provider() {
        let (_d, vars) = instance(&[("models.json", MODELS)]);
        assert!(findings(vars, &me(), loads(&[])).is_empty());
        let (_d, vars) = instance(&[("models.json", MODELS)]);
        let set = &[
            ("PI_ENSEMBLE_SUBAGENT_MODEL", "other"),
            ("PI_ENSEMBLE_PROVIDER_OPS", "q"),
            ("PI_ENSEMBLE_MODEL_OPS", "m"),
        ];
        assert_eq!(
            findings(vars, &me(), loads(set)),
            [
                "tehas: models.json does not list model other under provider p",
                "tehas: models.json does not list model m under provider q",
            ]
        );
    }

    #[test]
    fn a_key_is_a_word_or_a_token_that_is_set() {
        let text =
            r#"{"providers": {"p": {"apiKey": "$TEHAS_MODEL_KEY_A", "models": [{"id": "m"}]}}}"#;
        let (_d, vars) = instance(&[("models.json", text)]);
        assert_eq!(
            findings(vars, &me(), loads(&[])),
            ["tehas: models.json names TEHAS_MODEL_KEY_A and it is not set: put it in ~/.config/tehas/env"]
        );
        let (_d, vars) = instance(&[("models.json", text)]);
        assert!(findings(vars, &me(), loads(&[("TEHAS_MODEL_KEY_A", "k")])).is_empty());
        let text = r#"{"providers": {"p": {"apiKey": "$(cat x)", "models": [{"id": "m"}]}}}"#;
        let (_d, vars) = instance(&[("models.json", text)]);
        let said = findings(vars, &me(), loads(&[]));
        assert_eq!(said.len(), 1);
        assert!(
            said[0].starts_with("tehas: models.json: provider p has an apiKey"),
            "{said:?}"
        );
    }

    #[test]
    fn the_products_session_model_is_listed_too() {
        let (d, mut vars) = instance(&[("models.json", MODELS)]);
        let repo = d.0.join("repo");
        fs::create_dir_all(repo.join(".pi")).unwrap();
        let repo = repo.to_str().unwrap().to_string();
        vars.set("TEHAS_WORK_REPO", repo.as_str());
        let settings = format!("{repo}/.pi/settings.json");
        let with = |text: &str, vars: Vars| {
            fs::write(&settings, text).unwrap();
            findings(vars, &me(), loads(&[]))
        };
        assert!(with(
            r#"{"defaultProvider": "p", "defaultModel": "session"}"#,
            vars.clone()
        )
        .is_empty());
        assert!(with(r#"{"theme": "dark"}"#, vars.clone()).is_empty());
        assert!(with("not json", vars.clone()).is_empty());
        assert_eq!(
            with(r#"{"defaultProvider": "p", "defaultModel": "gone"}"#, vars),
            [format!("tehas: models.json does not list the product's session model p/gone (from {settings})")]
        );
    }
}
