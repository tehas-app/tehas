//! `tehas-contractor doctor`: what a contractor should not hold, and the disk
//! encryption it should have. Each finding is one line; none is fixed here.

use std::fs;
use std::path::Path;

use crate::host::Paths;

/// The names of a model's endpoint or key, which a contractor never needs:
/// its cycles reach the model through the factory.
const MODEL_NAMES: [&str; 6] = [
    "PI_ENSEMBLE_API_KEY_",
    "PI_ENSEMBLE_BASE_URL_",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_BASE_URL",
];

/// The findings of one look at the host: none is a healthy contractor.
/// `vars` are the program's environment; `sys` is where the kernel's block
/// devices are listed (`/sys`).
pub fn findings(paths: &Paths, vars: &[(String, String)], sys: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let idle = !paths.job_file().exists();
    for (name, value) in vars {
        if !value.is_empty() && MODEL_NAMES.iter().any(|m| name.starts_with(m)) {
            out.push(format!(
                "{name} is set: a contractor holds no model endpoint or key, its cycles use the factory's"
            ));
        }
    }
    if idle {
        if paths.models().exists() {
            out.push(format!(
                "{} is here with no job running: a contractor holds no models of its own",
                paths.models().display()
            ));
        }
        if paths.token_dir().join("token").exists() {
            out.push("a cycle token is on disk with no job running".to_string());
        }
        let dirty = ["work", "repos"]
            .iter()
            .any(|d| fs::read_dir(paths.home.join(d)).is_ok_and(|mut r| r.next().is_some()));
        if dirty {
            out.push("the workspace holds a checkout with no job running".to_string());
        }
    }
    if !encrypted(sys) {
        out.push(
            "no encrypted disk is found: an idle box stolen powered off gives its identity key away; set up full disk encryption with the key off the box (TPM or network unlock)"
                .to_string(),
        );
    }
    out
}

/// Whether the kernel lists a dm-crypt device: one whose dm uuid starts with
/// `CRYPT-`.
pub fn encrypted(sys: &Path) -> bool {
    let Ok(blocks) = fs::read_dir(sys.join("block")) else {
        return false;
    };
    blocks.flatten().any(|b| {
        fs::read_to_string(b.path().join("dm/uuid")).is_ok_and(|u| u.starts_with("CRYPT-"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tehas_core::testdir::TestDir;

    fn paths(d: &TestDir) -> Paths {
        Paths {
            home: d.path().join("tehas"),
            user_home: d.path().to_path_buf(),
        }
    }

    fn crypt(sys: &Path, uuid: &str) {
        let dm = sys.join("block/dm-0/dm");
        fs::create_dir_all(&dm).unwrap();
        fs::write(dm.join("uuid"), uuid).unwrap();
    }

    #[test]
    fn a_clean_encrypted_box_has_nothing_to_say() {
        let d = TestDir::new("doctor-clean");
        let sys = d.path().join("sys");
        crypt(&sys, "CRYPT-LUKS2-abc-root\n");
        let vars = vec![("PATH".to_string(), "/usr/bin".to_string())];
        assert_eq!(findings(&paths(&d), &vars, &sys), Vec::<String>::new());
    }

    #[test]
    fn a_model_setting_a_leftover_and_no_encryption_are_each_flagged() {
        let d = TestDir::new("doctor-dirty");
        let p = paths(&d);
        let sys = d.path().join("sys");
        crypt(&sys, "LVM-abc\n");
        fs::create_dir_all(p.models().parent().unwrap()).unwrap();
        fs::write(p.models(), "{}").unwrap();
        fs::create_dir_all(p.token_dir()).unwrap();
        fs::write(p.token_dir().join("token"), "ghs_x").unwrap();
        fs::create_dir_all(p.home.join("work/widget-7")).unwrap();
        let vars = vec![("PI_ENSEMBLE_API_KEY_MAIN".to_string(), "sk".to_string())];
        let found = findings(&p, &vars, &sys);
        assert_eq!(found.len(), 5, "{found:?}");
        assert!(found[0].starts_with("PI_ENSEMBLE_API_KEY_MAIN is set"));
        assert!(found.iter().any(|f| f.contains("no encrypted disk")));
        // While a job runs its files are its own.
        fs::create_dir_all(p.outsource()).unwrap();
        fs::write(p.job_file(), "{}").unwrap();
        assert_eq!(findings(&p, &[], &sys).len(), 1);
    }
}
