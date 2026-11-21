use super::solid::{self, satisfies};
use super::text::{changelog, choose, env_get, env_set, Held, Moved, Ver};
use super::tools::{sha256, World};
use super::{check, run};
use serde_json::json;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tehas_core::testdir::TestDir;

fn v(s: &str) -> Ver {
    Ver::parse(s).expect(s)
}

#[test]
fn a_version_is_three_numbers() {
    assert_eq!(Ver::parse("0.20.3"), Some(Ver(0, 20, 3)));
    for bad in ["1.2", "1.2.3.4", "1.2.3-rc.1", "v1.2.3", "1..3", ""] {
        assert_eq!(Ver::parse(bad), None, "{bad}");
    }
    assert!(v("0.10.1") > v("0.9.9"));
}

#[test]
fn the_newest_of_the_same_major_is_taken_and_a_later_major_is_held() {
    let found = [
        v("0.15.0"),
        v("0.20.2"),
        v("0.19.9"),
        v("1.0.0"),
        v("2.1.0"),
    ];
    assert_eq!(choose(v("0.15.0"), &found), (v("0.20.2"), Some(v("2.1.0"))));
    // Under 0.x a minor is the same major.
    assert_eq!(choose(v("0.5.0"), &[v("0.10.1")]), (v("0.10.1"), None));
    // A pin ahead of what is listed stays.
    assert_eq!(choose(v("1.4.4"), &[v("1.4.2")]), (v("1.4.4"), None));
}

#[test]
fn a_line_of_versions_env_is_read_and_set_in_place() {
    let env = "# NODE_VERSION=1 in a comment\nNODE_VERSION=22.1.0\nNODE_SHA256=aa\n";
    assert_eq!(env_get(env, "NODE_VERSION").as_deref(), Some("22.1.0"));
    assert_eq!(env_get(env, "NODE"), None);
    let set = env_set(env, "NODE_VERSION", "22.2.0").unwrap();
    assert_eq!(
        set,
        "# NODE_VERSION=1 in a comment\nNODE_VERSION=22.2.0\nNODE_SHA256=aa\n"
    );
    assert!(env_set(env, "GH_VERSION", "1")
        .unwrap_err()
        .contains("no GH_VERSION"));
}

const LOG: &str = "# Changelog\n\n## Unreleased\n\n### Changes\n\n- a change\n\n### Adopter action\n\nNone.\n\n## v0.1.0 (2026-01-01)\n\n### Changes\n\n- old\n";

fn moved(name: &str, from: &str, to: &str) -> Moved {
    Moved {
        name: name.into(),
        from: from.into(),
        to: to.into(),
    }
}

fn held(name: &str, current: &str, newest: &str) -> Held {
    Held {
        name: name.into(),
        current: current.into(),
        newest: newest.into(),
        file: "host/versions.env".into(),
    }
}

#[test]
fn the_changelog_names_each_move_and_what_was_held_back() {
    let out = changelog(
        LOG,
        Some("v0.2.0"),
        &[moved("gh", "2.97.0", "2.103.3")],
        &[held("Pi", "1.5.0", "2.0.0")],
    )
    .unwrap();
    assert_eq!(
        out,
        "# Changelog\n\n## Unreleased\n\n### Changes\n\n- a change\n- The pinned gh moves from 2.97.0 to 2.103.3.\n\n\
### Held back\n\n- Pi 2.0.0 is out, a new major version. `host/versions.env` keeps 1.5.0: the move is a person's call.\n\n\
### Adopter action\n\nNone.\n\n## v0.1.0 (2026-01-01)\n\n### Changes\n\n- old\n"
    );
    // A second run keeps where the release started, and replaces what it held.
    let again = changelog(&out, None, &[moved("gh", "2.103.3", "2.104.0")], &[]).unwrap();
    assert!(
        again.contains("- The pinned gh moves from 2.97.0 to 2.104.0.\n"),
        "{again}"
    );
    assert!(!again.contains("Held back"), "{again}");
    assert!(again.contains("- a change\n- The pinned gh moves"));
    assert!(
        again.contains("2.104.0.\n\n### Adopter action\n\nNone.\n\n## v0.1.0"),
        "{again}"
    );
}

#[test]
fn the_changelog_takes_the_section_already_named_for_the_release() {
    let log =
        "# Changelog\n\n## Unreleased\n\n## v0.2.0 (2026-11-05)\n\n### Adopter action\n\nNone.\n";
    let out = changelog(
        log,
        Some("v0.2.0"),
        &[moved("jq", "1.8.1", "1.8.2")],
        &[held("Node", "22.2.0", "24.0.0")],
    )
    .unwrap();
    assert_eq!(
        out,
        "# Changelog\n\n## Unreleased\n\n## v0.2.0 (2026-11-05)\n\n### Changes\n\n- The pinned jq moves from 1.8.1 to 1.8.2.\n\n\
### Held back\n\n- Node 24.0.0 is out, a new major version. `host/versions.env` keeps 22.2.0: the move is a person's call.\n\n\
### Adopter action\n\nNone.\n"
    );
    assert!(changelog("# Changelog\n", None, &[], &[]).is_err());
}

