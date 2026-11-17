//! The `TEHAS_*` names tehas reads that are no setting of an instance, each
//! with why. They are internal: any release may change them. A name an
//! operator sets in an instance has a row in [`super::rules::TABLE`]; the test
//! below holds every name the installed source reads to a rule, to this
//! list, to the load's kept or retired names, or to a row of
//! api/renames-1.0.tsv, so a new knob cannot go in without a rule.

use super::{load, renamed, rules};

/// One name each.
pub const UNRULED: &[(&str, &str)] = &[
    ("TEHAS_WORK_DIR", "set by the load: where a cycle works"),
    (
        "TEHAS_CYCLE_LOGIN",
        "refused by the load: a cycle's token comes from the GitHub App",
    ),
    (
        "TEHAS_CYCLE_DONE_WAIT",
        "a test's bound on the poll's stop of a cycle",
    ),
    (
        "TEHAS_DISPATCH_LOCK_WAIT",
        "tehas update's wait, from the environment of its run",
    ),
    (
        "TEHAS_UPDATE_LOCK_WAIT",
        "tehas update's wait, from the environment of its run",
    ),
    (
        "TEHAS_UPDATE_LOCK_FD",
        "handed by tehas update to install.sh",
    ),
    ("TEHAS_ENERGY_NOW", "a test's clock"),
    ("TEHAS_FLAGS_TEST_NOW", "a test's clock"),
    ("TEHAS_STORE_TEST_NOW", "a test's clock"),
    (
        "TEHAS_STORE_TEST_BUSY_REPORT",
        "a test's view of the store's retries",
    ),
    ("TEHAS_MODELS_FILE", "a test's model list"),
    (
        "TEHAS_GH_BIN",
        "the sandbox's gh shim: the real gh, for tests",
    ),
    ("TEHAS_GH_RETRY_WAIT", "the gh shims' pause, for tests"),
    (
        "TEHAS_RELAY_DIR",
        "the sandbox's gh shim: the relay, for tests",
    ),
    (
        "TEHAS_RELAY_WAIT",
        "the sandbox's gh shim: the relay's wait",
    ),
    ("TEHAS_GITHUB_WRITES_LOG", "the gh shim's log of writes"),
    ("TEHAS_MCP_MASK", "handed by tehasbot mcp to the MCP server"),
    (
        "TEHAS_PREVIEW_SETTLE",
        "handed by the console to a preview run",
    ),
    ("TEHAS_REFRESH_LOG", "handed by the console to a refresh"),
    ("TEHAS_ETC", "the login script's /etc, for tests"),
    ("TEHAS_BASE", "an argument of the sandbox image build"),
    ("TEHAS_UID", "an argument of the sandbox image build"),
    ("TEHAS_CYCLE_BIN", "an argument of the sandbox image build"),
    (
        "TEHAS_RUKAS_IMAGES",
        "an argument of the sandbox image build",
    ),
    ("TEHAS_SSH", "release tooling, not installed"),
    ("TEHAS_GATE_POLL", "release tooling, not installed"),
    ("TEHAS_PACK_ROOT", "release tooling, not installed"),
];

/// Every name that starts with one of these.
pub const FAMILIES: &[(&str, &str)] = &[
    (
        "TEHAS_MODEL_KEY_",
        "the model endpoints' tokens, from ~/.config/tehas/env",
    ),
    (
        "TEHAS_SLICE_",
        "what the slice's probe prints for the console",
    ),
    ("TEHAS_TEST_", "the tests' and the gate's own"),
    ("TEHAS_FIXER_", "refused by the load: the fixer is gone"),
];