/// A world of fixed answers. npm writes the lockfile it is given.
#[derive(Default)]
struct Fake {
    bodies: HashMap<String, Vec<u8>>,
    unpacked: String,
    lock: String,
    cutoff: Option<String>,
    asked: RefCell<Vec<String>>,
    /// The roots tools/vendor-solid was run in.
    vendored: RefCell<Vec<PathBuf>>,
    vendor_error: Option<String>,
}

impl Fake {
    fn with(mut self, url: &str, body: impl Into<Vec<u8>>) -> Self {
        self.bodies.insert(url.to_string(), body.into());
        self
    }
    fn json(self, url: &str, body: serde_json::Value) -> Self {
        self.with(url, body.to_string())
    }
}

impl World for Fake {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.asked.borrow_mut().push(url.to_string());
        self.bodies
            .get(url)
            .cloned()
            .ok_or_else(|| format!("cannot get {url}"))
    }
    fn unpack(&self, _: &[u8], member: &str) -> Result<String, String> {
        assert_eq!(member, "src/embedding/engine.rs");
        Ok(self.unpacked.clone())
    }
    fn npm_cutoff(&self) -> Option<String> {
        self.cutoff.clone()
    }
    fn npm_lock(&self, dir: &Path) -> Result<(), String> {
        fs::write(dir.join("package-lock.json"), &self.lock).map_err(|e| e.to_string())
    }
    fn vendor_solid(&self, root: &Path) -> Result<(), String> {
        self.vendored.borrow_mut().push(root.to_path_buf());
        self.vendor_error.clone().map_or(Ok(()), Err)
    }
}

const H1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const DIGEST: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
const NEW_DIGEST: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";
const REV: &str = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";
const NEW_REV: &str = "614241f622f53c4eeff9890bdc4f31cfecc418b3";

fn releases(tags: &[&str]) -> serde_json::Value {
    let mut all: Vec<_> = tags
        .iter()
        .map(|t| json!({"tag_name": t, "draft": false, "prerelease": false}))
        .collect();
    all.push(json!({"tag_name": "v99.0.0", "draft": false, "prerelease": true}));
    serde_json::Value::Array(all)
}

fn npm(versions: &[&str]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for v in versions {
        map.insert(v.to_string(), json!({}));
    }
    map.insert("9.0.0".into(), json!({"deprecated": "no"}));
    let times: serde_json::Map<_, _> = versions
        .iter()
        .enumerate()
        .map(|(i, v)| {
            (
                v.to_string(),
                json!(format!("2026-10-0{}T08:00:00.000Z", i + 1)),
            )
        })
        .collect();
    json!({ "versions": map, "time": times })
}

fn crate_versions(versions: &[(&str, bool)]) -> serde_json::Value {
    json!({ "versions": versions.iter().map(|(n, y)| json!({"num": n, "yanked": y})).collect::<Vec<_>>() })
}

/// Every source answering with what is pinned, the pins of a tree at
/// `root`, and the changes the tests make on top.
fn sources() -> Fake {
    let gh = "https://api.github.com/repos";
    Fake::default()
        .json("https://nodejs.org/dist/index.json", json!([{"version": "v22.1.0"}, {"version": "v24.0.0"}, {"version": "v22.2.0"}]))
        .with(
            "https://nodejs.org/dist/v22.2.0/SHASUMS256.txt",
            format!("{}  node-v22.2.0-linux-arm64.tar.xz\n{H1}  node-v22.2.0-linux-x64.tar.xz\n", "9".repeat(64)),
        )
        .json(&format!("{gh}/cli/cli/releases?per_page=100"), releases(&["v2.1.0"]))
        .json(&format!("{gh}/oven-sh/bun/releases?per_page=100"), releases(&["bun-v1.1.0"]))
        .json(&format!("{gh}/koalaman/shellcheck/releases?per_page=100"), releases(&["v0.11.0", "v0.12.0"]))
        .with("https://github.com/koalaman/shellcheck/releases/download/v0.12.0/shellcheck-v0.12.0.linux.x86_64.tar.xz", "tarball")
        .json(&format!("{gh}/jqlang/jq/releases?per_page=100"), releases(&["jq-1.8.2"]))
        .json(&format!("{gh}/rust-lang/rust/releases?per_page=100"), releases(&["1.99.1"]))
        .json(&format!("{gh}/rust-lang/rustup/tags?per_page=100"), json!([{"name": "1.29.2"}]))
        .json(&format!("{gh}/DeusData/codebase-memory-mcp/releases?per_page=100"), releases(&["v0.12.1"]))
        .json(&format!("{gh}/google/osv-scanner/releases?per_page=100"), releases(&["v2.7.0", "v2.7.1"]))
        .with(
            "https://github.com/google/osv-scanner/releases/download/v2.7.1/osv-scanner_SHA256SUMS",
            format!("{H1}  osv-scanner_linux_amd64\n"),
        )
        .json("https://registry.npmjs.org/@earendil-works%2Fpi-coding-agent", npm(&["1.4.0", "1.5.0", "2.0.0"]))
        .json("https://registry.npmjs.org/ctx7", npm(&["0.8.2"]))
        .json("https://registry.npmjs.org/parallel-web-cli", npm(&["0.9.4"]))
        .json("https://registry.npmjs.org/playwright", npm(&["1.64.1"]))
        .json("https://crates.io/api/v1/crates/vipune", crate_versions(&[("0.15.0", false), ("0.20.2", false), ("0.20.3", true)]))
        .with("https://crates.io/api/v1/crates/vipune/0.20.2/download", "crate")
        .json(
            &format!("https://huggingface.co/api/models/BAAI/bge-small-en-v1.5/tree/{NEW_REV}/onnx"),
            json!([{"path": "onnx/model.onnx", "lfs": {"oid": H1}}]),
        )
        .with(&format!("https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/{NEW_REV}/tokenizer.json"), "tokens")
        .json("https://crates.io/api/v1/crates/double-o", crate_versions(&[("0.5.0", false)]))
        .json(
            "https://hub.docker.com/v2/repositories/library/caddy/tags?page_size=100&name=2.",
            json!({"results": [{"name": "2.11.7"}, {"name": "2.11.8-alpine"}], "next": "https://hub.docker.com/page2"}),
        )
        .json("https://hub.docker.com/page2", json!({"results": [{"name": "2.11.8"}], "next": null}))
        .json("https://hub.docker.com/v2/repositories/library/caddy/tags/2.11.8", json!({"digest": NEW_DIGEST}))
        .json("https://registry.npmjs.org/solid-js", solid_doc("solid-js", &[("2.0.3", json!({}))]))
        .json("https://registry.npmjs.org/@solidjs%2Fsignals", solid_doc("@solidjs/signals", &[("2.0.3", json!({}))]))
        .json("https://registry.npmjs.org/@solidjs%2Fweb", solid_doc("@solidjs/web", &[("2.0.3", json!({"solid-js": "^2.0.3"}))]))
        .json("https://registry.npmjs.org/@solidjs%2Fhtml", solid_doc("@solidjs/html", &[("2.0.1", json!({"@solidjs/web": "^2.0.3"}))]))
}

fn tree() -> TestDir {
    let dir = TestDir::new("pins");
    let env = format!(
        "# header\nNODE_VERSION=22.1.0\nGH_VERSION=2.1.0\nBUN_VERSION=1.1.0\nSHELLCHECK_VERSION=0.11.0\nJQ_VERSION=1.8.2\n\
PI_VERSION=1.4.0\nCTX7_VERSION=0.8.2\nPARALLEL_CLI_VERSION=0.9.4\nPLAYWRIGHT_VERSION=1.64.1\nRUST_VERSION=1.99.1\n\
RUSTUP_VERSION=1.29.2\nVIPUNE_VERSION=0.15.0\nVIPUNE_MODEL=BAAI/bge-small-en-v1.5\nVIPUNE_MODEL_REVISION={REV}\n\
OO_VERSION=0.5.0\nCBM_VERSION=0.12.1\nOSV_SCANNER_VERSION=2.7.0\nNODE_SHA256=aa\nSHELLCHECK_SHA256=bb\nVIPUNE_MODEL_SHA256_ONNX=cc\n\
VIPUNE_MODEL_SHA256_TOKENIZER=dd\nPI_LOCK_SHA256=ee\nOSV_SCANNER_SHA256=ff\nCADDY_VERSION=2.11.7\nCADDY_DIGEST={DIGEST}\n"
    );
    for (path, text) in [
        ("host/versions.env", env),
        ("CHANGELOG.md", "# Changelog\n\n## Unreleased\n\n### Adopter action\n\nNone.\n".to_string()),
        ("host/npm/pi/package.json", "{\n  \"dependencies\": {\n    \"@earendil-works/pi-coding-agent\": \"1.4.0\"\n  }\n}\n".to_string()),
        ("host/edge/docs.compose.yaml", format!("    # caddy 2.11.7, named by digest\n    image: caddy@{DIGEST}\n")),
        ("host/edge/router.compose.yaml", format!("    # caddy 2.11.7, named by digest\n    image: caddy@{DIGEST}\n")),
        ("preview/Dockerfile", format!("# caddy 2.11.7, named by digest\nFROM caddy@{DIGEST}\n")),
        (solid::MAIN_RS, SOLID_MAIN.to_string()),
    ] {
        let p = dir.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    dir
}

fn lock(version: &str) -> String {
    json!({"packages": {
        "": {"dependencies": {"@earendil-works/pi-coding-agent": version}},
        "node_modules/@earendil-works/pi-coding-agent": {"version": version}
    }})
    .to_string()
}

fn vipune_engine(rev: &str) -> String {
    format!("pub const EMBED_MODEL_ID: &str = \"BAAI/bge-small-en-v1.5\";\n/// doc\npub const EMBED_MODEL_REVISION: &str = \"{rev}\";\n")
}

#[test]
fn check_says_what_each_tool_would_move_to_and_changes_nothing() {
    let dir = tree();
    let before = fs::read_to_string(dir.join("host/versions.env")).unwrap();
    let lines = check(&dir, &sources()).unwrap();
    assert_eq!(lines[0], "Node 22.1.0 22.2.0 held 24.0.0");
    assert!(lines.contains(&"jq 1.8.2 1.8.2".to_string()), "{lines:?}");
    assert!(
        lines.contains(&"Pi 1.4.0 1.5.0 held 2.0.0".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"vipune 0.15.0 0.20.2".to_string()),
        "a yanked crate is not one: {lines:?}"
    );
    assert_eq!(
        fs::read_to_string(dir.join("host/versions.env")).unwrap(),
        before
    );
}

#[test]
fn move_writes_each_version_with_what_moves_with_it() {
    let dir = tree();
    let world = Fake {
        unpacked: vipune_engine(NEW_REV),
        lock: lock("1.5.0"),
        ..sources()
    };
    let out = run(&dir, Some("v0.3.0"), &world).unwrap();
    assert_eq!(
        out,
        [
            "moved Node: 22.1.0 to 22.2.0",
            "moved shellcheck: 0.11.0 to 0.12.0",
            "moved Pi: 1.4.0 to 1.5.0",
            &format!("moved vipune model: {REV} to {NEW_REV}"),
            "moved vipune: 0.15.0 to 0.20.2",
            "moved OSV-Scanner: 2.7.0 to 2.7.1",
            "moved Caddy: 2.11.7 to 2.11.8",
            "held Node: 22.2.0, 24.0.0 is out",
            "held Pi: 1.5.0, 2.0.0 is out",
        ]
    );
    let env = fs::read_to_string(dir.join("host/versions.env")).unwrap();
    for (key, value) in [
        ("NODE_VERSION", "22.2.0"),
        ("NODE_SHA256", H1),
        ("SHELLCHECK_SHA256", &sha256(b"tarball")),
        ("PI_VERSION", "1.5.0"),
        ("PI_LOCK_SHA256", &sha256(lock("1.5.0").as_bytes())),
        ("VIPUNE_VERSION", "0.20.2"),
        ("VIPUNE_MODEL_REVISION", NEW_REV),
        ("VIPUNE_MODEL_SHA256_ONNX", H1),
        ("VIPUNE_MODEL_SHA256_TOKENIZER", &sha256(b"tokens")),
        ("GH_VERSION", "2.1.0"),
        ("OSV_SCANNER_VERSION", "2.7.1"),
        ("OSV_SCANNER_SHA256", H1),
        ("CADDY_VERSION", "2.11.8"),
        ("CADDY_DIGEST", NEW_DIGEST),
    ] {
        assert_eq!(env_get(&env, key).as_deref(), Some(value), "{key}");
    }
    assert!(env.starts_with("# header\n"));
    let manifest = fs::read_to_string(dir.join("host/npm/pi/package.json")).unwrap();
    assert!(manifest.contains("\"@earendil-works/pi-coding-agent\": \"1.5.0\""));
    for f in [
        "host/edge/docs.compose.yaml",
        "host/edge/router.compose.yaml",
        "preview/Dockerfile",
    ] {
        let text = fs::read_to_string(dir.join(f)).unwrap();
        assert!(
            text.contains(&format!("caddy@{NEW_DIGEST}")) && text.contains("caddy 2.11.8,"),
            "{f}: {text}"
        );
    }
    let log = fs::read_to_string(dir.join("CHANGELOG.md")).unwrap();
    assert!(
        log.contains("### Changes\n\n- The pinned Node moves from 22.1.0 to 22.2.0.\n"),
        "{log}"
    );
    assert!(
        log.contains("- Pi 2.0.0 is out, a new major version."),
        "{log}"
    );
    // Nothing GitHub serves is downloaded for a tool that did not move.
    assert!(!world
        .asked
        .borrow()
        .iter()
        .any(|u| u.contains("/download/v2.1.0/")));
}

#[test]
fn a_move_that_fails_stops_with_its_reason() {
    // A lockfile that does not pin the new version.
    let dir = tree();
    let world = Fake {
        unpacked: vipune_engine(NEW_REV),
        lock: lock("1.4.0"),
        ..sources()
    };
    let e = run(&dir, None, &world).unwrap_err();
    assert!(
        e.contains("does not pin @earendil-works/pi-coding-agent 1.5.0 after npm ran"),
        "{e}"
    );

    // A checksum list without the file.
    let dir = tree();
    let world = Fake {
        unpacked: vipune_engine(NEW_REV),
        lock: lock("1.5.0"),
        ..sources()
    }
    .with(
        "https://nodejs.org/dist/v22.2.0/SHASUMS256.txt",
        "nothing here\n",
    );
    let e = run(&dir, None, &world).unwrap_err();
    assert!(
        e.contains("Node 22.2.0:")
            && e.contains("lists no sha256 for node-v22.2.0-linux-x64.tar.xz"),
        "{e}"
    );

    // A source that cannot be read.
    let mut world = sources();
    world.bodies.remove("https://registry.npmjs.org/ctx7");
    let e = run(&tree(), None, &world).unwrap_err();
    assert!(e.starts_with("ctx7: cannot get"), "{e}");

    // A vipune that names no model revision.
    let world = Fake {
        unpacked: "nothing".into(),
        lock: lock("1.5.0"),
        ..sources()
    };
    let e = run(&tree(), None, &world).unwrap_err();
    assert!(
        e.contains("names no EMBED_MODEL_ID and EMBED_MODEL_REVISION"),
        "{e}"
    );
}

#[test]
fn an_npm_release_younger_than_npms_min_release_age_is_not_taken() {
    // Pi's 1.4.0, 1.5.0 and 2.0.0 are from the 1st, 2nd and 3rd of October.
    let lines = check(
        &tree(),
        &Fake {
            cutoff: Some("2026-10-01T12:00:00".into()),
            ..sources()
        },
    )
    .unwrap();
    assert!(lines.contains(&"Pi 1.4.0 1.4.0".to_string()), "{lines:?}");
    let lines = check(
        &tree(),
        &Fake {
            cutoff: Some("2026-10-02T08:00:00".into()),
            ..sources()
        },
    )
    .unwrap();
    assert!(lines.contains(&"Pi 1.4.0 1.5.0".to_string()), "{lines:?}");
}

#[test]
fn move_takes_a_release_or_a_prerelease_and_nothing_else() {
    let args = |v: &str| {
        ["move", "--release", v, "--root"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };
    // A root that is no tree fails after the arguments are taken: 1, not 64.
    for good in ["v1.0.0", "v1.0.0-beta.1", "v1.0.0-rc.2"] {
        let mut a = args(good);
        a.push("/nonexistent/tehas-pins-root".to_string());
        assert_ne!(super::main(&a), 64, "{good}");
    }
    for bad in ["1.0.0", "v1.0", "v1.0.0-beta", "latest"] {
        let mut a = args(bad);
        a.push("/nonexistent/tehas-pins-root".to_string());
        assert_eq!(super::main(&a), 64, "{bad}");
    }
}

/// tools/vendor-solid/src/main.rs as the tests know it: its shape, its pins
/// and a line after them that names a version and is no pin.
const SOLID_MAIN: &str = r#"//! Bundles the pinned Solid 2 packages.

struct Pin {
    name: &'static str,
    version: &'static str,
    tarball: &'static str,
    integrity: &'static str,
}

const PINS: [Pin; 4] = [
    Pin {
        name: "solid-js",
        version: "2.0.3",
        tarball: "https://registry.npmjs.org/solid-js/-/solid-js-2.0.3.tgz",
        integrity: "sha512-OLD1",
    },
    Pin {
        name: "@solidjs/signals",
        version: "2.0.3",
        tarball: "https://registry.npmjs.org/@solidjs/signals/-/signals-2.0.3.tgz",
        integrity: "sha512-OLD2",
    },
    Pin {
        name: "@solidjs/web",
        version: "2.0.3",
        tarball: "https://registry.npmjs.org/@solidjs/web/-/web-2.0.3.tgz",
        integrity: "sha512-OLD3",
    },
    Pin {
        name: "@solidjs/html",
        version: "2.0.1",
        tarball: "https://registry.npmjs.org/@solidjs/html/-/html-2.0.1.tgz",
        integrity: "sha512-OLD4",
    },
];

const NOT_A_PIN: [&str; 2] = ["name: \"solid-js\",", "version: \"2.0.3\","];
"#;

fn integrity(name: &str, v: &str) -> String {
    format!(
        "sha512-{}{}==",
        sha256(format!("{name}@{v}").as_bytes()),
        "x".repeat(22)
    )
}

fn tarball(name: &str, v: &str) -> String {
    let base = name.rsplit('/').next().unwrap();
    format!("https://registry.npmjs.org/{name}/-/{base}-{v}.tgz")
}

/// A registry document of a Solid package: each version with its peer
/// dependencies, published a day apart from the 1st of October.
fn solid_doc(name: &str, versions: &[(&str, serde_json::Value)]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    let mut times = serde_json::Map::new();
    for (i, (v, peers)) in versions.iter().enumerate() {
        map.insert(
            v.to_string(),
            json!({
                "peerDependencies": peers,
                "dist": {"tarball": tarball(name, v), "integrity": integrity(name, v)},
            }),
        );
        times.insert(
            v.to_string(),
            json!(format!("2026-10-{:02}T08:00:00.000Z", i + 1)),
        );
    }
    json!({ "versions": map, "time": times })
}

fn solid_url(name: &str) -> String {
    format!("https://registry.npmjs.org/{}", name.replace('/', "%2F"))
}

/// The sources with these Solid documents in place of the ones that offer
/// only what is pinned.
fn with_solid(docs: Vec<(&str, serde_json::Value)>) -> Fake {
    docs.into_iter()
        .fold(sources(), |f, (name, doc)| f.json(&solid_url(name), doc))
}

/// The entry of SOLID_MAIN for `name`, its three strings replaced.
fn main_with(text: &str, name: &str, old: (&str, &str), new: &str) -> String {
    let (version, integrity_was) = old;
    let was = format!(
        "        version: \"{version}\",\n        tarball: \"{}\",\n        integrity: \"{integrity_was}\",\n",
        tarball(name, version)
    );
    let now = format!(
        "        version: \"{new}\",\n        tarball: \"{}\",\n        integrity: \"{}\",\n",
        tarball(name, new),
        integrity(name, new)
    );
    assert_eq!(text.matches(&was).count(), 1, "{name}");
    text.replace(&was, &now)
}

#[test]
fn check_lists_each_solid_package_with_its_newest_2x_and_a_held_3x() {
    let dir = tree();
    let world = with_solid(vec![(
        "solid-js",
        solid_doc(
            "solid-js",
            &[
                ("2.0.3", json!({})),
                ("2.0.4", json!({})),
                ("3.0.0", json!({})),
            ],
        ),
    )]);
    let lines = check(&dir, &world).unwrap();
    for want in [
        "solid-js 2.0.3 2.0.4 held 3.0.0",
        "@solidjs/signals 2.0.3 2.0.3",
        "@solidjs/web 2.0.3 2.0.3",
        "@solidjs/html 2.0.1 2.0.1",
    ] {
        assert!(lines.contains(&want.to_string()), "{want}: {lines:?}");
    }
    assert_eq!(
        fs::read_to_string(dir.join(solid::MAIN_RS)).unwrap(),
        SOLID_MAIN
    );
    assert!(world.vendored.borrow().is_empty());
}

#[test]
fn move_takes_solid_patches_rewrites_their_pins_and_rebuilds_the_bundle() {
    let dir = tree();
    let world = Fake {
        unpacked: vipune_engine(NEW_REV),
        lock: lock("1.5.0"),
        ..with_solid(vec![
            (
                "solid-js",
                solid_doc("solid-js", &[("2.0.3", json!({})), ("2.0.4", json!({}))]),
            ),
            (
                "@solidjs/signals",
                solid_doc(
                    "@solidjs/signals",
                    &[("2.0.3", json!({})), ("2.0.4", json!({}))],
                ),
            ),
            (
                "@solidjs/web",
                solid_doc(
                    "@solidjs/web",
                    &[
                        ("2.0.3", json!({"solid-js": "^2.0.3"})),
                        ("2.0.4", json!({"solid-js": "^2.0.4"})),
                    ],
                ),
            ),
            (
                "@solidjs/html",
                solid_doc(
                    "@solidjs/html",
                    &[
                        ("2.0.1", json!({"@solidjs/web": "^2.0.3"})),
                        ("3.0.0", json!({"@solidjs/web": "^3.0.0"})),
                    ],
                ),
            ),
        ])
    };
    let out = run(&dir, Some("v0.3.0"), &world).unwrap();
    let solid_lines: Vec<&String> = out.iter().filter(|l| l.contains("solid")).collect();
    assert_eq!(
        solid_lines,
        [
            "moved solid-js: 2.0.3 to 2.0.4",
            "moved @solidjs/signals: 2.0.3 to 2.0.4",
            "moved @solidjs/web: 2.0.3 to 2.0.4",
            "held @solidjs/html: 2.0.1, 3.0.0 is out",
        ]
    );
    let mut want = main_with(SOLID_MAIN, "solid-js", ("2.0.3", "sha512-OLD1"), "2.0.4");
    want = main_with(&want, "@solidjs/signals", ("2.0.3", "sha512-OLD2"), "2.0.4");
    want = main_with(&want, "@solidjs/web", ("2.0.3", "sha512-OLD3"), "2.0.4");
    assert_eq!(fs::read_to_string(dir.join(solid::MAIN_RS)).unwrap(), want);
    assert_eq!(*world.vendored.borrow(), [dir.to_path_buf()]);
    let log = fs::read_to_string(dir.join("CHANGELOG.md")).unwrap();
    assert!(
        log.contains("- The pinned solid-js moves from 2.0.3 to 2.0.4.\n"),
        "{log}"
    );
    assert!(
        log.contains("- @solidjs/html 3.0.0 is out, a new major version. `tools/vendor-solid/src/main.rs` keeps 2.0.1: the move is a person's call.\n"),
        "{log}"
    );
    assert!(
        log.contains("- Node 24.0.0 is out, a new major version. `host/versions.env` keeps 22.2.0"),
        "{log}"
    );
}

#[test]
fn a_solid_release_whose_peers_are_not_met_is_not_taken() {
    // solid-js 2.0.4 asks for a signals that is not out, web 2.0.4 for that
    // solid-js, and html 2.0.2 for that web.
    let mut solid_js = solid_doc("solid-js", &[("2.0.3", json!({})), ("2.0.4", json!({}))]);
    solid_js["versions"]["2.0.4"]["dependencies"] =
        json!({"@solidjs/signals": "^2.0.4", "seroval": "^1.8.0"});
    let world = with_solid(vec![
        ("solid-js", solid_js),
        (
            "@solidjs/web",
            solid_doc(
                "@solidjs/web",
                &[
                    ("2.0.3", json!({"solid-js": "^2.0.3"})),
                    ("2.0.4", json!({"solid-js": "^2.0.4"})),
                ],
            ),
        ),
        (
            "@solidjs/html",
            solid_doc(
                "@solidjs/html",
                &[
                    ("2.0.1", json!({"@solidjs/web": "^2.0.3"})),
                    ("2.0.2", json!({"@solidjs/web": "^2.0.4"})),
                ],
            ),
        ),
    ]);
    let lines = check(&tree(), &world).unwrap();
    assert!(
        lines.contains(&"@solidjs/web 2.0.3 2.0.3 kept: 2.0.4 asks for solid-js ^2.0.4, and solid-js would be 2.0.3".to_string()),
        "{lines:?}"
    );
    let dir = tree();
    let world = Fake {
        unpacked: vipune_engine(NEW_REV),
        lock: lock("1.5.0"),
        ..world
    };
    let out = run(&dir, None, &world).unwrap();
    let solid_lines: Vec<&String> = out.iter().filter(|l| l.contains("solid")).collect();
    assert_eq!(
        solid_lines,
        [
            "kept solid-js 2.0.3: 2.0.4 asks for @solidjs/signals ^2.0.4, and @solidjs/signals would be 2.0.3",
            "kept @solidjs/web 2.0.3: 2.0.4 asks for solid-js ^2.0.4, and solid-js would be 2.0.3",
            "kept @solidjs/html 2.0.1: 2.0.2 asks for @solidjs/web ^2.0.4, and @solidjs/web would be 2.0.3",
        ]
    );
    assert_eq!(
        fs::read_to_string(dir.join(solid::MAIN_RS)).unwrap(),
        SOLID_MAIN
    );
    assert!(world.vendored.borrow().is_empty());
}

#[test]
fn keeping_a_later_solid_pin_can_keep_an_earlier_one() {
    // solid-js 2.0.4 asks for an html that would move, and that html for a
    // web that is not out: html is kept, and then solid-js.
    let world = with_solid(vec![
        (
            "solid-js",
            solid_doc(
                "solid-js",
                &[
                    ("2.0.3", json!({})),
                    ("2.0.4", json!({"@solidjs/html": "^2.0.2"})),
                ],
            ),
        ),
        (
            "@solidjs/html",
            solid_doc(
                "@solidjs/html",
                &[
                    ("2.0.1", json!({"@solidjs/web": "^2.0.3"})),
                    ("2.0.2", json!({"@solidjs/web": "^2.0.5"})),
                ],
            ),
        ),
    ]);
    let lines = check(&tree(), &world).unwrap();
    for want in [
        "solid-js 2.0.3 2.0.3 kept: 2.0.4 asks for @solidjs/html ^2.0.2, and @solidjs/html would be 2.0.1",
        "@solidjs/html 2.0.1 2.0.1 kept: 2.0.2 asks for @solidjs/web ^2.0.5, and @solidjs/web would be 2.0.3",
    ] {
        assert!(lines.contains(&want.to_string()), "{want}: {lines:?}");
    }
}

#[test]
fn a_solid_pin_that_stays_keeps_a_move_it_does_not_meet() {
    // No newer web is out, and web 2.0.3 asks for solid-js ~2.0.3.
    let world = with_solid(vec![
        (
            "solid-js",
            solid_doc("solid-js", &[("2.0.3", json!({})), ("2.1.0", json!({}))]),
        ),
        (
            "@solidjs/web",
            solid_doc("@solidjs/web", &[("2.0.3", json!({"solid-js": "~2.0.3"}))]),
        ),
    ]);
    let lines = check(&tree(), &world).unwrap();
    let want = "solid-js 2.0.3 2.0.3 kept: @solidjs/web 2.0.3 asks for solid-js ~2.0.3, which 2.1.0 does not meet";
    assert!(lines.contains(&want.to_string()), "{lines:?}");
}

#[test]
fn a_solid_release_younger_than_npms_min_release_age_is_not_taken() {
    // 2.0.3 is from the 1st of October, 2.0.4 from the 2nd.
    let docs = || {
        vec![(
            "solid-js",
            solid_doc("solid-js", &[("2.0.3", json!({})), ("2.0.4", json!({}))]),
        )]
    };
    for (cutoff, want) in [
        ("2026-10-01T12:00:00", "solid-js 2.0.3 2.0.3"),
        ("2026-10-02T08:00:00", "solid-js 2.0.3 2.0.4"),
    ] {
        let world = Fake {
            cutoff: Some(cutoff.into()),
            ..with_solid(docs())
        };
        let lines = check(&tree(), &world).unwrap();
        assert!(lines.contains(&want.to_string()), "{cutoff}: {lines:?}");
    }
}

#[test]
fn no_solid_move_leaves_main_rs_and_the_bundle_alone() {
    let dir = tree();
    let world = Fake {
        unpacked: vipune_engine(NEW_REV),
        lock: lock("1.5.0"),
        ..sources()
    };
    let out = run(&dir, None, &world).unwrap();
    assert!(!out.iter().any(|l| l.contains("solid")), "{out:?}");
    assert_eq!(
        fs::read_to_string(dir.join(solid::MAIN_RS)).unwrap(),
        SOLID_MAIN
    );
    assert!(world.vendored.borrow().is_empty());
}

#[test]
fn a_solid_move_that_fails_stops_with_its_reason() {
    let newer = |dist: serde_json::Value| {
        let mut doc = solid_doc("solid-js", &[("2.0.3", json!({})), ("2.0.4", json!({}))]);
        doc["versions"]["2.0.4"]["dist"] = dist;
        Fake {
            unpacked: vipune_engine(NEW_REV),
            lock: lock("1.5.0"),
            ..with_solid(vec![("solid-js", doc)])
        }
    };
    let good = integrity("solid-js", "2.0.4");
    for (dist, why) in [
        (
            json!({"tarball": "https://example.org/solid-js-2.0.4.tgz", "integrity": good}),
            "tarball",
        ),
        (
            json!({"tarball": tarball("solid-js", "2.0.4"), "integrity": "sha1-abc"}),
            "integrity",
        ),
        (
            json!({"tarball": tarball("solid-js", "2.0.4"), "integrity": format!("{good}\"")}),
            "integrity",
        ),
        (
            json!({"tarball": tarball("solid-js", "2.0.4"), "integrity": "sha512-abc="}),
            "integrity",
        ),
        (json!({}), "tarball"),
    ] {
        let dir = tree();
        let e = run(&dir, None, &newer(dist)).unwrap_err();
        assert!(e.starts_with("solid-js 2.0.4: ") && e.contains(why), "{e}");
    }

    let dir = tree();
    let world = Fake {
        vendor_error: Some("tools/vendor-solid failed".into()),
        ..newer(json!({"tarball": tarball("solid-js", "2.0.4"), "integrity": good}))
    };
    let e = run(&dir, None, &world).unwrap_err();
    assert!(e.contains("tools/vendor-solid failed"), "{e}");

    let dir = tree();
    fs::write(dir.join(solid::MAIN_RS), "no pins here\n").unwrap();
    let e = check(&dir, &sources()).unwrap_err();
    assert!(e.contains("names no PINS"), "{e}");
}

#[test]
fn a_pin_of_vendor_solid_is_read_and_set_in_place() {
    let pins = solid::pins(SOLID_MAIN).unwrap();
    let names: Vec<(&str, &str)> = pins
        .iter()
        .map(|p| (p.name.as_str(), p.version.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("solid-js", "2.0.3"),
            ("@solidjs/signals", "2.0.3"),
            ("@solidjs/web", "2.0.3"),
            ("@solidjs/html", "2.0.1"),
        ]
    );
    let web = solid::Pin {
        name: "@solidjs/web".into(),
        version: "2.0.4".into(),
        tarball: tarball("@solidjs/web", "2.0.4"),
        integrity: integrity("@solidjs/web", "2.0.4"),
    };
    assert_eq!(
        solid::set(SOLID_MAIN, &web).unwrap(),
        main_with(
            SOLID_MAIN,
            "@solidjs/web",
            ("2.0.3", "sha512-OLD3"),
            "2.0.4"
        )
    );
    let other = solid::Pin {
        name: "solid-js/store".into(),
        ..web
    };
    assert!(solid::set(SOLID_MAIN, &other)
        .unwrap_err()
        .contains("no pin of solid-js/store"));

    // The real file reads, and setting each pin to itself changes nothing.
    let real = include_str!("../../../../tools/vendor-solid/src/main.rs");
    let pins = solid::pins(real).unwrap();
    let names: Vec<&str> = pins.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "solid-js",
            "@solidjs/signals",
            "@solidjs/web",
            "@solidjs/html"
        ]
    );
    for p in &pins {
        assert!(p.integrity.starts_with("sha512-"), "{}", p.name);
        assert_eq!(solid::set(real, p).unwrap(), real, "{}", p.name);
    }
}