/// Whether tehas knows `name`: a ruled setting, a name the load keeps,
/// refuses or derives, a renamed or removed setting, or one of this file.
pub fn known(name: &str) -> bool {
    let ruled = |n: &str| rules::TABLE.iter().any(|s| s.name == n);
    // The load gives each size setting in megabytes too.
    let derived = name.strip_suffix("_MB").is_some_and(|base| {
        rules::TABLE
            .iter()
            .any(|s| s.name == base && matches!(s.rule, rules::Rule::FreeDisk))
    });
    ruled(name)
        || derived
        || load::kept(name)
        || load::retired(name)
        || renamed::rows().iter().any(|r| r.old == name)
        || UNRULED.iter().any(|(n, _)| *n == name)
        || FAMILIES.iter().any(|(f, _)| name.starts_with(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if !matches!(
                    name.as_str(),
                    "target" | "tests" | "docs" | "fun" | "node_modules"
                ) {
                    walk(&p, out);
                }
            } else {
                out.push(p);
            }
        }
    }

    /// The `TEHAS_` names in `text`: after a quote in Rust, as words in a
    /// script.
    fn names(text: &str, rust: bool) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find("TEHAS_") {
            let before = rest[..at].chars().last();
            let tail = &rest[at..];
            let end = tail
                .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
                .unwrap_or(tail.len());
            let word_start = !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            if (rust && before == Some('"')) || (!rust && word_start) {
                found.push(tail[..end].to_string());
            }
            rest = &tail[end.max(1)..];
        }
        found
    }

    /// Rust source with its test module, if it ends with one, cut off.
    fn installed_rust(text: &str) -> &str {
        let cut = text
            .match_indices("#[cfg(test)]")
            .find(|(at, m)| text[at + m.len()..].trim_start().starts_with("mod "))
            .map_or(text.len(), |(at, _)| at);
        &text[..cut]
    }

    #[test]
    fn every_name_the_source_reads_is_ruled_or_internal() {
        let root = root();
        let mut files = Vec::new();
        walk(&root.join("console"), &mut files);
        for dir in ["bin", "lib", "libexec", "host"] {
            walk(&root.join(dir), &mut files);
        }
        files.push(root.join("install.sh"));
        files.push(root.join("bootstrap.sh"));
        let mut unknown = Vec::new();
        let mut read = 0;
        for f in files {
            let Ok(text) = fs::read_to_string(&f) else {
                continue;
            };
            let name = f.file_name().unwrap_or_default().to_string_lossy();
            let rust = name.ends_with(".rs");
            if rust && (name.ends_with("_tests.rs") || name == "tests.rs") {
                continue;
            }
            read += 1;
            let text = if rust { installed_rust(&text) } else { &text };
            for n in names(text, rust) {
                // The bare prefix is the family itself.
                if n != "TEHAS_" && !known(&n) {
                    unknown.push(format!(
                        "{n} in {}",
                        f.strip_prefix(&root).unwrap_or(&f).display()
                    ));
                }
            }
        }
        assert!(
            read > 100,
            "the source was not found under {}",
            root.display()
        );
        unknown.sort();
        unknown.dedup();
        assert!(
            unknown.is_empty(),
            "give each a rule in settings/rules.rs, or a line in settings/unruled.rs:\n{}",
            unknown.join("\n")
        );
    }

    #[test]
    fn the_scan_finds_a_name_in_a_string_and_in_a_script() {
        assert_eq!(
            names(r#"env("TEHAS_NEW_KNOB") // TEHAS_X"#, true),
            ["TEHAS_NEW_KNOB"]
        );
        assert_eq!(
            names("x=${TEHAS_NEW_KNOB:-1} MY_TEHAS_Y", false),
            ["TEHAS_NEW_KNOB"]
        );
        assert!(!known("TEHAS_NEW_KNOB"));
        assert!(known("TEHAS_REPO"));
        assert!(known("TEHAS_CLEAN_MIN_FREE_MB"));
        assert!(!known("TEHAS_REPO_MB"));
        assert!(known("TEHAS_MODEL_KEY_MAIN"));
        assert!(known("TEHAS_HOME"));
        assert!(known("TEHAS_OWNER"));
        assert_eq!(
            installed_rust("a\n#[cfg(test)]\nmod tests { \"TEHAS_X\" }"),
            "a\n"
        );
    }

    #[test]
    fn no_unruled_name_has_a_rule_too() {
        for (name, _) in UNRULED {
            assert!(
                !rules::TABLE.iter().any(|s| s.name == *name),
                "{name} is ruled"
            );
        }
    }
}