#[test]
fn a_range_of_npm_is_met_or_not() {
    for (range, ver, met) in [
        ("^2.0.3", "2.0.3", true),
        ("^2.0.3", "2.9.0", true),
        ("^2.0.3", "2.0.2", false),
        ("^2.0.3", "3.0.0", false),
        ("^0.2.3", "0.2.9", true),
        ("^0.2.3", "0.3.0", false),
        ("^0.0.3", "0.0.4", false),
        ("^2.0.0-rc.22", "2.0.1", true),
        ("~2.1.0", "2.1.9", true),
        ("~2.1.0", "2.2.0", false),
        ("2.0.3", "2.0.3", true),
        ("=2.0.3", "2.0.4", false),
        ("2.x", "2.7.1", true),
        ("2", "3.0.0", false),
        ("*", "9.9.9", true),
        ("", "1.0.0", true),
        (">=2.0.0 <3.0.0", "2.5.0", true),
        (">=2.0.0 <3.0.0", "3.0.0", false),
        ("^1.9.0 || ^2.0.0", "2.0.3", true),
        ("^1.9.0 || ^2.1.0", "2.0.3", false),
        (">2.0.3", "2.0.4", true),
        ("<=2.0.3", "2.0.4", false),
        (">2.0.3-rc.1", "2.0.3", true),
        (">2.0.3-rc.1", "2.0.2", false),
        ("~2.1.3", "2.1.2", false),
        ("~2.1.3", "2.1.3", true),
        // Forms this does not read are not met.
        ("2.0.0 - 2.9.0", "2.0.3", false),
        ("2.0.3-rc.1", "2.0.3", false),
        ("latest", "2.0.3", false),
        ("workspace:*", "2.0.3", false),
    ] {
        assert_eq!(satisfies(range, v(ver)), met, "{range} {ver}");
    }
}
